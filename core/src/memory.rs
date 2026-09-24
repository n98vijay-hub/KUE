//! What KUE keeps about the owner's world, and why it kept it.
//!
//! This is not the conversation, and it is not a log. `facts.rs` already says
//! what KUE knows *now* and how it came to know it; this says what is worth
//! keeping **after this run ends**, in categories a person would recognise, with
//! the reason it was kept attached to it.
//!
//! THE RULES THIS MODULE EXISTS TO ENFORCE
//!
//! 1. **Nothing is kept without a reason.** A memory carries the governed
//!    reason it exists (`Why`), and there is no constructor without one.
//! 2. **A model cannot make a memory true.** `Memory::proposed_by_model` has no
//!    parameter for state and always produces CANDIDATE, which retrieval never
//!    offers as something the owner said. VERIFIED needs a `Verification` from
//!    the action pipeline (`facts.rs`) and a constructor only this crate can
//!    call.
//! 3. **Nothing is overwritten silently.** A memory that disagrees with one
//!    already kept does not replace it: the owner is asked, unless they said a
//!    replacing word ("instead", "not any more"), in which case the old one
//!    becomes SUPERSEDED and stays readable as history.
//! 4. **Forgetting forgets.** `forget` removes the words, keeping only that
//!    something was forgotten and when. Retrieval never returns it again.
//!
//! WHAT A MEMORY MAY HOLD: one sentence a person could read, its class, its
//! state, where it came from, when it was said, how long it is good for, and
//! what produced it. No file contents, no descriptors, no raw sensor payload.
//! The privacy firewall decides every write and every journey, by the
//! `DataKind` the memory declares.

use crate::facts::{FactSource, Verification};
use crate::privacy::DataKind;
use serde::{Deserialize, Serialize};

/// What kind of thing this is. Each exists because KUE behaves differently
/// about it — not for completeness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryClass {
    /// How the owner likes things done ("I prefer PDF reports"). Consulted
    /// before KUE proposes; superseded when they change their mind.
    Preference,
    /// Something about the owner's world they told KUE ("I work on the SAP
    /// project"). Retrieved as context; never inferred into existence.
    Fact,
    /// Something the owner is trying to get done that outlives one request.
    /// Distinct from a goal in flight (goal.rs), which is this run's.
    Goal,
    /// Work KUE did and verified. The only class that may be VERIFIED, because
    /// it is the only one with a read-back behind it.
    Work,
    /// A decision made about how to proceed, kept so "why?" has an answer
    /// later ("you approved plan p2").
    Decision,
    /// Something true of one task and not beyond it ("for this cleanup, leave
    /// the installers"). Expires, and is never read as a preference.
    TaskNote,
}

impl MemoryClass {
    pub fn tag(self) -> &'static str {
        match self {
            MemoryClass::Preference => "PREFERENCE", MemoryClass::Fact => "FACT",
            MemoryClass::Goal => "GOAL", MemoryClass::Work => "WORK",
            MemoryClass::Decision => "DECISION", MemoryClass::TaskNote => "TASK_NOTE",
        }
    }

    pub fn from_tag(tag: &str) -> Option<MemoryClass> {
        Some(match tag {
            "PREFERENCE" => MemoryClass::Preference, "FACT" => MemoryClass::Fact,
            "GOAL" => MemoryClass::Goal, "WORK" => MemoryClass::Work,
            "DECISION" => MemoryClass::Decision, "TASK_NOTE" => MemoryClass::TaskNote,
            _ => return None,
        })
    }

    /// What the owner sees as a heading.
    pub fn heading(self) -> &'static str {
        match self {
            MemoryClass::Preference => "How you like things done",
            MemoryClass::Fact => "About you and your work",
            MemoryClass::Goal => "What you're working towards",
            MemoryClass::Work => "What KUE did",
            MemoryClass::Decision => "Decisions",
            MemoryClass::TaskNote => "Just for now",
        }
    }
}

