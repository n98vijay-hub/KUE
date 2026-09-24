# KUE — personal computing architecture

**2026-09-20 · `main` = `kue/runtime-safety` = `5cbdbc0` · one canonical line**

The destination is a personal computing intelligence layer: it understands its
owner and their working context, communicates naturally, reasons about goals,
researches, plans, operates permitted tools, verifies results, remembers what
matters, adapts, and proactively helps — while remaining governed, private and
fail-safe.

This document maps what exists to that destination. Every claim is labelled by
evidence class, because the difference between *implemented*, *tested* and *live
verified* is the difference between a product and a demo.

| Class | Meaning |
|---|---|
| **LIVE** | Observed working on this Mac, with a record |
| **TESTED** | Automated tests pass; the physical world was replaced by a stub |
| **BUILT** | Code exists; neither of the above |
| **BROKEN** | Measured failing against its own requirement |
| **ABSENT** | Not implemented |

---

## 1 · Current architecture, as built

One Tauri 2 process (`lantern`, linking the Rust core) supervising five Swift
helpers over JSON lines. No network code exists anywhere in the product source.

```
 React window ──invoke()──▶ 47 Tauri commands ──▶ lantern-core ──▶ SQLite
      ▲                                              │  (events, snapshots,
      └──── Surface projection ◀────────────────────┘   ledger, perception
                                                        samples, stage timings)
 helpers: LanternSense (camera, Vision, mic, wake) · KueAct (one allowlisted
          OS verb + read-back) · kue-auth (Touch ID) · lantern-mind (Apple
          on-device model) · kue-voice (speech out)
```

**The request pipeline as it really runs:**

```
text / push-to-talk transcript
  → safety::screen        (word-list refusal)
  → intent::classify      (rule-based: action | goal | answer-by-rule | model)
  → authorization         (Operation → level, from the face session or Touch ID)
  → EITHER transaction::propose → confirm → execute → verify
     OR    router → firewall → model (a LEAF: it answers, it never acts)
```

The model is not in the action path at all — verified by tracing every command
and every `invoke`. That property is the foundation everything else is built on.

### What is LIVE (observed on this Mac, with a record)

Camera capture and Vision face detection · face tracking · enrollment · the
privacy firewall **enforcing in production** (21,972 denials of one kind) ·
local event memory (61 MB, 9,258 events) · pause (29 uses) · push-to-talk (49
sessions) · on-device answers (31, with times) · open app / document / folder
(Lantern-era build) · storage inspection (820 findings, 5.0 GB, measured today) ·
Trash round trip (through the executor) · voice output (34 sentences) ·
**perception observability** (893 samples today).

### What is BROKEN (measured)

| | Measurement |
|---|---|
| Identity matching | 460 access changes/hour with the owner present; 65.6 % of uncertainty is an *ambiguous* measurement, 16.9 % lateness, **0 % contradiction** |
| Owner session | Inherits the above |
| Model latency | Prefill 13,893 ms, generation 3,353 ms — **81 % is reading the prompt**; the prompt is 2,741 chars, **48 % of it a 41-row capability dump sent on every question** |
| Perception under model load | 4 of 4 model runs pushed the measurement age past the 3.0 s staleness limit (max 3,417 ms; idle worst 268 ms) |
| Hands-free | Implemented, **0 live wakes ever**, off at every launch, cannot confirm or cancel by voice |
| Pause persistence | 8,470 identical snapshots/day; ~14 hours of pause evicts KUE's entire observed history |

### What is TESTED but never LIVE

Goals and plans (5 fixed templates) · arithmetic · the safety boundary on real
speech · the storage flow *from the window* · Touch ID completing an action ·
kill on `KUE.app` · quit/switch app, URLs, notifications, files in `~/KUE` ·
the echo guard · the Diagnostics panel on screen.

### What is ABSENT

Cloud/Claude reasoning · web research · typing, clicking or reading other apps ·
calendar · personal memory with recall · proactivity · speaker identity ·
onboarding · Swift unit tests · Developer ID, hardened runtime, sandbox,
entitlements.

---

## 2 · Target architecture

Fourteen domains, each with inputs, outputs, authority and tests. **Extracted
incrementally from what exists — never rewritten wholesale.**

