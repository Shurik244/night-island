use crate::island::*;

pub(in crate::island) const CHAR_INSET: f32 = 30.0;

#[derive(Clone, Copy)]
pub(in crate::island) struct PetReturn {
    pub start: Instant,
    pub from: (f32, f32),
}

pub(in crate::island) struct Pet {
    pub returning: Option<PetReturn>,
    pub(in crate::island) behavior: crate::ui::pet_behavior::PetBehavior,
    pub(in crate::island) speech: crate::ui::pet_speech::PetSpeech,
    pub(in crate::island) speech_cue: crate::ui::pet_speech::SpeechCue,
    pub(in crate::island) voice_phase: crate::ui::pet_speech::VoicePhase,
    pub(in crate::island) char_layer: Layer,
    pub(in crate::island) motion: crate::ui::pet_motion::PetMotion,
    pub(in crate::island) controls_at: Instant,
    pub(in crate::island) cursor: Option<(i32, i32)>,
    pub(in crate::island) desktop: Option<crate::ui::companion::CompanionWindow>,
    pub(in crate::island) born: Instant,
    pub(in crate::island) beat: f32,
    pub(in crate::island) beat_at: Instant,
    pub(in crate::island) chat: Option<super::companion_chat::CompanionChat>,
    pub(in crate::island) next_blink: Instant,
    pub(in crate::island) blink_until: Instant,
    pub(in crate::island) rng: u32,
    pub(in crate::island) reaction_until: Option<(Reaction, Instant)>,
    pub(in crate::island) frame: u64,
    pub(in crate::island) char_key: Option<(Reaction, bool, [u8; 3], u64)>,
}

impl App {
    pub(in crate::island) fn drag_character(&mut self) {
        let Some((x, y)) = self.pointer.character_press else { return };
        let mut point = windows::Win32::Foundation::POINT::default();
        if unsafe { GetCursorPos(&mut point) }.is_err()
            || (point.x - x).abs().max((point.y - y).abs()) < (8.0 * self.scale).round() as i32
        {
            return;
        }
        self.pointer.character_press = None;
        self.pointer.press = None;
        self.kill_timer(Timer::Expand);
        let island = self.pill_screen(false);
        self.set_character_detached(true);
        if let Some(desktop) = &mut self.pet.desktop {
            desktop.begin_drag(point, island);
        } else {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
        }
        self.char_tick();
    }

