# KUE master architecture

As built on 2026-09-17, branch `kue/runtime-safety` at `0263e59`, and where it
has to go. Per-capability status and proof live in
[KUE_MASTER_STATUS.md](KUE_MASTER_STATUS.md) and `core/src/capabilities.rs`
(test-enforced to agree); this document is the map.

## 1. Processes and how they talk

```
                                 ┌─────────────────────────── KUE.app (one signed bundle) ────────────────────────────┐
  owner ─ keyboard/click ─────▶  │  React window (src/)  ◀── Tauri events: lantern://context, conversation, actions    │
                                 │     │ invoke (46 commands)                                                         │
                                 │     ▼                                                                              │
                                 │  lantern shell (src-tauri/, Rust)  — owns processes, timers, the pump              │
                                 │     │ in-process calls                                                             │
                                 │     ▼                                                                              │
                                 │  lantern-core (core/, Rust)  — every decision; no I/O of its own                   │
                                 │                                                                                    │
                                 │  child processes, each one JSON object per line over stdin/stdout:                 │
                                 │   LanternSense.app  camera · Vision · NSWorkspace · HID idle · microphone ·         │
                                 │                     SpeechAnalyzer · wake boundary · face enrollment               │
                                 │   lantern-mind      Apple FoundationModels (on-device), text in / text out         │
                                 │   kue-voice         AVSpeechSynthesizer                                            │
                                 │   KueAct.app        the Action Broker's executor, one request per run              │
                                 │   kue-auth          LocalAuthentication (Touch ID / password), one prompt per run  │
                                 └────────────────────────────────────────────────────────────────────────────────────┘
  ~/Library/Application Support/Lantern/   lantern.sqlite3 (events, snapshots, privacy ledger) · enrollment.json · KILLED latch · lantern.lock
```

No process makes a network request. Nothing is sent to any server.

## 2. The authority chain (what decides)

```
REQUEST (typed / push-to-talk / wake)
  → SAFETY BOUNDARY            core/src/safety.rs         refuses by rule, before anything else
  → INTENT                     core/src/intent.rs         what is wanted; never acts
  → AUTHORIZATION              core/src/authz.rs          identity session + levels 0–4 + OS auth; per step, when it runs
  → GOAL / STEPS               core/src/goal.rs           plan state machine; a plan carries no authority
  → TRANSACTION                core/src/transaction.rs    registry gate → firewall → authorize → confirm → execute → verify
  → ACTION BROKER              core/src/actions.rs        closed allowlist of ActionKinds, risk per kind
  → EXECUTOR                   act/ (KueAct)              does it, reads the result back
  → VERIFICATION               ActionRecord::finish       unverified success becomes UNKNOWN_RESULT
  → RECORD                     core/src/events.rs → store
```

A question that is not an action goes: safety → intent → authorization
(LEVEL_2) → model router (`router.rs`) → privacy firewall
(`clear_model_context`) → model boundary (`model.rs`) → on-device model →
answer checks → window/voice.

Kill switch (`core/src/runtime.rs`) sits outside the chain: it stops sensing,
memory writes, the model process, speech and pending actions, and survives a
relaunch. No model, automation or voice has a path to it.

## 3. The privacy chain

```
raw sensor (frames, audio)          stays inside LanternSense, never written or sent
  → derived measurements            face distances, pose, levels — to the core only
  → conclusions / summaries         identity state, activity, app name, counts
  → firewall decision per kind × destination   core/src/privacy.rs (policy v1, versioned code)
  → sealed Cleared<T>               the only type a destination accepts (memory, window, model, speech, action path)
  → ledger row                      every decision recorded
```

Destinations: Interface, LocalMemory, LocalModel, ExternalModel, DiagnosticLog
(speech is checked as Interface; action targets are a data kind, ACTION_TARGET,
bound for the Interface). Classes: PRIVATE, LOCAL_ONLY, DERIVED_ONLY,
USER_APPROVAL_REQUIRED (its approval flow is not implemented, so it denies
everywhere except the window), NEVER_STORE, NEVER_COLLECT; CLOUD_ALLOWED exists
in the policy but no data kind has it. Unknown kind = deny at compile time.

## 4. The core, module by module

| Area | Modules (lines) | Responsibility |
|---|---|---|
| Sensing input | `sensor.rs` (380), `environment.rs` (166) | wire format from LanternSense |
| Understanding presence | `engine.rs` (2,310), `evidence.rs` (224), `context.rs` (444) | readings → identity, activity, context object; deterministic confidence |
| Identity & authorization | `authz.rs` (761) | identity session, levels, OS-auth grants |
| Safety & runtime | `safety.rs` (331), `runtime.rs` (379) | refusal rules; running/paused/killed/recovering |
| Privacy | `privacy.rs` (822), `store.rs` (472), `pump.rs` (60), `events.rs` (239) | policy, clearance, memory, ledger |
| Requests | `intent.rs` (702), `task.rs` (166), `actions.rs` (962), `apps.rs` (187), `folders.rs` (270), `calculate.rs` (383) | classification, parsing, allowlist, resolution, arithmetic |
| Plans & execution | `goal.rs` (768), `transaction.rs` (1,575) | goals, steps, the transaction |
| Storage | `storage.rs` (858) | volume, inventory, findings, explanations |
| Models | `router.rs` (148), `model.rs` (300), `conversation.rs` (496) | which model, boundary, conversation + answer checks |
| Voice | `voice/` (8 files, 2,119) | wake policy, speech pipeline, narration, echo, references |
| Truth about KUE | `capabilities.rs` (1,442), `surface.rs` (903) | registry; what the window may show |
| Not yet | `agent.rs` (152) | web/computer stage order and `UntrustedText` only |

