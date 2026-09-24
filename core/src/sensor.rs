//! Typed mirror of the sensing layer's wire protocol.
//!
//! These types carry MEASUREMENTS ONLY. The sensing layer is forbidden from
//! sending conclusions, and this module is deliberately incapable of expressing
//! one — there is no "is_owner" or "activity" field anywhere below.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMeasurement {
    pub track_id: String,
    pub frames_tracked: u32,
    pub track_age_seconds: f64,
    pub detection_confidence: f64,
    pub bounding_box: BBox,
    pub roll_deg: Option<f64>,
    pub yaw_deg: Option<f64>,
    pub pitch_deg: Option<f64>,
    pub capture_quality: Option<f64>,
    pub landmarks_available: bool,
    pub geometry_distance: Option<f64>,
    pub feature_print_distance: Option<f64>,
    pub descriptor_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CameraStatus {
    pub state: String,
    pub permission: String,
    pub device_name: Option<String>,
    pub device_id: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentStats {
    pub sample_count: u32,
    pub created_at: Option<f64>,
    pub feature_print_revision: Option<i64>,
    pub geometry_self_mean: Option<f64>,
    pub geometry_self_max: Option<f64>,
    pub geometry_self_p95: Option<f64>,
    pub feature_print_self_mean: Option<f64>,
    pub feature_print_self_max: Option<f64>,
    pub feature_print_self_p95: Option<f64>,
    pub yaw_spread_deg: Option<f64>,
    pub pitch_spread_deg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontmostApp {
    pub name: Option<String>,
    pub bundle_id: Option<String>,
}

/// How well one descriptor separates the owner from a different person.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DescriptorSeparation {
    pub name: String,
    pub within_owner_min: Option<f64>,
    pub within_owner_median: Option<f64>,
    pub within_owner_max: Option<f64>,
    pub owner_vs_probe_min: Option<f64>,
    pub owner_vs_probe_median: Option<f64>,
    pub owner_vs_probe_max: Option<f64>,
    /// Closest between-person distance ÷ widest within-owner distance.
    /// Above 1.0 the sets are cleanly separated; at or below 1.0 they overlap.
    pub separation_ratio: Option<f64>,
    /// SEPARATED | OVERLAPPING | INSUFFICIENT_DATA
    pub verdict: String,
}

/// The measured answer to "can this matcher actually tell two people apart?".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeparationReport {
    pub owner_samples: u32,
    pub probe_samples: u32,
    pub probe_label: String,
    pub geometry: DescriptorSeparation,
    pub feature_print: DescriptorSeparation,
    pub reject_side_validated: bool,
    pub note: String,
}

