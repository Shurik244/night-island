use std::ffi::c_void;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, EndPaint, GetDC, PAINTSTRUCT, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};

use crate::island::*;

const MAX_SEARCH_CHARS: usize = 40;
const SCROLL_STEP: f32 = 64.0;
pub(in crate::island) const WM_SETTINGS_WINDOW_CHANGED: u32 = WM_APP + 72;
static SETTINGS_REFRESH_PENDING: AtomicBool = AtomicBool::new(false);

fn notify_settings_window_changed(hwnd: HWND) {
    unsafe {
        let parent = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut _);
        if !parent.0.is_null()
            && !SETTINGS_REFRESH_PENDING.swap(true, Ordering::AcqRel)
            && PostMessageW(Some(parent), WM_SETTINGS_WINDOW_CHANGED, WPARAM(0), LPARAM(0)).is_err()
        {
            SETTINGS_REFRESH_PENDING.store(false, Ordering::Release);
        }
    }
}

pub(in crate::island) fn on_settings_window_changed() {
    SETTINGS_REFRESH_PENDING.store(false, Ordering::Release);
    with_app(|a| {
        if let Some(dock) = &mut a.desktop_dock {
            dock.refresh_windows();
        }
    });
}

impl App {
    pub(in crate::island) fn open_guide(&mut self) {
        self.sw_focus(None);
        self.end_rebind();
        self.ui.sw.dropdown = None;
        self.ui.sw.hover = None;
        self.ui
            .sw
            .guide
            .get_or_insert_with(|| view::GuideUi::new(&self.settings, crate::ui::feedback::motion_allowed()))
            .visible = true;
        self.refresh_guide_voice();
        self.open_settings_window();
        self.render_settings_win();
    }

    pub(in crate::island) fn refresh_guide_voice(&mut self) -> bool {
        let Some(guide) = self.ui.sw.guide.as_mut() else { return false };
        guide.refresh_settings(&self.settings);
        guide.refresh_models();
        guide.connect_model = ai::has_key(self.settings.ai.provider);
        let mut speech = self.settings.ai.clone();
        guide.apply_speech_choice(&mut speech);
        guide.connect_speech = ai::can_transcribe(&speech) && ai::voice_ready(&speech);
        guide.visible
    }

