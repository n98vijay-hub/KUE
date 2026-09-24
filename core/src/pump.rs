//! The one path from the engine to local memory.
//!
//! Every write goes through the privacy firewall, and nothing is written at all
//! while KUE is killed. Kept out of the shell so both rules are tested here.

use crate::engine::Engine;
use crate::privacy::Firewall;
use crate::store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTick {
    /// Wrote what was due (possibly nothing).
    Written,
    /// KUE is killed or recovering: nothing was written.
    BlockedByKillSwitch,
}

#[derive(Debug)]
pub struct MemoryPump {
    last_event_id: u64,
    last_snapshot: f64,
    snapshot_every: f64,
}

impl MemoryPump {
    pub fn new(snapshot_every_seconds: f64) -> Self {
        MemoryPump { last_event_id: 0, last_snapshot: 0.0, snapshot_every: snapshot_every_seconds }
    }

    /// Writes new events, a periodic snapshot, and the privacy ledger.
    ///
    /// A refused event is skipped for good, not retried: refusal is a policy
    /// outcome, not a transient failure. A failed write is returned and the
    /// unwritten events are retried next tick.
    pub fn tick(&mut self, engine: &mut Engine, fw: &mut Firewall, st: &Store, now: f64) -> Result<MemoryTick, String> {
        if engine.is_killed() {
            // Events that happen while killed are never written, including ones
            // from before the kill that had not been flushed yet. The kill
            // record itself survives in the latch and is written as part of the
            // Recovered event.
            self.last_event_id = engine.events_since(self.last_event_id).iter()
                .map(|e| e.id).max().unwrap_or(self.last_event_id);
            // Nothing measured while killed is kept either, and it is discarded
            // rather than held: a kill is not a pause with a buffer.
            engine.telemetry().take_samples();
            engine.telemetry().take_spans();
            return Ok(MemoryTick::BlockedByKillSwitch);
        }
        // What the owner asked KUE to keep, and what they asked it to forget.
        // Through the firewall like everything else; a memory policy refuses is
        // dropped rather than held. A kill returned above, so nothing here runs
        // while killed.
        for m in engine.memory_mut().take_pending() {
            if let Some(cleared) = fw.clear_memory(&m, now) {
                st.record_memory(&cleared).map_err(|e| e.to_string())?;
            }
        }
        for id in engine.memory_mut().take_forgotten() {
            st.forget_memory(&id).map_err(|e| e.to_string())?;
        }
        for ev in engine.events_since(self.last_event_id) {
            if let Some(cleared) = fw.clear_event(&ev, now) {
                st.record_event(&cleared).map_err(|e| e.to_string())?;
            }
            self.last_event_id = self.last_event_id.max(ev.id);
        }
        if now - self.last_snapshot >= self.snapshot_every {
            st.record_snapshot(&fw.clear_snapshot(&engine.build_context(now), now)).map_err(|e| e.to_string())?;
            self.last_snapshot = now;
        }
        // KUE's account of its own machinery. Written through the firewall like
        // everything else, and dropped — not retried — if policy refuses it:
        // diagnostics are never a reason to hold data the policy does not allow.
        let samples = engine.telemetry().take_samples();
        if !samples.is_empty() {
            if let Some(cleared) = fw.clear_perception_samples(samples, now) {
                st.record_perception_samples(&cleared).map_err(|e| e.to_string())?;
            }
        }
        // Percentiles over every span are computed in memory; only the slow
        // ones and the rare ones are written down. A tick that took no time is
        // not evidence of anything, and 316,000 rows a day of it would crowd
        // out the ones that are.
        let floor = engine.config.perception.persist_stage_above_ms;
        let spans: Vec<_> = engine.telemetry().take_spans().into_iter()
            .filter(|s| s.stage.always_worth_keeping() || s.duration_ms() >= floor)
            .collect();
        if !spans.is_empty() {
            if let Some(cleared) = fw.clear_stage_timings(spans, now) {
                st.record_stage_timings(&cleared).map_err(|e| e.to_string())?;
            }
        }
        if let Some(rows) = fw.take_ledger(now) {
            st.record_ledger(&rows).map_err(|e| e.to_string())?;
        }
        Ok(MemoryTick::Written)
    }
}
