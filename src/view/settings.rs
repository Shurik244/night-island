use crate::view::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sid {
    InterfaceFont,
    LegacyIcons,
    HeaderIcons,
    HeaderIconColors,
    HeaderActiveAccent,
    MusicTrackIntro,
    PetMusicCuriosity,
    PetReturnAnimation,
    LiquidIdleTravel,
    TopEnabled,
    TopAutoHide,
    TopTray,
    TopApplication,
    TopWifi,
    TopBluetooth,
    TopVolume,
    TopBattery,
    TopFullscreen,
    TopReserve,
    TopClearControls,
    TopMicrophone,
    TopKeyboard,
    TopWorkspaces,
    TopAnimations,
    TopEntrance,
    TopGap,
    TopMotion,
    TopLiquid,

    DockEnabled,
    DockReplaceTaskbar,
    DockExperimentalCapture,
    DockAutoHide,
    DockRunning,
    DockPreview,
    DockFullscreen,
    DockEconomy,
    DockLockPins,
    DockSeparator,
    DockShowAdd,
    DockImportTaskbar,
    DockAnimations,
    DockIconSize,
    DockZoom,
    DockOpacity,
    DockBlur,
    DockTintHue,
    DockTintSaturation,
    DockTintBrightness,
    DockMargin,
    DockMonitor,
    DockImport,
    DockAdd,
    DockTaskbar,

    PillBlur,
    Autostart,
    PillOpacity,
    AgyAuto,
    VoicePaste,
    VoiceSeparate,
    PanelHue,
    PanelSat,
    PanelVal,
    AccentHue,
    AccentSat,
    Language,
    Weather,
    Placement,
    Edge,
    Style,
    Size,
    LiquidReach,
    LiquidStrength,
    LiquidSoftness,
    Tint,
    PillWeather,
    PillWeatherSide,
    PillBattery,
    MusicEq,
    MusicTitle,
    MusicCover,
    PreferPlayingMedia,
    MusicLyrics,
    MusicProgress,
    PillUnifiedLayout,
    EqSmooth,
    HomeWidgets,
    HomeWidgetDetails,
    HomeWeatherWidget,
    LiveActivities,
    LiveMusicPriority,
    LiveReminders,
    LiveRecording,
    LiveDownloads,
    CoverMode,
    FrameRate,
    EqStyle,
    EqResponse,
    EqStrength,
    EqGradient,
    EqWhenOpen,
    BumpTint,
    ProgressStyle,
    TabClipboard,
    TabText,
    TabNote,
    TabWeather,
    TabRecord,
    TabAi,
    TabPrivacy,
    TabActivity,
    TabSteam,
    TabSystem,
    MetricsInterval,
    MemoryTools,
    MemoryManager,
    TabControls,
    TabMixer,
    TabNotifications,
    TabYandex,
    SystemNotifications,
    WaveRatings,
    WaveLegacyLayout,
    WaveSearch,
    NotificationTiles,
    NotificationOpenApp,
    AiNotificationCards,
    AiCompactReplies,
    NotificationAdaptiveSize,
    PetNotifications,
    WindowsNotificationsSettings,
    NotifyMic,
    NotifyDownloads,
    NotifyCapture,
    NotifyRecording,
    NotifyAgy,
    NotifyIntegrations,
    NotifyErrors,
    NotifyClipboard,
    NotifyClipboardText,
    NotifyLockKeys,
    NotifyBattery,
    OpenLogs,
    DevTools,
    AiProvider,
    NotifyAi,
    NotifyUpdate,
    NotifyPrivacy,
    NotifyAgents,
    NotifySteam,
    ActivityTrack,
    ActivityAi,
    VpnPill,
    SharedLyrics,
    UpdateCheck,
    UpdateBeta,
    SpikeColor,
    Spikes,
    Character,
    CharacterDetached,
    PetPerformance,
    PetAutonomy,
    PetVoice,
    PetVoiceInput,
    RecFormat,
    RecFps,
    RecSystem,
    RecMic,
    OpenDelay,
    CloseDelay,
    OpenSpeed,
    IslandAnimations,
    TabAnimKind,
    TextLayout,
    TabAnimSpeed,
    AnimCurve,
    CloseSpeed,
    AutoHide,
    LockDrag,
    Acrylic,
    Capture,
    CaptureSelf,
    GameMode,
    GameAi,
    GameWindowed,
    AiCheckOnSave,
    HidePillSecs,
    GhostMode,
    GhostOpacity,
    GhostThrough,
    GhostHold,
    AsstEnabled,
    AsstWake,
    AsstSounds,
    SoundEnabled,
    SoundVolume,
    SoundPreset,
    AsstSpeak,
    AsstVolume,
    AsstInstall,
    AsstReview,
    AsstWebAsk,
    AsstFollowUp,
    AsstNatural,
    AsstFishStream,
    AsstFishExpressions,
    AsstYandex,
    AsstMedia,
    AsstControls,
    AsstOpen,
    AsstReminders,
    AsstMemory,
    AsstWebSearch,
    AsstFilesRead,
    AsstFilesWrite,
    AsstShell,
    AsstBrowser,
    AsstPower,
    AsstScreen,
    AsstIsland,
    AsstSteps,
    AsstTokens,
    AsstSeconds,
    AsstOutput,
    AsstInstructions,
    AsstLocal,
    AsstLocalInstall,
    AsstTtsProvider,
}

pub(in crate::view) fn asst_cap(id: Sid) -> Option<crate::services::assistant::Cap> {
    use crate::services::assistant::Cap;
    Some(match id {
        Sid::AsstMedia => Cap::Media,
        Sid::AsstControls => Cap::Controls,
        Sid::AsstOpen => Cap::Open,
        Sid::AsstReminders => Cap::Reminders,
        Sid::AsstMemory => Cap::Memory,
        Sid::AsstWebSearch => Cap::WebSearch,
        Sid::AsstFilesRead => Cap::FilesRead,
        Sid::AsstFilesWrite => Cap::FilesWrite,
        Sid::AsstShell => Cap::Shell,
        Sid::AsstBrowser => Cap::Browser,
        Sid::AsstPower => Cap::Power,
        Sid::AsstScreen => Cap::Screen,
        Sid::AsstIsland => Cap::Island,
        _ => return None,
    })
}

pub fn is_assistant_level(id: Sid) -> bool {
    asst_cap(id).is_some()
}

