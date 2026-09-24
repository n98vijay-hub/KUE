import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ContextObject, DescriptorSeparation } from "../types";

function Row({ d }: { d: DescriptorSeparation }) {
  const tone =
    d.verdict === "SEPARATED" ? "affirm" : d.verdict === "OVERLAPPING" ? "alert" : "absent";
  return (
    <div style={{ padding: "10px 0", borderTop: "1px solid var(--hairline-soft)" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 10 }}>
        <span style={{ fontSize: 12, color: "var(--ink)" }}>{d.name}</span>
        <span className={`chip ${tone}`}>{d.verdict}</span>
      </div>
      {d.within_owner_max != null && d.owner_vs_probe_min != null && (
        <div className="ev-meta" style={{ marginTop: 5 }}>
          within you up to {d.within_owner_max.toFixed(3)} · nearest other person{" "}
          {d.owner_vs_probe_min.toFixed(3)}
          {d.separation_ratio != null && <> · gap {d.separation_ratio.toFixed(2)}×</>}
        </div>
      )}
    </div>
  );
}

/**
 * The one panel that can falsify Lantern's identity claims.
 *
 * Until this has been run, "UNKNOWN_PERSON" rests on nothing: thresholds are
 * derived from the owner's own samples, which say nothing about anyone else.
 * Probe samples are stored separately and are never used for matching.
 */
export function IdentityCheck({ ctx, onAction }: { ctx: ContextObject; onAction: (m: string) => void }) {
  const [busy, setBusy] = useState(false);
  const r = ctx.identity_check;
  const canCapture = ctx.sensors.camera_state === "RUNNING" && !ctx.sensors.paused;

  async function run(cmd: string, msg: string) {
    setBusy(true);
    try {
      await invoke(cmd);
      onAction(msg);
    } catch (e) {
      onAction(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rail-block">
      <div className="label">Identity check</div>
      <div className="card">
        <p style={{ fontSize: 12, color: "var(--ink-2)" }}>
          Measures whether this matcher can actually tell you apart from someone
          else. Samples captured here are stored separately and are{" "}
          <strong style={{ color: "var(--ink)" }}>never</strong> added to your
          enrolled profile. Only ask someone who has agreed to it — their face
          descriptors are kept on this Mac until you clear them.
        </p>

        {r && r.probe_samples > 0 ? (
          <>
            <div className="kv" style={{ marginTop: 10 }}>
              <span className="k">Your samples</span>
              <span className="v">{r.owner_samples}</span>
            </div>
            <div className="kv">
              <span className="k">Other-person samples</span>
              <span className="v">{r.probe_samples}</span>
            </div>
            <Row d={r.geometry} />
            <Row d={r.feature_print} />
            <div
              className="caveat"
              style={
                r.reject_side_validated
                  ? undefined
                  : { borderColor: "rgba(217,138,131,0.3)", background: "rgba(217,138,131,0.06)" }
              }
            >
              {r.reject_side_validated ? (
                <strong style={{ color: "var(--affirm)" }}>Reject side measured.</strong>
              ) : (
                <strong>Reject side not validated.</strong>
              )}{" "}
              {r.note}
            </div>
          </>
        ) : (
          <div className="caveat" style={{ marginTop: 10 }}>
            <strong>Never measured.</strong> Lantern has no data on anyone's face
            but yours, so it cannot know whether a stranger would read as
            “unknown” or as you. Capture at least 3 samples of another person to
            find out.
          </div>
        )}

        <div className="row-actions">
          <button
            className="btn"
            disabled={busy || !canCapture}
            onClick={() => run("probe_capture", "Capturing a sample of the other person — this will not touch your profile.")}
          >
            Capture another person
          </button>
          <button className="btn" disabled={busy} onClick={() => run("probe_report", "Recomputed.")}>
            Recompute
          </button>
          {r && r.probe_samples > 0 && (
            <button className="btn" disabled={busy} onClick={() => run("probe_reset", "Identity-check samples cleared.")}>
              Clear
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
