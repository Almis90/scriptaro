//! Reproducible typing policy; contains no platform or playback state.
use crate::Defaults;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingPreset {
    Steady,
    Natural,
    Brisk,
}

/// A preset name or an explicit timing mapping. Seeded variation is for pacing,
/// not security, and restarts for each type_text action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypingProfile {
    Preset(TypingPreset),
    Custom(TypingTiming),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypingTiming {
    pub interval_ms: u64,
    #[serde(default)]
    pub jitter_ms: u64,
    #[serde(default)]
    pub word_pause_ms: u64,
    #[serde(default)]
    pub punctuation_pause_ms: u64,
    #[serde(default)]
    pub line_pause_ms: u64,
    #[serde(default = "default_seed")]
    pub seed: u64,
}
fn default_seed() -> u64 {
    1
}
impl TypingTiming {
    pub fn uniform(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            jitter_ms: 0,
            word_pause_ms: 0,
            punctuation_pause_ms: 0,
            line_pause_ms: 0,
            seed: 1,
        }
    }
}
impl TypingProfile {
    pub fn timing(&self) -> TypingTiming {
        match self {
            Self::Custom(timing) => *timing,
            Self::Preset(TypingPreset::Steady) => TypingTiming::uniform(40),
            Self::Preset(TypingPreset::Natural) => TypingTiming {
                interval_ms: 45,
                jitter_ms: 15,
                word_pause_ms: 80,
                punctuation_pause_ms: 160,
                line_pause_ms: 300,
                seed: 1,
            },
            Self::Preset(TypingPreset::Brisk) => TypingTiming {
                interval_ms: 15,
                jitter_ms: 5,
                word_pause_ms: 20,
                punctuation_pause_ms: 60,
                line_pause_ms: 120,
                seed: 1,
            },
        }
    }
}
impl Defaults {
    /// Step profile > step fixed interval > default profile > legacy character delay.
    /// Validation rejects a step specifying both a profile and interval_ms.
    pub fn typing_timing(
        &self,
        profile: Option<&TypingProfile>,
        interval_ms: Option<u64>,
    ) -> TypingTiming {
        if let Some(profile) = profile {
            profile.timing()
        } else if let Some(interval) = interval_ms {
            TypingTiming::uniform(interval)
        } else if let Some(profile) = &self.typing_profile {
            profile.timing()
        } else {
            TypingTiming::uniform(self.character_delay_ms)
        }
    }
}
