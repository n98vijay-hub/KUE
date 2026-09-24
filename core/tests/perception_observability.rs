use lantern_core::config::Config;
use lantern_core::measurement::MeasurementState;
use lantern_core::runtime::Principal;
use lantern_core::sensor::*;
use lantern_core::Engine;

fn enrollment(samples: u32) -> EnrollmentStats {
    EnrollmentStats {
        sample_count: samples,
        created_at: Some(0.0),
        feature_print_revision: Some(1),
        geometry_self_p95: Some(0.1849),
        geometry_self_mean: Some(0.1046),
        geometry_self_max: Some(0.1849),
        feature_print_self_p95: Some(0.1361),
        feature_print_self_mean: Some(0.0954),
        feature_print_self_max: Some(0.1361),
        yaw_spread_deg: Some(21.0),
        pitch_spread_deg: Some(13.9),
    }
}

fn status(state: &str) -> SensorMessage {
    SensorMessage::Status {
        camera: CameraStatus {
            state: state.into(),
            permission: "AUTHORIZED".into(),
            device_name: Some("FaceTime HD Camera".into()),
            device_id: Some("dev".into()),
            detail: None,
        },
        sensing_active: state == "RUNNING",
        computer_sampling_active: Some(true),
        enrollment: enrollment(8),
        microphone_permission: None,
    }
}

fn one_face(ratio: f64) -> FaceMeasurement {
    FaceMeasurement {
        track_id: "T1".into(),
        frames_tracked: 10,
        track_age_seconds: 4.0,
        detection_confidence: 0.9,
        bounding_box: BBox { x: 0.3, y: 0.2, w: 0.3, h: 0.4 },
        roll_deg: Some(0.0),
        yaw_deg: Some(2.0),
        pitch_deg: Some(3.0),
        capture_quality: Some(0.9),
        landmarks_available: true,
        // Distances as a multiple of the owner's own enrollment spread.
        geometry_distance: Some(0.1849 * ratio),
        feature_print_distance: Some(0.1361 * ratio),
        descriptor_status: "OK".into(),
    }
}

fn perception(ts: f64, faces: Vec<FaceMeasurement>) -> SensorMessage {
    SensorMessage::Perception { ts, face_count: faces.len() as u32, faces, frame_seq: (ts * 10.0) as i64, processed_fps: 4.0 }
}

fn face(ts: f64, ratio: f64) -> SensorMessage { perception(ts, vec![one_face(ratio)]) }
fn empty_frame(ts: f64) -> SensorMessage { perception(ts, vec![]) }

fn engine() -> Engine {
    let mut e = Engine::new(Config::default_config(), "test".into());
    e.set_sensing_process_up(true, 0.0);
    e.ingest(status("RUNNING"), 0.0);
    e
}

fn camera(state: &str) -> SensorMessage { status(state) }

/// The sensing layer's heartbeat: alive, working, capture running.
fn heartbeat(ts: f64, busy: bool) -> SensorMessage {
    SensorMessage::SenseHealth {
        ts,
        capture_running: true,
        vision_busy: busy,
        loop_alive: true,
        last_capture_at: Some(ts),
        last_analyzed_at: Some(ts),
        analyze_ms_last: if busy { 2800.0 } else { 45.0 },
        analyze_ms_p50: 45.0,
        analyze_ms_max: if busy { 2800.0 } else { 60.0 },
        capture_gap_ms_max: if busy { 3100.0 } else { 260.0 },
        frames_captured: 100,
        frames_analyzed: 96,
        frames_dropped: if busy { 40 } else { 4 },
    }
}

/// The owner, recognised, with a live pipeline.
fn seated(e: &mut Engine, from: f64) -> f64 {
    let mut t = from;
    for _ in 0..6 {
        e.ingest(heartbeat(t, false), t);
        e.ingest(face(t, 0.9), t);
        t += 0.25;
    }
    t
}

#[test]
fn a_fresh_measurement_of_the_owner_reads_as_fresh() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementFresh);
}

