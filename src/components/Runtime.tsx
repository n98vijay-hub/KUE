/** What KUE is doing — the one runtime state the core owns, projected.
 *
 * Every value here comes from `get_runtime`. The window computes no state of
 * its own: WAITING_FOR_USER is shown because the core is waiting, not because a
 * button is on screen. Turns are KUE's own sentences and the owner's words;
 * the core never puts a file path or a target into them. */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface RuntimeTurn { kind: string; said: string; at: number; request_id: string | null }
/** One entry of the conversation, labelled in core (`Pipeline::thread`). */
export interface ShownTurn { shown: string; said: string; at: number; request_id: string | null }
export interface RuntimeReport {
  state?: string;
  conversation_id?: string;
  active?: { request_id: string; state: string; decision: string | null; transport: string;
             goal_id: string | null; tool_execution_id: string | null; verification_id: string | null } | null;
  open?: { kind: string; facets?: { kind: string; value: string }[] } | null;
  turns?: RuntimeTurn[];
  thread?: ShownTurn[];
  now?: ShownTurn | null;
  /** The plan the live request follows, as core reads it, and its preview.
   *  `steps` is what each step would actually do, written in core. */
  plan?: {
    plan: { plan_id: string; state: string; risk: string | null; reversible: boolean;
            steps: { index: number; step: string; state: string; risk: string | null }[];
            approved_by: { by: string; step: number }[]; proposed_by: { by: string; detail: string } };
    preview: { lines: string[] };
    steps: string[];
    version: number;
    revision_of: string | null;
    waiting: boolean;
    /** What the owner has told KUE that shaped this — their own sentences. */
    memory_used?: string[];
  } | null;
  refused_transitions?: number;
  /** Why the conversation and plan are not shown: KUE is not sure it's the owner. */
  withheld?: string;
}

/** The word beside each entry. Core decided which entry is which — a RESULT
 *  is a result because it verified — so this only names the category. */
const SHOWN_AS: Record<string, string> = {
  USER_TURN: "You", KUE_TURN: "KUE", QUESTION: "KUE asks", SUGGESTION: "KUE suggests", PROGRESS: "Progress",
  PLANNING: "Working it out", WAITING_FOR_PLAN: "Waiting for you",
  WAITING_FOR_CONFIRMATION: "Waiting for you", WAITING_FOR_SELECTION: "Waiting for you", AUTHORIZING: "macOS",
  ACTING: "Acting", VERIFYING: "Checking", RESULT: "Verified", UNVERIFIED: "Not verified",
  BLOCKED: "Blocked", FAILED: "Failed", CANCELLED: "Cancelled",
};
const NEEDS_ATTENTION = ["UNVERIFIED", "BLOCKED", "FAILED", "QUESTION", "WAITING_FOR_CONFIRMATION",
                         "WAITING_FOR_SELECTION", "WAITING_FOR_PLAN"];

/** What each step's state is called. A step reads as done only when core says
 *  SUCCEEDED, which it says when what the step did was read back and matched —
 *  so nothing here can call a step done before it was verified. */
const STEP_STATE: Record<string, string> = {
  PENDING: "to do", READY: "to do", RUNNING: "doing it now", SUCCEEDED: "verified",
  FAILED: "failed", BLOCKED: "blocked", CANCELLED: "cancelled", SKIPPED: "not needed",
  VERIFICATION_FAILED: "not verified",
};
const STEP_DONE = ["SUCCEEDED"];
const STEP_WRONG = ["FAILED", "BLOCKED", "VERIFICATION_FAILED"];

/** The plan, exactly as the owner is agreeing to it.
 *
 *  Every line comes from core: the steps are what KUE would really do, in the
 *  order it would do them, and the sentences under them are core's own
 *  preview. The window adds nothing and leaves nothing out — this is the thing
 *  a "do it" applies to, so it is shown in full. */
export function Plan({ report }: { report: RuntimeReport }) {
  const p = report.plan;
  if (!p) return null;
  const proposer = p.plan.proposed_by.detail?.replace(/_/g, " ").toLowerCase() ?? "kue";
  return (
    <div className={`plan-card${p.waiting ? " plan-waiting" : ""}`} aria-live="polite">
      <div className="plan-head">
        <span className="label">{p.waiting ? "Plan — waiting for you" : "Plan"}</span>
        <span className="plan-meta">
          {p.plan.plan_id} · proposed by {proposer}
          {p.version > 1 && ` · version ${p.version}`}
          {p.revision_of && ` · you changed ${p.revision_of}`}
        </span>
      </div>
      <ol className="plan-steps">
        {p.plan.steps.map((st, i) => (
          <li className={`plan-step ${st.state.toLowerCase()}`} key={st.index}>
            <span className="plan-what">{p.steps[i] ?? st.step.replace(/_/g, " ").toLowerCase()}</span>
            <span className={`plan-state${STEP_WRONG.includes(st.state) ? " caution" : ""}`}>
              {STEP_STATE[st.state] ?? st.state.replace(/_/g, " ").toLowerCase()}
              {STEP_DONE.includes(st.state) && <span className="activity-verified"> ✓</span>}
            </span>
          </li>
        ))}
      </ol>
      {p.preview.lines.map((l, i) => <p className="plan-line" key={i}>{l}</p>)}
      {/* Memory changed what is being proposed, so the owner is told — in
          their own words, with no ids. The detail opens; it does not shout. */}
      {(p.memory_used?.length ?? 0) > 0 && (
        <details className="plan-memory">
          <summary>
            KUE used {p.memory_used!.length === 1 ? "one thing" : `${p.memory_used!.length} things`} you told it
          </summary>
          {p.memory_used!.map((m, i) => <p className="plan-line" key={i}>“{m}”</p>)}
        </details>
      )}
    </div>
  );
}

