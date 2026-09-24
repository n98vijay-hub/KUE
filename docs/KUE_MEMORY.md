# KUE · S9 — personal memory

**2026-09-23**, commits `d34f593`, `6c6ae0d`, `a38bb28` and this one. Built on
S5–S8b. No second memory system: the states and sources are `facts.rs`'s, the
privacy firewall decides every write, and the pump that already refuses to
write while killed writes these too.

Memory here is not the conversation and not a log. It is what is worth keeping
**after a run ends**, in classes a person would recognise, each carrying the
reason it exists.

## What KUE keeps, and why each class exists

| Class | Why it exists | Example |
|---|---|---|
| `PREFERENCE` | How the owner likes things done. Consulted before KUE proposes; replaced when they change their mind | "You prefer PDF reports" |
| `FACT` | Something about their world they told KUE. Never inferred into existence | "You work on the SAP project" |
| `GOAL` | Something they are working towards that outlives one request | "You're preparing for interviews" |
| `WORK` | What KUE did and read back. The only class that may be VERIFIED | "KUE carried out a run commands plan of 2 steps: create directory, create file" |
| `DECISION` | So "why did you do that?" has an answer later | "You approved a plan of 2 steps" |
| `TASK_NOTE` | True of one task, not beyond it. Expires after four hours | "For this clean-up, you asked me to leave out the installer" |

`RECURRING_PATTERN` and a speculative `USER_PROFILE` were **deliberately
excluded**: both would be KUE inferring things about a person from their
behaviour, and neither has an evidence rule good enough to stand behind yet.

## Lifecycle

```
CANDIDATE ──(only the owner's own words)──▶ CONFIRMED ──┐
     ▲                                                   ├─▶ SUPERSEDED (a later memory replaced it)
     │ a model proposing                                 ├─▶ STALE (past what it was good for)
     │                                                   └─▶ DELETED (the owner forgot it; the words go)
  VERIFIED ◀──(a read-back from the action pipeline)──── WORK only
```

- **CANDIDATE** is never returned as current, never acted on, never spoken as
  something the owner said.
- **VERIFIED** needs a `facts::Verification` — the token the executor's
  read-back produces — and a constructor only this crate can call.
- **SUPERSEDED** keeps the old words readable as history; retrieval never
  offers them as current.
- **DELETED** is a tombstone: the statement, subject and provenance are
  emptied in the index and the row is removed from the store, with the
  write-ahead log checkpointed so the words are not left there either
  (`secure_delete` is already on). What remains is that something was
  forgotten, and when.

## Governance

| Rule | Where |
|---|---|
| Nothing is kept without a governed reason (`Why`: the owner said so, they corrected KUE, they decided, KUE verified, a model proposed) | no constructor without one |
| A model cannot make a memory true | `proposed_by_model` takes no state, and always yields CANDIDATE |
| A model cannot mark work done | `Memory::verified` needs the action pipeline's `Verification`, and is `pub(crate)` |
| Nothing is overwritten silently | a preference that disagrees is **asked** about; the owner is told how to replace it |
| Memory is the owner's | adding, asking and forgetting need LEVEL_2, like the conversation |
| A killed KUE keeps nothing, a paused KUE keeps nothing new | `Engine::memory_refusal`, and the pump returns before writing while killed |
| Privacy decides every write | `Firewall::clear_memory` by the memory's own `DataKind` |

**What privacy means in practice:** the owner's words are `OWNER_MESSAGE`,
which policy allows to stay on this Mac. A memory naming one of their files
would be `ACTION_TARGET`, which policy allows KUE to *show* them and **not** to
store — so what KUE keeps about its own work names the tools and the number of
steps, never the files. The read-back proves the memory may exist; it is not
kept inside it. A store test holds this.

## In the conversation, by rule, with no model

| Said | KUE |
|---|---|
| "Remember that I prefer PDF reports." | "I'll remember that you prefer PDF reports." |
| "What do you remember about my report preferences?" | "You prefer PDF reports." |
| "Why do you remember that?" | "On 2026-09-23, you said: “I prefer PDF reports”." — or, with nothing recorded, exactly that |
| "Remember that I prefer DOCX reports." | "You told me before that you prefer PDF reports. If that's changed, say “remember that I prefer DOCX reports instead” and I'll replace it." |
| "…instead." | replaced; the old one becomes SUPERSEDED |
| "Forget that I prefer PDF reports." | "Forgotten: … It's gone from what I keep." |
| "Forget that I work." (fits two) | names both and asks which — KUE does not choose what to destroy |
| A question KUE has been told the answer to | "From what you've told me: …" — **memory comes before a model** |

## Retrieval

By the words a question and a memory share, best first — never by asking a
model what is relevant. Candidates, superseded, stale and deleted memories are
not returned. When one memory fits best, that is the one; a tie is a question.

