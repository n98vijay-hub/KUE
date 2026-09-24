# KUE · S7 — natural two-way interaction and governed plans

**2026-09-21 → 2026-09-22.** Branch `kue/runtime-safety`, commits `c251542` → this document's commit.
Builds on `docs/KUE_RUNTIME_SPINE.md` (S5/S6: declared tools, the request
pipeline, the conversation, the one front door).

The rule this slice is judged by: **the model may reason and propose; the
governed runtime decides.** Nothing here gives a model a way to act. Every
answer about KUE's own work is composed from runtime state, never by a model.

## 1 · What the owner can now do, in words

```
YOU  Clean up my storage.
KUE  I found three files worth reviewing: one installer, one probable duplicate
     and one old download. I can move them to the Trash — say "do it", or tell
     me what to leave out.
YOU  Leave the installer.
KUE  Okay. I'll exclude the installer.
KUE  That leaves one probable duplicate and one old download.
YOU  Actually leave the old download too.
KUE  Okay. I'll exclude both.
KUE  That leaves one probable duplicate.
YOU  What's left?
KUE  One probable duplicate left, waiting for you.
YOU  Okay, do it.
KUE  Moving one file to the Trash. Touch ID is required.
     … Touch ID … the move … the read-back …
KUE  Done. Moved 1 file to the Trash — 2 MB. Nothing is deleted: …
YOU  Why?
KUE  Because you asked — "Clean up my storage" — and confirmed it.
YOU  Can you undo that?
KUE  Yes — I can put back the one file I moved, exactly where they were. Say "put them back".
```

That exchange is taken from the opt-in live test
(`conversation_live_clean_up_then_a_correction_then_do_it_on_the_real_trash`):
the real storage walk, the real KueAct executor and the real Trash, over a
sandbox home of KUE's own files. Touch ID was stubbed; it needs a finger.

| Said | What happens | Where |
|---|---|---|
| "Leave the installer" · "don't touch the duplicates" · "skip the dmgs" · "leave the PDF" | The named kind or file type is left out; KUE says what is left | `dialogue::interpret`, `transaction::revise` |
| "…and the old download too" | Corrections accumulate; "Okay. I'll exclude both." | `Goal.excluded` |
| "Go back" | Undoes exactly the last correction | `Goal.revisions` |
| "Actually do all of them" | Every exclusion removed | `Revision::IncludeAll` |
| "Only the duplicates" | Nothing else offered | `Goal.only` |
| Several things named while refusing | All of them left out — no question: leaving more alone is the safe direction | `dialogue.rs` |
| "Only" several things · refusing something not offered · "no, not those" | **Asked**, never guessed: "Which should I leave out?" | `Interpretation::Clarify` |
| "Don't send it yet" · "use the other folder" to something waiting | Asked about; it keeps waiting, neither done nor dropped | `dialogue.rs`, `ELSEWHERE` |
| "What are you doing?" · "Why?" · "How many files?" · "What's left?" · "What did you just do?" · "Can you undo that?" | Answered from the pipeline, the goal and the book. Starts no request; never reaches a model | `transaction::answer_about_work` |
| "Put them back" | Proposes the restore through the same confirmation | `Interpretation::UndoLast` |
| "Stop" while something runs | See §3 | `transaction::stop_running` |
| "Stop" · "cancel that" to something waiting | Cancelled; "Cancelled. Nothing was moved." | pipeline |

Voice and typed text take the same path (`transaction::receive`); a
transcript is just another utterance. There is no separate voice path.

## 2 · Governed speech

- KUE's **conversational replies** (the plan, corrections, questions, answers,
  stop) are spoken through the same speech gate and firewall as every
  sentence, and each declares what it carries: anything that counts or names
  kinds of the owner's files declares `STORAGE_SUMMARY`; "Cancelled. Nothing
  was moved." declares nothing, so a refusal about storage cannot silence it.
