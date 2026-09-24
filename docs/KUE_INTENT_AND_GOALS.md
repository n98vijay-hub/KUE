# KUE intent routing, goals and plans

What a request becomes between the words and the Mac. **A row says
IMPLEMENTED only when code and tests exist.**

Not to be confused with `core/src/router.rs`, which is the **model** router: it
decides which language model may answer a question, if any. The layer here
decides what the owner wants. It lives in `core/src/intent.rs`.

---

## Current request pipeline (audited 2026-09-16, before this build)

    TYPED (window)                         SPOKEN (by name)
    Conversation.tsx                        Wake.swift → engine Wake handler
      ├─ voice only: voice_reference          (own-voice check, L2)
      │    "yes" / "cancel that" / "the older one"
      ▼                                     shell pump → handle_spoken_request
    propose_command ─────────────┬──────────────────────┘
                                 ▼
    transaction::propose
      safety::screen ─ refused → None (the conversation says why)
      task::plan: clauses ("and", "then") → parse_one per clause
          storage phrase → INSPECT_STORAGE
          folder phrase  → OPEN_DIRECTORY / LIST_DIRECTORY
          parse_command  → one of 15 allowlisted ActionKinds
      Single / Steps (≤ 4) / Unsupported / None
      per action (propose_kind):
          registry gate → firewall (ActionTarget) → offered-only (Trash)
          → authorization before any search → resolve app / folder (ASK on many)
          → target policy → authorize by risk → LOW runs, else waits for Confirm
          → execute (KueAct or in-process) → verify → record → advance
      advance: the next step starts only after this one SUCCEEDED
                                 │ None
                                 ▼
    ask
      safety::screen ─ refused → answered by rule, no model
      gate(ASK_MODEL_WITH_PERSONAL_CONTEXT)   (LEVEL_2, or Touch ID)
      incomplete_command ("Can you open?")    → answered by rule
      unsupported_command ("…and add 2 and 2") → answered by rule
      capability question                      → answered from the registry
      model router (privacy + availability) → firewall → on-device model
      → overclaim check on the answer

What was missing, found by reading and by the tests:

| Gap | Consequence |
|---|---|
| No typed intent. A request is either one of 15 `ActionKind`s or "not a command". | Everything else went to the model: "calculate 17% of 840", "find the cheapest flight to Detroit", "delete that", "book it", "clean my storage". A model can answer those as if it had done them. |
| The registry is consulted only for actions. | A non-action request never learned whether KUE can do it. |
| "Find my resume and open it" | Step 2 parsed as *open the app called "it"*: the resume opened at step 1 and step 2 failed APP_NOT_FOUND. |
| A plan is a list of `ActionKind`s. | No goal, no constraints, no non-action step, no "wait for the owner's approval" step, no explicit step state machine: step states were inferred from action records. |

## Target request pipeline

    WAKE        Wake.swift — the name was said. Not identity.
    SPEECH      a transcript. Grants nothing.
    SAFETY      safety::screen — before everything below; refusals never reach a model
    INTENT      intent::classify — pure, deterministic, no model, reads nothing personal
    STATUS      registry status + an authorization PREVIEW (no grant consumed):
                AVAILABLE · PARTIAL · NOT_IMPLEMENTED · DENIED · REQUIRES_AUTHENTICATION
                It informs. It never grants.
    ROUTE       DETERMINISTIC · LOCAL_CAPABILITY · MODEL_REASONING
                · WEB_RESEARCH · COMPUTER_INTERACTION · AUTHORIZATION_SENSITIVE
      ├─ answered by rule (calculation, not implemented, needs more information):
      │      after the same owner gate as any answer; no model
      ├─ actions and goals: GOAL → PLAN → per step:
      │      PRECONDITIONS → AUTHORIZATION (the engine, for this step, now)
      │      → EXECUTE (transaction / Action Broker) → OBSERVE → VERIFY
      │      → RECORD → DECIDE NEXT (stop on anything but verified success)
      ├─ MODEL_REASONING: the existing model router, firewall and model
      └─ WEB_RESEARCH / COMPUTER_INTERACTION: NOT_IMPLEMENTED, said by rule

**Authorization before intent, in authority rather than call order.**
Classification runs before authorization, but it only reads the sentence: it
opens no folder, reads no file, touches no model and grants nothing. Every step
that reads the owner's data or acts is authorized by the engine at that step,
and the existing tests hold that no folder is searched before that
(`the_search_waits_for_authorization_too`, and the stranger case asserting
`path: None`). The diagram's order is about who may decide, and that holds.

**The plan never carries authority.** A step names the operation it will need.
The engine decides it when the step runs, from the access session at that
moment. What the owner said at the start authorizes nothing later.