**To a model**, when one is asked at all: at most five memories, chosen by the
same matching, each labelled with its state ("[CONFIRMED the owner told KUE]
…"). The store is never handed over.

## On screen

"What KUE remembers" lists what is kept under core's own headings, each with
where it came from and when, and a **Forget** beside it. A memory with no
recorded reason says so rather than being given one. What is no longer current
is counted, never read back as current. Someone KUE is not sure of sees none
of it.

## Verified

- **TEST VERIFIED:** 552 core tests (21 new), 11 shell (12 opt-in), 38 window,
  `tsc` clean. Including: a model's words never become memory; only the action
  pipeline can make one VERIFIED; a disagreement is asked about; forgetting
  removes the words; a task note expires; a stranger reaches none of it; a
  killed or paused KUE keeps nothing; what policy will not keep is not written;
  bounded growth drops what is no longer current first.
- **LIVE VERIFIED on this Mac, 2026-09-23:**
  - **Across two processes** (`s9_memory_on_this_mac_survives_the_process_that_made_it`):
    one process wrote "You prefer PDF reports" through the real firewall and
    pump into a real SQLite file; a second process — the test binary run again
    — opened that file, answered "You prefer PDF reports.", gave the
    provenance "On 2026-09-23, you said: “I prefer PDF reports”", forgot it on
    request, and the row was gone from the file.
  - **With the real on-device model** (`s8b_plan_from_the_real_model`): after
    the model proposed a plan, the owner corrected it and approved it, and KUE
    kept exactly two memories — `Decision Confirmed Owner — You approved a
    plan of 1 step: create directory` and `Work Verified Action — KUE carried
    out a run commands plan of 1 step: create directory`. Nothing from the
    model, and no file named in either.
  - **On real files** (`conversation_live_clean_up_…`): "leave the installer"
    and "leave the old download" became TASK_NOTEs that expire in four hours,
    and no preference.
- **UNVERIFIED:** memory driven from the running app's window (the sheet, the
  Forget button, `get_memories`); memory across a real app restart (the
  mechanism is live verified across two processes, but not yet by quitting and
  reopening `KUE.app`); a memory a model proposes reaching the owner as a
  question — there is no such flow, by design.
