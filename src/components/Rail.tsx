import { useState } from "react";
import type {
  Capability, Condition, ContextObject, EvidenceItem, LanternEvent, ProcessUsage,
  RememberedEvent, Resources,
} from "../types";

function clock(ts: number) {
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

export function Sensors({ ctx }: { ctx: ContextObject }) {
  const s = ctx.sensors;
  return (
    <div className="rail-block">
      <div className="label">Sensing</div>
      <div className="card">
        <div className="kv">
          <span className="k">Sensing layer</span>
          <span className="v">{s.sensing_process}</span>
        </div>
        <div className="kv">
          <span className="k">Camera</span>
          <span className="v">
            {s.camera_state}
            {s.paused && (
              /* The sensing layer's own word, unmasked — so "paused" is something
                 you can check rather than something you take on trust. */
              <span style={{ color: "var(--ink-3)" }}>
                {" "}· layer reports {s.camera_state_reported}
              </span>
            )}
          </span>
        </div>
        <div className="kv">
          <span className="k">Permission</span>
          <span className="v">{s.camera_permission}</span>
        </div>
        <div className="kv">
          <span className="k">Device</span>
          <span className={`v ${s.camera_device ? "" : "dim"}`}>
            {s.camera_device ?? "—"}
          </span>
        </div>
        <div className="kv">
          <span className="k">Analysis rate</span>
          <span className={`v ${s.processed_fps ? "" : "dim"}`}>
            {s.paused || !s.processed_fps ? "—" : `${s.processed_fps.toFixed(1)} fps`}
          </span>
        </div>
        <div className="kv">
          <span className="k">Activity sampling</span>
          <span className="v">
            {s.computer_sampling_reported == null ? "—" : s.computer_sampling_reported ? "ACTIVE" : "STOPPED"}
          </span>
        </div>
        <div className="kv">
          <span className="k">Frontmost app</span>
          <span className={`v ${ctx.computer.frontmost_app ? "" : "dim"}`}>
            {ctx.computer.frontmost_app ?? "—"}
          </span>
        </div>
        <div className="kv">
          <span className="k">Since last input</span>
          <span className="v">
            {ctx.computer.idle_seconds == null
              ? "—"
              : `${ctx.computer.idle_seconds.toFixed(0)}s`}
          </span>
        </div>
        {s.camera_detail && (
          <div className="caveat" style={{ marginTop: 12 }}>
            {s.camera_detail}
          </div>
        )}
      </div>
    </div>
  );
}

function day(ts: number) {
  return new Date(ts * 1000).toLocaleDateString([], { month: "short", day: "numeric" });
}

/** The remembered answer to "why did you think that?" — never recomputed now. */
function Why({ confidence, evidence }: { confidence: number; evidence: EvidenceItem[] }) {
  return (
    <div className="tl-why">
      <div>confidence at the time {(confidence * 100).toFixed(0)}%</div>
      {evidence.map((e) => (
        <div key={e.id}>
          {e.polarity === "contradicts" ? "− " : "+ "}
          {e.statement}
        </div>
      ))}
    </div>
  );
}

function Row({ ts, summary, detail, why, showDay }: {
  ts: number; summary: string; detail: string | null;
  why: { confidence: number; evidence: EvidenceItem[] } | null; showDay?: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="tl-row">
      <div className="tl-time">{showDay ? `${day(ts)} ` : ""}{clock(ts)}</div>
      <div>
        <div className="tl-text">{summary}</div>
        {detail && <div className="tl-detail">{detail}</div>}
        {why && (
          <button className="disclosure tl-why-toggle" onClick={() => setOpen((o) => !o)}>
            {open ? "hide why" : "why?"}
          </button>
        )}
        {why && open && <Why {...why} />}
      </div>
    </div>
  );
}

export function Timeline({ events, remembered }: { events: LanternEvent[]; remembered: RememberedEvent[] }) {
  return (
    <div className="rail-block">
      <div className="label">Recent events</div>
      {events.length === 0 ? (
        <p className="empty">Nothing recorded yet.</p>
      ) : (
        <div className="timeline">
          {events.slice(0, 22).map((e) => (
            <Row key={e.id} ts={e.ts} summary={e.summary} detail={e.detail}
              why={e.provenance && { confidence: e.provenance.confidence, evidence: e.provenance.evidence }} />
          ))}
        </div>
      )}
      {remembered.length > 0 && (
        <>
          <div className="label" style={{ marginTop: 18 }}>Remembered from before this launch</div>
          <div className="timeline remembered">
            {remembered.slice(0, 15).map((e, i) => (
              <Row key={`${e.ts}-${i}`} ts={e.ts} summary={e.summary} detail={e.detail} showDay
                why={e.confidence == null ? null : { confidence: e.confidence, evidence: e.evidence }} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

export function Capabilities({ caps }: { caps: Capability[] }) {
  const real = caps.filter((c) => c.status === "REAL").length;
  const partial = caps.filter((c) => c.status === "PARTIAL").length;
  return (
    <div className="rail-block">
      <div className="label">What this build actually does</div>
      <p className="cap-note" style={{ marginBottom: 12 }}>
        Of {caps.length} capabilities in the long-term vision, {real} are real and{" "}
        {partial} partial, with their limits stated. The rest are listed so nothing
        here is mistaken for more than it is.
      </p>
      <div className="card">
        {caps.map((c) => (
          <div className="cap-row" key={c.name}>
            <div>
              <div className="cap-name">{c.name}</div>
              <div className="cap-note">{c.note}</div>
            </div>
            <span className={`cap-tag ${c.status}`}>{c.status.replace("_", " ")}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// UNKNOWN sorts just under ACTIVE: something KUE has not been able to check is
// closer to a problem than to a clean bill of health.
const STATUS_ORDER: Record<Condition["status"], number> = {
  ACTIVE: 0, UNKNOWN: 1, CLEAR: 2, NOT_IMPLEMENTED: 3, NOT_APPLICABLE: 4,
};

/**
 * Every named failure state, whether or not it is happening. A failure is shown
 * because the core reported it — never inferred here from a missing value.
 */
export function Conditions({ conditions }: { conditions: Condition[] }) {
  const sorted = [...conditions].sort((a, b) => STATUS_ORDER[a.status] - STATUS_ORDER[b.status]);
  const active = conditions.filter((c) => c.status === "ACTIVE").length;
  return (
    <div className="rail-block">
      <div className="label">Failure states</div>
      <p className="cap-note" style={{ marginBottom: 12 }}>
        {active === 0 ? "Nothing is failing right now." : `${active} active.`} Identity
        states are listed here too, because each is a limit on what Lantern can say.
      </p>
      <div className="card">
        {sorted.map((c) => (
          <div className="cap-row" key={c.code}>
            <div>
              <div className="cap-name mono">{c.code}</div>
              <div className="cap-note">{c.detail}</div>
            </div>
            <span className={`cap-tag ${c.status}`}>{c.status.replace("_", " ")}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

const pct = (v: number | null) => (v == null ? "—" : `${v < 1 ? v.toFixed(2) : v.toFixed(1)}%`);
const mb = (v: number | null) => (v == null ? "—" : `${v.toFixed(0)} MB`);

function UsageRow({ label, u }: { label: string; u: ProcessUsage }) {
  return (
    <div className="kv">
      <span className="k">{label}</span>
      <span className={`v ${u.cpu_percent == null ? "dim" : ""}`}>
        {pct(u.cpu_percent)} CPU · {mb(u.footprint_mb)}
      </span>
    </div>
  );
}

/** What Lantern costs to run — every figure measured, and the unmeasured named. */
export function Cost({ r }: { r: Resources }) {
  return (
    <div className="rail-block">
      <div className="label">What Lantern costs</div>
      <div className="card">
        <UsageRow label="Sensing layer, now" u={r.sensing} />
        <UsageRow label="Shell, now" u={r.shell} />
        <div className="kv">
          <span className="k">Sensing while observing</span>
          <span className={`v ${r.sensing_cpu_observing_percent == null ? "dim" : ""}`}>
            {pct(r.sensing_cpu_observing_percent)}
            {r.observing_seconds_measured > 0 && ` over ${r.observing_seconds_measured.toFixed(0)}s`}
          </span>
        </div>
        <div className="kv">
          <span className="k">Sensing while paused</span>
          <span className={`v ${r.sensing_cpu_paused_percent == null ? "dim" : ""}`}>
            {pct(r.sensing_cpu_paused_percent)}
            {r.paused_seconds_measured > 0 && ` over ${r.paused_seconds_measured.toFixed(0)}s`}
          </span>
        </div>
        <div className="kv">
          <span className="k">Thermal state</span>
          <span className="v">{r.thermal_state ?? "—"}</span>
        </div>
        <div className="kv">
          <span className="k">Power</span>
          <span className="v">
            {r.power_source ?? "—"}
            {r.battery_percent != null && ` · ${r.battery_percent.toFixed(0)}%`}
            {r.low_power_mode && " · Low Power Mode"}
          </span>
        </div>
        <div className="kv">
          <span className="k">Analysis rate</span>
          <span className="v">{r.analysis_fps_target.toFixed(1)} fps</span>
        </div>
        <p className="cap-note" style={{ marginTop: 10 }}>{r.analysis_fps_reason}</p>
        <div className="caveat" style={{ marginTop: 12 }}>
          <strong>Not measured.</strong>
          {r.not_measured.map((n) => <span key={n}> {n}</span>)}
        </div>
      </div>
    </div>
  );
}