#[test]
fn a_late_measurement_from_a_living_pipeline_is_delayed_not_stale_or_conflicting() {
    // This is the case that has been costing the owner their session: Vision
    // stalls while the model reads its prompt, and until now the core could only
    // say "uncertain". The heartbeat keeps arriving, so the pipeline is proven
    // alive and the lateness is named as lateness.
    let mut e = engine();
    let mut t = seated(&mut e, 10.0);
    for _ in 0..12 {
        t += 0.5;
        e.ingest(heartbeat(t, true), t);   // alive, and busy in Vision
    }
    assert!(t - 11.5 >= 5.0, "more than five seconds without a frame");
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementDelayed);
    assert_ne!(e.measurement_state(t), MeasurementState::MeasurementConflict);
    assert_ne!(e.measurement_state(t), MeasurementState::MeasurementStale);
}

#[test]
fn the_same_lateness_with_a_silent_pipeline_is_stale() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);
    // No heartbeat after this point: nothing says another measurement is coming.
    let later = t + 8.0;
    assert_eq!(e.measurement_state(later), MeasurementState::MeasurementStale);
}

#[test]
fn a_fresh_disagreeing_measurement_is_a_conflict_and_never_lateness() {
    let mut e = engine();
    let mut t = seated(&mut e, 10.0);
    // Somebody else, measured, now: beyond the reject threshold.
    for _ in 0..3 {
        t += 0.25;
        e.ingest(heartbeat(t, false), t);
        e.ingest(face(t, 5.0), t);
    }
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementConflict);

    // A reading BETWEEN the thresholds is ambiguous, not contradictory. The
    // first live run of this instrument showed most samples landing here, and
    // filing them as conflict would have argued for looser thresholds on
    // evidence that never said "somebody else".
    t += 0.25;
    e.ingest(heartbeat(t, false), t);
    e.ingest(face(t, 2.0), t);
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementAmbiguous);

    // And two faces is also a conflict: it is a fresh measurement that
    // contradicts any claim about one person being there.
    e.ingest(heartbeat(t + 0.25, false), t + 0.25);
    e.ingest(perception(t + 0.25, vec![one_face(0.9), one_face(0.9)]), t + 0.25);
    assert_eq!(e.measurement_state(t + 0.25), MeasurementState::MeasurementConflict);
}

#[test]
fn nobody_in_front_of_the_camera_is_its_own_answer() {
    let mut e = engine();
    let mut t = seated(&mut e, 10.0);
    t += 0.25;
    e.ingest(heartbeat(t, false), t);
    e.ingest(empty_frame(t), t);
    assert_eq!(e.measurement_state(t), MeasurementState::NoPerson,
        "an empty frame is a measurement, not a failure to measure");
}

#[test]
fn the_camera_stopping_pausing_and_being_killed_are_three_different_answers() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);

    e.ingest(camera("STOPPED"), t);
    assert_eq!(e.measurement_state(t), MeasurementState::CameraUnavailable);
    e.ingest(camera("RUNNING"), t);

    e.set_paused(true, t);
    assert_eq!(e.measurement_state(t), MeasurementState::CameraPaused,
        "paused is the owner's own doing and must never read as a fault");
    e.set_paused(false, t);

    let latch = std::env::temp_dir().join(format!("kue-obs-latch-{}-{}", std::process::id(), t as u64));
    let _ = std::fs::remove_dir_all(&latch);
    std::fs::create_dir_all(&latch).unwrap();
    e.attach_kill_latch(latch.join("KILLED"), t);
    e.kill(Principal::Owner, "test", t).unwrap();
    assert_eq!(e.measurement_state(t), MeasurementState::CameraKilled);
    let _ = std::fs::remove_dir_all(&latch);
}

#[test]
fn sensing_dying_is_unavailable_and_never_a_person() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);
    e.set_sensing_process_up(false, t);
    assert_eq!(e.measurement_state(t), MeasurementState::CameraUnavailable);
    assert!(e.measurement_state(t).without_current_evidence());
}

#[test]
fn a_measurement_resumes_after_a_delay_and_says_so() {
    let mut e = engine();
    let mut t = seated(&mut e, 10.0);
    for _ in 0..8 { t += 0.5; e.ingest(heartbeat(t, true), t); }
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementDelayed);

    t += 0.25;
    e.ingest(heartbeat(t, false), t);
    e.ingest(face(t, 0.9), t);
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementFresh,
        "the pipeline caught up, and KUE says so rather than staying unsure");
}

