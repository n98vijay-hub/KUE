//! What the window is allowed to show, decided here.
//!
//! The React window renders this and sends gestures back. It does not decide
//! what a state means, does not compose a sentence about KUE, and cannot show
//! anything this projection did not produce. That is the point: a claim like
//! "Listening…" or "It's open" is a claim about the world, and it has to be
//! derived from what the world actually reported, in one place that can be
//! tested, rather than from a component's idea of what probably happened.
//!
//! The invariants are the tests at the bottom of this file:
//!
//! * Success is never shown without a verified record.
//! * Listening is never shown unless the sensing layer says the microphone is
//!   live, in a report fresh enough to believe.
//! * Acting is never shown unless an action is executing.
//! * Stopped overrides everything, and no sensor reads "on" while stopped.
//! * Unknown stays unknown.
//!
//! Every sentence here is KUE speaking in its own words. No engineering tag
//! (`LEVEL_2`, `PRIVACY_DENIED`, a descriptor distance, a policy version)
//! reaches this projection; those stay in Diagnostics, which reads the context
//! object directly.

use serde::Serialize;

use crate::actions::{ActionRecord, ActionState};
use crate::authz::{AccessBlock, AccessState};
use crate::capabilities::Permission;
use crate::runtime::RuntimeState;
use crate::voice::narration;

/// KUE as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SystemState { Running, Paused, Stopped, Recovering }

/// Who KUE believes is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Presence {
    Recognised,
    /// Recognised, carried through frames where the face could not be measured.
    RecognisedHeld,
    Uncertain,
    SomeoneElse,
    MoreThanOne,
    Nobody,
    Locked,
    NotWatching,
    NotEnrolled,
    CameraBlocked,
    Stopped,
}

/// What KUE is doing this second.
///
/// There is no `VERIFYING`: today verification happens inside the executor, in
/// the same step as execution, so there is no moment where KUE is verifying and
/// not acting. A state with no producer would be a progress indicator for work
/// that is not happening. It joins this enum when a primitive verifies
/// separately from executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Activity {
    Idle,
    Listening,
    Understanding,
    Thinking,
    CheckingAccess,
    WaitingForYou,
    WaitingForMacos,
    Acting,
    Speaking,
    Done,
    UncertainResult,
    Failed,
    Blocked,
    NotFound,
}

/// One trust signal, in a word the owner reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Signal {
    /// ON · OFF · BLOCKED · UNAVAILABLE · UNKNOWN · PAUSED
    pub state: &'static str,
    /// What the owner reads. Never only a colour.
    pub word: String,
}

impl Signal {
    fn new(state: &'static str, word: &str) -> Self { Signal { state, word: word.into() } }
}

/// What KUE can sense and where anything goes. Always visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trust {
    pub camera: Signal,
    pub microphone: Signal,
    pub computer: Signal,
    pub external_ai: Signal,
    pub memory: Signal,
    pub kue: Signal,
}

/// One sentence about what KUE understands right now, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextLine {
    pub sentence: String,
    /// OBSERVED — measured this moment. INFERRED — worked out from what was
    /// measured, and labelled as such rather than stated as fact.
    pub basis: &'static str,
}

/// Something that needs the owner, with the control that answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attention {
    /// CONFIRM · AUTHENTICATE · PERMISSION · ENROLL · RECOVER
    pub kind: &'static str,
    pub sentence: String,
    /// The action this concerns, when it concerns one.
    pub action_id: Option<String>,
    /// For a PERMISSION item: which macOS grant it is about, so the window can
    /// offer the control that opens the right pane. The window used to decide
    /// that by looking for the word "Camera" in the sentence, which meant
    /// rewording the sentence silently removed the button — and the microphone
    /// never got one at all.
    pub permission: Option<Permission>,
}

/// Everything the window may show, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Surface {
    pub system: SystemState,
    pub system_sentence: String,
    pub presence: Presence,
    pub presence_sentence: String,
    pub presence_detail: Option<String>,
    pub activity: Activity,
    pub activity_sentence: Option<String>,
    pub activity_action_id: Option<String>,
    /// True only for a SUCCEEDED record that recorded what it verified. The
    /// window may show a verified mark only when this is true.
    pub activity_verified: bool,
    pub trust: Trust,
    /// Three to five plain sentences. Never a measurement: head-pose angles,
    /// capture-quality scores and face counts are Diagnostics, not context.
    pub context: Vec<ContextLine>,
    pub attention: Vec<Attention>,
}

