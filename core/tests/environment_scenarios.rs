//! Body, hands, scene and light: scenario tests.

use lantern_core::config::Config;
use lantern_core::context::ActivityState;
use lantern_core::engine::Engine;
use lantern_core::sensor::*;
use std::collections::BTreeMap;

fn engine() -> Engine {
    Engine::new(Config::default_config(), "test".into())
}

fn status_running() -> SensorMessage {
    SensorMessage::Status {
        camera: CameraStatus { state: "RUNNING".into(), permission: "AUTHORIZED".into(), ..Default::default() },
        sensing_active: true,
        computer_sampling_active: Some(true),
        enrollment: EnrollmentStats::default(),
        microphone_permission: None,
    }
}

fn face() -> FaceMeasurement {
    FaceMeasurement {
        track_id: "T1".into(), frames_tracked: 10, track_age_seconds: 4.0, detection_confidence: 0.9,
        bounding_box: BBox { x: 0.3, y: 0.2, w: 0.3, h: 0.4 },
        roll_deg: Some(0.0), yaw_deg: Some(2.0), pitch_deg: Some(3.0), capture_quality: Some(0.5),
        landmarks_available: true, geometry_distance: None, feature_print_distance: None,
        descriptor_status: "NO_ENROLLMENT".into(),
    }
}

fn perception(ts: f64, faces: Vec<FaceMeasurement>) -> SensorMessage {
    SensorMessage::Perception { ts, face_count: faces.len() as u32, faces, frame_seq: (ts * 4.0) as i64, processed_fps: 4.0 }
}

fn computer(ts: f64, idle: f64) -> SensorMessage {
    SensorMessage::Computer {
        ts, frontmost_app: FrontmostApp { name: Some("Safari".into()), bundle_id: None }, idle_seconds: idle,
    }
}

fn joint(x: f64, y: f64, c: f64) -> JointPoint { JointPoint { x, y, confidence: c } }

fn upper_body(confidence: f64) -> BodyMeasurement {
    let mut joints = BTreeMap::new();
    joints.insert("nose".into(), joint(0.45, 0.35, confidence));
    joints.insert("neck".into(), joint(0.45, 0.6, confidence));
    joints.insert("leftShoulder".into(), joint(0.6, 0.65, confidence));
    joints.insert("rightShoulder".into(), joint(0.3, 0.65, confidence));
    BodyMeasurement { confidence, joints }
}

fn pose(ts: f64, bodies: Vec<BodyMeasurement>, hands: Vec<HandMeasurement>, brightness: f64) -> SensorMessage {
    SensorMessage::Pose { ts, bodies, hands, brightness: Some(brightness), error: None }
}

fn hand(x: f64, y: f64) -> HandMeasurement {
    HandMeasurement {
        chirality: "right".into(), confidence: 0.8,
        bounding_box: Some(BBox { x, y, w: 0.1, h: 0.12 }), wrist: None, joints_located: 19,
    }
}

/// A face and recent input, long enough for the face absence grace to matter.
fn at_computer(e: &mut Engine) -> f64 {
    e.set_sensing_process_up(true, 0.0);
    e.ingest(status_running(), 0.0);
    let mut t = 0.0;
    for _ in 0..8 {
        e.ingest(perception(t, vec![face()]), t);
        e.ingest(computer(t, 1.0), t);
        t += 0.5;
    }
    t
}

/// Frames with no face, past the absence grace window, with input continuing.
fn face_missing_until(e: &mut Engine, mut t: f64, until: f64, with_body: bool, brightness: f64) -> f64 {
    while t < until {
        e.ingest(perception(t, vec![]), t);
        e.ingest(computer(t, 1.0), t);
        let bodies = if with_body { vec![upper_body(0.8)] } else { vec![] };
        e.ingest(pose(t, bodies, vec![], brightness), t);
        t += 0.5;
    }
    t
}

#[test]
fn a_visible_upper_body_keeps_presence_when_face_detection_misses() {
    let mut e = engine();
    let t = at_computer(&mut e);
    let t = face_missing_until(&mut e, t, t + 8.0, true, 0.4);
    let ctx = e.build_context(t);
    assert_eq!(ctx.activity.state, ActivityState::AtComputerInteracting);
    let ids: Vec<_> = ctx.activity.evidence_ids.iter().map(String::as_str).collect();
    assert!(ids.contains(&"upper_body_visible"), "{ids:?}");
    assert!(ids.contains(&"face_not_detected"), "the missing face must still be shown: {ids:?}");
    assert!(!ids.contains(&"face_absent"), "{ids:?}");
    assert_eq!(ctx.environment.upper_body_visible, Some(true));
    assert!(ctx.activity.confidence.value > 0.0 && ctx.activity.confidence.value < 0.95,
        "a body without a face is moderate evidence, not certainty: {}", ctx.activity.confidence.value);
}

