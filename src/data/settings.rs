use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::keys::{Combo, Hotkey};
use crate::ui::characters::Character;

const MAX_LYRICS_APPS: usize = 20;

const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    #[default]
    Floating,
    Docked,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Ru,
    En,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hang {
    Down,
    Up,
    Right,
    Left,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub fn name(self) -> &'static str {
        match self {
            Edge::Top => "Top edge",
            Edge::Bottom => "Bottom edge",
            Edge::Left => "Left edge",
            Edge::Right => "Right edge",
        }
    }

    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|e| *e == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 4 } else { (i + 3) % 4 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecordFormat {
    #[default]
    Mp4,
    Gif,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    Flat,
    #[default]
    Liquid,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum BatteryStyle {
    Hidden,
    Icon,
    #[default]
    Percent,
}

impl BatteryStyle {
    pub fn name(self) -> &'static str {
        match self {
            BatteryStyle::Hidden => "Hidden",
            BatteryStyle::Icon => "Icon only",
            BatteryStyle::Percent => "With percent",
        }
    }

    pub const ALL: [BatteryStyle; 3] = [BatteryStyle::Hidden, BatteryStyle::Icon, BatteryStyle::Percent];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 3 } else { (i + 2) % 3 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TabAnim {
    None,
    Fade,
    #[default]
    Slide,
    Zoom,
    Rise,
}

impl TabAnim {
    pub fn name(self) -> &'static str {
        match self {
            TabAnim::None => "None",
            TabAnim::Fade => "Fade",
            TabAnim::Slide => "Slide",
            TabAnim::Zoom => "Zoom",
            TabAnim::Rise => "Rise",
        }
    }

    pub const ALL: [TabAnim; 5] = [TabAnim::None, TabAnim::Fade, TabAnim::Slide, TabAnim::Zoom, TabAnim::Rise];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 5 } else { (i + 4) % 5 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum AnimCurve {
    #[default]
    Spring,
    Smooth,
    Out,
    Linear,
}

impl AnimCurve {
    pub fn name(self) -> &'static str {
        match self {
            AnimCurve::Spring => "Spring",
            AnimCurve::Smooth => "Smooth",
            AnimCurve::Out => "Ease out",
            AnimCurve::Linear => "Linear",
        }
    }

    pub const ALL: [AnimCurve; 4] = [AnimCurve::Spring, AnimCurve::Smooth, AnimCurve::Out, AnimCurve::Linear];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 4 } else { (i + 3) % 4 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextLayout {
    #[default]
    Categories,
    List,
    Palette,
}

impl TextLayout {
    pub fn name(self) -> &'static str {
        match self {
            TextLayout::Categories => "Groups",
            TextLayout::List => "List",
            TextLayout::Palette => "Chips and catalogue",
        }
    }

    pub const ALL: [TextLayout; 3] = [TextLayout::Categories, TextLayout::List, TextLayout::Palette];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 3 } else { (i + 2) % 3 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum CoverMode {
    #[default]
    Crop,
    Stretch,
    Fit,
}

impl CoverMode {
    pub fn name(self) -> &'static str {
        match self {
            CoverMode::Crop => "Crop to square",
            CoverMode::Stretch => "Stretch",
            CoverMode::Fit => "Keep proportions",
        }
    }

    pub const ALL: [CoverMode; 3] = [CoverMode::Crop, CoverMode::Stretch, CoverMode::Fit];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 3 } else { (i + 2) % 3 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum FrameRate {
    #[default]
    Fps30,
    Fps60,
    Monitor,
    Adaptive,
}

impl FrameRate {
    pub fn name(self) -> &'static str {
        match self {
            FrameRate::Fps30 => "30 FPS",
            FrameRate::Fps60 => "60 FPS",
            FrameRate::Monitor => "Monitor refresh rate",
            FrameRate::Adaptive => "Adaptive",
        }
    }

    pub const ALL: [FrameRate; 4] = [FrameRate::Fps30, FrameRate::Fps60, FrameRate::Monitor, FrameRate::Adaptive];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 4 } else { (i + 3) % 4 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum EqStyle {
    #[default]
    Spikes,
    Wave,
    Bars,
    Halo,
    Aura,
}

impl EqStyle {
    pub fn name(self) -> &'static str {
        match self {
            EqStyle::Spikes => "Spikes",
            EqStyle::Wave => "Wave",
            EqStyle::Bars => "Bars",
            EqStyle::Halo => "Halo around the pill",
            EqStyle::Aura => "Aura around the pill",
        }
    }

    pub const ALL: [EqStyle; 5] = [EqStyle::Spikes, EqStyle::Wave, EqStyle::Bars, EqStyle::Halo, EqStyle::Aura];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 5 } else { (i + 4) % 5 }]
    }

    pub fn around(self) -> bool {
        matches!(self, EqStyle::Halo | EqStyle::Aura)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum EqResponse {
    Calm,
    #[default]
    Responsive,
    Energetic,
}

impl EqResponse {
    pub const ALL: [Self; 3] = [Self::Calm, Self::Responsive, Self::Energetic];

    pub fn name(self) -> &'static str {
        match self {
            Self::Calm => "Calm",
            Self::Responsive => "Responsive",
            Self::Energetic => "Energetic",
        }
    }

    pub fn step(self, forward: bool) -> Self {
        let count = Self::ALL.len();
        let index = Self::ALL.iter().position(|value| *value == self).unwrap_or(0);
        Self::ALL[if forward { (index + 1) % count } else { (index + count - 1) % count }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum EqGradient {
    #[default]
    Single,
    Vertical,
    Horizontal,
}

impl EqGradient {
    pub fn name(self) -> &'static str {
        match self {
            EqGradient::Single => "One colour",
            EqGradient::Vertical => "Gradient outwards",
            EqGradient::Horizontal => "Gradient left to right",
        }
    }

    pub const ALL: [EqGradient; 3] = [EqGradient::Single, EqGradient::Vertical, EqGradient::Horizontal];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 3 } else { (i + 2) % 3 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum BumpTint {
    Off,
    #[default]
    Equalizer,
    Progress,
}

impl BumpTint {
    pub fn name(self) -> &'static str {
        match self {
            BumpTint::Off => "Off",
            BumpTint::Equalizer => "Equalizer colours",
            BumpTint::Progress => "Progress colours",
        }
    }

    pub const ALL: [BumpTint; 3] = [BumpTint::Off, BumpTint::Equalizer, BumpTint::Progress];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % 3 } else { (i + 2) % 3 }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProgressStyle {
    Strip,
    OutlineFromLeft,
    OutlineBottom,
    #[default]
    Fill,
}

impl ProgressStyle {
    pub fn name(self) -> &'static str {
        match self {
            ProgressStyle::Strip => "Strip along the bottom",
            ProgressStyle::OutlineFromLeft => "Outline from the left side",
            ProgressStyle::OutlineBottom => "Outline, the bottom fills",
            ProgressStyle::Fill => "Fill the island",
        }
    }

    pub const ALL: [ProgressStyle; 4] =
        [ProgressStyle::Strip, ProgressStyle::OutlineFromLeft, ProgressStyle::OutlineBottom, ProgressStyle::Fill];

    pub fn step(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        let count = Self::ALL.len();
        Self::ALL[if forward { (i + 1) % count } else { (i + count - 1) % count }]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceFont {
    #[default]
    Segoe,
    Montserrat,
    SanFrancisco,
    Custom,
}

impl InterfaceFont {
    pub const ALL: [Self; 4] = [Self::Segoe, Self::Montserrat, Self::SanFrancisco, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "Custom font",
            Self::Segoe => "Segoe UI",
            Self::Montserrat => "Montserrat",
            Self::SanFrancisco if crate::ui::fonts::sf_available() => "SF Pro",
            Self::SanFrancisco => "SF Pro (not installed)",
        }
    }

    pub fn step(self, forward: bool) -> Self {
        let index = Self::ALL.iter().position(|font| *font == self).unwrap_or(0);
        Self::ALL[(index + if forward { 1 } else { Self::ALL.len() - 1 }) % Self::ALL.len()]
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub interface_font: InterfaceFont,
    pub interface_font_file: String,
    pub tour_revision: u8,
    pub top_panel: crate::data::top_panel::TopPanelSettings,
    pub desktop_dock: crate::data::desktop_dock::DockSettings,
    pub language: Language,
    pub legacy_icons: bool,
    pub header_icons: bool,
    pub header_icon_colors: bool,
    pub header_active_accent: bool,
    pub weather: bool,
    pub weather_city: String,
    pub weather_lat: Option<f64>,
    pub weather_lon: Option<f64>,
    pub pill_opacity: f32,
    pub pill_blur: bool,
    pub agy_auto: bool,
    pub discord_overlay: bool,
    pub overlay_pos: Option<[i32; 2]>,
    pub panel_hue: f32,
    pub panel_sat: f32,
    pub panel_val: f32,
    pub accent_hue: f32,
    pub accent_sat: f32,
    pub accent_val: f32,
    pub placement: Placement,
    pub dock_edge: Edge,
    pub dock_offset: f32,
    pub float_pos: Option<[i32; 2]>,
    pub system_notifications: bool,
    pub wave_ratings: bool,
    pub wave_search: bool,
    pub notification_tiles: u8,
    pub notification_open_app: bool,
    pub ai_notification_cards: bool,
    pub ai_compact_replies: bool,
    pub notification_adaptive_size: bool,
    pub pet_notifications: bool,
    pub music_track_intro: bool,
    pub pet_music_curiosity: bool,
    pub pet_return_animation: bool,
    pub liquid_idle_travel: bool,
    pub pill_weather: bool,
    pub pill_weather_side: WeatherSide,
    pub pill_battery: BatteryStyle,
    pub music_eq: bool,
    pub music_title: bool,
    pub music_cover: bool,
    pub prefer_playing_media: bool,
    pub metrics_tab: Vec<String>,
    pub metrics_pill: Vec<String>,
    pub metrics_secs: u32,
    pub memory_tools: bool,
    pub memory_watch: bool,
    pub memory_excluded: Vec<String>,
    pub activity_track: bool,
    pub activity_ai: bool,
    pub vpn_pill: bool,
    pub voice_paste: bool,
    pub music_lyrics: bool,
    pub music_progress: bool,
    pub pill_unified_layout: bool,
    pub eq_smooth: bool,
    pub cover_mode: CoverMode,
    pub frame_rate: FrameRate,
    pub eq_style: EqStyle,
    pub eq_response: EqResponse,
    pub eq_strength: f32,
    pub eq_gradient: EqGradient,
    pub eq_when_open: bool,
    pub bump_tint: BumpTint,
    pub progress_style: ProgressStyle,
    pub text_layout: TextLayout,
    pub dev_tools: bool,
    pub shared_lyrics: bool,
    pub update_check: bool,
    pub update_beta: bool,
    pub ai: AiSettings,
    pub assistant: crate::services::assistant::Assistant,
    pub hidden_tabs: Vec<String>,
    pub notify: NotifySettings,
    pub spike_color: bool,
    pub record_format: RecordFormat,
    pub record_fps: u32,
    pub record_system_audio: bool,
    pub record_mic: bool,
    pub character: Character,
    pub pet_performance: crate::ui::pet_motion::PetPerformance,
    pub pet_voice_reactions: bool,
    pub pet_voice_input: bool,
    pub pet_autonomy: bool,
    pub custom_pet: crate::ui::pet_design::PetDesign,
    #[serde(alias = "spark_detached")]
    pub character_detached: bool,
    #[serde(alias = "spark_pos")]
    pub character_pos: Option<[i32; 2]>,
    pub edge_spikes: bool,
    pub style: Style,
    pub capture_hidden: bool,
    pub capture_self_hidden: bool,
    pub game_mode: bool,
    pub auto_hide: bool,
    pub lock_drag: bool,
    pub acrylic: bool,
    pub scale_percent: u32,
    pub tint_open: f32,
    pub liquid_reach: f32,
    pub liquid_strength: f32,
    pub liquid_softness: f32,
    pub open_delay_ms: u32,
    pub close_delay_ms: u32,
    pub open_speed: f32,
    pub island_animations: bool,
    pub close_speed: f32,
    pub tab_anim: TabAnim,
    pub tab_order: Vec<String>,
    pub header_tabs: Vec<String>,
    pub tab_drag: bool,
    pub last_tab: String,
    pub remember_tab: bool,
    pub ai_sub: String,
    pub tab_anim_speed: f32,
    pub anim_curve: AnimCurve,
    pub keys: BTreeMap<String, Combo>,
    pub hide_pill_secs: u32,
    pub ghost_mode: bool,
    pub ghost_opacity: f32,
    pub ghost_click_through: bool,
    pub ghost_hold: bool,
    pub home_actions: Vec<u8>,
    pub lyrics_apps: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tour_revision: 0,
            top_panel: Default::default(),
            desktop_dock: Default::default(),
            language: Language::Ru,
            legacy_icons: false,
            header_icons: true,
            header_icon_colors: false,
            header_active_accent: true,
            interface_font: InterfaceFont::Segoe,
            interface_font_file: String::new(),
            weather: true,
            weather_city: String::new(),
            weather_lat: None,
            weather_lon: None,
            pill_opacity: 1.0,
            pill_blur: false,
            agy_auto: false,
            discord_overlay: false,
            overlay_pos: None,
            panel_hue: 240.0,
            panel_sat: 0.08,
            panel_val: 0.10,
            accent_hue: 220.0,
            accent_sat: 0.10,
            accent_val: 0.85,
            placement: Placement::Docked,
            dock_edge: Edge::Top,
            dock_offset: 0.5,
            float_pos: None,
            system_notifications: false,
            wave_ratings: true,
            wave_search: true,
            notification_tiles: 4,
            notification_open_app: false,
            ai_notification_cards: true,
            ai_compact_replies: true,
            notification_adaptive_size: true,
            pet_notifications: true,
            music_track_intro: true,
            pet_music_curiosity: true,
            pet_return_animation: true,
            liquid_idle_travel: true,
            pill_weather: true,
            pill_weather_side: WeatherSide::Left,
            pill_battery: BatteryStyle::Percent,
            music_eq: true,
            music_title: true,
            music_cover: true,
            prefer_playing_media: false,
            voice_paste: true,
            pet_voice_reactions: true,
            pet_voice_input: true,
            activity_track: true,
            activity_ai: false,
            vpn_pill: true,
            metrics_tab: ["cpu", "ram", "net_down", "net_up", "gpu", "disk", "uptime"].map(String::from).to_vec(),
            metrics_pill: Vec::new(),
            metrics_secs: 2,
            memory_tools: true,
            memory_watch: true,
            memory_excluded: Vec::new(),
            music_lyrics: true,
            music_progress: true,
            pill_unified_layout: true,
            eq_smooth: true,
            cover_mode: CoverMode::Fit,
            frame_rate: FrameRate::Adaptive,
            eq_style: EqStyle::Bars,
            eq_response: EqResponse::Responsive,
            eq_strength: 1.5,
            eq_gradient: EqGradient::Horizontal,
            eq_when_open: true,
            bump_tint: BumpTint::Equalizer,
            progress_style: ProgressStyle::Fill,
            text_layout: TextLayout::Categories,
            dev_tools: false,
            shared_lyrics: true,
            update_check: true,
            update_beta: false,
            ai: AiSettings::default(),
            assistant: crate::services::assistant::Assistant::default(),
            hidden_tabs: Vec::new(),
            notify: NotifySettings::default(),
            spike_color: true,
            record_format: RecordFormat::Mp4,
            record_fps: 60,
            record_system_audio: true,
            record_mic: true,
            character: Character::Spark,
            pet_performance: Default::default(),
            pet_autonomy: true,
            custom_pet: Default::default(),
            character_detached: false,
            character_pos: None,
            edge_spikes: true,
            style: Style::Liquid,
            capture_hidden: false,
            capture_self_hidden: true,
            game_mode: true,
            auto_hide: false,
            lock_drag: true,
            acrylic: true,
            scale_percent: 100,
            tint_open: 0.5,
            liquid_reach: 160.0,
            liquid_strength: 24.0,
            liquid_softness: 0.55,
            open_delay_ms: 50,
            close_delay_ms: 50,
            open_speed: 1.0,
            island_animations: true,
            tab_anim: TabAnim::Zoom,
            tab_order: Vec::new(),
            header_tabs: DEFAULT_HEADER_TABS.map(String::from).to_vec(),
            tab_drag: false,
            last_tab: String::new(),
            remember_tab: true,
            ai_sub: "Chat".into(),
            tab_anim_speed: 1.0,
            anim_curve: AnimCurve::Spring,
            close_speed: 1.0,
            keys: BTreeMap::new(),
            hide_pill_secs: 5,
            ghost_mode: true,
            ghost_opacity: 0.3,
            ghost_click_through: true,
            ghost_hold: false,
            home_actions: Vec::new(),
            lyrics_apps: Vec::new(),
        }
    }
}

pub const METRIC_KEYS: [&str; 7] = ["cpu", "ram", "gpu", "disk", "net_down", "net_up", "uptime"];

pub const HOME_ACTION_COUNT: usize = 23;

fn clamp_f(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

fn clean_app_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(40).collect::<String>().trim().to_uppercase()
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    #[default]
    Claude,
    OpenAi,
    Gemini,
}

impl AiProvider {
    pub fn id(self) -> &'static str {
        match self {
            AiProvider::Claude => "claude",
            AiProvider::OpenAi => "openai",
            AiProvider::Gemini => "gemini",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AiProvider::Claude => "Claude",
            AiProvider::OpenAi => "OpenAI-compatible",
            AiProvider::Gemini => "Gemini",
        }
    }

    pub const ALL: [AiProvider; 3] = [AiProvider::Claude, AiProvider::OpenAi, AiProvider::Gemini];

    pub fn step(self, forward: bool) -> Self {
        let at = Self::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Self::ALL[(at + if forward { 1 } else { Self::ALL.len() - 1 }) % Self::ALL.len()]
    }

    pub fn default_model(self) -> &'static str {
        match self {
            AiProvider::Claude => "claude-haiku-5-5",
            AiProvider::OpenAi => "gpt-4o-mini",
            AiProvider::Gemini => "gemini-2.0-flash",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct AiSettings {
    pub provider: AiProvider,
    pub claude_model: String,
    pub openai_model: String,
    pub openai_base: String,
    pub gemini_model: String,
    pub voice_separate: bool,
    pub voice_base: String,
    pub voice_model: String,
    pub voice_wake: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Claude,
            claude_model: AiProvider::Claude.default_model().into(),
            openai_model: AiProvider::OpenAi.default_model().into(),
            openai_base: "https://api.openai.com/v1".into(),
            gemini_model: AiProvider::Gemini.default_model().into(),
            voice_separate: false,
            voice_base: "https://api.openai.com/v1".into(),
            voice_model: "whisper-1".into(),
            voice_wake: "Island".into(),
        }
    }
}

