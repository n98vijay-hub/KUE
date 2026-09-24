//! Configuration: every decision threshold and weight used by the reasoning core.
//!
//! Nothing elsewhere in this crate may hard-code a threshold. If you find a bare
//! numeric comparison in a reasoning path, it belongs here instead.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub camera: CameraCfg,
    pub identity: IdentityCfg,
    pub computer: ComputerCfg,
    pub pause: PauseCfg,
    pub performance: PerformanceCfg,
    pub environment: EnvironmentCfg,
    pub activity: ActivityCfg,
    pub confidence: ConfidenceCfg,
    pub sensor_reliability: SensorReliabilityCfg,
    pub evidence_weights: EvidenceWeightsCfg,
    pub storage: StorageCfg,
    #[serde(default)]
    pub access: crate::authz::AccessCfg,
    #[serde(default)]
    pub voice: crate::voice::VoiceCfg,
    /// Windows for the measurement assessment. Observation only: nothing here
    /// changes an identity or authorization decision.
    #[serde(default)]
    pub perception: PerceptionCfg,
}

/// How KUE judges its own measuring. These are not identity thresholds and must
/// never become them: widening `fresh_within_seconds` makes KUE *describe* a
/// reading as current, and changes nothing about who it believes is there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionCfg {
    /// A measurement younger than this is current.
    pub fresh_within_seconds: f64,
    /// A pipeline heartbeat older than this proves nothing about now.
    pub heartbeat_within_seconds: f64,
    /// How often a perception sample is kept when nothing changed. Every change
    /// of measurement state is kept regardless.
    pub sample_every_seconds: f64,
    /// A routine stage is written to local memory only when it took at least
    /// this long. Measured on this Mac: the pump's own tick and context build
    /// run four times a second and are almost always under a millisecond, which
    /// would be 316,000 rows a day of "nothing happened". The slow ones are the
    /// evidence; percentiles over all of them are still computed in memory.
    /// Stages that are rare and always interesting (the model's phases, the
    /// safety screen, intent routing, privacy clearance) are always written.
    pub persist_stage_above_ms: u64,
}

impl Default for PerceptionCfg {
    fn default() -> Self {
        PerceptionCfg { fresh_within_seconds: 1.0, heartbeat_within_seconds: 3.0,
                        sample_every_seconds: 1.0, persist_stage_above_ms: 50 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCfg {
    pub target_fps: f64,
    pub observation_stale_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCfg {
    pub accept_ratio: f64,
    pub reject_ratio: f64,
    pub min_geometry_reference: f64,
    pub min_featureprint_reference: f64,
    pub geometry_weight: f64,
    pub featureprint_weight: f64,
    pub min_capture_quality: f64,
    pub max_abs_yaw_deg: f64,
    pub max_abs_pitch_deg: f64,
    pub confirm_frames: u32,
    pub hold_unmeasurable_seconds: f64,
    pub event_min_seconds: f64,
    pub minimum_samples_for_identity: u32,
    pub min_descriptors_for_claim: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerCfg {
    pub recent_input_seconds: f64,
    pub observation_stale_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceCfg {
    pub reduced_fps: f64,
    pub reduce_on_thermal_states: Vec<String>,
    pub reduce_in_low_power_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCfg {
    pub pose_interval_seconds: f64,
    pub scene_interval_seconds: f64,
    pub joint_min_confidence: f64,
    pub upper_body_min_joints: u32,
    pub body_counts_as_presence: bool,
    pub hand_min_confidence: f64,
    pub hand_min_joints: u32,
    pub scene_label_min_confidence: f64,
    pub scene_max_labels: usize,
    pub low_light_brightness: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseCfg {
    pub in_flight_grace_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityCfg {
    pub face_absent_grace_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceCfg {
    pub temporal_floor: f64,
    pub full_confidence_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReliabilityCfg {
    pub camera_vision: f64,
    pub system_workspace: f64,
    pub system_hid: f64,
    pub temporal: f64,
}

impl SensorReliabilityCfg {
    pub fn for_source(&self, s: Source) -> f64 {
        match s {
            Source::CameraVision => self.camera_vision,
            Source::SystemWorkspace => self.system_workspace,
            Source::SystemHid => self.system_hid,
            Source::Temporal => self.temporal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    CameraVision,
    SystemWorkspace,
    SystemHid,
    Temporal,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::CameraVision => "camera · Apple Vision",
            Source::SystemWorkspace => "system · NSWorkspace",
            Source::SystemHid => "system · HID idle timer",
            Source::Temporal => "temporal · event history",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceWeightsCfg {
    pub identity_confirmed: f64,
    pub identity_uncertain: f64,
    pub face_present: f64,
    pub face_absent: f64,
    pub multiple_people: f64,
    pub recent_input: f64,
    pub no_recent_input: f64,
    pub frontmost_app_known: f64,
    pub track_continuity: f64,
    pub low_capture_quality: f64,
    pub camera_unavailable: f64,
    pub computer_unavailable: f64,
    pub upper_body_visible: f64,
    pub low_light: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCfg {
    pub max_events: u64,
    pub max_snapshots: u64,
    pub snapshot_interval_seconds: u64,
}

/// Configuration embedded at compile time, used when no config file is present
/// so the app always has a complete, valid set of thresholds.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../../config/lantern.toml");

impl Config {
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn default_config() -> Self {
        Config::from_toml(DEFAULT_CONFIG_TOML).expect("bundled default config must parse")
    }

    /// Loads config from `path`, falling back to the embedded default.
    /// Returns the config and a note describing which source was used.
    pub fn load(path: &std::path::Path) -> (Self, String) {
        match std::fs::read_to_string(path) {
            Ok(s) => match Config::from_toml(&s) {
                Ok(c) => (c, format!("loaded from {}", path.display())),
                Err(e) => (
                    Config::default_config(),
                    format!("{} is invalid ({e}); using built-in defaults", path.display()),
                ),
            },
            Err(_) => (
                Config::default_config(),
                "using built-in defaults (no config file found)".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_config_parses() {
        let c = Config::default_config();
        assert_eq!(c.schema_version, 1);
        assert!(c.identity.reject_ratio > c.identity.accept_ratio);
        assert!(c.confidence.temporal_floor > 0.0 && c.confidence.temporal_floor <= 1.0);
    }

    #[test]
    fn sensor_reliabilities_are_within_unit_range() {
        let c = Config::default_config();
        for s in [
            Source::CameraVision,
            Source::SystemWorkspace,
            Source::SystemHid,
            Source::Temporal,
        ] {
            let r = c.sensor_reliability.for_source(s);
            assert!(r > 0.0 && r <= 1.0, "{s:?} reliability out of range: {r}");
        }
    }
}
