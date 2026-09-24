//! The context object: Lantern's structured representation of the current situation.
//!
//! This — not a chat transcript — is the system's primary artifact.
//!
//! The type system enforces the observation/inference split the whole project
//! rests on: `observations` may only be things a sensor measured, `inferences`
//! are conclusions derived from them and always carry their evidence,
//! `predictions` are explicitly about the future and are never asserted as fact,
//! and `unknowns` is a first-class list of what the system cannot determine.

use crate::evidence::{ConfidenceBreakdown, EvidenceItem};
use serde::{Deserialize, Serialize};

/// Bumped whenever the shape of `ContextObject` changes.
///   1 — initial
///   2 — `identity_check`: the measured separation between you and another person
///   3 — `identity.held_for_seconds`: a claim carried through unmeasurable frames
///   4 — pause covers computer context: activity `PAUSED`, `computer.age_seconds`,
///       `sensors.computer_sampling_reported`, `sensors.readings_after_pause`
///   5 — `conditions`: every named failure state, with its status
///   6 — `resources`: measured CPU and memory, thermal and power state, the
///       analysis-rate policy; capability status `PARTIAL`
///   7 — events carry `provenance`; `remembered_events` from before this launch
///   8 — `environment`: body and hand pose, scene labels, animals, frame brightness
pub const CONTEXT_SCHEMA_VERSION: u32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityState {
    MyFaceConfirmed,
    UnknownPerson,
    IdentityUncertain,
    NoFace,
    MultiplePeople,
    /// The camera is not running, so identity is not merely unknown but unasked.
    NotObserving,
}

impl IdentityState {
    pub fn label(&self) -> &'static str {
        match self {
            IdentityState::MyFaceConfirmed => "MY_FACE_CONFIRMED",
            IdentityState::UnknownPerson => "UNKNOWN_PERSON",
            IdentityState::IdentityUncertain => "IDENTITY_UNCERTAIN",
            IdentityState::NoFace => "NO_FACE",
            IdentityState::MultiplePeople => "MULTIPLE_PEOPLE",
            IdentityState::NotObserving => "NOT_OBSERVING",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActivityState {
    /// A person is visible and the computer has recent input.
    AtComputerInteracting,
    /// A person is visible but there has been no recent input.
    PresentNotInteracting,
    /// Recent input, but nobody is visible to the camera.
    InputWithoutVisiblePerson,
    /// Neither a visible person nor recent input.
    NoActivityDetected,
    /// Not enough working sensors to say anything.
    Unknown,
    /// Sensing is paused by explicit action. Not an inference about you.
    Paused,
}

impl ActivityState {
    pub fn label(&self) -> &'static str {
        match self {
            ActivityState::AtComputerInteracting => "AT_COMPUTER_INTERACTING",
            ActivityState::PresentNotInteracting => "PRESENT_NOT_INTERACTING",
            ActivityState::InputWithoutVisiblePerson => "INPUT_WITHOUT_VISIBLE_PERSON",
            ActivityState::NoActivityDetected => "NO_ACTIVITY_DETECTED",
            ActivityState::Unknown => "UNKNOWN",
            ActivityState::Paused => "PAUSED",
        }
    }

    pub fn human(&self) -> &'static str {
        match self {
            ActivityState::AtComputerInteracting => "At the computer, interacting with it",
            ActivityState::PresentNotInteracting => "Present, but not touching the computer",
            ActivityState::InputWithoutVisiblePerson => "Computer in use, nobody visible to the camera",
            ActivityState::NoActivityDetected => "No activity detected",
            ActivityState::Unknown => "Not enough signal to say",
            ActivityState::Paused => "Paused — nothing is being observed",
        }
    }
}

/// Something a sensor measured. Never a conclusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub statement: String,
    pub source: crate::config::Source,
    /// Age of the underlying measurement, in seconds.
    pub age_seconds: f64,
}

/// A conclusion drawn from observations. Always carries its evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inference {
    pub id: String,
    pub statement: String,
    pub confidence: ConfidenceBreakdown,
    pub evidence_ids: Vec<String>,
}