impl AiSettings {
    pub fn model(&self, provider: AiProvider) -> &str {
        match provider {
            AiProvider::Claude => &self.claude_model,
            AiProvider::OpenAi => &self.openai_model,
            AiProvider::Gemini => &self.gemini_model,
        }
    }

    pub fn set_model(&mut self, provider: AiProvider, model: &str) {
        let model = model.trim().to_string();
        match provider {
            AiProvider::Claude => self.claude_model = model,
            AiProvider::OpenAi => self.openai_model = model,
            AiProvider::Gemini => self.gemini_model = model,
        }
    }

    pub fn sanitize(&mut self) {
        use crate::services::ai::{clean_base, valid_model};
        let d = AiSettings::default();
        for provider in [AiProvider::Claude, AiProvider::OpenAi, AiProvider::Gemini] {
            if !valid_model(self.model(provider), provider) {
                self.set_model(provider, d.model(provider));
            }
        }
        self.openai_base = clean_base(&self.openai_base).unwrap_or(d.openai_base);
        self.voice_base = clean_base(&self.voice_base).unwrap_or(d.voice_base);
        if !valid_model(&self.voice_model, AiProvider::OpenAi) {
            self.voice_model = d.voice_model;
        }
        self.voice_wake = crate::tools::voicecmd::clean_wake_setting(&self.voice_wake).unwrap_or(d.voice_wake);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Notice {
    Mic,
    Download,
    Capture,
    Recording,
    Agy,
    Integrations,
    Error,
    Clipboard,
    LockKeys,
    Battery,
    Ai,
    Steam,
    Agents,
    Privacy,
    Update,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct NotifySettings {
    pub mic: bool,
    pub downloads: bool,
    pub capture: bool,
    pub recording: bool,
    pub agy: bool,
    pub integrations: bool,
    pub errors: bool,
    pub clipboard: bool,
    pub clipboard_text: bool,
    pub lock_keys: bool,
    pub battery: bool,
    pub ai: bool,
    pub update: bool,
    pub privacy: bool,
    pub agents: bool,
    pub steam: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            mic: true,
            downloads: true,
            capture: true,
            recording: true,
            agy: true,
            integrations: true,
            errors: true,
            clipboard: false,
            clipboard_text: false,
            lock_keys: false,
            battery: false,
            ai: true,
            update: true,
            privacy: true,
            agents: true,
            steam: true,
        }
    }
}

impl NotifySettings {
    pub fn on(&self, notice: Notice) -> bool {
        match notice {
            Notice::Mic => self.mic,
            Notice::Download => self.downloads,
            Notice::Capture => self.capture,
            Notice::Recording => self.recording,
            Notice::Agy => self.agy,
            Notice::Integrations => self.integrations,
            Notice::Error => self.errors,
            Notice::Clipboard => self.clipboard,
            Notice::LockKeys => self.lock_keys,
            Notice::Battery => self.battery,
            Notice::Ai => self.ai,
            Notice::Update => self.update,
            Notice::Privacy => self.privacy,
            Notice::Agents => self.agents,
            Notice::Steam => self.steam,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum WeatherSide {
    #[default]
    Left,
    Right,
}

impl WeatherSide {
    pub const ALL: [Self; 2] = [Self::Left, Self::Right];
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "Left of the clock",
            Self::Right => "Right of the clock",
        }
    }
    pub fn step(self, _forward: bool) -> Self {
        if self == Self::Left { Self::Right } else { Self::Left }
    }
}

pub const HEADER_TAB_LIMIT: usize = HIDEABLE_TABS.len();
pub const DEFAULT_HEADER_TABS: [&str; 5] = ["Clipboard", "Text", "Note", "Weather", "Record"];

pub const HIDEABLE_TABS: [&str; 13] = [
    "Clipboard",
    "Text",
    "Note",
    "Weather",
    "Record",
    "AI",
    "Privacy",
    "Activity",
    "Steam",
    "Controls",
    "App mixer",
    "Notifications",
    "My Wave",
];

impl Settings {
    pub fn hang(&self) -> Hang {
        if self.placement != Placement::Docked {
            return Hang::Down;
        }
        match self.dock_edge {
            Edge::Top => Hang::Down,
            Edge::Bottom => Hang::Up,
            Edge::Left => Hang::Right,
            Edge::Right => Hang::Left,
        }
    }