#[derive(Clone, Copy)]
pub(in crate::view) enum Kind {
    Segment([&'static str; 2]),
    Cycle,
    Slider,
    Toggle,
    Button,
}

pub(in crate::view) fn tab_toggle_name(id: Sid) -> Option<&'static str> {
    Some(match id {
        Sid::TabClipboard => "Clipboard",
        Sid::TabText => "Text",
        Sid::TabNote => "Note",
        Sid::TabWeather => "Weather",
        Sid::TabRecord => "Record",
        Sid::TabAi => "AI",
        Sid::TabPrivacy => "Privacy",
        Sid::TabActivity => "Activity",
        Sid::TabSteam => "Steam",
        Sid::TabSystem => "System",
        Sid::TabControls => "Controls",
        Sid::TabMixer => "App mixer",
        Sid::TabNotifications => "Notifications",
        Sid::TabYandex => "My Wave",
        _ => return None,
    })
}

pub(in crate::view) const RECORD_ROWS: [(Sid, &str, Kind); 4] = [
    (Sid::RecFormat, "Format", Kind::Segment(["MP4", "GIF"])),
    (Sid::RecFps, "Frame rate", Kind::Slider),
    (Sid::RecSystem, "System sound", Kind::Toggle),
    (Sid::RecMic, "Microphone", Kind::Toggle),
];

pub(in crate::view) const THEME_ROWS: [(Sid, &str, Kind); 9] = [
    (Sid::PanelHue, "Panel hue", Kind::Slider),
    (Sid::PanelSat, "Panel saturation", Kind::Slider),
    (Sid::PanelVal, "Panel brightness", Kind::Slider),
    (Sid::Tint, "Panel opacity", Kind::Slider),
    (Sid::Acrylic, "Panel blur", Kind::Toggle),
    (Sid::PillOpacity, "Pill opacity", Kind::Slider),
    (Sid::PillBlur, "Pill blur", Kind::Toggle),
    (Sid::AccentHue, "Accent hue", Kind::Slider),
    (Sid::AccentSat, "Accent saturation", Kind::Slider),
];

pub(in crate::view) fn rows_for(tab: Tab) -> &'static [(Sid, &'static str, Kind)] {
    match tab {
        Tab::Record => &RECORD_ROWS,
        _ => &[],
    }
}

pub(in crate::view) const SET_ROW_H: f32 = 30.0;

pub(in crate::view) const SET_PER_COL: usize = 10;

pub(in crate::view) const SET_COL_GAP: f32 = 28.0;

pub(in crate::view) const SET_LABEL_W: f32 = 136.0;

pub(in crate::view) fn setting_row(pill: Rect, scale: f32, index: usize) -> (Rect, Rect) {
    let (x, y, w, _) = pill;
    let (side, gap) = (SIDE_PAD * scale, SET_COL_GAP * scale);
    let col_w = (w - 2.0 * side - gap) / 2.0;
    let (col, row) = ((index / SET_PER_COL) as f32, (index % SET_PER_COL) as f32);
    let left = x + side + col * (col_w + gap);
    let top = y + (LIST_TOP + 2.0) * scale + row * SET_ROW_H * scale;
    let rect = (left, top, left + col_w, top + SET_ROW_H * scale);
    let ctl = (left + SET_LABEL_W * scale, top + 4.0 * scale, left + col_w, top + (SET_ROW_H - 4.0) * scale);
    (rect, ctl)
}

pub(in crate::view) fn slider_range(id: Sid) -> (f32, f32, f32) {
    match id {
        Sid::TopGap => (12.0, 160.0, 2.0),
        Sid::TopMotion => (160.0, 800.0, 20.0),
        Sid::DockIconSize => (32.0, 72.0, 2.0),
        Sid::DockZoom => (1.0, 1.7, 0.05),
        Sid::DockOpacity => (0.2, 1.0, 0.02),
        Sid::DockTintHue => (0.0, 360.0, 1.0),
        Sid::DockTintSaturation => (0.0, 1.0, 0.01),
        Sid::DockTintBrightness => (0.02, 0.35, 0.01),
        Sid::DockMargin => (0.0, 40.0, 1.0),
        Sid::Size => (80.0, 200.0, 5.0),
        Sid::LiquidReach => (60.0, 300.0, 5.0),
        Sid::LiquidStrength => (6.0, 40.0, 1.0),
        Sid::EqStrength => (0.4, 2.0, 0.1),
        Sid::LiquidSoftness => (0.0, 1.0, 0.05),
        Sid::Tint => (0.2, 1.0, 0.05),
        Sid::PillOpacity => (0.2, 1.0, 0.05),
        Sid::PanelHue | Sid::AccentHue => (0.0, 360.0, 1.0),
        Sid::PanelSat | Sid::AccentSat => (0.0, 1.0, 0.01),
        Sid::PanelVal => (0.0, 0.3, 0.005),
        Sid::OpenDelay => (0.0, 1000.0, 10.0),
        Sid::CloseDelay => (0.0, 3000.0, 50.0),
        Sid::OpenSpeed | Sid::CloseSpeed | Sid::TabAnimSpeed => (0.4, 2.5, 0.1),
        Sid::RecFps => (10.0, 60.0, 5.0),
        Sid::HidePillSecs => (5.0, 10.0, 1.0),
        Sid::MetricsInterval => (1.0, 10.0, 1.0),
        Sid::AsstVolume | Sid::SoundVolume => (0.0, 100.0, 1.0),
        Sid::AsstSteps => (3.0, 20.0, 1.0),
        Sid::AsstTokens => (500.0, 16_000.0, 100.0),
        Sid::AsstSeconds => (5.0, 180.0, 5.0),
        Sid::AsstOutput => (500.0, 20_000.0, 500.0),
        Sid::GhostOpacity => (0.05, 1.0, 0.05),
        _ => (0.0, 1.0, 0.01),
    }
}

pub(in crate::view) fn slider_value(id: Sid, c: &Settings) -> f32 {
    match id {
        Sid::TopGap => c.top_panel.island_gap as f32,
        Sid::TopMotion => c.top_panel.motion_ms as f32,
        Sid::DockIconSize => c.desktop_dock.icon_size as f32,
        Sid::DockZoom => c.desktop_dock.zoom,
        Sid::DockOpacity => {
            if c.desktop_dock.blur {
                c.desktop_dock.blur_opacity
            } else {
                c.desktop_dock.opacity
            }
        }
        Sid::DockTintHue => c.desktop_dock.tint_hue,
        Sid::DockTintSaturation => c.desktop_dock.tint_saturation,
        Sid::DockTintBrightness => c.desktop_dock.tint_brightness,
        Sid::DockMargin => c.desktop_dock.margin as f32,
        Sid::Size => c.scale_percent as f32,
        Sid::LiquidReach => c.liquid_reach,
        Sid::LiquidStrength => c.liquid_strength,
        Sid::EqStrength => c.eq_strength,
        Sid::LiquidSoftness => c.liquid_softness,
        Sid::Tint => c.tint_open,
        Sid::PillOpacity => c.pill_opacity,
        Sid::PanelHue => c.panel_hue,
        Sid::PanelSat => c.panel_sat,
        Sid::PanelVal => c.panel_val,
        Sid::AccentHue => c.accent_hue,
        Sid::AccentSat => c.accent_sat,
        Sid::OpenDelay => c.open_delay_ms as f32,
        Sid::CloseDelay => c.close_delay_ms as f32,
        Sid::OpenSpeed => c.open_speed,
        Sid::TabAnimSpeed => c.tab_anim_speed,
        Sid::CloseSpeed => c.close_speed,
        Sid::RecFps => c.record_fps as f32,
        Sid::HidePillSecs => c.hide_pill_secs as f32,
        Sid::MetricsInterval => c.metrics_secs as f32,
        Sid::SoundVolume => f32::from(c.sounds.volume),
        Sid::AsstVolume => f32::from(c.assistant.volume),
        Sid::AsstSteps => f32::from(c.assistant.max_steps),
        Sid::AsstTokens => f32::from(c.assistant.reply_tokens),
        Sid::AsstSeconds => f32::from(c.assistant.command_secs),
        Sid::AsstOutput => f32::from(c.assistant.output_chars),
        Sid::GhostOpacity => c.ghost_opacity,
        _ => 0.0,
    }
}