/// What a memory is worth. These never collapse into one another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryState {
    /// Somebody — a model, a rule — thinks this might be worth keeping. Never
    /// retrieved as something the owner said, and never acted on.
    Candidate,
    /// The owner said it, or a governed rule promoted it from work that was
    /// done. This is what retrieval normally returns.
    Confirmed,
    /// KUE did it and read the world back. Only reachable with a
    /// `Verification` from the action pipeline.
    Verified,
    /// A later memory replaced it. Kept, because a change of mind is worth
    /// knowing; never returned as current.
    Superseded,
    /// Past the time it was good for. Never returned as current.
    Stale,
    /// The owner forgot it. The words are gone; only that it happened remains.
    Deleted,
}

impl MemoryState {
    pub fn tag(self) -> &'static str {
        match self {
            MemoryState::Candidate => "CANDIDATE", MemoryState::Confirmed => "CONFIRMED",
            MemoryState::Verified => "VERIFIED", MemoryState::Superseded => "SUPERSEDED",
            MemoryState::Stale => "STALE", MemoryState::Deleted => "DELETED",
        }
    }

    pub fn from_tag(tag: &str) -> Option<MemoryState> {
        Some(match tag {
            "CANDIDATE" => MemoryState::Candidate, "CONFIRMED" => MemoryState::Confirmed,
            "VERIFIED" => MemoryState::Verified, "SUPERSEDED" => MemoryState::Superseded,
            "STALE" => MemoryState::Stale, "DELETED" => MemoryState::Deleted,
            _ => return None,
        })
    }

    /// May this be told to the owner — or a model — as something that holds
    /// now? A candidate has not been agreed to; the rest have stopped being
    /// true, been replaced, or been forgotten.
    pub fn is_current(self) -> bool { matches!(self, MemoryState::Confirmed | MemoryState::Verified) }
}

/// Why this is kept. There is no constructor without one: "we had it" is not a
/// reason to keep something about a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Why {
    /// The owner asked KUE to remember it, in so many words.
    OwnerSaidSo,
    /// The owner corrected KUE, and the correction was about how they want
    /// things done rather than about one task.
    OwnerCorrected,
    /// KUE did something and verified it.
    WorkVerified,
    /// The owner approved a plan, or chose between options.
    OwnerDecided,
    /// A model suggested it. A reason to ASK, never a reason to keep.
    ModelProposed,
}

impl Why {
    pub fn tag(self) -> &'static str {
        match self {
            Why::OwnerSaidSo => "OWNER_SAID_SO", Why::OwnerCorrected => "OWNER_CORRECTED",
            Why::WorkVerified => "WORK_VERIFIED", Why::OwnerDecided => "OWNER_DECIDED",
            Why::ModelProposed => "MODEL_PROPOSED",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Why> {
        Some(match tag {
            "OWNER_SAID_SO" => Why::OwnerSaidSo, "OWNER_CORRECTED" => Why::OwnerCorrected,
            "WORK_VERIFIED" => Why::WorkVerified, "OWNER_DECIDED" => Why::OwnerDecided,
            "MODEL_PROPOSED" => Why::ModelProposed,
            _ => return None,
        })
    }
}

/// One thing KUE keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    /// What it is about, as a key: two memories with the same subject are
    /// about the same thing, and the later one may replace the earlier.
    pub subject: String,
    /// The memory itself, as a sentence a person would read.
    pub statement: String,
    pub class: MemoryClass,
    pub state: MemoryState,
    pub source: FactSource,
    pub why: Why,
    /// What the privacy firewall decides about before this is written or
    /// travels anywhere.
    pub privacy: DataKind,
    pub created_at: f64,
    pub updated_at: f64,
    /// How long it is good for, in seconds. None means "until it is replaced".
    pub valid_for: Option<f64>,
    /// What produced it: the owner's own words, an action tag, a rule. Never
    /// invented — when there is nothing to say, retrieval says so.
    pub provenance: String,
    /// The request, goal or plan it belongs to, when it belongs to one.
    pub relates_to: Option<String>,
    /// The memory that replaced this one.
    pub superseded_by: Option<String>,
    pub deleted_at: Option<f64>,
}

