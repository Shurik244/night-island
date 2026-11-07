use crate::island::*;

pub(in crate::island) const NOTICE_MS: u32 = 1600;
pub(in crate::island) const VOLUME_OVERLAY_MS: u32 = 1300;

impl App {
    pub(in crate::island) fn on_privacy(&mut self) {
        self.log_privacy_events();
        let now = privacy::snapshot();
        if now == self.ui.privacy {
            return;
        }
        self.ui.privacy = now;
        self.repaint_collapsed();
    }

    pub(in crate::island) fn notify(&mut self, kind: Notice, glyph: char, text: &str) {
        self.notify_for(kind, glyph, text, NOTICE_MS);
    }

    pub(in crate::island) fn notify_for(&mut self, kind: Notice, glyph: char, text: &str, ms: u32) {
        if self.settings.notify.on(kind) {
            let card = if kind == Notice::Ai {
                self.settings.ai_notification_cards
            } else {
                self.settings.pet_notifications && self.pet.desktop.is_some()
            };
            if card {
                self.post_local_notification(kind, text);
            } else {
                self.show_notice_for(glyph, text, ms);
            }
        }
    }

    pub(in crate::island) fn show_notice_for(&mut self, glyph: char, text: &str, ms: u32) {
        self.set_notice_for(glyph, text, ms, false);
    }

    pub(in crate::island) fn show_compact_reply(&mut self, text: &str) {
        self.set_notice_for('\u{E8F2}', text, 7000, true);
    }

    fn set_notice_for(&mut self, glyph: char, text: &str, ms: u32, compact: bool) {
        self.react(if glyph == '\u{E783}' { Reaction::Attention } else { Reaction::Done }, 2300);
        self.system_notices.compact_reply = compact;
        self.notice_link = None;
        self.ui.notice = Some((glyph, text.to_string()));
        self.update_eq();
        self.repaint_collapsed();
        self.set_timer(Timer::Notice, ms);
    }

    pub(in crate::island) fn on_lockkey(&mut self, vk: usize) {
        let vk = vk as u16;
        if !self.lock_pending.contains(&vk) {
            self.lock_pending.push(vk);
        }
        self.set_timer(Timer::LockKey, 40);
    }

    pub(in crate::island) fn report_lockkeys(&mut self) {
        for vk in std::mem::take(&mut self.lock_pending) {
            let on = lockkeys::state(vk);
            if let Some(text) = lockkeys::describe(usize::from(vk), on) {
                self.notify_for(Notice::LockKeys, if on { '\u{E72E}' } else { '\u{E785}' }, text, 1200);
            }
        }
    }

    pub(in crate::island) fn on_power(&mut self) {
        const LOW_PERCENT: u8 = 15;
        let Some((percent, charging)) = view::battery() else { return };
        let Some((was_percent, was_charging)) = self.power_last.replace((percent, charging)) else { return };
        if charging != was_charging {
            let text = if charging { "Charger connected" } else { "Charger disconnected" };
            let glyph = if charging { '\u{E83E}' } else { '\u{E83F}' };
            let message = format!("{} · {percent}%", crate::ui::i18n::tr(text));
            self.notify_for(Notice::Battery, glyph, &message, 2200);
        } else if !charging && percent <= LOW_PERCENT && was_percent > LOW_PERCENT {
            let message = format!("{} · {percent}%", crate::ui::i18n::tr("Battery low"));
            self.notify_for(Notice::Battery, '\u{E850}', &message, 4000);
        }
    }