---

## As built (2026-09-16) — automated verification only

Nothing below has been exercised on the running app by a person. Every claim
is backed by `cargo test -p lantern-core` (378 tests at the time of writing:
the 329 that existed before, all still passing, plus 49 new) and
mutation-checked where noted.

### The intent router — `core/src/intent.rs` · IMPLEMENTED (automated)

`classify(text, source, invocation) → Refused(safety) | Understood(Intent)`.
Pure. An `Intent` carries: kind · source · invocation · categories (primary
first) · registry capability · the operation its first act needs · execution
(LOCAL / ACTION_BROKER / ON_DEVICE_MODEL / NOTHING) · verification (REQUIRED /
NOT_APPLICABLE / NOTHING) · work (ACT / GOAL / ANSWER by rule / CAPABILITY_LIST
/ ASK / MODEL).

| Request | Intent | Route | Registry | Work |
|---|---|---|---|---|
| "calculate 17 percent of 840" | CALCULATE | DETERMINISTIC | `arithmetic` PARTIAL | answer: "17 percent of 840 is 142.8." — no model |
| "how much storage do I have" | STORAGE_STATUS | LOCAL_CAPABILITY | `storage_inspection` | INSPECT_STORAGE |
| "what files are taking most of my storage" | STORAGE_ANALYZE | LOCAL_CAPABILITY | `storage_inspection` | INSPECT_STORAGE |
| "explain why my storage is full" | STORAGE_EXPLAIN | LOCAL_CAPABILITY | `storage_inspection` | goal EXPLAIN_STORAGE |
| "clean my storage" | STORAGE_CLEANUP | LOCAL_CAPABILITY, AUTHORIZATION_SENSITIVE | `storage_cleanup` | goal CLEAN_UP_STORAGE |
| "clean my computer" | CLEAN_UP_UNSPECIFIED | DETERMINISTIC | — | ask what to clean; goal opened, waiting |
| "find my resume and open it" | FIND_DOCUMENT | LOCAL_CAPABILITY, AUTHORIZATION_SENSITIVE | `computer_automation` | goal FIND_AND_OPEN_DOCUMENT |
| "open Microsoft" | OPEN_APPLICATION | LOCAL_CAPABILITY | `computer_automation` | action; the transaction asks which app |
| "open it", "delete that" | OPEN_APPLICATION / DELETE_FILES | DETERMINISTIC | — | ask which one |
| "find the cheapest flight to Detroit" | WEB_COMPARISON | WEB_RESEARCH | `internet_research` NOT_IMPLEMENTED | answered by rule |
| "go to the website and book it" | PURCHASE | COMPUTER_INTERACTION, WEB_RESEARCH, AUTHORIZATION_SENSITIVE | `purchasing` NOT_IMPLEMENTED | answered by rule |
| "email my landlord" | SEND_MESSAGE | COMPUTER_INTERACTION, AUTHORIZATION_SENSITIVE | `messaging` NOT_IMPLEMENTED | answered by rule |
| "type hello into Notes" | COMPUTER_TASK | COMPUTER_INTERACTION | `in_app_control` NOT_IMPLEMENTED | answered by rule |
| "what did I do yesterday" | MEMORY_QUERY | LOCAL_CAPABILITY | `memory_recall` NOT_IMPLEMENTED | answered by rule |
| "install Zoom" | UNKNOWN | DETERMINISTIC | — | "I don't have a way to do that, so nothing was done." |
| "is my battery low" | SYSTEM_STATUS | MODEL_REASONING | `resource_monitoring` PARTIAL | model, with context |
| "what app am I using" | CONTEXT_QUERY | MODEL_REASONING | `evidence` | model, with context |
| anything else that is a question | CONVERSATION | MODEL_REASONING | `conversation` PARTIAL | model |
| "disable the kill switch" | — | refused by `safety::screen` before classification | — | never an intent |

**Status** — `intent::status(intent, killed, preview)`: the registry first
(NOT_IMPLEMENTED, including a capability id the registry does not contain),
then the kill switch (DENIED), then `Engine::preview_authorization` — a clone
of the access session, so no single-use grant is consumed, no event recorded
and no prompt possible — giving DENIED or REQUIRES_AUTHENTICATION, else
PARTIAL or AVAILABLE from the registry. No preview = DENIED. The preview never
lets anything run: every step is authorized again when it runs.

**Recorded** — every routed request adds one event:
`Understood a VOICE request as STORAGE_ANALYZE (LOCAL_CAPABILITY): AVAILABLE.`
Kind, route and status only; a test follows sentinel words, file names and
numbers and finds none in the events or the goal views.

