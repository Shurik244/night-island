use crate::island::*;

impl App {
    pub(in crate::island) fn open_dev_menu(&mut self) {
        self.close_home_mixer();
        let items = volume::devices();
        self.ui.home.dev_menu = (!items.is_empty()).then_some((items, 0));
        self.repaint_expanded();
    }

    pub(in crate::island) fn click_dev_menu(&mut self, x: i32, y: i32) {
        let pill = self.pill_rect(true);
        let Some((items, scroll)) = self.ui.home.dev_menu.take() else { return };
        let wide = view::home_wide(&self.ui);
        if let Some(Some(row)) = view::hit_menu(pill, self.scale, x as f32, y as f32, items.len(), wide) {
            if let Some(d) = items.get(scroll + row) {
                volume::set_default(&d.id);
                self.refresh_home();
                self.ui.mic_muted = volume::mic_muted();
            }
        } else if view::hit_menu(pill, self.scale, x as f32, y as f32, items.len(), wide).is_some() {
            self.ui.home.dev_menu = Some((items, scroll));
            return;
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn click_home(&mut self, x: i32, y: i32) {
        if self.ui.home.dev_menu.is_some() {
            return self.click_dev_menu(x, y);
        }
        let pill = self.pill_rect(true);
        match view::hit_home(pill, self.scale, x as f32, y as f32, &self.ui) {
            Some(view::HomeHit::WidgetControl(control)) => self.widget_control(control),
            Some(view::HomeHit::WidgetHandle(id)) => self.begin_widget_resize(id, x, y),
            Some(view::HomeHit::Action(id)) if self.ui.home.widgets.editing && !self.ui.home.edit => {
                self.begin_widget_drag(id as u8, x, y);
            }
            Some(view::HomeHit::Live(index)) => self.open_live_activity(index),
            Some(view::HomeHit::Notification) => {
                if self.settings.notification_open_app {
                    self.activate_notification();
                } else {
                    self.expand_notification_text();
                }
            }
            Some(view::HomeHit::WaveRate(rating)) => {
                if let Some(id) = self.ui.yandex.state.current_id.clone() {
                    crate::services::yandex::send(crate::services::yandex::Command::Rate(id, rating));
                }
            }
            Some(view::HomeHit::NotificationClose) => {
                self.dismiss_home_notification();
            }
            Some(view::HomeHit::NotificationExpand) => self.expand_notification_text(),
            Some(view::HomeHit::Mixer(_)) => self.click_mixer(x, y),
            Some(view::HomeHit::Media(i)) => media::control(match i {
                0 => media::Control::Previous,
                1 => media::Control::PlayPause,
                _ => media::Control::Next,
            }),
            Some(view::HomeHit::Player) => media::next_player(),
            Some(view::HomeHit::Wave) => self.select_tab(view::Tab::Yandex),
            Some(view::HomeHit::Action(i)) if self.ui.home.edit => self.toggle_home_action(i),
            Some(view::HomeHit::Action(i))
                if self.settings.home_widgets
                    || self.ui.notifications.preview.is_some()
                    || !self.ui.live.is_empty() =>
            {
                self.home_action(i)
            }
            Some(view::HomeHit::Action(i)) => {
                match self.settings.home_actions.iter().position(|&a| usize::from(a) == i) {
                    Some(slot) => self.ui.home.tile_press = Some((slot, x, y)),
                    None => self.home_action(i),
                }
            }
            Some(view::HomeHit::Edit) => {
                if self.settings.home_widgets {
                    self.toggle_widget_editor();
                    return;
                }
                self.close_home_mixer();
                self.ui.home.edit = !self.ui.home.edit;
                self.ui.home.edit_scroll = 0;
                self.ui.home.hover_act = None;
                self.sync_metrics();
                self.repaint_expanded();
            }
            Some(view::HomeHit::Dock(i)) => self.ui.home.dock_press = Some((i, x, y)),
            Some(view::HomeHit::DockUnpin(i)) => {
                if let (Some(item), Some(store)) = (self.ui.home.dock.get(i), &self.store) {
                    store.dock_remove(item.id);
                }
                self.ui.home.hover_act = None;
                self.reload_dock();
                self.repaint_expanded();
            }
            Some(view::HomeHit::DockAdd) => self.want_dock_dialog = true,
            Some(view::HomeHit::DockMore) => {
                self.ui.home.dock_open = !self.ui.home.dock_open;
                self.ui.home.hover_act = None;
                self.repaint_expanded();
            }
            Some(view::HomeHit::DiscordOverlay) => self.toggle_overlay(),
            Some(view::HomeHit::DiscordMute) => discord::set_mute(!self.ui.discord.self_mute),
            Some(view::HomeHit::DiscordDeaf) => discord::set_deaf(!self.ui.discord.self_deaf),
            Some(view::HomeHit::Seek(f)) => {
                self.begin_seek(f);
            }
            Some(view::HomeHit::VolumeIcon) => {
                self.close_home_mixer();
                if self.ui.home.vol_track {
                    self.ui.home.vol_track = false;
                } else if let Some(level) = volume::app_volume(&self.ui.media.source) {
                    self.ui.home.vol_track = true;
                    self.ui.home.app_vol = level;
                } else {
                    self.notify(Notice::Error, '\u{E767}', "Player volume is unavailable. Right-click for the mixer");
                }
                self.refresh_home();
                self.repaint_expanded();
            }
            Some(view::HomeHit::Volume) => {
                self.pointer.vol_drag = true;
                self.drag_volume(x);
            }
            Some(view::HomeHit::Device) => self.open_dev_menu(),
            Some(view::HomeHit::Extra(i)) => match i {
                0 => spotify::toggle_shuffle(),
                1 => spotify::cycle_repeat(),
                _ => spotify::toggle_like(),
            },
            None => {}
        }
    }

    pub(in crate::island) fn right_click_home_volume(&mut self, x: i32, y: i32) -> bool {
        if self.state != State::Expanded || self.ui.tab != view::Tab::Home || self.ui.home.dev_menu.is_some() {
            return false;
        }
        if !matches!(
            view::hit_home(self.pill_rect(true), self.scale, x as f32, y as f32, &self.ui),
            Some(view::HomeHit::VolumeIcon)
        ) {
            return false;
        }
        self.finish_mixer();
        let from = view::home_mixer_progress(&self.ui);
        self.ui.home.mixer_open = !self.ui.home.mixer_open;
        self.ui.home.mixer_motion = Some((from, Instant::now()));
        self.ui.home.edit = false;
        self.ui.home.dev_menu = None;
        self.ui.home.dock_open = false;
        self.sync_mixer();
        self.set_timer(Timer::UiAnim, self.frame_ms());
        self.repaint_expanded();
        true
    }

    fn toggle_home_action(&mut self, id: usize) {
        let id = id as u8;
        if self.settings.home_widgets
            && !self.settings.home_actions.contains(&id)
            && !view::can_resize_widget(&self.ui, id, crate::data::home_widgets::WidgetSize::Tile)
        {
            self.ui.home.widgets.status = crate::ui::i18n::tr("Not enough room in the grid").into_owned();
            self.repaint_expanded();
            return;
        }
        match self.settings.home_actions.iter().position(|&a| a == id) {
            Some(at) => {
                self.settings.home_actions.remove(at);
                self.settings.home_widget_sizes.remove(&id);
                if id == view::weather_widget_id() {
                    self.settings.home_weather_widget = false;
                }
            }
            None if self.settings.home_actions.len() >= view::MAX_HOME_TILES => {
                return self.notify(Notice::Error, '\u{E783}', "Up to 8 tiles fit: take one off first");
            }
            None => {
                self.settings.home_widget_sizes.remove(&id);
                self.settings.home_actions.push(id);
            }
        }
        if self.ui.home.widgets.editing {
            self.ui.home.widgets.selected = self.settings.home_actions.contains(&id).then_some(id);
            self.ui.home.widgets.status.clear();
        }
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        self.sync_metrics();
        self.repaint_expanded();
    }

    fn home_info_click(&mut self, id: usize) {
        let Some(view::Tile::Info(_, _, info)) = view::tile_kind(id) else { return };
        let tab = match info {
            view::Info::Metric(_) => Some(view::Tab::Controls),
            view::Info::ScreenTime => Some(view::Tab::Activity),
            view::Info::Weather => Some(view::Tab::Weather),
            view::Info::Reminder => Some(view::Tab::Note),
            view::Info::Steam => Some(view::Tab::Steam),
            view::Info::Vpn => return self.toggle_vpn(),
            view::Info::Battery | view::Info::Agents | view::Info::Clock => None,
        };
        if let Some(tab) = tab
            .filter(|t| view::tab_in_bar(&self.settings, *t, view::TABS.iter().find(|x| x.0 == *t).map_or("", |x| x.2)))
        {
            self.select_tab(tab);
        }
    }

    pub(in crate::island) fn drag_volume(&mut self, x: i32) {
        if !self.pointer.vol_drag {
            return;
        }
        if self.ui.tab == view::Tab::Yandex {
            let level =
                view::wave_volume_level_for(self.pill_rect(true), self.scale, x as f32, self.ui.yandex.legacy_layout);
            let current = self.ui.yandex.volume_preview.unwrap_or(self.ui.yandex.state.config.volume);
            if level != current && crate::services::yandex::set_volume(f32::from(level) / 100.0) {
                self.ui.yandex.volume_preview = Some(level);
                self.repaint_expanded();
            }
            return;
        }
        let (tl, tr, _) = view::volume_track(self.pill_rect(true), self.scale, view::home_wide(&self.ui));
        let level = ((x as f32 - tl) / (tr - tl).max(1.0)).clamp(0.0, 1.0);
        let current = if self.ui.home.vol_track { self.ui.home.app_vol } else { self.ui.home.vol };
        if (level - current).abs() > 0.004 {
            if self.ui.home.vol_track {
                if volume::set_app_volume(&self.ui.media.source, level) {
                    self.ui.home.app_vol = level;
                }
            } else {
                self.ui.home.vol = level;
                volume::set(level);
            }
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn begin_seek(&mut self, fraction: f32) {
        self.pointer.seek_drag = true;
        self.ui.home.seek = Some(fraction.clamp(0.0, 1.0));
        unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn drag_seek(&mut self, x: i32) {
        if !self.pointer.seek_drag {
            return;
        }
        self.ui.home.seek = Some(if self.ui.tab == view::Tab::Yandex {
            view::wave_fraction(self.pill_rect(true), self.scale, x as f32)
        } else {
            let (l, r, _) = view::seek_track(self.pill_rect(true), self.scale, view::home_wide(&self.ui));
            ((x as f32 - l) / (r - l).max(1.0)).clamp(0.0, 1.0)
        });
        self.repaint_expanded();
    }

    pub(in crate::island) fn finish_seek(&mut self) {
        if !self.pointer.seek_drag {
            return;
        }
        self.pointer.seek_drag = false;
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        }
        let wave = self.ui.tab == view::Tab::Yandex;
        let progress = if wave { crate::services::yandex::progress() } else { media::progress() };
        if let (Some(f), Some((_, len))) = (self.ui.home.seek.take(), progress) {
            if wave {
                crate::services::yandex::send(crate::services::yandex::Command::Seek(f * len));
            } else {
                media::seek(f * len);
            }
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn home_action(&mut self, i: usize) {
        match i {
            0 | 1 | 2 | 3 | 4 | 6 => {
                if i == 6 && self.ui.recording_secs.is_some() {
                    self.collapse();
                    return self.toggle_recording();
                }
                self.collapse();
                match i {
                    0 => self.capture(capture::Mode::Area),
                    1 => capture::fullscreen(self.hwnd),
                    2 => self.capture(capture::Mode::Ocr),
                    3 => self.capture(capture::Mode::Qr),
                    4 => self.capture(capture::Mode::Picker),
                    _ => self.toggle_recording(),
                }
            }
            5 => {
                self.collapse();
                self.plain_paste();
            }
            7 => self.toggle_command(),
            _ => self.home_info_click(i),
        }
    }

    pub(in crate::island) fn refresh_home(&mut self) {
        if let Some(v) = volume::get() {
            self.ui.home.vol = v;
        }
        self.ui.home.device = volume::device_name().unwrap_or_default();
        self.refresh_screen_time();
        if self.ui.home.vol_track {
            match volume::app_volume(&self.ui.media.source) {
                Some(v) => self.ui.home.app_vol = v,
                None => self.ui.home.vol_track = false,
            }
        }
        self.start_progress_timer();
    }

    pub(in crate::island) fn start_progress_timer(&self) {
        let home = self.state == State::Expanded
            && match self.ui.tab {
                view::Tab::Home => self.ui.media.playing,
                view::Tab::Yandex => self.ui.yandex.state.media.playing,
                _ => false,
            };
        let pill = self.progress_pill_visible() && view::progress_strip_on(&self.ui);
        if home || pill {
            let interval = if pill && view::pill_progress_fill_on(&self.ui) { self.frame_ms() } else { 1000 };
            self.set_timer(Timer::Progress, interval);
        } else {
            self.kill_timer(Timer::Progress);
        }
    }

    pub(in crate::island) fn progress_pill_visible(&self) -> bool {
        self.pill_motion.intro.is_none()
            && !self.notification_card_visible()
            && self.state == State::Collapsed
            && !self.gaming
            && (!self.pill_hidden || (self.ghosted && self.settings.ghost_opacity > 0.0))
            && unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub(in crate::island) fn reload_dock(&mut self) {
        self.ui.home.dock = self.store.as_ref().map(Store::dock).unwrap_or_default();
    }

    pub(in crate::island) fn open_dock_item(&mut self, i: usize) {
        let Some(path) = view::dock_path(&self.ui, i).map(str::to_string) else { return };
        if std::path::Path::new(&path).exists() {
            cmd::shell_open(&path, None);
            self.collapse();
        } else {
            if let (Some(item), Some(store)) = (self.ui.home.dock.get(i), &self.store) {
                store.dock_remove(item.id);
            }
            self.reload_dock();
            self.notify(Notice::Error, '\u{E783}', "File not found");
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn finish_dock_dialog(&mut self, picked: Option<String>) {
        self.popup_open = false;
        self.pointer.tracking = false;
        if let (Some(path), Some(store)) = (picked, &self.store) {
            store.dock_add(&path);
            self.reload_dock();
        }
        self.repaint_expanded();
        if !self.cursor_inside() {
            self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(1));
        }
    }

    pub(in crate::island) fn dock_target_at(&self, x: i32, y: i32) -> Option<(usize, String)> {
        if self.state != State::Expanded || self.ui.tab != view::Tab::Home || self.ui.home.dev_menu.is_some() {
            return None;
        }
        let pill = self.pill_rect(true);
        match view::hit_home(pill, self.scale, x as f32, y as f32, &self.ui) {
            Some(view::HomeHit::Dock(i)) => view::dock_path(&self.ui, i).map(|p| (i, p.to_string())),
            _ => None,
        }
    }

    pub(in crate::island) fn apply_dock_choice(&mut self, choice: Option<usize>, index: usize, path: &str) {
        self.pointer.tracking = false;
        match choice {
            Some(1) => cmd::reveal(std::path::Path::new(path)),
            Some(2) => cmd::shell_open(path, Some("runas")),
            Some(3) => {
                if let (Some(item), Some(store)) = (self.ui.home.dock.get(index), &self.store) {
                    store.dock_remove(item.id);
                }
                self.reload_dock();
            }
            _ => {}
        }
        self.repaint_expanded();
        if !self.cursor_inside() {
            self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(1));
        }
    }

    fn dock_slots(&self) -> Vec<view::Rect> {
        view::dock_layout(self.pill_rect(true), self.scale, &self.ui).slots
    }

    pub(in crate::island) fn step_reorder(&mut self) -> bool {
        let slots = self.dock_slots();
        let Some(r) = self.ui.home.reorder.as_mut() else { return false };
        let cursor = r.cursor;
        r.step(&slots, cursor)
    }

    pub(in crate::island) fn take_drag(&mut self, x: i32, y: i32, left_down: bool) -> Option<std::path::PathBuf> {
        if !left_down || self.ui.tab != view::Tab::Home || self.state != State::Expanded {
            return None;
        }
        let (fx, fy) = (x as f32, y as f32);
        if let Some(from) = self.ui.home.reorder.as_ref().map(|r| r.from) {
            let (px, py, pw, ph) = self.pill_rect(true);
            let margin = 6.0 * self.scale;
            let inside = fx >= px - margin && fx < px + pw + margin && fy >= py - margin && fy < py + ph + margin;
            if inside {
                if let Some(r) = self.ui.home.reorder.as_mut() {
                    r.cursor = (fx, fy);
                }
                self.step_reorder();
                self.set_timer(Timer::UiAnim, self.frame_ms());
                self.repaint_expanded();
                return None;
            }
            self.ui.home.reorder = None;
            self.ui.home.dock_press = None;
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            self.repaint_expanded();
            let path = std::path::PathBuf::from(self.ui.home.dock.get(from)?.path.clone());
            return path.exists().then_some(path);
        }
        let (index, x0, y0) = self.ui.home.dock_press?;
        const THRESHOLD: i32 = 6;
        if (x - x0).abs() < THRESHOLD && (y - y0).abs() < THRESHOLD {
            return None;
        }
        let slots = self.dock_slots();
        if index >= slots.len() {
            self.ui.home.dock_press = None;
            return None;
        }
        self.ui.home.reorder = Some(view::Reorder::new(&slots, index, (fx, fy)));
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
        }
        self.set_timer(Timer::UiAnim, self.frame_ms());
        self.repaint_expanded();
        None
    }

    pub(in crate::island) fn finish_dock_press(&mut self) {
        let press = self.ui.home.dock_press.take();
        if let Some(r) = self.ui.home.reorder.take() {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            if r.to != r.from
                && let (Some(item), Some(store)) = (self.ui.home.dock.get(r.from), &self.store)
            {
                store.dock_move(item.id, r.to);
                self.reload_dock();
            }
            return self.repaint_expanded();
        }
        if let Some((i, ..)) = press
            && self.state == State::Expanded
            && self.ui.tab == view::Tab::Home
        {
            self.open_dock_item(i);
        }
    }

    fn tile_slots(&self) -> Vec<view::Rect> {
        let pill = self.pill_rect(true);
        (0..self.settings.home_actions.len().min(view::MAX_HOME_TILES))
            .map(|i| view::action_tile(pill, self.scale, i))
            .collect()
    }

    pub(in crate::island) fn step_tile_reorder(&mut self) -> bool {
        let slots = self.tile_slots();
        let Some(r) = self.ui.home.tile_reorder.as_mut() else { return false };
        let cursor = r.cursor;
        r.step(&slots, cursor)
    }

    pub(in crate::island) fn drag_tiles(&mut self, x: i32, y: i32) {
        if self.drag_widget_resize(x, y) || self.drag_widget(x, y) {
            return;
        }
        if self.state != State::Expanded || self.ui.tab != view::Tab::Home {
            return;
        }
        let cursor = (x as f32, y as f32);
        if let Some(r) = self.ui.home.tile_reorder.as_mut() {
            r.cursor = cursor;
            self.step_tile_reorder();
            self.set_timer(Timer::UiAnim, self.frame_ms());
            return self.repaint_expanded();
        }
        let Some((slot, x0, y0)) = self.ui.home.tile_press else { return };
        if (x - x0).abs() < 6 && (y - y0).abs() < 6 {
            return;
        }
        let slots = self.tile_slots();
        if slot >= slots.len() {
            self.ui.home.tile_press = None;
            return;
        }
        self.ui.home.tile_reorder = Some(view::Reorder::new(&slots, slot, cursor));
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
        }
        self.set_timer(Timer::UiAnim, self.frame_ms());
        self.repaint_expanded();
    }

    pub(in crate::island) fn finish_tile_press(&mut self) {
        if self.finish_widget_resize() || self.finish_widget_drag() {
            return;
        }
        let press = self.ui.home.tile_press.take();
        if let Some(r) = self.ui.home.tile_reorder.take() {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            if r.to != r.from && r.from < self.settings.home_actions.len() {
                let id = self.settings.home_actions.remove(r.from);
                let at = r.to.min(self.settings.home_actions.len());
                self.settings.home_actions.insert(at, id);
                self.ui.cfg = self.settings.clone();
                self.settings.save();
            }
            return self.repaint_expanded();
        }
        if let Some((slot, ..)) = press
            && self.state == State::Expanded
            && self.ui.tab == view::Tab::Home
            && let Some(&id) = self.settings.home_actions.get(slot)
        {
            self.home_action(usize::from(id));
        }
    }
}