/// A statement about the future. Never presented as fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub id: String,
    pub statement: String,
    pub basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonTrack {
    pub track_id: String,
    pub frames_tracked: u32,
    pub age_seconds: f64,
    pub capture_quality: Option<f64>,
    pub yaw_deg: Option<f64>,
    pub pitch_deg: Option<f64>,
    pub geometry_distance: Option<f64>,
    pub feature_print_distance: Option<f64>,
    pub descriptor_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityBlock {
    pub state: IdentityState,
    pub confidence: ConfidenceBreakdown,
    /// Plain-language reason, especially when the state is uncertain.
    pub detail: String,
    /// Distance thresholds actually used, derived from measured enrollment spread.
    pub accept_threshold: Option<f64>,
    pub reject_threshold: Option<f64>,
    pub combined_distance: Option<f64>,
    /// Per-descriptor distances, in units of your measured enrollment spread.
    /// Shown separately because averaging two disagreeing signals into one
    /// number would invent a precision the measurements do not have.
    pub geometry_ratio: Option<f64>,
    pub featureprint_ratio: Option<f64>,
    /// False when the descriptors point to different conclusions.
    pub descriptors_agree: bool,
    /// Present while a claim is carried through frames that could not be
    /// measured: seconds since the descriptors were last actually measured.
    pub held_for_seconds: Option<f64>,
    pub enrolled_samples: u32,
    /// True until the matcher has been MEASURED against a non-enrolled face and
    /// both descriptors were found to separate the two people.
    pub reject_side_unvalidated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerBlock {
    pub frontmost_app: Option<String>,
    pub frontmost_bundle_id: Option<String>,
    pub idle_seconds: Option<f64>,
    pub recent_input: Option<bool>,
    pub recent_input_threshold_seconds: f64,
    /// Age of the last computer-activity reading, current or not.
    pub age_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorsBlock {
    pub sensing_process: String,
    pub camera_state: String,
    /// The sensing layer's own last-reported state, never masked by pause.
    pub camera_state_reported: String,
    pub camera_permission: String,
    pub camera_device: Option<String>,
    pub camera_detail: Option<String>,
    pub processed_fps: Option<f64>,
    /// The sensing layer's own report of whether it is sampling computer activity.
    pub computer_sampling_reported: Option<bool>,
    /// Readings that arrived after pause took effect. Anything above zero means
    /// the sensing layer did not stop when asked.
    pub readings_after_pause: u64,
    pub paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityStatus {
    Real,
    /// Works, with material limits stated in its note.
    Partial,
    Simulated,
    Placeholder,
    NotImplemented,
}

/// Measured resource use of one process.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessUsageBlock {
    /// CPU over the most recent measurement interval; 100 is one full core.
    pub cpu_percent: Option<f64>,
    /// Physical memory footprint, as Activity Monitor's "Memory" column counts it.
    pub footprint_mb: Option<f64>,
    pub cpu_seconds_total: Option<f64>,
}

/// A located joint of the most complete body in view. Top-left origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointView {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandView {
    pub chirality: String,
    pub confidence: f64,
    pub bounding_box: Option<crate::sensor::BBox>,
    /// The hand overlaps the (slightly widened) face rectangle.
    pub near_face: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelView {
    pub identifier: String,
    pub confidence: f64,
}

/// What the camera shows beyond faces. Every value is a measurement or a
/// threshold comparison; nothing here says what a posture or a room means.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentBlock {
    pub pose_age_seconds: Option<f64>,
    pub pose_interval_seconds: f64,
    /// None while there is no current pose reading.
    pub upper_body_visible: Option<bool>,
    pub upper_body_joints_located: u32,
    pub body_count: u32,
    pub joints: Vec<JointView>,
    pub hands: Vec<HandView>,
    pub brightness: Option<f64>,
    pub low_light: Option<bool>,
    pub low_light_threshold: f64,
    pub scene_age_seconds: Option<f64>,
    pub scene_interval_seconds: f64,
    pub scene_labels: Vec<LabelView>,
    pub animals: Vec<LabelView>,
    pub errors: Vec<String>,
}

/// What Lantern costs to run, measured rather than estimated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesBlock {
    pub sensing: ProcessUsageBlock,
    pub shell: ProcessUsageBlock,
    /// Average sensing-layer CPU while the camera was observing, over
    /// `observing_seconds_measured` of measured time.
    pub sensing_cpu_observing_percent: Option<f64>,
    pub observing_seconds_measured: f64,
    /// Average sensing-layer CPU while paused, over `paused_seconds_measured`.
    pub sensing_cpu_paused_percent: Option<f64>,
    pub paused_seconds_measured: f64,
    pub thermal_state: Option<String>,
    pub low_power_mode: Option<bool>,
    pub battery_percent: Option<f64>,
    pub power_source: Option<String>,
    pub analysis_fps_target: f64,
    pub analysis_fps_reason: String,
    /// Costs this build does not measure, named so their absence is not read as zero.
    pub not_measured: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConditionStatus {
    /// The failure is happening now.
    Active,
    /// Checked, and not happening.
    Clear,
    /// Cannot happen in this build, because the thing it concerns is not used.
    NotApplicable,
    /// The capability the condition concerns does not exist in this build.
    NotImplemented,
    /// KUE has not been able to check. Not the same as clear: an unknown is
    /// reported as an unknown rather than resolved in KUE's favour.
    Unknown,
}

/// One named failure state and whether it holds right now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub code: String,
    pub status: ConditionStatus,
    pub detail: String,
}

/// One row of the honesty panel: what this build actually does and does not do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub status: CapabilityStatus,
    pub note: String,
}

