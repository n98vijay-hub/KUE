# KUE runtime spine — declared tools, one pipeline, a conversation

**2026-09-21 · S5 and S6.** Written after building, so it describes what exists.
Every claim is labelled **IMPLEMENTED**, **TESTED** or **LIVE**.

## The interaction model it is built for

```
USER SPEAKS / TYPES / (later) KUE NOTICES
  → UNDERSTANDS what it refers to        dialogue::interpret   (open item? new request?)
  → DETERMINES INTENT                    intent::classify      (rules; no model)
  → CHECKS CONTEXT                       engine, facts, registry
  → DECIDES: ANSWER · ASK · SUGGEST · PLAN · ACT      pipeline::Decision (recorded)
  → POLICY + AUTHORIZATION               authz, privacy, Touch ID — unchanged
  → TOOL                                 tools::declared → broker → executor
  → VERIFICATION                         read-back → VerifiedFact
  → RESPONSE                             a turn in the conversation
```

Nothing in this path was built around the Ask button. The button — and the
Speak button, and the hands-free listener — are transports. All three now make
the same call.

## S5 · Declared tools — `core/src/tools.rs`

**IMPLEMENTED · TESTED.** Sixteen declarations: the fifteen actions the broker
allows, plus exact arithmetic. Each states input and output fields, the data it
touches, its preconditions, its executor, what is read back to verify it, how
it is undone, and what has been seen working.

| Rule | How it is enforced |
|---|---|
| Only what exists is declared | Every declaration answers to a registry row that is not NOT_IMPLEMENTED; WEB_SEARCH, TYPE_TEXT, CLICK_TARGET, CALENDAR_*, SEND_MESSAGE, DELETE_FILE have no declaration (test) |
| Nothing executes undeclared | The broker refuses an action with no declaration (`propose_kind`) |
| Nothing is written twice | Risk, authorization operation and confirm-needed are computed by `actions::risk`, `operation_for`, `needs_confirmation` — the broker's own functions (test) |
| Status follows evidence | LIVE_VERIFIED only with a dated record; never more than the capability row allows. OPEN_APPLICATION is LIVE; CLOSE_APPLICATION and SHOW_NOTIFICATION are TESTED because nobody has seen them work |
| Changes say how they are undone | MOVE_TO_TRASH ↔ RESTORE_FROM_TRASH; OPEN_URL states it cannot be undone |

This is the list a model will one day choose from — by id and arguments — with
the runtime deciding whether the choice runs. **No model reaches it today.**

## S6 · The pipeline — `core/src/pipeline.rs`

**IMPLEMENTED · TESTED.** One lifecycle for every request:

```
RECEIVED → UNDERSTANDING → CHECKING → DECIDING ─┬─▶ ANSWERING ─────────▶ COMPLETED
                                                ├─▶ ASKING ─▶ WAITING_FOR_USER
                                                ├─▶ SUGGESTING
                                                ├─▶ PLANNING ─▶ AUTHORIZING | WAITING_FOR_USER
                                                └─▶ AUTHORIZING ─▶ ACTING ─▶ VERIFYING ─▶ COMPLETED
                                                                                    ├─▶ UNCERTAIN
                                                                                    └─▶ FAILED
WAITING_FOR_USER ─ confirm ─▶ AUTHORIZING   (re-authorized, never trusted)
                 ─ correct ─▶ PLANNING      (revised, not replaced)
any live state   ─▶ CANCELLED · REFUSED · BLOCKED · PAUSED · KILLED
```

| Property | Test |
|---|---|
| A request cannot reach COMPLETED without ACTING and VERIFYING | `a_request_cannot_claim_to_have_finished…` |
| Illegal steps are refused and counted | same; `refused_transitions` is projected |
| UNCERTAIN (could not verify) is never upgraded to COMPLETED | `a_result_kue_could_not_verify_is_never_completed` |
| Kill ends every live request; nothing stays open | `kill_ends_every_live_request…`, `after_a_kill_do_it_confirms_nothing` |
| Nothing is confirmed across a pause | `nothing_is_confirmed_across_a_pause` |
| Every transport takes the same path | `every_transport_enters_the_same_pipeline`, `typed_and_spoken_requests_take_the_same_path` |
| Bounded; finished work is forgotten first | `the_pipeline_is_bounded…` |

