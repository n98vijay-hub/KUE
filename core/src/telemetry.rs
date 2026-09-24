//! How long things take, and what the model was doing while they took it.
//!
//! KUE could measure its own answer time end to end (20 s median, 107 s worst,
//! from its own event log) and nothing else. It could not say whether a given
//! answer was spent queueing, reading the prompt or generating — and it could
//! not say what Vision was doing meanwhile. That question is the whole reason
//! this slice exists, so the timeline records both.
//!
//! **What may be recorded:** a stage, an opaque operation id, a start, an end
//! and an outcome. **What may not:** the question, the answer, a transcript, a
//! file name, an app name, a face, an embedding. The `op` field is sanitised in
//! code (`Span::new`) rather than by convention — anything that is not an
//! identifier character is dropped, so a sentence cannot become a span id even
//! by mistake.

use serde::{Deserialize, Serialize};

/// The stages of one request or one perception cycle. Named after what the
/// machine is doing, not after which module owns it, so a timeline reads the
/// same way whoever wrote the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage {
    /// Camera frame arrival to hand-off (measured in the sensing layer).
    CameraCapture,
    /// Vision landmark + descriptor work for one frame (sensing layer).
    VisionAnalyze,
    /// Turning distances into an identity state (core).
    IdentityDecision,
    /// One pass of the shell's pump.
    CoreTick,
    /// The word-list refusal screen.
    SafetyScreen,
    /// Rule-based routing of a request to work.
    IntentRouting,
    /// Building the context object.
    ContextBuild,
    /// Clearing data through the privacy firewall.
    PrivacyClearance,
    /// Submitted to the model process, not yet started.
    ModelQueue,
    /// The model reading the prompt — the phase that is suspected of starving
    /// Vision, and which this slice exists to measure.
    ModelPrefill,
    /// The model producing text, from first token to last.
    ModelGeneration,
    /// Everything after the model's last token until the answer is shown.
    Response,
    /// Speaking a sentence aloud.
    Speech,
    /// Performing an action through the broker.
    ActionExecute,
    /// Reading back the world to see whether the action happened.
    ActionVerify,
}

impl Stage {
    /// Stages that happen once per request rather than four times a second.
    /// These are always written to local memory: they are the timeline of what
    /// KUE did, and there are never many of them.
    pub fn always_worth_keeping(self) -> bool {
        matches!(self, Stage::ModelQueue | Stage::ModelPrefill | Stage::ModelGeneration
                     | Stage::SafetyScreen | Stage::IntentRouting | Stage::PrivacyClearance
                     | Stage::ActionExecute | Stage::ActionVerify | Stage::Speech | Stage::Response)
    }

    pub fn tag(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }
    /// Unknown tag → None, which every caller treats as "do not record".
    pub fn from_tag(tag: &str) -> Option<Stage> {
        serde_json::from_value(serde_json::Value::String(tag.to_string())).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpanOutcome {
    Ok,
    /// The stage ended because something refused it — a policy, not a fault.
    Refused,
    Failed,
    /// It ended, but KUE cannot say whether it worked. The same word the action
    /// pipeline uses, for the same reason.
    UnknownResult,
}

/// One measured stretch of time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub stage: Stage,
    /// An opaque id tying stages of one request together. Never content.
    pub op: String,
    pub start: f64,
    pub end: f64,
    pub outcome: SpanOutcome,
}

impl Span {
    /// The only way to build a span. `op` is reduced to identifier characters
    /// and 24 of them, so no sentence, path or name can survive into the record
    /// even if a caller passes one by mistake.
    pub fn new(stage: Stage, op: &str, start: f64, end: f64, outcome: SpanOutcome) -> Span {
        let op: String = op.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(24).collect();
        Span { stage, op, start, end: end.max(start), outcome }
    }
    pub fn duration_ms(&self) -> u64 { ((self.end - self.start) * 1000.0).round().max(0.0) as u64 }
}

/// What the on-device model is doing. Recorded as transitions so a perception
/// sample taken at any instant can name the model's phase at that instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelPhase {
    ModelIdle,
    /// Handed to the model process, no token back yet.
    ModelQueued,
    /// The prompt is being read. Entered when the request is sent, left on the
    /// first token — the closest measurable boundary from outside the model.
    ModelPrefill,
    ModelGenerating,
}

