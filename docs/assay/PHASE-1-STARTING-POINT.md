# Phase 1: the actual starting point

| | |
|---|---|
| Date | 15 October 2026 |
| For | Vijay (owner). Nothing here is decided until you approve it |
| Scope | KUE, NUX and TRIAD as they exist today, what Assay can reuse, and the first vertical slice |
| Method | Read-only. Each repository was exported from its current commit with `git archive` (tracked files only, no personal data, no `.env`), and its tests were run in a clean Linux machine. Nothing in any repository was changed |

## How to read the evidence column

| Mark | Meaning |
|---|---|
| **Ran** | Executed today and the result observed |
| **Read** | Confirmed in today's code by reading it, not executed |
| **Prior** | Taken from an earlier audit or record and not re-checked today |
| **Not checked** | Could not be checked from here, with the reason |

---

## 1. Summary in six lines

1. **All three codebases are healthier than their reputations.** All 560 of KUE's core tests and 40 window tests pass, all 520 of NUX's unit tests pass, and TRIAD passes 277 tests (3 skipped). Every count is from today's runs.
2. **KUE's Rust core compiles and passes its tests on Linux.** It is not tied to macOS, so Assay's engine can be cross-platform from day one.
3. **KUE's file-write path is already the right shape for Assay.** It confines the write to a permitted folder, refuses links that lead outside it, never overwrites, then reads the file back and compares it byte for byte.
4. **Three gaps sit exactly where the slice must work:**
   - KUE has no tamper-evident record.
   - KUE's approval is a five-minute window rather than tied to one action.
   - KUE deliberately never stores what an action touched.
5. **TRIAD's hash chain is a sound design, but it does not detect deletion of the newest record.** I proved this today. Its `is_contested()` gate exists only as a mention in a comment.
6. **NUX contributes no code to the slice.** It contributes two disciplines, decision logging with probabilities and honest outcome arithmetic, which the judgment layer needs later.

---

## 2. KUE (`MY PERSONAL AI ASSISTANT`, branch `story/S0-11`, commit `1761df7`, 7 Oct)

The last product-code commit is 24 Sep. Everything since is process and documents.

### 2.1 Tests

| Suite | Result | Evidence |
|---|---|---|
| Core library (`lantern-core`, unit tests) | 279 of 279 pass | **Ran.** One test failed only because my machine ran as an administrator, who can read a folder the test deliberately locks. Run as a normal user, it passes |
| Core scenario tests (7 files) | 281 of 281 pass (privacy 4, transaction 143, engine 86, environment 11, perception 17, privacy scenarios 14, runtime 6) | **Ran** |
| Core total | **560 pass, 0 fail** | **Ran**, on Linux x86-64 with Rust 1.97 |
| Window tests (vitest) | **40 of 40 pass** | **Ran** |
| Tauri shell and Swift helpers | Not run | **Not checked.** They need macOS. No Swift tests exist (Prior) |
| Live tests: camera, Touch ID, microphone | Not run | **Not checked.** They need your Mac and you present |
| JavaScript dependencies shipped in the app | 0 known vulnerabilities | **Ran** (`npm audit --omit=dev`). One high-severity advisory exists in a build-only tool (`source-map-js`). It does not ship |
| Rust dependency advisories | Pending | The checker was still installing when this report was written. Result in §8 |

### 2.2 The working pipeline: one request, end to end

**Read**, in `core/src/transaction.rs`, `authz.rs` and `actions.rs`.

```
Proposed → PrivacyChecked → Authorized → AwaitingConfirmation → Reauthorized → Executing → Verified
```

