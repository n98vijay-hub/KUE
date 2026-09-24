# KUE execution plan

**2026-09-20.** Slices, in dependency order. Each is the smallest change that
materially strengthens the architecture, and each carries its own acceptance
criteria, failure conditions and rollback. No slice begins before its
dependencies are met; no slice is called done on tests alone.

Format per slice: **objective · dependencies · research · files · agents ·
implementation · tests · live tests · acceptance · failure conditions ·
rollback · evidence.**

---

## S0 · Repository truth — ✅ DONE 2026-09-20

`main` = `kue/runtime-safety` = one canonical line; seven at-risk documents
committed; personal files moved out; cleanup report written; nothing deleted.
Evidence: `docs/KUE_REPO_CLEANUP.md`.

## S1 · Perception observability — ✅ DONE 2026-09-20

Eight measurement states, a 1 Hz sensing heartbeat, per-stage timings, the
model's phase on the same timeline, both records through the privacy firewall.
Identity decisions provably unchanged. Evidence:
`docs/evidence/KUE_FOUNDATION_OBSERVABILITY_2026-09-20.md`.

---

## S2 · Latency: the prompt diet and the fast path — ✅ DONE 2026-09-20, with a negative result

**Objective.** Ordinary requests answered in milliseconds without the model, and
the model's prompt cut to what the question needs. Also an identity fix: prefill
is what stalls perception.

**Dependencies.** None unmet. Needs no camera, no owner, no second person.

**Research.** Done and measured: the prompt is 2,741 chars (~685 tokens) —
instructions 1,176, **capability dump 1,319 (48 %)**, recent events 220.
Prefill 13,893 ms against 3,353 ms of generation.

**Files.** `core/src/privacy.rs` (prompt assembly), `core/src/capabilities.rs`
(a short capability line), `core/src/intent.rs` (fast-path rules),
`core/src/conversation.rs`, `core/examples/prompt_size.rs`.

**Agents.** One. `privacy.rs` and `capabilities.rs` are single-owner files.

**Implementation.** (a) Replace the 41-row dump with one short, stable sentence
of what KUE can and cannot do, derived from the registry; (b) route requests for
things KUE cannot do — reminders, calendar, email, web, typing in apps — to a
rule-based refusal that names the nearest thing KUE *can* do; (c) keep the
capability question itself on the rule path; (d) re-measure.

**Tests.** Prompt size asserted under a budget; every `NotImplemented` registry
row has a fast-path phrase; "put a reminder" answers by rule with no model call;
the refusal names an alternative; existing model-boundary tests unchanged.

**Live tests.** Ask KUE one capability question and one unsupported request from
the window; confirm zero `MODEL_*` spans for them. Ask one real question;
compare `MODEL_PREFILL` before and after.

**Acceptance.** Prompt ≤ 1,400 chars for an ordinary question; unsupported
requests answered in < 300 ms with no model call; measured prefill reduction
recorded from KUE's own telemetry.

**Failure conditions.** Prefill does not fall materially → the bottleneck is not
prompt size; publish that and investigate model init/prewarm instead. Refusals
become vague → revert; honesty outranks speed.

**Rollback.** One commit per part; `git revert`. The capability line is data, not
structure.

**Evidence.** `prompt_size` before/after; stage timings from the running app.

**Outcome, measured.**

| | |
|---|---|
| Reminders and calendar | reached the model, ~30 s → refused by rule in **~1 ms**, naming what KUE can do instead |
| Capability line in the prompt | 1,319 → **373 chars**; whole prompt 2,741 → **2,312** |
| **Did prefill fall?** | **No, not materially.** A/B against the real model, same question with and without the removed text: **mean 11.5 s short against 13.3 s long**, inside a 10–15 s per-run spread |
| A better lead, found by accident | Two model processes contending **doubles** the answer time: 22.3 s against 11.5 s |

So the failure condition written into this slice fired: **prompt size is not the
bottleneck.** The prompt diet stays (less data to a model is right on its own
terms, and the table was redundant with the rule path), but the latency work
moves to model session lifetime, prewarming and contention — not to trimming
more text. The A/B is now a repeatable test
(`model_latency_against_prompt_size`, `--lib`, ignored by default).

