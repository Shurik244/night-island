use crate::island::*;

pub(crate) fn set_host_backdrop(hwnd: HWND, enabled: bool) {
    #[repr(C)]
    struct AccentPolicy {
        state: i32,
        flags: i32,
        gradient_color: u32,
        animation_id: i32,
    }
    #[repr(C)]
    struct CompositionAttrib {
        attrib: u32,
        data: *mut core::ffi::c_void,
        size: usize,
    }
    type SetWca = unsafe extern "system" fn(HWND, *mut CompositionAttrib) -> i32;

    const ACCENT_DISABLED: i32 = 0;
    const ACCENT_ENABLE_HOSTBACKDROP: i32 = 5;
    const WCA_ACCENT_POLICY: u32 = 19;

    unsafe {
        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else { return };
        let Some(proc) = GetProcAddress(user32, windows::core::s!("SetWindowCompositionAttribute")) else {
            return;
        };
        let set_wca: SetWca = core::mem::transmute(proc);
        let mut policy = AccentPolicy {
            state: if enabled { ACCENT_ENABLE_HOSTBACKDROP } else { ACCENT_DISABLED },
            flags: 0,
            gradient_color: 0,
            animation_id: 0,
        };
        let mut data = CompositionAttrib {
            attrib: WCA_ACCENT_POLICY,
            data: &mut policy as *mut _ as *mut _,
            size: size_of::<AccentPolicy>(),
        };
        set_wca(hwnd, &mut data);
    }
}

impl App {
    pub(in crate::island) fn repaint_collapsed(&mut self) {
        self.sync_live_activities();
        if self.pill_motion.intro.is_some()
            && (self.ui.notice.is_some()
                || self.ui.volume.is_some()
                || (self.settings.live_music_priority && !self.ui.live.is_empty())
                || view::legacy_download(&self.ui).is_some())
        {
            self.cancel_pill_motion();
            self.update_eq();
        }
        self.fit_width();
        if self.music.spectrum_running {
            self.place_spectrum();
        }
        if self.pill_motion.intro != Some(super::pill_motion::IntroPhase::Fading) {
            let c = self.collapsed_surface_rect();
            let _ = self.collapsed.paint(&self.gpu, |p| {
                let _ = view::paint_collapsed(p, c, self.scale, &self.ui);
            });
        }
        self.paint_progress_fill();
        self.update_weather_timer();
        self.maybe_show_notification();
    }

    pub(in crate::island) fn apply_pill_opacity(&mut self) {
        let alpha = self.settings.pill_opacity;
        if self.state != State::Expanded {
            let _ = self.tint.SetOpacity(alpha);
        }
        let _ = self.liquid.bump.sprite.SetOpacity(1.0);
        let _ = self.liquid.bump_fill.SetOpacity(alpha);
        let expanded = self.state == State::Expanded;
        let hidden = matches!(self.state, State::Hidden | State::Hiding);
        let _ = self.set_geometry_now(self.win, expanded, hidden);
    }

    pub(in crate::island) fn apply_theme(&mut self) {
        let (panel, accent) = crate::ui::theme::colors(&self.settings);
        crate::ui::theme::set(panel, accent);
        let (r, g, b) = crate::ui::theme::panel_u8();
        if let Ok(brush) = self.compositor.CreateColorBrushWithColor(Color { A: 255, R: r, G: g, B: b }) {
            let _ = self.tint.SetBrush(&brush);
        }
        self.paint_ears();
        self.paint_liquid_body(0.0);
        self.apply_accent();
        self.repaint_themed_surfaces();
    }

    pub(in crate::island) fn repaint_themed_surfaces(&mut self) {
        self.repaint();
        self.update_notification_card();
        if let Some(chat) = &mut self.pet.chat {
            chat.repaint();
        }
        if let Some(panel) = &mut self.top_panel {
            panel.repaint();
        }
    }

    pub(in crate::island) fn apply_accent(&self) {
        let base = self.ui.media.accent.unwrap_or_else(crate::ui::theme::accent_u8);
        let color = |(r, g, b): (u8, u8, u8)| Color { A: 255, R: r, G: g, B: b };
        let last = (self.music.spectrum_brushes.len().max(2) - 1) as f32;
        for (i, brush) in self.music.spectrum_brushes.iter().enumerate() {
            let _ = brush.SetColor(color(media::gradient(base, i as f32 / last)));
        }
    }

    pub(in crate::island) fn progress_fill_active(&self) -> bool {
        view::pill_progress_fill_on(&self.ui) && media::progress().is_some()
    }

    pub(in crate::island) fn paint_progress_fill(&self) {
        let progress = media::progress().filter(|_| view::pill_progress_fill_on(&self.ui));
        let _ = self.progress_fill.paint(&self.gpu, |p| {
            if let Some((pos, len)) = progress {
                let pill = self.pill_rect(false);
                let ear = if self.docked() && self.edge() == Edge::Top { EAR * self.scale } else { 0.0 };
                let range = (pill.0 - ear, pill.1, pill.0 + pill.2 + ear, pill.1 + pill.3);
                let size = self.progress_fill.size();
                let _ = p.fill_progress_surface(
                    range,
                    (size.0 as f32, size.1 as f32),
                    pos / len,
                    self.ui.media.accent.unwrap_or_else(crate::ui::theme::accent_u8),
                    self.settings.vertical(),
                    self.edge() == Edge::Left,
                );
            }
        });
        self.paint_ears();
    }