| Step | What the code does | Where |
|---|---|---|
| Declared actions only | 15 action kinds in a closed list, each with a fixed risk. A new action cannot exist without a risk | `actions.rs:33`, `actions.rs:171` |
| Unknown means deny | An unknown risk tag is refused | `actions.rs` `risk_from_tag` |
| Confinement | `PermittedRoots::resolve` canonicalises the path and refuses one that escapes the folder through a symbolic link | `actions.rs` ~line 690 |
| Authorisation | Each risk maps to an operation and a required level (0 to 4) | `authz.rs:144-172` |
| Confirm | The window confirms by record id, and the action content is held in the core, so the window cannot change it | `transaction.rs:1249` |
| Re-check at confirm | Kill switch, the target check and authorisation all run again at the moment of Confirm | `transaction.rs:1253-1275` |
| Execute file write | Opened with `create_new` (refuses to overwrite), written, `fsync`ed | `actions.rs:750` |
| Verify | Reads the file back and compares the bytes. Equal: Succeeded. Different: PartiallySucceeded. Unreadable: UnknownResult | `actions.rs:763-769` |
| Record | An event row holding the action's kind, risk and outcome. **Never its target (path)**, by design of the privacy firewall | `engine.rs:751`, `store.rs:563` |

### 2.3 Authorisation levels as the code enforces them today

**Read**, in `authz.rs`.

| Risk | Operation | Level needed | How it is reached today |
|---|---|---|---|
| Low | ActionLowRisk | 2 | Face confirmed by camera, **or** a Touch ID grant |
| Medium (includes write a file) | ActionMediumRisk | 2 | Same as above |
| High (move to Trash, move, quit an app) | ActionHighRisk | 3 | Touch ID or password grant |
| Critical | ActionCriticalRisk | 4, fresh, physical | Single-use physical confirmation within 30 s |

Four facts from the code that matter for Assay:

1. **The face is a key.** A confirmed face alone gives level 2, enough to write a file (`authz.rs:483`).
2. **A Touch ID grant is a time window, not one action.** It lasts `strong_auth_valid_seconds = 300` (`config/lantern.toml:202`), so every medium or high action within five minutes rides on one fingerprint.
3. **An unreliable camera blocks you, and Touch ID cannot override it.** When identity is "uncertain", owner operations are denied outright (`authz.rs:527-532`). That is why identity flapping made KUE unusable.
4. **With the camera off, Touch ID works cleanly.** Identity "not observing" gives level 0, and a Touch ID grant then lifts it (`authz.rs:487-488`). So **the face can be removed from authority without rewriting the authorisation model.** Assay starts in that mode.

### 2.4 Verification and undo

| Capability | State | Evidence |
|---|---|---|
| Read-back verification for file create, folder create, move, Trash, put back | Implemented in code, and the tests pass | **Ran / Read** |
| Undo of Trash (put back to exact original place, refused if taken) | Implemented, and the tests pass | **Ran / Read** |
| Undo of a created file | **None**. The tool declares "not possible: KUE does not delete" | **Read**, `tools.rs:438` |

### 2.5 Identity controls

| Item | State | Evidence |
|---|---|---|
| Face recognition flapping (up to 830 changes an hour) | Not re-measured. No fix was built after 20 Sep | **Prior**. Needs the camera |
| Face alone grants level 2, with no liveness check | Still true | **Read**, `authz.rs:483` |
| Approval helper (`kue-auth`, `kue-act`) found by searching paths and trusted if the file exists, with no signature check | Still true | **Read**, `src-tauri/src/broker.rs:15-28` |
| App signing | Ad hoc (`codesign --sign -`), no hardened runtime, no entitlements file | **Prior**, build scripts not re-read today |

### 2.6 Defects and debt carried into Assay

| # | Defect | Severity for Assay | Evidence |
|---|---|---|---|
| K-1 | Face confirmation is an authorisation key | **High.** Must not carry over | Read |
| K-2 | The Touch ID grant covers five minutes of actions rather than one approved action | **High.** Assay needs approval bound to one action | Read |
| K-3 | Helpers trusted by path, not signature. A swapped `kue-auth` could report Touch ID success | **High** | Read |
| K-4 | 410 occurrences of `.lock().unwrap()` in the core and shell source (tests included), each a place where a locked state is taken without handling failure. One panic can disable later commands, including Kill (155 in `transaction.rs`, 229 in `src-tauri/src/lib.rs`) | Medium. Must be fixed on the path Assay uses | Ran (count) |
| K-5 | No tamper-evident record. Events are plain rows | **High.** Receipts are the product | Read |
| K-6 | The record never stores what an action touched. Right for KUE's privacy model; Assay needs a different design (§5.4) | Design decision | Read |
| K-7 | Three shell tests passed when the helpers were missing | Medium | Prior (6 Oct), not re-checked |
| K-8 | The Lantern → KUE rename is unfinished: crate `lantern-core`, `lantern.toml`, data folder `Lantern` | Low | Read |
| K-9 | `transaction.rs` is 3,405 lines and binds actions to goals, plans and narration | Medium. Assay takes the skeleton, not the file | Read |
| K-10 | No network code anywhere in the core or the shell | Not a defect. A strength to keep | Read (searched) |

