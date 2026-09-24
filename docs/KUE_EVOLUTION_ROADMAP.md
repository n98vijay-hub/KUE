# KUE evolution roadmap

**2026-09-19.** Written against measured state (`KUE_CURRENT_STATE.md`), not a
feature wish-list. Each rung names the **gate** that must be measured before the
next one starts. A rung is never "done because the tests pass".

The rule that orders everything: *a capability that rests on an unreliable layer
inherits its unreliability and multiplies its consequences.* Today the owner's
authorization changes up to 830 times an hour. Every capability above it —
spoken requests, actions, personal answers, memory, proactivity, computer use —
is gated on it.

```
                  ADAPTIVE PERSONAL AI          ← the product
                          ▲
                    PROACTIVITY                  needs: memory + calendar + stable identity
                          ▲
            WEB RESEARCH ─┴─ COMPUTER USE        needs: tools + planning + a threat model
                          ▲
                      PLANNING                   needs: tools with schemas
                          ▲
             MEMORY ──────┴────── TOOLS          needs: runtime + policy
                          ▲
                      RUNTIME                    one authoritative state machine
                          ▲
                    PERFORMANCE                  answers that do not starve perception
                          ▲
                     IDENTITY                    the owner is recognised, stably
                          ▲
                   RELIABILITY                   observability, recovery, verification
                          ▲
                   FOUNDATION                    one repository, one truth, one build
```

## R0 · FOUNDATION — one repository, one truth  *(hours)*

**Why first.** Until 2026-09-19 the newest analysis of KUE existed only as
untracked files inside Claude worktrees, and three branches each held documents
the others lacked. `main` is still 74 commits behind and is the pre-rename
Lantern prototype: anyone who opens the repository folder sees the wrong product.

**Done in this session:** the seven documents are committed; the three
documentation lines are merged into `kue/runtime-safety` @ `1377f13`, whose
source is byte-identical to the audited tip.

**Remaining, needs the owner's word:** fast-forward `main` to this line (pure
fast-forward, nothing is rewritten); decide what happens to 12 empty agent
branches and 11 GB of `.claude/worktrees`; move `Claude outputs/` (personal
résumés) out of the repository folder; put `KUE.app` somewhere that is not a
build directory inside a Claude worktree.

**Gate:** `git rev-list --count main..kue/runtime-safety` = 0, and the app the
owner opens is built from the branch the owner reads.

## R1 · RELIABILITY — KUE can tell what happened  *(days)*

Not a feature; the precondition for diagnosing everything else.

- Distinguish **delayed** from **absent**: a sensing heartbeat, so the core can
  tell "alive but late" from "no reading". Today both look identical and both
  destroy authorization.
- Per-stage latency instrumentation (wake → voice → intent → context → model
  queue → prefill → generation → action → verification), recorded as
  aggregates, never content.
- Subsystem health: HEALTHY / DEGRADED / FAILED / RECOVERING for camera,
  microphone, model, executor, database, helpers — and a visible answer to
  "why is KUE not doing anything?" (today: it has been **paused for two days**
  and says so nowhere the owner looked).
- Database integrity check and a stated recovery path.
- Stop `#[ignore]` test rot: the live suite runs as a set, or its tests are
  deleted. One has been asserting a sentence that changed weeks ago.

**Gate:** from a 30-minute recording, every stall > 1 s is attributable to a
named stage, and a killed helper appears as DEGRADED within one tick.

## R2 · IDENTITY — the owner stays the owner  *(1–2 weeks, owner at the Mac)*

The measured failure: a perception older than 3.0 s is returned as
`Immediate(IdentityUncertain)` — delay is treated as contradiction — while the
on-device model's prefill stalls Vision for 3–6 s.

- Explicit states: FRESH_MEASUREMENT · MEASUREMENT_DELAYED · MEASUREMENT_STALE ·
  NO_PERSON · UNKNOWN_PERSON · IDENTITY_UNCERTAIN · AUTHORIZED_USER ·
  MULTIPLE_PEOPLE · CAMERA_UNAVAILABLE.
- Temporal identity: an *earned* match may be carried across a bounded delay on
  the same track while the sensing process is proven alive. **Uncertainty never
  creates authorization**; a measured conflict, a second face, a new track, a
  dead sensor or a stale reading still demote at once.
- Measure the reject side for the first time, with a consenting second person.
- Decide the in-between-measurement rule from harness data, not from taste.

**Gate:** owner seated 30 minutes with five spoken questions → **< 10 access
changes/hour** with no real contrary evidence; **0 false accepts** across a
second-person run; numbers exported from the event log and committed.
**Invariant that may not be traded for this gate:** conflicting identity fails
closed.

## R3 · PERFORMANCE — answers that do not cost perception  *(1 week, parallel in `core/`)*

p50 19.9 s, p90 31.4 s, max 107.2 s, measured from KUE's own log. Latency is not
only a product problem: prefill is what starves Vision and breaks R2.

- Route by class: LOCAL_DETERMINISTIC (arithmetic, capability and status
  questions — already possible by rule), LOCAL_RETRIEVAL (storage, folders),
  LOCAL_REASONING (the model), then later EXTERNAL_RESEARCH, COMPUTER_AUTOMATION.
