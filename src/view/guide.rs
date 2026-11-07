use std::time::{Duration, Instant};

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

use crate::data::settings::{AiSettings, Settings};
use crate::platform::{localvoice, wake, wake_install};
use crate::services::assistant::{Level, Permissions, perms};
use crate::ui::characters::Character;
use crate::view::*;

const SCENE: Rect = (426.0, 151.0, 892.0, 556.0);
mod preview;
use preview::IslandPreview;
pub use preview::{PreviewAction, PreviewLesson};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GuideMode {
    #[default]
    Quick,
    Detailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Welcome,
    Preset,
    Essentials,
    Practice,
    Voice,
    Ready,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Preset => "Preset",
            Self::Essentials => "Essentials",
            Self::Practice => "Try it",
            Self::Voice => "Voice",
            Self::Ready => "Ready",
        }
    }
}

impl GuideMode {
    fn pages(self) -> &'static [Page] {
        match self {
            Self::Quick => &[Page::Welcome, Page::Preset, Page::Essentials, Page::Ready],
            Self::Detailed => {
                &[Page::Welcome, Page::Preset, Page::Essentials, Page::Practice, Page::Voice, Page::Ready]
            }
        }
    }
}
const QUICK_ROWS: [QuickSetting; 8] = [
    QuickSetting::Dock,
    QuickSetting::Top,
    QuickSetting::Weather,
    QuickSetting::Notifications,
    QuickSetting::Pet,
    QuickSetting::Equalizer,
    QuickSetting::Lyrics,
    QuickSetting::Game,
];
const ACCESS_ROWS: [Access; 4] = [Access::Keep, Access::Basic, Access::Confirm, Access::Full];
const PRESET_ROWS: [Preset; 4] = [Preset::Keep, Preset::Island, Preset::Dock, Preset::Desktop];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    Keep,
    Island,
    Dock,
    Desktop,
}

