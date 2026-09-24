# Architecture

## Four layers, with enforced boundaries

```
┌──────────────────────────────────────────────────────────────┐
│  React / TypeScript        src/                              │
│  Draws the context object. Owns no state of consequence.     │
│  Has NO command that reaches a sensor.                       │
└───────────────▲──────────────────────────────┬───────────────┘
                │ lantern://context event      │ invoke()
┌───────────────┴──────────────────────────────▼───────────────┐
│  Tauri shell               src-tauri/                        │
│  Window, IPC, and supervision of the sensing process.        │
│  Does not perceive. Does not reason.                         │
└───────────────▲──────────────────────────────┬───────────────┘
                │ SensorMessage                │ command lines
┌───────────────┴──────────────────────────────▼───────────────┐
│  Rust core                 core/                             │
│  Temporal state, evidence, confidence, context object,       │
│  local memory. Never touches a device.                       │
└───────────────▲──────────────────────────────┬───────────────┘
                │ JSON lines (stdout)          │ JSON lines (stdin)
┌───────────────┴──────────────────────────────▼───────────────┐
│  Swift sensing layer       sensing/                          │
│  AVFoundation + Vision + NSWorkspace + HID idle,             │
│  plus its own CPU, memory, thermal and power readings.       │
│  Emits MEASUREMENTS ONLY. Cannot express a conclusion.       │
└──────────────────────────────────────────────────────────────┘
```

The boundaries are structural, not conventional:

- `core::sensor::SensorMessage` has no field that could carry a conclusion —
  no `is_owner`, no `activity`. The sensing layer *cannot* send one.
- `evidence::compute_confidence` is the only function in the codebase that
  produces a confidence number, and it is a pure function of
  `(evidence, config)`.
- Every decision threshold lives in `config/lantern.toml`. A bare numeric
  comparison in a reasoning path is a bug.

## Why the sensing layer is a separate `.app`

This was the first thing built, because it determines everything else.

Measured on macOS 26.7 (Xcode 26.6, Swift 6.3):

| Shape | Result |
|---|---|
| Bare `swiftc` binary | `requestAccess` returns **false instantly**, no prompt, status stays `notDetermined` |
| Same binary + `-sectcreate __TEXT __info_plist` | `Bundle.main` reads the usage string, but TCC **still auto-denies** |
| `.app` bundle, spawned from a shell | **Denied** — TCC attributes the request to the shell |
| `.app` bundle, launched via LaunchServices | **Prompts, then grants** |
| Helper spawned by a LaunchServices-launched parent `.app` | **Prompts**, attributed to the parent |

The conclusion: macOS TCC attributes a camera request to the **responsible
process**, and refuses to prompt for anything without a stable bundle identity.
So the sensing layer ships as `LanternSense.app` nested inside
`Lantern.app/Contents/Resources/`, and `NSCameraUsageDescription` appears in
**both** Info.plists — the helper's own, and Lantern's, because Lantern is the
responsible process for the helper's request.

This shape also buys crash isolation from AVFoundation and Vision, and lets the
sensing layer be exercised on its own:

```bash
open sensing/bundle/LanternSense.app --args --selftest --enroll 6 --duration 30 --out /tmp/s.jsonl
```

## The protocol

Newline-delimited JSON. The core writes commands to stdin; the sensing layer
writes messages to stdout.

**A real bug this surfaced:** Vision writes diagnostics straight to fd 1
(observed: `VTEST: error: DetectFaceLandmarksRequest was cancelled.`), which
corrupts a JSON-lines stream. The sensing layer now takes a private `dup(1)` for
the protocol and points fd 1 at stderr, so framework noise can never reach the
core. The core *also* skips unparseable lines, as defence in depth.

## Identity

Two descriptors, both derived from the camera and therefore **not independent**:

1. **Geometry** — centroids and extents of ten landmark regions, in a frame
   whose origin is the pupil midpoint, x-axis along the inter-pupil line, and
   scale the inter-pupillary distance. This removes roll and scale. It does not
   remove yaw or pitch, which is why head pose is separately bounded.
2. **FeaturePrint** — `GenerateImageFeaturePrintRequest` on a tightly cropped
   face, compared with Apple's own `distance(to:)`.

Thresholds are expressed in units of **your own measured enrollment spread**
(leave-one-out p95 distance between your samples). A ratio of 1.0 means "as far
from your enrolled samples as they typically are from each other".

Claims are conjunctive: `MY_FACE_CONFIRMED` requires *every* available descriptor
inside the accept band, `UNKNOWN_PERSON` requires *every* one past the reject
band, and at least `min_descriptors_for_claim` must be present. Anything else is
`IDENTITY_UNCERTAIN`, with the disagreement surfaced as a contradiction.

Promotion is deliberately asymmetric. `MY_FACE_CONFIRMED` and `UNKNOWN_PERSON`
are assertions about a person, so they must hold for `confirm_frames` frames.
`NO_FACE` applies immediately — a stale "confirmed" after the lens is covered
would be a lie, and there is a test for exactly that.

`MULTIPLE_PEOPLE` outranks any match. Lantern will not name someone in a group.