| Domain | Owns | Today |
|---|---|---|
| **KUE Sense** | Camera, Vision, microphone, HID idle, frontmost app, pipeline health | `sensing/` + `engine.rs` (perception half) — LIVE |
| **KUE Voice** | Microphone lifecycle, VAD, transcription, wake, directedness, speech out | `Wake.swift`, `Voice.swift`, `voice/` — wake never used live |
| **KUE Identity** | Measurement states, descriptors, temporal identity, fusion | `engine.rs` (identity half) + `measurement.rs` — BROKEN |
| **KUE Context** | Observation / inference / prediction / unknown, with provenance | `context.rs`, `evidence.rs` — LIVE, no correlation over time |
| **KUE Evidence** | Verified facts, tool results, confidence, contradiction | `evidence.rs` + `ActionRecord` — **no VerifiedFact type yet** |
| **KUE Memory** | Projects, decisions, preferences, corrections, commitments | events + snapshots only — no recall |
| **KUE Reason** | Model routing, prompts, structured output, grounding | `model.rs`, `router.rs` — one local model, leaf-only |
| **KUE Plan** | Goal → plan → step → observe → verify → adapt | `goal.rs` — 5 fixed templates |
| **KUE Policy** | Authorization levels, autonomy levels, interruption policy | `authz.rs` — levels only; autonomy scattered |
| **KUE Tools** | Declared capabilities with schemas, executors, verifiers, rollback | `capabilities.rs` — prose rows, not declarations |
| **KUE Act** | Executing one allowlisted operation | `transaction.rs`, `KueAct` — LIVE |
| **KUE Verify** | Read-back, `UNKNOWN_RESULT`, never "done" unassured | `transaction.rs` — LIVE |
| **KUE Privacy** | Classification, clearance, ledger, retention | `privacy.rs` — LIVE and enforcing |
| **KUE Runtime** | One authoritative state machine, health, recovery | scattered across `lib.rs` — **absent as a thing** |

### Current → target, per domain

| Domain | Gap | Move |
|---|---|---|
| Sense | Health exists only for perception | Extend the heartbeat pattern to microphone and executor |
| Voice | Button-driven; wake is a phrase match | Ambient VAD → directedness decision → intent (§ voice below) |
| Identity | Cannot say *why* a measurement was ambiguous | Descriptor-margin forensics, then a temporal policy chosen from data |
| Context | No correlation, no "what the owner is trying to do" | Add project/goal context after memory exists |
| Evidence | A verified action is not a first-class fact the model must respect | **`VerifiedFact` type + grounding rule** (high leverage, small) |
| Memory | Absent | Governed categories with source, confidence, retention |
| Reason | 685 tokens of prompt for every question, half of it a capability dump | Prompt diet, fast-path routing, structured output |
| Plan | Templates | Dynamic plan with replan/abort, after tools are declarations |
| Policy | Autonomy implicit in each action's risk | One autonomy policy: observe / suggest / safe-automate / confirm / strong-auth |
| Tools | Prose registry | Declarations with input/output schema, executor, verifier, rollback, health |
| Act | Fine | Extend only through declared tools |
| Verify | Fine | Feed results into Evidence as verified facts |
| Privacy | Fine | Add retention policy as data grows |
| Runtime | Absent | One state machine the window, voice, logs and automation all read |

---

## 3 · The sub-architectures

### Voice — from buttons to ambient

```
microphone → VAD → ephemeral audio → local transcription
          → speaker identity (ABSENT) → DIRECTEDNESS → intent → …
```

Directedness cannot rest on a wake word alone. Permitted signals: wake state,
speaker identity (when it exists), conversation state (did KUE just speak?),
semantic form (imperative to a machine vs. talk about a person), recent
interaction, and addressing cues. Every one of these is evidence with a
confidence, and an uncertain directedness decision means **do nothing**, not
guess. Raw audio is never retained.

### Identity — evidence, not classification

