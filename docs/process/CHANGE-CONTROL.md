# Change control for KUE

Version 1.0, 6 October 2026. Owner: Vijay (`n98vijay-hub`). A change to this document needs his approval.

This is how work enters KUE, how it is proved, and how it is signed. GitHub enforces it: the rules below are not a habit, they are settings that refuse a merge.

## Who does what

| Who | GitHub account | May | May not |
| --- | --- | --- | --- |
| The owner | `n98vijay-hub` | Accept a work order, test at his Mac, sign, approve a change to a protected file, cut a release, change the rules | Be bypassed. The rules apply to him too |
| The build crew | `vijayn2698` | Write work orders, code, tests and evidence. Open pull requests. Merge a build that passed every check | Approve anything. Change a protected file without approval. Change the repository's settings. Push straight to a protected branch |

The crew account is a machine account. It has no approval rights because GitHub does not let the author of a pull request approve it, and because it is not a code owner.

## The three gates for every story

| Gate | Earned credit | What the owner sees | What he does | What GitHub enforces |
| --- | --- | --- | --- | --- |
| **1. Acceptance** | Planned, 10% | A pull request named `Accept <story>`. It holds the work order: what will be built in plain words, and the checks that will prove it | Reads it. Approves, or comments what to change | The work order is a file only he owns. No approval, no merge |
| **2. Build and test** | Verified, 80% | A pull request named `Build <story>`: the change, the test results, the independent evidence, the safety review | Nothing, unless it touches a protected file | Four checks must pass on GitHub's own machine: Ledger rules, Core tests, Window tests, Owner brief. A story cannot become Verified without an accepted work order, evidence that says VERIFIED and a review that says PASS |
| **3. UAT and sign-off** | Signed, 100% | A pull request named `Sign-off <story>`. It holds the record of what he tested himself and what he saw | Tests at his Mac with `/kue-sign`. Approves the record | The record is a file only he owns. A story cannot become Signed without it. His approval is the signature; GitHub keeps who and when |

UAT means user acceptance testing: the owner uses the thing himself and says what he saw. A check he did not do is recorded as not done. It is never recorded as passed.

## What every pull request tells the owner

Nobody should approve what he cannot read. So every pull request, of every kind, opens with a brief written for the owner in plain words, on one screen:

| Part | What it answers |
| --- | --- |
| What this is | One sentence |
| What changes if you approve | Each file or group, named for what it is: what it is now, what it becomes |
| What does not change | What a reader might fear is touched and is not |
| Why | The story or the request it comes from |
| How it was checked | Each check, who or what ran it, the result |
| Not checked | What, and why |
| After you approve | What happens next, and who does it |
| To undo | How it is reversed if it turns out wrong |
| Your action | Approve, test first, or none |

A fourth check, **Owner brief**, reads the description and refuses a pull request whose brief is missing a part, is longer than one screen, or still has a placeholder in it. The check cannot tell whether the brief is true or clear. If a brief does not let the owner see what he is approving, he writes a comment saying so and does not approve.

A comment is never an approval. Only the Approve button is.

## The other two kinds of change

| Kind | When | What GitHub enforces |
| --- | --- | --- |
| **Change request** | The scope changes: a story is added, removed, resized or reworded, or a requirement changes | The backlog cannot change scope without a file under `docs/changes/`, which only the owner approves. New scope names the work of equal size it displaces (BR-17) |
| **Release** | A phase gate: `kue/release-1` goes into `main` | A pull request into `main` needs the owner's approval, whatever it contains. Only the owner can create a release tag |

## Protected files

A pull request that touches any of these waits for the owner, whatever kind it is. The list that GitHub uses is `.github/CODEOWNERS`.

- Work orders, sign-off records, change requests, this procedure
- The laws (`docs/product/rules.md`) and the requirements
- The gates themselves: `.github/`, and the crew's instructions in `.claude/` and `CLAUDE.md`
- Dependencies: the Cargo and npm manifests and lock files
- Security-critical code: authorisation, privacy, safety, actions, transactions, capabilities, the approval helper, the app's permissions

## Branches

| Branch | What it is | How it changes |
| --- | --- | --- |
| `main` | What has been released | Only by a release pull request the owner approved |
| `kue/release-1` | The integration line. Everything on it passed the checks | Only by pull request |
| `accept/<story>`, `story/<story>`, `signoff/<story>`, `change/<name>` | One per pull request | The crew pushes these. GitHub deletes its copy after the merge |

Nobody pushes straight to `main` or `kue/release-1`, nobody force-pushes, nobody deletes them. The bypass list is empty.

## The record

Every claim traces to something a person can open.

| Question | Where the answer is |
| --- | --- |
| What was asked for? | The requirement in `docs/product/requirements.json` and the story in `docs/product/backlog.json` |
| What did the owner agree to build? | `docs/work/<story>/work-order.md`, and his approval on the Accept pull request |
| What was built? | The Build pull request and its commits |
| Was it tested by a machine? | The check runs on the Build pull request |
| Was it tested independently of whoever built it? | `docs/work/<story>/evidence.md` |
| Does it break a law? | `docs/work/<story>/review.md` |
| Did the owner test it himself? | `docs/signoff/<story>.md` |
| Who signed, and when? | His approval on the Sign-off pull request |
| What was released? | The release tag on `main` |

## What this procedure does not prove

- The checks on GitHub run the core and window tests. The shell tests, and anything that needs a camera, microphone, Touch ID or the screen, run only on the owner's Mac. They are covered by the verifier's evidence and by UAT, not by GitHub.
- The evidence and the safety review are written by agents. They are independent of the builder, and they are still not a human review.
- The owner's approval proves he approved. It does not prove he read the record. Once a sprint he audits one signed story end to end (BR-16).

## Exceptions

There are none. If a rule is wrong, the owner changes the rule through a pull request to this file or to the repository's rulesets, and the change is itself on the record.