impl ModelPhase {
    pub fn tag(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }
}

/// Timings and phases held in memory between persists. Bounded on purpose: this
/// is diagnostics, and diagnostics may never be the reason KUE runs out of
/// memory or disk.
#[derive(Debug, Clone)]
pub struct Telemetry {
    spans: Vec<Span>,
    samples: Vec<crate::measurement::PerceptionSample>,
    phase: ModelPhase,
    phase_since: f64,
    max_spans: usize,
    max_samples: usize,
    dropped_spans: u64,
    dropped_samples: u64,
}

impl Default for Telemetry {
    fn default() -> Self { Telemetry::new(512, 512) }
}

impl Telemetry {
    pub fn new(max_spans: usize, max_samples: usize) -> Telemetry {
        Telemetry { spans: Vec::new(), samples: Vec::new(), phase: ModelPhase::ModelIdle,
                    phase_since: 0.0, max_spans, max_samples, dropped_spans: 0, dropped_samples: 0 }
    }

    /// Records one finished stage. Oldest is dropped when full, and the drop is
    /// counted rather than hidden.
    pub fn record(&mut self, span: Span) {
        if self.spans.len() >= self.max_spans { self.spans.remove(0); self.dropped_spans += 1; }
        self.spans.push(span);
    }

    pub fn record_sample(&mut self, s: crate::measurement::PerceptionSample) {
        if self.samples.len() >= self.max_samples { self.samples.remove(0); self.dropped_samples += 1; }
        self.samples.push(s);
    }

    /// Moves the model to a phase, and records the stretch just ended as a span
    /// so the timeline has both the transitions and the durations.
    pub fn model_phase(&mut self, phase: ModelPhase, op: &str, now: f64) {
        let ended = match self.phase {
            ModelPhase::ModelQueued => Some(Stage::ModelQueue),
            ModelPhase::ModelPrefill => Some(Stage::ModelPrefill),
            ModelPhase::ModelGenerating => Some(Stage::ModelGeneration),
            ModelPhase::ModelIdle => None,
        };
        if let Some(stage) = ended {
            if self.phase_since > 0.0 {
                self.record(Span::new(stage, op, self.phase_since, now, SpanOutcome::Ok));
            }
        }
        self.phase = phase;
        self.phase_since = now;
    }

    pub fn phase(&self) -> ModelPhase { self.phase }
    pub fn phase_since(&self) -> f64 { self.phase_since }
    pub fn dropped(&self) -> (u64, u64) { (self.dropped_spans, self.dropped_samples) }
    pub fn pending(&self) -> (usize, usize) { (self.spans.len(), self.samples.len()) }

    /// Takes what has accumulated, for the one place that persists it.
    pub fn take_spans(&mut self) -> Vec<Span> { std::mem::take(&mut self.spans) }
    pub fn take_samples(&mut self) -> Vec<crate::measurement::PerceptionSample> {
        std::mem::take(&mut self.samples)
    }