    fn on_guide_action(&mut self, action: view::GuideAction) {
        use crate::platform::{localvoice, wake_install};
        use view::GuideAction::*;
        if action == Resume {
            return self.open_guide();
        }
        let Some(guide) = self.ui.sw.guide.as_mut().filter(|guide| guide.visible) else { return };
        self.ui.sw.hover = None;
        let animate = crate::ui::feedback::motion_allowed();
        match action {
            Back => guide.move_by(-1, animate),
            Mode(mode) => guide.choose_mode(mode, animate),
            ChangeRoute => guide.change_route(),
            Extend => guide.extend(animate),
            Lesson(lesson) => guide.lesson(lesson, animate),
            Preview(action) => {
                guide.preview_action(action);
                if guide.preview_dragging() {
                    unsafe { SetCapture(self.settings_win.unwrap_or(self.hwnd)) };
                }
            }
            Select(preset) => guide.select(preset, animate),
            ToggleQuick(feature) => guide.toggle_quick(feature),
            ToggleAssistant => guide.toggle_assistant(),
            ToggleSpeechService => {
                guide.toggle_speech_service();
                self.refresh_guide_voice();
            }
            SelectAccess(access) => guide.select_access(access),
            SuggestWake => {
                guide.wake.set_text("Луна, Табуретка");
                guide.invalid_wake = false;
            }
            Download(model) if guide.can_download(model) => {
                match model {
                    view::VoiceModel::Wake => {
                        wake_install::start(self.hwnd);
                    }
                    view::VoiceModel::Local => {
                        localvoice::install(self.hwnd);
                    }
                }
                guide.poll_models();
            }
            ToggleModel(model) => guide.toggle_model(model),
            Next if !guide.last_page() => {
                if guide.voice_page()
                    && guide.wake_changed()
                    && !guide.wake.text().trim().is_empty()
                    && guide.validated_wake().is_none()
                {
                    guide.invalid_wake = true;
                    return self.render_settings_win();
                }
                guide.move_by(1, animate);
                self.sw_focus(None);
            }
            Next | Skip => {
                if action == Next {
                    self.apply_guide();
                }
                if let Some(hwnd) = self.settings_win {
                    unsafe {
                        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
                return;
            }
            OpenAssistant | OpenRecognition => {
                guide.visible = false;
                self.sw_focus(None);
                self.ui.sw.assistant_page = if action == OpenRecognition {
                    self.sync_ai_fields();
                    view::AssistantPage::Recognition
                } else {
                    view::AssistantPage::Model
                };
                if let Some(group) = view::GROUPS.iter().position(|group| group.name == "Assistant") {
                    self.open_settings_at(group);
                }
            }
            Download(_) | Resume => {}
        }
        self.render_settings_win();
    }

    fn apply_guide(&mut self) {
        let Some(guide) = self.ui.sw.guide.as_ref() else { return };
        let previous = self.settings.clone();
        guide.apply_choices(&mut self.settings);
        if guide.wake_changed()
            && let Some(wake) = guide.validated_wake()
        {
            self.settings.ai.voice_wake = wake;
            self.ui.int.wake.set_text(&self.settings.ai.voice_wake);
        }
        ai::configure(&self.settings.ai);
        self.settings.dismiss_tour();
        self.settings.assistant.apply();
        self.ui.cfg = self.settings.clone();
        if previous.desktop_dock != self.settings.desktop_dock {
            self.configure_desktop_dock();
        }
        if previous.top_panel != self.settings.top_panel {
            self.configure_top_panel();
        }
        if previous.weather != self.settings.weather {
            weather::set_enabled(self.settings.weather);
        }
        if previous.character != self.settings.character {
            self.apply_character();
            self.sync_pet_studio();
        }
        if previous.game_mode != self.settings.game_mode {
            self.start_game_watch();
        }
        if previous.system_notifications != self.settings.system_notifications {
            self.configure_system_notifications();
            if self.settings.system_notifications {
                let _ = crate::platform::notifications::request_access(self.hwnd);
            }
        }
        if previous.assistant.perms.enabled && !self.settings.assistant.perms.enabled {
            self.cancel_voice_input();
        }
        if previous.ai.voice_wake != self.settings.ai.voice_wake {
            self.restart_wake();
        }
        self.sync_wake();
        self.update_eq();
        self.repaint_collapsed();
        self.repaint_expanded();
        self.ui.cfg = self.settings.clone();
        self.settings.save();
    }

    pub(in crate::island) fn open_settings_at(&mut self, group: usize) {
        self.close_memory_tools();
        self.open_settings_window();
        if self.settings_win.is_some() {
            self.ui.sw.group = group;
            self.ui.sw.scroll = 0.0;
            self.ui.sw.dropdown = None;
            self.set_search("");
            self.render_settings_win();
        }
    }

    pub(in crate::island) fn open_settings_window(&mut self) {
        if let Some(hwnd) = self.settings_win {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
        static CLASS: Once = Once::new();
        let dpi = self.dpi_scale;
        let (cw, ch) = ((view::SW_SIZE.0 * dpi).round() as i32, (view::SW_SIZE.1 * dpi).round() as i32);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        unsafe {
            let Ok(instance) = GetModuleHandleW(None) else { return };
            CLASS.call_once(|| {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(sw_proc),
                    hInstance: instance.into(),
                    lpszClassName: w!("NightIsland.Settings"),
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    ..Default::default()
                };
                RegisterClassW(&class);
            });
            let (ow, oh) = (cw, ch);
            let (l, t, r, b) = Self::work_area();
            let x = (l + ((r - l) - ow as f32) / 2.0).max(l) as i32;
            let y = (t + ((b - t) - oh as f32) / 2.0).max(t) as i32;
            let Ok(hwnd) = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("NightIsland.Settings"),
                &windows::core::HSTRING::from(format!("Night Island · {}", crate::ui::i18n::tr("Settings"))),
                style,
                x,
                y,
                ow,
                oh,
                None,
                None,
                Some(instance.into()),
                None,
            ) else {
                crate::log_error!("could not create the settings window");
                return;
            };
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, self.hwnd.0 as isize);
            for (kind, cx, cy) in [
                (ICON_SMALL, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)),
                (ICON_BIG, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON)),
            ] {
                let icon = crate::ui::tray::app_icon(cx, cy);
                SendMessageW(hwnd, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(icon.0 as isize)));
            }
            let round: i32 = 2;
            let rounded = DwmSetWindowAttribute(
                hwnd,
                windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(33),
                (&raw const round).cast::<c_void>(),
                4,
            );
            if rounded.is_err() {
                let radius = (24.0 * dpi).round() as i32;
                let region = windows::Win32::Graphics::Gdi::CreateRoundRectRgn(0, 0, ow + 1, oh + 1, radius, radius);
                windows::Win32::Graphics::Gdi::SetWindowRgn(hwnd, Some(region), true);
            }
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            self.settings_win = Some(hwnd);
            let guide = self.ui.sw.guide.take();
            self.ui.sw = view::SwUi { guide, ..Default::default() };
            self.ui.int.spotify_id.set_text(&spotify::own_client_id());
            self.sync_voice_fields();
            self.reload_groups();
            spotify::reload_status();
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
        self.render_settings_win();
    }

    pub(in crate::island) fn sw_title_click(&self, x: i32, y: i32) -> u8 {
        match view::sw_hit(self.sw_size(), self.dpi_scale, &self.ui, x as f32, y as f32) {
            Some(view::SwHit::Minimize) => 1,
            Some(view::SwHit::Close) => 2,
            _ => 0,
        }
    }

    pub(in crate::island) fn sw_nc_zone(&self, x: i32, y: i32) -> Option<view::TitleZone> {
        view::sw_title_zone(self.sw_size(), self.dpi_scale, x as f32, y as f32)
    }

    fn sw_client(&self) -> Option<(i32, i32)> {
        let hwnd = self.settings_win?;
        let mut rc = windows::Win32::Foundation::RECT::default();
        unsafe { GetClientRect(hwnd, &mut rc).ok()? };
        Some((rc.right, rc.bottom))
    }

    fn sw_size(&self) -> (f32, f32) {
        self.sw_client().map_or(view::SW_SIZE, |(w, h)| (w as f32, h as f32))
    }

    pub(in crate::island) fn render_settings_win(&self) {
        self.ui.feedback.set_enabled(crate::ui::feedback::motion_allowed());
        let (Some(hwnd), Some((w, h))) = (self.settings_win, self.sw_client()) else { return };
        if w <= 0 || h <= 0 {
            return;
        }
        unsafe {
            let screen = GetDC(Some(hwnd));
            let mem = CreateCompatibleDC(Some(screen));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            if let Ok(dib) = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                let old = SelectObject(mem, dib.into());
                let painted = self.gpu.paint_dc(mem, (w, h), |p| {
                    let _ = view::paint_settings_window(p, (w as f32, h as f32), self.dpi_scale, &self.ui);
                });
                if painted.is_ok() {
                    let _ = BitBlt(screen, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
                }
                SelectObject(mem, old);
                let _ = DeleteObject(dib.into());
            }
            let _ = DeleteDC(mem);
            ReleaseDC(Some(hwnd), screen);
        }
        if self.ui.feedback.active() || self.ui.sw.guide.as_ref().is_some_and(view::GuideUi::animating) {
            self.set_timer(Timer::SwAnim, 16);
        } else if self.ui.sw.guide.as_ref().is_some_and(view::GuideUi::preview_ticking) {
            self.set_timer(Timer::SwAnim, 100);
        } else if self.ui.sw.guide.as_ref().is_some_and(view::GuideUi::downloading) {
            self.set_timer(Timer::SwAnim, 180);
        }
    }

    pub(in crate::island) fn on_sw_move(&mut self, x: i32, y: i32, left_down: bool) {
        let size = self.sw_size();
        if let Some(guide) = self.ui.sw.guide.as_mut().filter(|guide| guide.visible)
            && guide.preview_move(size, self.dpi_scale, x as f32, y as f32, left_down)
        {
            self.render_settings_win();
        }
        if left_down && self.ui.sw.drag.is_some() {
            return self.sw_drag(x);
        }
        if left_down && self.ui.sw.field == Some(view::Field::Search) && self.ui.sw.search_press {
            self.sw_click_field(view::Field::Search, x, true, true);
            return self.render_settings_win();
        }
        let hit = view::sw_hit(size, self.dpi_scale, &self.ui, x as f32, y as f32);
        if hit != self.ui.sw.hover {
            self.ui.sw.hover = hit;
            self.sw_plates_move();
            self.render_settings_win();
        }
    }

    fn sw_plates_move(&mut self) {
        if let Some(now) = view::Plate::of(self.ui.sw.hover)
            && !self.ui.sw.plates.iter().any(|(k, _)| *k == now)
        {
            self.ui.sw.plates.push((now, 0.0));
        }
        self.set_timer(Timer::SwAnim, 16);
    }

    pub(in crate::island) fn sw_anim_tick(&mut self) {
        if self.settings_win.is_none() {
            self.ui.sw.plates.clear();
            return;
        }
        if let Some(guide) = self.ui.sw.guide.as_mut().filter(|guide| guide.downloading()) {
            guide.poll_models();
        }
        if let Some(guide) = self.ui.sw.guide.as_mut() {
            guide.preview_tick();
        }
        let now = view::Plate::of(self.ui.sw.hover);
        let mut moving = false;
        for (key, level) in &mut self.ui.sw.plates {
            let target = if Some(*key) == now { 1.0 } else { 0.0 };
            let step = if crate::ui::feedback::motion_allowed() { 0.16 } else { 1.0 };
            *level = if *level < target { (*level + step).min(target) } else { (*level - step).max(target) };
            moving |= *level != target;
        }
        self.ui.sw.plates.retain(|(key, level)| *level > 0.0 || Some(*key) == now);
        self.render_settings_win();
        if moving || self.ui.feedback.active() {
            self.set_timer(Timer::SwAnim, 16);
        }
    }

    pub(in crate::island) fn on_sw_down(&mut self, x: i32, y: i32) {
        self.set_timer(Timer::SwAnim, 16);
        let size = self.sw_size();
        let hit = view::sw_hit(size, self.dpi_scale, &self.ui, x as f32, y as f32);
        if let Some(id) = self.ui.sw.dropdown.take() {
            if let Some(view::SwHit::Pick(_, k)) = hit {
                view::cycle_pick(id, &mut self.settings, k);
                self.setting_changed(id, true);
            }
            return self.render_settings_win();
        }
        self.ui.sw.quiet_sys = false;
        let waiting = self.ui.sw.capturing;
        if waiting.is_some() {
            self.end_rebind();
            if hit == waiting.map(|h| view::SwHit::Act(view::Act::Rebind(h))) {
                return self.render_settings_win();
            }
        }
        if let Some(view::SwHit::Field(field)) = hit {
            self.sw_focus(Some(field));
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            self.sw_click_field(field, x, shift, false);
            if field == view::Field::Search {
                self.ui.sw.search_press = true;
                unsafe {
                    let _ = SetCapture(self.settings_win.unwrap_or(self.hwnd));
                }
            }
            return self.render_settings_win();
        }
        self.sw_focus(None);
        match hit {
            Some(view::SwHit::Group(i)) => {
                self.close_memory_tools();
                self.ui.sw.group = i;
                self.ui.sw.scroll = 0.0;
                self.set_search("");
            }
            Some(view::SwHit::Guide(action)) => return self.on_guide_action(action),
            Some(view::SwHit::Memory(action)) => return self.on_memory_action(action),
            Some(view::SwHit::AssistantPage(page)) => {
                self.ui.sw.assistant_page = page;
                self.ui.sw.scroll = 0.0;
            }
            Some(view::SwHit::Row(id, hit)) => self.sw_row(id, hit, x),
            Some(view::SwHit::Act(act)) => self.sw_act(act),
            Some(view::SwHit::Theme(hit)) => {
                match hit {
                    view::ThemeHit::Preset(i) => crate::ui::theme::apply_preset(&mut self.settings, i),
                    view::ThemeHit::Panel(i) => crate::ui::theme::set_panel_swatch(&mut self.settings, i),
                    view::ThemeHit::Accent(i) => crate::ui::theme::set_accent_swatch(&mut self.settings, i),
                }
                self.apply_theme();
                self.ui.cfg = self.settings.clone();
                self.settings.save();
                self.repaint_expanded();
            }
            Some(view::SwHit::Field(_) | view::SwHit::Minimize | view::SwHit::Close | view::SwHit::Pick(..)) | None => {
            }
        }
        self.render_settings_win();
    }

    fn sw_focus(&mut self, field: Option<view::Field>) {
        if self.ui.sw.field == field {
            return;
        }
        if self.ui.sw.field == Some(view::Field::WakeWord) {
            self.commit_wake();
        }
        self.ui.sw.field = field;
        self.ui.sw.caret_on = true;
        if field.is_some() {
            self.set_timer(Timer::SwCaret, CARET_BLINK_MS);
        } else {
            self.kill_timer(Timer::SwCaret);
        }
    }

    pub(in crate::island) fn sw_caret_tick(&mut self) {
        if self.ui.sw.field.is_none() || self.settings_win.is_none() {
            return;
        }
        self.ui.sw.caret_on = !self.ui.sw.caret_on;
        self.set_timer(Timer::SwCaret, CARET_BLINK_MS);
        self.render_settings_win();
    }

    fn sw_edit_mut(&mut self) -> Option<&mut edit::TextEdit> {
        Some(match self.ui.sw.field? {
            view::Field::DiscordId => &mut self.ui.int.id,
            view::Field::DiscordSecret => &mut self.ui.int.secret,
            view::Field::SpotifyId => &mut self.ui.int.spotify_id,
            view::Field::AiKey => &mut self.ui.int.ai_key,
            view::Field::AiModel => &mut self.ui.int.ai_model,
            view::Field::AiBase => &mut self.ui.int.ai_base,
            view::Field::VoiceKey => &mut self.ui.int.voice_key,
            view::Field::VoiceModel => &mut self.ui.int.voice_model,
            view::Field::VoiceBase => &mut self.ui.int.voice_base,
            view::Field::TtsKey => &mut self.ui.int.tts_key,
            view::Field::TtsBase => &mut self.ui.int.tts_base,
            view::Field::TtsModel => &mut self.ui.int.tts_model,
            view::Field::TtsVoice => &mut self.ui.int.tts_voice,
            view::Field::WakeWord => &mut self.ui.int.wake,
            view::Field::GuideWake => &mut self.ui.sw.guide.as_mut()?.wake,
            view::Field::GroupName => &mut self.ui.int.group_name,
            view::Field::Search => &mut self.ui.int.search,
        })
    }

    fn sw_click_field(&mut self, field: view::Field, x: i32, extend: bool, dragging: bool) {
        let Some((_, inner)) = view::sw_field_rects(self.sw_size(), self.dpi_scale, &self.ui, field) else { return };
        let shown = {
            let text = view::field_edit(&self.ui, field).text();
            if matches!(
                field,
                view::Field::DiscordSecret | view::Field::AiKey | view::Field::VoiceKey | view::Field::TtsKey
            ) {
                "\u{2022}".repeat(text.chars().count())
            } else {
                text
            }
        };
        let Ok(layout) = crate::ui::gfx::make_layout(self.gpu.dwrite(), &shown, 100_000.0, 13.0 * self.dpi_scale)
        else {
            return;
        };
        if let Some(n) = self.sw_edit_mut() {
            let fx = if (x as f32) < inner.0 && !dragging { 1.0e6 } else { x as f32 - inner.0 };
            n.click(&layout, fx, 4.0, extend);
        }
    }

    pub(in crate::island) fn sync_ai_fields(&mut self) {
        let provider = self.settings.ai.provider;
        self.ui.int.ai_model.set_text(self.settings.ai.model(provider));
        self.ui.int.ai_base.set_text(&self.settings.ai.openai_base);
        self.ui.int.ai_key.set_text("");
        self.ui.int.ai_key_saved = ai::has_key(provider);
        let mut speech = self.settings.ai.clone();
        if let Some(guide) = &self.ui.sw.guide {
            guide.apply_speech_choice(&mut speech);
        }
        self.ui.int.voice_key_shared =
            ai::voice_shares_chat_key(&speech) && ai::has_key(crate::data::settings::AiProvider::OpenAi);
    }

    pub(in crate::island) fn sync_voice_fields(&mut self) {
        self.ui.int.voice_model.set_text(&self.settings.ai.voice_model);
        self.ui.int.voice_base.set_text(&self.settings.ai.voice_base);
        self.ui.int.voice_key.set_text("");
        self.ui.int.voice_key_saved = ai::has_voice_key();
        self.ui.int.tts_key.set_text("");
        self.ui.int.tts_base.set_text(&self.settings.assistant.tts_base);
        self.ui.int.tts_model.set_text(&self.settings.assistant.tts_model);
        self.ui.int.tts_voice.set_text(&self.settings.assistant.tts_voice);
        self.ui.int.tts_key_saved = ai::has_tts_key(self.settings.assistant.tts_provider);
        self.ui.int.tts_key_shared =
            ai::tts_key_is_shared(&self.settings.assistant.tts_base, self.settings.assistant.tts_provider);
        self.ui.int.wake.set_text(&self.settings.ai.voice_wake);
        self.sync_ai_fields();
    }

    fn commit_wake(&mut self) {
        let typed = self.ui.int.wake.text();
        if let Some(wake) = crate::tools::voicecmd::clean_wake_setting(&typed)
            && wake != self.settings.ai.voice_wake
        {
            self.settings.ai.voice_wake = wake;
            self.settings.save();
            self.ui.cfg = self.settings.clone();
            ai::configure(&self.settings.ai);
            self.restart_wake();
        }
    }

    fn save_wake(&mut self) {
        let Some(wake) = crate::tools::voicecmd::clean_wake_setting(&self.ui.int.wake.text()) else {
            return self.notify(Notice::Error, '\u{E783}', "Up to 5 names of up to 24 characters, separated by commas");
        };
        self.settings.ai.voice_wake = wake;
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        ai::configure(&self.settings.ai);
        self.restart_wake();
        self.sync_voice_fields();
        self.notify(Notice::Integrations, '\u{E73E}', "Command name saved");
    }

    fn save_voice(&mut self) {
        let key = self.ui.int.voice_key.text();
        let model = self.ui.int.voice_model.text().trim().to_string();
        let model = if model.is_empty() { "whisper-1".to_string() } else { model };
        if !ai::valid_model(&model, crate::data::settings::AiProvider::OpenAi) {
            return self.notify(Notice::Error, '\u{E783}', "That model name is not valid");
        }
        let Some(base) = ai::clean_base(&self.ui.int.voice_base.text()) else {
            return self.notify(Notice::Error, '\u{E783}', "The server address must be https:// (or local http://)");
        };
        if !key.trim().is_empty() && !ai::save_voice_key(&key) {
            return self.notify(Notice::Error, '\u{E783}', "That does not look like an API key");
        }
        self.settings.ai.voice_model = model;
        self.settings.ai.voice_base = base;
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        ai::configure(&self.settings.ai);
        self.sync_voice_fields();
        self.notify(Notice::Integrations, '\u{E73E}', "Speech service saved");
    }

    fn save_tts(&mut self) {
        let key = self.ui.int.tts_key.text();
        if !key.trim().is_empty() && !ai::save_tts_key(self.settings.assistant.tts_provider, &key) {
            return self.notify(Notice::Error, '\u{E783}', "That does not look like an API key");
        }
        let Some(base) = ai::clean_base(&self.ui.int.tts_base.text()) else {
            return self.notify(Notice::Error, '\u{E783}', "The server address must be https:// (or local http://)");
        };
        let given = self.ui.int.tts_model.text().trim().to_string();
        let model = if given.is_empty() { self.settings.assistant.tts_model.clone() } else { given };
        if !ai::valid_model(&model, crate::data::settings::AiProvider::OpenAi) {
            return self.notify(Notice::Error, '\u{E783}', "That model name is not valid");
        }
        let given = self.ui.int.tts_voice.text().trim().to_string();
        let voice = if given.is_empty() { self.settings.assistant.tts_voice.clone() } else { given };
        let voice_ok =
            voice.len() <= 40 && voice.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !voice_ok {
            return self.notify(Notice::Error, '\u{E783}', "That voice name is not valid");
        }
        self.settings.assistant.tts_base = base;
        self.settings.assistant.tts_model = model;
        self.settings.assistant.tts_voice = voice;
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        self.sync_voice_fields();
        self.notify(Notice::Integrations, '\u{E73E}', "Voice service saved");
    }

    pub(in crate::island) fn on_ai_checked(&mut self) {
        let Some((serial, key, result)) = ai::take_connection_check() else { return };
        if serial != self.ui.int.ai_check_serial {
            return;
        }
        self.ui.int.ai_check_pending = false;
        if key != ai::connection_key(&self.settings.ai) {
            self.render_settings_win();
            return;
        }

        self.ui.int.ai_check = Some(result);
        self.render_settings_win();
    }

    fn check_ai(&mut self) {
        self.ui.int.ai_check_key = ai::connection_key(&self.settings.ai);
        self.ui.int.ai_check_pending = true;
        self.ui.int.ai_check = None;
        self.ui.int.ai_check_serial = ai::check_connection(self.hwnd, self.settings.ai.clone());
        self.render_settings_win();
    }

    fn save_ai(&mut self) {
        let provider = self.settings.ai.provider;
        let key = self.ui.int.ai_key.text();
        if !key.trim().is_empty() && !ai::valid_key(&key) {
            return self.notify(Notice::Error, '\u{E783}', "That does not look like an API key");
        }
        let model = self.ui.int.ai_model.text().trim().to_string();
        let model = if model.is_empty() { provider.default_model().to_string() } else { model };
        if !ai::valid_model(&model, provider) {
            return self.notify(Notice::Error, '\u{E783}', "That model name is not valid");
        }
        let mut next = self.settings.ai.clone();
        if provider == crate::data::settings::AiProvider::OpenAi {
            let Some(base) = ai::clean_base(&self.ui.int.ai_base.text()) else {
                return self.notify(
                    Notice::Error,
                    '\u{E783}',
                    "The server address must be https:// (or local http://)",
                );
            };
            next.openai_base = base;
        }
        next.set_model(provider, &model);
        if !key.trim().is_empty() && !ai::save_key(provider, &key) {
            return self.notify(Notice::Error, '\u{E783}', "Could not save the key");
        }
        self.settings.ai = next;
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        ai::configure(&self.settings.ai);
        self.sync_ai_fields();
        if self.settings.ai_check_on_save {
            self.check_ai();
        } else {
            self.ui.int.ai_check_pending = false;
            self.ui.int.ai_check = None;
            self.notify(Notice::Integrations, '\u{E73E}', "AI settings saved");
        }
    }

    fn begin_rebind(&mut self, hotkey: Hotkey) {
        self.ui.sw.capturing = Some(hotkey);
        unregister_hotkeys(self.hwnd);
    }

    pub(in crate::island) fn end_rebind(&mut self) {
        if self.ui.sw.capturing.take().is_some() && !self.gaming {
            register_hotkeys(self.hwnd, &self.settings);
        }
    }

    fn reapply_hotkeys(&mut self) -> Vec<Hotkey> {
        if self.gaming {
            return Vec::new();
        }
        unregister_hotkeys(self.hwnd);
        register_hotkeys(self.hwnd, &self.settings)
    }

    fn keys_changed(&mut self) {
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        self.render_settings_win();
    }

    fn capture_key(&mut self, hotkey: Hotkey, vk: u16) {
        use crate::data::keys::{ALT, CTRL, Combo, SHIFT, WIN, valid_vk};
        let down = |k: u16| unsafe { GetKeyState(i32::from(k)) < 0 || GetAsyncKeyState(i32::from(k)) < 0 };
        let vk32 = u32::from(vk);
        if matches!(vk32, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
            return;
        }
        let mut mods = 0;
        for (bit, keys) in [(CTRL, [0x11, 0xA2, 0xA3]), (ALT, [0x12, 0xA4, 0xA5]), (SHIFT, [0x10, 0xA0, 0xA1])] {
            if keys.into_iter().any(down) {
                mods |= bit;
            }
        }
        if down(0x5B) || down(0x5C) {
            mods |= WIN;
        }
        if mods & ALT != 0 {
            self.ui.sw.quiet_sys = true;
        }
        if vk32 == u32::from(VK_ESCAPE.0) {
            return self.set_hotkey(hotkey, Combo::NONE);
        }
        let combo = if mods == 0 && vk32 == u32::from(VK_DELETE.0) {
            hotkey.default_combo()
        } else if mods == 0 || !valid_vk(vk32) {
            let why = if mods == 0 { "Add Ctrl, Alt, Shift or Win" } else { "That key cannot be used" };
            return self.notify(Notice::Error, '\u{E783}', why);
        } else {
            Combo::new(mods, vk32)
        };
        if let Some(other) = self.settings.hotkey_using(hotkey, combo) {
            return self.notify(Notice::Error, '\u{E783}', &format!("Already used by: {}", other.label()));
        }
        self.set_hotkey(hotkey, combo);
    }

    fn set_hotkey(&mut self, hotkey: Hotkey, combo: crate::data::keys::Combo) {
        let old = self.settings.combo(hotkey);
        self.settings.set_combo(hotkey, combo);
        self.ui.sw.capturing = None;
        if self.reapply_hotkeys().contains(&hotkey) {
            self.settings.set_combo(hotkey, old);
            self.reapply_hotkeys();
            self.notify(Notice::Error, '\u{E783}', "Another program already uses these keys");
        }
        self.keys_changed();
    }

    pub(in crate::island) fn finish_font_import(&mut self) {
        let Some(result) = crate::ui::fonts::take_import() else { return };
        self.ui.sw.font_busy = false;
        match result {
            Ok(font) => {
                crate::ui::fonts::use_custom(font.data);
                self.settings.interface_font_file = font.path;
                self.settings.interface_font = crate::data::settings::InterfaceFont::Custom;
                self.ui.sw.font_status = crate::ui::i18n::tr("Font loaded and saved").into_owned();
                self.setting_changed(view::Sid::InterfaceFont, true);
            }
            Err(error) => self.ui.sw.font_status = crate::ui::i18n::tr(&error).into_owned(),
        }
        self.render_settings_win();
    }

    fn sw_act(&mut self, act: view::Act) {
        match act {
            view::Act::SoundImport(index) => {
                if !self.ui.sw.sound_busy && index < crate::platform::sounds::Cue::OPTIONS.len() {
                    unsafe {
                        let _ = PostMessageW(
                            Some(self.hwnd),
                            crate::platform::sounds::WM_SOUND_PICK,
                            WPARAM(index),
                            LPARAM(0),
                        );
                    }
                }
            }
            view::Act::SoundPreview(index) => {
                if let Some(cue) = crate::platform::sounds::Cue::OPTIONS.get(index) {
                    crate::platform::sounds::play(*cue, 100);
                }
            }
            view::Act::SoundReset(index) => {
                if let Some(cue) = crate::platform::sounds::Cue::OPTIONS.get(index) {
                    self.settings.sounds.custom.remove(cue.key());
                    self.setting_changed(view::Sid::SoundPreset, true);
                    self.render_settings_win();
                }
            }
            view::Act::SoundResetAll => {
                self.settings.sounds.custom.clear();
                self.settings.sounds.preset = crate::data::sounds::SoundPreset::Classic;
                self.setting_changed(view::Sid::SoundPreset, true);
                self.render_settings_win();
            }
            view::Act::FontDownload => {
                cmd::shell_open(crate::ui::fonts::download_url(self.settings.interface_font), None)
            }
            view::Act::FontImport => {
                if !self.ui.sw.font_busy {
                    unsafe {
                        let _ = PostMessageW(Some(self.hwnd), crate::ui::fonts::WM_FONT_PICK, WPARAM(0), LPARAM(0));
                    }
                }
            }
            view::Act::OpenGuide => self.open_guide(),
            view::Act::Rebind(hotkey) => self.begin_rebind(hotkey),
            view::Act::KeysReset => {
                self.settings.keys.clear();
                self.reapply_hotkeys();
                self.keys_changed();
            }
            view::Act::LyricsAppAdd => {
                let source = self.ui.media.source.clone();
                if self.settings.add_lyrics_app(&source) {
                    self.lyrics_apps_changed();
                }
            }
            view::Act::Agent(i) => {
                if let Some(agent) = agents::Agent::ALL.get(i).copied() {
                    let result =
                        if agents::connected(agent) { agents::disconnect(agent) } else { agents::connect(agent) };
                    if let Err(why) = result {
                        self.notify_for(Notice::Error, '\u{E783}', why, 5000);
                    }
                }
            }
            view::Act::Metric(i) => {
                if let Some(metric) = metrics::Metric::ALL.get(i) {
                    let key = metric.key().to_string();
                    let on_tab = self.settings.metrics_tab.contains(&key);
                    let on_pill = self.settings.metrics_pill.contains(&key);
                    let (to_tab, to_pill) = match (on_tab, on_pill) {
                        (false, false) => (true, false),
                        (true, false) => (false, true),
                        (false, true) => (true, true),
                        (true, true) => (false, false),
                    };
                    for (list, on) in
                        [(&mut self.settings.metrics_tab, to_tab), (&mut self.settings.metrics_pill, to_pill)]
                    {
                        list.retain(|k| *k != key);
                        if on {
                            list.push(key.clone());
                        }
                    }
                    self.metrics_changed();
                }
            }
            view::Act::DockRemove(i) => {
                if i < self.settings.desktop_dock.pins.len() {
                    self.settings.desktop_dock.pins.remove(i);
                    self.save_desktop_dock();
                }
            }
            view::Act::DockHiddenRemove(i) => {
                if i < self.settings.desktop_dock.hidden_apps.len() {
                    self.settings.desktop_dock.hidden_apps.remove(i);
                    self.save_desktop_dock();
                }
            }
            view::Act::LyricsAppRemove(i) => {
                if i < self.settings.lyrics_apps.len() {
                    self.settings.lyrics_apps.remove(i);
                    self.lyrics_apps_changed();
                }
            }
            view::Act::DiscordMain => match self.ui.discord.status {
                discord::Status::NoConfig => self.save_credentials(),
                discord::Status::Idle | discord::Status::Error(_) => discord::connect(),
                discord::Status::NotRunning | discord::Status::Connected => discord::disconnect(),
                discord::Status::Authorizing => {}
            },
            view::Act::DiscordMute => discord::set_mute(!self.ui.discord.self_mute),
            view::Act::YandexMain => {
                use crate::services::yandex::{self, Command};
                let state = &self.ui.yandex.state;
                yandex::send(if state.has_token {
                    Command::Disconnect
                } else if state.busy || !state.code.is_empty() {
                    Command::Cancel
                } else {
                    Command::Connect
                });
            }
            view::Act::YandexCoverCache => crate::services::yandex::send(
                crate::services::yandex::Command::CacheCovers(!self.ui.yandex.state.config.cache_covers),
            ),
            view::Act::YandexAutoplay => crate::services::yandex::send(crate::services::yandex::Command::Autoplay(
                !self.ui.yandex.state.config.autoplay,
            )),
            view::Act::YandexBrowser => cmd::shell_open("https://ya.ru/device", None),
            view::Act::YandexCopy => {
                self.ui.yandex.copied = clips::sys::write_text(self.hwnd, &self.ui.yandex.state.code);
            }
            view::Act::YandexWave => {
                if let Some(hwnd) = self.settings_win {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                }
                self.open_tab(view::Tab::Yandex);
            }
            view::Act::DiscordDeaf => discord::set_deaf(!self.ui.discord.self_deaf),
            view::Act::DiscordOverlay => self.toggle_overlay(),
            view::Act::SpotifyMain => match self.ui.spotify.status {
                spotify::Status::Connected => spotify::disconnect(),
                spotify::Status::Connecting => {}
                _ => {
                    spotify::reload_status();
                    spotify::connect();
                }
            },
            view::Act::UpdateCheck => {
                update::configure(self.settings.update_check, self.settings.update_beta);
                update::check(self.hwnd);
            }
            view::Act::UpdateDownload => {
                if let update::Status::Available(found) = update::state() {
                    update::download(self.hwnd, found);
                }
            }
            view::Act::UpdateInstall => {
                if let update::Status::Ready(_, path) = update::state() {
                    match update::install(&path) {
                        Ok(()) => unsafe {
                            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                        },
                        Err(e) => {
                            crate::log_warn!("could not install the update: {e}");
                            self.notify(Notice::Error, '\u{E783}', "The update could not be installed");
                        }
                    }
                }
            }
            view::Act::AiSave => self.save_ai(),
            view::Act::AiCheck => self.check_ai(),
            view::Act::GameAiSettings => {
                if let Some(group) = view::GROUPS.iter().position(|g| g.name == "Assistant") {
                    self.open_settings_at(group);
                    self.ui.sw.assistant_page = view::AssistantPage::Model;
                    self.render_settings_win();
                }
            }
            view::Act::GameAlwaysShow => {
                crate::platform::game_mode::set_rule(Some(false));
                self.on_game_timer();
                self.render_settings_win();
            }
            view::Act::GameTreatAsGame => {
                crate::platform::game_mode::set_rule(Some(true));
                self.on_game_timer();
                self.render_settings_win();
            }
            view::Act::GameAutomatic => {
                crate::platform::game_mode::set_rule(None);
                self.on_game_timer();
                self.render_settings_win();
            }
            view::Act::GameRemoveRule(index) => {
                if let Some((path, _)) = crate::platform::game_mode::rules().get(index) {
                    crate::platform::game_mode::remove_rule(path);
                }
                self.on_game_timer();
                self.render_settings_win();
            }
            view::Act::VoiceSave => self.save_voice(),
            view::Act::TtsSave => self.save_tts(),
            view::Act::TtsRemoveKey => {
                ai::remove_tts_key(self.settings.assistant.tts_provider);
                self.sync_voice_fields();
            }
            view::Act::WakeSave => self.save_wake(),
            view::Act::GroupAdd => {
                let name = self.ui.int.group_name.text();
                let added = self.store.as_ref().and_then(|s| s.act_group_add(&name)).is_some();
                if added {
                    self.ui.int.group_name.set_text("");
                    self.reload_groups();
                } else {
                    self.notify(Notice::Error, '\u{E783}', "That group name is taken, empty or one group too many");
                }
            }
            view::Act::GroupDelete(id) => {
                if let Some(store) = &self.store {
                    store.act_group_delete(id);
                }
                self.reload_groups();
                self.reload_activity();
            }
            view::Act::VoiceRemoveKey => {
                ai::remove_voice_key();
                self.sync_voice_fields();
            }
            view::Act::AiRemoveKey => {
                ai::remove_key(self.settings.ai.provider);
                self.ui.int.ai_check_pending = false;
                self.ui.int.ai_check = None;
                self.ui.int.ai_check_serial = 0;
                self.sync_ai_fields();
            }
            view::Act::SpotifySaveId => {
                if spotify::set_client_id(&self.ui.int.spotify_id.text()) {
                    self.notify(Notice::Integrations, '\u{E73E}', "Client ID saved");
                } else {
                    self.notify(Notice::Error, '\u{E783}', "That does not look like a Client ID");
                }
            }
        }
    }

    fn metrics_changed(&mut self) {
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        if let Some(sample) = metrics::latest() {
            self.ui.metrics.pill = view::pill_text(&sample, &self.settings.metrics_pill);
        }
        self.sync_metrics();
        self.repaint_collapsed();
    }

    fn lyrics_apps_changed(&mut self) {
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        self.ui.lyric = view::LyricUi::default();
        self.update_eq();
        self.repaint_collapsed();
    }

    fn sw_row(&mut self, id: view::Sid, hit: view::SetHit, x: i32) {
        match hit {
            view::SetHit::Option(n) => {
                if view::is_assistant_level(id)
                    || matches!(
                        id,
                        view::Sid::Character
                            | view::Sid::AsstTtsProvider
                            | view::Sid::PetPerformance
                            | view::Sid::Edge
                            | view::Sid::PillBattery
                            | view::Sid::AiProvider
                            | view::Sid::CoverMode
                            | view::Sid::TabAnimKind
                            | view::Sid::AnimCurve
                            | view::Sid::TextLayout
                    )
                {
                    view::cycle_set(id, &mut self.settings, n == 1);
                } else {
                    view::segment_set(id, &mut self.settings, n);
                }
                self.setting_changed(id, true);
                if id == view::Sid::AsstTtsProvider {
                    self.sync_voice_fields();
                }
            }
            view::SetHit::Label => {}
            view::SetHit::Select => self.ui.sw.dropdown = Some(id),
            view::SetHit::Toggle => {
                let on = !view::toggle_get(id, &self.settings);
                view::toggle_set(id, &mut self.settings, on);
                self.setting_changed(id, true);
            }
            view::SetHit::Slider => {
                self.pointer.slider = Some(id);
                self.ui.sw.drag = Some(id);
                self.sw_drag(x);
                unsafe { SetCapture(self.settings_win.unwrap_or_default()) };
            }
            view::SetHit::Button => {
                if id == view::Sid::MemoryManager {
                    self.open_memory_tools();
                }
                if matches!(
                    id,
                    view::Sid::DockImport
                        | view::Sid::DockImportTaskbar
                        | view::Sid::DockMonitor
                        | view::Sid::DockAdd
                        | view::Sid::DockTaskbar
                ) {
                    self.desktop_dock_setting(id);
                }
                if id == view::Sid::AsstInstall {
                    self.install_wake_runtime();
                }
                if id == view::Sid::AsstLocalInstall {
                    self.install_local_voice();
                }
                if id == view::Sid::AsstInstructions
                    && let Some(file) = crate::services::assistant::memory::ensure_instructions_file()
                {
                    cmd::shell_open(&file.to_string_lossy(), None);
                }
                if id == view::Sid::WindowsNotificationsSettings {
                    cmd::shell_open("ms-settings:notifications", None);
                }
                if id == view::Sid::OpenLogs
                    && let Some(dir) = crate::data::log::dir()
                {
                    let _ = std::fs::create_dir_all(&dir);
                    cmd::shell_open(&dir.to_string_lossy(), None);
                }
            }
        }
    }

    fn sw_drag(&mut self, x: i32) {
        let Some(id) = self.ui.sw.drag else { return };
        let Some((left, right)) = view::sw_slider_track(self.sw_size(), self.dpi_scale, &self.ui, id) else { return };
        let norm = (x as f32 - left) / (right - left).max(1.0);
        view::slider_apply(id, &mut self.settings, norm);
        self.setting_changed(id, id != view::Sid::Size);
        self.render_settings_win();
    }

    pub(in crate::island) fn set_search(&mut self, text: &str) {
        self.ui.int.search.set_text(text);
        self.sync_search();
    }

    fn sync_search(&mut self) {
        if !self.ui.int.search.is_empty() {
            self.close_memory_tools();
        }
        let text = self.ui.int.search.text();
        if text != self.ui.sw.search {
            self.ui.sw.search = text;
            self.ui.sw.scroll = 0.0;
            self.ui.sw.hover = None;
            self.ui.sw.plates.clear();
        }
    }

    pub(in crate::island) fn on_sw_up(&mut self) {
        unsafe {
            let _ = ReleaseCapture();
        }
        self.ui.sw.search_press = false;
        if let Some(guide) = self.ui.sw.guide.as_mut()
            && guide.preview_release()
        {
            self.render_settings_win();
        }
        if self.ui.sw.drag.take().is_some() {
            self.finish_slider();
            self.render_settings_win();
        }
    }

    pub(in crate::island) fn on_sw_wheel(&mut self, delta: i32) {
        if self.ui.sw.guide.as_ref().is_some_and(|guide| guide.visible) {
            return;
        }
        self.ui.sw.dropdown = None;
        let max = view::max_scroll(self.sw_size(), self.dpi_scale, &self.ui);
        let scroll = (self.ui.sw.scroll - delta as f32 / 120.0 * SCROLL_STEP * self.dpi_scale).clamp(0.0, max);
        if scroll != self.ui.sw.scroll {
            self.ui.sw.scroll = scroll;
            self.render_settings_win();
        }
    }

    pub(in crate::island) fn on_sw_char(&mut self, c: u16) -> bool {
        if let Some(guide) = self.ui.sw.guide.as_mut().filter(|guide| guide.visible) {
            if c == 0x09 && guide.voice_page() {
                let focused = self.ui.sw.field == Some(view::Field::GuideWake);
                self.sw_focus((!focused).then_some(view::Field::GuideWake));
                self.render_settings_win();
                return false;
            }
            if self.ui.sw.field == Some(view::Field::GuideWake) {
                if c == 0x0D {
                    self.on_guide_action(view::GuideAction::Next);
                } else {
                    guide.invalid_wake = false;
                    self.on_sw_field_char(c);
                    self.render_settings_win();
                }
                return false;
            }
            return c == 0x1B;
        }
        if c == 0x1B && self.ui.sw.dropdown.take().is_some() {
            self.render_settings_win();
            return false;
        }
        if self.ui.sw.field.is_none() && ((c >= 0x20 && c != 0x7F) || c == 0x16) {
            self.sw_focus(Some(view::Field::Search));
        }
        if self.ui.sw.field == Some(view::Field::Search) {
            match c {
                0x1B => {
                    if self.ui.int.search.is_empty() {
                        return true;
                    }
                    self.set_search("");
                    self.render_settings_win();
                    return false;
                }
                0x0D | 0x09 => return false,
                _ => {}
            }
        }
        if self.ui.sw.field.is_some() {
            self.on_sw_field_char(c);
            self.sync_search();
            self.render_settings_win();
            return false;
        }
        if c == 0x1B {
            if self.ui.sw.search.is_empty() {
                return true;
            }
            self.set_search("");
            self.render_settings_win();
        }
        false
    }

    fn on_sw_field_char(&mut self, unit: u16) {
        let Some(field) = self.ui.sw.field else { return };
        let max_chars = match field {
            view::Field::Search => MAX_SEARCH_CHARS,
            view::Field::GuideWake => crate::tools::voicecmd::MAX_WAKES * (crate::tools::voicecmd::MAX_WAKE_CHARS + 2),
            _ => 400,
        };
        let hwnd = self.settings_win.unwrap_or(self.hwnd);
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        match unit {
            0x1B => self.sw_focus(None),
            0x09 => {
                let next = view::sw_next_field(&self.ui, field);
                self.sw_focus(next);
            }
            0x0D => {
                let act = match field {
                    view::Field::SpotifyId => view::Act::SpotifySaveId,
                    view::Field::AiKey | view::Field::AiModel | view::Field::AiBase => view::Act::AiSave,
                    view::Field::VoiceKey | view::Field::VoiceModel | view::Field::VoiceBase => view::Act::VoiceSave,
                    view::Field::TtsKey | view::Field::TtsBase | view::Field::TtsModel | view::Field::TtsVoice => {
                        view::Act::TtsSave
                    }
                    view::Field::WakeWord => view::Act::WakeSave,
                    view::Field::GroupName => view::Act::GroupAdd,
                    _ => view::Act::DiscordMain,
                };
                self.sw_act(act);
            }
            0x08 => {
                if let Some(n) = self.sw_edit_mut() {
                    n.backspace(ctrl);
                }
            }
            0x01 => {
                if let Some(n) = self.sw_edit_mut() {
                    n.select_all();
                }
            }
            0x03 | 0x18 => {
                if !matches!(
                    field,
                    view::Field::DiscordSecret | view::Field::AiKey | view::Field::VoiceKey | view::Field::TtsKey
                ) && let Some(n) = self.sw_edit_mut()
                    && let Some(text) = n.selected_text()
                {
                    crate::data::clips::sys::write_text(hwnd, &text);
                    if unit == 0x18 {
                        n.delete_selection();
                    }
                }
            }
            0x16 => {
                if let Some(text) = crate::data::clips::sys::read_text(hwnd)
                    && let Some(n) = self.sw_edit_mut()
                {
                    let line: String = text.lines().next().unwrap_or("").trim().chars().take(max_chars).collect();
                    n.insert(&line);
                }
            }
            0xD800..=0xDBFF => {
                self.pending_high_surrogate = Some(unit);
                return;
            }
            u if u < 0x20 || u == 0x7F => return,
            u => {
                let c = if (0xDC00..=0xDFFF).contains(&u) {
                    let Some(high) = self.pending_high_surrogate.take() else { return };
                    char::decode_utf16([high, u]).next().and_then(|r| r.ok())
                } else {
                    char::from_u32(u32::from(u))
                };
                if let (Some(c), Some(n)) = (c, self.sw_edit_mut())
                    && n.text().chars().count() < max_chars
                {
                    let mut buf = [0u8; 4];
                    n.insert(c.encode_utf8(&mut buf));
                }
            }
        }
        self.ui.sw.caret_on = true;
        if self.ui.sw.field.is_some() {
            self.set_timer(Timer::SwCaret, CARET_BLINK_MS);
        }
        self.render_settings_win();
    }

    pub(in crate::island) fn on_sw_keydown(&mut self, vk: u16) -> bool {
        if self.ui.sw.field != Some(view::Field::GuideWake)
            && let Some(guide) = self.ui.sw.guide.as_ref().filter(|guide| guide.visible)
        {
            let action = match vk {
                v if v == VK_LEFT.0 => view::GuideAction::Back,
                v if v == VK_RIGHT.0 && !guide.last_page() => view::GuideAction::Next,
                v if v == VK_RETURN.0 => view::GuideAction::Next,
                _ => return false,
            };
            self.on_guide_action(action);
            return true;
        }
        if let Some(hotkey) = self.ui.sw.capturing {
            self.capture_key(hotkey, vk);
            return true;
        }
        self.ui.sw.quiet_sys = false;
        if self.ui.sw.field.is_none() {
            return false;
        }
        let down = |k: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetKeyState(k.0 as i32) < 0 };
        let (ctrl, shift) = (down(VK_CONTROL), down(VK_SHIFT));
        let Some(n) = self.sw_edit_mut() else { return false };
        match vk {
            v if v == VK_LEFT.0 => n.left(shift, ctrl),
            v if v == VK_RIGHT.0 => n.right(shift, ctrl),
            v if v == VK_HOME.0 => n.caret_to_start(shift),
            v if v == VK_END.0 => n.caret_to_end(shift),
            v if v == VK_DELETE.0 => n.delete(ctrl),
            _ => return false,
        }
        self.ui.sw.caret_on = true;
        self.sync_search();
        self.render_settings_win();
        true
    }

    pub(in crate::island) fn on_sw_destroyed(&mut self) {
        self.close_memory_tools();
        self.commit_wake();
        self.end_rebind();
        self.ui.int.search.set_text("");
        self.kill_timer(Timer::SwCaret);
        self.settings_win = None;
        self.pointer.slider = None;
        let guide = self.ui.sw.guide.take();
        if guide.as_ref().is_some_and(|guide| guide.visible) && self.settings.tour_pending() {
            self.settings.dismiss_tour();
            self.ui.cfg.dismiss_tour();
            self.settings.save();
        }
        self.ui.sw = view::SwUi { guide: guide.filter(|guide| !guide.visible), ..Default::default() };
    }
}

