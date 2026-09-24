/** The projection the core decides and the window renders (`core/src/surface.rs`).
 *
 * The window adds no states of its own. If a sentence is not here, KUE does not
 * say it; if a state is not here, the window cannot show it. */

export type SystemState = "RUNNING" | "PAUSED" | "STOPPED" | "RECOVERING";

export type Presence =
  | "RECOGNISED" | "RECOGNISED_HELD" | "UNCERTAIN" | "SOMEONE_ELSE" | "MORE_THAN_ONE"
  | "NOBODY" | "LOCKED" | "NOT_WATCHING" | "NOT_ENROLLED" | "CAMERA_BLOCKED" | "STOPPED";

export type Activity =
  | "IDLE" | "LISTENING" | "UNDERSTANDING" | "THINKING" | "CHECKING_ACCESS"
  | "WAITING_FOR_YOU" | "WAITING_FOR_MACOS" | "ACTING" | "SPEAKING"
  | "DONE" | "UNCERTAIN_RESULT" | "FAILED" | "BLOCKED" | "NOT_FOUND";

export type SignalState = "ON" | "OFF" | "BLOCKED" | "UNAVAILABLE" | "UNKNOWN" | "PAUSED";

export interface Signal { state: SignalState; word: string }

export interface Trust {
  camera: Signal;
  microphone: Signal;
  computer: Signal;
  external_ai: Signal;
  memory: Signal;
  kue: Signal;
}

export interface ContextLine {
  sentence: string;
  basis: "OBSERVED" | "INFERRED";
}

export type Permission = "CAMERA" | "MICROPHONE" | "SPEECH_RECOGNITION" | "NOTIFICATIONS" | "ACCESSIBILITY";

export interface Attention {
  kind: "CONFIRM" | "AUTHENTICATE" | "PERMISSION" | "ENROLL" | "RECOVER";
  sentence: string;
  action_id: string | null;
  /** Which macOS grant a PERMISSION item is about. Read this, never the sentence. */
  permission: Permission | null;
}

export interface Surface {
  system: SystemState;
  system_sentence: string;
  presence: Presence;
  presence_sentence: string;
  presence_detail: string | null;
  activity: Activity;
  activity_sentence: string | null;
  activity_action_id: string | null;
  /** The only permission to show a verified mark. */
  activity_verified: boolean;
  trust: Trust;
  context: ContextLine[];
  attention: Attention[];
}

/** How the presence glyph reads. Shape and word carry the state; colour only
 *  reinforces it, so this is never the sole signal. */
export const PRESENCE_GLYPH: Record<Presence, string> = {
  RECOGNISED: "here",
  RECOGNISED_HELD: "here",
  UNCERTAIN: "unsure",
  SOMEONE_ELSE: "unsure",
  MORE_THAN_ONE: "unsure",
  NOBODY: "off",
  LOCKED: "off",
  NOT_WATCHING: "off",
  NOT_ENROLLED: "unsure",
  CAMERA_BLOCKED: "blocked",
  STOPPED: "off",
};

/** How the activity mark reads. `working` is the only animated one, and it
 *  means a real process is running whose duration is unknown. */
export const ACTIVITY_TONE: Record<Activity, string> = {
  IDLE: "",
  LISTENING: "working",
  UNDERSTANDING: "working",
  THINKING: "working",
  CHECKING_ACCESS: "working",
  ACTING: "working",
  SPEAKING: "working",
  WAITING_FOR_YOU: "waiting",
  WAITING_FOR_MACOS: "waiting",
  DONE: "done",
  UNCERTAIN_RESULT: "waiting",
  FAILED: "wrong",
  BLOCKED: "wrong",
  NOT_FOUND: "wrong",
};

export const SIGNAL_TONE: Record<SignalState, string> = {
  ON: "on",
  OFF: "off",
  BLOCKED: "blocked",
  UNAVAILABLE: "blocked",
  UNKNOWN: "unknown",
  PAUSED: "paused",
};
