pub mod agent;
pub mod env;
pub mod memory;
pub mod perms;
pub mod recent;
pub mod shell;
pub mod tools;
pub mod web;

use serde::{Deserialize, Serialize};

pub use perms::{Cap, Level, Permissions};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TtsProvider {
    #[default]
    OpenAi,
    Eleven,
    Fish,
}

impl TtsProvider {
    pub const ALL: [TtsProvider; 3] = [TtsProvider::OpenAi, TtsProvider::Eleven, TtsProvider::Fish];

    pub fn name(self) -> &'static str {
        match self {
            TtsProvider::OpenAi => "OpenAI-compatible",
            TtsProvider::Eleven => "ElevenLabs",
            TtsProvider::Fish => "Fish Audio",
        }
    }

    pub fn defaults(self) -> (&'static str, &'static str, &'static str) {
        match self {
            TtsProvider::OpenAi => OPENAI_TTS,
            TtsProvider::Eleven => ELEVEN_TTS,
            TtsProvider::Fish => FISH_TTS,
        }
    }

    pub fn needs_own_key(self) -> bool {
        self != TtsProvider::OpenAi
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Assistant {
    pub perms: Permissions,
    pub wake: bool,
    pub sounds: bool,
    pub speak: bool,
    pub volume: u8,
    pub review: bool,
    pub ask_after_web: bool,
    pub follow_up: bool,
    pub natural_voice: bool,
    pub fish_streaming: bool,
    pub fish_expressions: bool,
    pub yandex_voice: bool,
    pub local_voice: bool,
    pub tts_provider: TtsProvider,
    #[serde(skip_serializing)]
    tts_eleven: bool,
    pub tts_base: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub max_steps: u8,
    pub reply_tokens: u16,
    pub command_secs: u8,
    pub output_chars: u16,
}

pub const STEPS_RANGE: (u8, u8) = (3, 20);
pub const TOKENS_RANGE: (u16, u16) = (500, 16_000);
pub const SECS_RANGE: (u8, u8) = (5, 180);
pub const OUTPUT_RANGE: (u16, u16) = (500, 20_000);
pub const OPENAI_TTS: (&str, &str, &str) = ("https://api.openai.com/v1", "gpt-4o-mini-tts", "nova");
pub const ELEVEN_TTS: (&str, &str, &str) =
    ("https://api.elevenlabs.io", "eleven_multilingual_v2", "21m00Tcm4TlvDq8ikWAM");
pub const FISH_TTS: (&str, &str, &str) = ("https://api.fish.audio", "s2.1-pro", FISH_DEFAULT_VOICE);
pub const FISH_DEFAULT_VOICE: &str = "default";

impl Default for Assistant {
    fn default() -> Self {
        Self {
            perms: Permissions::default(),
            wake: false,
            sounds: true,
            speak: true,
            volume: 70,
            review: true,
            ask_after_web: false,
            follow_up: true,
            natural_voice: false,
            fish_streaming: true,
            fish_expressions: true,
            yandex_voice: true,
            local_voice: false,
            tts_provider: TtsProvider::OpenAi,
            tts_eleven: false,
            tts_base: OPENAI_TTS.0.into(),
            tts_model: OPENAI_TTS.1.into(),
            tts_voice: OPENAI_TTS.2.into(),
            max_steps: 10,
            reply_tokens: 3000,
            command_secs: 60,
            output_chars: 2500,
        }
    }
}

impl Assistant {
    pub fn set_provider(&mut self, provider: TtsProvider) {
        if self.tts_provider == provider {
            return;
        }
        let (base, model, voice) = provider.defaults();
        self.tts_provider = provider;
        self.tts_base = base.into();
        self.tts_model = model.into();
        self.tts_voice = voice.into();
    }

    pub fn sanitize(&mut self) {
        self.perms.sanitize();
        self.volume = self.volume.min(100);
        self.max_steps = self.max_steps.clamp(STEPS_RANGE.0, STEPS_RANGE.1);
        self.reply_tokens = self.reply_tokens.clamp(TOKENS_RANGE.0, TOKENS_RANGE.1);
        self.command_secs = self.command_secs.clamp(SECS_RANGE.0, SECS_RANGE.1);
        self.output_chars = self.output_chars.clamp(OUTPUT_RANGE.0, OUTPUT_RANGE.1);
        if std::mem::take(&mut self.tts_eleven) && self.tts_provider == TtsProvider::OpenAi {
            self.tts_provider = TtsProvider::Eleven;
        }
        let (base, model, voice) = self.tts_provider.defaults();
        self.tts_base = crate::services::ai::clean_base(&self.tts_base).unwrap_or_else(|| base.to_string());
        if !crate::services::ai::valid_model(&self.tts_model, crate::data::settings::AiProvider::OpenAi) {
            self.tts_model = model.to_string();
        }
        let voice_ok = !self.tts_voice.is_empty()
            && self.tts_voice.len() <= 40
            && self.tts_voice.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !voice_ok {
            self.tts_voice = voice.to_string();
        }
    }

    pub fn apply(&self) {
        crate::services::ai::set_reply_tokens(u32::from(self.reply_tokens));
        shell::configure(u64::from(self.command_secs), usize::from(self.output_chars));
    }

    pub fn cue_volume(&self) -> u8 {
        if self.perms.enabled && self.sounds { self.volume } else { 0 }
    }

    pub fn voice_volume(&self) -> u8 {
        if self.perms.enabled && self.speak { self.volume } else { 0 }
    }
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Off, Level::Ask, Level::Auto];

    pub fn name(self) -> &'static str {
        match self {
            Level::Off => "Off",
            Level::Ask => "Ask first",
            Level::Auto => "Allowed",
        }
    }

    pub fn step(self, forward: bool) -> Level {
        let at = Level::ALL.iter().position(|l| *l == self).unwrap_or(0);
        let n = Level::ALL.len();
        Level::ALL[if forward { (at + 1) % n } else { (at + n - 1) % n }]
    }
}

