//! What KUE knows about **the act of measuring**, kept apart from what it
//! concluded.
//!
//! THE DISTINCTION THIS MODULE EXISTS FOR:
//!
//! * **No measurement** — nothing has been measured at all.
//! * **Measurement delayed** — a measurement is late, and the pipeline that
//!   produces it is proven to be alive and working.
//! * **Measurement stale** — a measurement is late and *nothing proves another
//!   one is coming*.
//! * **Measurement conflict** — a fresh measurement arrived and it disagrees
//!   with what KUE was claiming.
//!
//! These are not the same, and until now KUE could not tell them apart: a frame
//! older than `camera.observation_stale_seconds` produced `IdentityUncertain`
//! immediately, whether the camera had died or Vision was simply three seconds
//! behind (`engine.rs::classify_frame`). The measured consequence is in
//! `docs/reports/KUE_RECONNAISSANCE_2026-09-19.md`: 830 access-level changes in
//! an hour, 51 % of them less than a second apart.
//!
//! **This module changes no behaviour.** It is observation only: the identity
//! decision and the access session are untouched by it. It exists so the next
//! slice can be chosen from evidence rather than from taste.
//!
//! Nothing here holds an image, a crop, a descriptor or any biometric vector —
//! only times, counts and categories.

use serde::{Deserialize, Serialize};

/// How the measuring is going, as distinct from what was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MeasurementState {
    /// A measurement arrived within the fresh window and agrees with, or
    /// establishes, what KUE believes.
    MeasurementFresh,
    /// No new measurement, **and the pipeline is proven alive**. This is the
    /// state that today's code cannot express: it reports uncertainty instead.
    MeasurementDelayed,
    /// No new measurement and nothing proves another one is coming.
    MeasurementStale,
    /// A fresh measurement says this is somebody else, or that more than one
    /// person is there. Evidence against the claim, not an absence of evidence.
    MeasurementConflict,
    /// A fresh measurement was taken and it settles nothing: the descriptors
    /// fall between the accept and reject thresholds. This is NOT a conflict —
    /// nothing says "someone else" — and it is not lateness either. It was
    /// separated from both because the first live run of this instrument showed
    /// 18 of 30 samples landing here, which would otherwise have been filed as
    /// contradiction and used to argue for looser thresholds.
    MeasurementAmbiguous,
    /// A fresh measurement found nobody in front of the camera.
    NoPerson,
    /// The camera is not running, or the process that owns it is gone.
    CameraUnavailable,
    /// The owner paused KUE. Not a failure.
    CameraPaused,
    /// The kill switch is engaged. Not a failure.
    CameraKilled,
}

impl MeasurementState {
    pub fn tag(self) -> &'static str {
        match self {
            MeasurementState::MeasurementFresh => "MEASUREMENT_FRESH",
            MeasurementState::MeasurementDelayed => "MEASUREMENT_DELAYED",
            MeasurementState::MeasurementStale => "MEASUREMENT_STALE",
            MeasurementState::MeasurementConflict => "MEASUREMENT_CONFLICT",
            MeasurementState::MeasurementAmbiguous => "MEASUREMENT_AMBIGUOUS",
            MeasurementState::NoPerson => "NO_PERSON",
            MeasurementState::CameraUnavailable => "CAMERA_UNAVAILABLE",
            MeasurementState::CameraPaused => "CAMERA_PAUSED",
            MeasurementState::CameraKilled => "CAMERA_KILLED",
        }
    }

    /// Is this state one in which KUE has *no current evidence* about who is
    /// there? True for every state except a fresh measurement.
    ///
    /// Note what this is NOT: it is not permission to assume the owner. Nothing
    /// in this module may be used to grant anything — the access session keeps
    /// its own rules.
    pub fn without_current_evidence(self) -> bool {
        !matches!(self, MeasurementState::MeasurementFresh | MeasurementState::MeasurementConflict
                      | MeasurementState::MeasurementAmbiguous | MeasurementState::NoPerson)
    }

    /// What the owner would be told, if asked why KUE is unsure.
    pub fn said(self) -> &'static str {
        match self {
            MeasurementState::MeasurementFresh => "Measuring now.",
            MeasurementState::MeasurementDelayed => "The camera is working, but the last reading is late.",
            MeasurementState::MeasurementStale => "No recent reading, and nothing says another is coming.",
            MeasurementState::MeasurementConflict => "The last reading says this is somebody else.",
            MeasurementState::MeasurementAmbiguous => "I measured, and it settles nothing either way.",
            MeasurementState::NoPerson => "Nobody is in front of the camera.",
            MeasurementState::CameraUnavailable => "The camera is not running.",
            MeasurementState::CameraPaused => "Paused, so nothing is being measured.",
            MeasurementState::CameraKilled => "Stopped, so nothing is being measured.",
        }
    }
}

