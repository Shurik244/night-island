use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::Result;

use crate::data::clips::{Clip, ShelfItem};
use crate::data::settings::{BatteryStyle, Language, Placement, RecordFormat, Settings, Style};
use crate::platform::media::Media;
use crate::platform::privacy::Privacy;
use crate::platform::system::Radios;
use crate::services::weather::Sky;
use crate::tools::cmd;
use crate::tools::edit::{self, TextEdit};
use crate::tools::texttools::ops;
use crate::ui::gfx::{Painter, WHITE, rgba};

mod activity;
mod agy;
mod chat;
mod clipboard;
mod command;
mod companion_popup;
mod controls;
mod discord;
mod frame;
mod guide;
mod home;
mod home_widgets;
mod integrations;
mod live;
mod memory_tools;
mod metrics;
mod mixer;
mod music_intro;
mod note;
mod notifications;
mod pet_studio;
mod pill;
mod pill_vertical;
mod privacy;
mod record;
mod remind;
mod reorder;
mod settings;
mod settings_window;
mod steam;
mod text;
mod tips;
mod wave_theme;
mod weather;
mod widgets;
mod yandex;

pub use self::reorder::Reorder;
pub use self::tips::content_tip;
pub use self::{
    activity::*, agy::*, chat::*, clipboard::*, command::*, companion_popup::*, controls::*, discord::*, frame::*,
    home::*, integrations::*, metrics::*, mixer::*, music_intro::*, note::*, notifications::*, pill::*,
    pill_vertical::*, privacy::*, record::*, remind::*, settings::*, settings_window::*, steam::*, text::*, weather::*,
    widgets::*, yandex::*,
};
pub use guide::*;
pub use home_widgets::*;
pub use live::*;
pub use memory_tools::*;
pub use pet_studio::*;

pub type Rect = (f32, f32, f32, f32);
pub use wave_theme::{WaveColours, wave_colour};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Clipboard,
    Text,
    Note,
    Weather,
    Record,
    Agy,
    Chat,
    Ai,
    Privacy,
    Activity,
    Steam,
    Controls,
    Mixer,
    Notifications,
    Yandex,
    Settings,
}

pub const TABS: [(Tab, char, &str); 17] = [
    (Tab::Home, '\u{E80F}', "Home"),
    (Tab::Clipboard, '\u{E77F}', "Clipboard"),
    (Tab::Text, '\u{E8E9}', "Text"),
    (Tab::Note, '\u{E70B}', "Note"),
    (Tab::Weather, '\u{F800}', "Weather"),
    (Tab::Record, '\u{E7C8}', "Record"),
    (Tab::Agy, '\u{E943}', "AGY"),
    (Tab::Chat, '\u{E8F2}', "Chat"),
    (Tab::Ai, '\u{E99A}', "AI"),
    (Tab::Privacy, '\u{EA18}', "Privacy"),
    (Tab::Activity, '\u{E823}', "Activity"),
    (Tab::Steam, '\u{E7FC}', "Steam"),
    (Tab::Controls, '\u{E9E9}', "Controls"),
    (Tab::Mixer, '\u{E9E9}', "App mixer"),
    (Tab::Notifications, '\u{EA8F}', "Notifications"),
    (Tab::Yandex, crate::ui::icons::YANDEX_MUSIC_GLYPH, "My Wave"),
    (Tab::Settings, '\u{E713}', "Settings"),
];

