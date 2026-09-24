//! The reasoning core.
//!
//! Ingests measurements from the sensing layer, maintains temporal state, and
//! produces the context object.
//!
//! LAYERING RULES enforced here:
//!  * This module never touches a sensor. It only consumes `SensorMessage`.
//!  * It never invents evidence: every `EvidenceItem` traces to a measurement.
//!  * It never invents confidence: all numbers come from `evidence::compute_confidence`.
//!  * It is deterministic: `now` is injected, so the same inputs give the same
//!    output and the whole thing is unit-testable with no camera present.

use crate::config::{Config, Source};
use crate::context::*;
use crate::environment::{Environment, PoseSnapshot, SceneSnapshot};
use crate::events::{Event, EventKind, EventLog, Provenance, RememberedEvent, Stability};
use crate::evidence::{compute_confidence, EvidenceItem, Polarity};
use crate::sensor::*;
use crate::runtime::{KillSwitch, Principal, Refusal, RuntimeState};
use crate::authz::{AccessSession, AccessState, AuthLevel, Decision, IdentityBasis, IdentityObservation, OsAuthKind, Operation};

/// Plain-language reason a face capture was refused, from the sensing layer's code.
fn capture_refusal(code: &str) -> String {
    match code {
        "NO_FACE" => "no face was visible".into(),
        "MULTIPLE_FACES" => "more than one face was visible".into(),
        "LANDMARKS_UNAVAILABLE" => "facial landmarks could not be measured".into(),
        "LOW_CAPTURE_QUALITY" => "capture quality was too low".into(),
        "CAMERA_NOT_RUNNING" => "the camera is not running".into(),
        other => format!("sensing layer reported {other}"),
    }
}

/// Describes an enrollment outcome as what actually happened.
///
/// The sensing layer reports removals and resets through the same message as a
/// refused capture. Rendering all of those as "rejected" once logged a
/// successful removal as "Enrollment sample rejected: REMOVED_1".
fn enrollment_outcome(accepted: bool, reason: &str, count: u32) -> String {
    if accepted {
        return format!("Enrollment sample stored ({count}).");
    }
    if reason == "RESET" {
        return "Enrollment cleared. All stored face descriptors were deleted.".into();
    }
    if let Some(n) = reason.strip_prefix("REMOVED_") {
        return format!("Removed {n} enrollment sample(s); {count} remain.");
    }
    format!("Enrollment sample not stored: {}.", capture_refusal(reason))
}

/// What an activity state asserts: (someone is present, the computer is
/// receiving input). `None` means the state makes no claim about it.
fn activity_claim(state: ActivityState) -> (Option<bool>, Option<bool>) {
    match state {
        ActivityState::AtComputerInteracting => (Some(true), Some(true)),
        ActivityState::PresentNotInteracting => (Some(true), Some(false)),
        ActivityState::InputWithoutVisiblePerson => (Some(false), Some(true)),
        ActivityState::NoActivityDetected => (Some(false), Some(false)),
        ActivityState::Unknown | ActivityState::Paused => (None, None),
    }
}

/// Whether a reading supports or contradicts the displayed statement.
///
/// `claimed` is what the statement asserts about a proposition; `reading` is
/// what was measured, or `None` when the sensor for it is unavailable.
fn bearing(claimed: Option<bool>, reading: Option<bool>) -> Polarity {
    match (claimed, reading) {
        (Some(c), Some(r)) if c == r => Polarity::Supports,
        (Some(_), Some(_)) => Polarity::Contradicts,
        // The statement rests on a signal that is not there.
        (Some(_), None) => Polarity::Contradicts,
        // "Not enough signal to say" is borne out by a missing signal...
        (None, None) => Polarity::Supports,
        // ...and undercut by a working one.
        (None, Some(_)) => Polarity::Contradicts,
    }
}

#[derive(Debug, Clone, Default)]
struct PerceptionSnapshot {
    ts: f64,
    face_count: u32,
    faces: Vec<FaceMeasurement>,
    processed_fps: f64,
}

#[derive(Debug, Clone, Default)]
struct ComputerSnapshot {
    ts: f64,
    frontmost: FrontmostApp,
    idle_seconds: f64,
}

/// How long after a bare invocation the next sentence still counts as the
/// request. Short on purpose: a wake from across the room must not capture a
/// sentence spoken a minute later to somebody else.
pub const WAKE_UTTERANCE_SECONDS: f64 = 12.0;

/// How long a listening session has to open after KUE asks for one. Longer than
/// the shell's half-second tick and the sensing layer's start-up; shorter than
/// the utterance window, so a wake nobody follows up on does not leave KUE deaf
/// for twelve seconds.
pub const WAKE_SESSION_START_SECONDS: f64 = 3.0;

/// Said when KUE is asked to keep something it must not keep now.
pub const MEMORY_KILLED: &str = "I'm stopped, so I'm not keeping anything new.";
pub const MEMORY_PAUSED: &str = "I'm paused, so I won't keep anything new. Resume and tell me again.";

pub struct Engine {
    pub config: Config,
    pub config_note: String,
    events: EventLog,

    perception: Option<PerceptionSnapshot>,
    /// The perception pipeline's own heartbeat. Observation only: no decision
    /// in this build reads it, so identity and authorization are unchanged by
    /// its presence. See `measurement.rs`.
    pipeline: Option<crate::measurement::PipelineHealth>,
    /// Stage timings and perception samples, waiting to be persisted.
    telemetry: crate::telemetry::Telemetry,
    /// What KUE knows, with how it came to know it. A model can read from this
    /// and never write to it: see `facts.rs`.
    facts: crate::facts::Facts,
    /// The conversation and every request in it, however it arrived. The one
    /// owner of "what is KUE doing"; the window projects it (pipeline.rs).
    requests: crate::pipeline::Pipeline,
    /// What KUE keeps about the owner's world, across runs (memory.rs). The
    /// index is authoritative while KUE runs; the store holds the same rows.
    memory: crate::memory::MemoryBook,
    /// Counts the owner's "stop"s. Long-running work notes the count when it
    /// starts and ends early if it changes — so a stop reaches only the work
    /// that was running when it was said, never work started afterwards.
    stop_epoch: u64,
    /// The last measurement state, so only changes are worth a line.
    last_measurement: Option<crate::measurement::MeasurementState>,
    /// When a perception sample was last kept, so a 4 fps camera does not
    /// produce four rows a second forever.
    last_sample_at: f64,
    computer: Option<ComputerSnapshot>,
    camera: CameraStatus,
    enrollment: EnrollmentStats,
    separation: Option<crate::sensor::SeparationReport>,

    sensing_process_up: bool,
    /// The kill switch. While killed, `paused` is also held true so every
    /// existing guard that drops readings applies; `set_paused(false)` is refused.
    kill_switch: crate::runtime::KillSwitch,
    /// Authorization session: LOCKED until the owner is confirmed.
    access: AccessSession,
    access_recorded: Option<(AccessState, AuthLevel)>,
    voice: VoiceBlock,
    /// What the wake boundary last said. Never a transcript: a state, the
    /// phrase, and when it was reported.
    wake: crate::voice::wake::WakeReport,
    /// Requests that arrived by voice after the name was heard, waiting to be
    /// run. In memory, drained by the shell within a tick, never stored.
    wake_requests: Vec<String>,
    /// KUE's own recent sentences, as words (`voice::echo`).
    own_speech: Vec<crate::voice::echo::OwnSentence>,
    /// When the name was heard with nothing after it. The next thing said is
    /// the request, and only for a while: an old wake must not capture a
    /// sentence spoken a minute later to somebody else.
    wake_awaiting: Option<f64>,
    /// Set when the bare invocation was heard, cleared when the shell has been
    /// told to open the listening session that catches the request. One ask per
    /// wake: a tick every half second must not restart the session under the
    /// owner mid-sentence.
    wake_capture_pending: bool,
    /// When that session was asked for. After it has had time to come up, a
    /// microphone that is not open means it never did, or it ended with nothing
    /// said — and KUE goes back to listening for its name rather than standing
    /// deaf for the rest of the window.
    wake_capture_asked: Option<f64>,
    /// When the sensing layer last reported the microphone (engine clock). A
    /// LISTENING that is not refreshed is not believed: see `voice::policy::microphone_busy`.
    voice_reported_at: Option<f64>,
    /// Transcripts waiting for the shell to hand to the conversation. Never in
    /// the context object, never in events, never in memory.
    transcripts: Vec<(u64, String, bool)>,
    paused: bool,
    paused_at: Option<f64>,
    /// Camera or computer readings that arrived after pause had taken effect.
    /// Pause means no sensor data is collected, so any count here is a failure
    /// of the sensing layer to stop — surfaced, never hidden.
    readings_after_pause: u64,
    /// What the sensing layer last said about its own computer-activity sampling.
    computer_sampling_reported: Option<bool>,
    /// The most recent frame Apple Vision failed to analyse: (ts, stage, message).
    last_analysis_failure: Option<(f64, String, String)>,
    /// Set by the shell when local memory cannot be opened or written.
    storage_error: Option<String>,
    /// Events from before this launch, newest first.
    remembered: Vec<RememberedEvent>,

    // Resource measurement.
    sensing_usage: UsageTrack,
    shell_usage: UsageTrack,
    /// (CPU seconds, wall seconds) accumulated while observing and while paused.
    observing_cpu: (f64, f64),
    paused_cpu: (f64, f64),
    thermal_state: Option<String>,
    low_power_mode: Option<bool>,
    battery_percent: Option<f64>,
    power_source: Option<String>,

    // Temporal trackers.
    raw_identity: Stability<IdentityState>,
    public_identity: Stability<IdentityState>,
    activity: Stability<ActivityState>,
    face_present: Stability<bool>,
    /// Tracks the count itself. A bool cannot distinguish one person from two,
    /// so the people-count event needs its own tracker.
    face_count: Stability<u32>,
    recent_input: Stability<bool>,
    frontmost_app: Stability<Option<String>>,
    camera_state: Stability<String>,

    last_face_seen_ts: Option<f64>,
    /// Timestamp of the camera frame identity was last derived from. Derivation
    /// also re-runs on computer-context messages; identity must count frames,
    /// not evaluations.
    last_identity_frame_ts: Option<f64>,
    /// When the descriptors were last actually measured on a single visible face.
    last_measured_identity_ts: Option<f64>,
    /// True while an established claim is carried through frames that could not
    /// be measured. See `update_identity`.
    carrying_identity: bool,
    /// Why the latest evaluation reached its identity conclusion. Authorization
    /// needs this: see `IdentityBasis`.
    frame_basis: IdentityBasis,
    /// Why the frames a claim is currently carried through could not be measured.
    carried_why: IdentityBasis,
    /// The identity state most recently written to the event stream. Display is
    /// live; the event stream records only states that settled (see
    /// `record_settled_identity`).
    identity_recorded: Option<IdentityState>,
    /// States shown, then left before settling, since the last recorded one.
    identity_brief_states: u32,

    /// Body, hands, scene and light.
    environment: Environment,
}

/// Successive CPU-time samples of one process, turned into a rate.
#[derive(Debug, Clone, Default)]
struct UsageTrack {
    last: Option<(f64, f64)>,
    cpu_percent: Option<f64>,
    footprint_bytes: Option<u64>,
}

impl UsageTrack {
    /// Records a sample. Returns the (CPU seconds, wall seconds) elapsed since the
    /// previous one, or None when there is no valid interval — the first sample,
    /// or a counter that went backwards because the process restarted.
    fn record(&mut self, ts: f64, cpu_seconds: f64, footprint_bytes: Option<u64>) -> Option<(f64, f64)> {
        let delta = match self.last {
            Some((t0, c0)) if ts > t0 && cpu_seconds >= c0 => Some((cpu_seconds - c0, ts - t0)),
            _ => None,
        };
        self.cpu_percent = delta.map(|(c, w)| c / w * 100.0);
        self.last = Some((ts, cpu_seconds));
        self.footprint_bytes = footprint_bytes;
        delta
    }

    fn block(&self) -> ProcessUsageBlock {
        ProcessUsageBlock {
            cpu_percent: self.cpu_percent,
            footprint_mb: self.footprint_bytes.map(|b| b as f64 / (1024.0 * 1024.0)),
            cpu_seconds_total: self.last.map(|(_, c)| c),
        }
    }
}

/// What one camera frame can say about identity.
enum FrameIdentity {
    /// A fact that overrides any match, or nothing to judge: applies at once.
    Immediate(IdentityState),
    /// One face is visible but this frame could not be measured — head pose out
    /// of range, capture quality too low, or a descriptor missing. It says
    /// nothing about WHO is there, in either direction. Carries which of those it was.
    Unmeasurable(IdentityBasis),
    /// Both descriptors were measured; this is what they say.
    Measured(IdentityState),
}

impl Engine {
    pub fn new(config: Config, config_note: String) -> Self {
        let access_cfg = config.access.clone();
        Engine {
            config,
            config_note,
            events: EventLog::new(2000),
            perception: None,
            pipeline: None,
            telemetry: crate::telemetry::Telemetry::default(),
            facts: crate::facts::Facts::new(64),
            requests: crate::pipeline::Pipeline::new("c1"),
            memory: crate::memory::MemoryBook::new(500),
            stop_epoch: 0,
            last_measurement: None,
            last_sample_at: 0.0,
            computer: None,
            camera: CameraStatus { state: "NOT_STARTED".into(), permission: "NOT_DETERMINED".into(), ..Default::default() },
            enrollment: EnrollmentStats::default(),
            separation: None,
            sensing_process_up: false,
            kill_switch: crate::runtime::KillSwitch::in_memory(),
            access: AccessSession::new(access_cfg),
            access_recorded: None,
            voice_reported_at: None,
            wake: crate::voice::wake::WakeReport::default(),
            wake_requests: Vec::new(),
            own_speech: Vec::new(),
            wake_awaiting: None,
            wake_capture_pending: false,
            wake_capture_asked: None,
            voice: VoiceBlock { state: "IDLE".into(), speech_recognition: "ON_DEVICE".into(),
                                speaker_identity: "NOT_IMPLEMENTED".into(), ..Default::default() },
            transcripts: Vec::new(),
            paused: false,
            paused_at: None,
            readings_after_pause: 0,
            computer_sampling_reported: None,
            last_analysis_failure: None,
            storage_error: None,
            remembered: Vec::new(),
            sensing_usage: UsageTrack::default(),
            shell_usage: UsageTrack::default(),
            observing_cpu: (0.0, 0.0),
            paused_cpu: (0.0, 0.0),
            thermal_state: None,
            low_power_mode: None,
            battery_percent: None,
            power_source: None,
            raw_identity: Stability::new(),
            public_identity: Stability::new(),
            activity: Stability::new(),
            face_present: Stability::new(),
            face_count: Stability::new(),
            recent_input: Stability::new(),
            frontmost_app: Stability::new(),
            camera_state: Stability::new(),
            last_face_seen_ts: None,
            last_identity_frame_ts: None,
            last_measured_identity_ts: None,
            carrying_identity: false,
            frame_basis: IdentityBasis::NotObserving,
            carried_why: IdentityBasis::NotObserving,
            identity_recorded: None,
            identity_brief_states: 0,
            environment: Environment::default(),
        }
    }