/// What the sensing layer says about its own pipeline. Aggregates only: no
/// image, no crop, no descriptor, nothing about who was seen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineHealth {
    /// When the sensing layer sent this, on its clock.
    pub ts: f64,
    /// The capture session is configured and delivering.
    pub capture_running: bool,
    /// Vision was inside an analysis when the heartbeat was taken.
    pub vision_busy: bool,
    /// The analysis loop iterated within the sensing layer's own liveness
    /// window. False means the loop is wedged, not merely slow.
    pub loop_alive: bool,
    /// When a frame last arrived from the camera.
    pub last_capture_at: Option<f64>,
    /// When an analysis last finished.
    pub last_analyzed_at: Option<f64>,
    /// How long the last analysis took, and the shape of recent ones.
    pub analyze_ms_last: f64,
    pub analyze_ms_p50: f64,
    pub analyze_ms_max: f64,
    /// Longest gap between two captured frames in the last window.
    pub capture_gap_ms_max: f64,
    pub frames_captured: u64,
    pub frames_analyzed: u64,
    /// Frames the camera delivered that were never analysed because a newer one
    /// arrived first. Normal at a low target rate; a rising count during model
    /// work is the signal this slice is looking for.
    pub frames_dropped: u64,
}

impl PipelineHealth {
    /// Is the pipeline proven to be alive *now*?
    ///
    /// Proof, not assumption: a heartbeat recent enough to mean anything, a
    /// running capture session, and evidence that work is actually happening —
    /// an iterating loop, an analysis in progress, or an analysis that finished
    /// within the same window.
    ///
    /// That third clause was added from live data. A single analysis that takes
    /// 3.2 s keeps the loop from iterating, and in the moment right after it
    /// finishes `vision_busy` is already false while `loop_alive` has not
    /// recovered yet. Four samples in the first correlation run landed exactly
    /// there and were reported STALE — a pipeline that had just produced a
    /// measurement, described as one that might be dead. An instrument that
    /// mislabels the case it was built to study is worse than no instrument.
    pub fn alive(&self, now: f64, heartbeat_within: f64) -> bool {
        let analysed_recently = self.last_analyzed_at
            .is_some_and(|t| now - t <= heartbeat_within);
        now - self.ts <= heartbeat_within
            && self.capture_running
            && (self.loop_alive || self.vision_busy || analysed_recently)
    }
}

/// What the newest frame said, as far as the measurement question cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    /// A measurement was made and it supports what KUE believes.
    Agrees,
    /// A measurement was made and it says somebody else, or more than one
    /// person.
    Disagrees,
    /// A measurement was made and it settles nothing: between the thresholds.
    Ambiguous,
    /// A measurement was made and nobody was there.
    NoFace,
    /// A frame arrived but could not be measured (quality, pose, descriptor).
    /// Not evidence either way.
    Unmeasurable,
    /// No frame has arrived at all.
    None,
}

