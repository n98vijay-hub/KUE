# KUE agent architecture

The shape KUE is being built into, and exactly how much of it exists. This is a
map, not a claim: most of it is **PLANNED**, and says so.

---

## The loop

    RECEIVE → UNDERSTAND → GATHER CONTEXT → PLAN → SELECT TOOLS
      → CHECK PRIVACY → CHECK AUTHORIZATION → EXECUTE → OBSERVE → VERIFY
      → EXPLAIN → REMEMBER

| Step | Today | Status |
|---|---|---|
| RECEIVE | Typed, push-to-talk, or the invocation (`Wake.swift`) | IMPLEMENTED |
| UNDERSTAND | The intent router (`intent.rs`): typed intents by rule; the on-device model only for questions | PARTIAL — rule-based, see `KUE_INTENT_AND_GOALS.md` |
| GATHER CONTEXT | Identity, presence, frontmost app, idle, conditions, storage | PARTIAL |
| PLAN | Goals with explicit step states (`goal.rs`): fixed blueprints and clause splitting into ≤ 4 steps | PARTIAL — no model-proposed plans |
| SELECT TOOLS | The action allowlist, via the capability registry | IMPLEMENTED |
| CHECK PRIVACY | The firewall, on every target and every prompt | IMPLEMENTED |
| CHECK AUTHORIZATION | The access session, by risk | IMPLEMENTED |
| EXECUTE | The Action Broker → `KueAct` | IMPLEMENTED |
| OBSERVE + VERIFY | Read back after every action; unverified = UNKNOWN_RESULT | IMPLEMENTED |
| EXPLAIN | Narration from recorded state; evidence beside every finding | IMPLEMENTED |
| REMEMBER | Events, snapshots, the privacy ledger. No preferences | PARTIAL |

## Where the authority is, and is not

    ┌──────────────┐
    │    MODEL     │  interprets, plans, explains, chooses among PERMITTED tools
    └──────┬───────┘
           │ proposes
    ┌──────▼──────────────────────────────────────────────────┐
    │  IDENTITY · PRIVACY · AUTHORIZATION · BROKER · VERIFY    │  decides
    └──────┬──────────────────────────────────────────────────┘
           │ executes
    ┌──────▼───────┐
    │    macOS     │
    └──────────────┘

The model is not the source of truth, the authorization authority, the privacy
authority, the identity authority, or the executor. Today it is weaker still: it
never chooses an action at all. Commands are parsed deterministically, and the
model answers questions. When it does start proposing actions, it will propose
into the same gates — the ones its answers cannot reach.

Three mechanisms make that structural rather than intended:

- **The allowlist**: a closed `ActionKind` enum. There is no generic command.
- **The registry gate**: an action not covered by an implemented capability is
  refused at runtime, so a kind cannot become something KUE does but never says.
- **The offered-only rule**: file moves are refused for any path the last
  storage report did not show the owner — checked when asked and again at the
  moment of moving.

## What exists, by area

| Area | Status |
|---|---|
| Identity: face, presence, access session, lock and timeouts | IMPLEMENTED |
| Identity: voice | NOT IMPLEMENTED |
| Invocation (wake) | PARTIAL — `KUE_VOICE_ARCHITECTURE.md` |
| Storage: inspect, analyse, recommend, move to Trash, restore, verify | IMPLEMENTED |
| Computer: open/quit/switch apps, links, folders, listings, documents | IMPLEMENTED |
| Computer: type, click, scroll, read a window | NOT IMPLEMENTED (needs Accessibility) |
| Web: search, read, compare, forms | NOT IMPLEMENTED |
| Goals with explicit step states, per-step authorization and verification | IMPLEMENTED (automated) — `goal.rs` |
| Plans of more than four steps, or plans a model proposes | NOT IMPLEMENTED |
| Memory of preferences and decisions | NOT IMPLEMENTED |
| Proactivity | NOT IMPLEMENTED, deliberately |

## The next layer, and why in this order

1. **Speaker identity as a factor** — deferred until the owner's microphone and
   room test and a model licence decision. Never equal to Touch ID.
2. ~~An intent router~~ — built (`intent.rs`), automated only.
3. ~~A goal and plan representation~~ — built (`goal.rs`), automated only.
   Rollback is not generalised: the one recovery defined reports a failed move.
4. **The web agent** — the largest new attack surface in the product, because a
   page can carry instructions. It needs its own session, its own adversarial
   corpus, and the rule that a page is data and never an instruction.
5. **In-app control** — Accessibility, semantic targets first, verification
   mandatory.
6. **Preferences in memory**, then and only then **proactivity**, from measured
   thresholds and with evidence.

Items 2 and 3 are built; nothing else above is started. Each is its own phase, verified on this Mac before the
next begins.
