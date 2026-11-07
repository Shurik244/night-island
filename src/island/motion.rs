use crate::island::*;

pub(in crate::island) const OPEN_BASE_MS: f32 = 420.0;
pub(in crate::island) const CLOSE_BASE_MS: f32 = 320.0;
pub(in crate::island) const AUTOHIDE_DELAY_MS: u32 = 1500;

pub(in crate::island) struct OpenMotion {
    pub(in crate::island) progress: crate::ui::reveal::Reveal,
    closed: (Vector2, Vector2, Vector2),
}

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;

impl App {
    pub(in crate::island) fn open_content_point(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let Some(motion) = &self.open_motion else { return Some((x, y)) };
        let p = motion.progress.value(Instant::now());
        if p < 0.72 {
            return None;
        }
        let (at, size, _) = motion.closed;
        let (to_at, to_size, _) = self.pill(self.win, true, false);
        let width = size.X + (to_size.X - size.X) * p;
        let height = size.Y + (to_size.Y - size.Y) * p;
        let scale = (width / to_size.X).min(height / to_size.Y).clamp(0.01, 1.0);
        let dx = (at.X + size.X * 0.5 - to_at.X - to_size.X * 0.5) * (1.0 - p);
        let dy = (at.Y + size.Y * 0.5 - to_at.Y - to_size.Y * 0.5) * (1.0 - p);
        let full = self.pill_full();
        let center = (full.0 + full.2 * 0.5, full.1 + full.3 * 0.5);
        Some((
            ((x as f32 - dx - center.0) / scale + center.0).round() as i32,
            ((y as f32 - dy - center.1) / scale + center.1).round() as i32,
        ))
    }

    pub(in crate::island) fn transition_hit(&self, x: i32, y: i32) -> Option<bool> {
        let motion = self.open_motion.as_ref()?;
        let now = Instant::now();
        if !motion.progress.active(now) {
            return None;
        }
        let p = motion.progress.value(now);
        let (at, size, radius) = motion.closed;
        let (to_at, to_size, to_radius) = self.pill(self.win, true, false);
        let lerp = |a: f32, b: f32| a + (b - a) * p;
        Some(view::rounded_body_contains(
            (lerp(at.X, to_at.X), lerp(at.Y, to_at.Y), lerp(size.X, to_size.X), lerp(size.Y, to_size.Y)),
            lerp(radius.X, to_radius.X),
            (x as f32, y as f32),
        ))
    }

    pub(in crate::island) fn transition_ms(&self, expanded: bool) -> u64 {
        self.open_motion.as_ref().map_or_else(
            || if expanded { self.open_ms() } else { self.close_ms() },
            |motion| motion.progress.duration().as_millis().max(1) as u64,
        )
    }

    fn open_scalar(&self, value: impl Fn(f32) -> f32) -> Result<CompositionAnimation> {
        let motion = self.open_motion.as_ref().unwrap();
        let animation = self.compositor.CreateScalarKeyFrameAnimation()?;
        let linear = self.compositor.CreateLinearEasingFunction()?;
        for i in 0..=60 {
            let t = i as f32 / 60.0;
            animation.InsertKeyFrameWithEasingFunction(t, value(motion.progress.frame(t)), &linear)?;
        }
        animation.SetDuration(motion.progress.duration().max(Duration::from_millis(1)).into())?;
        animation.cast()
    }

    pub(in crate::island) fn open_vector(&self, from: Vector2, to: Vector2) -> Result<CompositionAnimation> {
        let motion = self.open_motion.as_ref().unwrap();
        let animation = self.compositor.CreateVector2KeyFrameAnimation()?;
        let linear = self.compositor.CreateLinearEasingFunction()?;
        for i in 0..=60 {
            let t = i as f32 / 60.0;
            let p = motion.progress.frame(t);
            let value = Vector2 { X: from.X + (to.X - from.X) * p, Y: from.Y + (to.Y - from.Y) * p };
            animation.InsertKeyFrameWithEasingFunction(t, value, &linear)?;
        }
        animation.SetDuration(motion.progress.duration().max(Duration::from_millis(1)).into())?;
        animation.cast()
    }

