/** What is filling up this Mac, and what KUE found worth reviewing.
 *
 * Every sentence, size and heading here was written by the core when it took
 * the measurement (`get_storage`). This file arranges them. It computes no
 * total, rounds no number and describes no finding — if a sentence is missing,
 * the window shows nothing rather than inventing one.
 *
 * Each finding shows the same four things, in the same order: what it is, what
 * was measured, what KUE worked out from that, and what it would cost if KUE
 * were wrong. The last one is not a footnote — a recommendation without its
 * downside is an instruction. */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sheet } from "./Sheets";

export interface Candidate {
  path: string;
  name: string;
  area: string;
  size_said: string;
  where_said: string;
  category: string;
  evidence: string;
  reason: string;
  basis: "OBSERVED" | "INFERRED";
  caution: string;
  risk: string;
  reversible: boolean;
  recommended: string;
}

export interface StorageReport {
  measured_at: number;
  summary: { volume: { capacity: number; available: number; used: number } | null; areas: { name: string; bytes: number; files: number }[] };
  candidates: Candidate[];
  found: number;
  counts: { category: string; files: number; bytes: number; heading: string; said: string }[];
  reclaimable: number;
  scanned: number;
  truncated: boolean;
  unreadable: string[];
}

export interface StorageView {
  report: StorageReport | null;
  withheld: string | null;
  said: { headline: string; finding: string; areas: string[]; limits: string[]; measured: string; cannot: string } | null;
  can_undo: number;
}

/** Why there is nothing to show, in the owner's words rather than the code's. */
const WITHHELD: Record<string, string> = {
  KILLED: "KUE is stopped. It is reading nothing about this Mac.",
  AUTHORIZATION_REQUIRED: "KUE shows this to you, and only to you. Face the camera, or use Touch ID.",
  PRIVACY_DENIED: "The privacy firewall withheld this.",
};

export function StorageSheet({ onClose }: { onClose: () => void }) {
  const [view, setView] = useState<StorageView | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState("");
  const [chosen, setChosen] = useState<string[]>([]);

  const load = useCallback(() => {
    invoke<StorageView>("get_storage").then(setView).catch((e) => setFailed(String(e)));
  }, []);
  useEffect(load, [load]);

  const check = useCallback(async () => {
    setBusy(true);
    setFailed("");
    setChosen([]);
    try { setView(await invoke<StorageView>("check_storage")); }
    catch (e) { setFailed(String(e)); }
    finally { setBusy(false); }
  }, []);

  /// Both of these only ASK. KUE decides whether the request is allowed, and
  /// the confirmation happens in the window where every other decision does —
  /// so the sheet closes rather than growing a second place to agree to things.
  const trash = useCallback(async () => {
    setBusy(true);
    try { await invoke("trash_selected", { paths: chosen }); onClose(); }
    catch (e) { setFailed(String(e)); }
    finally { setBusy(false); }
  }, [chosen, onClose]);

  const undo = useCallback(async () => {
    setBusy(true);
    try { await invoke("undo_trash"); onClose(); }
    catch (e) { setFailed(String(e)); }
    finally { setBusy(false); }
  }, [onClose]);

  return (
    <Sheet title="Your storage" onClose={onClose}>
      <StorageBody view={view} busy={busy} failed={failed} onCheck={check}
                   chosen={chosen} onChoose={setChosen} onTrash={trash} onUndo={undo} />
    </Sheet>
  );
}

/** The sheet's contents, from the projection alone. Separated from the fetching
 *  so that what the window is allowed to put on screen can be tested. */
