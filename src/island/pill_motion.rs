use crate::island::*;

pub(in crate::island) const WIDTH_MS: u64 = 360;
const INTRO_HOLD_MS: u32 = 1700;
pub(in crate::island) const REVEAL_MS: u64 = 720;
const INTRO_RETURN_MS: u64 = 680;
const OUTGOING_MS: u32 = 280;

#[derive(Clone, Copy)]
pub(in crate::island) struct WidthTween {
    pub from: view::Rect,
    pub to: view::Rect,
    pub room: f32,
    pub start: Instant,
    pub duration: u64,
}

impl WidthTween {
    pub fn at(&self, now: Instant) -> view::Rect {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / (self.duration.max(1) as f32 / 1000.0);
        let p = crate::ui::anim::ease(crate::data::settings::AnimCurve::Smooth, t);
        let lerp = |a: f32, b: f32| a + (b - a) * p;
        (
            lerp(self.from.0, self.to.0),
            lerp(self.from.1, self.to.1),
            lerp(self.from.2, self.to.2),
            lerp(self.from.3, self.to.3),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::island) enum IntroPhase {
    Fading,
    Present,
    Returning,
}

#[derive(Default)]
pub(in crate::island) struct MusicEntranceGate {
    introduced: bool,
}

impl MusicEntranceGate {
    pub fn take(&mut self, playing: bool, named: bool, visible: bool) -> bool {
        if self.introduced || !playing || !named {
            return false;
        }
        self.introduced = true;
        visible
    }
}

pub(in crate::island) struct PillMotion {
    pub content: Option<PillContent>,
    pub width: Option<WidthTween>,
    pub intro: Option<IntroPhase>,
    pub intro_started: Option<Instant>,
    pub intro_width: f32,
    pub gate: MusicEntranceGate,
    pub track: (String, String, String),
    pub cover: Layer,
    pub previous: Layer,
    pub title: Layer,
    pub title_clip: windows::UI::Composition::InsetClip,
}

#[derive(PartialEq, Eq)]
pub(in crate::island) enum PillContent {
    Notice(char, String),
    Volume,
    Download(String),
    Normal,
}

impl PillContent {
    pub fn current(ui: &view::UiState) -> Self {
        if let Some((glyph, text)) = &ui.notice {
            Self::Notice(*glyph, text.clone())
        } else if ui.volume.is_some() {
            Self::Volume
        } else if let Some(name) = &ui.downloading {
            Self::Download(name.clone())
        } else {
            Self::Normal
        }
    }
}

impl App {
    fn intro_layout(&self) -> view::MusicEntranceLayout {
        view::music_entrance_layout(self.pill_rect(false), self.scale, &self.ui)
    }

    fn title_position(&self, rect: view::Rect, ratio: f32) -> Vector3 {
        let short = 34.0 * self.scale * ratio;
        let rotation = self.intro_layout().rotation;
        if rotation < 0.0 {
            Vector3 { X: (rect.0 + rect.2 - short) / 2.0, Y: rect.3, Z: 0.0 }
        } else if rotation > 0.0 {
            Vector3 { X: (rect.0 + rect.2 + short) / 2.0, Y: rect.1, Z: 0.0 }
        } else {
            Vector3 { X: rect.0, Y: (rect.1 + rect.3 - short) / 2.0, Z: 0.0 }
        }
    }

    pub(in crate::island) fn refresh_music_intro(&self) {
        if self.pill_motion.intro.is_none() {
            return;
        }
        let _ = self.pill_motion.cover.paint(&self.gpu, |p| {
            let _ = view::paint_music_entrance_cover(p, self.scale, &self.ui);
        });
        let _ = self.pill_motion.title.paint(&self.gpu, |p| {
            let _ = view::paint_music_entrance_title(p, self.scale, &self.ui);
        });
    }

    pub(in crate::island) fn place_music_intro(&self) {
        let layout = self.intro_layout();
        let cover = &self.pill_motion.cover.sprite;
        let title = &self.pill_motion.title.sprite;
        let _ = cover.StopAnimation(h!("Offset"));
        let _ = title.StopAnimation(h!("Offset"));
        let _ = cover.SetOffset(Vector3 { X: layout.cover.0, Y: layout.cover.1, Z: 0.0 });
        let _ = title.SetRotationAngleInDegrees(layout.rotation);
        let _ = title.SetOffset(self.title_position(layout.title, 1.0));
    }

    pub(in crate::island) fn animate_pill_vector(
        &self,
        sprite: &SpriteVisual,
        name: &windows::core::HSTRING,
        from: Vector3,
        to: Vector3,
        ms: u64,
    ) -> Result<()> {
        let easing = self
            .compositor
            .CreateCubicBezierEasingFunction(Vector2 { X: 0.45, Y: 0.0 }, Vector2 { X: 0.55, Y: 1.0 })?;
        let animation = self.compositor.CreateVector3KeyFrameAnimation()?;
        animation.InsertKeyFrame(0.0, from)?;
        animation.InsertKeyFrameWithEasingFunction(1.0, to, &easing)?;
        animation.SetDuration(Duration::from_millis(ms).into())?;
        sprite.StartAnimation(name, &animation)
    }

    fn intro_alpha(&self, sprite: &SpriteVisual, from: f32, to: f32, wait: f32, ms: u64) -> Result<()> {
        let animation = self.compositor.CreateScalarKeyFrameAnimation()?;
        animation.InsertKeyFrame(0.0, from)?;
        if wait > 0.0 {
            animation.InsertKeyFrame(wait, from)?;
        }
        animation.InsertKeyFrame(1.0, to)?;
        animation.SetDuration(Duration::from_millis(ms).into())?;
        sprite.StartAnimation(h!("Opacity"), &animation)
    }

    pub(in crate::island) fn maybe_start_music_intro(&mut self) {
        let track = (self.ui.media.title.clone(), self.ui.media.artist.clone(), self.ui.media.source.clone());
        if self.settings.music_track_intro && (!self.ui.media.playing || self.pill_motion.track != track) {
            self.pill_motion.gate = Default::default();
        }
        self.pill_motion.track = track;
        if !self.settings.island_animations || !crate::ui::feedback::motion_allowed() {
            return;
        }
        let visible = !self.notification_card_visible()
            && self.state == State::Collapsed
            && !self.pill_hidden
            && !self.gaming
            && !self.character_interacting()
            && self.ui.notice.is_none()
            && self.ui.volume.is_none()
            && self.ui.downloading.is_none();
        if !self.pill_motion.gate.take(self.ui.media.playing, !self.ui.media.title.trim().is_empty(), visible) {
            return;
        }
        if self.pill_motion.intro == Some(IntroPhase::Fading) {
            return;
        }
        if self.pill_motion.intro.is_none() {
            self.pill_motion.intro = Some(IntroPhase::Fading);
            self.pill_motion.intro_width = self.cw;
            self.kill_timer(Timer::Autohide);
            let _ = self.intro_alpha(&self.collapsed.sprite, 1.0, 0.0, 0.0, u64::from(OUTGOING_MS));
            if !self.settings.pet_music_curiosity {
                let _ = self.intro_alpha(&self.pet.char_layer.sprite, 1.0, 0.0, 0.0, u64::from(OUTGOING_MS));
            }
            self.update_weather_timer();
            self.set_timer(Timer::MusicIntro, OUTGOING_MS + 20);
            return;
        }
        self.start_music_reveal();
    }

    fn start_music_reveal(&mut self) {
        self.ui.track_in_flight = true;
        self.pill_motion.intro = Some(IntroPhase::Present);
        let work = self.island_work_area();
        let room = if self.settings.vertical() { work.3 - work.1 } else { work.2 - work.0 };
        self.pill_motion.intro_width = view::music_entrance_width(&self.ui).min((room / self.scale - 32.0).max(100.0));
        for sprite in [&self.pill_motion.cover.sprite, &self.pill_motion.title.sprite] {
            let _ = sprite.StopAnimation(h!("Opacity"));
            let _ = sprite.SetOpacity(0.0);
            let _ = sprite.StopAnimation(h!("Scale"));
        }
        let _ = self.collapsed.sprite.StopAnimation(h!("Opacity"));
        let _ = self.collapsed.sprite.SetOpacity(0.0);
        self.kill_timer(Timer::Autohide);
        let _ = self.pill_motion.previous.sprite.SetIsVisible(false);
        self.reset_liquid_pull();
        self.update_eq();
        self.repaint_collapsed();
        self.place_music_intro();
        self.refresh_music_intro();
        self.pill_motion.intro_started = Some(Instant::now());
        let layout = self.intro_layout();
        let pill = self.pill_rect(false);
        let center = view::music_entrance_cover_at(pill, self.scale, &self.ui, 0.0);
        let delta = (center.0 - layout.cover.0, center.1 - layout.cover.1);
        let shifted_title =
            (layout.title.0 + delta.0, layout.title.1 + delta.1, layout.title.2 + delta.0, layout.title.3 + delta.1);
        let easing = self.ease();
        if let Ok(easing) = easing {
            for (sprite, from, to) in [
                (
                    &self.pill_motion.cover.sprite,
                    Vector3 { X: center.0, Y: center.1, Z: 0.0 },
                    Vector3 { X: layout.cover.0, Y: layout.cover.1, Z: 0.0 },
                ),
                (
                    &self.pill_motion.title.sprite,
                    self.title_position(shifted_title, 1.0),
                    self.title_position(layout.title, 1.0),
                ),
            ] {
                let _ = sprite.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
                if let Ok(animation) = self.compositor.CreateVector3KeyFrameAnimation() {
                    let _ = animation.InsertKeyFrame(0.0, from);
                    let _ = animation.InsertKeyFrame(0.24, from);
                    let _ = animation.InsertKeyFrameWithEasingFunction(1.0, to, &easing);
                    let _ = animation.SetDuration(Duration::from_millis(REVEAL_MS).into());
                    let _ = sprite.StartAnimation(h!("Offset"), &animation);
                }
                let _ = sprite.SetIsVisible(true);
            }
            let length = if self.settings.vertical() {
                layout.title.3 - layout.title.1
            } else {
                layout.title.2 - layout.title.0
            };
            if let Ok(clip) = self.compositor.CreateScalarKeyFrameAnimation() {
                let _ = clip.InsertKeyFrame(0.0, 240.0 * self.scale);
                let _ = clip.InsertKeyFrame(0.24, 240.0 * self.scale);
                let _ = clip.InsertKeyFrameWithEasingFunction(1.0, (240.0 * self.scale - length).max(0.0), &easing);
                let _ = clip.SetDuration(Duration::from_millis(REVEAL_MS).into());
                let _ = self.pill_motion.title_clip.StartAnimation(h!("RightInset"), &clip);
            }
        }
        let cover = &self.pill_motion.cover.sprite;
        let _ = cover.SetCenterPoint(Vector3 { X: 14.0 * self.scale, Y: 14.0 * self.scale, Z: 0.0 });
        let _ = self.animate_pill_vector(
            cover,
            h!("Scale"),
            Vector3 { X: 0.72, Y: 0.72, Z: 1.0 },
            Vector3 { X: 1.0, Y: 1.0, Z: 1.0 },
            260,
        );
        let _ = self.intro_alpha(cover, 0.0, 1.0, 0.0, 260);
        let _ = self.intro_alpha(&self.pill_motion.title.sprite, 0.0, 1.0, 0.28, REVEAL_MS);
        if !self.settings.pet_music_curiosity {
            let _ = self.pet.char_layer.sprite.StopAnimation(h!("Opacity"));
            let _ = self.pet.char_layer.sprite.SetOpacity(0.0);
        }
        self.pet.char_key = None;
        self.set_timer(Timer::Char, 1);
        self.set_timer(Timer::MusicIntro, INTRO_HOLD_MS);
    }

    pub(in crate::island) fn music_intro_step(&mut self) {
        match self.pill_motion.intro {
            Some(IntroPhase::Fading) => self.start_music_reveal(),
            Some(IntroPhase::Present) => {
                let _ = self.pill_motion.cover.sprite.SetCenterPoint(Vector3::default());
                let origin = self.window_origin(Win::Small);
                let cover_from = self.pill_motion.cover.sprite.Offset().unwrap_or_default();
                let title_from = self.pill_motion.title.sprite.Offset().unwrap_or_default();
                self.pill_motion.intro = Some(IntroPhase::Returning);
                self.pill_motion.intro_width = view::collapsed_width(&self.ui, COLLAPSED.0);
                self.repaint_collapsed();
                let new_origin = self.window_origin(Win::Small);
                let adjust = |at: Vector3| Vector3 {
                    X: at.X + origin.0 - new_origin.0,
                    Y: at.Y + origin.1 - new_origin.1,
                    Z: 0.0,
                };
                let layout = self.intro_layout();
                let cover = layout.final_cover.unwrap_or(layout.cover);
                let cover_to = Vector3 { X: cover.0, Y: cover.1, Z: 0.0 };
                let _ = self.animate_pill_vector(
                    &self.pill_motion.cover.sprite,
                    h!("Offset"),
                    adjust(cover_from),
                    cover_to,
                    INTRO_RETURN_MS,
                );
                let _ = self.animate_pill_vector(
                    &self.pill_motion.cover.sprite,
                    h!("Scale"),
                    Vector3 { X: 1.0, Y: 1.0, Z: 1.0 },
                    Vector3 {
                        X: (cover.2 - cover.0) / (28.0 * self.scale),
                        Y: (cover.3 - cover.1) / (28.0 * self.scale),
                        Z: 1.0,
                    },
                    INTRO_RETURN_MS,
                );
                let ratio = if self.settings.pill_unified_layout { 13.0 / 15.0 } else { 12.5 / 15.0 };
                let title = layout.final_title.unwrap_or(layout.title);
                let title_to = self.title_position(title, ratio);
                let _ = self.animate_pill_vector(
                    &self.pill_motion.title.sprite,
                    h!("Offset"),
                    adjust(title_from),
                    title_to,
                    INTRO_RETURN_MS,
                );
                let _ = self.animate_pill_vector(
                    &self.pill_motion.title.sprite,
                    h!("Scale"),
                    Vector3 { X: 1.0, Y: 1.0, Z: 1.0 },
                    Vector3 { X: ratio, Y: ratio, Z: 1.0 },
                    INTRO_RETURN_MS,
                );
                let length = if layout.rotation == 0.0 { title.2 - title.0 } else { title.3 - title.1 };
                if let Ok(clip) = self.compositor.CreateScalarKeyFrameAnimation() {
                    let _ = clip.InsertKeyFrame(1.0, (240.0 * self.scale - length / ratio).max(0.0));
                    let _ = clip.SetDuration(Duration::from_millis(INTRO_RETURN_MS).into());
                    let _ = self.pill_motion.title_clip.StartAnimation(h!("RightInset"), &clip);
                }
                if layout.final_cover.is_none() {
                    let _ = self.intro_alpha(&self.pill_motion.cover.sprite, 1.0, 0.0, 0.3, INTRO_RETURN_MS);
                }
                if layout.final_title.is_none() {
                    let _ = self.intro_alpha(&self.pill_motion.title.sprite, 1.0, 0.0, 0.3, INTRO_RETURN_MS);
                }
                for sprite in [&self.collapsed.sprite, &self.pet.char_layer.sprite] {
                    if sprite != &self.pet.char_layer.sprite || !self.settings.pet_music_curiosity {
                        let _ = self.intro_alpha(sprite, 0.0, 1.0, 0.55, INTRO_RETURN_MS);
                    }
                }
                self.set_timer(Timer::MusicIntro, INTRO_RETURN_MS as u32 + 30);
            }
            Some(IntroPhase::Returning) => {
                self.pill_motion.intro = None;
                self.ui.track_in_flight = false;
                for sprite in [&self.collapsed.sprite, &self.pet.char_layer.sprite] {
                    let _ = sprite.StopAnimation(h!("Opacity"));
                    let _ = sprite.SetOpacity(1.0);
                }
                self.width_motion_done();
                self.update_eq();
                self.repaint_collapsed();
                for sprite in [&self.pill_motion.cover.sprite, &self.pill_motion.title.sprite] {
                    let _ = sprite.SetIsVisible(false);
                    for name in [h!("Offset"), h!("Scale"), h!("Opacity")] {
                        let _ = sprite.StopAnimation(name);
                    }
                }
                let _ = self.pill_motion.title_clip.StopAnimation(h!("RightInset"));
                self.update_weather_timer();
                self.schedule_autohide();
            }
            None => {}
        }
    }

    pub(in crate::island) fn pill_motion_busy(&self) -> bool {
        self.pill_motion.width.is_some() || self.pill_motion.intro.is_some()
    }

    pub(in crate::island) fn stop_pill_attachment_motion(&self) {
        let _ = self.collapsed.sprite.StopAnimation(h!("Offset"));
        for sprite in std::iter::once(&self.pet.char_layer.sprite).chain(self.music.spectrum.iter()) {
            let _ = sprite.StopAnimation(h!("Offset"));
        }
    }

    pub(in crate::island) fn width_motion_ms(&self) -> u64 {
        if self.pill_motion.intro == Some(IntroPhase::Returning) { INTRO_RETURN_MS } else { WIDTH_MS }
    }

    pub(in crate::island) fn width_room(&self) -> f32 {
        self.pill_motion.width.map_or(self.cw, |tween| tween.room.max(self.cw))
    }

    pub(in crate::island) fn collapsed_surface_rect(&self) -> view::Rect {
        let (x, y, w, h) = self.pill_rect(false);
        let offset = self.collapsed.sprite.Offset().unwrap_or_default();
        (x - offset.X, y - offset.Y, w, h)
    }

    pub(in crate::island) fn pill_screen_geometry(&self) -> view::Rect {
        let (at, size, _) = self.pill(Win::Small, false, false);
        let origin = self.window_origin(Win::Small);
        (at.X + origin.0, at.Y + origin.1, size.X, size.Y)
    }

    pub(in crate::island) fn animate_width_geometry(&self, from: view::Rect) -> Result<()> {
        self.restore_liquid_body();
        let origin = self.window_origin(Win::Small);
        let (at, size, radius) = self.pill(Win::Small, false, false);
        for name in [h!("Offset"), h!("Size"), h!("CornerRadius")] {
            self.geometry.StopAnimation(name)?;
        }
        self.geometry.SetOffset(Vector2 { X: from.0 - origin.0, Y: from.1 - origin.1 })?;
        self.geometry.SetSize(Vector2 { X: from.2, Y: from.3 })?;
        self.geometry.SetCornerRadius(radius)?;
        let easing = self
            .compositor
            .CreateCubicBezierEasingFunction(Vector2 { X: 0.45, Y: 0.0 }, Vector2 { X: 0.55, Y: 1.0 })?;
        for (name, target) in [(h!("Offset"), at), (h!("Size"), size)] {
            let animation = self.compositor.CreateVector2KeyFrameAnimation()?;
            animation.InsertKeyFrameWithEasingFunction(1.0, target, &easing)?;
            animation.SetDuration(Duration::from_millis(self.width_motion_ms()).into())?;
            self.geometry.StartAnimation(name, &animation)?;
        }
        Ok(())
    }

    pub(in crate::island) fn width_motion_done(&mut self) {
        if matches!(self.pill_motion.intro, Some(IntroPhase::Present | IntroPhase::Returning)) {
            return;
        }
        if self.pill_motion.width.take().is_none() || self.state != State::Collapsed {
            return;
        }
        let _ = self.pill_motion.previous.sprite.SetIsVisible(false);
        let _ = self.pill_motion.previous.sprite.StopAnimation(h!("Opacity"));
        if self.pill_motion.intro.is_none() {
            let _ = self.collapsed.sprite.StopAnimation(h!("Opacity"));
            let _ = self.collapsed.sprite.SetOpacity(1.0);
        }
        self.stop_pill_attachment_motion();
        self.set_decoration_through(false);
        self.place(Win::Small);
        for name in [h!("Offset"), h!("Size")] {
            let _ = self.geometry.StopAnimation(name);
        }
        let _ = self.set_geometry_now(Win::Small, false, false);
        self.start_liquid();
        self.maybe_show_notification();
    }

    pub(in crate::island) fn cancel_pill_motion(&mut self) {
        if !self.pill_motion_busy() {
            return;
        }
        self.kill_timer(Timer::WidthMotion);
        self.kill_timer(Timer::MusicIntro);
        self.ui.track_in_flight = false;
        self.set_decoration_through(false);
        self.pill_motion.intro = None;
        self.pill_motion.width = None;
        self.stop_pill_attachment_motion();
        let _ = self.pill_motion.previous.sprite.SetIsVisible(false);
        let _ = self.pill_motion.previous.sprite.StopAnimation(h!("Opacity"));
        for sprite in [&self.pill_motion.cover.sprite, &self.pill_motion.title.sprite] {
            let _ = sprite.SetIsVisible(false);
            for name in [h!("Offset"), h!("Scale"), h!("Opacity")] {
                let _ = sprite.StopAnimation(name);
            }
        }
        for sprite in [&self.collapsed.sprite, &self.pet.char_layer.sprite] {
            let _ = sprite.StopAnimation(h!("Opacity"));
            let _ = sprite.SetOpacity(1.0);
        }
        for name in [h!("Offset"), h!("Size")] {
            let _ = self.geometry.StopAnimation(name);
        }
        if self.state == State::Collapsed {
            self.cw = view::collapsed_width(&self.ui, COLLAPSED.0);
            self.place(Win::Small);
            let size = window_size(self.scale, Win::Small, self.docked(), self.cw, &window_room(&self.settings));
            let _ = self.collapsed.resize(size);
            self.fit_bump();
            self.update_offsets();
            let _ = self.set_geometry_now(Win::Small, false, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "system" fn test_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    #[test]
    fn only_the_first_named_playback_gets_an_entrance() {
        let mut gate = MusicEntranceGate::default();
        assert!(!gate.take(false, false, true));
        assert!(!gate.take(true, false, true));
        assert!(gate.take(true, true, true));
        assert!(!gate.take(true, true, true));
        assert!(!gate.take(false, true, true));
        assert!(!gate.take(true, true, true));
        let mut hidden = MusicEntranceGate::default();
        assert!(!hidden.take(true, true, false));
        assert!(!hidden.take(true, true, true));
    }

    #[test]
    fn width_changes_retarget_from_the_visible_size_and_keep_shrinking_edges_inside_the_window() {
        let start = Instant::now();
        for (from, to) in [
            ((0.0, 8.0, 500.0, 34.0), (0.0, 8.0, 200.0, 34.0)),
            ((900.0, 8.0, 200.0, 34.0), (750.0, 8.0, 500.0, 34.0)),
            ((0.0, 200.0, 42.0, 200.0), (0.0, 50.0, 42.0, 500.0)),
        ] {
            let tween = WidthTween { from, to, room: 500.0, start, duration: WIDTH_MS };
            for ms in (0..=360).step_by(15) {
                let frame = tween.at(start + Duration::from_millis(ms));
                assert!(frame.0 >= from.0.min(to.0) - 0.01);
                assert!(frame.0 + frame.2 <= (from.0 + from.2).max(to.0 + to.2) + 0.01);
                assert!(frame.1 >= from.1.min(to.1) - 0.01);
                assert!(frame.1 + frame.3 <= (from.1 + from.3).max(to.1 + to.3) + 0.01);
            }
            let now = start + Duration::from_millis(135);
            let visible = tween.at(now);
            let retarget = WidthTween { from: visible, to: from, room: 500.0, start: now, duration: WIDTH_MS };
            let first = retarget.at(now);
            assert!((first.0 - visible.0).abs() < 0.01 && (first.2 - visible.2).abs() < 0.01);
            assert!((retarget.at(now + Duration::from_secs(1)).2 - from.2).abs() < 0.01);
        }
    }

    #[test]
    #[ignore = "Requires an interactive Windows desktop and an isolated Composition runtime"]
    fn music_entrance_and_width_changes_complete_and_cancel_without_leaking_layers() -> Result<()> {
        use windows::Win32::System::WinRT::RoUninitialize;
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
        }
        struct Runtime;
        impl Drop for Runtime {
            fn drop(&mut self) {
                unsafe {
                    RoUninitialize();
                }
            }
        }
        let _runtime = Runtime;
        let _queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })?
        };
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(test_proc),
            hInstance: instance.into(),
            lpszClassName: w!("NightIsland.MusicMotion.Test"),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&class);
        }
        for style in [Style::Flat, Style::Liquid] {
            for (edge, float_pos) in Edge::ALL
                .into_iter()
                .map(|edge| (edge, None))
                .chain([(Edge::Top, Some([0, 80])), (Edge::Top, Some([App::screen_size().0 as i32, 80]))])
            {
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
                        class.lpszClassName,
                        w!("Music motion test"),
                        WS_POPUP,
                        0,
                        0,
                        1,
                        1,
                        None,
                        None,
                        Some(instance.into()),
                        None,
                    )?
                };
                let settings = Settings {
                    style,
                    placement: if float_pos.is_some() { Placement::Floating } else { Placement::Docked },
                    float_pos,
                    dock_edge: edge,
                    music_eq: false,
                    edge_spikes: false,
                    music_cover: style == Style::Flat,
                    music_title: style == Style::Flat,
                    pill_battery: if style == Style::Liquid {
                        crate::data::settings::BatteryStyle::Percent
                    } else {
                        crate::data::settings::BatteryStyle::Hidden
                    },
                    metrics_pill: if style == Style::Liquid {
                        ["cpu", "ram", "gpu"].map(String::from).to_vec()
                    } else {
                        Vec::new()
                    },
                    ..Settings::default()
                };
                let mut app = App::new_visuals(hwnd, 1.0, settings)?;
                app.ui.cfg = app.settings.clone();
                app.ui.char_inset = CHAR_INSET;
                app.place(Win::Small);
                app.ui.media = media::Media {
                    title: "Midnight City".into(),
                    artist: "M83".into(),
                    playing: true,
                    source: "TEST".into(),
                    cover: media::debug_cover(8, &[100, 80, 180, 255].repeat(64)),
                    ..Default::default()
                };
                if style == Style::Liquid {
                    app.ui.metrics.pill = "CPU  23%  RAM  64%  GPU  41%".into();
                    app.ui.privacy.mic = true;
                    app.ui.privacy.cam = true;
                }
                app.repaint_collapsed();
                app.width_motion_done();
                let before_fade = (app.cw, app.collapsed.size());
                app.maybe_start_music_intro();
                assert!(app.pill_motion.intro == Some(IntroPhase::Fading));
                app.repaint_collapsed();
                assert_eq!((app.cw, app.collapsed.size()), before_fade);
                app.music_intro_step();
                assert!(app.pill_motion.intro == Some(IntroPhase::Present));
                if style == Style::Flat
                    && edge == Edge::Top
                    && float_pos.is_none()
                    && let Some(path) = std::env::var_os("NI_MUSIC_PREVIEW")
                {
                    view::render_music_entrance_preview(&app.gpu, &app.ui, std::path::Path::new(&path))?;
                }
                assert!(app.pill_motion.cover.sprite.IsVisible()?);
                let presentation_width = app.cw;
                app.ui.recording_secs = Some(5);
                app.repaint_collapsed();
                assert_eq!(app.cw, presentation_width);
                app.width_motion_done();
                app.music_intro_step();
                assert!(app.pill_motion.intro == Some(IntroPhase::Returning));
                assert_eq!(app.pill_motion.width.unwrap().duration, INTRO_RETURN_MS);
                assert!(app.ui.track_in_flight);
                app.music_intro_step();
                assert!(!app.ui.track_in_flight);
                assert!(!app.pill_motion_busy());
                assert!(!app.pill_motion.cover.sprite.IsVisible()? && !app.pill_motion.title.sprite.IsVisible()?);
                assert_eq!(app.collapsed.sprite.Opacity()?, 1.0);
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                assert!(app.pill_motion.intro.is_none());
                if style == Style::Liquid {
                    app.start_idle();
                    assert!(app.liquid.idle_running && app.liquid.body_active.get());
                    app.stop_idle();
                    app.settings.liquid_idle_travel = false;
                    app.start_idle();
                    assert!(app.liquid.idle_running && app.liquid.body_active.get());
                    app.stop_idle();
                    app.settings.liquid_idle_travel = true;
                }
                app.ui.media.title = "A different track".into();
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                assert!(app.pill_motion.intro == Some(IntroPhase::Present));
                let visible = app.pill_motion.width.unwrap().at(Instant::now());
                app.ui.media.title = "Switched again immediately".into();
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                assert!((app.pill_motion.width.unwrap().from.2 - visible.2).abs() < 10.0);
                assert!(app.pill_motion.intro == Some(IntroPhase::Present));
                app.music_intro_step();
                app.music_intro_step();
                app.settings.music_track_intro = false;
                app.ui.media.title = "Legacy behavior".into();
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                assert!(app.pill_motion.intro.is_none());
                app.settings.music_track_intro = true;
                app.ui.notice = Some(('!', "Busy".into()));
                app.ui.media.title = "Started while a message was visible".into();
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                assert!(app.pill_motion.intro.is_none());
                app.ui.notice = None;
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                assert!(app.pill_motion.intro.is_none());
                app.ui.notice = Some(('\u{E722}', "A longer notification that expands the island".into()));
                app.repaint_collapsed();
                assert!(app.pill_motion.width.is_some());
                assert!(app.pill_motion.previous.sprite.IsVisible()?);
                let mut window = windows::Win32::Foundation::RECT::default();
                unsafe {
                    GetWindowRect(hwnd, &mut window)?;
                }
                let tween = app.pill_motion.width.unwrap();
                for ms in (0..=360).step_by(30) {
                    let frame = tween.at(tween.start + Duration::from_millis(ms));
                    if !app.settings.vertical() {
                        assert!(frame.0 >= window.left as f32 - 1.0 && frame.0 + frame.2 <= window.right as f32 + 1.0);
                    } else {
                        assert!(frame.1 >= window.top as f32 - 1.0 && frame.1 + frame.3 <= window.bottom as f32 + 1.0);
                    }
                }
                app.ui.notice = None;
                app.repaint_collapsed();
                app.cancel_pill_motion();
                assert!(!app.pill_motion_busy());
                assert!(!app.pill_motion.previous.sprite.IsVisible()?);
                assert_eq!(app.window_size(Win::Small), app.window_size(Win::Big));
                assert_eq!(app.window_origin(Win::Small), app.window_origin(Win::Big));
                app.ui.notice = Some(('!', "Copied".into()));
                app.repaint_collapsed();
                app.width_motion_done();
                let width = app.cw;
                app.ui.notice = Some(('?', "Copied".into()));
                app.repaint_collapsed();
                assert_eq!(app.cw, width);
                assert!(app.pill_motion.width.is_some());
                assert!(app.pill_motion.previous.sprite.IsVisible()?);
                app.width_motion_done();
                app.ui.notice = None;
                app.ui.volume = Some(0.4);
                app.repaint_collapsed();
                app.width_motion_done();
                app.ui.volume = Some(0.5);
                app.repaint_collapsed();
                assert!(app.pill_motion.width.is_none());
                app.ui.volume = None;
                app.repaint_collapsed();
                assert!(app.pill_motion.previous.sprite.IsVisible()?);
                app.width_motion_done();
                app.settings.island_animations = false;
                app.ui.notice = Some(('!', "Copied".into()));
                app.repaint_collapsed();
                assert!(app.pill_motion.width.is_none());
                assert!(!app.pill_motion.previous.sprite.IsVisible()?);
                app.ui.notice = None;
                app.repaint_collapsed();
                app.settings.island_animations = true;
                if float_pos.is_none() {
                    app.settings.character = Character::Spark;
                    app.set_character_detached(true);
                    let from = app.pet.desktop.as_ref().unwrap().center();
                    app.begin_pet_return();
                    assert!(app.pet.returning.is_some());
                    assert!(app.pet.desktop.is_some());
                    app.pet.returning.as_mut().unwrap().start = Instant::now() - Duration::from_millis(260);
                    app.char_tick();
                    let center = app.pet.desktop.as_ref().unwrap().center();
                    assert!(center.0.is_finite() && center.1.is_finite());
                    assert_ne!(from, center);
                    let hwnd = app.pet.desktop.as_ref().unwrap().hwnd();
                    assert_ne!(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize, 0);
                    app.pet.returning.as_mut().unwrap().start = Instant::now() - Duration::from_millis(600);
                    app.char_tick();
                    assert!(app.pet.returning.is_none() && app.pet.desktop.is_none());
                    assert!(!app.settings.character_detached);
                    app.set_character_detached(true);
                    app.begin_pet_return();
                    app.char_tick();
                    app.pill_hidden = true;
                    app.char_tick();
                    assert!(app.pet.returning.is_none() && app.pet.desktop.is_some());
                    let hwnd = app.pet.desktop.as_ref().unwrap().hwnd();
                    assert_eq!(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize, 0);
                    app.pill_hidden = false;
                    app.settings.pet_return_animation = false;
                    app.begin_pet_return();
                    assert!(app.pet.returning.is_none() && app.pet.desktop.is_none());
                }
                app.pill_motion.gate = Default::default();
                app.maybe_start_music_intro();
                if app.pill_motion.intro == Some(IntroPhase::Fading) {
                    app.music_intro_step();
                }
                app.ui.tab = view::Tab::Note;
                app.expand();
                assert!(app.state == State::Expanded && !app.pill_motion_busy());
                let local = app.collapsed_surface_rect();
                let pad = app.small_pad();
                assert!((local.0 - pad.0 * app.scale).abs() < 0.01);
                assert!((local.1 - pad.1 * app.scale).abs() < 0.01);
                assert!(!app.pill_motion.cover.sprite.IsVisible()?);
                app.music_intro_step();
                assert!(app.state == State::Expanded && app.pill_motion.intro.is_none());
                drop(app);
                unsafe {
                    DestroyWindow(hwnd)?;
                }
            }
        }
        Ok(())
    }
}
