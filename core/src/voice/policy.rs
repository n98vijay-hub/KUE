//! The speech gate: whether KUE may say a sentence now.
//!
//! In order, and each refusal means nothing is spoken:
//!   1. KILLED              — KUE says nothing while killed.
//!   2. SILENCED            — verbosity is SILENT.
//!   3. LISTENING           — the microphone is live; KUE does not talk over you or into it.
//!                            Live means a fresh STARTING or LISTENING from a running
//!                            sensing layer (`microphone_busy`). FINISHING is not listening:
//!                            the microphone is already off. A stale report is not live.
//!   4. EMPTY               — nothing to say.
//!   5. OWNER_REQUIRED      — a sentence built from any data is said only at LEVEL_2, the
//!                            level at which the window shows the conversation and actions.
//!                            It is refused here when the session is not yours (locked, a
//!                            stranger, a second person). During a dip in identity confidence
//!                            it is cleared but waits in the queue for LEVEL_2 (`speaker`).
//!                            A sentence with no data ("I need you to authenticate
//!                            first.") may be said to whoever is there.
//!   6. PRIVACY_DENIED      — the firewall refused a kind the sentence carries.
//!   7. NO_PROVIDER         — no provider may speak it (external speech is refused).

use super::provider::{route, ProviderAvailability, VoiceProviderId};
use super::{fit_for_speech, SpeechDraft, Verbosity, VoiceSettings};
use crate::authz::{AccessBlock, AccessState, AuthLevel, SessionPhase};
use crate::engine::Engine;
use crate::privacy::{Cleared, Firewall};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpeechRefusal {
    Killed,
    Silenced,
    Listening,
    Empty,
    OwnerRequired,
    PrivacyDenied,
    NoProvider(String),
}

/// While listening, the sensing layer reports the level every 0.2 s. A
/// LISTENING not refreshed for this long is stale: the process stalled or its
/// report was lost, and it does not keep KUE silent.
pub const LISTENING_FRESH_SECONDS: f64 = 2.0;
/// STARTING covers the macOS permission prompt and loading the speech model.
pub const STARTING_FRESH_SECONDS: f64 = 15.0;

/// Whether the microphone is live now. Only a fresh report from a running
/// sensing layer counts; paused, killed, finished, cancelled or failed never do.
pub fn microphone_busy(engine: &Engine, now: f64) -> bool {
    if engine.is_killed() || engine.is_paused() || !engine.sensing_process_up() { return false; }
    let Some(at) = engine.voice_reported_at() else { return false };
    let age = now - at;
    match engine.voice_state() {
        "LISTENING" => age <= LISTENING_FRESH_SECONDS,
        "STARTING" => age <= STARTING_FRESH_SECONDS,
        _ => false,
    }
}

/// The session is not the owner's: locked, someone who is not you, or more than one person.
/// Unknown identity, a head turned away or a missed frame is not this: it is uncertainty.
pub fn session_not_yours(access: &AccessBlock) -> bool {
    matches!(access.state, AccessState::Locked | AccessState::UnknownPerson | AccessState::MultiplePeople)
        || access.phase == SessionPhase::Locked
}

/// Decides one sentence. Ok is the only form a provider accepts.
pub fn clear_for_speech(engine: &Engine, firewall: &mut Firewall, mut draft: SpeechDraft, settings: &VoiceSettings,
                        available: ProviderAvailability, now: f64)
    -> Result<(Cleared<SpeechDraft>, VoiceProviderId), SpeechRefusal>
{
    if engine.is_killed() { return Err(SpeechRefusal::Killed); }
    if settings.verbosity == Verbosity::Silent { return Err(SpeechRefusal::Silenced); }
    if microphone_busy(engine, now) { return Err(SpeechRefusal::Listening); }
    draft.text = fit_for_speech(&draft.text);
    if draft.text.is_empty() { return Err(SpeechRefusal::Empty); }
    let access = engine.access_block(now);
    if !draft.carries.is_empty() && access.level < AuthLevel::Level2 && session_not_yours(&access) {
        return Err(SpeechRefusal::OwnerRequired);
    }
    let carries = draft.carries.clone();
    let cleared = firewall.clear_utterance(draft, now).ok_or(SpeechRefusal::PrivacyDenied)?;
    let decision = route(&carries, available);
    match decision.chosen {
        Some(p) => Ok((cleared, p)),
        None => Err(SpeechRefusal::NoProvider(decision.considered.iter()
            .map(|(p, o)| format!("{p:?}: {o}")).collect::<Vec<_>>().join("; "))),
    }
}