/// What the projection is derived from. Assembled by the shell from the engine,
/// the action book and the speech controller, and by tests from literals — so
/// every state below can be produced deliberately and checked.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    pub runtime: RuntimeState,
    pub paused: bool,
    pub access: &'a AccessBlock,
    pub sensing_up: bool,
    /// The sensing layer's camera state: RUNNING, STOPPED, NO_CAMERA, …
    pub camera_state: &'a str,
    /// AUTHORIZED · DENIED · RESTRICTED · NOT_DETERMINED
    pub camera_permission: &'a str,
    pub enrolled_samples: u32,
    pub required_samples: u32,
    /// The sensing layer's microphone state: IDLE, STARTING, LISTENING, FINISHING.
    pub microphone_state: &'a str,
    /// Whether that report is fresh enough to believe — `voice::policy::microphone_busy`
    /// decides this from the same clock the speech gate uses.
    pub microphone_live: bool,
    pub microphone_permission: Option<&'a str>,
    /// Whether the microphone is open waiting for KUE's name, from
    /// `voice::wake::listening_now` — the same freshness judgement, for the
    /// same reason. The window must never say this from a setting: a setting
    /// says what was asked for, not what is running.
    pub waiting_for_name: bool,
    /// The invocation, for saying what KUE is listening for rather than making
    /// the owner remember.
    pub wake_phrase: &'a str,
    /// Whether KUE is speaking, from the speech controller.
    pub speaking: bool,
    /// Whether the sensing layer is sampling which app is in front.
    pub computer_sampling: Option<bool>,
    pub memory_writable: bool,
    /// Seconds the on-device model has been working on the current question.
    pub thinking_seconds: Option<f64>,
    pub records: &'a [ActionRecord],
    /// What an open goal is waiting for the owner to do, from the goal's own
    /// state (`TaskView::waiting`), or None. Only from the action list the
    /// owner may see: withheld lists give none.
    pub goal_waiting: Option<&'a str>,
    /// The application in front, if the sensing layer reported one.
    pub frontmost_app: Option<&'a str>,
    /// Seconds since the last keyboard or trackpad input. Never what was typed.
    pub idle_seconds: Option<f64>,
    /// What KUE worked out you are doing, in its own words.
    pub activity_inference: Option<&'a str>,
    pub people_in_view: usize,
    pub now: f64,
}

/// How long a finished action stays on the activity line before KUE goes quiet.
pub const OUTCOME_SECONDS: f64 = 12.0;

pub fn project(i: &Inputs) -> Surface {
    let system = match i.runtime {
        RuntimeState::Killed => SystemState::Stopped,
        RuntimeState::Recovering => SystemState::Recovering,
        _ if i.paused => SystemState::Paused,
        RuntimeState::Paused => SystemState::Paused,
        RuntimeState::Running => SystemState::Running,
    };
    let stopped = matches!(system, SystemState::Stopped);

    let (presence, presence_sentence, presence_detail) = presence(i, system);
    let (activity, activity_sentence, activity_action_id, activity_verified) = activity(i, system);

    Surface {
        system,
        system_sentence: match system {
            SystemState::Running => "KUE is running.".into(),
            SystemState::Paused => "KUE is paused. Nothing is being sensed.".into(),
            SystemState::Stopped => "KUE is stopped.".into(),
            SystemState::Recovering => "KUE is being started again.".into(),
        },
        presence, presence_sentence, presence_detail,
        activity, activity_sentence, activity_action_id, activity_verified,
        trust: trust(i, system),
        context: if stopped { Vec::new() } else { context(i) },
        attention: if stopped { Vec::new() } else { attention(i) },
    }
}

fn presence(i: &Inputs, system: SystemState) -> (Presence, String, Option<String>) {
    use Presence::*;
    let say = |p: Presence, s: &str| (p, s.to_string(), None);

    if system == SystemState::Stopped {
        return say(Stopped, "I'm stopped.");
    }
    if matches!(i.camera_permission, "DENIED" | "RESTRICTED") {
        return (CameraBlocked, "macOS is blocking the camera.".into(),
            Some("You can grant it in System Settings → Privacy & Security → Camera.".into()));
    }
    if system == SystemState::Paused || !i.sensing_up || i.camera_state != "RUNNING" {
        return say(NotWatching, "I'm not watching — the camera is off.");
    }
    if i.enrolled_samples < i.required_samples {
        return (NotEnrolled, "I don't know your face yet.".into(),
            Some(format!("I have {} of the {} samples I need.", i.enrolled_samples, i.required_samples)));
    }
    match i.access.state {
        AccessState::Locked | AccessState::AuthenticationRequired =>
            (Locked, "Locked.".into(), Some("Show your face, or use Touch ID.".into())),
        AccessState::MultiplePeople => say(MoreThanOne, "There's more than one person here."),
        AccessState::UnknownPerson => say(SomeoneElse, "Someone I don't know is here."),
        AccessState::IdentityUncertain | AccessState::AuthorizedUserLowConfidence =>
            say(Uncertain, "I'm not certain it's you."),
        AccessState::NoPerson => say(Nobody, "Nobody is in view."),
        AccessState::AuthorizedUser => match i.access.held_without_measurement_seconds {
            Some(_) => (RecognisedHeld, "I recognise you.".into(),
                Some("I can't see your face clearly this moment.".into())),
            None => say(Recognised, "I recognise you."),
        },
    }
}

