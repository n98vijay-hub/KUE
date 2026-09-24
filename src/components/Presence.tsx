/** Presence, the trust strip, the activity line, and what needs you.
 *
 * Every sentence here comes from the core's projection (`get_surface`). These
 * components choose a shape and a colour for a state; they never decide what a
 * state means, and they never compose a sentence about KUE. */

import { ACTIVITY_TONE, PRESENCE_GLYPH, SIGNAL_TONE } from "../surface";
import type { Attention as AttentionItem, Signal, Surface } from "../surface";

export function Presence({ s }: { s: Surface }) {
  return (
    <div className="presence">
      <span
        className={`presence-glyph ${PRESENCE_GLYPH[s.presence]}`}
        role="img"
        aria-label={s.presence_sentence}
      />
      <div>
        <div className="presence-said">{s.presence_sentence}</div>
        {s.presence_detail && <div className="presence-detail">{s.presence_detail}</div>}
      </div>
    </div>
  );
}

/** What KUE can sense and where anything goes. Each signal carries a word, so
 *  it is never colour alone — and the word is the core's, not ours. */
export function TrustStrip({ s, stale, onOpen }: { s: Surface; stale?: boolean; onOpen: () => void }) {
  const t = s.trust;
  const live: Signal[] = [t.camera, t.microphone, t.computer, t.external_ai, t.memory];
  // A signal nobody has refreshed is not evidence. Rather than repeat the last
  // one it heard, KUE says it does not know.
  const order: Signal[] = stale
    ? live.map((sig) => ({ state: "UNKNOWN" as const, word: `${sig.word.split(" ")[0]} — not sure` }))
    : live;
  return (
    <div className="trust" role="group" aria-label="What KUE can sense">
      {order.map((sig, i) => (
        <button key={i} className={`trust-chip ${SIGNAL_TONE[sig.state]}`} onClick={onOpen}>
          <span className="trust-dot" aria-hidden="true" />
          {sig.word}
        </button>
      ))}
    </div>
  );
}

/** What KUE is doing this second. Empty when it is doing nothing — KUE shows
 *  nothing rather than a shape pretending to be activity. */
export function ActivityLine({ s }: { s: Surface }) {
  if (!s.activity_sentence) return <div className="activity" aria-live="polite" />;
  return (
    <div className={`activity ${ACTIVITY_TONE[s.activity]}`} aria-live="polite">
      <span className="activity-mark" aria-hidden="true" />
      <span>{s.activity_sentence}</span>
      {/* The mark is shown only when the core recorded what was verified. */}
      {s.activity_verified && <span className="activity-verified">✓ verified</span>}
    </div>
  );
}

/** The one thing asking for you, with the control that answers it. */
export function Attention({
  item, busy, onConfirm, onCancel, onSettings,
}: {
  item: AttentionItem;
  busy: boolean;
  onConfirm: (id: string) => void;
  onCancel: (id: string) => void;
  onSettings: (permission: string) => void;
}) {
  return (
    <div className="attention" role="group" aria-live="assertive">
      <span className="attention-said">{item.sentence}</span>
      <span className="attention-row">
        {(item.kind === "CONFIRM" || item.kind === "AUTHENTICATE") && item.action_id && (
          <>
            <button className="kue-btn affirm" disabled={busy} onClick={() => onConfirm(item.action_id!)}>
              {item.kind === "AUTHENTICATE" ? "Use Touch ID" : "Go ahead"}
            </button>
            <button className="kue-btn plain" disabled={busy} onClick={() => onCancel(item.action_id!)}>
              Not now
            </button>
          </>
        )}
        {/* Which grant this is about is a field on the item. Reading it out of
            the sentence meant a reworded sentence silently lost the button. */}
        {item.kind === "PERMISSION" && item.permission && (
          <button className="kue-btn" onClick={() => onSettings(item.permission!)}>Open Settings</button>
        )}
      </span>
    </div>
  );
}
