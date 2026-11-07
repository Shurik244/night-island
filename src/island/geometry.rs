use crate::island::*;

pub(in crate::island) const COLLAPSED: (f32, f32) = (200.0, 34.0);
pub(in crate::island) const EXPANDED: (f32, f32) = (640.0, 384.0);
pub(in crate::island) const TOP_MARGIN: f32 = 8.0;
pub(in crate::island) const PAD: f32 = 20.0;
pub(in crate::island) const EXPANDED_RADIUS: f32 = 28.0;
pub(in crate::island) const EAR: f32 = 14.0;
pub(in crate::island) const DOCK_RADIUS: f32 = 16.0;
pub(in crate::island) const STRIP_H: i32 = 3;

pub(in crate::island) static SNAP_FIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(in crate::island) const RING_ROOM: f32 = 40.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::island) enum OpenEq {
    Off,
    Under,
    Around,
}

#[derive(Clone, Copy)]
pub(in crate::island) struct Room {
    pub bottom: f32,
    pub liquid: bool,
    pub ring: bool,
    pub open: OpenEq,
    pub vertical: bool,
    pub hang: Hang,
}

pub(in crate::island) fn small_pads(room: &Room, docked: bool) -> (f32, f32, f32) {
    let base_top = if docked { 0.0 } else { TOP_MARGIN };
    let ring = if room.ring { RING_ROOM } else { 0.0 };
    let inward = room.bottom;
    if room.liquid {
        let top = if docked && room.hang == Hang::Down { 0.0 } else { base_top.max(BUMP_ROOM) };
        let bottom = if docked && room.hang == Hang::Up { ring } else { BUMP_ROOM.max(ring) };
        return (BUMP_ROOM.max(PAD).max(ring), top, bottom);
    }
    match room.hang {
        Hang::Down => (PAD.max(ring), base_top.max(ring), inward),
        Hang::Up => (PAD.max(ring), base_top.max(inward).max(ring), ring),
        Hang::Left | Hang::Right => (PAD.max(inward).max(ring), base_top.max(ring), ring),
    }
}

pub(in crate::island) fn collapsed_dims(vertical: bool, cw: f32) -> (f32, f32) {
    if vertical { (view::VERT_W, cw) } else { (cw, COLLAPSED.1) }
}

pub(in crate::island) fn window_room(settings: &Settings) -> Room {
    let open = match (settings.eq_when_open && settings.edge_spikes, settings.eq_style.around()) {
        (false, _) => OpenEq::Off,
        (true, false) => OpenEq::Under,
        (true, true) => OpenEq::Around,
    };
    Room {
        bottom: bump_room_for(settings),
        liquid: settings.style == Style::Liquid,
        ring: settings.ring_wanted(),
        open,
        vertical: settings.vertical(),
        hang: settings.hang(),
    }
}

pub(in crate::island) fn big_pad(open: OpenEq, docked: bool) -> (f32, f32, f32) {
    let top = if docked { 0.0 } else { TOP_MARGIN };
    match open {
        OpenEq::Off => (PAD, top, PAD),
        OpenEq::Under => (PAD, top, PAD.max(RING_ROOM)),
        OpenEq::Around => (PAD.max(RING_ROOM), top.max(RING_ROOM), PAD.max(RING_ROOM)),
    }
}

pub(in crate::island) fn window_size(scale: f32, win: Win, docked: bool, cw: f32, room: &Room) -> (i32, i32) {
    let (w, h) = if win == Win::Big { EXPANDED } else { collapsed_dims(room.vertical, cw) };
    let small = small_pads(room, docked);
    let pad = match win {
        Win::Small => small.0,
        Win::Big => big_pad(room.open, docked).0,
        Win::Strip => PAD,
    };
    let width = ((w + 2.0 * pad) * scale).round() as i32;
    match win {
        Win::Strip => (width, STRIP_H),
        Win::Small => (width, ((h + small.1 + small.2) * scale).round() as i32),
        Win::Big => {
            let (_, top, bottom) = big_pad(room.open, docked);
            (width, ((h + top + bottom) * scale).round() as i32)
        }
    }
}

impl App {
    pub(in crate::island) fn collapsed(&self) -> (f32, f32) {
        collapsed_dims(self.settings.vertical(), self.cw)
    }

    pub(in crate::island) fn edge(&self) -> Edge {
        self.settings.dock_edge
    }

    pub(in crate::island) fn autohide_ok(&self) -> bool {
        self.settings.auto_hide
            && !(self.docked() && self.edge() != Edge::Top)
            && (self.docked() || self.settings.float_pos.is_none())
    }