    pub(in crate::island) fn open_vector3(&self, value: impl Fn(f32) -> Vector3) -> Result<CompositionAnimation> {
        let motion = self.open_motion.as_ref().unwrap();
        let animation = self.compositor.CreateVector3KeyFrameAnimation()?;
        let linear = self.compositor.CreateLinearEasingFunction()?;
        for i in 0..=60 {
            let t = i as f32 / 60.0;
            animation.InsertKeyFrameWithEasingFunction(t, value(motion.progress.frame(t)), &linear)?;
        }
        animation.SetDuration(motion.progress.duration().max(Duration::from_millis(1)).into())?;
        animation.cast()
    }

    fn animate_open_close(&mut self, expanded: bool) -> Result<()> {
        use crate::ui::reveal::smooth_range;

        let ms = if expanded { self.open_ms() } else { self.close_ms() };
        let closed = self.pill(self.win, false, false);
        let opened = self.pill(self.win, true, false);
        let motion = self.open_motion.get_or_insert_with(|| OpenMotion {
            progress: crate::ui::reveal::Reveal::new(f32::from(!expanded)),
            closed: if expanded {
                (
                    self.geometry.Offset().unwrap_or(closed.0),
                    self.geometry.Size().unwrap_or(closed.1),
                    self.geometry.CornerRadius().unwrap_or(closed.2),
                )
            } else {
                closed
            },
        });
        if !expanded && motion.progress.value(Instant::now()) == 1.0 {
            motion.closed = closed;
        }
        motion.progress.retarget(expanded, ms, self.settings.anim_curve, Instant::now());
        let closed = motion.closed;
        self.geometry.StartAnimation(h!("Offset"), &self.open_vector(closed.0, opened.0)?)?;
        self.geometry.StartAnimation(h!("Size"), &self.open_vector(closed.1, opened.1)?)?;
        self.geometry.StartAnimation(h!("CornerRadius"), &self.open_vector(closed.2, opened.2)?)?;
        self.tint.StartAnimation(
            h!("Opacity"),
            &self.open_scalar(|p| {
                self.settings.pill_opacity + (self.settings.tint_open - self.settings.pill_opacity) * p
            })?,
        )?;
        for layer in [&self.collapsed, &self.progress_fill, &self.pet.char_layer] {
            layer.sprite.StartAnimation(h!("Opacity"), &self.open_scalar(|p| 1.0 - smooth_range(p, 0.0, 0.25))?)?;
        }
        self.expanded.sprite.StartAnimation(h!("Opacity"), &self.open_scalar(|p| smooth_range(p, 0.72, 0.97))?)?;
        let full = self.pill_full();
        let center = Vector3 { X: full.0 + full.2 * 0.5, Y: full.1 + full.3 * 0.5, Z: 0.0 };
        self.expanded.sprite.SetCenterPoint(center)?;
        self.expanded.sprite.StartAnimation(
            h!("Scale"),
            &self.open_vector3(|p| {
                let width = closed.1.X + (opened.1.X - closed.1.X) * p;
                let height = closed.1.Y + (opened.1.Y - closed.1.Y) * p;
                let scale = (width / opened.1.X).min(height / opened.1.Y).clamp(0.0, 1.0);
                Vector3 { X: scale, Y: scale, Z: 1.0 }
            })?,
        )?;
        let dx = closed.0.X + closed.1.X * 0.5 - opened.0.X - opened.1.X * 0.5;
        let dy = closed.0.Y + closed.1.Y * 0.5 - opened.0.Y - opened.1.Y * 0.5;
        self.expanded.sprite.StartAnimation(
            h!("Offset"),
            &self.open_vector3(|p| Vector3 { X: dx * (1.0 - p), Y: dy * (1.0 - p), Z: 0.0 })?,
        )?;
        let (_, _, opacity) = self.ear_targets(self.win, false, false);
        self.bind_ears()?;
        for ear in &self.ears {
            ear.sprite.StartAnimation(
                h!("Opacity"),
                &self.open_scalar(|p| opacity * self.settings.pill_opacity * (1.0 - smooth_range(p, 0.0, 0.3)))?,
            )?;
        }
        Ok(())
    }

