# KUE engineering reconnaissance

**2026-09-19 · branch `kue/runtime-safety` @ `1377f13` · source byte-identical to the audited tip `b893684`**

Reconciles the forensic audit of 2026-09-19 against the repository, the tests,
the running application and **KUE's own event log** — which the audit was denied
by the platform and which this session could read (read-only). Where the two
disagree, the log wins and the disagreement is stated.

No functional code was changed while producing this. Three documentation
commits were made: the seven at-risk untracked analyses, and the merge of three
documentation lines into one.

---

## 1 · Current architecture map

One Tauri 2 process (`lantern`, linking the Rust core `lantern-core`) supervising
five Swift helpers over JSON lines on stdin/stdout. No network code exists
anywhere in the product source.

```
 ┌──────────────── KUE.app (one process) ────────────────┐
 │  React window ──invoke()──▶ 46 Tauri commands         │     ┌─ LanternSense.app ── camera, Vision, mic,
 │        ▲                        │                     │     │   enrollment, frontmost app, idle, wake
 │        │  Surface projection    ▼                     │◀───▶┤
 │        └──────────────── lantern-core ────────────┐   │     ├─ KueAct.app ─── one allowlisted OS verb + read-back
 │          engine · authz · privacy · intent ·      │   │     ├─ kue-auth ───── Touch ID / password
 │          transaction · goal · storage · surface   │   │     ├─ lantern-mind ─ Apple FoundationModels (on-device)
 │          capabilities · model · router · store    │   │     └─ kue-voice ──── AVSpeechSynthesizer
 │                          │                        │   │
 │                          ▼  SQLite (WAL, secure_delete)     ~/Library/Application Support/Lantern/
 └───────────────────────────────────────────────────┘         lantern.sqlite3 · enrollment.json · KILLED latch
```

**The real request pipeline** (verified by reading every command and every
`invoke`): text or transcript → `safety::screen` → `intent::classify` →
authorization (`Operation` → level, from the face session or Touch ID) → either
`transaction::propose → confirm → execute → verify` **or** the model as a *leaf*
for answering questions. The model is not in the action path at all. That is the
single best property of this codebase and it is intact.

**Where the layers actually live:** identity, evidence, wake state and resource
measurement are all inside `engine.rs` (2,310 lines); the command table, five
process lifecycles, the pump and the wake loop are all inside `src-tauri/src/lib.rs`
(1,933 lines). Those two files are the system's real coupling.

## 2 · Current capability matrix

`docs/KUE_CURRENT_STATE.md` (rewritten today). Summary: **3 GREEN** — the
privacy firewall (enforcing in production), local event memory, and storage
inspection (measured live today) — against **~20 YELLOW**, **4 ORANGE**,
**3 RED** and **14 GRAY**. Test counts measured today on this branch:
**388 core**, **10 macOS shell** (+6 opt-in live), **21 window**, `tsc` clean.

The four numbers that decide the engineering order, all from KUE's own log:

| | |
|---|---|
| Access-level changes in the worst hour | **830** (and 763, 755 in others) |
| Share of access changes less than 1 second apart | **51 %** (2,372 of 4,653) |
| On-device answer time | n=31 · p50 **19.9 s** · p90 **31.4 s** · max **107.2 s** |
| Hands-free wakes, ever recorded | **0** |

## 3 · Current failure matrix

