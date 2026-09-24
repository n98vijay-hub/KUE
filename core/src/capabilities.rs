//! What KUE can do — one authoritative list.
//!
//! Every surface that says what KUE can do reads this module: the capability
//! panel, the Capabilities sheet, the answer to "what can you do?", the context
//! the model is given, what KUE says out loud, the Action Broker's mapping from
//! an action to the capability that permits it, and Diagnostics. They cannot
//! disagree, because there is only one list.
//!
//! Two properties matter more than the words:
//!
//! 1. **A capability carries the phrases that count as claiming it.** The
//!    model-answer checker (`conversation::overclaims`) used to hold its own
//!    copy of those phrases, matched to capabilities by name. Renaming a
//!    capability would have silently stopped the correction — a truthfulness
//!    regression that no test caught, because the checker's tests build their
//!    own rows. The phrases now travel with the capability, and
//!    `every_claim_phrase_belongs_to_a_live_capability` fails if they ever
//!    drift apart.
//!
//! 2. **Status is what is true, not what is intended.** `NotImplemented` rows
//!    are load-bearing: they are how KUE answers honestly, and how a model
//!    answer that claims one gets corrected.

use serde::{Deserialize, Serialize};

use crate::actions::Risk;
use crate::context::{Capability, CapabilityStatus};
use crate::privacy::DataKind;

/// Where a capability sits in the Capabilities sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Group {
    /// What KUE can sense.
    Senses,
    /// Who KUE thinks you are, and what that permits.
    Identity,
    /// What KUE works out from what it senses.
    Understanding,
    /// What KUE can do to this Mac.
    Doing,
    /// Listening and speaking.
    Voice,
    /// What is kept, and what is never kept.
    Memory,
    /// The guarantees that hold even when everything else fails.
    Safety,
}

/// What must be true of the owner before this capability may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Authorization {
    /// Runs as part of sensing; there is no request to authorize.
    None,
    /// Requires a recognised owner session (LEVEL_2 or above).
    OwnerSession,
    /// Owner session, and the owner confirms this particular request.
    Confirmation,
    /// Owner session, confirmation, and macOS authentication for this one act.
    StrongAuth,
}

/// A macOS grant this capability cannot work without.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Permission {
    Camera,
    Microphone,
    SpeechRecognition,
    Notifications,
    /// Control of other applications through the Accessibility API.
    Accessibility,
}

impl Permission {
    pub fn user_name(self) -> &'static str {
        match self {
            Permission::Camera => "Camera",
            Permission::Microphone => "Microphone",
            Permission::SpeechRecognition => "Speech Recognition",
            Permission::Notifications => "Notifications",
            Permission::Accessibility => "Accessibility",
        }
    }

    /// The pane in System Settings where the owner — and only the owner —
    /// grants it. KUE never changes a permission itself.
    pub fn settings_pane(self) -> &'static str {
        match self {
            Permission::Camera => "Privacy & Security → Camera",
            Permission::Microphone => "Privacy & Security → Microphone",
            Permission::SpeechRecognition => "Privacy & Security → Speech Recognition",
            Permission::Notifications => "Notifications",
            Permission::Accessibility => "Privacy & Security → Accessibility",
        }
    }
}

/// How KUE establishes that this capability did what it said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verification {
    /// Nothing is claimed, so nothing needs verifying.
    NotApplicable,
    /// The sensing layer's own report is the measurement.
    Measured,
    /// System state is read back after the act (the app is running, the window
    /// is frontmost).
    SystemState,
    /// The filesystem is read back after the act.
    FileSystem,
    /// A separate process reports the outcome it observed.
    ProcessReport,
    /// Deterministic arithmetic over recorded inputs.
    Arithmetic,
    /// There is nothing to verify because the capability does not exist.
    NotImplemented,
}

/// What has actually been seen, a separate question from whether a capability
/// exists (`CapabilityStatus`) and whether it can be used now (`Availability`).
///
/// LIVE_VERIFIED means it worked on this Mac through KUE's own app or its own
/// helper, with the real permissions and sensors: not a stand-in, not a
/// prerecorded clip, not a test double. Anything less is said as less. A test
/// passing, a type existing or a document describing it moves nothing here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "level", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Proof {
    /// Worked on this Mac. `on` is the date it was first seen; `seen` is what was seen.
    LiveVerified { on: &'static str, seen: &'static str },
    /// Part of it worked on this Mac; `not_seen` is the part that has not.
    PartlyLiveVerified { on: &'static str, seen: &'static str, not_seen: &'static str },
    /// Not seen working on this Mac. Automated tests are all that has checked it.
    TestVerifiedOnly,
    /// It does not exist, so there is nothing to have seen.
    NotApplicable,
}

impl Proof {
    pub fn tag(self) -> &'static str {
        match self {
            Proof::LiveVerified { .. } => "LIVE_VERIFIED",
            Proof::PartlyLiveVerified { .. } => "PARTLY_LIVE_VERIFIED",
            Proof::TestVerifiedOnly => "TEST_VERIFIED_ONLY",
            Proof::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

/// Whether a capability can be used *right now*, as opposed to whether it
/// exists. Computed from live facts, never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Availability {
    Available,
    /// Usable, with something missing. `reason` is a sentence for the owner.
    Degraded { reason: String },
    /// Implemented, but not usable right now.
    Unavailable { reason: String },
    /// Does not exist in this build.
    NotImplemented,
}

/// One capability, completely described.
#[derive(Debug, Clone)]
pub struct CapabilitySpec {
    /// Stable identifier. Never shown, never translated, safe to rename around.
    pub id: &'static str,
    /// The subsystem that implements it, for Diagnostics.
    pub internal_name: &'static str,
    /// The name in the capability table and in the model's context. Changing
    /// this is safe for the overclaim checker, because `claim_phrases` lives
    /// on the same record.
    pub name: &'static str,
    /// The name a person reads in the Capabilities sheet.
    pub user_name: &'static str,
    pub group: Group,
    /// The engineering description: exactly what is implemented and what is not.
    pub note: &'static str,
    /// One plain sentence for the Capabilities sheet.
    pub ui_description: &'static str,
    /// What KUE says out loud if asked. Shorter; no jargon; no numbers.
    pub voice_description: &'static str,
    /// The material limits, stated at the claim rather than in a footnote.
    /// Empty when there are none worth stating.
    pub limits: &'static str,
    pub status: CapabilityStatus,
    pub risk: Risk,
    pub authorization: Authorization,
    /// The kinds of data this capability touches. The privacy firewall, not
    /// this list, is what enforces handling; this is what the owner is told.
    pub privacy: &'static [DataKind],
    pub permissions: &'static [Permission],
    pub verification: Verification,
    /// What has actually been seen working. Not whether it exists (`status`)
    /// or can be used now (`availability`), and never inferred from either.
    pub proof: Proof,
    /// Not implemented on purpose, rather than not built yet. The difference
    /// is one KUE states out loud, and it must not be inferred from the first
    /// word of an English sentence: `capability_answer` keyed off
    /// `note.starts_with("Deliberately")` until this field existed, so
    /// rewording a note silently moved a capability between categories.
    pub deliberate: bool,
    /// Wordings that amount to DENYING this capability. Used to catch a model
    /// answer that tells the owner KUE cannot do something it can — measured
    /// live on 2026-09-22, when the model answered "No, I can't calculate
    /// anything" to a question about square roots. Only meaningful on rows
    /// that exist.
    pub denial_phrases: &'static [&'static str],
    /// Wordings that amount to claiming this capability. Used to catch a model
    /// answer that claims something KUE cannot do. Only meaningful on rows
    /// that are `NotImplemented`.
    pub claim_phrases: &'static [&'static str],
    /// `ActionKind` tags this capability covers. Every executable action must
    /// be covered by exactly one implemented capability.
    pub action_tags: &'static [&'static str],
}

use Authorization as Au;
use CapabilityStatus::{NotImplemented as Missing, Partial, Real};
use Group::*;
use Permission as Perm;
use Verification as V;

