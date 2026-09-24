//! Evidence and deterministic confidence.
//!
//! CONTRACT: confidence is a pure function of (evidence list, config). No model,
//! no heuristic fudge, no randomness. The arithmetic below is the whole of it,
//! and every input is surfaced to the UI so a person can recompute the number by
//! hand from what is on screen. If that ever stops being true, this file is wrong.

use crate::config::{Config, Source};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Supports,
    Contradicts,
}

/// A single piece of evidence bearing on one conclusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// Stable identifier so the UI can link a conclusion to its evidence.
    pub id: String,
    /// Exactly what was measured, in plain language. Never a conclusion.
    pub statement: String,
    pub polarity: Polarity,
    /// Importance of this kind of evidence, from config.
    pub weight: f64,
    /// How strongly the observation holds, 0.0–1.0. Derived from measurements.
    pub strength: f64,
    pub source: Source,
    /// Trust in the source, from config.
    pub reliability: f64,
}

impl EvidenceItem {
    pub fn new(
        id: impl Into<String>,
        statement: impl Into<String>,
        polarity: Polarity,
        weight: f64,
        strength: f64,
        source: Source,
        cfg: &Config,
    ) -> Self {
        EvidenceItem {
            id: id.into(),
            statement: statement.into(),
            polarity,
            weight,
            strength: strength.clamp(0.0, 1.0),
            source,
            reliability: cfg.sensor_reliability.for_source(source),
        }
    }

    /// weight x strength x reliability, signed by polarity.
    pub fn contribution(&self) -> f64 {
        let m = self.weight * self.strength * self.reliability;
        match self.polarity {
            Polarity::Supports => m,
            Polarity::Contradicts => -m,
        }
    }

    /// The share of the denominator this item accounts for.
    pub fn capacity(&self) -> f64 {
        self.weight * self.reliability
    }
}

/// Every number behind a confidence value, so the UI can show the full working.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    pub supporting: f64,
    pub contradicting: f64,
    pub denominator: f64,
    /// (supporting - contradicting) / denominator, before clamping.
    pub raw: f64,
    /// Scales confidence by how long the conclusion has held steady.
    pub temporal_factor: f64,
    pub stable_seconds: f64,
    /// Final value: clamp(raw, 0, 1) x temporal_factor.
    pub value: f64,
    pub formula: String,
}

/// Computes confidence from evidence. This is the ONLY place a confidence
/// number is produced anywhere in Lantern.
pub fn compute_confidence(
    evidence: &[EvidenceItem],
    stable_seconds: f64,
    cfg: &Config,
) -> ConfidenceBreakdown {
    let mut supporting = 0.0;
    let mut contradicting = 0.0;
    let mut denominator = 0.0;

    for e in evidence {
        match e.polarity {
            Polarity::Supports => supporting += e.contribution(),
            Polarity::Contradicts => contradicting += -e.contribution(),
        }
        denominator += e.capacity();
    }

    let raw = if denominator > 0.0 {
        (supporting - contradicting) / denominator
    } else {
        0.0
    };

    // Temporal consistency: a conclusion that has just appeared is worth less
    // than the same conclusion held steady for several seconds.
    let floor = cfg.confidence.temporal_floor;
    let full = cfg.confidence.full_confidence_seconds.max(0.001);
    let temporal_factor = floor + (1.0 - floor) * (stable_seconds / full).clamp(0.0, 1.0);

    let value = raw.clamp(0.0, 1.0) * temporal_factor;

    ConfidenceBreakdown {
        supporting,
        contradicting,
        denominator,
        raw,
        temporal_factor,
        stable_seconds,
        value,
        formula: format!(
            "({supporting:.3} − {contradicting:.3}) ÷ {denominator:.3} = {raw:.3}; \
             clamp(0,1) × temporal {temporal_factor:.3} = {value:.3}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg() -> Config {
        Config::default_config()
    }

    fn item(id: &str, p: Polarity, w: f64, s: f64, c: &Config) -> EvidenceItem {
        EvidenceItem::new(id, "test", p, w, s, Source::SystemHid, c)
    }

    #[test]
    fn no_evidence_yields_zero_confidence() {
        let c = cfg();
        let b = compute_confidence(&[], 100.0, &c);
        assert_eq!(b.value, 0.0);
        assert_eq!(b.denominator, 0.0);
    }

    #[test]
    fn confidence_is_hand_checkable() {
        let c = cfg();
        // system_hid reliability is 1.0, so contributions are weight x strength.
        let ev = vec![
            item("a", Polarity::Supports, 2.0, 1.0, &c),     // +2.0, capacity 2.0
            item("b", Polarity::Contradicts, 1.0, 0.5, &c),  // -0.5, capacity 1.0
        ];
        let b = compute_confidence(&ev, c.confidence.full_confidence_seconds, &c);
        assert!((b.supporting - 2.0).abs() < 1e-9);
        assert!((b.contradicting - 0.5).abs() < 1e-9);
        assert!((b.denominator - 3.0).abs() < 1e-9);
        assert!((b.raw - 0.5).abs() < 1e-9); // (2.0 - 0.5) / 3.0
        assert!((b.temporal_factor - 1.0).abs() < 1e-9);
        assert!((b.value - 0.5).abs() < 1e-9);
    }

    #[test]
    fn contradicting_evidence_can_drive_confidence_to_zero() {
        let c = cfg();
        let ev = vec![
            item("a", Polarity::Supports, 1.0, 1.0, &c),
            item("b", Polarity::Contradicts, 3.0, 1.0, &c),
        ];
        let b = compute_confidence(&ev, 100.0, &c);
        assert!(b.raw < 0.0);
        assert_eq!(b.value, 0.0, "negative raw must clamp to zero, never go negative");
    }

    #[test]
    fn a_fresh_conclusion_is_discounted_relative_to_a_settled_one() {
        let c = cfg();
        let ev = vec![item("a", Polarity::Supports, 1.0, 1.0, &c)];
        let fresh = compute_confidence(&ev, 0.0, &c);
        let settled = compute_confidence(&ev, c.confidence.full_confidence_seconds, &c);
        assert!(fresh.value < settled.value);
        assert!((fresh.temporal_factor - c.confidence.temporal_floor).abs() < 1e-9);
        assert!((settled.value - 1.0).abs() < 1e-9);
    }

    #[test]
    fn confidence_never_leaves_the_unit_interval() {
        let c = cfg();
        for w in [0.1, 1.0, 5.0, 100.0] {
            for s in [0.0, 0.5, 1.0] {
                for stable in [0.0, 1.0, 1000.0] {
                    let ev = vec![
                        item("a", Polarity::Supports, w, s, &c),
                        item("b", Polarity::Contradicts, w * 0.3, s, &c),
                    ];
                    let b = compute_confidence(&ev, stable, &c);
                    assert!((0.0..=1.0).contains(&b.value), "out of range: {}", b.value);
                }
            }
        }
    }

    #[test]
    fn reliability_scales_contribution_and_capacity_together() {
        let c = cfg();
        // camera_vision reliability < 1.0, so an all-camera evidence set still
        // reaches raw = 1.0 when everything supports: reliability cancels out.
        let ev = vec![EvidenceItem::new(
            "cam", "face present", Polarity::Supports, 2.0, 1.0, Source::CameraVision, &c,
        )];
        let b = compute_confidence(&ev, 1000.0, &c);
        assert!((b.raw - 1.0).abs() < 1e-9);
    }
}
