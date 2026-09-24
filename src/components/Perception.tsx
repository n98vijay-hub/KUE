/** How the measuring is going — KUE's account of its own machinery.
 *
 * This is the window's half of the perception-observability slice. It renders
 * only what the core measured: a measurement state, the pipeline's heartbeat,
 * stage timings, and what local memory costs. Nothing here is computed in the
 * window, and nothing is shown that was not measured — an absent number is
 * absent, never a zero that reads like a measurement.
 *
 * It lives in Diagnostics on purpose. The owner should not need to read
 * milliseconds to trust KUE; an engineer diagnosing why KUE lost the owner
 * does. */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface PipelineHealth {
  ts: number;
  capture_running: boolean;
  vision_busy: boolean;
  loop_alive: boolean;
  last_capture_at: number | null;
  last_analyzed_at: number | null;
  analyze_ms_last: number;
  analyze_ms_p50: number;
  analyze_ms_max: number;
  capture_gap_ms_max: number;
  frames_captured: number;
  frames_analyzed: number;
  frames_dropped: number;
}

export interface StageSummary {
  stage: string;
  count: number;
  p50_ms: number;
  p90_ms: number;
  max_ms: number;
}

export interface StoreFootprint {
  events: number;
  snapshots: number;
  perception_samples: number;
  stage_timings: number;
  ledger_rows: number;
  file_bytes: number;
  reusable_bytes: number;
}

export interface PerceptionReport {
  measurement?: string;
  measurementSaid?: string;
  pipeline?: PipelineHealth | null;
  modelPhase?: string;
  stages?: StageSummary[];
  dropped?: { spans: number; samples: number };
  store?: StoreFootprint | null;
}

/** Bytes as a person reads them. Decimal, as macOS counts. */
function size(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return `${bytes} B`;
}

/** The same tones the identity chip uses, so one vocabulary reads across the
 *  view: measuring well is affirm, late or contradicted is caution, and a
 *  camera that is off is simply absent — not an alarm. */
const TONE: Record<string, string> = {
  MEASUREMENT_FRESH: "affirm",
  NO_PERSON: "absent",
  MEASUREMENT_DELAYED: "caution",
  MEASUREMENT_STALE: "caution",
  MEASUREMENT_CONFLICT: "alert",
  MEASUREMENT_AMBIGUOUS: "caution",
  CAMERA_UNAVAILABLE: "absent",
  CAMERA_PAUSED: "absent",
  CAMERA_KILLED: "absent",
};

export function Perception({ report }: { report: PerceptionReport }) {
  const p = report.pipeline ?? null;
  const stages = report.stages ?? [];
  const store = report.store ?? null;
  return (
    <div className="section">
      <div className="label">How the measuring is going</div>
      <div className="plain-list">
        <div className="plain-item">
          <span className={`chip ${TONE[report.measurement ?? ""] ?? "absent"}`}>
            {(report.measurement ?? "UNKNOWN").replace(/_/g, " ")}
          </span>
          <span>{report.measurementSaid ?? "No assessment yet."}</span>
        </div>
        <div className="plain-item">
          <span className="bullet">·</span>
          <span>
            Model: {(report.modelPhase ?? "MODEL_IDLE").replace(/_/g, " ").toLowerCase()}
          </span>
        </div>
      </div>

      {p ? (
        <div className="plain-list">
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>
              Pipeline {p.loop_alive ? "iterating" : "not iterating"}
              {p.vision_busy ? ", Vision busy" : ""}
              {p.capture_running ? ", capture running" : ", capture stopped"}
            </span>
          </div>
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>
              Analysis {Math.round(p.analyze_ms_last)} ms last · {Math.round(p.analyze_ms_p50)} ms median ·{" "}
              {Math.round(p.analyze_ms_max)} ms worst
            </span>
          </div>
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>
              Frames {p.frames_analyzed} analysed of {p.frames_captured} captured
              {p.frames_dropped > 0 ? ` · ${p.frames_dropped} superseded before analysis` : ""}
              {p.capture_gap_ms_max > 0 ? ` · longest gap ${Math.round(p.capture_gap_ms_max)} ms` : ""}
            </span>
          </div>
        </div>
      ) : (
        <div className="plain-list">
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>The sensing layer has not reported its pipeline yet.</span>
          </div>
        </div>
      )}

      {stages.length > 0 && (
        <>
          <div className="label" style={{ marginTop: 16 }}>Stages, since this launch</div>
          <div className="plain-list">
            {stages.map((s) => (
              <div className="plain-item" key={s.stage}>
                <span className="bullet">·</span>
                <span>
                  {s.stage.replace(/_/g, " ").toLowerCase()} — {s.count} measured · {s.p50_ms} ms median ·{" "}
                  {s.p90_ms} ms p90 · {s.max_ms} ms worst
                </span>
              </div>
            ))}
          </div>
        </>
      )}

      {store && (
        <div className="plain-list">
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>
              Local memory {size(store.file_bytes)} — {store.events} events, {store.snapshots} snapshots,{" "}
              {store.perception_samples} perception samples, {store.stage_timings} stage timings
              {store.reusable_bytes > 0 ? ` · ${size(store.reusable_bytes)} of the file is free space it will reuse` : ""}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}

/** Polls the core for the perception report while Diagnostics is open. */
export function usePerception(active: boolean): PerceptionReport {
  const [report, setReport] = useState<PerceptionReport>({});
  useEffect(() => {
    if (!active) return;
    let alive = true;
    const pull = () => {
      invoke<PerceptionReport>("get_perception")
        .then((r) => { if (alive) setReport(r); })
        .catch(() => {});
    };
    pull();
    const t = setInterval(pull, 1000);
    return () => { alive = false; clearInterval(t); };
  }, [active]);
  return report;
}
