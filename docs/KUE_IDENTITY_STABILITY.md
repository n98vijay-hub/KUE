# KUE identity stability — investigation

Opened 2026-09-17. Status: **FAILED live**, one root cause **established**, the
others **not yet established**. Nothing is fixed in this document.

Tags: **MEASURED** (a number read or timed on this Mac, with how) ·
**ESTABLISHED** (a cause the measurements show) · **HYPOTHESIS** (consistent
with the evidence, not shown) · **DESIGN OPTION** (not decided) ·
**EXPERIMENT** (what would settle it).

## Acceptance criterion (the owner's)

With the owner sitting normally in front of KUE for a meaningful session,
access stays steady unless there is real contrary evidence: nobody there, a
second person, a stranger, the camera failing, pause, lock or kill.
**"Conflicting identity = do not assume owner" stays true after any fix.**

## Evidence used

- KUE's event log and context snapshots on this Mac,
  `~/Library/Application Support/Lantern/lantern.sqlite3`, read only. Only the
  running app writes to it. It keeps kinds, states and reasons — never images,
  face measurements, distances or track ids (those are refused by the privacy
  policy for memory), so it can say *what* changed and the stated reason, not
  the numbers behind a face decision.
- Two timing experiments run on 2026-09-17 with **no camera and no
  microphone** (KUE was paused), in the session scratchpad: a Vision timing
  tool on a synthetic image, and the on-device model process
  (`KUE.app/Contents/MacOS/lantern-mind`) driven directly.
- The code paths: `sensing/Sources/LanternSense/Camera.swift` (analysis loop),
  `core/src/engine.rs` (`classify_frame`, `update_identity`,
  `perception_is_fresh`), `core/src/authz.rs`, `config/lantern.toml`.

## 1. How often, and why — MEASURED

Access changes per hour with the recorded reason (event log, `AccessChanged`):

| Hour | Changes | Measured conflict | Low capture quality | Stale reading | Pending corroboration | Multiple people | No person | To owner L2 |
|---|---|---|---|---|---|---|---|---|
| 2026-09-15 08 | 566 | 264 | 6 | 1 | 7 | 12 | 10 | 245 |
| 2026-09-15 13 | 763 | 234 | 49 | 10 | 43 | 2 | 53 | 180 |
| 2026-09-15 19 | 830 | 47 | 354 | 0 | 0 | 0 | 262 | 69 |
| 2026-09-16 09 | 755 | 290 | 14 | 13 | 53 | 3 | 63 | 237 |
| 2026-09-17 10 | 43 (4 min) | 3 | 1 | 9 | 5 | 7 | 0 | 16 |

(The "multiple people" column counts transitions *into* MULTIPLE_PEOPLE; each
is followed by a LOCKED transition.) The mix changes with the session: measured
conflicts dominate morning desk sessions, low capture quality and "no person"
dominate the evening of 2026-09-15, and on 2026-09-17 stale readings during
model answers and a burst of extra faces dominate. **There is more than one
cause.** The 2026-09-14 fix (carrying a match through unmeasurable frames) did
not address any of these.

## 2. Stale readings during model answers — ESTABLISHED

**The pattern (MEASURED, event log, 2026-09-17).** Three spoken questions went to
the on-device model at 10:45:13, 10:45:44 and 10:46:39. Each time access went to
IDENTITY_UNCERTAIN with reason STALE_READING about 8–9 s after the prompt was
sent, recurred every few seconds, and recovered on the same second the answer
arrived (10:45:33→ recovery 10:45:30; 10:46:19; 10:47:10). No stale reading
occurred outside those windows.

**Not the app stalling (MEASURED, context snapshots).** Snapshots are written
by the app's pump every ~10 s. Through all three answers the gaps stayed at
10.0–10.5 s and the camera state stayed RUNNING. The consumer kept running; the
face readings it was waiting for stopped arriving.

**The sensing layer stalls under model load (MEASURED, timing experiment).**
The two Vision requests the analysis loop makes for every frame — face
landmarks, then the image feature print — timed on a 1280×720 synthetic image
at the loop's 4 Hz:

| Condition | Frames | p50 | p95 | max |
|---|---|---|---|---|
| Alone | 43 | 24 ms | 29 ms | 96 ms |
| While the on-device model answered three questions | 54 | 26 ms | **3,142 ms** | **6,307 ms** |

