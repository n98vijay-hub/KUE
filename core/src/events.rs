//! The temporal event stream.
//!
//! Lantern must not draw conclusions from a single frame. Events are recorded
//! only on meaningful CHANGE, never per frame, which keeps the timeline readable
//! and makes "how long has this been true" a real, answerable question.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventKind {
    SensingStarted,
    SensingStopped,
    Paused,
    Resumed,
    Killed,
    RecoveryBegun,
    Recovered,
    AccessChanged,
    Authentication,
    AuthorizationDenied,
    ModelInteraction,
    /// What a request was understood as: its kind, route and status. Never its words or targets.
    RequestUnderstood,
    VoiceSession,
    /// A sentence KUE was asked to say ended: completed, cancelled, blocked or failed. Never its text.
    SpeechOutput,
    Action,
    CameraStateChanged,
    FaceAppeared,
    FaceDisappeared,
    PeopleCountChanged,
    IdentityStateChanged,
    FrontmostAppChanged,
    InputActivityChanged,
    ActivityStateChanged,
    EnrollmentChanged,
    IdentityCheckChanged,
    AnalysisRateChanged,
    SensingProcessDown,
    Error,
}

/// Why a conclusion changed: the evidence and confidence at that moment.
///
/// Recorded only for changes of CONCLUSION (identity, activity). Observations
/// such as "a face appeared" are facts about a sensor and carry no "why".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Exactly `compute_confidence(evidence, stable_seconds)` at the time.
    pub confidence: f64,
    pub stable_seconds: f64,
    pub evidence: Vec<crate::evidence::EvidenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    /// Unix seconds.
    pub ts: f64,
    pub kind: EventKind,
    /// What happened, stated as an observation.
    pub summary: String,
    pub detail: Option<String>,
    pub provenance: Option<Provenance>,
}

/// An event read back from local memory, from before the current launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberedEvent {
    pub ts: f64,
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
    pub confidence: Option<f64>,
    pub evidence: Vec<crate::evidence::EvidenceItem>,
}

/// Bounded in-memory timeline. Persistence is handled separately by `store`.
pub struct EventLog {
    events: VecDeque<Event>,
    next_id: u64,
    capacity: usize,
}

impl EventLog {
    pub fn new(capacity: usize) -> Self {
        EventLog { events: VecDeque::new(), next_id: 1, capacity: capacity.max(16) }
    }

    pub fn record(
        &mut self,
        ts: f64,
        kind: EventKind,
        summary: impl Into<String>,
        detail: Option<String>,
    ) -> Event {
        self.record_with(ts, kind, summary, detail, None)
    }

    /// Records a change of conclusion together with why it was concluded.
    pub fn record_with(
        &mut self,
        ts: f64,
        kind: EventKind,
        summary: impl Into<String>,
        detail: Option<String>,
        provenance: Option<Provenance>,
    ) -> Event {
        let e = Event { id: self.next_id, ts, kind, summary: summary.into(), detail, provenance };
        self.next_id += 1;
        self.events.push_back(e.clone());
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
        e
    }

    /// Events recorded after `id`, oldest first. Used to persist only what is new.
    pub fn since(&self, id: u64) -> Vec<Event> {
        self.events.iter().filter(|e| e.id > id).cloned().collect()
    }

    /// Most recent events, newest first by the time they describe.
    ///
    /// A settled identity state is recorded once it has held, dated from when it
    /// began, so it can be recorded after an event that happened a moment later.
    /// The timeline is ordered by when things happened, not by when they were
    /// written down.
    pub fn recent(&self, n: usize) -> Vec<Event> {
        let mut out: Vec<Event> = self.events.iter().rev().take(n).cloned().collect();
        out.sort_by(|a, b| b.ts.total_cmp(&a.ts).then(b.id.cmp(&a.id)));
        out
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Tracks how long a value has held steady — the basis for temporal confidence.
#[derive(Debug, Clone)]
pub struct Stability<T: PartialEq + Clone> {
    current: Option<T>,
    since: f64,
    /// Consecutive observations agreeing with `current`.
    pub streak: u32,
}

impl<T: PartialEq + Clone> Stability<T> {
    pub fn new() -> Self {
        Stability { current: None, since: 0.0, streak: 0 }
    }

    /// Feeds an observation. Returns true when the value CHANGED.
    pub fn observe(&mut self, value: T, ts: f64) -> bool {
        match &self.current {
            Some(c) if *c == value => {
                self.streak += 1;
                false
            }
            _ => {
                self.current = Some(value);
                self.since = ts;
                self.streak = 1;
                true
            }
        }
    }

    pub fn value(&self) -> Option<&T> {
        self.current.as_ref()
    }

    pub fn stable_seconds(&self, now: f64) -> f64 {
        if self.current.is_some() { (now - self.since).max(0.0) } else { 0.0 }
    }
}

impl<T: PartialEq + Clone> Default for Stability<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_is_bounded_and_newest_first() {
        let mut l = EventLog::new(16);
        for i in 0..40 {
            l.record(i as f64, EventKind::Error, format!("e{i}"), None);
        }
        assert_eq!(l.len(), 16);
        let r = l.recent(3);
        assert_eq!(r[0].summary, "e39");
        assert_eq!(r[1].summary, "e38");
    }

    #[test]
    fn recent_is_ordered_by_when_things_happened() {
        let mut l = EventLog::new(16);
        l.record(10.0, EventKind::FrontmostAppChanged, "app", None);
        // Recorded afterwards, but describing a state that began earlier.
        l.record(9.0, EventKind::IdentityStateChanged, "identity", None);
        l.record(11.0, EventKind::FaceAppeared, "face", None);
        let order: Vec<_> = l.recent(3).into_iter().map(|e| e.summary).collect();
        assert_eq!(order, ["face", "app", "identity"]);
    }

    #[test]
    fn ids_are_monotonic() {
        let mut l = EventLog::new(4);
        let a = l.record(0.0, EventKind::Paused, "a", None);
        let b = l.record(1.0, EventKind::Resumed, "b", None);
        assert!(b.id > a.id);
    }

    #[test]
    fn stability_reports_change_and_duration() {
        let mut s: Stability<&str> = Stability::new();
        assert!(s.observe("A", 100.0), "first observation is a change");
        assert!(!s.observe("A", 101.0));
        assert!(!s.observe("A", 105.0));
        assert_eq!(s.streak, 3);
        assert!((s.stable_seconds(105.0) - 5.0).abs() < 1e-9);

        assert!(s.observe("B", 106.0), "differing value is a change");
        assert_eq!(s.streak, 1);
        assert!((s.stable_seconds(106.0)).abs() < 1e-9);
    }
}