fn activity(i: &Inputs, system: SystemState) -> (Activity, Option<String>, Option<String>, bool) {
    use Activity::*;
    if system == SystemState::Stopped {
        return (Idle, None, None, false);
    }
    // The microphone first: KUE does not talk over you, and does not claim to
    // be listening on anything but a live report from the sensing layer.
    if i.microphone_live {
        return (Listening, Some("Listening…".into()), None, false);
    }
    if i.microphone_state == "FINISHING" && i.sensing_up {
        return (Understanding, Some("Working out what you said…".into()), None, false);
    }

    // A goal waiting for the owner outranks a finished action's line, but not
    // an action in progress or one waiting for the owner itself.
    let in_progress = crate::actions::current(i.records, i.now, OUTCOME_SECONDS).is_some_and(|r| matches!(r.state,
        ActionState::Reauthorizing | ActionState::RequiresConfirmation | ActionState::RequiresStrongAuth | ActionState::Executing));
    if let (Some(waiting), false) = (i.goal_waiting, in_progress) {
        return (WaitingForYou, Some(waiting.to_string()), None, false);
    }
    if let Some(r) = crate::actions::current(i.records, i.now, OUTCOME_SECONDS) {
        let line = narration::describe(r);
        let id = Some(r.id.clone());
        let state = match r.state {
            ActionState::Reauthorizing => Some((CheckingAccess, Some("Checking it's still you.".into()))),
            ActionState::RequiresConfirmation => Some((WaitingForYou, line.clone())),
            ActionState::RequiresStrongAuth => Some((WaitingForMacos, line.clone())),
            ActionState::Executing => Some((Acting, line.clone())),
            // Verified success is the only success. A SUCCEEDED record with no
            // recorded verification is reported as an unconfirmed outcome — the
            // executor is supposed to make that impossible, and if it ever
            // happens the window must not turn it into a claim.
            ActionState::Succeeded => Some(match r.verification {
                Some(_) => (Done, line.clone()),
                None => (UncertainResult, Some("I can't confirm that finished.".into())),
            }),
            ActionState::UnknownResult => Some((UncertainResult, line.clone()
                .or_else(|| Some("I can't confirm that finished.".into())))),
            ActionState::Failed | ActionState::PartiallySucceeded => Some((Failed, line.clone())),
            ActionState::Denied | ActionState::AuthorizationExpired | ActionState::PrivacyDenied =>
                Some((Blocked, line.clone())),
            ActionState::NoMatches => Some((NotFound, line.clone()
                .or_else(|| Some("I couldn't find anything matching that.".into())))),
            ActionState::Proposed | ActionState::Authorized | ActionState::Cancelled => None,
        };
        if let Some((a, sentence)) = state {
            let verified = a == Done && r.verification.is_some();
            return (a, sentence, id, verified);
        }
    }

    if let Some(s) = i.thinking_seconds {
        return (Thinking, Some(format!("Thinking on this Mac… {s:.0}s")), None, false);
    }
    if i.speaking {
        return (Speaking, Some("Speaking.".into()), None, false);
    }
    (Idle, None, None, false)
}

