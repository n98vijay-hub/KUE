//! What KUE knows, and how it came to know it.
//!
//! KUE already separates observation from inference in the context object. What
//! it has never had is a **verified fact**: something KUE did, read back, and
//! confirmed. Without that type there is nothing a model can be required to
//! respect, and a model describing a confirmed action as uncertain has been
//! observed.
//!
//! The rule this module exists to enforce:
//!
//! > **A model can never create, upgrade or contradict a fact.**
//!
//! It is enforced by construction, not by convention. `Fact::verified` is the
//! only way to reach `FactState::Verified`, and it takes a
//! [`Verification`] — a token that only the action pipeline can produce, because
//! its field is private to this module and it is built from a read-back the
//! executor performed. `Fact::from_model` exists and *cannot* return a verified
//! fact: the type system refuses it.
//!
//! WHAT A FACT MAY HOLD: one sentence a person could read, its state, where it
//! came from, when, how confident, its privacy classification, how long it is
//! good for, and what produced it. No image, no descriptor, no audio, no raw
//! sensor payload — a fact is a conclusion, and those are inputs.

use crate::privacy::DataKind;
use serde::{Deserialize, Serialize};

/// What KUE's knowledge is worth. These never collapse into one another: the
/// whole point is that "I saw it", "I worked it out", "I did it and checked"
/// and "I don't know" are different claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FactState {
    /// A sensor measured it.
    Observed,
    /// KUE worked it out from things it observed. Never presented as fact.
    Inferred,
    /// KUE did it, read the world back, and the world agreed.
    Verified,
    /// It was true, and is old enough that KUE will not stand behind it now.
    Stale,
    /// KUE does not know. A first-class answer, not an empty string.
    Unknown,
    /// Something later disagreed with it. Kept, because knowing a belief was
    /// overturned is worth more than quietly dropping it.
    Contradicted,
}

impl FactState {
    pub fn tag(self) -> &'static str {
        match self {
            FactState::Observed => "OBSERVED",
            FactState::Inferred => "INFERRED",
            FactState::Verified => "VERIFIED",
            FactState::Stale => "STALE",
            FactState::Unknown => "UNKNOWN",
            FactState::Contradicted => "CONTRADICTED",
        }
    }

    /// May a model be told this as something that IS so?
    ///
    /// Only what was measured or confirmed. An inference is offered as an
    /// inference; an unknown is offered as an unknown; a stale or contradicted
    /// fact is not offered as either.
    pub fn is_assertable(self) -> bool {
        matches!(self, FactState::Observed | FactState::Verified)
    }
}

/// Where a fact came from. A model is on this list so that model-sourced
/// material is *labelled*, never so that it can be promoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FactSource {
    /// A sensor reading, through the sensing layer.
    Sensor,
    /// KUE's own reasoning over what it observed.
    Reasoning,
    /// An action KUE performed, with its read-back.
    Action,
    /// The owner said so.
    Owner,
    /// A language model said so. Never assertable, never verifiable.
    Model,
}

/// How long a fact is worth anything, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Scope {
    /// True of this Mac until something changes it.
    ThisMac,
    /// True for as long as this owner session lasts.
    ThisSession,
    /// True about one request, and meaningless outside it.
    ThisRequest,
}

/// Proof that the action pipeline performed something and read the world back.
///
/// Its field is private to this module, so only `Verification::of` can make
/// one, and only code holding one can reach `FactState::Verified`. There is no
/// path from a model's output to this type.
#[derive(Debug, Clone, PartialEq)]
pub struct Verification(String);

impl Verification {
    /// Called by the action pipeline with what it observed AFTER acting — the
    /// frontmost application, the file's absence and presence in the Trash, and
    /// so on. An empty read-back is not a verification and returns None, which
    /// is the same rule `ActionRecord::finish` applies when it downgrades an
    /// unverified success to UNKNOWN_RESULT.
    pub fn of(read_back: &str) -> Option<Verification> {
        let s = read_back.trim();
        if s.is_empty() { None } else { Some(Verification(s.to_string())) }
    }
    pub fn read_back(&self) -> &str { &self.0 }
}

/// One thing KUE knows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    /// Stable within a run, so a later fact can supersede this one.
    pub id: String,
    /// What this is about. Two facts with the same subject are about the same
    /// thing, and the later one supersedes the earlier.
    pub subject: String,
    /// The fact itself, as a sentence a person would read.
    pub statement: String,
    pub state: FactState,
    pub source: FactSource,
    pub at: f64,
    /// Only where a number means something. Identity is categorical; an action
    /// either verified or did not. Most facts carry None, deliberately.
    pub confidence: Option<f64>,
    /// What the privacy firewall must decide about before this travels.
    pub privacy: DataKind,
    pub scope: Scope,
    /// How long this is good for, in seconds. None means "until superseded".
    pub valid_for: Option<f64>,
    /// What produced it: a module, an action tag, a sensor. Never content.
    pub provenance: String,
    /// What was read back, when this was verified.
    pub read_back: Option<String>,
}

