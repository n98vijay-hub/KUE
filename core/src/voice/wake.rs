//! Waiting to hear its name.
//!
//! The sensing layer holds the microphone and decides, inside its own boundary,
//! whether the invocation was spoken (`sensing/Sources/LanternSense/Wake.swift`).
//! This module holds what the rest of KUE is allowed to believe about that:
//!
//!   * **when the listener may run at all** — the owner turned it on, KUE is
//!     running, not paused, not killed, the sensing layer is up, and macOS has
//!     granted the microphone;
//!   * **whether it is running right now** — from a report that is recent
//!     enough to mean anything, so the window cannot keep saying "listening for
//!     its name" after the listener has died;
//!   * **what a wake is worth** — nothing, in authority terms. Hearing a name
//!     is not knowing who said it. A wake starts a request; the request is
//!     authorized exactly like one that was typed.
//!
//! The listener runs while KUE is LOCKED on purpose: being able to walk up and
//! summon it is the point. What the owner is allowed to have done afterwards is
//! decided where it is always decided — in the access session.

use serde::{Deserialize, Serialize};

/// The invocation KUE listens for when the owner has not chosen one.
///
/// Measured on this Mac, spoken by five voices: "computer" was heard 25 times
/// out of 25 with no false wake in 80 lines of ordinary speech, while "KUE" on
/// its own was heard 6 times out of 15 — it is one short syllable, and the
/// recogniser writes it as "Q", "okay", "who", "quay" or nothing at all.
/// `docs/KUE_VOICE_ARCHITECTURE.md` carries the numbers.
pub const DEFAULT_PHRASE: &str = "computer";

/// A report older than this says nothing about now. The boundary reports every
/// five seconds; twelve leaves room for a slow moment without claiming a
/// microphone that has stopped.
pub const FRESH_SECONDS: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WakeState {
    /// Not listening. The owner has not turned it on, or something stopped it.
    Off,
    Starting,
    /// Listening for the invocation, and for nothing else.
    Waiting,
    /// The invocation was heard. Momentary: the ordinary listening session takes over.
    Woke,
    PermissionDenied,
    Unavailable,
    Error,
}

impl WakeState {
    pub fn from_tag(tag: &str) -> WakeState {
        match tag {
            "STARTING" => WakeState::Starting,
            "WAITING" => WakeState::Waiting,
            "WOKE" => WakeState::Woke,
            "PERMISSION_DENIED" => WakeState::PermissionDenied,
            "UNAVAILABLE" => WakeState::Unavailable,
            "ERROR" => WakeState::Error,
            // Unknown state = not listening. Nothing is assumed from a word
            // this build does not know.
            _ => WakeState::Off,
        }
    }

    /// What the owner is told. No score, no jargon.
    pub fn said(self, phrase: &str) -> String {
        match self {
            WakeState::Waiting | WakeState::Woke => format!("Listening for “{phrase}”"),
            WakeState::Starting => "Starting to listen for its name".into(),
            WakeState::Off => "Not listening for its name".into(),
            WakeState::PermissionDenied => "Can't listen — no microphone access".into(),
            WakeState::Unavailable => "Can't listen — speech recognition is unavailable".into(),
            WakeState::Error => "Stopped listening for its name".into(),
        }
    }
}

/// What the wake boundary last reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WakeReport {
    pub state: WakeState,
    pub phrase: String,
    pub at: f64,
    /// The last wake's string-match confidence. NOT an acoustic score and NOT a
    /// biometric one: it says how well the words matched, never who spoke them.
    pub confidence: Option<f64>,
    pub detail: Option<String>,
}

impl Default for WakeReport {
    fn default() -> Self {
        WakeReport { state: WakeState::Off, phrase: DEFAULT_PHRASE.into(), at: 0.0, confidence: None, detail: None }
    }
}

/// Everything that must be true for the listener to run. All of it, or it does
/// not run: a microphone that is on for a reason KUE cannot state is exactly
/// what this product must never have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeConditions {
    pub owner_enabled: bool,
    pub running: bool,
    pub paused: bool,
    pub killed: bool,
    pub sensing_up: bool,
    /// macOS reported microphone access as granted.
    pub microphone_granted: bool,
    /// macOS reported microphone access as refused — denied, restricted, or a
    /// status KUE does not recognise. This is not the same as "not granted":
    /// access nobody has asked for yet is neither granted nor refused.
    pub microphone_refused: bool,
}

