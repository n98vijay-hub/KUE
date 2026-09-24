import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** What the core did with one input. No target or path: records are fetched
 *  through the firewall separately, as before. */
interface Heard {
  kind: "NEW_ACTION" | "QUESTION" | "CONFIRMED" | "CANCELLED" | "REVISED" | "CLARIFY" | "ANSWERED" | "REFERENCE";
  request_id: string | null;
  said: string | null;
}
import { listen } from "@tauri-apps/api/event";
import type { VoiceBlock, WakeInfo } from "../types";
import { Plan, Thread, useRuntime } from "./Runtime";

interface Exchange {
  question: string;
  answer: string;
  outcome: "ANSWERED" | "FAILED";
  seconds: number;
  model: string;
  withheld: string[];
  corrections: string[];
}

interface ActionRecord {
  id: string;
  source: string;
  action: { kind: string };
  description: string;
  risk: "LOW" | "MEDIUM" | "HIGH" | "CRITICAL";
  state: string;
  reason: string | null;
  verification: string | null;
  output: string | null;
  created_at: number;
  choices: string[];
  steps: string[];
}

/** get_actions: the records, or why they are withheld. */
interface ActionList {
  visible: boolean;
  withheld: "KILLED" | "AUTHORIZATION_REQUIRED" | "PRIVACY_DENIED" | null;
  withheld_because: string | null;
  records: ActionRecord[];
  /** KUE's own sentence for each record, written in core. */
  said: { id: string; said: string; tone: "WORKING" | "WAITING" | "DONE" | "UNCERTAIN" | "WRONG"; verified: boolean }[];
  /** Goals: requests of several steps. Kinds, states and KUE's own sentences only — no targets. */
  tasks: {
    id: string;
    goal: string;
    state: string;
    /** What the goal is waiting for you to do, written in core. */
    waiting: string | null;
    steps: { index: number; kind: string; state: string; step: string; said: string | null; action_id: string | null }[];
  }[];
}

const tilde = (p: string) => p.replace(/^\/Users\/[^/]+/, "~");


interface Payload {
  visible: boolean;
  withheld_because: string | null;
  /** The plain sentence for the window; withheld_because is the precise one. */
  withheld_said: string | null;
  conversation: {
    exchanges: Exchange[];
    pending: { id: string; question: string; partial: string; started_at: number } | null;
    cleared_because: string | null;
  };
  model: { name: string; running: boolean; availability: string | null; reason: string | null };
}

const human = (tag: string) => tag.toLowerCase().replace(/_/g, " ");

const APP_KINDS = ["OPEN_APPLICATION", "CLOSE_APPLICATION", "FOCUS_APPLICATION"];
/** An app name that matched several installed apps: you pick one; there is no plain Confirm. */
const isAppChoice = (a: ActionRecord) => APP_KINDS.includes(a.action.kind) && a.choices.length > 0;

interface VoiceSettings {
  voice: string | null;
  speed: number;
  volume: number;
  verbosity: "SILENT" | "BRIEF" | "NORMAL" | "DETAILED";
  speak_typed_answers: boolean;
}

/** get_speech: KUE's voice, as core reports it. Requests carry no spoken text. */
interface SpeechInfo {
  provider: { id: string; installed: boolean; running: boolean; spoken: number; cut_off: number; failures: number;
              last_failure: string | null; start_latency_ms: number | null };
  settings: VoiceSettings;
  voice: { identifier: string; name: string; language: string; quality: string } | null;
  voice_why: string;
  voices: { identifier: string; name: string; language: string; quality: string; gender: string; novelty: boolean }[];
  status: { state: string; speaking: { id: string; topic: string; priority: string; state: string } | null;
            queued: { id: string }[] };
}

interface ReferenceOutcome { record: ActionRecord | null; message: string | null }

/**
 * Conversation with Apple's on-device model. The model sees only what the
 * privacy firewall cleared for a local model; it can answer, and nothing else.
 */