/// The list. Order is the order the owner reads.
pub const REGISTRY: &[CapabilitySpec] = &[
    CapabilitySpec {
        id: "camera_capture",
        internal_name: "LanternSense.CaptureSession",
        name: "Camera capture",
        user_name: "Seeing who is here",
        group: Senses,
        note: "AVFoundation capture session, analysed in memory. No frame is stored or transmitted.",
        ui_description: "KUE uses the camera to see whether you are there. Frames are analysed as they arrive and thrown away.",
        voice_description: "I use the camera to see whether you're there. I never keep a picture.",
        limits: "No frame is ever written to disk, sent anywhere, or shown to a model.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::CameraFrame],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "The camera ran in KUE's sensing layer with the owner at the desk through a 12-minute session and 623 seconds of measured camera time, and went off within 0.2 s of pausing." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "face_detection",
        internal_name: "Vision.VNDetectFaceLandmarks",
        name: "Face detection",
        user_name: "Finding a face",
        group: Senses,
        note: "Apple Vision face rectangles and 76-point landmarks.",
        ui_description: "KUE finds faces in the camera's view and measures their shape.",
        voice_description: "I can tell when there's a face in view.",
        limits: "",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::FaceMeasurement],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Faces were detected in those sessions; a second face, detected twice, locked the session as designed." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "head_pose",
        internal_name: "Vision.headPose",
        name: "Head pose",
        user_name: "Which way you are facing",
        group: Senses,
        note: "Vision yaw, pitch and roll, in degrees.",
        ui_description: "KUE measures which way a head is turned.",
        voice_description: "I can tell which way you're facing.",
        limits: "Direction only. It says nothing about attention or mood.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::FaceMeasurement],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-15", seen: "The window showed yaw, pitch and roll measured from the camera — and was then changed, because a person should not be shown measurements." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "capture_quality",
        internal_name: "Vision.faceCaptureQuality",
        name: "Face capture quality",
        user_name: "How clear the view is",
        group: Senses,
        note: "Vision's own capture-quality score.",
        ui_description: "KUE measures how clearly it can see a face, and says so when the view is poor.",
        voice_description: "I can tell when I can't see you clearly.",
        limits: "",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::FaceMeasurement],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Measured between 0.12 and 0.27 with the owner at the desk; those low readings were one cause of identity flapping." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "face_tracking",
        internal_name: "sense.tracker",
        name: "Face tracking continuity",
        user_name: "Following the same person",
        group: Senses,
        note: "IoU tracker assigning stable track IDs across frames.",
        ui_description: "KUE keeps track of the same face from moment to moment, so it does not treat you as a new person each frame.",
        voice_description: "I can follow the same face from one moment to the next.",
        limits: "",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::FaceMeasurement],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Identity was carried on the same face track across unmeasurable frames through 623 seconds of on-device camera time." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "enrollment",
        internal_name: "sense.enrollment",
        name: "Face enrollment",
        user_name: "Learning your face",
        group: Identity,
        note: "Multi-sample enrollment storing derived descriptors only, never images.",
        ui_description: "KUE learns your face from several samples and keeps only the measurements, never the pictures.",
        voice_description: "I learn your face from a few samples. I keep the measurements, not the pictures.",
        limits: "Changing or deleting your enrollment requires Touch ID.",
        status: Real,
        risk: Risk::High,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::FaceDescriptor, DataKind::EnrollmentSummary],
        permissions: &[Perm::Camera],
        verification: V::FileSystem,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "The owner enrolled on this Mac; the derived descriptors on disk are what identity has matched against since." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "identity_matching",
        internal_name: "sense.matcher",
        name: "Identity matching",
        user_name: "Recognising you",
        group: Identity,
        note: "Landmark geometry plus Vision FeaturePrint distance. Calibrated on your samples only — the reject side is an unvalidated heuristic.",
        ui_description: "KUE compares the face it sees to what it learned of yours.",
        voice_description: "I can tell it's you, and I'll say so when I'm not certain.",
        limits: "Calibrated on your samples only. How well it rejects other people has not been measured, so KUE states uncertainty rather than hiding it.",
        status: Partial,
        risk: Risk::Medium,
        authorization: Au::None,
        privacy: &[DataKind::FaceDescriptor, DataKind::IdentityConclusion],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::PartlyLiveVerified { on: "2026-09-14", seen: "Matched the enrolled owner at the desk across on-device sessions.", not_seen: "A steady match: on 2026-09-16 the owner was measured as not matching 304 times, each followed by a match about a second later. Rejecting a different person has never been measured." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "frontmost_application",
        internal_name: "sense.workspace",
        name: "Frontmost application",
        user_name: "Which app is in front",
        group: Senses,
        note: "NSWorkspace. Application identity only; no window titles, documents or URLs.",
        ui_description: "KUE knows which application is in front. It does not read window titles, documents or web addresses.",
        voice_description: "I know which app is in front. I don't read what's in it.",
        limits: "The application's name, and nothing inside it.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::FrontmostApplication],
        permissions: &[],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "The sensing layer sampled the app in front about once a second on this Mac, and stopped while paused (3 samples in 3.2 s, none in 5 s paused, 4 in 3.2 s after resuming)." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "input_idle",
        internal_name: "sense.hidIdle",
        name: "Recent input detection",
        user_name: "Whether you are at the keyboard",
        group: Senses,
        note: "HID idle timer. Returns seconds since last input and nothing else — it cannot read keystrokes.",
        ui_description: "KUE knows how long it has been since you last touched the keyboard or trackpad. It cannot see what you type.",
        voice_description: "I know how long it's been since you last used the keyboard. I can't see what you type.",
        limits: "A number of seconds. Never a key, never a word.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::InputIdleTime],
        permissions: &[],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "The window showed seconds since the last keyboard or mouse input from the idle timer on this Mac, and sampling stopped while paused." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "event_stream",
        internal_name: "core.events",
        name: "Temporal event stream",
        user_name: "Keeping track of what changed",
        group: Memory,
        note: "Change-triggered event log with stability tracking.",
        ui_description: "KUE records when something changed — you arrived, you left, the app in front changed — with the time it happened.",
        voice_description: "I keep a record of what changed and when.",
        limits: "",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::EventRecord],
        permissions: &[],
        verification: V::Arithmetic,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Events recorded on this Mac were read back to measure identity flapping (158 access changes in 348 s) and to find the speech identity-dip defect." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "evidence",
        internal_name: "core.evidence",
        name: "Evidence and confidence",
        user_name: "Why KUE believes something",
        group: Understanding,
        note: "Deterministic arithmetic from configured weights. No model produces a confidence number.",
        ui_description: "Every conclusion KUE reaches can be traced to the measurements behind it.",
        voice_description: "I can tell you why I think what I think.",
        limits: "Confidence is arithmetic over measurements. No model invents a number.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::Evidence],
        permissions: &[],
        verification: V::Arithmetic,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Presence statements were shown on this Mac from live readings; a defect seen there — a paused KUE still claiming activity — was fixed." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "local_memory",
        internal_name: "core.store",
        name: "Local structured memory",
        user_name: "Remembering",
        group: Memory,
        note: "SQLite store of events and context snapshots on this Mac.",
        ui_description: "KUE keeps its conclusions in a file on this Mac. You can erase all of it.",
        voice_description: "I keep what I've worked out on this Mac, and you can erase it.",
        limits: "Erasing is all-or-nothing in this build.",
        status: Real,
        risk: Risk::Medium,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::EventRecord, DataKind::IdentityConclusion, DataKind::ActivityConclusion],
        permissions: &[],
        verification: V::FileSystem,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Events and snapshots were written to the database on this Mac and read back for the identity measurements." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "kill_switch",
        internal_name: "core.runtime",
        name: "Kill switch",
        user_name: "Stopping KUE",
        group: Safety,
        note: "KUE_RUNNING / KUE_PAUSED / KUE_KILLED / KUE_RECOVERING. A kill terminates the sensing process, blocks every memory write, is saved to a latch before anything else, and survives a relaunch. Only the owner can recover, in two steps; a model, an automation or a deleted latch cannot.",
        ui_description: "One control stops everything: the camera, the microphone, memory and speech. It survives a restart, and only you can undo it.",
        voice_description: "You can stop me completely at any time, and only you can start me again.",
        limits: "",
        status: Real,
        risk: Risk::Critical,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::SystemCondition],
        permissions: &[],
        verification: V::ProcessReport,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Killed from outside the window and relaunched while killed on this Mac: KUE stayed stopped." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "owner_session",
        internal_name: "core.authz",
        name: "Owner session and authorization",
        user_name: "Knowing what you are allowed to ask",
        group: Identity,
        note: "LEVEL_0–4 from camera identity plus macOS authentication. The session starts LOCKED, drops to owner-left after 10s unseen and locks at 60s; a stranger or a second person locks it at once and revokes Touch ID. Enrollment changes, deletion and kill recovery are gated. Unknown operation = DENY.",
        ui_description: "What KUE will do depends on how sure it is that you are there. It locks when you leave, and at once if someone else appears.",
        voice_description: "What I'll do depends on whether I'm sure it's you. I lock when you leave.",
        limits: "Anything KUE is unsure about is refused, not guessed.",
        status: Real,
        risk: Risk::High,
        authorization: Au::None,
        privacy: &[DataKind::IdentityConclusion],
        permissions: &[Perm::Camera],
        verification: V::Arithmetic,
        proof: Proof::PartlyLiveVerified { on: "2026-09-14", seen: "The owner reached LEVEL_2 from the camera and a spoken and a typed action ran under it (2026-09-15); a second face locked the session; the window showed Locked.", not_seen: "Access that stays steady while the owner sits at the desk. On 2026-09-16 it still changed 755 times in one hour, after the 2026-09-14 fix." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "os_authentication",
        internal_name: "kue-auth",
        name: "Touch ID / OS authentication",
        user_name: "Asking macOS to check it's you",
        group: Identity,
        note: "Apple LocalAuthentication in a separate helper. macOS draws the prompt; KUE receives only success or the reason for failure — never a fingerprint or password. LEVEL_4 requires a finger on the sensor for that one operation.",
        ui_description: "For anything serious, macOS itself asks for Touch ID. KUE is told only whether it worked.",
        voice_description: "For anything serious, macOS asks for your fingerprint. I only learn whether it worked.",
        limits: "KUE never sees a fingerprint or a password, and cannot bypass this prompt.",
        status: Real,
        risk: Risk::High,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::IdentityCheckResult],
        permissions: &[],
        verification: V::ProcessReport,
        proof: Proof::PartlyLiveVerified { on: "2026-09-14", seen: "The helper checked this Mac: Touch ID is present and both authentication policies are available.", not_seen: "A Touch ID prompt completed through KUE for an action. The storage move was checked with a stand-in answer." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "failure_reporting",
        internal_name: "core.conditions",
        name: "Failure-state reporting",
        user_name: "Telling you when something is wrong",
        group: Safety,
        note: "Every named failure state is reported with a status, including IPC failure detected by the interface.",
        ui_description: "When part of KUE fails, KUE says which part and what it means — it does not carry on as if nothing happened.",
        voice_description: "If something of mine fails, I'll tell you which part.",
        limits: "",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::SystemCondition],
        permissions: &[],
        verification: V::Measured,
        proof: Proof::TestVerifiedOnly,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "perception_observability",
        internal_name: "core.measurement",
        name: "Knowing how well it can see",
        user_name: "Knowing how well it can see you",
        group: Understanding,
        note: "KUE separates a measurement that is LATE from one that is STALE, AMBIGUOUS or CONTRADICTED, and records how the measuring went beside what it concluded: measurement age, Vision analysis time, capture gaps, dropped frames, the model's phase, the track and the access level at the same instant. The sensing layer sends a heartbeat every second whether or not a frame was produced, which is what makes \"late but alive\" something KUE can say at all. Observation only: no identity or authorization decision reads any of it.",
        ui_description: "KUE keeps track of how well it can see you, and tells the difference between a reading that is late and one that disagrees.",
        voice_description: "I keep track of how well I can see you, and I can tell a late reading from one that disagrees.",
        limits: "It measures the measuring; it changes no decision. Identity still demotes on a reading older than three seconds.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::PerceptionTiming, DataKind::StageTiming],
        permissions: &[],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-20", seen: "893 samples in 15 minutes on this Mac: 65.6% of the moments KUE was unsure were AMBIGUOUS measurements, 16.9% a live pipeline being late, 2.5% stale, and not one was a reading that said somebody else." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "resource_monitoring",
        internal_name: "core.environment",
        name: "Resource monitoring",
        user_name: "Watching its own cost",
        group: Understanding,
        note: "CPU time and memory footprint of the sensing layer and the shell, thermal state, Low Power Mode and battery, all measured. GPU, Neural Engine and energy impact are not measured.",
        ui_description: "KUE measures what it costs this Mac in processor time and memory.",
        voice_description: "I keep track of what I cost this Mac.",
        limits: "Processor and memory are measured. GPU, Neural Engine and energy impact are not.",
        status: Partial,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::SystemCondition],
        permissions: &[],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "The sensing layer measured its own cost on this Mac: 0.07% CPU and 4.1 MB while paused." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "thermal_adaptation",
        internal_name: "sense.rateControl",
        name: "Thermal-aware analysis rate",
        user_name: "Easing off when the Mac is hot",
        group: Understanding,
        note: "Camera analysis drops to a reduced rate under thermal pressure or in Low Power Mode, and says so.",
        ui_description: "When this Mac is hot or in Low Power Mode, KUE looks less often — and tells you it is.",
        voice_description: "When the Mac is hot, I look less often.",
        limits: "",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::SystemCondition],
        permissions: &[],
        verification: V::Measured,
        proof: Proof::TestVerifiedOnly,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "body_hand_pose",
        internal_name: "Vision.bodyPose/handPose",
        name: "Body and hand pose",
        user_name: "Seeing that you are there, not just your face",
        group: Senses,
        note: "Apple Vision body pose (15 upper and core joints) and hand pose (up to two hands), about once a second. Used for 'upper body visible' and 'hand overlaps face' only — no gesture, posture or mood is inferred.",
        ui_description: "KUE can tell an upper body is in view, and when a hand covers a face.",
        voice_description: "I can tell when you're in view even if I can't see your face clearly.",
        limits: "No gesture, posture or mood is inferred from this.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::BodyJointPositions, DataKind::HandPositions, DataKind::PresenceSummary],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::PartlyLiveVerified { on: "2026-09-14", seen: "An upper body was detected on this Mac (4 of 4 joints).", not_seen: "A hand overlapping the face has not been checked on this Mac." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "scene_recognition",
        internal_name: "Vision.classifyImage",
        name: "Object and scene recognition",
        user_name: "A rough sense of the surroundings",
        group: Senses,
        note: "Apple Vision whole-frame classification into its built-in label set, and animal detection, every 10 seconds. Labels describe the whole frame; objects are not located, and there is no custom object model.",
        ui_description: "Every ten seconds KUE forms a rough impression of the whole scene — indoors, a desk, a dog.",
        voice_description: "I get a rough sense of what's around you.",
        limits: "Labels describe the whole picture. KUE cannot locate an object or learn a new one.",
        status: Partial,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::SceneLabels],
        permissions: &[Perm::Camera],
        verification: V::Measured,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Scene labels and a brightness of 0.66 were read from the camera on this Mac." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "screen_context",
        internal_name: "—",
        name: "Screen context",
        user_name: "Reading your screen",
        group: Senses,
        note: "KUE never reads your screen. Would require explicit opt-in and a separate permission.",
        ui_description: "KUE cannot see your screen. Nothing in this build reads it.",
        voice_description: "I can't see your screen.",
        limits: "Not implemented. It would need your explicit opt-in and a separate macOS permission.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::ScreenContent],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["your screen", "the screen", "on screen", "screenshot", "what you're looking at"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "microphone",
        internal_name: "sense.voice + SpeechAnalyzer",
        name: "Microphone and voice",
        user_name: "Listening when you speak to it",
        group: Voice,
        note: "Push-to-talk has been used on this Mac (a spoken request was transcribed and reached the command parser); speech recognition was also verified from audio files. The open microphone for listening for KUE's name has not been exercised live. Push-to-talk: voice activity from audio level, then Apple SpeechAnalyzer transcription on this Mac (en-US model installed). The microphone runs only between Speak and a pause, stop, 30s limit, pause or kill; audio is never stored, and a transcript goes only to the conversation box. While the microphone is live (a fresh report from a running sensing layer) KUE does not speak; a stale report does not silence it.",
        ui_description: "The microphone runs while you use the speak control, and stops at a pause or after thirty seconds. Listening for its name is separate, off unless you turn it on, and shown in the window whenever it runs. Audio is never stored.",
        voice_description: "I listen when you use the speak button, or when you say my name if you turned that on. I never keep the audio.",
        limits: "KUE does not listen on its own unless you turn on listening for its name, and that open microphone has not yet been tried in a real room.",
        status: Partial,
        risk: Risk::Medium,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::AudioSample],
        permissions: &[Perm::Microphone, Perm::SpeechRecognition],
        verification: V::ProcessReport,
        proof: Proof::PartlyLiveVerified { on: "2026-09-14", seen: "Push-to-talk in KUE's window: spoken requests were transcribed on this Mac, and one opened an app (2026-09-15).", not_seen: "The microphone being released on pause, kill and lock, and a refused or revoked microphone permission, have not been exercised live." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "wake_word",
        internal_name: "sense.WakeEngine + core.voice.wake",
        name: "Listening for its name",
        user_name: "Answering when you say its name",
        group: Voice,
        note: "Off until the owner turns it on. When on, the microphone stays open and the sensing layer recognises speech ON THIS MAC inside a boundary that can emit one thing: whether the invocation was heard, and what was said in the same breath. Silence is never recognised — buffers reach the recogniser only while there is voice energy, with half a second of pre-roll, and each utterance gets its own recogniser, finished at the pause that ends it. Nothing is recorded, nothing is stored, and no audio leaves the machine. It stops with pause, kill, the sensing layer, or the owner's setting, and the trust strip says the microphone is open the whole time it is. The phrase was chosen on finished audio files, five voices spoken by `say`: “computer” heard 25 of 25 with 0 false wakes in 80 lines; “hello kue” 14 of 25; “KUE” alone 6 of 15 — it is one syllable and the recogniser writes it as Q, okay, who, quay or nothing. The LIVE listener's own code path, measured with a clip for a microphone and no end of input, woke for 0 of 20 invocations as first built; after three fixes it gave 105 of 105 expected outcomes over five voices (40 invocations heard with their request, 65 ordinary sentences ignored), waking about 1.9 s after speech ends. “Computer science is hard” wakes it, because the name comes first. None of this is a room or a real microphone. The invocation must come first in the sentence, which is what keeps ordinary speech about queues and computers from waking it. Hearing a name is not knowing who spoke: a wake grants nothing, and what follows it is authorized exactly like a typed request.",
        ui_description: "KUE can wait for you to say its name and then listen, so you do not have to click anything. The microphone is open while it waits, and the window says so.",
        voice_description: "If you turn it on, I'll wait for you to say my name instead of you clicking anything.",
        limits: "Off by default. It hears a name, not a person — anyone can say it, and what they ask for still has to pass everything else.",
        status: Partial,
        risk: Risk::Medium,
        authorization: Au::None,
        privacy: &[DataKind::AudioSample, DataKind::OwnerMessage],
        permissions: &[Perm::Microphone, Perm::SpeechRecognition],
        verification: V::Measured,
        proof: Proof::TestVerifiedOnly,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "voice_output",
        internal_name: "core.voice + kue-voice",
        name: "Voice output",
        user_name: "Speaking",
        group: Voice,
        note: "KUE speaks through one core pipeline: fixed KUE-written narration from an action's recorded state (never from a model), then the speech gate (kill, verbosity, live microphone, LEVEL_2 for anything built from your data), the privacy firewall, and a core queue with five priorities, interruption, de-duplication and per-request states, then Apple's on-device synthesizer in a separate process (a female voice by default; Samantha on this Mac). The window can only stop speech or change voice settings. Kill cancels speech at once and terminates the process; pause, lock and your speaking stop it and drop the queue. Tested against the real synthesizer at volume 0. No external or trained voice exists.",
        ui_description: "KUE speaks with a synthetic voice on this Mac. What it says is written by KUE from what actually happened, never by a model.",
        voice_description: "I speak with a voice built into this Mac. I say what happened, not what a model made up.",
        limits: "No voice is cloned or trained, and nothing is sent away to be spoken.",
        status: Partial,
        risk: Risk::Low,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::ActivityConclusion],
        permissions: &[],
        verification: V::ProcessReport,
        proof: Proof::LiveVerified { on: "2026-09-15", seen: "KUE's voice spoke an answer to completion from the bundled app, and a defect seen there — answers cut off when identity dipped — was fixed." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "speaker_identity",
        internal_name: "—",
        name: "Speaker identity",
        user_name: "Recognising who is speaking",
        group: Voice,
        note: "Not implemented. No Apple API verifies who is speaking, so it needs a third-party speaker model, and downloading one waits for the owner's decision on its licence and training data. Speech-to-text is not authentication: a voice grants no access level. As designed (docs/KUE_SPEAKER_IDENTITY.md), a voice match could only confirm or contradict the face session — never grant more than it, and never replace Touch ID.",
        ui_description: "KUE cannot tell who is speaking. A voice never grants access.",
        voice_description: "I can't tell who's speaking, and a voice never unlocks anything.",
        limits: "Not implemented. If it is built, your voice could only confirm or contradict the camera; it would never unlock anything on its own.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::AudioSample],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["recognize your voice", "recognise your voice", "voice recognition", "who is speaking", "know your voice"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "conversation",
        internal_name: "core.conversation",
        name: "Natural conversation",
        user_name: "Answering your questions",
        group: Understanding,
        note: "Typed questions answered from your live context, remembering the last four exchanges this session. Works end to end, but Apple's small on-device model gives short answers that can be shallow or wrong, in roughly 5–15 seconds. Questions can be typed or spoken. 'What can you do?' is answered from this list, not the model, and every model answer is treated as untrusted text (core/src/model.rs): cut where the model starts writing the owner's next turn, and corrected where it claims a capability marked not implemented or claims to have done something KUE never does (sent an email, bought, deleted) — while a true account of what KUE's own actions did is left alone. Nothing placed in a prompt, including an earlier answer, can begin a line of it. Needs LEVEL_2 or Touch ID; cleared when the session locks or KUE is killed; never written to memory.",
        ui_description: "You can ask KUE questions. It answers from what it can see, on this Mac, remembering the last few exchanges.",
        voice_description: "You can ask me questions, and I answer from what I can see, here on this Mac.",
        limits: "The on-device model is small: answers are short, take 5–15 seconds, and can be wrong. Every answer is checked against this list.",
        status: Partial,
        risk: Risk::Medium,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::IdentityConclusion, DataKind::ActivityConclusion],
        permissions: &[],
        verification: V::ProcessReport,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Typed and spoken questions were answered on this Mac by the on-device model, and an answer that claimed a capability KUE lacks was caught there." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "on_device_model",
        internal_name: "lantern-mind (FoundationModels)",
        name: "On-device language model",
        user_name: "Thinking on this Mac",
        group: Understanding,
        note: "Apple's FoundationModels model in a separate process with no sensor access. It receives only a prompt the privacy firewall cleared for a local model, chosen by the model router, and can only return text. Measured on this Mac: 5.6s for a one-line answer. Terminated by the kill switch.",
        ui_description: "The language model runs on this Mac, in its own process, with no access to the camera and no way to act.",
        voice_description: "The model I think with runs on this Mac. It can't see the camera and it can't do anything by itself.",
        limits: "It receives only what the privacy firewall cleared, and it can return text and nothing else.",
        status: Real,
        risk: Risk::Medium,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::IdentityConclusion],
        permissions: &[],
        verification: V::ProcessReport,
        proof: Proof::LiveVerified { on: "2026-09-14", seen: "Answered on this Mac, 5.6 s for a one-line answer." },
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "arithmetic",
        internal_name: "core.calculate",
        name: "Arithmetic",
        user_name: "Working out numbers",
        group: Understanding,
        note: "Addition, subtraction, multiplication, division, parentheses, percentages (17% of 840), whole-number powers (2^10, seven squared) and square roots (the square root of 1159), parsed by rule and computed in exact fractions in KUE's own process. The result is read back into a fraction and compared with the computed value before it is said. A root that is not exact is worked out to nine decimals and bracketed between two fractions which are squared to prove those digits; the answer is then said as about. A negative square root, a fractional exponent, and numbers too large to hold exactly are refused rather than approximated. No model is used, and not the Calculator app: typing into another app is not implemented.",
        ui_description: "KUE works out sums, percentages, powers and square roots itself, and checks the number before saying it.",
        voice_description: "I can work out sums, percentages, powers and square roots myself, and I check the answer before I say it.",
        limits: "Numbers and + − × ÷, brackets, percentages, whole-number powers and square roots. No units, currencies, dates, word problems or other functions.",
        status: Partial,
        risk: Risk::Low,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::OwnerMessage],
        permissions: &[],
        verification: V::Arithmetic,
        proof: Proof::TestVerifiedOnly,
        deliberate: false,
        denial_phrases: &["calculate", "calculate anything", "do math", "do maths", "do arithmetic", "work out sums", "do calculations", "work that out", "compute"],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "internet_research",
        internal_name: "—",
        name: "Internet research",
        user_name: "Looking things up online",
        group: Understanding,
        note: "KUE makes no network requests of any kind.",
        ui_description: "KUE cannot reach the internet. It makes no network requests at all.",
        voice_description: "I can't go online. Everything I know comes from this Mac.",
        limits: "Not implemented, by design: nothing about you leaves this Mac.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["internet", "the web", "browse", "browsing", "search online", "online search",
            "look things up", "look it up online", "search for information", "latest news", "the weather"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "external_model",
        internal_name: "router::ModelId::External",
        name: "External language model",
        user_name: "A cloud language model",
        group: Understanding,
        note: "Not implemented. No external model (Claude or any other) is configured, and KUE has no network code. The model router lists an external candidate only so its refusal is explicit and tested: privacy policy v1 refuses every kind of personal context to an external model. The boundary one would sit behind exists — a provider accepts only a prompt the firewall cleared for its own destination, and every answer is checked as untrusted text — but using one would need the owner's API key and the owner's decision to let sanitized context leave this Mac.",
        ui_description: "KUE does not use a cloud language model. Everything it answers is worked out on this Mac.",
        voice_description: "I don't use a cloud model. Everything I answer is worked out on this Mac.",
        limits: "Not implemented. It would need your API key, and your decision to let some context leave this Mac.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "emotion_reading",
        internal_name: "—",
        name: "Emotion or mood reading",
        user_name: "Reading how you feel",
        group: Understanding,
        note: "Deliberately excluded. Facial geometry does not reliably indicate emotion, and KUE will not pretend otherwise.",
        ui_description: "KUE does not guess how you feel. The shape of a face does not reliably show emotion.",
        voice_description: "I don't guess how you're feeling. A face doesn't reliably show that.",
        limits: "Deliberately excluded, and it will not be added on the strength of facial geometry.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::FaceMeasurement],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: true,
        denial_phrases: &[],
        claim_phrases: &["your mood", "your emotion", "emotional state", "how you're feeling", "how you are feeling"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "proactive_assistance",
        internal_name: "—",
        name: "Proactive assistance",
        user_name: "Acting on its own",
        group: Doing,
        note: "KUE observes and reports. It does not act on its own.",
        ui_description: "KUE never acts on its own. Everything it does begins with something you asked for.",
        voice_description: "I don't do anything on my own. You ask, then I act.",
        limits: "Not implemented. There are no reminders and no background actions.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["remind you", "reminders", "proactively", "anticipate your"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "pattern_recognition",
        internal_name: "—",
        name: "Personal pattern recognition",
        user_name: "Learning your habits",
        group: Memory,
        note: "Events are stored but not yet mined for patterns.",
        ui_description: "KUE records what happens but does not yet draw patterns from it.",
        voice_description: "I keep a record of what happens, but I don't yet find patterns in it.",
        limits: "Not implemented.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[DataKind::EventRecord],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["your habits", "your patterns", "your routine", "learn your"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "memory_recall",
        internal_name: "—",
        name: "Recalling the past",
        user_name: "Remembering what happened before",
        group: Understanding,
        note: "Not implemented. KUE records events and snapshots locally, but has no way to answer a question about the past from them — “what did I do yesterday” is answered by rule with that, not by a model. What the owner asks KUE to remember is a different capability (personal_memory), and that one exists.",
        ui_description: "KUE can't look back through what happened before yet. It does keep what you ask it to remember.",
        voice_description: "I can't look back through what happened before yet, though I do keep what you ask me to remember.",
        limits: "Not implemented.",
        status: Missing,
        risk: Risk::Medium,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::EventRecord],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["i remember everything that happened", "i can look back through"],
        action_tags: &[],
    },
    // What the owner tells KUE to keep, and what KUE did and checked. Separate
    // from recalling the past out of the event log, which is still missing:
    // one of them exists and the other does not, and the registry says so.
    CapabilitySpec {
        id: "personal_memory",
        internal_name: "core.memory",
        name: "Personal memory",
        user_name: "Keeping what you tell it",
        group: Understanding,
        note: "KUE keeps what the owner asks it to remember — how they like things done, something about their work, what they are working towards — and what it did and read back itself. Each memory carries where it came from, when, and how long it is good for. They are found again by the words the owner uses, replaced only when the owner says so (a disagreement is asked about, never overwritten), and forgotten on request: the words go from the store, and the write-ahead log is checkpointed so they are not left there. They survive a restart. Nothing is kept because a model suggested it: a model's memory is a CANDIDATE, which is never returned as something the owner said, and VERIFIED needs the action pipeline's own read-back.",
        ui_description: "KUE keeps what you ask it to remember, and what it did and checked — with where each one came from. You can ask why, and tell it to forget.",
        voice_description: "I keep what you ask me to remember, and what I've done and checked. I can tell you where each came from, and forget any of it when you say.",
        limits: "What you tell it, what it verified, decisions you made, and notes about the task in hand that expire after four hours. Not your files' contents, not the event log, and nothing a model made up. It stays on this Mac.",
        status: Partial,
        risk: Risk::Medium,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::OwnerMessage, DataKind::EventRecord],
        permissions: &[],
        verification: V::FileSystem,
        proof: Proof::TestVerifiedOnly,
        deliberate: false,
        denial_phrases: &["remember what you tell me", "keep anything you tell me"],
        claim_phrases: &[],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "storage_inspection",
        internal_name: "core.storage",
        name: "Storage inspection",
        user_name: "Working out what is filling up your Mac",
        group: Understanding,
        note: "Reads the volume's capacity and available space from the filesystem, then walks Desktop, Documents, Downloads and ~/KUE recording name, size and date — four levels deep, never into packages, hidden folders or links, stopping at 40,000 entries and saying so when it does. Groups the findings into probable duplicates (same name and size; contents are NOT compared), installers older than 30 days (.dmg, .pkg, .iso, .mpkg), downloads not changed in 180 days, and files over 512 MB not changed in 90 days. Nothing changed in the last 14 days is ever a candidate. Opens no file and changes nothing. macOS does not report when a file was last opened, so no finding rests on that.",
        ui_description: "KUE can measure how full your drive is, work out where the space has gone, and show you what looks worth reviewing — with the evidence for each one.",
        voice_description: "I can check your storage, work out where the space went, and show you what's worth reviewing.",
        limits: "Only Desktop, Documents, Downloads and KUE. It reads names, sizes and dates — never what is in a file — so duplicates are probable, not proven.",
        status: Real,
        risk: Risk::Low,
        authorization: Au::OwnerSession,
        privacy: &[DataKind::StorageInventory, DataKind::StorageSummary],
        permissions: &[],
        verification: V::FileSystem,
        proof: Proof::PartlyLiveVerified { on: "2026-09-15", seen: "Measured this Mac's drive and walked its folders through the transaction the app uses: 37,055 entries in 838 ms. The review sheet in the running app withheld the report because identity was locked.", not_seen: "The report shown to the owner in the window." },
        deliberate: false,
        denial_phrases: &["check your storage", "see your storage", "look at your storage", "inspect your storage", "see your files", "check your disk"],
        claim_phrases: &[],
        action_tags: &["INSPECT_STORAGE"],
    },
    CapabilitySpec {
        id: "storage_cleanup",
        internal_name: "core.transaction + KueAct trash/untrash",
        name: "Moving files to the Trash",
        user_name: "Clearing space, with your say-so",
        group: Doing,
        note: "Moves files the owner selected from a storage report to the Trash, using macOS's own trashItem so Finder's Put Back works — never a delete, and never removeItem. Only files KUE itself found and showed are eligible: a path that was not in the report is refused before authorization is asked for. At most 50 files at once, files only (not folders), nothing in ~/Library, nothing outside Desktop, Documents, Downloads and ~/KUE. HIGH risk: owner session, explicit selection, confirmation and macOS authentication. Each file is moved and checked on its own, so a batch where some fail is reported as what it was; the record says which. KUE never claims space was freed — the Trash is on the same volume, and emptying it is the owner\'s to do. Undo puts every file back exactly where it came from, and refuses any whose old place is now taken.",
        ui_description: "KUE can move files you pick to the Trash, one at a time, checking each one. Nothing is deleted, and it can put them all back.",
        voice_description: "I can move files you pick to the Trash, and put them back if you change your mind. I never delete anything.",
        limits: "Only files it found and showed you, at most 50 at once. It never empties the Trash, so no space comes back until you do.",
        status: Real,
        risk: Risk::High,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::StorageInventory, DataKind::ActionTarget],
        permissions: &[],
        verification: V::FileSystem,
        proof: Proof::PartlyLiveVerified { on: "2026-09-15", seen: "Two installers KUE created were found in its own report, moved to the real Trash and put back, through the executor the app uses.", not_seen: "A move started from the window with the owner recognised and a real Touch ID prompt; stand-ins answered for both." },
        deliberate: false,
        denial_phrases: &["move files to the trash", "move your files", "clean up your storage", "free up space"],
        claim_phrases: &[],
        action_tags: &["MOVE_TO_TRASH", "RESTORE_FROM_TRASH"],
    },
    CapabilitySpec {
        id: "calendar",
        internal_name: "—",
        name: "Calendar and reminders",
        user_name: "Your calendar and reminders",
        group: Doing,
        note: "Not implemented. KUE cannot read your calendar, add an event, or set a reminder — there is no EventKit code and no Reminders code, and no calendar permission is requested. A read-only calendar helper exists on a side branch, unbuilt and never run. What KUE can do instead is show a notification when you ask it to, in the moment.",
        ui_description: "KUE cannot read your calendar or set reminders. It can show you a notification when you ask for one.",
        voice_description: "I can't set reminders or read your calendar.",
        limits: "Not implemented.",
        status: Missing,
        risk: Risk::Low,
        authorization: Au::None,
        privacy: &[],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["set a reminder", "your calendar", "your schedule", "calendar event", "your meetings"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "permanent_deletion",
        internal_name: "—",
        name: "Deleting files",
        user_name: "Deleting anything for good",
        group: Doing,
        note: "Deliberately not implemented, and not planned. KUE moves files to the Trash and can put them back; it never empties the Trash, never deletes a file, and never overwrites one. Nothing in the code can: there is no delete verb in the executor and no allowlisted action for it. Emptying the Trash is the owner's, in Finder.",
        ui_description: "KUE never deletes anything. It moves files to the Trash, where you can put them back — emptying it is yours to do, in Finder.",
        voice_description: "I never delete anything. I can move files to the Trash, and you empty it yourself.",
        limits: "Not implemented, on purpose.",
        status: Missing,
        risk: Risk::Critical,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::ActionTarget],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: true,
        denial_phrases: &[],
        claim_phrases: &["empty your trash", "delete those files", "delete them for you", "i'll delete",
            "i can delete", "permanently delete", "erase those files"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "computer_automation",
        internal_name: "core.actions + KueAct",
        name: "Computer automation",
        user_name: "Doing things on this Mac",
        group: Doing,
        note: "Action Broker with the brief's allowlist plus three additions: open, quit and switch apps; open http(s) links; create folders and files, read and move files inside ~/KUE; show notifications; open a document by name (“open my resume”); open a folder by name in Finder and list what is inside one (names, kinds and dates only). App names are resolved against the apps installed on this Mac (“Chrome” → Google Chrome; several matches → you choose; none → not found, never guessed). Folders are searched only in Desktop, Documents, Downloads and ~/KUE. A request of several steps (“open the Tampa folder on Desktop and tell me what resumes are inside”) runs step by step, each authorized and verified, and stops at the first step that does not succeed. Commands are parsed without a model. LOW runs once authorized; MEDIUM needs Confirm; HIGH needs Confirm plus Touch ID. Every success is verified; anything unverified is UNKNOWN_RESULT. No shell, no deletion, no overwrite, no force-quit. Typing into or reading another app's window (e.g. Calculator arithmetic) is not implemented: it needs macOS Accessibility control, and such a request is refused whole before any step starts.",
        ui_description: "KUE can open and quit apps, open links, open a document or folder by name, list what is in a folder, create folders and files in its own folder, and show notifications. Each step is checked and verified.",
        voice_description: "I can open apps, open a document or folder by name, tell you what's in a folder, and make notes in my own folder.",
        limits: "Only this list. No shell commands, no deleting, no overwriting, and nothing outside your Desktop, Documents, Downloads and KUE folders.",
        status: Real,
        risk: Risk::Medium,
        authorization: Au::Confirmation,
        privacy: &[DataKind::ActionTarget],
        permissions: &[Perm::Notifications],
        verification: V::SystemState,
        proof: Proof::PartlyLiveVerified { on: "2026-09-14", seen: "From KUE's window, recorded in its event log: a spoken request opened an app and a typed request opened a document after confirmation, each succeeded with its read-back (2026-09-15), and a folder opened. KueAct also opened Chrome, Calculator and a PDF with read-back.", not_seen: "Quit, switch, links, notifications, and creating, reading or moving files in ~/KUE, from the window." },
        deliberate: false,
        denial_phrases: &["open files", "open applications", "open apps", "open documents", "open folders", "open a file", "open an application", "open an app", "open a document", "open a folder"],
        claim_phrases: &[],
        action_tags: &["OPEN_APPLICATION", "CLOSE_APPLICATION", "FOCUS_APPLICATION", "OPEN_URL",
            "CREATE_DIRECTORY", "CREATE_FILE", "READ_PERMITTED_FILE", "MOVE_PERMITTED_FILE",
            "SHOW_NOTIFICATION", "OPEN_DOCUMENT", "OPEN_DIRECTORY", "LIST_DIRECTORY"],
    },
    CapabilitySpec {
        id: "in_app_control",
        internal_name: "—",
        name: "Control inside other apps",
        user_name: "Typing and clicking in other apps",
        group: Doing,
        note: "Not implemented. Typing into, clicking in, or reading another application's window needs macOS Accessibility control, which this build neither requests nor uses. A request that needs it is refused whole, before any step starts, rather than half-run.",
        ui_description: "KUE cannot type into, click in, or read another app's window. A request that needs that is refused before anything starts.",
        voice_description: "I can't type into other apps yet, so I won't start a request that needs it.",
        limits: "Not implemented. It would need macOS Accessibility control, which only you can grant.",
        status: Missing,
        risk: Risk::High,
        authorization: Au::Confirmation,
        privacy: &[DataKind::WindowTitle, DataKind::ScreenContent],
        permissions: &[Perm::Accessibility],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        // Only wordings that can only be KUE claiming to act. "click on" and
        // "type into" are how a correct answer gives the owner instructions —
        // "you can click on Confirm" — and correcting that would be a false
        // correction of good advice, the same mistake as matching "the Safari
        // browser" as a claim to browse.
        denial_phrases: &[],
        claim_phrases: &["control your apps", "use your apps for you", "type it for you",
            "click it for you", "type that for you", "click that for you"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "purchasing",
        internal_name: "—",
        name: "Purchases and bookings",
        user_name: "Buying or booking things",
        group: Doing,
        note: "Not implemented. KUE makes no network requests and cannot act inside a website, so it cannot buy, order, book or pay. A request to is answered by rule and nothing starts. When built, a purchase would need your explicit confirmation and macOS authentication for that one purchase.",
        ui_description: "KUE cannot buy, order, book or pay for anything.",
        voice_description: "I can't buy or book anything, so I won't start a request that needs it.",
        limits: "Not implemented.",
        status: Missing,
        risk: Risk::Critical,
        authorization: Au::StrongAuth,
        privacy: &[DataKind::ActionTarget],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["placed the order", "completed the purchase", "booked it for you", "bought it for you"],
        action_tags: &[],
    },
    CapabilitySpec {
        id: "messaging",
        internal_name: "—",
        name: "Sending messages",
        user_name: "Sending messages for you",
        group: Doing,
        note: "Not implemented. KUE cannot send email, texts or messages, or post anything. A request to is answered by rule and nothing starts. When built, a message would be shown to you and sent only after you confirm it.",
        ui_description: "KUE cannot send email, texts or messages, or post anything.",
        voice_description: "I can't send messages or email for you.",
        limits: "Not implemented.",
        status: Missing,
        risk: Risk::High,
        authorization: Au::Confirmation,
        privacy: &[DataKind::ActionTarget],
        permissions: &[],
        verification: V::NotImplemented,
        proof: Proof::NotApplicable,
        deliberate: false,
        denial_phrases: &[],
        claim_phrases: &["sent the email", "sent it for you", "message has been sent", "email has been sent"],
        action_tags: &[],
    },
];

/// The capability rows shown in the panel and given to the model. Generated
/// from the registry, so the two can never disagree.
pub fn rows() -> Vec<Capability> {
    REGISTRY.iter()
        .map(|s| Capability { name: s.name.into(), status: s.status, note: s.note.into() })
        .collect()
}

/// What the model is told about KUE's capabilities — two short lines instead of
/// the whole table.
///
/// Measured 2026-09-20: the full table was 1,319 characters, **48 % of every
/// prompt KUE sent**, on every question, while 81 % of an answer's wall time
/// was prefill. The model does not need the table: the capability question is
/// answered from the registry by rule and never reaches a model, and the
/// over-claim checker corrects answers afterwards from the same rows.
///
/// Derived, so it cannot drift from the registry: what KUE *can* do is the rows
/// that carry action tags — the ones the broker will actually run — and what it
/// *cannot* do is the rows the over-claim checker guards.
pub fn model_lines() -> Vec<String> {
    let can: Vec<&str> = REGISTRY.iter()
        .filter(|s| !s.action_tags.is_empty() && s.status != CapabilityStatus::NotImplemented)
        .map(|s| s.name).collect();
    let cannot: Vec<&str> = REGISTRY.iter()
        .filter(|s| s.status == CapabilityStatus::NotImplemented && !s.claim_phrases.is_empty())
        .map(|s| s.name).collect();
    vec![
        format!("can: {}, and answering from what it senses", can.join(", ")),
        format!("cannot: {}", cannot.join(", ")),
    ]
}

/// One capability as the Capabilities sheet shows it: the person's words, the
/// limits, and whether it can be used right now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityView {
    pub id: &'static str,
    pub group: Group,
    pub name: &'static str,
    pub description: &'static str,
    pub limits: &'static str,
    pub status: CapabilityStatus,
    pub availability: Availability,
    /// Named in the owner's words ("Camera", "Accessibility"), with the pane
    /// only they can grant it in.
    pub permissions: Vec<(&'static str, &'static str)>,
    /// How KUE establishes it did what it said, in a sentence.
    pub verification: &'static str,
    /// What has actually been seen working on this Mac.
    pub proof: Proof,
}