impl Memory {
    /// Something the owner told KUE to remember. CONFIRMED because they said
    /// it — not because KUE worked anything out.
    pub fn told(id: &str, class: MemoryClass, subject: &str, statement: &str, privacy: DataKind, at: f64,
                said: &str) -> Memory {
        Memory { provenance: format!("you said: “{}”", said.trim()),
                 ..Memory::new(id, class, subject, statement, MemoryState::Confirmed, FactSource::Owner,
                               Why::OwnerSaidSo, privacy, at) }
    }

    /// Something a model suggested keeping. **Always CANDIDATE** — there is no
    /// parameter here for a state, a confidence or a verification, so no
    /// argument a model makes can change what this returns.
    pub fn proposed_by_model(id: &str, class: MemoryClass, subject: &str, statement: &str, at: f64) -> Memory {
        Memory::new(id, class, subject, statement, MemoryState::Candidate, FactSource::Model,
                    Why::ModelProposed, DataKind::ModelAnswer, at)
    }

    /// Work KUE did and read the world back for. Crate-private: it takes the
    /// `Verification` the action pipeline produced, and only this crate's
    /// action path has one.
    pub(crate) fn verified(id: &str, subject: &str, statement: &str, privacy: DataKind, at: f64,
                           provenance: &str, proof: Verification) -> Memory {
        // The read-back PROVES this may exist; it is not kept. What it says
        // names the owner's files, which policy allows KUE to show them and
        // not to store — so the memory records that the world was read back,
        // and how much of it, never which file.
        let _ = proof;
        Memory { provenance: format!("KUE did it and checked it: {provenance}"),
                 ..Memory::new(id, MemoryClass::Work, subject, statement, MemoryState::Verified,
                               FactSource::Action, Why::WorkVerified, privacy, at) }
    }

    /// A decision the owner made, kept so "why did you do that?" has an answer
    /// after the conversation has moved on.
    pub fn decided(id: &str, subject: &str, statement: &str, at: f64, provenance: &str) -> Memory {
        Memory { provenance: provenance.to_string(),
                 ..Memory::new(id, MemoryClass::Decision, subject, statement, MemoryState::Confirmed,
                               FactSource::Owner, Why::OwnerDecided, DataKind::EventRecord, at) }
    }

    fn new(id: &str, class: MemoryClass, subject: &str, statement: &str, state: MemoryState,
           source: FactSource, why: Why, privacy: DataKind, at: f64) -> Memory {
        Memory {
            id: id.to_string(), subject: subject.to_string(), statement: statement.trim().to_string(),
            class, state, source, why, privacy, created_at: at, updated_at: at,
            valid_for: None, provenance: String::new(), relates_to: None,
            superseded_by: None, deleted_at: None,
        }
    }

    pub fn valid_for(mut self, seconds: f64) -> Memory { self.valid_for = Some(seconds); self }
    pub fn about(mut self, what: &str) -> Memory { self.relates_to = Some(what.to_string()); self }

    /// What this is worth *now*. Time alone can make a memory stale; nothing
    /// else changes underneath the owner.
    pub fn state_at(&self, now: f64) -> MemoryState {
        match self.valid_for {
            Some(w) if now - self.created_at > w && self.state.is_current() => MemoryState::Stale,
            _ => self.state,
        }
    }

    pub fn is_current(&self, now: f64) -> bool { self.state_at(now).is_current() }

    /// The answer to "why do you remember that?". Never invented: when there
    /// is no provenance, it says exactly that.
    pub fn why_kept(&self, said_when: &str) -> String {
        if self.provenance.trim().is_empty() {
            return "I don't have enough evidence to tell you why that memory was created.".into();
        }
        format!("{said_when}, {}.", self.provenance)
    }
}