    pub fn vertical(&self) -> bool {
        self.placement == Placement::Docked && matches!(self.dock_edge, Edge::Left | Edge::Right)
    }

    pub fn ring_wanted(&self) -> bool {
        self.edge_spikes && self.eq_style.around()
    }

    pub fn set_header_tab(&mut self, name: &str, pinned: bool, replace: Option<&str>) -> bool {
        if !HIDEABLE_TABS.contains(&name) {
            return false;
        }
        if !pinned {
            self.header_tabs.retain(|item| item != name);
        } else if !self.header_tabs.iter().any(|item| item == name) {
            if let Some(old) = replace {
                let Some(index) = self.header_tabs.iter().position(|item| item == old) else { return false };
                self.header_tabs[index] = name.to_string();
            } else if self.header_tabs.len() < HEADER_TAB_LIMIT {
                self.header_tabs.push(name.to_string());
            } else {
                return false;
            }
        }
        self.set_tab_hidden(name, false);
        true
    }

    pub fn tab_hidden(&self, name: &str) -> bool {
        self.hidden_tabs.iter().any(|t| t == name)
    }

    pub fn set_tab_hidden(&mut self, name: &str, hidden: bool) {
        let Some(known) = HIDEABLE_TABS.iter().find(|t| **t == name) else { return };
        self.hidden_tabs.retain(|t| t != known);
        if hidden {
            self.hidden_tabs.push((*known).to_string());
        }
    }

