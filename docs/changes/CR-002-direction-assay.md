# CR-002: Direction change to Assay, and the decisions for its first slice

| | |
| --- | --- |
| Raised | 15 October 2026, by the owner |
| Kind | Change of direction (replaces the 5 October back-office direction) |
| Decision | Taken by the owner in sessions of 15 and 21 October 2026, recorded below in his words. Becomes binding when he approves the pull request that carries this file |

## What is asked

KUE stops being built as a private back office for daily life. The repository becomes the home of **Assay**: a layer every AI agent on the owner's computer connects through before it acts. Assay holds what needs the owner, stops what breaks his rules, verifies what happened, and keeps a tamper-evident receipt. Later it learns which actions he would approve.

The evidence for this change is `docs/assay/PHASE-1-STARTING-POINT.md`. That report found KUE's trust core sound (560 core tests and 40 window tests pass, and the core also passes on Linux). The back-office product it would serve has no code.

## The owner's decisions

| # | Decision | Owner's answer | When |
| --- | --- | --- | --- |
| Direction | Which direction to lock | "E: Assay + learns my judgment" | 15 Oct 2026 |
| Users | Who it is for | "all the people, mac or windows or linux." | 15 Oct 2026 |
| Docs | Where the documents live | "In the KUE repo + one index page" | 15 Oct 2026 |
| Budget | Monthly budget | "plan we will fix what next." Deferred | 15 Oct 2026 |
| D-2 to D-6 | Receipt design with an erasable details table (C-1), the `sha2` dependency, camera off with Touch ID for every held action, the `~/AssayLab` test folder, freezing the back-office backlog | "Approve all five" | 21 Oct 2026 |
| D-1 | NUX | "Stop reminders, tell users, take site down" | 21 Oct 2026 |
| D-7 | TRIAD's public page with false certification badges | Remove it, by deleting the Vercel project in the owner's Chrome with him approving | 21 Oct 2026 |
| Interviews | Customer interviews alongside the build | "Yes, write the script and tracker" | 21 Oct 2026 |

## What changes

1. **Product statement.** The "What KUE is" section of `CLAUDE.md` is replaced with the text in the appendix. `CLAUDE.md` is a protected file, so this needs the owner's approval on the pull request.
2. **Backlog frozen.** The 99 back-office stories in `docs/product/backlog.json` and the 133 requirements in `docs/product/requirements.json` stop being worked on. They are not deleted. A frozen copy goes on the branch `archive/back-office-2026-10` and stays there. On the release line, every back-office story not yet Built gets the status **Blocked**, with the reason "Frozen by CR-002".
3. **New epic A1, "First slice through the gate".** Its first story is A1-01 (SLICE-01). It has its own work order in `docs/work/A1-01/`.
4. **Laws.** Three are added to `docs/product/rules.md`, wording to be approved by the owner:
   - **BR-21.** The judgment layer may reduce how often Assay asks. It may never widen what is allowed. Stop rules and the strictest action types are decided by fixed code only.
   - **BR-22.** Assay promises only what it can prove. It gates what is routed through it, reports what it can see going around it, and publishes the list of what it cannot stop.
   - **BR-23.** A held action is approved by the operating system's own check (Touch ID on macOS), once per action. The approval is bound to a digest of that exact action. A face seen by the camera is never an approval.
5. **New dependency.** `sha2`, used only by the new `assay` crate. Approved under D-3.

## What it displaces (BR-17)

Everything left in the back-office plan: 96 stories marked "Not started" and one marked "Planned" (99 stories in all, 379 points, with 0.8 points earned). Sprint 0 work that served only the back office stops. The change-control machinery, the build crew and the gate tests carry over unchanged.

## What it costs and risks

- **Two months of planning go on the shelf.** The 5 October plan, requirements and prototype are kept, not lost.
- **The repository is public** (CR-001). Assay's direction, reports and work orders become visible to anyone once merged. If that is not wanted, the owner should decide before this pull request merges: make the repository private, or keep strategy documents out of it.
- **Nothing is proven about demand.** The interview kit in `docs/assay/interviews/` runs alongside the slice, to find out whether anyone will pay.

## Appendix: proposed "What KUE is" text for CLAUDE.md

> KUE is the codebase of **Assay**, a local layer that every AI agent on a person's computer connects through before it acts. It holds what needs the person, stops what breaks their rules, verifies the result independently, and keeps a tamper-evident receipt of every decision. It is built from KUE's Rust core (states, authorisation, path confinement, kill switch, privacy firewall). The camera, voice, on-device model, plans and the back-office direction are no longer part of the product; their code stays in the history.
>
> The current work is epic A1, "First slice through the gate": one file write from Claude Desktop, held, approved with Touch ID, verified against the file on disk and receipted. See `docs/work/A1-01/work-order.md`.
>
> The product owner is Vijay. He does not read code. He gives the work, runs the tests that need a person at the Mac, and is the only one who signs.