- **NOT IMPLEMENTED:** asking about the past out of the event log ("what did I
  do yesterday") — still `memory_recall`, still MISSING, and it says so.
- **DELIBERATELY EXCLUDED:** inferred patterns, a speculative profile, cloud
  or external-model memory, and any path by which a model writes, deletes or
  verifies a memory.

## Owner test (S9-L1)

With KUE running and recognising you:

1. `Remember that I prefer PDF reports.` → "I'll remember that you prefer PDF
   reports."
2. Open **What I remember** → it is listed under "How you like things done",
   with "2026-09-23, you said: …" beneath it.
3. `What do you remember about my report preferences?` → the memory.
4. `Why do you remember that?` → the date and your words.
5. Quit KUE, reopen it, and ask again → it is still there.
6. `Remember that I prefer DOCX reports.` → KUE asks rather than overwriting.
   Then `Remember that I prefer DOCX reports instead.` → replaced.
7. Press **Forget** in the sheet, or say `Forget that I prefer DOCX reports.`
   → gone, and still gone after a restart.

Pass: every reply matches, nothing is remembered you did not ask for, and
"why" always answers with something real. Fail: a memory with an invented
reason, a forgotten memory that comes back, or anything kept while KUE is
paused or stopped.

---

# KUE · S10 — memory in use

**2026-09-24**, commits `1d913d0`, `65e2b31` and this one. Built on S9. No
second retrieval system, no second planner: the same `MemoryBook::matching`,
the same goal, the same approval.

Memory stops being a drawer. What the owner said once changes what KUE
proposes next — and **nothing else**.

## The loop, as it ran on this Mac

```
YOU  Remember that I don't want installers included when cleaning my storage.
KUE  I'll remember that you don't want installers included when cleaning your storage.
      … later, a different request, no mention of installers …
YOU  Clean up my storage.
KUE  I found two files worth reviewing: one old download and one probable
     duplicate. I left out the installer, because you told me: "You don't want
     installers included when cleaning your storage". I can move them to the
     Trash — say "do it", or tell me what to leave out.
YOU  Why?
KUE  Because you told me: "…". That's why I left installers out of it.
YOU  Do it.
     … Touch ID … the move … the read-back …
KUE  Done. Moved 2 files to the Trash.
YOU  Actually, include installers in storage cleanup from now on.
KUE  I'll remember to include installers in storage cleanup from now on. That
     replaces what you told me before.
YOU  Clean up my storage.
KUE  I found one file worth reviewing: one installer. …
```

## Where memory enters, and where it does not

| Stage | Memory's part |
|---|---|
| Retrieval | `MemoryBook::matching` on the owner's own words — the same retrieval S9 built |
| **Proposal** | **the only thing memory changes.** A CONFIRMED PREFERENCE that reads as a wish about cleaning up seeds the goal's exclusions before anything is offered |
| Explanation | KUE says what it left out and quotes the sentence it came from |
| Approval | unchanged — the owner's yes, at LEVEL_2 |
| Authorization | unchanged — every step, when it runs; Touch ID where the risk says so |
| Execution | unchanged |
| Verification | unchanged — the read-back, never the memory |
| Outcome | the finished plan becomes a VERIFIED WORK memory, as in S9 |

**Memory is context, never authority.** A memory saying "I always approve
cleanups" changes nothing about approval; a test holds that, and a stranger
cannot add one.

## The reader

`dialogue::storage_wish` turns one remembered sentence into a wish about
cleaning up, and only when all three hold: it names a kind of file KUE's
clean-up knows (installers, duplicates, old downloads, large files), it says
plainly whether to leave that kind out or put it in, and it is about cleaning
at all. Everything else reads as nothing — which is the right answer for
almost every sentence, including "You prefer PDF reports".

`dialogue::is_standing` tells "from now on" apart from "for this one":
"include installers from now on" is a preference and is offered to memory as a
replacement; "leave the installers" during a clean-up is a correction to that
clean-up and stays a TASK_NOTE (S9).

## Authority

The classes and states S9 already has ARE the authority levels; no new ones
were invented.

| What | May it change a proposal? |
|---|---|
| CONFIRMED PREFERENCE that reads as a clean-up wish | **yes** — and it is recorded on the goal |
| CONFIRMED FACT / GOAL | no (S10 uses them as context for a model only) |
| VERIFIED WORK, DECISION | no — they explain the past |
| TASK_NOTE | no — it belonged to its own task |
| CANDIDATE, SUPERSEDED, STALE, DELETED | never retrieved at all |

## Conflict

Two saved preferences that disagree about the same kind of file: KUE applies
**neither**, says so, and asks which holds. It does not choose.

Worth being exact about how one arises. Face to face it cannot: S9 already
asks at the moment the second is said, because both must name the same kind of
file to be read as wishes about it, and that is what S9's write-time check
looks for. It arises when two such rows arrive **together from the store** — a
restart carrying rows written by an older build, or before that rule existed.
That is the path the test uses, and the handling is defence in depth.

## What the model gets

At most three of the owner's sentences that share words with the goal,
labelled as theirs, in the planning prompt. Never the store. The model cannot
name a memory: which memories shaped a plan is attached by KUE
(`HeldPlan::memory_refs`, `Goal::memory_refs`), and the proposal parser
accepts no unknown fields at all, so a model writing `memory_refs` is refused
unread.

## On screen

Over a plan: "KUE used one thing you told it", opening to their own sentence.
No ids. The memory itself is in "What I remember", with its provenance and a
Forget beside it.

## Verified

- **TEST VERIFIED:** 560 core tests (8 new for S10), 11 shell + 13 opt-in, 40
  window, `tsc` clean. Including: the preference changes the plan without
  being repeated; an unrelated preference stays out; two that disagree are
  asked about and neither applied; a changed preference changes the next plan;
  memory approves nothing and a stranger reaches none of it; the reader reads
  only what it should.
- **LIVE VERIFIED on this Mac, 2026-09-24**
  (`cargo test -p lantern --lib s10_memory_changes -- --ignored`): the whole
  loop above over real files, with the real storage walk, the real KueAct
  executor and the real Trash. The installer the preference protected was
  still on disk after the move; the report-format preference never entered the
  clean-up; the move verified by read-back ("moved 2 of 2 to the Trash, each
  checked"); the finished plan became a VERIFIED WORK memory naming tools and
  no files; the changed preference put installers back in the next plan.
  Touch ID was the one stand-in, as in every live test that moves a file.
- **LIVE VERIFIED, 2026-09-24** (`s8b_plan_from_the_real_model`, with a
  preference in memory): the owner's sentence reached the planning prompt under
  "WHAT THE OWNER HAS TOLD KUE BEFORE", and the held plan recorded which
  memory was put in front of the model (`HeldPlan::memory_refs`), attached by
  KUE. The plan then ran and verified as before. **What is NOT claimed:** that
  the model follows a preference. In that run it wrote one folder step and no
  note, so nothing about obedience was proved — KUE validates whatever comes
  back either way, which is the point.
- **UNVERIFIED:** any of this driven from the running app's window.
- **NOT IMPLEMENTED:** preferences about anything but storage clean-up
  (a wish about file types, folders or report formats is kept and retrieved,
  but nothing consults it yet); learning a preference from repetition.
- **DELIBERATELY EXCLUDED:** inferring a preference from behaviour; letting
  memory approve, authorize, or satisfy Touch ID; embeddings or any vector
  store; cloud memory.
