/** KUE.
 *
 * One presence, one stream, a quiet column of what KUE understands, and a trust
 * strip that is always visible. Every state and every sentence about KUE comes
 * from the core's projection (`get_surface`); this file arranges them and sends
 * gestures back. It decides nothing about what KUE is doing.
 *
 * Diagnostics is a mode, not a tab: ⌘⇧D, or the control in the title bar. */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ContextObject } from "./types";
import { SOURCE_LABEL } from "./types";
import type { Surface } from "./surface";
import { ActivityLine, Attention, Presence, TrustStrip } from "./components/Presence";
import { CapabilitiesSheet, MemorySheet, Sheet, TrustSheet } from "./components/Sheets";
import type { SheetName } from "./components/Sheets";
import { StorageSheet } from "./components/Storage";
import { Diagnostics } from "./components/Diagnostics";
import { KillButton, KilledScreen } from "./components/Kill";
import { Conversation } from "./components/Conversation";
import { isKilled } from "./killed";

export default function App() {
  const [ctx, setCtx] = useState<ContextObject | null>(null);
  const [surface, setSurface] = useState<Surface | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [diagnostics, setDiagnostics] = useState(false);
  const [sheet, setSheet] = useState<SheetName>(null);
  // IPC_FAILURE is the one failure the core cannot report, because reporting it
  // needs the channel that failed. The window detects it from the clock.
  const [ipcError, setIpcError] = useState("");
  const [lastUpdate, setLastUpdate] = useState(0);
  /** When the projection last arrived. A stale projection is not to be trusted:
   *  it would keep showing "Camera on" long after the camera stopped. */
  const [surfaceAt, setSurfaceAt] = useState(0);
  const [mountedAt] = useState(() => Date.now());
  const [clock, setClock] = useState(() => Date.now());

  useEffect(() => {
    let alive = true;
    const pull = () => {
      invoke<Surface>("get_surface")
        .then((s) => { if (alive) { setSurface(s); setSurfaceAt(Date.now()); } })
        .catch(() => {});
    };
    const received = (c: ContextObject) => {
      if (!alive) return;
      setCtx(c);
      setLastUpdate(Date.now());
      setIpcError("");
      pull();
    };
    invoke<ContextObject>("get_context").then(received).catch((e) => alive && setIpcError(String(e)));
    pull();

    let un: Promise<() => void> | null = null;
    try {
      un = listen<ContextObject>("lantern://context", (e) => received(e.payload));
      un.catch((e) => alive && setIpcError(String(e)));
    } catch (e) {
      setIpcError(String(e));
    }
    // An action or a spoken sentence can change what KUE is doing without the
    // context object changing, so the projection is also pulled on a tick.
    const tick = setInterval(() => { setClock(Date.now()); pull(); }, 1000);
    return () => {
      alive = false;
      clearInterval(tick);
      un?.then((f) => f()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "d") {
        e.preventDefault();
        setDiagnostics((d) => !d);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const togglePause = useCallback(async () => {
    if (!ctx) return;
    setBusy(true);
    try { await invoke("set_paused", { paused: !ctx.sensors.paused }); setNote(""); }
    catch (e) { setNote(String(e)); }
    finally { setBusy(false); }
  }, [ctx]);

  const confirm = useCallback(async (id: string) => {
    setBusy(true);
    try { await invoke("confirm_action", { id }); }
    catch (e) { setNote(String(e)); }
    finally { setBusy(false); }
  }, []);

  const cancel = useCallback(async (id: string) => {
    setBusy(true);
    try { await invoke("cancel_action", { id }); }
    catch (e) { setNote(String(e)); }
    finally { setBusy(false); }
  }, []);

  // ---- before the core has said anything ----

  if (!ctx || !surface) {
    const failed = ipcError !== "" || clock - mountedAt > 5000;
    return (
      <div className="kue">
        <div className="kue-top"><span className="kue-name">KUE</span></div>
        <div className="kue-body">
          <div className="kue-stream"><div className="kue-stream-inner">
            {failed ? (
              <div className="presence">
                <span className="presence-glyph blocked" role="img" aria-label="Not connected" />
                <div>
                  <div className="presence-said">I've lost contact with my own core.</div>
                  <div className="presence-detail">
                    Nothing has reached this window, so it cannot show what KUE observes.
                    {ipcError ? ` ${ipcError}` : ""}
                  </div>
                </div>
              </div>
            ) : (
              <div className="presence">
                <span className="presence-glyph off" role="img" aria-label="Starting" />
                <div className="presence-said">Starting…</div>
              </div>
            )}
          </div></div>
        </div>
      </div>
    );
  }

  const killed = isKilled(surface, ctx);
  const stalledFor = lastUpdate ? (clock - lastUpdate) / 1000 : null;
  /** A trust signal is a safety claim. Past a few seconds without a fresh
   *  projection, KUE says it does not know rather than repeating itself. */
  const trustStale = surfaceAt > 0 && clock - surfaceAt > 4000;

  return (
    <div className="kue">
      <div className="kue-top">
        <span className="kue-name">KUE</span>
        <span className="attention-row" style={{ marginLeft: "auto" }}>
          <button className="kue-btn plain" onClick={() => setDiagnostics((d) => !d)}
                  aria-pressed={diagnostics} title="⌘⇧D">
            {diagnostics ? "Done" : "Diagnostics"}
          </button>
          {!killed && (
            <button className="kue-btn" onClick={togglePause} disabled={busy}>
              {ctx.sensors.paused ? "Start watching" : "Pause"}
            </button>
          )}
          {!killed && <KillButton onAction={setNote} />}
        </span>
      </div>

      {/* Its own row: the trust strip is always visible, and must never be the
          thing that gets squeezed out when the window is narrow. */}
      {!killed && (
        <div className="kue-trust-row">
          <TrustStrip s={surface} stale={trustStale} onOpen={() => setSheet("trust")} />
        </div>
      )}

      {killed ? (
        <div className="kue-body"><div className="kue-stream"><div className="kue-stream-inner">
          <KilledScreen runtime={ctx.runtime} onAction={setNote} />
        </div></div></div>
      ) : diagnostics ? (
        <Diagnostics ctx={ctx} onAction={setNote} />
      ) : (
        <div className="kue-body">
          <div className="kue-stream">
            <div className="kue-stream-inner">
              <Presence s={surface} />
              <ActivityLine s={surface} />

              {stalledFor != null && stalledFor > 3 && (
                <div className="attention">
                  <span className="attention-said">
                    I've heard nothing from my own core for {stalledFor.toFixed(0)}s.
                    What you see may be out of date.
                  </span>
                </div>
              )}

              {surface.attention.map((a, i) => (
                <Attention key={`${a.kind}-${a.action_id ?? i}`} item={a} busy={busy}
                           onConfirm={confirm} onCancel={cancel}
                           onSettings={(permission) => invoke("open_permission_settings", { permission })} />
              ))}

              {note && (
                <div className="attention">
                  <span className="attention-said">{note}</span>
                  <span className="attention-row">
                    <button className="kue-btn plain" onClick={() => setNote("")}>Dismiss</button>
                  </span>
                </div>
              )}

              <Conversation voice={ctx.voice} onAction={setNote} />
            </div>
          </div>

          <div className="kue-context">
            <div className="ctx-label">What I understand</div>
            {/* Sentences from the core, each labelled with whether it was seen
                or worked out. Measurements belong in Diagnostics. */}
            {surface.context.map((c, i) => (
              <div className="ctx-item" key={i}>
                <span className="ctx-basis">{c.basis === "OBSERVED" ? "seen" : "worked out"}</span>
                <span>{c.sentence}</span>
              </div>
            ))}

            <div className="ctx-label" style={{ marginTop: 24 }}>More</div>
            <div className="attention-row" style={{ flexWrap: "wrap", gap: 8 }}>
              <button className="kue-btn plain" onClick={() => setSheet("storage")}>What's filling up this Mac</button>
              <button className="kue-btn plain" onClick={() => setSheet("capabilities")}>What I can do</button>
              <button className="kue-btn plain" onClick={() => setSheet("memory")}>What I remember</button>
              <button className="kue-btn plain" onClick={() => setSheet("trust")}>What I can sense</button>
              <button className="kue-btn plain" onClick={() => setSheet("context")}>Where this comes from</button>
            </div>
          </div>
        </div>
      )}

      {sheet === "trust" && <TrustSheet s={surface} onClose={() => setSheet(null)} />}
      {sheet === "capabilities" && <CapabilitiesSheet onClose={() => setSheet(null)} />}
      {sheet === "storage" && <StorageSheet onClose={() => setSheet(null)} />}
      {sheet === "memory" && <MemorySheet ctx={ctx} onClose={() => setSheet(null)} onAction={setNote} />}
      {sheet === "context" && (
        <Sheet title="Where this comes from" onClose={() => setSheet(null)}>
          {ctx.observations.map((o) => (
            <div className="sheet-row" key={o.id}>
              <div style={{ flex: 1 }}>
                <div className="sheet-row-name">{o.statement}</div>
                <div className="sheet-row-said">
                  {SOURCE_LABEL[o.source]}
                  {o.age_seconds > 0.5 ? ` · ${o.age_seconds.toFixed(1)}s ago` : ""}
                </div>
              </div>
            </div>
          ))}
          <div className="sheet-group">What I don't know</div>
          {ctx.unknowns.map((u, i) => <div className="sheet-row-said" key={i} style={{ marginBottom: 8 }}>{u}</div>)}
        </Sheet>
      )}
    </div>
  );
}
