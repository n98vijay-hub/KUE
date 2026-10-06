# Gate test of KUE's change control

Story S0-11. The test proves that GitHub, not habit, enforces the rules in `docs/process/CHANGE-CONTROL.md`.

| | |
| --- | --- |
| Date | 6 October 2026 |
| Repository | `n98vijay-hub/KUE` on GitHub |
| Visibility | Public |
| History | Published as a clean start on 6 October 2026. The earlier history is kept privately and is not on GitHub |
| Run by | the crew account `vijayn2698`, with the owner present |
| Starting point | `kue/release-1` at commit `abe946e` |

## Results

| # | Test | What was done | What GitHub answered | Expected? |
| --- | --- | --- | --- | --- |
| 1 | A direct push is refused | Branch `gate-test/direct` from `kue/release-1`, one line "Gate test, 6 October 2026" added to `docs/work/HANDOVER.md` (commit `5626101`), then `git push origin gate-test/direct:kue/release-1` | Refused: "GH013: Repository rule violations found for refs/heads/kue/release-1. Changes must be made through a pull request. 3 of 3 required status checks are expected." `kue/release-1` stayed at `abe946e` | Yes |
| 2 | Ordinary work merges on green checks, without the owner | The same commit pushed as `story/gate-test-open`, pull request #1 "Gate test 2: no protected file". Waited for the checks, then `gh pr merge 1 --merge` | Ledger rules, Core tests and Window tests passed. Merged by `vijayn2698` with no approval, as commit `7d3336a` | Yes |
| 3 | The Ledger rules refuse a false signature | Branch `story/gate-test-ledger`, S0-08 set to Signed in `docs/product/backlog.json`, pull request #2 "Gate test 3: signed without a record" | Ledger rules failed: "S0-08 is Signed but there is no sign-off record at docs/signoff/S0-08.md", and two more problems. Core tests and Window tests passed. GitHub marked it blocked. Closed without merging | Yes |
| 4 | The owner's files wait for him | Branch `change/gate-test-record` adding this file under `docs/process/`, which only the owner may approve. Waited for the checks, then `gh pr merge <number> --merge` | Pending | Pending |