The measured failure is ambiguity, not contradiction. The architecture must
therefore record **why** a measurement was ambiguous — which descriptor, how far
from which boundary — before any threshold moves. Then: continuous evidence →
temporal state → multi-factor fusion → authorization, where face is one factor,
voice is at most corroboration, and Touch ID remains the only strong factor.
`CONFLICT / UNKNOWN / UNCERTAIN → NOT AUTHORIZED` is invariant.

### Evidence and model grounding — the missing spine

Today a tool result lives in an `ActionRecord` and the model's context carries a
one-line event. Nothing stops the model describing a verified action as
uncertain, and that has been observed. The target:

```
SENSOR → EVIDENCE → TOOL RESULT → VERIFICATION → VERIFIED_FACT
       → sanitized model context → reasoning → response
```

`VERIFIED_FACT` is a type, carried into the prompt as fact, and the output
validator rejects an answer that contradicts one. The model may explain facts
and reason from them; it may never manufacture or downgrade them.

### Knowledge types

`OBSERVATION` · `INFERENCE` · `VERIFIED_FACT` · `MEMORY` · `PREDICTION` ·
`UNKNOWN` · `UNCERTAIN`. The context object already separates observation,
inference and unknown. Verified fact, memory and prediction are missing and must
never be merged into inference when they arrive.

### Intent — fast path and deep path

| Path | For | Budget |
|---|---|---|
| **Fast** | Arithmetic, capability and status questions, storage status, known app/file operations, "can you…" | **< 300 ms, no model** |
| **Deep** | Ambiguous intent, analysis, planning, research, multi-step goals | Model, with a prompt sized for the job |

A request KUE cannot do must be refused **on the fast path**, by rule, in
milliseconds — not after 30 seconds of model time, which is what happens today.

### Tools and governance

Every capability becomes a declaration: id, input schema, output schema,
authorization level, privacy class, risk, preconditions, executor, verifier,
rollback, availability, health, version. The model chooses among declared tools;
the governed runtime executes them. Every consequential operation passes
intent → privacy → policy → authorization → risk → preconditions → execution →
observation → verification → result. Unknown anything → DENY.

### Autonomy levels (centralised, not per-tool)

`OBSERVE` · `SUGGEST` · `SAFE_AUTOMATION` (safe and reversible) · `CONFIRM`
(consequential) · `STRONG_AUTH` (high risk). One policy object decides, so the
answer to "will KUE ask me first?" is the same everywhere.

### Memory, web research, computer use, proactivity

All four are designed against the same rule: they are **tools under governance**,
not new authorities. Web content is untrusted data that can never grant, change
policy, or execute. Computer use is a declared primitive set with observation
and verification per step. Memory never overrides security policy. Proactivity
requires importance, confidence, urgency, an interruption policy and quiet
hours. None of them starts before the layer beneath it is reliable — see
`KUE_DEPENDENCY_GRAPH.md`.

### Privacy, authorization, recovery, performance, testing

Unchanged in principle and already the strongest part of the system: classify →
policy → sanitize → authorize → send; unknown classification denies; the kill
switch overrides everything and does not auto-restart. What is missing is
**subsystem health and recovery** (HEALTHY / DEGRADED / FAILED / RECOVERING) and
a **latency budget** per stage — both now partly measurable thanks to the
telemetry landed on 2026-09-20. Testing stays three-layered: unit, scenario
replay, and live verification on this Mac, with live never inferred from tests.

---

## 4 · What must not be touched

The model's position as a leaf · `Cleared<T>` and the firewall's type discipline ·
kill latch semantics · fail-closed identity rules (second face, measured
conflict, new track) · verification downgrading unverified success to
`UNKNOWN_RESULT` · `config/lantern.toml` as the only home of thresholds · the
bundle identifier, helper identifiers and data folder (TCC grants and the kill
latch live there).

## 5 · What must be refactored, in order

1. **Extract identity** from `engine.rs` (2,310 lines) with its measurement
   types beside it.
2. **Split `lib.rs`** (1,933 lines: 47 commands, five lifecycles, the pump, the
   wake loop) by concern — the precondition for two agents ever working in the
   shell at once.
3. **One request parser** (rules currently live in four files).
4. **Generate the TypeScript types** from the Rust surface instead of mirroring
   them by hand.
5. **Registry → tool declarations**, once 1–3 are done.