export function Conversation({ voice, onAction }: { voice: VoiceBlock; onAction: (m: string) => void }) {
  const [data, setData] = useState<Payload | null>(null);
  const [draft, setDraft] = useState("");
  const [speech, setSpeech] = useState<SpeechInfo | null>(null);
  const [wake, setWake] = useState<WakeInfo | null>(null);
  const [actionList, setActionList] = useState<ActionList | null>(null);
  const actions = actionList?.records ?? [];
  /** KUE's sentence for one record, as core wrote it. */
  const line = (id: string) => actionList?.said?.find((l) => l.id === id);
  const setActions = (l: ActionList) => setActionList(l);
  const askRef = useRef<(q: string, source?: string) => void>(() => {});
  const [busy, setBusy] = useState(false);
  const [clock, setClock] = useState(() => Date.now());
  const endRef = useRef<HTMLDivElement>(null);
  /** The conversation core keeps — the authoritative one. */
  const runtime = useRuntime(true);

  useEffect(() => {
    let alive = true;
    const refresh = () => {
      invoke<Payload>("get_conversation").then((p) => alive && setData(p)).catch(() => {});
      invoke<WakeInfo>("get_wake").then((w) => alive && setWake(w)).catch(() => {});
      invoke<ActionList>("get_actions").then((a) => alive && setActions(a)).catch(() => {});
      invoke<SpeechInfo>("get_speech").then((v) => alive && setSpeech(v)).catch(() => {});
    };
    refresh();
    const poll = setInterval(refresh, 2000);
    const tick = setInterval(() => setClock(Date.now()), 500);
    const un = listen<Payload>("lantern://conversation", (e) => alive && setData(e.payload));
    // The event only says the list changed; records come through get_actions.
    const unActions = listen<null>("lantern://actions", () => {
      invoke<ActionList>("get_actions").then((a) => alive && setActions(a)).catch(() => {});
    });
    // On-device speech recognition of push-to-talk. A final transcript is asked straight away.
    const unHeard = listen<{ session: number; text: string; is_final: boolean }>("lantern://transcript", (e) => {
      if (!alive) return;
      setDraft(e.payload.text);
      if (e.payload.is_final && e.payload.text.trim()) askRef.current(e.payload.text, "VOICE");
    });
    return () => {
      alive = false; clearInterval(poll); clearInterval(tick);
      un.then((f) => f()).catch(() => {}); unHeard.then((f) => f()).catch(() => {});
      unActions.then((f) => f()).catch(() => {});
    };
  }, []);

  const pending = data?.conversation.pending ?? null;
  const exchanges = data?.conversation.exchanges ?? [];
  const threadLength = runtime.thread?.length ?? 0;
  useEffect(() => { endRef.current?.scrollIntoView({ block: "nearest" }); }, [exchanges.length, pending?.partial, threadLength]);

  // Speech is decided and produced by KUE Core; the window can only stop it or change how it sounds.
  async function setVoice(change: Partial<VoiceSettings>) {
    if (!speech) return;
    try {
      const settings = await invoke<VoiceSettings>("set_voice_settings", { settings: { ...speech.settings, ...change } });
      setSpeech({ ...speech, settings });
      invoke<SpeechInfo>("get_speech").then(setSpeech).catch(() => {});
    } catch (e) { onAction(String(e)); }
  }

  askRef.current = (q: string, source?: string) => { void askText(q, source); };

  async function ask() { await askText(draft, "TEXT"); }

  /** One front door for everything the owner says or types. The core decides
   *  whether it is new, a "yes" to what is waiting, a correction of the plan in
   *  progress, or a question — the window only hands it over. (Until 2026-09-21
   *  this was three calls, and a typed "yes" never confirmed anything.) */
  async function askText(text: string, source = "TEXT") {
    const q = text.trim();
    if (!q) return;
    setBusy(true);
    try {
      const heard = await invoke<Heard>("receive_input", { text: q, source });
      setDraft("");
      onAction(heard.said ?? "");
      invoke<ActionList>("get_actions").then(setActions).catch(() => {});
    } catch (e) {
      onAction(String(e));
    } finally {
      setBusy(false);
    }
  }

  const listening = voice.state === "LISTENING" || voice.state === "STARTING";
  const level = voice.level_db == null ? 0 : Math.max(0, Math.min(100, ((voice.level_db + 60) / 50) * 100));

  if (!data) return null;
  const m = data.model;
  const modelLine = m.availability === "UNAVAILABLE"
    ? `${m.name} unavailable${m.reason ? ` — ${m.reason}` : ""}`
    : `${m.name} · runs on this Mac · ${m.running ? "loaded" : "starts on first question"}`;

  return (
    <div className="section">
      <div className="convo">
        {actionList && !actionList.visible && actionList.withheld !== "AUTHORIZATION_REQUIRED" && (
          <p className="note alert">Actions withheld: {actionList.withheld_because}</p>
        )}
        {actionList && !actionList.visible && actionList.withheld === "AUTHORIZATION_REQUIRED" && data.visible && (
          <p className="cap-note">{actionList.withheld_because}</p>
        )}
        {!data.visible ? (
          <p className="cap-note">{data.withheld_said ?? data.withheld_because}</p>
        ) : (
          <>
            {(actionList?.tasks ?? []).slice(-2).map((t) => (
              <div key={t.id}>
                <div className="convo-meta">
                  {t.steps.length} steps · {t.steps.map((st) => `${st.index + 1}. ${human(st.kind)} — ${human(st.state)}`).join("  →  ")}
                </div>
                {/* What each step established and what KUE waits for: sentences
                    written in core from the goal's real state. */}
                {t.steps.filter((st) => st.said).map((st) => (
                  <p className="cap-note" key={st.index}>{st.said}</p>
                ))}
                {t.waiting && <p className="cap-note">{t.waiting}</p>}
              </div>
            ))}
            {actions.slice(-5).reverse().map((a) => (
              <div className="action-card" key={a.id}>
                <div className="action-head">
                  {/* KUE's sentence, written in core. The window never composes
                      one, and never turns a state name into English itself. */}
                  <span className="action-desc">{line(a.id)?.said ?? a.description}</span>
                  {line(a.id)?.verified && <span className="activity-verified">✓ verified</span>}
                </div>
                {a.reason && <div className="convo-meta">{a.reason}</div>}
                {a.output != null && <pre className="inspector" style={{ marginTop: 8 }}>{a.output}</pre>}
                {isAppChoice(a) && (a.state === "REQUIRES_CONFIRMATION" || a.state === "REQUIRES_STRONG_AUTH") && (
                  <div className="doc-choices">
                    <div className="convo-meta">{a.choices.length} installed apps match. Choose one — nothing opens until you do:</div>
                    {a.choices.map((c) => (
                      <button key={c} className="disclosure doc-choice" disabled={busy} onClick={async () => {
                        setBusy(true);
                        try { await invoke("confirm_action", { id: a.id, choice: c }); } catch (e) { onAction(String(e)); }
                        finally { setBusy(false); invoke<ActionList>("get_actions").then(setActions).catch(() => {}); }
                      }}>{c}</button>
                    ))}
                  </div>
                )}
                {!isAppChoice(a) && a.choices.length > 1 && (a.state === "REQUIRES_CONFIRMATION" || a.state === "REQUIRES_STRONG_AUTH") && (
                  <div className="doc-choices">
                    <div className="convo-meta">{a.choices.length} matching documents, newest first. Open a different one:</div>
                    {a.choices.slice(1).map((c) => (
                      <button key={c} className="disclosure doc-choice" disabled={busy} onClick={async () => {
                        setBusy(true);
                        try { await invoke("confirm_action", { id: a.id, choice: c }); } catch (e) { onAction(String(e)); }
                        finally { setBusy(false); invoke<ActionList>("get_actions").then(setActions).catch(() => {}); }
                      }}>{tilde(c)}</button>
                    ))}
                  </div>
                )}
                {(a.state === "REQUIRES_CONFIRMATION" || a.state === "REQUIRES_STRONG_AUTH") && (
                  <div className="killed-actions">
                    {!isAppChoice(a) && <button className="btn btn-pause" disabled={busy} onClick={async () => {
                      setBusy(true);
                      try { await invoke("confirm_action", { id: a.id }); } catch (e) { onAction(String(e)); }
                      finally { setBusy(false); invoke<ActionList>("get_actions").then(setActions).catch(() => {}); }
                    }}>Confirm</button>}
                    <button className="btn" onClick={() => invoke("cancel_action", { id: a.id })}>Cancel</button>
                  </div>
                )}
                {/* Everything engineering belongs here, not on the face of the
                    moment: what was asked, how far it got, what was verified. */}
                <details className="how">
                  <summary>How did this go?</summary>
                  <div className="convo-meta">{a.description}</div>
                  {a.verification && <div className="convo-meta">Checked afterwards: {a.verification}</div>}
                  <div className="convo-meta">
                    {human(a.action.kind)} · {a.risk.toLowerCase()} risk · asked by {a.source.toLowerCase()} · {a.id}
                    {a.steps.length > 0 && ` · ${a.steps.map(human).join(" → ")}`}
                  </div>
                </details>
              </div>
            ))}
            <Thread report={runtime} />
            <Plan report={runtime} />
            {exchanges.length === 0 && !pending && threadLength === 0 && (
              <p className="cap-note">
                {data.conversation.cleared_because
                  ? `Conversation cleared: ${data.conversation.cleared_because}`
                  : "Ask about what Lantern observes right now, or give a command: open Safari, open my resume, quit Notes, go to apple.com, create a file called todo.txt containing …, read the file todo.txt, notify me that … Documents are found in Desktop, Documents and Downloads and opened only after you confirm; new and changed files are limited to ~/KUE."}
              </p>
            )}
            {/* The model's answers are turns in the thread above. What only the
                exchange knows — corrections of a model claim, the model, how
                long it took, what the firewall withheld — stays visible here. */}
            {exchanges.filter((x) => x.corrections.length > 0).slice(-3).map((x, i) => (
              <div className="convo-turn" key={`c${i}`}>
                {x.corrections.map((c, j) => (
                  <div className="note alert convo-correction" key={j}>{c}</div>
                ))}
              </div>
            ))}
            {exchanges.length > 0 && (
              <details className="how">
                <summary>Answers from the on-device model</summary>
                {exchanges.map((x, i) => (
                  <div className="convo-turn" key={i}>
                    <div className="convo-q">{x.question}</div>
                    <div className={`convo-a ${x.outcome === "FAILED" ? "failed" : ""}`}>{x.answer || "(empty answer)"}</div>
                    <div className="convo-meta">
                      {x.model}{x.seconds > 0.05 ? ` · ${x.seconds.toFixed(1)}s` : ""}
                      {x.withheld.length > 0 && ` · firewall withheld ${x.withheld.map(human).join(", ")}`}
                    </div>
                  </div>
                ))}
              </details>
            )}
            {pending && (
              <div className="convo-turn">
                {/* The question is already in the thread, as the owner's turn. */}
                {threadLength === 0 && <div className="convo-q">{pending.question}</div>}
                <div className="convo-a pending">{pending.partial || "Thinking on this Mac…"}</div>
                <div className="convo-meta">
                  {((clock / 1000) - pending.started_at).toFixed(0)}s ·{" "}
                  <button className="disclosure" onClick={() => invoke("cancel_ask")}>Cancel</button>
                </div>
              </div>
            )}
            <div ref={endRef} />
          </>
        )}
        <div className="convo-input">
          <textarea
            value={draft}
            placeholder={data.visible ? "Ask a question…" : "Asking will request Touch ID if needed"}
            rows={2}
            maxLength={2000}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ask(); } }}
          />
          {listening ? (
            <button className="btn btn-kill" onClick={() => invoke("listen_stop").catch((e) => onAction(String(e)))}>
              Stop
            </button>
          ) : (
            <button className="btn" disabled={busy || !!pending || voice.state === "STARTING" || voice.state === "FINISHING"}
              title="Push-to-talk: listens until you pause, transcribes on this Mac, then asks"
              onClick={() => { setDraft(""); invoke("listen_start").catch((e) => onAction(String(e))); }}>
              Speak
            </button>
          )}
          <button className="btn btn-pause" disabled={busy || !!pending || !draft.trim()} onClick={ask}>Ask</button>
        </div>
        {(listening || voice.state === "FINISHING") && (
          <div className="voice-meter">
            <div className="voice-bar"><div style={{ width: `${level}%` }} className={voice.voice_active ? "on" : ""} /></div>
            <span>{voice.state === "FINISHING" ? "Transcribing on this Mac…" : voice.voice_active ? "Voice activity" : "Listening…"}</span>
          </div>
        )}
        {["PERMISSION_DENIED", "UNAVAILABLE", "ERROR", "NO_MICROPHONE"].includes(voice.state) && (
          <p className="cap-note" style={{ color: "var(--caution)", marginTop: 8 }}>
            Voice: {voice.state.replace(/_/g, " ").toLowerCase()}{voice.detail ? ` — ${voice.detail}` : ""}
          </p>
        )}
        <p className="cap-note" style={{ marginTop: 8 }}>
          {modelLine}. Kept for this session only; never written to memory. Speech is transcribed on this Mac;
          Lantern does not recognise who is speaking, and a voice grants no access.
        </p>
        {speech && (
          <details className="how">
            <summary>Voice</summary>
            <div className="cap-note voice-out" style={{ marginTop: 6 }}>
              {!speech.provider.installed ? "Speech output is not installed, so KUE cannot speak."
                : !speech.provider.running ? "Speech output is not running."
                : speech.voice ? `${speech.voice.name}, a voice built into this Mac. Nothing is sent anywhere to be spoken.`
                : speech.voice_why}
              {speech.status.state !== "IDLE" && (
                <>{" "}<button className="disclosure" onClick={() => invoke("stop_speaking")}>Stop speaking</button></>
              )}
              {" \u00b7 "}
              <select aria-label="How much KUE says" value={speech.settings.verbosity}
                      onChange={(e) => setVoice({ verbosity: e.target.value as VoiceSettings["verbosity"] })}>
                <option value="SILENT">Say nothing</option>
                <option value="BRIEF">Say less</option>
                <option value="NORMAL">Normal</option>
                <option value="DETAILED">Say more</option>
              </select>
              {" \u00b7 "}
              <select aria-label="Which voice" value={speech.settings.voice ?? ""}
                      onChange={(e) => setVoice({ voice: e.target.value || null })}>
                <option value="">Default voice</option>
                {speech.voices.filter((v) => v.language.startsWith("en")).map((v) => (
                  <option key={v.identifier} value={v.identifier}>{v.name} ({v.language}{v.novelty ? ", novelty" : ""})</option>
                ))}
              </select>
              {" \u00b7 "}
              <button className="disclosure" onClick={() => setVoice({ speak_typed_answers: !speech.settings.speak_typed_answers })}>
                {speech.settings.speak_typed_answers ? "Stop reading typed answers aloud" : "Read typed answers aloud"}
              </button>
              {speech.provider.last_failure && <span style={{ color: "var(--caution)" }}> · last failure: {speech.provider.last_failure}</span>}
            </div>

            {/* Hands-free. The words here are KUE's own (`said`, `why_not`),
                and the state is what is RUNNING, not what was asked for. */}
            {wake && (
              <div className="cap-note voice-out" style={{ marginTop: 6 }}>
                <button className="disclosure"
                        onClick={() => invoke<WakeInfo>("set_wake", { enabled: !wake.enabled }).then(setWake).catch((e) => onAction(String(e)))}>
                  {wake.enabled ? "Stop listening for its name" : "Listen for its name"}
                </button>
                {" \u00b7 "}
                <span>{wake.said}</span>
                {wake.enabled && !wake.listening && wake.why_not && <span style={{ color: "var(--caution)" }}> · {wake.why_not}</span>}
                {wake.enabled && (
                  <>
                    {" \u00b7 "}
                    <select aria-label="What to listen for" value={wake.phrase}
                            onChange={(e) => invoke<WakeInfo>("set_wake", { enabled: true, phrase: e.target.value }).then(setWake).catch((x) => onAction(String(x)))}>
                      <option value="computer">“Computer”</option>
                      <option value="hello kue">“Hello KUE”</option>
                      <option value="hey lantern">“Hey Lantern”</option>
                    </select>
                  </>
                )}
              </div>
            )}
          </details>
        )}
      </div>
    </div>
  );
}