Landmarks alone reached 3,442 ms and the feature print 3,098 ms under load.

**When, exactly (MEASURED, timeline run).** Vision frames timed against the
model's own messages, one run of three questions:

| Model | Vision during it |
|---|---|
| prompt sent at +4.1 s → first text at +23.7 s | stalls of 771, 507, then **3,067, 2,854, 2,984, 3,003, 3,036 ms back to back** |
| prompt sent at +25.8 s → first text at +40.2 s | **3,068, 3,009, 2,991, 3,002 ms back to back** |
| prompt sent at +42.3 s → first text at +57.1 s | one stall of **6,224 ms**, then normal |
| after first text, and between questions | normal (25 ms) |

So for the whole time the model reads its prompt, before it writes anything,
Vision completes about one frame every 3 s instead of four a second, and
recovers the moment text appears. A reading every ~3 s against a 3.0 s stale
limit is exactly the flicker in the log. The model is slowed too: first text
took 14.4–19.6 s with Vision running, against 10.3–11.3 s without.

**What it is not (MEASURED, E2, one run each).** Pinning Vision's requests to
the CPU through the public compute-device API did not remove the ~3 s stalls
(p95 2,971 ms); running the model process at background priority
(`taskpolicy -b`) did not either (p95 3,009 ms). Plain CPU scheduling priority
is not the lever, and the API-level CPU pin was not enough — whether because
some Vision stage ignores it or because the shared resource is not the Neural
Engine is **not established**.

**The mechanism (ESTABLISHED from code + the above).** The analysis loop is
serial: one frame's requests are awaited before the next frame is taken
(`Camera.swift`, `startLoop`). A multi-second Vision stall means no reading is
emitted for that long. The core treats a reading older than
`observation_stale_seconds = 3.0` as stale (`perception_is_fresh`), and a stale
reading is an *immediate* uncertain state — not carried like an unmeasurable
frame. So each Vision stall longer than 3 s drops the owner to LEVEL_0.

**Not established:** which shared resource the model and Vision contend for.
CPU priority is ruled out and an API-level CPU pin did not help (E2, done).

**User-visible cost:** while KUE is answering, the owner is not recognised, so
anything else asked in that time is refused, and data-bearing speech waits.

## 3. "A measurement did not match you" — NOT ESTABLISHED

**What the label means in code (ESTABLISHED).** A frame is *measured* when both
descriptors (landmark geometry and the Vision feature print) are available. It
is a match only if **both** distance ratios are ≤ `accept_ratio` = 1.15 × the
owner's own enrollment spread; a stranger only if both are ≥ `reject_ratio` =
3.0; **anything in between is IDENTITY_UNCERTAIN**, recorded as
MEASURED_CONFLICT, and demotes at once. So most "conflicts" are *ambiguous*
frames, not evidence of another person — but the code cannot tell which from
the log.

**Pattern (MEASURED).** 304 on 2026-09-16, each followed by a match about a
second later; 264 in one hour on 2026-09-15.

**HYPOTHESIS.** The feature-print distance drifts with light and background:
`config/lantern.toml` records a measurement on this Mac where it reached 4.5×
the enrollment spread across 20 minutes while geometry stayed at 0.91×. Frames
where it crosses 1.15× for a moment would produce exactly this pattern. Other
candidates: head pose near the limits, blur, a face crop that shifts with the
box. None can be separated without the per-frame numbers, which are not stored.

## 4. Low capture quality and "no person" — NOT ESTABLISHED

354 low-quality and 262 no-person transitions in one evening hour
(2026-09-15 19:00). **HYPOTHESIS:** evening light; Vision missing the face for a
frame. The existing rule already carries a match through an unmeasurable frame
for 2 s; these episodes exceeded it, or started from a state with no match to
carry. Needs E1.

## 5. Two or three faces — NOT ESTABLISHED, needs the owner

2026-09-17 10:44:45–10:45:04 and 10:47:41: KUE reported 2, then 3, then 2 faces,
locking the session and revoking Touch ID each time (7 transitions). The log
cannot say whether someone else was there: face boxes and detection confidence
are not kept. **Question for the owner:** was anyone else, a photo, a screen
showing a face, or a reflection in view then? If not, these are false
detections, and the rule "a second face locks at once" turns them into lockouts.
Any fix must keep a *real* second face locking immediately.