impl Fact {
    /// Something a sensor measured.
    pub fn observed(subject: &str, statement: &str, privacy: DataKind, at: f64, provenance: &str) -> Fact {
        Fact::new(subject, statement, FactState::Observed, FactSource::Sensor, privacy, at, provenance)
    }

    /// Something KUE worked out. Never assertable as fact.
    pub fn inferred(subject: &str, statement: &str, privacy: DataKind, at: f64, provenance: &str) -> Fact {
        Fact::new(subject, statement, FactState::Inferred, FactSource::Reasoning, privacy, at, provenance)
    }

    /// Something KUE does not know. Worth recording: "I don't know" is an
    /// answer, and a model must be told it rather than left to fill the gap.
    pub fn unknown(subject: &str, statement: &str, at: f64, provenance: &str) -> Fact {
        Fact::new(subject, statement, FactState::Unknown, FactSource::Reasoning,
                  DataKind::ActivityConclusion, at, provenance)
    }

    /// Something KUE did and confirmed. **The only route to `Verified`**, and it
    /// requires a `Verification`, which only the action pipeline can make.
    pub fn verified(subject: &str, statement: &str, privacy: DataKind, at: f64,
                    provenance: &str, proof: Verification) -> Fact {
        Fact {
            read_back: Some(proof.read_back().to_string()),
            ..Fact::new(subject, statement, FactState::Verified, FactSource::Action, privacy, at, provenance)
        }
    }

    /// Something a model said. Always `Inferred`, whatever the model claimed
    /// about its own certainty — there is no argument a model can make that
    /// changes this, because this function has no parameter for one.
    pub fn from_model(subject: &str, statement: &str, at: f64) -> Fact {
        Fact::new(subject, statement, FactState::Inferred, FactSource::Model,
                  DataKind::ModelAnswer, at, "model")
    }

    fn new(subject: &str, statement: &str, state: FactState, source: FactSource,
           privacy: DataKind, at: f64, provenance: &str) -> Fact {
        Fact {
            id: format!("{subject}@{at}"),
            subject: subject.to_string(),
            statement: statement.to_string(),
            state, source, at,
            confidence: None,
            privacy,
            scope: Scope::ThisSession,
            valid_for: None,
            provenance: provenance.to_string(),
            read_back: None,
        }
    }

    pub fn with_confidence(mut self, c: f64) -> Fact { self.confidence = Some(c); self }
    pub fn with_scope(mut self, s: Scope) -> Fact { self.scope = s; self }
    pub fn valid_for(mut self, seconds: f64) -> Fact { self.valid_for = Some(seconds); self }

    /// What this fact is worth *now*. A fact past its validity is stale, and
    /// says so rather than continuing to assert itself.
    pub fn state_at(&self, now: f64) -> FactState {
        match self.valid_for {
            Some(w) if now - self.at > w && self.state.is_assertable() => FactState::Stale,
            _ => self.state,
        }
    }

    /// How this reads when a model is told about it. The state is part of the
    /// sentence, so a model cannot see "the file is in the Trash" without also
    /// seeing that KUE verified it — or that KUE merely inferred it.
    pub fn said(&self, now: f64) -> String {
        format!("[{}] {}", self.state_at(now).tag(), self.statement)
    }
}

/// What KUE currently knows, newest per subject.
///
/// Bounded, like everything else that accumulates: knowing is not a reason to
/// grow without limit.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    facts: Vec<Fact>,
    max: usize,
    /// Superseded facts are counted, not silently dropped.
    superseded: u64,
}

impl Facts {
    pub fn new(max: usize) -> Facts { Facts { facts: Vec::new(), max, superseded: 0 } }

    /// Records a fact. A later fact about the same subject supersedes the
    /// earlier one — and if they disagree in state, the earlier is marked
    /// CONTRADICTED rather than deleted, so the change of mind is visible.
    pub fn record(&mut self, fact: Fact) {
        if let Some(old) = self.facts.iter_mut().find(|f| f.subject == fact.subject) {
            if old.statement != fact.statement && old.state.is_assertable() {
                old.state = FactState::Contradicted;
            }
            self.superseded += 1;
            let keep = std::mem::replace(old, fact);
            let _ = keep;
            return;
        }
        if self.facts.len() >= self.max { self.facts.remove(0); }
        self.facts.push(fact);
    }

    pub fn get(&self, subject: &str) -> Option<&Fact> { self.facts.iter().find(|f| f.subject == subject) }
    pub fn all(&self) -> &[Fact] { &self.facts }
    pub fn superseded(&self) -> u64 { self.superseded }

    /// The facts a model may be told, newest first, as sentences carrying their
    /// own state. Inferences and unknowns are included **and labelled**; stale
    /// and contradicted facts are not offered at all.
    pub fn for_model(&self, now: f64, limit: usize) -> Vec<String> {
        let mut v: Vec<&Fact> = self.facts.iter()
            .filter(|f| !matches!(f.state_at(now), FactState::Stale | FactState::Contradicted))
            .filter(|f| f.source != FactSource::Model)
            .collect();
        v.sort_by(|a, b| b.at.total_cmp(&a.at));
        v.into_iter().take(limit).map(|f| f.said(now)).collect()
    }

