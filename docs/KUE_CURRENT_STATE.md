# KUE current state — the truth model

**2026-09-19.** Branch `kue/runtime-safety` @ `1377f13`, whose **source is
byte-identical to the audited tip** `b893684` (`git diff b893684 HEAD -- ':!docs'`
is empty). `KUE.app` on disk was built from `0263e59`, one doc-only commit behind.

This document replaces the prose version written 2026-09-17 (in git history). It
exists to be *reconciled*, not believed: every row names the evidence that would
change it.

## How a colour is earned

| Colour | Meaning | Rule applied here |
|---|---|---|
| **GREEN** | Live verified **and** reliable | Observed working on this Mac, in KUE's own event log or a live test run, **and** no recorded failure mode that makes it unreliable |
| **YELLOW** | Implemented, insufficiently verified | Code complete, automated tests pass, but never observed on this Mac — or observed only on the pre-rename Lantern build |
| **ORANGE** | Partial / degraded | Works, but with a measured defect that changes how it can be used |
| **RED** | Broken | Measured failing against its own requirement |
| **GRAY** | Not implemented | No code, or types/documents only |

**Evidence classes.** `LOG` = KUE's own event log, `~/Library/Application
Support/Lantern/lantern.sqlite3`, read read-only on 2026-09-19 (this is the
evidence the 2026-09-19 forensic audit was denied by the platform). `LIVE` = a
test or command this session ran on this Mac today. `TEST` = automated tests,
executed. `CODE` = read in the repository. No row is GREEN on CODE or TEST alone.

## What the log says, in four numbers

| Measurement | Value | Query |
|---|---|---|
| Access-level changes, worst hour | **830** (2026-09-15 19:00); 755 and 763 in two other hours | `SELECT strftime('%m-%d %H',ts,'unixepoch','localtime') h, COUNT(*) FROM events WHERE kind='AccessChanged' GROUP BY h ORDER BY 2 DESC` |
| Access changes less than 1 s apart | **2,372 of 4,653** (51%) | `LAG(ts) OVER (ORDER BY ts)` over `AccessChanged` |
| On-device model answer time | n=31 · min **9.6 s** · p50 **19.9 s** · p90 **31.4 s** · max **107.2 s** | `summary LIKE '%answered in%'` |
| Hands-free wakes, ever | **0** | `summary LIKE '%its name%' OR '%wake%' OR '%Heard%'` → 0 rows |

The access states behind those changes: `IDENTITY_UNCERTAIN at LEVEL_0` **2,095**
· `AUTHORIZED_USER at LEVEL_2` 1,196 · `NO_PERSON at LEVEL_0` 628 ·
`AUTHORIZED_USER_LOW_CONFIDENCE at LEVEL_1` 604 · `LOCKED` 76 ·
`MULTIPLE_PEOPLE` 38 · `AUTHENTICATION_REQUIRED` 17. **The single most common
thing KUE believes about its owner is that it is not sure who they are.**

## Updated 2026-09-24 — S10: memory in use

See the S10 section of `docs/KUE_MEMORY.md`.

| Row | Was | Now | Because |
|---|---|---|---|
| **Personal memory** | kept, retrieved, forgotten — but it changed nothing | **it changes what KUE proposes** | A confirmed preference about cleaning up seeds the goal's exclusions before anything is offered, and KUE quotes the owner's own sentence as the reason. LIVE on real files 2026-09-24 |
| **Approval, authorization, verification** | — | **unchanged, and tested to be** | Memory is context, never authority: a memory that sounds like permission grants none |
| **Conflicting preferences** | S9 asked at the moment the second was said | **also handled when both arrive from the store** | Neither is applied and KUE asks which holds |

Test counts: **560 core**, 11 shell + 13 opt-in, **40 window**, `tsc` clean.

## Updated 2026-09-23 — S9: personal memory

See `docs/KUE_MEMORY.md`.

| Row | Was | Now | Because |
|---|---|---|---|
| **Personal memory** (new row `personal_memory`) | GRAY — "nothing told to KUE is remembered" | **YELLOW, live across two processes** | What the owner asks KUE to keep, what it did and verified, decisions and expiring task notes — each with provenance. Written through the firewall and the pump, read back by a second process on this Mac, forgotten on request with the row removed |
| `memory_recall` (asking about the past from the event log) | GRAY | **GRAY, unchanged and now honest about it** | Split from the row above: one of them exists, the other does not, and the registry says which |
| **The model's place** | proposes plans | **proposes plans; cannot write, delete or verify a memory** | `proposed_by_model` yields CANDIDATE and nothing else; VERIFIED needs the action pipeline's read-back |
| **"What I remember"** | a sentence about a database | **a list of what is kept, with why and a Forget** | Someone KUE is not sure of sees none of it |

Test counts: **552 core**, 11 shell + 12 opt-in, **38 window**, `tsc` clean.

## Updated 2026-09-23 — S8b: the model proposes, the runtime decides

| Row | Was | Now | Because |
|---|---|---|---|
| **Plans** | YELLOW (validated, never run; no model connected) | **GREEN for the spine** | LIVE on this Mac 2026-09-23: the on-device model proposed two steps, KUE checked and showed them, the owner's "do it" approved that plan, both steps ran and were read back from the file system, the goal completed |
| **Model's place** | answers questions | **proposes plans, authorizes nothing** | The proposal is untrusted input: no field for risk, approval, verification or authorization, and one carrying any is refused unread |
| **Arithmetic** | square roots fell to the model | **roots and powers by rule; what it cannot do says so** | `43d9d52`, and cube roots/logs/trigonometry now answered by rule rather than a model |
| **Multi-step requests** | half-parsed into one action with the rest swallowed into a name | **go to the planner** | Found by the live run: a folder was nearly named "Reports in my KUE folder and put a note in it" |

Test counts: **531 core**, 11 shell + 10 opt-in, **36 window**, `tsc` clean.

## Updated 2026-09-22 — S7: conversation and plans

| Row | Was | Now | Because |
|---|---|---|---|
| **Conversation that survives several turns** (new) | — | **YELLOW → partly GREEN** | Live in the running app 2026-09-22: questions answered from runtime state, stop during a model answer, the storage plan on this Mac, a clarification; full exchange LIVE on KUE's own files through the real Trash (Touch ID stubbed). Corrections by name in the app and spoken input: owner test S7-L1 |
| **Stop** (new) | — | **GREEN for reads and model answers**, TEST for the rest | A running read stops within 256 entries; a change underway is not pretended stopped |
| **Plans** (was "Goals / plans") | never live | **YELLOW → partly GREEN (S8a, 2026-09-22)** | `plan.rs` + `transaction::{propose_plan, offer_plan, approve_plan}`: a checked plan is held, previewed, approved as exactly what was shown, and run by the existing transaction — each step authorized and verified. LIVE on this Mac: three tools in `~/KUE`, each read back (`--ignored s8_plan`), identity and proposer stubbed. Still no model proposing, no plan correction, and no window control for it |
| **The owner's work, withheld from a stranger** (new) | — | TEST | Questions, corrections, "do it" to a plan, undo and `get_runtime` need LEVEL_2; stop/cancel do not |
| Pause | YELLOW | YELLOW, **one false alarm explained** | "A listening for its name arrived 38.0s after pause" (09:58:17, 2026-09-22) was the listener reporting OFF as the app quit; wake listening was not on. The check no longer counts an OFF report |

The six fixes found by the live run are TESTED; the running app predates them.

Test counts: **493 core**, 11 shell + 8 opt-in live, **32 window**, `tsc` clean.

## Updated 2026-09-20 — what the first instrumented day changed

The perception-observability slice landed and ran. Rows below are unchanged
unless named here; `docs/evidence/KUE_FOUNDATION_OBSERVABILITY_2026-09-20.md`
carries the numbers and what they do not cover.

| Row | Was | Now | Because |
|---|---|---|---|
| **Knowing how well it can see** (new) | — | **GREEN** | 893 samples measured live on this Mac; registry row `perception_observability`, proof dated today |
| **On-device model answers** | ORANGE, "p50 19.9 s" end to end | ORANGE, **now split**: prefill 13,893 ms, generation 3,353 ms | KUE's own stage timings, first measured today |
| **Face identity matching** | RED, cause assumed to be staleness | RED, **cause now measured**: 65.6 % of uncertainty is an AMBIGUOUS measurement, 16.9 % a live pipeline being late, 0 % a reading that said somebody else | 15-minute window, owner present |
| **Identity state machine** | RED | RED, unchanged by this slice **on purpose** | The carry rule, thresholds and levels were deliberately not touched; a test asserts the decisions are identical with and without the new instrument |
| Pause | YELLOW | YELLOW, **contract written** | `docs/KUE_PAUSE_CONTRACT.md`: sensing genuinely stops; snapshots do not, and 14 hours of pause evicts the observed history |
| Storage cleanup | YELLOW | YELLOW, **its live test repaired** | It had been asserting wording removed weeks earlier |

Test counts today: **416 core** (was 388), 10 macOS shell (+6 opt-in, 4 run by
hand), **24 window**, `tsc` clean.

## Capability matrix

`SEC` = authorization level the capability needs. Owner = the workstream that may
change it (see `KUE_EVOLUTION_ROADMAP.md`); **one owner per file**.

### Perception and identity

| Capability | Implementation | Automated test | Live verified | Current failure | SEC | Depends on | Owner | Next validation |
|---|---|---|---|---|---|---|---|---|
| Camera capture | `Camera.swift`, AVFoundation 4 fps | none (no Swift tests) | **LOG** — 149 camera-state events | Serial capture loop stalls under model load | — | TCC camera | B Sensing | Frame-interval histogram from the harness |
| Face detection / landmarks | `Perception.swift`, Vision | logic only (injected) | **LOG** — 289 face-appeared events | 2–3 face bursts with one person (F5) | — | Camera | B Sensing | Confidence floor experiment |
| Face tracking | IoU tracker, 1.5 s timeout | logic | **LOG** | Track change demotes authorization | — | Detection | B Sensing | Track-continuity stats |
| Enrollment | `Enrollment.swift`, LEVEL_3 | gate tests | **LOG** — 45 enrollment events, 105 KB `enrollment.json` | Plaintext at rest (S4) | L3 | — | E Security | Keychain-wrapped store |
| **Face identity matching** 🔴 | geometry + FeaturePrint ratios | **TEST** (thresholds) | **LOG — failing** | **RED.** `IDENTITY_UNCERTAIN` is the most common state; reject side never measured; no liveness | — | Enrollment, freshness | C Identity | Seated 30-min run: < 10 changes/h |
| **Identity state machine** 🔴 | `engine.rs:1194-1250` | 85 scenario tests | **LOG — failing** | **RED.** A perception older than **3.0 s** returns `Immediate(IdentityUncertain)` — delay is indistinguishable from contradiction | — | Camera freshness | C Identity | Replay: delayed-but-alive ≠ stale |
| **Access levels / owner session** 🔴 | `authz.rs` LEVEL_0–4 | yes | **LOG — failing** | **RED.** 830 changes/hour; inherits identity | — | Identity | C Identity | Same run; reasons histogram |
| Multiple-person lock | `face_count > 1` → lock | yes | **LOG** — 38 lock events | Fails safe; false positives lock the owner out | — | Detection | C Identity | With a consenting second person |
| Stranger → lock | `authz.rs` | yes | **never** (no stranger has ever been in front of it) | Reject side unmeasured — FAR unknown | — | Matching | C Identity | Identity Check with a second person |
| Touch ID / password | `kue-auth`, LocalAuthentication | gate tests (stubbed) | **never completed an action** | Helper trusted by stdout (S1) | L3/L4 | helper | E Security | Trash move from the window with a real prompt |

### Runtime, privacy, safety

| Capability | Implementation | Automated test | Live verified | Current failure | SEC | Depends on | Owner | Next validation |
|---|---|---|---|---|---|---|---|---|
| **Privacy firewall** 🟢 | `privacy.rs`, `Cleared<T>` type-sealed | 17 tests | **LOG — enforcing in production**: ledger holds real denials, e.g. `BODY_JOINT_POSITIONS / DERIVED_ONLY / LOCAL_MEMORY / DENY` ×21,972 | Helper stderr not routed through it | — | — | *frozen* | Re-check after any new destination |
| **Local event memory** 🟢 | `store.rs`, SQLite WAL, `secure_delete` | yes (real SQLite) | **LOG** — 61 MB, 8,900+ events, 5,000 snapshots, ledger | No integrity check/recovery; snapshot churn while paused (F12) | — | firewall | *frozen* | `PRAGMA integrity_check` on launch |
| Kill switch | latch file + terminate helpers | 22 tests | **Lantern build only** (1 recovery in log) | Never re-verified on `KUE.app`; in-flight executor survives ≤90 s | L3 to recover | — | F Live verification | Kill → relaunch → recover on `KUE.app` |
| Pause | `apply_pause` + sensing pause | yes | **LOG** — 29 pauses / 18 resumes | Resume needs no authorization (S5); **KUE has been paused since 2026-09-17 11:26 and nobody noticed** | L0 | — | F Live verification | Owner-visible "still paused" state |
| Safety refusal screen | `safety.rs` word lists | yes | never (no refusal recorded) | Rewording passes to the model (which has no authority) | — | — | D Routing | Live refusal of "grant yourself LEVEL_4" |
| Model boundary | `model.rs` `Admitted` | yes | **LOG** — 33 `LOCAL_MODEL` clearances, 5 kinds withheld per prompt | Under-claims not corrected (F6) | — | firewall | D Routing | Observe a correction firing |
| Capability registry | `capabilities.rs`, 41 rows | yes + doc-agreement | n/a | Proof dates predate later code | — | — | *one owner, end of phase* | Re-date proofs per phase |

### Language, voice, action

| Capability | Implementation | Automated test | Live verified | Current failure | SEC | Depends on | Owner | Next validation |
|---|---|---|---|---|---|---|---|---|
| **On-device model answers** 🟠 | `lantern-mind`, FoundationModels | boundary only | **LOG** — 31 answers | **ORANGE.** p50 **19.9 s**, max **107.2 s**; prefill starves Vision → drives the identity failure | L2 | firewall, router | D Routing | p50 time-to-first-text, before/after |
| Push-to-talk listening | `Voice.swift`, SpeechAnalyzer | audio-file tests | **LOG** — 49 sessions, 42 ended on silence | Release on lock never observed | L0 | TCC mic | G Hands-free | — |
| Spoken request → action | `handle_spoken_request` | yes | **LOG** — 7 of 11 actions succeeded (all 2026-09-14/15, **Lantern build**) | 3 failures were app-name resolution | L1/L2 | identity | G Hands-free | Repeat on `KUE.app` |
| **Hands-free wake** 🟠 | `Wake.swift` + `voice/wake.rs` | audio files (105 cases claimed) | **0 events — never run in a room** | **ORANGE.** Off at every launch (not persisted); cannot confirm or cancel by voice (F8) | L0 to wake | mic | G Hands-free | 20 real-room invocations |
| Voice output | `kue-voice` + speech gate | yes | **LOG** — 34 spoken outputs | No acoustic echo cancellation | — | firewall | G Hands-free | Own-voice test in a room |
| Open app / document / folder | KueAct verbs + read-back verify | 76 transaction tests (executor stubbed) | **LOG** — Lantern build only | Never exercised from `KUE.app` | L1/L2 | executor | F Live verification | One of each, from the window |
| **Storage inspection** 🟢 | `storage.rs`, 4 roots, depth 4 | yes | **LIVE today** — 820 findings, 5.0 GB, 27 k files, 6.2 s | Duplicates by name+size only | L1 | — | *frozen* | Same scan from the window |
| Storage cleanup (Trash/restore) | KueAct `trash`/`untrash` | yes + live test | **LIVE today** — round trip on KUE's own files passed | Window path (select → Touch ID → move) never run | L3 | executor, Touch ID | F Live verification | Full flow from the sheet |
| Goals / plans | `goal.rs`, 5 fixed templates | 12+ tests | never | Templates, not planning | varies | intent | H Planning (later) | One goal live |
| Arithmetic | `calculate.rs` exact fractions | yes | never | — | L0 | — | D Routing | "17% of 840" answered without the model |

### Not implemented (GRAY)

Claude/cloud model · web research · typing, clicking, reading other apps ·
calendar (a WIP branch, never built) · personal memory with recall · proactive
behaviour · speaker identity · face+voice fusion · onboarding · Swift unit tests
· Developer ID signing, hardened runtime, sandbox, entitlements (verified today:
`codesign -dvvv` → `flags=0x2(adhoc)`, `TeamIdentifier=not set`, **no
entitlements**).

## Failures found by this session that the forensic audit did not have

| # | Finding | Evidence |
|---|---|---|
| **N1** | **KUE has been paused since 2026-09-17 11:26 and is still running now.** 5,000 context snapshots written between 09-18 13:05 and 09-19 23:38, every one `NOT_OBSERVING / PAUSED`, zero events | `context_snapshots` + `pgrep` (pid 50407) |
| **N2** | A paused KUE still writes ~8,640 snapshots a day into a 61 MB database. Nothing tells the owner it is paused after a restart | same |
| **N3** | The live storage test has rotted: it asserts `said.cannot.contains("cannot move or delete")` while the sentence now reads "KUE never deletes anything…". `#[ignore]` tests are never run by anything, so this went unnoticed | `cargo test -p lantern storage_live -- --ignored` today, `lib.rs:1826` |
| **N4** | The macOS-only shell suite **passes** (10 tests) — the forensic audit could not run it at all. The three other live tests (Trash round trip, on-device model, cleared-context answer) also pass today | `./scripts/test-kue.sh --live` |
| **N5** | Model latency is worse than documented at the tail: **107.2 s** recorded, not 75 s | `ModelInteraction` rows |
| **N6** | The privacy firewall is demonstrably enforcing in production, not just in tests: 21,972 denials of one kind alone | `privacy_ledger` |

## What would change these rows

1. A 30-minute seated session with the camera on and five spoken questions,
   then the four queries in this document. That converts the identity rows from
   *measured broken* to *measured against a target*.
2. One run of each action **from the window on `KUE.app`**, which moves eight
   YELLOW rows to GREEN or RED.
3. Turning hands-free on once, in a real room.

Nothing else in this document can be promoted without the owner at the Mac.