#[test]
fn observing_the_measurement_changes_nothing_about_who_kue_believes_is_there() {
    // The point of the whole slice: measurement observability is observation.
    // Two engines, identical inputs; one has the heartbeat, the other does not.
    // Their identity and access decisions must be identical.
    let run = |with_heartbeat: bool| {
        let mut e = engine();
        let mut t = 10.0;
        for _ in 0..6 {
            if with_heartbeat { e.ingest(heartbeat(t, false), t); }
            e.ingest(face(t, 0.9), t);
            t += 0.25;
        }
        let a = format!("{:?}", e.build_context(t).identity.state);
        let lvl_seated = format!("{:?}", e.access_block(t).level);
        // Then a five-second stall. The shell's pump advances the access clock
        // every tick whether or not a message arrived, so both runs do too —
        // otherwise the comparison would be between a ticking engine and a
        // frozen one, and would prove nothing.
        for _ in 0..10 {
            t += 0.5;
            if with_heartbeat { e.ingest(heartbeat(t, true), t); }
            e.tick_access(t);
        }
        let b = format!("{:?}", e.build_context(t).identity.state);
        let lvl_stalled = format!("{:?}", e.access_block(t).level);
        (a, lvl_seated, b, lvl_stalled)
    };
    assert_eq!(run(true), run(false),
        "the heartbeat must not have moved identity or authorization by one step");
}

#[test]
fn a_delayed_measurement_still_does_not_authorize_anything() {
    // Whatever KUE now knows about lateness, lateness is not evidence about a
    // person, and the fail-closed rules still hold.
    let mut e = engine();
    let mut t = seated(&mut e, 10.0);
    let seated_level = format!("{:?}", e.access_block(t).level);
    for _ in 0..30 { t += 0.5; e.ingest(heartbeat(t, true), t); }
    assert_eq!(e.measurement_state(t), MeasurementState::MeasurementDelayed);
    let stalled_level = format!("{:?}", e.access_block(t).level);
    assert_ne!(stalled_level, seated_level,
        "this slice deliberately does NOT carry authorization across a delay — that is the next slice, and it is designed from this evidence");
    assert!(stalled_level.contains("Level0") || stalled_level.contains("Level1"),
        "a delayed measurement may never be an unlocked session: {stalled_level}");
}

#[test]
fn samples_are_kept_when_something_changes_and_not_four_times_a_second() {
    let mut e = engine();
    let mut t = 10.0;
    // Six frames in a second and a half: the state does not change, so the
    // sampler must not keep one per frame.
    let mut kept = 0;
    for _ in 0..6 {
        e.ingest(heartbeat(t, false), t);
        e.ingest(face(t, 0.9), t);
        if e.sample_measurement(t).is_some() { kept += 1; }
        t += 0.25;
    }
    assert!(kept <= 3, "kept {kept} rows for 1.5 s of unchanged state");

    // A change is always kept, whatever the timer says.
    let before = kept;
    e.ingest(empty_frame(t), t);
    assert!(e.sample_measurement(t).is_some(), "a change of measurement state is never skipped");
    assert!(before + 1 > before);
}

#[test]
fn what_a_sample_carries_is_times_and_categories_and_nothing_about_a_face() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);
    e.sample_measurement(t);
    let samples = e.telemetry().take_samples();
    let s = samples.last().expect("a sample was kept");
    let json = serde_json::to_string(s).unwrap();
    for forbidden in ["descriptor", "embedding", "featurePrint", "feature_print", "distance", "image", "crop"] {
        assert!(!json.contains(forbidden), "a sample carried {forbidden}: {json}");
    }
    assert_eq!(s.measurement, MeasurementState::MeasurementFresh);
    assert!(s.measurement_age_ms.is_some() && s.analyze_ms.is_some());
    assert_eq!(s.model_phase, "MODEL_IDLE");
}

#[test]
fn the_model_being_busy_does_not_stop_the_pipeline_being_seen_as_alive() {
    // The correlation this slice exists to make possible: the model is working,
    // Vision is late, and the record says both at the same instant.
    use lantern_core::telemetry::ModelPhase;
    let mut e = engine();
    let mut t = seated(&mut e, 10.0);
    e.telemetry().model_phase(ModelPhase::ModelPrefill, "ask-1", t);
    for _ in 0..10 { t += 0.5; e.ingest(heartbeat(t, true), t); }
    e.sample_measurement(t);

    let samples = e.telemetry().take_samples();
    let s = samples.last().unwrap();
    assert_eq!(s.measurement, MeasurementState::MeasurementDelayed);
    assert_eq!(s.model_phase, "MODEL_PREFILL");
    assert!(s.vision_busy.unwrap_or(false));
    assert!(s.capture_gap_ms.unwrap_or(0) > 1000, "the capture gap is recorded, not inferred");
}