/// How well one descriptor separated you from a different person, as measured.
///
/// Re-mapped from `sensor::DescriptorSeparation` rather than embedded: the
/// sensing layer's wire format is camelCase and the interface reads snake_case,
/// and an embedded sensor type silently carries the wrong key names across.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptorSeparationBlock {
    pub name: String,
    pub within_owner_min: Option<f64>,
    pub within_owner_median: Option<f64>,
    pub within_owner_max: Option<f64>,
    pub owner_vs_probe_min: Option<f64>,
    pub owner_vs_probe_median: Option<f64>,
    pub owner_vs_probe_max: Option<f64>,
    /// Closest between-person distance ÷ widest within-owner distance.
    pub separation_ratio: Option<f64>,
    /// SEPARATED | OVERLAPPING | INSUFFICIENT_DATA
    pub verdict: String,
}

impl From<&crate::sensor::DescriptorSeparation> for DescriptorSeparationBlock {
    fn from(d: &crate::sensor::DescriptorSeparation) -> Self {
        DescriptorSeparationBlock {
            name: d.name.clone(),
            within_owner_min: d.within_owner_min,
            within_owner_median: d.within_owner_median,
            within_owner_max: d.within_owner_max,
            owner_vs_probe_min: d.owner_vs_probe_min,
            owner_vs_probe_median: d.owner_vs_probe_median,
            owner_vs_probe_max: d.owner_vs_probe_max,
            separation_ratio: d.separation_ratio,
            verdict: d.verdict.clone(),
        }
    }
}

/// The measured answer to "can this matcher tell you apart from someone else?".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCheckBlock {
    pub owner_samples: u32,
    pub probe_samples: u32,
    pub probe_label: String,
    pub geometry: DescriptorSeparationBlock,
    pub feature_print: DescriptorSeparationBlock,
    pub reject_side_validated: bool,
    pub note: String,
}

impl From<&crate::sensor::SeparationReport> for IdentityCheckBlock {
    fn from(r: &crate::sensor::SeparationReport) -> Self {
        IdentityCheckBlock {
            owner_samples: r.owner_samples,
            probe_samples: r.probe_samples,
            probe_label: r.probe_label.clone(),
            geometry: (&r.geometry).into(),
            feature_print: (&r.feature_print).into(),
            reject_side_validated: r.reject_side_validated,
            note: r.note.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityBlock {
    pub state: ActivityState,
    pub label: String,
    pub human: String,
    pub confidence: ConfidenceBreakdown,
    pub evidence_ids: Vec<String>,
}

/// Voice, in the brief's three stages. No audio and no transcript text here.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceBlock {
    /// IDLE / STARTING / LISTENING / FINISHING / PERMISSION_DENIED / NO_MICROPHONE / UNAVAILABLE / ERROR
    pub state: String,
    pub microphone_permission: Option<String>,
    /// VOICE_ACTIVITY: audio energy above a fixed threshold. A level, not a speech classifier.
    pub voice_active: bool,
    pub level_db: Option<f64>,
    /// SPEECH_RECOGNITION: Apple SpeechAnalyzer on this Mac.
    pub speech_recognition: String,
    /// SPEAKER_IDENTITY: always NOT_IMPLEMENTED. A voice grants no authorization.
    pub speaker_identity: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeBlock {
    pub state: crate::runtime::RuntimeState,
    pub killed_at: Option<f64>,
    pub killed_by: Option<crate::runtime::Principal>,
    pub reason: Option<String>,
    /// Where the latch lives. Creating this file from outside the app kills KUE.
    pub latch_path: Option<String>,
    /// Set when a kill could not be saved: killed now, not across a relaunch.
    pub latch_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextObject {
    pub schema_version: u32,
    pub generated_at: f64,
    pub identity: IdentityBlock,
    pub people_detected: u32,
    pub tracks: Vec<PersonTrack>,
    pub activity: ActivityBlock,
    pub computer: ComputerBlock,
    pub sensors: SensorsBlock,
    pub observations: Vec<Observation>,
    pub inferences: Vec<Inference>,
    pub predictions: Vec<Prediction>,
    /// Things this system cannot determine. Kept first-class on purpose.
    pub unknowns: Vec<String>,
    /// Signals that disagree with each other right now.
    pub contradictions: Vec<String>,
    /// Every named failure state, active or not. A failure is reported, never
    /// left for the interface to infer from missing data.
    pub conditions: Vec<Condition>,
    pub resources: ResourcesBlock,
    pub environment: EnvironmentBlock,
    pub evidence: Vec<EvidenceItem>,
    /// Events from this launch, newest first. State changes carry provenance.
    pub recent_events: Vec<crate::events::Event>,
    /// Events read back from local memory, from before this launch.
    pub remembered_events: Vec<crate::events::RememberedEvent>,
    pub capabilities: Vec<Capability>,
    /// KUE_RUNNING / KUE_PAUSED / KUE_KILLED / KUE_RECOVERING, and why.
    pub runtime: RuntimeBlock,
    pub voice: VoiceBlock,
    /// Authorization: access state, LEVEL_0–4, session phase, OS-auth grant.
    pub access: crate::authz::AccessBlock,
    /// The measured separability of the matcher, when an identity check has run.
    pub identity_check: Option<IdentityCheckBlock>,
    pub config_note: String,
}