| # | Failure | Severity | Evidence (first-hand unless noted) | Fails safe? |
|---|---|---|---|---|
| F1 | **Identity flapping** — the owner is dropped to LEVEL_0 repeatedly while seated | **Critical** | 830 access changes/hour; `IDENTITY_UNCERTAIN at LEVEL_0` is the most common state (2,095 rows) | Yes (denies) |
| F2 | **Model latency** 20–35 s typical | **High** | 31 measured answers, p50 19.9 s, max 107.2 s | n/a |
| F3 | **Vision starved during model prefill** → causes F1 | **High** | Audit's stall timings 3–6.3 s vs a 3.0 s staleness limit (code read today) | n/a |
| F4 | Requests refused while identity settles | Medium | 2 `AuthorizationDenied` rows for `ASK_MODEL_WITH_PERSONAL_CONTEXT`, owner-initiated | Yes |
| F5 | False multiple-face detections | Medium | 38 `MULTIPLE_PEOPLE` access rows, 7 identity rows | Yes (locks) |
| F6 | Model under-claims capabilities | Medium | Only over-claims are corrected (`conversation.rs:164`) | n/a |
| F7 | Hands-free off at every launch, setting not persisted | Medium | `voice/mod.rs:184`; no `wake_enabled` in `lantern.toml` | n/a |
| F8 | Hands-free cannot confirm or cancel ("Computer, yes") | Medium | `reference::interpret` has one caller, the window command | n/a |
| F9 | `main` is not KUE — 74 commits behind, pre-rename Lantern | High (process) | `git rev-list --left-right --count` | n/a |
| F10 | Ad-hoc signing resets TCC grants on some rebuilds | Low | `codesign -dvvv` → `flags=0x2(adhoc)` | n/a |
| **N1** | **KUE has been paused since 2026-09-17 11:26 and is still running (pid 50407).** 5,000 snapshots since, every one `NOT_OBSERVING / PAUSED`, zero events | **High** | `context_snapshots`, `pgrep` | Yes, but silently |
| **N2** | A paused KUE writes ~8,640 snapshots/day into a 61 MB database | Medium | same | n/a |
| **N3** | The live storage test asserts a sentence that no longer exists; `#[ignore]` tests are run by nothing | Medium | reproduced today: `lib.rs:1826` panics on `said.cannot` | n/a |
| **N4** | Seven analyses existed only inside Claude worktrees (fixed today) | Was critical | `git status` before the commit | n/a |

**Not broken, verified today:** the macOS shell suite (10 tests) passes — the
audit could not run it; the Trash round trip, the on-device model call and the
cleared-context answer pass live; 388 core tests and 21 window tests pass.

## 4 · Identity failure analysis

**Mechanism, read in the code today** (`core/src/engine.rs:1194-1250`):

```rust
if !self.perception_is_fresh(now) { return Immediate(IdentityState::IdentityUncertain); }
```

`perception_is_fresh` = last frame within `camera.observation_stale_seconds` =
**3.0 s**. `Immediate` bypasses the carry logic entirely — unlike
`Unmeasurable(...)`, which *is* carried for 2 s. So:

1. The on-device model reads its prompt; Vision stalls 3–6 s (audit measurement,
   cause not yet established: CPU pinning and `taskpolicy -b` did not help).
2. At 3.0 s the core cannot tell *delayed* from *meaningless*, and returns
   `IdentityUncertain` immediately.
3. `AccessSession` demotes LEVEL_2 → LEVEL_0.
4. The owner's next spoken question needs LEVEL_2 and is refused.

**This is a design gap, not a tuning error.** The system has no state for
"measurement delayed but the sensor is alive". Raising the threshold would trade
the security invariant (stale evidence must not authorize) for comfort. The fix
direction is observability first: a sensing heartbeat that distinguishes *alive
and late* from *gone*, then a bounded carry of an **earned** match across a
delay on the same track — never a promotion from uncertainty.

**Second contributor:** the in-between measurement. `accept_ratio` 1.15,
`reject_ratio` 3.0; anything between is `Measured(IdentityUncertain)`, which
demotes at once and is treated as contrary evidence. The FeaturePrint descriptor
was measured drifting to 4.5× the owner's spread under lighting change alone.

**Never measured, at any point:** the reject side.
No stranger has ever been in front of it. FAR is unknown. There is no liveness
check, so a photo is an untested LEVEL_2 bypass (mitigated only by the attacker
needing the already-unlocked Mac).

## 5 · Latency failure analysis

From `ModelInteraction` rows written by KUE itself: **n=31, min 9.6 s, p50
19.9 s, p90 31.4 s, max 107.2 s**. Every answer also logs
"the prompt passed the privacy firewall with 5 kind(s) withheld", so the boundary
runs on the real path.

Three compounding causes, in order of what the evidence supports:

1. **Everything routes to the model that could not be answered by rule.** The
   capability line alone is ~820 characters of every prompt, and capability and
   status questions *have* a deterministic answer path that is not always used.