    pub(in crate::island) fn toggle_mic(&mut self) {
        let Some(muted) = volume::mic_toggle() else {
            return self.notify(Notice::Error, '\u{E720}', "No microphone found");
        };
        self.ui.mic_muted = Some(muted);
        self.notify(
            Notice::Mic,
            if muted { '\u{F781}' } else { '\u{E720}' },
            if muted { "Microphone muted" } else { "Microphone on" },
        );
        if self.state == State::Expanded {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn click_collapsed(&mut self, x: i32, y: i32) {
        if self.recording.is_some() {
            return self.stop_recording();
        }
        if let Some(path) = self.notice_link.take() {
            cmd::reveal(&path);
            self.ui.notice = None;
            self.kill_timer(Timer::Notice);
            self.update_eq();
            return self.repaint_collapsed();
        }
        let pill = self.pill_rect(false);
        if view::hit_mic_dot(pill, self.scale, &self.ui, x as f32, y as f32) {
            self.toggle_mic();
        }
    }

    pub(in crate::island) fn toggle_vpn(&mut self) {
        if self.ui.vpn_busy || !vpn::toggle(self.hwnd) {
            return;
        }
        self.ui.vpn_busy = true;
        self.repaint_collapsed();
        self.repaint_expanded();
    }

    pub(in crate::island) fn on_vpn(&mut self) {
        let state = vpn::state();
        let was = self.ui.vpn;
        let result = vpn::take_result();
        match result {
            Some(Ok(())) => self.set_timer(Timer::VpnWait, 15_000),
            Some(Err(why)) => {
                self.kill_timer(Timer::VpnWait);
                self.ui.vpn_busy = false;
                self.notify(Notice::Error, '\u{E783}', why);
            }
            None => {}
        }
        if was.is_some() && state.is_some() && state != was {
            self.kill_timer(Timer::VpnWait);
            self.ui.vpn_busy = false;
            let (glyph, text) = if state == Some(true) {
                (view::VPN_GLYPH, "VPN connected")
            } else {
                (view::VPN_GLYPH, "VPN disconnected")
            };
            self.notify(Notice::Integrations, glyph, text);
        }
        self.ui.vpn = state;
        self.ui.vpn_busy = self.ui.vpn_busy && (vpn::switching() || result.is_none_or(|r| r.is_ok()));
        self.update_eq();
        self.repaint_collapsed();
        self.repaint_expanded();
    }

    pub(in crate::island) fn vpn_wait_over(&mut self) {
        self.ui.vpn_busy = false;
        self.repaint_collapsed();
        self.repaint_expanded();
        self.repaint_expanded();
    }

    pub(in crate::island) fn on_downloads(&mut self) {
        for event in downloads::take_events() {
            match event {
                downloads::Event::Active(name) => {
                    match &name {
                        Some(_) => self.set_timer(Timer::DlStale, 25_000),
                        None => self.kill_timer(Timer::DlStale),
                    }
                    self.ui.downloading = name;
                }
                downloads::Event::Done(path) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if self.settings.notify.on(Notice::Download) {
                        self.show_notice_for('\u{E73E}', &format!("Downloaded {name}"), 7000);
                        self.notice_link = Some(path);
                    }
                }
            }
        }
        self.update_eq();
        self.repaint_collapsed();
    }

    pub(in crate::island) fn on_weather(&mut self) {
        let now = weather::snapshot();
        if now == self.ui.weather {
            return;
        }
        self.ui.weather = now;
        self.repaint_collapsed();
        if self.ui.tab == view::Tab::Weather {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn on_media(&mut self) {
        if demo::active("NI_DEBUG_MUSIC") {
            return;
        }
        let now = media::snapshot();
        if now == self.ui.media {
            return;
        }
        let track_changed = now.title != self.ui.media.title;
        if now.title != self.ui.media.title || now.source != self.ui.media.source || now.cover != self.ui.media.cover {
            self.finish_cover_motion();
        }
        if (track_changed || now.artist != self.ui.media.artist || now.source != self.ui.media.source)
            && self.ui.tab != view::Tab::Yandex
        {
            if self.pointer.seek_drag {
                unsafe {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                }
            }
            self.pointer.seek_drag = false;
            self.ui.home.seek = None;
        }
        if now.source != self.ui.media.source && self.ui.tab != view::Tab::Yandex {
            self.pointer.vol_drag = false;
        }
        if now.title != self.ui.media.title
            || now.artist != self.ui.media.artist
            || now.source != self.ui.media.source
            || now.cover != self.ui.media.cover
        {
            self.ui.home.track_transition.retarget(
                &self.ui.media,
                self.state == State::Expanded
                    && self.ui.tab == view::Tab::Home
                    && !now.title.is_empty()
                    && crate::ui::feedback::motion_allowed(),
            );
        }
        self.ui.media = now;
        if self.ui.home.vol_track {
            match volume::app_volume(&self.ui.media.source) {
                Some(level) => self.ui.home.app_vol = level,
                None => self.ui.home.vol_track = false,
            }
        }
        self.start_progress_timer();
        if self.pill_motion.intro.is_some()
            && (!self.ui.media.playing
                || (!self.settings.music_track_intro
                    && self.pill_motion.track
                        != (self.ui.media.title.clone(), self.ui.media.artist.clone(), self.ui.media.source.clone())))
        {
            self.cancel_pill_motion();
        }
        self.maybe_start_music_intro();
        self.refresh_music_intro();
        if track_changed && self.ui.media.source == "SPOTIFY" {
            spotify::refresh();
        }
        self.apply_accent();
        self.update_eq();
        self.repaint_collapsed();
        if self.ui.tab == view::Tab::Home {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn adjust_volume(&mut self, delta: i32) {
        let step = if delta > 0 { 0.02 } else { -0.02 };
        let Some(level) = volume::step(step) else { return };
        self.kill_timer(Timer::Expand);
        self.ui.volume = Some(level);
        self.update_eq();
        self.repaint_collapsed();
        self.set_timer(Timer::Volume, VOLUME_OVERLAY_MS);
    }

    pub(in crate::island) fn schedule_clock(&self) {
        self.set_timer(Timer::Clock, view::clock().ms_to_next_minute);
    }
}
