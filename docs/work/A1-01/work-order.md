# Work order A1-01: One file through the gate (SLICE-01)

Story: A1-01, epic A1 "First slice through the gate". Lead role: Architect. Evidence type: Demo plus automated tests. Owner at the Mac: Yes, for the user acceptance test only. Unattended: No. The story adds an approval path, and AS-08 forbids unattended runs on security code.
Serves: change request CR-002. Design source: `docs/assay/PHASE-1-STARTING-POINT.md` §6.
Written 21 October 2026. Nothing is built until the owner approves the `Accept A1-01` pull request.

## 1. In plain words

Claude Desktop will get its file tools through Assay instead of directly. When Claude asks to create a new file in `~/AssayLab`:

- Assay stops the request.
- macOS shows a Touch ID prompt with the file's name, size and a short fingerprint of its content.
- Only if you approve does Assay pass on exactly what you approved.
- Assay then opens the file itself and checks it matches.
- Assay writes a receipt that cannot be changed or cut short without it showing.

Anything outside the lab folder, any overwrite and any unknown tool is refused without asking you. Nothing in the KUE app changes. Assay is a new, separate program in the same repository.

## 2. Acceptance checks

"CI" means the check runs on GitHub for macOS and Linux, with a stand-in for Touch ID and a stand-in file server. "UAT" means you do it yourself at your Mac with real Claude Desktop and real Touch ID.

| # | Check | CI | UAT |
|---|---|---|---|
| 1 | A new file inside the lab, approved: the file exists, its SHA-256 equals the approved content, the receipt says Verified, and Claude gets success | ✓ | ✓ |
| 2 | Declined: no file, the receipt says "declined by owner", and Claude gets an error that says so | ✓ | ✓ |
| 3 | No answer for 60 seconds: treated as declined, no file | ✓ | ✓ |
| 4 | A path outside the lab: stopped with no prompt, and a receipt with the reason | ✓ | ✓ |
| 5 | A link inside the lab pointing outside it: stopped | ✓ | |
| 6 | The file already exists: stopped, and the file is byte-for-byte unchanged | ✓ | |
| 7 | The request changes between approval and sending (fault injected): nothing is sent, and the refusal is receipted | ✓ | |
| 8 | A stand-in server reports success but writes different bytes: the receipt says Failed and gives both hashes | ✓ | |
| 9 | Assay is killed hard right after replying: on restart the receipt is there and `assay receipts verify` says intact | ✓ | |
| 10 | One receipt edited directly in the database: `verify` names the broken record. The newest receipt deleted: `verify` catches it through the anchor (the TRIAD T-1 case) | ✓ | |
| 11 | Kill latch set: every write stopped, no prompt | ✓ | ✓ |
| 12 | The file server's read-only tools pass through with a receipt. Any other tool is stopped (BR-18). The tool names are checked against the pinned server version | ✓ | |
| 13 | Over 200 read calls, Assay adds at most 50 ms at the 95th percentile, measured and recorded | ✓ | |
| 14 | The `assay` binary has no network library in its dependency tree and opens no connection during the tests | ✓ | |
| 15 | `kue-auth` replaced by a different binary: Assay refuses to prompt and stops the write | | ✓ |
| 16 | Camera off and the KUE app not running: approval works through Touch ID alone | | ✓ |
| 17 | `assay receipts show` lists each receipt in plain words: time, agent, action, decision, result | ✓ | ✓ |
| 18 | `assay uninstall` restores Claude Desktop's configuration file byte for byte | ✓ | ✓ |

Each check has a test named for it, for example `a1_01_check_08_lying_server_is_failed`.

## 3. Steps, by owner