2. **Prefill dominates** and is what starves Vision (F3). Latency is therefore
   also the identity bug's power supply.
3. **No stage timing exists.** There is no instrumentation to say whether a
   given 30 s answer was queue, prefill or generation — which is why this
   analysis can measure the total and not the parts.

There is no measurement of how long a *deterministic* answer takes, because no
live arithmetic or capability request has ever been recorded.

## 6 · Security risk matrix

| ID | Severity | Finding | Verified today | Direction |
|---|---|---|---|---|
| S1 | **High** | Helpers located by path search and trusted by stdout. `kue-auth` is searched next to the executable, then **six parent directories**, then a **compile-time source path baked into the release binary** (`env!("CARGO_MANIFEST_DIR")`). Its stdout decides Touch ID success | read `src-tauri/src/auth.rs` | LocalAuthentication in-process, or verify the helper's signature; bundle-only paths in release |
| S2 | **High** | Face match alone grants LEVEL_2; no liveness; reject side never measured | code + log | Measure FAR; describe LEVEL_2 as convenience, not security |
| S3 | **Medium** | No sandbox, no hardened runtime, **no entitlements at all**, ad-hoc signature, no team id — "sensing has no network" is true only because no network code exists | `codesign -dvvv` + `codesign -d --entitlements -` today | Developer ID + hardened runtime + minimal entitlements per helper |
| S4 | Medium | Biometric-derived descriptors at rest in plaintext JSON (105 KB `enrollment.json`, plus probe samples of other people) | `ls -la` today | Keychain-wrapped key; retention policy for probe data |
| S5 | Medium | Resume-after-pause is LEVEL_0: anyone at the Mac can re-enable the camera | `authz.rs:153` | Deliberate decision; consider LEVEL_1+ |
| S6 | Medium | ~170 `.lock().unwrap()` in the shell; one panic while holding a lock poisons later IPC, including kill | count in `lib.rs` | Make the kill path poison-tolerant first |
| S7 | Low | Safety screen is word lists; rewording reaches the model (which has no authority) | `safety.rs` | Acceptable while the model stays authority-free |
| S8 | Low | Kill does not terminate an executor already running (≤ 90 s) | `transaction.rs` | Track the child PID |
| S9 | Low | KueAct's home-prefix check does not resolve parent-directory symlinks | `main.swift` | `resolvingSymlinksInPath()` |
| S10 | Info | Personal files (résumés, `Claude outputs/`) inside the repository folder; 11 GB of Claude worktrees | `git status` | Move out of the repository |
| S11 | Info | No threat model document exists | docs | Write one before Accessibility or web work |

**No bypass found** for: model → action, model → authorization, model → kill
recovery, window → arbitrary file trash, webview → remote content.

## 7 · Data and privacy flow

```
camera frames ─┐                        NEVER leave the sensing process (no encoder, no writer)
microphone ────┤ LanternSense ─ derived readings only ─▶ core ─ classify ─ decide(kind, destination)
HID idle ──────┘  (distances, boxes, poses, quality,        │        │
frontmost app ──   transcripts, app name — never pixels,    │        ├─▶ INTERFACE     (window)
file metadata ──   never audio, never keystrokes)           │        ├─▶ LOCAL_MEMORY  (SQLite)
typed text ─────                                            │        ├─▶ LOCAL_MODEL   (on-device)
                                                            │        ├─▶ EXTERNAL_MODEL → refused, none configured
                                                            │        └─▶ DIAGNOSTIC_LOG
                                                            ▼
                                                    privacy_ledger (kinds, decisions, counts — never content)
```

**Enforcement is real, not documentary** — the production ledger read today
contains, among others: `BODY_JOINT_POSITIONS / DERIVED_ONLY / LOCAL_MEMORY /
DENY` **21,972 times**, and the same kind denied to `LOCAL_MODEL` 33 times,
alongside `ALLOW` rows for `ACTIVITY_CONCLUSION`, `EVENT_RECORD`, `EVIDENCE`.
Clearance is enforced by a Rust type whose fields only the firewall module can
construct.