/// Everything the assessment needs. Deliberately a plain struct of facts: the
/// caller gathers them, this module decides nothing else.
#[derive(Debug, Clone)]
pub struct Observed<'a> {
    pub now: f64,
    pub paused: bool,
    pub killed: bool,
    pub sensing_up: bool,
    pub camera_running: bool,
    /// When the newest perception frame was produced, on the shared clock.
    pub last_measurement_at: Option<f64>,
    pub outcome: FrameOutcome,
    pub health: Option<&'a PipelineHealth>,
}

/// Windows, from configuration. No decision boundary is hard-coded in Rust.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Windows {
    /// A measurement younger than this is current.
    pub fresh_within: f64,
    /// A heartbeat older than this proves nothing about now.
    pub heartbeat_within: f64,
}

/// The one decision this module makes.
///
/// Order matters and is deliberate: the owner's own gestures (kill, pause) come
/// first, then the camera's availability, then the measurement itself. A paused
/// KUE is not "stale" — it is paused, and saying so is the difference between a
/// system that explains itself and one that merely reports symptoms.
pub fn assess(o: &Observed, w: Windows) -> MeasurementState {
    use MeasurementState::*;
    if o.killed { return CameraKilled; }
    if o.paused { return CameraPaused; }
    if !o.sensing_up || !o.camera_running { return CameraUnavailable; }

    let age = o.last_measurement_at.map(|t| o.now - t);
    let fresh = age.is_some_and(|a| a <= w.fresh_within);
    if fresh {
        return match o.outcome {
            FrameOutcome::Disagrees => MeasurementConflict,
            FrameOutcome::Ambiguous => MeasurementAmbiguous,
            FrameOutcome::NoFace => NoPerson,
            FrameOutcome::Agrees => MeasurementFresh,
            // A frame that could not be measured is not a measurement. It says
            // nothing about who is there, so the question becomes the same one
            // as for no frame at all: is another measurement coming?
            FrameOutcome::Unmeasurable | FrameOutcome::None => late(o, w),
        };
    }
    late(o, w)
}

/// No current measurement. The only question left is whether another one is
/// coming — which is exactly the question KUE could not ask before.
fn late(o: &Observed, w: Windows) -> MeasurementState {
    match o.health {
        Some(h) if h.alive(o.now, w.heartbeat_within) => MeasurementState::MeasurementDelayed,
        // No heartbeat at all, a stale one, a stopped capture session or a
        // wedged loop: nothing here says another measurement is coming.
        _ => MeasurementState::MeasurementStale,
    }
}