    /// Discards everything derived from computer-activity sampling.
    fn discard_computer(&mut self) {
        self.computer = None;
        self.recent_input = Stability::new();
        self.frontmost_app = Stability::new();
    }

    /// Discards everything derived from the camera. Discarded, not hidden.
    fn discard_perception(&mut self) {
        self.perception = None;
        self.last_face_seen_ts = None;
        self.raw_identity = Stability::new();
        self.public_identity = Stability::new();
        self.face_present = Stability::new();
        self.face_count = Stability::new();
        self.last_identity_frame_ts = None;
        self.last_measured_identity_ts = None;
        self.carrying_identity = false;
        self.frame_basis = IdentityBasis::NotObserving;
        self.carried_why = IdentityBasis::NotObserving;
        // The identity record starts afresh with the next observing session: the
        // disclosure "since the last recorded identity state" must never reach
        // back across a pause, and re-establishing identity is itself an event.
        self.identity_recorded = None;
        self.identity_brief_states = 0;
        self.environment.clear();
    }

    pub fn is_paused(&self) -> bool { self.paused }
    pub fn enrollment_stats(&self) -> &EnrollmentStats { &self.enrollment }

    // What the interface projection (`surface`) reads. Deliberately narrow: the
    // window gets the projection, not these.
    pub fn camera_state(&self) -> &str { &self.camera.state }
    pub fn camera_permission(&self) -> &str { &self.camera.permission }
    pub fn microphone_permission(&self) -> Option<&str> { self.voice.microphone_permission.as_deref() }
    pub fn computer_sampling_reported(&self) -> Option<bool> { self.computer_sampling_reported }
    pub fn samples_needed_for_identity(&self) -> u32 { self.config.identity.minimum_samples_for_identity }

    /// The application in front and how long since you touched the keyboard —
    /// only while the reading is recent enough to present as current, and only
    /// while KUE is observing. The application's name, never what is inside it.
    pub fn computer_now(&self, now: f64) -> Option<(Option<String>, f64)> {
        if self.paused { return None; }
        self.fresh_computer(now).map(|c| (c.frontmost.name.clone(), c.idle_seconds))
    }

    /// How many faces are in view, as the last perception reported.
    pub fn people_in_view(&self) -> usize {
        self.perception.as_ref().map(|p| p.face_count as usize).unwrap_or(0)
    }

    /// What KUE has worked out you are doing, in its own words. `None` when it
    /// has not worked anything out, which is not the same as "nothing".
    pub fn activity_sentence(&self, now: f64) -> Option<String> {
        let (state, _) = self.activity_now(now);
        (!matches!(state, ActivityState::Unknown)).then(|| state.human().to_string())
    }
    pub fn recent_events(&self, n: usize) -> Vec<Event> { self.events.recent(n) }
    pub fn events_since(&self, id: u64) -> Vec<Event> { self.events.since(id) }

    pub fn set_paused(&mut self, paused: bool, now: f64) {
        // Resuming cannot undo a kill. Only `complete_recovery` can.
        if !paused && self.kill_switch.is_killed() { return; }
        if self.paused == paused { return; }
        self.paused = paused;
        if paused { self.requests.pause(now); } else { self.requests.resume(now); }
        if paused {
            self.stop_observing(now);
            self.events.record(now, EventKind::Paused,
                "Paused. Camera and computer-activity sampling stopped; their readings were discarded.", None);
        } else {
            self.paused_at = None;
            self.readings_after_pause = 0;
            self.activity = Stability::new();
            self.events.record(now, EventKind::Resumed, "Resumed by explicit action.", None);
        }
    }

    fn stop_observing(&mut self, now: f64) {
        self.paused = true;
        self.transcripts.clear();
        self.forget_microphone(now, "observing stopped");
        self.discard_perception();
        self.discard_computer();
        self.paused_at = Some(now);
        self.readings_after_pause = 0;
        self.activity = Stability::new();
        self.activity.observe(ActivityState::Paused, now);
    }

    // MARK: Kill switch

    /// Reads the kill latch. Call before the sensing layer is started: if it
    /// says KILLED, nothing must start.
    pub fn attach_kill_latch(&mut self, path: impl Into<std::path::PathBuf>, now: f64) -> RuntimeState {
        self.kill_switch = KillSwitch::with_latch(path, now);
        if self.kill_switch.is_killed() {
            self.stop_observing(now);
            let r = self.kill_switch.record().cloned();
            self.events.record(now, EventKind::Killed,
                "Launched KILLED. The kill latch is set; nothing was started.",
                r.map(|r| format!("killed by {} — {}", r.by.label(), r.reason)));
        }
        self.runtime_state()
    }

    pub fn is_killed(&self) -> bool { self.kill_switch.is_killed() }
    pub fn runtime_state(&self) -> RuntimeState { self.kill_switch.state(self.paused) }

    /// Anyone may kill. Observation stops and every in-memory reading is
    /// discarded, whether or not the latch could be saved.
    pub fn kill(&mut self, by: Principal, reason: &str, now: f64) -> Result<(), String> {
        let already = self.kill_switch.is_killed() && !self.kill_switch.is_recovering();
        let saved = self.kill_switch.kill(by, reason, now);
        self.stop_observing(now);
        // Every request in flight ends KILLED, and nothing stays open for a
        // "yes" to confirm after the kill.
        self.requests.kill(now);
        if !already {
            self.events.record(now, EventKind::Killed,
                format!("KILLED by {}. Sensing stopped and local memory writes are blocked.", by.label()),
                Some(reason.to_string()));
        }
        saved
    }

    pub fn begin_recovery(&mut self, by: Principal, now: f64) -> Result<(), Refusal> {
        self.kill_switch.begin_recovery(by)?;
        self.events.record(now, EventKind::RecoveryBegun,
            "Recovery begun by the owner. KUE stays killed until recovery is completed.", None);
        Ok(())
    }

    pub fn cancel_recovery(&mut self, by: Principal) -> Result<(), Refusal> {
        self.kill_switch.cancel_recovery(by)
    }

    /// Owner only, after `begin_recovery`. Clears the latch. Leaves observation
    /// stopped: the shell starts the sensing layer again, explicitly.
    pub fn complete_recovery(&mut self, by: Principal, now: f64) -> Result<(), Refusal> {
        let record = self.kill_switch.complete_recovery(by)?;
        self.events.record(now, EventKind::Recovered,
            "Recovered by the owner. The kill latch is cleared.",
            Some(format!("had been killed {:.0}s earlier by {} — {}", (now - record.at).max(0.0),
                record.by.label(), record.reason)));
        self.set_paused(false, now);
        Ok(())
    }

    /// A latch created outside the app kills; returns true when that happens.
    pub fn poll_kill_latch(&mut self, now: f64) -> bool {
        if self.kill_switch.poll(now) {
            self.stop_observing(now);
            let r = self.kill_switch.record().cloned();
            self.events.record(now, EventKind::Killed,
                "KILLED from outside the app: the kill latch appeared.",
                r.map(|r| r.reason));
            return true;
        }
        false
    }

    // MARK: Authorization

    /// Feeds the current identity conclusion to the authorization session and
    /// records a change of access state or level. Called after every reading
    /// and on the shell's clock, so timers lapse without frames.
    pub fn tick_access(&mut self, now: f64) {
        let obs = self.identity_observation(now);
        self.access.observe_with(obs, now);
        self.record_access_change(now);
    }

    /// The identity conclusion as of `now`, with why it was reached and the face
    /// track it concerns. The same clock-based overrides as `current_identity`.
    fn identity_observation(&self, now: f64) -> IdentityObservation {
        let identity = self.current_identity(now);
        let basis = match identity {
            IdentityState::NotObserving => IdentityBasis::NotObserving,
            _ if !self.perception_is_fresh(now) => IdentityBasis::StaleReading,
            // A carried claim that lapsed on the clock: the frames since the last
            // measurement measured nothing, and the last one said why.
            IdentityState::IdentityUncertain if self.carrying_identity => self.carried_why,
            _ => self.frame_basis,
        };
        let track = self.perception.as_ref()
            .filter(|p| p.face_count == 1 && basis != IdentityBasis::StaleReading)
            .and_then(|p| p.faces.first())
            .map(|f| f.track_id.clone())
            .filter(|t| t != "untracked");
        IdentityObservation { identity, basis, track }
    }

    /// Records a change of access state or level, with the reason codes needed
    /// to explain it afterwards: the previous state and level, the identity
    /// basis, how long LEVEL_2 had been held without a measurement, and any OS
    /// grant. Codes and durations only — never a distance, a box or a track id.
    fn record_access_change(&mut self, now: f64) {
        let b = self.access.block(now);
        if self.access_recorded != Some((b.state, b.level)) {
            let tag = |v: serde_json::Value| v.as_str().map(String::from).unwrap_or_default();
            let prev = self.access_recorded.replace((b.state, b.level));
            if let Some((ps, pl)) = prev {
                let why = format!("{} [from {} at {}; identity {} ({}){}; OS authentication {}]",
                    b.detail, tag(serde_json::to_value(ps).unwrap_or_default()), tag(serde_json::to_value(pl).unwrap_or_default()),
                    self.current_identity(now).label(), b.basis.tag(),
                    b.held_without_measurement_seconds.map(|s| format!("; held without measurement for {s:.1}s")).unwrap_or_default(),
                    b.os_auth.map(|k| tag(serde_json::to_value(k).unwrap_or_default())).unwrap_or_else(|| "none".into()));
                self.events.record(now, EventKind::AccessChanged,
                    format!("Access: {} at {}.", tag(serde_json::to_value(b.state).unwrap_or_default()),
                        tag(serde_json::to_value(b.level).unwrap_or_default())),
                    Some(why));
            }
        }
    }

    /// The authorization decision for `op`. Every refusal is recorded.
    pub fn authorize(&mut self, op: Operation, by: Principal, now: f64) -> Decision {
        self.tick_access(now);
        let d = self.access.authorize(op, by, self.kill_switch.is_killed(), now);
        if let Decision::Deny(reason) = &d {
            self.events.record(now, EventKind::AuthorizationDenied,
                format!("Denied {} requested by {}.", op.tag(), by.label()), Some(reason.clone()));
        }
        d
    }

    /// What macOS LocalAuthentication returned. `result` is its code: SUCCESS,
    /// or the failure reason. Only SUCCESS creates a grant.
    pub fn record_os_auth(&mut self, kind: OsAuthKind, for_op: Option<Operation>, result: &str, now: f64) {
        let what = match kind { OsAuthKind::Strong => "Touch ID or password", OsAuthKind::Physical => "Touch ID (physical)" };
        let op = for_op.map(|o| format!(" for {}", o.tag())).unwrap_or_default();
        if result == "SUCCESS" {
            self.access.record_os_auth(kind, for_op, now);
            self.events.record(now, EventKind::Authentication, format!("macOS confirmed {what}{op}."), None);
        } else {
            self.events.record(now, EventKind::Authentication, format!("macOS did not confirm {what}{op}."), Some(result.to_string()));
        }
        self.record_access_change(now);
    }

    /// Records that a model was used. Never the question or the answer.
    /// A request refused by the safety boundary. The record carries the kind of
    /// request, never its words.
    pub fn record_refusal(&mut self, concern: crate::safety::Concern, now: f64) {
        self.events.record(now, EventKind::AuthorizationDenied,
            format!("Denied a request by what it asked for ({}). No model was asked and nothing was run.", concern.tag()),
            None);
    }

    /// What the engine would decide for `op` now, without deciding it: no grant
    /// is consumed, nothing is recorded, and no prompt can follow. For telling
    /// the owner where a request stands — never for letting anything run, which
    /// `authorize` decides again at the moment it would.
    pub fn preview_authorization(&self, op: Operation, now: f64) -> Decision {
        let mut session = self.access.clone();
        session.authorize(op, Principal::Owner, self.kill_switch.is_killed(), now)
    }

    /// A request, as the intent router understood it. Its kind and route, never its words.
    pub fn record_intent(&mut self, s: &crate::intent::IntentSummary, now: f64) {
        let categories: Vec<String> = s.categories.iter()
            .map(|c| serde_json::to_value(c).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()).collect();
        let status = serde_json::to_value(&s.status).ok().and_then(|v| v.get("status").and_then(|x| x.as_str()).map(str::to_string)).unwrap_or_default();
        let source = serde_json::to_value(s.source).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        self.events.record(now, EventKind::RequestUnderstood,
            format!("Understood a {source} request as {} ({}): {status}.", s.kind.tag(), categories.join(", ")), None);
    }

    pub fn record_model_event(&mut self, summary: String, detail: Option<String>, now: f64) {
        self.events.record(now, EventKind::ModelInteraction, summary, detail);
    }

    pub fn take_transcripts(&mut self) -> Vec<(u64, String, bool)> { std::mem::take(&mut self.transcripts) }
    pub fn voice_state(&self) -> &str { &self.voice.state }
    /// When the microphone state was last reported, on the engine clock.
    pub fn voice_reported_at(&self) -> Option<f64> { self.voice_reported_at }

    /// What the wake boundary last reported.
    pub fn wake(&self) -> &crate::voice::wake::WakeReport { &self.wake }

    /// Everything that must hold for the microphone to be open for the name.
    /// `owner_enabled` is the owner's setting, which lives in the shell's config.
    pub fn wake_conditions(&self, owner_enabled: bool) -> crate::voice::wake::WakeConditions {
        crate::voice::wake::WakeConditions {
            owner_enabled,
            running: self.sensing_process_up(),
            paused: self.paused,
            killed: self.kill_switch.is_killed(),
            sensing_up: self.sensing_process_up(),
            microphone_granted: self.voice.microphone_permission.as_deref() == Some("AUTHORIZED"),
            // Not yet reported, or never asked: neither granted nor refused.
            microphone_refused: !matches!(self.voice.microphone_permission.as_deref(),
                None | Some("AUTHORIZED") | Some("NOT_DETERMINED")),
        }
    }

    /// The owner turned listening for its name on. A failure KUE would
    /// otherwise wait out — macOS refused, the speech model was missing — is
    /// set aside, so the owner's gesture is what tries again, and tries now.
    /// A listener that is running is left alone.
    pub fn wake_turned_on(&mut self) {
        use crate::voice::wake::WakeState;
        if matches!(self.wake.state, WakeState::PermissionDenied | WakeState::Unavailable | WakeState::Error) {
            self.wake = crate::voice::wake::WakeReport { phrase: self.wake.phrase.clone(), ..Default::default() };
        }
    }

    /// What KUE is saying or has just said, as words, from the speech
    /// controller. Used only to recognise KUE's own voice waking it.
    pub fn set_own_speech(&mut self, own: Vec<crate::voice::echo::OwnSentence>) { self.own_speech = own; }