/// A joint position, normalized to the frame with a TOP-LEFT origin.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JointPoint {
    pub x: f64,
    pub y: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BodyMeasurement {
    pub confidence: f64,
    /// Keyed by Vision's joint name: nose, neck, leftShoulder, ...
    pub joints: std::collections::BTreeMap<String, JointPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HandMeasurement {
    pub chirality: String,
    pub confidence: f64,
    pub bounding_box: Option<BBox>,
    pub wrist: Option<JointPoint>,
    pub joints_located: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneLabel {
    pub identifier: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnimalMeasurement {
    pub label: String,
    pub confidence: f64,
    pub bounding_box: BBox,
}

/// One decoded message from the sensing layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SensorMessage {
    Hello {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        pid: i64,
        build: String,
    },
    Status {
        camera: CameraStatus,
        #[serde(rename = "sensingActive")]
        sensing_active: bool,
        /// Absent from sensing layers that predate real pause.
        #[serde(rename = "computerSamplingActive", default)]
        computer_sampling_active: Option<bool>,
        enrollment: EnrollmentStats,
        /// What macOS says about microphone access, read when the status is
        /// sent. Without it the core learned the permission only from a
        /// listening session — and the name listener would not start until it
        /// knew. Absent from sensing layers that predate the name listener.
        #[serde(rename = "microphonePermission", default)]
        microphone_permission: Option<String>,
    },
    Perception {
        ts: f64,
        #[serde(rename = "faceCount")]
        face_count: u32,
        faces: Vec<FaceMeasurement>,
        #[serde(rename = "frameSeq")]
        frame_seq: i64,
        #[serde(rename = "processedFps")]
        processed_fps: f64,
    },
    Computer {
        ts: f64,
        #[serde(rename = "frontmostApp")]
        frontmost_app: FrontmostApp,
        #[serde(rename = "idleSeconds")]
        idle_seconds: f64,
    },
    EnrollCaptured {
        accepted: bool,
        reason: String,
        enrollment: EnrollmentStats,
    },
    Devices {
        devices: Vec<std::collections::HashMap<String, String>>,
    },
    ProbeCaptured {
        accepted: bool,
        reason: String,
        #[serde(rename = "probeCount")]
        probe_count: u32,
    },
    SeparationReport {
        report: SeparationReport,
    },
    /// The sensing layer measuring ITSELF and the machine it runs on. Not a
    /// reading about the person, so it continues while paused.
    Health {
        ts: f64,
        #[serde(rename = "cpuSeconds")]
        cpu_seconds: f64,
        #[serde(rename = "footprintBytes", default)]
        footprint_bytes: Option<u64>,
        /// NOMINAL | FAIR | SERIOUS | CRITICAL, from ProcessInfo.thermalState.
        #[serde(rename = "thermalState", default)]
        thermal_state: Option<String>,
        #[serde(rename = "lowPowerMode", default)]
        low_power_mode: Option<bool>,
        #[serde(rename = "batteryPercent", default)]
        battery_percent: Option<f64>,
        #[serde(rename = "powerSource", default)]
        power_source: Option<String>,
    },
    /// The perception pipeline's own heartbeat: how the measuring is going,
    /// independent of what was measured.
    ///
    /// Sent on a fixed cadence whether or not a frame was produced, which is
    /// the entire point — a pipeline that is alive but late looks exactly like
    /// a dead one until something says "still here, still working". Aggregates
    /// only: times, counts and durations, never an image, a crop or a
    /// descriptor.
    SenseHealth {
        ts: f64,
        #[serde(rename = "captureRunning")]
        capture_running: bool,
        #[serde(rename = "visionBusy")]
        vision_busy: bool,
        /// The analysis loop iterated within the sensing layer's own liveness
        /// window. False means wedged, not merely slow.
        #[serde(rename = "loopAlive")]
        loop_alive: bool,
        #[serde(rename = "lastCaptureAt", default)]
        last_capture_at: Option<f64>,
        #[serde(rename = "lastAnalyzedAt", default)]
        last_analyzed_at: Option<f64>,
        #[serde(rename = "analyzeMsLast", default)]
        analyze_ms_last: f64,
        #[serde(rename = "analyzeMsP50", default)]
        analyze_ms_p50: f64,
        #[serde(rename = "analyzeMsMax", default)]
        analyze_ms_max: f64,
        #[serde(rename = "captureGapMsMax", default)]
        capture_gap_ms_max: f64,
        #[serde(rename = "framesCaptured", default)]
        frames_captured: u64,
        #[serde(rename = "framesAnalyzed", default)]
        frames_analyzed: u64,
        #[serde(rename = "framesDropped", default)]
        frames_dropped: u64,
    },
    /// Body and hand joints, and frame brightness, on a slower cadence than faces.
    Pose {
        ts: f64,
        #[serde(default)]
        bodies: Vec<BodyMeasurement>,
        #[serde(default)]
        hands: Vec<HandMeasurement>,
        #[serde(default)]
        brightness: Option<f64>,
        #[serde(default)]
        error: Option<String>,
    },
    /// The microphone listening session: state and audio LEVEL only. No audio.
    Voice {
        ts: f64,
        state: String,
        session: u64,
        #[serde(rename = "levelDb", default)]
        level_db: Option<f64>,
        #[serde(rename = "voiceActive", default)]
        voice_active: bool,
        #[serde(rename = "microphonePermission", default)]
        microphone_permission: Option<String>,
        #[serde(default)]
        detail: Option<String>,
    },
    /// The wake boundary: whether KUE is waiting to hear its name, and — only
    /// when it heard it — what was said in the same breath. Never a transcript
    /// of anything else; the boundary emits nothing while it is only waiting.
    Wake {
        ts: f64,
        state: String,
        #[serde(default)]
        phrase: String,
        #[serde(default)]
        confidence: Option<f64>,
        /// What followed the invocation. Present only with state WOKE.
        #[serde(default)]
        heard: Option<String>,
        #[serde(rename = "microphonePermission", default)]
        microphone_permission: Option<String>,
        #[serde(default)]
        detail: Option<String>,
    },
    /// On-device speech recognition of what the owner said during push-to-talk.
    Transcript {
        session: u64,
        text: String,
        #[serde(rename = "isFinal")]
        is_final: bool,
    },
    /// Whole-frame classification labels and animal detections.
    Scene {
        ts: f64,
        #[serde(default)]
        labels: Vec<SceneLabel>,
        #[serde(default)]
        animals: Vec<AnimalMeasurement>,
        #[serde(default)]
        error: Option<String>,
    },
    /// Apple Vision could not analyse a frame. Distinct from a frame with no
    /// face in it: a failed model is not an empty room.
    AnalysisFailed {
        ts: f64,
        stage: String,
        message: String,
    },
    Error {
        code: String,
        message: String,
    },
    Pong {
        id: i64,
    },
}

/// Parses one line from the sensing layer.
///
/// Returns `None` for anything that is not a well-formed protocol message.
/// This is deliberate: AVFoundation and Vision occasionally emit diagnostics on
/// the same file descriptor, and a stray log line must never stall the core.
pub fn parse_line(line: &str) -> Option<SensorMessage> {
    let t = line.trim();
    if t.is_empty() || !t.starts_with('{') {
        return None;
    }
    serde_json::from_str::<SensorMessage>(t).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_perception_line() {
        let line = r#"{"type":"perception","ts":1789413826.5,"faceCount":1,"v":1,"frameSeq":42,"processedFps":4.1,"faces":[{"trackId":"T1","framesTracked":9,"trackAgeSeconds":2.1,"detectionConfidence":0.78,"boundingBox":{"x":0.59,"y":0.45,"w":0.29,"h":0.52,"origin":"top-left"},"rollDeg":-0.7,"yawDeg":3.2,"pitchDeg":22.2,"captureQuality":0.42,"landmarksAvailable":true,"geometryDistance":0.073,"featurePrintDistance":0.097,"descriptorStatus":"OK"}]}"#;
        let m = parse_line(line).expect("should parse");
        match m {
            SensorMessage::Perception { face_count, faces, .. } => {
                assert_eq!(face_count, 1);
                assert_eq!(faces[0].track_id, "T1");
                assert_eq!(faces[0].descriptor_status, "OK");
                assert!((faces[0].geometry_distance.unwrap() - 0.073).abs() < 1e-9);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn a_status_line_carries_the_microphone_permission_and_older_ones_still_parse() {
        let with = r#"{"type":"status","v":1,"camera":{"state":"RUNNING","permission":"AUTHORIZED"},"sensingActive":true,"computerSamplingActive":true,"enrollment":{"sampleCount":0},"microphonePermission":"NOT_DETERMINED"}"#;
        match parse_line(with).expect("should parse") {
            SensorMessage::Status { microphone_permission, .. } =>
                assert_eq!(microphone_permission.as_deref(), Some("NOT_DETERMINED")),
            other => panic!("wrong variant: {other:?}"),
        }
        let without = r#"{"type":"status","v":1,"camera":{"state":"RUNNING","permission":"AUTHORIZED"},"sensingActive":true,"enrollment":{"sampleCount":0}}"#;
        match parse_line(without).expect("an older sensing layer still parses") {
            SensorMessage::Status { microphone_permission, .. } => assert!(microphone_permission.is_none()),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn parses_pose_and_scene_lines() {
        let pose = r#"{"type":"pose","ts":2.0,"v":1,"bodies":[{"confidence":0.8,"joints":{"neck":{"x":0.5,"y":0.6,"confidence":0.9}}}],"hands":[{"chirality":"left","confidence":0.7,"boundingBox":{"x":0.1,"y":0.2,"w":0.1,"h":0.1,"origin":"top-left"},"wrist":null,"jointsLocated":18}],"brightness":0.42}"#;
        match parse_line(pose).unwrap() {
            SensorMessage::Pose { bodies, hands, brightness, error, .. } => {
                assert_eq!(bodies[0].joints["neck"].confidence, 0.9);
                assert_eq!(hands[0].joints_located, 18);
                assert_eq!(brightness, Some(0.42));
                assert!(error.is_none());
            }
            other => panic!("wrong variant: {other:?}"),
        }
        let scene = r#"{"type":"scene","ts":3.0,"v":1,"labels":[{"identifier":"office","confidence":0.61}],"animals":[]}"#;
        match parse_line(scene).unwrap() {
            SensorMessage::Scene { labels, animals, .. } => {
                assert_eq!(labels[0].identifier, "office");
                assert!(animals.is_empty());
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn framework_noise_is_ignored_not_fatal() {
        assert!(parse_line("VTEST: error: DetectFaceLandmarksRequest was cancelled.").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
        assert!(parse_line("{not json}").is_none());
    }

    #[test]
    fn parses_computer_context_without_any_content_fields() {
        let line = r#"{"type":"computer","ts":1.0,"v":1,"frontmostApp":{"name":"Safari","bundleId":"com.apple.Safari"},"idleSeconds":3.5}"#;
        match parse_line(line).unwrap() {
            SensorMessage::Computer { frontmost_app, idle_seconds, .. } => {
                assert_eq!(frontmost_app.name.as_deref(), Some("Safari"));
                assert!((idle_seconds - 3.5).abs() < 1e-9);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
