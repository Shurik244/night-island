use crate::island::*;
use crate::services::yandex::{self, Command};

impl App {
    pub(in crate::island) fn on_yandex(&mut self) {
        let next = yandex::snapshot();
        if next.account.is_empty() {
            self.close_wave_search();
        }
        if next.search.as_ref().is_some_and(|result| {
            self.ui.yandex.search_sent.as_ref().is_some_and(|(request, _)| *request == result.request)
        }) {
            self.ui.yandex.search_sent = None;
        }
        if self.ui.tab == view::Tab::Yandex
            && self.pointer.seek_drag
            && (next.media.title != self.ui.yandex.state.media.title
                || next.media.artist != self.ui.yandex.state.media.artist
                || next.active != self.ui.yandex.state.active)
        {
            self.pointer.seek_drag = false;
            self.ui.home.seek = None;
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
        }
        let animate = self.state == State::Expanded
            && matches!(self.ui.tab, view::Tab::Home | view::Tab::Yandex)
            && self.settings.tab_anim != crate::data::settings::TabAnim::None;
        let wave_volume_changed = next.config.volume != self.ui.yandex.state.config.volume;
        self.ui.yandex.accept(next, animate);
        if wave_volume_changed
            && self.ui.home.vol_track
            && self.ui.media.source == yandex::SOURCE
            && !self.pointer.vol_drag
        {
            self.ui.home.app_vol = f32::from(self.ui.yandex.state.config.volume) / 100.0;
        }
        self.render_settings_win();
        if self.ui.yandex.colour_transition.is_some() {
            self.set_timer(Timer::UiAnim, self.frame_ms());
        }
        self.on_media();
        self.start_progress_timer();
        if self.ui.tab == view::Tab::Yandex || (wave_volume_changed && self.ui.tab == view::Tab::Home) {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn wave_search(&self) -> bool {
        self.ui.tab == view::Tab::Yandex && self.ui.yandex.search_open && !self.ui.cmd.open
    }

    pub(in crate::island) fn close_wave_search(&mut self) {
        if !self.ui.yandex.search_open {
            return;
        }
        self.kill_timer(Timer::WaveSearch);
        if self.wave_search() {
            self.end_edit(true);
        }
        self.ui.yandex.search_open = false;
        self.ui.yandex.search_selected = None;
        self.ui.yandex.hover = None;
        if self.ui.tab == view::Tab::Yandex {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn wave_search_changed(&mut self) {
        let input = &mut self.ui.yandex.search_input;
        let text = input.text();
        if text.chars().count() > view::SEARCH_MAX_CHARS {
            input.set_text(&text.chars().take(view::SEARCH_MAX_CHARS).collect::<String>());
        }
        self.ui.yandex.search_selected = None;
        let query = self.ui.yandex.search_query();
        if query.is_empty() || self.ui.yandex.search_result().is_some() {
            self.kill_timer(Timer::WaveSearch);
        } else {
            self.ui.yandex.search_page = 0;
            self.ui.yandex.search_selected = None;
            self.set_timer(Timer::WaveSearch, 350);
        }
        self.ui.caret_on = true;
        self.set_timer(Timer::Caret, super::super::editing::CARET_BLINK_MS);
        self.repaint_expanded();
    }

    pub(in crate::island) fn submit_wave_search(&mut self) {
        self.kill_timer(Timer::WaveSearch);
        let wave = &self.ui.yandex;
        let query = wave.search_query();
        if !wave.search_open || query.is_empty() || wave.search_sent.as_ref().is_some_and(|(_, sent)| sent == &query) {
            return;
        }
        if wave.search_result().is_some_and(|result| result.error.is_none()) {
            return;
        }
        if let Some(request) = yandex::search(query.clone()) {
            self.ui.yandex.search_page = 0;
            self.ui.yandex.search_selected = None;
            self.ui.yandex.search_sent = Some((request, query));
        } else {
            self.set_timer(Timer::WaveSearch, 350);
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn play_wave_result(&mut self, index: usize) {
        let wave = &self.ui.yandex;
        if wave.state.busy {
            return;
        }
        let Some(track) = wave.search_rows().get(index) else { return };
        let command = if wave.state.current_id.as_ref() == Some(&track.id) {
            Command::Toggle
        } else {
            Command::PlaySearch { query: wave.search_query(), id: track.id.clone() }
        };
        self.ui.yandex.search_selected = None;
        self.end_edit(false);
        yandex::send(command);
    }

    pub(in crate::island) fn click_yandex(&mut self, x: i32, y: i32) {
        use view::YandexHit as Hit;
        let hit = view::hit_yandex(self.pill_rect(true), self.scale, &self.ui.yandex, x as f32, y as f32);
        let command = match hit {
            Some(Hit::Home) => {
                self.select_tab(view::Tab::Home);
                return;
            }
            Some(Hit::Seek) => {
                if yandex::progress().is_none() {
                    return;
                }
                self.begin_seek(view::wave_fraction(self.pill_rect(true), self.scale, x as f32));
                return;
            }
            Some(Hit::Volume) => {
                self.pointer.vol_drag = true;
                unsafe {
                    windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
                }
                self.drag_volume(x);
                return;
            }
            Some(Hit::Search) => {
                if self.ui.yandex.search_open {
                    self.close_wave_search();
                } else {
                    self.ui.yandex.search_open = true;
                    self.ui.yandex.show_library = false;
                    self.begin_edit();
                    self.submit_wave_search();
                    self.repaint_expanded();
                }
                return;
            }
            Some(Hit::SearchInput) => {
                self.begin_edit();
                if let Some(layout) = self.active_layout() {
                    let inner = view::wave_search_field(self.pill_rect(true), self.scale).1;
                    let input = &mut self.ui.yandex.search_input;
                    let (caret, _, _) = edit::caret_xy(&layout, input.utf16_at(input.caret));
                    let shift = (caret - (inner.2 - inner.0) + 8.0 * self.scale).max(0.0);
                    input.click(&layout, x as f32 - inner.0 + shift, 0.0, false);
                }
                self.ui.yandex.search_selected = None;
                self.repaint_expanded();
                return;
            }
            Some(Hit::SearchPage(dir)) => {
                self.ui.yandex.search_page = self.ui.yandex.search_page.saturating_add_signed(dir as isize);
                self.ui.yandex.search_selected = None;
                self.repaint_expanded();
                return;
            }
            Some(Hit::PlaySearch(index)) => return self.play_wave_result(index),
            Some(Hit::Library) => {
                self.close_wave_search();
                self.ui.yandex.show_library = !self.ui.yandex.show_library;
                self.ui.yandex.hover = None;
                self.repaint_expanded();
                if !self.ui.yandex.show_library {
                    return;
                }
                Command::Library(0)
            }
            Some(Hit::LibraryPage(dir)) => {
                let Some(library) = &self.ui.yandex.state.library else { return };
                Command::Library(library.page.saturating_add_signed(dir as isize))
            }
            Some(Hit::PlayLiked(index)) => {
                let Some(track) = self.ui.yandex.state.library.as_ref().and_then(|page| page.tracks.get(index)) else {
                    return;
                };
                if self.ui.yandex.state.current_id.as_ref() == Some(&track.id) {
                    Command::Toggle
                } else {
                    Command::PlayLiked(track.id.clone())
                }
            }
            Some(Hit::Rate(rating)) => {
                let Some(id) = self.ui.yandex.state.current_id.clone() else { return };
                Command::Rate(id, rating)
            }
            Some(Hit::Connect) => Command::Connect,
            Some(Hit::Cancel) => Command::Cancel,
            Some(Hit::Disconnect) => Command::Disconnect,
            Some(Hit::Toggle) => Command::Toggle,
            Some(Hit::Previous) => Command::Previous,
            Some(Hit::Next) => Command::Next,
            Some(Hit::Stop) => Command::Stop,
            Some(Hit::Retry) if self.ui.yandex.show_library => {
                Command::Library(self.ui.yandex.state.library.as_ref().map_or(0, |page| page.page))
            }
            Some(Hit::Retry) => Command::Retry,
            Some(Hit::Autoplay) => Command::Autoplay(!self.ui.yandex.state.config.autoplay),
            Some(Hit::Filter(index, dir)) => {
                let Some(group) = self.ui.yandex.state.groups.get(index) else { return };
                let current = self
                    .ui
                    .yandex
                    .state
                    .config
                    .selection
                    .get(&group.key)
                    .and_then(|seed| group.items.iter().position(|i| &i.1 == seed))
                    .map_or(0, |i| i + 1);
                let next = (current as i32 + dir).rem_euclid(group.items.len() as i32 + 1) as usize;
                Command::Select(group.key.clone(), next.checked_sub(1).map(|i| group.items[i].1.clone()))
            }
            Some(Hit::Browser) => {
                cmd::shell_open("https://ya.ru/device", None);
                return;
            }
            Some(Hit::Copy) => {
                self.ui.yandex.copied = clips::sys::write_text(self.hwnd, &self.ui.yandex.state.code);
                self.repaint_expanded();
                return;
            }
            None => return,
        };
        yandex::send(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Renders synthetic My Wave states using Windows Composition"]
    fn my_wave_native_preview() -> Result<()> {
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
        }
        let _queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })?
        };
        let compositor = windows::UI::Composition::Compositor::new()?;
        let gpu = crate::ui::gfx::Gpu::new(&compositor)?;
        let surface = crate::ui::popup::PopupSurface::new((1320, 820))?;
        let mut ui = view::UiState::new();
        ui.tab = view::Tab::Yandex;
        ui.cfg.header_tabs = vec!["My Wave".into(), "Weather".into(), "App mixer".into()];
        use crate::services::yandex::{Group, LibraryPage, Rating, SearchResult, Track};
        crate::ui::i18n::set_ru(true);
        use crate::ui::gfx::rgba;
        let groups = vec![
            Group {
                key: "context".into(),
                name: "Wave activity".into(),
                items: vec![("Работаю".into(), "activity:work-background".into())],
            },
            Group {
                key: "diversity".into(),
                name: "Variety".into(),
                items: vec![("Незнакомое".into(), "settingDiversity:discover".into())],
            },
            Group {
                key: "moodEnergy".into(),
                name: "Mood".into(),
                items: vec![("Спокойное".into(), "settingMoodEnergy:calm".into())],
            },
            Group {
                key: "language".into(),
                name: "Language".into(),
                items: vec![("Без слов".into(), "settingLanguage:without-words".into())],
            },
        ];
        surface.paint(&gpu, |p| {
            p.fill_rect((0.0, 0.0, 1320.0, 820.0), rgba(0.025, 0.035, 0.06, 1.0))?;
            for (i, (x, y)) in [(10.0, 10.0), (670.0, 10.0), (10.0, 420.0), (670.0, 420.0)].into_iter().enumerate() {
                ui.tab = if i % 2 == 0 { view::Tab::Home } else { view::Tab::Yandex };
                ui.yandex.state = Default::default();
                ui.yandex.state.account = "Музыка для тебя".into();
                ui.yandex.state.groups = groups.clone();
                ui.yandex.state.active = true;
                ui.yandex.state.current_id = Some("1".into());
                ui.yandex.state.rating = Rating::Like;
                ui.yandex.state.config.volume = 37;
                ui.yandex.show_library = i == 3;
                ui.yandex.search_open = i == 1;
                ui.editing = i == 1;
                let tracks: Vec<_> = ["Ночной город", "Маяк", "Тёплый вечер", "Дорога домой"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, title)| Track {
                        id: (i + 1).to_string(),
                        title: title.into(),
                        artist: "Любимый исполнитель".into(),
                        cover: String::new(),
                        batch: String::new(),
                        session: String::new(),
                        palette: Default::default(),
                    })
                    .collect();
                if ui.yandex.show_library {
                    ui.yandex.state.library = Some(LibraryPage { page: 0, total: 12, tracks });
                } else if ui.yandex.search_open {
                    let query = "Любимый исполнитель";
                    ui.yandex.search_input.set_text(query);
                    ui.yandex.state.search =
                        Some(SearchResult { request: 1, query: query.into(), tracks, error: None });
                }
                ui.yandex.state.config.selection.insert("moodEnergy".into(), "settingMoodEnergy:calm".into());
                let rgb = if i < 2 { (238, 108, 48) } else { (50, 156, 230) };
                ui.yandex.state.palette = crate::services::yandex::palette::Palette {
                    average: Some(rgb),
                    mini_player: Some(rgb),
                    accent: Some(rgb),
                    wave_text: Some((245, 238, 230)),
                };
                ui.yandex.state.media = crate::platform::media::Media {
                    title: if i < 2 { "Тёплый вечер" } else { "Ночной город" }.into(),
                    artist: "Любимый исполнитель".into(),
                    source: "Night Island · Yandex Music".into(),
                    playing: true,
                    accent: Some(rgb),
                    ..Default::default()
                };
                let pixels: Vec<u8> = (0..96 * 96)
                    .flat_map(|n| {
                        let x = (n % 96) as f32 / 96.0;
                        let y = (n / 96) as f32 / 96.0;
                        let light = 0.3 + 0.7 * (1.0 - (x - y).abs());
                        [(rgb.0 as f32 * light) as u8, (rgb.1 as f32 * light) as u8, (rgb.2 as f32 * light) as u8, 255]
                    })
                    .collect();
                ui.yandex.state.media.cover = crate::platform::media::debug_cover(96, &pixels);
                ui.media = ui.yandex.state.media.clone();
                crate::platform::media::debug_timeline(83.0, 241.0);
                crate::services::yandex::debug_timeline(83.0, 241.0);
                p.fill_round_rect((x, y, x + 640.0, y + 384.0), 28.0, rgba(0.0, 0.0, 0.0, 1.0))?;
                view::paint_expanded(p, (x, y, 640.0, 384.0), 1.0, &ui, &[])?;
            }
            Ok(())
        })?;
        if let Some(path) = std::env::var_os("NI_WAVE_PREVIEW") {
            let rgba: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 1320, 820);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
        }
        Ok(())
    }
}