    pub fn combo(&self, hotkey: Hotkey) -> Combo {
        self.keys.get(hotkey.key()).copied().unwrap_or_else(|| hotkey.default_combo())
    }

    pub fn set_combo(&mut self, hotkey: Hotkey, combo: Combo) {
        if combo == hotkey.default_combo() {
            self.keys.remove(hotkey.key());
        } else {
            self.keys.insert(hotkey.key().to_string(), combo);
        }
    }

    pub fn hotkey_using(&self, hotkey: Hotkey, combo: Combo) -> Option<Hotkey> {
        if combo.is_none() {
            return None;
        }
        Hotkey::ALL.into_iter().find(|h| *h != hotkey && self.combo(*h) == combo)
    }

    pub fn lyrics_app_allowed(&self, source: &str) -> bool {
        self.lyrics_apps.is_empty() || self.lyrics_apps.iter().any(|a| a.eq_ignore_ascii_case(source))
    }

    pub fn add_lyrics_app(&mut self, source: &str) -> bool {
        let name = clean_app_name(source);
        if name.is_empty() || self.lyrics_apps.len() >= MAX_LYRICS_APPS || self.lyrics_apps.contains(&name) {
            return false;
        }
        self.lyrics_apps.push(name);
        true
    }

    fn sanitize_keys(&mut self) {
        let known: Vec<&str> = Hotkey::ALL.iter().map(|h| h.key()).collect();
        self.keys.retain(|name, combo| known.contains(&name.as_str()) && (combo.valid() || combo.is_none()));
        let mut seen: Vec<Combo> = Vec::new();
        for h in Hotkey::ALL {
            let combo = self.combo(h);
            if combo.is_none() {
                continue;
            }
            if seen.contains(&combo) {
                self.keys.remove(h.key());
                if seen.contains(&self.combo(h)) {
                    self.keys.clear();
                    return;
                }
            }
            seen.push(self.combo(h));
        }
    }