pub(in crate::view) fn slider_text(id: Sid, v: f32) -> String {
    match id {
        Sid::TopGap => format!("{v:.0} px"),
        Sid::TopMotion => format!("{v:.0} ms"),
        Sid::DockIconSize | Sid::DockMargin => format!("{v:.0} px"),
        Sid::DockZoom => format!("{v:.2}×"),
        Sid::DockOpacity | Sid::DockTintSaturation | Sid::DockTintBrightness => format!("{:.0}%", v * 100.0),
        Sid::DockTintHue => format!("{v:.0}°"),
        Sid::Size => format!("{v:.0}%"),
        Sid::LiquidReach | Sid::LiquidStrength => format!("{v:.0} px"),
        Sid::EqStrength => format!("\u{00D7}{v:.1}"),
        Sid::LiquidSoftness | Sid::Tint | Sid::PillOpacity | Sid::GhostOpacity | Sid::PanelSat | Sid::AccentSat => {
            format!("{:.0}%", v * 100.0)
        }
        Sid::PanelHue | Sid::AccentHue => format!("{v:.0}°"),
        Sid::PanelVal => format!("{:.0}%", v / 0.3 * 100.0),
        Sid::OpenDelay | Sid::CloseDelay => format!("{v:.0} ms"),
        Sid::OpenSpeed | Sid::CloseSpeed | Sid::TabAnimSpeed => format!("{v:.1}×"),
        Sid::RecFps => format!("{v:.0} fps"),
        Sid::HidePillSecs | Sid::MetricsInterval => format!("{v:.0} s"),
        Sid::AsstVolume | Sid::SoundVolume => format!("{v:.0}%"),
        Sid::AsstSteps | Sid::AsstTokens | Sid::AsstSeconds | Sid::AsstOutput => format!("{v:.0}"),
        _ => String::new(),
    }
}

pub fn slider_apply(id: Sid, c: &mut Settings, norm: f32) {
    let (min, max, step) = slider_range(id);
    let raw = min + (max - min) * norm.clamp(0.0, 1.0);
    let v = (raw / step).round() * step;
    match id {
        Sid::TopGap => c.top_panel.island_gap = v as u32,
        Sid::TopMotion => c.top_panel.motion_ms = v as u32,
        Sid::DockIconSize => c.desktop_dock.icon_size = v as u32,
        Sid::DockZoom => c.desktop_dock.zoom = v,
        Sid::DockOpacity => {
            if c.desktop_dock.blur {
                c.desktop_dock.blur_opacity = v
            } else {
                c.desktop_dock.opacity = v
            }
        }
        Sid::DockTintHue => c.desktop_dock.tint_hue = v,
        Sid::DockTintSaturation => c.desktop_dock.tint_saturation = v,
        Sid::DockTintBrightness => c.desktop_dock.tint_brightness = v,
        Sid::DockMargin => c.desktop_dock.margin = v as u32,
        Sid::Size => c.scale_percent = v as u32,
        Sid::LiquidReach => c.liquid_reach = v,
        Sid::LiquidStrength => c.liquid_strength = v,
        Sid::EqStrength => c.eq_strength = v,
        Sid::LiquidSoftness => c.liquid_softness = v,
        Sid::Tint => c.tint_open = v,
        Sid::PillOpacity => c.pill_opacity = v,
        Sid::PanelHue => c.panel_hue = v,
        Sid::PanelSat => c.panel_sat = v,
        Sid::PanelVal => c.panel_val = v,
        Sid::AccentHue => c.accent_hue = v,
        Sid::AccentSat => c.accent_sat = v,
        Sid::OpenDelay => c.open_delay_ms = v as u32,
        Sid::CloseDelay => c.close_delay_ms = v as u32,
        Sid::OpenSpeed => c.open_speed = v,
        Sid::TabAnimSpeed => c.tab_anim_speed = v,
        Sid::CloseSpeed => c.close_speed = v,
        Sid::RecFps => c.record_fps = v as u32,
        Sid::HidePillSecs => c.hide_pill_secs = v as u32,
        Sid::MetricsInterval => c.metrics_secs = v as u32,
        Sid::SoundVolume => c.sounds.volume = v as u8,
        Sid::AsstVolume => c.assistant.volume = v as u8,
        Sid::AsstSteps => c.assistant.max_steps = v as u8,
        Sid::AsstTokens => c.assistant.reply_tokens = v as u16,
        Sid::AsstSeconds => c.assistant.command_secs = v as u8,
        Sid::AsstOutput => c.assistant.output_chars = v as u16,
        Sid::GhostOpacity => c.ghost_opacity = v,
        _ => {}
    }
    c.sanitize();
}

pub fn toggle_get(id: Sid, c: &Settings) -> bool {
    toggle_value(id, c)
}

