// Mirrors lantern-core's context object. Field names are the Rust field names.

export type IdentityState =
  | "MY_FACE_CONFIRMED" | "UNKNOWN_PERSON" | "IDENTITY_UNCERTAIN"
  | "NO_FACE" | "MULTIPLE_PEOPLE" | "NOT_OBSERVING";

export type ActivityStateName =
  | "AT_COMPUTER_INTERACTING" | "PRESENT_NOT_INTERACTING"
  | "INPUT_WITHOUT_VISIBLE_PERSON" | "NO_ACTIVITY_DETECTED" | "UNKNOWN" | "PAUSED";

export type Source = "camera_vision" | "system_workspace" | "system_hid" | "temporal";

export type CapabilityStatus = "REAL" | "PARTIAL" | "SIMULATED" | "PLACEHOLDER" | "NOT_IMPLEMENTED";

export interface ConfidenceBreakdown {
  supporting: number;
  contradicting: number;
  denominator: number;
  raw: number;
  temporal_factor: number;
  stable_seconds: number;
  value: number;
  formula: string;
}

export interface EvidenceItem {
  id: string;
  statement: string;
  polarity: "supports" | "contradicts";
  weight: number;
  strength: number;
  source: Source;
  reliability: number;
}

export interface Observation {
  id: string;
  statement: string;
  source: Source;
  age_seconds: number;
}

export interface Inference {
  id: string;
  statement: string;
  confidence: ConfidenceBreakdown;
  evidence_ids: string[];
}

export interface Prediction { id: string; statement: string; basis: string }

export interface PersonTrack {
  track_id: string;
  frames_tracked: number;
  age_seconds: number;
  capture_quality: number | null;
  yaw_deg: number | null;
  pitch_deg: number | null;
  geometry_distance: number | null;
  feature_print_distance: number | null;
  descriptor_status: string;
}

/** Why a conclusion changed: the evidence and confidence at that moment. */
export interface Provenance {
  confidence: number;
  stable_seconds: number;
  evidence: EvidenceItem[];
}

export interface LanternEvent {
  id: number;
  ts: number;
  kind: string;
  summary: string;
  detail: string | null;
  provenance: Provenance | null;
}

/** An event read back from local memory, from before this launch. */
export interface RememberedEvent {
  ts: number;
  kind: string;
  summary: string;
  detail: string | null;
  confidence: number | null;
  evidence: EvidenceItem[];
}

export interface Capability { name: string; status: CapabilityStatus; note: string }

export type ConditionStatus = "ACTIVE" | "CLEAR" | "NOT_APPLICABLE" | "NOT_IMPLEMENTED" | "UNKNOWN";

export interface ProcessUsage {
  cpu_percent: number | null;
  footprint_mb: number | null;
  cpu_seconds_total: number | null;
}

export interface JointView { name: string; x: number; y: number; confidence: number }
export interface HandView {
  chirality: string;
  confidence: number;
  bounding_box: { x: number; y: number; w: number; h: number } | null;
  near_face: boolean;
}
export interface LabelView { identifier: string; confidence: number }

export interface EnvironmentBlock {
  pose_age_seconds: number | null;
  pose_interval_seconds: number;
  upper_body_visible: boolean | null;
  upper_body_joints_located: number;
  body_count: number;
  joints: JointView[];
  hands: HandView[];
  brightness: number | null;
  low_light: boolean | null;
  low_light_threshold: number;
  scene_age_seconds: number | null;
  scene_interval_seconds: number;
  scene_labels: LabelView[];
  animals: LabelView[];
  errors: string[];
}

export interface Resources {
  sensing: ProcessUsage;
  shell: ProcessUsage;
  sensing_cpu_observing_percent: number | null;
  observing_seconds_measured: number;
  sensing_cpu_paused_percent: number | null;
  paused_seconds_measured: number;
  thermal_state: string | null;
  low_power_mode: boolean | null;
  battery_percent: number | null;
  power_source: string | null;
  analysis_fps_target: number;
  analysis_fps_reason: string;
  not_measured: string[];
}

/** One named failure state, reported by the core whether or not it is happening. */
export interface Condition { code: string; status: ConditionStatus; detail: string }

export interface DescriptorSeparation {
  name: string;
  within_owner_min: number | null;
  within_owner_median: number | null;
  within_owner_max: number | null;
  owner_vs_probe_min: number | null;
  owner_vs_probe_median: number | null;
  owner_vs_probe_max: number | null;
  separation_ratio: number | null;
  verdict: "SEPARATED" | "OVERLAPPING" | "INSUFFICIENT_DATA";
}

