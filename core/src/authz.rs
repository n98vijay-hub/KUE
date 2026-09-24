//! KUE authorization: who may do what, right now.
//!
//! Identity and authorization are separate. The engine says what the camera
//! concluded about identity; this module turns that — plus any OS
//! authentication macOS has confirmed — into an authorization LEVEL, and turns
//! a requested OPERATION into a decision.
//!
//!   LEVEL_0  public / non-sensitive            anyone
//!   LEVEL_1  owner presence                    the confirmed owner was seen moments ago
//!   LEVEL_2  owner identity confirmed          MY_FACE_CONFIRMED, measured now
//!   LEVEL_3  strong OS authentication          Touch ID or the login password, via macOS
//!   LEVEL_4  explicit physical confirmation    a finger on Touch ID for THIS operation
//!
//! Rules that are not negotiable here:
//!  * unknown operation = DENY; unknown authorization = DENY;
//!  * an UNKNOWN_PERSON or MULTIPLE_PEOPLE locks the session and revokes any OS
//!    authentication — privileges are not inherited by whoever sits down;
//!  * conflicting or uncertain identity evidence denies owner operations, and OS
//!    authentication does not override it;
//!  * a model or an automation cannot perform owner-gesture operations and
//!    cannot create a grant: only a macOS authentication result can;
//!  * LEVEL_4 and fresh LEVEL_3 grants are single-use and bound to one operation.

use crate::context::IdentityState;
use crate::runtime::Principal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AuthLevel {
    #[serde(rename = "LEVEL_0")] Level0,
    #[serde(rename = "LEVEL_1")] Level1,
    #[serde(rename = "LEVEL_2")] Level2,
    #[serde(rename = "LEVEL_3")] Level3,
    #[serde(rename = "LEVEL_4")] Level4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccessState {
    NoPerson,
    UnknownPerson,
    IdentityUncertain,
    AuthorizedUser,
    AuthorizedUserLowConfidence,
    MultiplePeople,
    AuthenticationRequired,
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionPhase {
    OwnerPresent,
    OwnerLeft,
    Locked,
    /// The camera is not observing, so presence and departure cannot be seen.
    Unobserved,
}

/// What macOS confirmed. KUE never sees the fingerprint or the password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OsAuthKind {
    /// Touch ID or the login password (LAPolicy.deviceOwnerAuthentication).
    Strong,
    /// Touch ID only (LAPolicy.deviceOwnerAuthenticationWithBiometrics).
    Physical,
}

/// Everything that can be authorized. Exhaustive: adding an operation forces
/// a requirement to be written for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Operation {
    Kill,
    Pause,
    Resume,
    Listen,
    RestartSensing,
    ProbeCapture,
    ProbeReset,
    ProbeReport,
    AskModelGeneral,
    AskModelWithPersonalContext,
    EnrollmentCapture,
    EnrollmentUndo,
    EnrollmentReset,
    EraseMemory,
    PurgeLegacySnapshots,
    RecoverFromKill,
    ActionLowRisk,
    ActionMediumRisk,
    ActionHighRisk,
    ActionCriticalRisk,
}

impl Operation {
    pub const ALL: [Operation; 20] = [
        Operation::Kill, Operation::Pause, Operation::Resume, Operation::Listen, Operation::RestartSensing,
        Operation::ProbeCapture, Operation::ProbeReset, Operation::ProbeReport,
        Operation::AskModelGeneral, Operation::AskModelWithPersonalContext,
        Operation::EnrollmentCapture, Operation::EnrollmentUndo, Operation::EnrollmentReset,
        Operation::EraseMemory, Operation::PurgeLegacySnapshots, Operation::RecoverFromKill,
        Operation::ActionLowRisk, Operation::ActionMediumRisk, Operation::ActionHighRisk, Operation::ActionCriticalRisk,
    ];

    pub fn tag(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }

    /// Unknown tag → None, which every caller treats as DENY.
    pub fn from_tag(tag: &str) -> Option<Operation> {
        Operation::ALL.into_iter().find(|o| o.tag() == tag)
    }