**Ids carried:** conversation, request, goal, plan, tool execution, verification.
**The runtime state** (IDLE · LISTENING · UNDERSTANDING · CHECKING · ASKING ·
PLANNING · AUTHORIZING · ACTING · VERIFYING · COMPLETED · FAILED · BLOCKED ·
UNCERTAIN · WAITING_FOR_USER · RECOVERING · PAUSED · KILLED) is derived in the
core; kill, recovery and pause outrank any request.

**Transports:** VOICE and TYPED are wired. PROACTIVE, COMPUTER_CONTEXT and
SCHEDULED exist in the type and enter the same path, but **nothing produces
them yet** — they are reserved, not implemented features.

## S6 · The conversation — `core/src/dialogue.rs`

**IMPLEMENTED · TESTED.** Eleven turn kinds: USER_SPEECH, KUE_RESPONSE,
KUE_QUESTION, USER_CONFIRMATION, USER_CORRECTION, USER_CANCELLATION,
KUE_SUGGESTION, TOOL_PROGRESS, TOOL_RESULT, VERIFICATION_RESULT, GOAL_PROGRESS.
At most one thing is open at a time — an action awaiting a confirm, a plan
awaiting a choice, a question KUE asked — because "do it" can only mean one
thing. Turns live in memory, bounded; they are never written to local memory.

An utterance is interpreted against what is open:

| Heard | While | Becomes |
|---|---|---|
| "Do it", "yes", "go ahead" | an action awaits confirm | CONFIRMATION → `confirm` → re-authorization → Touch ID |
| "Do it" | a plan awaits a choice | the exact set KUE announced → move → Touch ID |
| "Stop", "no", "never mind" | anything | CANCELLATION; nothing runs |
| "No, don't touch the installers" | a storage plan | CORRECTION: installers excluded, plan re-stated |
| "Only the duplicates" | a storage plan | CORRECTION: only duplicates |
| "No, not those" / names two things | a storage plan | CLARIFY — KUE asks; never guesses |
| "Do it" | nothing open | a new request (which does nothing) |

**A correction binds.** The exclusion is enforced at the moment of moving: a file
in an excluded category is refused even if the storage sheet sends it
(`a_correction_binds_even_if_the_window_sends_the_file_anyway`).

## The front door

`transaction::receive` (core) and `receive_input` (shell) are the one entry for
every input. The window makes one call where it made three; the hands-free
listener makes the same call. Two consequences worth naming:

- A **typed** "yes" now confirms what is waiting. Before, only a spoken one did.
- The **hands-free** path can now confirm, cancel and correct. The forensic audit
  recorded that as broken (F8).

Every action record change syncs its request from `publish` and `cancel` — the
two places every record passes — so no button, goal step or transport can move
an action without the runtime knowing.

## What was verified, and how

| | Class | Evidence |
|---|---|---|
| Declarations, broker enforcement | TESTED | 7 tool tests |
| Pipeline legality, kill, pause, bounds | TESTED | 13 pipeline tests |
| Interpretation, clarification | TESTED | 6 dialogue tests |
| The owner's two exchanges end to end | TESTED | 8 scenario tests through `receive` with the real storage engine and goal, a pretend executor |
| **"Clean up my storage" → "No, don't touch the installers" → "Do it"** | **LIVE** | `conversation_live_…` on this Mac: the real storage walk, the real KueAct executor, the real Trash, over a home of KUE's own files. The installer stayed; two files moved, were verified in the Trash, and came back. **Touch ID was a stand-in** |
| The same exchange spoken to the running app | **UNVERIFIED — OWNER ACTION REQUIRED** | needs a microphone, a face, and a finger |

## A product decision to confirm

**"Do it" to a laid-out plan moves every file KUE announced.** That is the
exchange you described, and it is guarded: KUE states the count first, Touch ID
is required, the batch is capped at 50, everything goes to the Trash and can be
put back, and a correction binds. But it is a larger blast radius than the
storage sheet, where each file is ticked. If you would rather "do it" only ever
confirm files chosen in the sheet, it is a one-line change in `receive`.

## Not built in this slice

A proactive, scheduled or context-driven producer · model-proposed plans or
tool choices · corrections for anything other than storage categories ·
speaking the conversation's turns through the voice (they are recorded and
projected; the existing narration still speaks action outcomes) · the primary
window showing the conversation (it is in Diagnostics).

---

**Continued in `docs/KUE_CONVERSATION_AND_PLANS.md` (S7, 2026-09-22):** corrections across turns, questions about the work, stop and undo, governed speech, the conversation on screen, and first-class plans.