/// The words that decide what a memory is about. Everything else is grammar.
const NOT_A_TOPIC: [&str; 34] = ["that", "i", "you", "my", "me", "the", "a", "an", "to", "of", "in", "on",
    "for", "and", "or", "is", "are", "was", "were", "do", "dont", "don't", "please", "remember", "kue",
    "prefer", "prefers", "prefer's", "want", "wants", "like", "likes", "always", "never"];

/// The content words of a sentence, lower-cased, in order, without the words
/// every sentence has. Used to key a subject and to match a question against
/// what is kept — narrow on purpose: it is better to keep two memories than to
/// replace one KUE only half understood.
pub fn topic_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| w.len() > 1 && !NOT_A_TOPIC.contains(&w.as_str()))
        .collect()
}

/// The key two memories must share to be about the same thing.
pub fn subject_of(class: MemoryClass, statement: &str) -> String {
    let mut words = topic_words(statement);
    words.sort();
    words.dedup();
    format!("{}:{}", class.tag().to_lowercase(), words.join("-"))
}

/// What happened when something was offered to the memory.
#[derive(Debug, Clone, PartialEq)]
pub enum Remembered {
    /// Kept, and nothing it disagrees with was already there.
    Kept(String),
    /// Kept, and the memory it replaces is now SUPERSEDED.
    Replaced { id: String, superseded: String },
    /// Something already kept covers the same ground and says something else.
    /// KUE does not choose between them: the owner is asked.
    Conflicts { with: Vec<String> },
    /// The same thing is already kept, so there is nothing to do.
    AlreadyKnown(String),
}

/// What KUE keeps, and what is waiting to be written down.
///
/// Bounded like everything else that accumulates. The rows the store holds are
/// the same memories; this is the index, and it is authoritative while KUE
/// runs.
#[derive(Debug, Clone, Default)]
pub struct MemoryBook {
    memories: Vec<Memory>,
    /// Written by the pump on its next pass, through the firewall.
    pending: Vec<Memory>,
    /// Forgotten by the owner: removed from the store on the next pass.
    forgotten: Vec<String>,
    max: usize,
    next: u64,
    /// The memory the conversation last spoke about, so "why do you remember
    /// that?" has a "that". Never a guess: when nothing was mentioned, the
    /// question is answered by saying so.
    last_mentioned: Option<String>,
}

impl MemoryBook {
    pub fn new(max: usize) -> MemoryBook { MemoryBook { max, next: 1, ..Default::default() } }

    /// An id no other memory has, stable within a run and unique across runs
    /// because it carries the time it was made.
    pub fn id(&mut self, at: f64) -> String {
        let id = format!("m{}-{}", self.next, at.round() as i64);
        self.next += 1;
        id
    }

    /// Everything kept, including what is no longer current. The window shows
    /// current memories; Diagnostics and "why" need the rest.
    pub fn all(&self) -> &[Memory] { &self.memories }

    /// What holds now, newest first. Never a candidate, never superseded,
    /// never stale, never forgotten.
    pub fn current(&self, now: f64) -> Vec<&Memory> {
        let mut v: Vec<&Memory> = self.memories.iter().filter(|m| m.is_current(now)).collect();
        v.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        v
    }

    pub fn get(&self, id: &str) -> Option<&Memory> { self.memories.iter().find(|m| m.id == id) }

    pub fn mention(&mut self, id: &str) { self.last_mentioned = Some(id.to_string()); }
    pub fn last_mentioned(&self) -> Option<&Memory> {
        self.last_mentioned.as_ref().and_then(|id| self.get(id)).filter(|m| m.state != MemoryState::Deleted)
    }

    /// What is kept about a question, best first. Matching is on the words the
    /// question and the memory share — a memory is returned because it is
    /// about what was asked, not because a model thought it was relevant.
    pub fn matching(&self, question: &str, now: f64) -> Vec<&Memory> {
        self.scored(question, now).into_iter().map(|(_, m)| m).collect()
    }