pub(in crate::view) fn toggle_value(id: Sid, c: &Settings) -> bool {
    if let Some(name) = tab_toggle_name(id) {
        return !c.tab_hidden(name);
    }
    match id {
        Sid::LegacyIcons => c.legacy_icons,
        Sid::HeaderIcons => c.header_icons,
        Sid::HeaderIconColors => c.header_icon_colors,
        Sid::HeaderActiveAccent => c.header_active_accent,
        Sid::MusicTrackIntro => c.music_track_intro,
        Sid::PetMusicCuriosity => c.pet_music_curiosity,
        Sid::PetReturnAnimation => c.pet_return_animation,
        Sid::LiquidIdleTravel => c.liquid_idle_travel,
        Sid::IslandAnimations => c.island_animations,
        Sid::TopAutoHide => c.top_panel.auto_hide,
        Sid::TopTray => c.top_panel.tray,
        Sid::DockExperimentalCapture => c.desktop_dock.experimental_capture,
        Sid::DockReplaceTaskbar => c.desktop_dock.replace_taskbar,
        Sid::TopEnabled => c.top_panel.enabled,
        Sid::TopApplication => c.top_panel.application,
        Sid::TopWifi => c.top_panel.wifi,
        Sid::TopBluetooth => c.top_panel.bluetooth,
        Sid::TopVolume => c.top_panel.volume,
        Sid::TopBattery => c.top_panel.battery,
        Sid::TopFullscreen => c.top_panel.hide_fullscreen,
        Sid::TopReserve => c.top_panel.reserve_space,
        Sid::TopClearControls => c.top_panel.clear_window_controls,
        Sid::TopMicrophone => c.top_panel.microphone,
        Sid::TopKeyboard => c.top_panel.keyboard,
        Sid::TopWorkspaces => c.top_panel.workspaces,
        Sid::TopAnimations => c.top_panel.animations,
        Sid::TopLiquid => c.top_panel.liquid,
        Sid::PetVoice => c.pet_voice_reactions,
        Sid::PetVoiceInput => c.pet_voice_input,
        Sid::DockPreview => c.desktop_dock.hover_preview,
        Sid::DockBlur => c.desktop_dock.blur,
        Sid::DockEnabled => c.desktop_dock.enabled,
        Sid::DockAutoHide => c.desktop_dock.auto_hide,
        Sid::DockRunning => c.desktop_dock.show_running,
        Sid::DockFullscreen => c.desktop_dock.hide_fullscreen,
        Sid::DockEconomy => c.desktop_dock.economy,
        Sid::DockLockPins => c.desktop_dock.lock_pins,
        Sid::DockSeparator => c.desktop_dock.separator,
        Sid::DockShowAdd => c.desktop_dock.show_add,
        Sid::MemoryTools => c.memory_tools,
        Sid::DockAnimations => c.desktop_dock.animations,
        Sid::AutoHide => c.auto_hide,
        Sid::LockDrag => c.lock_drag,
        Sid::Acrylic => c.acrylic,
        Sid::Capture => c.capture_hidden,
        Sid::CaptureSelf => c.capture_self_hidden,
        Sid::GameMode => c.game_mode,
        Sid::GameAi => c.game_ai,
        Sid::GameWindowed => c.game_windowed,
        Sid::AiCheckOnSave => c.ai_check_on_save,
        Sid::GhostMode => c.ghost_mode,
        Sid::GhostThrough => c.ghost_click_through,
        Sid::GhostHold => c.ghost_hold,
        Sid::CharacterDetached => c.character_detached,
        Sid::PetAutonomy => c.pet_autonomy,
        Sid::Spikes => c.edge_spikes,
        Sid::PillWeather => c.pill_weather,
        Sid::MusicEq => c.music_eq,
        Sid::MusicTitle => c.music_title,
        Sid::MusicCover => c.music_cover,
        Sid::PreferPlayingMedia => c.prefer_playing_media,
        Sid::MusicLyrics => c.music_lyrics,
        Sid::MusicProgress => c.music_progress,
        Sid::PillUnifiedLayout => c.pill_unified_layout,
        Sid::EqSmooth => c.eq_smooth,
        Sid::LiveMusicPriority => c.live_music_priority,
        Sid::HomeWidgets => c.home_widgets,
        Sid::HomeWidgetDetails => c.home_widget_details,
        Sid::HomeWeatherWidget => c.home_weather_widget,
        Sid::LiveActivities => c.live_activities,
        Sid::LiveReminders => c.live_reminders,
        Sid::LiveRecording => c.live_recording,
        Sid::LiveDownloads => c.live_downloads,
        Sid::EqWhenOpen => c.eq_when_open,
        Sid::DevTools => c.dev_tools,
        Sid::NotifyAi => c.notify.ai,
        Sid::NotifyUpdate => c.notify.update,
        Sid::NotifyPrivacy => c.notify.privacy,
        Sid::NotifyAgents => c.notify.agents,
        Sid::NotifySteam => c.notify.steam,
        Sid::ActivityTrack => c.activity_track,
        Sid::ActivityAi => c.activity_ai,
        Sid::VpnPill => c.vpn_pill,
        Sid::SharedLyrics => c.shared_lyrics,
        Sid::UpdateCheck => c.update_check,
        Sid::UpdateBeta => c.update_beta,
        Sid::SystemNotifications => c.system_notifications,
        Sid::WaveLegacyLayout => c.wave_legacy_layout,
        Sid::WaveRatings => c.wave_ratings,
        Sid::WaveSearch => c.wave_search,
        Sid::NotificationOpenApp => c.notification_open_app,
        Sid::AiNotificationCards => c.ai_notification_cards,
        Sid::AiCompactReplies => c.ai_compact_replies,
        Sid::NotificationAdaptiveSize => c.notification_adaptive_size,
        Sid::PetNotifications => c.pet_notifications,
        Sid::NotifyMic => c.notify.mic,
        Sid::NotifyDownloads => c.notify.downloads,
        Sid::NotifyCapture => c.notify.capture,
        Sid::NotifyRecording => c.notify.recording,
        Sid::NotifyAgy => c.notify.agy,
        Sid::NotifyIntegrations => c.notify.integrations,
        Sid::NotifyErrors => c.notify.errors,
        Sid::NotifyClipboard => c.notify.clipboard,
        Sid::NotifyClipboardText => c.notify.clipboard_text,
        Sid::NotifyLockKeys => c.notify.lock_keys,
        Sid::NotifyBattery => c.notify.battery,
        Sid::SpikeColor => c.spike_color,
        Sid::PillBlur => c.pill_blur,
        Sid::Autostart => crate::platform::autostart::enabled(),
        Sid::Weather => c.weather,
        Sid::AgyAuto => c.agy_auto,
        Sid::VoicePaste => c.voice_paste,
        Sid::VoiceSeparate => c.ai.voice_separate,
        Sid::AsstEnabled => c.assistant.perms.enabled,
        Sid::AsstWake => c.assistant.wake,
        Sid::SoundEnabled => c.sounds.enabled,
        Sid::AsstSounds => c.assistant.sounds,
        Sid::AsstSpeak => c.assistant.speak,
        Sid::AsstReview => c.assistant.review,
        Sid::AsstWebAsk => c.assistant.ask_after_web,
        Sid::AsstFollowUp => c.assistant.follow_up,
        Sid::AsstNatural => c.assistant.natural_voice,
        Sid::AsstFishStream => c.assistant.fish_streaming,
        Sid::AsstFishExpressions => c.assistant.fish_expressions,
        Sid::AsstYandex => c.assistant.yandex_voice,
        Sid::AsstLocal => c.assistant.local_voice,
        Sid::RecSystem => c.record_system_audio,
        Sid::RecMic => c.record_mic,
        _ => false,
    }
}

