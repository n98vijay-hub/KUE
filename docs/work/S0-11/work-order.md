# Work order S0-11: Change control on GitHub, proven by a gate test

Story: S0-11, Sprint 0, epic E0. Lead role: Architect. Evidence type: Demo. Owner at the Mac: Yes. Unattended: No.
Requirements linked: none. The story serves change request CR-001 (`docs/changes/CR-001-change-control-on-github.md`).
Written 2026-10-06 on `accept/S0-11`. **This work order is written after the fact.** Most of the work it describes was done on 6 October 2026, with the owner present, before any work order existed. It plans only what is still missing. It changes no product code.
**Revised 2026-10-07 on `accept/S0-11`, after the owner accepted the first version** (pull request #5, approved 2026-10-06T20:09:40Z, merged as `58bedab`): the build started and then stopped, because the test file of gate test 5 was approved and merged by the owner against the plan, so check 8 is reworded, check 21 and step 5a are added, and the owner's answers and his statement of GitHub's rules page are written down here. A part this revision did not touch reads as first accepted, on 6 October 2026.

## 1. In plain words

Most of this work was done on 6 October 2026, with you present, before any work order was accepted. This work order is written afterwards, and says so. GitHub now holds the project and refuses changes that skip your rules. A test with four results is on record. The sign-off folder test gave the expected refusal; you then let its test file in, and a request you approve will remove it. Not yet proven: that GitHub requires four checks and refuses an unfilled summary. When this is done, the gate test record will hold those results in GitHub's own words, your statement of GitHub's rules page, and a note that the first tests ran early. Nothing in the app changes.

## 2. Acceptance checks

Each check passes or fails on its own. "The record" means `docs/process/GATE-TEST.md`. "The release line" means the branch `kue/release-1`. "The procedure" means `docs/process/CHANGE-CONTROL.md`.

The checks are written for the recommended answers in section 8. If the owner answers a question differently, the lead sends his answer back and the affected checks are rewritten before anything is run.

After each check, "Today" says where it stands at this revision, on 7 October 2026, after the first acceptance. In the first version it meant 6 October 2026, before acceptance. That is not a verdict. The verifier decides each result afterwards, in a separate run.

1. The record holds the owner's own dated statement of what GitHub's rules page shows: the branches covered, the required checks by name, whether an owner's review is required for his files, and who may bypass the rules. (story acceptance criteria, opening words "given the rulesets are on") *Today: the statement was taken on 7 October 2026 and is in section 3 of this work order, under "What the owner stated". It is not yet in the record.*
2. That statement names four required checks: Ledger rules, Core tests, Window tests and Owner brief, as the procedure says. (CR-001, the procedure) *Today: the statement names these four. He ticked them from a list the lead offered. It is not yet in the record.*
3. The record shows that a push by the crew straight to the release line was refused, with GitHub's message quoted. (story acceptance criteria, clause 1) *Today: on record as test 1.*
4. The record shows a pull request that touched no protected file, had every check green, and merged with no approval from anyone. (story acceptance criteria, clause 2) *Today: on record as test 2, with three checks.*
5. The same is shown again now that there are four checks: no protected file, four checks green, merged with no approval. (story acceptance criteria, clause 2; the procedure) *Today: not run.*
6. The record shows a pull request whose brief for the owner was left unfilled: the Owner brief check failed, and GitHub refused the merge although the other three checks were green. (CR-001, the procedure) *Today: not run.*
7. The record shows a pull request that added a file in the sign-off folder `docs/signoff/`, with all four checks green: GitHub refused the merge because the owner had not approved, with GitHub's answer quoted. (story acceptance criteria, clause 3) *Today: the test ran on 6 October 2026 as pull request #6. GitHub shows the four checks green. The refusal was observed by the lead in the session and is quoted in section 3. It is not yet in the record.*
8. The test file of check 7, `docs/signoff/gate-test-probe.md`, is no longer on the release line, and it was removed by a pull request the owner approved. (the owner's words of 7 October 2026 on check 8; law "never write a decision the owner did not state", BR-04) *Reworded on 7 October 2026 with the owner's words, which are in section 3 under "What the owner stated". As first accepted this check read: "The test file of check 7 is not on the release line. Its pull request was closed without merging." In that wording it failed: the owner approved pull request #6 and merged it himself. Today: the file is on the release line. Its removal is step 5a, not run.*
9. The record shows that a pull request which marked a story Signed with no sign-off record failed the Ledger rules check, with the message quoted. (story acceptance criteria, clause 4) *Today: on record as test 3.*
10. The story used for that test, S0-08, is not Signed on the release line. (story acceptance criteria, clause 4; BR-07) *Today: true in the files.*
11. The record states that the pull request of test 4 merged only after the owner approved, and gives who approved and the two times as GitHub returns them. (story acceptance criteria, clause 3; BR-04) *Today: missing from the record. GitHub has the facts; they were read back and are in section 3.*
12. Every result named in checks 1 to 11 has its own line in the record: what was done, what GitHub answered or the owner stated, and whether that was expected. (story acceptance criteria, last sentence) *Today: four of them are there.*
13. The four lines already in the record are unchanged. Everything new is added below them, with its date. (BR-08) *Today: not run.*
14. Every line in the record can be matched to what GitHub shows for the pull request it names, or to words the owner stated. (law "never write a test result the owner did not state", BR-04, BR-06) *Today: not run.*
15. The record says plainly that tests 1 to 4, and the change that added the Owner brief, were done before this work order was accepted. (the procedure, gate 1) *Today: not run.*
16. The owner's approval of the security-critical changes that arrived in the starting commit `abe946e`, without a pull request, is recorded in his own words with the date. (`docs/product/rules.md`, security-critical paths) *Today: his words were given on 6 October 2026 and are in sections 3 and 6 of this work order. They are not yet in the record.*
17. The owner's decision to make the repository public is recorded in his own words with the date. CR-001 says it is in the record. It is not. (CR-001, "What it costs and risks") *Today: his words were given on 6 October 2026 and are in section 3 of this work order. They are not yet in the record.*
18. After this story, no file on the release line says there are three required checks, apart from the quoted message of test 1. (the procedure) *Today: two files still say three. See section 3.*
19. From the acceptance of this work order onwards, this story changed only these files on the release line: the record, one line in `docs/work/HANDOVER.md`, the two wording corrections of check 18, files under `docs/work/S0-11/`, the probe file `docs/signoff/gate-test-probe.md`, which arrived on the release line by pull request #6 merged by the owner and is removed again by this story, and the status of S0-11 itself in `docs/product/backlog.json`, which only the lead sets. No product code changed. (crew rules) *Today: not run. The probe file was added to this list on 7 October 2026.*
20. From the acceptance of this work order onwards, the crew broke none of these rules: it changed no setting on GitHub, deleted no branch, forced no push, approved nothing, and merged nothing that GitHub refused. What happened before acceptance is put to the owner in questions 5 and 10. (`docs/product/rules.md`, "Never, in any run") *Today: not run.*
21. The record says plainly that the owner approved pull request #6 and merged it himself after GitHub's refusal, and gives who approved, who merged and the two times as GitHub returns them. (the owner's words of 7 October 2026 on check 8; BR-04, BR-08) *Added on 7 October 2026. Today: not in the record. GitHub has the facts; they are in section 3.*

## 3. Steps, by owner

There are no steps for the core builder, the Mac builder or the window builder. Nothing under `core/`, `src-tauri/`, `sensing/`, `act/`, `auth/`, `mind/`, `voice/`, `scripts/` or `src/` changes. Every step below is the owner's, or the lead's with the owner present in the session.

### What already exists

**Tests 1 to 4, and pull request #4 which added the Owner brief, were all done on 6 October 2026, before this work order was accepted.** Nothing below pretends otherwise. The table says what each of them covers and what it does not.

I read the files and asked GitHub, read-only, what it holds for pull requests #1 to #4. GitHub's times are in UTC.

| Part of the story | What covers it | What I confirmed | Standing |
| --- | --- | --- | --- |
| "Given the rulesets are on" | Nothing direct. Test 1's refusal shows that some rule guards the release line | The crew may not read GitHub's rules: that request is blocked for the crew on purpose. The lead tried once in the planning session and it was declined. Nobody has written down what the rules say | Not checked by the crew |
| Clause 1: a straight push is refused | Test 1 in the record | The commit `5626101` exists. The record quotes GitHub's refusal. The message says "3 of 3 required status checks", which was the count before the Owner brief was added | Covered. The count is out of date |
| Clause 2: ordinary work merges on green checks | Test 2, pull request #1 | GitHub: one file, `docs/work/HANDOVER.md`. Three checks succeeded. No review. Merged by the crew account at 18:18 as `7d3336a` | Covered with three checks. Not shown with four |
| Clause 3: a sign-off record waits for the owner | Test 4, pull request #3 | GitHub: the one file was `docs/process/GATE-TEST.md`, not a file in `docs/signoff/`. The owner's account approved at 18:27:57. Merged at 18:28:42 as `097a3e1`. The record gives the refusal and leaves out the merge | **Not tested as written.** Same rule, different folder |
| Clause 4: Signed with no record fails | Test 3, pull request #2 | GitHub: Ledger rules failed, the other two succeeded, closed at 18:22 without merging. S0-08 is "Not started" on the release line | Covered. The record quotes one of three messages and says "and two more problems" |
| "Each result is recorded" | `docs/process/GATE-TEST.md`, four lines | Added by pull request #3 with the owner's approval | Covered for what ran |
| The fourth check, Owner brief | Nothing in the record | Added by pull request #4, approved by the owner at 18:42:29, merged at 18:44:14 as `df9dd0c`. The check ran once, on that pull request, and passed. It has never been seen to fail. Nothing shows GitHub requires it | Not checked |

Also in place from the starting commit `abe946e`: the procedure, the list of files only the owner may approve (`.github/CODEOWNERS`), the three first checks, the Ledger rules script, the sign-off guide and CR-001. That commit was published as the clean start of the repository. It did not go through a pull request, so GitHub holds no approval for it (check 16).

### Where the files and the documents disagree

I trust the files. None of these stops the story except where a check above names it.

1. The sign-off form in `docs/signoff/README.md` says "Checks: Ledger rules, Core tests, Window tests". There are four checks since pull request #4. (check 18)
2. The opening note of `.github/workflows/kue-checks.yml` says "Three checks, each required". That file runs three; the fourth lives in `.github/workflows/kue-owner-brief.yml`. A reader would count three. (check 18)
3. CR-001 says the owner's decision on a public repository is "recorded by the owner in the gate test record". The record only says "Visibility: Public". (check 17)
4. The procedure says "GitHub deletes its copy after the merge". GitHub now lists two branches only, `main` and `kue/release-1`. So the copy of `story/gate-test-ledger` is gone too, and that pull request was closed, not merged. The procedure describes removal only after a merge. Nothing on record says who removed this one. (section 8, question 10)
5. `docs/product/rules.md` says every security-critical path is listed in `.github/CODEOWNERS`. Every path it names is listed. But it also says "anything that signs, notarises or sets entitlements", and six scripts that mention signing or entitlements are not listed: `act/build.sh`, `mind/build.sh`, `sensing/build.sh`, `voice/build.sh`, `scripts/build-app.sh` and `scripts/status-kue.sh`. The seventh that matched, `auth/build.sh`, is covered because all of `auth/` is listed. I searched for the words only. I did not judge whether each script signs or only reports. (section 8, question 9)
6. The procedure states things that no test has shown: the bypass list is empty; a pull request into `main` needs the owner; only the owner creates a release tag. The first is put to the owner in step 1. The others are outside this story (section 7).
7. The backlog says S0-11 is "Not started". Most of it is done. Status is the lead's to set.

### What the owner stated

Added at the revision of 7 October 2026, at the owner's request. His instruction, given on 7 October 2026 in the session:

> Write my answers to questions 5, 6 and 10 and my rules-page statement into the work order now, so they are in a file and not only in this session.

Every quotation in this part is copied character for character from what the lead passed on to me from the session. I did not see the owner give them, and I did not find them on GitHub: pull requests #5 and #6 carry no comment. Each entry gives the date, how the words were given, and the words. Where the lead told me the owner chose from options the lead offered, the entry says so, because those are not free words. Where the entry says "his words", the lead passed them on as the owner's own and named no options.

**1. What GitHub's rules page shows (step 1).**

First answers, 7 October 2026. Chosen by the owner from options the lead offered.

| Part | What he chose |
| --- | --- |
| Required checks, by name | He ticked all four: Ledger rules, Core tests, Window tests, Owner brief |
| Branches covered | "Only kue/release-1" |
| Is a review by the owner of a file required | "Yes, it is required" |
| Bypass list | "Nobody, it is empty" |

Correction to the branches part, later on 7 October 2026. Typed by the owner, in his own words:

> Default and kue/release-*. Below it, the page says: applies to 2 targets, kue/release-1 and main. The bypass list is empty.

The first answer and the correction are both kept, in that order. The record takes them the same way (step 5).

**2. Question 5: the starting commit `abe946e`.** 6 October 2026. His words:

> Yes. I approved the first change-control commit (abe946e) on 6 October 2026. I answered the CR-001 questions and ran the push myself. I did not read the files line by line. I relied on the description I was given.

**3. Question 6: a public repository.** 6 October 2026. His words:

> Yes. I decided on 6 October 2026 to make this repository public, with a clean start to its history.

**4. Question 10: who removed GitHub's copy of `story/gate-test-ledger`.** 7 October 2026. Chosen by the owner from three options the lead offered: "I did it myself", "I allowed the crew to do it", "I do not remember". He chose:

> I did it myself

The lead described that option to him as: he closed pull request #2 and removed its branch himself in the browser.

**5. Check 8: the probe file.** 7 October 2026. His words:

> I accept your recommendation for check 8: the probe file is removed by a pull request I approve, and the record says plainly that I approved and merged pull request 6 after GitHub's refusal.

**6. The titles of the test pull requests.** 7 October 2026. His words:

> Title the two remaining test pull requests "GATE TEST: owner does nothing".

### What happened after the first acceptance

Added at the revision of 7 October 2026. The owner approved the first version of this work order on GitHub: pull request #5, approved by `n98vijay-hub` at 2026-10-06T20:09:40Z, merged at 20:11:17Z as `58bedab`. The build then started and stopped.

On 7 October 2026 I asked GitHub, read-only, what it holds for pull requests #2, #3, #5 and #6. GitHub's times are in UTC. Each line below says whether GitHub shows it or whether it rests on what the lead saw in the session.

**Step 4 was done: what GitHub holds for tests 3 and 4.**

- Pull request #3, test 4. GitHub shows: approved by `n98vijay-hub` at 2026-10-06T18:27:57Z, merged at 18:28:42Z as `097a3e1`. GitHub records the crew account `vijayn2698` as the one who merged.
- Pull request #2, test 3. GitHub shows: one file, `docs/product/backlog.json`. No review. Ledger rules failed; Core tests and Window tests succeeded. Closed at 2026-10-06T18:22:52Z without merging.
- The three messages of the Ledger rules check on pull request #2, as the lead read them from GitHub's log of that check:
  1. "S0-08 is Signed but there is no sign-off record at docs/signoff/S0-08.md."
  2. "S0-08: Not started to Signed. Only a Verified story can be Signed."
  3. "S0-08: Not started to Signed. Signing needs the sign-off record docs/signoff/S0-08.md added or changed in the same pull request, which the owner must approve."

  I did not read that log myself. I compared the three with the Ledger rules script: they are, word for word, what the script writes for this case.

**Step 2 was run: test 5, on 6 October 2026, with the owner in the session.**

- GitHub shows: branch `signoff/gate-test-probe`, commit `a70a19a`, one new file `docs/signoff/gate-test-probe.md`, pull request #6 "Gate test 5: a file in the sign-off folder".
- GitHub shows: all four checks passed. Ledger rules, Core tests, Window tests, Owner brief.
- **Observed by the lead in the session. GitHub does not show this afterwards.** GitHub had asked `n98vijay-hub` for a review, the list of reviews was empty, and GitHub gave the state as "blocked". The lead asked GitHub once to merge, in the ordinary way. GitHub answered, word for word:

  > Pull request n98vijay-hub/KUE#6 is not mergeable: the base branch policy prohibits the merge.

  The list of reviews was still empty straight after. That is the expected result for check 7.
- Then, against the plan. GitHub shows: the owner approved pull request #6 at 2026-10-06T20:17:22Z. His note with the approval, as GitHub holds it, is two lines. The first line is "accepte". The second line is "move to next".
- As the lead reports from the session: the lead asked to close the pull request. The owner declined and said he would close it himself.
- GitHub shows: on 2026-10-07T19:04:23Z the pull request was merged as `18565e4`. GitHub records `n98vijay-hub` as the one who merged. It was not closed unmerged.
- This is not the hole that item 7 of step 2 describes. GitHub merged only after the owner's approval, and at his own request.

**Where that leaves the story.**

- The probe file `docs/signoff/gate-test-probe.md` is on the release line. I read it there: it says it is not a sign-off record and holds no decision of the owner. No story changed status: pull request #6 changed that one file only.
- Check 8, as first accepted, has failed and cannot pass as worded. It is reworded with the owner's words, check 21 is added, and step 5a removes the file (section 2, and below).
- A branch `story/gate-test-brief` exists on this Mac only. It is not pushed. It was cut from the release line at `18565e4` and has one commit, `1c3e618`, which adds the line "Gate tests 6 and 7, 7 October 2026" to `docs/work/HANDOVER.md`. Step 3 will use it.
- Nothing of tests 6 and 7 has run.

What I could not confirm from GitHub: the refusal itself, and that the review had been asked for and not yet given when the lead asked to merge. GitHub gives me no time for the lead's request. What it does show is the order of the rest: the first run of all four checks was complete at 20:16:38Z, the Owner brief check ran a second time and passed at 20:17:06Z, and the approval is at 20:17:22Z. The owner's own words of 7 October 2026, above, say he approved and merged "after GitHub's refusal".

### What is still to do

Nothing below starts before the owner has approved this work order on GitHub. This time the order is kept.

**Step 1. The owner, with the lead in the session: say what GitHub's rules page shows.**
Only the owner can open that page. The crew may not read it and never changes it. He reads out, and the lead writes down in his words:
- which branches the rules cover;
- which checks are required, by name;
- whether a review by the owner of a file is required before a merge;
- who is on the bypass list.

If Owner brief is not among the required checks, that is a finding, not a failure of his. Only he can add it. If he does, he says so and the lead writes that down too. Step 3 does not run until he has stated that four checks are required.
Files: none yet. His words go into the record in step 5.

**Done on 7 October 2026.** His statement, first answers and correction, is above under "What the owner stated". His first answers name four required checks. The procedure says `main` is protected, and his corrected statement names `main` among the two targets.

**Step 2. The lead, with the owner present: test 5, a file in the sign-off folder.**
1. Branch `signoff/gate-test-probe` from the release line.
2. Add one new file, `docs/signoff/gate-test-probe.md`. It says, in three sentences, that it is a probe for gate test 5 of S0-11, that it is not a sign-off record and holds no decision of the owner, and that it must not be merged. The name is chosen on purpose: the Ledger rules only inspect files in that folder that are named for a story. A file named for a story would need a decision line, and any decision line written here would be a decision the owner never made.
3. Push the branch. Open a pull request into the release line, titled "Gate test 5: a file in the sign-off folder", with a complete and true brief. Its last line reads "Your action: None. Do not approve. This is a test and will be closed."
4. Wait until all four checks are green. If one is not, stop and report. Do not go on with a red check, because then the refusal would prove nothing about the owner's approval.
5. Ask GitHub once, in the ordinary way, to merge it. No override of any kind. Keep GitHub's answer word for word, and whom it asked for a review.
6. If GitHub refuses: that is the expected result. Close the pull request without merging. Closing needs the owner's yes at the prompt. The branch stays; the crew deletes nothing.
7. If GitHub merges it: stop. The gate has a hole. Tell the owner at once and change nothing. The result is "failed" and is recorded as such.

Files: `docs/signoff/gate-test-probe.md`, on the test branch only. It never reaches the release line.

**Done on 6 October 2026. Outcome, added at the revision of 7 October 2026.** All four checks were green and GitHub refused the lead's one request to merge, which is the expected result. Then, against item 6 and the line above, the owner approved pull request #6 and on 7 October 2026 merged it himself, so the probe file did reach the release line and the pull request was not closed unmerged. The facts, and who vouches for each, are above under "What happened after the first acceptance".

**Step 3. The lead, with the owner present, and only after step 1 named four checks: tests 6 and 7, the brief.**

Changed at the revision of 7 October 2026: the branch in item 1, the title in item 2, and this paragraph and the next two.

**On this pull request the owner does nothing on GitHub: no approve, no merge, no close.** If a review by anyone appears on it before the last merge request, which is item 6, test 7 is spoiled. It is then recorded as "not run", with the reason, and repeated on a fresh branch. The repeat has a complete brief from the start: nothing is left unfilled a second time.

The owner's instruction of 7 October 2026 speaks of "the two remaining test pull requests". This plan has one, because tests 6 and 7 share a pull request. Any further test pull request in this story, such as a repeat of test 7, takes the same opening words in its title.

1. Use the branch `story/gate-test-brief` that already exists on this Mac, cut from the release line at `18565e4`, with its one commit `1c3e618`. That commit adds one true, dated line to `docs/work/HANDOVER.md`: "Gate tests 6 and 7, 7 October 2026". That file is not one of the owner's protected files; test 2 used it too. Before pushing, the lead brings this branch up to date with the release line by an ordinary merge, so that a refusal in test 6 can come only from the brief and not from the branch being behind. (Sentence added by the lead on 7 October 2026: the owner's statement does not say whether GitHub requires an up-to-date branch.)
2. Push. Open a pull request into the release line, titled `GATE TEST: owner does nothing (tests 6 and 7: a brief left unfilled on purpose)`, with one part of the brief left as the template has it. This is done on purpose and only here. The title starts with the owner's exact words and still says the unfilled brief is on purpose, as question 16 requires.
3. Wait for the checks. Expected: Owner brief fails, the other three pass. Keep the check's message.
4. Ask GitHub once, in the ordinary way, to merge it. Expected: refused. Keep the answer. That is test 6. If GitHub merges it, stop and tell the owner: the fourth check is not required, whatever the page says.
5. Complete the brief truthfully by replacing the description. The Owner brief check runs again by itself. Wait until all four are green.
6. Ask GitHub to merge. Expected: merged, with no approval from anyone. Keep the answer. That is test 7, and it is clause 2 shown again with four checks.

Files: `docs/work/HANDOVER.md`, one line.

**Step 4. The lead: read back what GitHub holds for test 4.**
Read-only. For pull request #3: who approved, when, when it merged and as which commit. For pull request #2, if GitHub still returns it: the full text of the three Ledger rules messages. Nothing is written from memory.

**Done.** What GitHub returned is above under "What happened after the first acceptance", with the three messages.

**Step 5. The lead, with the owner present: add to the record.**
Branch `story/S0-11` from the release line. File: `docs/process/GATE-TEST.md`. This is a security-critical path (section 6).
The table of tests 1 to 4 is not touched. Below it, a new part headed "Additions", with the date, holding:
- a note that tests 1 to 4, and the change that added the Owner brief, were done before the work order for S0-11 was accepted;
- a note on test 1: "3 of 3" was the number of required checks on that day, before the fourth was added;
- a note on test 3: the other two messages, if step 4 returned them;
- a note on test 4: the owner approved, then it merged, with the names, times and commit from step 4;
- the owner's statement from step 1, in his words;
- tests 5, 6 and 7, each as a line in the same form as tests 1 to 4;
- the owner's words on the starting commit `abe946e` and on making the repository public (questions 5 and 6), or "not yet answered".

Added at the revision of 7 October 2026. The "Additions" part also holds:
- the outcome of test 5: GitHub's refusal, marked as observed by the lead in the session, then the owner's approval of pull request #6 and his own merge of it, with who approved, who merged and the two times as GitHub returns them (check 21);
- the removal of the probe file `docs/signoff/gate-test-probe.md` by step 5a, and that the pull request which removes it waits for the owner's approval;
- the owner's answer to question 10, with the date and the three options he chose from;
- the owner's statement of GitHub's rules page as he first answered it and as he corrected it, in that order, each with its date and with how it was given.

His words are copied from "What the owner stated" above, character for character.

A test that did not run is written as "not run", with the reason. It is never left out and never written as passed.

**Step 5a. The lead, with the owner present: remove the probe file.**
Added at the revision of 7 October 2026. It is numbered 5a so that no other step changes its number.
On the same branch, `story/S0-11`. Remove `docs/signoff/gate-test-probe.md`. Nothing else in `docs/signoff/` is touched by this step. That folder is the owner's, so the Build pull request that carries the removal waits for his approval, which it does anyway. The Ledger rules do not refuse the removal: they forbid removing a sign-off record named for a story, and this file is not named for one (section 5).
Files: `docs/signoff/gate-test-probe.md`, removed.

**Step 6. The lead, with the owner present: two wording corrections.**
On the same branch. Both are security-critical paths or the owner's files.
- `docs/signoff/README.md`: the line of the form that lists three checks lists four.
- `.github/workflows/kue-checks.yml`: the opening note says these are three of the four checks and names where the fourth is. Only the note changes. No line that runs is touched.

**Afterwards, as for every story.**
The verifier, in a separate run, writes `docs/work/S0-11/evidence.md`. It does not repeat the gate tests. It reads the record and matches each line against what GitHub shows for the pull request named, read-only, and runs `./scripts/test-kue.sh`. The guardian writes `docs/work/S0-11/review.md`. The lead opens the Build pull request from `story/S0-11`. It touches the owner's files, so it waits for his approval.

## 4. Tests to add

None in code, and I say so plainly: this story's evidence type is Demo. The proof is the gate tests themselves, run on GitHub, each with GitHub's own answer on record.

What I checked: the two gate scripts, `.github/scripts/ledger_guard.py` and `.github/scripts/pr_brief_check.py`, are the only Python files in the repository. They have no test files. `./scripts/test-kue.sh` does not run them. The story does not ask for tests of them, so none is planned here (section 7, and question 13).

So that each demonstration can be named for what it proves, the lines of the record carry these names. They are labels, not code.

| Name | Proves | Record line |
| --- | --- | --- |
| `cr_001_direct_push_refused` | Clause 1 | Test 1, done |
| `cr_001_open_work_merges_on_green_checks` | Clause 2 | Test 2, done with three checks; test 7, to run with four |
| `cr_001_signed_without_record_fails_ledger_rules` | Clause 4 | Test 3, done |
| `cr_001_owner_file_waits_for_owner` | The mechanism behind clause 3, on the procedure folder | Test 4, done |
| `cr_001_signoff_folder_waits_for_owner` | Clause 3 | Test 5, to run |
| `cr_001_unfilled_brief_refused` | The fourth required check | Test 6, to run |
| `cr_001_rules_page_stated_by_owner` | "Given the rulesets are on" | The owner's statement, to take |

No fixture is needed. The probe file and the handover line are invented text. Nothing reads the owner's documents or KUE's own data.

`./scripts/test-kue.sh` must still pass at the end, with no test removed or skipped. No product code changes, so it is expected to. That is an expectation, not a result; the verifier runs it.

## 5. Laws at stake

| Law | How this story could break it | How the steps avoid it |
| --- | --- | --- |
| "Never push to `main` or `kue/release-1`" | Test 1 did exactly this, on purpose, because clause 1 of the story demands it. The owner was present and GitHub refused | It is not repeated. No remaining step pushes to a protected branch. Repeating it to read the new count of checks is not planned (question 2) |
| "Never merge a pull request that GitHub refuses" | Steps 2 and 3 ask GitHub to merge something that should be refused | One ordinary request each, with no override. A refusal is kept and obeyed. The crew's settings already forbid the override |
| "Never approve or review; never act as the owner" | The crew approving its own test to finish it | Only the owner's account approves. On test 5 he is asked not to. The crew works through its own account only |
| "Never change the repository's settings or rulesets" | The crew looking up or correcting the required checks itself | Step 1 is the owner's. The crew does not read the rules page and does not change it |
| "Never write a test result, a decision or a condition the owner did not state" | A test sign-off record with an invented decision; a result written from memory; the owner's approval of `abe946e` or of a public repository written for him | The probe file is not a record and holds no decision. Steps 4 and 5 copy from GitHub. Checks 16 and 17 take his words or say "not yet answered" |
| "Never delete a branch; never force-push; never push an old branch" | Tidying the test branches away | The test branches stay. Only `signoff/…`, `story/…` branches cut fresh from the release line are pushed |
| Security-critical paths | Changing `docs/process/**` or `.github/**` with nobody present | Steps 5 and 6 run only with the owner in the session. Their pull request waits for his approval (section 6) |
| AS-08, no unattended work on security or the main branch | Running any of this at night | The story is marked unattended: No. Every step needs him |
| BR-04, no claim without a record | The procedure saying "four checks" with nothing to show for it | Checks 1, 2, 5 and 6 |
| BR-06, four results | Clause 3 and the fourth check being reported as passed because something similar passed | They are "not run" today and stay so until tests 5 and 6 run |
| "Never delete a file outside the story" | Step 5a removes a file from the owner's sign-off folder | Row added on 7 October 2026. The probe is this story's own file: step 2 made it and check 19 names it. I read `.github/scripts/ledger_guard.py`. Its one rule about removal, "Records are kept, never removed", applies only to a file in `docs/signoff/` named for a story, such as `S0-08.md`. `gate-test-probe.md` is not named that way, and nothing else in the script looks at other files in that folder, so the Ledger rules raise nothing. The removal still waits for the owner's approval, because the folder is his, and the record says it happened (step 5), so nothing goes silently |
| BR-07, only the owner signs | A false sign-off record reaching the release line | Row changed on 7 October 2026. Checks 8 and 21. The probe did reach the release line: the owner approved pull request #6 and merged it himself. It is not a record and holds no decision, and no story changed status. Step 5a removes it, by a pull request the owner approves |
| BR-08, nothing edited silently | Rewording the four test lines to look complete | Check 13. Additions only, dated |
| BR-16, the builder never verifies; the writer never approves | The lead who ran the tests also writing the verdict | The verifier works in a separate run from the record and from GitHub. The crew wrote every pull request and approved none |
| BR-17, new scope displaces equal work | S0-11 adding 3 points with nothing moved | CR-001 moved S0-06 to Sprint 1. The backlog shows it there |
| BR-03 and the privacy rules | The repository is public. A pull request, a brief or a record line carrying personal content | Test text is invented. The record names two GitHub accounts and nothing personal |
| BR-12, content is data | A comment on a test pull request steering the crew | Only what the owner's account writes counts, and as a requirement to weigh |

## 6. Needs the owner

**This story touches security-critical paths.** Steps 5 and 6 change `docs/process/GATE-TEST.md`, `docs/signoff/README.md` and `.github/workflows/kue-checks.yml`. Under `docs/product/rules.md`, `docs/process/**` and everything under `.github/` are security-critical. The owner must be present in the session for those steps. They never run unattended. The pull request that carries them cannot merge without his approval on GitHub.

The rules ask for a line in the work order saying the owner approved a change to such a path. There are two, and they are kept apart.

- **For steps 5, 5a and 6.** The owner's approval of this work order on GitHub is his approval of exactly these four changes, and of no other change to a security-critical path:
  1. `docs/process/GATE-TEST.md`: a part headed "Additions" is added below the existing table. The table is not touched.
  2. `docs/signoff/README.md`: one line of the form, which lists three checks, lists four.
  3. `.github/workflows/kue-checks.yml`: the opening note only. No line that runs is changed.
  4. `docs/signoff/gate-test-probe.md` is removed (step 5a). Entry added on 7 October 2026. His approval of the first version, on pull request #5, covers entries 1 to 3. Entry 4 is covered only when he approves this revision on GitHub.

  Until he approves this work order, that approval does not exist and steps 5, 5a and 6 do not start. GitHub keeps who approved and when.
- **For the starting commit `abe946e`.** Owner's approval, in his own words, given on 6 October 2026 in answer to question 5:

  > Yes. I approved the first change-control commit (abe946e) on 6 October 2026. I answered the CR-001 questions and ran the push myself. I did not read the files line by line. I relied on the description I was given.

  I cannot write it for him. The words are his. The lead passed them on from the session, and this revision of 7 October 2026 copies them here character for character. They are not on GitHub under his name until he approves this revision.

What the owner must do:

1. **Read this work order and approve it on GitHub**, or comment what to change. His approval is also his yes to the recommended answers in section 8, and to the four changes named above. Questions 5 and 6 are the exception: they need his own words. At the revision of 7 October 2026: the first version has his approval; this revision changes the same file, which only he owns, so it waits for his approval again.
2. **Step 1:** open GitHub's rules page for the repository and say what it shows. Only he can. Done on 7 October 2026.
3. **Be present for steps 3, 5, 5a and 6.** Changed at the revision of 7 October 2026. **In step 3, on the pull request whose title begins "GATE TEST: owner does nothing", he does nothing on GitHub. He does not approve it. He does not merge it. He does not close it.** The crew asks GitHub to merge it, twice, and GitHub answers. If he approves or reviews it before the last of those requests, test 7 is spoiled and must be repeated. Step 2 is done; what happened there is in section 3.
4. **Say in his own words** whether he approved what arrived in the starting commit (question 5) and that he chose a public repository (question 6). Done on 6 October 2026. His words are in section 3.
5. **Approve the Build pull request** when the record, the evidence and the review are in it.
6. **Sign** with `/kue-sign S0-11`, which only he runs.

Nothing here needs a camera, a microphone, Touch ID or any new permission. "At the Mac" means present in the session and at GitHub in his own browser.

## 7. Not in this story

| A reader might expect | Where it lives |
| --- | --- |
| The first real sign-off record | The first story the owner signs with `/kue-sign`. It will show clause 3 again, with a real record |
| Proof that a pull request into `main` needs the owner, and that only he can create a release tag | Not tested. The first release pull request will show it. No story names it |
| Proof that a new push withdraws an approval already given | Not tested. The crew's planning instructions say it does. It matters for BR-07. No story names it (question 14) |
| Proof that a story cannot become Verified without evidence and a review, and that scope cannot change without a change request | The Ledger rules script holds both rules. Neither is in this story's acceptance criteria. The first Build pull request will exercise the first |
| Tests for the two gate scripts | No story. A change request if the owner wants them (question 13) |
| The shell tests running on GitHub | Not planned. The procedure says they stay on the owner's Mac |
| Adding the build scripts that sign to the owner's list | Its own change, after the owner answers question 9 |
| Tightening the crew's own push permissions | Its own change to `.claude/`, with the owner present (question 11) |
| Removing the old and test branches on this Mac | S0-02, and only with the owner's yes |
| Backup of what is not on GitHub | S0-03, as reworded by CR-001 |
| Deciding whether the repository should be public | The owner's decision. This story only records what he says |
| Rewording the story's acceptance criteria to mention the Owner brief | A change of scope. It would need a change request |
| Any change to the app | None. KUE itself is untouched |

## 8. Open questions

**For the owner.** Approving this work order is a yes to each recommendation, except 5 and 6. Questions 10 and 16, further down, are also his: the lead cannot answer them for him.

1. **Clause 3 was not tested as written. How should it be proven?** Test 4 used a file in the procedure folder. The rule is the same; the folder is not.
   - Option A: a real test, as in step 2. A probe file in the sign-off folder, all checks green, refused without your approval, then closed unmerged. Cost: about ten minutes with you present, and one more closed pull request. Weak point: the probe is not a real sign-off record, because a real one needs a decision and only you make decisions.
   - Option B: you accept test 4 as equal and say so in writing. Cost: nothing. Weak point: the story's own words stay untested.
   - Option C: wait for the first real sign-off. S0-01 is yours and could be signed first. Cost: S0-11 cannot be Verified until then.
   - Recommendation: A. It tests the right folder without inventing a decision. Please also say whether a probe file is enough for you, or whether you want C as well.
   - *At the revision of 7 October 2026: option A was run on 6 October 2026. GitHub refused as expected, then the owner approved the test pull request and merged it himself (section 3).*
2. **Does GitHub require four checks, as the procedure now says?** Nothing on record shows it. Test 1 says "3 of 3". Recommendation: you read the rules page aloud (step 1), then tests 6 and 7 show it in practice. If the fourth check is not required, only you can add it. Not recommended: pushing straight to the release line again to read the new count. The laws forbid that push and test 1 already proved the refusal.
3. **Should the record say that test 4's pull request merged after you approved?** It records the refusal only. GitHub shows your approval at 18:27:57 UTC and the merge at 18:28:42. Recommendation: yes, as a dated addition. The existing line is not reworded.
4. **Do you accept that the first four tests, and the Owner brief, were done before any work order was accepted?** The procedure says there are no exceptions. It could not be otherwise for the first step, because the gates did not exist yet. Recommendation: accept it, have the record say so plainly (check 15), and keep the order from here on.
5. **Did you approve the security-critical changes in the starting commit?** Commit `abe946e` brought the crew's instructions, the gates, the laws and the procedure in one step, with no pull request, so GitHub holds no approval for it. The rules ask for a line saying you approved. No recommendation on the answer: it must be your own words. I recommend you write it as a comment on the Accept pull request, so it is on GitHub under your name.
   - *Answered by the owner on 6 October 2026. His words are in section 3 under "What the owner stated", and in section 6.*
6. **Did you decide to make the repository public?** CR-001 says your decision is in the gate test record. It is not there. No recommendation on the answer. I recommend the same: your words, as a comment, then copied into the record.
   - *Answered by the owner on 6 October 2026. His words are in section 3 under "What the owner stated".*
7. **Is the Owner brief part of S0-11?** It was added the same day under this story's name, but the story's acceptance criteria do not mention it and CR-001 does not either. Recommendation: yes, treat it as part of S0-11, prove it with checks 5 and 6, and leave the story's wording alone.
8. **May the two "three checks" wordings be corrected in this story?** One is the sign-off form, which every future record will copy. The other is a note at the top of a gate file. Recommendation: yes, both, in the Build pull request, which waits for you anyway. Only words change.
9. **Should the build scripts that sign the app be on your list of protected files?** The laws say anything that signs is security-critical. Six scripts mention signing or entitlements and are not on the list GitHub uses (section 3), so a change to one could merge without you. Recommendation: yes, as its own small change right after this story, with you present. I have not judged each script.

**For the lead.**

10. **Who removed the GitHub copy of `story/gate-test-ledger`?** Its pull request was closed, not merged, and the procedure describes removal only after a merge. The laws need the owner's yes to delete a branch. If he said yes, or did it himself, say so in the record. Also: this Mac still lists four branches as being on GitHub that are not: `change/gate-test-record`, `change/owner-brief`, `story/gate-test-ledger` and `story/gate-test-open`. Do not prune without his yes.
    - *The first part was answered by the owner on 7 October 2026. His answer is in section 3 under "What the owner stated". The second part stands: do not prune without his yes.*
11. **The crew's own push permission is wider than it reads.** The rule that allows pushing `story/…` branches would also match a push that names a `story/…` branch and sends it to the release line. GitHub refused exactly that kind of push in test 1, so nothing broke. Recommendation: raise it with the owner as a change to `.claude/settings.json`.
12. **The verifier's instructions do not fit this story.** They tell it to compare `story/S0-11` with the release line, but most of the work is already merged, so that comparison shows only steps 5 and 6. They also ask for a code test named for each requirement, and their exception covers document stories, not Demo. Recommendation: tell the verifier to check pull requests #1 to #4 and the two new test pull requests directly on GitHub, read-only, and to treat the acceptance checks as the proof.
13. **Should the two gate scripts have tests of their own?** They decide what may merge and nothing tests them. Recommendation: not in this story. If wanted, a change request.
14. **Several claims in the procedure have never been tested** (section 7). Recommendation: leave them to first real use, and tell the owner they are unproven so he can ask for a wider gate test if he wants one.
15. **Test 1 used a branch named `gate-test/direct`**, which is not one of the four kinds the crew may push. It was never pushed under its own name, and it is still on this Mac. Recommendation: leave it; S0-02 covers tidying.
16. **In step 3, a pull request is opened with an unfilled brief on purpose.** The crew's planning instructions say never to do that, and those instructions are the owner's, so only he can allow it. Recommendation: he allows it only as written here, once, with him present, and that pull request's title says so. His approval of this work order is that yes. Without it, step 3 does not run and checks 5 and 6 stay "not run".
    - *Accepted with the first acceptance of this work order: pull request #5, approved by the owner at 2026-10-06T20:09:40Z. The title was changed by the owner on 7 October 2026. His words are in section 3 under "What the owner stated", and the new title is in step 3.*
17. **Where are the owner's spoken answers kept?** This work order assumes: written by the lead into the record's "Additions" in his words, then confirmed by his approval of the Build pull request. Confirm, or name another place.
