# Work order S0-07: Ledger schema v0 and Definition of Done adopted

Story: S0-07, Sprint 0, epic E0. Lead role: Business analyst. Evidence type: Document. Owner at the Mac: No.
Requirements linked: FR-101, FR-808. Open decision settled here: C-1 (`docs/product/rules.md`).
Written 2026-10-06 on `kue/release-1`. This work order plans a document. It changes no code.

## 1. In plain words

You will get one short document in plain language. It says what KUE writes down about every piece of work: the request as it was understood (work order), each stage it passes (step), the proof (evidence), and your signed decision (warrant). It explains how that record is protected against later change, and how KUE can still forget something without breaking that protection. That last point is your decision: the document sets out the choices with a recommendation and waits for your answer. It also states the Definition of Done: what must be true before work counts as verified, and before you sign. Once you approve, this sprint's work orders are checked against it. Nothing in the app changes.

## 2. Acceptance checks

Each check passes or fails on its own. "The document" means the new file named in section 3.

1. The document exists, is written in plain language, and a term that is not plain is explained the first time it is used. (story acceptance criteria)
2. The document lists every field of a **work order** record, and for each field says in one line what it holds and who or what writes it. (FR-101)
3. The document lists every field of a **step** record in the same way. The states a step may hold are quoted from FR-105 without change, and the document says they are a different list from the story status values in the backlog. (FR-101)
4. The document lists every field of an **evidence** record in the same way. The result field takes exactly four values: passed, failed, not run, inconclusive. (FR-101, BR-06)
5. The document lists every field of a **warrant** record in the same way, including the mark the owner gives and what exact content the mark is bound to. (FR-101, BR-07)
6. The document states the part that every record has in common, whatever its kind, and shows how each record is tied to the one before it. (FR-101)
7. The document states that a correction is always a new record that points at the old one, and that no record is edited or taken out. (FR-101, BR-08)
8. The document names all seven kinds in FR-101. Work order, step, evidence and warrant are specified in full. Item, promise and answer are named as reserved, carry only the common part, and each names the story that will specify it. (FR-101)
9. The document presents decision C-1 as a question to the owner, with at least two options, what each option costs, and one recommendation. Before the owner answers, the document does not state C-1 as decided. (FR-808, rules.md C-1)
10. For the recommended option, the document lists exactly what is left behind after a forget, field by field, and for each field says why it holds none of the forgotten content. (FR-808)
11. For the recommended option, the document explains in plain words why the integrity check still passes after a forget. (FR-808)
12. The document states the Definition of Done as a list in which every line can pass or fail, and beside each line names the record or file that proves it. (story title)
13. Every example in the document is invented. No real name, amount, date, document or file path of the owner or of any other person appears. (BR-03)
14. The owner's answer to C-1, and to every other question the document asks him, is recorded in the document with the date, in his own words or as "not yet answered". (story acceptance criteria, rules.md C-1) *Waits for the owner.*
15. The owner's approval of the schema and of the Definition of Done is recorded. (story acceptance criteria) *Waits for the owner.*
16. A register exists that lists every Sprint 0 work order present under `docs/work/` on the day of checking, and for each one says, field by field, whether it carries what the schema requires of a work order. (story acceptance criteria)
17. The story changed only the new document and files under `docs/work/S0-07/`. No code, test, configuration, `CLAUDE.md`, `docs/product/rules.md` or `docs/product/backlog.json` was changed by the analyst. (crew rule)

## 3. Steps, by owner

There are no steps for the core builder, the Mac builder or the window builder. No file under `core/`, `src-tauri/`, `sensing/`, `act/`, `auth/`, `mind/`, `voice/`, `scripts/` or `src/` changes.

**Step 1. kue-analyst: write the document as a draft.**
File to create: `docs/KUE_LEDGER_SCHEMA_V0.md` (new; the name follows the other files in `docs/`).
It holds, in this order:
- What the Work Ledger is and is not. It is the new record of work (feature F-01). It is not the existing privacy audit table, which the code also calls a "ledger" and which this story leaves alone.
- A short table of words the code already uses in another sense ("ledger", "evidence", "step"), so the builder of S1-01 does not reuse them by accident.
- The common part of every record, and how records are tied together.
- The four records in full: work order, step, evidence, warrant. One invented example of each.
- The three reserved kinds: item, promise, answer.
- Forgetting and wiping: decision C-1 as a question with options and a recommendation.
- The Definition of Done, each line with what proves it.
- "Questions for the owner": every question from section 8 that is his to answer, each with a recommendation.
- "What the owner decided": empty in the draft.
Commit message, made by the lead in this dry run and by the analyst on the story branch otherwise: `docs(S0-07): Ledger schema v0 and Definition of Done adopted`.

**Step 2. Lead, with the owner in the session: ask the questions.**
The lead shows the owner "Questions for the owner" and takes his answers. The lead passes the answers to the analyst exactly as given.

