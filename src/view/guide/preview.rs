use super::*;
use crate::ui::characters::{Pose, Reaction, draw_with_design};

const MUSIC: Rect = (445.0, 242.0, 873.0, 462.0);
const PET_HOME: (f32, f32) = (558.0, 281.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewLesson {
    Music,
    Notification,
    Pet,
}

impl PreviewLesson {
    pub const ALL: [Self; 3] = [Self::Music, Self::Notification, Self::Pet];

    pub fn label(self) -> &'static str {
        match self {
            Self::Music => "Music and volume",
            Self::Notification => "Incoming notifications",
            Self::Pet => "A pet on your desktop",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Music => "Try pause, next track and the volume slider.",
            Self::Notification => "Close the card with the cross, then try again.",
            Self::Pet => "Drag your pet out. Bring it back to the island.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewAction {
    Media(u8),
    Seek(u16),
    Volume(u8),
    Mute,
    Close,
    Pet,
    Reset,
    Explain,
}

pub(super) struct IslandPreview {
    ui: Box<UiState>,
    pub lesson: PreviewLesson,
    track: usize,
    touched: Instant,
    changed: Option<Instant>,
    shown: bool,
    pet: (f32, f32),
    look: (f32, f32),
    drag: Option<PreviewAction>,
    unmuted: f32,
    hint: Option<&'static str>,
}

impl IslandPreview {
    pub fn new(settings: &Settings) -> Self {
        let mut ui = Box::new(UiState::new());
        ui.cfg.character = settings.character;
        ui.cfg.custom_pet = settings.custom_pet.clone();
        ui.cfg.home_actions.clear();
        ui.cfg.header_tabs.clear();
        ui.cfg.hidden_tabs = crate::data::settings::HIDEABLE_TABS.map(String::from).to_vec();
        ui.home.preview_timeline = Some((32.0, 180.0));
        ui.home.device = crate::ui::i18n::tr("Preview").into_owned();
        ui.media = Self::media(0);
        Self {
            ui,
            lesson: PreviewLesson::Music,
            track: 0,
            touched: Instant::now(),
            changed: None,
            shown: true,
            pet: PET_HOME,
            look: (0.0, 0.0),
            drag: None,
            unmuted: 0.5,
            hint: None,
        }
    }

    fn media(index: usize) -> crate::platform::media::Media {
        let (title, accent) =
            [("Night walk", (125, 151, 227)), ("Quiet morning", (105, 193, 170)), ("City lights", (202, 139, 186))]
                [index];
        crate::platform::media::Media {
            title: crate::ui::i18n::tr(title).into_owned(),
            artist: crate::ui::i18n::tr("Sample track · no audio").into_owned(),
            source: "Night Island".into(),
            playing: false,
            accent: Some(accent),
            ..Default::default()
        }
    }

    pub fn select(&mut self, lesson: PreviewLesson, animate: bool) {
        self.release();
        self.lesson = lesson;
        self.hint = None;
        self.changed = animate.then(Instant::now);
        self.touched = Instant::now();
        self.shown = true;
    }

    pub fn act(&mut self, action: PreviewAction) {
        self.hint = None;
        match action {
            PreviewAction::Media(1) => {
                self.ui.media.playing = !self.ui.media.playing;
                self.touched = Instant::now();
            }
            PreviewAction::Media(i) => {
                let playing = self.ui.media.playing;
                let old = self.ui.media.clone();
                self.track = (self.track + if i == 0 { 2 } else { 1 }) % 3;
                self.ui.media = Self::media(self.track);
                self.ui.media.playing = playing;
                self.ui.home.preview_timeline = Some((0.0, 180.0));
                self.ui.home.track_transition.retarget(&old, crate::ui::feedback::motion_allowed());
                self.touched = Instant::now();
            }
            PreviewAction::Seek(value) => {
                self.ui.home.preview_timeline = Some((f32::from(value.min(1000)) * 0.18, 180.0));
                self.touched = Instant::now();
                self.drag = Some(action);
            }
            PreviewAction::Volume(value) => {
                self.ui.home.vol = f32::from(value.min(100)) / 100.0;
                self.drag = Some(action);
            }
            PreviewAction::Mute => {
                if self.ui.home.vol > 0.0 {
                    self.unmuted = self.ui.home.vol;
                    self.ui.home.vol = 0.0;
                } else {
                    self.ui.home.vol = self.unmuted.max(0.01);
                }
            }
            PreviewAction::Close => self.shown = false,
            PreviewAction::Pet => self.drag = Some(action),
            PreviewAction::Reset => {
                self.release();
                self.shown = true;
                self.pet = PET_HOME;
                self.look = (0.0, 0.0);
                self.changed = crate::ui::feedback::motion_allowed().then(Instant::now);
                self.ui.home.vol = 0.5;
                self.ui.home.preview_timeline = Some((32.0, 180.0));
                self.ui.media.playing = false;
            }
            PreviewAction::Explain => self.hint = Some("More tools are available after setup."),
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn release(&mut self) -> bool {
        let Some(action) = self.drag.take() else { return false };
        if action == PreviewAction::Pet && inside((540.0, 265.0, 788.0, 348.0), self.pet.0 + 28.0, self.pet.1 + 28.0) {
            self.pet = PET_HOME;
            self.hint = Some("Back on the island.");
        }
        true
    }

    fn music_geometry(layout: Layout) -> (Rect, f32) {
        let r = layout.rect(MUSIC);
        ((r.0, r.1, r.2 - r.0, r.3 - r.1), (r.2 - r.0) / 560.0)
    }

    fn notification_rect(&self) -> Rect {
        let t = self.changed.map_or(1.0, |at| eased(at.elapsed().as_millis(), 0, 340.0));
        (449.0, 270.0 - 16.0 * (1.0 - t), 869.0, 428.0 - 16.0 * (1.0 - t))
    }

    pub fn hit(&self, layout: Layout, x: f32, y: f32) -> Option<PreviewAction> {
        match self.lesson {
            PreviewLesson::Music => {
                let (pill, scale) = Self::music_geometry(layout);
                if !inside(layout.rect(MUSIC), x, y) {
                    return None;
                }
                let body = body_rect(pill, scale, Tab::Home);
                Some(match hit_home(body, scale, x, y, &self.ui) {
                    Some(HomeHit::Media(i)) => PreviewAction::Media(i as u8),
                    Some(HomeHit::Seek(fraction)) => PreviewAction::Seek((fraction * 1000.0).round() as u16),
                    Some(HomeHit::Volume) => {
                        let (left, right, _) = volume_track(body, scale, true);
                        PreviewAction::Volume(((x - left) / (right - left) * 100.0).clamp(0.0, 100.0).round() as u8)
                    }
                    Some(HomeHit::VolumeIcon) => PreviewAction::Mute,
                    _ => PreviewAction::Explain,
                })
            }
            PreviewLesson::Notification if self.shown => {
                let r = layout.rect(self.notification_rect());
                inside(notification_close((r.0, r.1, r.2 - r.0, r.3 - r.1), layout.scale), x, y)
                    .then_some(PreviewAction::Close)
            }
            PreviewLesson::Pet => {
                inside(layout.rect((self.pet.0, self.pet.1, self.pet.0 + 56.0, self.pet.1 + 56.0)), x, y)
                    .then_some(PreviewAction::Pet)
            }
            _ => None,
        }
    }

    pub fn pointer(&mut self, layout: Layout, x: f32, y: f32, down: bool) -> bool {
        if !down && self.dragging() {
            return self.release();
        }
        match self.drag {
            Some(PreviewAction::Pet) => {
                self.pet = (
                    ((x - layout.x) / layout.scale - 28.0).clamp(449.0, 813.0),
                    ((y - layout.y) / layout.scale - 28.0).clamp(224.0, 455.0),
                );
                true
            }
            Some(action @ (PreviewAction::Volume(_) | PreviewAction::Seek(_))) => {
                let (pill, scale) = Self::music_geometry(layout);
                let body = body_rect(pill, scale, Tab::Home);
                let volume = matches!(action, PreviewAction::Volume(_));
                let (left, right, _) =
                    if volume { volume_track(body, scale, true) } else { seek_track(body, scale, true) };
                let fraction = ((x - left) / (right - left)).clamp(0.0, 1.0);
                self.act(if volume {
                    PreviewAction::Volume((fraction * 100.0).round() as u8)
                } else {
                    PreviewAction::Seek((fraction * 1000.0).round() as u16)
                });
                true
            }
            _ => {
                let hover = match self.hit(layout, x, y) {
                    Some(PreviewAction::Media(i)) => Some(500 + usize::from(i)),
                    Some(PreviewAction::Seek(_)) => Some(520),
                    Some(PreviewAction::Mute) => Some(530),
                    _ => None,
                };
                let look = (
                    ((x - layout.x) / layout.scale - self.pet.0 - 28.0) / 100.0,
                    ((y - layout.y) / layout.scale - self.pet.1 - 28.0) / 100.0,
                );
                let look =
                    ((look.0.clamp(-1.0, 1.0) * 10.0).round() / 10.0, (look.1.clamp(-1.0, 1.0) * 10.0).round() / 10.0);
                let changed =
                    self.ui.home.hover_act != hover || (self.lesson == PreviewLesson::Pet && self.look != look);
                self.ui.home.hover_act = hover;
                self.look = look;
                changed
            }
        }
    }

    pub fn ticking(&self) -> bool {
        (self.lesson == PreviewLesson::Music && self.ui.media.playing) || self.animating()
    }

    pub fn animating(&self) -> bool {
        self.changed.is_some_and(|at| at.elapsed() < Duration::from_millis(400))
            || (self.lesson == PreviewLesson::Music && self.ui.home.track_transition.active())
    }

    pub fn tick(&mut self) {
        if self.lesson == PreviewLesson::Music && self.ui.media.playing {
            let now = Instant::now();
            if let Some((position, length)) = self.ui.home.preview_timeline.as_mut() {
                *position = (*position + now.duration_since(self.touched).as_secs_f32().min(0.5)) % *length;
            }
            self.touched = now;
        }
    }

    pub fn paint_music(&self, v: &Preview<'_>) -> Result<()> {
        let (pill, scale) = Self::music_geometry(v.layout);
        v.island(MUSIC.0, MUSIC.1, MUSIC.2 - MUSIC.0, MUSIC.3 - MUSIC.1)?;
        paint_expanded(v.p, pill, scale, &self.ui, &[])
    }

    pub fn paint(&self, v: &Preview<'_>) -> Result<()> {
        match self.lesson {
            PreviewLesson::Music => self.paint_music(v)?,
            PreviewLesson::Notification if self.shown => {
                let r = self.notification_rect();
                v.island(r.0, r.1, r.2 - r.0, r.3 - r.1)?;
                let r = v.layout.rect(r);
                paint_notification_card(
                    v.p,
                    (r.0, r.1, r.2 - r.0, r.3 - r.1),
                    v.layout.scale,
                    &sample_notification(),
                    0,
                    false,
                    None,
                )?;
            }
            PreviewLesson::Notification => {
                v.center("Notification dismissed", (449.0, 300.0, 869.0, 398.0), 16.0, WHITE)?
            }
            PreviewLesson::Pet => {
                v.island(540.0, 280.0, 248.0, 58.0)?;
                v.center("21:08", (631.0, 280.0, 770.0, 338.0), 20.0, WHITE)?;
                let r = v.layout.rect((self.pet.0, self.pet.1, self.pet.0 + 56.0, self.pet.1 + 56.0));
                let saved = v.p.push_offset(r.0, r.1);
                let character =
                    if self.ui.cfg.character == Character::None { Character::Spark } else { self.ui.cfg.character };
                let drawn = draw_with_design(
                    v.p,
                    character,
                    56.0 * v.layout.scale,
                    &Pose {
                        motion: Default::default(),
                        t: 0.0,
                        reaction: if self.dragging() { Reaction::Attention } else { Reaction::Idle },
                        look: self.look,
                        blink: false,
                        accent: crate::ui::theme::accent(),
                        beat: 0.0,
                    },
                    Some(&self.ui.cfg.custom_pet),
                );
                v.p.set_transform(&saved);
                drawn?;
            }
        }
        v.p.text_wrapped(
            self.hint.unwrap_or(self.lesson.hint()),
            v.layout.rect((453.0, 504.0, 865.0, 548.0)),
            12.5 * v.layout.scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 1.0, 1.0, 0.76),
        )
    }
}

fn sample_notification() -> crate::platform::notifications::Record {
    crate::platform::notifications::Record {
        id: 0,
        created: 0,
        app_id: String::new(),
        app: "Night Island".into(),
        title: crate::ui::i18n::tr("Example notification").into_owned(),
        body: crate::ui::i18n::tr("New messages can appear here while you work.").into_owned(),
        icon: None,
        avatar: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_real_control_geometry_without_changing_settings() {
        let settings = Settings::default();
        let before = serde_json::to_vec(&settings).unwrap();
        let mut preview = IslandPreview::new(&settings);
        for (size, dpi) in [((760.0, 550.0), 1.0), ((940.0, 680.0), 1.0), ((1880.0, 1360.0), 2.0)] {
            let layout = Layout::new(size, dpi);
            let (pill, scale) = IslandPreview::music_geometry(layout);
            let body = body_rect(pill, scale, Tab::Home);
            let button = crate::view::home::media_btn(body, scale, 1, true);
            assert_eq!(
                preview.hit(layout, (button.0 + button.2) / 2.0, (button.1 + button.3) / 2.0),
                Some(PreviewAction::Media(1))
            );
            let (left, right, y) = volume_track(body, scale, true);
            assert_eq!(preview.hit(layout, (left + right) / 2.0, y), Some(PreviewAction::Volume(50)));
            preview.act(PreviewAction::Volume(50));
            preview.pointer(layout, right + 400.0, y, true);
            assert_eq!(preview.ui.home.vol, 1.0);
            assert!(preview.release());
        }
        preview.act(PreviewAction::Media(1));
        preview.act(PreviewAction::Media(2));
        assert!(preview.ui.media.playing);
        assert_eq!(preview.track, 1);
        preview.act(PreviewAction::Seek(500));
        preview.release();
        assert_eq!(preview.ui.home.preview_timeline, Some((90.0, 180.0)));
        preview.select(PreviewLesson::Notification, false);
        let position = preview.ui.home.preview_timeline;
        preview.tick();
        assert_eq!(preview.ui.home.preview_timeline, position);
        assert!(!preview.ticking());
        preview.act(PreviewAction::Close);
        assert!(!preview.shown);
        preview.act(PreviewAction::Reset);
        assert!(preview.shown);
        assert_eq!(serde_json::to_vec(&settings).unwrap(), before);
    }

    #[test]
    fn dragging_a_pet_stays_in_preview_and_can_return_home() {
        let mut preview = IslandPreview::new(&Settings::default());
        preview.select(PreviewLesson::Pet, false);
        let layout = Layout::new((940.0, 680.0), 1.0);
        preview.act(PreviewAction::Pet);
        preview.pointer(layout, -500.0, 3000.0, true);
        assert_eq!(preview.pet, (449.0, 455.0));
        preview.release();
        preview.act(PreviewAction::Pet);
        preview.pointer(layout, 600.0, 310.0, true);
        preview.release();
        assert_eq!(preview.pet, PET_HOME);
        assert!(!preview.dragging());
    }
}