/// The sheet, in reading order, with availability decided from live facts.
pub fn sheet(f: &RuntimeFacts) -> Vec<CapabilityView> {
    REGISTRY.iter().map(|s| CapabilityView {
        id: s.id,
        group: s.group,
        name: s.user_name,
        description: s.ui_description,
        limits: s.limits,
        status: s.status,
        availability: availability(s, f),
        permissions: s.permissions.iter().map(|p| (p.user_name(), p.settings_pane())).collect(),
        verification: match s.verification {
            V::NotApplicable => "Nothing is claimed, so there is nothing to verify.",
            V::Measured => "What the sensing layer measured is the answer.",
            V::SystemState => "KUE checks this Mac afterwards and reports only what it found.",
            V::FileSystem => "KUE reads the folder back afterwards.",
            V::ProcessReport => "The process that did the work reports what happened.",
            V::Arithmetic => "Arithmetic over recorded measurements, which you can see in full.",
            V::NotImplemented => "Not implemented, so there is nothing to verify.",
        },
        proof: s.proof,
    }).collect()
}

pub fn find(id: &str) -> Option<&'static CapabilitySpec> {
    REGISTRY.iter().find(|s| s.id == id)
}

pub fn by_name(name: &str) -> Option<&'static CapabilitySpec> {
    REGISTRY.iter().find(|s| s.name == name)
}