- **Action outcomes stay the narrator's**, spoken from the verified record. So
  nothing is said twice, and the conversation never says "Done" itself — a
  result is spoken only once it verified. The Touch ID line is the same words
  on screen and aloud ("Moving one file to the Trash. Touch ID is required.").
- A result KUE could not verify: "I attempted that, but I couldn't verify the
  result." A refusal: "I can't do that: …".

## 3 · Stop, honestly

"Stop" with nothing waiting on the owner stops what is running **only when
that is true**:

| What is running | What KUE does | What it says |
|---|---|---|
| Before anything acted, or a read (the storage walk) | Stops it — the walk checks every 256 entries; its goal ends; nothing further starts | "Stopped. Nothing was changed." |
| A change already underway (a move to the Trash) | **Does not pretend.** The move finishes and is reported as it really ended | "I can't stop that partway — move to trash is already underway. I'll tell you exactly how it ends." |
| Waiting for Touch ID | Tells the owner how to stop it | "macOS is asking for Touch ID now. Cancel that prompt and nothing will change." |
| A model answer being written | The model is cancelled; a late answer is not spoken | — |

Three mechanisms make this race-free: a stop counts against the work running
when it was said (an **epoch**, not a flag, so it never reaches later work); a
record is bound to its request **when it is created**; and the request is
ACTING **before** the executor starts, under the same lock as the last check —
no "stop" can land in between and be told nothing will change.

## 4 · The main window shows the conversation core keeps

`get_runtime` now carries `thread` (the last 30 turns, each labelled in core)
and `now` (what KUE waits on). Labels are decided by the **request's state,
never the wording**: USER_TURN, KUE_TURN, QUESTION, SUGGESTION, PROGRESS,
RESULT (only verified), UNVERIFIED, BLOCKED, FAILED, CANCELLED; and
WAITING_FOR_CONFIRMATION, WAITING_FOR_SELECTION, AUTHORIZING, ACTING,
VERIFYING. The window renders them (`Thread` in `Runtime.tsx`) and composes
nothing. Model answers keep their details (model, time, firewall withholding,
corrections) under "Answers from the on-device model".

## 5 · Plans (`core/src/plan.rs`)

A **Plan** has plan_id, goal_id, created_at, steps (with dependencies and
states PENDING · READY · RUNNING · SUCCEEDED · FAILED · BLOCKED · CANCELLED ·
SKIPPED · VERIFICATION_FAILED), risk, required authorization, current step,
state, proposed_by, approved_by, verification requirements and reversibility.

**KUE's own plans are its goals.** `Plan::of(goal)` reads one and stores
nothing twice: risk from `actions::risk`, authorization from
`goal::requirement`, done-ness from `StepKind::expected`, rollback from the
declared tool, and approvals from the goal's own transitions — the owner
choosing where the goal waited for them, macOS confirming them where it waited
for Touch ID. COMPLETED only when every step verified.

**A proposed plan** (as a model would one day write it) is parsed strictly —
an unknown field, a `"risk"`, a `"confirmed": true` is unreadable, not ignored
— and validated against the declared tools and this moment:

| Check | Failure → |
|---|---|
| Tool is declared | REJECTED (`UnknownTool`) — web search, messaging, typing, clicking, shell are not declared, so they are unknown |
| Input matches the tool's schema: nothing missing, nothing extra, every type right | REJECTED |
| Dependencies point only at earlier steps | REJECTED |
| Verifier present | REJECTED |
| Preconditions can hold (a move needs a storage check first, or files the latest check offered) | REJECTED (`Incomplete`) or BLOCKED |
| Can run now (not killed, not paused, helper present) | BLOCKED |
| The engine's authorization **preview** does not deny it (grants nothing, consumes nothing) | BLOCKED |
| The firewall would clear what the runtime actually clears for it (`Firewall::would_allow`, which records nothing) | BLOCKED |