    pub(in crate::island) fn ease(&self) -> Result<windows::UI::Composition::CompositionEasingFunction> {
        let (x1, y1, x2, y2) =
            crate::ui::anim::bezier_points(self.settings.anim_curve).unwrap_or((0.33, 0.33, 0.67, 0.67));
        self.compositor.CreateCubicBezierEasingFunction(Vector2 { X: x1, Y: y1 }, Vector2 { X: x2, Y: y2 })?.cast()
    }

    pub(in crate::island) fn spring(&self, target: Vector2, ms: u64) -> Result<CompositionAnimation> {
        let anim = self.compositor.CreateVector2KeyFrameAnimation()?;
        anim.InsertKeyFrameWithEasingFunction(1.0, target, &self.ease()?)?;
        anim.SetDuration(Duration::from_millis(ms).into())?;
        anim.cast()
    }

    pub(in crate::island) fn animate_to(&mut self, expanded: bool, hidden: bool) -> Result<()> {
        self.restore_liquid_body();
        let ms = if expanded { self.open_ms() } else { self.close_ms() };
        let _ = self.collapsed.sprite.SetIsVisible(true);
        let _ = self.progress_fill.sprite.SetIsVisible(true);
        let _ = self.expanded.sprite.SetIsVisible(true);
        if self.attached_character_on() {
            let _ = self.pet.char_layer.sprite.SetIsVisible(true);
        }
        if self.win == Win::Big && !hidden {
            return self.animate_open_close(expanded);
        }
        let (offset, size, radius) = self.pill(self.win, expanded, hidden);
        self.geometry.StartAnimation(h!("Offset"), &self.spring(offset, ms)?)?;
        self.geometry.StartAnimation(h!("Size"), &self.spring(size, ms)?)?;
        self.geometry.StartAnimation(h!("CornerRadius"), &self.spring(radius, ms)?)?;

        let fade = self.compositor.CreateScalarKeyFrameAnimation()?;
        fade.InsertKeyFrame(1.0, if expanded { self.settings.tint_open } else { self.settings.pill_opacity })?;
        fade.SetDuration(Duration::from_millis(ms * 7 / 10).into())?;
        self.tint.StartAnimation(h!("Opacity"), &fade)?;

        for (layer, shown) in [
            (&self.collapsed, !expanded),
            (&self.progress_fill, !expanded),
            (&self.expanded, expanded),
            (&self.pet.char_layer, !expanded),
        ] {
            let a = self.compositor.CreateScalarKeyFrameAnimation()?;
            a.InsertKeyFrame(1.0, if shown { 1.0 } else { 0.0 })?;
            let fade_ms = if shown { ms * 6 / 10 } else { ms / 3 };
            a.SetDuration(Duration::from_millis(fade_ms).into())?;
            layer.sprite.StartAnimation(h!("Opacity"), &a)?;
        }

        let (_, _, ear_opacity) = self.ear_targets(self.win, expanded, hidden);
        self.bind_ears()?;
        for ear in &self.ears {
            let a = self.compositor.CreateScalarKeyFrameAnimation()?;
            a.InsertKeyFrame(1.0, ear_opacity)?;
            a.SetDuration(Duration::from_millis(ms / 2).into())?;
            ear.sprite.StartAnimation(h!("Opacity"), &a)?;
        }
        Ok(())
    }

