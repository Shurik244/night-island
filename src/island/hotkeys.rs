use crate::island::*;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS;

pub(in crate::island) const GAME_POLL_MS: u32 = 3000;

static SETTLE_LOOKS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
const SETTLE_MS: u32 = 250;

const GHOST_POLL_MS: u32 = 60;

pub(in crate::island) use crate::data::keys::Hotkey;

pub(in crate::island) fn register_hotkeys(hwnd: HWND, settings: &Settings) -> Vec<Hotkey> {
    Hotkey::ALL
        .into_iter()
        .filter(|h| {
            let c = settings.combo(*h);
            !c.is_none()
                && unsafe {
                    RegisterHotKey(Some(hwnd), *h as i32, HOT_KEY_MODIFIERS(c.mods) | MOD_NOREPEAT, c.vk).is_err()
                }
        })
        .collect()
}

pub(in crate::island) fn unregister_hotkeys(hwnd: HWND) {
    for h in Hotkey::ALL {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), h as i32);
        }
    }
}

pub(in crate::island) fn fullscreen_app_in_front(own: HWND) -> bool {
    use windows::Win32::UI::Shell::{
        QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
    };
    unsafe {
        if SHQueryUserNotificationState()
            .is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_BUSY || s == QUNS_PRESENTATION_MODE)
        {
            return true;
        }
        let fg = GetForegroundWindow();
        if fg.is_invalid() || fg == own || fg == GetShellWindow() || fg == GetDesktopWindow() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return false;
        }
        let style = GetWindowLongW(fg, GWL_STYLE) as u32;
        if style & WS_CAPTION.0 == WS_CAPTION.0 {
            return false;
        }
        let mut rc = windows::Win32::Foundation::RECT::default();
        if GetWindowRect(fg, &mut rc).is_err() {
            return false;
        }
        let monitor = windows::Win32::Graphics::Gdi::MonitorFromWindow(
            fg,
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        );
        let mut info = windows::Win32::Graphics::Gdi::MONITORINFO {
            cbSize: size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
            ..Default::default()
        };
        if !windows::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        rc.left <= m.left && rc.top <= m.top && rc.right >= m.right && rc.bottom >= m.bottom
    }
}

impl App {
    pub(in crate::island) fn start_game_watch(&mut self) {
        if self.settings.game_mode {
            winevent::start(self.hwnd);
            self.set_timer(Timer::Game, GAME_POLL_MS);
        } else {
            winevent::stop();
            self.kill_timer(Timer::Game);
            self.set_gaming(false);
        }
    }

    pub(in crate::island) fn on_foreground_changed(&mut self) {
        if !self.settings.game_mode {
            return;
        }
        SETTLE_LOOKS.store(4, std::sync::atomic::Ordering::Relaxed);
        self.on_game_timer();
    }

    pub(in crate::island) fn on_game_timer(&mut self) {
        if !self.settings.game_mode {
            return;
        }
        let gaming = self.state != State::Expanded && fullscreen_app_in_front(self.hwnd);
        self.set_gaming(gaming);
        let looks = SETTLE_LOOKS.load(std::sync::atomic::Ordering::Relaxed);
        if looks > 0 {
            SETTLE_LOOKS.store(looks - 1, std::sync::atomic::Ordering::Relaxed);
            self.set_timer(Timer::Game, SETTLE_MS);
        } else {
            self.set_timer(Timer::Game, GAME_POLL_MS);
        }
    }

    pub(in crate::island) fn set_gaming(&mut self, on: bool) {
        if on == self.gaming {
            return;
        }
        self.gaming = on;
        self.pet.behavior.suspend();
        self.pet.char_key = None;
        self.sync_mixer();
        if on {
            unregister_hotkeys(self.hwnd);
            if let Some(desktop) = &mut self.pet.desktop {
                desktop.hide();
            }
            self.kill_timer(Timer::Char);
            self.hide_window();
        } else {
            register_hotkeys(self.hwnd, &self.settings);
            if !self.pill_hidden || self.ghosted {
                self.show_window();
            }
            self.start_char();
        }
    }