/// The capability that permits an `ActionKind` tag, if any. The Action Broker
/// uses this to refuse an action no implemented capability covers.
pub fn for_action_tag(tag: &str) -> Option<&'static CapabilitySpec> {
    REGISTRY.iter().find(|s| s.action_tags.contains(&tag))
}

/// Capabilities that exist, in reading order.
pub fn implemented() -> impl Iterator<Item = &'static CapabilitySpec> {
    REGISTRY.iter().filter(|s| s.status != Missing)
}

/// Capabilities KUE does not have. These are load-bearing: they are how KUE
/// answers honestly and how an overclaiming model answer is corrected.
pub fn missing() -> impl Iterator<Item = &'static CapabilitySpec> {
    REGISTRY.iter().filter(|s| s.status == Missing)
}

/// The live facts that decide whether an implemented capability is usable now.
/// `None` for a permission means "not yet determined" — which is reported as
/// unknown, never as granted.
#[derive(Debug, Clone, Default)]
pub struct RuntimeFacts {
    pub killed: bool,
    pub paused: bool,
    pub sensing_running: bool,
    pub camera_granted: Option<bool>,
    pub microphone_granted: Option<bool>,
    pub speech_granted: Option<bool>,
    pub notifications_granted: Option<bool>,
    pub accessibility_granted: Option<bool>,
    pub model_available: bool,
    pub voice_available: bool,
}