fn trust(i: &Inputs, system: SystemState) -> Trust {
    let stopped = matches!(system, SystemState::Stopped);
    let paused = matches!(system, SystemState::Paused);

    // Nothing reads "on" while KUE is stopped, whatever a stale report says.
    if stopped {
        return Trust {
            camera: Signal::new("OFF", "Camera off"),
            microphone: Signal::new("OFF", "Microphone off"),
            computer: Signal::new("OFF", "Not watching this Mac"),
            external_ai: Signal::new("OFF", "Nothing leaves this Mac"),
            memory: Signal::new("OFF", "Not keeping anything"),
            kue: Signal::new("OFF", "KUE is stopped"),
        };
    }

    let camera = match (paused, i.sensing_up, i.camera_permission, i.camera_state) {
        (true, ..) => Signal::new("PAUSED", "Camera paused"),
        (_, _, "DENIED" | "RESTRICTED", _) => Signal::new("BLOCKED", "Camera blocked by macOS"),
        (_, _, "NOT_DETERMINED", _) => Signal::new("UNKNOWN", "Camera not yet allowed"),
        (_, false, ..) => Signal::new("UNAVAILABLE", "Camera not running"),
        (_, true, _, "RUNNING") => Signal::new("ON", "Camera on"),
        (_, true, _, _) => Signal::new("UNAVAILABLE", "Camera not running"),
    };

    let microphone = match (paused, i.sensing_up, i.microphone_permission, i.microphone_live) {
        (true, ..) => Signal::new("PAUSED", "Microphone paused"),
        (_, _, Some("DENIED" | "RESTRICTED"), _) => Signal::new("BLOCKED", "Microphone blocked by macOS"),
        (_, false, ..) => Signal::new("UNAVAILABLE", "Microphone unavailable"),
        (_, true, None, _) => Signal::new("UNKNOWN", "Microphone not yet checked"),
        (_, true, _, true) => Signal::new("ON", "Listening"),
        // Open, waiting for its name. Said plainly and with the phrase in it,
        // because a microphone that is on for a reason the owner cannot see is
        // exactly what this must never be.
        (_, true, _, false) if i.waiting_for_name =>
            Signal::new("ON", &format!("Listening for “{}”", i.wake_phrase)),
        (_, true, _, false) => Signal::new("OFF", "Microphone off"),
    };

    let computer = match (paused, i.sensing_up, i.computer_sampling) {
        (true, ..) => Signal::new("PAUSED", "Not watching this Mac"),
        (_, false, _) => Signal::new("UNAVAILABLE", "Not watching this Mac"),
        (_, true, Some(true)) => Signal::new("ON", "Sees which app is in front"),
        (_, true, Some(false)) => Signal::new("OFF", "Not watching this Mac"),
        (_, true, None) => Signal::new("UNKNOWN", "Not yet reported"),
    };

    Trust {
        camera, microphone, computer,
        // Policy v1 permits no data kind to reach an external model, so this is
        // a fact about the policy rather than a setting.
        external_ai: Signal::new("OFF", "Nothing leaves this Mac"),
        memory: if i.memory_writable { Signal::new("ON", "Keeping what it works out") }
                else { Signal::new("UNAVAILABLE", "Not keeping anything") },
        kue: if paused { Signal::new("PAUSED", "KUE is paused") } else { Signal::new("ON", "KUE is running") },
    }
}

/// What KUE understands, in sentences a person reads. Each is labelled with
/// whether it was measured or worked out, because the difference is the whole
/// point: KUE states what it saw, and marks what it concluded.
fn context(i: &Inputs) -> Vec<ContextLine> {
    let mut out = Vec::new();
    let seen = |s: String| ContextLine { sentence: s, basis: "OBSERVED" };
    let worked_out = |s: String| ContextLine { sentence: s, basis: "INFERRED" };

    if i.paused || !i.sensing_up {
        out.push(seen("I'm not sensing anything right now.".into()));
        return out;
    }
    out.push(seen(match i.people_in_view {
        0 => "Nobody is in the camera's view.".into(),
        1 => "One person is in the camera's view.".into(),
        n => format!("{n} people are in the camera's view."),
    }));
    if let Some(app) = i.frontmost_app {
        out.push(seen(format!("{app} is the application in front. I can't see inside it.")));
    }
    if let Some(idle) = i.idle_seconds {
        out.push(seen(if idle < 5.0 {
            "You used the keyboard or trackpad a moment ago.".into()
        } else if idle < 120.0 {
            format!("Nothing has been typed or clicked for {idle:.0} seconds.")
        } else {
            format!("Nothing has been typed or clicked for {:.0} minutes.", idle / 60.0)
        }));
    }
    if let Some(a) = i.activity_inference {
        out.push(worked_out(format!("{a}.")));
    }
    out.truncate(5);
    out
}