export interface SeparationReport {
  owner_samples: number;
  probe_samples: number;
  probe_label: string;
  geometry: DescriptorSeparation;
  feature_print: DescriptorSeparation;
  reject_side_validated: boolean;
  note: string;
}

export type RuntimeState = "KUE_RUNNING" | "KUE_PAUSED" | "KUE_KILLED" | "KUE_RECOVERING";
export type Principal = "OWNER" | "MODEL" | "AUTOMATION" | "EXTERNAL";

export interface RuntimeBlock {
  state: RuntimeState;
  killed_at: number | null;
  killed_by: Principal | null;
  reason: string | null;
  latch_path: string | null;
  latch_error: string | null;
}

export type AccessStateName = "NO_PERSON" | "UNKNOWN_PERSON" | "IDENTITY_UNCERTAIN" | "AUTHORIZED_USER"
  | "AUTHORIZED_USER_LOW_CONFIDENCE" | "MULTIPLE_PEOPLE" | "AUTHENTICATION_REQUIRED" | "LOCKED";

export interface AccessBlock {
  state: AccessStateName;
  level: "LEVEL_0" | "LEVEL_1" | "LEVEL_2" | "LEVEL_3" | "LEVEL_4";
  phase: "OWNER_PRESENT" | "OWNER_LEFT" | "LOCKED" | "UNOBSERVED";
  detail: string;
  owner_last_confirmed_seconds_ago: number | null;
  os_auth: "STRONG" | "PHYSICAL" | null;
  os_auth_expires_in_seconds: number | null;
  lock_reason: string | null;
  /** Why identity reads as it does: a measured match, a frame that measured nothing (and why), or contrary evidence. */
  basis: string;
  held_without_measurement_seconds: number | null;
  requirements: { operation: string; level: string; fresh: boolean; owner_gesture_only: boolean }[];
}

export interface VoiceBlock {
  state: string;
  microphone_permission: string | null;
  voice_active: boolean;
  level_db: number | null;
  speech_recognition: string;
  speaker_identity: string;
  detail: string | null;
}

export interface ContextObject {
  schema_version: number;
  generated_at: number;
  runtime: RuntimeBlock;
  access: AccessBlock;
  voice: VoiceBlock;
  identity: {
    state: IdentityState;
    confidence: ConfidenceBreakdown;
    detail: string;
    accept_threshold: number | null;
    reject_threshold: number | null;
    combined_distance: number | null;
    geometry_ratio: number | null;
    featureprint_ratio: number | null;
    descriptors_agree: boolean;
    held_for_seconds: number | null;
    enrolled_samples: number;
    reject_side_unvalidated: boolean;
  };
  people_detected: number;
  tracks: PersonTrack[];
  activity: {
    state: ActivityStateName;
    label: string;
    human: string;
    confidence: ConfidenceBreakdown;
    evidence_ids: string[];
  };
  computer: {
    frontmost_app: string | null;
    frontmost_bundle_id: string | null;
    idle_seconds: number | null;
    recent_input: boolean | null;
    recent_input_threshold_seconds: number;
    age_seconds: number | null;
  };
  sensors: {
    sensing_process: string;
    camera_state: string;
    camera_state_reported: string;
    camera_permission: string;
    camera_device: string | null;
    camera_detail: string | null;
    processed_fps: number | null;
    computer_sampling_reported: boolean | null;
    readings_after_pause: number;
    paused: boolean;
  };
  observations: Observation[];
  inferences: Inference[];
  predictions: Prediction[];
  unknowns: string[];
  contradictions: string[];
  conditions: Condition[];
  resources: Resources;
  environment: EnvironmentBlock;
  evidence: EvidenceItem[];
  recent_events: LanternEvent[];
  remembered_events: RememberedEvent[];
  capabilities: Capability[];
  identity_check: SeparationReport | null;
  config_note: string;
}

export const SOURCE_LABEL: Record<Source, string> = {
  camera_vision: "camera · Apple Vision",
  system_workspace: "system · NSWorkspace",
  system_hid: "system · HID idle timer",
  temporal: "temporal · event history",
};

/** Whether KUE is listening for its name, in KUE's own words.
 *
 *  `listening` is what is RUNNING — the core decides it from a live report and
 *  the same conditions that start the microphone — while `enabled` is only what
 *  the owner asked for. The window shows both and invents neither. */
export interface WakeInfo {
  enabled: boolean;
  phrase: string;
  listening: boolean;
  said: string;
  why_not: string | null;
}