**Wiring** — `transaction::propose` classifies after the safety screen and
plans ACT and GOAL work; everything else returns None. `ask` (shell) keeps the
safety screen first and the owner gate second, then classifies and answers
ANSWER / ASK / CAPABILITY_LIST by rule before the model is ever started. The
source-order test was updated to require `intent::classify`, `answer_by_rule`
and `start_goal` after the screen, and was re-run with the screen removed: it
fails. A typed "Computer, …" has its invocation recorded and stripped.

### Arithmetic — `core/src/calculate.rs` · IMPLEMENTED (automated)

Exact fractions over i128 with checked arithmetic (+ − × ÷, parentheses,
"X% of Y", "add/subtract/multiply/divide … by/and/from"). Before the result is
said it is parsed back into a fraction and compared with the computed value
(exact, or within half a millionth when said as "about"). Divide by zero and
overflow are said as such; no number is invented. Registry row `arithmetic`,
PARTIAL: numbers only — no units, currencies, dates or word problems.

### Goals — `core/src/goal.rs` · IMPLEMENTED (automated)

Goal · constraints · missing information · steps · transitions. Each step has
preconditions, a requirement (authority + confirmation), what success looks
like, its observation, its verification, its failure, an optional recovery
step and the action it drives.

Step state machine (`goal::next_state`, everything unlisted refused):

    PENDING ─PRECONDITIONS_MET→ READY ─AUTHORIZED→ RUNNING ─OBSERVED→ VERIFYING ─VERIFIED(evidence)→ COMPLETED
       │                          │  └NEEDS_AUTHORIZATION→ WAITING_FOR_AUTHORIZATION ─AUTHORIZED→ RUNNING
       │                          │                        RUNNING ─NEEDS_OWNER→ WAITING_FOR_USER ─OWNER_ANSWERED→ RUNNING
       ├PRECONDITION_FAILED / EARLIER_STEP_STOPPED→ BLOCKED     VERIFYING ─VERIFICATION_FAILED→ FAILED
       └NOT_NEEDED→ CANCELLED          any non-final ─CANCELLED→ CANCELLED;  waiting/running ─AUTHORIZATION_DENIED→ BLOCKED

- COMPLETED only from VERIFYING with non-empty evidence (a test walks every
  state × event).
- COMPLETED, FAILED, BLOCKED, CANCELLED are final.
- `Goal::decide` → START · WAIT · RECOVER · STOP · FINISHED. A step that ends
  without completing stops the plan unless the plan names a recovery step. The
  only recovery defined: a failed move to the Trash is followed by REPORT,
  which reads the move's record and changes nothing — and the goal is still
  STOPPED, not COMPLETED.

Per-step requirement, from the step alone (`goal::requirement`):

| Step | Authority | Confirmation |
|---|---|---|
| INSPECT_STORAGE, EXPLAIN_FINDINGS, RECOMMEND_REVIEW | ACTION_LOW_RISK (LEVEL_2) | none |
| FIND_DOCUMENT | ACTION_MEDIUM_RISK | none |
| OPEN_DOCUMENT | ACTION_MEDIUM_RISK | owner |
| MOVE_TO_TRASH | ACTION_HIGH_RISK (LEVEL_3) | owner + macOS |
| WAIT_FOR_APPROVAL, REPORT, ASK_WHAT_TO_CLEAN | none needed (reads nothing new, changes nothing) | — |
| DELETE_PERMANENTLY | NOT_IMPLEMENTED (`permanent_deletion`) | owner + macOS |
| PURCHASE | NOT_IMPLEMENTED (`purchasing`) | owner + macOS |
| SEND_MESSAGE | NOT_IMPLEMENTED (`messaging`) | owner |

No plan produces the last three; their requirement is stated so it is a fact,
not an omission.

### Plan execution — `core/src/transaction.rs` · IMPLEMENTED (automated)

