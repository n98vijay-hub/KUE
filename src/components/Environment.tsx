import type { EnvironmentBlock } from "../types";

/** Bones drawn between located joints. Only pairs where both ends were located. */
const BONES: [string, string][] = [
  ["nose", "neck"], ["neck", "leftShoulder"], ["neck", "rightShoulder"],
  ["leftShoulder", "leftElbow"], ["leftElbow", "leftWrist"],
  ["rightShoulder", "rightElbow"], ["rightElbow", "rightWrist"],
  ["neck", "root"], ["root", "leftHip"], ["root", "rightHip"],
  ["leftEye", "nose"], ["rightEye", "nose"], ["leftEar", "leftEye"], ["rightEar", "rightEye"],
];

const W = 280;
const H = 158; // the camera frame's 16:9, drawn small

function age(s: number | null) {
  if (s == null) return "never";
  return s < 1.5 ? "now" : `${s.toFixed(0)}s ago`;
}

export function Environment({ env, paused }: { env: EnvironmentBlock; paused: boolean }) {
  // Drawn as a mirror: your left on the left, as you would see yourself.
  const at = (x: number, y: number) => [(1 - x) * W, y * H] as const;
  const joints = new Map(env.joints.map((j) => [j.name, j]));
  const current = env.upper_body_visible != null;

  return (
    <div className="rail-block">
      <div className="label">In view</div>
      <div className="card env">
        <svg className="env-figure" viewBox={`0 0 ${W} ${H}`} role="img"
          aria-label="Located body joints and hands, mirrored">
          <rect x="0.5" y="0.5" width={W - 1} height={H - 1} rx="8" className="env-frame" />
          {BONES.map(([a, b]) => {
            const ja = joints.get(a), jb = joints.get(b);
            if (!ja || !jb) return null;
            const [x1, y1] = at(ja.x, ja.y), [x2, y2] = at(jb.x, jb.y);
            return <line key={`${a}-${b}`} x1={x1} y1={y1} x2={x2} y2={y2} className="env-bone" />;
          })}
          {env.joints.map((j) => {
            const [x, y] = at(j.x, j.y);
            return <circle key={j.name} cx={x} cy={y} r={2.4} className="env-joint"
              style={{ opacity: 0.35 + 0.65 * j.confidence }} />;
          })}
          {env.hands.map((h, i) => {
            const b = h.bounding_box;
            if (!b) return null;
            const [x, y] = at(b.x + b.w, b.y);
            return <rect key={i} x={x} y={y} width={b.w * W} height={b.h * H} rx="5"
              className={`env-hand ${h.near_face ? "near" : ""}`} />;
          })}
          {!current && (
            <text x={W / 2} y={H / 2 + 4} textAnchor="middle" className="env-empty">
              {paused ? "Paused" : "No current pose reading"}
            </text>
          )}
        </svg>

        <div className="kv">
          <span className="k">Upper body</span>
          <span className={`v ${current ? "" : "dim"}`}>
            {!current ? "—" : env.upper_body_visible
              ? `visible · ${env.upper_body_joints_located}/4 joints`
              : env.body_count > 0 ? `partial · ${env.upper_body_joints_located}/4 joints` : "not located"}
          </span>
        </div>
        <div className="kv">
          <span className="k">Hands</span>
          <span className={`v ${current ? "" : "dim"}`}>
            {!current ? "—" : env.hands.length === 0 ? "none located"
              : env.hands.map((h) => `${h.chirality}${h.near_face ? " (at face)" : ""}`).join(", ")}
          </span>
        </div>
        <div className="kv">
          <span className="k">Light</span>
          <span className={`v ${env.brightness == null ? "dim" : ""}`}>
            {env.brightness == null ? "—" : (
              <>
                <span className="env-light"><span style={{ width: `${Math.round(env.brightness * 100)}%` }} /></span>
                {env.brightness.toFixed(2)}{env.low_light ? " · too dark" : ""}
              </>
            )}
          </span>
        </div>
        <div className="kv">
          <span className="k">Scene</span>
          <span className={`v ${env.scene_labels.length ? "" : "dim"}`}>
            {env.scene_age_seconds == null || paused ? "—"
              : env.scene_labels.length === 0 ? "no confident label"
              : env.scene_labels.map((l) => `${l.identifier.replace(/_/g, " ")} ${l.confidence.toFixed(2)}`).join(" · ")}
          </span>
        </div>
        {env.animals.length > 0 && (
          <div className="kv">
            <span className="k">Animals</span>
            <span className="v">{env.animals.map((a) => `${a.identifier.toLowerCase()} ${a.confidence.toFixed(2)}`).join(", ")}</span>
          </div>
        )}
        {env.errors.map((e, i) => (
          <p className="cap-note" key={i} style={{ color: "var(--alert)" }}>{e}</p>
        ))}
        <p className="cap-note" style={{ marginTop: 8 }}>
          Pose {age(env.pose_age_seconds)}, every {env.pose_interval_seconds.toFixed(0)}s · scene{" "}
          {age(env.scene_age_seconds)}, every {env.scene_interval_seconds.toFixed(0)}s. Joint positions and
          whole-frame labels only — no gesture, posture or mood is inferred.
        </p>
      </div>
    </div>
  );
}