/// One row of evidence about one moment: what was measured, how the measuring
/// was going, and what the rest of the system was doing at the same instant.
///
/// This is the record the identity decision will be designed from. It carries
/// no embedding, no image, no audio and no private content — times, counts,
/// categories and the states KUE was already showing the owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerceptionSample {
    pub ts: f64,
    pub measurement: MeasurementState,
    /// The identity state KUE actually acted on, unchanged by this module.
    pub identity: String,
    /// The access level in force at the same instant.
    pub access_level: String,
    /// Age of the newest measurement, in milliseconds. None when none exists.
    pub measurement_age_ms: Option<u64>,
    /// How long the last Vision analysis took.
    pub analyze_ms: Option<u64>,
    /// Longest gap between captured frames in the sensing layer's last window.
    pub capture_gap_ms: Option<u64>,
    pub frames_dropped: Option<u64>,
    pub vision_busy: Option<bool>,
    /// What the on-device model was doing at the same instant — the correlation
    /// this slice exists to establish.
    pub model_phase: String,
    /// Track continuity: a new track is a different person as far as the
    /// access session is concerned, so it belongs in the record.
    pub track_id: Option<i64>,
    pub frames_tracked: Option<u32>,
    pub capture_quality: Option<f64>,
    pub face_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows() -> Windows { Windows { fresh_within: 1.0, heartbeat_within: 3.0 } }

    fn healthy(ts: f64) -> PipelineHealth {
        PipelineHealth {
            ts, capture_running: true, vision_busy: false, loop_alive: true,
            last_capture_at: Some(ts), last_analyzed_at: Some(ts),
            analyze_ms_last: 40.0, analyze_ms_p50: 38.0, analyze_ms_max: 55.0,
            capture_gap_ms_max: 260.0, frames_captured: 100, frames_analyzed: 96, frames_dropped: 4,
        }
    }

    fn observed<'a>(now: f64, last: Option<f64>, outcome: FrameOutcome, h: Option<&'a PipelineHealth>) -> Observed<'a> {
        Observed { now, paused: false, killed: false, sensing_up: true, camera_running: true,
                   last_measurement_at: last, outcome, health: h }
    }

    #[test]
    fn a_late_measurement_from_a_live_pipeline_is_delayed_not_stale() {
        let h = healthy(10.0);
        // Six seconds since the last frame — far past any staleness limit — but
        // the pipeline said "alive" one second ago. That is a DELAY.
        let o = observed(11.0, Some(5.0), FrameOutcome::None, Some(&h));
        assert_eq!(assess(&o, windows()), MeasurementState::MeasurementDelayed);

        // The same lateness with no heartbeat at all is STALE: nothing says
        // another measurement is coming.
        let o = observed(11.0, Some(5.0), FrameOutcome::None, None);
        assert_eq!(assess(&o, windows()), MeasurementState::MeasurementStale);
    }

    #[test]
    fn a_pipeline_is_only_alive_while_it_keeps_saying_so() {
        let h = healthy(10.0);
        assert!(h.alive(11.0, 3.0));
        assert!(!h.alive(14.0, 3.0), "a heartbeat older than the window proves nothing about now");

        let stopped = PipelineHealth { capture_running: false, ..healthy(10.0) };
        assert!(!stopped.alive(10.5, 3.0), "a stopped capture session is not a live pipeline");

        let wedged = PipelineHealth { loop_alive: false, vision_busy: false, last_analyzed_at: None,
                                      ..healthy(10.0) };
        assert!(!wedged.alive(10.5, 3.0), "a loop that stopped iterating, with nothing analysed, is not merely slow");

        // Busy in Vision counts as alive: that is exactly the case this slice
        // is trying to see — analysis running long, not analysis dead.
        let busy = PipelineHealth { loop_alive: false, vision_busy: true, ..healthy(10.0) };
        assert!(busy.alive(10.5, 3.0));

        // And the moment just after a long analysis finishes, when the loop has
        // not iterated yet and nothing is busy: the analysis itself is the proof.
        // Measured live on 2026-09-20 — four samples landed here and were
        // wrongly reported STALE before this clause existed.
        let just_finished = PipelineHealth {
            loop_alive: false, vision_busy: false, last_analyzed_at: Some(9.9), ..healthy(10.0)
        };
        assert!(just_finished.alive(10.5, 3.0));
        // But an analysis that finished long ago proves nothing.
        let long_ago = PipelineHealth {
            loop_alive: false, vision_busy: false, last_analyzed_at: Some(1.0), ..healthy(10.0)
        };
        assert!(!long_ago.alive(10.5, 3.0));
    }

    #[test]
    fn delayed_is_not_conflict_and_conflict_is_not_delayed() {
        let h = healthy(10.0);
        let delayed = assess(&observed(11.0, Some(5.0), FrameOutcome::None, Some(&h)), windows());
        let conflict = assess(&observed(10.5, Some(10.4), FrameOutcome::Disagrees, Some(&h)), windows());
        let stale = assess(&observed(20.0, Some(5.0), FrameOutcome::None, Some(&h)), windows());
        assert_ne!(delayed, conflict);
        assert_ne!(stale, conflict);
        assert_eq!(conflict, MeasurementState::MeasurementConflict,
            "a fresh measurement that disagrees is evidence, and must never be filed as lateness");
        assert_eq!(stale, MeasurementState::MeasurementStale,
            "the heartbeat is ten seconds old here, so nothing proves the pipeline alive");
    }

    #[test]
    fn an_ambiguous_measurement_is_neither_a_conflict_nor_lateness() {
        let h = healthy(10.0);
        let amb = assess(&observed(10.5, Some(10.4), FrameOutcome::Ambiguous, Some(&h)), windows());
        assert_eq!(amb, MeasurementState::MeasurementAmbiguous);
        assert_ne!(amb, MeasurementState::MeasurementConflict,
            "a reading between the thresholds says nothing about somebody else being there");
        assert_ne!(amb, MeasurementState::MeasurementDelayed);
        // It is still evidence: a measurement happened.
        assert!(!amb.without_current_evidence());
    }

    #[test]
    fn nobody_there_is_a_measurement_and_not_a_failure() {
        let h = healthy(10.0);
        assert_eq!(assess(&observed(10.5, Some(10.4), FrameOutcome::NoFace, Some(&h)), windows()),
                   MeasurementState::NoPerson);
        // An unmeasurable frame is NOT "nobody there" — it is no measurement.
        assert_eq!(assess(&observed(10.5, Some(10.4), FrameOutcome::Unmeasurable, Some(&h)), windows()),
                   MeasurementState::MeasurementDelayed);
    }

    #[test]
    fn the_owners_own_gestures_outrank_every_measurement_question() {
        let h = healthy(10.0);
        let mut o = observed(10.5, Some(10.4), FrameOutcome::Agrees, Some(&h));
        assert_eq!(assess(&o, windows()), MeasurementState::MeasurementFresh);

        o.paused = true;
        assert_eq!(assess(&o, windows()), MeasurementState::CameraPaused, "paused is not a fault");
        o.killed = true;
        assert_eq!(assess(&o, windows()), MeasurementState::CameraKilled, "kill outranks pause");

        let mut o = observed(10.5, Some(10.4), FrameOutcome::Agrees, Some(&h));
        o.camera_running = false;
        assert_eq!(assess(&o, windows()), MeasurementState::CameraUnavailable);
        let mut o = observed(10.5, Some(10.4), FrameOutcome::Agrees, Some(&h));
        o.sensing_up = false;
        assert_eq!(assess(&o, windows()), MeasurementState::CameraUnavailable);
    }

    #[test]
    fn nothing_measured_yet_is_never_reported_as_a_person() {
        let h = healthy(10.0);
        let o = observed(10.0, None, FrameOutcome::None, Some(&h));
        assert_eq!(assess(&o, windows()), MeasurementState::MeasurementDelayed,
            "the pipeline is alive and has not produced a first measurement yet");
        let o = observed(10.0, None, FrameOutcome::None, None);
        assert_eq!(assess(&o, windows()), MeasurementState::MeasurementStale);
        for s in [MeasurementState::MeasurementDelayed, MeasurementState::MeasurementStale,
                  MeasurementState::CameraUnavailable, MeasurementState::CameraPaused,
                  MeasurementState::CameraKilled] {
            assert!(s.without_current_evidence(), "{s:?} must never read as evidence about a person");
        }
        assert!(!MeasurementState::MeasurementFresh.without_current_evidence());
        assert!(!MeasurementState::NoPerson.without_current_evidence());
        assert!(!MeasurementState::MeasurementConflict.without_current_evidence());
    }

    #[test]
    fn every_state_says_something_a_person_could_read() {
        for s in [MeasurementState::MeasurementFresh, MeasurementState::MeasurementDelayed,
                  MeasurementState::MeasurementStale, MeasurementState::MeasurementConflict,
                  MeasurementState::MeasurementAmbiguous, MeasurementState::NoPerson,
                  MeasurementState::CameraUnavailable, MeasurementState::CameraPaused,
                  MeasurementState::CameraKilled] {
            assert!(!s.said().is_empty());
            assert!(!s.said().contains('_'), "{} reads like a tag", s.said());
            assert!(s.tag().chars().all(|c| c.is_ascii_uppercase() || c == '_'));
        }
    }
}
