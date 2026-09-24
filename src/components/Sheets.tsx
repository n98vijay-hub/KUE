/** Sheets: the explanations, opened from the thing they explain.
 *
 * They dim the stream rather than replacing it, close on Escape, and never
 * nest. Their content comes from the core — the capability registry, the
 * context object, the privacy status — never from wording kept here. */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ContextObject } from "../types";
import type { Surface } from "../surface";

export type SheetName = "trust" | "capabilities" | "memory" | "context" | "storage" | null;

export function Sheet({ title, onClose, children }: {
  title: string; onClose: () => void; children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Focus moves in on open and Escape closes, so a sheet is never a trap.
    // preventScroll matters: focusing the panel scrolled its body to wherever
    // the browser decided, so the sheet opened halfway down its own list.
    ref.current?.focus({ preventScroll: true });
    if (bodyRef.current) bodyRef.current.scrollTop = 0;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <>
      <div className="sheet-scrim" onClick={onClose} />
      <div className="sheet" role="dialog" aria-label={title} aria-modal="true" tabIndex={-1} ref={ref}>
        <div className="sheet-head">
          <span className="sheet-title">{title}</span>
          <button className="kue-btn plain" onClick={onClose} aria-label="Close">Close</button>
        </div>
        <div className="sheet-body" ref={bodyRef}>{children}</div>
      </div>
    </>
  );
}

/** What KUE can sense, and what never leaves this Mac. */
export function TrustSheet({ s, onClose }: { s: Surface; onClose: () => void }) {
  const rows: [string, { state: string; word: string }, string][] = [
    ["Camera", s.trust.camera, "KUE looks for a face to know whether you are there. No frame is stored or sent anywhere."],
    ["Microphone", s.trust.microphone, "The microphone runs while you use the speak control and, only if you turn it on, while KUE listens for its name — this row says which. The audio is never kept."],
    ["This Mac", s.trust.computer, "KUE sees which application is in front. It never reads window titles, documents or web addresses."],
    ["External AI", s.trust.external_ai, "Nothing about you may reach a model outside this Mac. The privacy policy permits no kind of data to leave."],
    ["Memory", s.trust.memory, "KUE keeps what it works out — that you were here, which app was in front — on this Mac. You can erase all of it."],
  ];
  return (
    <Sheet title="What KUE can sense" onClose={onClose}>
      {rows.map(([name, sig, said]) => (
        <div className="sheet-row" key={name}>
          <div style={{ flex: 1 }}>
            <div className="sheet-row-name">{name}</div>
            <div className="sheet-row-said">{said}</div>
          </div>
          <span className="pill">{sig.word}</span>
        </div>
      ))}
      <div className="sheet-group">Never collected</div>
      <div className="sheet-row-said">
        What you type, what is on your clipboard, what is on your screen, the titles of your
        windows, the addresses you visit, and the contents of the files KUE opens for you.
        None of these are read, and there is no code in KUE that could.
      </div>
    </Sheet>
  );
}

export interface CapabilityView {
  id: string;
  group: string;
  name: string;
  description: string;
  limits: string;
  status: "REAL" | "PARTIAL" | "SIMULATED" | "PLACEHOLDER" | "NOT_IMPLEMENTED";
  availability: { state: string; reason?: string };
  permissions: [string, string][];
  verification: string;
  proof:
    | { level: "LIVE_VERIFIED"; on: string; seen: string }
    | { level: "PARTLY_LIVE_VERIFIED"; on: string; seen: string; not_seen: string }
    | { level: "TEST_VERIFIED_ONLY" }
    | { level: "NOT_APPLICABLE" };
}

/** Whether it has been seen working on this Mac, in plain words — from the
 *  registry's proof, never from its status. Nothing for what does not exist. */