    /// What the window may show about a stage, from what is still in hand.
    pub fn summary(&self, stage: Stage) -> Option<StageSummary> {
        let mut d: Vec<u64> = self.spans.iter().filter(|s| s.stage == stage).map(|s| s.duration_ms()).collect();
        if d.is_empty() { return None; }
        d.sort_unstable();
        Some(StageSummary {
            stage,
            count: d.len() as u64,
            p50_ms: d[d.len() / 2],
            p90_ms: d[(d.len() * 9 / 10).min(d.len() - 1)],
            max_ms: *d.last().unwrap(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageSummary {
    pub stage: Stage,
    pub count: u64,
    pub p50_ms: u64,
    pub p90_ms: u64,
    pub max_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_cannot_carry_what_the_owner_said() {
        let s = Span::new(Stage::ModelPrefill, "what is my wife's name? ask-9", 1.0, 2.5, SpanOutcome::Ok);
        assert_eq!(s.op, "whatismywifesnameask-9", "only identifier characters survive");
        assert!(!s.op.contains(' ') && !s.op.contains('?') && !s.op.contains('\''));
        let long = Span::new(Stage::Response, &"x".repeat(200), 0.0, 0.0, SpanOutcome::Ok);
        assert_eq!(long.op.len(), 24, "and never more than an id's worth of it");
        // A path cannot survive either: the separators are gone.
        let p = Span::new(Stage::ActionExecute, "/Users/someone/Desktop/tax.pdf", 0.0, 1.0, SpanOutcome::Ok);
        assert!(!p.op.contains('/') && !p.op.contains('.'));
    }

    #[test]
    fn duration_is_never_negative_however_the_clock_behaves() {
        let s = Span::new(Stage::ModelQueue, "op1", 10.0, 9.0, SpanOutcome::Ok);
        assert_eq!(s.duration_ms(), 0, "a clock that went backwards is not a negative duration");
        assert_eq!(Span::new(Stage::CoreTick, "op1", 1.0, 1.25, SpanOutcome::Ok).duration_ms(), 250);
    }

    #[test]
    fn the_model_timeline_turns_phases_into_measured_stretches() {
        let mut t = Telemetry::default();
        assert_eq!(t.phase(), ModelPhase::ModelIdle);
        t.model_phase(ModelPhase::ModelQueued, "ask-1", 100.0);
        t.model_phase(ModelPhase::ModelPrefill, "ask-1", 100.2);
        t.model_phase(ModelPhase::ModelGenerating, "ask-1", 118.0);
        t.model_phase(ModelPhase::ModelIdle, "ask-1", 120.0);

        let spans = t.take_spans();
        let by = |st: Stage| spans.iter().find(|s| s.stage == st).map(|s| s.duration_ms());
        assert_eq!(by(Stage::ModelQueue), Some(200));
        assert_eq!(by(Stage::ModelPrefill), Some(17800), "prefill is the stretch this slice is hunting");
        assert_eq!(by(Stage::ModelGeneration), Some(2000));
        assert_eq!(t.phase(), ModelPhase::ModelIdle);
        assert!(t.take_spans().is_empty(), "taken once");
    }

    #[test]
    fn diagnostics_can_never_grow_without_bound() {
        let mut t = Telemetry::new(4, 2);
        for i in 0..10 { t.record(Span::new(Stage::CoreTick, "t", i as f64, i as f64 + 0.1, SpanOutcome::Ok)); }
        assert_eq!(t.pending().0, 4);
        assert_eq!(t.dropped().0, 6, "what was dropped is counted, not hidden");
    }

    #[test]
    fn a_summary_needs_no_content_to_be_useful() {
        let mut t = Telemetry::default();
        for ms in [10.0, 20.0, 30.0, 40.0, 500.0] {
            t.record(Span::new(Stage::VisionAnalyze, "frame", 0.0, ms / 1000.0, SpanOutcome::Ok));
        }
        let s = t.summary(Stage::VisionAnalyze).unwrap();
        assert_eq!((s.count, s.p50_ms, s.max_ms), (5, 30, 500));
        assert!(t.summary(Stage::Speech).is_none(), "no data is None, never a zero that reads like a measurement");
    }

    #[test]
    fn the_stages_that_happen_four_times_a_second_are_not_the_ones_always_kept() {
        assert!(Stage::ModelPrefill.always_worth_keeping());
        assert!(Stage::IntentRouting.always_worth_keeping());
        assert!(!Stage::CoreTick.always_worth_keeping());
        assert!(!Stage::ContextBuild.always_worth_keeping());
        assert!(!Stage::VisionAnalyze.always_worth_keeping(),
            "four a second; the slow ones are kept by duration, not by stage");
    }

    #[test]
    fn an_unknown_stage_is_not_recorded_as_some_other_stage() {
        assert_eq!(Stage::from_tag("MODEL_PREFILL"), Some(Stage::ModelPrefill));
        assert_eq!(Stage::from_tag("SOMETHING_NEW"), None);
        assert_eq!(Stage::ModelPrefill.tag(), "MODEL_PREFILL");
    }
}