### Frames, not evaluations

Every frame is classified once:

- **Immediate** — `NO_FACE` or `MULTIPLE_PEOPLE`. Applied at once.
- **Unmeasurable** — one face, but pose or quality out of bounds, or a descriptor
  missing. It says nothing about who the person is, so a standing match is
  *carried* for up to `hold_unmeasurable_seconds` and the interface says it is
  being carried and for how long.
- **Measured** — a real comparison. It counts toward `confirm_frames`, and if it
  disagrees it demotes at once, carry or not.

`confirm_frames` counts distinct camera frames. Computer-activity messages
re-run derivation between frames, and before this was fixed they let one frame
count several times toward confirmation.

### Shown instantly, remembered once settled

The context object always shows the current identity state. An
`IDENTITY_STATE_CHANGED` event is written only when a state has held for
`event_min_seconds`, and is dated from when it began. States that did not hold
are counted and disclosed in the next recorded event. Both counters reset on
pause, so a disclosure never reaches back across one.

The measurements behind both numbers are in comments beside them in
`config/lantern.toml`.

### Identity check

Probe samples — captured, with consent, of someone who is not you — are stored
apart from your enrollment and never used for matching. The sensing layer
reports, per descriptor, the within-owner and owner-vs-probe distance ranges
and whether they separate. Until probes exist the verdict is
`INSUFFICIENT_DATA`, and `reject_side_validated` is false.

### Descriptor versioning

`kDescriptorVersion` in `sensing/Sources/LanternSense/Enrollment.swift` versions
the *extraction pipeline*. Change how a descriptor is computed and stored samples
become incomparable, so the enrollment is discarded rather than silently
misread. This is the cost of storing descriptors instead of images — and storing
images is not on the table.

## Confidence

```
contribution = weight × strength × reliability      (signed by polarity)
capacity     = weight × reliability

raw        = (Σ supporting − Σ contradicting) ÷ Σ capacity
temporal   = floor + (1 − floor) × min(1, stable_seconds ÷ full_seconds)
confidence = clamp(raw, 0, 1) × temporal
```

- `weight` — importance of that kind of evidence, from config.
- `strength` — how strongly the observation holds (0–1), from the measurement.
- `reliability` — trust in the source, from config.

The falsifiable property, enforced by
`displayed_confidence_is_recomputable_from_displayed_evidence`: the rows shown in
the interface are exactly the rows that were counted, and their capacities sum to
the displayed denominator. You can check the number by hand.

A missing sensor is not silence — it is explicit *contradicting* evidence
(`camera_unavailable`, `computer_unavailable`), so a conclusion drawn with a
sensor off cannot reach high confidence.

Polarity is decided against the statement actually displayed. "No face visible"
supports "No activity detected" and contradicts "At the computer, interacting
with it"; an earlier version counted every item toward "at the computer"
whatever was shown. A missing sensor contradicts any statement that rests on it.
Paused is its own activity state, and the interface shows no confidence meter
for it, because nothing is being measured.

## Failure states

`ContextObject.conditions` lists every named failure state on every update, each
`ACTIVE`, `CLEAR`, `NOT_APPLICABLE` or `NOT_IMPLEMENTED`, with a sentence of
detail. Listing the clear ones is deliberate: a missing row cannot be told apart
from an unchecked one.

| Condition | Source |
|---|---|
| `CAMERA_UNAVAILABLE`, `PERMISSION_DENIED` | camera status from the sensing layer |
| `MODEL_UNAVAILABLE` | a Vision request that *failed* — reported as `analysisFailed`, never read as "no face" |
| `FRONTMOST_APP_UNAVAILABLE`, `INPUT_ACTIVITY_UNAVAILABLE` | missing or stale computer readings; a failed idle timer (−1) is unavailable, not idle |
| `SENSING_LAYER_DOWN` | the shell's supervision of the helper process |
| `STORAGE_UNAVAILABLE` | database open or write failures, including another running instance holding the lock; unwritten events are retried |
| `IPC_FAILURE` | detected by the interface itself when context updates stop arriving — the core cannot report its own silence |
| `NETWORK_UNAVAILABLE` | not applicable: Lantern has no network code |
| `MIC_UNAVAILABLE` | not implemented: there is no microphone path |
| `NO_FACE`, `UNKNOWN_PERSON`, `IDENTITY_UNCERTAIN`, `MULTIPLE_PEOPLE` | the current identity state; exactly one is active while the camera runs, with the identity block's explanation as detail |

## Cost

The sensing layer reports its own CPU time (`getrusage`) and physical memory
footprint (`TASK_VM_INFO`), the thermal state and Low Power Mode
(`ProcessInfo`), and battery (`IOKit` power sources) every 5 seconds, including
while paused. The shell measures itself the same way. The core turns cumulative
CPU time into percentages, kept separately for observing and paused time.

GPU and Neural Engine use and energy impact have no public per-process API.
They are listed as not measured rather than estimated.

Under `SERIOUS` or `CRITICAL` thermal state, or in Low Power Mode, the core asks
the sensing layer for `reduced_fps` and the context object says why. The policy
lives in the core; the sensing layer only applies the rate.