impl Cap {
    pub fn title(self) -> &'static str {
        match self {
            Cap::Media => "Music and media keys",
            Cap::Controls => "Volume, mic, Wi-Fi, Bluetooth, theme, lock",
            Cap::Open => "Open programs and games",
            Cap::Reminders => "Reminders and timers",
            Cap::Memory => "Remember things about you",
            Cap::WebSearch => "Search the internet",
            Cap::FilesRead => "Read and search files",
            Cap::FilesWrite => "Create and change files",
            Cap::Shell => "Command line",
            Cap::Browser => "Browser (open sites, read pages)",
            Cap::Power => "Shut down, restart, sleep",
            Cap::Screen => "Look at the screen (preview, needs a model that sees images)",
            Cap::Island => "Island data: weather, music, Spotify, Discord, Steam, notes",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_cycle_both_ways() {
        assert_eq!(Level::Off.step(true), Level::Ask);
        assert_eq!(Level::Auto.step(true), Level::Off);
        assert_eq!(Level::Off.step(false), Level::Auto);
    }

    #[test]
    fn the_natural_voice_settings_are_cleaned_and_the_follow_up_is_on_by_default() {
        let mut a = Assistant::default();
        assert!(a.follow_up && !a.natural_voice);
        a.tts_model = "bad model!".into();
        a.tts_voice = "../x".into();
        a.sanitize();
        let defaults = Assistant::default();
        assert_eq!((a.tts_model, a.tts_voice), (defaults.tts_model, defaults.tts_voice));
        let mut ok = Assistant { tts_model: "tts-1".into(), tts_voice: "alloy".into(), ..Assistant::default() };
        ok.sanitize();
        assert_eq!((ok.tts_model.as_str(), ok.tts_voice.as_str()), ("tts-1", "alloy"));
        let mut bad_base = Assistant { tts_base: "http://example.com/v1".into(), ..Assistant::default() };
        bad_base.sanitize();
        assert_eq!(bad_base.tts_base, "https://api.openai.com/v1", "plain http is only for this computer");
        let mut local = Assistant { tts_base: "http://localhost:8880/v1/".into(), ..Assistant::default() };
        local.sanitize();
        assert_eq!(local.tts_base, "http://localhost:8880/v1");
        let mut eleven = Assistant::default();
        eleven.set_provider(TtsProvider::Eleven);
        assert_eq!(
            (eleven.tts_base.as_str(), eleven.tts_model.as_str(), eleven.tts_voice.as_str()),
            ("https://api.elevenlabs.io", "eleven_multilingual_v2", "21m00Tcm4TlvDq8ikWAM")
        );
        eleven.tts_voice = "bad voice!".into();
        eleven.sanitize();
        assert_eq!(eleven.tts_voice, "21m00Tcm4TlvDq8ikWAM", "a bad voice falls back to the provider's own");
        eleven.set_provider(TtsProvider::Fish);
        assert_eq!(
            (eleven.tts_base.as_str(), eleven.tts_model.as_str(), eleven.tts_voice.as_str()),
            ("https://api.fish.audio", "s2.1-pro", "default")
        );
        eleven.tts_voice = "0123456789abcdef0123456789abcdef".into();
        eleven.sanitize();
        assert_eq!(eleven.tts_voice, "0123456789abcdef0123456789abcdef", "a Fish model id is kept");
        eleven.set_provider(TtsProvider::OpenAi);
        assert_eq!((eleven.tts_base.as_str(), eleven.tts_voice.as_str()), ("https://api.openai.com/v1", "nova"));
        let legacy: Assistant = serde_json::from_str(r#"{"tts_eleven":true}"#).unwrap();
        let mut legacy = legacy;
        legacy.sanitize();
        assert_eq!(legacy.tts_provider, TtsProvider::Eleven);
        assert!(!serde_json::to_string(&legacy).unwrap().contains("tts_eleven"));
        let old: Assistant = serde_json::from_str(r#"{"wake":true}"#).unwrap();
        assert!(old.follow_up && old.tts_model == "gpt-4o-mini-tts" && old.max_steps == 10);
        for (given, kept) in [(0u8, 3u8), (2, 3), (7, 7), (200, 20)] {
            let mut a = Assistant { max_steps: given, ..Assistant::default() };
            a.sanitize();
            assert_eq!(a.max_steps, kept);
        }
    }

    #[test]
    fn the_limits_stay_inside_their_ranges_and_reach_the_workers() {
        let mut a = Assistant { reply_tokens: 1, command_secs: 0, output_chars: 60_000, ..Assistant::default() };
        a.sanitize();
        assert_eq!((a.reply_tokens, a.command_secs, a.output_chars), (500, 5, 20_000));
        let mut big = Assistant { reply_tokens: 60_000, command_secs: 250, output_chars: 1, ..Assistant::default() };
        big.sanitize();
        assert_eq!((big.reply_tokens, big.command_secs, big.output_chars), (16_000, 180, 500));
        let old: Assistant = serde_json::from_str(r#"{"wake":true}"#).unwrap();
        assert_eq!((old.reply_tokens, old.command_secs, old.output_chars), (3000, 60, 2500));
        shell::configure(u64::from(old.command_secs), usize::from(old.output_chars));
        assert_eq!(shell::clip_output(&"x".repeat(5000)).chars().count(), 2500 + "\n[cut]".len());
    }

    #[test]
    fn muted_parts_have_zero_volume() {
        let mut a = Assistant::default();
        assert_eq!((a.cue_volume(), a.voice_volume()), (0, 0));
        a.perms.enabled = true;
        assert_eq!(a.cue_volume(), 70);
        a.sounds = false;
        a.speak = false;
        assert_eq!((a.cue_volume(), a.voice_volume()), (0, 0));
        a.volume = 250;
        a.sanitize();
        assert_eq!(a.volume, 100);
    }

    #[test]
    fn every_capability_has_a_title() {
        assert!(perms::ALL.iter().all(|c| !c.title().is_empty()));
    }
}