    fn path() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("NightIsland");
        Some(dir.join("settings.json"))
    }

    const TOUR_REVISION: u8 = 4;

    pub fn tour_pending(&self) -> bool {
        self.tour_revision < Self::TOUR_REVISION
    }

    pub fn dismiss_tour(&mut self) {
        self.tour_revision = self.tour_revision.max(Self::TOUR_REVISION);
    }

    pub fn load() -> Self {
        Self::load_from(Self::path())
    }

    fn load_from(path: Option<PathBuf>) -> Self {
        let file = path.map(fs::File::open);
        let fresh = file.as_ref().is_some_and(|f| f.as_ref().is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound));
        let file = file.and_then(Result::ok);
        let existed = file.is_some();
        let parsed = file.and_then(|f| {
            let mut buf = String::new();
            f.take(MAX_FILE_BYTES).read_to_string(&mut buf).ok()?;
            serde_json::from_str::<Settings>(&buf).ok()
        });
        if existed && parsed.is_none() {
            crate::log_warn!("settings.json is unreadable, using defaults");
        }
        let mut s = parsed.unwrap_or_else(|| {
            let mut defaults = Self::default();
            if !fresh {
                defaults.dismiss_tour();
            }
            defaults
        });
        s.sanitize();
        s
    }

    pub fn sanitize(&mut self) {
        self.desktop_dock.sanitize();
        self.top_panel.sanitize();
        let d = Settings::default();
        self.scale_percent = self.scale_percent.clamp(80, 200);
        if ![1, 2, 4].contains(&self.notification_tiles) {
            self.notification_tiles = 4;
        }
        let (chat, agy) = (self.tab_hidden("Chat"), self.tab_hidden("AGY"));
        self.hidden_tabs.retain(|t| t != "Chat" && t != "AGY");
        if chat && agy && !self.tab_hidden("AI") {
            self.hidden_tabs.push("AI".into());
        }
        let mut seen: Vec<String> = Vec::new();
        for t in &self.hidden_tabs {
            if HIDEABLE_TABS.contains(&t.as_str()) && !seen.contains(t) {
                seen.push(t.clone());
            }
        }
        self.hidden_tabs = seen;
        self.ai.sanitize();
        self.assistant.sanitize();
        self.record_fps = self.record_fps.clamp(10, 60);
        self.pill_opacity = clamp_f(self.pill_opacity, 0.2, 1.0, 1.0);
        let valid = |v: Option<f64>, lim: f64| v.filter(|x| x.is_finite() && x.abs() <= lim);
        self.weather_lat = valid(self.weather_lat, 90.0);
        self.weather_lon = valid(self.weather_lon, 180.0);
        if self.weather_lat.is_none() || self.weather_lon.is_none() {
            self.weather_lat = None;
            self.weather_lon = None;
        }
        self.weather_city =
            self.weather_city.chars().filter(|c| !c.is_control()).take(60).collect::<String>().trim().to_string();
        self.dock_offset = clamp_f(self.dock_offset, 0.0, 1.0, 0.5);
        self.float_pos = self.float_pos.map(|[x, y]| [x.clamp(-20_000, 20_000), y.clamp(-20_000, 20_000)]);
        self.custom_pet.sanitize();
        self.character_pos = self.character_pos.map(|[x, y]| [x.clamp(-20_000, 20_000), y.clamp(-20_000, 20_000)]);
        if self.character == Character::None {
            self.character_detached = false;
        }
        self.overlay_pos = self.overlay_pos.map(|[x, y]| [x.clamp(-20_000, 20_000), y.clamp(-20_000, 20_000)]);
        self.panel_hue = clamp_f(self.panel_hue, 0.0, 360.0, d.panel_hue);
        self.panel_sat = clamp_f(self.panel_sat, 0.0, 1.0, d.panel_sat);
        self.panel_val = clamp_f(self.panel_val, 0.0, 0.3, d.panel_val);
        self.accent_hue = clamp_f(self.accent_hue, 0.0, 360.0, d.accent_hue);
        self.accent_sat = clamp_f(self.accent_sat, 0.0, 1.0, d.accent_sat);
        self.accent_val = clamp_f(self.accent_val, 0.6, 1.0, d.accent_val);
        self.tint_open = clamp_f(self.tint_open, 0.2, 1.0, d.tint_open);
        self.liquid_reach = clamp_f(self.liquid_reach, 60.0, 300.0, d.liquid_reach);
        self.liquid_strength = clamp_f(self.liquid_strength, 6.0, 40.0, d.liquid_strength);
        self.eq_strength = clamp_f(self.eq_strength, 0.4, 2.0, d.eq_strength);
        self.liquid_softness = clamp_f(self.liquid_softness, 0.0, 1.0, d.liquid_softness);
        self.open_delay_ms = self.open_delay_ms.clamp(0, 1000);
        self.close_delay_ms = self.close_delay_ms.clamp(0, 3000);
        self.open_speed = clamp_f(self.open_speed, 0.4, 2.5, d.open_speed);
        self.close_speed = clamp_f(self.close_speed, 0.4, 2.5, d.close_speed);
        {
            let mut seen: Vec<String> = Vec::new();
            self.tab_order.retain(|n| {
                let ok =
                    n != "Home" && n != "Settings" && crate::view::TABS.iter().any(|t| t.2 == n) && !seen.contains(n);
                if ok {
                    seen.push(n.clone());
                }
                ok
            });
        }
        {
            let mut seen = Vec::new();
            self.header_tabs.retain(|name| {
                if seen.len() >= HEADER_TAB_LIMIT || !HIDEABLE_TABS.contains(&name.as_str()) || seen.contains(name) {
                    return false;
                }
                seen.push(name.clone());
                true
            });
        }
        if self.ai_sub != "Chat" && self.ai_sub != "AGY" {
            self.ai_sub = "Chat".into();
        }
        self.tab_anim_speed = clamp_f(self.tab_anim_speed, 0.4, 2.5, d.tab_anim_speed);
        self.hide_pill_secs = self.hide_pill_secs.clamp(5, 10);
        self.ghost_opacity = clamp_f(self.ghost_opacity, 0.05, 1.0, d.ghost_opacity);
        self.metrics_secs = self.metrics_secs.clamp(1, 10);
        self.memory_excluded = self
            .memory_excluded
            .iter()
            .take(128)
            .filter(|path| path.len() <= 2048 && !path.contains(['\0', '\r', '\n']))
            .map(|path| crate::data::desktop_dock::key(path))
            .collect();
        self.memory_excluded.sort();
        self.memory_excluded.dedup();
        for list in [&mut self.metrics_tab, &mut self.metrics_pill] {
            let mut seen: Vec<String> = Vec::new();
            for key in std::mem::take(list) {
                if METRIC_KEYS.contains(&key.as_str()) && !seen.contains(&key) {
                    seen.push(key);
                }
            }
            *list = seen;
        }
        let mut seen = Vec::new();
        for id in std::mem::take(&mut self.home_actions) {
            if usize::from(id) < HOME_ACTION_COUNT && !seen.contains(&id) {
                seen.push(id);
            }
        }
        self.home_actions = seen;
        self.sanitize_keys();
        let mut apps: Vec<String> = Vec::new();
        for a in &self.lyrics_apps {
            let name = clean_app_name(a);
            if !name.is_empty() && !apps.contains(&name) && apps.len() < MAX_LYRICS_APPS {
                apps.push(name);
            }
        }
        self.lyrics_apps = apps;
    }