export function proofLine(p: CapabilityView["proof"]): string | null {
  switch (p.level) {
    case "LIVE_VERIFIED":
      return "Seen working on this Mac.";
    case "PARTLY_LIVE_VERIFIED":
      return `Partly seen working on this Mac. Not yet seen: ${p.not_seen}`;
    case "TEST_VERIFIED_ONLY":
      return "Checked by automated tests. Not yet seen working on this Mac.";
    default:
      return null;
  }
}

const GROUP_NAME: Record<string, string> = {
  SENSES: "What KUE can sense",
  IDENTITY: "Knowing it's you",
  UNDERSTANDING: "Working things out",
  DOING: "Doing things on this Mac",
  VOICE: "Listening and speaking",
  MEMORY: "Remembering",
  SAFETY: "Safety",
};

// The status in plain words. The keys are the values core actually serialises —
// getting them wrong showed the enum name itself ("NOT_IMPLEMENTED") to a
// person, which is exactly what the normal experience must never do.
export const STATUS_PILL: Record<CapabilityView["status"], [string, string]> = {
  REAL: ["", "Working"],
  PARTIAL: ["partial", "With limits"],
  SIMULATED: ["partial", "Simulated"],
  PLACEHOLDER: ["partial", "Placeholder"],
  NOT_IMPLEMENTED: ["missing", "Can't do this"],
};

/** Straight from the capability registry, so this sheet, KUE's spoken answer
 *  and the model's context cannot disagree about what KUE can do. */
export function CapabilitiesSheet({ onClose }: { onClose: () => void }) {
  const [caps, setCaps] = useState<CapabilityView[] | null>(null);
  const [failed, setFailed] = useState("");
  useEffect(() => {
    invoke<CapabilityView[]>("get_capability_sheet").then(setCaps).catch((e) => setFailed(String(e)));
  }, []);

  if (failed) return <Sheet title="What KUE can do" onClose={onClose}><p>{failed}</p></Sheet>;
  if (!caps) return <Sheet title="What KUE can do" onClose={onClose}><p className="sheet-row-said">Reading the list…</p></Sheet>;

  const groups = Array.from(new Set(caps.map((c) => c.group)));
  return (
    <Sheet title="What KUE can do" onClose={onClose}>
      {groups.map((g) => (
        <div key={g}>
          <div className="sheet-group">{GROUP_NAME[g] ?? g}</div>
          {caps.filter((c) => c.group === g).map((c) => {
            const [tone, word] = STATUS_PILL[c.status] ?? ["missing", "Unknown"];
            const blocked = c.availability.state === "UNAVAILABLE" ? c.availability.reason : null;
            return (
              <div className="sheet-row" key={c.id}>
                <div style={{ flex: 1 }}>
                  <div className="sheet-row-name">{c.name}</div>
                  <div className="sheet-row-said">{c.description}</div>
                  {c.limits && <div className="sheet-row-limit">{c.limits}</div>}
                  {blocked && <div className="sheet-row-limit">{blocked}</div>}
                  {proofLine(c.proof) && <div className="sheet-row-limit">{proofLine(c.proof)}</div>}
                </div>
                <span className={`pill ${tone}`}>{word}</span>
              </div>
            );
          })}
        </div>
      ))}
    </Sheet>
  );
}

/** What KUE keeps, as the window shows it. Pure: every word comes from core —
 *  the statement, the heading it belongs under, and why it is kept. Where core
 *  has no reason recorded, this says so rather than writing one. */