    /// Forgets the wake listener, for the same reasons the microphone is
    /// forgotten: it stopped, or KUE did.
    pub fn forget_wake(&mut self) {
        self.wake = crate::voice::wake::WakeReport { phrase: self.wake.phrase.clone(), ..Default::default() };
        self.wake_awaiting = None;
        self.wake_capture_pending = false;
        self.wake_capture_asked = None;
        self.wake_requests.clear();
    }

    /// Takes the requests that arrived by voice after the name was heard.
    /// Empty while paused or killed — a request heard before a kill does not
    /// survive it.
    pub fn take_wake_requests(&mut self) -> Vec<String> {
        if self.paused || self.kill_switch.is_killed() { self.wake_requests.clear(); return Vec::new(); }
        std::mem::take(&mut self.wake_requests)
    }

    /// Is KUE waiting for the sentence that follows a bare invocation?
    pub fn wake_awaiting(&self, now: f64) -> bool {
        self.wake_awaiting.is_some_and(|at| now - at <= WAKE_UTTERANCE_SECONDS)
    }

    /// The one thing to do about the microphone this tick. The shell calls this
    /// every tick and does exactly what comes back — the conditions, the state
    /// and the decision all live here, where they can be tested without a
    /// microphone in the room.
    pub fn wake_step(&mut self, owner_enabled: bool, now: f64) -> crate::voice::wake::WakeStep {
        use crate::voice::wake::{self, Awaiting, WakeStep};
        // The ordinary listening session holds the same microphone.
        let busy = !matches!(self.voice.state.as_str(),
            "IDLE" | "PERMISSION_DENIED" | "UNAVAILABLE" | "ERROR" | "NO_MICROPHONE");
        let awaiting = match (self.wake_awaiting(now), self.wake_capture_pending) {
            (true, true) => Awaiting::Request,
            // Asked for, and still within the time a session takes to open. If
            // the microphone is not open by then, it never opened or it closed
            // again on silence — either way KUE listens for its name again
            // instead of waiting out the rest of the window deaf.
            (true, false) if self.wake_capture_asked
                .is_some_and(|at| now - at <= WAKE_SESSION_START_SECONDS) => Awaiting::Asked,
            _ => Awaiting::Nothing,
        };
        let stale = now - self.wake.at > wake::FRESH_SECONDS;
        let step = wake::next_step(self.wake.state, &self.wake_conditions(owner_enabled), busy, awaiting, stale);
        // Asked once, and the clock starts: if no microphone is open when it
        // runs out, the name listener comes back.
        if step == WakeStep::Capture {
            self.wake_capture_pending = false;
            self.wake_capture_asked = Some(now);
        }
        step
    }
    pub fn sensing_process_up(&self) -> bool { self.sensing_process_up }

    /// The microphone is off whenever observing stops or the process that held
    /// it is gone, whatever it last said. Without this a LISTENING from before a
    /// pause, kill or crash would stand forever.
    fn forget_microphone(&mut self, now: f64, why: &str) {
        if matches!(self.voice.state.as_str(), "STARTING" | "LISTENING" | "FINISHING") {
            self.events.record(now, EventKind::VoiceSession, format!("Stopped listening ({why})."), None);
        }
        self.voice.state = "IDLE".into();
        self.voice.voice_active = false;
        self.voice.level_db = None;
        self.voice_reported_at = None;
        self.wake_awaiting = None;
        self.wake_capture_pending = false;
        self.wake_capture_asked = None;
        self.wake_requests.clear();
        // The wake listener holds the same microphone, so it goes with it.
        // Without this, a window could keep saying KUE is listening for its
        // name after the process holding the microphone had gone.
        if self.wake.state != crate::voice::wake::WakeState::Off {
            self.events.record(now, EventKind::VoiceSession, format!("Stopped listening for its name ({why})."), None);
        }
        self.wake = crate::voice::wake::WakeReport { phrase: self.wake.phrase.clone(), ..Default::default() };
    }

    /// Records an action's kind, risk and outcome. Never its target.
    pub fn record_action_event(&mut self, summary: String, now: f64) {
        self.events.record(now, EventKind::Action, summary, None);
    }

    /// One ended speech request, from `voice::speaker::SpeechAudit::summary`. Carries no spoken text.
    pub fn record_speech_event(&mut self, summary: String, now: f64) {
        self.events.record(now, EventKind::SpeechOutput, summary, None);
    }

    pub fn access_block(&self, now: f64) -> crate::authz::AccessBlock { self.access.block(now) }

    pub fn lock_session(&mut self, now: f64) {
        self.access.lock_now("Locked by explicit action.");
        self.record_access_change(now);
    }

    fn runtime_block(&self) -> RuntimeBlock {
        let r = self.kill_switch.record();
        RuntimeBlock {
            state: self.runtime_state(),
            killed_at: r.map(|r| r.at),
            killed_by: r.map(|r| r.by),
            reason: r.map(|r| r.reason.clone()),
            latch_path: self.kill_switch.latch_path().map(|p| p.to_string_lossy().into_owned()),
            latch_error: self.kill_switch.latch_error().map(str::to_string),
        }
    }

    /// Records the shell process's own CPU time and memory footprint.
    pub fn record_shell_usage(&mut self, ts: f64, cpu_seconds: f64, footprint_bytes: u64) {
        self.shell_usage.record(ts, cpu_seconds, Some(footprint_bytes));
    }

    /// The analysis rate the sensing layer should run at right now.
    pub fn desired_fps(&self) -> f64 { self.fps_policy().0 }

    /// How many times slower than configured camera analysis is running now.
    fn rate_stretch(&self) -> f64 {
        let fps = self.fps_policy().0;
        if fps > 0.0 { (self.config.camera.target_fps / fps).max(1.0) } else { 1.0 }
    }

    /// Analysis rates for the sensing layer: (camera fps, pose interval, scene
    /// interval). Pose and scene passes slow down by the same factor as frames.
    pub fn desired_rates(&self) -> (f64, f64, f64) {
        let s = self.rate_stretch();
        let e = &self.config.environment;
        (self.desired_fps(), e.pose_interval_seconds * s, e.scene_interval_seconds * s)
    }