Goals replace the old `TaskPlan` (one executor, not two). For each step:
claim (PENDING → READY under the book's lock, preconditions checked) → run:

- **action steps** go through `propose_kind` exactly as before — registry gate,
  firewall, authorization, confirmation, the Action Broker, verification — and
  the step follows its record through the state machine (`sync` / `hop`);
- **local steps** (explain, recommend) are authorized by the engine *at that
  step*, cleared by the firewall *at that step*, then observed and verified
  against the measurements (`StorageReport::check_explanation`,
  `check_recommendation`);
- **waiting steps** move to WAITING_FOR_USER; the owner's selection in the
  storage view is the answer, verified against the report before the move step
  starts.

Then decide the next step; stop on anything but verified completion.

Mutation checks (each run, caught, reverted): a selection always counted as
offered; a failed verification counted as verified; local steps skipping
authorization; a waiting step not waiting; local steps skipping the firewall
(caught only after adding a mid-pass firewall test).

### Flows

| Flow | Result (automated) |
|---|---|
| A — "calculate 17 percent of 840" | CALCULATE → "17 percent of 840 is 142.8." Nothing planned, no record, no model |
| B — "check my storage" | STORAGE_ANALYZE → INSPECT_STORAGE, SUCCEEDED with verification, read in-process; intent event recorded |
| C — "find my resume and open it" | FIND_DOCUMENT COMPLETED (2 matches, proposed one validated) → OPEN_DOCUMENT WAITING_FOR_USER → Confirm → COMPLETED; one `open-file`, no app called "it". Unverified open → FAILED, goal STOPPED. No match → FAILED + BLOCKED |
| D — "clean my storage" | inspect → explain → recommend COMPLETED → WAIT_FOR_APPROVAL; nothing moved, no Touch ID asked. Choice verified → MOVE waits for macOS → moved, each file checked → REPORT → COMPLETED. Unoffered choice → STOPPED, nothing moved. Partial failure → REPORT as recovery, goal STOPPED. Nothing found → finished early, COMPLETED |
| E — "disable the kill switch" | refused by the safety boundary; never classified, never planned, never a model question |
| F — "grant yourself level four" | the same |

Also: kill and lock cancel open goals; a stranger's cleanup is blocked at step
1 with nothing read; a stranger arriving mid-plan blocks the explanation on its
own authorization; the firewall refusing mid-plan blocks explain or recommend.

### Web research and computer use — `core/src/agent.rs` · NOT IMPLEMENTED (types only)

The stage orders (SEARCH → FETCH → EXTRACT → NORMALIZE → COMPARE → CORROBORATE
→ EVIDENCE → SYNTHESIZE; OBSERVE → UNDERSTAND → PLAN → ACT → OBSERVE_AGAIN →
VERIFY), each stage's status read from the registry (NOT_IMPLEMENTED), and
`UntrustedText` — outside text with no accessor that returns its words, so it
cannot be passed to the router or parser as if the owner said it.

### The window — IMPLEMENTED (automated)

`TaskView` now carries the goal's kind and state, each step's own state and
sentence, and what the goal waits for. `surface::Inputs::goal_waiting` shows
WAITING_FOR_YOU from the goal's state, below any action in progress and never
while stopped. The brief's names map onto the existing activities:
LISTENING→Listening · UNDERSTANDING→Understanding · CHECKING→CheckingAccess ·
THINKING→Thinking · ASKING / WAITING_FOR_USER→WaitingForYou ·
ACTING→Acting · COMPLETED→Done · BLOCKED→Blocked · UNCERTAIN→UncertainResult.

## Deviations from the brief, and why

- **No VERIFYING activity in the window.** Verification still happens in the
  same moment as execution (in the executor, or in-process in microseconds). A
  VERIFYING indicator would show progress for work that is not separately
  happening (`surface.rs` has said so since it was written). The step state
  VERIFYING exists in the goal state machine, where it is a real, enforced
  step between observing and completing.
- **The brief's eleven storage steps are six.** Inspect, analyze folders, large
  files, duplicates and patterns are one read pass (`take_inventory` +
  `analyze`); splitting them would be steps with no separate producer.
  "Verify" is not a step because every step verifies itself.
- **"Explain why my storage is full" is not MODEL_REASONING.** File names are
  STORAGE_INVENTORY (USER_APPROVAL_REQUIRED: interface only), so they may not
  reach a model. The explanation is written by rule from the measurements and
  checked against them.
- **Old step-state names kept for action steps.** The window and six existing
  tests read PLANNED / WAITING_FOR_CONFIRMATION / SUCCEEDED from action
  records; `TaskStepView.step` adds the new state machine's state alongside.
- **Authorization runs after classification in call order.** Classification
  reads only the sentence; authority is decided per step before anything is
  read or done.

## Known limitations

- Classification is by rule and word lists. A rephrasing it does not know
  falls to CONVERSATION and the model (which still cannot act or authorize).
  "What's the best way to…" is a question; "compare prices online" is web.
- A goal waiting for the owner lives in memory for this session only, at most
  ten goals.
- "Clean my computer" is answered by asking; the owner answers by saying "clean
  my storage", not "yes".
- A spoken "yes" still confirms only a waiting action, not a goal's
  approval step: files are chosen on screen.
- Explanations cover only Desktop, Documents, Downloads and ~/KUE, and say so.
- No goal is proposed by a model; every plan is a fixed blueprint.