export function StorageBody({ view, busy, failed, onCheck, chosen = [], onChoose, onTrash, onUndo }: {
  view: StorageView | null; busy: boolean; failed: string; onCheck: () => void;
  chosen?: string[]; onChoose?: (paths: string[]) => void; onTrash?: () => void; onUndo?: () => void;
}) {
  const report = view?.report ?? null;
  const said = view?.said ?? null;
  const pick = (path: string) =>
    onChoose?.(chosen.includes(path) ? chosen.filter((p) => p !== path) : [...chosen, path]);

  return (
    <>
      {failed && <div className="sheet-row-said">{failed}</div>}

      {view?.withheld ? (
        <div className="sheet-row-said">{WITHHELD[view.withheld] ?? view.withheld}</div>
      ) : !report || !said ? (
        <>
          <div className="sheet-row-said">
            KUE has not looked yet. It will read the sizes and dates in your Desktop, Documents,
            Downloads and KUE folders — never what is inside a file — and tell you what it found.
          </div>
          <div className="attention-row" style={{ marginTop: "var(--s-4)" }}>
            <button className="kue-btn" onClick={onCheck} disabled={busy}>{busy ? "Looking…" : "Take a look"}</button>
          </div>
        </>
      ) : (
        <>
          <div className="storage-headline">{said.headline}</div>
          {report.summary.volume && (
            <div className="storage-bar" role="img" aria-label={said.headline}>
              <span className="storage-bar-fill"
                    style={{ width: `${Math.min(100, Math.round((report.summary.volume.used / report.summary.volume.capacity) * 100))}%` }} />
            </div>
          )}

          <div className="storage-measured">{said.measured}</div>

          <div className="sheet-group">Where it is</div>
          {said.areas.map((line) => <div className="sheet-row-said" key={line}>{line}</div>)}
          {said.limits.map((line) => <div className="storage-limit" key={line}>{line}</div>)}

          <div className="sheet-group">What I found</div>
          <div className="storage-finding">{said.finding}</div>

          {report.counts.map((c) => (
            <div key={c.category}>
              <div className="storage-kind">
                <span className="storage-kind-name">{c.heading}</span>
                <span className="sheet-row-said">{c.said}</span>
              </div>
              {report.candidates.filter((x) => x.category === c.category).map((x) => (
                <div className="storage-item" key={x.path}>
                  <div className="storage-item-head">
                    <label className="storage-pick">
                      <input type="checkbox" checked={chosen.includes(x.path)} onChange={() => pick(x.path)}
                             aria-label={`Choose ${x.name}`} />
                      <span className="storage-item-name" title={x.path}>{x.name}</span>
                    </label>
                    <span className="sheet-row-said">{x.size_said}</span>
                  </div>
                  <div className="sheet-row-said">{x.where_said}</div>
                  <div className="storage-because">
                    <span className="ctx-basis">measured</span>
                    <span>{x.evidence}</span>
                  </div>
                  <div className="storage-because">
                    <span className="ctx-basis">{x.basis === "OBSERVED" ? "seen" : "worked out"}</span>
                    <span>{x.reason}</span>
                  </div>
                  <div className="storage-because">
                    <span className="ctx-basis caution">if I'm wrong</span>
                    <span>{x.caution}</span>
                  </div>
                </div>
              ))}
            </div>
          ))}

          <div className="sheet-group">What I never do</div>
          <div className="sheet-row-said">{said.cannot}</div>

          <div className="attention-row" style={{ marginTop: "var(--s-5)", flexWrap: "wrap" }}>
            <button className="kue-btn" onClick={onTrash} disabled={busy || chosen.length === 0}>
              {chosen.length === 0 ? "Choose what to move" : `Move ${chosen.length} to the Trash`}
            </button>
            {chosen.length > 0 && (
              <button className="kue-btn plain" onClick={() => onChoose?.([])} disabled={busy}>Clear</button>
            )}
            {(view?.can_undo ?? 0) > 0 && (
              <button className="kue-btn plain" onClick={onUndo} disabled={busy}>
                Put back the {view!.can_undo} I moved
              </button>
            )}
            <button className="kue-btn plain" onClick={onCheck} disabled={busy}>{busy ? "Looking…" : "Look again"}</button>
          </div>
          {chosen.length > 0 && (
            <div className="sheet-row-said" style={{ marginTop: "var(--s-2)" }}>
              KUE will ask you to confirm, and macOS will ask for Touch ID, before anything moves.
            </div>
          )}
        </>
      )}
    </>
  );
}