    /// The command fragment carrying the current analysis rates.
    pub fn rates_json(&self) -> String {
        let (fps, pose, scene) = self.desired_rates();
        format!(r#""targetFps":{fps},"poseInterval":{pose},"sceneInterval":{scene}"#)
    }

    /// The analysis rate, and the measured reason for it.
    fn fps_policy(&self) -> (f64, String) {
        let p = &self.config.performance;
        if let Some(t) = self.thermal_state.as_ref().filter(|t| p.reduce_on_thermal_states.contains(t)) {
            return (p.reduced_fps, format!(
                "Reduced to {:.1} fps because macOS reports thermal state {t}.", p.reduced_fps));
        }
        if p.reduce_in_low_power_mode && self.low_power_mode == Some(true) {
            return (p.reduced_fps, format!(
                "Reduced to {:.1} fps because Low Power Mode is on.", p.reduced_fps));
        }
        let full = self.config.camera.target_fps;
        match (&self.thermal_state, self.low_power_mode) {
            (None, None) => (full, format!("{full:.1} fps. Thermal and power state have not been reported yet.")),
            _ => (full, format!("Full rate of {full:.1} fps: no thermal pressure and Low Power Mode is off.")),
        }
    }

    /// The evidence and confidence behind a conclusion, as it will be remembered.
    /// A conclusion that has only just changed has held for no time at all.
    fn provenance(&self, evidence: Vec<EvidenceItem>, stable_seconds: f64) -> Provenance {
        let c = compute_confidence(&evidence, stable_seconds, &self.config);
        Provenance { confidence: c.value, stable_seconds, evidence }
    }

    /// Events read back from local memory at launch, kept apart from this launch's.
    pub fn set_remembered(&mut self, events: Vec<RememberedEvent>) {
        self.remembered = events;
    }

    /// Reports whether local memory is working. `Some(error)` when it is not.
    pub fn set_storage_status(&mut self, error: Option<String>, now: f64) {
        match (&self.storage_error, &error) {
            (None, Some(e)) => { self.events.record(now, EventKind::Error,
                "Local memory unavailable. Events and snapshots are not being saved.", Some(e.clone())); }
            (Some(_), None) => { self.events.record(now, EventKind::Error,
                "Local memory is available again.", None); }
            _ => {}
        }
        self.storage_error = error;
    }

    pub fn set_sensing_process_up(&mut self, up: bool, now: f64) {
        if self.sensing_process_up == up { return; }
        self.sensing_process_up = up;
        if up {
            self.events.record(now, EventKind::SensingStarted, "Sensing layer connected.", None);
        } else {
            self.discard_perception();
            self.discard_computer();
            self.computer_sampling_reported = None;
            self.sensing_usage = UsageTrack::default();
            self.camera = CameraStatus { state: "NOT_STARTED".into(), permission: self.camera.permission.clone(), ..Default::default() };
            self.forget_microphone(now, "the sensing layer stopped");
            self.events.record(now, EventKind::SensingProcessDown,
                "Sensing layer is not running. No perception is available.", None);
        }
    }

    // MARK: - Ingest

    pub fn ingest(&mut self, msg: SensorMessage, now: f64) {
        self.ingest_reading(msg, now);
        self.tick_access(now);
    }

    fn ingest_reading(&mut self, msg: SensorMessage, now: f64) {
        match msg {
            SensorMessage::Hello { build, pid, .. } => {
                self.sensing_process_up = true;
                self.events.record(now, EventKind::SensingStarted,
                    "Sensing layer connected.", Some(format!("{build} (pid {pid})")));
            }
            SensorMessage::Status { camera, enrollment, computer_sampling_active, microphone_permission, .. } => {
                self.computer_sampling_reported = computer_sampling_active;
                if microphone_permission.is_some() { self.voice.microphone_permission = microphone_permission; }
                let prev_samples = self.enrollment.sample_count;
                if self.camera_state.observe(camera.state.clone(), now) && self.camera_state.streak == 1 {
                    let detail = camera.detail.clone();
                    self.events.record(now, EventKind::CameraStateChanged,
                        format!("Camera state: {}", camera.state), detail);
                }
                if enrollment.sample_count != prev_samples {
                    self.events.record(now, EventKind::EnrollmentChanged,
                        format!("Face enrollment now has {} samples.", enrollment.sample_count), None);
                }
                self.camera = camera;
                self.enrollment = enrollment;
            }
            SensorMessage::Perception { ts, face_count, faces, processed_fps, .. } => {
                if self.paused { return self.reading_while_paused("camera frame", now); }
                if face_count > 0 { self.last_face_seen_ts = Some(ts); }
                self.perception = Some(PerceptionSnapshot { ts, face_count, faces, processed_fps });
                self.update_derived(now);
            }
            SensorMessage::SenseHealth { ts, capture_running, vision_busy, loop_alive,
                                         last_capture_at, last_analyzed_at, analyze_ms_last,
                                         analyze_ms_p50, analyze_ms_max, capture_gap_ms_max,
                                         frames_captured, frames_analyzed, frames_dropped } => {
                // Accepted while paused, like KUE's measurement of its own cost:
                // it says how the pipeline is, never who is there. It is not a
                // reading about a person, so `reading_while_paused` does not
                // apply and nothing about identity or access consults it.
                self.pipeline = Some(crate::measurement::PipelineHealth {
                    ts: now, capture_running, vision_busy, loop_alive,
                    last_capture_at, last_analyzed_at,
                    analyze_ms_last, analyze_ms_p50, analyze_ms_max, capture_gap_ms_max,
                    frames_captured, frames_analyzed, frames_dropped,
                });
                if analyze_ms_last > 0.0 {
                    self.telemetry.record(crate::telemetry::Span::new(
                        crate::telemetry::Stage::VisionAnalyze, "frame",
                        now - analyze_ms_last / 1000.0, now, crate::telemetry::SpanOutcome::Ok));
                }
                let _ = ts;
            }
            SensorMessage::Health { ts, cpu_seconds, footprint_bytes, thermal_state,
                                    low_power_mode, battery_percent, power_source } => {
                // Lantern measuring itself: accepted while paused, and never counted
                // as a reading about the person.
                if let Some((cpu, wall)) = self.sensing_usage.record(ts, cpu_seconds, footprint_bytes) {
                    if self.paused {
                        self.paused_cpu.0 += cpu; self.paused_cpu.1 += wall;
                    } else if self.camera.state == "RUNNING" {
                        self.observing_cpu.0 += cpu; self.observing_cpu.1 += wall;
                    }
                }
                let before = self.fps_policy();
                self.thermal_state = thermal_state;
                self.low_power_mode = low_power_mode;
                self.battery_percent = battery_percent;
                self.power_source = power_source;
                let after = self.fps_policy();
                if after.0 != before.0 {
                    let verb = if after.0 < before.0 { "reduced" } else { "restored" };
                    self.events.record(now, EventKind::AnalysisRateChanged,
                        format!("Camera analysis rate {verb} to {:.1} fps.", after.0), Some(after.1));
                }
            }
            SensorMessage::Pose { ts, bodies, hands, brightness, error } => {
                if self.paused { return self.reading_while_paused("body and hand pose reading", now); }
                self.environment.pose = Some(PoseSnapshot { ts, bodies, hands, brightness, error });
                // A visible body can keep "present" from collapsing on a face miss.
                self.update_derived(now);
            }
            SensorMessage::Voice { state, level_db, voice_active, microphone_permission, detail, .. } => {
                let listening = state == "LISTENING";
                if listening && (self.paused || self.kill_switch.is_killed()) {
                    return self.reading_while_paused("microphone level", now);
                }
                if state != self.voice.state {
                    let summary = match state.as_str() {
                        "LISTENING" => "Listening (push-to-talk). Audio is analysed in memory only.".to_string(),
                        "IDLE" if self.voice.state != "STARTING" => format!("Stopped listening ({}).", detail.clone().unwrap_or_default()),
                        "PERMISSION_DENIED" => "Microphone access is denied.".to_string(),
                        "UNAVAILABLE" | "ERROR" | "NO_MICROPHONE" => format!("Listening failed: {state}."),
                        _ => String::new(),
                    };
                    if !summary.is_empty() {
                        self.events.record(now, EventKind::VoiceSession, summary, detail.clone());
                    }
                }
                self.voice.state = state;
                self.voice_reported_at = Some(now);
                self.voice.level_db = if listening { level_db } else { None };
                self.voice.voice_active = listening && voice_active;
                if microphone_permission.is_some() { self.voice.microphone_permission = microphone_permission; }
                self.voice.detail = detail;
            }
            SensorMessage::Wake { ts: _, state, phrase, confidence, heard, microphone_permission, detail } => {
                // A wake while KUE is paused or stopped is not a wake. The
                // listener should not be running at all then, and if a report
                // arrives from one that has not stopped yet, it is refused here
                // too rather than trusted.
                if self.paused || self.kill_switch.is_killed() {
                    self.wake = crate::voice::wake::WakeReport { phrase: self.wake.phrase.clone(), ..Default::default() };
                    // The listener saying it is OFF is not listening: the
                    // sensing layer answers a start while paused with OFF, and
                    // reports OFF when it is stopped as KUE quits. Only a
                    // report that it is (still) listening means pause did not
                    // stop it. (A false alarm of exactly this kind was logged
                    // live on 2026-09-22, as the app quit while paused.)
                    if crate::voice::wake::WakeState::from_tag(&state) == crate::voice::wake::WakeState::Off { return; }
                    return self.reading_while_paused("listening for its name", now);
                }
                let state = crate::voice::wake::WakeState::from_tag(&state);
                // Only state CHANGES are recorded, and the record carries no
                // words: a line each time someone in the room spoke would be a
                // record of when people were talking.
                if state != self.wake.state {
                    let summary = match state {
                        crate::voice::wake::WakeState::Waiting =>
                            format!("Listening for “{}”. Nothing is recognised until someone speaks, and nothing leaves unless the name is heard.",
                                if phrase.is_empty() { self.wake.phrase.clone() } else { phrase.clone() }),
                        crate::voice::wake::WakeState::Woke => "Heard its name.".to_string(),
                        crate::voice::wake::WakeState::Off => "Stopped listening for its name.".to_string(),
                        crate::voice::wake::WakeState::PermissionDenied => "Microphone access is denied.".to_string(),
                        crate::voice::wake::WakeState::Unavailable | crate::voice::wake::WakeState::Error =>
                            "Could not listen for its name.".to_string(),
                        crate::voice::wake::WakeState::Starting => String::new(),
                    };
                    if !summary.is_empty() {
                        self.events.record(now, EventKind::VoiceSession, summary, detail.clone());
                    }
                }
                self.wake = crate::voice::wake::WakeReport {
                    state,
                    phrase: if phrase.is_empty() { self.wake.phrase.clone() } else { phrase },
                    at: now,
                    confidence,
                    detail,
                };
                if microphone_permission.is_some() { self.voice.microphone_permission = microphone_permission; }
                // What was said in the same breath becomes a request, handled
                // exactly like a typed one — authorized, checked and verified.
                // Hearing a name is not permission to do anything.
                if state == crate::voice::wake::WakeState::Woke
                    && crate::voice::echo::is_own_speech(&self.wake.phrase, heard.as_deref().unwrap_or(""), &self.own_speech, now) {
                    // KUE heard its own sentence come back through the
                    // microphone. Not a request and not a reason to open a
                    // listening session — which would also cut KUE off
                    // mid-sentence. The record carries no words.
                    self.events.record(now, EventKind::VoiceSession,
                        "Heard its own voice say its name. Nothing was done.", None::<String>);
                    self.wake.state = crate::voice::wake::WakeState::Off;
                } else if state == crate::voice::wake::WakeState::Woke {
                    match heard.map(|h| h.trim().to_string()).filter(|h| !h.is_empty()) {
                        Some(request) => {
                            self.wake_awaiting = None;
                            self.wake_capture_pending = false;
                            self.wake_capture_asked = None;
                            if self.wake_requests.len() < 4 { self.wake_requests.push(request); }
                        }
                        // Only the name: the next thing said is the request, and
                        // it needs a listening session to be said into.
                        None => {
                            self.wake_awaiting = Some(now);
                            self.wake_capture_pending = true;
                            self.wake_capture_asked = None;
                        }
                    }
                    // A wake ENDS the wake session. The boundary hands the
                    // microphone over and stops without saying so again
                    // (`Wake.swift`, `stop(reason: "WOKE")`), so KUE records the
                    // stop itself rather than waiting for a report that never
                    // comes. Without this the listener never starts again and
                    // hands-free works exactly once per launch.
                    self.wake.state = crate::voice::wake::WakeState::Off;
                }
            }
            SensorMessage::Transcript { session, text, is_final } => {
                if self.paused || self.kill_switch.is_killed() { return; }
                // Said right after the name, with nothing else in between: the
                // request KUE was woken for, so it does not have to be asked
                // for twice. Anything later is an ordinary transcript and waits
                // for the owner to send it.
                if is_final && self.wake_awaiting.is_some_and(|at| now - at <= WAKE_UTTERANCE_SECONDS) {
                    self.wake_awaiting = None;
                    self.wake_capture_pending = false;
                    self.wake_capture_asked = None;
                    let request = text.trim().to_string();
                    if !request.is_empty() && self.wake_requests.len() < 4 { self.wake_requests.push(request); }
                }
                self.transcripts.push((session, text, is_final));
                if self.transcripts.len() > 50 { self.transcripts.remove(0); }
            }
            SensorMessage::Scene { ts, labels, animals, error } => {
                if self.paused { return self.reading_while_paused("scene reading", now); }
                self.environment.scene = Some(SceneSnapshot { ts, labels, animals, error });
            }
            SensorMessage::AnalysisFailed { ts, stage, message } => {
                if self.paused { return self.reading_while_paused("frame analysis", now); }
                // Record the transition into failure, not every failed frame.
                let already = self.model_unavailable().is_some();
                self.last_analysis_failure = Some((ts, stage.clone(), message.clone()));
                if !already {
                    self.events.record(now, EventKind::Error,
                        format!("Apple Vision could not analyse camera frames ({stage})."), Some(message));
                }
                self.update_derived(now);
            }
            SensorMessage::Computer { ts, frontmost_app, idle_seconds } => {
                if self.paused { return self.reading_while_paused("computer-activity sample", now); }
                let name = frontmost_app.name.clone();
                if self.frontmost_app.observe(name.clone(), ts) && self.frontmost_app.streak == 1 {
                    if let Some(n) = &name {
                        self.events.record(ts, EventKind::FrontmostAppChanged,
                            format!("{n} became the frontmost application."), None);
                    }
                }
                // A negative idle time means the HID idle timer could not be read.
                // That is an unavailable reading, not an idle computer.
                if idle_seconds < 0.0 {
                    self.recent_input = Stability::new();
                }
                let recent = idle_seconds < self.config.computer.recent_input_seconds;
                if idle_seconds >= 0.0 && self.recent_input.observe(recent, ts) && self.recent_input.streak == 1 {
                    self.events.record(ts, EventKind::InputActivityChanged,
                        if recent { "Recent keyboard or mouse input detected." }
                        else { "No recent keyboard or mouse input." },
                        Some(format!("{idle_seconds:.0}s since last input (threshold {:.0}s)",
                            self.config.computer.recent_input_seconds)));
                }
                self.computer = Some(ComputerSnapshot { ts, frontmost: frontmost_app, idle_seconds });
                self.update_derived(now);
            }
            SensorMessage::EnrollCaptured { accepted, reason, enrollment } => {
                self.enrollment = enrollment;
                let summary = enrollment_outcome(accepted, &reason, self.enrollment.sample_count);
                self.events.record(now, EventKind::EnrollmentChanged, summary, None);
            }
            SensorMessage::Error { code, message } => {
                self.events.record(now, EventKind::Error, format!("Sensing error: {code}"), Some(message));
            }
            SensorMessage::SeparationReport { report } => {
                // The capture itself is recorded by ProbeCaptured. Record the report
                // only when its conclusion changes, so the timeline carries findings
                // rather than a line per recomputation.
                let verdict = |r: &SeparationReport| (r.reject_side_validated,
                    r.geometry.verdict.clone(), r.feature_print.verdict.clone());
                let changed = self.separation.as_ref().map(verdict) != Some(verdict(&report));
                if changed && report.probe_samples > 0 {
                    self.events.record(now, EventKind::IdentityCheckChanged,
                        format!("Identity check: geometry {}, feature print {}.",
                            report.geometry.verdict, report.feature_print.verdict),
                        Some(report.note.clone()));
                }
                self.separation = Some(report);
            }
            SensorMessage::ProbeCaptured { accepted, reason, probe_count } => {
                let summary = match (accepted, reason.as_str()) {
                    (true, _) => format!(
                        "Identity check sample {probe_count} captured (not added to your profile)."),
                    (false, "RESET") => "Identity check samples cleared.".to_string(),
                    (false, r) => format!("Identity check sample not captured: {}.", capture_refusal(r)),
                };
                self.events.record(now, EventKind::IdentityCheckChanged, summary, None);
            }
            SensorMessage::Devices { .. } | SensorMessage::Pong { .. } => {}
        }
    }

    /// A reading arrived while paused. It is dropped regardless. One already in
    /// flight when pause was sent is expected; anything later means the sensing
    /// layer did not stop sampling, and is recorded as such.
    fn reading_while_paused(&mut self, what: &str, now: f64) {
        let Some(at) = self.paused_at else { return };
        let late = now - at;
        if late <= self.config.pause.in_flight_grace_seconds { return; }
        self.readings_after_pause += 1;
        if self.readings_after_pause == 1 {
            self.events.record(now, EventKind::Error,
                format!("A {what} arrived {late:.1}s after pause took effect. The sensing layer did not stop sampling."),
                Some("The reading was discarded.".into()));
        }
    }

    /// Why Apple Vision is failing, while the most recent camera result is a
    /// failure rather than an analysed frame.
    fn model_unavailable(&self) -> Option<(&str, &str)> {
        let (ts, stage, message) = self.last_analysis_failure.as_ref()?;
        let analysed_since = self.perception.as_ref().map(|p| p.ts > *ts).unwrap_or(false);
        (!analysed_since && !self.paused).then_some((stage.as_str(), message.as_str()))
    }

    /// The last computer-activity reading, only while it is recent enough to be
    /// presented as current.
    fn fresh_computer(&self, now: f64) -> Option<&ComputerSnapshot> {
        self.computer.as_ref()
            .filter(|c| now - c.ts <= self.config.computer.observation_stale_seconds)
    }

    // MARK: - Derived temporal state

    fn perception_is_fresh(&self, now: f64) -> bool {
        self.perception.as_ref()
            .map(|p| now - p.ts <= self.config.camera.observation_stale_seconds)
            .unwrap_or(false)
    }

    fn camera_observing(&self, now: f64) -> bool {
        !self.paused && self.camera.state == "RUNNING" && self.perception_is_fresh(now)
    }

    /// No face is detected, but an upper body is, and config lets that count.
    fn body_stands_in_for_face(&self, now: f64) -> bool {
        self.config.environment.body_counts_as_presence
            && self.face_present_now(now) == Some(false)
            && self.environment.upper_body_visible(&self.config.environment, now, self.rate_stretch()) == Some(true)
    }

    /// Whether someone is in front of the camera: a face, or failing that a body.
    fn person_present_now(&self, now: f64) -> Option<bool> {
        if self.body_stands_in_for_face(now) { Some(true) } else { self.face_present_now(now) }
    }

    /// Some(true/false) when the camera is actually looking; None when it is not.
    fn face_present_now(&self, now: f64) -> Option<bool> {
        if !self.camera_observing(now) { return None; }
        let p = self.perception.as_ref()?;
        if p.face_count > 0 { return Some(true); }
        // A brief dropout should not immediately read as "gone".
        match self.last_face_seen_ts {
            Some(t) if now - t <= self.config.activity.face_absent_grace_seconds => Some(true),
            _ => Some(false),
        }
    }

    fn update_derived(&mut self, now: f64) {
        let present = self.face_present_now(now);
        if let Some(p) = present {
            if self.face_present.observe(p, now) && self.face_present.streak == 1 {
                self.events.record(now,
                    if p { EventKind::FaceAppeared } else { EventKind::FaceDisappeared },
                    if p { "A face became visible to the camera." }
                    else { "No face is visible to the camera." }, None);
            }
        }

        if let Some(n) = self.perception.as_ref().map(|p| p.face_count) {
            if self.face_count.observe(n, now) && self.face_count.streak == 1 && n > 1 {
                self.events.record(now, EventKind::PeopleCountChanged,
                    format!("{n} faces are now visible."), None);
            }
        }

        self.update_identity(now);

        let act = self.infer_activity_state(now);
        if self.activity.observe(act, now) && self.activity.streak == 1 {
            let why = self.provenance(self.activity_evidence(now), 0.0);
            self.events.record_with(now, EventKind::ActivityStateChanged,
                format!("Activity: {}", act.human()), None, Some(why));
        }
    }

    /// Claims require corroboration over several frames; non-claims apply at once.
    ///
    /// This asymmetry is deliberate. Promoting to MY_FACE_CONFIRMED or
    /// UNKNOWN_PERSON is an assertion about a person, so it must be earned.
    /// Dropping to NO_FACE must be instant, otherwise covering the lens would
    /// leave a stale "confirmed" on screen — which would be a lie.
    fn promote_identity(&self, raw: IdentityState) -> IdentityState {
        let needs_corroboration = matches!(raw, IdentityState::MyFaceConfirmed | IdentityState::UnknownPerson);
        if !needs_corroboration {
            return raw;
        }
        if self.raw_identity.streak >= self.config.identity.confirm_frames {
            raw
        } else {
            IdentityState::IdentityUncertain
        }
    }

    /// What the latest camera frame can say about identity.
    fn classify_frame(&self, now: f64) -> FrameIdentity {
        use FrameIdentity::*;
        if self.paused || !self.sensing_process_up { return Immediate(IdentityState::NotObserving); }
        if self.camera.state != "RUNNING" { return Immediate(IdentityState::NotObserving); }
        if !self.perception_is_fresh(now) { return Immediate(IdentityState::IdentityUncertain); }

        let p = match &self.perception { Some(p) => p, None => return Immediate(IdentityState::NotObserving) };
        if p.face_count == 0 { return Immediate(IdentityState::NoFace); }
        // More than one face outranks any match: we will not name a person in a crowd.
        if p.face_count > 1 { return Immediate(IdentityState::MultiplePeople); }

        let cfg = &self.config.identity;
        if self.enrollment.sample_count < cfg.minimum_samples_for_identity {
            return Immediate(IdentityState::IdentityUncertain);
        }
        let f = &p.faces[0];
        if f.descriptor_status != "OK" { return Unmeasurable(IdentityBasis::DescriptorUnavailable); }
        if f.capture_quality.map(|q| q < cfg.min_capture_quality).unwrap_or(true) {
            return Unmeasurable(IdentityBasis::LowCaptureQuality);
        }
        if f.yaw_deg.map(|y| y.abs() > cfg.max_abs_yaw_deg).unwrap_or(false)
            || f.pitch_deg.map(|y| y.abs() > cfg.max_abs_pitch_deg).unwrap_or(false) {
            return Unmeasurable(IdentityBasis::HeadPose);
        }

        let (g, p_) = self.descriptor_ratios(f);
        let available: Vec<f64> = [g, p_].into_iter().flatten().collect();

        // A claim about a person needs corroboration. One descriptor is never
        // enough, and descriptors that disagree cancel rather than average.
        if (available.len() as u32) < cfg.min_descriptors_for_claim {
            return Unmeasurable(IdentityBasis::DescriptorUnavailable);
        }
        if available.iter().all(|d| *d <= cfg.accept_ratio) {
            Measured(IdentityState::MyFaceConfirmed)
        } else if available.iter().all(|d| *d >= cfg.reject_ratio) {
            Measured(IdentityState::UnknownPerson)
        } else {
            Measured(IdentityState::IdentityUncertain)
        }
    }

    /// Advances the identity trackers by one evaluation.
    ///
    /// Claims (MY_FACE_CONFIRMED, UNKNOWN_PERSON) are earned over
    /// `confirm_frames` measured frames. Once earned, a claim is CARRIED through
    /// frames that could not be measured for at most `hold_unmeasurable_seconds`
    /// since the last measurement — such a frame is not evidence about who is
    /// there, so it neither extends nor breaks the claim. A frame that is measured
    /// and disagrees demotes at once, and NO_FACE, MULTIPLE_PEOPLE and
    /// NOT_OBSERVING always apply immediately, so a carried claim can never
    /// survive the face leaving or someone else arriving.
    fn update_identity(&mut self, now: f64) {
        let frame_ts = self.perception.as_ref().map(|p| p.ts);
        let new_frame = frame_ts.is_some() && frame_ts != self.last_identity_frame_ts;
        let uncertain = IdentityState::IdentityUncertain;

        let promoted = match self.classify_frame(now) {
            FrameIdentity::Immediate(s) => {
                self.frame_basis = match s {
                    IdentityState::NoFace => IdentityBasis::NoFace,
                    IdentityState::MultiplePeople => IdentityBasis::MultiplePeople,
                    IdentityState::NotObserving => IdentityBasis::NotObserving,
                    _ if !self.perception_is_fresh(now) => IdentityBasis::StaleReading,
                    _ => IdentityBasis::NotEnrolled,
                };
                self.carrying_identity = false;
                self.last_measured_identity_ts = None;
                if new_frame || self.raw_identity.value() != Some(&s) {
                    self.raw_identity.observe(s, now);
                }
                s
            }
            FrameIdentity::Measured(s) => {
                if new_frame {
                    self.raw_identity.observe(s, now);
                    self.last_measured_identity_ts = Some(now);
                    self.carrying_identity = false;
                }
                let promoted = self.promote_identity(s);
                self.frame_basis = match (s, promoted) {
                    (IdentityState::MyFaceConfirmed, IdentityState::MyFaceConfirmed) => IdentityBasis::MeasuredMatch,
                    (IdentityState::MyFaceConfirmed, _) => IdentityBasis::PendingCorroboration,
                    (IdentityState::UnknownPerson, IdentityState::UnknownPerson) => IdentityBasis::MeasuredStranger,
                    _ => IdentityBasis::MeasuredConflict,
                };
                promoted
            }
            FrameIdentity::Unmeasurable(why) => {
                let held = self.public_identity.value().copied()
                    .filter(|s| matches!(s, IdentityState::MyFaceConfirmed | IdentityState::UnknownPerson));
                match held {
                    Some(claim) if self.hold_is_open(now) => {
                        self.carrying_identity = true;
                        self.carried_why = why;
                        self.frame_basis = if claim == IdentityState::MyFaceConfirmed { IdentityBasis::CarriedMatch }
                                           else { IdentityBasis::MeasuredStranger };
                        claim
                    }
                    _ => {
                        self.carrying_identity = false;
                        self.frame_basis = why;
                        if new_frame || self.raw_identity.value() != Some(&uncertain) {
                            self.raw_identity.observe(uncertain, now);
                        }
                        uncertain
                    }
                }
            }
        };
        if new_frame { self.last_identity_frame_ts = frame_ts; }

        let min = self.config.identity.event_min_seconds;
        let leaving = self.public_identity.value().copied();
        let held = self.public_identity.stable_seconds(now);
        if self.public_identity.observe(promoted, now) && self.public_identity.streak == 1 {
            if let Some(prev) = leaving {
                if held < min && Some(prev) != self.identity_recorded {
                    self.identity_brief_states += 1;
                }
            }
        }
        self.record_settled_identity(now);
    }

    /// Writes the identity state to the event stream once it has held for
    /// `identity.event_min_seconds`, dated from when it began.
    ///
    /// The DISPLAY changes on the frame that changes it — NO_FACE still applies
    /// at once. Memory is for meaningful events: a state Vision produced for one
    /// frame is not one. Nothing is hidden, though: the states that came and went
    /// in between are counted in the next recorded event.
    fn record_settled_identity(&mut self, now: f64) {
        let Some(current) = self.public_identity.value().copied() else { return };
        let held = self.public_identity.stable_seconds(now);
        let min = self.config.identity.event_min_seconds;
        if Some(current) == self.identity_recorded || held < min { return; }

        let detail = (self.identity_brief_states > 0).then(|| format!(
            "{} brief state change(s) since the last recorded identity state did not hold for {min:.1}s \
             and were not recorded individually.", self.identity_brief_states));
        let why = self.provenance(self.identity_evidence(now), held);
        self.events.record_with(now - held, EventKind::IdentityStateChanged,
            format!("Identity state: {}", current.label()), detail, Some(why));
        self.identity_recorded = Some(current);
        self.identity_brief_states = 0;
    }

    /// True while a claim may still be carried: the last real measurement is
    /// within `hold_unmeasurable_seconds`.
    fn hold_is_open(&self, now: f64) -> bool {
        self.last_measured_identity_ts
            .map(|t| now - t <= self.config.identity.hold_unmeasurable_seconds)
            .unwrap_or(false)
    }

    /// Seconds since the carried claim was last measured, while one is carried.
    fn carried_for(&self, now: f64) -> Option<f64> {
        if !self.carrying_identity || !self.hold_is_open(now) { return None; }
        self.last_measured_identity_ts.map(|t| (now - t).max(0.0))
    }

    /// Per-descriptor distances expressed in units of your own measured
    /// enrollment spread. Returned separately, never pre-averaged: when the two
    /// disagree that disagreement is the finding, not something to smooth over.
    fn descriptor_ratios(&self, f: &FaceMeasurement) -> (Option<f64>, Option<f64>) {
        let cfg = &self.config.identity;
        let geo_ref = self.enrollment.geometry_self_p95.unwrap_or(0.0).max(cfg.min_geometry_reference);
        let fp_ref = self.enrollment.feature_print_self_p95.unwrap_or(0.0).max(cfg.min_featureprint_reference);
        (f.geometry_distance.map(|d| d / geo_ref), f.feature_print_distance.map(|d| d / fp_ref))
    }

    /// True when both descriptors are present and land on the same side of the
    /// accept/reject boundaries.
    fn descriptors_agree(&self, f: &FaceMeasurement) -> bool {
        let cfg = &self.config.identity;
        let (g, p_) = self.descriptor_ratios(f);
        match (g, p_) {
            (Some(a), Some(b)) => {
                let side = |d: f64| if d <= cfg.accept_ratio { 0 } else if d >= cfg.reject_ratio { 2 } else { 1 };
                side(a) == side(b)
            }
            _ => false,
        }
    }

    /// Distance expressed in units of your own measured enrollment spread.
    /// 1.0 means "as far from the enrolled samples as they typically are from
    /// each other". Returns None when neither descriptor is usable.
    fn combined_distance(&self, f: &FaceMeasurement) -> Option<f64> {
        let cfg = &self.config.identity;
        let geo_ref = self.enrollment.geometry_self_p95.unwrap_or(0.0).max(cfg.min_geometry_reference);
        let fp_ref = self.enrollment.feature_print_self_p95.unwrap_or(0.0).max(cfg.min_featureprint_reference);

        let g = f.geometry_distance.map(|d| d / geo_ref);
        let p = f.feature_print_distance.map(|d| d / fp_ref);
        match (g, p) {
            (Some(g), Some(p)) => {
                let (gw, pw) = (cfg.geometry_weight, cfg.featureprint_weight);
                Some((gw * g + pw * p) / (gw + pw))
            }
            (Some(g), None) => Some(g),
            (None, Some(p)) => Some(p),
            (None, None) => None,
        }
    }

    /// The identity state as of `now`.
    ///
    /// Reads the cached temporal state ONLY while the camera is genuinely
    /// observing. If sensing has stopped, been paused, or gone stale, the cached
    /// value is not merely out of date — reporting it would be a false claim
    /// about a person. So it is discarded rather than trusted.
    // MARK: - Measurement observability (observation only)
    //
    // Nothing below changes an identity or authorization decision. It exists so
    // the next slice can be designed from evidence: what KUE measured, how the
    // measuring was going, and what the model was doing at the same instant.

    /// What the newest frame says, as far as the *measurement* question cares.
    /// Derived from the same `classify_frame` the identity decision uses, so
    /// the two can never drift apart.
    fn frame_outcome(&self, now: f64) -> crate::measurement::FrameOutcome {
        use crate::measurement::FrameOutcome as FO;
        if self.perception.is_none() { return FO::None; }
        match self.classify_frame(now) {
            FrameIdentity::Immediate(IdentityState::NoFace) => FO::NoFace,
            // More than one face is a fresh measurement, and it contradicts any
            // claim about one person being there.
            FrameIdentity::Immediate(IdentityState::MultiplePeople) => FO::Disagrees,
            // Not enrolled, too few samples, or a reading past the staleness
            // limit: a frame, but not a measurement of who is there.
            FrameIdentity::Immediate(IdentityState::IdentityUncertain) => FO::Unmeasurable,
            FrameIdentity::Immediate(_) => FO::None,
            FrameIdentity::Unmeasurable(_) => FO::Unmeasurable,
            FrameIdentity::Measured(IdentityState::MyFaceConfirmed) => FO::Agrees,
            // Measured, and it says somebody else.
            FrameIdentity::Measured(IdentityState::UnknownPerson) => FO::Disagrees,
            // Measured, and it settles nothing: between accept and reject.
            FrameIdentity::Measured(_) => FO::Ambiguous,
        }
    }

    /// How the measuring is going right now.
    pub fn measurement_state(&self, now: f64) -> crate::measurement::MeasurementState {
        let cfg = &self.config.perception;
        crate::measurement::assess(
            &crate::measurement::Observed {
                now,
                paused: self.paused,
                killed: self.kill_switch.is_killed(),
                sensing_up: self.sensing_process_up,
                camera_running: self.camera.state == "RUNNING",
                last_measurement_at: self.perception.as_ref().map(|p| p.ts),
                outcome: self.frame_outcome(now),
                health: self.pipeline.as_ref(),
            },
            crate::measurement::Windows {
                fresh_within: cfg.fresh_within_seconds,
                heartbeat_within: cfg.heartbeat_within_seconds,
            },
        )
    }

    /// The pipeline's last heartbeat, for diagnostics.
    pub fn pipeline_health(&self) -> Option<&crate::measurement::PipelineHealth> { self.pipeline.as_ref() }

    /// Takes one row of evidence about this moment, if one is due.
    ///
    /// Due means: the measurement state changed, or the sampling interval has
    /// elapsed. A 4 fps camera must not write four rows a second for hours, and
    /// a state change must never be missed because a timer had not elapsed.
    /// Returns what was recorded, if anything.
    pub fn sample_measurement(&mut self, now: f64) -> Option<crate::measurement::MeasurementState> {
        let state = self.measurement_state(now);
        let changed = self.last_measurement != Some(state);
        if !changed && now - self.last_sample_at < self.config.perception.sample_every_seconds {
            return None;
        }
        self.last_measurement = Some(state);
        self.last_sample_at = now;

        let face = self.perception.as_ref().and_then(|p| p.faces.first());
        let sample = crate::measurement::PerceptionSample {
            ts: now,
            measurement: state,
            // Serialised through serde, so these read exactly as the event log
            // and the window already spell them.
            identity: serde_json::to_value(self.current_identity(now)).ok()
                .and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
            access_level: serde_json::to_value(self.access.block(now).level).ok()
                .and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
            measurement_age_ms: self.perception.as_ref().map(|p| ((now - p.ts) * 1000.0).max(0.0) as u64),
            analyze_ms: self.pipeline.as_ref().map(|h| h.analyze_ms_last.round().max(0.0) as u64),
            capture_gap_ms: self.pipeline.as_ref().map(|h| h.capture_gap_ms_max.round().max(0.0) as u64),
            frames_dropped: self.pipeline.as_ref().map(|h| h.frames_dropped),
            vision_busy: self.pipeline.as_ref().map(|h| h.vision_busy),
            model_phase: self.telemetry.phase().tag(),
            track_id: face.and_then(|f| f.track_id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<i64>().ok()),
            frames_tracked: face.map(|f| f.frames_tracked),
            capture_quality: face.and_then(|f| f.capture_quality),
            face_count: self.perception.as_ref().map(|p| p.face_count),
        };
        self.telemetry.record_sample(sample);
        Some(state)
    }

    /// "Stop": work running now ends early, saying so.
    pub fn request_stop(&mut self) { self.stop_epoch += 1; }
    pub fn stop_epoch(&self) -> u64 { self.stop_epoch }
    /// Whether work that started at `epoch` has been told to stop since — or
    /// KUE was killed, which stops everything.
    pub fn stopped_since(&self, epoch: u64) -> bool { self.stop_epoch != epoch || self.kill_switch.is_killed() }

    /// What KUE keeps about the owner's world.
    pub fn memory(&self) -> &crate::memory::MemoryBook { &self.memory }

    /// For the one caller that may change it: the governed memory path in
    /// `transaction`, which checks who is asking first.
    pub fn memory_mut(&mut self) -> &mut crate::memory::MemoryBook { &mut self.memory }

    /// Why KUE will not keep anything at this moment, if it will not. A kill
    /// means nothing is written at all; a pause means nothing new is kept
    /// until the owner resumes, which is what pausing is for.
    pub fn memory_refusal(&self) -> Option<&'static str> {
        if self.is_killed() { return Some(MEMORY_KILLED); }
        if self.is_paused() { return Some(MEMORY_PAUSED); }
        None
    }