**Step 3. kue-analyst: record the answers.**
File: `docs/KUE_LEDGER_SCHEMA_V0.md`. Fill "What the owner decided" with each answer and its date. Where an answer differs from the recommendation, revise the affected part of the schema. Nothing is written as decided that the owner did not say.

**Step 4. kue-analyst: write the Sprint 0 register.**
File to create: `docs/work/S0-07/sprint-0-register.md`. One row per Sprint 0 work order found under `docs/work/`, one column per field the schema requires of a work order, each cell "present" or "missing". A missing field is reported, not filled in.

This story has no step that changes `docs/product/rules.md` or `CLAUDE.md`. See section 7.

## 4. Tests to add

None in code. This story's deliverable is a document, and its two requirements are proved in code by later stories:

- FR-101 is proved by tests in S1-01, for example `fr_101_records_are_append_only_and_chained`.
- FR-808 is proved by tests in S10-04, for example `fr_808_forget_leaves_marker_and_integrity_holds`.

Those names are suggestions for the later work orders. They are not added here. In this story FR-101 and FR-808 are proved by acceptance checks 2 to 11, read by the verifier in the document, and by the owner's recorded approval.

No fixture is needed. The examples in the document are invented (check 13).

## 5. Laws at stake

| Law | How this story could break it | How the steps avoid it |
| --- | --- | --- |
| BR-08, append-only | A design for forgetting that edits or removes a record | Check 7. The recommended C-1 option never touches the chained part; forgetting adds a record and erases only the separate content |
| BR-19, leaving is clean | A design that protects the record so well that nothing can be forgotten | Checks 9 to 11. C-1 is put to the owner; the document does not choose for him |
| BR-03, data stays on the Mac | The part that survives a forget still giving away what was forgotten; real data used in an example | Checks 10 and 13. Question 2 in section 8 asks the owner about the one known weak point |
| BR-04, no claim without a record | A Definition of Done that accepts a claim with no record behind it | Check 12: every line names what proves it |
| BR-06, four results | An evidence record with a yes/no result | Check 4 |
| BR-07, a yes is bound to what was seen | A warrant that names a story but not the exact content shown | Check 5 |
| BR-01, BR-10, AS-03 | A record field that a model fills in directly: a state, a number, an approval | Checks 2 to 5: each field says who or what writes it. The document must show no state, number or mark written by a model |
| BR-15, BR-16 | A Definition of Done that lets the builder verify its own work, or shows unverified work above 40 | Check 12. The adopted text keeps the separate verifier run and the credit table |
| BR-18, unknown means highest risk | A schema that is silent on unknown kinds | Check 8 names the seven kinds. Refusing any other kind is FR-103 in S1-01; the document says so |
| AS-07, agent runs are recorded | A schema with no place for a run's start, inputs by reference, result and end | The step and evidence records must leave room for these. Filling them belongs to later stories; no story in the backlog names AS-07 directly |
| AS-08, no unattended work on deleting or privacy policy | C-1 decided by an agent in an unattended run | Checks 9 and 14. Only the owner answers |
| "Never, in any run" | Editing `rules.md`, `CLAUDE.md` or anything in `KUE Life Back Office/` | Check 17. Any rewording of the laws is left out of this story (section 7) |

## 6. Needs the owner

No security-critical path is changed by this story. After the owner answers C-1, the paragraph in `docs/product/rules.md` that calls C-1 open will be out of date. Updating it is a separate change, outside this story, made by the lead with the owner in the session (section 7).

What the owner must do:

1. **Decide C-1**: how KUE forgets something when its record never deletes. Question 1 in section 8. `rules.md` says this is settled in this story, before the ledger store is built in S1-01.
2. **Answer questions 2 to 7** in section 8. Each has a recommendation.
3. **Read the document and approve it**, or say what to change. His approval is acceptance check 15. The final signature is `/kue-sign S0-07`, which only he runs.

Nothing needs him at the Mac for a live test. No permission is needed.

The backlog marks this story as able to run unattended. It cannot finish that way: the draft can be written with nobody present, but checks 14 and 15 need his answers.

## 7. Not in this story

| A reader might expect | Where it lives |
| --- | --- |
| The ledger store itself, and the check that detects an altered record | S1-01 (FR-101, FR-102, CHK-14) |
| Refusing a record of an unknown kind; ledger writes passing the privacy firewall | S1-01 (FR-103) |
| The rules for which step state may follow which | S1-03 (FR-105) |
| Refusing "Verified" without linked evidence | S1-04 (FR-106) |
| Existing goals, plans and actions writing to the ledger | S1-05 (FR-107) |
| The build crew's runs writing to the ledger | S1-06 (FR-108) |
| Percent complete computed from the ledger | S2-01 (FR-109) |
| Full fields for item, promise and answer | The stories that build them, in epics E3 and E4 |
| How a yes is signed and bound to content in the app | S9-02 (FR-703, CHK-13) |
| Building forget, wipe and "see everything kept" | S10-04 (FR-805 to FR-808) |
| Export on leaving | FR-809, which has no story until the owner accepts the gaps G-1 to G-3 named in `rules.md` |
| Protecting the stored file itself | S5-02 and S5-03 (NFR-04), with the threat model outline in S0-08 |
| Making today's "erase everything" remove the two measurement tables it now leaves behind | S10-04 (FR-807). Found while reading the code for this work order; reported in the handback, not fixed here |
| Changing the existing privacy audit table, or how the app forgets a memory today | Not planned here. Section 8, question 8, and the handback note what was found |
| Rewording the C-1 paragraph in `docs/product/rules.md`, or the Definition of Done in `CLAUDE.md`, once the owner has answered | A separate change after this story, made by the lead with the owner in the session. `rules.md` is a security-critical path |
| Choosing and adding the fingerprint function the chain needs | S1-01, with the owner's yes, because it adds a dependency |