    /// The text macOS shows in its authentication prompt. Written by KUE for the
    /// operation, never supplied by a model.
    pub fn prompt(self) -> &'static str {
        match self {
            Operation::EnrollmentCapture => "add a face sample to your KUE enrollment",
            Operation::EnrollmentUndo => "remove face samples from your KUE enrollment",
            Operation::EnrollmentReset => "delete your entire KUE face enrollment",
            Operation::EraseMemory => "permanently erase KUE's local memory",
            Operation::PurgeLegacySnapshots => "permanently delete KUE's pre-firewall snapshots",
            Operation::RecoverFromKill => "recover KUE from the kill switch and restart sensing",
            Operation::ActionHighRisk => "let KUE perform a high-risk action on your Mac",
            Operation::ActionCriticalRisk => "let KUE perform a critical action on your Mac",
            _ => "authorize a KUE operation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Requirement {
    pub level: AuthLevel,
    /// Must be authenticated for this operation now; an earlier grant does not count.
    pub fresh: bool,
    /// Only a gesture in Lantern's own window may request it.
    pub owner_gesture_only: bool,
    pub allowed_while_killed: bool,
}

pub const fn requirement(op: Operation) -> Requirement {
    use AuthLevel::*;
    use Operation::*;
    const fn r(level: AuthLevel, fresh: bool, owner_gesture_only: bool, allowed_while_killed: bool) -> Requirement {
        Requirement { level, fresh, owner_gesture_only, allowed_while_killed }
    }
    match op {
        // Stopping is always safe, for anyone, in any state.
        Kill => r(Level0, false, false, true),
        Pause => r(Level0, false, false, true),
        // Restoring observation exposes nothing by itself.
        Resume | RestartSensing => r(Level0, false, false, false),
        // Anyone may speak. What is heard grants nothing: a transcript is not
        // authentication, and asking a model with it is gated separately.
        Listen => r(Level0, false, false, false),
        // Samples of someone else, into a separate store that grants nothing.
        ProbeCapture | ProbeReset | ProbeReport => r(Level1, false, true, false),
        AskModelGeneral => r(Level1, false, false, false),
        AskModelWithPersonalContext => r(Level2, false, false, false),
        // Changes who counts as you.
        EnrollmentCapture | EnrollmentUndo => r(Level3, false, true, false),
        // Irreversible deletion.
        EnrollmentReset => r(Level4, true, true, false),
        EraseMemory | PurgeLegacySnapshots => r(Level4, true, true, true),
        RecoverFromKill => r(Level3, true, true, true),
        // Actions on the Mac, by the Action Broker's risk classification. Only
        // from Lantern's own window: no model or automation may act.
        ActionLowRisk | ActionMediumRisk => r(Level2, false, true, false),
        ActionHighRisk => r(Level3, false, true, false),
        ActionCriticalRisk => r(Level4, true, true, false),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", content = "reason", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Allow,
    Deny(String),
    NeedsStrongAuth,
    NeedsPhysicalConfirmation,
}

/// Why the identity layer reached its conclusion on this evaluation.
///
/// The identity label alone loses the distinction authorization depends on:
/// IDENTITY_UNCERTAIN is shown both when a measurement CONTRADICTS the owner and
/// when a frame could not be MEASURED at all. The first is evidence and revokes;
/// the second is the absence of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityBasis {
    /// Both descriptors matched, corroborated over consecutive frames.
    MeasuredMatch,
    /// A measured match carried through frames that could not be measured.
    CarriedMatch,
    /// This frame matched but has not yet been corroborated over enough frames.
    PendingCorroboration,
    /// One face, not measurable: capture quality under the floor.
    LowCaptureQuality,
    /// One face, not measurable: head turned past the limits.
    HeadPose,
    /// One face, not measurable: a descriptor could not be computed.
    DescriptorUnavailable,
    /// A measurement that does not match you: descriptors disagree, fall between
    /// accept and reject, or a stranger reading not yet corroborated.
    MeasuredConflict,
    /// A corroborated measurement of someone who is not you.
    MeasuredStranger,
    MultiplePeople,
    NoFace,
    /// Camera readings stopped arriving.
    StaleReading,
    NotEnrolled,
    NotObserving,
}

impl IdentityBasis {
    pub fn tag(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }

    /// A frame that says nothing about who is there.
    fn is_unmeasured(self) -> bool {
        matches!(self, IdentityBasis::PendingCorroboration | IdentityBasis::LowCaptureQuality
            | IdentityBasis::HeadPose | IdentityBasis::DescriptorUnavailable)
    }
}

/// One evaluation of the identity layer, as authorization receives it.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityObservation {
    pub identity: IdentityState,
    pub basis: IdentityBasis,
    /// The face track, when exactly one tracked face is in a fresh frame.
    pub track: Option<String>,
}