    pub(in crate::island) fn paint_ears(&self) {
        let r = EAR * self.scale;
        for (i, ear) in self.ears.iter().enumerate() {
            let _ = ear.paint(&self.gpu, |p| {
                let _ = p.fill_ear(r, i == 0, {
                    let (r, g, b) = crate::ui::theme::panel();
                    crate::ui::gfx::rgba(r, g, b, 1.0)
                });
                if let Some((pos, len)) = media::progress().filter(|_| view::pill_progress_fill_on(&self.ui)) {
                    let width = self.cw * self.scale;
                    let left = if i == 0 { 0.0 } else { -width - r };
                    let _ = p.fill_ear_progress(
                        r,
                        i == 0,
                        (left, left + width + r * 2.0),
                        pos / len,
                        self.ui.media.accent.unwrap_or_else(crate::ui::theme::accent_u8),
                    );
                }
            });
        }
    }

    pub(in crate::island) fn repaint(&self) {
        if self.pill_motion.intro != Some(super::pill_motion::IntroPhase::Fading) {
            let c = self.collapsed_surface_rect();
            let _ = self.collapsed.paint(&self.gpu, |p| {
                let _ = view::paint_collapsed(p, c, self.scale, &self.ui);
            });
        }
        self.paint_progress_fill();
        self.repaint_expanded();
    }

    pub(in crate::island) fn repaint_expanded(&self) {
        self.ui.feedback.set_enabled(crate::ui::feedback::motion_allowed());
        let e = self.pill_full();
        let _ = self.expanded.paint(&self.gpu, |p| {
            let _ = view::paint_expanded(p, e, self.scale, &self.ui, &self.clipboard.clips);
        });
        if self.state == State::Expanded
            && (self.ui.feedback.active()
                || self.ui.home.track_transition.active()
                || self.ui.home.widgets.motion.borrow().active()
                || self.ui.home.widgets.editor_active())
        {
            self.set_timer(Timer::UiAnim, self.frame_ms());
        }
        self.update_weather_timer();
    }

    pub(in crate::island) fn update_weather_timer(&self) {
        if self.weather_animating() {
            self.set_timer(Timer::Weather, self.frame_ms());
        } else {
            self.kill_timer(Timer::Weather);
        }
    }

    pub(in crate::island) fn weather_animating(&self) -> bool {
        let shown = match self.state {
            State::Expanded => self.ui.tab == view::Tab::Weather,
            State::Collapsed => view::pill_weather_visible(&self.ui) && !view::pill_progress_fill_on(&self.ui),
            _ => false,
        };
        shown
            && self.pill_motion.intro.is_none()
            && !self.notification_card_visible()
            && self.settings.weather
            && self.ui.weather.is_some()
            && !self.gaming
            && (!self.pill_hidden || (self.ghosted && self.settings.ghost_opacity > 0.0))
            && crate::ui::weather_icons::animations_enabled()
            && unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub(in crate::island) fn weather_tick(&self) {
        if !self.weather_animating() {
            return;
        }
        if self.state == State::Expanded {
            return self.repaint_expanded();
        }
        let pill = self.collapsed_surface_rect();
        if let Some(rect) = view::pill_weather_rect(pill, self.scale, &self.ui) {
            let _ = self
                .collapsed
                .paint_region(&self.gpu, rect, |p| view::paint_pill_weather_icon(p, pill, self.scale, &self.ui));
        }
        self.update_weather_timer();
    }

    pub(in crate::island) fn set_blur(&self, open: bool) {
        let on = if open {
            self.settings.acrylic
        } else {
            self.settings.pill_blur && self.state != State::Hidden && self.state != State::Hiding
        };
        let _ = self.backdrop.SetIsVisible(on);
        set_host_backdrop(self.hwnd, on);
        let _ = self.liquid.bump_blur.SetIsVisible(self.settings.pill_blur);
        if self.settings.pill_blur && !open && !self.spikes_active() {
            let amp = if self.liquid.idle_running { IDLE_AMP * self.scale } else { self.liquid.bump_amp };
            self.paint_liquid_body(amp);
        }
    }

    pub(in crate::island) fn apply_capture(&self) {
        if let Some(panel) = &self.top_panel {
            panel.set_capture_hidden(self.settings.capture_hidden);
        }
        if let Some(dock) = &self.desktop_dock {
            dock.set_capture_hidden(self.settings.capture_hidden);
        }
        let affinity = if self.settings.capture_hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, affinity);
        }
        if let Some(desktop) = &self.pet.desktop {
            desktop.set_capture_hidden(self.settings.capture_hidden);
        }
        if let Some(chat) = &self.pet.chat {
            chat.set_capture_hidden(self.settings.capture_hidden);
        }
        self.update_notification_card();
    }

    pub(in crate::island) fn apply_style(&mut self) {
        self.set_decoration_through(false);
        if self.liquid_on() {
            self.start_liquid();
        } else {
            self.kill_timer(Timer::Liquid);
            self.liquid_tick();
        }
        self.apply_layout();
        self.refresh_eq_shape();
    }

    pub(in crate::island) fn apply_menu_choice(&mut self, id: usize) {
        match id {
            tray::ID_TOGGLE => self.toggle(),
            tray::ID_SETTINGS => self.open_settings_window(),
            tray::ID_GUIDE => self.open_guide(),
            tray::ID_RESET_POS => self.reset_position(),
            _ => {}
        }
    }
}