    /// The conversation and its requests.
    pub fn requests(&self) -> &crate::pipeline::Pipeline { &self.requests }
    pub fn requests_mut(&mut self) -> &mut crate::pipeline::Pipeline { &mut self.requests }

    /// The one state of KUE as a whole: kill, recovery and pause outrank any
    /// request; otherwise it is what the newest live request is doing.
    pub fn kue_state(&self, listening: bool) -> crate::pipeline::RuntimeState {
        self.requests.state(crate::pipeline::Overlay {
            killed: self.kill_switch.is_killed() && !self.kill_switch.is_recovering(),
            recovering: self.kill_switch.is_recovering(),
            paused: self.paused,
            listening,
        })
    }

    /// What KUE knows. Read by the prompt builder and the answer checker.
    pub fn facts(&self) -> &crate::facts::Facts { &self.facts }

    /// Records something KUE knows. The only writer outside this module is the
    /// action pipeline, which brings a read-back with it.
    pub fn record_fact(&mut self, fact: crate::facts::Fact) { self.facts.record(fact); }

    /// Where stage timings and samples accumulate until the shell persists them.
    pub fn telemetry(&mut self) -> &mut crate::telemetry::Telemetry { &mut self.telemetry }

    fn current_identity(&self, now: f64) -> IdentityState {
        if self.paused || !self.sensing_process_up || self.camera.state != "RUNNING" {
            return IdentityState::NotObserving;
        }
        if !self.perception_is_fresh(now) {
            return IdentityState::IdentityUncertain;
        }
        // A carried claim lapses on the clock, not only when the next frame lands.
        if self.carrying_identity && !self.hold_is_open(now) {
            return IdentityState::IdentityUncertain;
        }
        self.public_identity.value().copied().unwrap_or(IdentityState::NotObserving)
    }