    pub(in crate::island) fn expand(&mut self) {
        if self.state == State::Expanded {
            return;
        }
        self.set_decoration_through(false);
        self.kill_timer(Timer::Expand);
        let notification_origin = self.system_notices.origin.take();
        self.hide_notification_card();
        self.cancel_pill_motion();
        match self.state {
            State::Expanded => return,
            State::Collapsing => self.kill_timer(Timer::Shrink),
            State::Hiding => self.kill_timer(Timer::HideDone),
            _ => {}
        }
        self.stop_idle();
        self.kill_timer(Timer::Autohide);
        self.set_blur(true);
        if self.win != Win::Big {
            let hidden = self.state == State::Hidden;
            self.place(Win::Big);
            let _ = self.set_geometry_now(Win::Big, false, hidden);
        }
        self.state = State::Expanded;
        if self.ui.tab == view::Tab::Chat && self.ui.chat.pet.enabled {
            self.sync_pet_studio();
        }
        if let Some((x, y, w, h)) = notification_origin {
            let (wx, wy) = self.window_origin(Win::Big);
            let _ = self.geometry.SetOffset(windows_numerics::Vector2 { X: x - wx, Y: y - wy });
            let _ = self.geometry.SetSize(windows_numerics::Vector2 { X: w, Y: h });
            let _ =
                self.geometry.SetCornerRadius(windows_numerics::Vector2 { X: 26.0 * self.scale, Y: 26.0 * self.scale });
        }
        self.sync_mixer();
        self.update_eq();
        self.sync_metrics();
        let _ = self.animate_to(true, false);
        self.update_top_panel(false);
        self.set_timer(Timer::Settle, self.transition_ms(true) as u32 + 30);
        self.start_cover_motion(true);
        self.update_weather_timer();
        if matches!(self.ui.tab, view::Tab::Controls | view::Tab::Home) {
            self.select_tab(self.ui.tab);
        }
    }

    pub(in crate::island) fn collapse(&mut self) {
        if self.state != State::Expanded || self.ui.remind.alert.is_some() {
            return;
        }
        self.ui.cmd.open = false;
        self.ui.armed = None;
        self.ui.home.dev_menu = None;
        self.close_home_mixer();
        self.end_edit(true);
        self.kill_timer(Timer::Collapse);
        self.kill_timer(Timer::Settle);
        self.state = State::Collapsing;
        self.update_eq();
        if !self.open_motion.as_ref().is_some_and(|motion| motion.progress.active(Instant::now())) {
            self.cw = view::collapsed_width(&self.ui, COLLAPSED.0);
        }
        let small = window_size(self.scale, Win::Small, self.docked(), self.cw, &window_room(&self.settings));
        let _ = self.collapsed.resize(small);
        self.update_offsets();
        self.repaint_collapsed();
        self.finish_mixer();
        self.sync_mixer();
        self.update_weather_timer();
        let _ = self.animate_to(false, false);
        self.update_top_panel(false);
        self.start_cover_motion(false);
        self.set_timer(Timer::Shrink, self.transition_ms(false) as u32 + 30);
    }

    pub(in crate::island) fn shrink_window(&mut self) {
        if self.state != State::Collapsing {
            return;
        }
        self.finish_cover_motion();
        self.open_motion = None;
        self.ui.home.track_transition.finish();
        self.state = State::Collapsed;
        self.set_blur(false);
        self.win = Win::Small;
        let small = window_size(self.scale, Win::Small, self.docked(), self.cw, &window_room(&self.settings));
        let _ = self.collapsed.resize(small);
        self.resize_ring();
        self.fit_bump();
        let _ = self.set_geometry_now(Win::Small, false, false);
        self.update_offsets();
        self.schedule_autohide();
        self.start_liquid();
        self.update_eq();
        self.repaint_collapsed();
        self.settle_layers();
        let _ = self.compositor.RequestCommitAsync();
        self.place(Win::Small);
        self.update_top_panel(false);
        self.sync_metrics();
        self.maybe_show_notification();
    }