#[test]
fn without_a_body_the_same_face_miss_reads_as_nobody_visible() {
    let mut e = engine();
    let t = at_computer(&mut e);
    let t = face_missing_until(&mut e, t, t + 8.0, false, 0.4);
    let ctx = e.build_context(t);
    assert_eq!(ctx.activity.state, ActivityState::InputWithoutVisiblePerson);
    assert_eq!(ctx.environment.upper_body_visible, Some(false));
}

#[test]
fn a_body_is_not_counted_when_config_says_not_to() {
    let mut cfg = Config::default_config();
    cfg.environment.body_counts_as_presence = false;
    let mut e = Engine::new(cfg, "test".into());
    let t = at_computer(&mut e);
    let t = face_missing_until(&mut e, t, t + 8.0, true, 0.4);
    assert_eq!(e.build_context(t).activity.state, ActivityState::InputWithoutVisiblePerson);
}

#[test]
fn a_partial_body_below_the_joint_threshold_is_not_an_upper_body() {
    let mut e = engine();
    let t = at_computer(&mut e);
    // Joints located, but below joint_min_confidence.
    let mut t2 = t;
    while t2 < t + 8.0 {
        e.ingest(perception(t2, vec![]), t2);
        e.ingest(computer(t2, 1.0), t2);
        e.ingest(pose(t2, vec![upper_body(0.2)], vec![], 0.4), t2);
        t2 += 0.5;
    }
    let ctx = e.build_context(t2);
    assert_eq!(ctx.environment.upper_body_visible, Some(false));
    assert_eq!(ctx.activity.state, ActivityState::InputWithoutVisiblePerson);
}

#[test]
fn a_stale_pose_reading_is_not_presented_as_current() {
    let mut e = engine();
    let t = at_computer(&mut e);
    e.ingest(pose(t, vec![upper_body(0.8)], vec![], 0.4), t);
    assert_eq!(e.build_context(t).environment.upper_body_visible, Some(true));
    // Face frames keep arriving, pose readings stop.
    let mut t2 = t;
    while t2 < t + 10.0 {
        e.ingest(perception(t2, vec![face()]), t2);
        t2 += 0.5;
    }
    let ctx = e.build_context(t2);
    assert_eq!(ctx.environment.upper_body_visible, None);
    assert!(ctx.environment.joints.is_empty());
    assert!(ctx.environment.pose_age_seconds.unwrap() > 9.0);
}

#[test]
fn a_dark_room_weighs_against_saying_nobody_is_there() {
    let mut e = engine();
    e.set_sensing_process_up(true, 0.0);
    e.ingest(status_running(), 0.0);
    let mut t = 0.0;
    while t < 12.0 {
        e.ingest(perception(t, vec![]), t);
        e.ingest(computer(t, 300.0), t);
        e.ingest(pose(t, vec![], vec![], 0.05), t);
        t += 0.5;
    }
    let dark = e.build_context(t);
    assert_eq!(dark.activity.state, ActivityState::NoActivityDetected);
    assert!(dark.activity.evidence_ids.iter().any(|x| x == "low_light"));
    assert_eq!(dark.environment.low_light, Some(true));

    let mut lit = engine();
    lit.set_sensing_process_up(true, 0.0);
    lit.ingest(status_running(), 0.0);
    let mut t = 0.0;
    while t < 12.0 {
        lit.ingest(perception(t, vec![]), t);
        lit.ingest(computer(t, 300.0), t);
        lit.ingest(pose(t, vec![], vec![], 0.5), t);
        t += 0.5;
    }
    let lit = lit.build_context(t);
    assert!(!lit.activity.evidence_ids.iter().any(|x| x == "low_light"));
    assert!(dark.activity.confidence.value < lit.activity.confidence.value,
        "dark {} should be below lit {}", dark.activity.confidence.value, lit.activity.confidence.value);
}

