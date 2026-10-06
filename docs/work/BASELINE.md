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

- The 13 ignored shell tests. *Corrected 6 October 2026: this line first said that all 13 are the live tests run by `--live` or `--live-all`. That was wrong. The flags run 5 of them. The rest are listed under "The ignored shell tests" at the end of this file.*
- No build was made.

## What this means for the crew

The Definition of Done says `./scripts/test-kue.sh` must pass. It does not pass today, before any story. Until the three failures above are dealt with, no story can honestly reach Verified on that rule. This needs the owner's decision.

## Second run

- Date: 6 October 2026, 10:47 CDT
- Branch: `kue/release-1`
- Commit: `bd8d2685e5653f6e0bad9ca8dbae7ec85b03afb2` ("chore(S0-05): install the build crew"). No product source file differs from the first run.
- Command: `./scripts/test-kue.sh` (no `--live`, no `--live-all`)
- Result: **passed**. The script ended with "everything that ran passed".

### What changed between the two runs

Three things, all at the owner's instruction. No source file was changed.

1. **Window packages installed** with `npm ci`. It installed 50 packages, exactly as `package-lock.json` lists them. `package.json` and `package-lock.json` were checked before and after and are unchanged. vitest 5.0.1 is now installed in this folder.
2. **Sensing helper rebuilt** with `./sensing/build.sh`, and nothing else rebuilt. The helper went from the build of 14 September 2026 (467 KB) to a build of today (875 KB). Asked directly for its wake gate check, the new helper answers "ok: true, no failures".
3. **The owner quit the KUE app** that had been running from the old worktree. `./scripts/status-kue.sh` reported "KUE is not running" before the run.

### Numbers

| Suite | Passed | Failed | Ignored | Result |
| --- | --- | --- | --- | --- |
| core (`cargo test -p lantern-core`) | 560 | 0 | 0 | passed |
| shell (`cargo test -p lantern`) | 11 reported: 8 really ran, 3 stopped early | 0 | 13 | passed as reported; 3 of the passes proved nothing |
| window, type check (`npx tsc --noEmit`) | not counted | 0 errors | not counted | passed |
| window, tests (`npx vitest run`) | 120 | 0 | 0 | passed, on the project's own vitest |

### Three of the eleven shell passes stopped early

Added 6 October 2026, after the second run, by running the shell suite once more with its hidden output shown.

Three tests begin by looking for a helper. When the helper is not built in this folder, the test prints "skipped" and ends, and the test tool counts that as a pass. The script shows only the totals, so this was not visible. The same three stopped early in the first run.

| Test | What it printed | Helper it needs | Built by |
| --- | --- | --- | --- |
| `the_authentication_helper_reports_this_macs_hardware` | skipped: kue-auth not built | the authentication helper, `auth/bin/kue-auth` | `./auth/build.sh` |
| `the_real_macos_voice_speaks_a_cleared_sentence_and_the_kill_switch_cuts_it_off` | skipped: kue-voice not built | the voice helper, `voice/bin/kue-voice` | `./voice/build.sh` |
| `the_shell_owns_exactly_one_model_process` | skipped: mind not built | the on-device model helper, `mind/bin/lantern-mind` | `./mind/build.sh` |

So the honest second-run count for the shell suite is: 8 really ran and passed, 3 stopped early, 13 ignored. None of the 560 core tests printed "skipped".

### What was established about the first run's failures

- **Type check:** established. It failed because vitest was not installed in this folder. With it installed, the type check passes. The caveat on the 120 window tests no longer applies.
- **The two shell tests:** partly established. Both now pass. Two things changed together, the rebuilt helper and the quit app, so this run does not say which one mattered. What is known: the old helper was built ten days before the code it was tested against and was about half the size of the new one, and the wake test's first step is to ask the helper for a check that the new helper answers. That points to the old build. It was not proved by testing each change alone.

### Not run, and not checked

- The 13 ignored shell tests, as in the first run.
- No app build was made. `./scripts/status-kue.sh` still reports the app builds in this folder as older than the code.
- The other helpers (voice, mind, auth, act) were not rebuilt.

### Noticed, not fixed

- `npm ci` reported one known weakness rated high in an installed package, `source-map-js` (advisory GHSA-68fv-2mgg-jv7q, a way to make a program hang with a crafted file). It comes in through vite, the tool that builds the window, and is not shipped in the app. Fixing it would change `package-lock.json`. **Waiting for the owner's approval of a change to `package-lock.json`.** The owner said on 6 October 2026: not now.
- The Swift compiler printed warnings while building the sensing helper. The build succeeded. They were not examined.

## Third run

- Date: 6 October 2026, 11:32 CDT
- Branch: `kue/release-1`
- Commit: `28f0115ae7adf6a13b46f21c0f866c3eff7a50dd` ("chore(S0-05): baseline second run and corrections"). No product source file differs from the first or second run.
- Command: `./scripts/test-kue.sh`, run once (no `--live`, no `--live-all`)
- Result: **passed**. The script ended with "everything that ran passed". All 11 shell tests really ran.