// MARK: - Grounding: what KUE knows, and what a model may say about it
//
// These belong beside the observability tests because they are the same
// principle applied one layer up: KUE records how it came to know a thing, and
// nothing downstream may quietly upgrade it.

use lantern_core::facts::{Fact, FactState, Facts, Verification};
use lantern_core::privacy::{DataKind, Firewall};

#[test]
fn a_model_answer_that_doubts_a_verified_action_is_corrected_before_it_is_shown() {
    let verified = Fact::verified("action:MOVE_TO_TRASH", "KUE moved 3 files to the Trash and verified it",
                                  DataKind::EventRecord, 10.0, "MOVE_TO_TRASH",
                                  Verification::of("3 absent from Desktop, 3 present in Trash").unwrap());
    let facts = vec![&verified];

    // The failure this exists to stop: KUE did it, checked it, and the model
    // tells the owner it is not sure.
    let doubting = lantern_core::model::check_answer_against(
        "I moved the files, but I can't confirm whether they reached the Trash.", &[], &facts);
    assert!(doubting.corrections.iter().any(|c| c.contains("KUE did this and checked")),
        "a model doubted a verified action and nothing corrected it: {:?}", doubting.corrections);
    assert!(doubting.corrections.iter().any(|c| c.contains("verified it")));

    // An ordinary answer about the same fact is left alone: the model may
    // explain a verified fact, and usually should.
    let explaining = lantern_core::model::check_answer_against(
        "The three files are in the Trash, where you can put them back.", &[], &facts);
    assert!(explaining.corrections.is_empty(), "{:?}", explaining.corrections);
}

#[test]
fn only_the_action_pipeline_can_make_a_fact_verified() {
    // There is no constructor a model's output can reach: Fact::from_model is
    // the only door from a model, and it cannot express VERIFIED.
    let from_model = Fact::from_model("action:MOVE_TO_TRASH", "KUE moved the files and verified it", 10.0);
    assert_eq!(from_model.state, FactState::Inferred);
    assert!(!from_model.state.is_assertable());

    // And a model-sourced claim is never offered back to a model as context.
    let mut facts = Facts::new(8);
    facts.record(from_model);
    assert!(facts.verified(10.0).is_empty(), "a model's claim became a verified fact");
    assert!(facts.for_model(10.0, 8).is_empty());
}

#[test]
fn facts_reach_the_model_carrying_the_state_they_were_earned_in() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);
    e.record_fact(Fact::verified("action:OPEN_APPLICATION", "KUE opened Safari and verified it",
                                 DataKind::EventRecord, t, "OPEN_APPLICATION",
                                 Verification::of("Safari is running and frontmost").unwrap()));
    e.record_fact(Fact::inferred("doing", "The owner may be reading", DataKind::ActivityConclusion, t, "core"));
    e.record_fact(Fact::unknown("why", "KUE does not know why Safari is open", t, "core"));

    let known = e.facts().for_model(t, 12);
    let ctx = e.build_context(t);
    let mut fw = Firewall::new();
    let cleared = fw.clear_model_context_with(&ctx, &[], "what did you just do?", &known, t)
        .expect("the firewall cleared the prompt");
    let prompt = &cleared.value().prompt;

    assert!(prompt.contains("KNOWN (each line's state is part of the fact)"));
    assert!(prompt.contains("[VERIFIED] KUE opened Safari and verified it"));
    assert!(prompt.contains("[INFERRED] The owner may be reading"));
    assert!(prompt.contains("[UNKNOWN] KUE does not know why Safari is open"));
    // The states are never dropped on the way in: an inference cannot arrive
    // looking like something KUE confirmed.
    assert!(!prompt.contains("[VERIFIED] The owner may be reading"));
}

#[test]
fn a_stale_fact_is_not_offered_to_a_model_at_all() {
    let mut e = engine();
    let t = seated(&mut e, 10.0);
    e.record_fact(Fact::observed("storage", "The drive is 90% full", DataKind::StorageSummary, t, "storage")
        .valid_for(30.0));
    assert_eq!(e.facts().for_model(t + 5.0, 12).len(), 1);
    assert!(e.facts().for_model(t + 60.0, 12).is_empty(),
        "an old reading was still being offered as current");
}