- Remove the ~820-character capability line from every prompt; shrink context;
  measure prewarming; consider answering while the camera keeps its slot.

**Gate:** p50 time-to-first-text for the ten most common requests, before and
after, measured in the app; arithmetic and capability questions never reach the
model; Vision frame interval during an answer stays under the staleness limit.

## R4 · RUNTIME — one authoritative state machine  *(1 week)*

STARTING · INITIALIZING · LOCKED · READY · LISTENING · UNDERSTANDING · THINKING ·
PLANNING · ACTING · VERIFYING · WAITING_FOR_USER · UNCERTAIN · PAUSED · KILLED ·
RECOVERING · ERROR — decided in the core, reflected by window, voice, logs and
automation. The window already renders a core-decided projection; this extends
that discipline to the whole runtime and removes the last places where the shell
infers state. Prerequisite for splitting `lib.rs` (1,933 lines) and `engine.rs`
(2,310 lines) by concern.

**Gate:** every user-visible state string traces to one enum in the core; a
scenario replay drives all sixteen states; no React component computes a state.

## R5 · TOOLS — capabilities become declarations  *(1–2 weeks)*

The registry (41 rows) describes capabilities in prose for humans. A tool
registry describes them for the *system*: id, input/output schema,
authorization level, privacy class, risk, preconditions, executor, verifier,
rollback, availability, health, latency, last verified, version. The model then
chooses **among declared tools**; it still executes nothing.

**Gate:** the existing verbs (open app, open folder, find/open document, list,
trash, restore, inspect storage) are declarations; the broker refuses anything
not declared; an unavailable tool is reported as unavailable rather than
hallucinated.

## R6 · MEMORY — KUE remembers what matters  *(1–2 weeks)*

Not chat history. Categories with provenance: preference, decision, ongoing
goal, project, important fact, routine, past action, successful/failed workflow,
temporal commitment, correction. Each with source, confidence, timestamp, scope,
retention, privacy class and a reason for keeping it. Corrections ("ask me
first") are memory, and memory never overrides policy.

**Gate:** "remember that…" → "what do you know about…" survives a restart; an
unknown question answers *"I don't have that in my memory"*; deletion is real.

## R7 · PLANNING — goals instead of templates  *(2 weeks)*

Five fixed templates today. A plan engine: GOAL → PLAN → STEP → EXECUTE →
OBSERVE → VERIFY → ADAPT, where a failed step is classified RECOVERABLE /
REQUIRES_USER / REPLAN / ABORT instead of blindly continuing.

**Gate:** a goal with a deliberately failing middle step replans or stops and
says why — proven by replay, then once live.

## R8 · COMPUTER USE and R9 · WEB RESEARCH *(later, in that order)*

Both need R4–R7 and a written threat model. Computer use means Accessibility —
the largest single increase in what KUE could do, and in what a mistake costs.
Web content is untrusted input that may never become an instruction. Neither may
start while identity flaps and answers take twenty seconds.

**Gate before either:** a threat model document, and the security hardening
below already in place.

## R10 · PROACTIVITY — last, on purpose

Needs memory, calendar and stable identity. An interruption policy (importance,
urgency, confidence, what the owner is doing, quiet hours, recent interruptions)
comes before the first notification is ever shown.

## Running alongside — security hardening

Independent of the ladder, different files, must land before R8/R9:
in-process LocalAuthentication or signature-verified helpers; helpers loaded
only from the bundle (today `kue-auth` is found by walking six parent
directories, then a **compile-time source path baked into the release binary**,
and its stdout is trusted); Developer ID + hardened runtime + entitlements
(today: ad-hoc, no team id, no entitlements at all); encrypt enrollment and
probe descriptors; the threat model.

## What must NOT be built yet, and why

| Not yet | Because |
|---|---|
| Claude / any cloud model | Multiplies a 20 s path and adds an outbound-data policy KUE does not have. R3 first |
| Web research | Untrusted input into a system whose planner is five templates |
| Accessibility / in-app control | Highest-consequence capability; needs R4, R5 and a threat model |
| Proactive behaviour | Would interrupt the owner based on an identity that changes 830 times an hour |
| Speaker identity | Owner's model-licence decision is open, and voice can only ever corroborate |

## Invariants no rung may trade away

UNKNOWN IDENTITY ≠ OWNER · IDENTITY UNCERTAINTY ≠ OWNER · WAKE ≠ AUTHORIZATION ·
MODEL OUTPUT ≠ AUTHORIZATION, POLICY, or COMPLETION · ACTION ≠ SUCCESS ·
SUCCESS REQUIRES VERIFICATION · KILL SWITCH OVERRIDES EVERYTHING · PAUSE STOPS
SENSING · PRIVACY FIREWALL CANNOT BE BYPASSED BY A MODEL · UNKNOWN CAPABILITY,
OPERATION or CLASSIFICATION = DENY · CONFLICTING IDENTITY = FAIL CLOSED.

If a gate can only be met by weakening one of these, the gate is wrong — report
it and stop.