fn default_unmeasured_hold() -> f64 { 15.0 }
fn default_face_gap_grace() -> f64 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCfg {
    /// Owner presence (LEVEL_1) is held this long after the owner was last confirmed.
    pub owner_left_after_seconds: f64,
    /// The session locks when the owner has not been confirmed for this long.
    pub lock_after_seconds: f64,
    /// A Touch ID / password grant lasts at most this long.
    pub strong_auth_valid_seconds: f64,
    /// A single-use grant must be used within this many seconds.
    pub single_use_window_seconds: f64,
    /// LEVEL_2 survives frames that cannot be measured for at most this long
    /// after the last corroborated measured match, and only on the same face track.
    #[serde(default = "default_unmeasured_hold")]
    pub unmeasured_hold_seconds: f64,
    /// A gap with no face shorter than this does not end that continuity.
    #[serde(default = "default_face_gap_grace")]
    pub face_gap_grace_seconds: f64,
}

impl Default for AccessCfg {
    fn default() -> Self {
        AccessCfg { owner_left_after_seconds: 10.0, lock_after_seconds: 60.0,
                    strong_auth_valid_seconds: 300.0, single_use_window_seconds: 30.0,
                    unmeasured_hold_seconds: default_unmeasured_hold(), face_gap_grace_seconds: default_face_gap_grace() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessBlock {
    pub state: AccessState,
    pub level: AuthLevel,
    pub phase: SessionPhase,
    pub detail: String,
    pub owner_last_confirmed_seconds_ago: Option<f64>,
    pub os_auth: Option<OsAuthKind>,
    pub os_auth_expires_in_seconds: Option<f64>,
    pub lock_reason: Option<String>,
    /// Why identity reads as it does on this evaluation.
    pub basis: IdentityBasis,
    /// Seconds since the last corroborated measured match on the face in view,
    /// while LEVEL_2 is held through frames that cannot be measured.
    pub held_without_measurement_seconds: Option<f64>,
    pub requirements: Vec<OperationRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRequirement {
    pub operation: Operation,
    pub level: AuthLevel,
    pub fresh: bool,
    pub owner_gesture_only: bool,
}

#[derive(Debug, Clone)]
struct Grant { at: f64, kind: OsAuthKind }

/// The last corroborated measured match, and the face track it was made on.
#[derive(Debug, Clone)]
struct Verified { at: f64, track: Option<String>, seen_at: f64 }

#[derive(Debug, Clone)]
pub struct AccessSession {
    cfg: AccessCfg,
    identity: IdentityState,
    carried: bool,
    basis: IdentityBasis,
    track: Option<String>,
    verified: Option<Verified>,
    last_owner_evidence: Option<f64>,
    locked: bool,
    lock_reason: Option<String>,
    grant: Option<Grant>,
    single_use: Option<(Operation, OsAuthKind, f64)>,
}

impl AccessSession {
    /// A new session is LOCKED: nobody has been confirmed yet.
    pub fn new(cfg: AccessCfg) -> Self {
        AccessSession { cfg, identity: IdentityState::NotObserving, carried: false,
            basis: IdentityBasis::NotObserving, track: None, verified: None, last_owner_evidence: None,
            locked: true, lock_reason: Some("Nobody has been confirmed since KUE started.".into()),
            grant: None, single_use: None }
    }

    fn lock(&mut self, reason: String) {
        if !self.locked { self.lock_reason = Some(reason); }
        self.locked = true;
        self.grant = None;
        self.single_use = None;
        self.verified = None;
    }

    /// Feeds an identity conclusion without a basis or track. Conservative: an
    /// uncertain reading is treated as a conflict, so nothing is held through it.
    pub fn observe(&mut self, identity: IdentityState, carried: bool, now: f64) {
        let basis = match identity {
            IdentityState::MyFaceConfirmed if carried => IdentityBasis::CarriedMatch,
            IdentityState::MyFaceConfirmed => IdentityBasis::MeasuredMatch,
            IdentityState::UnknownPerson => IdentityBasis::MeasuredStranger,
            IdentityState::MultiplePeople => IdentityBasis::MultiplePeople,
            IdentityState::NoFace => IdentityBasis::NoFace,
            IdentityState::IdentityUncertain => IdentityBasis::MeasuredConflict,
            IdentityState::NotObserving => IdentityBasis::NotObserving,
        };
        self.observe_with(IdentityObservation { identity, basis, track: None }, now);
    }

    /// Feeds the engine's identity conclusion for one evaluation, with why it
    /// was reached and which face track it concerns.
    ///
    /// Continuity: LEVEL_2 earned by a corroborated measured match is kept
    /// through frames that measure nothing — low capture quality, head pose, a
    /// missing descriptor, a match still being corroborated — while the SAME
    /// face track stays in view, for at most `unmeasured_hold_seconds` after
    /// that match, and across a gap with no face of at most
    /// `face_gap_grace_seconds`. It ends at once on any measurement that does
    /// not match, a different track, a stranger, a second face, stale readings,
    /// the camera stopping, or a lock. It never unlocks a locked session.
    pub fn observe_with(&mut self, obs: IdentityObservation, now: f64) {
        let IdentityObservation { identity, basis, track } = obs;
        self.identity = identity;
        self.carried = basis == IdentityBasis::CarriedMatch;
        self.basis = basis;
        self.track = track.clone();
        let same_track = |v: &Verified| matches!((&v.track, &track), (Some(a), Some(b)) if a == b);
        match basis {
            IdentityBasis::MeasuredMatch => {
                self.verified = Some(Verified { at: now, track: track.clone(), seen_at: now });
            }
            IdentityBasis::CarriedMatch => {
                if let Some(v) = self.verified.as_mut() { v.seen_at = now; }
            }
            b if b.is_unmeasured() => {
                match self.verified.as_mut() {
                    Some(v) if same_track(v) => v.seen_at = now,
                    _ => self.verified = None,
                }
            }
            IdentityBasis::NoFace => {
                if self.verified.as_ref().is_some_and(|v| now - v.seen_at > self.cfg.face_gap_grace_seconds) {
                    self.verified = None;
                }
            }
            _ => self.verified = None,
        }
        let carried = self.carried;
        match identity {
            IdentityState::MyFaceConfirmed => {
                self.last_owner_evidence = Some(now);
                // Re-authentication after a lock needs a measured match, not a held one.
                if !carried && self.locked {
                    self.locked = false;
                    self.lock_reason = None;
                }
            }
            IdentityState::UnknownPerson =>
                self.lock("A face that is not yours was seen; owner privileges are not inherited.".into()),
            IdentityState::MultiplePeople =>
                self.lock("More than one person was seen; owner privileges are withheld.".into()),
            IdentityState::NoFace | IdentityState::IdentityUncertain => {
                let gone = self.last_owner_evidence.map(|t| now - t).unwrap_or(f64::INFINITY);
                if gone >= self.cfg.lock_after_seconds {
                    self.lock(format!("You have not been confirmed for {:.0}s.", self.cfg.lock_after_seconds));
                }
            }
            // The camera is off: departure cannot be observed, so nothing changes.
            IdentityState::NotObserving => {}
        }
    }

    /// Records what macOS confirmed. Only the shell calls this, and only with
    /// the result of a LocalAuthentication evaluation.
    pub fn record_os_auth(&mut self, kind: OsAuthKind, for_op: Option<Operation>, now: f64) {
        self.grant = Some(Grant { at: now, kind });
        self.last_owner_evidence = Some(now);
        self.locked = false;
        self.lock_reason = None;
        self.single_use = for_op.map(|op| (op, kind, now));
    }

    pub fn lock_now(&mut self, reason: &str) { self.lock(reason.to_string()); }

    fn grant_valid(&self, now: f64) -> Option<&Grant> {
        self.grant.as_ref().filter(|g| now - g.at < self.cfg.strong_auth_valid_seconds)
    }

    fn observing(&self) -> bool { self.identity != IdentityState::NotObserving }

    /// Seconds since the last corroborated measured match, while that match
    /// still stands for the face in view (see `observe_with`). None otherwise.
    pub fn continuity(&self, now: f64) -> Option<f64> {
        if self.locked { return None; }
        let v = self.verified.as_ref()?;
        let age = now - v.at;
        if age > self.cfg.unmeasured_hold_seconds { return None; }
        let holds = match self.basis {
            IdentityBasis::MeasuredMatch | IdentityBasis::CarriedMatch => true,
            b if b.is_unmeasured() => matches!((&v.track, &self.track), (Some(a), Some(b)) if a == b),
            IdentityBasis::NoFace => now - v.seen_at <= self.cfg.face_gap_grace_seconds,
            _ => false,
        };
        holds.then_some(age.max(0.0))
    }

    /// Continuity through frames that measured nothing, as opposed to a match
    /// measured or carried right now.
    fn held_without_measurement(&self, now: f64) -> Option<f64> {
        if matches!(self.identity, IdentityState::MyFaceConfirmed) { return None; }
        self.continuity(now)
    }

    pub fn phase(&self, now: f64) -> SessionPhase {
        if self.locked { return SessionPhase::Locked; }
        if !self.observing() { return SessionPhase::Unobserved; }
        if self.continuity(now).is_some() { return SessionPhase::OwnerPresent; }
        match self.last_owner_evidence {
            Some(t) if now - t < self.cfg.owner_left_after_seconds => SessionPhase::OwnerPresent,
            _ => SessionPhase::OwnerLeft,
        }
    }

    pub fn level(&self, now: f64) -> AuthLevel {
        use AuthLevel::*;
        if self.locked { return Level0; }
        let recent = self.last_owner_evidence.map(|t| now - t < self.cfg.owner_left_after_seconds).unwrap_or(false);
        let held = self.held_without_measurement(now).is_some();
        let base = match self.identity {
            IdentityState::UnknownPerson | IdentityState::MultiplePeople => return Level0,
            // Uncertain because nothing could be measured, on the face that was
            // just confirmed: see `observe_with`. Uncertain because a measurement
            // disagreed: LEVEL_0, at once.
            IdentityState::IdentityUncertain => if held { Level2 } else { return Level0 },
            // A carried match is the identity layer's own conclusion: it lasts at
            // most `hold_unmeasurable_seconds` after a measured match and ends at
            // once on a disagreeing measurement, no face or a second face. The
            // frames it is carried through are not evidence about who is there,
            // so they may not lower authorization either. Measured on this Mac,
            // treating them as LEVEL_1 flipped the level on every frame whose
            // capture quality fell under the floor (158 changes in 348 s).
            // `carried` still matters for unlocking: see `observe`.
            IdentityState::MyFaceConfirmed => Level2,
            IdentityState::NoFace => if held { Level2 } else if recent { Level1 } else { return Level0 },
            IdentityState::NotObserving => Level0,
        };
        if self.grant_valid(now).is_some() { base.max(Level3) } else { base }
    }

    pub fn state(&self, now: f64) -> AccessState {
        match self.identity {
            IdentityState::UnknownPerson => return AccessState::UnknownPerson,
            IdentityState::MultiplePeople => return AccessState::MultiplePeople,
            _ => {}
        }
        if self.locked { return AccessState::Locked; }
        if self.held_without_measurement(now).is_some() { return AccessState::AuthorizedUser; }
        match (self.identity, self.level(now)) {
            (IdentityState::IdentityUncertain, _) => AccessState::IdentityUncertain,
            (IdentityState::MyFaceConfirmed, _) => AccessState::AuthorizedUser,
            (IdentityState::NoFace, AuthLevel::Level0) => AccessState::NoPerson,
            (IdentityState::NoFace, _) => AccessState::AuthorizedUserLowConfidence,
            (IdentityState::NotObserving, AuthLevel::Level3 | AuthLevel::Level4) => AccessState::AuthorizedUser,
            (IdentityState::NotObserving, _) => AccessState::AuthenticationRequired,
            _ => AccessState::AuthenticationRequired,
        }
    }

    /// The decision for one operation. Consumes a matching single-use grant.
    pub fn authorize(&mut self, op: Operation, by: Principal, killed: bool, now: f64) -> Decision {
        let req = requirement(op);
        if killed && !req.allowed_while_killed {
            return Decision::Deny("KUE is killed.".into());
        }
        if req.owner_gesture_only && by != Principal::Owner {
            return Decision::Deny(format!("{} may only be requested from KUE's own window, not by {}.",
                op.tag(), by.label()));
        }
        if req.level == AuthLevel::Level0 { return Decision::Allow; }
        // A model or automation cannot trigger an OS prompt it would benefit from.
        let may_prompt = by == Principal::Owner;
        match self.identity {
            IdentityState::UnknownPerson => return Decision::Deny(
                "A face that is not yours is in front of the camera. Owner operations are denied, and OS authentication does not override it.".into()),
            IdentityState::MultiplePeople => return Decision::Deny(
                "More than one person is in front of the camera. Owner operations are denied.".into()),
            IdentityState::IdentityUncertain if req.level >= AuthLevel::Level1 && self.held_without_measurement(now).is_none() =>
                return Decision::Deny(if self.basis == IdentityBasis::MeasuredConflict {
                    "Identity evidence conflicts right now: a measurement did not match you. Face the camera; OS authentication does not override conflicting evidence.".into()
                } else {
                    format!("Identity evidence is uncertain right now ({}). Face the camera; OS authentication does not override uncertain evidence.", self.basis.tag())
                }),
            _ => {}
        }
        let single_use_ok = |s: &Option<(Operation, OsAuthKind, f64)>, need_physical: bool| match s {
            Some((o, kind, at)) => *o == op && now - at < self.cfg.single_use_window_seconds
                && (!need_physical || *kind == OsAuthKind::Physical),
            None => false,
        };
        if req.level == AuthLevel::Level4 {
            if single_use_ok(&self.single_use, true) { self.single_use = None; return Decision::Allow; }
            return if may_prompt { Decision::NeedsPhysicalConfirmation }
                   else { Decision::Deny("Physical confirmation is required.".into()) };
        }
        if req.fresh {
            if single_use_ok(&self.single_use, false) { self.single_use = None; return Decision::Allow; }
            return if may_prompt { Decision::NeedsStrongAuth }
                   else { Decision::Deny("Fresh OS authentication is required.".into()) };
        }
        if self.level(now) >= req.level { return Decision::Allow; }
        if may_prompt { Decision::NeedsStrongAuth }
        else { Decision::Deny(format!("{} requires {:?}; the current level is {:?}.", op.tag(), req.level, self.level(now))) }
    }

    pub fn block(&self, now: f64) -> AccessBlock {
        let state = self.state(now);
        let level = self.level(now);
        let grant = self.grant_valid(now);
        let held = self.held_without_measurement(now);
        let detail = match state {
            AccessState::AuthorizedUser if grant.is_some() => "Authenticated by macOS and not contradicted by the camera.".into(),
            AccessState::AuthorizedUser if held.is_some() => format!(
                "Confirmed by both descriptors {:.0}s ago; the same face is still in view but cannot be measured right now ({}). \
                 Held for at most {:.0}s without a new measurement, and ended at once by any measurement that does not match.",
                held.unwrap_or(0.0), self.basis.tag(), self.cfg.unmeasured_hold_seconds),
            AccessState::AuthorizedUser if self.carried =>
                "Your face was confirmed by both descriptors moments ago; this frame could not be measured, so the match is carried briefly.".into(),
            AccessState::AuthorizedUser => "Your face is confirmed by both descriptors, measured now.".into(),
            AccessState::AuthorizedUserLowConfidence => "You were confirmed moments ago but are not measured right now.".into(),
            AccessState::NoPerson => "Nobody is visible. Owner presence has lapsed.".into(),
            AccessState::UnknownPerson => "A face that is not yours: the session is locked and OS authentication is revoked.".into(),
            AccessState::MultiplePeople => "More than one person: the session is locked and OS authentication is revoked.".into(),
            AccessState::IdentityUncertain if self.basis == IdentityBasis::MeasuredConflict =>
                "A measurement did not match you: owner operations are denied.".into(),
            AccessState::IdentityUncertain => format!("Identity evidence is uncertain ({}): owner operations are denied.", self.basis.tag()),
            AccessState::AuthenticationRequired => "The camera is not observing. Authenticate with Touch ID for anything above LEVEL_0.".into(),
            AccessState::Locked => format!("Locked. {} Face the camera to re-confirm, or use Touch ID.",
                self.lock_reason.clone().unwrap_or_default()),
        };
        AccessBlock {
            state, level, phase: self.phase(now), detail,
            owner_last_confirmed_seconds_ago: self.last_owner_evidence.map(|t| (now - t).max(0.0)),
            os_auth: grant.map(|g| g.kind),
            os_auth_expires_in_seconds: grant.map(|g| (self.cfg.strong_auth_valid_seconds - (now - g.at)).max(0.0)),
            lock_reason: if self.locked { self.lock_reason.clone() } else { None },
            basis: self.basis,
            held_without_measurement_seconds: held,
            requirements: Operation::ALL.iter().map(|&op| {
                let r = requirement(op);
                OperationRequirement { operation: op, level: r.level, fresh: r.fresh, owner_gesture_only: r.owner_gesture_only }
            }).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AuthLevel::*;
    use IdentityState::*;

    fn session() -> AccessSession { AccessSession::new(AccessCfg::default()) }

    fn confirmed_owner() -> AccessSession {
        let mut s = session();
        s.observe(MyFaceConfirmed, false, 0.0);
        s
    }

    #[test]
    fn a_new_session_is_locked_until_the_owner_is_confirmed() {
        let mut s = session();
        assert_eq!((s.state(0.0), s.level(0.0)), (AccessState::Locked, Level0));
        s.observe(MyFaceConfirmed, false, 1.0);
        assert_eq!((s.state(1.0), s.level(1.0)), (AccessState::AuthorizedUser, Level2));
    }

    #[test]
    fn a_held_claim_does_not_unlock_a_locked_session() {
        let mut s = session();
        s.observe(MyFaceConfirmed, true, 1.0);
        assert_eq!(s.state(1.0), AccessState::Locked);
    }

    #[test]
    fn a_carried_match_keeps_the_level_it_was_measured_at() {
        // The measured flap: measured, carried, measured, carried... at ~4 fps.
        let mut s = confirmed_owner();
        let mut t = 0.0;
        for i in 0..240 {
            t += 0.25;
            s.observe(MyFaceConfirmed, i % 2 == 1, t);
            assert_eq!((s.state(t), s.level(t)), (AccessState::AuthorizedUser, Level2), "frame {i}");
        }
    }

    #[test]
    fn owner_present_then_left_then_locked_then_reconfirmed() {
        let mut s = confirmed_owner();
        s.observe(NoFace, false, 5.0);
        assert_eq!((s.phase(5.0), s.level(5.0)), (SessionPhase::OwnerPresent, Level1));
        s.observe(NoFace, false, 20.0);
        assert_eq!((s.phase(20.0), s.level(20.0), s.state(20.0)), (SessionPhase::OwnerLeft, Level0, AccessState::NoPerson));
        s.observe(NoFace, false, 61.0);
        assert_eq!(s.phase(61.0), SessionPhase::Locked);
        s.observe(MyFaceConfirmed, false, 70.0);
        assert_eq!((s.phase(70.0), s.level(70.0)), (SessionPhase::OwnerPresent, Level2));
    }

    #[test]
    fn an_unknown_person_locks_and_revokes_os_authentication() {
        let mut s = confirmed_owner();
        s.record_os_auth(OsAuthKind::Strong, None, 1.0);
        assert_eq!(s.level(1.0), Level3);
        s.observe(UnknownPerson, false, 2.0);
        assert_eq!((s.state(2.0), s.level(2.0)), (AccessState::UnknownPerson, Level0));
        s.observe(NoFace, false, 3.0);
        assert_eq!(s.state(3.0), AccessState::Locked);
        s.observe(MyFaceConfirmed, false, 4.0);
        assert_eq!(s.level(4.0), Level2, "the owner is back, but the Touch ID grant stays revoked");
    }

    #[test]
    fn os_authentication_cannot_override_contradicting_camera_evidence() {
        let mut s = confirmed_owner();
        s.observe(UnknownPerson, false, 1.0);
        assert!(matches!(s.authorize(Operation::EnrollmentCapture, Principal::Owner, false, 1.0), Decision::Deny(_)),
            "denied outright, not offered a Touch ID prompt");
        let mut s = confirmed_owner();
        s.observe(IdentityUncertain, false, 1.0);
        assert!(matches!(s.authorize(Operation::AskModelWithPersonalContext, Principal::Owner, false, 1.0), Decision::Deny(_)));
        s.record_os_auth(OsAuthKind::Strong, None, 2.0);
        assert!(matches!(s.authorize(Operation::EnrollmentCapture, Principal::Owner, false, 2.0), Decision::Deny(_)));
    }

    #[test]
    fn a_model_cannot_grant_itself_anything() {
        let mut s = confirmed_owner();
        for op in Operation::ALL {
            let r = requirement(op);
            let d = s.authorize(op, Principal::Model, false, 0.5);
            if r.owner_gesture_only || r.fresh || r.level > Level2 {
                assert!(matches!(d, Decision::Deny(_)), "{op:?} must be denied to a model, got {d:?}");
            }
            assert!(!matches!(d, Decision::NeedsStrongAuth | Decision::NeedsPhysicalConfirmation),
                "a model must never cause an OS prompt ({op:?})");
        }
    }

    #[test]
    fn level_4_is_single_use_bound_to_one_operation_and_needs_a_finger() {
        let mut s = confirmed_owner();
        assert_eq!(s.authorize(Operation::EraseMemory, Principal::Owner, false, 1.0), Decision::NeedsPhysicalConfirmation);
        s.record_os_auth(OsAuthKind::Strong, Some(Operation::EraseMemory), 1.0);
        assert_eq!(s.authorize(Operation::EraseMemory, Principal::Owner, false, 1.5), Decision::NeedsPhysicalConfirmation,
            "a password is strong authentication, not physical confirmation");
        s.record_os_auth(OsAuthKind::Physical, Some(Operation::EraseMemory), 2.0);
        assert_eq!(s.authorize(Operation::PurgeLegacySnapshots, Principal::Owner, false, 2.5), Decision::NeedsPhysicalConfirmation,
            "bound to the operation it was given for");
        assert_eq!(s.authorize(Operation::EraseMemory, Principal::Owner, false, 3.0), Decision::Allow);
        assert_eq!(s.authorize(Operation::EraseMemory, Principal::Owner, false, 3.1), Decision::NeedsPhysicalConfirmation,
            "used once");
    }

    #[test]
    fn a_single_use_grant_expires() {
        let mut s = confirmed_owner();
        s.record_os_auth(OsAuthKind::Physical, Some(Operation::EraseMemory), 1.0);
        assert_eq!(s.authorize(Operation::EraseMemory, Principal::Owner, false, 40.0), Decision::NeedsPhysicalConfirmation);
    }

    #[test]
    fn recovery_needs_fresh_authentication_even_with_a_valid_grant() {
        let mut s = session();
        s.record_os_auth(OsAuthKind::Strong, None, 1.0);
        assert_eq!(s.authorize(Operation::RecoverFromKill, Principal::Owner, true, 2.0), Decision::NeedsStrongAuth);
        s.record_os_auth(OsAuthKind::Strong, Some(Operation::RecoverFromKill), 3.0);
        assert_eq!(s.authorize(Operation::RecoverFromKill, Principal::Owner, true, 4.0), Decision::Allow);
    }

    #[test]
    fn killed_denies_everything_but_stopping_recovery_and_deletion() {
        let mut s = confirmed_owner();
        for op in Operation::ALL {
            let d = s.authorize(op, Principal::Owner, true, 0.5);
            if !requirement(op).allowed_while_killed {
                assert_eq!(d, Decision::Deny("KUE is killed.".into()), "{op:?}");
            }
        }
        assert_eq!(s.authorize(Operation::Kill, Principal::Model, true, 0.5), Decision::Allow);
    }

    #[test]
    fn camera_off_needs_touch_id_and_the_grant_expires() {
        let mut s = confirmed_owner();
        s.observe(NotObserving, false, 1.0);
        s.observe(NotObserving, false, 70.0);
        assert_eq!((s.state(70.0), s.level(70.0)), (AccessState::AuthenticationRequired, Level0));
        assert_eq!(s.authorize(Operation::EnrollmentCapture, Principal::Owner, false, 70.0), Decision::NeedsStrongAuth);
        s.record_os_auth(OsAuthKind::Strong, None, 71.0);
        assert_eq!(s.authorize(Operation::EnrollmentCapture, Principal::Owner, false, 72.0), Decision::Allow);
        assert_eq!(s.level(71.0 + 301.0), Level0);
    }

    #[test]
    fn every_operation_tag_round_trips_and_unknown_tags_are_denied() {
        for op in Operation::ALL { assert_eq!(Operation::from_tag(&op.tag()), Some(op)); }
        assert_eq!(Operation::from_tag("DISABLE_KILL_SWITCH"), None);
        assert_eq!(Operation::from_tag("GRANT_LEVEL_4"), None);
        assert_eq!(Operation::from_tag(""), None);
    }

    #[test]
    fn deletion_and_identity_changes_require_the_owner_and_more_than_a_face() {
        for op in [Operation::EnrollmentCapture, Operation::EnrollmentUndo, Operation::EnrollmentReset,
                   Operation::EraseMemory, Operation::PurgeLegacySnapshots, Operation::RecoverFromKill] {
            let r = requirement(op);
            assert!(r.level >= Level3 && r.owner_gesture_only, "{op:?}");
        }
    }
}