    fn infer_activity_state(&self, now: f64) -> ActivityState {
        if self.paused { return ActivityState::Paused; }
        let present = self.person_present_now(now);
        let recent = self.fresh_computer(now).and(self.recent_input.value().copied());
        match (present, recent) {
            (Some(true), Some(true)) => ActivityState::AtComputerInteracting,
            (Some(true), Some(false)) => ActivityState::PresentNotInteracting,
            (Some(false), Some(true)) => ActivityState::InputWithoutVisiblePerson,
            (Some(false), Some(false)) => ActivityState::NoActivityDetected,
            (None, Some(true)) => ActivityState::AtComputerInteracting,
            (None, Some(false)) => ActivityState::NoActivityDetected,
            (_, None) => ActivityState::Unknown,
        }
    }
}

// MARK: - Context assembly

impl Engine {
    /// Builds evidence bearing on the activity statement actually displayed.
    /// Every item traces to a measurement; nothing here is asserted.
    ///
    /// Each reading is about one proposition — is someone present, is the
    /// computer receiving input — and whether it supports or contradicts is
    /// decided against what the DISPLAYED state asserts about that proposition
    /// (see `bearing`). Evidence once had a fixed direction, so "No face is
    /// visible" lowered confidence in "nobody visible to the camera".
    fn activity_evidence(&self, now: f64) -> Vec<EvidenceItem> {
        let cfg = &self.config;
        let w = &cfg.evidence_weights;
        let mut ev = Vec::new();

        if self.paused {
            // Not an inference about you at all: a statement about Lantern, backed
            // by the Paused event in the history.
            let since = self.paused_at.map(|t| now - t).unwrap_or(0.0);
            ev.push(EvidenceItem::new(
                "sensing_paused",
                format!("Paused by explicit action {since:.0}s ago; no camera frame or computer-activity sample is being read"),
                Polarity::Supports, w.camera_unavailable, 1.0, Source::Temporal, cfg));
            return ev;
        }

        let (state, _) = self.activity_now(now);
        let (claims_presence, claims_input) = activity_claim(state);
        let present = self.face_present_now(now);

        // Saying nobody is there rests on the camera seeing an empty room. In a
        // dark room it may simply not be seeing.
        if claims_presence == Some(false)
            && self.environment.low_light(&cfg.environment, now, self.rate_stretch()) == Some(true) {
            ev.push(EvidenceItem::new(
                "low_light",
                format!("The room is dark (frame brightness {:.2}, below {:.2}), so the camera may miss a person",
                    self.environment.brightness(&cfg.environment, now, self.rate_stretch()).unwrap_or(0.0),
                    cfg.environment.low_light_brightness),
                Polarity::Contradicts, w.low_light, 1.0, Source::CameraVision, cfg));
        }
        let identity = self.current_identity(now);
        let face = self.perception.as_ref().and_then(|p| p.faces.first());
        let n = self.perception.as_ref().map(|p| p.face_count).unwrap_or(0);

        match (present, face) {
            (Some(true), Some(f)) => {
                ev.push(EvidenceItem::new(
                    "face_present",
                    format!("{n} face{} visible to the camera", if n > 1 { "s" } else { "" }),
                    bearing(claims_presence, Some(true)), w.face_present,
                    // Detection strength scales with Vision's own capture quality.
                    (f.capture_quality.unwrap_or(0.0) / 0.5).clamp(0.3, 1.0),
                    Source::CameraVision, cfg));
                if f.frames_tracked >= 3 {
                    ev.push(EvidenceItem::new(
                        "track_continuity",
                        format!("The same face has been tracked for {:.0}s ({} frames)",
                            f.track_age_seconds, f.frames_tracked),
                        bearing(claims_presence, Some(true)), w.track_continuity,
                        (f.track_age_seconds / 5.0).clamp(0.2, 1.0),
                        Source::CameraVision, cfg));
                }
                // A caveat on the presence reading itself, so it only ever counts
                // against a statement that rests on that reading.
                if claims_presence == Some(true)
                    && f.capture_quality.map(|q| q < cfg.identity.min_capture_quality).unwrap_or(false) {
                    ev.push(EvidenceItem::new(
                        "low_capture_quality",
                        format!("Face capture quality is low ({:.2}, below {:.2})",
                            f.capture_quality.unwrap_or(0.0), cfg.identity.min_capture_quality),
                        Polarity::Contradicts, w.low_capture_quality, 1.0,
                        Source::CameraVision, cfg));
                }
            }
            (Some(true), None) => {
                // Inside the absence grace window: the latest frame has no face.
                // Say exactly that, rather than that a face is visible.
                let grace = cfg.activity.face_absent_grace_seconds;
                let ago = self.last_face_seen_ts.map(|t| (now - t).max(0.0)).unwrap_or(grace);
                ev.push(EvidenceItem::new(
                    "face_recently_present",
                    format!("No face in the latest frame; one was visible {ago:.1}s ago \
                             (a face is treated as present for {grace:.0}s after it was last seen)"),
                    bearing(claims_presence, Some(true)), w.face_present,
                    (1.0 - ago / grace.max(0.001)).clamp(0.2, 1.0),
                    Source::Temporal, cfg));
            }
            (Some(false), _) if self.body_stands_in_for_face(now) => {
                let located = self.environment.best_upper_body(&cfg.environment, now, self.rate_stretch())
                    .map(|(_, n)| n).unwrap_or(0);
                ev.push(EvidenceItem::new(
                    "upper_body_visible",
                    format!("No face is detected, but an upper body is visible ({located} of {} upper-body joints located)",
                        crate::environment::UPPER_BODY_JOINTS.len()),
                    bearing(claims_presence, Some(true)), w.upper_body_visible,
                    (located as f64 / crate::environment::UPPER_BODY_JOINTS.len() as f64).clamp(0.3, 1.0),
                    Source::CameraVision, cfg));
                // The missing face is still a reading, and it weighs against presence,
                // but as a caveat rather than as a full absence: the body is right there.
                ev.push(EvidenceItem::new(
                    "face_not_detected", "No face is detected — the head may be turned away, or detection missed it",
                    bearing(claims_presence, Some(false)), w.low_capture_quality, 1.0, Source::CameraVision, cfg));
            }
            (Some(false), _) => {
                ev.push(EvidenceItem::new(
                    "face_absent", "No face is visible to the camera",
                    bearing(claims_presence, Some(false)), w.face_absent, 1.0, Source::CameraVision, cfg));
            }
            (None, _) => {
                ev.push(EvidenceItem::new(
                    "camera_unavailable",
                    match self.model_unavailable() {
                        Some((stage, _)) if self.camera.state == "RUNNING" => format!(
                            "Camera frames are arriving but Apple Vision could not analyse them ({stage}), so presence cannot be checked"),
                        _ => format!("Camera is not observing (state: {}), so presence cannot be checked", self.camera.state),
                    },
                    bearing(claims_presence, None), w.camera_unavailable, 1.0, Source::CameraVision, cfg));
            }
        }

        if identity == IdentityState::MyFaceConfirmed {
            ev.push(EvidenceItem::new(
                "identity_confirmed", "The visible face matches your enrolled profile",
                bearing(claims_presence, Some(true)), w.identity_confirmed, 1.0, Source::CameraVision, cfg));
        } else if identity == IdentityState::MultiplePeople {
            ev.push(EvidenceItem::new(
                "multiple_people", "More than one person is visible",
                bearing(claims_presence, Some(true)), w.multiple_people, 1.0, Source::CameraVision, cfg));
        }

        match self.fresh_computer(now) {
            None => {
                ev.push(EvidenceItem::new(
                    "computer_unavailable",
                    match &self.computer {
                        Some(c) => format!("No computer-activity reading for {:.0}s", now - c.ts),
                        None => "No computer-activity reading has been received".to_string(),
                    },
                    bearing(claims_input, None), w.computer_unavailable, 1.0, Source::SystemHid, cfg));
            }
            Some(c) if c.idle_seconds < 0.0 => {
                ev.push(EvidenceItem::new(
                    "input_unavailable", "The keyboard and mouse idle timer could not be read",
                    bearing(claims_input, None), w.computer_unavailable, 1.0, Source::SystemHid, cfg));
            }
            Some(c) => {
                let recent = c.idle_seconds < cfg.computer.recent_input_seconds;
                if recent {
                    ev.push(EvidenceItem::new(
                        "recent_input",
                        format!("Keyboard or mouse input {:.0}s ago (threshold {:.0}s)",
                            c.idle_seconds, cfg.computer.recent_input_seconds),
                        bearing(claims_input, Some(true)), w.recent_input,
                        // Fresher input is a stronger reading.
                        (1.0 - c.idle_seconds / cfg.computer.recent_input_seconds).clamp(0.2, 1.0),
                        Source::SystemHid, cfg));
                } else {
                    ev.push(EvidenceItem::new(
                        "no_recent_input",
                        format!("No keyboard or mouse input for {:.0}s", c.idle_seconds),
                        bearing(claims_input, Some(false)), w.no_recent_input, 1.0, Source::SystemHid, cfg));
                }
                // There is always SOME frontmost app, so it is not evidence that
                // nobody is using the computer. It only corroborates use.
                if claims_input == Some(true) {
                    if let Some(name) = &c.frontmost.name {
                        ev.push(EvidenceItem::new(
                            "frontmost_app_known", format!("{name} is the frontmost application"),
                            Polarity::Supports, w.frontmost_app_known, 1.0, Source::SystemWorkspace, cfg));
                    }
                }
            }
        }
        ev
    }

    fn identity_evidence(&self, now: f64) -> Vec<EvidenceItem> {
        let cfg = &self.config;
        let w = &cfg.evidence_weights;
        let mut ev = Vec::new();
        let state = self.current_identity(now);

        // A carried claim rests on the last MEASURED frames, not on this one, so
        // this frame's distances are not offered as support. Its support decays
        // across the hold window and the unmeasurable frame counts against it.
        if let (Some(age), IdentityState::MyFaceConfirmed | IdentityState::UnknownPerson) =
            (self.carried_for(now), state)
        {
            let hold = cfg.identity.hold_unmeasurable_seconds;
            ev.push(EvidenceItem::new(
                "identity_last_measured",
                format!("Both descriptors last agreed {age:.1}s ago (a match is carried for at most {hold:.1}s)"),
                Polarity::Supports, w.identity_confirmed,
                (1.0 - age / hold.max(0.001)).clamp(0.0, 1.0),
                Source::Temporal, cfg));
            ev.push(EvidenceItem::new(
                "identity_unmeasurable",
                format!("This frame could not be measured: {}", self.identity_detail(now)),
                Polarity::Contradicts, w.identity_uncertain, 1.0, Source::CameraVision, cfg));
            ev.push(EvidenceItem::new(
                "identity_streak",
                format!("Earned over {} consecutive measured frames (minimum {})",
                    self.raw_identity.streak, cfg.identity.confirm_frames),
                Polarity::Supports, w.track_continuity, 1.0, Source::Temporal, cfg));
            return ev;
        }

        let f = self.perception.as_ref().and_then(|p| p.faces.first());
        match state {
            IdentityState::MyFaceConfirmed => {
                if let (Some(f), Some(d)) = (f, f.and_then(|f| self.combined_distance(f))) {
                    ev.push(EvidenceItem::new(
                        "identity_distance",
                        format!("Face distance {:.2}× your enrollment spread (accept at ≤ {:.2}×)",
                            d, cfg.identity.accept_ratio),
                        Polarity::Supports, w.identity_confirmed,
                        (1.0 - d / cfg.identity.accept_ratio).clamp(0.2, 1.0),
                        Source::CameraVision, cfg));
                    ev.push(EvidenceItem::new(
                        "identity_quality",
                        format!("Capture quality {:.2} (minimum {:.2})",
                            f.capture_quality.unwrap_or(0.0), cfg.identity.min_capture_quality),
                        Polarity::Supports, w.face_present,
                        (f.capture_quality.unwrap_or(0.0) / 0.5).clamp(0.2, 1.0),
                        Source::CameraVision, cfg));
                }
                ev.push(EvidenceItem::new(
                    "identity_streak",
                    format!("Held for {} consecutive frames (minimum {})",
                        self.raw_identity.streak, cfg.identity.confirm_frames),
                    Polarity::Supports, w.track_continuity, 1.0, Source::Temporal, cfg));
            }
            IdentityState::MultiplePeople => {
                ev.push(EvidenceItem::new(
                    "multiple_people",
                    format!("{} faces detected; KUE will not identify anyone in a group",
                        self.perception.as_ref().map(|p| p.face_count).unwrap_or(0)),
                    Polarity::Supports, w.multiple_people, 1.0, Source::CameraVision, cfg));
            }
            IdentityState::NoFace => {
                ev.push(EvidenceItem::new(
                    "face_absent", "No face detected in the current frame",
                    Polarity::Supports, w.face_absent, 1.0, Source::CameraVision, cfg));
            }
            IdentityState::UnknownPerson => {
                if let Some(d) = f.and_then(|f| self.combined_distance(f)) {
                    ev.push(EvidenceItem::new(
                        "identity_distance",
                        format!("Face distance {:.2}× your enrollment spread (reject at ≥ {:.2}×)",
                            d, cfg.identity.reject_ratio),
                        Polarity::Supports, w.identity_confirmed, 1.0, Source::CameraVision, cfg));
                }
            }
            IdentityState::IdentityUncertain | IdentityState::NotObserving => {
                ev.push(EvidenceItem::new(
                    "identity_uncertain", self.identity_detail(now),
                    Polarity::Supports, w.identity_uncertain, 1.0, Source::CameraVision, cfg));
            }
        }
        ev
    }