## 8. Open questions

**For the owner.** Each is asked again in the document, with the same recommendation.

1. **C-1. How does KUE forget, if its record never deletes?**
   - Option A, the proposal in `rules.md`: each record has two parts. A small part (its number, its kind, when, and its tie to the record before) is chained and never changes. The content sits apart and can be erased. Forgetting erases the content and adds a new record that says a deletion happened. Tampering stays detectable; the forgotten words are gone.
   - Option B: forgetting removes the whole record and re-ties the chain. This breaks BR-08, and a tampered record could no longer be told from a forgotten one.
   - Option C: nothing in the ledger can be forgotten. This breaks BR-19 and FR-806.
   - Recommendation: A.
2. **What may stay behind after a forget?** Under option A the chained part would keep a fingerprint of the erased content, so that changes are detectable. For short content, such as an amount or a payee name, a plain fingerprint can be guessed back by trying likely values. Does "none of the deleted content" (FR-808) rule that out? Recommendation: yes, rule it out. Mix each fingerprint with a random value that is erased together with the content, so what remains cannot be guessed back. Also to confirm: the times of the original record and of the forgetting stay in the marker. The existing code says, in a comment, that times are themselves a record of the owner's day and that a full erase removes everything. In fact today's full erase leaves two tables of once-a-second measurements, with their times, in place (see the handback and section 7).
3. **What does "wipe everything" leave?** FR-807 says all KUE data is removed. Recommendation: nothing remains, not even a note that a wipe happened; a new ledger starts from its first record. The alternative is a single first record saying "started after a wipe".
4. **Which Definition of Done is adopted?** Two texts exist and differ. `CLAUDE.md` has six points that define *Verified* (80%) and leaves *Signed* to the owner. The concept document's version also asks that the app builds, that a real test on the Mac has a recorded outcome, and that "the ledger holds the evidence". The ledger does not exist until S1-01. Recommendation: adopt the six points in `CLAUDE.md` as the gate for Verified; add the concept document's two lines (a recorded outcome for every live test, and the owner's signature) as the gate for Signed; and state that until S1-06 the files in `docs/work/<story>/` stand in for "the ledger holds the evidence".
5. **What does "this sprint's work orders are recorded against it" mean?** No ledger exists in Sprint 0. Recommendation: it means each Sprint 0 work order file under `docs/work/` carries the fields the schema requires, shown in a register (check 16). Two things follow. Most Sprint 0 work orders will not exist yet when this story is checked, so the register covers those that do. And S0-01 and S0-09 are marked Built in the backlog but have no work order under `docs/work/`: should one be written for each after the fact, or are they listed as "no work order"?
6. **Four kinds or seven?** The story names four records; FR-101 names seven. Recommendation: specify four in full and reserve item, promise and answer (check 8).
7. **Is a rejection a warrant?** The concept document says a warrant is marked Approved, Interim or Rejected. The sign command records a warrant only for Approved and Interim; a rejection sets the story back to Planned with a note. Recommendation: in the schema a rejection is also a warrant record, so it stays on the record (BR-08).

**For the lead.**

8. **The word "ledger" is already taken in the code.** It names the privacy audit table, which adds up counts in place and is removed by a full erase. Recommendation: the document always says "Work Ledger" for the new record and states that the privacy audit table is separate and unchanged. Renaming anything in code is not part of this story.
9. **The verifier's rule does not fit a document story.** It requires a test named for each requirement in the work order. This story adds no test (section 4). Should the verifier treat checks 2 to 11 as the proof for FR-101 and FR-808, or does the lead want something else?
10. **Unattended flag.** The backlog says this story can run unattended; `rules.md` says the owner settles C-1 in it. Under the crew's own rule an unattended run would stop at Planned. Should the flag in the backlog be changed to "No"? Only the lead edits the backlog.
11. **Where is the approval recorded?** This work order assumes two places: the "What the owner decided" section of the document (checks 14 and 15), and then `/kue-sign S0-07`. Confirm, or name another place.
12. **S1-01 will need a new dependency.** The core has no fingerprint function today, and the chain needs one. Adding it needs the owner's yes, so S1-01 cannot be built unattended until that yes is given. Worth asking him in the same sitting as C-1.