impl WakeConditions {
    /// May the listener be running, and may the window say it is? Only with
    /// access macOS has actually granted.
    pub fn may_listen(&self) -> bool {
        self.may_start() && self.microphone_granted
    }

    /// May KUE try to start it? The same conditions, except that access nobody
    /// has asked for does not forbid the attempt — the attempt is what asks.
    /// macOS shows the owner its own prompt, and the sensing layer opens
    /// nothing unless the answer is yes. Requiring a grant before the first
    /// attempt meant the question was never put, so turning this on on a Mac
    /// where the microphone had not been used did nothing at all.
    pub fn may_start(&self) -> bool {
        self.owner_enabled && self.running && !self.paused && !self.killed && self.sensing_up
            && !self.microphone_refused
    }

    /// Why not, for the owner. None when it may listen.
    pub fn why_not(&self) -> Option<&'static str> {
        if self.may_listen() { return None; }
        Some(if !self.owner_enabled { "Listening for its name is turned off." }
            else if self.killed { "KUE is stopped." }
            else if self.paused { "KUE is paused." }
            else if !self.sensing_up { "The sensing layer is not running." }
            else if self.microphone_refused { "macOS has not granted microphone access." }
            else if !self.microphone_granted { "Waiting for macOS to allow the microphone." }
            else { "KUE is not running." })
    }
}

/// What the shell should do about the microphone on this tick.
///
/// A wake ENDS a wake session: the boundary hands the microphone over and stops
/// (`Wake.swift`, `stop(reason: "WOKE")`). Nothing restarts it on its own, so
/// this decision is taken every tick from the conditions and the state — which
/// is what makes hands-free work more than once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeStep {
    /// Start listening for the invocation.
    Listen,
    /// Stop: a condition no longer holds, or the microphone is wanted elsewhere.
    Stop,
    /// The name was heard on its own. Open an ordinary listening session, so
    /// the sentence that follows is heard — by the path that says, in the
    /// window, that KUE is listening.
    Capture,
    Nothing,
}

/// Where KUE stands between hearing the bare invocation and hearing what was
/// wanted. The name listener must not be restarted in that gap: it would take
/// the microphone back and then discard the very sentence KUE is waiting for,
/// because that sentence does not begin with the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Awaiting {
    /// Nothing outstanding.
    Nothing,
    /// Woken by the name alone. The listening session has not been opened yet.
    Request,
    /// The session has been asked for. The owner is speaking into it.
    Asked,
}

/// The one decision, as a function of facts. No clock, no microphone: the shell
/// supplies what it knows and does exactly what comes back.
///
/// `busy` means the ordinary listening session holds the microphone. `stale`
/// means the last report is older than `FRESH_SECONDS` — long enough that a
/// failure has had its say and may be tried again.
pub fn next_step(state: WakeState, conditions: &WakeConditions, busy: bool,
                 awaiting: Awaiting, stale: bool) -> WakeStep {
    let running = matches!(state, WakeState::Waiting | WakeState::Starting);
    if !conditions.may_start() {
        return if running { WakeStep::Stop } else { WakeStep::Nothing };
    }
    if busy {
        // One microphone, one user of it. The sensing layer stops the wake
        // listener when a listening session starts; this says so too, so the
        // two cannot disagree.
        return if running { WakeStep::Stop } else { WakeStep::Nothing };
    }
    match awaiting {
        Awaiting::Request => return WakeStep::Capture,
        // Asked for, not yet running: wait. The wake lapses on its own if the
        // session never opens, and then the name listener comes back.
        Awaiting::Asked => return WakeStep::Nothing,
        Awaiting::Nothing => {}
    }
    match state {
        // Off covers both "never started" and "woke, and therefore stopped".
        WakeState::Off => WakeStep::Listen,
        // A failure is not retried every half second — but it is not final
        // either: a model can finish installing without KUE being restarted.
        // A refusal by macOS never gets here (`may_start`), so it is not
        // re-asked on a timer; the owner turning the setting on again is what
        // re-reads it.
        WakeState::PermissionDenied | WakeState::Unavailable | WakeState::Error if stale => WakeStep::Listen,
        _ => WakeStep::Nothing,
    }
}