/// Whether this capability can be used right now, and why not when it cannot.
///
/// Deliberately conservative: an unknown permission reports unavailable with
/// the reason "KUE has not been able to check", never available.
pub fn availability(spec: &CapabilitySpec, f: &RuntimeFacts) -> Availability {
    if spec.status == Missing {
        return Availability::NotImplemented;
    }
    if f.killed && spec.id != "kill_switch" {
        return Availability::Unavailable { reason: "KUE is stopped.".into() };
    }
    for p in spec.permissions {
        let granted = match p {
            Perm::Camera => f.camera_granted,
            Perm::Microphone => f.microphone_granted,
            Perm::SpeechRecognition => f.speech_granted,
            Perm::Notifications => f.notifications_granted,
            Perm::Accessibility => f.accessibility_granted,
        };
        match granted {
            Some(true) => {}
            Some(false) => return Availability::Unavailable {
                reason: format!("macOS has not granted {} access. You can grant it in System Settings → {}.",
                    p.user_name(), p.settings_pane()) },
            None => return Availability::Unavailable {
                reason: format!("KUE has not been able to check whether macOS grants {} access.", p.user_name()) },
        }
    }
    if spec.permissions.contains(&Perm::Camera) && !f.sensing_running {
        return Availability::Unavailable { reason: "KUE's sensing layer is not running.".into() };
    }
    if f.paused && spec.group == Senses {
        return Availability::Unavailable { reason: "KUE is paused, so it is not watching or listening.".into() };
    }
    if spec.id == "on_device_model" && !f.model_available {
        return Availability::Unavailable { reason: "The on-device model is not available right now.".into() };
    }
    if spec.id == "conversation" && !f.model_available {
        return Availability::Degraded { reason: "The on-device model is not available, so KUE can answer only from its own list.".into() };
    }
    if spec.id == "voice_output" && !f.voice_available {
        return Availability::Unavailable { reason: "KUE's voice is not available right now.".into() };
    }
    if spec.status == Partial && !spec.limits.is_empty() {
        return Availability::Degraded { reason: spec.limits.into() };
    }
    Availability::Available
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_date(d: &str) -> bool {
        let b = d.as_bytes();
        b.len() == 10 && b[4] == b'-' && b[7] == b'-'
            && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    }

    /// Existing is not the same as having been seen working, and nothing that
    /// does not exist has been seen working.
    #[test]
    fn proof_agrees_with_status_and_says_what_was_seen() {
        for s in REGISTRY {
            match (s.status, s.proof) {
                (Missing, Proof::NotApplicable) => {}
                (Missing, p) => panic!("{} is not implemented but is marked {}", s.id, p.tag()),
                (_, Proof::NotApplicable) => panic!("{} exists; its proof must say how far it has been checked", s.id),
                (_, Proof::LiveVerified { on, seen }) => {
                    assert!(is_date(on), "{}: {on:?} is not a date", s.id);
                    assert!(seen.split_whitespace().count() >= 6, "{}: say what was seen, not just that it was", s.id);
                }
                (_, Proof::PartlyLiveVerified { on, seen, not_seen }) => {
                    assert!(is_date(on), "{}: {on:?} is not a date", s.id);
                    assert!(seen.split_whitespace().count() >= 6, "{}: say what was seen", s.id);
                    assert!(not_seen.split_whitespace().count() >= 4, "{}: say what has not been seen", s.id);
                }
                (_, Proof::TestVerifiedOnly) => {}
            }
        }
    }

    /// The master status document and the registry are one claim, made twice.
    /// Each proof level has a block in the document listing capability ids in
    /// backticks; the sets must be equal, so neither can say more than the other.
    #[test]
    fn what_the_model_is_told_is_short_and_derived_from_the_registry() {
        let lines = model_lines();
        let total: usize = lines.iter().map(|l| l.len()).sum();
        assert!(total < 600, "the capability line is {total} chars; it was 1,319 and that was 48% of every prompt");
        let joined = lines.join(" ");
        // Derived, not written: every claim the over-claim checker guards is named.
        for s in REGISTRY.iter().filter(|s| s.status == CapabilityStatus::NotImplemented && !s.claim_phrases.is_empty()) {
            assert!(joined.contains(s.name), "the model is not told KUE cannot do {}", s.name);
        }
        // And everything the broker can actually run is named as something it can.
        for s in REGISTRY.iter().filter(|s| !s.action_tags.is_empty() && s.status != CapabilityStatus::NotImplemented) {
            assert!(lines[0].contains(s.name), "the model is not told KUE can {}", s.name);
        }
        assert!(lines[0].starts_with("can: ") && lines[1].starts_with("cannot: "));
    }

    #[test]
    fn the_master_status_lists_the_same_proof_as_the_registry() {
        use std::collections::BTreeSet;
        let doc = include_str!("../../docs/KUE_MASTER_STATUS.md");
        let known: BTreeSet<&str> = REGISTRY.iter().map(|s| s.id).collect();
        let mut seen_anywhere = BTreeSet::new();
        for level in ["LIVE_VERIFIED", "PARTLY_LIVE_VERIFIED", "TEST_VERIFIED_ONLY", "NOT_APPLICABLE"] {
            let (open, close) = (format!("<!-- registry:{level} -->"), format!("<!-- /registry:{level} -->"));
            let block = doc.split(open.as_str()).nth(1).and_then(|b| b.split(close.as_str()).next())
                .unwrap_or_else(|| panic!("KUE_MASTER_STATUS.md has no {level} block"));
            let in_doc: BTreeSet<&str> = block.split('`').skip(1).step_by(2).filter(|t| known.contains(t)).collect();
            let in_registry: BTreeSet<&str> = REGISTRY.iter().filter(|s| s.proof.tag() == level).map(|s| s.id).collect();
            assert_eq!(in_doc, in_registry, "{level}: the document and the registry disagree");
            seen_anywhere.extend(in_doc);
        }
        assert_eq!(seen_anywhere, known, "every capability is in exactly one proof block");
    }

    #[test]
    fn the_sheet_carries_proof_from_the_registry() {
        let f = RuntimeFacts::default();
        for (view, spec) in sheet(&f).iter().zip(REGISTRY) {
            assert_eq!(view.proof, spec.proof, "{}", spec.id);
        }
        let v = serde_json::to_value(&sheet(&f)).unwrap();
        let first = &v[0]["proof"];
        assert!(first["level"].is_string(), "the window reads proof.level: {first}");
        // And the field names the window reads, which serde's rename_all does not touch.
        let partly = v.as_array().unwrap().iter().map(|c| &c["proof"])
            .find(|p| p["level"] == "PARTLY_LIVE_VERIFIED").expect("a partly verified row");
        for field in ["on", "seen", "not_seen"] {
            assert!(partly[field].is_string(), "the window reads proof.{field}: {partly}");
        }
    }

    #[test]
    fn ids_and_names_are_unique() {
        for (i, a) in REGISTRY.iter().enumerate() {
            for b in REGISTRY.iter().skip(i + 1) {
                assert_ne!(a.id, b.id, "duplicate capability id {}", a.id);
                assert_ne!(a.name, b.name, "duplicate capability name {}", a.name);
            }
        }
    }

    /// The mirror of the claim-phrase invariant: a denial phrase may only sit
    /// on a capability KUE HAS. On a missing one it would "correct" an answer
    /// that was telling the owner the truth.
    #[test]
    fn every_denial_phrase_belongs_to_a_capability_kue_has() {
        for s in REGISTRY {
            if s.denial_phrases.is_empty() { continue; }
            assert_ne!(s.status, Missing,
                "{} carries denial phrases but does not exist — correcting a true denial is worse than missing one", s.id);
            assert!(by_name(s.name).is_some(), "{} is unreachable by name", s.id);
            for p in s.denial_phrases {
                assert!(!s.claim_phrases.contains(p), "{}: {p:?} is both a claim and a denial", s.id);
            }
        }
    }

    /// The reason the registry exists. A claim phrase that no longer resolves
    /// to a capability silently stops correcting an overclaiming model answer.
    #[test]
    fn every_claim_phrase_belongs_to_a_missing_capability() {
        for s in REGISTRY {
            if s.claim_phrases.is_empty() { continue; }
            assert_eq!(s.status, Missing,
                "{} carries claim phrases but is not NotImplemented — the checker would never fire, \
                 or would correct a capability KUE actually has", s.id);
            assert!(by_name(s.name).is_some(), "{} is unreachable by name", s.id);
        }
    }

    #[test]
    fn claim_phrases_do_not_overlap_between_capabilities() {
        for (i, a) in REGISTRY.iter().enumerate() {
            for b in REGISTRY.iter().skip(i + 1) {
                for p in a.claim_phrases {
                    assert!(!b.claim_phrases.contains(p),
                        "phrase {p:?} claims both {} and {} — the correction would be ambiguous", a.id, b.id);
                }
            }
        }
    }

    /// No action may execute under a capability that does not exist.
    #[test]
    fn every_action_tag_is_covered_by_one_implemented_capability() {
        for tag in ["OPEN_APPLICATION", "CLOSE_APPLICATION", "FOCUS_APPLICATION", "OPEN_URL",
                    "CREATE_DIRECTORY", "CREATE_FILE", "READ_PERMITTED_FILE", "MOVE_PERMITTED_FILE",
                    "SHOW_NOTIFICATION", "OPEN_DOCUMENT", "OPEN_DIRECTORY", "LIST_DIRECTORY"] {
            let owners: Vec<_> = REGISTRY.iter().filter(|s| s.action_tags.contains(&tag)).collect();
            assert_eq!(owners.len(), 1, "{tag} is covered by {} capabilities, not one", owners.len());
            assert_ne!(owners[0].status, Missing, "{tag} executes under a capability marked not implemented");
        }
    }

    #[test]
    fn a_not_implemented_capability_has_no_executable_path() {
        for s in missing() {
            assert!(s.action_tags.is_empty(), "{} is not implemented but claims actions", s.id);
            assert_eq!(s.verification, V::NotImplemented, "{} cannot verify what it cannot do", s.id);
        }
    }

    #[test]
    fn every_capability_states_its_limits_or_has_none_to_state() {
        for s in REGISTRY {
            assert!(!s.ui_description.is_empty(), "{} has no description for a person", s.id);
            assert!(!s.voice_description.is_empty(), "{} has nothing to say out loud", s.id);
            if s.status == Partial || s.status == Missing {
                assert!(!s.limits.is_empty(), "{} is {:?} and must state its limits", s.id, s.status);
            }
        }
    }

    #[test]
    fn an_unknown_permission_is_never_reported_as_available() {
        let facts = RuntimeFacts { sensing_running: true, model_available: true, voice_available: true, ..Default::default() };
        let camera = find("camera_capture").unwrap();
        match availability(camera, &facts) {
            Availability::Unavailable { reason } => assert!(reason.contains("has not been able to check")),
            other => panic!("unknown camera permission reported as {other:?}"),
        }
    }

    #[test]
    fn a_denied_permission_names_the_settings_pane_the_owner_must_use() {
        let facts = RuntimeFacts { camera_granted: Some(false), sensing_running: true, ..Default::default() };
        match availability(find("camera_capture").unwrap(), &facts) {
            Availability::Unavailable { reason } => {
                assert!(reason.contains("System Settings"));
                assert!(reason.contains("Camera"));
            }
            other => panic!("denied camera permission reported as {other:?}"),
        }
    }

    #[test]
    fn nothing_but_the_kill_switch_is_available_once_kue_is_stopped() {
        let facts = RuntimeFacts { killed: true, camera_granted: Some(true), microphone_granted: Some(true),
            speech_granted: Some(true), notifications_granted: Some(true), accessibility_granted: Some(true),
            sensing_running: true, model_available: true, voice_available: true, ..Default::default() };
        for s in implemented() {
            let a = availability(s, &facts);
            if s.id == "kill_switch" { continue; }
            assert!(matches!(a, Availability::Unavailable { .. }),
                "{} is {:?} while KUE is stopped", s.id, a);
        }
    }

    /// The sheet is a view of the registry, not a second list.
    #[test]
    fn the_sheet_is_the_registry_and_states_every_limit() {
        let facts = RuntimeFacts { camera_granted: Some(true), microphone_granted: Some(true),
            speech_granted: Some(true), notifications_granted: Some(true), accessibility_granted: Some(false),
            sensing_running: true, model_available: true, voice_available: true, ..Default::default() };
        let sheet = sheet(&facts);
        assert_eq!(sheet.len(), REGISTRY.len());
        for (v, s) in sheet.iter().zip(REGISTRY) {
            assert_eq!(v.id, s.id);
            assert_eq!(v.name, s.user_name);
            assert!(!v.description.is_empty());
            assert!(!v.verification.is_empty());
            if s.status == Partial || s.status == Missing { assert!(!v.limits.is_empty(), "{}", s.id); }
        }
        // A permission macOS has not granted is named, with where to grant it.
        let typing = sheet.iter().find(|v| v.id == "in_app_control").unwrap();
        assert_eq!(typing.availability, Availability::NotImplemented);
        assert_eq!(typing.permissions, vec![("Accessibility", "Privacy & Security → Accessibility")]);
    }

    #[test]
    fn the_rows_the_panel_shows_are_the_registry() {
        let rows = rows();
        assert_eq!(rows.len(), REGISTRY.len());
        for (row, spec) in rows.iter().zip(REGISTRY) {
            assert_eq!(row.name, spec.name);
            assert_eq!(row.status, spec.status);
            assert_eq!(row.note, spec.note);
        }
    }
}
