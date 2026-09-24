/** Diagnostics: the instrument view.
 *
 * Every panel the window has ever had, mounted unchanged. This is where the
 * engineering vocabulary lives — access levels, identity bases, descriptor
 * distances, evidence arithmetic, the raw context object — because a person
 * reading this view wants exactly that, and a person who is not never sees it.
 *
 * Deliberately not restyled to the new design system beyond the contrast fix
 * the tokens give it for free: these panels work, and rewriting working
 * functionality to make it match would be a lot of risk for a view whose job is
 * to be dense. */

import { useMemo, useState } from "react";
import type { ContextObject } from "../types";
import { SOURCE_LABEL } from "../types";
import { ConfidenceMeter, Evidence } from "./Evidence";
import { Capabilities, Conditions, Cost, Sensors, Timeline } from "./Rail";
import { Identity, IDENTITY_TONE, Memory } from "./Identity";
import { IdentityCheck } from "./IdentityCheck";
import { Privacy } from "./Privacy";
import { Environment } from "./Environment";
import { Access } from "./Access";
import { Perception, usePerception } from "./Perception";
import { Runtime, useRuntime } from "./Runtime";

export function Diagnostics({ ctx, onAction }: { ctx: ContextObject; onAction: (s: string) => void }) {
  const [showJson, setShowJson] = useState(false);
  const paused = ctx.sensors.paused;
  // How the measuring is going, polled only while this view is open.
  const perception = usePerception(true);
  const runtime = useRuntime(true);

  const activityEvidence = useMemo(() => {
    const ids = new Set(ctx.activity.evidence_ids);
    return ctx.evidence.filter((e) => ids.has(e.id));
  }, [ctx]);

  return (
    <div className="body">
      <div className="main">
        <div className="main-inner">
          <div className="hero">
            <div className="hero-kicker">
              <span className="label">Right now</span>
              <span className={`chip ${IDENTITY_TONE[ctx.identity.state]}`}>
                {ctx.identity.state.replace(/_/g, " ")}
              </span>
              {ctx.people_detected > 0 && (
                <span className="chip absent">
                  {ctx.people_detected} {ctx.people_detected === 1 ? "person" : "people"}
                </span>
              )}
            </div>
            <h1 className="hero-statement">
              {ctx.activity.human}
              <span className="qualifier">
                {ctx.activity.state === "PAUSED"
                  ? " — by your explicit action."
                  : " — an inference, not an observation."}
              </span>
            </h1>
            <p className="hero-detail">{ctx.identity.detail}</p>
          </div>

          <Runtime report={runtime} />
          <Perception report={perception} />

          {paused ? (
            /* A confidence meter reads as a claim about you. Pause is not an
               inference, so there is no number to show — only what it rests on. */
            <div className="section">
              <div className="label">Why</div>
              <div className="plain-list">
                {activityEvidence.map((e) => (
                  <div className="plain-item" key={e.id}>
                    <span className="bullet">·</span>
                    <span>{e.statement}.</span>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <>
              <ConfidenceMeter label="Confidence in this reading" confidence={ctx.activity.confidence} />
              <div className="section">
                <div className="label">Why — the full arithmetic</div>
                <Evidence items={activityEvidence} confidence={ctx.activity.confidence} />
              </div>
            </>
          )}

          <div className="section">
            <div className="label">Observed</div>
            <div className="plain-list">
              {ctx.observations.map((o) => (
                <div className="plain-item" key={o.id}>
                  <span className="bullet">·</span>
                  <span>
                    {o.statement}{" "}
                    <span className="src">
                      {SOURCE_LABEL[o.source]}
                      {o.age_seconds > 0.5 ? ` · ${o.age_seconds.toFixed(1)}s ago` : ""}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          </div>

          {ctx.contradictions.length > 0 && (
            <div className="section">
              <div className="label">Signals that disagree</div>
              <div className="note-list">
                {ctx.contradictions.map((c, i) => <div className="note alert" key={i}>{c}</div>)}
              </div>
            </div>
          )}

          {ctx.predictions.length > 0 && (
            <div className="section">
              <div className="label">Prediction</div>
              <div className="note-list">
                {ctx.predictions.map((p) => (
                  <div className="note warm" key={p.id}>
                    {p.statement}
                    <span className="basis">{p.basis}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="section">
            <div className="label">What KUE does not know</div>
            <div className="note-list">
              {ctx.unknowns.map((u, i) => <div className="note" key={i}>{u}</div>)}
            </div>
          </div>

          <div className="section">
            <div className="label">Context object · schema v{ctx.schema_version}</div>
            <button className="disclosure" onClick={() => setShowJson((s) => !s)}>
              {showJson ? "Hide" : "Show"} the structured object the interface is drawn from
            </button>
            {showJson && <pre className="inspector">{JSON.stringify(ctx, null, 2)}</pre>}
            <p className="cap-note" style={{ marginTop: 10 }}>Configuration: {ctx.config_note}</p>
          </div>
        </div>
      </div>

      <div className="rail">
        <Access access={ctx.access} onAction={onAction} />
        <Sensors ctx={ctx} />
        <Identity ctx={ctx} onAction={onAction} />
        <Environment env={ctx.environment} paused={paused} />
        <IdentityCheck ctx={ctx} onAction={onAction} />
        <Timeline events={ctx.recent_events} remembered={ctx.remembered_events} />
        <Conditions conditions={ctx.conditions} />
        <Cost r={ctx.resources} />
        <Privacy onAction={onAction} />
        <Memory onAction={onAction} />
        <Capabilities caps={ctx.capabilities} />
      </div>
    </div>
  );
}