impl Preset {
    fn label(self) -> &'static str {
        match self {
            Self::Keep => "Keep my layout",
            Self::Island => "Island only",
            Self::Dock => "Island and dock",
            Self::Desktop => "Full workspace",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Keep => "Use the panels you already have.",
            Self::Island => "A compact island and the Windows taskbar.",
            Self::Dock => "Add an auto-hiding app dock.",
            Self::Desktop => "Add the dock and system controls at the top.",
        }
    }

    pub fn apply(self, settings: &mut Settings) {
        if self == Self::Keep {
            return;
        }
        let dock = matches!(self, Self::Dock | Self::Desktop);
        settings.desktop_dock.enabled = dock;
        settings.top_panel.enabled = self == Self::Desktop;
        if dock {
            settings.desktop_dock.auto_hide = true;
        }
        if self == Self::Desktop {
            settings.top_panel.auto_hide = true;
            settings.top_panel.reserve_space = false;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickSetting {
    Dock,
    Top,
    Weather,
    Notifications,
    Pet,
    Equalizer,
    Lyrics,
    Game,
}

impl QuickSetting {
    fn setting(self) -> Option<Sid> {
        match self {
            Self::Dock => Some(Sid::DockEnabled),
            Self::Top => Some(Sid::TopEnabled),
            Self::Weather => Some(Sid::Weather),
            Self::Notifications => Some(Sid::SystemNotifications),
            Self::Pet => None,
            Self::Equalizer => Some(Sid::MusicEq),
            Self::Lyrics => Some(Sid::MusicLyrics),
            Self::Game => Some(Sid::GameMode),
        }
    }

    fn read(self, settings: &Settings) -> bool {
        self.setting().map_or(settings.character != Character::None, |id| toggle_get(id, settings))
    }

    fn write(self, settings: &mut Settings, enabled: bool) {
        if let Some(id) = self.setting() {
            toggle_set(id, settings, enabled);
        } else if !enabled {
            settings.character = Character::None;
        } else if settings.character == Character::None {
            settings.character = Character::Spark;
        }
    }

    fn visual(self) -> (char, &'static str) {
        match self {
            Self::Dock => ('\u{E7F4}', "Desktop dock"),
            Self::Top => ('\u{E8A9}', "Top panel"),
            Self::Weather => ('\u{F800}', "Weather"),
            Self::Notifications => ('\u{EA8F}', "Windows notifications"),
            Self::Pet => ('\u{E7C9}', "Pet"),
            Self::Equalizer => ('\u{E9D9}', "Equalizer"),
            Self::Lyrics => ('\u{E8D2}', "Track lyrics"),
            Self::Game => ('\u{E7FC}', "Game mode"),
        }
    }

    fn rect(self) -> Rect {
        let y = 220.0 + self as u8 as f32 * 42.0;
        (48.0, y, 414.0, y + 39.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Keep,
    Basic,
    Confirm,
    Full,
}

impl Access {
    fn label(self) -> &'static str {
        match self {
            Self::Keep => "Keep current permissions",
            Self::Basic => "Basic access",
            Self::Confirm => "Ask before actions",
            Self::Full => "Full access",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::Keep => "Keep the access already configured for each tool.",
            Self::Basic => "Music, reminders and apps. Files and terminal stay locked.",
            Self::Confirm => "Allow all tools and confirm actions before they run.",
            Self::Full => "Allow files, terminal, browser and screen without routine prompts.",
        }
    }

    fn rect(self) -> Rect {
        let y = 249.0 + self as u8 as f32 * 59.0;
        (460.0, y, 874.0, y + 54.0)
    }

    fn apply(self, perms: &mut Permissions) {
        if self == Self::Keep {
            return;
        }
        for cap in perms::ALL {
            perms.set(
                cap,
                match self {
                    Self::Basic => cap.default_level(),
                    Self::Confirm => Level::Ask,
                    Self::Full => Level::Auto,
                    Self::Keep => unreachable!(),
                },
            );
        }
    }
}

const ASSISTANT_ROW: Rect = (460.0, 201.0, 874.0, 242.0);
const SPEECH_ESSENTIALS: Rect = (460.0, 489.0, 874.0, 525.0);
const SPEECH_VOICE: Rect = (453.0, 448.0, 865.0, 487.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceModel {
    Wake,
    Local,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ModelStatus {
    #[default]
    Missing,
    Downloading(u8),
    Installed,
    Failed(&'static str),
}

impl VoiceModel {
    fn index(self) -> usize {
        usize::from(self == Self::Local)
    }

    fn status(self) -> ModelStatus {
        match self {
            Self::Wake => match wake_install::state() {
                wake_install::State::Idle => ModelStatus::Missing,
                wake_install::State::Working(step) => ModelStatus::Downloading(step),
                wake_install::State::Ready => ModelStatus::Installed,
                wake_install::State::Failed(why) => ModelStatus::Failed(why),
            },
            Self::Local => match localvoice::state() {
                localvoice::State::Idle => ModelStatus::Missing,
                localvoice::State::Working(step) => ModelStatus::Downloading(step),
                localvoice::State::Ready => ModelStatus::Installed,
                localvoice::State::Failed(why) => ModelStatus::Failed(why),
            },
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Wake => "Wake-word model",
            Self::Local => "Local voice (Piper)",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::Wake => "Local detection · about 46 MB",
            Self::Local => "Offline speech · up to 44 MB",
        }
    }

    fn row(self) -> Rect {
        let y = 349.0 + self.index() as f32 * 75.0;
        (48.0, y, 404.0, y + 65.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideAction {
    Next,
    Back,
    Skip,
    Resume,
    Mode(GuideMode),
    ChangeRoute,
    Extend,
    Lesson(PreviewLesson),
    Preview(PreviewAction),
    Select(Preset),
    ToggleQuick(QuickSetting),
    ToggleAssistant,
    ToggleSpeechService,
    SelectAccess(Access),
    SuggestWake,
    Download(VoiceModel),
    ToggleModel(VoiceModel),
    OpenAssistant,
    OpenRecognition,
}

pub struct GuideUi {
    page: usize,
    mode: GuideMode,
    preview: IslandPreview,
    pub visible: bool,
    pub preset: Preset,
    quick_before: [bool; QUICK_ROWS.len()],
    quick: [Option<bool>; QUICK_ROWS.len()],
    assistant_before: bool,
    assistant: Option<bool>,
    access: Access,
    before_full: Option<(Access, Option<bool>)>,
    demo_started: Option<Instant>,
    pub wake: TextEdit,
    wake_before_text: String,
    pub invalid_wake: bool,
    pub connect_speech: bool,
    pub connect_model: bool,
    speech_separate_before: bool,
    speech_separate: Option<bool>,
    wake_enabled: Option<bool>,
    local_enabled: Option<bool>,
    wake_before: bool,
    local_before: bool,
    models: [ModelStatus; 2],
}

impl GuideUi {
    pub fn new(settings: &Settings, animate: bool) -> Self {
        let mut guide = Self {
            page: 0,
            mode: GuideMode::Quick,
            preview: IslandPreview::new(settings),
            visible: true,
            preset: Preset::Keep,
            quick_before: QUICK_ROWS.map(|feature| feature.read(settings)),
            quick: [None; QUICK_ROWS.len()],
            assistant_before: settings.assistant.perms.enabled,
            assistant: None,
            access: Access::Keep,
            before_full: None,
            demo_started: animate.then(Instant::now),
            wake: TextEdit::default(),
            wake_before_text: settings.ai.voice_wake.clone(),
            invalid_wake: false,
            connect_speech: false,
            connect_model: false,
            speech_separate_before: settings.ai.voice_separate,
            speech_separate: None,
            wake_enabled: None,
            local_enabled: None,
            wake_before: settings.assistant.wake,
            local_before: settings.assistant.local_voice && settings.assistant.speak,
            models: [ModelStatus::Missing; 2],
        };
        guide.wake.set_text(&settings.ai.voice_wake);
        guide
    }

    pub fn refresh_settings(&mut self, settings: &Settings) {
        if !self.wake_changed() {
            self.wake.set_text(&settings.ai.voice_wake);
        }
        self.wake_before_text.clone_from(&settings.ai.voice_wake);
        self.quick_before = QUICK_ROWS.map(|feature| feature.read(settings));
        self.assistant_before = settings.assistant.perms.enabled;
        self.speech_separate_before = settings.ai.voice_separate;
        self.wake_before = settings.assistant.wake;
        self.local_before = settings.assistant.local_voice && settings.assistant.speak;
    }

    pub fn wake_changed(&self) -> bool {
        self.wake.text() != self.wake_before_text
    }

    pub fn refresh_models(&mut self) {
        for (model, installed) in [(VoiceModel::Wake, wake::installed()), (VoiceModel::Local, localvoice::ready())] {
            self.models[model.index()] = match (model.status(), installed) {
                (ModelStatus::Missing, true) => ModelStatus::Installed,
                (status, _) => status,
            };
        }
    }

    pub fn poll_models(&mut self) {
        for model in [VoiceModel::Wake, VoiceModel::Local] {
            let status = model.status();
            let cached = &mut self.models[model.index()];
            if status != ModelStatus::Missing || *cached != ModelStatus::Installed {
                *cached = status;
            }
        }
    }

    pub fn downloading(&self) -> bool {
        self.visible && self.voice_page() && self.models.iter().any(|s| matches!(s, ModelStatus::Downloading(_)))
    }

    pub fn can_download(&self, model: VoiceModel) -> bool {
        !self.downloading() && matches!(self.models[model.index()], ModelStatus::Missing | ModelStatus::Failed(_))
    }

    fn enabled(&self, model: VoiceModel) -> bool {
        match model {
            VoiceModel::Wake => self.assistant_enabled() && self.wake_enabled.unwrap_or(self.wake_before),
            VoiceModel::Local => self.local_enabled.unwrap_or(self.local_before),
        }
    }

    fn can_toggle(&self, model: VoiceModel) -> bool {
        self.models[model.index()] == ModelStatus::Installed
            && (model == VoiceModel::Local || self.enabled(model) || (self.connect_model && self.connect_speech))
    }

    pub fn toggle_model(&mut self, model: VoiceModel) {
        if self.can_toggle(model) {
            let enabled = !self.enabled(model);
            match model {
                VoiceModel::Wake => {
                    self.wake_enabled = Some(enabled);
                    if enabled {
                        self.assistant = Some(true);
                    }
                }
                VoiceModel::Local => self.local_enabled = Some(enabled),
            }
        }
    }

    pub fn validated_wake(&self) -> Option<String> {
        let wake = crate::tools::voicecmd::clean_wake_setting(&self.wake.text())?;
        let mut seen = std::collections::HashSet::new();
        (!wake.is_empty() && wake.split(", ").all(|name| seen.insert(name.to_lowercase()))).then_some(wake)
    }

    fn current(&self) -> Page {
        self.mode.pages()[self.page]
    }

    pub fn choose_mode(&mut self, mode: GuideMode, animate: bool) {
        self.mode = mode;
        self.page = 1;
        self.demo_started = animate.then(Instant::now);
        self.preview.release();
    }

    pub fn change_route(&mut self) {
        self.page = 0;
        self.preview.release();
    }

    pub fn extend(&mut self, animate: bool) {
        self.mode = GuideMode::Detailed;
        self.page = 3;
        self.demo_started = animate.then(Instant::now);
        self.preview.release();
    }

    pub fn lesson(&mut self, lesson: PreviewLesson, animate: bool) {
        self.preview.select(lesson, animate);
    }

    pub fn preview_action(&mut self, action: PreviewAction) {
        self.preview.act(action);
    }

    pub fn preview_dragging(&self) -> bool {
        self.visible && self.preview.dragging()
    }

    pub fn preview_release(&mut self) -> bool {
        self.preview.release()
    }

    fn interactive_preview(&self) -> bool {
        matches!(self.current(), Page::Welcome | Page::Preset | Page::Practice)
    }

    pub fn preview_move(&mut self, size: (f32, f32), dpi: f32, x: f32, y: f32, down: bool) -> bool {
        if !self.visible || !self.interactive_preview() {
            return false;
        }
        self.preview.pointer(Layout::new(size, dpi), x, y, down)
    }

    pub fn preview_tick(&mut self) {
        if self.preview_ticking() {
            self.preview.tick();
        }
    }

    pub fn preview_ticking(&self) -> bool {
        self.visible && self.interactive_preview() && self.preview.ticking()
    }

    pub fn last_page(&self) -> bool {
        self.current() == Page::Ready
    }

    pub fn voice_page(&self) -> bool {
        self.current() == Page::Voice
    }

    pub fn essentials_page(&self) -> bool {
        self.current() == Page::Essentials
    }

    fn quick_value(&self, feature: QuickSetting) -> bool {
        let index = feature as usize;
        self.quick[index].unwrap_or(match (feature, self.preset) {
            (QuickSetting::Dock, Preset::Island) | (QuickSetting::Top, Preset::Island | Preset::Dock) => false,
            (QuickSetting::Dock, Preset::Dock | Preset::Desktop) | (QuickSetting::Top, Preset::Desktop) => true,
            _ => self.quick_before[index],
        })
    }

    pub fn toggle_quick(&mut self, feature: QuickSetting) {
        self.quick[feature as usize] = Some(!self.quick_value(feature));
    }

    fn assistant_enabled(&self) -> bool {
        self.assistant.unwrap_or(self.assistant_before)
    }

    pub fn toggle_assistant(&mut self) {
        self.assistant = Some(!self.assistant_enabled());
    }

    pub fn separate_speech(&self) -> bool {
        self.speech_separate.unwrap_or(self.speech_separate_before)
    }

    pub fn toggle_speech_service(&mut self) {
        let enabled = !self.separate_speech();
        self.speech_separate = (enabled != self.speech_separate_before).then_some(enabled);
    }

    pub fn apply_speech_choice(&self, settings: &mut AiSettings) {
        if let Some(enabled) = self.speech_separate {
            settings.voice_separate = enabled;
        }
    }

    pub fn select_access(&mut self, access: Access) {
        if access == Access::Full {
            if let Some((previous, assistant)) = self.before_full.take() {
                self.access = previous;
                self.assistant = assistant;
                return;
            }
            self.before_full = Some((self.access, self.assistant));
        } else {
            self.before_full = None;
        }
        self.access = access;
        if access != Access::Keep {
            self.assistant = Some(true);
        }
    }

    pub fn apply_choices(&self, settings: &mut Settings) {
        self.preset.apply(settings);
        for feature in QUICK_ROWS {
            if let Some(enabled) = self.quick[feature as usize] {
                if enabled && !feature.read(settings) {
                    match feature {
                        QuickSetting::Dock => {
                            settings.desktop_dock.auto_hide = true;
                        }
                        QuickSetting::Top => {
                            settings.top_panel.reserve_space = false;
                            settings.top_panel.auto_hide = true;
                        }
                        _ => {}
                    }
                }
                feature.write(settings, enabled);
            }
        }
        self.access.apply(&mut settings.assistant.perms);
        self.apply_speech_choice(&mut settings.ai);
        if let Some(enabled) = self.assistant {
            settings.assistant.perms.enabled = enabled;
        }
        if let Some(enabled) = self.wake_enabled {
            settings.assistant.wake = enabled;
        }
        if let Some(enabled) = self.local_enabled {
            settings.assistant.local_voice = enabled;
            if enabled {
                settings.assistant.speak = true;
            }
        }
    }

    pub fn select(&mut self, preset: Preset, animate: bool) {
        if self.preset != preset {
            self.preset = preset;
            self.demo_started = animate.then(Instant::now);
        }
    }

    pub fn move_by(&mut self, offset: isize, animate: bool) {
        let page = self.page.saturating_add_signed(offset).min(self.mode.pages().len() - 1);
        if page != self.page {
            self.page = page;
            self.preview.release();
            if self.current() != Page::Practice {
                self.preview.select(PreviewLesson::Music, false);
            }
            self.demo_started = animate.then(Instant::now);
        }
    }

    fn elapsed(&self) -> Duration {
        self.demo_started.map_or(Duration::MAX, |started| started.elapsed())
    }

    fn voice_phase(&self) -> usize {
        match self.elapsed().as_millis() {
            0..=349 => 0,
            350..=1299 => 1,
            1300..=2099 => 2,
            2100..=3499 => 3,
            _ => 4,
        }
    }

    pub fn animating(&self) -> bool {
        let duration = if self.voice_page() { 3500 } else { 1050 };
        self.visible
            && ((!self.essentials_page() && self.elapsed() < Duration::from_millis(duration))
                || (self.interactive_preview() && self.preview.animating()))
    }

    fn panels(&self) -> (bool, bool) {
        (self.quick_value(QuickSetting::Dock), self.quick_value(QuickSetting::Top))
    }
}

fn eased(ms: u128, delay: u128, duration: f32) -> f32 {
    let t = (ms.saturating_sub(delay) as f32 / duration).min(1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy)]
struct Layout {
    x: f32,
    y: f32,
    scale: f32,
}

impl Layout {
    fn new((w, h): (f32, f32), dpi: f32) -> Self {
        let available = (h - 40.0 * dpi).max(1.0);
        let scale = (w / 940.0).min(available / 640.0).min(dpi).max(0.001);
        Self { x: (w - 940.0 * scale) / 2.0, y: h - 680.0 * scale - (available - 640.0 * scale) / 2.0, scale }
    }

    fn rect(self, (l, t, r, b): Rect) -> Rect {
        (self.x + l * self.scale, self.y + t * self.scale, self.x + r * self.scale, self.y + b * self.scale)
    }
}

fn preset_row(index: usize) -> Rect {
    let y = 270.0 + 69.0 * index as f32;
    (48.0, y, 404.0, y + 60.0)
}

fn controls(guide: &GuideUi) -> Vec<(GuideAction, &'static str, Rect)> {
    use GuideAction::*;
    let mut buttons = vec![
        (Skip, "Skip setup", (48.0, 601.0, 219.0, 647.0)),
        (Next, if guide.last_page() { "Apply and finish" } else { "Continue" }, (706.0, 601.0, 892.0, 647.0)),
    ];
    if guide.page > 0 {
        buttons.push((Back, "Back", (591.0, 601.0, 694.0, 647.0)));
    }
    if guide.current() == Page::Welcome {
        buttons.retain(|(action, _, _)| *action != Next);
        buttons.push((Mode(GuideMode::Quick), "Quick setup", (48.0, 277.0, 404.0, 385.0)));
        buttons.push((Mode(GuideMode::Detailed), "Guided setup", (48.0, 399.0, 404.0, 523.0)));
    } else {
        buttons.push((ChangeRoute, "Change route", (241.0, 601.0, 428.0, 647.0)));
    }
    if guide.current() == Page::Practice {
        for (i, lesson) in PreviewLesson::ALL.into_iter().enumerate() {
            let y = 276.0 + 66.0 * i as f32;
            buttons.push((Lesson(lesson), lesson.label(), (48.0, y, 404.0, y + 54.0)));
        }
        buttons.push((Preview(PreviewAction::Reset), "Try again", (48.0, 495.0, 224.0, 534.0)));
    }
    if guide.last_page() && guide.mode == GuideMode::Quick {
        buttons.push((Extend, "Explore and set up voice", (48.0, 521.0, 404.0, 560.0)));
    }
    if guide.current() == Page::Preset {
        buttons.extend(
            PRESET_ROWS.into_iter().enumerate().map(|(i, preset)| (Select(preset), preset.label(), preset_row(i))),
        );
    }
    if guide.essentials_page() {
        buttons
            .extend(QUICK_ROWS.into_iter().map(|feature| (ToggleQuick(feature), feature.visual().1, feature.rect())));
        buttons.push((ToggleAssistant, "Voice assistant", ASSISTANT_ROW));
        buttons.extend(ACCESS_ROWS.into_iter().map(|access| (SelectAccess(access), access.label(), access.rect())));
        buttons.push((ToggleSpeechService, "Separate speech service", SPEECH_ESSENTIALS));
        if guide.separate_speech() {
            buttons.push((OpenRecognition, "Configure speech service", (460.0, 529.0, 874.0, 555.0)));
        }
    }
    if guide.voice_page() {
        buttons.push((SuggestWake, "Suggest names", (264.0, 248.0, 404.0, 274.0)));
        buttons.push((ToggleSpeechService, "Separate speech service", SPEECH_VOICE));
        if guide.separate_speech() {
            let right = if guide.connect_model { 865.0 } else { 655.0 };
            buttons.push((OpenRecognition, "Configure speech service", (453.0, 513.0, right, 546.0)));
        }
        if !guide.connect_model {
            let left = if guide.separate_speech() { 663.0 } else { 453.0 };
            buttons.push((OpenAssistant, "Connect an AI service", (left, 513.0, 865.0, 546.0)));
        }
        for model in [VoiceModel::Wake, VoiceModel::Local] {
            let label = match guide.models[model.index()] {
                ModelStatus::Failed(_) => "Retry",
                _ => "Download",
            };
            let (_, top, _, bottom) = model.row();
            let r = (295.0, top + 15.0, 391.0, bottom - 15.0);
            if guide.can_download(model) {
                buttons.push((Download(model), label, r));
            } else if guide.can_toggle(model) {
                buttons.push((ToggleModel(model), if guide.enabled(model) { "On" } else { "Enable" }, r));
            }
        }
    }
    buttons
}

pub fn guide_hit(size: (f32, f32), dpi: f32, guide: &GuideUi, x: f32, y: f32) -> Option<GuideAction> {
    let layout = Layout::new(size, dpi);
    controls(guide)
        .into_iter()
        .find(|(_, _, rect)| inside(layout.rect(*rect), x, y))
        .map(|(action, _, _)| action)
        .or_else(|| {
            guide.interactive_preview().then(|| guide.preview.hit(layout, x, y)).flatten().map(GuideAction::Preview)
        })
}

pub fn guide_wake_rects(size: (f32, f32), dpi: f32, guide: &GuideUi) -> Option<(Rect, Rect)> {
    if !guide.voice_page() {
        return None;
    }
    let layout = Layout::new(size, dpi);
    Some((layout.rect((48.0, 277.0, 404.0, 321.0)), layout.rect((61.0, 277.0, 391.0, 321.0))))
}

pub fn guide_button(p: &Painter, r: Rect, s: f32, label: &str, primary: bool, hover: bool) -> Result<()> {
    let fill = if primary {
        acc(if hover { 0.51 } else { 0.37 })
    } else {
        rgba(1.0, 1.0, 1.0, if hover { 0.13 } else { 0.067 })
    };
    p.fill_round_rect(r, 12.0 * s, fill)?;
    if primary {
        p.stroke_round_rect(r, 12.0 * s, s, acc(0.78))?;
    }
    p.text(label, r, 13.5 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)
}

struct Preview<'a> {
    p: &'a Painter<'a>,
    layout: Layout,
}

impl Preview<'_> {
    fn round(&self, rect: Rect, radius: f32, color: D2D1_COLOR_F) -> Result<()> {
        self.p.fill_round_rect(self.layout.rect(rect), radius * self.layout.scale, color)
    }

    fn text(&self, label: &str, rect: Rect, size: f32, strong: bool, color: D2D1_COLOR_F) -> Result<()> {
        self.p.text(
            label,
            self.layout.rect(rect),
            size * self.layout.scale,
            if strong { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
            DWRITE_TEXT_ALIGNMENT_LEADING,
            color,
        )
    }

    fn icon(&self, glyph: char, rect: Rect, size: f32) -> Result<()> {
        self.p.glyph(glyph, self.layout.rect(rect), size * self.layout.scale, WHITE)
    }

    fn island(&self, x: f32, y: f32, width: f32, height: f32) -> Result<()> {
        self.round((x, y, x + width, y + height), 24.0, rgba(0.015, 0.018, 0.023, 1.0))?;
        self.p.stroke_round_rect(
            self.layout.rect((x, y, x + width, y + height)),
            24.0 * self.layout.scale,
            self.layout.scale,
            rgba(1.0, 1.0, 1.0, 0.13),
        )
    }

    fn pet(&self, x: f32, y: f32, size: f32) -> Result<()> {
        self.pose_pet(x, y, size, crate::ui::characters::Reaction::Idle, 0.0)
    }

    fn pose_pet(&self, x: f32, y: f32, size: f32, reaction: crate::ui::characters::Reaction, mouth: f32) -> Result<()> {
        use crate::ui::characters::{Character, Pose, draw_with_design};
        let r = self.layout.rect((x, y, x + size, y + size));
        let saved = self.p.push_offset(r.0, r.1);
        let result = draw_with_design(
            self.p,
            Character::Spark,
            size * self.layout.scale,
            &Pose {
                motion: crate::ui::pet_motion::Motion { mouth, ..Default::default() },
                t: 0.0,
                reaction,
                look: (0.35, 0.0),
                blink: false,
                accent: crate::ui::theme::accent(),
                beat: 0.0,
            },
            None,
        );
        self.p.set_transform(&saved);
        result
    }

    fn center(&self, label: &str, rect: Rect, size: f32, color: D2D1_COLOR_F) -> Result<()> {
        self.p.text(
            label,
            self.layout.rect(rect),
            size * self.layout.scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            color,
        )
    }

    fn glow(&self, center: (f32, f32), radii: (f32, f32), color: D2D1_COLOR_F) -> Result<()> {
        let (x, y, _, _) = self.layout.rect((center.0, center.1, center.0, center.1));
        let s = self.layout.scale;
        self.p.fill_ellipse_gradient((x, y, radii.0 * s, radii.1 * s), color, rgba(color.r, color.g, color.b, 0.0))
    }

    fn curve(&self, points: [(f32, f32); 4], width: f32, color: D2D1_COLOR_F) -> Result<()> {
        self.p.bezier(
            points.map(|(x, y)| {
                let (x, y, _, _) = self.layout.rect((x, y, x, y));
                (x, y)
            }),
            width * self.layout.scale,
            color,
        )
    }
}

fn paint_route(v: &Preview<'_>, mode: GuideMode, rect: Rect, hover: bool) -> Result<()> {
    let quick = mode == GuideMode::Quick;
    let (l, t, r, b) = rect;
    v.round(
        rect,
        18.0,
        if quick {
            acc(if hover { 0.28 } else { 0.18 })
        } else {
            rgba(1.0, 1.0, 1.0, if hover { 0.11 } else { 0.065 })
        },
    )?;
    v.icon(if quick { '\u{E945}' } else { '\u{E8A5}' }, (l + 16.0, t + 16.0, l + 46.0, t + 46.0), 19.0)?;
    v.text(
        if quick { "Quick setup" } else { "Guided setup" },
        (l + 57.0, t + 12.0, r - 20.0, t + 42.0),
        18.0,
        true,
        WHITE,
    )?;
    v.text(
        if quick { "About 1 minute" } else { "About 5 minutes" },
        (l + 57.0, t + 42.0, r - 20.0, t + 64.0),
        12.0,
        false,
        acc(1.0),
    )?;
    v.p.text_wrapped(
        if quick {
            "Layout, essential switches and assistant permissions."
        } else {
            "Everything in quick setup, hands-on practice, pet and voice models."
        },
        v.layout.rect((l + 18.0, t + 69.0, r - 18.0, b - 10.0)),
        12.5 * v.layout.scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        rgba(1.0, 1.0, 1.0, 0.74),
    )
}

fn paint_voice(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    let p = v.p;
    let l = v.layout;
    use crate::ui::characters::Reaction;
    let phase = guide.voice_phase();
    let elapsed = guide.elapsed().as_secs_f32().min(3.5);
    let mouth = if phase == 3 { 0.18 + 0.69 * (elapsed * 16.0).sin().abs() } else { 0.0 };
    v.island(474.0, 220.0, 370.0, 225.0)?;
    v.pose_pet(
        490.0,
        230.0,
        46.0,
        match phase {
            0 => Reaction::Idle,
            3 | 4 => Reaction::Done,
            _ => Reaction::Attention,
        },
        mouth,
    )?;
    v.text("Assistant", (548.0, 232.0, 824.0, 259.0), 16.0, true, WHITE)?;
    v.text("Example only · microphone off", (548.0, 259.0, 826.0, 277.0), 11.0, false, rgba(1.0, 1.0, 1.0, 0.64))?;
    for (i, (icon, label)) in
        [('\u{E720}', "Listening…"), ('\u{E99A}', "Thinking…"), ('\u{E8F2}', "Speaking")].into_iter().enumerate()
    {
        let x = 495.0 + i as f32 * 109.0;
        let active = phase == i + 1;
        v.round((x, 288.0, x + 101.0, 350.0), 12.0, acc(if active { 0.42 } else { 0.11 }))?;
        v.icon(icon, (x + 7.0, 290.0, x + 94.0, 322.0), 15.0)?;
        p.text(
            label,
            l.rect((x + 3.0, 322.0, x + 98.0, 347.0)),
            11.0 * l.scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 1.0, 1.0, if active { 1.0 } else { 0.63 }),
        )?;
    }
    v.round((495.0, 366.0, 823.0, 434.0), 12.0, rgba(1.0, 1.0, 1.0, 0.075))?;
    v.text("Remind me in ten minutes.", (510.0, 371.0, 809.0, 399.0), 13.0, false, rgba(1.0, 1.0, 1.0, 0.71))?;
    if phase >= 3 {
        v.text("I’ll remind you in ten minutes.", (510.0, 399.0, 809.0, 426.0), 13.0, true, WHITE)?;
    } else if phase > 0 {
        for i in 0..12 {
            let x = 512.0 + 6.0 * i as f32;
            let h = 4.0 + 12.0 * (elapsed * 13.0 - i as f32 * 0.7).sin().abs();
            v.round((x, 413.0 - h / 2.0, x + 3.0, 413.0 + h / 2.0), 1.5, acc(0.75))?;
        }
    }
    Ok(())
}

fn paint_stage(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    let p = v.p;
    let l = v.layout;
    let stage = l.rect(SCENE);
    let hero = matches!(guide.current(), Page::Welcome | Page::Preset | Page::Practice);
    p.fill_round_gradient(stage, 26.0 * l.scale, rgba(0.065, 0.082, 0.15, 1.0), rgba(0.032, 0.037, 0.059, 1.0))?;
    v.glow((648.0, 321.0), (214.0, 142.0), acc(if hero { 0.29 } else { 0.15 }))?;
    v.glow((772.0, 399.0), (115.0, 99.0), rgba(0.16, 0.41, 0.69, 0.16))?;
    p.push_clip(l.rect((440.0, 217.0, 878.0, 479.0)));
    let backdrop = (|| -> Result<()> {
        for i in 0..7 {
            let y = i as f32 * 13.0;
            v.curve(
                [(444.0, 389.0 + y), (551.0, 232.0 + y), (706.0, 513.0 + y), (875.0, 295.0 + y)],
                1.0,
                acc(if hero { 0.23 } else { 0.067 }),
            )?;
        }
        for (x, y, radius) in [
            (473.0, 298.0, 1.0),
            (504.0, 244.0, 1.5),
            (591.0, 263.0, 1.0),
            (724.0, 238.0, 1.5),
            (810.0, 258.0, 1.0),
            (842.0, 319.0, 1.5),
            (479.0, 420.0, 1.5),
            (831.0, 439.0, 1.0),
        ] {
            v.round((x - radius, y - radius, x + radius, y + radius), radius, acc(if hero { 0.57 } else { 0.35 }))?;
        }
        Ok(())
    })();
    p.pop_clip();
    backdrop?;
    p.stroke_round_rect(stage, 26.0 * l.scale, l.scale, rgba(0.7, 0.74, 1.0, 0.25))?;
    v.text(
        match guide.current() {
            Page::Welcome | Page::Preset | Page::Practice => "Live island preview",
            Page::Voice => "Voice preview",
            _ => "On your desktop",
        },
        (453.0, 176.0, 701.0, 209.0),
        13.0,
        true,
        WHITE,
    )?;
    v.round((744.0, 180.0, 865.0, 205.0), 12.5, rgba(1.0, 1.0, 1.0, 0.065))?;
    v.round((757.0, 190.0, 763.0, 196.0), 3.0, acc(0.98))?;
    v.text("Demo mode", (772.0, 180.0, 857.0, 205.0), 10.5, true, rgba(1.0, 1.0, 1.0, 0.75))?;
    match guide.current() {
        Page::Preset => paint_workspace(v, guide),
        Page::Welcome | Page::Practice => guide.preview.paint(v),
        Page::Voice => paint_voice(v, guide),
        _ => paint_ready(v, guide),
    }
}

fn paint_workspace(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    let ms = guide.elapsed().as_millis();
    let t = eased(ms, 190, 590.0);
    let (dock, top) = guide.panels();
    let width = 194.0 + 234.0 * t;
    let x = 659.0 - width / 2.0;
    let y = 269.0 - 27.0 * t;
    let height = 42.0 + 178.0 * t;
    v.glow((659.0, y + height * 0.7), (188.0, 80.0), rgba(0.01, 0.02, 0.07, 0.8))?;
    if top {
        let slide = 29.0 * eased(ms, 60, 460.0);
        v.island(480.0 - slide, 215.0, 110.0, 27.0)?;
        v.island(728.0 + slide, 215.0, 110.0, 27.0)?;
        v.center("Explorer", (483.0 - slide, 215.0, 586.0 - slide, 242.0), 10.0, WHITE)?;
        v.icon('\u{E701}', (733.0 + slide, 215.0, 778.0 + slide, 242.0), 12.0)?;
        v.icon('\u{E767}', (783.0 + slide, 215.0, 828.0 + slide, 242.0), 12.0)?;
    }
    v.island(x, y, width, height)?;
    if t >= 0.36 {
        v.p.push_clip(v.layout.rect((x + 13.0, y + 7.0, x + width - 13.0, y + height - 10.0)));
        let result = guide.preview.paint_music(v);
        v.p.pop_clip();
        result?;
    } else {
        v.pet(x + 8.0, y + 3.0, 35.0)?;
        v.center("21:08    18°", (x + 44.0, y, x + 181.0, y + 42.0), 14.0, WHITE)?;
    }
    if dock {
        let y = 507.0 + 14.0 * (1.0 - eased(ms, 510, 460.0));
        v.round((533.0, y, 785.0, y + 40.0), 14.0, rgba(0.035, 0.047, 0.075, 1.0))?;
        for (i, (glyph, color)) in [
            ('\u{E8B7}', rgba(0.58, 0.42, 0.23, 1.0)),
            ('\u{E774}', rgba(0.29, 0.42, 0.73, 1.0)),
            ('\u{E189}', acc(0.7)),
            ('\u{E8F2}', rgba(0.35, 0.30, 0.67, 1.0)),
            ('\u{E70B}', rgba(0.23, 0.49, 0.45, 1.0)),
            ('\u{E713}', rgba(0.34, 0.37, 0.45, 1.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let x = 542.0 + 40.0 * i as f32;
            let r = (x, y + 5.0, x + 31.0, y + 35.0);
            v.round(r, 8.0, color)?;
            v.icon(glyph, r, 15.0)?;
        }
    }
    Ok(())
}

fn paint_ready(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    let ms = guide.elapsed().as_millis();
    let card_y = 234.0 - 20.0 * (1.0 - eased(ms, 70, 530.0));
    let card = v.layout.rect((479.0, card_y, 839.0, card_y + 147.0));
    let record = crate::platform::notifications::Record {
        id: 0,
        created: 0,
        app_id: String::new(),
        app: "Night Island".into(),
        title: crate::ui::i18n::tr("Example notification").into_owned(),
        body: crate::ui::i18n::tr("New messages can appear here while you work.").into_owned(),
        icon: None,
        avatar: None,
    };
    v.island(479.0, card_y, 360.0, 147.0)?;
    let _ = paint_notification_card(
        v.p,
        (card.0, card.1, card.2 - card.0, card.3 - card.1),
        v.layout.scale,
        &record,
        0,
        false,
        None,
    )?;
    let pet = eased(ms, 360, 610.0);
    v.island(479.0, 423.0, 170.0, 37.0)?;
    v.center("21:08    18°", (525.0, 423.0, 636.0, 460.0), 12.0, WHITE)?;
    v.pet(485.0 + 247.0 * pet, 426.0 + 21.0 * pet, 33.0 + 33.0 * pet)?;
    v.text(
        "Drag Spark off the island to let it roam.",
        (479.0, 509.0, 839.0, 539.0),
        12.0,
        true,
        rgba(1.0, 1.0, 1.0, 0.72),
    )
}

fn paint_preset(v: &Preview<'_>, guide: &GuideUi, preset: Preset, rect: Rect, hover: bool) -> Result<()> {
    let (x, y, r, b) = rect;
    let selected = preset == guide.preset;
    let fill = if selected { acc(0.18) } else { rgba(1.0, 1.0, 1.0, if hover { 0.11 } else { 0.055 }) };
    v.round(rect, 13.0, fill)?;
    if selected {
        v.p.stroke_round_rect(v.layout.rect(rect), 13.0 * v.layout.scale, v.layout.scale, acc(0.64))?;
    }
    let glyph = match preset {
        Preset::Keep => '\u{E73E}',
        Preset::Island => '\u{E737}',
        Preset::Dock => '\u{E7F4}',
        Preset::Desktop => '\u{E8A9}',
    };
    v.round((x + 13.0, y + 12.0, x + 49.0, b - 12.0), 10.0, acc(if selected { 0.35 } else { 0.16 }))?;
    v.icon(glyph, (x + 13.0, y + 12.0, x + 49.0, b - 12.0), 15.0)?;
    v.text(preset.label(), (x + 62.0, y + 8.0, r - 37.0, y + 32.0), 14.0, true, WHITE)?;
    v.text(preset.description(), (x + 62.0, y + 32.0, r - 20.0, b - 8.0), 11.5, false, rgba(1.0, 1.0, 1.0, 0.70))?;
    if selected {
        v.icon('\u{E73E}', (r - 32.0, y + 6.0, r - 9.0, y + 29.0), 12.0)?;
    }
    Ok(())
}

fn paint_quick_toggle(v: &Preview<'_>, rect: Rect, glyph: char, label: &str, enabled: bool, hover: bool) -> Result<()> {
    let (x, y, r, b) = rect;
    let mid = (y + b) / 2.0;
    v.round(rect, 11.0, rgba(1.0, 1.0, 1.0, if hover { 0.11 } else { 0.055 }))?;
    v.icon(glyph, (x + 9.0, y, x + 39.0, b), 15.0)?;
    v.text(label, (x + 48.0, y, r - 58.0, b), 13.0, true, WHITE)?;
    paint_switch(v, r - 13.0, mid, enabled, acc(0.8))
}

fn paint_switch(v: &Preview<'_>, right: f32, y: f32, enabled: bool, tone: D2D1_COLOR_F) -> Result<()> {
    let fill = if enabled { tone } else { rgba(1.0, 1.0, 1.0, 0.17) };
    v.round((right - 42.0, y - 10.0, right, y + 10.0), 10.0, fill)?;
    let dot = right - if enabled { 11.0 } else { 31.0 };
    v.round((dot - 7.0, y - 7.0, dot + 7.0, y + 7.0), 7.0, WHITE)
}

fn paint_access_panel(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    let rect = v.layout.rect((442.0, 151.0, 892.0, 560.0));
    let scale = v.layout.scale;
    v.p.fill_round_gradient(rect, 23.0 * scale, rgba(0.085, 0.094, 0.15, 1.0), rgba(0.039, 0.045, 0.065, 1.0))?;
    v.p.stroke_round_rect(rect, 23.0 * scale, scale, acc(0.31))?;
    v.text("Assistant access", (460.0, 160.0, 874.0, 197.0), 18.0, true, WHITE)?;
    if !guide.separate_speech() {
        v.text(
            "Use this if your AI service cannot recognize speech.",
            (460.0, 529.0, 874.0, 555.0),
            11.5,
            false,
            rgba(1.0, 1.0, 1.0, 0.68),
        )?;
    }
    Ok(())
}

fn paint_access_choice(v: &Preview<'_>, guide: &GuideUi, access: Access, rect: Rect, hover: bool) -> Result<()> {
    let (x, y, r, b) = rect;
    let selected = access == guide.access;
    let tone = if access == Access::Full { rgba(1.0, 0.74, 0.39, 0.95) } else { acc(0.95) };
    v.round(
        rect,
        11.0,
        if selected {
            rgba(tone.r, tone.g, tone.b, 0.15)
        } else {
            rgba(1.0, 1.0, 1.0, if hover { 0.10 } else { 0.045 })
        },
    )?;
    if selected {
        v.p.stroke_round_rect(
            v.layout.rect(rect),
            11.0 * v.layout.scale,
            v.layout.scale,
            rgba(tone.r, tone.g, tone.b, 0.58),
        )?;
    }
    let full = access == Access::Full;
    let glyph = if full {
        '\u{E7BA}'
    } else if selected {
        '\u{E73E}'
    } else {
        '\u{E739}'
    };
    v.p.glyph(glyph, v.layout.rect((x + 9.0, y + 8.0, x + 33.0, y + 32.0)), 13.0 * v.layout.scale, tone)?;
    if full {
        paint_switch(v, r - 13.0, y + 19.0, selected, tone)?;
    }
    v.text(access.label(), (x + 41.0, y + 7.0, r - if full { 58.0 } else { 10.0 }, y + 32.0), 13.0, true, WHITE)?;
    v.p.text_wrapped(
        access.detail(),
        v.layout.rect((x + 41.0, y + 32.0, r - 10.0, b - 6.0)),
        11.5 * v.layout.scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        rgba(1.0, 1.0, 1.0, 0.73),
    )
}

fn paint_models(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    for model in [VoiceModel::Wake, VoiceModel::Local] {
        let (x, y, r, b) = model.row();
        v.round((x, y, r, b), 13.0, rgba(1.0, 1.0, 1.0, 0.065))?;
        v.text(model.label(), (x + 13.0, y + 8.0, r - 116.0, y + 33.0), 13.5, true, WHITE)?;
        let status = guide.models[model.index()];
        let detail = match status {
            ModelStatus::Missing => model.detail(),
            ModelStatus::Downloading(_) => "Downloading model…",
            ModelStatus::Installed => "Installed",
            ModelStatus::Failed(_) => "Download failed",
        };
        v.text(detail, (x + 13.0, y + 33.0, r - 116.0, b - 8.0), 11.5, false, rgba(1.0, 1.0, 1.0, 0.70))?;
        if let ModelStatus::Downloading(step) = status {
            let left = r - 106.0;
            v.round((left, y + 34.0, r - 14.0, y + 38.0), 2.0, acc(0.18))?;
            v.round((left, y + 34.0, left + 92.0 * (0.10 + f32::from(step) / 112.0), y + 38.0), 2.0, acc(0.9))?;
        }
    }
    let failure = guide.models.iter().find_map(|status| match status {
        ModelStatus::Failed(why) => Some(*why),
        _ => None,
    });
    if let Some(why) = failure {
        v.p.text_wrapped(
            why,
            v.layout.rect((48.0, 503.0, 404.0, 549.0)),
            11.5 * v.layout.scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 0.68, 0.62, 1.0),
        )?;
    } else {
        let hint = if guide.downloading() {
            "You can continue while the model downloads."
        } else {
            "Both downloads are optional."
        };
        v.text(hint, (48.0, 500.0, 404.0, 525.0), 11.5, false, rgba(1.0, 1.0, 1.0, 0.66))?;
    }
    Ok(())
}

fn paint_ready_summary(v: &Preview<'_>, guide: &GuideUi) -> Result<()> {
    for (i, (icon, title, body)) in [
        ('\u{E7F4}', "Workspace", guide.preset.label()),
        (
            '\u{EA8F}',
            "Windows notifications",
            if guide.quick_value(QuickSetting::Notifications) {
                "Windows may ask for notification access."
            } else {
                "Grant access from the Notifications tab."
            },
        ),
        (
            '\u{E99A}',
            "Voice assistant",
            if guide.access != Access::Keep && guide.assistant_enabled() {
                guide.access.label()
            } else if guide.enabled(VoiceModel::Wake) {
                "Wake-word listening is selected."
            } else if guide.connect_model && guide.connect_speech {
                "Voice services are connected."
            } else {
                "Connect voice services later under Assistant."
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 277.0 + 82.0 * i as f32;
        v.round((48.0, y, 404.0, y + 72.0), 14.0, rgba(1.0, 1.0, 1.0, 0.055))?;
        v.round((62.0, y + 15.0, 102.0, y + 55.0), 11.0, acc(0.18))?;
        v.icon(icon, (62.0, y + 15.0, 102.0, y + 55.0), 17.0)?;
        v.text(title, (117.0, y + 9.0, 394.0, y + 36.0), 14.0, true, WHITE)?;
        v.text(body, (117.0, y + 36.0, 394.0, y + 61.0), 11.5, false, rgba(1.0, 1.0, 1.0, 0.70))?;
    }
    Ok(())
}

pub fn paint_guide(p: &Painter, size: (f32, f32), dpi: f32, guide: &GuideUi, ui: &UiState) -> Result<()> {
    let l = Layout::new(size, dpi);
    let v = Preview { p, layout: l };
    let muted = rgba(1.0, 1.0, 1.0, 0.73);
    v.round((48.0, 61.0, 80.0, 93.0), 9.0, acc(0.26))?;
    v.icon('\u{E7F4}', (48.0, 61.0, 80.0, 93.0), 16.0)?;
    v.text("Night Island", (91.0, 61.0, 300.0, 93.0), 17.0, true, WHITE)?;
    let steps = &guide.mode.pages()[1..];
    let step_width = 484.0 / steps.len() as f32;
    for (i, page) in steps.iter().enumerate() {
        let label = page.label();
        let x = 408.0 + i as f32 * step_width;
        let selected = *page == guide.current();
        let color = if selected { WHITE } else { muted };
        v.round(
            (x, 61.0, x + step_width - 7.0, 94.0),
            10.0,
            if selected { acc(0.20) } else { rgba(1.0, 1.0, 1.0, 0.055) },
        )?;
        v.center(label, (x + 3.0, 61.0, x + step_width - 10.0, 94.0), 12.0, color)?;
    }
    v.round((48.0, 116.0, 892.0, 117.0), 0.5, rgba(1.0, 1.0, 1.0, 0.12))?;
    let (title, summary) = match guide.current() {
        Page::Welcome => ("Make Island yours", "Start with the essentials or explore everything with a guide."),
        Page::Preset => ("Choose your layout", "Pick a preset to see how it looks. It applies when you finish."),
        Page::Essentials => ("Essentials", "Pick the features you want to use."),
        Page::Practice => {
            ("Try the real island", "These are the same controls you will use on your desktop. Try them here.")
        }
        Page::Voice => ("Make it respond to you", "Give the assistant a name and download the voice models you want."),
        Page::Ready => {
            ("Ready to use Island", "Check your choices and apply them. You can change them later in Settings.")
        }
    };
    v.text(title, (48.0, 147.0, 414.0, 193.0), 28.0, true, WHITE)?;
    p.text_wrapped(
        summary,
        l.rect((48.0, 196.0, 404.0, if guide.essentials_page() { 217.0 } else { 252.0 })),
        14.0 * l.scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        muted,
    )?;
    if guide.essentials_page() {
        paint_access_panel(&v, guide)?;
    } else {
        paint_stage(&v, guide)?;
    }
    if let Some((field, inner)) = guide_wake_rects(size, dpi, guide) {
        v.text("Assistant wake words", (48.0, 248.0, 255.0, 274.0), 13.0, true, WHITE)?;
        paint_line_edit(
            p,
            field,
            inner,
            &guide.wake,
            false,
            "Luna, Taburetka",
            ui.sw.field == Some(Field::GuideWake),
            ui.sw.caret_on,
            l.scale,
        )?;
        v.text(
            if guide.invalid_wake {
                "Enter distinct names of up to 24 characters."
            } else {
                "Use two different-sounding names, separated by commas."
            },
            (48.0, 321.0, 404.0, 347.0),
            11.5,
            false,
            if guide.invalid_wake { rgba(1.0, 0.65, 0.60, 1.0) } else { muted },
        )?;
        paint_models(&v, guide)?;
        let hint = if guide.separate_speech() {
            if guide.connect_speech { "A speech API key is available." } else { "Set the speech server and API key." }
        } else {
            "Use this if your AI service cannot recognize speech."
        };
        v.center(hint, (453.0, 488.0, 865.0, 511.0), 11.5, muted)?;
    } else if guide.last_page() {
        paint_ready_summary(&v, guide)?;
    }
    v.round((48.0, 578.0, 892.0, 579.0), 0.5, rgba(1.0, 1.0, 1.0, 0.12))?;
    for (action, label, rect) in controls(guide) {
        let hover = ui.sw.hover == Some(SwHit::Guide(action));
        match action {
            GuideAction::Mode(mode) => paint_route(&v, mode, rect, hover)?,
            GuideAction::Lesson(lesson) => {
                guide_button(p, l.rect(rect), l.scale, label, guide.preview.lesson == lesson, hover)?;
            }
            GuideAction::Select(preset) => paint_preset(&v, guide, preset, rect, hover)?,
            GuideAction::ToggleQuick(feature) => {
                let (glyph, _) = feature.visual();
                paint_quick_toggle(&v, rect, glyph, label, guide.quick_value(feature), hover)?;
            }
            GuideAction::ToggleAssistant => {
                paint_quick_toggle(&v, rect, '\u{E720}', label, guide.assistant_enabled(), hover)?;
            }
            GuideAction::ToggleSpeechService => {
                paint_quick_toggle(&v, rect, '\u{E720}', label, guide.separate_speech(), hover)?;
            }
            GuideAction::SelectAccess(access) => paint_access_choice(&v, guide, access, rect, hover)?,
            _ => {
                let primary = matches!(action, GuideAction::Next | GuideAction::Download(VoiceModel::Wake))
                    || matches!(action, GuideAction::ToggleModel(model) if guide.enabled(model));
                guide_button(p, l.rect(rect), l.scale, label, primary, hover)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_controls_fit_small_windows_and_high_dpi() {
        let mut guide = GuideUi::new(&Settings::default(), false);
        guide.toggle_speech_service();
        guide.move_by(-1, false);
        assert_eq!(guide.page, 0);
        guide.mode = GuideMode::Detailed;
        for step in 0..guide.mode.pages().len() {
            for (size, dpi) in [((760.0, 550.0), 1.0), ((940.0, 680.0), 1.0), ((1880.0, 1360.0), 2.0)] {
                let layout = Layout::new(size, dpi);
                for (action, _, rect) in controls(&guide) {
                    let r = layout.rect(rect);
                    assert!(r.0 >= 0.0 && r.1 >= 40.0 * dpi && r.2 <= size.0 && r.3 <= size.1);
                    assert_eq!(guide_hit(size, dpi, &guide, (r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0), Some(action));
                }
                assert_eq!(guide_wake_rects(size, dpi, &guide).is_some(), guide.voice_page());
            }
            assert_eq!(guide.page, step);
            guide.move_by(1, false);
        }
        assert!(guide.last_page());
        assert!(!guide.animating());
    }

    #[test]
    fn switching_routes_keeps_choices_and_quick_setup_skips_voice() {
        let mut settings = Settings::default();
        let mut guide = GuideUi::new(&settings, false);
        guide.choose_mode(GuideMode::Quick, false);
        guide.select(Preset::Desktop, false);
        guide.toggle_quick(QuickSetting::Top);
        guide.move_by(1, false);
        assert!(guide.essentials_page());
        assert!(controls(&guide).iter().any(|(action, _, _)| *action == GuideAction::ToggleSpeechService));
        guide.toggle_speech_service();
        assert!(!settings.ai.voice_separate);
        guide.move_by(1, false);
        assert!(guide.last_page());
        assert!(!guide.voice_page());
        guide.extend(false);
        assert_eq!(guide.current(), Page::Practice);
        assert_eq!(guide.preset, Preset::Desktop);
        assert!(!guide.quick_value(QuickSetting::Top));
        guide.move_by(1, false);
        assert!(guide.voice_page());
        guide.wake.set_text("Luna, Taburetka");
        guide.change_route();
        guide.choose_mode(GuideMode::Detailed, false);
        assert_eq!(guide.wake.text(), "Luna, Taburetka");
        assert!(!guide.quick_value(QuickSetting::Top));
        guide.refresh_settings(&settings);
        guide.apply_choices(&mut settings);
        assert!(settings.ai.voice_separate);
        guide.refresh_settings(&settings);
        guide.toggle_speech_service();
        guide.apply_choices(&mut settings);
        assert!(!settings.ai.voice_separate);
        guide.visible = false;
        assert!(!guide.preview_ticking());
    }

    #[test]
    fn presets_preserve_taskbar_choice_pins_and_other_settings() {
        let mut settings = Settings { scale_percent: 120, ..Settings::default() };
        let pins = settings.desktop_dock.pins.clone();
        let unchanged = serde_json::to_vec(&settings).unwrap();
        Preset::Keep.apply(&mut settings);
        assert_eq!(serde_json::to_vec(&settings).unwrap(), unchanged);
        Preset::Desktop.apply(&mut settings);
        assert!(settings.desktop_dock.enabled && settings.top_panel.enabled);
        assert!(settings.desktop_dock.replace_taskbar && !settings.top_panel.reserve_space);
        settings.desktop_dock.replace_taskbar = false;
        Preset::Desktop.apply(&mut settings);
        assert!(!settings.desktop_dock.replace_taskbar);
        Preset::Island.apply(&mut settings);
        assert!(!settings.desktop_dock.enabled && !settings.top_panel.enabled);
        assert_eq!(settings.desktop_dock.pins, pins);
        assert_eq!(settings.scale_percent, 120);
    }

    #[test]
    fn enabling_dock_in_guide_preserves_default_and_explicit_taskbar_choices() {
        for replace in [true, false] {
            let mut settings = Settings::default();
            settings.desktop_dock.enabled = false;
            settings.desktop_dock.replace_taskbar = replace;
            let mut guide = GuideUi::new(&settings, false);
            guide.toggle_quick(QuickSetting::Dock);
            guide.apply_choices(&mut settings);
            assert!(settings.desktop_dock.enabled);
            assert_eq!(settings.desktop_dock.replace_taskbar, replace);
        }
    }

    #[test]
    fn essentials_override_presets_and_full_access_requires_an_explicit_choice() {
        let mut settings = Settings::default();
        settings.assistant.perms.set(perms::ALL[0], Level::Off);
        let mut guide = GuideUi::new(&settings, false);
        guide.select(Preset::Desktop, false);
        guide.toggle_quick(QuickSetting::Top);
        guide.apply_choices(&mut settings);
        assert!(settings.desktop_dock.enabled);
        assert!(!settings.top_panel.enabled);
        assert!(!settings.assistant.perms.enabled);
        assert_eq!(settings.assistant.perms.configured(perms::ALL[0]), Level::Off);
        guide.select_access(Access::Confirm);
        guide.apply_choices(&mut settings);
        assert!(settings.assistant.perms.enabled);
        assert!(perms::ALL.into_iter().all(|cap| settings.assistant.perms.level(cap) == Level::Ask));
        guide.select_access(Access::Full);
        guide.apply_choices(&mut settings);
        assert!(perms::ALL.into_iter().all(|cap| settings.assistant.perms.level(cap) == Level::Auto));
        assert_eq!(settings.assistant.perms.decide(crate::services::assistant::Cap::Shell, true), Level::Ask);
        guide.select_access(Access::Full);
        guide.apply_choices(&mut settings);
        assert!(perms::ALL.into_iter().all(|cap| settings.assistant.perms.level(cap) == Level::Ask));
        guide.toggle_assistant();
        guide.apply_choices(&mut settings);
        assert!(!settings.assistant.perms.enabled);
    }

    #[test]
    fn voice_setup_autoplays_and_installed_models_are_opt_in() {
        let mut guide = GuideUi::new(&Settings::default(), false);
        guide.choose_mode(GuideMode::Detailed, false);
        guide.move_by(3, true);
        assert!(guide.animating());
        guide.models = [ModelStatus::Installed, ModelStatus::Downloading(50)];
        assert!(!guide.can_download(VoiceModel::Local));
        guide.toggle_model(VoiceModel::Wake);
        assert_eq!(guide.wake_enabled, None);
        guide.connect_model = true;
        guide.connect_speech = true;
        guide.toggle_model(VoiceModel::Wake);
        assert_eq!(guide.wake_enabled, Some(true));
        guide.wake.set_text(" Luna, Taburetka ");
        assert_eq!(guide.validated_wake().as_deref(), Some("Luna, Taburetka"));
        guide.wake.set_text("Luna, luna");
        assert!(guide.validated_wake().is_none());
        guide.move_by(1, false);
        assert!(!guide.animating());
    }
}
