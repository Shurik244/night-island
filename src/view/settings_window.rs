use crate::data::keys::Hotkey;
use crate::services::{discord, spotify};
use crate::view::*;

pub struct Group {
    pub glyph: char,
    pub name: &'static str,
    pub(in crate::view) rows: &'static [(Sid, &'static str, Kind)],
    pub(in crate::view) custom: Option<fn(&UiState) -> Vec<Item>>,
    pub(in crate::view) sections: &'static [(usize, &'static str)],
}

const GENERAL: [(Sid, &str, Kind); 10] = [
    (Sid::Language, "Language", Kind::Segment(["Русский", "English"])),
    (Sid::Weather, "Weather", Kind::Toggle),
    (Sid::Autostart, "Start with Windows", Kind::Toggle),
    (Sid::GameMode, "Game mode", Kind::Toggle),
    (Sid::AutoHide, "Auto-hide", Kind::Toggle),
    (Sid::LockDrag, "Lock dragging", Kind::Toggle),
    (Sid::Capture, "Hide in screenshots", Kind::Toggle),
    (Sid::CaptureSelf, "Hide island in my captures", Kind::Toggle),
    (Sid::DevTools, "Developer tools", Kind::Toggle),
    (Sid::ActivityTrack, "Track activity (time in programs)", Kind::Toggle),
];

const ISLAND: [(Sid, &str, Kind); 23] = [
    (Sid::HomeWidgets, "Resizable widgets on Home", Kind::Toggle),
    (Sid::HomeWidgetDetails, "Detailed tool widgets", Kind::Toggle),
    (Sid::HomeWeatherWidget, "Weather widget in unused space", Kind::Toggle),
    (Sid::LiveActivities, "Live activities", Kind::Toggle),
    (Sid::LiveMusicPriority, "Activities take priority over track introductions", Kind::Toggle),
    (Sid::LiveReminders, "Timer and reminder countdowns", Kind::Toggle),
    (Sid::LiveRecording, "Recording activity cards", Kind::Toggle),
    (Sid::LiveDownloads, "Download activity cards", Kind::Toggle),
    (Sid::GhostMode, "Hotkey makes the pill a ghost", Kind::Toggle),
    (Sid::GhostOpacity, "Ghost visibility", Kind::Slider),
    (Sid::GhostThrough, "Ghost ignores the mouse", Kind::Toggle),
    (Sid::GhostHold, "Ghost only while held", Kind::Toggle),
    (Sid::HidePillSecs, "Hide the pill for", Kind::Slider),
    (Sid::Placement, "Placement", Kind::Segment(["Floating", "Docked"])),
    (Sid::Edge, "Edge", Kind::Cycle),
    (Sid::Style, "Shape", Kind::Segment(["Flat", "Liquid"])),
    (Sid::Character, "Character", Kind::Cycle),
    (Sid::CharacterDetached, "Release the pet from the island", Kind::Toggle),
    (Sid::PetPerformance, "Pet animation quality", Kind::Cycle),
    (Sid::PetAutonomy, "Pet walks, rests and sleeps on its own", Kind::Toggle),
    (Sid::Size, "Size", Kind::Slider),
    (Sid::OpenDelay, "Open delay", Kind::Slider),
    (Sid::CloseDelay, "Close delay", Kind::Slider),
];

const ANIMATIONS: [(Sid, &str, Kind); 10] = [
    (Sid::IslandAnimations, "Animate island opening and closing", Kind::Toggle),
    (Sid::MusicTrackIntro, "Present each new track", Kind::Toggle),
    (Sid::PetMusicCuriosity, "Pet watches the new track", Kind::Toggle),
    (Sid::PetReturnAnimation, "Island welcomes the returning pet", Kind::Toggle),
    (Sid::AnimCurve, "Animation curve", Kind::Cycle),
    (Sid::TabAnimKind, "Tab change", Kind::Cycle),
    (Sid::TabAnimSpeed, "Tab change speed", Kind::Slider),
    (Sid::OpenSpeed, "Open speed", Kind::Slider),
    (Sid::CloseSpeed, "Close speed", Kind::Slider),
    (Sid::FrameRate, "Frame rate", Kind::Cycle),
];

const APPEARANCE: [(Sid, &str, Kind); 10] = [
    (Sid::InterfaceFont, "Interface font", Kind::Cycle),
    (Sid::LegacyIcons, "Classic interface icons", Kind::Toggle),
    (Sid::HeaderIcons, "Icons beside tab names", Kind::Toggle),
    (Sid::HeaderIconColors, "Colored header icons", Kind::Toggle),
    (Sid::HeaderActiveAccent, "Expressive active tab", Kind::Toggle),
    (Sid::LiquidReach, "Liquid reach", Kind::Slider),
    (Sid::LiquidStrength, "Liquid depth", Kind::Slider),
    (Sid::LiquidSoftness, "Liquid softness", Kind::Slider),
    (Sid::LiquidIdleTravel, "Idle wave travels along the edge", Kind::Toggle),
    (Sid::TextLayout, "Text tools", Kind::Cycle),
];

const PILL: [(Sid, &str, Kind); 23] = [
    (Sid::PillUnifiedLayout, "Balanced pill layout", Kind::Toggle),
    (Sid::VpnPill, "VPN shield and button (Happ)", Kind::Toggle),
    (Sid::PillWeather, "Weather on the pill", Kind::Toggle),
    (Sid::PillWeatherSide, "Weather position", Kind::Cycle),
    (Sid::PillBattery, "Battery on the pill", Kind::Cycle),
    (Sid::MusicEq, "Equalizer", Kind::Toggle),
    (Sid::MusicTitle, "Track title", Kind::Toggle),
    (Sid::MusicCover, "Cover art on the pill", Kind::Toggle),
    (Sid::PreferPlayingMedia, "Prefer the player that is playing", Kind::Toggle),
    (Sid::CoverMode, "Cover shape", Kind::Cycle),
    (Sid::MusicLyrics, "Synced lyrics", Kind::Toggle),
    (Sid::SharedLyrics, "Shared lyrics cache", Kind::Toggle),
    (Sid::MusicProgress, "Song progress strip", Kind::Toggle),
    (Sid::ProgressStyle, "Progress style", Kind::Cycle),
    (Sid::Spikes, "Edge spikes", Kind::Toggle),
    (Sid::EqStyle, "Equalizer shape", Kind::Cycle),
    (Sid::EqSmooth, "Smooth liquid equalizer", Kind::Toggle),
    (Sid::EqResponse, "Liquid equalizer response", Kind::Cycle),
    (Sid::EqStrength, "Equalizer strength", Kind::Slider),
    (Sid::EqGradient, "Equalizer colours", Kind::Cycle),
    (Sid::EqWhenOpen, "Equalizer while the island is open", Kind::Toggle),
    (Sid::BumpTint, "Liquid edge colour while music plays", Kind::Cycle),
    (Sid::SpikeColor, "Spike tips in track colour", Kind::Toggle),
];

const NOTIFICATIONS: [(Sid, &str, Kind); 20] = [
    (Sid::SystemNotifications, "Windows notifications from the island", Kind::Toggle),
    (Sid::NotificationTiles, "Notification preview size", Kind::Cycle),
    (Sid::NotificationOpenApp, "Click a notification to open its app", Kind::Toggle),
    (Sid::NotificationAdaptiveSize, "Size cards to fit their content", Kind::Toggle),
    (Sid::WindowsNotificationsSettings, "Windows notification settings", Kind::Button),
    (Sid::NotifyMic, "Microphone mute", Kind::Toggle),
    (Sid::NotifyDownloads, "Finished downloads", Kind::Toggle),
    (Sid::NotifyCapture, "Screenshots, text and QR", Kind::Toggle),
    (Sid::NotifyRecording, "Screen recording", Kind::Toggle),
    (Sid::NotifyAgy, "AGY", Kind::Toggle),
    (Sid::NotifyIntegrations, "Discord and Spotify", Kind::Toggle),
    (Sid::NotifyErrors, "Errors and limits", Kind::Toggle),
    (Sid::NotifyClipboard, "Copied to the clipboard", Kind::Toggle),
    (Sid::NotifyClipboardText, "Show the copied text", Kind::Toggle),
    (Sid::NotifyLockKeys, "Caps, Num and Scroll Lock", Kind::Toggle),
    (Sid::NotifyBattery, "Charger and low battery", Kind::Toggle),
    (Sid::NotifyUpdate, "New version available", Kind::Toggle),
    (Sid::NotifyPrivacy, "Screen capture by other programs", Kind::Toggle),
    (Sid::NotifyAgents, "Coding agents (waiting, finished)", Kind::Toggle),
    (Sid::NotifySteam, "Steam games (started, ended)", Kind::Toggle),
];

const ACTIVITY: [(Sid, &str, Kind); 1] = [(Sid::ActivityAi, "Sort unknown programs with AI", Kind::Toggle)];

const ABOUT: [(Sid, &str, Kind); 3] = [
    (Sid::UpdateCheck, "Check for updates", Kind::Toggle),
    (Sid::UpdateBeta, "Receive beta versions", Kind::Toggle),
    (Sid::OpenLogs, "Logs folder", Kind::Button),
];

const ROWS_TABS_ONLY: [(Sid, &str, Kind); 14] = [
    (Sid::TabClipboard, "Clipboard", Kind::Toggle),
    (Sid::TabText, "Text", Kind::Toggle),
    (Sid::TabNote, "Note", Kind::Toggle),
    (Sid::TabWeather, "Weather", Kind::Toggle),
    (Sid::TabRecord, "Record", Kind::Toggle),
    (Sid::TabAi, "AI", Kind::Toggle),
    (Sid::TabPrivacy, "Privacy", Kind::Toggle),
    (Sid::TabActivity, "Activity", Kind::Toggle),
    (Sid::TabSteam, "Steam", Kind::Toggle),
    (Sid::TabSystem, "System", Kind::Toggle),
    (Sid::TabControls, "Controls", Kind::Toggle),
    (Sid::TabMixer, "App mixer", Kind::Toggle),
    (Sid::TabNotifications, "Notifications", Kind::Toggle),
    (Sid::TabYandex, "My Wave", Kind::Toggle),
];

pub const SIDEBAR_SECTIONS: [(usize, &str); 5] =
    [(0, "Application"), (7, "Look and feel"), (12, "Monitoring"), (15, "Services"), (18, "Program")];

pub const GROUPS: [Group; 19] = [
    Group {
        glyph: '\u{E713}',
        name: "General",
        rows: &GENERAL,
        custom: None,
        sections: &[(0, "Application"), (3, "Behaviour"), (6, "Privacy"), (8, "Advanced")],
    },
    Group { glyph: '\u{E7FC}', name: "Game mode", rows: &[], custom: Some(game_mode_items), sections: &[] },
    Group {
        glyph: '\u{E80F}',
        name: "Island",
        rows: &ISLAND,
        custom: None,
        sections: &[
            (0, "Widgets"),
            (3, "Live activities"),
            (8, "Ghost mode"),
            (13, "Placement and look"),
            (21, "Open and close"),
        ],
    },
    Group { glyph: '\u{E7F4}', name: "Desktop dock", rows: &[], custom: Some(desktop_dock_items), sections: &[] },
    Group { glyph: '\u{E8A9}', name: "Top panel", rows: &[], custom: Some(top_panel_items), sections: &[] },
    Group { glyph: '\u{E71D}', name: "Tabs", rows: &ROWS_TABS_ONLY, custom: None, sections: &[] },
    Group { glyph: '\u{E765}', name: "Keys", rows: &[], custom: Some(key_items), sections: &[] },
    Group {
        glyph: '\u{E790}',
        name: "Appearance",
        rows: &APPEARANCE,
        custom: None,
        sections: &[(0, "Interface"), (2, "Liquid shape"), (5, "Text tab")],
    },
    Group { glyph: '\u{EB52}', name: "Theme", rows: &THEME_ROWS, custom: Some(theme_items), sections: &[] },
    Group {
        glyph: '\u{E9D9}',
        name: "Animations",
        rows: &ANIMATIONS,
        custom: None,
        sections: &[(0, "Curve"), (1, "Tabs"), (3, "Island")],
    },
    Group {
        glyph: '\u{E8D6}',
        name: "Pill and music",
        rows: &PILL,
        custom: Some(pill_items),
        sections: &[(0, "Pill"), (5, "Music"), (14, "Edges")],
    },
    Group { glyph: '\u{E767}', name: "Sounds", rows: &[], custom: Some(sound_items), sections: &[] },
    Group {
        glyph: '\u{EA8F}',
        name: "Notifications",
        rows: &NOTIFICATIONS,
        custom: None,
        sections: &[(0, "System"), (11, "Clipboard and keys"), (14, "Apps and services")],
    },
    Group { glyph: '\u{E950}', name: "System", rows: &[], custom: Some(metrics_items), sections: &[] },
    Group { glyph: '\u{E823}', name: "Activity", rows: &ACTIVITY, custom: Some(activity_items), sections: &[] },
    Group { glyph: '\u{E71B}', name: "Integrations", rows: &[], custom: Some(integration_items), sections: &[] },
    Group { glyph: '\u{E720}', name: "Assistant", rows: &[], custom: Some(assistant_items), sections: &[] },
    Group {
        glyph: '\u{E756}',
        name: "Agents",
        rows: &[(Sid::AgyAuto, "AGY auto-approve", Kind::Toggle)],
        custom: Some(agent_items),
        sections: &[],
    },
    Group { glyph: '\u{E946}', name: "About", rows: &ABOUT, custom: Some(about_items), sections: &[] },
];