pub fn toggle_set(id: Sid, c: &mut Settings, on: bool) {
    if let Some(name) = tab_toggle_name(id) {
        return c.set_tab_hidden(name, !on);
    }
    match id {
        Sid::LegacyIcons => c.legacy_icons = on,
        Sid::HeaderIcons => c.header_icons = on,
        Sid::HeaderIconColors => c.header_icon_colors = on,
        Sid::HeaderActiveAccent => c.header_active_accent = on,
        Sid::MusicTrackIntro => c.music_track_intro = on,
        Sid::PetMusicCuriosity => c.pet_music_curiosity = on,
        Sid::PetReturnAnimation => c.pet_return_animation = on,
        Sid::LiquidIdleTravel => c.liquid_idle_travel = on,
        Sid::IslandAnimations => c.island_animations = on,
        Sid::TopAutoHide => c.top_panel.auto_hide = on,
        Sid::TopTray => c.top_panel.tray = on,
        Sid::DockExperimentalCapture => c.desktop_dock.experimental_capture = on,
        Sid::DockReplaceTaskbar => c.desktop_dock.replace_taskbar = on,
        Sid::TopEnabled => c.top_panel.enabled = on,
        Sid::TopApplication => c.top_panel.application = on,
        Sid::TopWifi => c.top_panel.wifi = on,
        Sid::TopBluetooth => c.top_panel.bluetooth = on,
        Sid::TopVolume => c.top_panel.volume = on,
        Sid::TopBattery => c.top_panel.battery = on,
        Sid::TopFullscreen => c.top_panel.hide_fullscreen = on,
        Sid::TopReserve => c.top_panel.reserve_space = on,
        Sid::TopClearControls => c.top_panel.clear_window_controls = on,
        Sid::TopMicrophone => c.top_panel.microphone = on,
        Sid::TopKeyboard => c.top_panel.keyboard = on,
        Sid::TopWorkspaces => c.top_panel.workspaces = on,
        Sid::TopAnimations => c.top_panel.animations = on,
        Sid::TopLiquid => c.top_panel.liquid = on,
        Sid::PetVoice => c.pet_voice_reactions = on,
        Sid::PetVoiceInput => c.pet_voice_input = on,
        Sid::DockPreview => c.desktop_dock.hover_preview = on,
        Sid::DockBlur => c.desktop_dock.blur = on,
        Sid::DockEnabled => c.desktop_dock.enabled = on,
        Sid::DockAutoHide => c.desktop_dock.auto_hide = on,
        Sid::DockRunning => c.desktop_dock.show_running = on,
        Sid::DockFullscreen => c.desktop_dock.hide_fullscreen = on,
        Sid::DockEconomy => c.desktop_dock.economy = on,
        Sid::DockLockPins => c.desktop_dock.lock_pins = on,
        Sid::DockSeparator => c.desktop_dock.separator = on,
        Sid::DockShowAdd => c.desktop_dock.show_add = on,
        Sid::MemoryTools => c.memory_tools = on,
        Sid::DockAnimations => c.desktop_dock.animations = on,
        Sid::AutoHide => c.auto_hide = on,
        Sid::LockDrag => c.lock_drag = on,
        Sid::Acrylic => c.acrylic = on,
        Sid::Capture => c.capture_hidden = on,
        Sid::CaptureSelf => c.capture_self_hidden = on,
        Sid::GameMode => c.game_mode = on,
        Sid::GameAi => c.game_ai = on,
        Sid::GameWindowed => c.game_windowed = on,
        Sid::AiCheckOnSave => c.ai_check_on_save = on,
        Sid::GhostMode => c.ghost_mode = on,
        Sid::GhostThrough => c.ghost_click_through = on,
        Sid::GhostHold => c.ghost_hold = on,
        Sid::PetAutonomy => c.pet_autonomy = on,
        Sid::CharacterDetached => {
            c.character_detached = on;
            if on && c.character == crate::ui::characters::Character::None {
                c.character = crate::ui::characters::Character::Spark;
            }
        }
        Sid::Spikes => c.edge_spikes = on,
        Sid::PillWeather => c.pill_weather = on,
        Sid::MusicEq => c.music_eq = on,
        Sid::MusicTitle => c.music_title = on,
        Sid::MusicCover => c.music_cover = on,
        Sid::PreferPlayingMedia => c.prefer_playing_media = on,
        Sid::MusicLyrics => c.music_lyrics = on,
        Sid::MusicProgress => c.music_progress = on,
        Sid::PillUnifiedLayout => c.pill_unified_layout = on,
        Sid::EqSmooth => c.eq_smooth = on,
        Sid::LiveMusicPriority => c.live_music_priority = on,
        Sid::HomeWidgets => c.home_widgets = on,
        Sid::HomeWidgetDetails => c.home_widget_details = on,
        Sid::HomeWeatherWidget => c.home_weather_widget = on,
        Sid::LiveActivities => c.live_activities = on,
        Sid::LiveReminders => c.live_reminders = on,
        Sid::LiveRecording => c.live_recording = on,
        Sid::LiveDownloads => c.live_downloads = on,
        Sid::EqWhenOpen => c.eq_when_open = on,
        Sid::DevTools => c.dev_tools = on,
        Sid::NotifyAi => c.notify.ai = on,
        Sid::NotifyUpdate => c.notify.update = on,
        Sid::NotifyPrivacy => c.notify.privacy = on,
        Sid::NotifyAgents => c.notify.agents = on,
        Sid::NotifySteam => c.notify.steam = on,
        Sid::ActivityTrack => c.activity_track = on,
        Sid::ActivityAi => c.activity_ai = on,
        Sid::VpnPill => c.vpn_pill = on,
        Sid::SharedLyrics => c.shared_lyrics = on,
        Sid::UpdateCheck => c.update_check = on,
        Sid::UpdateBeta => c.update_beta = on,
        Sid::SystemNotifications => c.system_notifications = on,
        Sid::WaveLegacyLayout => c.wave_legacy_layout = on,
        Sid::WaveRatings => c.wave_ratings = on,
        Sid::WaveSearch => c.wave_search = on,
        Sid::NotificationOpenApp => c.notification_open_app = on,
        Sid::AiNotificationCards => c.ai_notification_cards = on,
        Sid::AiCompactReplies => c.ai_compact_replies = on,
        Sid::NotificationAdaptiveSize => c.notification_adaptive_size = on,
        Sid::PetNotifications => c.pet_notifications = on,
        Sid::NotifyMic => c.notify.mic = on,
        Sid::NotifyDownloads => c.notify.downloads = on,
        Sid::NotifyCapture => c.notify.capture = on,
        Sid::NotifyRecording => c.notify.recording = on,
        Sid::NotifyAgy => c.notify.agy = on,
        Sid::NotifyIntegrations => c.notify.integrations = on,
        Sid::NotifyErrors => c.notify.errors = on,
        Sid::NotifyClipboard => c.notify.clipboard = on,
        Sid::NotifyClipboardText => c.notify.clipboard_text = on,
        Sid::NotifyLockKeys => c.notify.lock_keys = on,
        Sid::NotifyBattery => c.notify.battery = on,
        Sid::SpikeColor => c.spike_color = on,
        Sid::PillBlur => c.pill_blur = on,
        Sid::Autostart => crate::platform::autostart::set(on),
        Sid::Weather => c.weather = on,
        Sid::AgyAuto => c.agy_auto = on,
        Sid::VoicePaste => c.voice_paste = on,
        Sid::VoiceSeparate => c.ai.voice_separate = on,
        Sid::AsstEnabled => c.assistant.perms.enabled = on,
        Sid::AsstWake => c.assistant.wake = on,
        Sid::SoundEnabled => c.sounds.enabled = on,
        Sid::AsstSounds => c.assistant.sounds = on,
        Sid::AsstSpeak => c.assistant.speak = on,
        Sid::AsstReview => c.assistant.review = on,
        Sid::AsstWebAsk => c.assistant.ask_after_web = on,
        Sid::AsstFollowUp => c.assistant.follow_up = on,
        Sid::AsstNatural => c.assistant.natural_voice = on,
        Sid::AsstFishStream => c.assistant.fish_streaming = on,
        Sid::AsstFishExpressions => c.assistant.fish_expressions = on,
        Sid::AsstYandex => c.assistant.yandex_voice = on,
        Sid::AsstLocal => c.assistant.local_voice = on,
        Sid::RecSystem => c.record_system_audio = on,
        Sid::RecMic => c.record_mic = on,
        _ => {}
    }
}

