import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RuntimeBlock } from "../types";

const at = (ts: number | null) => (ts ? new Date(ts * 1000).toLocaleString() : "unknown time");

/** One press, no confirmation: stopping is always safe. Undoing it is not one press. */
export function KillButton({ onAction, disabled }: { onAction: (s: string) => void; disabled?: boolean }) {
  const [busy, setBusy] = useState(false);
  return (
    <button
      className="btn btn-kill"
      disabled={busy || disabled}
      title="Terminates sensing and blocks memory writes. Survives a relaunch. Recovery takes two deliberate steps."
      onClick={async () => {
        setBusy(true);
        try { await invoke("kill_kue"); onAction(""); }
        catch (e) { onAction(String(e)); }
        finally { setBusy(false); }
      }}
    >
      Kill
    </button>
  );
}

export function KilledScreen({ runtime, onAction }: { runtime: RuntimeBlock; onAction: (s: string) => void }) {
  const [busy, setBusy] = useState(false);
  const recovering = runtime.state === "KUE_RECOVERING";
  const run = (cmd: string) => async () => {
    setBusy(true);
    try { await invoke(cmd); onAction(""); }
    catch (e) { onAction(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="killed">
      <div className="hero-kicker">
        <span className="label">Runtime</span>
        <span className="chip alert">{runtime.state}</span>
      </div>
      <h1 className="hero-statement">
        {recovering ? "Recovering — still killed." : "Killed."}{" "}
        <span className="qualifier">Nothing is running.</span>
      </h1>
      <p className="hero-detail">
        The sensing process was terminated, not paused, so the camera is released. Nothing is
        observed, analysed, or written to local memory. Relaunching Lantern starts nothing.
      </p>

      <div className="killed-record">
        <div><span className="k">Killed</span>{at(runtime.killed_at)}</div>
        <div><span className="k">By</span>{runtime.killed_by ?? "unknown"}</div>
        <div><span className="k">Reason</span>{runtime.reason ?? "—"}</div>
      </div>

      {runtime.latch_error && (
        <div className="banner" style={{ marginTop: 18 }}>
          <span><strong>KILL_LATCH_NOT_SAVED.</strong> {runtime.latch_error}</span>
        </div>
      )}

      <div className="section">
        <div className="label">Recovery · owner only</div>
        {!recovering ? (
          <>
            <p className="cap-note">
              Recovery takes two deliberate steps in this window. A model, an automation, or a
              latch file deleted from outside cannot recover KUE.
            </p>
            <div className="killed-actions">
              <button className="btn" disabled={busy} onClick={run("begin_recovery")}>Begin recovery</button>
            </div>
          </>
        ) : (
          <>
            <p className="cap-note">
              Completing recovery clears the kill latch and starts the sensing layer and camera again.
            </p>
            <div className="killed-actions">
              <button className="btn btn-pause" disabled={busy} onClick={run("complete_recovery")}>
                Complete recovery — start KUE
              </button>
              <button className="btn" disabled={busy} onClick={run("cancel_recovery")}>Stay killed</button>
            </div>
          </>
        )}
      </div>

      {runtime.latch_path && (
        <div className="section">
          <div className="label">Kill from outside the app</div>
          <p className="cap-note">
            Creating this file kills a running KUE within a second, and makes the next launch start killed:
          </p>
          <pre className="inspector">touch "{runtime.latch_path}"</pre>
        </div>
      )}
    </div>
  );
}