**Known gaps:** helper **stderr** is not routed through the firewall (sensing's
stderr is echoed to KUE's own stderr); `enrollment.json` and `identity_probes.json`
are plaintext; nothing constrains the Swift helpers by type — only by what they
are written to emit.

## 8 · Current dependency graph (core modules)

```
safety ── (nothing)                      evidence ── config
context ── authz config events evidence runtime sensor
authz ── context runtime
privacy ── actions context events model storage voice
router ── authz privacy            model ── capabilities config context conversation engine privacy router
actions ── authz folders privacy   storage ── actions folders privacy
goal ── actions authz              intent ── actions authz calculate capabilities context conversation goal safety task voice
capabilities ── actions context privacy
conversation ── capabilities context model privacy safety task voice
surface ── actions authz capabilities runtime voice
store ── config engine events evidence privacy
engine ── authz capabilities config context environment events evidence intent runtime safety sensor voice
transaction ── actions apps authz capabilities context engine folders goal intent privacy runtime safety storage task voice
```

Two observations. **`transaction` depends on fifteen modules including `engine`**,
and `model` depends on `engine` too — so the "leaf" boundary is respected in
data flow but not in module structure. **`safety` and `evidence` are leaves**,
which is why they are safe to touch in parallel.

## 9 · Technical debt map

| Rank | Debt | Cost it imposes |
|---|---|---|
| Critical | Identity decision coupled to model load (3.0 s staleness vs 3–6 s stalls) | The top live failure |
| Critical | `main` is not KUE (F9) | Any new work started from `main` rebuilds Lantern |
| High | `lib.rs` 1,933 lines (46 commands + 5 lifecycles + pump + wake loop) | Serialization point: two agents cannot work here |
| High | `engine.rs` 2,310 lines (perception + identity + evidence + wake + resources) | Same; and identity cannot be changed in isolation |
| High | Helper trust by path + stdout; ad-hoc signing (S1, S3) | Security is conventional, not enforced |
| High | Zero Swift tests for the five helpers | The most platform-risky code is unguarded |
| High | Hands-free path skips reference handling (F8) | The voice loop cannot be closed |
| Medium | Request parsing in four places (`actions::parse_command`, `task::plan`, `intent::classify`, `conversation` helpers) | One sentence, four rule sets |
| Medium | Hand-mirrored TypeScript types (`src/types.ts`, `surface.ts`) | Silent drift from the Rust |
| Medium | ~170 `.lock().unwrap()` in the shell (S6) | One panic cascades into IPC failure |
| Medium | Rename half-done: crate names, bundle id `dev.lantern.desktop`, data folder, `lantern.toml`, helper names | Confusion; the eventual id change resets TCC |
| Medium | Wake setting not persisted (F7) | Hands-free cannot be the default |
| Medium | No database integrity check or recovery path | Corruption is silent |
| Medium | `#[ignore]` live tests rot unnoticed (N3) | False confidence in the one suite that touches reality |
| Low | 12 empty agent branches, 11 GB of worktrees, `Claude outputs/` in the repo | Clutter, accidental-commit risk |

## 10 · What should be refactored (in this order)

1. **`engine.rs` → extract identity** into its own module with the sensing
   freshness/heartbeat types beside it. Behaviour-preserving, test-covered by
   the 85 existing scenario tests.
2. **`lib.rs` → split by concern**: commands / helper lifecycles / pump / wake
   loop. This is the precondition for any two agents working in the shell.
3. **One request parser.** Fold `actions::parse_command` and `task::plan` into
   `intent::classify` behind one entry point.
4. **Generate the TypeScript types** from the Rust `Surface`/`ContextObject`
   instead of mirroring them by hand.
5. **Capability registry → tool declarations** (R5), once 1–3 are done.

## 11 · What should not be touched

- **The privacy firewall's type discipline** (`Cleared<T>`, module-private
  fields, exhaustive `classify`). It is the strongest thing in the codebase and
  it is provably enforcing in production.
- **The model's position as a leaf.** No model output may ever reach
  `propose_command`, `confirm_action` or a grant.
- **The kill latch semantics** (file on disk, checked before start, recovery
  needs fresh LEVEL_3).
