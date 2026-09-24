import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface LedgerRow {
  kind: string;
  class: string;
  destination: string;
  decision: "ALLOW" | "DENY";
  reason: string;
  first_ts: number;
  last_ts: number;
  count: number;
  policy_version: number;
}

interface PrivacyStatus {
  policy_version: number;
  decisions_allowed: number;
  decisions_denied: number;
  legacy_snapshots: number | null;
  ledger: LedgerRow[];
}

const human = (tag: string) => tag.toLowerCase().replace(/_/g, " ");

/**
 * The privacy firewall, made visible.
 *
 * Every write to local memory passes the firewall; this panel reads back its
 * audit ledger. Rows are metadata only — which kind of data, where it was
 * headed, what was decided and how often — never the data itself.
 */
export function Privacy({ onAction }: { onAction: (m: string) => void }) {
  const [status, setStatus] = useState<PrivacyStatus | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    try {
      setStatus(await invoke<PrivacyStatus>("privacy_status"));
    } catch (e) {
      onAction(String(e));
    }
  }

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 4000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function purge() {
    setBusy(true);
    try {
      onAction(String(await invoke("purge_legacy_snapshots")));
      setConfirming(false);
      refresh();
    } catch (e) {
      onAction(String(e));
    } finally {
      setBusy(false);
    }
  }

  const denied = (status?.ledger ?? []).filter((r) => r.decision === "DENY");
  const allowed = (status?.ledger ?? []).filter((r) => r.decision === "ALLOW");
  const legacy = status?.legacy_snapshots ?? 0;

  return (
    <div className="rail-block">
      <div className="label">Privacy firewall</div>
      <div className="card">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 10 }}>
          <span className="chip affirm">Enforcing · policy v{status?.policy_version ?? "—"}</span>
          <span className="cap-note">
            {status ? `${status.decisions_allowed} allowed · ${status.decisions_denied} refused` : "—"}
          </span>
        </div>
        <p style={{ fontSize: 12, color: "var(--ink-2)", marginTop: 10 }}>
          Nothing reaches local memory without passing this firewall. Memory keeps an
          allowlisted summary; body joint positions, hand positions and per-frame
          face measurements are used live and never stored. No data kind may leave
          this Mac under the current policy.
        </p>

        {legacy > 0 && (
          <div className="caveat" style={{ borderColor: "rgba(217,138,131,0.3)", background: "rgba(217,138,131,0.06)" }}>
            <strong>{legacy} snapshot{legacy === 1 ? "" : "s"} predate the firewall.</strong>{" "}
            They were written before this policy existed and may contain body joint
            positions and face measurement tracks. Purging deletes them and compacts
            the database so the content is removed from disk.
            <div className="row-actions">
              {confirming ? (
                <>
                  <button className="btn" disabled={busy} onClick={purge}>Confirm purge</button>
                  <button className="btn" disabled={busy} onClick={() => setConfirming(false)}>Cancel</button>
                </>
              ) : (
                <button className="btn" onClick={() => setConfirming(true)}>Purge pre-firewall snapshots</button>
              )}
            </div>
          </div>
        )}

        {denied.length > 0 && (
          <div style={{ marginTop: 12 }}>
            <div className="label" style={{ marginBottom: 4 }}>Refused</div>
            {denied.map((r) => (
              <div className="kv" key={`${r.kind}-${r.destination}-${r.policy_version}`}>
                <span className="k" title={r.reason}>
                  {human(r.kind)} → {human(r.destination)}
                </span>
                <span className="v">{r.count}×</span>
              </div>
            ))}
          </div>
        )}
        {allowed.length > 0 && (
          <div style={{ marginTop: 12 }}>
            <div className="label" style={{ marginBottom: 4 }}>Allowed</div>
            {allowed.map((r) => (
              <div className="kv" key={`${r.kind}-${r.destination}-${r.policy_version}`}>
                <span className="k">{human(r.kind)} → {human(r.destination)}</span>
                <span className="v dim">{r.count}×</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
