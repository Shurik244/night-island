use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoicePhase {
    #[default]
    Idle,
    Recording,
    Transcribing,
    Command,
}

impl VoicePhase {
    pub fn cue(self) -> SpeechCue {
        match self {
            Self::Idle => SpeechCue::Quiet,
            Self::Recording => SpeechCue::Listening,
            Self::Transcribing | Self::Command => SpeechCue::Thinking,
        }
    }

    pub fn pending(self) -> bool {
        matches!(self, Self::Transcribing | Self::Command)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SpeechCue {
    #[default]
    Quiet,
    Listening,
    Thinking,
    Speaking {
        amplitude: f32,
    },
}

impl SpeechCue {
    pub fn engaged(self) -> bool {
        self != Self::Quiet
    }
}

#[derive(Default)]
pub struct PetSpeech {
    mouth: f32,
}

impl PetSpeech {
    pub fn advance(&mut self, cue: SpeechCue, dt: f32, reduced_motion: bool) -> f32 {
        let target = match cue {
            SpeechCue::Speaking { amplitude } if amplitude.is_finite() => amplitude.clamp(0.0, 1.0),
            _ => 0.0,
        };
        if reduced_motion {
            self.mouth = 0.0;
        } else {
            let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
            self.mouth += (target - self.mouth) * (1.0 - (-dt * 24.0).exp());
            if target == 0.0 && self.mouth < 0.005 {
                self.mouth = 0.0;
            }
        }
        self.mouth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_phases_distinguish_the_microphone_from_requests_and_never_fake_speech() {
        for phase in
            [VoicePhase::Idle, VoicePhase::Recording, VoicePhase::Transcribing, VoicePhase::Command, VoicePhase::Idle]
        {
            assert_eq!(phase.cue().engaged(), phase != VoicePhase::Idle);
            assert_eq!(phase.pending(), matches!(phase, VoicePhase::Transcribing | VoicePhase::Command));
            assert_eq!(phase.cue() == SpeechCue::Listening, phase == VoicePhase::Recording);
            assert!(!matches!(phase.cue(), SpeechCue::Speaking { .. }));
        }
    }

    #[test]
    fn speech_envelopes_are_bounded_stop_cleanly_and_honor_reduced_motion() {
        let mut speech = PetSpeech::default();
        for amplitude in [f32::NAN, -1.0, 0.2, 1.0, 100.0] {
            let mouth = speech.advance(SpeechCue::Speaking { amplitude }, 0.1, false);
            assert!((0.0..=1.0).contains(&mouth));
        }
        assert!(speech.advance(SpeechCue::Speaking { amplitude: 1.0 }, 0.1, false) > 0.8);
        for _ in 0..30 {
            speech.advance(SpeechCue::Quiet, 1.0 / 30.0, false);
        }
        assert_eq!(speech.mouth, 0.0);
        assert_eq!(speech.advance(SpeechCue::Speaking { amplitude: 1.0 }, 0.1, true), 0.0);
        for cue in [SpeechCue::Quiet, SpeechCue::Listening, SpeechCue::Thinking, SpeechCue::Speaking { amplitude: 0.5 }]
        {
            let encoded = serde_json::to_string(&cue).unwrap();
            assert_eq!(serde_json::from_str::<SpeechCue>(&encoded).unwrap(), cue);
            assert_eq!(cue.engaged(), cue != SpeechCue::Quiet);
        }
    }
}