    pub fn save(&self) {
        if cfg!(test) {
            return;
        }
        let Some(path) = Self::path() else { return };
        let Some(dir) = path.parent() else { return };
        if fs::create_dir_all(dir).is_err() {
            return;
        }
        let Ok(json) = serde_json::to_string_pretty(self) else { return };
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_err() || fs::rename(&tmp, &path).is_err() {
            crate::log_warn!("could not save settings.json");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_tour_opens_for_new_and_existing_profiles_once() {
        let path = std::env::temp_dir().join(format!("ni-tour-settings-{}.json", std::process::id()));
        assert!(Settings::load_from(Some(path.clone())).tour_pending());
        fs::write(&path, r#"{"scale_percent":120,"tour_revision":2}"#).unwrap();
        let mut settings = Settings::load_from(Some(path.clone()));
        assert!(settings.tour_pending());
        assert_eq!(settings.scale_percent, 120);
        settings.dismiss_tour();
        fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
        let settings = Settings::load_from(Some(path.clone()));
        assert!(!settings.tour_pending());
        assert_eq!(settings.scale_percent, 120);
        fs::write(&path, "invalid").unwrap();
        assert!(!Settings::load_from(Some(path.clone())).tour_pending());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn older_settings_keep_spark_and_custom_pets_survive_restarts() {
        use crate::ui::pet_motion::PetPerformance;
        let legacy: Settings = serde_json::from_str(r#"{"character":"spark"}"#).unwrap();
        assert_eq!(legacy.character, Character::Spark);
        assert_eq!(legacy.pet_performance, PetPerformance::Balanced);
        assert!(legacy.pet_autonomy);
        for performance in PetPerformance::ALL {
            let mut settings =
                Settings { character: Character::Custom, pet_performance: performance, ..Default::default() };
            settings.pet_autonomy = false;
            settings.custom_pet.name = "Мишка".into();
            settings.custom_pet.ear_size = 40.0;
            settings.sanitize();
            let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored.character, Character::Custom);
            assert_eq!(restored.pet_performance, performance);
            assert!(!restored.pet_autonomy);
            assert_eq!(restored.custom_pet, settings.custom_pet);
            assert_eq!(restored.custom_pet.ear_size, 1.25);
        }
    }

    #[test]
    fn equalizer_response_migrates_without_replacing_existing_preferences() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"eq_style":"bars","eq_strength":1.0,"bump_tint":"progress","eq_smooth":true}"#)
                .unwrap();
        assert_eq!(settings.eq_response, EqResponse::Responsive);
        for response in EqResponse::ALL {
            settings.eq_response = response;
            let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored.eq_response, response);
            assert_eq!(restored.eq_style, EqStyle::Bars);
            assert_eq!(restored.eq_strength, 1.0);
            assert_eq!(restored.bump_tint, BumpTint::Progress);
            assert!(restored.eq_smooth);
        }
    }

    #[test]
    fn weather_side_defaults_to_left_and_survives_settings_round_trips() {
        let legacy: Settings = serde_json::from_str(r#"{"pill_weather":true}"#).unwrap();
        assert_eq!(legacy.pill_weather_side, WeatherSide::Left);
        for side in WeatherSide::ALL {
            let settings = Settings { pill_weather_side: side, ..Settings::default() };
            let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored.pill_weather_side, side);
        }
    }

    #[test]
    fn header_tabs_add_without_replacing_and_can_move_back_to_more() {
        let mut settings = Settings::default();
        assert!(settings.set_header_tab("AI", true, None));
        assert_eq!(settings.header_tabs.len(), 6);
        assert!(settings.set_header_tab("AI", false, None));
        assert!(settings.set_header_tab("AI", true, Some("Record")));
        assert!(settings.header_tabs.iter().any(|name| name == "AI"));
        assert!(!settings.header_tabs.iter().any(|name| name == "Record"));
        assert!(settings.set_header_tab("AI", false, None));
        assert!(!settings.tab_hidden("AI"));
        settings.set_tab_hidden("Privacy", true);
        assert!(settings.set_header_tab("Privacy", true, None));
        assert!(!settings.tab_hidden("Privacy"));
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored.header_tabs, settings.header_tabs);
        let mut invalid: Settings = serde_json::from_str(r#"{"header_tabs":["Home","AI","AI","evil","Settings","Weather","Record","Privacy","Activity","Controls"]}"#).unwrap();
        invalid.sanitize();
        assert_eq!(invalid.header_tabs, ["AI", "Weather", "Record", "Privacy", "Activity", "Controls"]);
    }

    #[test]
    fn notices_keep_their_old_defaults_and_read_partial_files() {
        let n = NotifySettings::default();
        assert!(n.on(Notice::Mic) && n.on(Notice::Download) && n.on(Notice::Error));
        assert!(!n.on(Notice::Clipboard) && !n.on(Notice::LockKeys) && !n.on(Notice::Battery) && !n.clipboard_text);
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.notify, n);
        let partial: Settings = serde_json::from_str(r#"{"notify":{"lock_keys":true,"mic":false}}"#).unwrap();
        assert!(partial.notify.lock_keys && !partial.notify.mic && partial.notify.downloads);
    }

    #[test]
    fn hostile_ai_settings_fall_back_to_defaults() {
        let json = r#"{"ai":{"provider":"gemini","gemini_model":"../x?y","claude_model":"claude-3","openai_base":"http://evil.example/v1"}}"#;
        let mut s: Settings = serde_json::from_str(json).unwrap();
        s.sanitize();
        assert_eq!(s.ai.provider, AiProvider::Gemini);
        assert_eq!(s.ai.gemini_model, AiProvider::Gemini.default_model());
        assert_eq!(s.ai.claude_model, "claude-3");
        assert_eq!(s.ai.openai_base, "https://api.openai.com/v1");
        assert_eq!(AiProvider::Claude.step(true), AiProvider::OpenAi);
        assert_eq!(AiProvider::Claude.step(false), AiProvider::Gemini);
    }

    #[test]
    fn hidden_tabs_are_validated_and_toggle() {
        let mut s: Settings =
            serde_json::from_str(r#"{"hidden_tabs":["Weather","Weather","Home","evil","Text"]}"#).unwrap();
        s.sanitize();
        assert_eq!(s.hidden_tabs, ["Weather", "Text"]);
        assert!(!Settings::default().tab_hidden("Activity"), "no tab starts hidden");
        s.set_tab_hidden("Weather", false);
        s.set_tab_hidden("Home", true);
        assert_eq!(s.hidden_tabs, ["Text"]);
    }

    #[test]
    fn chat_and_agy_became_the_ai_tab() {
        let mut s: Settings = serde_json::from_str(r#"{"hidden_tabs":["AGY","Text"]}"#).unwrap();
        s.sanitize();
        assert_eq!(s.hidden_tabs, ["Text"]);
        let mut s: Settings = serde_json::from_str(r#"{"hidden_tabs":["AGY","Chat"],"ai_sub":"junk"}"#).unwrap();
        s.sanitize();
        assert_eq!(s.hidden_tabs, ["AI"]);
        assert_eq!(s.ai_sub, "Chat");
    }

    #[test]
    fn the_tab_order_drops_unknown_and_fixed_tabs() {
        let mut s: Settings =
            serde_json::from_str(r#"{"tab_order":["Text","Home","Text","nope","Settings","Weather"]}"#).unwrap();
        s.sanitize();
        assert_eq!(s.tab_order, ["Text", "Weather"]);
    }

    #[test]
    fn hotkeys_default_change_and_survive_a_hostile_file() {
        let mut s = Settings::default();
        assert_eq!(s.combo(Hotkey::HidePill), Hotkey::HidePill.default_combo());
        let c = Combo::new(crate::data::keys::CTRL | crate::data::keys::SHIFT, 'H' as u32);
        s.set_combo(Hotkey::HidePill, c);
        assert_eq!(s.combo(Hotkey::HidePill), c);
        assert_eq!(s.hotkey_using(Hotkey::Mic, c), Some(Hotkey::HidePill));
        s.set_combo(Hotkey::HidePill, Hotkey::HidePill.default_combo());
        assert!(s.keys.is_empty());

        let json = r#"{"keys":{"toggle":{"mods":0,"vk":69},"bogus":{"mods":2,"vk":69},
            "mic":{"mods":2,"vk":69},"hide_pill":{"mods":2,"vk":70}},"hide_pill_secs":99}"#;
        let mut s: Settings = serde_json::from_str(json).unwrap();
        s.sanitize();
        assert_eq!(s.combo(Hotkey::Toggle), Hotkey::Toggle.default_combo());
        assert!(!s.keys.contains_key("bogus"));
        assert_eq!(s.combo(Hotkey::Mic), Combo::new(2, 69));
        assert_eq!(s.combo(Hotkey::HidePill), Combo::new(2, 70));
        assert_eq!(s.hide_pill_secs, 10);
        assert!(s.ghost_mode && s.ghost_click_through && !s.ghost_hold);
        let mut s: Settings = serde_json::from_str(r#"{"ghost_opacity":-3}"#).unwrap();
        s.sanitize();
        assert_eq!(s.ghost_opacity, 0.05);
        assert!(s.home_actions.is_empty());
        let mut s: Settings = serde_json::from_str(r#"{"home_actions":[3,3,99,0]}"#).unwrap();
        s.sanitize();
        assert_eq!(s.home_actions, [3, 0]);

        let json = r#"{"keys":{"mic":{"mods":2,"vk":69}}}"#;
        let mut s: Settings = serde_json::from_str(json).unwrap();
        s.sanitize();
        assert_eq!(s.combo(Hotkey::Mic), Hotkey::Mic.default_combo());
    }

    #[test]
    fn trusted_lyric_players() {
        let mut s = Settings::default();
        assert!(s.lyrics_app_allowed("SPOTIFY") && s.lyrics_app_allowed(""));
        assert!(s.add_lyrics_app(" spotify "));
        assert!(!s.add_lyrics_app("SPOTIFY") && !s.add_lyrics_app("  "));
        assert!(s.lyrics_app_allowed("Spotify") && !s.lyrics_app_allowed("CHROME") && !s.lyrics_app_allowed(""));
        let mut s: Settings = serde_json::from_str(r#"{"lyrics_apps":["a","A","","b\u0007"]}"#).unwrap();
        s.sanitize();
        assert_eq!(s.lyrics_apps, ["A", "B"]);
    }

    #[test]
    fn hostile_values_are_clamped() {
        let json = r#"{"scale_percent":99999,"tint_open":1e30,"liquid_reach":-5,"open_speed":0,"open_delay_ms":4000000,"placement":"docked"}"#;
        let mut s: Settings = serde_json::from_str(json).unwrap();
        s.sanitize();
        assert_eq!(s.scale_percent, 200);
        assert_eq!(s.tint_open, 1.0);
        assert_eq!(s.liquid_reach, 60.0);
        assert_eq!(s.open_speed, 0.4);
        assert_eq!(s.open_delay_ms, 1000);
        assert_eq!(s.placement, Placement::Docked);
    }

    #[test]
    fn old_files_and_garbage_still_load() {
        let s: Settings = serde_json::from_str(r#"{"liquid":true,"auto_hide":true}"#).unwrap();
        assert!(s.auto_hide);
        assert_eq!(s.style, Style::Liquid);
        assert!(serde_json::from_str::<Settings>("not json").is_err());
    }

    #[test]
    fn player_priority_is_opt_in_and_persists() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.prefer_playing_media);
        settings.prefer_playing_media = true;
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(restored.prefer_playing_media);
    }

    #[test]
    fn new_profiles_use_spark_and_fill_without_overriding_saved_choices() {
        let fresh: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(fresh.character, Character::Spark);
        assert_eq!(fresh.progress_style, ProgressStyle::Fill);
        let saved: Settings = serde_json::from_str(r#"{"character":"none","progress_style":"outlinebottom"}"#).unwrap();
        assert_eq!(saved.character, Character::None);
        assert_eq!(saved.progress_style, ProgressStyle::OutlineBottom);
    }

    #[test]
    fn progress_modes_remain_selectable_and_round_trip() {
        for (i, mode) in ProgressStyle::ALL.iter().copied().enumerate() {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(serde_json::from_str::<ProgressStyle>(&json).unwrap(), mode);
            assert_eq!(mode.step(true), ProgressStyle::ALL[(i + 1) % ProgressStyle::ALL.len()]);
            assert_eq!(mode.step(false).step(true), mode);
        }
        assert_eq!(ProgressStyle::Strip.step(false), ProgressStyle::Fill);
    }

    #[test]
    fn desktop_character_position_and_mode_survive_settings_round_trips() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"character":"spark","character_detached":true,"character_pos":[-200,400]}"#)
                .unwrap();
        settings.sanitize();
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(restored.character_detached);
        assert_eq!(restored.character_pos, Some([-200, 400]));
        settings.character = Character::None;
        settings.sanitize();
        assert!(!settings.character_detached);
    }

    #[test]
    fn roaming_supports_every_character_and_preserves_old_spark_settings() {
        for character in Character::ALL.into_iter().filter(|c| *c != Character::None) {
            let mut settings = Settings { character, character_detached: true, ..Settings::default() };
            settings.sanitize();
            assert!(settings.character_detached, "{character:?}");
            let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored.character, character);
            assert!(restored.character_detached);
        }
        let old: Settings =
            serde_json::from_str(r#"{"character":"spark","spark_detached":true,"spark_pos":[120,340]}"#).unwrap();
        assert!(old.character_detached);
        assert_eq!(old.character_pos, Some([120, 340]));
    }
}

#[cfg(test)]
mod real_file {
    #[test]
    #[ignore]
    fn load_real() {
        println!("{:?}", super::Settings::load());
    }
}