- **The fail-closed identity rules**: second face locks immediately; measured
  conflict demotes immediately; a carried claim never unlocks a locked session.
- **The action verification discipline**: unverified success is downgraded to
  `UNKNOWN_RESULT`.
- **`config/lantern.toml` as the single home of thresholds.** Do not reintroduce
  constants into Rust.
- The data folder name, bundle identifier and helper identifiers — until a
  deliberate migration with the owner (TCC grants and the kill latch live there).

## 12 · Which work can be parallelized

| Can run together | Why it is safe |
|---|---|
| Repository hygiene (R0) → then everything else | Git refs only |
| **Sensing heartbeat** (Swift, `Camera.swift`/`Protocol.swift`) and **core identity** (`engine.rs`, `authz.rs`) | Only after the wire format is frozen in a single commit by one agent — they share it |
| **Latency/routing** (`intent.rs`, `conversation.rs`, `model.rs`, `router.rs`, `mind.rs`) | Different files from identity; leaf-ward modules |
| **Security hardening** (`auth.rs`, `broker.rs`, `speech.rs`, `build-app.sh`, entitlements) | Different files; but no one else may run `build-app.sh` meanwhile |
| **Swift test harness** (new target) | New files only |
| Documentation and evidence collection | Read-only |

| Must be sequential | Why |
|---|---|
| Anything in `src-tauri/src/lib.rs` | One file, 46 commands, the pump and the wake loop |
| Anything in `engine.rs` | Identity, evidence, wake state and resources share it |
| Registry + `KUE_MASTER_STATUS.md` | A test holds them equal; one owner, at the end of a phase |
| Wire-format changes | Swift and Rust must land in one commit |

## 13 · Files that must have a single owner

`src-tauri/src/lib.rs` · `core/src/engine.rs` · `core/src/authz.rs` ·
`core/src/privacy.rs` · `core/src/transaction.rs` · `core/src/capabilities.rs`
(with `docs/KUE_MASTER_STATUS.md`) · `core/src/surface.rs` ·
`sensing/Sources/LanternSense/Protocol.swift` (with `core/src/sensor.rs`) ·
`config/lantern.toml` · `scripts/build-app.sh`.

## 14 · The next three engineering slices

**Slice 1 — Observability of perception delay** *(core + sensing, one owner for the wire format)*
Give the system the ability to distinguish *delayed* from *absent*: a sensing
heartbeat carrying frame-interval aggregates and liveness; new identity states
`MEASUREMENT_DELAYED` / `MEASUREMENT_STALE` distinct from `IDENTITY_UNCERTAIN`;
per-stage timing for the model path. **No threshold changes and no carry-rule
changes in this slice** — measurement first, so the fix is chosen from data.

**Slice 2 — Bounded carry across a proven delay** *(core identity only, after Slice 1 data)*
An **earned** match may survive a delay while the sensing process is proven
alive, the track is unchanged and the delay is bounded. Uncertainty still never
promotes; conflict, second face, new track, dead sensor and pause still demote
at once. Thresholds are only revisited if Slice 1 shows the in-between
measurement — not the delay — is the dominant cause.

**Slice 3 — Routing so ordinary requests never wait on the model** *(core, parallel to 1–2)*
Classify requests (deterministic / retrieval / reasoning); answer arithmetic,
capability and status questions by rule; remove the capability line from the
prompt; record stage timings. Latency is the identity bug's power supply, so
this slice is also an identity fix.

## 15 · Acceptance criteria

| Slice | Accepted when |
|---|---|
| 1 | From a 30-minute seated recording: every gap > 1 s between camera frames is attributed to a named cause; the core reports `MEASUREMENT_DELAYED` (not `IDENTITY_UNCERTAIN`) whenever the sensing process is alive and late; a killed sensing process still produces `MEASUREMENT_STALE` within one tick; the model path reports queue / prefill / generation separately for ten answers. **No change in access-level behaviour** — the histogram before and after is statistically the same |
| 2 | Same 30-minute protocol with five spoken questions: **< 10 access changes per hour** with no real contrary evidence; **zero** LEVEL_2 grants during a measured conflict, a second face, a new track or a dead sensor (asserted by replay tests); with a consenting second person, **zero false accepts** |
| 3 | "What is 17% of 840", "what can you do", "am I recognised" are answered **without any model call** (asserted in tests and observed in the log); p50 time-to-first-text for the ten most common requests recorded before and after; during a model answer the camera frame interval stays under the staleness limit |

