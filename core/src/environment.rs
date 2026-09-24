//! What the camera shows beyond faces: body, hands, scene and light.
//!
//! The sensing layer reports joint positions, classification labels and frame
//! brightness with Vision's own confidence. This module decides — against
//! `[environment]` thresholds — what counts as an upper body, a hand, a label
//! worth showing, or a room too dark to trust. It never infers identity,
//! gesture meaning, posture quality or mood from any of it.

use crate::config::EnvironmentCfg;
use crate::context::{EnvironmentBlock, HandView, JointView, LabelView};
use crate::sensor::{AnimalMeasurement, BBox, BodyMeasurement, HandMeasurement, SceneLabel};

pub const UPPER_BODY_JOINTS: [&str; 4] = ["nose", "neck", "leftShoulder", "rightShoulder"];

#[derive(Debug, Clone)]
pub struct PoseSnapshot {
    pub ts: f64,
    pub bodies: Vec<BodyMeasurement>,
    pub hands: Vec<HandMeasurement>,
    pub brightness: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SceneSnapshot {
    pub ts: f64,
    pub labels: Vec<SceneLabel>,
    pub animals: Vec<AnimalMeasurement>,
    pub error: Option<String>,
}

/// A located hand, with whether it overlaps the face.
#[derive(Debug, Clone)]
pub struct Hand {
    pub chirality: String,
    pub confidence: f64,
    pub bbox: Option<BBox>,
    pub near_face: bool,
}

#[derive(Debug, Default)]
pub struct Environment {
    pub pose: Option<PoseSnapshot>,
    pub scene: Option<SceneSnapshot>,
}

/// A reading is current for two and a half passes: one missed pass is normal
/// under load, two are not.
fn stale_after(interval: f64, stretch: f64) -> f64 {
    interval * stretch.max(1.0) * 2.5 + 0.5
}

fn intersects(a: &BBox, b: &BBox) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

impl Environment {
    pub fn clear(&mut self) {
        self.pose = None;
        self.scene = None;
    }

    /// `stretch` is how many times slower than configured the analysis is
    /// currently running (1.0 normally; more under thermal pressure).
    pub fn fresh_pose(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Option<&PoseSnapshot> {
        self.pose.as_ref().filter(|p| now - p.ts <= stale_after(cfg.pose_interval_seconds, stretch))
    }

    pub fn fresh_scene(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Option<&SceneSnapshot> {
        self.scene.as_ref().filter(|s| now - s.ts <= stale_after(cfg.scene_interval_seconds, stretch))
    }

    /// The body with the most located upper-body joints, and that count.
    pub fn best_upper_body<'a>(&'a self, cfg: &EnvironmentCfg, now: f64, stretch: f64)
        -> Option<(&'a BodyMeasurement, u32)> {
        let pose = self.fresh_pose(cfg, now, stretch)?;
        pose.bodies.iter()
            .map(|b| (b, UPPER_BODY_JOINTS.iter()
                .filter(|j| b.joints.get(**j).map(|p| p.confidence >= cfg.joint_min_confidence).unwrap_or(false))
                .count() as u32))
            .max_by_key(|(_, n)| *n)
    }

    /// Some(true/false) while a pose reading is current; None when there is none.
    pub fn upper_body_visible(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Option<bool> {
        self.fresh_pose(cfg, now, stretch)?;
        Some(self.best_upper_body(cfg, now, stretch).map(|(_, n)| n >= cfg.upper_body_min_joints).unwrap_or(false))
    }

    pub fn hands(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64, face: Option<&BBox>) -> Vec<Hand> {
        let Some(pose) = self.fresh_pose(cfg, now, stretch) else { return Vec::new() };
        // The face box is widened a little: a hand resting on a chin or cheek
        // sits just outside the detected face rectangle.
        let face = face.map(|f| BBox { x: f.x - f.w * 0.2, y: f.y - f.h * 0.2, w: f.w * 1.4, h: f.h * 1.4 });
        pose.hands.iter()
            .filter(|h| h.confidence >= cfg.hand_min_confidence && h.joints_located >= cfg.hand_min_joints)
            .map(|h| Hand {
                chirality: h.chirality.clone(),
                confidence: h.confidence,
                bbox: h.bounding_box.clone(),
                near_face: match (&h.bounding_box, &face) {
                    (Some(hb), Some(fb)) => intersects(hb, fb),
                    _ => false,
                },
            })
            .collect()
    }

    pub fn brightness(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Option<f64> {
        self.fresh_pose(cfg, now, stretch)?.brightness
    }

    pub fn low_light(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Option<bool> {
        self.brightness(cfg, now, stretch).map(|b| b < cfg.low_light_brightness)
    }

    pub fn labels(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Vec<LabelView> {
        let Some(scene) = self.fresh_scene(cfg, now, stretch) else { return Vec::new() };
        let mut v: Vec<_> = scene.labels.iter()
            .filter(|l| l.confidence >= cfg.scene_label_min_confidence)
            .map(|l| LabelView { identifier: l.identifier.clone(), confidence: l.confidence })
            .collect();
        v.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        v.truncate(cfg.scene_max_labels);
        v
    }

    pub fn animals(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64) -> Vec<LabelView> {
        let Some(scene) = self.fresh_scene(cfg, now, stretch) else { return Vec::new() };
        scene.animals.iter()
            .filter(|a| a.confidence >= cfg.scene_label_min_confidence)
            .map(|a| LabelView { identifier: a.label.clone(), confidence: a.confidence })
            .collect()
    }

    pub fn block(&self, cfg: &EnvironmentCfg, now: f64, stretch: f64, face: Option<&BBox>) -> EnvironmentBlock {
        let pose = self.fresh_pose(cfg, now, stretch);
        let scene = self.fresh_scene(cfg, now, stretch);
        let best = self.best_upper_body(cfg, now, stretch);
        let mut errors = Vec::new();
        if let Some(e) = pose.and_then(|p| p.error.clone()) { errors.push(e); }
        if let Some(e) = scene.and_then(|s| s.error.clone()) { errors.push(e); }
        EnvironmentBlock {
            pose_age_seconds: self.pose.as_ref().map(|p| (now - p.ts).max(0.0)),
            pose_interval_seconds: cfg.pose_interval_seconds * stretch.max(1.0),
            upper_body_visible: self.upper_body_visible(cfg, now, stretch),
            upper_body_joints_located: best.map(|(_, n)| n).unwrap_or(0),
            body_count: pose.map(|p| p.bodies.len() as u32).unwrap_or(0),
            joints: best.map(|(b, _)| b.joints.iter()
                .filter(|(_, p)| p.confidence >= cfg.joint_min_confidence)
                .map(|(name, p)| JointView { name: name.clone(), x: p.x, y: p.y, confidence: p.confidence })
                .collect()).unwrap_or_default(),
            hands: self.hands(cfg, now, stretch, face).into_iter().map(|h| HandView {
                chirality: h.chirality, confidence: h.confidence, bounding_box: h.bbox, near_face: h.near_face,
            }).collect(),
            brightness: self.brightness(cfg, now, stretch),
            low_light: self.low_light(cfg, now, stretch),
            low_light_threshold: cfg.low_light_brightness,
            scene_age_seconds: self.scene.as_ref().map(|s| (now - s.ts).max(0.0)),
            scene_interval_seconds: cfg.scene_interval_seconds * stretch.max(1.0),
            scene_labels: self.labels(cfg, now, stretch),
            animals: self.animals(cfg, now, stretch),
            errors,
        }
    }
}