    /// The one memory a question fits BEST, when one does. A tie is not a
    /// best fit: KUE asks rather than choosing which of the owner's memories
    /// it meant — the same rule a correction to a plan follows.
    pub fn best(&self, question: &str, now: f64) -> Option<&Memory> {
        let scored = self.scored(question, now);
        match scored.as_slice() {
            [(_, m)] => Some(m),
            [(top, m), (next, _), ..] if top > next => Some(m),
            _ => None,
        }
    }

    fn scored(&self, question: &str, now: f64) -> Vec<(usize, &Memory)> {
        let asked = topic_words(question);
        if asked.is_empty() { return Vec::new(); }
        let mut scored: Vec<(usize, &Memory)> = self.memories.iter()
            .filter(|m| m.is_current(now))
            .filter_map(|m| {
                let words = topic_words(&m.statement);
                let hits = asked.iter()
                    .filter(|a| words.iter().any(|w| w == *a || w.starts_with(a.as_str()) || a.starts_with(w.as_str())))
                    .count();
                (hits > 0).then_some((hits, m))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.updated_at.total_cmp(&a.1.updated_at)));
        scored
    }

    /// Offers a memory. Nothing already kept is overwritten: a memory about
    /// the same subject that says something else is a CONFLICT, and the owner
    /// decides. `replacing` is the owner saying so in so many words ("instead",
    /// "not any more"), which is a decision, not a guess.
    pub fn remember(&mut self, m: Memory, replacing: bool, now: f64) -> Remembered {
        // What could be the same thing said differently. Two memories of the
        // same subject always could. Beyond that, only PREFERENCES: one way of
        // liking something done excludes another, while two facts that share a
        // word usually add to each other ("you work at SAP", "you work on
        // HANA") and both are worth keeping.
        let same: Vec<&Memory> = self.memories.iter()
            .filter(|old| old.is_current(now) && old.class == m.class)
            .filter(|old| old.subject == m.subject
                          || (m.class == MemoryClass::Preference && shares_ground(&old.statement, &m.statement)))
            .collect();
        if let Some(identical) = same.iter().find(|old| old.statement.eq_ignore_ascii_case(&m.statement)) {
            return Remembered::AlreadyKnown(identical.id.clone());
        }
        match (same.len(), replacing) {
            (0, _) => { let id = m.id.clone(); self.keep(m); Remembered::Kept(id) }
            (_, true) => {
                let ids: Vec<String> = same.iter().map(|o| o.id.clone()).collect();
                let new_id = m.id.clone();
                for id in &ids { self.supersede(id, &new_id, now); }
                self.keep(m);
                Remembered::Replaced { id: new_id, superseded: ids.join(", ") }
            }
            (_, false) => Remembered::Conflicts { with: same.iter().map(|o| o.id.clone()).collect() },
        }
    }

    /// Keeps a memory the owner has decided about — used after a conflict was
    /// put to them and they said to replace.
    pub fn replace(&mut self, old_ids: &[String], m: Memory, now: f64) -> Remembered {
        let new_id = m.id.clone();
        for id in old_ids { self.supersede(id, &new_id, now); }
        self.keep(m);
        Remembered::Replaced { id: new_id, superseded: old_ids.join(", ") }
    }

    fn supersede(&mut self, old_id: &str, new_id: &str, now: f64) {
        if let Some(old) = self.memories.iter_mut().find(|x| x.id == old_id) {
            old.state = MemoryState::Superseded;
            old.superseded_by = Some(new_id.to_string());
            old.updated_at = now;
            let changed = old.clone();
            self.pending.push(changed);
        }
    }

    fn keep(&mut self, m: Memory) {
        if self.memories.len() >= self.max {
            // The oldest thing no longer current goes first; KUE does not drop
            // something that holds now to make room for something new.
            let drop_at = self.memories.iter().position(|x| !x.state.is_current())
                .unwrap_or(0);
            let dropped = self.memories.remove(drop_at);
            self.forgotten.push(dropped.id);
        }
        self.pending.push(m.clone());
        self.memories.push(m);
    }

