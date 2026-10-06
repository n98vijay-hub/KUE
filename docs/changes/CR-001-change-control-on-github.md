# CR-001: Change control on GitHub

| | |
| --- | --- |
| Raised | 6 October 2026, by the owner |
| Kind | New scope |
| Decision | Accepted by the owner, 6 October 2026 |

## What is asked

Keep the work the way an enterprise would: the repository on GitHub, protected branches, checks that must pass, and acceptance, UAT and sign-off that can be seen and cannot be skipped.

## What changes

1. A new story enters Sprint 0:
   **S0-11. Change control on GitHub: protected lines, required checks, and acceptance and sign-off gates proven by a gate test.** 3 points. Lead role: Architect. Needs the owner at the Mac.
   Acceptance: given the rulesets are on, when the crew pushes straight to `kue/release-1`, GitHub refuses; when a pull request touches no protected file and its checks pass, it merges without approval; when a pull request adds a sign-off record, it cannot merge until the owner approves; when a pull request marks a story Signed with no record, the Ledger rules check fails. Each result is recorded in `docs/process/GATE-TEST.md`.
2. The crew's way of working changes from local merges to pull requests. The procedure is `docs/process/CHANGE-CONTROL.md`.
3. Two laws are reworded in `docs/product/rules.md`: the crew now pushes its own work branches, and never a protected one.

## What it displaces (BR-17)

This adds 3 points to Sprint 0. Proposal for the owner to accept or change:

- S0-03, "Private off-machine backup of the repository" (2 points), keeps its points but changes meaning: GitHub now holds the code, so S0-03 covers only what is not on GitHub: the planning folder, KUE's own data, and the full local history.
- S0-06, "Keep-awake setup and watchdog" (2 points), moves to Sprint 1. It is needed for the night shift, which does not start until three stories have gone through with the owner watching.
- Because S0-06 moves, the Foundation epic E0 now runs from Sprint 0 to Sprint 1.

Net effect on Sprint 0: plus 1 point.

## What it costs and risks

- Every story now needs the owner twice on GitHub: once to accept, once to sign.
- A public repository shows the source, the documents and the history to everyone. See the decision recorded by the owner in the gate test record.
- The checks on GitHub cover the core and window tests only. The shell tests stay on the owner's Mac.