Shell: `lib.rs` (1,933 — 46 Tauri commands, process management, the pump),
`speech.rs`, `sensing.rs`, `mind.rs`, `broker.rs`, `auth.rs`.
Window: 13 components; `Diagnostics.tsx` holds the engineering views
(identity, access, evidence, environment, privacy, rail).
Tests: 388 core (lib 193, action transaction 76, engine 85, privacy 13,
environment 11, runtime 6, action privacy 4); shell has 16 tests, several live.

## 5. The product pipeline, layer by layer

The owner's pipeline RECOGNIZE → OBSERVE → IDENTIFY → CORRELATE → UNDERSTAND →
REMEMBER → REASON → PREDICT → ASSIST → ACT → VERIFY, against what exists:

| Layer | Exists | State |
|---|---|---|
| RECOGNIZE | camera, face detection, enrollment | live, but **identity flaps** (FAILED live) |
| OBSERVE | frontmost app, input idle, pose, scene, light | live |
| IDENTIFY | face identity session; no speaker identity | partly live; voice not built |
| CORRELATE | evidence + context object | live, single-moment; no cross-time correlation |
| UNDERSTAND | intent router (rules), on-device model | intent router ran live for 5 spoken questions, all classed as conversation; model live but 10–35 s |
| REMEMBER | event log + snapshots | live; **no personal memory** (nothing told to KUE is kept) |
| REASON | on-device model only | slow; no planning model; no external model |
| PREDICT | — | not implemented |
| ASSIST | answers, storage explanations | partly live |
| ACT | Action Broker: open/quit apps, links, documents, folders, Trash | partly live; **no in-app control** |
| VERIFY | read-back per action | live for the actions seen |

## 6. What is architecturally wrong or missing

1. **The on-device model shares a resource with face analysis**, and each
   question stops identity for the length of the model's prompt reading
   (`KUE_IDENTITY_STABILITY.md` §2). Any design that sends ordinary questions
   to that model will keep flapping identity.
2. **A stale reading is treated as immediate uncertainty**, while an
   unmeasurable frame is carried for 2 s. Sensing that is alive but delayed is
   indistinguishable from sensing that stopped.
3. **An ambiguous face measurement is treated like contrary evidence** (§3 of
   the identity doc). Not proven wrong yet; needs the harness.
4. **Three request parsers stacked:** `actions::parse_command` (single
   command) inside `task::plan` (multi-step) inside `intent::classify` (typed
   intent), plus `conversation.rs` helpers (`incomplete_command`,
   `is_capability_question`) that `intent.rs` calls back into. It works, but
   the rules for one request are spread across four files.
   `conversation::unsupported_command` is dead in production (only a test
   calls it).
5. **Capability questions still reach the model.** Only "what can you do?" is
   answered by rule; "can you open files?" goes to the model with an
   820-character capability list, and the model still denied a capability KUE
   has. Answers are checked for *over*claims, never for *under*claims.
6. **Hands-free listening is off on every launch** (not persisted) and has
   never run live; the window's primary input is still Speak and a text box.
7. **No latency instrumentation**: stage timings exist only as event
   timestamps.
8. **The shell `lib.rs` is 1,933 lines** holding commands, process lifecycles,
   the pump, the wake loop and tests — the part most likely to conflict when
   several people work at once.
9. **Names**: data folder, crate, sensing bundle and identifiers still say
   Lantern (deliberate — see `KUE_RENAME_PLAN.md`), which is confusing in
   logs and paths.
10. **Ad-hoc signing**: helpers could be replaced by anyone with write access to
    the bundle; camera permission can reset on rebuild.

## 7. Target architecture (additions, in dependency order)

```
                    ┌──────────── owner presence: face + (later) voice fusion ───────────┐
wake / typed ──▶ safety ──▶ intent (rules first; model only for ambiguity) ──▶ authorization per step
                                     │
                     ┌───────────────┼─────────────────────────────────┐
                     ▼               ▼                                 ▼
              answer by rule   reasoning provider              goal / plan (goal.rs)
              (registry,       (on-device, or Claude           │
               context,        behind model.rs, output =       ▼
               state)          text or a structured proposal   transaction → broker → executors:
                               that re-enters intent)            KueAct (apps, files) ·
                                                                 computer agent (Accessibility) ·
                                                                 web agent (search/fetch, UntrustedText)
                                                                 │
                                                                 ▼
                                                    verification → events → personal memory
                                                                 │
                                          calendar + time + memory ──▶ proactive decisions (with reason,
                                                                        evidence, cooldown) ──▶ owner question
```

Rules that do not change: the model is never the authority; web and screen
text is data (`UntrustedText`); every step is authorized when it runs; failed
verification stops; kill stops everything.