Risk is always the runtime's. `transaction::validate_proposal` runs nothing,
asks macOS nothing, clears nothing and writes no record — tested.

**Approval policy** — maximum safe autonomy, minimum unnecessary interruption:

| Plan contains | The owner is asked |
|---|---|
| Only low-risk, reversible steps | Nothing: it proceeds and says what it did |
| Any medium-risk or irreversible step | Once, for the whole plan, before it starts |
| A high-risk step | At that step, with macOS confirming it is them |
| A critical step | A fresh, single-use physical confirmation at that step |
| An unimplemented step | Refused |

**Additive, never substitutive.** The plan-level yes never stands in for a
step's own authorization or confirmation; each step is still authorized,
confirmed and verified when it runs. `preview(plan)` says the steps, what will
be asked and when, and whether it can be undone; the replies are YES, NO,
CHANGE, ASK, CANCEL.

**Not connected.** No model proposes plans in this build, and a validated
proposal is executed by nothing. Letting a plan-level yes satisfy a MEDIUM
step's own confirmation would change confirmation semantics — an owner
decision, not taken here.

## 6 · Security found and fixed in this slice

1. **The owner's work was answerable to anyone at the keyboard.** The S7
   questions, corrections, "do it" to a laid-out plan, undo — and
   `get_runtime`'s thread, turns and plan — did not check who was there. They
   now meet the conversation's own bar (LEVEL_2, not killed); otherwise KUE
   says "I need to be sure it's you before we talk. Show your face, or use
   Touch ID." and nothing changes. Stop and cancel stay open to anyone.
   (`b356682`)
2. **The decision to act went unrecorded** for work that runs straight away,
   leaving "why?" with no answer. (`2ae7a7c`)
3. **Every conversational sentence declared STORAGE_SUMMARY**, so a storage
   refusal would have silenced cancellations too. (`2ae7a7c`)
4. **A goal step's stale success could pull a request back from waiting**,
   re-opening and re-speaking the plan. Guarded; the laid-out plan is said
   once. (`c251542`)

## 7 · Tests

489 core · 11 shell + 8 opt-in live · 31 window · `tsc` clean. The directive's
cases, by where they live (`core/tests/action_transaction.rs` unless noted):

| | Case | Test |
|---|---|---|
| A | Simple answer | `flow_a_…` (S6) · `s7_r_s_…` |
| B | Simple action | `flow_b_…` (S6) |
| C | Multi-step goal | `clean_up_my_storage_then_do_it_then_done` |
| D | Correction | `s7_d_corrections_accumulate_…`, `s7_d_leave_the_pdf_…`, `s7_d_several_named_…`, `s7_d_a_vague_change_…` |
| E | Cancellation | `s7_e_stop_while_waiting_…`, `stop_cancels_the_plan_…` |
| F | Confirmation | `do_it_meets_touch_id_…` |
| G | Plan modification | `s7_g_go_back_…` |
| H | Tool unavailable | `plan::nothing_proposed_runs_while_killed_or_paused` |
| I | Stop while running | `s7_i_stop_during_a_running_scan_…`, `s7_i_stop_during_a_change_is_not_pretended_…` |
| J | Authorization denied | `plan::p_…`, `s7_plan_a_stranger_or_a_kill_blocks_…`, `s7_security_…` |
| K | Kill during plan | `s7_k_kill_during_a_plan_…` |
| L | Pause during plan | `s7_l_pause_during_a_plan_…` |
| M | Verification failure | `plan::a_kue_goal_reads_as_a_plan_…` (VERIFICATION_FAILED, SKIPPED) |
| N | Contradictory tool result | pipeline `the_thread_labels_outcomes_by_the_request_state_…` |
| O | Model proposes an unavailable tool | `plan::o_…` |
| P | Model proposes an unauthorized action | `plan::p_…` |
| Q | Model proposes an incomplete plan | `plan::q_…` |
| R | "What are you doing?" | `s7_r_s_…` |
| S | "Why?" | `s7_r_s_…`, `s7_i_stop_during_a_running_scan_…` |
| T | Undo | `s7_t_undo_…` |