pub(in crate::view) fn segment_value(id: Sid, c: &Settings) -> usize {
    match id {
        Sid::Language => usize::from(c.language == Language::En),
        Sid::Placement => usize::from(c.placement == Placement::Docked),
        Sid::Style => usize::from(c.style == Style::Liquid),
        Sid::RecFormat => usize::from(c.record_format == RecordFormat::Gif),
        _ => 0,
    }
}

pub fn cycle_set(id: Sid, c: &mut Settings, forward: bool) {
    if let Some(cap) = asst_cap(id) {
        let next = c.assistant.perms.configured(cap).step(forward);
        return c.assistant.perms.set(cap, next);
    }
    match id {
        Sid::AsstTtsProvider => {
            let all = crate::services::assistant::TtsProvider::ALL;
            let at = all.iter().position(|p| *p == c.assistant.tts_provider).unwrap_or(0);
            let next = if forward { (at + 1) % all.len() } else { (at + all.len() - 1) % all.len() };
            c.assistant.set_provider(all[next]);
        }
        Sid::NotificationTiles => {
            c.notification_tiles = match (c.notification_tiles, forward) {
                (1, true) | (4, false) => 2,
                (2, true) | (1, false) => 4,
                _ => 1,
            };
        }
        Sid::Character => c.character = c.character.step(forward),
        Sid::InterfaceFont => c.interface_font = c.interface_font.step(forward),
        Sid::PetPerformance => c.pet_performance = c.pet_performance.step(forward),
        Sid::TopEntrance => c.top_panel.entrance = c.top_panel.entrance.step(forward),
        Sid::AiProvider => c.ai.provider = c.ai.provider.step(forward),
        Sid::PillBattery => c.pill_battery = c.pill_battery.step(forward),
        Sid::PillWeatherSide => c.pill_weather_side = c.pill_weather_side.step(forward),
        Sid::CoverMode => c.cover_mode = c.cover_mode.step(forward),
        Sid::FrameRate => c.frame_rate = c.frame_rate.step(forward),
        Sid::EqStyle => c.eq_style = c.eq_style.step(forward),
        Sid::SoundPreset => {
            c.sounds.preset = if c.sounds.preset == crate::data::sounds::SoundPreset::Classic {
                crate::data::sounds::SoundPreset::Glass
            } else {
                crate::data::sounds::SoundPreset::Classic
            }
        }
        Sid::EqResponse => c.eq_response = c.eq_response.step(forward),
        Sid::EqGradient => c.eq_gradient = c.eq_gradient.step(forward),
        Sid::BumpTint => c.bump_tint = c.bump_tint.step(forward),
        Sid::ProgressStyle => c.progress_style = c.progress_style.step(forward),
        Sid::TabAnimKind => c.tab_anim = c.tab_anim.step(forward),
        Sid::TextLayout => c.text_layout = c.text_layout.step(forward),
        Sid::AnimCurve => c.anim_curve = c.anim_curve.step(forward),
        Sid::Edge => {
            c.dock_edge = c.dock_edge.step(forward);
            c.dock_offset = 0.5;
        }
        _ => {}
    }
}

pub(in crate::view) fn cycle_name(id: Sid, c: &Settings) -> &'static str {
    if let Some(cap) = asst_cap(id) {
        return c.assistant.perms.configured(cap).name();
    }
    match id {
        Sid::AsstTtsProvider => c.assistant.tts_provider.name(),
        Sid::NotificationTiles => match c.notification_tiles {
            1 => "1 tile",
            2 => "2 tiles",
            _ => "4 tiles",
        },
        Sid::Character => c.character.name(),
        Sid::InterfaceFont => c.interface_font.name(),
        Sid::PetPerformance => c.pet_performance.name(),
        Sid::TopEntrance => c.top_panel.entrance.name(),
        Sid::AiProvider => c.ai.provider.name(),
        Sid::Edge => c.dock_edge.name(),
        Sid::PillBattery => c.pill_battery.name(),
        Sid::PillWeatherSide => c.pill_weather_side.name(),
        Sid::CoverMode => c.cover_mode.name(),
        Sid::FrameRate => c.frame_rate.name(),
        Sid::EqStyle => c.eq_style.name(),
        Sid::SoundPreset => c.sounds.preset.name(),
        Sid::EqResponse => c.eq_response.name(),
        Sid::EqGradient => c.eq_gradient.name(),
        Sid::BumpTint => c.bump_tint.name(),
        Sid::ProgressStyle => c.progress_style.name(),
        Sid::TabAnimKind => c.tab_anim.name(),
        Sid::TextLayout => c.text_layout.name(),
        Sid::AnimCurve => c.anim_curve.name(),
        _ => "",
    }
}