    /// The owner forgetting something. The words go; that it was forgotten,
    /// and when, stays — so the count of what KUE keeps is still honest.
    pub fn forget(&mut self, id: &str, now: f64) -> Option<Memory> {
        let m = self.memories.iter_mut().find(|m| m.id == id)?;
        let gone = m.clone();
        m.state = MemoryState::Deleted;
        m.statement = String::new();
        m.subject = String::new();
        m.provenance = String::new();
        m.deleted_at = Some(now);
        m.updated_at = now;
        self.forgotten.push(id.to_string());
        Some(gone)
    }

    /// Memories to write down, and memories to remove from the store. Taken by
    /// the pump, which clears each through the firewall; a kill means they are
    /// never written (pump.rs).
    pub fn take_pending(&mut self) -> Vec<Memory> { std::mem::take(&mut self.pending) }
    pub fn take_forgotten(&mut self) -> Vec<String> { std::mem::take(&mut self.forgotten) }

    /// What the store held when KUE started. Replaces the index; nothing is
    /// queued for writing, because these came from there.
    pub fn load(&mut self, rows: Vec<Memory>) {
        self.next = self.next.max(rows.len() as u64 + 1);
        self.memories = rows.into_iter().filter(|m| m.state != MemoryState::Deleted).collect();
    }

    pub fn counts(&self, now: f64) -> (usize, usize) {
        (self.current(now).len(), self.memories.iter().filter(|m| !m.state.is_current()).count())
    }
}