fn attention(i: &Inputs) -> Vec<Attention> {
    let mut out = Vec::new();
    for r in i.records.iter().rev() {
        match r.state {
            ActionState::RequiresConfirmation => out.push(Attention {
                kind: "CONFIRM",
                sentence: narration::describe(r).unwrap_or_else(|| "I need your OK.".into()),
                action_id: Some(r.id.clone()), permission: None }),
            ActionState::RequiresStrongAuth => out.push(Attention {
                kind: "AUTHENTICATE",
                sentence: narration::describe(r).unwrap_or_else(|| "macOS needs to check it's you.".into()),
                action_id: Some(r.id.clone()), permission: None }),
            _ => {}
        }
    }
    if matches!(i.camera_permission, "DENIED" | "RESTRICTED") {
        out.push(Attention { kind: "PERMISSION",
            sentence: "I need camera access, and only you can grant it — System Settings → Privacy & Security → Camera.".into(),
            action_id: None, permission: Some(Permission::Camera) });
    }
    if matches!(i.microphone_permission, Some("DENIED" | "RESTRICTED")) {
        out.push(Attention { kind: "PERMISSION",
            sentence: "I need microphone access to hear you — System Settings → Privacy & Security → Microphone.".into(),
            action_id: None, permission: Some(Permission::Microphone) });
    }
    if i.sensing_up && i.enrolled_samples < i.required_samples {
        out.push(Attention { kind: "ENROLL",
            sentence: "I don't know your face yet. Let me learn it, and I'll know it's you.".into(),
            action_id: None, permission: None });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{ActionKind, Risk};
    use crate::authz::{AuthLevel, IdentityBasis, SessionPhase};

    fn access(state: AccessState) -> AccessBlock {
        AccessBlock {
            state,
            level: if state == AccessState::AuthorizedUser { AuthLevel::Level2 } else { AuthLevel::Level0 },
            phase: match state {
                AccessState::Locked | AccessState::AuthenticationRequired => SessionPhase::Locked,
                AccessState::AuthorizedUser => SessionPhase::OwnerPresent,
                _ => SessionPhase::OwnerLeft,
            },
            detail: String::new(),
            owner_last_confirmed_seconds_ago: None,
            os_auth: None,
            os_auth_expires_in_seconds: None,
            lock_reason: None,
            basis: IdentityBasis::MeasuredMatch,
            held_without_measurement_seconds: None,
            requirements: Vec::new(),
        }
    }

    fn record(state: ActionState, verification: Option<&str>) -> ActionRecord {
        ActionRecord {
            id: "a1".into(),
            source: "test".into(),
            action: ActionKind::OpenApplication { name: "Google Chrome".into() },
            description: "Open Google Chrome".into(),
            risk: Risk::Low,
            state,
            reason: None,
            verification: verification.map(Into::into),
            output: None,
            created_at: 0.0,
            updated_at: Some(0.0),
            choices: Vec::new(),
            steps: Vec::new(),
            awaiting_since: None,
            task: None,
            sentences: None,
        }
    }

    /// A Mac where everything is working and the owner is there.
    fn good<'a>(access: &'a AccessBlock, records: &'a [ActionRecord]) -> Inputs<'a> {
        Inputs {
            runtime: RuntimeState::Running,
            paused: false,
            access,
            sensing_up: true,
            camera_state: "RUNNING",
            camera_permission: "AUTHORIZED",
            enrolled_samples: 5,
            required_samples: 5,
            microphone_state: "IDLE",
            microphone_live: false,
            microphone_permission: Some("AUTHORIZED"),
            waiting_for_name: false,
            wake_phrase: "computer",
            speaking: false,
            computer_sampling: Some(true),
            memory_writable: true,
            thinking_seconds: None,
            records,
            goal_waiting: None,
            frontmost_app: Some("Safari"),
            idle_seconds: Some(1.0),
            activity_inference: Some("At the computer, interacting with it"),
            people_in_view: 1,
            now: 1.0,
        }
    }

    #[test]
    fn a_goal_waiting_for_the_owner_is_shown_from_the_goal_and_only_when_nothing_is_in_progress() {
        let a = access(AccessState::AuthorizedUser);
        let done = [record(ActionState::Succeeded, Some("read 12 items"))];
        let mut i = good(&a, &done);
        i.goal_waiting = Some("Choose what to move to the Trash. Nothing moves until you do.");
        let s = project(&i);
        assert_eq!(s.activity, Activity::WaitingForYou);
        assert_eq!(s.activity_sentence.as_deref(), i.goal_waiting);
        assert!(!s.activity_verified, "waiting is not a verified outcome");

        let running = [record(ActionState::Executing, None)];
        let mut i = good(&a, &running);
        i.goal_waiting = Some("Choose what to move to the Trash. Nothing moves until you do.");
        assert_eq!(project(&i).activity, Activity::Acting, "work in progress is not hidden behind a wait");

        // No goal waiting: nothing is invented.
        assert_eq!(project(&good(&a, &done)).activity, Activity::Done);
        // Stopped overrides everything.
        let mut i = good(&a, &done);
        i.goal_waiting = Some("x");
        i.runtime = RuntimeState::Killed;
        assert_eq!(project(&i).activity, Activity::Idle);
    }

    #[test]
    fn the_owner_in_view_is_recognised_and_nothing_else_is_claimed() {
        let a = access(AccessState::AuthorizedUser);
        let s = project(&good(&a, &[]));
        assert_eq!(s.presence, Presence::Recognised);
        assert_eq!(s.presence_sentence, "I recognise you.");
        assert_eq!(s.activity, Activity::Idle);
        assert_eq!(s.activity_sentence, None);
        assert!(s.attention.is_empty());
    }

    #[test]
    fn uncertainty_is_stated_rather_than_resolved_in_kues_favour() {
        for (state, expected) in [
            (AccessState::IdentityUncertain, Presence::Uncertain),
            (AccessState::UnknownPerson, Presence::SomeoneElse),
            (AccessState::MultiplePeople, Presence::MoreThanOne),
            (AccessState::NoPerson, Presence::Nobody),
            (AccessState::Locked, Presence::Locked),
        ] {
            let a = access(state);
            let s = project(&good(&a, &[]));
            assert_eq!(s.presence, expected, "{state:?}");
            assert!(!s.presence_sentence.contains("recognise"), "{state:?} claimed recognition");
        }
    }

    /// The invariant with the most ways to go wrong: a window that says "done"
    /// because a state name looked final.
    #[test]
    fn success_is_never_shown_without_a_verified_record() {
        let a = access(AccessState::AuthorizedUser);

        let verified = [record(ActionState::Succeeded, Some("Google Chrome is running."))];
        let s = project(&good(&a, &verified));
        assert_eq!(s.activity, Activity::Done);
        assert!(s.activity_verified);

        // The same state with nothing recorded as verified is not success.
        let unverified = [record(ActionState::Succeeded, None)];
        let s = project(&good(&a, &unverified));
        assert_eq!(s.activity, Activity::UncertainResult);
        assert!(!s.activity_verified);
        assert_eq!(s.activity_sentence.unwrap(), "I can't confirm that finished.");

        for state in [ActionState::Executing, ActionState::Failed, ActionState::UnknownResult,
                      ActionState::Denied, ActionState::PrivacyDenied, ActionState::NoMatches,
                      ActionState::AuthorizationExpired, ActionState::RequiresConfirmation] {
            let r = [record(state, Some("this must not be believed"))];
            let s = project(&good(&a, &r));
            assert!(!s.activity_verified, "{state:?} reported as verified");
            assert_ne!(s.activity, Activity::Done, "{state:?} reported as done");
        }
    }

    #[test]
    fn an_open_microphone_waiting_for_its_name_always_says_so() {
        // The one thing a person must never have to discover: that KUE's
        // microphone is open. It is on the always-visible strip, with the word
        // being listened for, and it comes from the live report — not from the
        // setting, which says what was asked for rather than what is running.
        let a = access(AccessState::AuthorizedUser);

        let mut waiting = good(&a, &[]);
        waiting.waiting_for_name = true;
        waiting.wake_phrase = "computer";
        let s = project(&waiting);
        assert_eq!(s.trust.microphone.state, "ON");
        assert_eq!(s.trust.microphone.word, "Listening for “computer”");
        assert_eq!(s.activity, Activity::Idle, "waiting for a name is not the same as listening to you");

        // Not listening: the strip says the microphone is off, and means it.
        let mut off = good(&a, &[]);
        off.waiting_for_name = false;
        assert_eq!(project(&off).trust.microphone.word, "Microphone off");

        // Stopped or paused wins over any report at all.
        let mut stopped = good(&a, &[]);
        stopped.waiting_for_name = true;
        stopped.runtime = RuntimeState::Killed;
        assert_eq!(project(&stopped).trust.microphone.state, "OFF");
        let mut paused = good(&a, &[]);
        paused.waiting_for_name = true;
        paused.paused = true;
        assert_eq!(project(&paused).trust.microphone.state, "PAUSED");

        // And an actual listening session outranks it: KUE is listening to you,
        // not for its name.
        let mut both = good(&a, &[]);
        both.waiting_for_name = true;
        both.microphone_state = "LISTENING";
        both.microphone_live = true;
        assert_eq!(project(&both).trust.microphone.word, "Listening");
    }

    #[test]
    fn listening_is_never_shown_unless_the_microphone_is_actually_live() {
        let a = access(AccessState::AuthorizedUser);

        let mut live = good(&a, &[]);
        live.microphone_state = "LISTENING";
        live.microphone_live = true;
        assert_eq!(project(&live).activity, Activity::Listening);
        assert_eq!(project(&live).trust.microphone.state, "ON");

        // A LISTENING the sensing layer has not refreshed is not believed.
        let mut stale = good(&a, &[]);
        stale.microphone_state = "LISTENING";
        stale.microphone_live = false;
        assert_eq!(project(&stale).activity, Activity::Idle);
        assert_eq!(project(&stale).trust.microphone.state, "OFF");

        // Nor is one from a sensing layer that is gone.
        let mut gone = good(&a, &[]);
        gone.microphone_state = "LISTENING";
        gone.microphone_live = false;
        gone.sensing_up = false;
        assert_eq!(project(&gone).activity, Activity::Idle);
        assert_eq!(project(&gone).trust.microphone.state, "UNAVAILABLE");
    }

    #[test]
    fn acting_is_never_shown_unless_an_action_is_executing() {
        let a = access(AccessState::AuthorizedUser);
        let executing = [record(ActionState::Executing, None)];
        assert_eq!(project(&good(&a, &executing)).activity, Activity::Acting);

        for state in [ActionState::Proposed, ActionState::Authorized, ActionState::RequiresConfirmation,
                      ActionState::Reauthorizing, ActionState::Succeeded, ActionState::Failed] {
            let r = [record(state, Some("v"))];
            assert_ne!(project(&good(&a, &r)).activity, Activity::Acting, "{state:?}");
        }
    }

    #[test]
    fn stopped_overrides_everything_and_no_sensor_reads_on() {
        let a = access(AccessState::AuthorizedUser);
        let busy = [record(ActionState::Executing, None)];
        let mut i = good(&a, &busy);
        i.runtime = RuntimeState::Killed;
        i.microphone_live = true;
        i.speaking = true;
        i.thinking_seconds = Some(3.0);

        let s = project(&i);
        assert_eq!(s.system, SystemState::Stopped);
        assert_eq!(s.presence, Presence::Stopped);
        assert_eq!(s.activity, Activity::Idle);
        assert!(s.attention.is_empty());
        for sig in [&s.trust.camera, &s.trust.microphone, &s.trust.computer,
                    &s.trust.memory, &s.trust.kue, &s.trust.external_ai] {
            assert_eq!(sig.state, "OFF", "{sig:?} still reads on while KUE is stopped");
        }
    }

    #[test]
    fn paused_says_so_rather_than_pretending_to_watch() {
        let a = access(AccessState::AuthorizedUser);
        let mut i = good(&a, &[]);
        i.paused = true;
        let s = project(&i);
        assert_eq!(s.system, SystemState::Paused);
        assert_eq!(s.presence, Presence::NotWatching);
        assert_eq!(s.trust.camera.state, "PAUSED");
        assert_eq!(s.trust.microphone.state, "PAUSED");
        assert_eq!(s.trust.computer.state, "PAUSED");
    }

    #[test]
    fn an_unchecked_permission_reads_unknown_not_off() {
        let a = access(AccessState::AuthorizedUser);
        let mut i = good(&a, &[]);
        i.microphone_permission = None;
        i.computer_sampling = None;
        let s = project(&i);
        assert_eq!(s.trust.microphone.state, "UNKNOWN");
        assert_eq!(s.trust.computer.state, "UNKNOWN");

        let mut i = good(&a, &[]);
        i.camera_permission = "NOT_DETERMINED";
        assert_eq!(project(&i).trust.camera.state, "UNKNOWN");
    }

    #[test]
    fn a_blocked_permission_is_named_with_the_pane_the_owner_must_open() {
        let a = access(AccessState::AuthorizedUser);
        let mut i = good(&a, &[]);
        i.camera_permission = "DENIED";
        let s = project(&i);
        assert_eq!(s.presence, Presence::CameraBlocked);
        assert_eq!(s.trust.camera.state, "BLOCKED");
        let p = s.attention.iter().find(|a| a.kind == "PERMISSION").expect("no way to fix it");
        assert!(p.sentence.contains("System Settings"));
        assert!(p.sentence.contains("only you can grant"));
        // Which grant it is, is a field — not something the window reads out of
        // the sentence. Rewording the sentence must not remove the button.
        assert_eq!(p.permission, Some(crate::capabilities::Permission::Camera));

        let mut i = good(&a, &[]);
        i.microphone_permission = Some("DENIED");
        let s = project(&i);
        let m = s.attention.iter().find(|a| a.permission == Some(crate::capabilities::Permission::Microphone))
            .expect("a denied microphone offers no way to fix it");
        assert!(m.sentence.contains("System Settings"));
    }

    #[test]
    fn a_waiting_action_is_the_one_thing_asking_for_the_owner() {
        let a = access(AccessState::AuthorizedUser);
        let waiting = [record(ActionState::RequiresConfirmation, None)];
        let s = project(&good(&a, &waiting));
        assert_eq!(s.activity, Activity::WaitingForYou);
        assert_eq!(s.activity_action_id.as_deref(), Some("a1"));
        assert_eq!(s.attention.len(), 1);
        assert_eq!(s.attention[0].kind, "CONFIRM");
        assert_eq!(s.attention[0].action_id.as_deref(), Some("a1"));
    }

    #[test]
    fn an_action_in_progress_outranks_a_newer_finished_one() {
        let a = access(AccessState::AuthorizedUser);
        let mut done = record(ActionState::Succeeded, Some("done"));
        done.id = "a2".into();
        done.created_at = 0.9;
        let running = record(ActionState::Executing, None);
        let s = project(&good(&a, &[running, done]));
        assert_eq!(s.activity, Activity::Acting);
        assert_eq!(s.activity_action_id.as_deref(), Some("a1"));
    }

    #[test]
    fn an_outcome_stops_being_the_headline_once_it_is_old() {
        let a = access(AccessState::AuthorizedUser);
        let mut stale = record(ActionState::Succeeded, Some("Google Chrome is running."));
        stale.updated_at = Some(0.0);
        let mut i = good(&a, std::slice::from_ref(&stale));
        i.now = OUTCOME_SECONDS + 1.0;
        assert_eq!(project(&i).activity, Activity::Idle);
    }

    /// The case that separates "this outcome is stale" from "this action was
    /// slow". A document search, a Touch ID wait or a step of a task can easily
    /// take longer than the outcome window; measuring age from when it was
    /// proposed would throw its result away the moment it arrived.
    #[test]
    fn a_slow_action_still_gets_to_report_what_it_did() {
        let a = access(AccessState::AuthorizedUser);
        let mut slow = record(ActionState::Succeeded, Some("Google Chrome is running."));
        slow.created_at = 0.0;                          // proposed a minute ago
        slow.updated_at = Some(60.0);                   // finished a second ago
        let mut i = good(&a, std::slice::from_ref(&slow));
        i.now = 61.0;

        let s = project(&i);
        assert_eq!(s.activity, Activity::Done, "a slow action's result was discarded as stale");
        assert!(s.activity_verified);
    }

    #[test]
    fn not_being_enrolled_is_said_plainly_and_offered_a_way_out() {
        let a = access(AccessState::NoPerson);
        let mut i = good(&a, &[]);
        i.enrolled_samples = 0;
        let s = project(&i);
        assert_eq!(s.presence, Presence::NotEnrolled);
        assert!(s.presence_detail.unwrap().contains("0 of the 5"));
        assert!(s.attention.iter().any(|a| a.kind == "ENROLL"));
    }

    /// Both the spoken status and this projection reach the same window. If
    /// they picked the current action by different rules they could name two
    /// different actions on one screen — the disagreement a single projection
    /// exists to prevent.
    #[test]
    fn the_spoken_status_and_the_window_are_always_about_the_same_action() {
        let a = access(AccessState::AuthorizedUser);
        let mut older = record(ActionState::Succeeded, Some("Google Chrome is running."));
        older.id = "a1".into();
        older.updated_at = Some(0.5);
        for (id, state) in [("a2", ActionState::Executing), ("a2", ActionState::RequiresConfirmation),
                            ("a2", ActionState::Failed), ("a2", ActionState::Succeeded)] {
            let mut newer = record(state, Some("Google Chrome is running."));
            newer.id = id.into();
            newer.updated_at = Some(1.0);
            let records = [older.clone(), newer];

            let s = project(&good(&a, &records));
            let spoken = narration::live_status(&records, 1.0);
            assert_eq!(s.activity_action_id, spoken.map(|l| l.action_id),
                "{state:?}: the window and the spoken line disagree about which action is current");
        }
    }

    /// The context column is what KUE understands, not what it measured. The
    /// window used to show the observation list straight from the context
    /// object — "Head pose — yaw -20°, pitch 9°, roll 6°; capture quality 0.19"
    /// and "1 face(s) detected" — which is Diagnostics wearing a friendly label.
    #[test]
    fn context_is_sentences_a_person_reads_never_measurements() {
        let a = access(AccessState::AuthorizedUser);
        let s = project(&good(&a, &[]));
        assert!(!s.context.is_empty() && s.context.len() <= 5);
        for line in &s.context {
            assert!(matches!(line.basis, "OBSERVED" | "INFERRED"), "{line:?}");
            for measurement in ["yaw", "pitch", "roll", "capture quality", "face(s)", "0.", "°"] {
                assert!(!line.sentence.contains(measurement),
                    "a measurement reached the context column: {:?}", line.sentence);
            }
        }
        // What was worked out is labelled as worked out, not stated as seen.
        let worked = s.context.iter().find(|l| l.basis == "INFERRED").expect("no inference labelled");
        assert!(worked.sentence.starts_with("At the computer"));

        // Paused says so instead of listing what it can no longer see.
        let mut i = good(&a, &[]);
        i.paused = true;
        let s = project(&i);
        assert_eq!(s.context.len(), 1);
        assert_eq!(s.context[0].sentence, "I'm not sensing anything right now.");
    }

    #[test]
    fn no_sentence_the_window_shows_carries_an_engineering_tag() {
        let a = access(AccessState::AuthorizedUser);
        let states = [ActionState::Executing, ActionState::Succeeded, ActionState::Failed,
                      ActionState::Denied, ActionState::PrivacyDenied, ActionState::NoMatches,
                      ActionState::UnknownResult, ActionState::RequiresConfirmation,
                      ActionState::RequiresStrongAuth, ActionState::Reauthorizing,
                      ActionState::AuthorizationExpired];
        let leaks = ["LEVEL_", "PRIVACY_DENIED", "UNKNOWN_RESULT", "AUTHORIZATION_EXPIRED",
                     "REQUIRES_", "ActionState", "_UNAVAILABLE", "policy v", "descriptor"];
        for state in states {
            let r = [record(state, Some("Google Chrome is running."))];
            let s = project(&good(&a, &r));
            let mut text = vec![s.system_sentence, s.presence_sentence];
            text.extend(s.presence_detail);
            text.extend(s.activity_sentence);
            text.extend(s.attention.into_iter().map(|a| a.sentence));
            for t in text {
                for leak in leaks {
                    assert!(!t.contains(leak), "{state:?} showed {leak:?} to a person: {t:?}");
                }
            }
        }
    }
}