    fn companion_over_island(&self) -> bool {
        if self.gaming
            || (self.pill_hidden && !self.ghosted)
            || !matches!(self.state, State::Collapsed | State::Expanded)
        {
            return false;
        }
        let Some(desktop) = &self.pet.desktop else { return false };
        if !desktop.can_dock() {
            return false;
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        unsafe {
            if !IsWindowVisible(self.hwnd).as_bool() || GetWindowRect(self.hwnd, &mut rect).is_err() {
                return false;
            }
        }
        let pill = if self.state == State::Expanded { self.pill_full() } else { self.pill_rect(false) };
        let (cx, cy) = desktop.center();
        crate::ui::companion::dock_hit(
            (rect.left as f32 + pill.0, rect.top as f32 + pill.1, pill.2, pill.3),
            (cx, cy),
            self.scale,
        )
    }

    pub(in crate::island) fn attached_character_on(&self) -> bool {
        self.settings.character != Character::None && self.pet.desktop.is_none()
    }

    pub(in crate::island) fn character_interacting(&self) -> bool {
        self.pet.returning.is_some()
            || self.pointer.character_press.is_some()
            || self.pet.desktop.as_ref().is_some_and(|desktop| desktop.grabbed())
    }

    pub(in crate::island) fn char_size(&self) -> (f32, f32) {
        (characters::BOX_W * self.scale, characters::BOX_H * self.scale)
    }

    fn char_origin(&self) -> (f32, f32) {
        let s = self.scale;
        let (x, y, w, h) = self.pill_rect(false);
        if self.settings.vertical() {
            (x + (w - characters::BOX_W * s) / 2.0, y + 6.0 * s)
        } else {
            (x + 6.0 * s, y + (h - characters::BOX_H * s) / 2.0)
        }
    }

    pub(in crate::island) fn place_char(&self) {
        let (x, y) = self.char_origin();
        let _ = self.pet.char_layer.sprite.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 });
    }

    pub(in crate::island) fn apply_character(&mut self) {
        self.pet.returning = None;
        let _ = self._visual.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
        if let Some(desktop) = &mut self.pet.desktop {
            desktop.cancel_return();
        }
        self.pet.behavior.suspend();
        if self.settings.character == Character::None {
            self.settings.character_detached = false;
            self.ui.cfg.character_detached = false;
        }
        let detached = self.settings.character_detached && self.settings.character != Character::None;
        if detached && self.pet.desktop.is_none() {
            match crate::ui::companion::CompanionWindow::new(
                self.hwnd,
                self.settings.character_pos,
                self.scale,
                &self.gpu,
            ) {
                Ok(window) => {
                    self.settings.character_pos = Some(window.position());
                    self.pet.desktop = Some(window);
                }
                Err(error) => {
                    self.settings.character_detached = false;
                    self.ui.cfg.character_detached = false;
                    crate::log_warn!("could not release character: {error}");
                }
            }
        } else if !detached && let Some(window) = self.pet.desktop.take() {
            self.settings.character_pos = Some(window.position());
            drop(window);
            self.restore_pill();
            self.reveal();
        }
        let on = self.attached_character_on();
        self.ui.char_inset = if on { CHAR_INSET } else { 0.0 };
        let _ = self.pet.char_layer.sprite.SetIsVisible(on && self.state != State::Expanded);
        if let Some(desktop) = &self.pet.desktop {
            desktop.set_capture_hidden(self.settings.capture_hidden);
        }
        self.update_eq();
        self.pet.char_key = None;
        self.place_char();
        self.place_spectrum();
        self.repaint_collapsed();
        if on || self.pet.desktop.is_some() {
            self.set_timer(Timer::Char, 1);
        } else {
            self.kill_timer(Timer::Char);
        }
        self.start_liquid();
    }

    pub(in crate::island) fn start_char(&self) {
        if !self.gaming
            && self.settings.character != Character::None
            && (self.state == State::Collapsed || self.pet.desktop.is_some())
        {
            self.set_timer(Timer::Char, 60);
        }
    }

    pub(in crate::island) fn can_release_character(&self, x: i32, y: i32) -> bool {
        if self.state != State::Collapsed
            || self.pill_motion.intro.is_some()
            || self.settings.character == Character::None
            || self.pet.desktop.is_some()
        {
            return false;
        }
        let (left, top) = self.char_origin();
        let (w, h) = self.char_size();
        let dx = (x as f32 - left - w / 2.0) / (w * 0.46);
        let dy = (y as f32 - top - h / 2.0) / (h * 0.5);
        dx * dx + dy * dy <= 1.0
    }

    pub(in crate::island) fn set_character_detached(&mut self, detached: bool) {
        self.settings.character_detached = detached;
        if detached && self.settings.character == Character::None {
            self.settings.character = Character::Spark;
        } else if !detached {
            self.restore_pill();
            self.reveal();
        }
        self.apply_character();
        self.ui.cfg = self.settings.clone();
        self.settings.save();
    }

    pub(in crate::island) fn begin_pet_return(&mut self) {
        if self.pet.returning.is_some() {
            return;
        }
        let Some(desktop) = &self.pet.desktop else { return };
        if !self.settings.pet_return_animation
            || !self.settings.island_animations
            || self.settings.pet_performance == crate::ui::pet_motion::PetPerformance::Still
            || !crate::ui::feedback::motion_allowed()
            || self.pill_hidden
            || self.gaming
        {
            self.set_character_detached(false);
            return;
        }
        self.pet.returning = Some(PetReturn { start: Instant::now(), from: desktop.center() });
        self.pet.behavior.suspend();
        self.cancel_pill_motion();
        self.reset_liquid_pull();
        self.kill_timer(Timer::Collapse);
        self.kill_timer(Timer::Expand);
        self.kill_timer(Timer::Autohide);
        self.set_timer(Timer::Char, 1);
    }

    fn pet_return_step(&mut self) {
        let Some(returning) = self.pet.returning else { return };
        let p = (returning.start.elapsed().as_secs_f32() / 0.52).clamp(0.0, 1.0);
        let _ = self._visual.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
        if self.gaming || self.pill_hidden || !matches!(self.state, State::Collapsed | State::Expanded) {
            self.pet.returning = None;
            if let Some(desktop) = &mut self.pet.desktop {
                desktop.cancel_return();
            }
            self.reset_liquid_pull();
            return;
        }
        if p >= 1.0 || self.pet.desktop.is_none() {
            self.pet.returning = None;
            self.reset_liquid_pull();
            self.set_character_detached(false);
            let sprite = &self.pet.char_layer.sprite;
            let _ = sprite.SetCenterPoint(Vector3 { X: self.char_size().0 / 2.0, Y: self.char_size().1 / 2.0, Z: 0.0 });
            let _ = sprite.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
            let _ = self.animate_pill_vector(
                sprite,
                h!("Scale"),
                Vector3 { X: 0.65, Y: 0.8, Z: 1.0 },
                Vector3 { X: 1.0, Y: 1.0, Z: 1.0 },
                220,
            );
            self.react(Reaction::Done, 800);
            return;
        }
        let origin = self.window_origin(self.win);
        let character = self.char_origin();
        let size = self.char_size();
        let target = if self.state == State::Expanded {
            let r = self.pill_full();
            (origin.0 + r.0 + r.2 / 2.0, origin.1 + r.1 + 24.0 * self.scale)
        } else {
            (origin.0 + character.0 + size.0 / 2.0, origin.1 + character.1 + size.1 / 2.0)
        };
        let ease = p * p * (3.0 - 2.0 * p);
        let center = (
            returning.from.0 + (target.0 - returning.from.0) * ease,
            returning.from.1 + (target.1 - returning.from.1) * ease,
        );
        if let Some(desktop) = &mut self.pet.desktop {
            desktop.return_to(center, ease);
        }
        if self.state == State::Expanded || (self.state == State::Collapsed && !self.liquid_on()) {
            let r = self.pill_rect(self.state == State::Expanded);
            let pulse = (std::f32::consts::PI * p).sin() * if self.state == State::Expanded { 0.3 } else { 1.0 };
            let _ = self._visual.SetCenterPoint(Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 });
            let _ = self._visual.SetScale(Vector3 { X: 1.0 + 0.018 * pulse, Y: 1.0 + 0.045 * pulse, Z: 1.0 });
        }
        if self.state == State::Collapsed && self.liquid_on() {
            let (at, size, _) = self.pill(self.win, false, false);
            let vertical = self.settings.vertical();
            let normal = if vertical {
                (if self.edge() == Edge::Left { 1.0 } else { -1.0 }, 0.0)
            } else {
                (0.0, if self.docked() && self.edge() == Edge::Bottom { -1.0 } else { 1.0 })
            };
            let point = if vertical {
                (if normal.0 > 0.0 { at.X + size.X } else { at.X }, (center.1 - origin.1).clamp(at.Y, at.Y + size.Y))
            } else {
                ((center.0 - origin.0).clamp(at.X, at.X + size.X), if normal.1 > 0.0 { at.Y + size.Y } else { at.Y })
            };
            let depth = (std::f32::consts::PI * p).sin() * 14.0 * self.scale;
            self.liquid.pull = crate::ui::gfx::ElasticPull { point, normal, spread: 30.0 * self.scale, depth };
            self.liquid.pull_ready = true;
            self.liquid.bump_amp = depth;
            self.paint_liquid_body(depth);
        }
    }

    pub(in crate::island) fn on_companion(&mut self, id: usize, source: isize) {
        if self.pet.desktop.as_ref().is_none_or(|window| window.hwnd().0 as isize != source) {
            return;
        }
        match crate::ui::companion::Action::from_id(id) {
            Some(crate::ui::companion::Action::Voice) => self.pet_voice_input(),
            Some(crate::ui::companion::Action::Chat) => {
                self.react(Reaction::Done, 700);
                self.open_companion_chat();
            }
            Some(crate::ui::companion::Action::ToggleIsland) => self.toggle_island_from_character(),
            Some(action @ (crate::ui::companion::Action::Moved | crate::ui::companion::Action::Dropped)) => {
                self.pet.behavior.suspend();
                if action == crate::ui::companion::Action::Dropped && self.companion_over_island() {
                    self.begin_pet_return();
                    self.react(Reaction::Done, 650);
                    return;
                }
                if let Some(window) = &self.pet.desktop {
                    window.keep_on_screen();
                    self.settings.character_pos = Some(window.position());
                    self.ui.cfg = self.settings.clone();
                    self.settings.save();
                }
                if action == crate::ui::companion::Action::Dropped {
                    self.react(Reaction::Done, 650);
                }
                self.schedule_autohide();
                if action == crate::ui::companion::Action::Dropped && self.state == State::Expanded && !self.ui.pinned {
                    self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(1));
                }
            }
            Some(crate::ui::companion::Action::Dock) => self.begin_pet_return(),
            Some(crate::ui::companion::Action::Previous) => {
                media::control(media::Control::Previous);
                self.react(Reaction::Music, 600);
            }
            Some(crate::ui::companion::Action::PlayPause) => {
                media::control(media::Control::PlayPause);
                self.react(Reaction::Done, 500);
            }
            Some(crate::ui::companion::Action::Next) => {
                media::control(media::Control::Next);
                self.react(Reaction::Music, 600);
            }
            None => {}
        }
    }

    pub(in crate::island) fn react(&mut self, reaction: Reaction, ms: u64) {
        self.pet.reaction_until = Some((reaction, Instant::now() + Duration::from_millis(ms)));
        self.start_char();
    }

    pub(in crate::island) fn next_random(&mut self) -> u32 {
        let mut x = self.pet.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.pet.rng = x;
        x
    }

    pub(in crate::island) fn idle_ms() -> u32 {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        unsafe { if GetLastInputInfo(&mut info).as_bool() { GetTickCount().wrapping_sub(info.dwTime) } else { 0 } }
    }

    pub(in crate::island) fn char_tick(&mut self) {
        self.pet_return_step();
        let character = self.settings.character;
        if self.gaming {
            self.pet.behavior.suspend();
            self.pet.char_key = None;
            if let Some(desktop) = &mut self.pet.desktop {
                desktop.hide();
            }
            return;
        }
        let detached = self.pet.desktop.is_some();
        if self.pet.desktop.as_ref().is_some_and(|desktop| desktop.dragging()) {
            self.kill_timer(Timer::Expand);
            self.kill_timer(Timer::Collapse);
            self.kill_timer(Timer::Autohide);
        }
        if character == Character::None || (!detached && (self.state != State::Collapsed || self.pill_hidden)) {
            return;
        }
        let now = Instant::now();
        if now >= self.pet.next_blink {
            self.pet.blink_until = now + Duration::from_millis(180);
            self.pet.next_blink = now + Duration::from_millis(2500 + u64::from(self.next_random() % 3500));
        }
        let lid = crate::ui::pet_motion::eyelid(self.pet.blink_until.saturating_duration_since(now).as_secs_f32());
        let blink = lid > 0.92;

        let mut reaction = if self.ui.remind.alert.is_some() {
            Reaction::Attention
        } else if let Some((r, _)) = self.pet.reaction_until.filter(|(_, u)| now < *u) {
            r
        } else if self.ui.media.playing {
            Reaction::Music
        } else if Self::idle_ms() > 90_000 {
            Reaction::Sleep
        } else {
            Reaction::Idle
        };

        let mut pt = windows::Win32::Foundation::POINT::default();
        let mut rc = windows::Win32::Foundation::RECT::default();
        let (cw, ch) = self.char_size();
        let cursor = unsafe { GetCursorPos(&mut pt) }.ok().map(|_| (pt.x, pt.y));
        let look = unsafe {
            if cursor.is_some() && GetWindowRect(self.hwnd, &mut rc).is_ok() {
                let (x, y) = self.char_origin();
                let (cx, cy) = self
                    .pet
                    .desktop
                    .as_ref()
                    .map_or((rc.left as f32 + x + cw / 2.0, rc.top as f32 + y + ch / 2.0), |desktop| desktop.center());
                ((pt.x as f32 - cx) / 260.0, (pt.y as f32 - cy) / 200.0)
            } else {
                (0.0, 0.0)
            }
        };

        let t = now.duration_since(self.pet.born).as_secs_f32();
        let dt = now.duration_since(self.pet.beat_at).as_secs_f32().min(0.25);
        let near = cursor.is_some_and(|point| {
            self.pet
                .desktop
                .as_ref()
                .map_or(look.0.abs() < 0.36 && look.1.abs() < 0.46, |desktop| desktop.pointer_near(point))
        });
        let held = self.character_interacting();
        let still = self.settings.pet_performance == crate::ui::pet_motion::PetPerformance::Still
            || !crate::ui::feedback::motion_allowed();
        let chat_open =
            self.pet.chat.as_ref().is_some_and(|chat| unsafe { IsWindowVisible(chat.window.hwnd()).as_bool() })
                || (self.state == State::Expanded && self.ui.tab == view::Tab::Chat && !self.pill_hidden);
        let chat_pending =
            self.pet.chat.as_ref().is_some_and(|chat| chat.pending) || self.ui.chat.pet.pending || self.ui.chat.pending;
        use crate::ui::pet_speech::SpeechCue;
        let voice_cue = if self.settings.pet_voice_reactions {
            let phase = self.pet.voice_phase.cue();
            crate::platform::speech::level().map_or_else(
                || {
                    if phase == SpeechCue::Quiet && crate::platform::speech::busy() {
                        SpeechCue::Thinking
                    } else {
                        phase
                    }
                },
                |amplitude| SpeechCue::Speaking { amplitude },
            )
        } else {
            SpeechCue::Quiet
        };
        let speech_cue = if voice_cue.engaged() {
            voice_cue
        } else if self.pet.speech_cue.engaged() {
            self.pet.speech_cue
        } else if chat_pending {
            SpeechCue::Thinking
        } else {
            SpeechCue::Quiet
        };
        let urgent = self.ui.remind.alert.is_some() || self.pet.reaction_until.is_some_and(|(_, until)| now < until);
        let pet_card = self.pet_notification_visible();
        let behavior = self.pet.desktop.as_ref().map(|desktop| {
            self.pet.behavior.advance(&crate::ui::pet_behavior::Environment {
                dt,
                enabled: self.settings.pet_autonomy && self.pet.returning.is_none(),
                reduced_motion: still,
                held,
                engaged: near || desktop.controls_visible() || chat_open || speech_cue.engaged() || urgent || pet_card,
                position: desktop.position().map(|v| v as f32),
                bounds: desktop.walking_bounds(),
                scale: self.scale,
            })
        });
        if let Some(frame) = &behavior {
            if frame.event.is_some() {
                self.pet.char_key = None;
            }
            if let Some(position) = frame.position
                && let Some(desktop) = &self.pet.desktop
            {
                desktop.walk_to(position);
            }
            if frame.activity == crate::ui::pet_behavior::Activity::Sleeping {
                reaction = Reaction::Sleep;
            } else if self.settings.pet_autonomy && reaction == Reaction::Sleep {
                reaction = Reaction::Idle;
            }
        }
        let curious = self.settings.pet_music_curiosity
            && !detached
            && self.pill_motion.intro == Some(super::pill_motion::IntroPhase::Present)
            && !speech_cue.engaged();
        let interest_target = curious.then(|| {
            let progress = self
                .pill_motion
                .intro_started
                .map_or(1.0, |start| start.elapsed().as_secs_f32() / (super::pill_motion::REVEAL_MS as f32 / 1000.0));
            let cover = view::music_entrance_cover_at(self.pill_rect(false), self.scale, &self.ui, progress);
            let origin = self.char_origin();
            (
                ((cover.0 + cover.2) / 2.0 - origin.0 - cw / 2.0) / (70.0 * self.scale),
                ((cover.1 + cover.3) / 2.0 - origin.1 - ch / 2.0) / (70.0 * self.scale),
            )
        });
        let gaze = if held {
            cursor.zip(self.pet.cursor).map_or((0.0, 0.0), |((x, y), (px, py))| {
                (
                    ((x - px) as f32 / (dt.max(0.001) * 900.0)).clamp(-1.0, 1.0),
                    ((y - py) as f32 / (dt.max(0.001) * 900.0)).clamp(-1.0, 1.0),
                )
            })
        } else if cursor.is_some() {
            crate::ui::pet_motion::pointer_gaze(look)
        } else {
            self.pet.motion.look
        };
        let (gaze, interest) = self.pet.motion.interest_gaze(gaze, interest_target, dt, still);
        let gaze_moving = (gaze.0 - self.pet.motion.look.0).abs() + (gaze.1 - self.pet.motion.look.1).abs() > 0.015;
        self.pet.cursor = cursor;
        let mut motion = self.pet.motion.advance(gaze, dt, near, held, still);
        motion.lid = if still { 0.0 } else { lid };
        motion.behavior = behavior.as_ref().map_or(Default::default(), |frame| frame.pose);
        motion.mouth = self.pet.speech.advance(speech_cue, dt, still);
        if speech_cue.engaged() {
            reaction = Reaction::Idle;
            motion.warmth = motion.warmth.max(0.45);
            if !still && !held {
                let target = match speech_cue {
                    SpeechCue::Listening => 0.1 + crate::platform::dictation::level() * 0.18,
                    SpeechCue::Thinking => (t * 1.8).sin() * 0.1,
                    _ => 0.0,
                };
                motion.lean += target;
                motion.stretch += target.abs() * 0.12;
            }
        }
        motion.warmth = motion.warmth.max(0.35 * interest);
        if !still {
            motion.lean += 0.08 * interest;
        }
        let look = self.pet.motion.look;
        let target = if detached || self.music.spectrum_running {
            (0..3).map(audio::level).sum::<f32>() / 3.0
        } else {
            0.5 + 0.5 * (t * 7.5).sin()
        };
        self.pet.beat_at = now;
        self.pet.beat += (target.clamp(0.0, 1.0) - self.pet.beat) * (1.0 - (-dt * 8.0).exp());
        let beat = self.pet.beat;
        let accent =
            self.ui.media.accent.map_or((0.65, 0.7, 1.0), |(r, g, b)| {
                (f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0)
            });
        let pose = characters::Pose { t, reaction, look, blink: blink && !still, accent, beat, motion };
        let settling =
            [motion.behavior.sit, motion.behavior.sleep].iter().any(|value| *value > 0.005 && *value < 0.995);
        let interval = self.settings.pet_performance.interval(
            near || held
                || self.pet.returning.is_some()
                || gaze_moving
                || pose.lively()
                || motion.warmth > 0.0
                || motion.behavior.walk > 0.0
                || settling
                || speech_cue.engaged(),
            reaction == Reaction::Sleep && !settling,
            detached,
        );

        let docking =
            self.pet.desktop.as_ref().is_some_and(|desktop| desktop.dragging()) && self.companion_over_island();
        if let Some(desktop) = &mut self.pet.desktop {
            if desktop.set_docking(docking) {
                self.pet.char_key = None;
            }
            let available = self.ui.media.playing || !self.ui.media.title.is_empty() || self.ui.media.players > 0;
            if self.pet.returning.is_none()
                && (near || held || desktop.controls_visible())
                && now.duration_since(self.pet.controls_at) >= Duration::from_millis(80)
            {
                self.pet.controls_at = now;
                if let Err(error) = desktop.update_controls(available, &self.ui.media, media::progress(), self.scale) {
                    crate::log_warn!("could not draw companion music controls: {error}");
                }
            }
        }

        if !still {
            self.pet.frame = self.pet.frame.wrapping_add(1);
        }
        let color = [accent.0, accent.1, accent.2].map(|v| (v * 255.0).round() as u8);
        let key = (reaction, pose.blink, color, if still { 0 } else { self.pet.frame });
        if self.pet.char_key != Some(key) {
            let drawn = if let Some(desktop) = &mut self.pet.desktop {
                desktop.render_designed(&self.gpu, character, &pose, self.scale, Some(&self.settings.custom_pet))
            } else {
                self.pet.char_layer.paint(&self.gpu, |p| {
                    let draw = if self.settings.pill_unified_layout {
                        characters::draw_attached
                    } else {
                        characters::draw_with_design
                    };
                    let _ = draw(p, character, cw, &pose, Some(&self.settings.custom_pet));
                })
            };
            match drawn {
                Ok(()) => self.pet.char_key = Some(key),
                Err(error) => crate::log_warn!("could not draw character: {error}"),
            }
        }
        self.set_timer(Timer::Char, interval);
        self.maybe_show_notification();
    }
}