export function MemoryList({ memory, busy, onForget }: {
  memory: MemoryReport | null; busy?: boolean; onForget: (id: string) => void;
}) {
  if (memory && memory.visible === false) return <div className="sheet-row-said">{memory.withheld}</div>;
  const kept = memory?.kept ?? [];
  if (memory?.visible && kept.length === 0) {
    return (
      <div className="sheet-row-said">
        Nothing yet. Tell KUE “remember that…” and it will keep it — with where it came from.
      </div>
    );
  }
  const groups: { heading: string; rows: Kept[] }[] = [];
  for (const k of kept) {
    const g = groups.find((x) => x.heading === k.heading);
    if (g) g.rows.push(k); else groups.push({ heading: k.heading, rows: [k] });
  }
  return (
    <>
      {groups.map((g) => (
        <div key={g.heading}>
          <div className="sheet-group">{g.heading}</div>
          {g.rows.map((k) => (
            <div className="sheet-row" key={k.id}>
              <div style={{ flex: 1 }}>
                <div className="sheet-row-name">{k.statement}</div>
                <div className="sheet-row-said">
                  {k.why ? `${k.when}, ${k.why}` : "KUE has no record of why this was kept."}
                  {k.expires && " · only for the task in hand"}
                </div>
              </div>
              <button className="kue-btn plain" disabled={busy} onClick={() => onForget(k.id)}>Forget</button>
            </div>
          ))}
        </div>
      ))}
      {(memory?.past ?? 0) > 0 && (
        <div className="sheet-row-said" style={{ marginTop: 8 }}>
          {memory?.past} older {memory?.past === 1 ? "memory has" : "memories have"} been replaced, expired or
          forgotten. KUE does not read those back to you as current.
        </div>
      )}
    </>
  );
}

/** One thing KUE keeps: what it is, why it is kept, and when. */
export interface Kept {
  id: string; class: string; heading: string; statement: string;
  state: string; why: string; when: string; expires: boolean;
}
export interface MemoryReport { visible?: boolean; withheld?: string; kept?: Kept[]; current?: number; past?: number }

/** What KUE keeps about the owner, and what they can do about it: look at it,
 *  ask why it is there, forget one, or erase everything. The words are core's;
 *  the window groups them under the headings core gave them. */
export function MemorySheet({ ctx, onClose, onAction }: {
  ctx: ContextObject; onClose: () => void; onAction: (s: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [memory, setMemory] = useState<MemoryReport | null>(null);
  const load = () => { invoke<MemoryReport>("get_memories").then(setMemory).catch(() => {}); };
  useEffect(load, []);

  return (
    <Sheet title="What KUE remembers" onClose={onClose}>
      <MemoryList memory={memory} busy={busy} onForget={async (id) => {
        setBusy(true);
        try { onAction(await invoke<string>("forget_memory", { id })); }
        catch (e) { onAction(String(e)); }
        finally { setBusy(false); load(); }
      }} />

      <div className="sheet-group">What KUE did recently</div>
      {ctx.remembered_events.length === 0 && (
        <div className="sheet-row-said">Nothing has been kept yet this session.</div>
      )}
      {ctx.remembered_events.slice(0, 8).map((e, i) => (
        <div className="sheet-row" key={`${e.ts}-${i}`}>
          <div style={{ flex: 1 }}>
            <div className="sheet-row-name">{e.summary}</div>
            {e.detail && <div className="sheet-row-said">{e.detail}</div>}
          </div>
        </div>
      ))}

      <div className="sheet-group">Never kept</div>
      <div className="sheet-row-said">
        Camera frames, audio, what you type, your clipboard, window titles, web addresses,
        and the contents of any file KUE opens for you.
      </div>

      <div className="sheet-group">Erasing</div>
      <div className="sheet-row-said" style={{ marginBottom: 12 }}>
        Forget removes one memory: its words go from KUE's store, and it is never read back
        to you again. Erase everything removes all of it, along with the events and snapshots.
      </div>
      {confirming ? (
        <span className="attention-row">
          <button className="kue-btn danger" disabled={busy} onClick={async () => {
            setBusy(true);
            try { onAction(await invoke<string>("erase_memory")); }
            catch (e) { onAction(String(e)); }
            finally { setBusy(false); setConfirming(false); }
          }}>Erase everything</button>
          <button className="kue-btn plain" onClick={() => setConfirming(false)}>Keep it</button>
        </span>
      ) : (
        <button className="kue-btn" onClick={() => setConfirming(true)}>Erase everything KUE remembers</button>
      )}
    </Sheet>
  );
}