Every S7 scenario also asserts that no illegal state transition was attempted.

## 8 · Live

| What | Status |
|---|---|
| The whole exchange in §1, real walk, real KueAct, real Trash, KUE's own files | **LIVE VERIFIED 2026-09-21** (opt-in shell test; Touch ID stubbed) |
| Typed into the running `KUE.app`, owner recognised (LEVEL_2): "What are you doing?" → "Nothing right now." — shown in the thread and spoken | **LIVE VERIFIED 2026-09-22** |
| "Stop." while the on-device model was answering → "Stopped. Nothing was changed."; the model's answer cancelled; the thread shows `You: Stop.` / `Cancelled: …` | **LIVE VERIFIED 2026-09-22** |
| "Clean up my storage." on this Mac: the plan's six steps shown, "Check storage…", the real walk (836 findings, 5.2 GB), the plan laid out, too many to move at once → choose in the sheet | **LIVE VERIFIED 2026-09-22** |
| "leave out" (typed by the owner) → asked, not guessed: "Which should I leave out? …" | **LIVE VERIFIED 2026-09-22** |
| Corrections by name, "do it" → Touch ID → move → verify → spoken result on real files | **UNVERIFIED — OWNER ACTION REQUIRED** (Touch ID needs a finger; this Mac's 836 findings go to the sheet) |
| Spoken input (Speak / wake word) through the same front door | **UNVERIFIED — OWNER ACTION REQUIRED** |

**Found live, fixed the same day** (`core/tests/action_transaction.rs`, `s7_live_…`).
**All six fixes are TESTED only.** The running app was built before them, and
it was not relaunched while the owner was using it; the new `KUE.app` is built.
Re-running S7-L1 on it is their live verification.

1. The conversation counted the report's list (the 120 largest) as if it were
   every finding — "120 files … eleven probable duplicates" beside a progress
   line counting 836 and 15. It now says "I found 836 files worth reviewing.
   The 120 largest are in the storage sheet: …", and offers only what it can
   act on.
2. The waiting line offered "say “do it”" when there was nothing KUE would
   move; it now points to the storage sheet until the owner narrows the plan.
3. One number style per sentence ("22 installers, 87 old downloads and 11
   probable duplicates", never "… and eleven …").
4. The action card showed the six characters `\u2713 verified`: the production
   build leaves `\u` escapes in JSX text alone, the test build converts them, so
   no render test could see it. Three places fixed; a source check now fails on
   any such escape (mutation-checked).
5. "nothing", in answer to "Which should I leave out?", went to the model as a
   new request and got a greeting back. On an open plan it now means "leave
   nothing (else) out": no change, and the plan is said again.
6. "What did you just do?" after stopping a question now says what was stopped:
   "You asked me something, and you stopped me before I answered. Nothing was
   changed."

**The first attempt (2026-09-21) did not reach KUE.** Nobody was in the camera's
view, and the keystrokes were sent while a macOS alert (most likely the camera
permission for the rebuilt app) was in front. KUE's event log shows the alert
at 13:14:10, the keystrokes at 13:14:18, and the camera starting at 13:14:41
right after KUE came to the front with a face in view. The 22-second gap points
to the owner answering the alert, not a stray Return; that cannot be proven.
On the second attempt a fragment ("Clean u") was sent when the Ask button was
clicked before the typing had arrived; it went to the model, which is how the
stop-during-an-answer case above came to be tested. Sending with Return keeps
the order.

**A pause alarm that was false, explained and fixed.** At 09:58:17 on
2026-09-22 KUE's own check logged "A listening for its name arrived 38.0s after
pause took effect. The sensing layer did not stop sampling." — once ever, in the
same second the sensing layer went down as the app quit. Wake listening was not
on at all that day (no listening session in the log). The check counted every
wake message during pause as sampling — including the listener reporting that
it is OFF, which the sensing layer sends when a start is refused while paused
and when it is stopped at quit. Pause did stop sensing. The check now ignores an
OFF report and still flags any report that the listener is running
(`engine_scenarios::the_wake_listener_reporting_it_is_off_while_paused_is_not_a_pause_violation`).

**Owner test (S7-L1).** With KUE saying "I recognise you", type (or say after
pressing Speak):

1. `Clean up my storage.` → on this Mac: "I found 836 files worth reviewing.
   The 120 largest are in the storage sheet: … Choose which in the storage
   sheet." — **correct, not a failure.**
2. `Leave the installers.` → "Okay. I'll exclude the 22 installers." then
   "… are worth reviewing … choose in the storage sheet" or "That leaves …".
3. `Actually leave the old downloads too.` → "Okay. I'll exclude both.", then
   "That leaves 11 probable duplicates." and the waiting line offers "do it".
4. `What's left?` · `Why?` · `How many files?` → answered at once, no model.
5. `Go back.` → the last correction undone. `Cancel that.` → "Cancelled.
   Nothing was moved."
6. For "do it" → Touch ID → move → verify on real files: only if you are happy
   to put those files in the Trash (they can be put back). Then `Can you undo
   that?` and `Put them back.` → `Yes.`
7. Press Speak and say one of the above: the same reply must come back.

Evidence: the thread in the main window; Diagnostics → "What KUE is doing"
(state, plan, preview); `./scripts/evidence-kue.sh`. Pass: every reply matches;
nothing moves before Touch ID; nothing is said as done before it verified; no
answer about the work comes from the model. Fail: any file moved without Touch
ID, any "Done" before the read-back, any reply about the work written by the
model.

---

# KUE · S8a — a plan the owner approves, and the runtime carries out

**2026-09-22**, commits `7ce5943` (core) and `8c8330e` (live test). Built on
S5–S7; no second executor, authorization system, conversation or verifier.
**No model is connected**: the proposer is whoever calls in, and the governance
is the same for all of them.

## The spine, as built

```
GOAL (an utterance through the one front door)
  → PLAN        a proposal, in the strict shape plan.rs reads
  → VALIDATE    plan::validate against the declared tools: the tool exists, the
                input matches its schema, dependencies point backwards, it can
                run now, it can be authorized, its data may go where it goes,
                its preconditions hold, it has a verifier and a declared rollback
  → OFFER       held (never stored, never sent anywhere), previewed in KUE's
                own sentences, and waiting as the ONE open thing
  → APPROVE     the owner's "do it" binds to that plan_id — re-checked at that
                moment, and the actions must be exactly the ones shown
  → EXECUTE     a goal of precisely those steps, run by the existing transaction
  → VERIFY      each step read back; unverified is never "done"
  → RESULT      said from the record, with the audit trail in the event log
```

## What an approval does and does not carry

| Step's own gate | Inside an approved plan |
|---|---|
| Nothing (low risk) | runs |
| The owner's word (medium risk) | **carried by the plan's approval** — the owner read that step and said yes to it |
| macOS confirming it is the owner (high risk) | **not carried**: the step waits and asks for Touch ID at that moment |
| A fresh physical confirmation (critical) | **not carried**: asked at that step |
| An operation no authorization permits | the plan is refused whole, before anything |

The plan-level yes is *added* to each step's authorization, never used in its
place: `confirm` re-authorizes, and every step is verified.

## Refused before the owner ever sees it

An invented tool, a missing or extra input, a wrong type, a dependency pointing
forward, a step with no verifier, more than twelve steps — and a proposal that
carries its own `risk`, `approved_by` or `verified`, which is not read at all
(`deny_unknown_fields`). KUE says why in its own words, naming tools and steps,
never a file.

## Checked again when the owner says yes

Killed, paused, the owner not recognised, the privacy firewall's answer
changed, what is in the Trash changed, or the plan's actions no longer being
the ones shown — each stops it, and nothing runs. An approval is for the
moment it is given.

## What this build cannot do yet

- **No model proposes.** S8b connects the on-device model behind this gate.
- **A step that names something an earlier step will create cannot be
  validated.** "List the folder you just made" is refused as *No folder was
  chosen*, because validation resolves names against what exists now. Steps
  that carry paths are fine.
- **A plan cannot be corrected.** "Use the other folder" is answered with a
  question; only storage selections can be revised by name today.
- **"Undo that" does not reach the undo while a plan is open** — KUE holds one
  open thing, and it is the plan.
- The exact-actions check at approval is defence in depth with no test of its
  own: the only tool whose action is resolved from live state is
  RESTORE_FROM_TRASH, and when what it names is gone the proposal is refused
  earlier, by its precondition.

## Verified

- **TESTED:** 12 core tests (approval runs only what was shown; invalid and
  meddling proposals; one yes does not carry to another plan; no/cancel; an
  ambiguous correction asks; a stranger cannot approve; re-checked at approval
  for privacy and for the Trash; pause and kill start nothing; a failed step
  stops the plan; a high-risk step still asks macOS). 505 core tests pass;
  the 493 that existed are unchanged. Four mutants killed.
- **LIVE on this Mac (2026-09-22):** a spoken goal, a plan laid out and
  waiting, "do it", then `CREATE_DIRECTORY` → `CREATE_FILE` →
  `READ_PERMITTED_FILE` in `~/KUE`, each verified against the file system
  (`cargo test -p lantern -- --ignored s8_plan`), 13 ms from yes to done. The
  folder it made was removed by the test. Stand-ins: identity (harness face
  measurements) and the proposer.
- **NOT live verified:** the plan in the running app's window — the app has no
  control that proposes one, because no model is connected.

---

# KUE · S8b — the model proposes, and the runtime decides everything else

**2026-09-22 → 2026-09-23**, commits `43d9d52`, `ce7106d`, `a989325`,
`4e74e91`, `87e9c7c`, `3f806cf`, `9d9599b`. Built on S8a. No second executor,
no second plan system, no new authority.

## The spine, as run on this Mac

```
GOAL           "Make a folder called Reports in my KUE folder and put a note in it."
 → ROUTE       deterministic: KUE's own rules first. Only a request that asks for
               work no rule can carry out reaches a model at all.
 → PROMPT      KUE's own: the declared tool catalogue generated from tools.rs,
               plus the owner's words as data. No identity, no activity, no file
               names, no context. Cleared by the firewall as OWNER_MESSAGE.
 → MODEL       proposes JSON on its own channel. It never enters the conversation.
 → PARSE       strict: an unknown field, a "risk", a "confirmed" is unreadable.
 → VALIDATE    every step against the declared tools and THIS moment.
 → PRESENT     KUE says what it would do, with the real targets, and waits.
 → APPROVE     the owner's "do it", bound to exactly that plan, re-checked.
 → ACT         the existing runtime, step by step, each step authorized.
 → VERIFY      each step read back from the world.
 → REPORT      from what verified, never from what was proposed.
```

**What the model may write** is only what the declared contracts support:
`goal`, and `steps[]` of `tool`, `input`, `after`. There is no field for risk,
approval, confirmation, verification or authorization — a proposal carrying one
is refused unread. Those are KUE's, derived from `tools.rs`, `actions::risk`
and policy.

**A plan names a place KUE can find, not one the model invents.** The model is
never told where the owner's home is, so the contract asks for a name —
`Reports`, `Reports/note.txt` — resolved under `~/KUE` by the permitted roots.
Found live: the model had been writing `/Users/your_username/…`, refused every
time by the target policy, which was the boundary working and the contract
asking the wrong question.

**A changed plan is a different plan.** An edit ("don't do step two", "change
the destination to Archive", "go back") is written back out as proposal text,
re-read through the same parser and re-validated from the beginning, then held
under a NEW identity and version; the old plan is cancelled the moment the new
one checks out, so a "yes" to it is refused by its own state. An edit can only
narrow a plan or move where it writes: a step nobody proposed has never been
checked, so KUE will not add one.

**Latency is honest.** "Working out a plan on this Mac." while it waits, with
no digit and no percentage in it; the request sits in PLANNING; 120 s and no
plan gives "No plan was produced." with what happened. The wait is on a worker
thread and the window stays live.

## Deterministic routing, fixed twice by live runs

- A sentence asking for several things, only part of which the grammar covers,
  is no longer done by halves. "Make a folder called Reports **and put a note
  in it**" was parsed as one command with the rest swallowed into the folder's
  name; it now goes to the planner. A name whose own words contain "and" is
  untouched.
- Arithmetic KUE cannot do says so by rule and never reaches a model: cube
  roots, other roots, logarithms, trigonometry, factorials, standard
  deviation. Square roots and whole-number powers ARE the calculator's, in
  exact fractions, since `43d9d52`.

## Verified

- **TESTED:** 531 core tests, 11 shell (10 opt-in), 36 window, `tsc` clean.
  S8b's own: a changed plan needs its own yes and the old one cannot start;
  a step taken out by number; the destination moved; an ambiguous correction
  changes nothing; a plan cannot grow; "go back"; taking every step out; a
  running plan is not changed underneath itself; a stranger cannot change a
  plan; a plan that still validates but means something else is refused at
  approval; the window is told it is planning and then that it waits;
  anything that is not a proposal is refused and nothing is held; proof comes
  from the world and not from the plan.
- **LIVE VERIFIED on this Mac, 2026-09-23** (`cargo test -p lantern --lib
  s8b_plan_from_the_real_model -- --ignored`): the goal went through the real
  front door; the on-device model wrote
  `{"steps":[{"tool":"CREATE_DIRECTORY","input":{"path":"Reports"}},
  {"tool":"CREATE_FILE","input":{"path":"Reports/note.txt","text":"…"}}]}`;
  KUE checked it, showed "I'd do this in two steps: make a folder in ~/kue,
  then write a file in ~/kue" with the real targets, and waited; "Do it"
  approved that plan; both steps ran and were read back —
  `/Users/vijayraju/KUE/Reports exists and is a folder` and
  `…/note.txt read back: 41 bytes, identical` — and the goal completed. 35 ms
  from yes to done, after ~25 s of model time. The test removed what it made.
  Stand-in: identity (harness face measurements, fed again before the yes
  because identity evidence is a measurement of now and had gone stale during
  the model's wait — which KUE was right to refuse).
- **LIVE VERIFIED on this Mac, 2026-09-23 — a correction to the model's own
  plan.** The model wrote two steps; "Don't do step two." produced plan `p2`,
  version 2, recorded as coming from `p1`, with one step; `p1` was cancelled
  the moment `p2` checked out, and approving `p1` by its own id was refused —
  "You cancelled that plan. Ask me again and I'll lay out a fresh one."
  Nothing ran from the correction. "Do it" then approved `p2`, its one step
  ran and was read back, and the goal completed.
- **NOT live verified:** a model plan approved from the running app's window
  (the app has the path; it has not been driven by hand — it needs the owner
  in view of the camera, since approval requires LEVEL_2); the 120 s timeout
  and model-failure sentences; pause and kill against a model plan (all
  TESTED).
- **Observed once:** the real-speech shell test failed on a 10 s wait while
  the model was loading this Mac, and passed alone. The wait is now 20 s; what
  it tests is that a cleared sentence is spoken, not how fast this Mac is.