    fn hide_window(&mut self) {
        self.finish_cover_motion();
        self.ui.home.track_transition.finish();
        if self.gaming || !self.pet_notification_visible() {
            self.hide_notification_card();
        }
        self.cancel_pill_motion();
        self.kill_timer(Timer::Expand);
        self.kill_timer(Timer::Liquid);
        self.kill_timer(Timer::Weather);
        self.kill_timer(Timer::Progress);
        self.stop_idle();
        self.set_bump_visible(false);
        self.update_eq();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    fn show_window(&mut self) {
        if self.ui.tab == view::Tab::Chat && self.ui.chat.pet.enabled {
            self.sync_pet_studio();
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        self.update_eq();
        self.start_liquid();
        self.repaint_collapsed();
        self.start_char();
    }

    pub(in crate::island) fn toggle_island_from_character(&mut self) {
        if self.pill_hidden || matches!(self.state, State::Hidden | State::Hiding) {
            self.restore_pill();
            self.reveal();
            return;
        }
        if self.state == State::Expanded {
            self.collapse();
            if self.state == State::Expanded {
                return;
            }
        }
        self.kill_timer(Timer::PillBack);
        self.kill_timer(Timer::GhostHold);
        self.pill_hidden = true;
        self.ghosted = false;
        let _ = self.root.StopAnimation(h!("Opacity"));
        let _ = self.root.SetOpacity(1.0);
        self.hide_window();
    }

    pub(in crate::island) fn hide_pill_for_a_while(&mut self) {
        if self.gaming {
            return;
        }
        if self.pill_hidden {
            return self.restore_pill();
        }
        if self.state == State::Expanded {
            self.collapse();
            if self.state == State::Expanded {
                return;
            }
        }
        self.pill_hidden = true;
        self.ghost_stage = 0;
        if self.settings.ghost_mode {
            self.ghosted = true;
            self.apply_ghost(true);
            if self.settings.ghost_hold {
                self.set_timer(Timer::GhostHold, GHOST_POLL_MS);
            } else {
                self.set_timer(Timer::PillBack, self.settings.hide_pill_secs * 1000);
            }
            return;
        }
        self.hide_window();
        self.set_timer(Timer::PillBack, self.settings.hide_pill_secs * 1000);
    }

    pub(in crate::island) fn restore_pill(&mut self) {
        if !self.pill_hidden {
            return;
        }
        self.pill_hidden = false;
        self.kill_timer(Timer::PillBack);
        self.kill_timer(Timer::GhostHold);
        if self.ghosted {
            self.ghosted = false;
            return self.apply_ghost(false);
        }
        if !self.gaming {
            self.show_window();
        }
    }

    pub(in crate::island) fn pill_back_tick(&mut self) {
        if !self.ghosted || self.ghost_stage == 2 {
            return self.restore_pill();
        }
        let (x, y, w, h) = self.pill_screen(false);
        let mut pt = windows::Win32::Foundation::POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        let over = (x..=x + w).contains(&(pt.x as f32)) && (y..=y + h).contains(&(pt.y as f32));
        match (self.ghost_stage, over) {
            (0, false) => self.restore_pill(),
            (0, true) => {
                self.ghost_stage = 1;
                self.set_timer(Timer::PillBack, GHOST_POLL_MS);
            }
            (_, true) => self.set_timer(Timer::PillBack, GHOST_POLL_MS),
            (_, false) => {
                self.ghost_stage = 2;
                self.set_timer(Timer::PillBack, self.settings.hide_pill_secs * 1000);
            }
        }
    }

    pub(in crate::island) fn ghost_hold_tick(&mut self) {
        if !self.ghosted {
            return;
        }
        let vk = self.settings.combo(Hotkey::HidePill).vk as i32;
        let down = unsafe { GetAsyncKeyState(vk) } < 0;
        if down {
            self.set_timer(Timer::GhostHold, GHOST_POLL_MS);
        } else {
            self.restore_pill();
        }
    }

    fn apply_ghost(&mut self, on: bool) {
        let target = if on { self.settings.ghost_opacity } else { 1.0 };
        if let Some(panel) = &mut self.top_panel {
            panel.set_ghost(target, on && self.settings.ghost_click_through);
        }
        if let Ok(fade) = self.compositor.CreateScalarKeyFrameAnimation() {
            let _ = fade.InsertKeyFrame(1.0, target);
            let _ = fade.SetDuration(Duration::from_millis(180).into());
            let _ = self.root.StartAnimation(h!("Opacity"), &fade);
        }
        let through = (on && self.settings.ghost_click_through) || self.pointer.decoration_through;
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let bits = (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize;
            let styled = if through { ex | bits } else { ex & !bits };
            if styled != ex {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, styled);
            }
            if through {
                let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), 255, LWA_ALPHA);
            }
        }
        if through {
            self.kill_timer(Timer::Expand);
            self.kill_timer(Timer::Collapse);
        }
        self.pointer.tracking = false;
        self.update_weather_timer();
        self.start_progress_timer();
    }
}
