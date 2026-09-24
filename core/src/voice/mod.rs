//! KUE voice output: what KUE says is decided here, in core; how it sounds is
//! left to a provider.
//!
//!   ACTION RECORD / ANSWER → NARRATOR (KUE-authored sentences, `narration`)
//!     → SPEECH GATE (kill switch, verbosity, microphone, level, `policy`)
//!     → PRIVACY FIREWALL (`Firewall::clear_utterance`, a sealed `Cleared<SpeechDraft>`)
//!     → SPEECH QUEUE (`speaker`: priority, interruption, kill/pause/lock, request states, audit)
//!     → VOICE PROVIDER (`provider`: MACOS_NATIVE on this Mac; EXTERNAL_TTS refused)
//!     → AUDIO OUTPUT
//!
//! A model never writes narration, and nothing reaches a provider except a
//! `Cleared<SpeechDraft>`: a model answer is spoken only as the text the
//! firewall cleared, so a model cannot make KUE say something merely by
//! generating it. Personality settings (voice, speed, volume, verbosity) change
//! how KUE sounds and how much it says; none of them changes what it may say.

pub mod echo;
pub mod narration;
pub mod policy;
pub mod provider;
pub mod reference;
pub mod speaker;
pub mod wake;

use crate::privacy::DataKind;
use serde::{Deserialize, Serialize};

/// How a request reached KUE. Narration may repeat back a name the owner said
/// aloud — anyone in the room already heard it — but never reads out a target
/// that was only typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InputSource {
    #[default]
    Text,
    Voice,
}

impl InputSource {
    /// "VOICE" is voice; anything else is treated as typed, which speaks less.
    pub fn from_tag(tag: &str) -> InputSource {
        if tag == "VOICE" { InputSource::Voice } else { InputSource::Text }
    }
}

/// How urgently a sentence should be heard, lowest first. The queue says the
/// highest first; AUTHORIZATION_REQUIRED and CRITICAL_SAFETY cut off anything
/// lower that is being said (`speaker`), the rest wait their turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    /// An answer to a question.
    GeneralInformation,
    /// "Opening Calculator.", "Checking your access."
    ActionProgress,
    /// What happened: done, failed, unverified, cancelled, refused, not found.
    ActionResult,
    /// KUE needs you: a confirmation, Touch ID, or to authenticate again.
    AuthorizationRequired,
    /// Reserved for a safety notice. Nothing in this build produces one.
    CriticalSafety,
}

/// How much KUE says about what it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verbosity {
    /// Nothing is spoken.
    Silent,
    /// Questions for you, outcomes and problems.
    Brief,
    /// Brief, plus a word when an action starts.
    Normal,
    /// Normal, plus the access check at Confirm.
    Detailed,
}

/// The longest sentence KUE will speak. A longer model answer is cut at the
/// last sentence end before this, and the window shows the whole of it.
pub const MAX_SPOKEN_CHARS: usize = 1200;

/// Something KUE may say, before the firewall has seen it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeechDraft {
    pub text: String,
    /// Every kind of data the sentence is built from. The firewall checks each.
    /// Empty only for a sentence that carries no data at all ("Cancelled.").
    pub carries: Vec<DataKind>,
    pub priority: Priority,
    /// What the sentence is about ("a3", "answer"), for de-duplication.
    pub topic: String,
}

impl SpeechDraft {
    pub fn new(text: impl Into<String>, carries: &[DataKind], priority: Priority, topic: impl Into<String>) -> Self {
        SpeechDraft { text: text.into(), carries: carries.to_vec(), priority, topic: topic.into() }
    }
}

/// Cuts text to at most `MAX_SPOKEN_CHARS`, at a sentence end where there is one.
pub fn fit_for_speech(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() <= MAX_SPOKEN_CHARS { return t.to_string(); }
    let head: String = t.chars().take(MAX_SPOKEN_CHARS).collect();
    match head.rfind(['.', '!', '?']) {
        Some(i) if i > MAX_SPOKEN_CHARS / 3 => head[..=i].to_string(),
        _ => head,
    }
}