    pub(in crate::island) fn settle_layers(&self) {
        if self.pill_motion_busy()
            || self.open_motion.as_ref().is_some_and(|motion| motion.progress.active(Instant::now()))
        {
            return;
        }
        let expanded = match self.state {
            State::Expanded => true,
            State::Collapsed => false,
            _ => return,
        };
        let _ = self.collapsed.sprite.SetIsVisible(!expanded);
        let _ = self.progress_fill.sprite.SetIsVisible(!expanded);
        let _ = self.expanded.sprite.SetIsVisible(expanded);
        if expanded {
            let _ = self.pet.char_layer.sprite.SetIsVisible(false);
            self.set_bump_visible(false);
            for bar in &self.music.spectrum {
                let _ = bar.SetIsVisible(false);
            }
        } else {
            let _ = self.pet.char_layer.sprite.SetIsVisible(self.attached_character_on());
        }
        let tint_want = if expanded { self.settings.tint_open } else { self.settings.pill_opacity };
        let layers = [
            ("collapsed", &self.collapsed.sprite, if expanded { 0.0 } else { 1.0 }),
            ("progress", &self.progress_fill.sprite, if expanded { 0.0 } else { 1.0 }),
            ("expanded", &self.expanded.sprite, if expanded { 1.0 } else { 0.0 }),
            ("character", &self.pet.char_layer.sprite, if expanded { 0.0 } else { 1.0 }),
        ];
        for (name, sprite, want) in layers {
            let have = sprite.Opacity().unwrap_or(want);
            if (have - want).abs() > 0.02 {
                crate::log_warn!("layer {name} stuck at opacity {have:.2}, should be {want:.2}; fixed");
                let _ = sprite.StopAnimation(h!("Opacity"));
                let _ = sprite.SetOpacity(want);
            }
        }
        let have = self.tint.Opacity().unwrap_or(tint_want);
        if (have - tint_want).abs() > 0.02 {
            crate::log_warn!("tint stuck at opacity {have:.2}, should be {tint_want:.2}; fixed");
            let _ = self.tint.StopAnimation(h!("Opacity"));
            let _ = self.tint.SetOpacity(tint_want);
        }
    }

    pub(in crate::island) fn schedule_autohide(&self) {
        if !self.notification_card_visible()
            && self.pill_motion.intro.is_none()
            && self.autohide_ok()
            && self.state == State::Collapsed
            && !self.cursor_inside()
        {
            self.set_timer(Timer::Autohide, AUTOHIDE_DELAY_MS);
        }
    }

    pub(in crate::island) fn hide(&mut self) {
        self.hide_notification_card();
        self.cancel_pill_motion();
        if self.state != State::Collapsed {
            return;
        }
        self.state = State::Hiding;
        self.update_weather_timer();
        self.update_eq();
        let _ = self.animate_to(false, true);
        self.set_timer(Timer::HideDone, self.close_ms() as u32 + 60);
    }

    pub(in crate::island) fn hide_done(&mut self) {
        if self.state != State::Hiding {
            return;
        }
        self.state = State::Hidden;
        self.place(Win::Strip);
        let _ = self.set_geometry_now(Win::Strip, false, true);
    }

    pub(in crate::island) fn reveal(&mut self) {
        match self.state {
            State::Hiding => self.kill_timer(Timer::HideDone),
            State::Hidden => {
                self.place(Win::Small);
                let _ = self.set_geometry_now(Win::Small, false, true);
            }
            _ => return,
        }
        self.state = State::Collapsed;
        let _ = self.animate_to(false, false);
        self.start_liquid();
        self.update_eq();
        self.update_weather_timer();
    }

    pub(in crate::island) fn peek(&mut self) {
        self.expand();
        if !self.cursor_inside() {
            self.set_timer(Timer::Collapse, 3000);
        }
    }

    pub(in crate::island) fn toggle(&mut self) {
        match self.state {
            State::Expanded => self.collapse(),
            _ => self.expand(),
        }
    }
}
