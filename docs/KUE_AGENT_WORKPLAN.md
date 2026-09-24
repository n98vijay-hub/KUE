# KUE agent workplan

2026-09-17. How KUE's work is split so parallel agents do not collide, and
when each piece may start. Only the integration role declares a milestone
complete, against the completion standard in §6.

## 1. What experience says

- 2026-09-16: five background agents started at once all stopped at the
  owner's usage limit within minutes and committed nothing (one left
  uncommitted Swift). One focused research agent, started later with its
  sources handed to it, finished and its record was merged.
- So: **at most two or three agents at a time**, each with one deliverable,
  its sources or files named, research before code, and a commit before it
  reports. The integration role (this session) does work that needs this Mac's
  event log, live runs or the owner.

## 2. Branches

| Branch | Purpose |
|---|---|
| `main` | Untouched. Its working tree is the repository root the owner may use; fast-forwarding it is the owner's call. |
| `kue/runtime-safety` | **The integration line today** (everything since 2026-09-14). Proposed to become `develop` — a rename the owner approves, since worktrees point at it. |
| `kue/<area>` | One per agent slice, branched from the integration line at a named commit, merged back by the integration role. |
| `backup/*` | Kept. |

Existing: `kue/computer-use-research`, `kue/web-agent-research` (running
today); `kue/speaker-model-decision` (merged); `kue/calendar-helper` (unbuilt
WIP, kept). Empty agent branches are listed for deletion in
`KUE_CURRENT_STATE.md` §H — pending the owner.

## 3. Roles, ownership and first tasks

Ownership is by path. An agent edits only its paths; anything else is a request
to the integration role.

| # | Role | Owns | Shared (coordinate first) | First task | Starts when |
|---|---|---|---|---|---|
| 1 | System auditor | `docs/KUE_CURRENT_STATE.md`, `docs/KUE_MASTER_ARCHITECTURE.md` | registry proof | done 2026-09-17; re-audit each milestone | — |
| 2 | Identity & presence | `sensing/…/Camera.swift`, `Perception.swift`, `Probe.swift`, `core/src/authz.rs`, identity parts of `engine.rs` | `sensor.rs` ↔ `Protocol.swift` wire format; `config/lantern.toml` | E2b; aggregates-only stability harness; owner session; stale-reading fix | now (integration role, needs this Mac) |
| 3 | Performance | new `core/src/latency.rs`, timing hooks in `src-tauri/src/lib.rs` ask/propose paths, `mind/` | `lib.rs` | per-stage latency record + Diagnostics view; ≥10-run prompt-size and prewarm experiments | now |
| 4 | Voice | `sensing/…/Voice.swift`, `Wake.swift`, `core/src/voice/`, `src-tauri/src/speech.rs` | `lib.rs` wake loop | persist hands-free; Phase 1 live checklist with the owner; end-of-speech timing | after identity fix (a flapping session refuses spoken requests) |
| 5 | Computer agent | `act/`, new `core/src/computer/` | `actions.rs` allowlist, `transaction.rs` | research doc (running); then stage 1–2 slice | after its research is merged and the owner grants Accessibility |
| 6 | Web research | new `core/src/web/`, a new helper process if chosen | `privacy.rs` (new destination), `agent.rs` | research doc (running); then first slice | after research + the owner's policy decision and provider/key |
| 7 | Memory & proactivity | new `core/src/memory/`, `store.rs` schema | `privacy.rs` (new kinds), `surface.rs` | design note: memory kinds, retention, deletion, what may be remembered | after latency and identity; calendar needs the owner's grant |
| 8 | UX/UI | `src/` | `surface.rs` (new states only via core) | apply `KUE_UX_PRODUCT_SPEC.md` §2–5 to the existing projection | now, window-only changes; states that need core wait for their owners |
| 9 | Integration / release | `scripts/`, `docs/KUE_MASTER_STATUS.md`, `core/src/capabilities.rs`, merges | everything | this plan; merge, test, build, live-check, registry | always |

## 4. Integration protocol

1. Agent commits on its branch and reports in the §30 format: WHAT I
   RESEARCHED, FOUND, CHANGED, DID NOT CHANGE, WHY, FILES, TESTS, LIVE TESTS,
   PERFORMANCE, SECURITY TESTS, LIMITATIONS, DEPENDENCIES, NEXT STEP.
2. Integration reads the diff, then runs: `cargo test -p lantern-core`,
   `cargo check -p lantern --tests`, the targeted shell tests,
   `npx tsc --noEmit`, `npx vitest run`.
3. Merge into the integration line; rebuild `KUE.app` with
   `scripts/build-kue.sh`.
4. Live check on this Mac — by the owner where the camera, microphone, Touch ID
   or a permission is involved — recorded from KUE's event log, not memory.
5. Registry `proof` and `KUE_MASTER_STATUS.md` updated only from step 4.
6. Local tag at each milestone.

## 5. Order of work

**Wave 1 — now**
- Research: computer use, web agent (running as agents).
- Identity: E2b; stability harness (aggregates only); owner session.
- Latency: instrumentation; answer-by-rule for capability and status
  questions; prompt-size and prewarm measurements.
- Registry: `FAILED_LIVE` as a proof level; under-claim check.
- UX: window-only restyle toward the spec (no new states).

**Wave 2 — after the owner session**
- Stale-reading fix and, if established, the ambiguous-measurement fix; live
  acceptance run.
- Prompt and routing changes chosen from the latency numbers; live re-measure.
- Hands-free persisted and offered in onboarding; Phase 1 voice checklist live.

**Wave 3 — after owner decisions**
- Speaker identity (model decision + Phase 1).
- Computer agent stage 1–2 (Accessibility grant).
- Web agent first slice (policy + provider/key).
- Claude provider behind `model.rs` (key + policy).

**Wave 4**
- Personal memory → calendar (grant) → proactivity.
- Advanced automation; UX polish and onboarding complete; release engineering
  (`verify-kue.sh`, /Applications install, Developer ID signing).

## 6. Completion standard

A slice is complete only when all hold: architecture note; implementation;
unit and integration tests; security and failure tests; build; **real Mac test
with a recorded outcome**; registry and master status updated from that
outcome; the window shows it truthfully; documentation updated. Compiling,
passing tests, or a model describing it does not count.

## 7. Owner decisions waiting

| Decision | Unblocks |
|---|---|
| Sit for an identity measurement session (aggregates only, no images kept); say whether anyone else was in view at 10:44–10:47 on 2026-09-17 | identity fix |
| Run `powermetrics` for E2b (needs your password) | contention source |
| Speaker model: the ten decisions in `KUE_SPEAKER_MODEL_DECISION.md` | speaker identity |
| Claude: API key, and what context (if any) may leave this Mac | Claude provider |
| Web research: provider, key, and what queries may leave this Mac | web agent |
| Accessibility permission for KUE, when the computer agent is ready | computer agent |
| Calendar permission, when memory is ready | meetings |
| Rename `kue/runtime-safety` → `develop`; delete empty agent branches; fast-forward `main` | branch hygiene |
| Private GitHub repository | off-machine backup |
| Install `KUE.app` into /Applications | normal launch |