#[test]
fn hands_are_reported_with_whether_they_overlap_the_face() {
    let mut e = engine();
    let t = at_computer(&mut e);
    // Face box is x 0.3..0.6, y 0.2..0.6. One hand on the chin, one far away.
    e.ingest(perception(t, vec![face()]), t);
    e.ingest(pose(t, vec![upper_body(0.8)], vec![hand(0.42, 0.55), hand(0.85, 0.85)], 0.4), t);
    let ctx = e.build_context(t);
    assert_eq!(ctx.environment.hands.len(), 2);
    assert_eq!(ctx.environment.hands.iter().filter(|h| h.near_face).count(), 1);
    assert!(ctx.observations.iter().any(|o| o.id == "obs_hands" && o.statement.contains("one overlaps the face")),
        "{:?}", ctx.observations.iter().map(|o| &o.statement).collect::<Vec<_>>());
}

#[test]
fn scene_labels_below_threshold_are_not_shown() {
    let mut e = engine();
    let t = at_computer(&mut e);
    e.ingest(SensorMessage::Scene {
        ts: t,
        labels: vec![
            SceneLabel { identifier: "office".into(), confidence: 0.62 },
            SceneLabel { identifier: "computer_keyboard".into(), confidence: 0.41 },
            SceneLabel { identifier: "aquarium".into(), confidence: 0.08 },
        ],
        animals: vec![AnimalMeasurement { label: "Cat".into(), confidence: 0.9,
            bounding_box: BBox { x: 0.7, y: 0.6, w: 0.2, h: 0.2 } }],
        error: None,
    }, t);
    let ctx = e.build_context(t);
    let ids: Vec<_> = ctx.environment.scene_labels.iter().map(|l| l.identifier.as_str()).collect();
    assert_eq!(ids, ["office", "computer_keyboard"]);
    assert_eq!(ctx.environment.animals.len(), 1);
    let scene = ctx.observations.iter().find(|o| o.id == "obs_scene").unwrap();
    assert!(scene.statement.contains("computer keyboard 0.41") && !scene.statement.contains("aquarium"));
}

#[test]
fn pose_and_scene_are_discarded_on_pause_and_late_ones_counted() {
    let mut e = engine();
    let t = at_computer(&mut e);
    e.ingest(pose(t, vec![upper_body(0.8)], vec![hand(0.42, 0.55)], 0.4), t);
    e.set_paused(true, t);
    let ctx = e.build_context(t);
    assert_eq!(ctx.environment.upper_body_visible, None);
    assert!(ctx.environment.hands.is_empty());
    // In flight: tolerated. Late: counted.
    e.ingest(pose(t + 0.5, vec![upper_body(0.8)], vec![], 0.4), t + 0.5);
    e.ingest(SensorMessage::Scene { ts: t + 5.0, labels: vec![], animals: vec![], error: None }, t + 5.0);
    let ctx = e.build_context(t + 5.0);
    assert_eq!(ctx.sensors.readings_after_pause, 1);
    assert_eq!(ctx.environment.upper_body_visible, None);
    assert!(ctx.environment.scene_age_seconds.is_none());
}

#[test]
fn pose_and_scene_slow_down_with_the_camera_under_thermal_pressure() {
    let mut e = engine();
    let (fps, pose_s, scene_s) = e.desired_rates();
    assert_eq!((fps, pose_s, scene_s), (4.0, 1.0, 10.0));
    e.ingest(SensorMessage::Health {
        ts: 1.0, cpu_seconds: 1.0, footprint_bytes: None, thermal_state: Some("SERIOUS".into()),
        low_power_mode: Some(false), battery_percent: None, power_source: None,
    }, 1.0);
    let (fps, pose_s, scene_s) = e.desired_rates();
    assert_eq!((fps, pose_s, scene_s), (1.0, 4.0, 40.0));
    assert!(e.rates_json().contains(r#""poseInterval":4"#), "{}", e.rates_json());
}

#[test]
fn environment_block_uses_snake_case_keys() {
    let mut e = engine();
    let t = at_computer(&mut e);
    e.ingest(pose(t, vec![upper_body(0.8)], vec![hand(0.42, 0.55)], 0.4), t);
    let json = serde_json::to_string(&e.build_context(t)).unwrap();
    for key in ["\"upper_body_visible\"", "\"near_face\"", "\"bounding_box\"", "\"scene_labels\"", "\"low_light_threshold\""] {
        assert!(json.contains(key), "missing {key}");
    }
    assert!(!json.contains("\"boundingBox\""));
}