| Step | Who | What | Paths |
|---|---|---|---|
| 1 | kue-core-builder | New crate `assay-gate` in the workspace, producing the binary `assay`. Depends on `lantern-core` for path confinement, the kill latch and action states, on the existing `serde_json`, and on `sha2` (approved under D-3) | `assay/` (new), root `Cargo.toml`, `Cargo.lock` |
| 2 | kue-core-builder | Standard input/output pass-through for MCP messages. `tools/call` is intercepted; everything else is forwarded unchanged | `assay/src/proxy.rs` |
| 3 | kue-core-builder | Normalise `write_file` into an action of class `file.create`. Apply the scope rules with `PermittedRoots::resolve`: inside the lab, no link escape, not existing, at most 20,000 characters | `assay/src/action.rs` |
| 4 | kue-core-builder | Digest the action in canonical JSON. Hold it. Call the approver. Re-check the request bytes against the digest before sending | `assay/src/gate.rs` |
| 5 | kue-core-builder | Approver for macOS: run `kue-auth strong "<prompt>"`, after checking the helper's code-signature hash against the one recorded when it was built. A stand-in approver for CI | `assay/src/approve.rs` |
| 6 | kue-core-builder | Independent check of the written file: not a link, still inside the lab, SHA-256 compared. Outcomes are Verified, Failed or Unconfirmed | `assay/src/verify.rs` |
| 7 | kue-core-builder | Receipt store: SQLite with WAL and full sync. A hash chain over record headers, with a separate erasable details table (C-1). Triggers that refuse UPDATE and DELETE on headers. An anchor file holding the head sequence number and hash. Commands `assay receipts verify` and `assay receipts show` | `assay/src/receipts.rs` |
| 8 | kue-core-builder | `assay install` and `assay uninstall` for Claude Desktop's configuration, keeping a byte-exact backup | `assay/src/install.rs` |
| 9 | kue-core-builder | Tests for checks 1 to 14, 17 and 18, plus a stand-in file server that can be told to lie | `assay/tests/` |
| 9a | the lead | GitHub's checks today run only the KUE core tests, and only on macOS. Add a job "Assay tests" that runs the `assay-gate` tests on macOS and on Linux for every pull request. For GitHub to refuse a merge when it fails, the owner adds it to the required checks for `kue/release-1` and `main`; the crew cannot change those settings. Added 21 October 2026 on the owner's instruction | `.github/workflows/kue-checks.yml` |
| 10 | kue-mac-builder | Build script records `kue-auth`'s code-signature hash for step 5. **`auth/` itself is not changed** | `scripts/` |
| 11 | kue-verifier | Separate run: evidence for every check | `docs/work/A1-01/evidence.md` |
| 12 | kue-guardian | Law review, including BR-21 to BR-23 once they are accepted | `docs/work/A1-01/review.md` |
| 13 | Owner | User acceptance test at the Mac: checks 1 to 4, 11 and 15 to 18, about 45 minutes, with `/kue-sign` | `docs/signoff/A1-01.md` |

Two things are deliberately not touched: `core/src/authz.rs` and the KUE app. The slice does not need KUE's camera-based levels, so KUE's own authorisation is left exactly as it is.

## 4. Protected paths this story touches

| Path | Why | Rule |
|---|---|---|
| root `Cargo.toml`, `Cargo.lock`, `assay/Cargo.toml` | Adds the new crate and `sha2` | Dependency files need the owner's approval on the pull request |
| `scripts/` (build script for step 10) | Records a code-signature hash | Anything that signs or concerns signing needs the owner present |
| `.github/workflows/kue-checks.yml` (step 9a) | Runs Assay's tests on GitHub on macOS and Linux | The gates are the owner's; a change needs his approval on the pull request |

## 5. Not in this story

Overwrite, edit, move, delete and undo. Any agent other than Claude Desktop. Agent hooks and the shell. Windows and Linux approval prompts (the CI tests still run on Linux). Any window or owner console. Learning. Earned autonomy. Rules other than the fixed scope rules above.

## 6. What the owner needs on the Mac before the user acceptance test

1. Node.js, to run the reference filesystem MCP server. The crew pins one version and records it in the evidence.
2. A new, empty folder `~/AssayLab`, holding test files only and nothing personal.
3. Claude Desktop installed.

## 7. Questions for the owner

1. **The Touch ID prompt shows the file's path inside the lab, for example `notes/plan.txt`.** `kue-auth` takes its prompt as a command-line argument, so another program running as you could see that path while the prompt is open. This is acceptable for test files in `~/AssayLab`. Fixing it means changing `auth/`, which is security-critical, so it is left for a later story. Recommended: accept for this slice.
2. **Approvals while you are away.** If you are away from the Mac, each held write waits 60 seconds and is then declined. Recommended: keep 60 seconds. The crew will measure Claude Desktop's own tool timeout and record it; if it is shorter, it will ask you before changing anything.

## 8. Estimate

About one week of building and testing, plus your session of about 45 minutes. This is an estimate, not a commitment.