## 6. Requests refused while identity was settling — MEASURED

10:45:02 on 2026-09-17: a spoken question was refused with PENDING_CORROBORATION
— after a MULTIPLE_PEOPLE lock, a match needs `confirm_frames = 3` agreeing
frames before LEVEL_2, and the question arrived inside that window.

## 7. The stability harness — DESIGN (not built)

Goal: measure everything the owner listed without storing images or per-person
biometric data beyond the session.

**Where:** inside `lantern-sense`, off by default, started only by the owner
from Diagnostics (owner gesture), stopped by pause, lock and kill like the
camera.

**What it measures, per analysed frame (derived numbers only):** frame capture
time and analysis start/end (so frame interval, Vision latency per request,
emit interval, reading age at the core); face count with each face's detection
confidence and box size; capture quality; yaw/pitch/roll; mean brightness; the
geometry ratio and feature-print ratio against the enrollment reference; track
continuity (same track or new, never the id); whether the on-device model was
generating; sensing and shell CPU, memory footprint, thermal state; the access
state and reason the core then decided.

**What it keeps — DESIGN OPTION for the owner.** (a) Aggregates only:
histograms and counts per condition (e.g. ratio distribution while matched vs
at each conflict), shown in Diagnostics, discarded at session end. (b) Also a
per-frame file for one session, in Application Support, deleted automatically
after the investigation. Option (b) keeps face-derived numbers on disk, which
policy v1 does not allow for memory; it would need the owner's explicit
decision. Option (a) needs none.

**Acceptance run:** 20–30 minutes seated, normal work, including five spoken
questions answered by the model; then a short run with a consenting second
person entering and leaving view.

## 8. Experiments

- **E1** — Harness session (§7) with the owner: which descriptor and which
  ratio produce each conflict; capture quality and pose at each low-quality
  episode; confidence and size of any extra face.
- **E2 — DONE 2026-09-17.** Vision pinned to CPU under model load: still
  stalls. Model at background priority: still stalls. Neither is the lever.
- **E2b** — What else stops during prefill: time a pure-CPU loop and a Metal
  (GPU) compute loop under the same load; watch the system with
  `powermetrics` (needs the owner's password — the owner runs it) for ANE and
  GPU activity during prefill.
- **E3** — Whether a stall is visible to the core as "sensing alive but slow"
  rather than "sensing gone": the sensing layer already emits a periodic
  health/status message; measure whether it keeps arriving during a stall.

## 9. Fix options — DESIGN OPTIONS, none chosen

For stale readings (cause established):
- Remove the overlap: most requests do not need the on-device model at all
  (see `KUE_PERFORMANCE_PROFILE.md`); a request answered by rule causes no
  stall. A smaller prompt shortens the prefill, which is the stall window.
- Reduce the contention: E2 ruled out CPU priority; E2b may find a resource
  face analysis can use that prefill does not.
- Tell apart "sensing alive but delayed" from "nobody is measured": carry an
  earned match across a delay only while the sensing process is demonstrably
  alive (E3), on the same face track, for a short bounded time, and demote the
  moment a measurement arrives that does not match. A delay is not evidence
  about who is there; it must also never *extend* access past lock, pause,
  kill, or a camera stop.

For ambiguous measurements (cause not established): nothing until E1 says which
descriptor and how far. Candidates then: treat an in-between frame like an
unmeasurable one within the existing 2 s hold, while a stranger-side frame, a
second face or a track change still denies at once; or re-anchor the enrollment
reference for lighting. **Not an option:** raising `accept_ratio` or lowering
`reject_ratio` without the measurements.

## 10. Order

1. E2 — done. E2b (no camera; the `powermetrics` part needs the owner).
2. Build the harness (§7, aggregates only) — small, owner-gated.
3. E1 with the owner, plus the answer to §5's question.
4. Fix stale readings; re-run E1; count access changes.
5. Fix the ambiguous-measurement cause if E1 establishes it; re-run.
6. Live acceptance run; registry `owner_session` and `identity_matching`
   updated only from its result.
