import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ContextObject, IdentityState } from "../types";

export const IDENTITY_TONE: Record<IdentityState, string> = {
  MY_FACE_CONFIRMED: "affirm",
  UNKNOWN_PERSON: "alert",
  IDENTITY_UNCERTAIN: "caution",
  NO_FACE: "absent",
  MULTIPLE_PEOPLE: "caution",
  NOT_OBSERVING: "absent",
};

const REQUIRED_SAMPLES = 5;

export function Identity({ ctx, onAction }: { ctx: ContextObject; onAction: (m: string) => void }) {
  const [busy, setBusy] = useState(false);
  const id = ctx.identity;
  const enrolled = id.enrolled_samples;
  const canCapture = ctx.sensors.camera_state === "RUNNING" && !ctx.sensors.paused;

  async function capture() {
    setBusy(true);
    try {
      await invoke("enroll_capture");
      onAction("Capturing a sample — hold still for a moment.");
    } catch (e) {
      onAction(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function undo() {
    setBusy(true);
    try {
      await invoke("enroll_undo", { count: 1 });
      onAction("Removed the most recent enrollment sample.");
    } catch (e) {
      onAction(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    setBusy(true);
    try {
      await invoke("enroll_reset");
      onAction("Enrollment cleared. All stored face descriptors were deleted.");
    } catch (e) {
      onAction(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rail-block">
      <div className="label">Identity</div>
      <div className="card">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 10 }}>
          <span className={`chip ${IDENTITY_TONE[id.state]}`}>{id.state.replace(/_/g, " ")}</span>
          {/* A percentage next to "uncertain" reads as a claim about a person.
              Confidence is only meaningful when there is a conclusion to hold. */}
          {id.state === "MY_FACE_CONFIRMED" || id.state === "UNKNOWN_PERSON" ? (
            <span className="conf-value">{(id.confidence.value * 100).toFixed(0)}%</span>
          ) : (
            <span className="conf-value" style={{ color: "var(--ink-4)" }}>—</span>
          )}
        </div>

        {id.held_for_seconds != null && (
          /* The badge above is the last MEASURED result, not this frame's. */
          <div className="held-note">
            carried · last measured {id.held_for_seconds.toFixed(1)}s ago
          </div>
        )}

        <p style={{ marginTop: 12, fontSize: 12, color: "var(--ink-2)" }}>{id.detail}</p>

        <div className="pips" title={`${enrolled} enrolled samples`}>
          {Array.from({ length: REQUIRED_SAMPLES }, (_, i) => (
            <div key={i} className={`pip ${i < enrolled ? "on" : ""}`} />
          ))}
        </div>
        <div className="kv" style={{ borderTop: "none", paddingTop: 0 }}>
          <span className="k">Enrolled samples</span>
          {/* The count is read from the sensing layer; without it, 0 would be a false statement. */}
          <span className="v">
            {ctx.sensors.sensing_process === "DOWN" && enrolled === 0 ? "not read — sensing is off" : enrolled}
          </span>
        </div>
        {(id.geometry_ratio != null || id.featureprint_ratio != null) && (
          <>
            <div className="kv">
              <span className="k">Landmark geometry</span>
              <span className="v">
                {id.geometry_ratio == null ? "—" : `${id.geometry_ratio.toFixed(2)}× spread`}
              </span>
            </div>
            <div className="kv">
              <span className="k">Image feature print</span>
              <span className="v">
                {id.featureprint_ratio == null ? "—" : `${id.featureprint_ratio.toFixed(2)}× spread`}
              </span>
            </div>
            {id.accept_threshold != null && id.reject_threshold != null && (
              <div className="kv">
                <span className="k">Decision bands</span>
                <span className="v dim">
                  accept ≤ {id.accept_threshold.toFixed(2)} · reject ≥{" "}
                  {id.reject_threshold.toFixed(2)}
                </span>
              </div>
            )}
            {id.geometry_ratio != null && id.featureprint_ratio != null && !id.descriptors_agree && (
              <div className="caveat" style={{ marginTop: 10 }}>
                <strong>The two descriptors disagree.</strong> Lantern will not
                claim or deny a match while its own measurements conflict. This
                usually means the lighting has changed since you enrolled —
                capturing a few samples in the current light will help.
              </div>
            )}
          </>
        )}

        <div className="row-actions">
          <button className="btn" onClick={capture} disabled={busy || !canCapture}>
            Capture sample
          </button>
          <button className="btn" onClick={undo} disabled={busy || enrolled === 0}>
            Remove last
          </button>
          <button className="btn" onClick={reset} disabled={busy || enrolled === 0}>
            Clear enrollment
          </button>
        </div>

        <div className="caveat">
          <strong>Read this before trusting a match.</strong> Lantern compares
          facial landmark geometry and an Apple Vision image feature print. Neither
          is a production face-recognition model. Thresholds are calibrated from
          the spread of your own samples only
          {id.reject_side_unvalidated && (
            <> — it has <strong>never been tested against anyone else's face</strong>, so
            “unknown person” is an unvalidated heuristic</>
          )}
          . Enroll across varied lighting and head angles, and treat every match as
          weak evidence rather than proof.
        </div>
      </div>
    </div>
  );
}

export function Memory({ onAction }: { onAction: (m: string) => void }) {
  const [stats, setStats] = useState<{ events?: number; snapshots?: number; path?: string } | null>(null);
  const [confirming, setConfirming] = useState(false);

  async function refresh() {
    try {
      setStats(await invoke("storage_stats"));
    } catch (e) {
      onAction(String(e));
    }
  }

  async function erase() {
    try {
      onAction(String(await invoke("erase_memory")));
      setConfirming(false);
      refresh();
    } catch (e) {
      onAction(String(e));
    }
  }

  return (
    <div className="rail-block">
      <div className="label">Local memory</div>
      <div className="card">
        <p style={{ fontSize: 12, color: "var(--ink-2)" }}>
          Events and context snapshots are stored in a SQLite file on this Mac.
          No image, audio, keystroke, window title, or URL is ever written — the
          sensing layer does not produce them, so there is nothing to store.
        </p>
        {stats && (
          <>
            <div className="kv" style={{ marginTop: 10 }}>
              <span className="k">Events</span>
              <span className="v">{stats.events ?? 0}</span>
            </div>
            <div className="kv">
              <span className="k">Snapshots</span>
              <span className="v">{stats.snapshots ?? 0}</span>
            </div>
          </>
        )}
        <div className="row-actions">
          <button className="btn" onClick={refresh}>
            {stats ? "Refresh" : "Show contents"}
          </button>
          {confirming ? (
            <>
              <button className="btn" onClick={erase}>Confirm erase</button>
              <button className="btn" onClick={() => setConfirming(false)}>Cancel</button>
            </>
          ) : (
            <button className="btn" onClick={() => setConfirming(true)}>Erase everything</button>
          )}
        </div>
      </div>
    </div>
  );
}