/// How KUE sounds and how much it says. None of these fields widens what may
/// be spoken: every sentence still passes the gate and the firewall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceSettings {
    /// A voice identifier you chose; None means KUE's default choice.
    pub voice: Option<String>,
    /// 1.0 is the voice's normal pace; 0.6–1.6.
    pub speed: f64,
    /// 0.0–1.0 of the system output volume.
    pub volume: f64,
    pub verbosity: Verbosity,
    /// Answers to questions you SPOKE are spoken. Answers to typed questions
    /// are spoken only when this is on.
    pub speak_typed_answers: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings { voice: None, speed: 1.0, volume: 1.0, verbosity: Verbosity::Normal, speak_typed_answers: false }
    }
}

impl VoiceSettings {
    pub const MIN_SPEED: f64 = 0.6;
    pub const MAX_SPEED: f64 = 1.6;

    /// Settings as received from the window, clamped to their ranges.
    pub fn clamped(mut self) -> Self {
        self.speed = if self.speed.is_finite() { self.speed.clamp(Self::MIN_SPEED, Self::MAX_SPEED) } else { 1.0 };
        self.volume = if self.volume.is_finite() { self.volume.clamp(0.0, 1.0) } else { 1.0 };
        self.voice = self.voice.filter(|v| !v.trim().is_empty());
        self
    }

    /// AVSpeechUtterance's rate scale, where 0.5 is the default pace.
    pub fn provider_rate(&self) -> f64 {
        0.5 + (self.speed.clamp(Self::MIN_SPEED, Self::MAX_SPEED) - 1.0) * 0.25
    }
}

/// `[voice]` in lantern.toml: the starting settings, and the spoken-confirmation window.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceCfg {
    pub verbosity: Verbosity,
    pub speed: f64,
    pub volume: f64,
    /// A voice identifier; empty means KUE's default choice.
    pub voice: String,
    /// The language KUE's default voice is chosen for.
    pub language: String,
    pub speak_typed_answers: bool,
    /// A spoken "yes" confirms only a request that has waited at most this long.
    pub spoken_confirmation_seconds: f64,
    /// Whether KUE listens for its name without being asked. OFF until the
    /// owner turns it on: a microphone that opens itself is not something a
    /// person should discover.
    #[serde(default)]
    pub wake_enabled: bool,
    /// What to listen for. Measured phrases and their numbers are in
    /// `docs/KUE_VOICE_ARCHITECTURE.md`; the default is the one that measured
    /// best on this Mac.
    #[serde(default = "default_wake_phrase")]
    pub wake_phrase: String,
}

fn default_wake_phrase() -> String { crate::voice::wake::DEFAULT_PHRASE.to_string() }

impl Default for VoiceCfg {
    fn default() -> Self {
        let s = VoiceSettings::default();
        VoiceCfg { verbosity: s.verbosity, speed: s.speed, volume: s.volume, voice: String::new(),
                   language: "en-US".into(), speak_typed_answers: s.speak_typed_answers, spoken_confirmation_seconds: 60.0,
                   wake_enabled: false, wake_phrase: default_wake_phrase() }
    }
}

impl VoiceCfg {
    pub fn settings(&self) -> VoiceSettings {
        VoiceSettings { voice: Some(self.voice.clone()), speed: self.speed, volume: self.volume,
                        verbosity: self.verbosity, speak_typed_answers: self.speak_typed_answers }.clamped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_at_a_sentence_end() {
        let long = format!("{} Last sentence that does not fit.", "Short sentence. ".repeat(100));
        let cut = fit_for_speech(&long);
        assert!(cut.chars().count() <= MAX_SPOKEN_CHARS);
        assert!(cut.ends_with('.'), "{cut}");
        assert_eq!(fit_for_speech("  Hello.  "), "Hello.");
    }

    #[test]
    fn settings_from_the_window_are_clamped() {
        let s = VoiceSettings { voice: Some("  ".into()), speed: 9.0, volume: -1.0, ..Default::default() }.clamped();
        assert_eq!((s.voice, s.speed, s.volume), (None, VoiceSettings::MAX_SPEED, 0.0));
        let s = VoiceSettings { speed: f64::NAN, volume: f64::INFINITY, ..Default::default() }.clamped();
        assert_eq!((s.speed, s.volume), (1.0, 1.0));
        assert_eq!(VoiceSettings::default().provider_rate(), 0.5);
    }
}
