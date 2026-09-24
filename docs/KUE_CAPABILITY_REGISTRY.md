# The capability registry

`core/src/capabilities.rs` is the one list of what KUE can do. Everything that
says what KUE can do reads it: the Capabilities sheet, the answer to "what can
you do?", what KUE says out loud, the context a model is given, the Action
Broker's runtime gate, and Diagnostics. They cannot disagree, because there is
only one list.

**35 capabilities** as of this phase.

## What a row carries

| Field | What it decides |
|---|---|
| `id` | Stable, never shown. Safe to rename everything else around. |
| `name`, `user_name` | The engineering name and the one a person reads. |
| `group` | Senses · Identity · Understanding · Doing · Voice · Memory · Safety |
| `note` | Exactly what is implemented, and what is not. For Diagnostics and the model. |
| `ui_description`, `voice_description` | One sentence each, on screen and aloud. |
| `limits` | The material limits, at the claim rather than in a footnote. |
| `status` | `Real` · `Partial` · `Simulated` · `Placeholder` · `NotImplemented` |
| `risk`, `authorization` | What it costs to be wrong, and what the owner must be. |
| `privacy` | The data kinds it touches. The firewall enforces; this is what the owner is told. |
| `permissions` | macOS grants it cannot work without, with the Settings pane only the owner can use. |
| `verification` | How KUE establishes it did what it said. |
| `proof` | What has been **seen**: `LiveVerified { on, seen }` · `PartlyLiveVerified { on, seen, not_seen }` · `TestVerifiedOnly` · `NotApplicable`. Separate from `status` (does it exist) and from availability (can it be used now). |
| `deliberate` | Not built *on purpose*, as opposed to not built yet. |
| `claim_phrases` | Wordings that amount to claiming it — how a model answer gets corrected. |
| `action_tags` | The `ActionKind`s this capability permits. |

## The invariants, each a test

1. **Every executable action is covered by exactly one implemented capability.**
   The Action Broker refuses at runtime any action whose tag is not covered
   (`propose_kind`), so a kind added to the parser without a capability cannot
   quietly become something KUE does but never says it does.
2. **Claim phrases live only on rows KUE does NOT have.** A phrase on a `Real`
   row would either never fire or would correct a capability KUE actually has.
   When storage cleanup became real, this test failed until the phrases moved
   to `permanent_deletion` — which is the invariant doing its job.
3. **Renaming a capability breaks nothing.** The phrases travel with the row, so
   the overclaim checker cannot silently drift from it.
4. **Proof agrees with status, and with the master status document.** A
   not-implemented row has no proof; an implemented one says how far it has
   been checked, and a live one says when and what was seen. The LIVE, PARTLY
   LIVE, TEST ONLY and NOT APPLICABLE blocks of
   [KUE_MASTER_STATUS.md](KUE_MASTER_STATUS.md) must list exactly the
   registry's rows at each level, or `the_master_status_lists_the_same_proof_as_the_registry`
   fails. A capability moves to LIVE_VERIFIED only when it worked on this Mac
   through KUE's own app or helper — never because a test passed.

## Availability is computed, never stored

`availability(spec, facts)` reads live facts: an unchecked macOS permission
reports **unavailable**, never available; a denied one names the Settings pane
only the owner can use. `Accessibility` reports not granted because KUE neither
requests nor uses it.

## The `Doing` group today

| Capability | Status |
|---|---|
| Computer automation — open/quit/switch apps, links, folders, listings, documents, notifications, files in `~/KUE` | **Real** |
| Storage inspection (in `Understanding`) — the volume, the folders, the findings | **Real** |
| Moving files to the Trash, and putting them back | **Real** |
| Control inside other apps — typing, clicking, reading another app's window | **NotImplemented** (needs macOS Accessibility) |
| Deleting anything for good, emptying the Trash | **NotImplemented, deliberate** |
| Purchases and bookings | **NotImplemented** — would need your confirmation and macOS authentication |
| Sending messages | **NotImplemented** — would be shown to you and sent only after you confirm |

Added with the intent router, in `Understanding`: **Arithmetic** (Partial —
exact, numbers only) and **Recalling the past** (NotImplemented).

The last two are load-bearing. They are how KUE answers honestly when asked,
and how a model answer claiming either gets corrected before the owner sees it.

## Adding a capability

1. Write the row first, with `status` telling the truth about the code that
   exists at that moment — `NotImplemented` is a valid, useful state to ship.
2. If it executes something, add its `action_tags`, and the `ActionKind` and
   `ALLOWLIST` entry in the same commit, or the tests fail. That is deliberate.
3. Name its `privacy` kinds. If a kind does not exist yet, add it to the policy
   and classify it there — not in a paragraph.
4. Say how it is verified. `NotApplicable` is only for capabilities that claim
   nothing.
5. If it is not built on purpose, set `deliberate` and give it `claim_phrases`.
6. Set `proof` to `TestVerifiedOnly` (or `NotApplicable` if it does not exist),
   and add it to the matching block in `KUE_MASTER_STATUS.md`. Change it only
   after seeing it work on this Mac, and write down what was seen.