pub type SubTabs = [(Tab, char, &'static str); 2];
pub const SUBTABS: [(Tab, SubTabs); 1] = [(Tab::Ai, [(Tab::Chat, '\u{E8F2}', "Chat"), (Tab::Agy, '\u{E943}', "AGY")])];

pub const SUB_H: f32 = 26.0;

pub fn body_rect(pill: Rect, scale: f32, tab: Tab) -> Rect {
    if bar_tab(tab) != tab { (pill.0, pill.1 + SUB_H * scale, pill.2, pill.3 - SUB_H * scale) } else { pill }
}

pub fn bar_tab(tab: Tab) -> Tab {
    SUBTABS.iter().find(|(_, subs)| subs.iter().any(|s| s.0 == tab)).map_or(tab, |g| g.0)
}

pub fn tab_from_name(name: &str) -> Option<Tab> {
    TABS.iter().find(|t| t.2 == name).map(|t| t.0)
}

pub fn tab_name(tab: Tab) -> &'static str {
    TABS.iter().find(|t| t.0 == tab).map_or("Home", |t| t.2)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClipHit {
    Row(usize),
    Pin(usize),
    Delete(usize),
}

#[derive(Clone, Copy)]
pub struct TabChange {
    pub from: Tab,
    pub start: std::time::Instant,
    pub ms: u32,
    pub dir: f32,
}

pub struct TabDrag {
    pub reorder: Reorder,
    pub items: Vec<usize>,
}

pub struct UiState {
    pub feedback: crate::ui::feedback::Feedback,
    pub pressed_action: Option<usize>,
    pub cover_in_flight: bool,
    pub track_in_flight: bool,
    pub composited_progress: bool,
    pub tab: Tab,
    pub tab_drag: Option<TabDrag>,
    pub tab_press: Option<(usize, i32, i32)>,
    pub tab_change: Option<TabChange>,
    pub hover_tab: Option<usize>,
    pub hover_sub: Option<Tab>,
    pub clip: ClipUi,
    pub note: TextEdit,
    pub remind: RemindUi,
    pub cfg: Settings,
    pub text: TextUi,
    pub editing: bool,
    pub caret_on: bool,
    pub media: Media,
    pub mixer: MixerUi,
    pub notifications: NotificationsUi,
    pub yandex: YandexUi,
    pub lyric: LyricUi,
    pub update: crate::services::update::Status,
    pub notes: NotesUi,
    pub sw: SwUi,
    pub privacy: Privacy,
    pub mic_muted: Option<bool>,
    pub vpn: Option<bool>,
    pub vpn_busy: bool,
    pub notice: Option<(char, String)>,
    pub volume: Option<f32>,
    pub recording_secs: Option<u32>,
    pub downloading: Option<String>,
    pub char_inset: f32,
    pub cmd: CommandUi,
    pub radios: Radios,
    pub dark: bool,
    pub hover_ctl: Option<usize>,
    pub armed: Option<usize>,
    pub pinned: bool,
    pub home: HomeUi,
    pub hover_head: Option<HeadHit>,
    pub hand: bool,
    pub tip: Option<(String, (f32, f32))>,
    pub weather: Option<crate::services::weather::Weather>,
    pub spotify: crate::services::spotify::State,
    pub discord: crate::services::discord::State,
    pub int: IntegrationsUi,
    pub city: CityUi,
    pub agy: AgyUi,
    pub chat: ChatUi,
    pub privacy_log: PrivacyUi,
    pub activity: ActivityUi,
    pub live: Vec<LiveActivity>,
    pub live_changed: Option<std::time::Instant>,
    pub steam: SteamUi,
    pub metrics: MetricsUi,
    pub agents: Vec<crate::services::agents::Session>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            tab: Tab::Home,
            feedback: Default::default(),
            pressed_action: None,
            cover_in_flight: false,
            track_in_flight: false,
            composited_progress: false,
            agy: AgyUi::default(),
            chat: ChatUi::default(),
            privacy_log: PrivacyUi::default(),
            activity: ActivityUi::default(),
            live: Vec::new(),
            live_changed: None,
            steam: SteamUi::default(),
            metrics: MetricsUi::default(),
            agents: Vec::new(),
            cmd: CommandUi::default(),
            remind: RemindUi::default(),
            int: IntegrationsUi::default(),
            text: TextUi::default(),
            clip: ClipUi::default(),
            city: CityUi::default(),
            home: HomeUi::default(),
            mixer: MixerUi::default(),
            notifications: NotificationsUi::default(),
            yandex: YandexUi::default(),
            hover_tab: None,
            hover_sub: None,
            note: TextEdit::default(),
            cfg: Settings::default(),
            editing: false,
            caret_on: true,
            media: Media::default(),
            lyric: LyricUi::default(),
            update: crate::services::update::Status::default(),
            notes: NotesUi::default(),
            sw: SwUi::default(),
            privacy: Privacy::default(),
            mic_muted: None,
            tab_change: None,
            tab_drag: None,
            tab_press: None,
            vpn: None,
            vpn_busy: false,
            notice: None,
            volume: None,
            recording_secs: None,
            downloading: None,
            char_inset: 0.0,
            radios: Radios::default(),
            dark: true,
            hover_ctl: None,
            armed: None,
            pinned: false,
            hover_head: None,
            hand: false,
            tip: None,
            weather: None,
            spotify: crate::services::spotify::State::default(),
            discord: crate::services::discord::State::default(),
        }
    }
}

fn inside(r: Rect, px: f32, py: f32) -> bool {
    px >= r.0 && px < r.2 && py >= r.1 && py < r.3
}

pub(crate) fn rounded_body_contains((left, top, width, height): Rect, radius: f32, point: (f32, f32)) -> bool {
    let x = point.0 - left;
    let y = point.1 - top;
    if !inside((0.0, 0.0, width, height), x, y) {
        return false;
    }
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let dx = x - x.clamp(radius, width - radius);
    let dy = y - y.clamp(radius, height - radius);
    dx * dx + dy * dy <= radius * radius
}

const DIM: f32 = 0.6;

fn acc(alpha: f32) -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    let (r, g, b) = crate::ui::theme::accent();
    rgba(r, g, b, alpha)
}

const GREEN: (f32, f32, f32) = (0.4, 0.85, 0.55);

const RED: (f32, f32, f32) = (1.0, 0.35, 0.35);

impl UiState {
    pub fn action_hover(&self, id: usize) -> f32 {
        self.feedback.sample(id as u64, f32::from(self.home.hover_act == Some(id)), 0.0, 140)
    }

    pub fn action_rect(&self, id: usize, rect: Rect) -> Rect {
        let pressed = self.feedback.sample(100_000 + id as u64, f32::from(self.pressed_action == Some(id)), 0.0, 90);
        let inset = (rect.2 - rect.0).min(rect.3 - rect.1) * 0.035 * pressed;
        (rect.0 + inset, rect.1 + inset, rect.2 - inset, rect.3 - inset)
    }
}