    /// Plain-language reason for the identity state being displayed.
    ///
    /// Takes the resolved state so the sentence can never contradict the label
    /// next to it — an earlier version described the raw per-frame candidate and
    /// read "not yet held for 3 frames" underneath a CONFIRMED badge.
    fn identity_detail_for(&self, state: IdentityState, now: f64) -> String {
        if let (Some(age), IdentityState::MyFaceConfirmed | IdentityState::UnknownPerson) =
            (self.carried_for(now), state)
        {
            let who = if state == IdentityState::MyFaceConfirmed { "you" } else { "not you" };
            return format!(
                "Carrying the last measured result ({who}) from {age:.1}s ago — this frame could not be \
                 measured. {} The result lapses to uncertain if nothing measurable arrives within {:.1}s \
                 of the last measurement. This is a weak biometric match, not proof.",
                self.identity_detail(now), self.config.identity.hold_unmeasurable_seconds);
        }
        match state {
            IdentityState::MyFaceConfirmed => {
                let (g, p_) = self.perception.as_ref()
                    .and_then(|p| p.faces.first())
                    .map(|f| self.descriptor_ratios(f))
                    .unwrap_or((None, None));
                match (g, p_) {
                    (Some(a), Some(b)) => format!(
                        "Both face descriptors agree this is you — landmark geometry {a:.2}× and image \
                         feature print {b:.2}× your enrollment spread, against an accept boundary of {:.2}×. \
                         Held for {} consecutive frames. This is a weak biometric match, not proof.",
                        self.config.identity.accept_ratio, self.raw_identity.streak),
                    _ => "Both face descriptors agree this is you. This is a weak biometric match, not proof.".into(),
                }
            }
            IdentityState::UnknownPerson => format!(
                "Both face descriptors place this face beyond the reject boundary of {:.2}× your enrollment \
                 spread. The matcher has never been tested against a face that is not yours, so treat this \
                 as a weak signal rather than a verdict.",
                self.config.identity.reject_ratio),
            _ => self.identity_detail(now),
        }
    }

    /// Plain-language reason for the current identity state.
    fn identity_detail(&self, now: f64) -> String {
        let cfg = &self.config.identity;
        if self.kill_switch.is_killed() { return "Killed — the sensing process is terminated and nothing is being observed.".into(); }
        if self.paused { return "Paused — the camera is off and nothing is being observed.".into(); }
        if !self.sensing_process_up { return "The sensing layer is not running.".into(); }
        if self.camera.state != "RUNNING" {
            return match self.camera.state.as_str() {
                "PERMISSION_DENIED" => "Camera access is denied in System Settings.".into(),
                "NO_CAMERA" => "No camera was found on this Mac.".into(),
                "DISCONNECTED" => "The camera was disconnected.".into(),
                s => format!("Camera is not running (state: {s})."),
            };
        }
        if !self.perception_is_fresh(now) {
            if let Some((stage, message)) = self.model_unavailable() {
                return format!("Apple Vision could not analyse recent camera frames ({stage}: {message}). \
                                Identity cannot be judged, and this is not the same as no face being present.");
            }
            return "Camera observations are stale; waiting for a fresh frame.".into();
        }
        let p = match &self.perception { Some(p) => p, None => return "No perception data yet.".into() };
        if p.face_count == 0 { return "No face is visible.".into(); }
        if p.face_count > 1 { return format!("{} people are visible.", p.face_count); }
        if self.enrollment.sample_count < cfg.minimum_samples_for_identity {
            return format!("Only {} enrollment sample(s); {} are required before identity is judged.",
                self.enrollment.sample_count, cfg.minimum_samples_for_identity);
        }
        let f = &p.faces[0];
        if f.descriptor_status != "OK" {
            return format!("Face descriptors unavailable ({}).", f.descriptor_status);
        }
        match f.capture_quality {
            Some(q) if q < cfg.min_capture_quality =>
                return format!("Capture quality {q:.2} is below the {:.2} needed to judge identity.", cfg.min_capture_quality),
            None => return "Capture quality could not be measured.".into(),
            _ => {}
        }
        if f.yaw_deg.map(|y| y.abs() > cfg.max_abs_yaw_deg).unwrap_or(false) {
            return format!("Head turned too far ({:.0}° yaw, limit {:.0}°) to judge identity.",
                f.yaw_deg.unwrap_or(0.0).abs(), cfg.max_abs_yaw_deg);
        }
        if f.pitch_deg.map(|y| y.abs() > cfg.max_abs_pitch_deg).unwrap_or(false) {
            return format!("Head tilted too far ({:.0}° pitch, limit {:.0}°) to judge identity.",
                f.pitch_deg.unwrap_or(0.0).abs(), cfg.max_abs_pitch_deg);
        }
        let (g, p_) = self.descriptor_ratios(f);
        let available = [g, p_].into_iter().flatten().count() as u32;
        if available < cfg.min_descriptors_for_claim {
            return format!("Only {available} of the {} descriptors needed for a claim are available in this frame.",
                cfg.min_descriptors_for_claim);
        }
        if !self.descriptors_agree(f) {
            if let (Some(a), Some(b)) = (g, p_) {
                return format!(
                    "The two face descriptors disagree — landmark geometry reads {a:.2}× your enrollment spread,                      image feature print reads {b:.2}× (accept ≤ {:.2}×, reject ≥ {:.2}×).                      KUE will not pick a side when its own measurements conflict.",
                    cfg.accept_ratio, cfg.reject_ratio);
            }
        }
        match self.combined_distance(f) {
            Some(d) if d > cfg.accept_ratio && d < cfg.reject_ratio => format!(
                "Face distance {d:.2}× enrollment spread falls between accept ({:.2}×) and reject ({:.2}×) — too close to call.",
                cfg.accept_ratio, cfg.reject_ratio),
            Some(_) => format!("Descriptors agree, but the reading has not yet held for {} consecutive frames.",
                cfg.confirm_frames),
            None => "No usable face descriptor in this frame.".into(),
        }
    }

    fn contradictions(&self, now: f64) -> Vec<String> {
        let mut out = Vec::new();
        let present = self.face_present_now(now);
        let recent = self.fresh_computer(now).and(self.recent_input.value().copied());

        if present == Some(false) && recent == Some(true) {
            out.push("The computer is receiving input, but no face is visible to the camera. Someone may be using it off-camera, or the camera's view may be obstructed.".into());
        }
        if present == Some(true) && recent == Some(false) {
            out.push("A person is visible but the computer has had no recent input.".into());
        }
        if let Some(w) = self.separation_warning() { out.push(w); }
        if self.paused && self.camera.state == "RUNNING" {
            out.push("Lantern is paused, but the sensing layer still reports its camera as RUNNING. \
                      The capture session may not have torn down. Treat the camera as possibly live.".into());
        }
        if self.paused && self.computer_sampling_reported == Some(true) {
            out.push("Lantern is paused, but the sensing layer still reports computer-activity sampling \
                      as active. Input timing and the frontmost app may still be being read.".into());
        }
        if self.paused && self.readings_after_pause > 0 {
            out.push(format!(
                "{} sensor reading(s) arrived after pause took effect. They were discarded, but the \
                 sensing layer did not stop sampling when asked.", self.readings_after_pause));
        }
        if self.camera.state == "RUNNING" && !self.perception_is_fresh(now) && !self.paused {
            out.push("The camera reports RUNNING but no recent frames have been analysed.".into());
        }
        if let Some(f) = self.perception.as_ref().and_then(|p| p.faces.first()) {
            if self.perception.as_ref().map(|p| p.face_count) == Some(1) && !self.paused {
                let (g, fp) = self.descriptor_ratios(f);
                if let (Some(a), Some(b)) = (g, fp) {
                    if !self.descriptors_agree(f) {
                        out.push(format!(
                            "The two face descriptors disagree: landmark geometry says {a:.2}× enrollment spread, \
                             image feature print says {b:.2}×. The feature print is a general image-similarity \
                             embedding and drifts with lighting and background, so a gap this wide usually means \
                             the lighting has changed rather than that the person has."));
                    }
                }
            }
        }
        if self.current_identity(now) == IdentityState::MyFaceConfirmed {
            if let Some(f) = self.perception.as_ref().and_then(|p| p.faces.first()) {
                if f.capture_quality.map(|q| q < 0.35).unwrap_or(false) {
                    out.push(format!("Identity is confirmed, but capture quality is only {:.2} — treat the match as weak.",
                        f.capture_quality.unwrap_or(0.0)));
                }
            }
        }
        out
    }

    /// A matcher measured NOT to separate two people is the most important thing
    /// Lantern can tell you about itself.
    fn separation_warning(&self) -> Option<String> {
        let r = self.separation.as_ref()?;
        if r.probe_samples == 0 { return None; }
        if r.reject_side_validated { return None; }
        let failing: Vec<&str> = [&r.geometry, &r.feature_print].iter()
            .filter(|d| d.verdict == "OVERLAPPING")
            .map(|d| d.name.as_str()).collect();
        if failing.is_empty() { return None; }
        Some(format!(
            "Measured on your own samples: {} did not separate you from {}. A different \
             person landed closer to your enrolled profile than your own samples land to \
             each other, so identity claims from this matcher are unreliable in both directions.",
            failing.join(" and "), r.probe_label))
    }

    fn unknowns(&self, now: f64) -> Vec<String> {
        let mut u = vec![
            "What you are thinking, feeling, or intending.".to_string(),
            "Whether you are focused, distracted, tired, or stressed.".to_string(),
            "What is on your screen — screen context is not implemented in this build.".to_string(),
            "Anything you say while the microphone is off — it runs only while you use the speak \
             control or, if you turned it on, while KUE listens for its name. The audio is never kept.".to_string(),
            "What you are typing. Only the time since the last input is read, never its content.".to_string(),
        ];
        if self.face_present_now(now) == Some(false) {
            u.push("Whether you are still nearby but outside the camera's view.".to_string());
        }
        if self.enrollment.sample_count > 0 {
            match self.separation.as_ref() {
                Some(r) if r.probe_samples > 0 => u.push(format!(
                    "Whether face matching generalises. It has been measured against {} sample(s) of {} only.",
                    r.probe_samples, r.probe_label)),
                _ => u.push("How well face matching separates you from other people — it has been calibrated on your samples only, and never measured against anyone else.".to_string()),
            }
        }
        if self.paused {
            u.push("Everything the camera and computer-activity sampling would otherwise observe. KUE is paused.".to_string());
        }
        u
    }

    fn observations(&self, now: f64) -> Vec<Observation> {
        let mut o = Vec::new();
        if let Some(p) = &self.perception {
            let age = now - p.ts;
            o.push(Observation {
                id: "obs_faces".into(),
                statement: format!("{} face(s) detected in the camera frame.", p.face_count),
                source: Source::CameraVision, age_seconds: age });
            if let Some(f) = p.faces.first() {
                o.push(Observation {
                    id: "obs_pose".into(),
                    statement: format!("Head pose — yaw {:.0}°, pitch {:.0}°, roll {:.0}°; capture quality {:.2}.",
                        f.yaw_deg.unwrap_or(0.0), f.pitch_deg.unwrap_or(0.0), f.roll_deg.unwrap_or(0.0),
                        f.capture_quality.unwrap_or(0.0)),
                    source: Source::CameraVision, age_seconds: age });
            }
            let env = &self.config.environment;
            let s = self.rate_stretch();
            if let Some(pose) = self.environment.fresh_pose(env, now, s) {
                let age = now - pose.ts;
                let located = self.environment.best_upper_body(env, now, s).map(|(_, n)| n).unwrap_or(0);
                o.push(Observation {
                    id: "obs_body".into(),
                    statement: if located >= env.upper_body_min_joints {
                        format!("Upper body visible — {located} of 4 joints located (nose, neck, shoulders).")
                    } else if pose.bodies.is_empty() {
                        "No body is located in the camera frame.".to_string()
                    } else {
                        format!("A body is only partly located — {located} of 4 upper-body joints.")
                    },
                    source: Source::CameraVision, age_seconds: age });
                let face_box = p.faces.first().map(|f| &f.bounding_box);
                let hands = self.environment.hands(env, now, s, face_box);
                o.push(Observation {
                    id: "obs_hands".into(),
                    statement: match hands.len() {
                        0 => "No hands are located in the camera frame.".to_string(),
                        n => {
                            let sides: Vec<_> = hands.iter().map(|h| h.chirality.as_str()).collect();
                            let near = hands.iter().filter(|h| h.near_face).count();
                            format!("{n} hand{} located ({}){}.", if n > 1 { "s" } else { "" }, sides.join(", "),
                                match near { 0 => String::new(), 1 => "; one overlaps the face".into(),
                                             k => format!("; {k} overlap the face") })
                        }
                    },
                    source: Source::CameraVision, age_seconds: age });
                if let Some(b) = pose.brightness {
                    o.push(Observation {
                        id: "obs_light".into(),
                        statement: if b < env.low_light_brightness {
                            format!("Frame brightness {b:.2} — below {:.2}, too dark for reliable face detection.", env.low_light_brightness)
                        } else {
                            format!("Frame brightness {b:.2} (0 is black, 1 is white).")
                        },
                        source: Source::CameraVision, age_seconds: age });
                }
                if let Some(e) = &pose.error {
                    o.push(Observation {
                        id: "obs_pose_error".into(),
                        statement: format!("Body or hand pose could not be analysed: {e}."),
                        source: Source::CameraVision, age_seconds: age });
                }
            }
            if let Some(scene) = self.environment.fresh_scene(env, now, s) {
                let age = now - scene.ts;
                let labels = self.environment.labels(env, now, s);
                o.push(Observation {
                    id: "obs_scene".into(),
                    statement: if labels.is_empty() {
                        format!("No scene label reached {:.2} confidence.", env.scene_label_min_confidence)
                    } else {
                        format!("Whole-frame labels: {}.", labels.iter()
                            .map(|l| format!("{} {:.2}", l.identifier.replace('_', " "), l.confidence))
                            .collect::<Vec<_>>().join(", "))
                    },
                    source: Source::CameraVision, age_seconds: age });
                for a in self.environment.animals(env, now, s) {
                    o.push(Observation {
                        id: format!("obs_animal_{}", a.identifier.to_lowercase()),
                        statement: format!("An animal is detected: {} ({:.2}).", a.identifier.to_lowercase(), a.confidence),
                        source: Source::CameraVision, age_seconds: age });
                }
            }
            o.push(Observation {
                id: "obs_fps".into(),
                statement: format!("Analysing about {:.1} frames per second.", p.processed_fps),
                source: Source::CameraVision, age_seconds: age });
        }
        if let (Some(c), None, false) = (&self.computer, self.fresh_computer(now), self.paused) {
            o.push(Observation {
                id: "obs_computer_stale".into(),
                statement: format!("No computer-activity reading for {:.0} seconds.", now - c.ts),
                source: Source::SystemHid, age_seconds: now - c.ts });
        }
        if let Some(c) = self.fresh_computer(now) {
            let age = now - c.ts;
            if let Some(n) = &c.frontmost.name {
                o.push(Observation {
                    id: "obs_frontmost".into(),
                    statement: format!("{n} is the frontmost application."),
                    source: Source::SystemWorkspace, age_seconds: age });
            }
            o.push(Observation {
                id: "obs_idle".into(),
                statement: if c.idle_seconds < 0.0 { "The keyboard and mouse idle timer could not be read.".to_string() }
                           else { format!("{:.0} seconds since the last keyboard or mouse event.", c.idle_seconds) },
                source: Source::SystemHid, age_seconds: age });
        }
        o.push(Observation {
            id: "obs_camera_state".into(),
            statement: format!("Camera state is {} (permission: {}).", self.camera.state, self.camera.permission),
            source: Source::CameraVision, age_seconds: 0.0 });
        o
    }