---

## S2a · Latency, take two: where the seconds actually go

**Objective.** Find what an 11.5-second answer to "what is the capital of
France?" is actually spent on, now that prompt size is excluded.

**Research first.** Whether `lantern-mind` holds one `LanguageModelSession` or
builds one per request; what Apple's FoundationModels costs at session
creation; whether prewarming exists; whether the guardrail pass is per-request;
what `powermetrics` shows during an answer (owner-run).

**Files.** `mind/Sources/LanternMind/main.swift`, `src-tauri/src/mind.rs`,
`core/src/telemetry.rs` (a first-token span, so prefill is measured inside KUE
rather than inferred).

**Acceptance.** The 11.5 s is attributed to named parts, each measured; and a
change is proposed only against the largest one.

**Failure condition.** If the time is inside Apple's model and not attributable,
say so — and the answer becomes routing (never call it) plus a second provider,
not micro-optimisation.

## S3 · Verified facts and model grounding

**Objective.** A verified tool result becomes a first-class fact the model must
respect: it may explain it, never contradict or downgrade it.

**Dependencies.** None unmet.

**Files.** `core/src/evidence.rs` (a `VerifiedFact` type), `transaction.rs`
(emit on verification), `privacy.rs` (carry into the prompt as fact),
`model.rs` (validator rejects contradiction), `conversation.rs`.

**Tests.** A verified Trash move cannot be described as unknown; an unverified
one cannot be described as done; `UNKNOWN_RESULT` survives the model unchanged.

**Live.** One action from the window, then ask KUE what it just did.

**Acceptance.** The model's answer about a verified action is consistent with
the record, or it is corrected before display, with a test that fails if the
validator is removed.

---

## S4 · Identity forensics: why a measurement is ambiguous

**Objective.** Record, per ambiguous measurement, *which* descriptor was
responsible and *how far* it sat from its boundary — derived numbers only.

**Dependencies.** S1 (done). Acceptance of any *fix* is owner-blocked; the
forensics are not.

**Files.** `core/src/measurement.rs` (sample fields), `engine.rs` (fill them),
`store.rs` (columns), `sensing/…/Protocol.swift` only if a new derived field is
needed (single commit, both sides).

**Tests.** Margins are derived, never raw descriptors; a sample carries no
distance that could reconstruct a face; ambiguity attribution matches the
classifier's own arithmetic.

**Live.** Ordinary use collects it; then the owner's seated run.

**Acceptance.** For a seated session, the share of ambiguity attributable to
each descriptor is reportable, with margins.

**Failure conditions.** If the margins show no dominant cause, say so — the next
step becomes enrollment quality or capture quality, not thresholds.

---

## S5/S6 · Declared tools, runtime pipeline, conversation — ✅ DONE 2026-09-21

See `docs/KUE_RUNTIME_SPINE.md`. Declared tools enforced by the broker; one
transport-independent pipeline with legal transitions and ids; a conversation
that interprets confirmations, cancellations and corrections against what is
open; one front door for every input. The owner's clean-up exchange ran LIVE
through the real Trash (Touch ID stood in). Spoken to the running app:
UNVERIFIED — OWNER ACTION REQUIRED.

## S7 · Natural two-way interaction and governed plans — ✅ DONE 2026-09-22

See `docs/KUE_CONVERSATION_AND_PLANS.md`. Corrections across turns (exclude,
only, include all, go back, leave nothing out, by kind or file type); vague
changes asked about; questions about the work answered from runtime state;
undo; stop that stops only what can honestly be stopped; governed speech with
declared data kinds; the main window shows the conversation core keeps; a
first-class Plan (goals read as plans; proposals parsed strictly, validated
against the declared tools and the moment, never executed; additive,
risk-aware approval; preview). The owner's work and plan need recognition
(LEVEL_2); stop and cancel do not. Live in the running app: questions, stop
during a model answer, the storage plan on this Mac, a clarification.
**Not connected:** no model proposes plans. **Owner-blocked:** "do it" →
Touch ID on real files; spoken input.