pub fn tab_settings_group(tab: crate::view::Tab) -> Option<usize> {
    use crate::view::Tab;
    let name = match tab {
        Tab::Home => "Pill and music",
        Tab::Clipboard => "Notifications",
        Tab::Text => "Appearance",
        Tab::Weather => "General",
        Tab::Activity => "Activity",
        Tab::Controls => "System",
        Tab::Chat | Tab::Ai => "Assistant",
        Tab::Agy => "Agents",
        Tab::Steam => "Integrations",
        _ => return None,
    };
    GROUPS.iter().position(|g| g.name == name)
}

pub const SW_SIZE: (f32, f32) = (940.0, 680.0);

const SIDEBAR_W: f32 = 220.0;
const PAD: f32 = 28.0;
const SEARCH_H: f32 = 38.0;
const HEADING_H: f32 = 44.0;
const SECTION_H: f32 = 46.0;
const THEME_W: f32 = 420.0;
const THEME_H: f32 = 268.0;
const TITLE_H: f32 = 40.0;
const TITLE_BTN_W: f32 = 46.0;
const ROW_H: f32 = 46.0;
const SERVICE_H: f32 = 56.0;
const NOTE_H: f32 = 19.0;
const CTL_W: f32 = 300.0;
const GROUP_H: f32 = 25.0;
const SIDE_SECTION_H: f32 = 22.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    DiscordId,
    DiscordSecret,
    SpotifyId,
    AiKey,
    AiModel,
    AiBase,
    VoiceKey,
    VoiceModel,
    VoiceBase,
    TtsKey,
    TtsBase,
    TtsModel,
    TtsVoice,
    WakeWord,
    GuideWake,
    GroupName,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    SoundImport(usize),
    SoundPreview(usize),
    SoundReset(usize),
    SoundResetAll,
    FontImport,
    FontDownload,
    YandexMain,
    YandexWave,
    YandexBrowser,
    YandexCopy,
    YandexAutoplay,
    YandexCoverCache,
    DiscordMain,
    DiscordMute,
    DiscordDeaf,
    DiscordOverlay,
    SpotifyMain,
    SpotifySaveId,
    AiSave,
    AiCheck,
    GameAlwaysShow,
    GameTreatAsGame,
    GameAutomatic,
    GameAiSettings,
    GameRemoveRule(usize),
    AiRemoveKey,
    VoiceSave,
    VoiceRemoveKey,
    TtsSave,
    TtsRemoveKey,
    WakeSave,
    GroupAdd,
    GroupDelete(i64),
    UpdateCheck,
    UpdateDownload,
    UpdateInstall,
    Rebind(Hotkey),
    KeysReset,
    LyricsAppAdd,
    LyricsAppRemove(usize),
    DockRemove(usize),
    DockHiddenRemove(usize),
    Metric(usize),
    Agent(usize),
    OpenGuide,
}