/// Whether two statements are about the same ground: they share a content
/// word that is not a value. Narrow on purpose — when KUE cannot tell, it
/// keeps both and lets the owner say.
fn shares_ground(a: &str, b: &str) -> bool {
    let (x, y) = (topic_words(a), topic_words(b));
    x.iter().any(|w| y.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: f64) -> f64 { t }

    #[test]
    fn a_model_cannot_make_a_memory_that_holds() {
        let m = Memory::proposed_by_model("m1", MemoryClass::Preference, "preference:pdf",
                                          "The owner prefers PDF reports", at(10.0));
        assert_eq!(m.state, MemoryState::Candidate);
        assert_eq!(m.source, FactSource::Model);
        assert_eq!(m.why, Why::ModelProposed);
        assert!(!m.is_current(at(10.0)), "a candidate is never current");

        // And it is not retrieved as something the owner said.
        let mut book = MemoryBook::new(10);
        book.remember(m, false, at(10.0));
        assert!(book.current(at(10.0)).is_empty());
        assert!(book.matching("what do you remember about reports", at(10.0)).is_empty());
    }

    #[test]
    fn what_the_owner_said_is_kept_and_found_again() {
        let mut book = MemoryBook::new(10);
        let id = book.id(at(10.0));
        let m = Memory::told(&id, MemoryClass::Preference, &subject_of(MemoryClass::Preference, "I prefer PDF reports"),
                             "You prefer PDF reports", DataKind::OwnerMessage, at(10.0), "remember that I prefer PDF reports");
        assert_eq!(book.remember(m, false, at(10.0)), Remembered::Kept(id.clone()));

        let found = book.matching("what do you remember about my report preferences", at(20.0));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].statement, "You prefer PDF reports");
        assert_eq!(found[0].state, MemoryState::Confirmed);
        assert!(found[0].why_kept("On 23 September").contains("you said: “remember that I prefer PDF reports”"));

        // Offered again, it is not kept twice.
        let again = Memory::told(&book.id(at(30.0)), MemoryClass::Preference,
                                 &subject_of(MemoryClass::Preference, "I prefer PDF reports"),
                                 "You prefer PDF reports", DataKind::OwnerMessage, at(30.0), "same again");
        assert_eq!(book.remember(again, false, at(30.0)), Remembered::AlreadyKnown(id));
    }

    #[test]
    fn a_disagreement_is_asked_about_and_a_replacement_is_not() {
        let mut book = MemoryBook::new(10);
        let first = book.id(at(10.0));
        book.remember(Memory::told(&first, MemoryClass::Preference, "preference:pdf-reports",
                                   "You prefer PDF reports", DataKind::OwnerMessage, at(10.0), "…"), false, at(10.0));

        // Same ground, different words, and no word that means "replace": ask.
        let second = book.id(at(20.0));
        let docx = Memory::told(&second, MemoryClass::Preference, "preference:docx-reports",
                                "You prefer DOCX reports", DataKind::OwnerMessage, at(20.0), "…");
        assert_eq!(book.remember(docx.clone(), false, at(20.0)), Remembered::Conflicts { with: vec![first.clone()] });
        assert_eq!(book.current(at(20.0)).len(), 1, "nothing was kept while KUE was unsure");

        // The owner says it replaces: the old one is superseded, and stays readable.
        let r = book.remember(docx, true, at(21.0));
        assert_eq!(r, Remembered::Replaced { id: second.clone(), superseded: first.clone() });
        let current = book.current(at(21.0));
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].statement, "You prefer DOCX reports");
        let old = book.get(&first).unwrap();
        assert_eq!(old.state, MemoryState::Superseded);
        assert_eq!(old.superseded_by.as_deref(), Some(second.as_str()));
        assert_eq!(old.statement, "You prefer PDF reports", "history stays readable");
    }

    #[test]
    fn forgetting_removes_the_words_and_retrieval_never_returns_them() {
        let mut book = MemoryBook::new(10);
        let id = book.id(at(10.0));
        book.remember(Memory::told(&id, MemoryClass::Preference, "preference:pdf",
                                   "You prefer PDF reports", DataKind::OwnerMessage, at(10.0), "…"), false, at(10.0));
        let gone = book.forget(&id, at(30.0)).expect("it was there");
        assert_eq!(gone.statement, "You prefer PDF reports");

        assert!(book.matching("reports", at(31.0)).is_empty());
        assert!(book.current(at(31.0)).is_empty());
        let tomb = book.get(&id).unwrap();
        assert_eq!(tomb.state, MemoryState::Deleted);
        assert_eq!(tomb.statement, "", "the words are gone");
        assert_eq!(tomb.deleted_at, Some(30.0));
        assert!(book.take_forgotten().contains(&id), "the store is told to remove it");
    }

    #[test]
    fn a_memory_that_was_only_good_for_a_while_stops_being_current() {
        let mut book = MemoryBook::new(10);
        let id = book.id(at(10.0));
        let note = Memory::told(&id, MemoryClass::TaskNote, "task:installers",
                                "For this cleanup, leave the installers", DataKind::OwnerMessage, at(10.0), "…")
            .valid_for(60.0);
        book.remember(note, false, at(10.0));
        assert_eq!(book.current(at(30.0)).len(), 1);
        assert!(book.current(at(100.0)).is_empty(), "it was for one task, and the task is over");
        assert_eq!(book.get(&id).unwrap().state_at(at(100.0)), MemoryState::Stale);
        assert!(book.matching("installers", at(100.0)).is_empty());
    }

    #[test]
    fn only_the_action_pipeline_can_make_a_verified_memory() {
        // The one route: a read-back the executor produced.
        let proof = Verification::of("/Users/x/KUE/Reports exists and is a folder").expect("a read-back");
        let m = Memory::verified("m1", "work:create_directory", "KUE made a folder in ~/KUE and checked it",
                                 DataKind::EventRecord, at(10.0), "CREATE_DIRECTORY", proof);
        assert_eq!(m.state, MemoryState::Verified);
        assert_eq!(m.source, FactSource::Action);
        assert!(m.provenance.contains("did it and checked"));
        assert!(!m.provenance.contains("/Users/"), "the read-back proves it; it is not kept");
        assert!(m.is_current(at(10.0)));

        // What a model produces cannot reach that state, whatever it says.
        let claim = Memory::proposed_by_model("m2", MemoryClass::Work, "work:create_directory",
                                              "VERIFIED: KUE made the folder, confirmed", at(11.0));
        assert_eq!(claim.state, MemoryState::Candidate);
    }

    #[test]
    fn two_facts_about_the_same_thing_can_both_be_true_and_both_are_kept() {
        let mut book = MemoryBook::new(10);
        for said in ["You work at SAP", "You work on the HANA project"] {
            let id = book.id(at(1.0));
            let m = Memory::told(&id, MemoryClass::Fact, &subject_of(MemoryClass::Fact, said), said,
                                 DataKind::OwnerMessage, at(1.0), said);
            assert!(matches!(book.remember(m, false, at(1.0)), Remembered::Kept(_)), "{said}");
        }
        assert_eq!(book.current(at(2.0)).len(), 2);
        // Preferences are different: one way of wanting something done
        // excludes another, so KUE asks rather than keeping both.
        let a = book.id(at(3.0));
        book.remember(Memory::told(&a, MemoryClass::Preference, "preference:reports-pdf", "You prefer PDF reports",
                                   DataKind::OwnerMessage, at(3.0), "…"), false, at(3.0));
        let b = book.id(at(4.0));
        let r = book.remember(Memory::told(&b, MemoryClass::Preference, "preference:reports-docx", "You prefer DOCX reports",
                                           DataKind::OwnerMessage, at(4.0), "…"), false, at(4.0));
        assert_eq!(r, Remembered::Conflicts { with: vec![a] });
    }

    #[test]
    fn the_memory_the_words_fit_best_is_the_one_meant_and_a_tie_is_not() {
        let mut book = MemoryBook::new(10);
        for said in ["You work on the SAP project", "You work with the HANA team"] {
            let id = book.id(at(1.0));
            book.remember(Memory::told(&id, MemoryClass::Fact, &subject_of(MemoryClass::Fact, said), said,
                                       DataKind::OwnerMessage, at(1.0), said), false, at(1.0));
        }
        assert_eq!(book.best("that I work on the SAP project", at(2.0)).map(|m| m.statement.as_str()),
                   Some("You work on the SAP project"));
        // "I work" fits both equally: no best, so the caller asks.
        assert!(book.best("that I work", at(2.0)).is_none());
        assert_eq!(book.matching("that I work", at(2.0)).len(), 2);
    }

    #[test]
    fn a_subject_is_what_the_sentence_is_about_and_nothing_else() {
        assert_eq!(subject_of(MemoryClass::Preference, "I prefer PDF reports"), "preference:pdf-reports");
        // The words every sentence has do not make two memories different.
        assert_eq!(subject_of(MemoryClass::Preference, "that I prefer PDF reports"),
                   subject_of(MemoryClass::Preference, "I prefer PDF reports"));
        assert_ne!(subject_of(MemoryClass::Preference, "I prefer PDF reports"),
                   subject_of(MemoryClass::Fact, "I prefer PDF reports"), "a preference is not a fact");
    }

    #[test]
    fn what_is_kept_is_bounded_and_what_holds_now_is_kept_last() {
        let mut book = MemoryBook::new(3);
        for (i, said) in ["You work on the SAP project", "Your desk machine is this MacBook",
                          "Your interview is in October"].into_iter().enumerate() {
            let id = book.id(at(i as f64));
            book.remember(Memory::told(&id, MemoryClass::Fact, &subject_of(MemoryClass::Fact, said),
                                       said, DataKind::OwnerMessage, at(i as f64), "…"),
                          false, at(i as f64));
        }
        assert_eq!(book.all().len(), 3, "facts that merely share the language are all kept");
        // One of them is no longer current; that is the one that goes.
        let stale_id = book.all()[1].id.clone();
        book.forget(&stale_id, at(4.0));
        let id = book.id(at(5.0));
        book.remember(Memory::told(&id, MemoryClass::Fact, "fact:new", "Something new",
                                   DataKind::OwnerMessage, at(5.0), "…"), false, at(5.0));
        assert_eq!(book.all().len(), 3);
        assert!(book.current(at(5.0)).iter().any(|m| m.statement == "Something new"));
        assert!(book.current(at(5.0)).iter().any(|m| m.statement == "You work on the SAP project"),
                "what holds now was not dropped to make room");
    }
}