extern "system" fn sw_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SHOWWINDOW | WM_SIZE | WM_ACTIVATE => {
            notify_settings_window_changed(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            unsafe {
                BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
            }
            with_app(|a| a.render_settings_win());
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = point_from(lparam);
            let left_down = wparam.0 & 0x0001 != 0;
            with_app(|a| a.on_sw_move(x, y, left_down));
            LRESULT(0)
        }
        WM_NCCALCSIZE if wparam.0 != 0 => LRESULT(0),
        WM_NCHITTEST => {
            let mut pt =
                windows::Win32::Foundation::POINT { x: lparam.0 as i16 as i32, y: (lparam.0 >> 16) as i16 as i32 };
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
            }
            let mut zone = None;
            with_app(|a| zone = a.sw_nc_zone(pt.x, pt.y));
            match zone {
                Some(view::TitleZone::Caption) => LRESULT(HTCAPTION as isize),
                _ => LRESULT(HTCLIENT as isize),
            }
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point_from(lparam);
            let mut title = 0;
            with_app(|a| title = a.sw_title_click(x, y));
            match title {
                1 => unsafe {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                },
                2 => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => with_app(|a| a.on_sw_down(x, y)),
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with_app(|a| a.on_sw_up());
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wparam.0 >> 16) as i16 as i32;
            with_app(|a| a.on_sw_wheel(delta));
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            let mut taken = false;
            with_app(|a| taken = a.on_sw_keydown(vk));
            if msg == WM_SYSKEYDOWN && !taken {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            LRESULT(0)
        }
        WM_SYSCHAR | WM_SYSKEYUP => {
            let mut quiet = false;
            with_app(|a| quiet = a.ui.sw.quiet_sys);
            if quiet { LRESULT(0) } else { unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) } }
        }
        WM_KILLFOCUS => {
            with_app(|a| {
                a.end_rebind();
                a.render_settings_win();
            });
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_CHAR => {
            let mut close = false;
            with_app(|a| close = a.on_sw_char(wparam.0 as u16));
            if close {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            with_app(|a| a.on_sw_destroyed());
            notify_settings_window_changed(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Creates temporary native settings windows offscreen"]
    fn native_settings_window_is_listed_minimized_and_refreshes_without_app_borrow() -> Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(sw_proc),
                hInstance: instance.into(),
                lpszClassName: w!("NightIsland.Settings"),
                ..Default::default()
            });
            let parent = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Settings test parent"),
                WS_POPUP,
                -12000,
                -12000,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            struct Windows(Vec<HWND>);
            impl Drop for Windows {
                fn drop(&mut self) {
                    for hwnd in self.0.iter().rev() {
                        unsafe {
                            let _ = DestroyWindow(*hwnd);
                        }
                    }
                    SETTINGS_REFRESH_PENDING.store(false, Ordering::Release);
                }
            }
            let mut windows = Windows(vec![parent]);
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("NightIsland.Settings"),
                w!("Night Island settings test"),
                WS_OVERLAPPEDWINDOW,
                -12000,
                -12000,
                400,
                300,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            windows.0.push(hwnd);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, parent.0 as isize);
            SETTINGS_REFRESH_PENDING.store(false, Ordering::Release);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            notify_settings_window_changed(hwnd);
            notify_settings_window_changed(hwnd);
            let mut message = MSG::default();
            assert!(
                PeekMessageW(
                    &mut message,
                    Some(parent),
                    WM_SETTINGS_WINDOW_CHANGED,
                    WM_SETTINGS_WINDOW_CHANGED,
                    PM_REMOVE
                )
                .as_bool()
            );
            assert!(
                !PeekMessageW(
                    &mut message,
                    Some(parent),
                    WM_SETTINGS_WINDOW_CHANGED,
                    WM_SETTINGS_WINDOW_CHANGED,
                    PM_REMOVE
                )
                .as_bool()
            );
            on_settings_window_changed();
            let listed = crate::platform::desktop_dock::windows();
            assert!(listed.iter().any(|w| w.handle == hwnd.0 as isize));
            assert!(!listed.iter().any(|w| w.handle == parent.0 as isize));
            let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            assert!(IsIconic(hwnd).as_bool());
            assert!(crate::platform::desktop_dock::windows().iter().any(|w| w.handle == hwnd.0 as isize));
            assert!(
                PeekMessageW(
                    &mut message,
                    Some(parent),
                    WM_SETTINGS_WINDOW_CHANGED,
                    WM_SETTINGS_WINDOW_CHANGED,
                    PM_REMOVE
                )
                .as_bool()
            );
            on_settings_window_changed();
            let _ = ShowWindow(hwnd, SW_HIDE);
            assert!(!crate::platform::desktop_dock::windows().iter().any(|w| w.handle == hwnd.0 as isize));
            assert!(
                PeekMessageW(
                    &mut message,
                    Some(parent),
                    WM_SETTINGS_WINDOW_CHANGED,
                    WM_SETTINGS_WINDOW_CHANGED,
                    PM_REMOVE
                )
                .as_bool()
            );
            on_settings_window_changed();
            DestroyWindow(hwnd)?;
            windows.0.pop();
            assert!(
                PeekMessageW(
                    &mut message,
                    Some(parent),
                    WM_SETTINGS_WINDOW_CHANGED,
                    WM_SETTINGS_WINDOW_CHANGED,
                    PM_REMOVE
                )
                .as_bool()
            );
            assert!(!IsWindow(Some(hwnd)).as_bool());
        }
        Ok(())
    }
}