## Privacy firewall

`core/src/privacy.rs` stands between everything Lantern perceives and everywhere
data can go. Policy version 1 governs one destination with a real write path —
local memory — and pins the refusals for destinations that do not exist yet.

**Classification is exhaustive.** Every `DataKind` maps to a `PrivacyClass`
through a single `match`; an unclassified kind does not compile. Kinds and
destinations named at runtime (`check_tags`) that are not recognised are denied.

| Class | Interface | Local memory | Local model | External | Logs |
|---|---|---|---|---|---|
| NEVER_COLLECT | deny | deny | deny | deny | deny |
| NEVER_STORE | allow | deny | deny | deny | deny |
| DERIVED_ONLY | allow | deny | deny | deny | deny |
| PRIVATE | allow | allow | deny | deny | deny |
| USER_APPROVAL_REQUIRED | allow | deny¹ | deny¹ | deny¹ | deny |
| LOCAL_ONLY | allow | allow | allow | deny | deny |
| CLOUD_ALLOWED | allow | allow | allow | allow | allow |

¹ Until an approval flow exists, which is NOT_IMPLEMENTED. No data kind carries
CLOUD_ALLOWED in policy v1, and a test fails if one ever does without review.

**The store cannot be handed unchecked data.** `Cleared<T>` has a private field,
so only the privacy module can construct one. `Store::record_event`,
`record_snapshot` and `record_ledger` accept `&Cleared<_>` and nothing else.

**Memory is an allowlist.** A snapshot is a `MemorySnapshot` naming exactly what
is kept, not the context object with fields removed. A field added to the
context object is therefore not stored until it is classified and added. The key
set is pinned by a test. Mutation-checked: when the policy was deliberately
broken to allow joint positions into memory, joints still could not reach disk,
because the allowlist has no field to hold them.

**Evidence text** stored with events may quote derived ratios and durations —
that is what lets memory answer "why?" — but never raw coordinates, track ids,
boxes or descriptor distances. A marker-scan test over the events table enforces
this, mutation-checked to fail if a raw value enters the prose.

**Old data.** Snapshots carry `policy_version`. Rows written before the firewall
are version 0, counted in the interface, and deleted only when the owner chooses
to purge them. The purge compacts the database; a test reads the raw file and WAL
bytes to confirm the content is gone.

## Memory and provenance

Events that change a *conclusion* (identity, activity) are stored with the
evidence rows and confidence that produced them. Observations such as "a face
appeared" carry none — they are facts about a sensor. At launch, the most recent
40 events are read back before sensing starts and shown as remembered, so
everything in that list predates the launch. Databases from before provenance
existed are migrated in place; old rows keep no "why" rather than an invented
one.

The store holds an exclusive lock on `lantern.lock` beside the database for its
whole life.

## Pause

Pausing goes UI → IPC → shell. The shell sends `{"cmd":"pause"}` to the sensing
layer **before** it tells the core:

1. The sensing layer tears the `AVCaptureSession` down completely — inputs and
   outputs removed, not merely stopped — cancels computer-activity sampling, and
   abandons any pending enrollment or probe capture. The camera indicator light
   goes out.
2. The core discards perception and computer readings, resets the identity
   trackers and counters, and shows the `PAUSED` activity state.
3. A reading arriving while paused is dropped at ingest. One that arrives more
   than `in_flight_grace_seconds` after pause took effect means the sensing layer
   did not stop; it is counted and reported as a contradiction.

Resuming is the reverse: the core is unpaused first, then the sensing layer is
told to resume at the rate the core currently wants. Resume only ever follows
an explicit action.

`pause_stops_every_reading_from_the_real_sensing_layer` (in `src-tauri`) runs
this against the real Swift binary without ever starting the camera. It asserts
that computer readings arrive before pause, that none arrive and none are held
during 5 seconds of pause with sampling reported stopped, and that they return
after resume. Driven by hand over stdin, the same binary sent 3 readings before
pause, 0 during 5 seconds of pause and 4 after resume.

## Testing

- `cargo test -p lantern-core` — 78 tests: confidence arithmetic, identity
  promotion and carry, event settling, pause, staleness, polarity, failure
  states, cost, provenance and migration. Key tests were checked by reverting
  the fix and confirming they fail.
- `cargo test -p lantern` — 2 end-to-end tests that spawn the real sensing
  binary with no camera: pause, and the shell's self-measurement.
- `cargo run -p lantern-core --example replay -- stream.jsonl` — feeds a recorded
  sensing stream through the real parser and engine, with the stream's own
  timestamps as the clock. Useful for measuring a change against real data.

## Extending it

The interfaces that future signals would plug into already exist:

- A new sensor adds a variant to `SensorMessage` and emits measurements. It
  cannot emit conclusions — the enum has nowhere to put one.
- A new conclusion adds evidence items and reads its confidence from
  `compute_confidence`. It cannot invent a number.
- A new capability adds a row to `Engine::capabilities()` with an honest status,
  which the interface renders whether or not it is implemented.