Each slice also: 388+ core tests still pass; the macOS shell suite passes; no
security invariant weakened; a commit per completed slice.

## 16 · Live tests required from the owner

These cannot be performed without the owner at the Mac. Nothing below is marked
pass or fail until the owner runs it — **UNVERIFIED — OWNER ACTION REQUIRED**.

| # | Test | Why it needs the owner |
|---|---|---|
| L1 | **Resume KUE** (it has been paused since 2026-09-17) and sit in front of it for 30 minutes with the camera on, asking five spoken questions | A face and a room |
| L2 | Run one action of each kind from the window on `KUE.app`: open an app, open a document (confirm), open a folder | Confirmation click |
| L3 | Complete a **Touch ID prompt** for a Trash move started from the storage sheet | A fingerprint |
| L4 | Kill KUE, relaunch it, recover it | Physical gesture + Touch ID |
| L5 | Turn hands-free on, say "Computer, what's filling up my Mac", then "Computer, yes" to a pending action | A microphone and a room |
| L6 | Identity Check with a **consenting second person**, then have them sit alone in front of the camera | Another human being |
| L7 | `./scripts/test-kue.sh --live-all` once, start to finish | Opens Chrome and Finder and speaks aloud |

## 17 · Expected evidence for each test

| # | Evidence to capture |
|---|---|
| L1 | `SELECT strftime('%m-%d %H',ts,'unixepoch','localtime') h, COUNT(*) FROM events WHERE kind='AccessChanged' GROUP BY h` for that hour; the summary histogram; the `ModelInteraction` answer times |
| L2 | `Action` rows with `Succeeded` **dated after the `KUE.app` build**; the read-back in each record |
| L3 | An `Action` row for `MOVE_TO_TRASH` plus an OS-auth event; the file present in the Trash |
| L4 | `KILLED` latch present, then a `Recovered` event; camera and mic released (no helper processes) |
| L5 | The first non-zero wake event in the log's history; a `RequestUnderstood` row whose source is the wake path |
| L6 | Probe separation numbers; zero `MY_FACE_CONFIRMED` while the second person is alone in frame |
| L7 | The script's summary line, and the live tests' names and outcomes |

Paste the query output into `docs/evidence/` with the date. Rows dated after the
current build promote capability rows from YELLOW to GREEN — or to RED, which is
equally useful.

## 18 · Rollback plan

| Change | How it is undone |
|---|---|
| The three documentation commits made today (`9d2ee2f`, `0d820f2`, `1377f13`) | `git revert`; no source file was touched (`git diff b893684 HEAD -- ':!docs'` is empty) |
| Any slice | One slice per commit; `git revert <sha>` restores the previous behaviour. No slice may mix a behaviour change with a refactor |
| Wire-format change (sensing ↔ core) | Swift and Rust land in the same commit, so one revert covers both. The old build keeps working because helpers are versioned by the bundle they ship in |
| Threshold changes in `config/lantern.toml` | The file is data: restore the previous values and relaunch; no rebuild needed |
| A refactor that extracts a module | Behaviour-preserving by definition; the 388 core tests and the macOS shell suite are the gate. If either fails, revert rather than patch forward |
| Data-affecting changes (schema, enrollment format, data folder) | Not in these slices. When they come: back up `~/Library/Application Support/Lantern/` first (`scripts/backup-kue.sh`), and the kill latch moves before anything else |
| `main` fast-forward | `git branch -f main 43d7a84` restores it exactly; nothing is rewritten |

**Stop conditions honoured in this reconnaissance:** no branch deleted, no data
deleted, no biometric semantics changed, no authorization level changed, no
security check disabled, no cloud transmission introduced, no external provider
added, no Accessibility automation enabled, no bundle identity changed, no
persistent user data modified, no migration run.