#[derive(Default)]
pub struct SwUi {
    pub dock_status: String,
    pub font_status: String,
    pub sound_status: String,
    pub sound_busy: bool,
    pub font_busy: bool,
    pub memory: MemoryUi,
    pub guide: Option<GuideUi>,
    pub group: usize,
    pub assistant_page: AssistantPage,
    pub scroll: f32,
    pub search: String,
    pub hover: Option<SwHit>,
    pub plates: Vec<(Plate, f32)>,
    pub drag: Option<Sid>,
    pub field: Option<Field>,
    pub dropdown: Option<Sid>,
    pub caret_on: bool,
    pub search_press: bool,
    pub capturing: Option<Hotkey>,
    pub quiet_sys: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Plate {
    Row(Sid),
    Field(Field),
    Act(Act),
}

impl Plate {
    pub fn of(hit: Option<SwHit>) -> Option<Plate> {
        match hit? {
            SwHit::Row(id, _) => Some(Plate::Row(id)),
            SwHit::Field(f) => Some(Plate::Field(f)),
            SwHit::Act(a) => Some(Plate::Act(a)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SwHit {
    Pick(Sid, usize),
    Theme(ThemeHit),
    Minimize,
    Close,
    Group(usize),
    AssistantPage(AssistantPage),
    Guide(GuideAction),
    Memory(MemoryAction),
    Row(Sid, SetHit),
    Field(Field),
    Act(Act),
}

pub(in crate::view) enum Item {
    Heading(&'static str),
    Section(&'static str),
    ThemeBlock,
    FontPreview,
    Row(Sid, &'static str, Kind),
    Service { name: &'static str, glyph: char, status: String, live: bool },
    Field(Field, &'static str, &'static str),
    Button(Act, String, bool),
    Pair(Act, String, String, bool),
    Note(&'static str),
    Text(String),
    Message(String, bool),
}

impl Item {
    fn height(&self) -> f32 {
        match self {
            Item::Heading(_) => HEADING_H,
            Item::Section(_) => SECTION_H,
            Item::ThemeBlock => THEME_H,
            Item::FontPreview => 136.0,
            Item::Row(..) | Item::Field(..) | Item::Button(..) | Item::Pair(..) => ROW_H,
            Item::Service { .. } => SERVICE_H,
            Item::Note(_) | Item::Text(_) => NOTE_H,
            Item::Message(..) => 70.0,
        }
    }
}

fn discord_status_text(d: &discord::State) -> (String, bool) {
    use discord::Status as D;
    let text = match &d.status {
        D::NoConfig => "Needs a Client ID".to_string(),
        D::Idle => "Not connected".to_string(),
        D::NotRunning => "Not running".to_string(),
        D::Authorizing => "Approve in Discord…".to_string(),
        D::Connected if d.channel.is_empty() => "Not in a voice channel".to_string(),
        D::Connected => d.channel.clone(),
        D::Error(why) => why.clone(),
    };
    (text, d.status == D::Connected)
}

fn spotify_status_text(s: &spotify::State) -> (String, bool) {
    use spotify::Status as S;
    let text = match &s.status {
        S::NoClientId => "Needs a Client ID".to_string(),
        S::Disconnected => "Not connected".to_string(),
        S::Connecting => "Waiting for the browser…".to_string(),
        S::Connected => "Connected".to_string(),
        S::Error(why) => why.clone(),
    };
    (text, s.status == S::Connected)
}

fn game_mode_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![
        Item::Heading("Game mode"),
        Item::Row(Sid::GameMode, "Game mode", Kind::Toggle),
        Item::Row(Sid::GameAi, "Learn unknown apps with AI", Kind::Toggle),
        Item::Row(Sid::GameWindowed, "Hide in windowed games too", Kind::Toggle),
        Item::Note("AI sees only the executable name and product description."),
        Item::Note("No window titles, screenshots or full paths are sent."),
        Item::Note("Unknown apps stay visible until classified. Desktop and Island windows are never games."),
    ];
    if ui.cfg.game_ai && !crate::services::ai::has_key(ui.cfg.ai.provider) {
        out.push(Item::Note("Without an AI key, built-in rules and your own rules still work."));
        out.push(Item::Button(Act::GameAiSettings, "Set up the AI model".into(), true));
    }
    if let Some((name, status)) = crate::platform::game_mode::last_status() {
        out.push(Item::Section("Last active app"));
        out.push(Item::Text(name));
        out.push(Item::Text(crate::ui::i18n::tr(&status).into_owned()));
        out.push(Item::Button(Act::GameAlwaysShow, "Always show the island in this app".into(), true));
        out.push(Item::Button(Act::GameTreatAsGame, "Treat this app as a game".into(), true));
        out.push(Item::Button(Act::GameAutomatic, "Recognize this app automatically".into(), true));
    }
    let rules = crate::platform::game_mode::rules();
    if !rules.is_empty() {
        out.push(Item::Section("Your app rules"));
    }
    for (index, (_, label)) in rules.into_iter().enumerate() {
        out.push(Item::Pair(Act::GameRemoveRule(index), label, "Remove rule".into(), false));
    }
    out
}

fn integration_items(ui: &UiState) -> Vec<Item> {
    use discord::Status as D;
    let mut out = vec![Item::Heading("Integrations")];

    let wave = &ui.yandex.state;
    let status = if !wave.error.is_empty() {
        wave.error.clone()
    } else if !wave.code.is_empty() {
        crate::ui::i18n::tr("Waiting for the browser…").into_owned()
    } else if wave.busy {
        crate::ui::i18n::tr("Connecting…").into_owned()
    } else if wave.has_token {
        wave.account.clone()
    } else {
        crate::ui::i18n::tr("Not connected").into_owned()
    };
    out.push(Item::Service {
        name: "Yandex Music",
        glyph: crate::ui::icons::YANDEX_MUSIC_GLYPH,
        status,
        live: wave.has_token,
    });
    if !wave.code.is_empty() {
        out.push(Item::Text(format!("{}: {}", crate::ui::i18n::tr("Code"), wave.code)));
        out.push(Item::Button(Act::YandexCopy, if ui.yandex.copied { "Copied" } else { "Copy code" }.into(), true));
        out.push(Item::Button(Act::YandexBrowser, "Open sign-in page".into(), true));
    }
    let label = if wave.has_token {
        "Disconnect"
    } else if wave.busy || !wave.code.is_empty() {
        "Cancel"
    } else {
        "Connect"
    };
    out.push(Item::Button(Act::YandexMain, label.into(), true));
    if wave.has_token {
        out.push(Item::Pair(
            Act::YandexAutoplay,
            "Start My Wave with the island".into(),
            if wave.config.autoplay { "On" } else { "Off" }.into(),
            wave.config.autoplay,
        ));
    }
    out.push(Item::Button(Act::YandexWave, "Open My Wave".into(), true));
    out.push(Item::Pair(
        Act::YandexCoverCache,
        "Keep music covers on this computer".into(),
        if wave.config.cache_covers { "On" } else { "Off" }.into(),
        wave.config.cache_covers,
    ));
    out.push(Item::Row(Sid::WaveLegacyLayout, "Original My Wave layout", Kind::Toggle));
    out.push(Item::Row(Sid::WaveRatings, "Liked tracks and ratings in My Wave", Kind::Toggle));
    out.push(Item::Row(Sid::WaveSearch, "Track search in My Wave", Kind::Toggle));

    let (status, live) = discord_status_text(&ui.discord);
    out.push(Item::Service { name: "Discord", glyph: crate::ui::icons::DISCORD_GLYPH, status, live });
    match &ui.discord.status {
        D::NoConfig => {
            out.push(Item::Field(Field::DiscordId, "Client ID", "Client ID"));
            out.push(Item::Field(Field::DiscordSecret, "Client Secret", "Client Secret"));
            out.push(Item::Note("discord.com/developers: New Application, OAuth2: Redirect http://127.0.0.1,"));
            out.push(Item::Note("App Testers: add yourself. The credentials stay on this computer."));
            out.push(Item::Button(Act::DiscordMain, "Save".into(), discord_fields_ok(ui)));
        }
        D::Idle | D::Error(_) => {
            out.push(Item::Note("Discord must be running; you will see an Authorize prompt inside it."));
            out.push(Item::Button(Act::DiscordMain, "Connect".into(), true));
        }
        D::Authorizing => out.push(Item::Button(Act::DiscordMain, "Approve in Discord…".into(), false)),
        D::NotRunning => out.push(Item::Button(Act::DiscordMain, "Disconnect".into(), true)),
        D::Connected => {
            let mute = if ui.discord.self_mute { "Unmute the microphone" } else { "Mute the microphone" };
            let deaf = if ui.discord.self_deaf { "Undeafen" } else { "Deafen" };
            let overlay = if ui.cfg.discord_overlay { "Hide the call overlay" } else { "Show the call overlay" };
            out.push(Item::Button(Act::DiscordMute, mute.into(), ui.discord.voice_available));
            out.push(Item::Button(Act::DiscordDeaf, deaf.into(), ui.discord.voice_available));
            if !ui.discord.voice_available {
                out.push(Item::Note(if ui.discord.voice_error.is_some() {
                    "Discord voice controls unavailable"
                } else {
                    "Waiting for Discord voice settings"
                }));
            }
            out.push(Item::Button(Act::DiscordOverlay, overlay.into(), true));
            out.push(Item::Button(Act::DiscordMain, "Disconnect".into(), true));
        }
    }

    let (status, live) = spotify_status_text(&ui.spotify);
    out.push(Item::Service { name: "Spotify", glyph: crate::ui::icons::SPOTIFY_GLYPH, status, live });
    out.push(Item::Note("Shuffle, repeat and likes for the playing track."));
    match &ui.spotify.status {
        spotify::Status::Connecting => {
            out.push(Item::Button(Act::SpotifyMain, "Waiting for the browser…".into(), false));
        }
        spotify::Status::Connected => out.push(Item::Button(Act::SpotifyMain, "Disconnect".into(), true)),
        _ => out.push(Item::Button(Act::SpotifyMain, "Connect".into(), true)),
    }
    out.push(Item::Field(Field::SpotifyId, "Own Client ID", "Optional: your Spotify app"));
    out.push(Item::Button(Act::SpotifySaveId, "Save the Client ID".into(), true));
    out
}

fn agent_items(ui: &UiState) -> Vec<Item> {
    use crate::services::agents::{self, Agent, State};
    let mut out = vec![Item::Heading("Coding agents"), Item::Row(Sid::AgyAuto, "AGY auto-approve", Kind::Toggle)];
    out.push(Item::Note("The island shows when an agent works, waits for you or finishes."));
    out.push(Item::Note("Nothing is changed in their settings until you connect."));
    for (i, agent) in Agent::ALL.into_iter().enumerate() {
        let state = if agents::connected(agent) { "Disconnect" } else { "Connect" };
        out.push(Item::Pair(Act::Agent(i), agent.name().into(), state.into(), false));
    }
    out.push(Item::Note("Claude Code, Gemini CLI, Cursor, Copilot CLI: hooks in their settings files."));
    out.push(Item::Note("Codex CLI: its notify command (finished only); a command it already has keeps running."));
    out.push(Item::Note("Perplexity has no hooks or command line to connect to."));
    out.push(Item::Section("Sessions"));
    if ui.agents.is_empty() {
        out.push(Item::Note("No sessions heard from yet."));
    }
    let tr = |s: &str| crate::ui::i18n::tr(s).into_owned();
    for s in ui.agents.iter().take(8) {
        let word = match s.state {
            State::Working => tr("working"),
            State::Waiting => tr("waiting for you"),
            State::Done => tr("finished"),
        };
        out.push(Item::Text(format!("{} · {} — {word}", s.agent.name(), s.project)));
    }
    out
}

fn metrics_items(ui: &UiState) -> Vec<Item> {
    use crate::platform::metrics::Metric;
    let mut out = vec![Item::Heading("System"), Item::Row(Sid::MetricsInterval, "Read every", Kind::Slider)];
    out.push(Item::Row(Sid::MemoryTools, "Memory manager", Kind::Toggle));
    if ui.cfg.memory_tools {
        out.push(Item::Row(Sid::MemoryManager, "Free memory and watch applications", Kind::Button));
    }
    out.push(Item::Section("Figures"));
    for (i, m) in Metric::ALL.into_iter().enumerate() {
        let on_tab = ui.cfg.metrics_tab.iter().any(|k| k == m.key());
        let on_pill = ui.cfg.metrics_pill.iter().any(|k| k == m.key());
        let state = match (on_tab, on_pill) {
            (false, false) => "Hidden",
            (true, false) => "On the tab",
            (false, true) => "On the pill",
            (true, true) => "Tab and pill",
        };
        out.push(Item::Pair(Act::Metric(i), m.label().into(), state.into(), false));
    }
    out.push(Item::Note("Click a figure to move it: hidden, the System tab, the pill, or both."));
    out.push(Item::Note("Nothing is read while no figure is on screen."));
    out
}

fn theme_items(_ui: &UiState) -> Vec<Item> {
    let row = |i: usize| {
        let (id, label, kind) = THEME_ROWS[i];
        Item::Row(id, label, kind)
    };
    vec![
        Item::Heading("Theme"),
        Item::Section("Preview and presets"),
        Item::ThemeBlock,
        Item::Section("Panel"),
        row(0),
        row(1),
        row(2),
        row(3),
        row(4),
        Item::Section("Pill"),
        row(5),
        row(6),
        Item::Section("Accent"),
        row(7),
        row(8),
    ]
}

fn theme_col(row: Rect, scale: f32) -> (f32, f32, f32) {
    (row.0, row.1 + 4.0 * scale, row.0 + THEME_W * scale)
}

const PRESET_COLS: usize = 4;

fn theme_preset_tile(col: (f32, f32, f32), scale: f32, i: usize) -> Rect {
    let (l, t, r) = col;
    let gap = 6.0 * scale;
    let w = (r - l - (PRESET_COLS - 1) as f32 * gap) / PRESET_COLS as f32;
    let (c, row) = ((i % PRESET_COLS) as f32, (i / PRESET_COLS) as f32);
    let left = l + c * (w + gap);
    let top = t + 76.0 * scale + row * (30.0 * scale + gap);
    (left, top, left + w, top + 30.0 * scale)
}

fn theme_swatch(col: (f32, f32, f32), scale: f32, row: usize, i: usize) -> Rect {
    let (l, t, r) = col;
    let n = crate::ui::theme::PANEL_SWATCHES.len() as f32;
    let gap = 5.0 * scale;
    let d = ((r - l) - (n - 1.0) * gap) / n;
    let left = l + i as f32 * (d + gap);
    let top = t + (166.0 + row as f32 * 52.0) * scale;
    (left, top, left + d, top + d)
}

fn hit_theme_block(row: Rect, scale: f32, px: f32, py: f32) -> Option<ThemeHit> {
    let col = theme_col(row, scale);
    if let Some(i) = (0..crate::ui::theme::PRESETS.len()).find(|&i| inside(theme_preset_tile(col, scale, i), px, py)) {
        return Some(ThemeHit::Preset(i));
    }
    for i in 0..crate::ui::theme::PANEL_SWATCHES.len() {
        if inside(theme_swatch(col, scale, 0, i), px, py) {
            return Some(ThemeHit::Panel(i));
        }
        if inside(theme_swatch(col, scale, 1, i), px, py) {
            return Some(ThemeHit::Accent(i));
        }
    }
    None
}

fn paint_theme_block(p: &Painter, row: Rect, scale: f32, ui: &UiState, hovered: Option<ThemeHit>) -> Result<()> {
    let col = theme_col(row, scale);
    let (l, t, r) = col;
    let (panel, accent) = crate::ui::theme::colors(&ui.cfg);
    let preview = (l, t, r, t + 58.0 * scale);
    let alpha = ui.cfg.tint_open;
    p.fill_round_rect(preview, 29.0 * scale, rgba(0.85, 0.85, 0.85, 1.0))?;
    p.fill_round_rect((l, t, (l + r) / 2.0, t + 58.0 * scale), 29.0 * scale, rgba(0.35, 0.35, 0.35, 1.0))?;
    p.fill_round_rect(preview, 29.0 * scale, rgba(panel.0, panel.1, panel.2, alpha))?;
    p.stroke_round_rect(preview, 29.0 * scale, scale, rgba(1.0, 1.0, 1.0, 0.12))?;
    p.fill_ellipse(
        l + 28.0 * scale,
        t + 29.0 * scale,
        7.0 * scale,
        7.0 * scale,
        rgba(accent.0, accent.1, accent.2, 1.0),
    )?;
    p.text("12:34", preview, 17.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    for k in 0..4 {
        let h = (8.0 + k as f32 * 4.0) * scale;
        let x = r - 56.0 * scale + k as f32 * 9.0 * scale;
        p.fill_round_rect(
            (x, t + 29.0 * scale - h / 2.0, x + 4.0 * scale, t + 29.0 * scale + h / 2.0),
            2.0 * scale,
            rgba(accent.0, accent.1, accent.2, 1.0),
        )?;
    }
    p.text(
        "Presets",
        (l, t + 58.0 * scale, r, t + 74.0 * scale),
        11.5 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.55),
    )?;
    for (i, (name, _)) in crate::ui::theme::PRESETS.iter().enumerate() {
        let tile = theme_preset_tile(col, scale, i);
        let (pc, ac) = crate::ui::theme::preset_colors(i);
        let hover = hovered == Some(ThemeHit::Preset(i));
        p.fill_round_rect(tile, 10.0 * scale, rgba(pc.0, pc.1, pc.2, 1.0))?;
        p.stroke_round_rect(
            tile,
            10.0 * scale,
            if hover { 1.6 } else { 1.0 } * scale,
            rgba(1.0, 1.0, 1.0, if hover { 0.5 } else { 0.16 }),
        )?;
        p.fill_ellipse(
            tile.0 + 11.0 * scale,
            (tile.1 + tile.3) / 2.0,
            4.5 * scale,
            4.5 * scale,
            rgba(ac.0, ac.1, ac.2, 1.0),
        )?;
        p.text(
            name,
            (tile.0 + 20.0 * scale, tile.1, tile.2 - 2.0 * scale, tile.3),
            10.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
    }
    let near = |a: f32, b: f32, eps: f32| (a - b).abs() <= eps;
    for (line, label, swatches) in
        [(0usize, "Panel", &crate::ui::theme::PANEL_SWATCHES), (1, "Accent", &crate::ui::theme::ACCENT_SWATCHES)]
    {
        let first = theme_swatch(col, scale, line, 0);
        p.text(
            label,
            (l, first.1 - 20.0 * scale, r, first.1 - 4.0 * scale),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.55),
        )?;
        for (i, v) in swatches.iter().enumerate() {
            let rect = theme_swatch(col, scale, line, i);
            let (cr, cg, cb) = crate::ui::theme::hsv(v[0], v[1], v[2]);
            let hover = hovered == Some(if line == 0 { ThemeHit::Panel(i) } else { ThemeHit::Accent(i) });
            p.fill_round_rect(rect, (rect.2 - rect.0) / 2.0, rgba(cr, cg, cb, 1.0))?;
            let current = if line == 0 {
                near(ui.cfg.panel_hue, v[0], 1.0)
                    && near(ui.cfg.panel_sat, v[1], 0.03)
                    && near(ui.cfg.panel_val, v[2], 0.01)
            } else {
                near(ui.cfg.accent_hue, v[0], 1.0)
                    && near(ui.cfg.accent_sat, v[1], 0.03)
                    && near(ui.cfg.accent_val, v[2], 0.03)
            };
            let ring =
                if current { rgba(1.0, 1.0, 1.0, 0.95) } else { rgba(1.0, 1.0, 1.0, if hover { 0.6 } else { 0.22 }) };
            p.stroke_round_rect(rect, (rect.2 - rect.0) / 2.0, if current { 2.0 } else { 1.0 } * scale, ring)?;
        }
    }
    Ok(())
}

fn pill_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![Item::Heading("Pill and music")];
    if let Some(group) = GROUPS.iter().find(|g| g.name == "Pill and music") {
        out.extend(rows_with_sections(group));
    }
    out.push(Item::Note(
        "Calm keeps the previous liquid response. Responsive follows beats faster; Energetic adds more movement.",
    ));
    out.push(Item::Note("Turn off balanced layout or smooth equalizer to use their classic appearance."));
    out.push(Item::Note("Player priority: pause My Wave and start a video to control the browser from Home. My Wave keeps its own controls."));
    out.push(Item::Section("Players for lyrics"));
    let apps = &ui.cfg.lyrics_apps;
    if apps.is_empty() {
        out.push(Item::Note("Any player. Add one to take lyrics only from the players you trust."));
    } else {
        out.push(Item::Note("Lyrics are looked up only while one of these plays."));
    }
    for (i, app) in apps.iter().enumerate() {
        out.push(Item::Pair(Act::LyricsAppRemove(i), app.clone(), "Remove".into(), false));
    }
    let source = &ui.media.source;
    let can_add = !source.is_empty() && !apps.iter().any(|a| a.eq_ignore_ascii_case(source));
    let label = if source.is_empty() {
        "Add the player (start playing first)".into()
    } else {
        format!("Add the player {source}")
    };
    out.push(Item::Button(Act::LyricsAppAdd, label, can_add));
    out
}

fn hotkey_item(ui: &UiState, hotkey: Hotkey) -> Item {
    let capturing = ui.sw.capturing == Some(hotkey);
    let text = if capturing {
        "Press the new keys (Esc clears)…".to_string()
    } else {
        crate::ui::i18n::tr(&ui.cfg.combo(hotkey).text()).into_owned()
    };
    Item::Pair(Act::Rebind(hotkey), hotkey.label().into(), text, capturing)
}

fn key_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![Item::Heading("Keys")];
    out.extend(Hotkey::ALL.into_iter().filter(|h| *h != Hotkey::Dictate).map(|h| hotkey_item(ui, h)));
    out.push(Item::Note("Click a key, then press the new combination with Ctrl, Alt, Shift or Win."));
    out.push(Item::Note("Esc cancels, Delete brings the default back."));
    out.push(Item::Button(Act::KeysReset, "Reset all keys".into(), !ui.cfg.keys.is_empty()));
    out
}

fn about_items(ui: &UiState) -> Vec<Item> {
    use crate::services::update::{self, Status as U};
    let mut out = vec![
        Item::Heading("About"),
        Item::Text(format!("Night Island {}", update::CURRENT)),
        Item::Button(Act::OpenGuide, "Open quick tour".into(), true),
    ];
    let line = match &ui.update {
        U::Unknown | U::NotConfigured if !update::configured() => {
            "Updates are off: this build has no release key".to_string()
        }
        U::Unknown | U::NotConfigured => "Not checked yet".to_string(),
        U::Checking => "Checking…".to_string(),
        U::UpToDate => "You have the latest version".to_string(),
        U::Available(f) => format!("Version {} is available", f.version),
        U::Downloading(f) => format!("Downloading version {}…", f.version),
        U::Ready(f, _) => format!("Version {} is downloaded and checked", f.version),
        U::Failed(why) => why.clone(),
    };
    out.push(Item::Text(line));
    let busy = matches!(ui.update, U::Checking | U::Downloading(_));
    out.push(Item::Button(Act::UpdateCheck, "Check now".into(), update::configured() && !busy));
    match &ui.update {
        U::Available(f) => {
            if let Some(first) = f.notes.lines().next().filter(|l| !l.trim().is_empty()) {
                out.push(Item::Text(first.chars().take(110).collect()));
            }
            out.push(Item::Button(Act::UpdateDownload, format!("Download version {}", f.version), true));
        }
        U::Ready(f, _) => {
            out.push(Item::Button(Act::UpdateInstall, format!("Install version {} and restart", f.version), true));
        }
        _ => {}
    }
    for &(id, label, kind) in &ABOUT {
        out.push(Item::Row(id, label, kind));
        if id == Sid::CharacterDetached {
            out.push(Item::Note("Click for mini AI chat, right-click to hide/show the island. Drag to move."));
        }
    }
    out
}

fn activity_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![Item::Heading("Activity")];
    for &(id, label, kind) in &ACTIVITY {
        out.push(Item::Row(id, label, kind));
    }
    out.push(Item::Note("Off by default. Programs are grouped by a built-in table first, and by your choices."));
    out.push(Item::Note("On: the file names of programs nothing else knows (nothing more) go to your AI service."));
    out.push(Item::Text(format!(
        "{} {} / {}",
        crate::ui::i18n::tr("AI requests today:"),
        ui.activity.ai_used,
        crate::services::ai::SORT_REQUESTS_PER_DAY
    )));
    out.push(Item::Heading("Groups"));
    for group in &ui.activity.groups {
        if group.builtin {
            out.push(Item::Text(group.name.clone()));
        } else {
            out.push(Item::Pair(Act::GroupDelete(group.id), group.name.clone(), "Delete".into(), false));
        }
    }
    out.push(Item::Field(Field::GroupName, "New group", "Name"));
    out.push(Item::Button(Act::GroupAdd, "Add group".into(), ui.activity.groups.len() < 12));
    out.push(Item::Note("Open a group in the Activity tab and click a program to move it to the next group."));
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssistantPage {
    #[default]
    Overview,
    Model,
    Recognition,
    Voice,
    Permissions,
    Advanced,
}

impl AssistantPage {
    const ALL: [(Self, &'static str); 6] = [
        (Self::Overview, "Assistant overview"),
        (Self::Model, "AI model"),
        (Self::Recognition, "Speech recognition"),
        (Self::Voice, "Voice"),
        (Self::Permissions, "Assistant permissions"),
        (Self::Advanced, "Advanced"),
    ];
}

fn assistant_items(ui: &UiState) -> Vec<Item> {
    assistant_page_items(ui, ui.sw.assistant_page)
}

fn assistant_page_items(ui: &UiState, page: AssistantPage) -> Vec<Item> {
    use AssistantPage::*;
    use Kind::{Button, Slider, Toggle};
    match page {
        Overview => vec![
            Item::Heading("Assistant"),
            Item::Row(Sid::AsstEnabled, "Voice assistant", Toggle),
            Item::Row(Sid::AsstFollowUp, "Keep listening after an answer", Toggle),
            Item::Section("Assistant replies"),
            Item::Row(Sid::NotifyAi, "AI answers", Toggle),
            Item::Row(Sid::AiNotificationCards, "Show AI answers as cards", Toggle),
            Item::Row(Sid::AiCompactReplies, "Short AI replies inside the island", Toggle),
            Item::Note("Long replies use cards. Detached pet replies stay beside the pet."),
            Item::Section("Desktop pet"),
            Item::Row(Sid::PetVoice, "Pet reacts to voice assistant", Toggle),
            Item::Row(Sid::PetVoiceInput, "Talk to the detached pet", Toggle),
            Item::Row(Sid::PetNotifications, "Show cards by the detached pet", Toggle),
        ],
        Model => assistant_model_items(ui),
        Recognition => assistant_recognition_items(ui),
        Voice => assistant_voice_items(ui),
        Permissions => {
            let permission = |id, label| Item::Row(id, label, Kind::Cycle);
            vec![
                Item::Heading("What it may do"),
                permission(Sid::AsstMedia, "Music and media keys"),
                Item::Row(Sid::AsstYandex, "Voice commands for My Wave", Toggle),
                permission(Sid::AsstControls, "Assistant system controls"),
                Item::Note("Volume, mic, Wi-Fi, Bluetooth, theme, lock"),
                permission(Sid::AsstOpen, "Open programs and games"),
                permission(Sid::AsstReminders, "Reminders and timers"),
                permission(Sid::AsstMemory, "Remember things about you"),
                permission(Sid::AsstWebSearch, "Search the internet"),
                permission(Sid::AsstFilesRead, "Read and search files"),
                permission(Sid::AsstFilesWrite, "Create and change files"),
                permission(Sid::AsstShell, "Command line"),
                permission(Sid::AsstBrowser, "Browser (open sites, read pages)"),
                permission(Sid::AsstPower, "Shut down, restart, sleep"),
                permission(Sid::AsstScreen, "View screen with an image-capable model"),
                permission(Sid::AsstIsland, "Read Island data"),
            ]
        }
        Advanced => vec![
            Item::Heading("How it works"),
            Item::Row(Sid::AsstInstructions, "Edit assistant instructions", Button),
            Item::Row(Sid::AsstReview, "Review commands before confirmation", Toggle),
            Item::Row(Sid::AsstWebAsk, "Ask first after reading web pages", Toggle),
            Item::Section("Request limits"),
            Item::Row(Sid::AsstSteps, "Steps per request", Slider),
            Item::Row(Sid::AsstTokens, "Tokens per step", Slider),
            Item::Row(Sid::AsstSeconds, "Command time limit in seconds", Slider),
            Item::Row(Sid::AsstOutput, "Characters of command output kept", Slider),
        ],
    }
}

fn assistant_model_items(ui: &UiState) -> Vec<Item> {
    use crate::data::settings::AiProvider;
    let provider = ui.cfg.ai.provider;
    let saved = ui.int.ai_key_saved;
    let mut out = vec![Item::Heading("AI model"), Item::Row(Sid::AiProvider, "Service", Kind::Cycle)];
    let matches = ui.int.ai_check_key == crate::services::ai::connection_key(&ui.cfg.ai);
    let checked = matches && matches!(ui.int.ai_check, Some(Ok(())));
    let status = if matches && ui.int.ai_check_pending {
        "Checking connection…"
    } else if checked {
        "Connection verified"
    } else if saved {
        "Key saved; connection not verified"
    } else {
        "No key yet"
    };
    out.push(Item::Service { name: provider.name(), glyph: '\u{E99A}', status: status.into(), live: checked });
    out.push(Item::Field(
        Field::AiKey,
        "API key",
        if saved { "Saved: type a new one to replace it" } else { "Paste the key here" },
    ));
    out.push(Item::Field(Field::AiModel, "Model", provider.default_model()));
    if provider == AiProvider::OpenAi {
        out.push(Item::Field(Field::AiBase, "Server address", "https://api.openai.com/v1"));
    }
    out.push(Item::Row(Sid::AiCheckOnSave, "Test the connection after saving", Kind::Toggle));
    out.push(Item::Button(Act::AiSave, if ui.cfg.ai_check_on_save { "Save and check" } else { "Save" }.into(), true));
    if ui.int.ai_check_key == crate::services::ai::connection_key(&ui.cfg.ai) {
        if ui.int.ai_check_pending {
            out.push(Item::Text("Checking key, model and server…".into()));
        } else if let Some(result) = &ui.int.ai_check {
            out.push(Item::Message(
                match result {
                    Ok(()) => "Connection verified. The model answered.".into(),
                    Err(why) => why.clone(),
                },
                result.is_ok(),
            ));
        }
    }
    out.push(Item::Note("Checking sends one small test request to the selected model."));
    if saved {
        out.push(Item::Button(Act::AiCheck, "Check connection".into(), !(ui.int.ai_check_pending && matches)));
    }

    if saved {
        out.push(Item::Button(Act::AiRemoveKey, "Remove the saved key".into(), true));
    }
    out.push(Item::Note("Ask in the command bar (Alt+Space): start with ?, for example  ? what is DPAPI"));
    out.push(Item::Note("The key is stored encrypted for your Windows account and is never shown again."));
    out.push(Item::Note("OpenAI-compatible: OpenAI, OpenRouter, or a local server such as LM Studio."));
    out
}

fn assistant_recognition_items(ui: &UiState) -> Vec<Item> {
    let from_guide = ui.sw.guide.as_ref().is_some_and(|guide| !guide.visible && guide.separate_speech());
    let mut out = vec![Item::Heading("Speech recognition")];
    if !from_guide {
        out.extend([
            Item::Row(Sid::AsstWake, "Listen for the call word", Kind::Toggle),
            Item::Field(Field::WakeWord, "Voice command name", "Island, Айленд"),
            Item::Button(Act::WakeSave, "Save command name".into(), true),
            Item::Row(Sid::AsstInstall, "Download the wake-word model (46 MB)", Kind::Button),
            Item::Section("Dictation"),
            hotkey_item(ui, Hotkey::Dictate),
            Item::Row(Sid::VoicePaste, "Paste dictated text", Kind::Toggle),
        ]);
    }
    out.push(Item::Section("Speech service"));
    if !from_guide {
        out.push(Item::Row(Sid::VoiceSeparate, "Separate speech service", Kind::Toggle));
    } else if !ui.cfg.ai.voice_separate {
        out.push(Item::Note("Finish setup to activate the separate speech service."));
    }
    if !from_guide && !ui.cfg.ai.voice_separate {
        out.push(Item::Note("Uses the model service. Enable a separate speech service if it cannot transcribe."));
    }
    if from_guide || ui.cfg.ai.voice_separate {
        let saved = ui.int.voice_key_saved;
        let shared = ui.int.voice_key_shared;
        out.push(Item::Field(
            Field::VoiceKey,
            "Speech API key",
            if saved {
                "Saved: type a new one to replace it"
            } else if shared {
                "Same server: the key of the chat service is used"
            } else {
                "Paste the key here"
            },
        ));
        out.push(Item::Field(Field::VoiceModel, "Speech model", "whisper-1"));
        out.push(Item::Field(Field::VoiceBase, "Speech server address", "https://api.openai.com/v1"));
        out.push(Item::Button(Act::VoiceSave, "Save speech service".into(), true));
        if saved {
            out.push(Item::Button(Act::VoiceRemoveKey, "Remove the speech key".into(), true));
        }
        out.push(Item::Note("Any OpenAI-compatible /audio/transcriptions server: OpenAI, Groq, your own Whisper."));
    }
    out
}

fn sound_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![
        Item::Heading("Sounds"),
        Item::Row(Sid::SoundEnabled, "Interface sounds", Kind::Toggle),
        Item::Row(Sid::SoundPreset, "Sound preset", Kind::Cycle),
        Item::Row(Sid::SoundVolume, "Effects volume", Kind::Slider),
        Item::Note("Glass is an original sound set inspired by iPhone chimes."),
        Item::Note("Choose a PCM 16-bit WAV up to 15 seconds"),
        Item::Button(Act::SoundResetAll, "Restore original sounds".into(), true),
    ];
    if !ui.sw.sound_status.is_empty() {
        out.push(Item::Text(ui.sw.sound_status.clone()));
    }
    out.extend(sound_cue_items(ui, false));
    out
}

fn sound_cue_items(ui: &UiState, assistant_only: bool) -> Vec<Item> {
    let mut out = Vec::new();
    for (i, cue) in crate::platform::sounds::Cue::OPTIONS.iter().enumerate().take(if assistant_only { 3 } else { 4 }) {
        out.push(Item::Section(cue.label()));
        out.push(Item::Pair(
            Act::SoundImport(i),
            crate::ui::i18n::tr("Choose sound").into_owned(),
            crate::ui::i18n::tr(if ui.cfg.sounds.custom.contains_key(cue.key()) {
                "Custom sound"
            } else {
                "Preset sound"
            })
            .into_owned(),
            ui.sw.sound_busy,
        ));
        out.push(Item::Button(
            Act::SoundPreview(i),
            "Preview sound".into(),
            !ui.sw.sound_busy && ui.cfg.sounds.enabled && ui.cfg.sounds.volume > 0,
        ));
        if ui.cfg.sounds.custom.contains_key(cue.key()) {
            out.push(Item::Button(Act::SoundReset(i), "Use preset sound".into(), true));
        }
    }
    out
}

fn assistant_voice_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![
        Item::Heading("Voice"),
        Item::Row(Sid::AsstSpeak, "Speak answers aloud", Kind::Toggle),
        Item::Row(Sid::AsstSounds, "Sound cues", Kind::Toggle),
        Item::Row(Sid::SoundPreset, "Sound preset", Kind::Cycle),
        Item::Row(Sid::AsstVolume, "Assistant volume", Kind::Slider),
        Item::Section("Voice source"),
        Item::Row(Sid::AsstLocal, "Local Piper voice", Kind::Toggle),
    ];
    if ui.cfg.assistant.local_voice {
        out.push(Item::Row(Sid::AsstLocalInstall, "Download the local voice (up to 44 MB)", Kind::Button));
    }
    out.push(Item::Row(Sid::AsstNatural, "Cloud voice", Kind::Toggle));
    if ui.cfg.assistant.natural_voice {
        let (saved, shared) = (ui.int.tts_key_saved, ui.int.tts_key_shared);
        let provider = ui.cfg.assistant.tts_provider;
        let eleven = provider == crate::services::assistant::TtsProvider::Eleven;
        let fish = provider == crate::services::assistant::TtsProvider::Fish;
        out.push(Item::Row(Sid::AsstTtsProvider, "Cloud voice service", Kind::Cycle));
        out.push(Item::Field(
            Field::TtsKey,
            "Voice API key",
            if saved {
                "Saved: type a new one to replace it"
            } else if shared {
                "Same server: the speech service key is used"
            } else {
                "Paste the key here"
            },
        ));
        let (base_hint, model_hint, voice_hint) = provider.defaults();
        out.push(Item::Field(Field::TtsBase, "Voice server address", base_hint));
        out.push(Item::Field(Field::TtsModel, "Voice model", model_hint));
        out.push(Item::Field(Field::TtsVoice, if eleven || fish { "Voice ID" } else { "Voice name" }, voice_hint));
        out.push(Item::Button(Act::TtsSave, "Save the voice service".into(), true));
        if saved {
            out.push(Item::Button(Act::TtsRemoveKey, "Remove the voice key".into(), true));
        }
        if fish {
            out.push(Item::Row(Sid::AsstFishStream, "Stream Fish Audio playback", Kind::Toggle));
            out.push(Item::Row(Sid::AsstFishExpressions, "Expressive Fish Audio delivery", Kind::Toggle));
            out.push(Item::Note("Fish Audio: the key is in fish.audio, Developers, API keys."));
            out.push(Item::Note(
                "The voice ID is the ID of a voice model from fish.audio/discovery; \"default\" uses the stock voice.",
            ));
            out.push(Item::Note("Models: s2.1-pro (default), s2-pro, s1; s2.1-pro-free is the free developer tier."));
        } else if eleven {
            out.push(Item::Note(
                "ElevenLabs: the key is in elevenlabs.io, profile, API keys (paid after the free quota).",
            ));
            out.push(Item::Note(
                "The voice ID is under Voices, the ID of a voice you added; the default is a stock voice.",
            ));
            out.push(Item::Note("Models: eleven_multilingual_v2 (speaks Russian), eleven_flash_v2_5 (faster)."));
        } else {
            out.push(Item::Note("Any OpenAI-compatible server with /audio/speech. The address is where requests go:"));
            out.push(Item::Note(
                "OpenAI https://api.openai.com/v1 (key: platform.openai.com/api-keys, paid), or your own",
            ));
            out.push(Item::Note(
                "server (Kokoro-FastAPI, openedai-speech), e.g. http://localhost:8880/v1 with any key.",
            ));
            out.push(Item::Note("OpenAI voices: alloy, ash, coral, echo, fable, nova, onyx, sage, shimmer."));
        }
        out.push(Item::Note("The key is stored encrypted for your Windows account and is never shown again."));
    }
    out.push(Item::Section("Sound cues"));
    out.push(Item::Row(Sid::SoundEnabled, "Interface sounds", Kind::Toggle));
    out.push(Item::Row(Sid::SoundVolume, "Effects volume", Kind::Slider));
    if !ui.sw.sound_status.is_empty() {
        out.push(Item::Text(ui.sw.sound_status.clone()));
    }
    out.extend(sound_cue_items(ui, true));
    out
}

fn matches(label: &str, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || label.to_lowercase().contains(&q) || crate::ui::i18n::tr(label).to_lowercase().contains(&q)
}

fn top_panel_items(ui: &UiState) -> Vec<Item> {
    use crate::ui::i18n::tr;
    let mut items = vec![
        Item::Row(Sid::TopEnabled, "Enable top panel", Kind::Toggle),
        Item::Text(
            tr("Companion blocks beside the island, on the main display. Colours follow the island theme.")
                .into_owned(),
        ),
        Item::Section("Visible blocks"),
        Item::Row(Sid::TopTray, "System tray", Kind::Toggle),
        Item::Row(Sid::DockTaskbar, "Set up the Windows system tray", Kind::Button),
        Item::Text(tr("Windows taskbar settings: Other system tray icons → Hidden icon menu.").into_owned()),
        Item::Row(Sid::TopApplication, "Active application and window actions", Kind::Toggle),
        Item::Row(Sid::TopWifi, "Wi-Fi", Kind::Toggle),
        Item::Row(Sid::TopBluetooth, "Bluetooth", Kind::Toggle),
        Item::Row(Sid::TopVolume, "System volume", Kind::Toggle),
        Item::Row(Sid::TopBattery, "Battery and power", Kind::Toggle),
        Item::Row(Sid::TopMicrophone, "Microphone mute switch", Kind::Toggle),
        Item::Row(Sid::TopKeyboard, "Keyboard layout switch", Kind::Toggle),
        Item::Row(Sid::TopWorkspaces, "Windows and desktops", Kind::Toggle),
        Item::Section("Behaviour"),
        Item::Row(Sid::TopAnimations, "Animate panel interactions", Kind::Toggle),
        Item::Row(Sid::TopEntrance, "Panel entrance", Kind::Cycle),
        Item::Note("Island mode places the panels beside the island. Edge modes keep them in the screen corners."),
        Item::Row(Sid::TopGap, "Distance from the island", Kind::Slider),
        Item::Row(Sid::TopMotion, "Panel animation duration", Kind::Slider),
        Item::Row(Sid::TopLiquid, "Liquid panel edges", Kind::Toggle),
        Item::Row(Sid::TopAutoHide, "Show top panel at the screen edge", Kind::Toggle),
        Item::Text(tr("Autohide frees the top edge. Island visibility is configured separately.").into_owned()),
        Item::Row(Sid::TopFullscreen, "Hide top panel in fullscreen apps", Kind::Toggle),
        Item::Row(Sid::TopReserve, "Keep maximized windows below the top panel", Kind::Toggle),
        Item::Row(Sid::TopClearControls, "Keep window buttons accessible", Kind::Toggle),
        Item::Text(
            tr("Music, time and notifications stay in the island. Right-click a block to customize the panel.")
                .into_owned(),
        ),
    ];
    if ui.cfg.top_panel.auto_hide {
        items.retain(|item| !matches!(item, Item::Row(Sid::TopReserve, _, _)));
    }
    if ui.cfg.top_panel.entrance != crate::data::top_panel::PanelEntrance::Island {
        items.retain(|item| !matches!(item, Item::Row(Sid::TopGap, _, _)));
    }
    items
}

fn desktop_dock_items(ui: &UiState) -> Vec<Item> {
    let mut out = vec![
        Item::Heading("Desktop dock"),
        Item::Row(Sid::DockEnabled, "Enable desktop dock", Kind::Toggle),
        Item::Note("One process with Island. Enable after closing your other dock."),
        Item::Section("Capture protection"),
        Item::Row(Sid::DockExperimentalCapture, "Experimental capture protection", Kind::Toggle),
        Item::Text(crate::ui::i18n::tr("Alt + right-click an app in the dock to hide its windows from capture. Repeat to restore.").into_owned()),
        Item::Text(crate::ui::i18n::tr("Current windows only. Browsers, messengers, editors and players; 64-bit, without administrator rights.").into_owned()),
        Item::Text(crate::ui::i18n::tr("A temporary module runs inside the chosen app. Anti-cheat blocks this mode.").into_owned()),
        Item::Text(crate::ui::i18n::tr("New windows are configured separately").into_owned()),
        Item::Text(crate::ui::i18n::tr("Some recording methods can ignore Windows capture protection.").into_owned()),
        Item::Section("Behaviour"),
        Item::Row(Sid::DockAutoHide, "Automatically hide the dock", Kind::Toggle),
        Item::Row(Sid::DockRunning, "Show running applications", Kind::Toggle),
        Item::Row(Sid::DockPreview, "Window previews and controls on hover", Kind::Toggle),
        Item::Row(Sid::DockFullscreen, "Hide over fullscreen applications", Kind::Toggle),
        Item::Note("Fullscreen apps hide the dock only on their monitor. Alt+Tab brings it back."),
        Item::Row(Sid::DockAnimations, "Animate dock interactions", Kind::Toggle),
        Item::Row(Sid::DockSeparator, "Separate pinned and running applications", Kind::Toggle),
        Item::Row(Sid::DockLockPins, "Lock dock icon positions", Kind::Toggle),
        Item::Row(Sid::DockShowAdd, "Show the add button in the dock", Kind::Toggle),
        Item::Row(Sid::DockEconomy, "Economy animations (30 fps)", Kind::Toggle),
        Item::Section("Appearance"),
        Item::Row(Sid::DockIconSize, "Dock icon size", Kind::Slider),
        Item::Row(Sid::DockZoom, "Magnification on hover", Kind::Slider),
        Item::Row(Sid::DockBlur, "Blur behind the dock", Kind::Toggle),
        Item::Row(Sid::DockOpacity, "Dock opacity", Kind::Slider),
        Item::Row(Sid::DockTintHue, "Dock tint", Kind::Slider),
        Item::Row(Sid::DockTintSaturation, "Dock tint saturation", Kind::Slider),
        Item::Row(Sid::DockTintBrightness, "Dock tint brightness", Kind::Slider),
        Item::Row(Sid::DockMargin, "Bottom margin", Kind::Slider),
        Item::Row(Sid::DockMonitor, "Switch dock monitor", Kind::Button),
        Item::Text(if ui.cfg.desktop_dock.monitor.is_empty() {
            crate::ui::i18n::tr("Primary monitor").into_owned()
        } else {
            crate::platform::desktop_dock::monitors()
                .iter()
                .enumerate()
                .find(|(_, m)| m.device == ui.cfg.desktop_dock.monitor)
                .map(|(i, m)| {
                    format!(
                        "{} {} · {} × {}",
                        crate::ui::i18n::tr("Monitor"),
                        i + 1,
                        m.bounds.right - m.bounds.left,
                        m.bounds.bottom - m.bounds.top
                    )
                })
                .unwrap_or_else(|| crate::ui::i18n::tr("Primary monitor").into_owned())
        }),
        Item::Section("Applications"),
        Item::Row(Sid::DockImportTaskbar, "Import Windows taskbar pins", Kind::Button),
        Item::Row(Sid::DockImport, "Import applications from MyDockFinder", Kind::Button),
        Item::Row(Sid::DockAdd, "Add application or file", Kind::Button),
        Item::Note("Drop shortcuts onto the dock. Drag pinned icons to reorder."),
        Item::Note("Right-click for windows, pinning and settings. Scroll for more apps."),
        Item::Row(Sid::DockTaskbar, "Windows taskbar settings", Kind::Button),
        Item::Row(Sid::DockReplaceTaskbar, "Replace Windows taskbar while dock is enabled", Kind::Toggle),
        Item::Note("The Windows taskbar returns when the dock or Island closes."),
    ];
    if !ui.sw.dock_status.is_empty() {
        let index = out.iter().position(|item| matches!(item, Item::Row(Sid::DockImport, ..))).unwrap_or(0);
        out.insert(index + 1, Item::Text(ui.sw.dock_status.clone()));
    }
    out.push(Item::Section("Games and application rules"));
    out.push(Item::Note("For a windowed game, right-click its dock icon and choose Hide dock in this app."));
    for (i, path) in ui.cfg.desktop_dock.hidden_apps.iter().enumerate() {
        let name = std::path::Path::new(path).file_name().unwrap_or_default().to_string_lossy().into_owned();
        out.push(Item::Pair(Act::DockHiddenRemove(i), name, "Remove rule".into(), false));
    }
    if !ui.cfg.desktop_dock.pins.is_empty() {
        out.push(Item::Section("Pinned applications"));
    }
    for (i, pin) in ui.cfg.desktop_dock.pins.iter().enumerate() {
        out.push(Item::Pair(Act::DockRemove(i), pin.name.clone(), "Remove".into(), false));
    }
    out
}

fn rows_with_sections(group: &Group) -> Vec<Item> {
    let mut out = Vec::new();
    for (i, &(id, label, kind)) in group.rows.iter().enumerate() {
        if let Some(&(_, title)) = group.sections.iter().find(|(at, _)| *at == i) {
            out.push(Item::Section(title));
        }
        out.push(Item::Row(id, label, kind));
        if id == Sid::PetAutonomy {
            out.push(Item::Note("Pets pause near the pointer and during chat. Reduced motion stops walking."));
        }
    }
    out
}

pub(in crate::view) fn items(ui: &UiState) -> Vec<Item> {
    let mut out = Vec::new();
    if ui.sw.search.trim().is_empty() {
        let group = &GROUPS[ui.sw.group.min(GROUPS.len() - 1)];
        if let Some(build) = group.custom {
            return build(ui);
        }
        out.push(Item::Heading(group.name));
        out.extend(rows_with_sections(group));
        if group.name == "Appearance" {
            let at = out.iter().position(|item| matches!(item, Item::Row(Sid::InterfaceFont, _, _))).unwrap_or(0);
            let mut font_items = vec![
                Item::FontPreview,
                Item::Button(
                    Act::FontImport,
                    crate::ui::i18n::tr(if ui.sw.font_busy { "Loading font…" } else { "Load your font…" }).into_owned(),
                    !ui.sw.font_busy,
                ),
                Item::Button(
                    Act::FontDownload,
                    crate::ui::i18n::tr(
                        if ui.cfg.interface_font == crate::data::settings::InterfaceFont::SanFrancisco {
                            "SF Pro website (macOS package)"
                        } else {
                            "Download font"
                        },
                    )
                    .into_owned(),
                    true,
                ),
                Item::Note("TTF, OTF or TTC, up to 16 MB. The font is saved in Island."),
                Item::Note("Download opens the official font website."),
            ];
            if ui.cfg.interface_font == crate::data::settings::InterfaceFont::SanFrancisco {
                font_items.push(Item::Note("Apple provides a DMG package. Import a TTF or OTF file, not the DMG."));
                if !crate::ui::fonts::sf_available() {
                    font_items.push(Item::Note("SF Pro is not installed. The preview currently uses Segoe UI."));
                }
            }
            if ui.cfg.interface_font == crate::data::settings::InterfaceFont::Custom
                && crate::ui::fonts::custom_name().is_empty()
            {
                font_items.push(Item::Note("Load a font file to replace Segoe UI in the preview."));
            }
            if !ui.sw.font_status.is_empty() {
                font_items.push(Item::Text(ui.sw.font_status.clone()));
            }
            out.splice(at + 1..at + 1, font_items);
        }
        return out;
    }
    for group in &GROUPS {
        if group.name == "Assistant" {
            let start = out.len();
            for (page, title) in AssistantPage::ALL {
                let page_matches = matches(title, &ui.sw.search);
                let found: Vec<_> = assistant_page_items(ui, page)
                    .into_iter()
                    .filter(|item| match item {
                        Item::Row(_, label, _) | Item::Field(_, label, _) => {
                            page_matches || matches(label, &ui.sw.search)
                        }
                        Item::Pair(_, label, _, _) | Item::Button(_, label, _) => {
                            page_matches || matches(label, &ui.sw.search)
                        }
                        _ => false,
                    })
                    .collect();
                if !found.is_empty() {
                    out.push(Item::Section(title));
                    out.extend(found);
                }
            }
            if out.len() != start {
                out.insert(start, Item::Heading(group.name));
            }
            continue;
        }
        let found: Vec<_> = group.rows.iter().filter(|r| matches(r.1, &ui.sw.search)).collect();
        if found.is_empty() {
            continue;
        }
        out.push(Item::Heading(group.name));
        out.extend(found.into_iter().map(|&(id, label, kind)| Item::Row(id, label, kind)));
    }
    out
}

struct Layout {
    sidebar: Rect,
    search: Rect,
    assistant_nav: Option<Rect>,
    resume_guide: Option<Rect>,
    content: Rect,
}

fn layout(size: (f32, f32), scale: f32, ui: &UiState) -> Layout {
    let (w, h) = size;
    let left = (SIDEBAR_W + PAD) * scale;
    let top = TITLE_H * scale;
    let right = w - PAD * scale;
    let body = top + (18.0 + SEARCH_H + 10.0) * scale;
    let assistant_nav = (ui.sw.search.trim().is_empty()
        && GROUPS.get(ui.sw.group).is_some_and(|group| group.name == "Assistant"))
    .then_some((left, body, right, body + 34.0 * scale));
    let search_top = top + 18.0 * scale;
    let search_bottom = search_top + SEARCH_H * scale;
    let resume_guide = ui
        .sw
        .guide
        .as_ref()
        .filter(|guide| !guide.visible)
        .map(|_| (right - 184.0 * scale, search_top, right, search_bottom));
    Layout {
        sidebar: (0.0, top, SIDEBAR_W * scale, h),
        search: (left, search_top, resume_guide.map_or(right, |r| r.0 - 8.0 * scale), search_bottom),
        assistant_nav,
        resume_guide,
        content: (left, assistant_nav.map_or(body, |nav| nav.3 + 8.0 * scale), right, h - 12.0 * scale),
    }
}

fn assistant_nav_rects(nav: Rect, scale: f32) -> impl Iterator<Item = (AssistantPage, &'static str, Rect)> {
    let step = (nav.2 - nav.0 + 4.0 * scale) / AssistantPage::ALL.len() as f32;
    AssistantPage::ALL.into_iter().enumerate().map(move |(i, (page, label))| {
        let x = nav.0 + step * i as f32;
        (page, label, (x, nav.1, x + step - 4.0 * scale, nav.3))
    })
}

fn group_rect(l: &Layout, scale: f32, i: usize) -> Rect {
    let sections = SIDEBAR_SECTIONS.iter().filter(|s| s.0 <= i).count() as f32;
    let top = l.sidebar.1 + 6.0 * scale + (i as f32 * GROUP_H + sections * SIDE_SECTION_H) * scale;
    (l.sidebar.0 + 12.0 * scale, top, l.sidebar.2 - 12.0 * scale, top + (GROUP_H - 4.0) * scale)
}

fn placed(size: (f32, f32), scale: f32, ui: &UiState) -> Vec<(Item, f32)> {
    let l = layout(size, scale, ui);
    let mut y = l.content.1 - ui.sw.scroll;
    items(ui)
        .into_iter()
        .map(|item| {
            let top = y;
            y += item.height() * scale;
            (item, top)
        })
        .collect()
}

pub fn max_scroll(size: (f32, f32), scale: f32, ui: &UiState) -> f32 {
    let l = layout(size, scale, ui);
    let total: f32 = items(ui).iter().map(Item::height).sum::<f32>() * scale;
    (total - (l.content.3 - l.content.1)).max(0.0)
}

fn row_rects(l: &Layout, scale: f32, top: f32) -> (Rect, Rect) {
    let row = (l.content.0, top, l.content.2, top + ROW_H * scale);
    let ctl = (row.2 - CTL_W * scale, top + 7.0 * scale, row.2, top + (ROW_H - 7.0) * scale);
    (row, ctl)
}

const DROP_ITEM_H: f32 = 30.0;

fn dropdown_rects(size: (f32, f32), scale: f32, ui: &UiState) -> Option<(Rect, Vec<Rect>)> {
    let id = ui.sw.dropdown?;
    let (names, _) = cycle_options(id, &ui.cfg);
    if names.is_empty() {
        return None;
    }
    let l = layout(size, scale, ui);
    let top = placed(size, scale, ui).into_iter().find_map(|(item, top)| match item {
        Item::Row(row_id, ..) if row_id == id => Some(top),
        _ => None,
    })?;
    let (_, ctl) = row_rects(&l, scale, top);
    let h = names.len() as f32 * DROP_ITEM_H * scale + 8.0 * scale;
    let below = ctl.3 + 4.0 * scale;
    let y0 = if below + h <= size.1 - 8.0 * scale { below } else { (ctl.1 - 4.0 * scale - h).max(8.0 * scale) };
    let list = (ctl.0, y0, ctl.2, y0 + h);
    let items = (0..names.len())
        .map(|k| {
            let t = list.1 + 4.0 * scale + k as f32 * DROP_ITEM_H * scale;
            (list.0 + 4.0 * scale, t, list.2 - 4.0 * scale, t + DROP_ITEM_H * scale)
        })
        .collect();
    Some((list, items))
}

fn title_buttons(size: (f32, f32), scale: f32) -> (Rect, Rect) {
    let (bw, bh) = (TITLE_BTN_W * scale, TITLE_H * scale);
    let close = (size.0 - bw, 0.0, size.0, bh);
    ((size.0 - 2.0 * bw, 0.0, size.0 - bw, bh), close)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleZone {
    Button,
    Caption,
}

pub fn sw_title_zone(size: (f32, f32), scale: f32, px: f32, py: f32) -> Option<TitleZone> {
    if py < 0.0 || py >= TITLE_H * scale {
        return None;
    }
    let (min, close) = title_buttons(size, scale);
    Some(if inside(min, px, py) || inside(close, px, py) { TitleZone::Button } else { TitleZone::Caption })
}

pub fn sw_hit(size: (f32, f32), scale: f32, ui: &UiState, px: f32, py: f32) -> Option<SwHit> {
    let l = layout(size, scale, ui);
    if let Some((list, items)) = dropdown_rects(size, scale, ui) {
        if let (Some(id), Some(k)) = (ui.sw.dropdown, items.iter().position(|r| inside(*r, px, py))) {
            return Some(SwHit::Pick(id, k));
        }
        if inside(list, px, py) {
            return None;
        }
    }
    let (min, close) = title_buttons(size, scale);
    if inside(min, px, py) {
        return Some(SwHit::Minimize);
    }
    if inside(close, px, py) {
        return Some(SwHit::Close);
    }
    if let Some(guide) = ui.sw.guide.as_ref().filter(|guide| guide.visible) {
        if guide_wake_rects(size, scale, guide).is_some_and(|(rect, _)| inside(rect, px, py)) {
            return Some(SwHit::Field(Field::GuideWake));
        }
        return guide_hit(size, scale, guide, px, py).map(SwHit::Guide);
    }
    if l.resume_guide.is_some_and(|rect| inside(rect, px, py)) {
        return Some(SwHit::Guide(GuideAction::Resume));
    }
    if inside(l.search, px, py) {
        return Some(SwHit::Field(Field::Search));
    }
    if let Some(i) = (0..GROUPS.len()).find(|&i| inside(group_rect(&l, scale, i), px, py)) {
        return Some(SwHit::Group(i));
    }
    if ui.sw.memory.open {
        return hit_memory(l.content, scale, ui, px, py).map(SwHit::Memory);
    }
    if let Some(nav) = l.assistant_nav
        && let Some((page, _, _)) = assistant_nav_rects(nav, scale).find(|(_, _, rect)| inside(*rect, px, py))
    {
        return Some(SwHit::AssistantPage(page));
    }
    if !inside(l.content, px, py) {
        return None;
    }
    for (item, top) in placed(size, scale, ui) {
        let (row, ctl) = row_rects(&l, scale, top);
        match item {
            Item::Row(id, _, kind) => {
                if !inside(row, px, py) {
                    continue;
                }
                let on_label = px < ctl.0 - 6.0 * scale;
                let hit = match kind {
                    Kind::Segment(_) | Kind::Cycle | Kind::Slider if on_label => SetHit::Label,
                    Kind::Cycle => SetHit::Select,
                    Kind::Segment(opts) => {
                        let t = ((px - ctl.0) / (ctl.2 - ctl.0)).clamp(0.0, 0.999);
                        SetHit::Option((t * opts.len() as f32) as usize)
                    }
                    Kind::Slider => SetHit::Slider,
                    Kind::Toggle => SetHit::Toggle,
                    Kind::Button => SetHit::Button,
                };
                return Some(SwHit::Row(id, hit));
            }
            Item::ThemeBlock => {
                if let Some(h) = hit_theme_block(row, scale, px, py) {
                    return Some(SwHit::Theme(h));
                }
            }
            Item::Field(field, ..) if inside(row, px, py) => return Some(SwHit::Field(field)),
            Item::Button(act, _, enabled) if enabled && inside(row, px, py) => return Some(SwHit::Act(act)),
            Item::Pair(act, ..) if inside(row, px, py) => return Some(SwHit::Act(act)),
            _ => {}
        }
    }
    None
}

pub fn sw_slider_track(size: (f32, f32), scale: f32, ui: &UiState, id: Sid) -> Option<(f32, f32)> {
    let l = layout(size, scale, ui);
    let top = placed(size, scale, ui).into_iter().find_map(|(item, top)| match item {
        Item::Row(row_id, ..) if row_id == id => Some(top),
        _ => None,
    })?;
    let (_, ctl) = row_rects(&l, scale, top);
    Some((ctl.0 + 4.0 * scale, ctl.2 - 54.0 * scale))
}

pub fn sw_field_rects(size: (f32, f32), scale: f32, ui: &UiState, field: Field) -> Option<(Rect, Rect)> {
    if field == Field::GuideWake {
        return guide_wake_rects(size, scale, ui.sw.guide.as_ref().filter(|guide| guide.visible)?);
    }
    let l = layout(size, scale, ui);
    if field == Field::Search {
        let s = l.search;
        return Some((s, (s.0 + 40.0 * scale, s.1, s.2 - 12.0 * scale, s.3)));
    }
    let top = placed(size, scale, ui).into_iter().find_map(|(item, top)| match item {
        Item::Field(f, ..) if f == field => Some(top),
        _ => None,
    })?;
    let (_, ctl) = row_rects(&l, scale, top);
    Some((ctl, (ctl.0 + 12.0 * scale, ctl.1, ctl.2 - 10.0 * scale, ctl.3)))
}

pub fn sw_next_field(ui: &UiState, field: Field) -> Option<Field> {
    let fields: Vec<Field> = items(ui)
        .into_iter()
        .filter_map(|i| match i {
            Item::Field(f, ..) => Some(f),
            _ => None,
        })
        .collect();
    let at = fields.iter().position(|f| *f == field)?;
    fields.get((at + 1) % fields.len()).copied()
}

pub fn field_edit(ui: &UiState, field: Field) -> &TextEdit {
    match field {
        Field::DiscordId => &ui.int.id,
        Field::DiscordSecret => &ui.int.secret,
        Field::SpotifyId => &ui.int.spotify_id,
        Field::AiKey => &ui.int.ai_key,
        Field::AiModel => &ui.int.ai_model,
        Field::AiBase => &ui.int.ai_base,
        Field::VoiceKey => &ui.int.voice_key,
        Field::VoiceModel => &ui.int.voice_model,
        Field::VoiceBase => &ui.int.voice_base,
        Field::TtsKey => &ui.int.tts_key,
        Field::TtsBase => &ui.int.tts_base,
        Field::TtsModel => &ui.int.tts_model,
        Field::TtsVoice => &ui.int.tts_voice,
        Field::WakeWord => &ui.int.wake,
        Field::GuideWake => ui.sw.guide.as_ref().map_or(&ui.int.wake, |guide| &guide.wake),
        Field::GroupName => &ui.int.group_name,
        Field::Search => &ui.int.search,
    }
}

fn row_hover(p: &Painter, row: Rect, scale: f32, ui: &UiState, plate: Plate) -> Result<()> {
    let level = ui.sw.plates.iter().find(|(k, _)| *k == plate).map_or(0.0, |(_, v)| *v);
    if level < 0.01 {
        return Ok(());
    }
    p.fill_round_rect(
        (row.0 - HOVER_BLEED * scale, row.1 + 2.0 * scale, row.2 + HOVER_BLEED * scale, row.3 - 2.0 * scale),
        10.0 * scale,
        rgba(1.0, 1.0, 1.0, 0.045 * level),
    )
}

const HOVER_BLEED: f32 = 14.0;

pub fn paint_settings_window(p: &Painter, size: (f32, f32), scale: f32, ui: &UiState) -> Result<()> {
    let l = layout(size, scale, ui);
    let (pr, pg, pb) = crate::ui::theme::panel();
    p.fill_rect((0.0, 0.0, size.0, size.1), rgba(pr, pg, pb, 1.0))?;
    let guide = ui.sw.guide.as_ref().filter(|guide| guide.visible);
    if guide.is_none() {
        p.fill_rect(l.sidebar, rgba(1.0, 1.0, 1.0, 0.045))?;
    }

    let bar = (0.0, 0.0, size.0, TITLE_H * scale);
    p.fill_rect(bar, rgba(0.0, 0.0, 0.0, 0.18))?;
    p.fill_rect((0.0, bar.3 - 1.0, size.0, bar.3), rgba(1.0, 1.0, 1.0, 0.07))?;
    p.glyph('\u{E713}', (14.0 * scale, 0.0, 38.0 * scale, bar.3), 15.0 * scale, rgba(1.0, 1.0, 1.0, 0.8))?;
    p.text(
        if guide.is_some() { "Quick tour" } else { "Settings" },
        (42.0 * scale, 0.0, size.0 / 2.0, bar.3),
        13.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.9),
    )?;
    let (min, close) = title_buttons(size, scale);
    let pill = |r: Rect| (r.0 + 4.0 * scale, r.1 + 6.0 * scale, r.2 - 4.0 * scale, r.3 - 6.0 * scale);
    if ui.sw.hover == Some(SwHit::Minimize) {
        p.fill_round_rect(pill(min), 9.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
    }
    if ui.sw.hover == Some(SwHit::Close) {
        p.fill_round_rect(pill(close), 9.0 * scale, rgba(0.9, 0.2, 0.2, 0.9))?;
    }
    let (cx, cy) = ((min.0 + min.2) / 2.0, (min.1 + min.3) / 2.0);
    p.fill_rect((cx - 5.0 * scale, cy, cx + 5.0 * scale, cy + scale.max(1.0)), rgba(1.0, 1.0, 1.0, 0.9))?;
    p.glyph('\u{E711}', close, 14.0 * scale, WHITE)?;
    if let Some(guide) = guide {
        return paint_guide(p, size, scale, guide, ui);
    }

    let searching = !ui.sw.search.trim().is_empty();
    for (n, &(first, title)) in SIDEBAR_SECTIONS.iter().enumerate() {
        let r = group_rect(&l, scale, first);
        let y = r.1 - SIDE_SECTION_H * scale + 2.0 * scale;
        if n > 0 {
            p.fill_rect(
                (l.sidebar.0 + 14.0 * scale, y + 2.0 * scale, l.sidebar.2 - 14.0 * scale, y + 2.0 * scale + 1.0),
                rgba(1.0, 1.0, 1.0, 0.08),
            )?;
        }
        p.text(
            title,
            (l.sidebar.0 + 18.0 * scale, y + 4.0 * scale, l.sidebar.2, y + SIDE_SECTION_H * scale - 2.0 * scale),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.45),
        )?;
    }
    for (i, group) in GROUPS.iter().enumerate() {
        let r = group_rect(&l, scale, i);
        let selected = i == ui.sw.group && !searching;
        if selected {
            p.fill_round_rect(r, 10.0 * scale, acc(0.22))?;
        } else if ui.sw.hover == Some(SwHit::Group(i)) {
            p.fill_round_rect(r, 10.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
        }
        let alpha = if selected { 1.0 } else { 0.7 };
        p.glyph(
            group.glyph,
            (r.0 + 6.0 * scale, r.1, r.0 + 38.0 * scale, r.3),
            16.0 * scale,
            rgba(1.0, 1.0, 1.0, alpha),
        )?;
        p.text(
            group.name,
            (r.0 + 42.0 * scale, r.1, r.2, r.3),
            13.5 * scale,
            if selected { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, alpha),
        )?;
    }

    if let Some(rect) = l.resume_guide {
        guide_button(
            p,
            rect,
            scale,
            "Resume quick tour",
            true,
            ui.sw.hover == Some(SwHit::Guide(GuideAction::Resume)),
        )?;
    }
    let search_inner = (l.search.0 + 40.0 * scale, l.search.1, l.search.2 - 12.0 * scale, l.search.3);
    paint_line_edit(
        p,
        l.search,
        search_inner,
        &ui.int.search,
        false,
        "Search settings",
        ui.sw.field == Some(Field::Search),
        ui.sw.caret_on,
        scale,
    )?;
    p.glyph(
        '\u{E721}',
        (l.search.0 + 4.0 * scale, l.search.1, l.search.0 + 38.0 * scale, l.search.3),
        14.0 * scale,
        rgba(1.0, 1.0, 1.0, 0.6),
    )?;

    if ui.sw.memory.open {
        return paint_memory(p, l.content, scale, ui);
    }
    if let Some(nav) = l.assistant_nav {
        for (page, label, rect) in assistant_nav_rects(nav, scale) {
            let selected = ui.sw.assistant_page == page;
            let hovered = ui.sw.hover == Some(SwHit::AssistantPage(page));
            let fill = if selected { acc(0.26) } else { rgba(1.0, 1.0, 1.0, if hovered { 0.10 } else { 0.05 }) };
            p.fill_round_rect(rect, 10.0 * scale, fill)?;
            p.text(
                label,
                rect,
                12.0 * scale,
                if selected { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, if selected || hovered { 1.0 } else { 0.78 }),
            )?;
        }
    }

    p.push_clip((l.content.0 - HOVER_BLEED * scale, l.content.1, l.content.2 + HOVER_BLEED * scale, l.content.3));
    let result = (|| -> Result<()> {
        let placed = placed(size, scale, ui);
        if placed.is_empty() {
            return p.text(
                "Nothing found",
                (l.content.0, l.content.1, l.content.2, l.content.1 + 40.0 * scale),
                14.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, 0.5),
            );
        }
        for (item, top) in placed {
            if top > l.content.3 || top + item.height() * scale < l.content.1 {
                continue;
            }
            let (row, ctl) = row_rects(&l, scale, top);
            let label_area = (row.0, row.1, ctl.0 - 12.0 * scale, row.3);
            match item {
                Item::Heading(name) => {
                    p.text(
                        name,
                        (l.content.0, top, l.content.2, top + HEADING_H * scale),
                        18.0 * scale,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        WHITE,
                    )?;
                }
                Item::ThemeBlock => {
                    let hovered = match ui.sw.hover {
                        Some(SwHit::Theme(h)) => Some(h),
                        _ => None,
                    };
                    paint_theme_block(p, row, scale, ui, hovered)?;
                }
                Item::Section(title) => {
                    if top > l.content.1 + 4.0 * scale {
                        let y = top + 10.0 * scale;
                        p.fill_rect((row.0, y, row.2, y + 1.0), rgba(1.0, 1.0, 1.0, 0.08))?;
                    }
                    p.text(
                        title,
                        (row.0, top + 14.0 * scale, row.2, top + SECTION_H * scale),
                        12.5 * scale,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.55),
                    )?;
                }
                Item::Row(id, label, kind) => {
                    row_hover(p, row, scale, ui, Plate::Row(id))?;
                    p.text(
                        label,
                        label_area,
                        14.0 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.9),
                    )?;
                    paint_control(p, scale, &ui.cfg, id, kind, ctl, &ui.feedback)?;
                }
                Item::Service { name, glyph, status, live } => {
                    let d = 38.0 * scale;
                    let circle = (row.0, top + 8.0 * scale, row.0 + d, top + 8.0 * scale + d);
                    let colour = match name {
                        "Discord" => rgba(0.35, 0.4, 0.95, 1.0),
                        "Spotify" => rgba(0.12, 0.84, 0.38, 1.0),
                        "Yandex Music" => rgba(1.0, 0.85, 0.12, 1.0),
                        _ => acc(0.9),
                    };
                    p.fill_round_rect(circle, d / 2.0, rgba(colour.r, colour.g, colour.b, 0.12))?;
                    p.glyph(glyph, circle, 24.0 * scale, colour)?;
                    p.text(
                        name,
                        (circle.2 + 12.0 * scale, circle.1 - 2.0 * scale, row.2, circle.1 + 20.0 * scale),
                        15.0 * scale,
                        DWRITE_FONT_WEIGHT_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        WHITE,
                    )?;
                    let status_colour = if live { rgba(0.4, 0.9, 0.55, 1.0) } else { rgba(1.0, 1.0, 1.0, 0.6) };
                    p.text(
                        &status,
                        (circle.2 + 12.0 * scale, circle.1 + 19.0 * scale, row.2, circle.3 + 2.0 * scale),
                        12.5 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        status_colour,
                    )?;
                }
                Item::Field(field, label, hint) => {
                    row_hover(p, row, scale, ui, Plate::Field(field))?;
                    p.text(
                        label,
                        label_area,
                        14.0 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.9),
                    )?;
                    let inner = (ctl.0 + 12.0 * scale, ctl.1, ctl.2 - 10.0 * scale, ctl.3);
                    paint_line_edit(
                        p,
                        ctl,
                        inner,
                        field_edit(ui, field),
                        matches!(field, Field::DiscordSecret | Field::AiKey | Field::VoiceKey | Field::TtsKey),
                        hint,
                        ui.sw.field == Some(field),
                        ui.sw.caret_on,
                        scale,
                    )?;
                }
                Item::Button(act, label, enabled) => {
                    let hovered = enabled && ui.sw.hover == Some(SwHit::Act(act));
                    let fill = if hovered {
                        0.2
                    } else if enabled {
                        0.12
                    } else {
                        0.05
                    };
                    p.fill_round_rect(ctl, 11.0 * scale, rgba(1.0, 1.0, 1.0, fill))?;
                    p.text(
                        &label,
                        ctl,
                        13.5 * scale,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        rgba(1.0, 1.0, 1.0, if enabled { 1.0 } else { 0.4 }),
                    )?;
                }
                Item::Pair(act, label, button, active) => {
                    row_hover(p, row, scale, ui, Plate::Act(act))?;
                    p.text(
                        &label,
                        label_area,
                        14.0 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.9),
                    )?;
                    if active {
                        p.fill_round_rect(ctl, 11.0 * scale, acc(0.45))?;
                    } else {
                        let fill = if ui.sw.hover == Some(SwHit::Act(act)) { 0.2 } else { 0.12 };
                        p.fill_round_rect(ctl, 11.0 * scale, rgba(1.0, 1.0, 1.0, fill))?;
                    }
                    p.text(
                        &button,
                        ctl,
                        13.5 * scale,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        WHITE,
                    )?;
                }
                Item::FontPreview => {
                    let card = (l.content.0, top + 4.0 * scale, l.content.2, top + 128.0 * scale);
                    p.fill_round_rect(card, 18.0 * scale, rgba(1.0, 1.0, 1.0, 0.055))?;
                    let custom = crate::ui::fonts::custom_name();
                    let label = if ui.cfg.interface_font == crate::data::settings::InterfaceFont::Custom
                        && !custom.is_empty()
                    {
                        custom.as_str()
                    } else {
                        ui.cfg.interface_font.name()
                    };
                    p.text(
                        label,
                        (card.0 + 16.0 * scale, card.1 + 8.0 * scale, card.2 - 16.0 * scale, card.1 + 30.0 * scale),
                        12.0 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.5),
                    )?;
                    p.text(
                        "Night Island · Привет, мир!",
                        (card.0 + 16.0 * scale, card.1 + 34.0 * scale, card.2 - 16.0 * scale, card.1 + 67.0 * scale),
                        22.0 * scale,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        WHITE,
                    )?;
                    let pill = (
                        card.0 + 16.0 * scale,
                        card.1 + 78.0 * scale,
                        (card.0 + 286.0 * scale).min(card.2 - 16.0 * scale),
                        card.1 + 110.0 * scale,
                    );
                    p.fill_round_rect(pill, 16.0 * scale, rgba(0.0, 0.0, 0.0, 1.0))?;
                    p.text(
                        "Night Drive     21:51     6°",
                        pill,
                        13.0 * scale,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        WHITE,
                    )?;
                }
                Item::Text(text) => {
                    p.text(
                        &text,
                        (l.content.0, top, l.content.2, top + NOTE_H * scale),
                        12.5 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.75),
                    )?;
                }
                Item::Message(text, good) => {
                    let color = if good { rgba(0.35, 0.9, 0.55, 1.0) } else { rgba(1.0, 0.4, 0.4, 1.0) };
                    let rect = (l.content.0, top + 3.0 * scale, l.content.2, top + 64.0 * scale);
                    p.fill_round_rect(rect, 12.0 * scale, rgba(color.r, color.g, color.b, 0.1))?;
                    p.glyph(
                        if good { '\u{E73E}' } else { '\u{E783}' },
                        (rect.0 + 8.0 * scale, rect.1, rect.0 + 34.0 * scale, rect.3),
                        17.0 * scale,
                        color,
                    )?;
                    p.text_wrapped(
                        &crate::ui::i18n::tr(&text),
                        (rect.0 + 42.0 * scale, rect.1 + 5.0 * scale, rect.2 - 10.0 * scale, rect.3 - 5.0 * scale),
                        12.5 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        color,
                    )?;
                }
                Item::Note(text) => {
                    p.text(
                        text,
                        (l.content.0, top, l.content.2, top + NOTE_H * scale),
                        11.5 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.5),
                    )?;
                }
            }
        }
        Ok(())
    })();
    p.pop_clip();
    result?;

    if let (Some(id), Some((list, items))) = (ui.sw.dropdown, dropdown_rects(size, scale, ui)) {
        let (names, now) = cycle_options(id, &ui.cfg);
        let (pr, pg, pb) = crate::ui::theme::panel();
        let lift = |c: f32| c + (1.0 - c) * 0.12;
        p.fill_round_rect(
            (list.0 - 2.0 * scale, list.1 + 2.0 * scale, list.2 + 2.0 * scale, list.3 + 6.0 * scale),
            12.0 * scale,
            rgba(0.0, 0.0, 0.0, 0.25),
        )?;
        p.fill_round_rect(list, 10.0 * scale, rgba(lift(pr), lift(pg), lift(pb), 1.0))?;
        p.stroke_round_rect(list, 10.0 * scale, scale, rgba(1.0, 1.0, 1.0, 0.14))?;
        for (k, (name, r)) in names.iter().zip(items).enumerate() {
            let hot = ui.sw.hover == Some(SwHit::Pick(id, k));
            if k == now {
                p.fill_round_rect(r, 7.0 * scale, acc(0.35))?;
            } else if hot {
                p.fill_round_rect(r, 7.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
            }
            p.text(
                name,
                (r.0 + 10.0 * scale, r.1, r.2 - 28.0 * scale, r.3),
                12.5 * scale,
                if k == now { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            if k == now {
                p.glyph('\u{E73E}', (r.2 - 26.0 * scale, r.1, r.2 - 4.0 * scale, r.3), 13.0 * scale, WHITE)?;
            }
        }
    }

    let max = max_scroll(size, scale, ui);
    if max > 0.0 {
        let (track_top, track_h) = (l.content.1, l.content.3 - l.content.1);
        let total = track_h + max;
        let bar_h = (track_h * track_h / total).max(24.0 * scale);
        let y = track_top + (track_h - bar_h) * (ui.sw.scroll / max);
        p.fill_round_rect(
            (size.0 - 12.0 * scale, y, size.0 - 8.0 * scale, y + bar_h),
            2.0 * scale,
            rgba(1.0, 1.0, 1.0, 0.25),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_dock_controls_persist_and_sidebar_stays_inside_the_window() {
        let mut ui = UiState::new();
        ui.sw.group = GROUPS.iter().position(|g| g.name == "Desktop dock").unwrap();
        let rows = items(&ui);
        for sid in [
            Sid::DockEnabled,
            Sid::DockAutoHide,
            Sid::DockRunning,
            Sid::DockPreview,
            Sid::DockFullscreen,
            Sid::DockEconomy,
            Sid::DockIconSize,
            Sid::DockZoom,
            Sid::DockOpacity,
            Sid::DockBlur,
            Sid::DockTintHue,
            Sid::DockTintSaturation,
            Sid::DockTintBrightness,
            Sid::DockMargin,
            Sid::DockMonitor,
            Sid::DockImport,
            Sid::DockAdd,
            Sid::DockTaskbar,
        ] {
            assert!(rows.iter().any(|item| matches!(item, Item::Row(id, ..) if *id == sid)));
        }
        toggle_set(Sid::DockEnabled, &mut ui.cfg, true);
        toggle_set(Sid::DockAutoHide, &mut ui.cfg, true);
        slider_apply(Sid::DockIconSize, &mut ui.cfg, 1.0);
        toggle_set(Sid::DockBlur, &mut ui.cfg, true);
        slider_apply(Sid::DockTintHue, &mut ui.cfg, 0.5);
        slider_apply(Sid::DockTintSaturation, &mut ui.cfg, 0.6);
        slider_apply(Sid::DockTintBrightness, &mut ui.cfg, 0.5);
        slider_apply(Sid::DockOpacity, &mut ui.cfg, 0.5);
        assert!((ui.cfg.desktop_dock.blur_opacity - 0.6).abs() < 0.001);
        assert_eq!(ui.cfg.desktop_dock.opacity, 0.94);
        toggle_set(Sid::DockBlur, &mut ui.cfg, false);
        slider_apply(Sid::DockOpacity, &mut ui.cfg, 1.0);
        assert_eq!(ui.cfg.desktop_dock.opacity, 1.0);
        assert!((ui.cfg.desktop_dock.blur_opacity - 0.6).abs() < 0.001);
        ui.cfg.desktop_dock.hidden_apps.push(r"c:\games\game.exe".into());
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Pair(Act::DockHiddenRemove(0), ..))));
        let saved = serde_json::to_string(&ui.cfg).unwrap();
        let restored: Settings = serde_json::from_str(&saved).unwrap();
        assert!(restored.desktop_dock.enabled && restored.desktop_dock.auto_hide);
        assert_eq!(restored.desktop_dock.icon_size, 72);
        assert_eq!(restored.desktop_dock.tint_hue, 180.0);
        assert_eq!(restored.desktop_dock.hidden_apps.len(), 1);
        for scale in [1.0, 1.5, 2.0] {
            let size = (SW_SIZE.0 * scale, SW_SIZE.1 * scale);
            assert!(group_rect(&layout(size, scale, &UiState::new()), scale, GROUPS.len() - 1).3 <= size.1);
        }
    }

    #[test]
    fn wave_integration_covers_sign_in_code_and_connected_account() {
        let mut ui = UiState::new();
        let rows = integration_items(&ui);
        assert!(rows.iter().any(|row| matches!(row, Item::Service { name: "Yandex Music", .. })));
        assert!(rows.iter().any(|row| matches!(row, Item::Button(Act::YandexMain,label,true) if label == "Connect")));
        ui.yandex.state.code = "TEST-CODE".into();
        let rows = integration_items(&ui);
        assert!(rows.iter().any(|row| matches!(row, Item::Button(Act::YandexCopy, _, true))));
        assert!(rows.iter().any(|row| matches!(row, Item::Button(Act::YandexBrowser, _, true))));
        ui.yandex.state.code.clear();
        ui.yandex.state.has_token = true;
        ui.yandex.state.account = "Account".into();
        let rows = integration_items(&ui);
        assert!(
            rows.iter().any(|row| matches!(row, Item::Button(Act::YandexMain,label,true) if label == "Disconnect"))
        );
        assert!(rows.iter().any(|row| matches!(row, Item::Pair(Act::YandexAutoplay, ..))));
        assert!(rows.iter().any(|row| matches!(row, Item::Button(Act::YandexWave, _, true))));
    }

    #[test]
    fn notification_size_dropdown_and_settings_round_trip() {
        let mut settings = Settings::default();
        for (index, count) in [1, 2, 4].into_iter().enumerate() {
            cycle_pick(Sid::NotificationTiles, &mut settings, index);
            assert_eq!(settings.notification_tiles, count);
            assert_eq!(cycle_options(Sid::NotificationTiles, &settings).1, index);
            toggle_set(Sid::NotificationOpenApp, &mut settings, true);
            let saved: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(saved.notification_tiles, count);
            assert!(saved.notification_open_app);
        }
        settings.notification_tiles = 255;
        settings.sanitize();
        assert_eq!(settings.notification_tiles, 4);
        let migrated: Settings = serde_json::from_str("{\"ai_notification_cards\":false}").unwrap();
        assert!(!migrated.ai_notification_cards);
        assert!(migrated.ai_compact_replies && migrated.notification_adaptive_size);
        for sid in [Sid::AiCompactReplies, Sid::NotificationAdaptiveSize] {
            toggle_set(sid, &mut settings, false);
            let saved: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert!(!toggle_get(sid, &saved));
        }
    }

    #[test]
    fn search_finds_rows_in_both_languages() {
        assert!(matches("Synced lyrics", "lyr"));
        assert!(matches("Synced lyrics", "  "));
        assert!(!matches("Synced lyrics", "battery"));
    }

    #[test]
    fn assistant_pages_keep_setup_and_controls_in_one_section() {
        let mut ui = UiState::new();
        let group = tab_settings_group(Tab::Chat).unwrap();
        ui.sw.group = group;
        ui.cfg.ai.voice_separate = true;
        ui.cfg.assistant.natural_voice = true;
        assert_eq!(GROUPS[group].name, "Assistant");
        assert!(!GROUPS.iter().any(|group| group.name == "AI"));
        for scale in [0.8, 1.0, 1.5, 2.0] {
            let size = (SW_SIZE.0 * scale, SW_SIZE.1 * scale);
            let nav = layout(size, scale, &ui).assistant_nav.unwrap();
            for (page, _, rect) in assistant_nav_rects(nav, scale) {
                assert_eq!(
                    sw_hit(size, scale, &ui, (rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0),
                    Some(SwHit::AssistantPage(page)),
                );
            }
        }
        ui.sw.assistant_page = AssistantPage::Model;
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Field(Field::AiKey, ..))));
        ui.sw.assistant_page = AssistantPage::Recognition;
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Field(Field::VoiceKey, ..))));
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Pair(Act::Rebind(Hotkey::Dictate), ..))));
        ui.cfg.ai.voice_separate = false;
        assert!(!items(&ui).iter().any(|item| matches!(item, Item::Field(Field::VoiceKey, ..))));
        let mut guide = GuideUi::new(&ui.cfg, false);
        guide.toggle_speech_service();
        guide.visible = false;
        ui.sw.guide = Some(guide);
        let rows = items(&ui);
        assert!(rows.iter().any(|item| matches!(item, Item::Field(Field::VoiceKey, ..))));
        assert!(rows.iter().any(|item| matches!(item, Item::Button(Act::VoiceSave, ..))));
        assert!(!ui.cfg.ai.voice_separate);
        ui.sw.guide = None;
        ui.sw.assistant_page = AssistantPage::Voice;
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Row(Sid::AsstVolume, ..))));
        ui.sw.search = "Voice API key".into();
        assert!(layout(SW_SIZE, 1.0, &ui).assistant_nav.is_none());
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Field(Field::TtsKey, ..))));
        assert!(!items(&ui).iter().any(|item| matches!(item, Item::Field(Field::AiKey, ..))));
    }

    #[test]
    fn every_group_row_is_unique_and_groups_have_content() {
        let mut seen = Vec::new();
        for g in &GROUPS {
            assert!(!g.rows.is_empty() || g.custom.is_some(), "{} is empty", g.name);
            for (id, ..) in g.rows {
                assert!(!seen.contains(id), "{id:?} listed twice");
                seen.push(*id);
            }
        }
    }

    #[test]
    fn desktop_character_preserves_the_selected_pet_and_is_searchable() {
        let mut ui = UiState::new();
        ui.cfg.character = crate::ui::characters::Character::Cat;
        toggle_set(Sid::CharacterDetached, &mut ui.cfg, true);
        assert!(ui.cfg.character_detached);
        assert_eq!(ui.cfg.character, crate::ui::characters::Character::Cat);
        ui.sw.search = "release".into();
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Row(Sid::CharacterDetached, ..))));
        toggle_set(Sid::CharacterDetached, &mut ui.cfg, false);
        assert!(!ui.cfg.character_detached);
        assert_eq!(ui.cfg.character, crate::ui::characters::Character::Cat);
    }

    #[test]
    fn pet_autonomy_can_be_disabled_without_docking_or_changing_the_pet() {
        let mut ui = UiState::new();
        ui.cfg.character_detached = true;
        let character = ui.cfg.character;
        toggle_set(Sid::PetAutonomy, &mut ui.cfg, false);
        assert!(!toggle_get(Sid::PetAutonomy, &ui.cfg));
        assert!(ui.cfg.character_detached);
        assert_eq!(ui.cfg.character, character);
        ui.sw.search = "sleeps".into();
        assert!(items(&ui).iter().any(|item| matches!(item, Item::Row(Sid::PetAutonomy, ..))));
        toggle_set(Sid::PetAutonomy, &mut ui.cfg, true);
        assert!(toggle_get(Sid::PetAutonomy, &ui.cfg));
    }

    #[test]
    fn the_integration_group_follows_the_services_state() {
        let mut ui = UiState::new();
        ui.sw.group = GROUPS.iter().position(|g| g.name == "Integrations").unwrap();
        ui.discord.status = discord::Status::NoConfig;
        let discord_fields = |ui: &UiState| {
            items(ui).iter().filter(|i| matches!(i, Item::Field(Field::DiscordId | Field::DiscordSecret, ..))).count()
        };
        assert_eq!(discord_fields(&ui), 2);
        let save_enabled = |ui: &UiState| {
            items(ui).iter().any(|i| matches!(i, Item::Button(Act::DiscordMain, label, true) if label == "Save"))
        };
        assert!(!save_enabled(&ui));
        ui.int.id.set_text("123456789012345678");
        ui.int.secret.set_text("secret");
        assert!(save_enabled(&ui));
        ui.discord.status = discord::Status::Connected;
        assert_eq!(discord_fields(&ui), 0);
        assert!(items(&ui).iter().any(|i| matches!(i, Item::Button(Act::DiscordMute, ..))));
    }
}