---

## 3. NUX (`wach-nux`, branch `main`, commit `e8f7be3`, 8 Sep)

354 commits. Three finished fix branches (`one-surface`, `e0-separate-and-shell`, `model-seam-and-traces`) remain unmerged.

### 3.1 Tests and build

| Check | Result | Evidence |
|---|---|---|
| Unit tests (vitest) | **520 of 520 pass, in 33 files** | **Ran**. The August audit's "test suite broken" no longer holds |
| Type check (`tsc --noEmit`) | Clean | **Ran** |
| Lint | 1 error, 56 warnings | **Ran** |
| End-to-end tests (Playwright, 7 specs) | Not run | **Not checked.** They need a live Supabase project and credentials, which I did not use |

### 3.2 The live application

| Item | State | Evidence |
|---|---|---|
| Live page | Serves the August product: "Every AI can tell you what to do. NUX comes back to find out if you did." Record and Sign in buttons, "Free to start" | **Ran** (fetched today) |
| Production commit | The Vercel project was last updated 31 Aug 2026 20:27 UTC, matching the audit's claim that production runs `c936fce` | **Read** (project record). Vercel refused the deployment list for this team, so the exact commit is **not independently confirmed** |
| One item | Text withheld from the published copy (owner's decision, 21 October 2026) | |
| Two identities on one page | Body says follow-through, title and metadata say "It hears what you mean" | Prior |

### 3.3 Database and security controls

**Read**, in `supabase/migrations/` (15 files, 1,610 lines).

| Item | State |
|---|---|
| Tables created by migrations | About 50, including commitments, arrivals, arrival_decisions, user_policy, memories, memory_receipts, hypotheses, feature_flags, stripe_events |
| Row-level security | 92 policies across the migrations. Every user table checked has `auth.uid() = user_id` policies |
| Live database | **Not checked.** No credentials were used. Table and policy counts come from the migrations, not the live database |
| Guards on the live path | Distress check on the free-text field with no model involved (`safety-gate.ts`), safe redirect (`safe-next.ts`), PII stripping (`strip-pii.ts`), rate limiting, cron authentication |
| Feature flags | Deterministic SHA-256 bucketing, unknown flag means off. **Admin emails are hard-coded in source** (`flags.ts:15`), a minor finding |

### 3.4 Follow-through logic

**Read**, in `followthrough.ts` (402 lines, fully unit-tested).

- At most 3 open commitments.
- A 48-hour grace window, after which a silent commitment becomes a miss.
- Pattern claims need at least 5 settled commitments.
- Every claim is computed by code, never by a model.

This is the honest-outcome arithmetic Assay needs later, for actions whose result lands after a delay.

### 3.5 Memory and learning capabilities

| Capability | State | Evidence |
|---|---|---|
| Memory vault, Meaning Engine, five-layer model, theory of mind, metacognition | Code present (`brain.ts`, `user-model.ts`, `theory-of-mind.ts`, `metacognition.ts`), reachable only through 21 archived routes behind `legacy_*` flags | Read |
| Memory receipts | A table that records which memory was used and quotes the span that justified it | Read |
| Adaptive layer, Phase 1 | Logs every send or hold decision with its context features and the exact probability it was chosen with (`SEND_PROPENSITY = 1/9`). **No learning runs**, by design, until there is enough data | Read |
| "Agent tools" (`src/lib/agent/`) | In-app functions only (create a task, show a chart), used only by the archived chat. **Nothing acts outside the app** | Read |

### 3.6 Unresolved issues

1. One item withheld from the published copy (owner's decision, 21 October 2026).
2. Live user data sits in a product that is paused in practice. NUX's last commit was 37 days ago.
3. `CLAUDE.md` still instructs every session to build the July product (Prior).
4. The three finished fix branches are unmerged.

---

## 4. TRIAD (`Triad`, branch `pre-test-production`, commit `1cbecf7`, 28 Aug)

37 commits. Python 3.12+, FastAPI, SQLAlchemy, SQLite for demos, PostgreSQL in production.

### 4.1 Tests

| Check | Result | Evidence |
|---|---|---|
| Full test suite | **277 pass, 3 skipped, 0 fail** in 83 s | **Ran** (Python 3.13, SQLite) |
| PostgreSQL-only tests | 2 not run, because they need PostgreSQL. The suite says so loudly rather than hiding it | Ran |

### 4.2 Audit, evidence and gate components

| Component | Implemented | Tested | Reusable for Assay |
|---|---|---|---|
| Hash-chained, append-only event store: SHA-256 over canonical JSON (sorted keys, fixed field order), per-project chain, genesis hash (`events/store.py`) | Yes | Yes, 8 tests: linking, clean verify, ORM refuses update and delete, payload tampering below the ORM, middle deletion, per-project independence, position-dependent hashing | **The design ports to Rust. The code does not** (Python versus Rust) |
| `verify_chain` (detects recomputed-hash mismatch, broken link, sequence gap) | Yes | Yes | Design reusable, **with one fix (T-1)** |
| `is_contested()` gate that an external build agent calls | **No.** It appears only in a docstring (`detect/join.py:191`). The function it would use, `contested_slots`, exists | Partly | Concept only. Assay's own gate API is new work |
| State machines with legal transitions only (`domain/state_machines.py`) | Yes | Yes | Concept. KUE already has the same in Rust |
| Synthetic-world measurement (`sim/`): a world with a known answer key, scored as curves, not single numbers | Yes | Yes | **Method reusable** for Assay's injection test set and for measuring the judgment layer |
| RACI in code (`domain/raci.py`) | Yes | Yes | Not part of Assay |

### 4.3 Defects found today

| # | Defect | Evidence |
|---|---|---|
| T-1 | **Deleting the newest record is not detected.** The test named "deleting the tail is detected" actually deletes record 3 of 5 (a middle record). I deleted record 5 of 5 and `verify_chain` returned **intact = True**. A chain cannot prove its own end without an outside anchor | **Ran**, with a temporary probe test (removed afterwards, never written to your repository) |
| T-2 | `public/v2.html` and `public/v4.html` still state "SOC 2 Type II, current, report 12 Mar 2026" and ISO 27001. Neither exists | **Read.** Whether these pages are publicly served is **not checked** (the obvious URL serves someone else's page) |

---

## 5. Cross-project reuse map

### 5.1 Reuse unchanged (as a library inside Assay)

| Component | From | Why it holds |
|---|---|---|
| `PermittedRoots::resolve`: canonical path confinement and link-escape refusal | KUE `actions.rs` | Tested. This is exactly the scope check a file write needs |
| `create_new` + `fsync` + read-back pattern for file creation | KUE `actions.rs:750-770` | Tested. Becomes Assay's verifier for its own executor tests |
| Kill switch with a latch file that survives restart | KUE `engine.rs`, `runtime` | Tested. Assay's stop-everything control |
| Privacy firewall: data kinds and destinations | KUE `privacy.rs` | Tested. Keeps receipt content off logs and interfaces that must not hold it |
| `ActionState` vocabulary: Succeeded, Failed, PartiallySucceeded, UnknownResult, Denied, Cancelled, AuthorizationExpired | KUE `actions.rs` | Already matches BR-06's honesty rule |
| SQLite store setup (`rusqlite`, bundled) | KUE `store.rs` | Same engine Assay needs |

### 5.2 Reuse with adaptation

| Component | From | Change needed |
|---|---|---|
| Authorisation model, levels 0 to 4 | KUE `authz.rs` | Remove the face as a key (K-1). Add per-action, single-use approval bound to an action digest for every held action (K-2). The single-use mechanism already exists for level 4 and becomes the default for holds |
| `kue-auth` Touch ID helper | KUE `auth/` | Verify its code signature before trusting it (K-3). Show the action's own words in the Touch ID prompt |
| Confirm, gate, execute, verify skeleton | KUE `transaction.rs` | Extract about 300 lines from 3,405. Leave goals, plans and narration behind (K-9). Replace `.lock().unwrap()` on this path (K-4) |
| Action list (15 kinds, closed) | KUE `actions.rs` | Keep the closed-list principle. Add a normaliser that maps an outside agent's tool call to a declared action class. Anything unmapped is stopped (BR-18) |
| Hash-chained record | TRIAD `events/store.py` | Port to Rust. Add an outside anchor that stores the head sequence and hash, to fix T-1. Add SQLite triggers that refuse UPDATE and DELETE |
| Synthetic answer-key measurement | TRIAD `sim/` | Becomes the injection test set and the judgment-accuracy harness |
| Decision logging with features and probabilities | NUX `adaptive.ts`, `arrival_decisions` | Becomes the data design for the judgment layer (Release 2), not the slice |
| Outcome arithmetic with grace windows | NUX `followthrough.ts` | Becomes the checker for actions whose outcome lands later (Release 2 or 3) |
| Config file `lantern.toml` | KUE | Becomes `assay.toml` with its own thresholds |

### 5.3 Remove: not part of Assay

| Component | From | Note |
|---|---|---|
| Camera sensing, face identity, enrolment | KUE `sensing/`, `authz.rs` identity paths | Removed from authority now. The code stays in the KUE history |
| Voice, wake word, speech | KUE `voice/`, `speech.rs` | Not needed |
| On-device model, intent classifier, dialogue, goals, plans | KUE `mind/`, `intent.rs`, `dialogue.rs`, `goal.rs`, `plan.rs` | Assay never chats or plans |
| Storage inspection and clean-up product | KUE `storage.rs` | Not Assay's job |
| Life back office plan, 133 requirements, 99-story backlog | KUE `docs/product/`, `KUE Life Back Office/` | Frozen and parked on a branch, not deleted |
| The whole NUX consumer app, including its 85 API routes and Supabase product data | NUX | Paused. Owner decisions in §7 |
| TRIAD detection engine, adapters, API | TRIAD | Not Assay |

### 5.4 Build from scratch

| Component | Why nothing existing covers it |
|---|---|
| MCP proxy: sits between an agent host and an MCP server, passes every message through, and intercepts `tools/call` | No project has any MCP or network-protocol code |
| Tool-call normaliser (outside tool name and arguments to an Assay action class) | KUE only knows its own 15 actions |
| Independent verifier for actions another program performed | KUE verifies only what KUE itself executed |
| Receipt design that solves K-6: the chain covers record headers holding hashes of the path and content, while the readable path sits in a separate table that can be erased. This is the C-1 proposal already written in KUE's laws, now applied | KUE stores no target; TRIAD stores everything in the hashed payload |
| Approval prompt bound to an action digest, shown with the action's own words | KUE's approval is per record id inside its own window |
| `assay receipts verify` and `assay receipts show` commands | New |

---

## 6. The first vertical slice: SLICE-01, "one file, through the gate"

### 6.1 The story

As the owner, when an AI agent asks to create a new file in my lab folder, Assay does five things:

- holds the request,
- asks me with Touch ID, showing exactly what will be written,
- lets it through only if I approve,
- checks that the file on disk matches exactly what I approved,
- keeps a receipt that survives a restart and shows if anyone tampered with it.

### 6.2 What is in and out

| In | Out (later slices) |
|---|---|
| One agent host: Claude Desktop | Other agents, host hooks, shell |
| One downstream server: the reference filesystem MCP server, pinned to one version, limited to `~/AssayLab` | Other servers |
| One gated action: `write_file` to a **new** file inside `~/AssayLab` | Overwrite, edit, move, delete, undo |
| Read-only tools of that server pass through and get a receipt | Rules editor, learning, earned autonomy |
| Touch ID approval with the camera off | Windows Hello, Linux prompts (the automated tests still run on Linux) |
| Receipts in SQLite with chain and anchor, plus `verify` and `show` commands | Approval window, owner console |

### 6.3 The exact path of one request

1. Claude Desktop starts `assay proxy -- <filesystem server command> ~/AssayLab` as its MCP server. Assay starts the real server as its child process.
2. Every message passes through unchanged, except `tools/call`.
3. `tools/call` for `write_file` is normalised to an action of class `file.create`, holding:
   - the agent (from the client's own identification),
   - the server,
   - the path,
   - the content's SHA-256,
   - the byte count.
4. **Scope.** The path is resolved with KUE's `PermittedRoots::resolve`. Outside `~/AssayLab`, or escaping through a link: **Stop**. File already exists: **Stop** (no overwrite in this slice). Text longer than 20,000 characters: **Stop**.
5. **Decision.** Every in-scope write is **Hold** in this slice. Nothing has been earned yet.
6. **Digest.** Assay hashes, in canonical JSON: agent, server, tool, path, content SHA-256, and the exact request bytes.
7. **Approval.** Assay prompts with Touch ID through `kue-auth`, after checking the helper's code signature. The prompt reads: *"Assay: let Claude create `notes/plan.txt` (412 bytes, sha 3f9a1c2e)"*.
   - No answer within 60 s: declined.
   - Kill latch set: declined without prompting.
8. **Forward.** On approval, Assay sends the original request bytes. It first re-hashes them and refuses if they differ from the digest.
9. **Verify independently.** When the server answers:
   - An error is recorded as **Failed**.
   - On success, Assay checks the file itself: it exists, it is a regular file and not a link, it is still inside the lab, and its SHA-256 equals the approved content. That is **Verified**.
   - A different hash is **Failed** ("the server reported success but the file differs").
   - A file Assay cannot read is **Unconfirmed**.
10. **Receipt before reply.** Assay writes the receipt and commits it to disk (SQLite WAL, `synchronous=FULL`) **before** the agent gets its answer, then moves the anchor to the new head.
11. **Reply.** The agent receives the server's result. If Assay stopped or the owner declined, it receives a tool error that says so in plain words.

### 6.4 Acceptance criteria

The CI column marks tests that run automatically on GitHub for macOS and Linux, using a stub approver and a stub server. The UAT column marks checks you do yourself at your Mac with real Claude Desktop and real Touch ID.

| ID | Given / When / Then | CI | UAT |
|---|---|---|---|
| S1-AC01 | Given a new path inside the lab, when the owner approves, then the file exists, its SHA-256 equals the approved content, the receipt says Verified, and the agent receives success | ✓ | ✓ |
| S1-AC02 | When the owner declines, then no file exists, the receipt says "declined by owner", and the agent receives a tool error naming the reason | ✓ | ✓ |
| S1-AC03 | When nobody answers for 60 s, then the request is declined (timeout), and no file exists | ✓ | ✓ |
| S1-AC04 | Given a path outside the lab, then the request is stopped with no prompt and a receipt recording the reason | ✓ | ✓ |
| S1-AC05 | Given a link inside the lab that points outside it, then the request is stopped | ✓ | |
| S1-AC06 | Given an existing file, then the request is stopped and the file's bytes are unchanged | ✓ | |
| S1-AC07 | Given the request bytes change between approval and forwarding (fault injected in the test), then nothing is forwarded and the receipt records the refusal | ✓ | |
| S1-AC08 | Given a stub server that reports success but writes different bytes, then the receipt says Failed and quotes both hashes | ✓ | |
| S1-AC09 | Given Assay is killed with `kill -9` right after replying, when it restarts, then the receipt is present and `assay receipts verify` reports intact | ✓ | |
| S1-AC10 | When one receipt is edited directly in SQLite, then `verify` reports broken at that sequence. When the **newest** receipt is deleted, then `verify` reports it through the anchor (the TRIAD T-1 case) | ✓ | |
| S1-AC11 | Given the kill latch is set, then every write is stopped with no prompt | ✓ | ✓ |
| S1-AC12 | The server's read-only tools pass through with a receipt. Any tool not on the list is stopped (BR-18). Tool names are checked against the pinned server version | ✓ | |
| S1-AC13 | Over 200 read calls, Assay adds at most 50 ms at the 95th percentile, measured | ✓ | |
| S1-AC14 | The Assay binary has no network dependency in its Cargo tree and opens no network connection during the test run | ✓ | |
| S1-AC15 | Given `kue-auth` replaced by an unsigned binary, then Assay refuses to prompt and stops the write | | ✓ |
| S1-AC16 | With the camera off and KUE not running, approval works through Touch ID alone | | ✓ |
| S1-AC17 | `assay receipts show` lists each receipt in plain words: time, agent, action, decision, result | ✓ | ✓ |
| S1-AC18 | Uninstalling the slice restores Claude Desktop's original configuration byte for byte | ✓ | ✓ |

### 6.5 Where it lives and what it needs

- **Code.** A new crate in the KUE workspace (working name `assay-gate`, binary `assay`), on a `story/` branch cut from `kue/release-1`. It depends on `lantern-core` for path confinement, the kill switch and states. It goes through the same change-control process as every KUE story.
- **New dependency.** `sha2` for SHA-256. Your rules require your yes before any dependency is added. Nothing else is needed: JSON-RPC over standard input and output is handled with the existing `serde_json`, and the signature check calls macOS's own `codesign`.
- **On your Mac.** Node.js, to run the reference filesystem server, and a new empty folder `~/AssayLab` containing only test files.
- **Size, estimated.** About one week of build and test by the crew, and one session of about 45 minutes for your UAT. This is an estimate, not a commitment. It becomes one when the work order is accepted.

---

## 7. Decisions only you can make before the slice starts

| # | Decision | My recommendation |
|---|---|---|
| D-1 | NUX: the product holds live user data (one detail withheld from the published copy by the owner's decision of 21 October 2026) | Put the site into a "paused" state and tell its users. Or take it down and export, then delete, user data. Either way, record the decision |
| D-2 | Adopt the C-1 receipt design: chain over headers, erasable readable content | Adopt. The slice is specified on it |
| D-3 | Approve the `sha2` dependency | Approve |
| D-4 | Approval mode: camera off, Touch ID for every held action, no five-minute window | Approve. Fixes K-1, K-2 and the identity-flapping problem in one move |
| D-5 | Lab folder `~/AssayLab`, synthetic files only | Approve |
| D-6 | Freeze the back-office backlog and move it to a parked branch, with no deletion | Approve |
| D-7 | TRIAD: remove or correct the false certification claims in `public/v2.html` and `public/v4.html` before anyone sees them | Correct them now, whatever happens to TRIAD |

After your answers, the next step is the work order for SLICE-01, as an `Accept` pull request in the usual way.

---

## 8. Housekeeping and open checks

- **Rust dependency advisories (`cargo audit`, Ran, 444 crates, advisory database of 15 Oct):** **0 vulnerabilities.** There are 7 warnings, all arriving through the Tauri window stack, not the core:
  - 6 crates are unmaintained: `proc-macro-error` and five `unic-*` crates.
  - 1 is an "unsound" advisory in `glib` (RUSTSEC-2024-0429), which is used only by Tauri on Linux.

  None of these reach `lantern-core` or the slice's `assay` binary. They matter when the window is ported to Linux. GitHub's earlier "one high, one moderate" alert is not reproduced by today's database.
- **Files I added to your folders** to copy the code out for testing. I cannot delete files in your folders, so please remove these when convenient:
  - `MY PERSONAL AI ASSISTANT/Claude outputs/phase1-kue-src-1761df7.tar.gz` and `phase1-kue-docs-1761df7.tar.gz`. This folder is ignored by git.
  - `wach-nux/Claude outputs/phase1-nux-src-e8f7be3.tar.gz` and `phase1-nux-full-e8f7be3.tar.gz`. Untracked by git.
  - `Triad/outputs/phase1-triad-src-1cbecf7.tar.gz`. Untracked, but inside a tracked folder, so do not commit it.
- **Not checked from here:** the camera and identity rate, Touch ID end to end, the Swift tests, NUX end-to-end tests, the live NUX database, and the exact production commit on Vercel.