pub fn cycle_options(id: Sid, c: &Settings) -> (Vec<&'static str>, usize) {
    use crate::data::settings::*;
    fn all<T: Copy + PartialEq>(items: &[T], now: T, name: fn(T) -> &'static str) -> (Vec<&'static str>, usize) {
        (items.iter().map(|i| name(*i)).collect(), items.iter().position(|i| *i == now).unwrap_or(0))
    }
    if let Some(cap) = asst_cap(id) {
        let now = c.assistant.perms.configured(cap);
        return all(&crate::services::assistant::Level::ALL, now, |v| v.name());
    }
    match id {
        Sid::AsstTtsProvider => {
            all(&crate::services::assistant::TtsProvider::ALL, c.assistant.tts_provider, |v| v.name())
        }
        Sid::NotificationTiles => (
            vec!["1 tile", "2 tiles", "4 tiles"],
            [1, 2, 4].iter().position(|v| *v == c.notification_tiles).unwrap_or(2),
        ),
        Sid::PetPerformance => all(&crate::ui::pet_motion::PetPerformance::ALL, c.pet_performance, |v| v.name()),
        Sid::TopEntrance => all(&crate::data::top_panel::PanelEntrance::ALL, c.top_panel.entrance, |v| v.name()),
        Sid::Character => all(&crate::ui::characters::Character::ALL, c.character, |v| v.name()),
        Sid::InterfaceFont => all(&InterfaceFont::ALL, c.interface_font, |v| v.name()),
        Sid::AiProvider => all(&AiProvider::ALL, c.ai.provider, |v| v.name()),
        Sid::PillBattery => all(&BatteryStyle::ALL, c.pill_battery, |v| v.name()),
        Sid::PillWeatherSide => all(&WeatherSide::ALL, c.pill_weather_side, |v| v.name()),
        Sid::Edge => all(&Edge::ALL, c.dock_edge, |v| v.name()),
        Sid::CoverMode => all(&CoverMode::ALL, c.cover_mode, |v| v.name()),
        Sid::FrameRate => all(&FrameRate::ALL, c.frame_rate, |v| v.name()),
        Sid::EqStyle => all(&EqStyle::ALL, c.eq_style, |v| v.name()),
        Sid::SoundPreset => all(&crate::data::sounds::SoundPreset::ALL, c.sounds.preset, |v| v.name()),
        Sid::EqResponse => all(&EqResponse::ALL, c.eq_response, |v| v.name()),
        Sid::EqGradient => all(&EqGradient::ALL, c.eq_gradient, |v| v.name()),
        Sid::BumpTint => all(&BumpTint::ALL, c.bump_tint, |v| v.name()),
        Sid::ProgressStyle => all(&ProgressStyle::ALL, c.progress_style, |v| v.name()),
        Sid::TabAnimKind => all(&TabAnim::ALL, c.tab_anim, |v| v.name()),
        Sid::AnimCurve => all(&AnimCurve::ALL, c.anim_curve, |v| v.name()),
        Sid::TextLayout => all(&TextLayout::ALL, c.text_layout, |v| v.name()),
        _ => (Vec::new(), 0),
    }
}

pub fn cycle_pick(id: Sid, c: &mut Settings, k: usize) {
    use crate::data::settings::*;
    if let Some(cap) = asst_cap(id) {
        let levels = crate::services::assistant::Level::ALL;
        return c.assistant.perms.set(cap, levels[k.min(levels.len() - 1)]);
    }
    match id {
        Sid::AsstTtsProvider => {
            let all = crate::services::assistant::TtsProvider::ALL;
            c.assistant.set_provider(all[k.min(all.len() - 1)]);
        }
        Sid::NotificationTiles => c.notification_tiles = [1, 2, 4][k.min(2)],
        Sid::InterfaceFont => c.interface_font = InterfaceFont::ALL[k.min(InterfaceFont::ALL.len() - 1)],
        Sid::Character => {
            c.character = crate::ui::characters::Character::ALL[k.min(crate::ui::characters::Character::ALL.len() - 1)]
        }
        Sid::PetPerformance => c.pet_performance = crate::ui::pet_motion::PetPerformance::ALL[k.min(3)],
        Sid::TopEntrance => c.top_panel.entrance = crate::data::top_panel::PanelEntrance::ALL[k.min(2)],
        Sid::AiProvider => c.ai.provider = AiProvider::ALL[k.min(2)],
        Sid::PillBattery => c.pill_battery = BatteryStyle::ALL[k.min(2)],
        Sid::PillWeatherSide => c.pill_weather_side = WeatherSide::ALL[k.min(1)],
        Sid::Edge => {
            c.dock_edge = Edge::ALL[k.min(3)];
            c.dock_offset = 0.5;
        }
        Sid::CoverMode => c.cover_mode = CoverMode::ALL[k.min(2)],
        Sid::FrameRate => c.frame_rate = FrameRate::ALL[k.min(3)],
        Sid::EqStyle => c.eq_style = EqStyle::ALL[k.min(4)],
        Sid::SoundPreset => c.sounds.preset = crate::data::sounds::SoundPreset::ALL[k.min(1)],
        Sid::EqResponse => c.eq_response = EqResponse::ALL[k.min(EqResponse::ALL.len() - 1)],
        Sid::EqGradient => c.eq_gradient = EqGradient::ALL[k.min(2)],
        Sid::BumpTint => c.bump_tint = BumpTint::ALL[k.min(2)],
        Sid::ProgressStyle => c.progress_style = ProgressStyle::ALL[k.min(ProgressStyle::ALL.len() - 1)],
        Sid::TabAnimKind => c.tab_anim = TabAnim::ALL[k.min(4)],
        Sid::AnimCurve => c.anim_curve = AnimCurve::ALL[k.min(3)],
        Sid::TextLayout => c.text_layout = TextLayout::ALL[k.min(2)],
        _ => {}
    }
}