    /// Every named failure state, with its current status.
    fn conditions(&self, now: f64) -> Vec<Condition> {
        use ConditionStatus::*;
        let row = |code: &str, status: ConditionStatus, detail: String| Condition {
            code: code.into(), status, detail };
        let mut out = Vec::new();

        let paused_note = || "Paused by explicit action; this is off on purpose, not failing.".to_string();

        out.push(if !self.sensing_process_up {
            row("SENSING_LAYER_DOWN", Active, "The sensing layer is not running. Nothing is being observed.".into())
        } else {
            row("SENSING_LAYER_DOWN", Clear, "The sensing layer is running.".into())
        });

        let camera_problem: Option<String> = if !self.sensing_process_up {
            Some("There is no camera without the sensing layer.".into())
        } else {
            match self.camera.state.as_str() {
                "NO_CAMERA" => Some("No camera was found on this Mac.".into()),
                "DISCONNECTED" => Some("The camera was disconnected.".into()),
                "PERMISSION_DENIED" => Some("macOS is blocking camera access.".into()),
                "ERROR" => Some(self.camera.detail.clone().unwrap_or_else(|| "The capture session failed.".into())),
                _ => None,
            }
        };
        out.push(match (self.paused, camera_problem) {
            (true, _) => row("CAMERA_UNAVAILABLE", Clear, paused_note()),
            (false, Some(d)) => row("CAMERA_UNAVAILABLE", Active, d),
            (false, None) => row("CAMERA_UNAVAILABLE", Clear, format!("Camera state is {}.", self.camera.state)),
        });

        out.push(if matches!(self.camera.permission.as_str(), "DENIED" | "RESTRICTED") {
            row("PERMISSION_DENIED", Active, format!(
                "Camera permission is {}. Grant it in System Settings > Privacy & Security > Camera.",
                self.camera.permission))
        } else {
            row("PERMISSION_DENIED", Clear, format!("Camera permission is {}.", self.camera.permission))
        });

        out.push(match self.model_unavailable() {
            Some((stage, message)) => row("MODEL_UNAVAILABLE", Active,
                format!("Apple Vision could not analyse camera frames ({stage}: {message}).")),
            None => row("MODEL_UNAVAILABLE", Clear, "The most recent camera frame was analysed by Apple Vision.".into()),
        });

        // Push-to-talk capture exists, so this condition is real. It was
        // reported NOT_IMPLEMENTED long after the microphone was built, which
        // made the panel state something untrue.
        out.push(match (self.paused, self.sensing_process_up, self.voice.microphone_permission.as_deref()) {
            (true, _, _) => row("MIC_UNAVAILABLE", Clear, paused_note()),
            (_, false, _) => row("MIC_UNAVAILABLE", Active,
                "There is no microphone without the sensing layer.".into()),
            (_, _, Some(p @ ("DENIED" | "RESTRICTED"))) => row("MIC_UNAVAILABLE", Active, format!(
                "Microphone permission is {p}. Grant it in System Settings > Privacy & Security > Microphone.")),
            (_, _, Some(p)) => row("MIC_UNAVAILABLE", Clear, format!(
                "Microphone permission is {p}. The microphone runs only while you use the speak control or while KUE listens for its name, and the window shows both.")),
            (_, _, None) => row("MIC_UNAVAILABLE", Unknown,
                "The sensing layer has not reported whether macOS grants microphone access.".into()),
        });

        let identity = self.current_identity(now);
        for (code, state) in [("NO_FACE", IdentityState::NoFace),
                              ("UNKNOWN_PERSON", IdentityState::UnknownPerson),
                              ("IDENTITY_UNCERTAIN", IdentityState::IdentityUncertain),
                              ("MULTIPLE_PEOPLE", IdentityState::MultiplePeople)] {
            out.push(if identity == state {
                row(code, Active, self.identity_detail_for(state, now))
            } else {
                row(code, Clear, format!("Identity currently reads {}.", identity.label()))
            });
        }

        let fresh = self.fresh_computer(now);
        let no_reading = || match &self.computer {
            Some(c) => format!("No computer-activity reading for {:.0}s.", now - c.ts),
            None => "No computer-activity reading has been received.".to_string(),
        };
        out.push(match (self.paused, fresh) {
            (true, _) => row("FRONTMOST_APP_UNAVAILABLE", Clear, paused_note()),
            (false, None) => row("FRONTMOST_APP_UNAVAILABLE", Active, no_reading()),
            (false, Some(c)) if c.frontmost.name.is_none() =>
                row("FRONTMOST_APP_UNAVAILABLE", Active, "macOS reports no frontmost application.".into()),
            (false, Some(c)) => row("FRONTMOST_APP_UNAVAILABLE", Clear,
                format!("{} is the frontmost application.", c.frontmost.name.clone().unwrap_or_default())),
        });
        out.push(match (self.paused, fresh) {
            (true, _) => row("INPUT_ACTIVITY_UNAVAILABLE", Clear, paused_note()),
            (false, None) => row("INPUT_ACTIVITY_UNAVAILABLE", Active, no_reading()),
            (false, Some(c)) if c.idle_seconds < 0.0 =>
                row("INPUT_ACTIVITY_UNAVAILABLE", Active, "The keyboard and mouse idle timer could not be read.".into()),
            (false, Some(_)) => row("INPUT_ACTIVITY_UNAVAILABLE", Clear, "Input timing is being read.".into()),
        });

        out.push(match &self.storage_error {
            Some(e) => row("STORAGE_UNAVAILABLE", Active,
                format!("Local memory cannot be written ({e}). Events and snapshots are not being saved.")),
            None => row("STORAGE_UNAVAILABLE", Clear, "Local memory is being written.".into()),
        });

        out.push(row("IPC_FAILURE", Clear,
            "This context object reached the interface, so IPC is working. The interface raises this \
             itself if updates stop arriving.".into()));
        out.push(row("NETWORK_UNAVAILABLE", NotApplicable,
            "KUE makes no network requests, so network availability cannot affect it.".into()));
        out
    }

    /// The activity state as of `now`, and how long it has held.
    ///
    /// Derived from current readings rather than read from the tracker alone:
    /// the tracker only advances when a message arrives, so if readings stop it
    /// would otherwise keep presenting the last state as current. When the two
    /// differ, the state has not held for any time yet.
    fn activity_now(&self, now: f64) -> (ActivityState, f64) {
        let state = self.infer_activity_state(now);
        let stable = if self.activity.value() == Some(&state) { self.activity.stable_seconds(now) } else { 0.0 };
        (state, stable)
    }

    fn predictions(&self, now: f64) -> Vec<Prediction> {
        let mut p = Vec::new();
        let (act, stable) = self.activity_now(now);
        // Only project forward when there is an actual run of history to project from.
        if stable >= self.config.confidence.full_confidence_seconds
            && !matches!(act, ActivityState::Unknown | ActivityState::Paused) {
            p.push(Prediction {
                id: "pred_activity_persists".into(),
                statement: format!("If nothing changes, \"{}\" is likely to continue.", act.human()),
                basis: format!("This state has held for {stable:.0}s without interruption. \
                                This is an extrapolation, not an observation.") });
        }
        p
    }

    /// The honesty panel. Every signal from the long-term vision, and what this
    /// build actually does about it.
    ///
    /// One list, held in `capabilities::REGISTRY`, so the panel, the sheet, the
    /// spoken answer, the model's context and the Action Broker cannot disagree
    /// about what KUE can do.
    pub fn capabilities(&self) -> Vec<Capability> {
        crate::capabilities::rows()
    }

    pub fn build_context(&self, now: f64) -> ContextObject {
        let identity_state = self.current_identity(now);
        let (activity_state, activity_stable) = self.activity_now(now);

        let id_ev = self.identity_evidence(now);
        let id_conf = compute_confidence(&id_ev, self.public_identity.stable_seconds(now), &self.config);

        let act_ev = self.activity_evidence(now);
        let act_conf = compute_confidence(&act_ev, activity_stable, &self.config);

        let cfg = &self.config.identity;
        let geo_ref = self.enrollment.geometry_self_p95.unwrap_or(0.0).max(cfg.min_geometry_reference);
        let fp_ref = self.enrollment.feature_print_self_p95.unwrap_or(0.0).max(cfg.min_featureprint_reference);
        let has_enrollment = self.enrollment.sample_count >= cfg.minimum_samples_for_identity;

        let face = self.perception.as_ref().and_then(|p| p.faces.first());
        let combined = face.and_then(|f| self.combined_distance(f));

        let tracks = self.perception.as_ref().map(|p| p.faces.iter().map(|f| PersonTrack {
            track_id: f.track_id.clone(),
            frames_tracked: f.frames_tracked,
            age_seconds: f.track_age_seconds,
            capture_quality: f.capture_quality,
            yaw_deg: f.yaw_deg,
            pitch_deg: f.pitch_deg,
            geometry_distance: f.geometry_distance,
            feature_print_distance: f.feature_print_distance,
            descriptor_status: f.descriptor_status.clone(),
        }).collect()).unwrap_or_default();

        let mut all_evidence = id_ev.clone();
        for e in &act_ev {
            if !all_evidence.iter().any(|x| x.id == e.id) { all_evidence.push(e.clone()); }
        }

        let inferences = vec![
            Inference {
                id: "inf_activity".into(),
                statement: format!("{} (inference).", activity_state.human()),
                confidence: act_conf.clone(),
                evidence_ids: act_ev.iter().map(|e| e.id.clone()).collect() },
            Inference {
                id: "inf_identity".into(),
                statement: format!("Identity reads as {} (inference).", identity_state.label()),
                confidence: id_conf.clone(),
                evidence_ids: id_ev.iter().map(|e| e.id.clone()).collect() },
        ];

        ContextObject {
            schema_version: CONTEXT_SCHEMA_VERSION,
            generated_at: now,
            identity: IdentityBlock {
                state: identity_state,
                confidence: id_conf,
                detail: self.identity_detail_for(identity_state, now),
                accept_threshold: has_enrollment.then_some(cfg.accept_ratio),
                reject_threshold: has_enrollment.then_some(cfg.reject_ratio),
                combined_distance: combined,
                geometry_ratio: face.and_then(|f| self.descriptor_ratios(f).0),
                featureprint_ratio: face.and_then(|f| self.descriptor_ratios(f).1),
                descriptors_agree: face.map(|f| self.descriptors_agree(f)).unwrap_or(false),
                held_for_seconds: match identity_state {
                    IdentityState::MyFaceConfirmed | IdentityState::UnknownPerson => self.carried_for(now),
                    _ => None,
                },
                enrolled_samples: self.enrollment.sample_count,
                // Only a MEASUREMENT against a non-enrolled face can clear this.
                reject_side_unvalidated: !self.separation.as_ref()
                    .map(|r| r.reject_side_validated).unwrap_or(false),
            },
            people_detected: self.perception.as_ref().map(|p| p.face_count).unwrap_or(0),
            tracks,
            activity: ActivityBlock {
                state: activity_state,
                label: activity_state.label().into(),
                human: activity_state.human().into(),
                confidence: act_conf,
                evidence_ids: act_ev.iter().map(|e| e.id.clone()).collect() },
            computer: ComputerBlock {
                // Only a reading recent enough to be current is presented as one.
                frontmost_app: self.fresh_computer(now).and_then(|c| c.frontmost.name.clone()),
                frontmost_bundle_id: self.fresh_computer(now).and_then(|c| c.frontmost.bundle_id.clone()),
                idle_seconds: self.fresh_computer(now).map(|c| c.idle_seconds),
                recent_input: self.fresh_computer(now).and(self.recent_input.value().copied()),
                recent_input_threshold_seconds: self.config.computer.recent_input_seconds,
                age_seconds: self.computer.as_ref().map(|c| (now - c.ts).max(0.0)) },
            sensors: SensorsBlock {
                sensing_process: if self.sensing_process_up { "RUNNING".into() } else { "DOWN".into() },
                camera_state: if self.kill_switch.is_killed() { "KILLED".to_string() }
                    else if self.paused { "PAUSED".to_string() } else { self.camera.state.clone() },
                // What the sensing layer last said about itself, unmasked. If this
                // reads RUNNING while paused, that discrepancy is surfaced as a
                // contradiction rather than hidden behind the PAUSED label.
                camera_state_reported: self.camera.state.clone(),
                camera_permission: self.camera.permission.clone(),
                camera_device: self.camera.device_name.clone(),
                camera_detail: self.camera.detail.clone(),
                processed_fps: self.perception.as_ref().map(|p| p.processed_fps),
                computer_sampling_reported: self.computer_sampling_reported,
                readings_after_pause: self.readings_after_pause,
                paused: self.paused },
            observations: self.observations(now),
            inferences,
            predictions: self.predictions(now),
            unknowns: self.unknowns(now),
            contradictions: self.contradictions(now),
            conditions: self.conditions(now),
            environment: self.environment.block(&self.config.environment, now, self.rate_stretch(),
                self.perception.as_ref().and_then(|p| p.faces.first()).map(|f| &f.bounding_box)),
            resources: {
                let (fps, reason) = self.fps_policy();
                let avg = |(cpu, wall): (f64, f64)| (wall > 0.0).then(|| cpu / wall * 100.0);
                ResourcesBlock {
                    sensing: self.sensing_usage.block(),
                    shell: self.shell_usage.block(),
                    sensing_cpu_observing_percent: avg(self.observing_cpu),
                    observing_seconds_measured: self.observing_cpu.1,
                    sensing_cpu_paused_percent: avg(self.paused_cpu),
                    paused_seconds_measured: self.paused_cpu.1,
                    thermal_state: self.thermal_state.clone(),
                    low_power_mode: self.low_power_mode,
                    battery_percent: self.battery_percent,
                    power_source: self.power_source.clone(),
                    analysis_fps_target: fps,
                    analysis_fps_reason: reason,
                    not_measured: vec![
                        "GPU and Neural Engine use — macOS offers no public per-process API for it.".into(),
                        "Energy impact — not exposed by a public API.".into(),
                        "The WebKit processes that draw this interface — they run outside KUE's process and are not attributed to it.".into(),
                    ],
                }
            },
            evidence: all_evidence,
            recent_events: self.events.recent(40),
            remembered_events: self.remembered.clone(),
            capabilities: self.capabilities(),
            runtime: self.runtime_block(),
            access: self.access.block(now),
            voice: self.voice.clone(),
            identity_check: self.separation.as_ref().map(IdentityCheckBlock::from),
            config_note: format!("{} · geometry ref {:.3}, featureprint ref {:.3}",
                self.config_note, geo_ref, fp_ref),
        }
    }
}
