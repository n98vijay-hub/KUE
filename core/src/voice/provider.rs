//! Voice providers, and which voice KUE uses.
//!
//! Candidates in this build:
//!   MACOS_NATIVE — AVSpeechSynthesizer in the `kue-voice` process, on this Mac.
//!   EXTERNAL_TTS — none is configured. Listed so the refusal is explicit and
//!                  tested: sending a sentence to a speech service off this Mac
//!                  is a transmission, and policy v1 lets no personal data leave.
//!
//! A KUE-trained voice does not exist and is not listed.

use crate::privacy::{classify, decide, DataKind, Destination};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VoiceProviderId {
    MacosNative,
    ExternalTts,
}

pub const PROVIDERS: [VoiceProviderId; 2] = [VoiceProviderId::MacosNative, VoiceProviderId::ExternalTts];

#[derive(Debug, Clone, Copy)]
pub struct ProviderAvailability {
    /// The `kue-voice` executable was found.
    pub macos_native: bool,
    pub external_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderDecision {
    pub chosen: Option<VoiceProviderId>,
    pub considered: Vec<(VoiceProviderId, String)>,
}

/// Picks the provider for one sentence. The on-device synthesizer keeps the
/// text on this Mac, so it needs nothing beyond the clearance the sentence
/// already has; an external service must be allowed every kind the sentence
/// carries off the device.
pub fn route(carries: &[DataKind], available: ProviderAvailability) -> ProviderDecision {
    let mut considered = Vec::new();
    for p in PROVIDERS {
        let outcome = match p {
            VoiceProviderId::MacosNative if !available.macos_native => "not available: kue-voice was not found".to_string(),
            VoiceProviderId::MacosNative => "chosen".to_string(),
            VoiceProviderId::ExternalTts => {
                match carries.iter().find(|k| !decide(classify(**k), Destination::ExternalModel).is_allow()) {
                    Some(k) => format!("refused by privacy policy: {} may not leave this Mac", k.tag()),
                    None if !available.external_configured => "not configured".to_string(),
                    None => "chosen".to_string(),
                }
            }
        };
        let chosen = outcome == "chosen";
        considered.push((p, outcome));
        if chosen { return ProviderDecision { chosen: Some(p), considered }; }
    }
    ProviderDecision { chosen: None, considered }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VoiceQuality {
    Default,
    Enhanced,
    Premium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VoiceGender {
    Female,
    Male,
    Unspecified,
}

/// One installed voice, as `kue-voice` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub identifier: String,
    pub name: String,
    pub language: String,
    pub quality: VoiceQuality,
    pub gender: VoiceGender,
    /// Bells, Bubbles, Zarvox and the like.
    pub novelty: bool,
    /// A Personal Voice: a synthetic copy of a real person's voice. Never used.
    pub personal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceChoice {
    pub voice: Option<VoiceInfo>,
    /// Why this voice, in words for the window.
    pub why: String,
}

/// KUE's default voice: female, not a novelty, not a Personal Voice, the best
/// installed quality, in your language where possible. Measured on this Mac on
/// 2026-09-15: every installed English voice is DEFAULT quality, and Samantha
/// (en-US, compact) is the only en-US female voice that is not a novelty. An
/// Enhanced or Premium voice you install in System Settings is preferred
/// automatically; KUE never downloads one itself.
fn default_voice<'a>(voices: &'a [VoiceInfo], language: &str) -> Option<&'a VoiceInfo> {
    let lang_prefix = language.split(['-', '_']).next().unwrap_or("");
    let rank = |v: &VoiceInfo| (
        v.quality,
        v.language.eq_ignore_ascii_case(language),
        v.language.split(['-', '_']).next().unwrap_or("").eq_ignore_ascii_case(lang_prefix),
    );
    voices.iter()
        .filter(|v| !v.personal && !v.novelty && v.gender == VoiceGender::Female)
        .filter(|v| v.language.split(['-', '_']).next().unwrap_or("").eq_ignore_ascii_case(lang_prefix))
        // Highest rank wins; among equals, the first in name order, so the choice is stable.
        .min_by(|a, b| rank(b).cmp(&rank(a)).then_with(|| a.name.cmp(&b.name)))
}

pub fn choose_voice(voices: &[VoiceInfo], requested: Option<&str>, language: &str) -> VoiceChoice {
    let fallback = |note: &str| {
        let v = default_voice(voices, language);
        let why = match &v {
            Some(v) => format!("{note}{} ({}, {} quality): KUE's default — a female voice in your language, the best quality installed.",
                v.name, v.language, format!("{:?}", v.quality).to_lowercase()),
            None => format!("{note}No installed female voice in {language} that is not a novelty voice. Choose a voice, or add one in System Settings → Accessibility → Spoken Content."),
        };
        VoiceChoice { voice: v.cloned(), why }
    };
    let Some(id) = requested else { return fallback("") };
    match voices.iter().find(|v| v.identifier == id) {
        Some(v) if v.personal => fallback("A Personal Voice is never used. "),
        Some(v) => VoiceChoice { voice: Some(v.clone()), why: format!("{} ({}), chosen by you.", v.name, v.language) },
        None => fallback("The voice you chose is not installed. "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, name: &str, lang: &str, q: VoiceQuality, g: VoiceGender, novelty: bool, personal: bool) -> VoiceInfo {
        VoiceInfo { identifier: id.into(), name: name.into(), language: lang.into(), quality: q, gender: g, novelty, personal }
    }

    /// The English voices `kue-voice` reported on this Mac, 2026-09-15 (abridged, same shape).
    fn measured() -> Vec<VoiceInfo> {
        use VoiceGender::*;
        use VoiceQuality::*;
        vec![
            v("com.apple.voice.super-compact.en-AU.Karen", "Karen", "en-AU", Default, Female, false, false),
            v("com.apple.voice.super-compact.en-IE.Moira", "Moira", "en-IE", Default, Female, false, false),
            v("com.apple.voice.compact.en-US.Samantha", "Samantha", "en-US", Default, Female, false, false),
            v("com.apple.voice.compact.en-ZA.Tessa", "Tessa", "en-ZA", Default, Female, false, false),
            v("com.apple.voice.compact.en-GB.Daniel", "Daniel", "en-GB", Default, Male, false, false),
            v("com.apple.speech.synthesis.voice.Fred", "Fred", "en-US", Default, Male, false, false),
            v("com.apple.speech.synthesis.voice.Bubbles", "Bubbles", "en-US", Default, Unspecified, true, false),
            v("com.apple.speech.synthesis.voice.Princess", "Superstar", "en-US", Default, Unspecified, true, false),
            v("com.apple.eloquence.en-US.Shelley", "Shelley", "en-US", Default, Unspecified, false, false),
        ]
    }

    #[test]
    fn on_this_mac_the_default_voice_is_samantha() {
        let c = choose_voice(&measured(), None, "en-US");
        assert_eq!(c.voice.unwrap().name, "Samantha");
        assert!(c.why.contains("default quality"), "{}", c.why);
    }

    #[test]
    fn an_installed_enhanced_or_premium_voice_is_preferred() {
        let mut voices = measured();
        voices.push(v("com.apple.voice.enhanced.en-GB.Stephanie", "Stephanie", "en-GB", VoiceQuality::Enhanced, VoiceGender::Female, false, false));
        assert_eq!(choose_voice(&voices, None, "en-US").voice.unwrap().name, "Stephanie", "quality outranks locale");
        voices.push(v("com.apple.voice.premium.en-US.Zoe", "Zoe", "en-US", VoiceQuality::Premium, VoiceGender::Female, false, false));
        assert_eq!(choose_voice(&voices, None, "en-US").voice.unwrap().name, "Zoe");
    }

    #[test]
    fn a_personal_voice_is_never_used_even_when_asked_for() {
        let mut voices = measured();
        voices.push(v("personal.1", "My Voice", "en-US", VoiceQuality::Premium, VoiceGender::Female, false, true));
        assert_eq!(choose_voice(&voices, None, "en-US").voice.unwrap().name, "Samantha");
        let c = choose_voice(&voices, Some("personal.1"), "en-US");
        assert_eq!(c.voice.unwrap().name, "Samantha");
        assert!(c.why.starts_with("A Personal Voice is never used"), "{}", c.why);
    }

    #[test]
    fn novelty_voices_are_never_the_default_but_you_may_choose_one() {
        let voices: Vec<VoiceInfo> = measured().into_iter().filter(|x| x.novelty || x.gender != VoiceGender::Female).collect();
        let c = choose_voice(&voices, None, "en-US");
        assert_eq!(c.voice, None, "no female non-novelty voice: say so rather than pick Bubbles");
        assert_eq!(choose_voice(&measured(), Some("com.apple.speech.synthesis.voice.Bubbles"), "en-US").voice.unwrap().name, "Bubbles");
        let missing = choose_voice(&measured(), Some("not.installed"), "en-US");
        assert_eq!(missing.voice.unwrap().name, "Samantha");
        assert!(missing.why.starts_with("The voice you chose is not installed"));
    }

    #[test]
    fn speech_stays_on_this_mac() {
        let both = ProviderAvailability { macos_native: true, external_configured: true };
        assert_eq!(route(&[DataKind::ModelAnswer], both).chosen, Some(VoiceProviderId::MacosNative));

        let no_local = ProviderAvailability { macos_native: false, external_configured: true };
        for kind in [DataKind::ModelAnswer, DataKind::ActionTarget, DataKind::EventRecord, DataKind::IdentityConclusion] {
            let d = route(&[kind], no_local);
            assert_eq!(d.chosen, None, "{kind:?} would be sent to an external speech service");
            assert!(d.considered[1].1.starts_with("refused by privacy policy"), "{:?}", d.considered);
        }
    }
}