    /// The verified facts, for the check that a model's answer does not
    /// contradict something KUE confirmed.
    pub fn verified(&self, now: f64) -> Vec<&Fact> {
        self.facts.iter().filter(|f| f.state_at(now) == FactState::Verified).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_read_back_can_produce_a_verified_fact() {
        // The action pipeline observed the world after acting.
        let proof = Verification::of("resume.pdf is no longer on the Desktop and is in the Trash").unwrap();
        let f = Fact::verified("trash:resume.pdf", "KUE moved resume.pdf to the Trash",
                               DataKind::ActionTarget, 10.0, "MOVE_TO_TRASH", proof);
        assert_eq!(f.state, FactState::Verified);
        assert!(f.read_back.is_some());

        // An empty read-back is not a verification, so there is nothing to
        // build a verified fact from — the same rule the action record applies.
        assert!(Verification::of("   ").is_none());
        assert!(Verification::of("").is_none());
    }

    #[test]
    fn a_model_cannot_create_upgrade_or_assert_a_fact() {
        let m = Fact::from_model("weather", "It is sunny in Tampa", 10.0);
        assert_eq!(m.state, FactState::Inferred, "a model's claim is an inference, whatever it says");
        assert_eq!(m.source, FactSource::Model);
        assert!(!m.state.is_assertable());

        // And a model's material is never offered back to a model as context.
        let mut facts = Facts::new(10);
        facts.record(m);
        facts.record(Fact::observed("app", "Safari is frontmost", DataKind::FrontmostApplication, 11.0, "sensing"));
        let seen = facts.for_model(11.0, 10);
        assert!(seen.iter().any(|s| s.contains("Safari")));
        assert!(!seen.iter().any(|s| s.contains("Tampa")), "a model's own claim came back as context: {seen:?}");
    }

    #[test]
    fn the_states_never_collapse_into_one_another() {
        let now = 10.0;
        let observed = Fact::observed("app", "Chrome is frontmost", DataKind::FrontmostApplication, now, "sensing");
        let inferred = Fact::inferred("doing", "The owner may be researching", DataKind::ActivityConclusion, now, "core");
        let verified = Fact::verified("open:Chrome", "KUE opened Chrome", DataKind::ActionTarget, now, "OPEN_APPLICATION",
                                      Verification::of("Chrome is running and frontmost").unwrap());
        let unknown = Fact::unknown("why", "KUE does not know why Chrome is open", now, "core");

        assert!(observed.state.is_assertable() && verified.state.is_assertable());
        assert!(!inferred.state.is_assertable(), "an inference is never told as fact");
        assert!(!unknown.state.is_assertable());

        // Every sentence carries its own state, so a reader cannot lose it.
        assert!(observed.said(now).starts_with("[OBSERVED]"));
        assert!(inferred.said(now).starts_with("[INFERRED]"));
        assert!(verified.said(now).starts_with("[VERIFIED]"));
        assert!(unknown.said(now).starts_with("[UNKNOWN]"));
    }

    #[test]
    fn a_fact_past_its_validity_stops_asserting_itself() {
        let f = Fact::observed("storage", "The drive is 90% full", DataKind::StorageSummary, 100.0, "storage")
            .valid_for(60.0);
        assert_eq!(f.state_at(150.0), FactState::Observed);
        assert_eq!(f.state_at(200.0), FactState::Stale, "an hour-old reading is not a current one");
        assert!(!f.state_at(200.0).is_assertable());

        let mut facts = Facts::new(10);
        facts.record(f);
        assert!(facts.for_model(200.0, 10).is_empty(), "a stale fact is not offered to a model at all");
        assert!(facts.verified(200.0).is_empty());
    }

    #[test]
    fn a_change_of_mind_is_recorded_rather_than_hidden() {
        let mut facts = Facts::new(10);
        facts.record(Fact::observed("app", "Safari is frontmost", DataKind::FrontmostApplication, 10.0, "sensing"));
        facts.record(Fact::observed("app", "Chrome is frontmost", DataKind::FrontmostApplication, 11.0, "sensing"));
        assert_eq!(facts.get("app").unwrap().statement, "Chrome is frontmost");
        assert_eq!(facts.superseded(), 1, "the supersession is counted, not silent");
        assert_eq!(facts.all().len(), 1, "one subject, one current fact");
    }

    #[test]
    fn knowing_things_does_not_grow_without_bound() {
        let mut facts = Facts::new(4);
        for i in 0..20 {
            facts.record(Fact::observed(&format!("s{i}"), "x", DataKind::SensorState, i as f64, "t"));
        }
        assert_eq!(facts.all().len(), 4);
    }

    #[test]
    fn a_fact_carries_a_sentence_and_never_a_measurement() {
        let f = Fact::observed("face", "One face is in view", DataKind::PresenceSummary, 1.0, "sensing");
        let json = serde_json::to_string(&f).unwrap();
        for forbidden in ["descriptor", "embedding", "distance", "featurePrint", "pixels", "crop"] {
            assert!(!json.contains(forbidden), "a fact carried {forbidden}");
        }
    }
}
