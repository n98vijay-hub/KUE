---
name: kue-status
description: "Tell the owner in plain words where the KUE build stands. Gives percent complete earned by evidence, what waits for his approval on GitHub, what waits for his test at the Mac, what is blocked and what is next. Reads only."
---

# Where the build stands

Read only. Change no file, commit nothing, push nothing, merge nothing.

1. `git fetch origin`. Read the backlog as it stands on GitHub: `git show origin/kue/release-1:docs/product/backlog.json`. Read the top three entries of `docs/work/HANDOVER.md`. Run `./scripts/status-kue.sh`.
2. List the open pull requests: `gh pr list --state open --json number,title,headRefName,isDraft,reviewDecision,statusCheckRollup,url`.
3. Compute in code, not in your head, for example with a short `python3 -c`:
   - stories by status
   - percent complete = sum(points × credit) ÷ sum(points), with credit Not started 0, Planned 0.1, Built 0.4, Verified 0.8, Signed 1, Blocked 0
   - the same for the current sprint. The plan starts Monday 5 October 2026 and each sprint is seven days.
   Only what is merged into `kue/release-1` counts. Work in an open pull request has earned nothing yet.
4. Answer in this order, in plain words, with no code terms:
   - **Percent complete**, overall and for this sprint, with the points behind it.
   - **Waiting for your approval on GitHub:** each open Accept, Sign-off, Change or Release pull request, and any Build pull request that touches a protected file. One line and the link for each.
   - **Waiting for your test at the Mac:** each Verified story with no open sign-off pull request. For each: type `/kue-sign <id>`.
   - **In flight:** open Build pull requests and the state of their checks.
   - **Blocked:** each `Blocked:` pull request and what would unblock it.
   - **Yours:** owner stories due this sprint.
   - **Next:** what `/kue-next` would build, or that `/kue-plan` is needed first.
   - **Odd things:** a failing check on `kue/release-1`, local files changed and not committed, `gh` signed in as the owner instead of the crew, a local `kue/release-1` that differs from GitHub's.

Never round a number up. Never count work that is not merged.
