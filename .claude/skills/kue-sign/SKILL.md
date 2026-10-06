---
name: kue-sign
description: "Walk the owner through testing a verified KUE story himself, write the sign-off record from what he reports, and open the sign-off pull request he approves on GitHub. Only the owner runs this, by typing it himself."
argument-hint: "<story id> [interim | reject <reason>]"
disable-model-invocation: true
---

# The owner tests and signs a story

Arguments given: `$ARGUMENTS`

This command prepares the owner's signature. It never gives it. The signature is his approval, on GitHub, of the pull request this command opens. Run it only because the owner typed it in this session. Never run it in an unattended run, and never on your own initiative.

## 1. Check the ground

1. Main folder, not a worktree. Branch `kue/release-1`, no modified or staged files.
2. `gh auth status` must show the crew account, never `n98vijay-hub`.
3. `git fetch origin`, then `git pull --ff-only origin kue/release-1`.
4. Find the story in `docs/product/backlog.json`. Its status must be `Verified`, or its `role` must be `Owner` or `Product owner`. If not, say what its status is and stop.
5. If a pull request from `signoff/<id>` is already open, say so, give its link and stop.

## 2. Show what was proved

In plain words, from `docs/work/<id>/evidence.md` and `docs/work/<id>/review.md`:

- what was proved, and by what
- what was not checked
- the checks that wait for him

For an owner story there is no evidence file. Ask him what was done and what record he keeps of it.

## 3. He tests it himself (UAT)

Note the commit on `kue/release-1` now: `git rev-parse --short HEAD`. That is the build under test.

Make the list of what he will test: every check in the evidence marked "not run" because it waits for him, and every acceptance check in the work order that a person can see or do. For a document story, the test is reading the document.

If a check needs the app running, tell him which build he is about to test and ask before building or starting anything. The copy of KUE he normally runs may come from an older folder; say so if `./scripts/status-kue.sh` shows it.

Then go through the list one check at a time:

1. Tell him exactly what to do and what he should see.
2. Wait for his answer.
3. Write down what he says he saw, in his words, and the result: Passed, Failed or Not done.

Never fill in a result he did not give. A check he skips is Not done.

## 4. His decision

Ask him to choose. Use the argument if he gave one, and still do steps 2 and 3 first.

- **Approved:** offered only when every check he tested passed and none is Not done.
- **Interim:** he accepts it with a condition. Ask for the condition and a date. Any Failed or Not done check is named in the condition.
- **Rejected:** ask for his reason.

## 5. Write the record and open the pull request

1. `git switch -c signoff/<id>` from `kue/release-1`.
2. Write `docs/signoff/<id>.md` in the form given in `docs/signoff/README.md`. If the file exists, add a new dated section below the old ones and change nothing above it. Fill the pull request numbers by looking them up: `gh pr list --state merged --search "<id> in:title" --json number,title`.
3. In `docs/product/backlog.json`, for this story only:
   - **Approved:** status `Signed`, `warrant: "Approved"`, `signoff: "docs/signoff/<id>.md"`.
   - **Interim:** status `Signed`, `warrant: "Interim"`, the condition in `note`, `signoff` as above.
   - **Rejected:** status `Planned`, his reason in `note`.
   Add `updatedAt`.
4. Commit both files by name: `signoff(<id>): record for the owner, <decision>`.
5. `git push -u origin signoff/<id>`.
6. Open the pull request into `kue/release-1`, titled `Sign-off <id>: <story title> (<decision>)`. Write the body to a file outside the repository and pass it with `--body-file`. It starts with the owner's brief, exactly in the form of `.github/pull_request_template.md`: the heading `## For the owner` and its nine parts, in plain words, at most 350 words. "What this is" says this is his signature on the story. "How it was checked" gives the count of checks he tested himself and their results. "Your action" is "Approve". Then the heading `## Details`, **Kind:** Sign-off, and the new section of the record, then: "**To sign:** open the Files changed tab, press Review changes, choose Approve, and submit. Your approval is your signature. Writing a comment is not an approval. To change the record, write a comment instead."
7. `git switch kue/release-1`.

## 6. Say it

Tell him: the link, that the story is not signed until he approves there, and the percent complete as it will stand after the merge.

## Limits

- Never approve, review or merge this pull request. Merging happens after his approval, by him or by the next `/kue-next` run.
- Never write a result, a decision or a condition he did not state.
- Never change an older section of a sign-off record.
- Commit by naming paths. Never push to `main` or `kue/release-1`.
