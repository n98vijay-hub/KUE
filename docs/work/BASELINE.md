# Baseline

The state of the tests before the build crew changed any product code.

- Date: 6 October 2026, about 10:30 CDT
- Branch: `kue/release-1`
- Commit: `87df77058a244a7831a20f41e1971945a5fc2aac` ("The live model plan is given what the owner said, and records that it was")
- Command: `./scripts/test-kue.sh` (no `--live`, no `--live-all`)
- Result: **failed**. The script ended with "failed: shell types".

## Numbers

| Suite | Passed | Failed | Ignored | Result |
| --- | --- | --- | --- | --- |
| core (`cargo test -p lantern-core`) | 560 | 0 | 0 | passed |
| shell (`cargo test -p lantern`) | 9 | 2 | 13 | **failed** |
| window, type check (`npx tsc --noEmit`) | not counted | 1 error | not counted | **failed** |
| window, tests (`npx vitest run`) | 120 | 0 | 0 | passed, with the caveat below |

## What failed

Nothing here was fixed. The cause of each is not established unless stated.

### Shell: two tests

Both failed in the full run and again when each was run alone straight after.

1. `the_wake_boundary_hears_its_name_in_speech_made_on_this_mac`
   - Where: `src-tauri/src/lib.rs` line 2897.
   - What it reported: the test asks the sensing helper for its wake gate check and expects the answer "ok: true". The helper gave back nothing the test could read.
2. `pause_stops_every_reading_from_the_real_sensing_layer`
   - Where: `src-tauri/src/lib.rs` line 1845.
   - What it reported: the test starts the sensing helper, waits two and a half seconds and expects the helper to report that it is sampling computer activity. No such report arrived.

Both tests run the real sensing helper built in this folder. Two things were true during the run and may matter; neither was tested as the cause:

- `./scripts/status-kue.sh` reports the builds in this folder as dated 14 September 2026, older than the last commit of 24 September 2026, and says "rebuild".
- A copy of KUE was running from the old worktree `.claude/worktrees/personal-ai-prototype-continue-47ecde`, with its sensing, voice and mind helpers.

### Window: type check

- Error: `src/window.test.tsx(14,38): error TS2307: Cannot find module 'vitest' or its corresponding type declarations.`
- Established cause: `package.json` lists `vitest`, but it is not installed in `node_modules` in this folder. `node_modules` holds 23 packages and none of them is vitest.

### Window tests: a caveat on the 120

Because vitest is not installed here, `npx vitest run` did not use the project's own copy. `npx` takes a missing tool from the npm registry, or from its own cache outside this folder, and runs that. Which of the two happened was not checked. So the 120 passing tests ran on a copy of vitest that is not the project's, and its version was not checked against the one `package.json` asks for. Treat the 120 as "passed, on a tool that was not the installed one" until the packages are installed and the run is repeated.

## Not run

- The 13 ignored shell tests. They are the live tests and run only with `--live` or `--live-all`, which need the owner's request.
- No build was made.

## What this means for the crew

The Definition of Done says `./scripts/test-kue.sh` must pass. It does not pass today, before any story. Until the three failures above are dealt with, no story can honestly reach Verified on that rule. This needs the owner's decision.
