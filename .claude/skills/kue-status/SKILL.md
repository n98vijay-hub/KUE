---
name: kue-status
description: "Tell the owner in plain words where the KUE build stands. Gives percent complete earned by evidence, what waits for his signature, what is blocked and what is next. Reads only."
---

# Where the build stands

Read only. Change nothing and commit nothing.

1. Read `docs/product/backlog.json`, the top three entries of `docs/work/HANDOVER.md`, and run `./scripts/status-kue.sh` and `git branch --list "story/*"`.
2. Compute in code, not in your head, for example with a short `python3 -c`:
   - stories by status
   - percent complete = sum(points × credit) ÷ sum(points), with credit Not started 0, Planned 0.1, Built 0.4, Verified 0.8, Signed 1, Blocked 0
   - the same for the current sprint. The plan starts Monday 5 October 2026 and each sprint is seven days.
3. Answer in this order, in plain words, with no code terms:
   - **Percent complete**, overall and for this sprint, with the points behind it.
   - **Waiting for your signature:** each Verified story, one line each, and the file to read (`docs/work/<id>/evidence.md`).
   - **Waiting for you at the Mac:** live checks from the handover.
   - **Blocked:** each story and what would unblock it.
   - **Yours:** owner stories due this sprint.
   - **Next:** the story `/kue-next` would take.
   - **Odd things:** a dirty checkout, a story branch with no status, a failing test.

Never round a number up. A story that was built and not verified counts 40%, not done.