### What changed since the second run

Four helpers were built in this folder, one at a time, each with its own build script, at the owner's instruction and with him at the Mac. No source file was changed. KUE was not running.

Each script was read before it was run. None downloads anything. None asks for a permission while building. Each signs what it builds with a local ad-hoc signature (no certificate, no keychain, no Apple account); the owner approved that before the first build.

| Helper | Script | Result | What it built |
| --- | --- | --- | --- |
| authentication | `./auth/build.sh` | built, signature verifies | `auth/bin/kue-auth` |
| voice | `./voice/build.sh` | built, signature verifies | `voice/bin/kue-voice` |
| on-device model | `./mind/build.sh` | built, signature verifies, 4 compiler warnings not examined | `mind/bin/lantern-mind` |
| actions | `./act/build.sh` | built, signature verifies | `act/bundle/KueAct.app` |

All four outputs are ignored by git. The sensing helper built for the second run was left as it was.

### Numbers

| Suite | Passed | Failed | Ignored | Result |
| --- | --- | --- | --- | --- |
| core (`cargo test -p lantern-core`) | 560 | 0 | 0 | passed |
| shell (`cargo test -p lantern`) | 11: all 11 really ran, 0 stopped early | 0 | 13 | passed |
| window, type check (`npx tsc --noEmit`) | not counted | 0 errors | not counted | passed |
| window, tests (`npx vitest run`) | 120 | 0 | 0 | passed |

### Did the three tests really run this time

The script shows only totals, so this was checked by running the shell suite once more straight afterwards with its hidden output shown. That extra run also gave 11 passed, 0 failed, 13 ignored, and the word "skipped" did not appear anywhere in its output.

| Test | Second run | Third run | What was seen |
| --- | --- | --- | --- |
| `the_authentication_helper_reports_this_macs_hardware` | stopped early | **really ran, passed** | The helper answered with this Mac's authentication hardware: Touch ID present and available |
| `the_real_macos_voice_speaks_a_cleared_sentence_and_the_kill_switch_cuts_it_off` | stopped early | **really ran, passed** | The voice helper started, named its default voice and reported a start time of about 54 thousandths of a second |
| `the_shell_owns_exactly_one_model_process` | stopped early | **really ran, passed** | It printed no "skipped" line and passed. Its only quiet exit is the "mind not built" one, and the helper is now built |

### Not run, and not checked

- The 13 ignored shell tests, as before. No live test was run.
- No app build was made. `./scripts/status-kue.sh` still reports the app builds in this folder as older than the code.
- Whether any test in the standard run uses the actions helper was not checked. None stopped early for lack of it.
- After the runs, no helper process was left running.

### What this means for the crew

`./scripts/test-kue.sh` now passes in this folder with every helper built, and every test in the standard run really runs. This is the baseline a story is measured against. It holds only while the helpers stay built: on a fresh copy of the code without them, the script would again report passes for tests that stop early. That fault is assigned to S4-05.

## The ignored shell tests

The shell suite reports 13 ignored. That is 12 different tests; one is counted twice.

**Run by `--live` (3):**
- `storage_live_measures_this_mac_and_the_window_gets_only_what_it_may_show`
- `trash_live_moves_kues_own_files_to_the_real_trash_and_puts_them_back`
- `a_real_question_is_answered_on_device_from_the_cleared_context_only`

**Run by `--live-all` as well (2):**
- `open_chrome_live_resolves_the_installed_app_and_the_executor_verifies_it`
- `the_tampa_request_runs_on_this_macs_real_folders_finder_and_voice`

**Run by neither flag (7).** Each runs only when someone names it by hand:
- `conversation_live_clean_up_then_a_correction_then_do_it_on_the_real_trash`
- `model_latency_against_prompt_size` (this is the one counted twice)
- `s8_plan_runs_three_declared_tools_in_kue_on_this_mac`
- `s8b_plan_from_the_real_model`
- `s9_memory_on_this_mac_survives_the_process_that_made_it`
- `s9_second_process_reads_what_the_first_one_kept`
- `s10_memory_changes_the_plan_on_this_mac_and_nothing_else`

### Two faults found while counting. Not fixed.

Both are in `src-tauri/src/lib.rs`, around lines 2041 to 2111, and both come from one misplaced line. On 6 October 2026 the owner assigned both, together with the tests that stop early and still count as passed, to story S4-05. The note on S4-05 in `docs/product/backlog.json` lists file and line.

1. **`--live` names a test that does not exist as a test.** The script's `--live` list has a fourth name, `ask_the_real_model`. That function is in the code, but the line that marks it as a test sits above the next function instead. So the name matches nothing. Asked to list what that name would run, the test tool answered "0 tests". When nothing runs, the tool reports success, so with `--live` the script would print a tick for `ask_the_real_model` although nothing ran. This was found by listing only; `--live` itself was not run.
2. **`model_latency_against_prompt_size` carries the test mark twice**, which is why it is counted twice among the 13.