/// Is the listener running right now? A stale report is not a yes: the same
/// judgement the microphone's own freshness uses, for the same reason — a
/// window that keeps saying the microphone is on after it stopped is lying.
pub fn listening_now(report: &WakeReport, conditions: &WakeConditions, now: f64) -> bool {
    conditions.may_listen()
        && matches!(report.state, WakeState::Waiting | WakeState::Woke | WakeState::Starting)
        && now - report.at <= FRESH_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> WakeConditions {
        WakeConditions { owner_enabled: true, running: true, paused: false, killed: false,
                         sensing_up: true, microphone_granted: true, microphone_refused: false }
    }

    fn never_asked() -> WakeConditions {
        WakeConditions { microphone_granted: false, microphone_refused: false, ..ok() }
    }

    fn refused() -> WakeConditions {
        WakeConditions { microphone_granted: false, microphone_refused: true, ..ok() }
    }

    fn waiting(at: f64) -> WakeReport {
        WakeReport { state: WakeState::Waiting, phrase: "computer".into(), at, confidence: None, detail: None }
    }

    #[test]
    fn every_condition_must_hold_for_the_microphone_to_be_open() {
        assert!(ok().may_listen());
        for (name, c) in [
            ("off", WakeConditions { owner_enabled: false, ..ok() }),
            ("killed", WakeConditions { killed: true, ..ok() }),
            ("paused", WakeConditions { paused: true, ..ok() }),
            ("not running", WakeConditions { running: false, ..ok() }),
            ("no sensing", WakeConditions { sensing_up: false, ..ok() }),
            ("no permission", refused()),
            ("not yet asked", never_asked()),
        ] {
            assert!(!c.may_listen(), "{name} should stop the listener");
            assert!(c.why_not().is_some(), "{name} should say why");
        }
        assert!(ok().why_not().is_none());
    }

    #[test]
    fn a_stale_report_never_means_the_microphone_is_open() {
        assert!(listening_now(&waiting(100.0), &ok(), 100.0));
        assert!(listening_now(&waiting(100.0), &ok(), 100.0 + FRESH_SECONDS));
        assert!(!listening_now(&waiting(100.0), &ok(), 100.0 + FRESH_SECONDS + 0.1),
            "the boundary stopped reporting, so KUE stops claiming");
        // And the conditions win over any report at all.
        assert!(!listening_now(&waiting(100.0), &WakeConditions { killed: true, ..ok() }, 100.0));
        assert!(!listening_now(&waiting(100.0), &WakeConditions { paused: true, ..ok() }, 100.0));
    }

    #[test]
    fn a_state_kue_does_not_know_is_not_listening() {
        assert_eq!(WakeState::from_tag("WAITING"), WakeState::Waiting);
        assert_eq!(WakeState::from_tag("SOMETHING_NEW"), WakeState::Off);
        assert_eq!(WakeState::from_tag(""), WakeState::Off);
        let off = WakeReport { state: WakeState::from_tag("SOMETHING_NEW"), at: 1.0, ..Default::default() };
        assert!(!listening_now(&off, &ok(), 1.0));
    }

    #[test]
    fn the_listener_starts_again_after_it_has_woken() {
        // A wake stops the wake session. If nothing starts it again, hands-free
        // works exactly once per launch — so this is the test that says it does.
        let go = |state, busy, awaiting, stale| next_step(state, &ok(), busy, awaiting, stale);
        assert_eq!(go(WakeState::Off, false, Awaiting::Nothing, true), WakeStep::Listen);
        assert_eq!(go(WakeState::Waiting, false, Awaiting::Nothing, false), WakeStep::Nothing, "already listening");

        // Woken with the request in the same breath: nothing to capture, so the
        // listener goes straight back to waiting for the next invocation.
        assert_eq!(go(WakeState::Off, false, Awaiting::Nothing, false), WakeStep::Listen);

        // Woken by the bare name: the sentence that follows is captured first…
        assert_eq!(go(WakeState::Off, false, Awaiting::Request, false), WakeStep::Capture);
        // …asked for once…
        assert_eq!(go(WakeState::Off, false, Awaiting::Asked, false), WakeStep::Nothing);
        // …and the name listener does not take the microphone back mid-sentence,
        // which would discard the very sentence KUE is waiting for.
        assert_eq!(go(WakeState::Off, true, Awaiting::Asked, true), WakeStep::Nothing);
    }

    #[test]
    fn one_microphone_has_one_user_at_a_time() {
        assert_eq!(next_step(WakeState::Waiting, &ok(), true, Awaiting::Nothing, false), WakeStep::Stop,
            "a listening session takes the microphone from the name listener");
        assert_eq!(next_step(WakeState::Off, &ok(), true, Awaiting::Nothing, false), WakeStep::Nothing);

        // Every condition that forbids listening stops it, and none of them start it.
        for c in [WakeConditions { owner_enabled: false, ..ok() }, WakeConditions { paused: true, ..ok() },
                  WakeConditions { killed: true, ..ok() }, WakeConditions { sensing_up: false, ..ok() },
                  refused()] {
            assert_eq!(next_step(WakeState::Waiting, &c, false, Awaiting::Nothing, false), WakeStep::Stop);
            assert_eq!(next_step(WakeState::Off, &c, false, Awaiting::Request, true), WakeStep::Nothing,
                "nothing here may open a microphone");
        }
    }

    #[test]
    fn a_failure_is_not_retried_at_once_and_is_not_final_either() {
        for state in [WakeState::Unavailable, WakeState::Error] {
            assert_eq!(next_step(state, &ok(), false, Awaiting::Nothing, false), WakeStep::Nothing);
            assert_eq!(next_step(state, &ok(), false, Awaiting::Nothing, true), WakeStep::Listen,
                "a model can finish installing without restarting KUE");
        }
    }

    #[test]
    fn turning_it_on_where_the_microphone_was_never_asked_for_asks() {
        // The defect: the listener waited for a grant that only the listener
        // could ask for, so on a Mac that had never used KUE's microphone the
        // setting did nothing. Access nobody has asked for starts an attempt;
        // macOS puts the question to the owner.
        assert_eq!(next_step(WakeState::Off, &never_asked(), false, Awaiting::Nothing, false), WakeStep::Listen);
        // …and while the question is open, the window does not claim a microphone.
        let starting = WakeReport { state: WakeState::Starting, ..waiting(1.0) };
        assert!(!listening_now(&starting, &never_asked(), 1.0));
        assert!(!listening_now(&waiting(1.0), &never_asked(), 1.0));
        assert_eq!(never_asked().why_not(), Some("Waiting for macOS to allow the microphone."));
    }

    #[test]
    fn a_refusal_by_macos_is_not_asked_again_on_a_timer() {
        // A retry every few seconds would put a denial in the event log every
        // few seconds, and would keep knocking on a door the owner closed.
        for state in [WakeState::Off, WakeState::PermissionDenied] {
            for stale in [false, true] {
                assert_eq!(next_step(state, &refused(), false, Awaiting::Nothing, stale), WakeStep::Nothing);
            }
        }
        // A listener still running when access is withdrawn is stopped.
        assert_eq!(next_step(WakeState::Waiting, &refused(), false, Awaiting::Nothing, false), WakeStep::Stop);
        assert_eq!(refused().why_not(), Some("macOS has not granted microphone access."));
    }

    #[test]
    fn what_the_owner_is_told_names_the_phrase_and_no_score() {
        assert_eq!(WakeState::Waiting.said("computer"), "Listening for “computer”");
        assert_eq!(WakeState::Off.said("computer"), "Not listening for its name");
        for s in [WakeState::Waiting, WakeState::Off, WakeState::PermissionDenied, WakeState::Error] {
            let text = s.said("computer");
            assert!(!text.contains("0."), "{text} reads like a score");
            assert!(!text.contains('_'), "{text} carries a tag");
        }
    }
}
