# KUE intelligence layers — where the product actually is

The layer model from the brief, with the honest state of each. The point of
this document is the boundary: which layer KUE is genuinely on, and therefore
what may be built next.

**IMPLEMENTED** · **PARTIAL** · **NOT BUILT**

---

## Layer 0 — Foundation · IMPLEMENTED

Sensing (camera, Vision, face measurements, body and hand pose, scene, light),
identity and its state machine, temporal context, evidence and confidence,
local memory, the privacy firewall, runtime states and the kill switch,
authorization levels, the Action Broker, action transactions with verification,
voice in and out, and the basic computer actions (apps, folders, documents,
links, notifications).

Not rebuilt in this phase, and not to be rebuilt. `docs/KUE_STATUS.md` records
what each part measures and what it does not.

## Layer 1 — Awareness · PARTIAL

*KUE can inspect and understand the relevant environment, rather than only
answering what it is asked.*

| Part | State |
|---|---|
| Which application is in front | IMPLEMENTED |
| **Being addressed out loud (the invocation)** | **PARTIAL** (this phase) — `KUE_VOICE_ARCHITECTURE.md` |
| Whether the owner is here, and who they are | IMPLEMENTED |
| Input idle time (no keylogging) | IMPLEMENTED |
| System conditions: thermal state, KUE's own cost | IMPLEMENTED |
| **Storage: the volume, and what is in the allowed folders** | **IMPLEMENTED** (this phase) |
| Applications installed | IMPLEMENTED (names and bundle ids, for resolving "open Chrome" — not sizes, not last used) |
| Recent activity across the machine | NOT BUILT |
| What is on screen | NOT BUILT, and deliberately: `ScreenContent` is NEVER_COLLECT under policy v1 |

## Layer 2 — Analysis · PARTIAL

*Comparing information and identifying patterns.*

Storage is the first real instance: duplicates by name and size, installers, old
downloads, large files — each by a fixed rule, computed in pure code, never by a
model. `core/src/storage.rs`.

Elsewhere: identity contradiction detection and settling (IMPLEMENTED),
activity conclusions from presence and idle time (IMPLEMENTED). Cross-signal
pattern recognition over time — "you do this every Monday" — is NOT BUILT, and
the registry says so.

## Layer 3 — Reasoning · PARTIAL

*What is happening, why it matters, what the options are — with observed,
inferred and unknown kept apart.*

This separation is now structural in two places:

- context lines carry `basis: OBSERVED | INFERRED` and the window prints
  *seen* / *worked out* beside each;
- every storage finding carries `evidence` (measured), `reason` (inferred), and
  `caution` (what it costs to be wrong), and the sheet shows all three.

What is missing: reasoning that spans domains, or that holds a question open
across turns. A follow-up like "what can I optimise?" on its own is **not**
resolved from the previous turn — KUE does not decide from context what an
ambiguous sentence meant. That is a real gap, deliberately left rather than
guessed.

## Layer 4 — Assistance · PARTIAL

Storage recommendations are specific, evidenced and counted. They are also the
only ones: KUE does not yet recommend anything about apps, work patterns or
time.

## Layer 5 — Action · PARTIAL

The Action Broker, the allowlist, authorization by risk, confirmation and
atomic claim are all IMPLEMENTED — for opening things. Nothing in KUE moves or
deletes a file outside `~/KUE`, and storage cleanup is NOT BUILT. When it is, it
will be reversible (Trash, never delete) and verified per item.

## Layer 6 — Verification · IMPLEMENTED for what exists

Every action records what was checked after it ran; SUCCEEDED without
verification evidence becomes UNKNOWN_RESULT by construction. The storage pass
verifies itself with what it read: the entry count, the areas, the volume
figures, and whether it stopped at the limit.

The verification to be careful about is the one not yet built: moving files to
the Trash frees no space until the Trash is emptied, so "free space increased"
must never be claimed from a move. `KUE_STORAGE_INTELLIGENCE.md` fixes the
sentence in advance.

## Layer 7 — Memory · PARTIAL

Local memory holds events, snapshots and the privacy ledger. It does **not**
hold preferences learned from decisions ("this owner archives rather than
deletes"), because the decisions do not exist yet — there is nothing to learn
from. Storage findings are classified out of memory on purpose.

## Layer 8 — Proactive intelligence · NOT BUILT

Nothing in KUE raises a finding the owner did not ask for. The capability row
`proactive_assistance` is NOT_IMPLEMENTED and is one of the rows that corrects a
model answer claiming otherwise.

The order matters: proactivity on top of layers that are not reliable is how a
product becomes noise. Storage is the first candidate for it — a drive that
passes 90% is a real, measurable, non-invented signal — but not before the
review flow has been used and the findings have proved worth interrupting for.

---

## What the next layer needs, in order

0. **Done since this was written:** cleanup that acts (Trash, per-item
   verification, restore) and the invocation. What follows is unchanged.

1. ~~Cleanup that acts~~ — done.
2. **Selection and memory of decisions** — what the owner kept, so the second
   report is better than the first.
3. **Content hashing for duplicates**, with its own data kind and authorization.
4. **A held question** — resolving "what can I optimise?" from the turn before,
   explicitly and visibly, rather than by guessing.
5. Only then: proactivity, and only from measured thresholds.