**Next (S8, goal planning):** wire `validate_proposal` behind a model that
proposes from the declared tools only, with the preview as the owner's gate —
after the owner decides whether a plan-level yes may stand in for a MEDIUM
step's own confirm, and after S7-L1 has been re-run on the current build.

## S9 · Personal memory — ✅ DONE 2026-09-23

See `docs/KUE_MEMORY.md`. Six classes with a governed reason each, a lifecycle
where only the action pipeline reaches VERIFIED and only the owner's own words
reach CONFIRMED, contradiction that asks rather than overwrites, forgetting
that removes the words, retrieval by shared words (and memory before any
model), and a window surface with provenance. Live across two processes on
this Mac, with the real model, and on real files. **Not yet live:** driven
from the app's window, and across a real app restart.

## S10 · Memory in use — ✅ DONE 2026-09-24

See `docs/KUE_MEMORY.md` (S10). A preference stated once shapes the clean-up
plan proposed later, with the reason quoted back; a changed preference changes
the next plan; an unrelated one stays out; two that disagree are asked about.
Approval, authorization and verification are untouched, and tested to be.
Live on real files. **Not yet live:** a model-proposed plan shaped by a
preference, and any of it driven from the app's window.

## S5 (original) · Runtime state machine

**Objective.** One authoritative state the window, voice, logs and automation
all read: STARTING · LOCKED · READY · LISTENING · UNDERSTANDING · THINKING ·
PLANNING · ACTING · VERIFYING · WAITING_FOR_USER · UNCERTAIN · PAUSED · KILLED ·
RECOVERING · ERROR.

**Dependencies.** S2 and S3 (so the states describe something real).

**Files.** New `core/src/runtime_state.rs`; `surface.rs`; `lib.rs` (split by
concern first — precondition for parallel work).

**Acceptance.** Every user-visible state traces to one enum; no React component
computes a state; a scenario replay drives all fifteen.

---

## S6 · Governed tool declarations

Registry rows become declarations (input/output schema, authorization, privacy,
risk, preconditions, executor, verifier, rollback, health, version). The broker
refuses anything undeclared. **Dependencies:** S3, S5.

## S7 · Ambient voice

VAD → ephemeral audio → local transcription → directedness decision → intent.
Wake stays as explicit invocation and privacy mode. **Dependencies:** identity
stability, S2, S5. **Owner-blocked** for acceptance (a real room).

## S8 · Dynamic planning
## S9 · Personal memory
## S10 · Speaker identity (owner decision pending)
## S11 · Web research
## S12 · Computer use via Accessibility (last, with a threat model)
## S13 · Calendar and work context
## S14 · Proactive intelligence

Each keeps the same twelve-field shape and starts only when
`KUE_DEPENDENCY_GRAPH.md` shows its edges are satisfied.

---

## Running alongside — security hardening

In-process LocalAuthentication or signature-verified helpers; helpers loaded
only from the bundle (today `kue-auth` is found by walking six parent
directories, then a compile-time source path baked into the release binary);
Developer ID + hardened runtime + entitlements (today: ad-hoc, no team id, no
entitlements); encrypt enrollment and probe descriptors; the threat model.
Different files from S2–S5, so it may run in parallel — but no one else may run
`build-app.sh` while it does.

## Owner actions outstanding

| # | Action | Unblocks |
|---|---|---|
| L1 | 30 minutes seated, five questions, then `./scripts/evidence-kue.sh 1800` | Identity acceptance criteria |
| L2 | One action of each kind from the window on this build | Eight capability rows |
| L3 | Touch ID completing a Trash move from the storage sheet | The storage flow end to end |
| L4 | Kill → relaunch → recover on `KUE.app` | Kill verification on the current build |
| L6 | Identity Check with a consenting second person | Any threshold change, ever |
| L7 | `powermetrics` during a model answer | Whether the stall is contention |