pub fn segment_set(id: Sid, c: &mut Settings, option: usize) {
    match id {
        Sid::Language => c.language = if option == 1 { Language::En } else { Language::Ru },
        Sid::Placement => c.placement = if option == 1 { Placement::Docked } else { Placement::Floating },
        Sid::Style => c.style = if option == 1 { Style::Liquid } else { Style::Flat },
        Sid::RecFormat => c.record_format = if option == 1 { RecordFormat::Gif } else { RecordFormat::Mp4 },
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SetHit {
    Select,
    Label,
    Option(usize),
    Slider,
    Toggle,
    Button,
}

pub fn slider_track(pill: Rect, scale: f32, id: Sid, tab: Tab) -> Option<(f32, f32)> {
    let index = rows_for(tab).iter().position(|r| r.0 == id)?;
    let (_, ctl) = setting_row(pill, scale, index);
    Some((ctl.0 + 4.0 * scale, ctl.2 - 54.0 * scale))
}

pub fn hit_setting(pill: Rect, scale: f32, px: f32, py: f32, tab: Tab) -> Option<(Sid, SetHit)> {
    for (i, (id, _, kind)) in rows_for(tab).iter().enumerate() {
        let (row, ctl) = setting_row(pill, scale, i);
        if !inside(row, px, py) || px < ctl.0 - 6.0 * scale {
            continue;
        }
        return Some((
            *id,
            match kind {
                Kind::Segment(opts) => {
                    let t = ((px - ctl.0) / (ctl.2 - ctl.0)).clamp(0.0, 0.999);
                    SetHit::Option((t * opts.len() as f32) as usize)
                }
                Kind::Cycle => SetHit::Option(usize::from(px > (ctl.0 + ctl.2) / 2.0)),
                Kind::Slider => SetHit::Slider,
                Kind::Toggle => SetHit::Toggle,
                Kind::Button => SetHit::Button,
            },
        ));
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeHit {
    Preset(usize),
    Panel(usize),
    Accent(usize),
}

pub(in crate::view) fn track_color(id: Sid, t: f32, c: &Settings) -> Option<(f32, f32, f32)> {
    use crate::ui::theme::hsv;
    Some(match id {
        Sid::PanelHue => hsv(t * 360.0, c.panel_sat.max(0.7), 0.55),
        Sid::PanelSat => hsv(c.panel_hue, t, 0.4),
        Sid::PanelVal => hsv(c.panel_hue, c.panel_sat.max(0.5), t * 0.6),
        Sid::AccentHue => hsv(t * 360.0, c.accent_sat.max(0.5), 1.0),
        Sid::AccentSat => hsv(c.accent_hue, t, c.accent_val),
        _ => return None,
    })
}

pub(in crate::view) fn paint_rows(
    p: &Painter,
    pill: Rect,
    scale: f32,
    ui: &UiState,
    rows: &[(Sid, &str, Kind)],
) -> Result<()> {
    for (i, (id, label, kind)) in rows.iter().enumerate() {
        let (row, ctl) = setting_row(pill, scale, i);
        p.text(
            label,
            (row.0 + 2.0 * scale, row.1, row.0 + SET_LABEL_W * scale, row.3),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.85),
        )?;
        paint_control(p, scale, &ui.cfg, *id, *kind, ctl, &ui.feedback)?;
    }
    Ok(())
}

pub(in crate::view) fn paint_control(
    p: &Painter,
    scale: f32,
    cfg: &Settings,
    id: Sid,
    kind: Kind,
    ctl: Rect,
    feedback: &crate::ui::feedback::Feedback,
) -> Result<()> {
    let accent = acc(1.0);
    let cy = (ctl.1 + ctl.3) / 2.0;
    match kind {
        Kind::Segment(opts) => {
            let selected = segment_value(id, cfg);
            p.fill_round_rect(ctl, 9.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
            let half = (ctl.2 - ctl.0) / 2.0;
            let position = feedback.sample(600_000 + id as u64, selected as f32, selected as f32, 180);
            p.fill_round_rect(
                (
                    ctl.0 + position * half + 2.0 * scale,
                    ctl.1 + 2.0 * scale,
                    ctl.0 + (position + 1.0) * half - 2.0 * scale,
                    ctl.3 - 2.0 * scale,
                ),
                7.0 * scale,
                acc(0.35),
            )?;
            for (n, text) in opts.iter().enumerate() {
                let r = (ctl.0 + n as f32 * half, ctl.1, ctl.0 + (n + 1) as f32 * half, ctl.3);
                let alpha = if n == selected { 1.0 } else { 0.6 };
                p.text(
                    text,
                    r,
                    12.0 * scale,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    rgba(1.0, 1.0, 1.0, alpha),
                )?;
            }
        }
        Kind::Cycle => {
            p.fill_round_rect(ctl, 9.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
            p.stroke_round_rect(ctl, 9.0 * scale, scale, rgba(1.0, 1.0, 1.0, 0.1))?;
            p.text(
                cycle_name(id, cfg),
                (ctl.0 + 12.0 * scale, ctl.1, ctl.2 - 30.0 * scale, ctl.3),
                12.5 * scale,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            p.text(
                "\u{25BE}",
                (ctl.2 - 28.0 * scale, ctl.1, ctl.2 - 6.0 * scale, ctl.3),
                13.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, 0.7),
            )?;
        }
        Kind::Slider => {
            let (tl, tr) = (ctl.0 + 4.0 * scale, ctl.2 - 54.0 * scale);
            let (min, max, _) = slider_range(id);
            let v = slider_value(id, cfg);
            let norm = ((v - min) / (max - min)).clamp(0.0, 1.0);
            let x = tl + (tr - tl) * norm;
            if track_color(id, 0.0, cfg).is_some() {
                const SEGMENTS: usize = 32;
                let seg = (tr - tl) / SEGMENTS as f32;
                for k in 0..SEGMENTS {
                    let (r, g, b) = track_color(id, (k as f32 + 0.5) / SEGMENTS as f32, cfg).unwrap_or((0.5, 0.5, 0.5));
                    let (left, right) = (tl + k as f32 * seg, tl + (k + 1) as f32 * seg + 0.6);
                    if k == 0 || k == SEGMENTS - 1 {
                        p.fill_round_rect(
                            (left, cy - 4.0 * scale, right, cy + 4.0 * scale),
                            4.0 * scale,
                            rgba(r, g, b, 1.0),
                        )?;
                    } else {
                        p.fill_rect((left, cy - 4.0 * scale, right, cy + 4.0 * scale), rgba(r, g, b, 1.0))?;
                    }
                }
            } else {
                p.fill_round_rect(
                    (tl, cy - 2.0 * scale, tr, cy + 2.0 * scale),
                    2.0 * scale,
                    rgba(1.0, 1.0, 1.0, 0.18),
                )?;
                p.fill_round_rect((tl, cy - 2.0 * scale, x, cy + 2.0 * scale), 2.0 * scale, accent)?;
            }
            p.fill_round_rect(
                (x - 7.0 * scale, cy - 7.0 * scale, x + 7.0 * scale, cy + 7.0 * scale),
                7.0 * scale,
                WHITE,
            )?;
            p.text(
                &slider_text(id, v),
                (tr + 10.0 * scale, ctl.1, ctl.2, ctl.3),
                12.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                rgba(1.0, 1.0, 1.0, 0.7),
            )?;
        }
        Kind::Button => {
            p.fill_round_rect(ctl, 9.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
            p.text(
                match id {
                    Sid::DockImport | Sid::DockImportTaskbar => "Import",
                    Sid::DockAdd => "Add",
                    Sid::DockMonitor => "Switch",
                    _ => "Open",
                },
                (ctl.0, ctl.1, ctl.2 - 26.0 * scale, ctl.3),
                12.0 * scale,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                WHITE,
            )?;
            p.glyph('\u{E76C}', (ctl.2 - 26.0 * scale, ctl.1, ctl.2, ctl.3), 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.75))?;
        }
        Kind::Toggle => {
            let on = f32::from(toggle_value(id, cfg));
            let level = feedback.sample(500_000 + id as u64, on, on, 180);
            let (w, h) = (40.0 * scale, 22.0 * scale);
            let track = (ctl.2 - w, cy - h / 2.0, ctl.2, cy + h / 2.0);
            p.fill_round_rect(
                track,
                h / 2.0,
                rgba(
                    1.0 + (accent.r - 1.0) * level,
                    1.0 + (accent.g - 1.0) * level,
                    1.0 + (accent.b - 1.0) * level,
                    0.16 + 0.74 * level,
                ),
            )?;
            let knob = 16.0 * scale;
            let kx = track.0 + 3.0 * scale + (w - knob - 6.0 * scale) * level;
            p.fill_round_rect((kx, cy - knob / 2.0, kx + knob, cy + knob / 2.0), knob / 2.0, WHITE)?;
        }
    }
    Ok(())
}
