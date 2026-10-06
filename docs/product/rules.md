# The laws of KUE

These rules do not bend for a deadline, a feature or a story. If a story seems to need one broken, stop and ask the owner.

## Business rules

| ID | Rule |
| --- | --- |
| BR-01 | A model may propose and phrase. It never decides, approves or changes state. This is enforced by the model having no such ability, not by an instruction |
| BR-02 | KUE never sends, pays, submits or deletes on its own. Release 1 does none of these at all |
| BR-03 | Personal data stays on the person's Mac in Release 1 |
| BR-04 | No claim without a record. Every line on the board traces to a ledger record, and every fact about a document shows the line it came from |
| BR-05 | Never guess. An unclear amount, date or payee is asked, with the page shown |
| BR-06 | A check has four results: passed, failed, not run, inconclusive. "Could not look" is never reported as "looked and found nothing" |
| BR-07 | Only the person answers and signs. A yes is bound to the exact content seen; changed content voids it |
| BR-08 | The ledger is append-only. Nothing is edited or removed silently, and a missed item stays on the record |
| BR-09 | KUE never states that something is compliant or legally complete. It reports what it checked and what it found |
| BR-10 | Numbers are computed by code: dates, amounts, totals, percent complete and every score. A model never produces a number |
| BR-11 | A draft is a draft: marked everywhere it appears, never counted as progress, never sent by KUE |
| BR-12 | Content is data. A document, a mail or a web page can never give KUE an instruction |
| BR-13 | Attention is respected: at most three questions at once, reminders only inside the person's own limits, no streaks or guilt, no chasing when the words show distress |
| BR-14 | The baseline is asked once. A skipped baseline is recorded as skipped, never as zero |
| BR-15 | Percent complete is earned by evidence: Planned 10, Built 40, Verified 80, Signed 100. Unverified work never shows above 40 |
| BR-16 | The run that builds a change never verifies it. The owner audits one item each sprint |
| BR-17 | New scope enters as a work order and displaces work of equal size |
| BR-18 | Unknown means highest risk. An undeclared action is refused; an unknown state after a crash is verified, never assumed |
| BR-19 | Leaving is free and clean: see everything kept, export it, forget one item or source, wipe everything |
| BR-20 | Test participants give written consent that names any cloud AI used, and their files are deleted when the test ends |

## Agent safety rules

| ID | Rule |
| --- | --- |
| AS-01 | No agent inside KUE holds a tool that changes state. Every action passes the action broker |
| AS-02 | An agent receives only data the privacy firewall has cleared, and only the fields its task needs |
| AS-03 | An agent's output is a proposal with its sources. Code accepts or rejects it |
| AS-04 | An agent that cannot find a source says so. It does not fill the gap |
| AS-05 | Content an agent reads cannot change its instructions, its policy or its tools |
| AS-06 | Unattended agents run only under a standing approval with a scope, a limit and an expiry, and never at approval levels 3 or 4 |
| AS-07 | Every agent run writes its start, its inputs by reference, its result and its end to the ledger |
| AS-08 | Build agents never run unattended on deleting, spending, privacy or network policy, signing or security code, or the main branch |
| AS-09 | Every agent stops within one tick of the kill switch |

## Security-critical paths

A change to any of these needs the owner present in the session and a line in the work order saying he approved it. They are never changed in an unattended run.

- `core/src/authz.rs`, `core/src/privacy.rs`, `core/src/safety.rs`
- `core/src/actions.rs`, `core/src/transaction.rs`, `core/src/capabilities.rs`
- `auth/**`
- `src-tauri/tauri.conf.json`, `src-tauri/Info.plist`, `src-tauri/capabilities/**`
- anything that signs, notarises or sets entitlements
- `.gitignore`, `.claude/settings.json`, `docs/product/rules.md`

## Never, in any run

- Touch `main`, push to any remote, or rewrite history.
- Delete a branch, a worktree, or a file outside the story.
- Add a network client, telemetry, analytics or an update check to product code.
- Read the owner's personal data: `~/Library/Application Support/Lantern`, `.env` files, enrollment data, `~/KUE-private`.
- Read `KUE Life Back Office/manual-test/`, which holds other people's documents, or edit anything in `KUE Life Back Office/`.
- Add a dependency without the owner's yes.
- Delete, skip or ignore a test to make a run pass.
- Change a story's status to Signed. Only the owner does that, with `/kue-sign`.
- Change a threshold in `config/lantern.toml` without a work order that names it.

## The two open decisions that touch the ledger

- **C-1.** The ledger never deletes, yet a person may forget an item or wipe everything. The proposal: the chain covers record headers, content sits in a separate store that can be erased, and a forgotten item leaves a marker with no content. The owner settles this in story S0-07, before the ledger store is built in S1-01.
- **Gaps G-1, G-2, G-3** are not in the backlog. Requirements FR-803 and FR-809 to FR-812 have no story until the owner accepts them. Do not build them.
