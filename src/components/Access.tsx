import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AccessBlock, AccessStateName } from "../types";

const TONE: Record<AccessStateName, string> = {
  AUTHORIZED_USER: "affirm",
  AUTHORIZED_USER_LOW_CONFIDENCE: "caution",
  NO_PERSON: "absent",
  AUTHENTICATION_REQUIRED: "absent",
  LOCKED: "absent",
  IDENTITY_UNCERTAIN: "caution",
  UNKNOWN_PERSON: "alert",
  MULTIPLE_PEOPLE: "alert",
};

const LEVEL_MEANING: Record<string, string> = {
  LEVEL_0: "public",
  LEVEL_1: "owner presence",
  LEVEL_2: "owner identity confirmed",
  LEVEL_3: "strong OS authentication",
  LEVEL_4: "physical confirmation",
};

const human = (tag: string) => tag.toLowerCase().replace(/_/g, " ");

/**
 * Authorization, made visible: what the camera and macOS currently allow, and
 * what each operation requires. KUE never sees a password or fingerprint.
 */
export function Access({ access, onAction }: { access: AccessBlock; onAction: (m: string) => void }) {
  const [busy, setBusy] = useState(false);
  const [hardware, setHardware] = useState<{ biometry?: string; strong_available?: boolean } | null>(null);
  const [showReqs, setShowReqs] = useState(false);

  useEffect(() => {
    invoke<{ biometry?: string; strong_available?: boolean }>("auth_hardware").then(setHardware).catch(() => {});
  }, []);

  const run = async (cmd: string) => {
    setBusy(true);
    try { const r = await invoke<string | null>(cmd); onAction(r ? String(r) : ""); }
    catch (e) { onAction(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="rail-block">
      <div className="label">Access</div>
      <div className="card">
        <div className="kv" style={{ borderTop: "none", paddingTop: 0 }}>
          <span className={`chip ${TONE[access.state]}`}>{access.state.replace(/_/g, " ")}</span>
          <span className="v">{access.level}</span>
        </div>
        <p className="cap-note" style={{ margin: "8px 0 4px" }}>{access.detail}</p>
        <div className="kv">
          <span className="k">Level means</span>
          <span className="v">{LEVEL_MEANING[access.level]}</span>
        </div>
        <div className="kv">
          <span className="k">Session</span>
          <span className="v">{human(access.phase)}</span>
        </div>
        <div className="kv">
          <span className="k">Identity basis</span>
          <span className="v">
            {human(access.basis)}
            {access.held_without_measurement_seconds != null
              ? ` · held ${access.held_without_measurement_seconds.toFixed(0)}s without a measurement` : ""}
          </span>
        </div>
        <div className="kv">
          <span className="k">You last confirmed</span>
          <span className="v">
            {access.owner_last_confirmed_seconds_ago == null ? "—" : `${access.owner_last_confirmed_seconds_ago.toFixed(0)}s ago`}
          </span>
        </div>
        <div className="kv">
          <span className="k">macOS authentication</span>
          <span className="v">
            {access.os_auth && access.os_auth_expires_in_seconds != null
              ? `${human(access.os_auth)} · ${Math.ceil(access.os_auth_expires_in_seconds / 60)} min left`
              : "none"}
          </span>
        </div>
        <div className="kv">
          <span className="k">Hardware</span>
          <span className="v">
            {hardware ? `${hardware.biometry === "TOUCH_ID" ? "Touch ID" : hardware.biometry ?? "unknown"}${hardware.strong_available ? "" : " · unavailable"}` : "—"}
          </span>
        </div>
        <div className="killed-actions">
          <button className="btn" disabled={busy} onClick={() => run("authenticate")}>Authenticate with Touch ID</button>
          <button className="btn" disabled={busy} onClick={() => run("lock_session")}>Lock</button>
        </div>
        <button className="disclosure" style={{ marginTop: 12 }} onClick={() => setShowReqs((s) => !s)}>
          {showReqs ? "Hide" : "Show"} what each operation requires
        </button>
        {showReqs && (
          <div style={{ marginTop: 8 }}>
            {access.requirements.map((r) => (
              <div className="kv" key={r.operation}>
                <span className="k">{human(r.operation)}</span>
                <span className="v">
                  {r.level}{r.fresh ? " · fresh" : ""}{r.owner_gesture_only ? " · owner only" : ""}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