    pub(in crate::island) fn work_area() -> (f32, f32, f32, f32) {
        use windows::Win32::UI::WindowsAndMessaging::{
            SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
        };
        let mut rc = windows::Win32::Foundation::RECT::default();
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&raw mut rc).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok()
        };
        if ok && rc.right > rc.left && rc.bottom > rc.top {
            (rc.left as f32, rc.top as f32, rc.right as f32, rc.bottom as f32)
        } else {
            let (w, h) = Self::screen_size();
            (0.0, 0.0, w, h)
        }
    }

    pub(in crate::island) fn island_work_area(&self) -> view::Rect {
        let (left, top, right, bottom) = Self::work_area();
        (left, self.top_panel.as_ref().map_or(top, |panel| panel.top()), right, bottom)
    }

    pub(in crate::island) fn screen_size() -> (f32, f32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN) as f32, GetSystemMetrics(SM_CYSCREEN) as f32) }
    }

    pub(in crate::island) fn pill_screen(&self, expanded: bool) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (sw, sh) = Self::screen_size();
        let (wl, wt, wr, wb) = self.island_work_area();
        let (cw, ch) = (self.collapsed().0 * s, self.collapsed().1 * s);
        let (ew, eh) = (EXPANDED.0 * s, EXPANDED.1 * s);
        let clamp = |v: f32, lo: f32, hi: f32| v.max(lo).min(hi.max(lo));
        let (cx, cy) = if self.docked() {
            match self.edge() {
                Edge::Top => (clamp(self.settings.dock_offset * sw - cw / 2.0, 0.0, sw - cw), wt),
                Edge::Bottom => (clamp(self.settings.dock_offset * sw - cw / 2.0, 0.0, sw - cw), wb - ch),
                Edge::Left => (wl, clamp(self.settings.dock_offset * sh - ch / 2.0, wt, wb - ch)),
                Edge::Right => (wr - cw, clamp(self.settings.dock_offset * sh - ch / 2.0, wt, wb - ch)),
            }
        } else {
            match self.settings.float_pos {
                Some([x, y]) => (clamp(x as f32 - cw / 2.0, 0.0, sw - cw), clamp(y as f32, 0.0, sh - ch)),
                None => ((sw - cw) / 2.0, self.top_panel.as_ref().map_or(0.0, |p| p.top()) + TOP_MARGIN * s),
            }
        };
        if !expanded {
            return (cx, cy, cw, ch);
        }
        let (mid_x, mid_y) = (cx + cw / 2.0, cy + ch / 2.0);
        let (ex, ey) = if self.docked() {
            match self.edge() {
                Edge::Top => (clamp(mid_x - ew / 2.0, 0.0, sw - ew), wt),
                Edge::Bottom => (clamp(mid_x - ew / 2.0, 0.0, sw - ew), wb - eh),
                Edge::Left => (wl, clamp(mid_y - eh / 2.0, wt, wb - eh)),
                Edge::Right => (wr - ew, clamp(mid_y - eh / 2.0, wt, wb - eh)),
            }
        } else {
            (clamp(mid_x - ew / 2.0, 0.0, sw - ew), clamp(cy, 0.0, sh - eh))
        };
        (ex, ey, ew, eh)
    }

    fn viewport(&self) -> (f32, f32, i32, i32) {
        let room = window_room(&self.settings);
        let small = window_size(self.scale, Win::Small, self.docked(), self.width_room(), &room);
        let big = window_size(self.scale, Win::Big, self.docked(), self.cw, &room);
        let closed = self.pill_screen(false);
        let open = self.pill_screen(true);
        let (sx, sy) = self.small_pad();
        let (bx, by, _) = self.big_pad();
        let small_at = (closed.0 - sx * self.scale, closed.1 - sy * self.scale);
        let big_at = (open.0 - bx * self.scale, open.1 - by * self.scale);
        let mut x = small_at.0.min(big_at.0).floor();
        let mut y = small_at.1.min(big_at.1).floor();
        let mut right = (small_at.0 + small.0 as f32).max(big_at.0 + big.0 as f32).ceil();
        let mut bottom = (small_at.1 + small.1 as f32).max(big_at.1 + big.1 as f32).ceil();
        if let Some(tween) = self.pill_motion.width {
            let (_, top, lower) = small_pads(&room, self.docked());
            let work = self.island_work_area();
            for rect in [tween.from, tween.to] {
                let (mut left, mut upper, mut end, mut lower_edge) = (rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3);
                if self.docked() {
                    match self.edge() {
                        Edge::Top => upper = upper.max(work.1),
                        Edge::Bottom => lower_edge = lower_edge.min(work.3),
                        Edge::Left => left = left.max(work.0),
                        Edge::Right => end = end.min(work.2),
                    }
                }
                x = x.min((left - sx * self.scale).floor());
                y = y.min((upper - top * self.scale).floor());
                right = right.max((end + sx * self.scale).ceil());
                bottom = bottom.max((lower_edge + lower * self.scale).ceil());
            }
        }
        (x, y, (right - x) as i32, (bottom - y) as i32)
    }

    pub(in crate::island) fn window_origin(&self, win: Win) -> (f32, f32) {
        if win == Win::Strip {
            return (self.pill_screen(false).0 - PAD * self.scale, 0.0);
        }
        let frame = self.viewport();
        (frame.0, frame.1)
    }

    pub(in crate::island) fn docked(&self) -> bool {
        self.settings.placement == Placement::Docked
    }

    pub(in crate::island) fn small_pad(&self) -> (f32, f32) {
        let (x, t, _) = small_pads(&window_room(&self.settings), self.docked());
        (x, t)
    }

    pub(in crate::island) fn big_pad(&self) -> (f32, f32, f32) {
        big_pad(window_room(&self.settings).open, self.docked())
    }

    pub(in crate::island) fn open_ms(&self) -> u64 {
        if self.settings.island_animations && crate::ui::feedback::motion_allowed() {
            (OPEN_BASE_MS / self.settings.open_speed) as u64
        } else {
            1
        }
    }

    pub(in crate::island) fn close_ms(&self) -> u64 {
        if self.settings.island_animations && crate::ui::feedback::motion_allowed() {
            (CLOSE_BASE_MS / self.settings.close_speed) as u64
        } else {
            1
        }
    }

    pub(in crate::island) fn window_size(&self, win: Win) -> (i32, i32) {
        if win == Win::Strip {
            return window_size(self.scale, win, self.docked(), self.cw, &window_room(&self.settings));
        }
        let frame = self.viewport();
        (frame.2, frame.3)
    }

    pub(in crate::island) fn pill_full(&self) -> view::Rect {
        let at = self.pill_screen(true);
        let origin = self.window_origin(self.win);
        (at.0 - origin.0, at.1 - origin.1, at.2, at.3)
    }

    pub(in crate::island) fn pill_rect(&self, expanded: bool) -> view::Rect {
        let at = self.pill_screen(expanded);
        let origin = self.window_origin(self.win);
        let full = (at.0 - origin.0, at.1 - origin.1, at.2, at.3);
        if expanded { view::body_rect(full, self.scale, self.ui.tab) } else { full }
    }

    pub(in crate::island) fn resize_ring(&mut self) {
        let size = self.window_size(self.win);
        if self.liquid.ring.size() == size {
            return;
        }
        let _ = self.liquid.ring.resize(size);
        let _ = self.liquid.ring.paint(&self.gpu, |_| {});
        let _ = self.liquid.ring.sprite.SetIsVisible(false);
    }

    pub(in crate::island) fn update_offsets(&self) {
        let pill = self.pill_rect(false);
        let pad = self.small_pad();
        let _ = self.collapsed.sprite.SetOffset(Vector3 {
            X: pill.0 - pad.0 * self.scale,
            Y: pill.1 - pad.1 * self.scale,
            Z: 0.0,
        });
        let _ = self.expanded.sprite.SetOffset(Vector3::default());
        let _ = self.liquid.ring.sprite.SetOffset(Vector3::default());
        self.place_bump();
        self.place_spectrum();
        self.place_char();
    }

    pub(in crate::island) fn fit_width(&mut self) {
        let content = super::pill_motion::PillContent::current(&self.ui);
        let content_changed = self.pill_motion.content.as_ref().is_some_and(|previous| *previous != content);
        self.pill_motion.content = Some(content);
        if self.state != State::Collapsed {
            return;
        }
        let animate = !SNAP_FIT.load(std::sync::atomic::Ordering::Relaxed)
            && self.settings.island_animations
            && crate::ui::feedback::motion_allowed()
            && !self.pill_hidden
            && !self.gaming
            && !self.notification_card_visible();
        let normal = view::collapsed_width(&self.ui, COLLAPSED.0);
        let want = if self.pill_motion.intro.is_some() { self.pill_motion.intro_width } else { normal };
        if (want - self.cw).abs() < 0.5 && !(animate && content_changed) {
            return;
        }
        let now = Instant::now();
        let from = self.pill_motion.width.map_or_else(|| self.pill_screen_geometry(), |tween| tween.at(now));
        let old_origin = self.window_origin(Win::Small);
        let old_target = self.pill_screen_geometry();
        let attachments: Vec<_> = std::iter::once(&self.pet.char_layer.sprite)
            .chain(self.music.spectrum.iter())
            .map(|sprite| (sprite.clone(), sprite.Offset().unwrap_or_default()))
            .collect();
        let old_offset = self.collapsed.sprite.Offset().unwrap_or_default();
        let crossfade = animate && self.pill_motion.intro.is_none();
        if crossfade {
            std::mem::swap(&mut self.collapsed, &mut self.pill_motion.previous);
            let _ = self.collapsed.sprite.StopAnimation(h!("Opacity"));
            let _ = self.collapsed.sprite.SetIsVisible(true);
        }
        self.reset_liquid_pull();
        self.cw = want;
        let to = self.pill_screen_geometry();
        let room = if self.settings.vertical() {
            (from.1 + from.3).max(to.1 + to.3) - from.1.min(to.1)
        } else {
            (from.0 + from.2).max(to.0 + to.2) - from.0.min(to.0)
        } / self.scale;
        let duration = self.width_motion_ms();
        self.pill_motion.width =
            animate.then_some(super::pill_motion::WidthTween { from, to, room, start: now, duration });
        if animate {
            self.set_bump_visible(false);
            let _ = self.liquid.ring.sprite.SetIsVisible(false);
        }
        self.place(Win::Small);
        let small = window_size(self.scale, Win::Small, self.docked(), self.cw, &window_room(&self.settings));
        let _ = self.collapsed.resize(small);
        self.resize_ring();
        self.fit_bump();
        self.update_offsets();
        if animate {
            let _ = self.animate_width_geometry(from);
            let origin = self.window_origin(Win::Small);
            for (sprite, old) in attachments {
                let target = sprite.Offset().unwrap_or_default();
                let start = Vector3 {
                    X: old.X + old_origin.0 - origin.0 + from.0 - old_target.0,
                    Y: old.Y + old_origin.1 - origin.1 + from.1 - old_target.1,
                    Z: 0.0,
                };
                let _ = self.animate_pill_vector(&sprite, h!("Offset"), start, target, duration);
            }
            self.set_timer(Timer::WidthMotion, duration as u32 + 30);
            self.set_timer(Timer::Liquid, 60);
            if crossfade {
                let origin = self.window_origin(Win::Small);
                let target = self.collapsed.sprite.Offset().unwrap_or_default();
                let start = Vector3 { X: target.X + from.0 - to.0, Y: target.Y + from.1 - to.1, Z: 0.0 };
                let _ = self.animate_pill_vector(&self.collapsed.sprite, h!("Offset"), start, target, duration);
                let previous = &self.pill_motion.previous.sprite;
                let _ = previous.SetOffset(Vector3 {
                    X: old_offset.X + old_origin.0 - origin.0,
                    Y: old_offset.Y + old_origin.1 - origin.1,
                    Z: 0.0,
                });
                let _ = previous.StopAnimation(h!("Opacity"));
                let _ = previous.SetIsVisible(true);
                if let Ok(fade) = self.compositor.CreateScalarKeyFrameAnimation() {
                    let _ = fade.InsertKeyFrame(0.0, 1.0);
                    let _ = fade.InsertKeyFrame(1.0, 0.0);
                    let _ = fade.SetDuration(Duration::from_millis(230).into());
                    let _ = previous.StartAnimation(h!("Opacity"), &fade);
                }
                let fade = self.compositor.CreateScalarKeyFrameAnimation();
                if let Ok(fade) = fade {
                    let _ = fade.InsertKeyFrame(0.0, 0.0);
                    let _ = fade.InsertKeyFrame(1.0, 1.0);
                    let _ = fade.SetDuration(Duration::from_millis(230).into());
                    let _ = self.collapsed.sprite.StartAnimation(h!("Opacity"), &fade);
                }
            }
        } else {
            self.kill_timer(Timer::WidthMotion);
            self.stop_pill_attachment_motion();
            let _ = self.pill_motion.previous.sprite.SetIsVisible(false);
            for name in [h!("Offset"), h!("Size")] {
                let _ = self.geometry.StopAnimation(name);
            }
            if self.pill_motion.intro.is_none() {
                let _ = self.collapsed.sprite.StopAnimation(h!("Opacity"));
                let _ = self.collapsed.sprite.SetOpacity(1.0);
            }
            let _ = self.set_geometry_now(Win::Small, false, false);
        }
    }

    pub(in crate::island) fn pill(&self, win: Win, expanded: bool, hidden: bool) -> (Vector2, Vector2, Vector2) {
        let (w, h) = if expanded { EXPANDED } else { self.collapsed() };
        let docked = self.docked();
        let radius = if expanded {
            EXPANDED_RADIUS
        } else if docked {
            DOCK_RADIUS.min(w.min(h) / 2.0)
        } else {
            h / 2.0
        } * self.scale;
        let (w, h) = (w * self.scale, h * self.scale);
        let origin = self.window_origin(win);
        let at = self.pill_screen(expanded);
        let (mut x, mut y) = (at.0 - origin.0, at.1 - origin.1);
        let (mut width, mut height) = (w, h);
        if docked {
            match self.edge() {
                Edge::Top => {
                    y -= radius;
                    height += radius;
                }
                Edge::Bottom => height += radius,
                Edge::Left => {
                    x -= radius;
                    width += radius;
                }
                Edge::Right => width += radius,
            }
        }
        if hidden {
            y = if docked { -h - radius - 2.0 } else { -h - 2.0 };
        }
        (Vector2 { X: x, Y: y }, Vector2 { X: width, Y: height }, Vector2 { X: radius, Y: radius })
    }

    pub(in crate::island) fn bind_ears(&self) -> Result<()> {
        for (i, ear) in self.ears.iter().enumerate() {
            let expression = if i == 0 {
                h!("Vector3(body.Offset.X - radius, body.Offset.Y + body.CornerRadius.Y, 0)")
            } else {
                h!("Vector3(body.Offset.X + body.Size.X, body.Offset.Y + body.CornerRadius.Y, 0)")
            };
            let animation = self.compositor.CreateExpressionAnimationWithExpression(expression)?;
            animation.SetReferenceParameter(h!("body"), &self.geometry)?;
            animation.SetScalarParameter(h!("radius"), EAR * self.scale)?;
            ear.sprite.StartAnimation(h!("Offset"), &animation)?;
        }
        Ok(())
    }

    pub(in crate::island) fn ear_targets(&self, win: Win, expanded: bool, hidden: bool) -> (f32, f32, f32) {
        let (off, size, _) = self.pill(win, expanded, hidden);
        let r = EAR * self.scale;
        let shown = self.docked() && self.edge() == Edge::Top && !expanded && !hidden;
        (off.X - r, off.X + size.X, if shown { 1.0 } else { 0.0 })
    }

    pub(in crate::island) fn place(&mut self, win: Win) {
        self.win = win;
        let (w, h) = self.window_size(win);
        let (x, y) = self.window_origin(win);
        unsafe {
            let _ =
                SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x.round() as i32, y.round() as i32, w, h, SWP_NOACTIVATE);
        }
        unsafe {
            use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, SetWindowRgn};
            if win == Win::Small {
                let pill = self.pill_rect(false);
                let (px, py) = self.small_pad();
                let small =
                    window_size(self.scale, Win::Small, self.docked(), self.width_room(), &window_room(&self.settings));
                let mut left = (pill.0 - px * self.scale).floor() as i32;
                let mut top = (pill.1 - py * self.scale).floor() as i32;
                let right = left + small.0 + 1;
                let bottom = top + small.1 + 1;
                if let Some(tween) = self.pill_motion.width {
                    let work = self.island_work_area();
                    for rect in [tween.from, tween.to] {
                        let mut from = (rect.0, rect.1);
                        if self.docked() && self.edge() == Edge::Left {
                            from.0 = from.0.max(work.0);
                        }
                        if self.docked() && self.edge() == Edge::Top {
                            from.1 = from.1.max(work.1);
                        }
                        left = left.min((from.0 - x - px * self.scale).floor() as i32);
                        top = top.min((from.1 - y - py * self.scale).floor() as i32);
                    }
                }
                let region = CreateRectRgn(left, top, right, bottom);
                if SetWindowRgn(self.hwnd, Some(region), true) == 0 {
                    let _ = DeleteObject(region.into());
                }
            } else {
                SetWindowRgn(self.hwnd, None, true);
            }
        }
        let _ = self.expanded.resize((w, h));
        let _ = self.progress_fill.resize((w, h));
        let _ = self.liquid.highlight.resize((w, h));
        self.resize_ring();
        self.update_offsets();
    }

    pub(in crate::island) fn set_geometry_now(&self, win: Win, expanded: bool, hidden: bool) -> Result<()> {
        for property in [h!("Offset"), h!("Size"), h!("CornerRadius")] {
            self.geometry.StopAnimation(property)?;
        }
        if expanded || hidden || !self.liquid_on() {
            self.restore_liquid_body();
        }
        let (offset, size, radius) = self.pill(win, expanded, hidden);
        self.geometry.SetOffset(offset)?;
        self.geometry.SetSize(size)?;
        self.geometry.SetCornerRadius(radius)?;
        let (_, _, opacity) = self.ear_targets(win, expanded, hidden);
        self.bind_ears()?;
        for ear in &self.ears {
            ear.sprite.StopAnimation(h!("Opacity"))?;
            ear.sprite.SetOpacity(opacity * self.settings.pill_opacity)?;
        }
        Ok(())
    }

    pub(in crate::island) fn cursor_inside(&self) -> bool {
        unsafe {
            let mut pt = windows::Win32::Foundation::POINT::default();
            let mut rc = windows::Win32::Foundation::RECT::default();
            GetCursorPos(&mut pt).is_ok()
                && GetWindowRect(self.hwnd, &mut rc).is_ok()
                && windows::Win32::Graphics::Gdi::PtInRect(&rc, pt).as_bool()
        }
    }

    pub(in crate::island) fn reset_position(&mut self) {
        let fresh = Settings::default();
        self.settings.placement = fresh.placement;
        self.settings.dock_edge = fresh.dock_edge;
        self.settings.dock_offset = fresh.dock_offset;
        self.settings.float_pos = fresh.float_pos;
        self.settings.save();
        self.ui.cfg = self.settings.clone();
        if matches!(self.state, State::Hiding | State::Hidden) {
            self.reveal();
        }
        self.apply_layout();
        self.render_settings_win();
        self.set_timer(Timer::Settle, 700);
    }

    pub(in crate::island) fn apply_layout(&mut self) {
        self.open_motion = None;
        let _ = self.expanded.sprite.StopAnimation(h!("Scale"));
        let _ = self.expanded.sprite.StopAnimation(h!("Offset"));
        let _ = self.expanded.sprite.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
        self.finish_cover_motion();
        self.hide_notification_card();
        self.cancel_pill_motion();
        self.ui.cfg = self.settings.clone();
        if self.state != State::Expanded {
            self.cw = view::collapsed_width(&self.ui, COLLAPSED.0);
        }
        self.scale = self.dpi_scale * self.settings.scale_percent as f32 / 100.0;
        let _ = self.pill_motion.cover.resize(((28.0 * self.scale).ceil() as i32, (28.0 * self.scale).ceil() as i32));
        let _ = self.pill_motion.title.resize(((240.0 * self.scale).ceil() as i32, (34.0 * self.scale).ceil() as i32));
        self.stop_idle();
        self.liquid.bump_amp = 0.0;
        self.liquid.bump_vel = 0.0;
        let docked = self.docked();
        let win = self.win;
        self.place(win);
        let expanded = self.state == State::Expanded;
        let hidden = matches!(self.state, State::Hidden | State::Hiding);
        let _ = self.set_geometry_now(win, expanded, hidden);
        let small = window_size(self.scale, Win::Small, docked, self.cw, &window_room(&self.settings));
        let _ = self.collapsed.resize(small);
        self.resize_ring();
        let _ = self.expanded.resize(self.window_size(Win::Big));
        let _ = self.liquid.bump.resize(bump_size(self.scale, self.collapsed()));
        let _ = self.liquid.highlight.resize(self.window_size(self.win));
        self.liquid.pull_ready = false;
        let _ = self
            .pet
            .char_layer
            .resize(((characters::BOX_W * self.scale).round() as i32, (characters::BOX_H * self.scale).round() as i32));
        self.pet.char_key = None;
        let _ = self.liquid.bump.sprite.SetCenterPoint(Vector3 {
            X: self.liquid.bump.size().0 as f32 / 2.0,
            Y: 0.0,
            Z: 0.0,
        });
        let ear_px = (EAR * self.scale).round() as i32;
        for ear in &mut self.ears {
            let _ = ear.resize((ear_px, ear_px));
        }
        self.paint_ears();
        self.update_offsets();
        self.repaint();
        self.start_liquid();
    }
}