/** The conversation — the one core keeps, whatever the owner typed or said.
 *  Every sentence is KUE's own or the owner's words; nothing is composed here. */
export function Thread({ report }: { report: RuntimeReport }) {
  const thread = report.thread ?? [];
  const now = report.now ?? null;
  if (thread.length === 0 && !now) return null;
  return (
    <div className="thread" aria-live="polite">
      {thread.map((t, i) => (
        <div className={`thread-turn ${t.shown.toLowerCase()}`} key={i}>
          <span className={`thread-who${NEEDS_ATTENTION.includes(t.shown) ? " caution" : ""}`}>
            {SHOWN_AS[t.shown] ?? "KUE"}
            {t.shown === "RESULT" && <span className="activity-verified"> ✓</span>}
          </span>
          <span className={t.shown === "USER_TURN" ? "convo-q" : "convo-a"}>{t.said}</span>
        </div>
      ))}
      {now && (
        <div className={`thread-turn now ${now.shown.toLowerCase()}`}>
          <span className={`thread-who${NEEDS_ATTENTION.includes(now.shown) ? " caution" : ""}`}>{SHOWN_AS[now.shown] ?? "KUE"}</span>
          <span className="convo-a pending">{now.said}</span>
        </div>
      )}
    </div>
  );
}

const WHO: Record<string, string> = {
  USER_SPEECH: "You", USER_CONFIRMATION: "You", USER_CORRECTION: "You", USER_CANCELLATION: "You",
};

export function Runtime({ report }: { report: RuntimeReport }) {
  const turns = report.turns ?? [];
  const a = report.active ?? null;
  return (
    <div className="section">
      <div className="label">What KUE is doing</div>
      {report.withheld && <p className="cap-note">{report.withheld}</p>}
      <div className="plain-list">
        <div className="plain-item">
          <span className="chip absent">{(report.state ?? "UNKNOWN").replace(/_/g, " ")}</span>
          <span>
            {a ? `Request ${a.request_id}: ${a.state.replace(/_/g, " ").toLowerCase()}` +
                 (a.decision ? ` · decided to ${a.decision.toLowerCase()}` : "") +
                 ` · by ${a.transport.toLowerCase()}`
               : "No request in progress."}
          </span>
        </div>
        {report.open && (
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>Waiting for you: {report.open.kind.replace(/_/g, " ").toLowerCase()}
              {report.open.facets && report.open.facets.length > 0
                ? ` (${report.open.facets.map((f) => f.value.replace(/_/g, " ").toLowerCase()).join(", ")})` : ""}
            </span>
          </div>
        )}
        {report.plan && (
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>
              Plan {report.plan.plan.plan_id} ({report.plan.plan.proposed_by.detail.replace(/_/g, " ").toLowerCase()}):{" "}
              {report.plan.plan.state.replace(/_/g, " ").toLowerCase()}
              {report.plan.plan.risk ? ` · ${report.plan.plan.risk.toLowerCase()} risk` : ""}
              {report.plan.plan.approved_by.length > 0
                ? ` · approved at step ${report.plan.plan.approved_by.map((a) => `${a.step + 1} (${a.by.replace(/_/g, " ").toLowerCase()})`).join(", ")}`
                : " · not yet approved"}
              {" · "}{report.plan.plan.steps.map((st) => `${st.index + 1}. ${st.step.replace(/_/g, " ").toLowerCase()} — ${st.state.replace(/_/g, " ").toLowerCase()}`).join("  →  ")}
            </span>
          </div>
        )}
        {report.plan && report.plan.preview.lines.map((l, i) => (
          <div className="plain-item" key={`pv${i}`}><span className="bullet">·</span><span>{l}</span></div>
        ))}
        {(report.refused_transitions ?? 0) > 0 && (
          <div className="plain-item">
            <span className="bullet">·</span>
            <span>{report.refused_transitions} illegal step{report.refused_transitions === 1 ? "" : "s"} refused</span>
          </div>
        )}
      </div>
      {turns.length > 0 && (
        <div className="plain-list">
          {turns.map((t, i) => (
            <div className="plain-item" key={i}>
              <span className="bullet">{WHO[t.kind] ?? "KUE"}</span>
              <span>{t.said}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function useRuntime(active: boolean): RuntimeReport {
  const [report, setReport] = useState<RuntimeReport>({});
  useEffect(() => {
    if (!active) return;
    let alive = true;
    const pull = () => { invoke<RuntimeReport>("get_runtime").then((r) => { if (alive) setReport(r); }).catch(() => {}); };
    pull();
    const t = setInterval(pull, 1000);
    return () => { alive = false; clearInterval(t); };
  }, [active]);
  return report;
}
