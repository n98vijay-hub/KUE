# KUE master status

Written 2026-09-16 at the end of Phase 0, on branch `kue/runtime-safety`.
This is the one place that says what KUE is today. The detailed records it
summarises are [KUE_STATUS.md](KUE_STATUS.md) and the per-area documents.

**How to read a status.** Two separate questions, never merged:

- **Does it exist?** REAL · PARTIAL · NOT_IMPLEMENTED (in `core/src/capabilities.rs`, `status`).
- **Has it been seen working on this Mac?** LIVE_VERIFIED · PARTLY_LIVE_VERIFIED ·
  TEST_VERIFIED_ONLY (the registry's `proof`).

LIVE_VERIFIED means it worked on this MacBook Air through KUE's own app or its
own helper, with the real permissions and sensors: not a stand-in, not a
prerecorded clip, not a test double. A passing test, a type, an enum, a
design document or an interface moves nothing to LIVE_VERIFIED.
DENIED and REQUIRES_AUTHENTICATION are not properties of a capability; they are
what a *request* gets at the moment it is made (`intent::status`,
`capabilities::availability`).

The capability blocks below are checked against the registry by
`capabilities::tests::the_master_status_lists_the_same_proof_as_the_registry`:
if this document and the registry disagree about any capability, the build's
tests fail.

---

## 1. Current architecture

```
 macOS                                KUE (one app bundle)
 ─────                                ────────────────────
 camera, Vision, NSWorkspace,   ──▶  LanternSense.app  (Swift; sensing, speech recognition,
 HID idle timer, microphone,          wake boundary, face enrollment — derived readings only)
 SpeechAnalyzer                              │ JSON lines, derived data
                                             ▼
                                     lantern (Tauri 2 shell, Rust) ── React window (display only;
                                             │                         no authority, no API keys)
                                             ▼
                                     lantern-core (Rust): the decisions
                                       safety boundary → intent router → authorization
                                       (identity session, levels 0–4) → goals/steps →
                                       transaction → Action Broker gate → verification;
                                       privacy firewall on every destination; kill switch;
                                       evidence; storage analysis; capability registry;
                                       speech pipeline; surface (what the window may show)
                                             │
            ┌───────────────┬───────────────┼────────────────┬─────────────────┐
            ▼               ▼               ▼                ▼                 ▼
       KueAct.app       kue-auth        kue-voice        lantern-mind      SQLite store
       (executor for    (LocalAuth-     (AVSpeech-       (Apple Founda-    ~/Library/Application
       allowlisted      entication:     Synthesizer)     tionModels,       Support/Lantern
       actions, reads   Touch ID /                       on-device;        (events, snapshots,
       results back)    password)                        text in/out)      ledger, kill latch)
```

Nothing in KUE makes a network request. There is no cloud model, no web
access and no telemetry.

## 2. Current capabilities

41 capabilities in one registry (`core/src/capabilities.rs`): 21 REAL, 8
PARTIAL, 12 NOT_IMPLEMENTED. Every surface that says what KUE can do — the
Capabilities sheet, the spoken answer to "what can you do?", the model's
context, the Action Broker's gate, Diagnostics — reads that list. The sheet now
also shows each capability's proof.

## 3. REAL capabilities

REAL means implemented and working within its stated limits. It does **not**
mean seen working on this Mac — see sections 4 and 5.

Camera capture, face detection, head pose, capture quality, face tracking,
enrollment, frontmost application, input idle time, event stream, evidence,
local memory, kill switch, owner session, OS authentication, failure
reporting, thermal adaptation, body and hand pose, on-device model, storage
inspection, storage cleanup, computer automation (the allowlisted Action
Broker).

(`scene_recognition`, `identity_matching`, `resource_monitoring`,
`microphone`, `wake_word`, `voice_output`, `conversation` and `arithmetic` are
PARTIAL.)

## 4. LIVE_VERIFIED capabilities

Seen working on this Mac, with the date first seen and what was seen in the
registry's `proof.seen`.

<!-- registry:LIVE_VERIFIED -->
| Capability | First seen | What was seen |
|---|---|---|
| `camera_capture` | 2026-09-14 | Sensing sessions with the owner at the desk; camera off within 0.2 s of pausing |
| `face_detection` | 2026-09-14 | Faces detected; a second face locked the session as designed |
| `head_pose` | 2026-09-15 | Angles shown in the window (since removed from the window) |
| `capture_quality` | 2026-09-14 | 0.12–0.27 at the desk — one cause of identity flapping |
| `face_tracking` | 2026-09-14 | Identity carried on one track through 623 s of camera time |
| `enrollment` | 2026-09-14 | The owner enrolled; identity has matched against it since |
| `frontmost_application` | 2026-09-14 | Sampled about once a second; stopped while paused |
| `input_idle` | 2026-09-14 | Seconds since input shown; stopped while paused |
| `event_stream` | 2026-09-14 | Read back to measure flapping (158 changes in 348 s) |
| `evidence` | 2026-09-14 | Presence statements from live readings; a pause defect found and fixed |
| `local_memory` | 2026-09-14 | Events and snapshots written and read back |
| `kill_switch` | 2026-09-14 | Killed from outside the window, relaunched while killed: stayed stopped |
| `resource_monitoring` | 2026-09-14 | 0.07% CPU and 4.1 MB while paused |
| `scene_recognition` | 2026-09-14 | Scene labels and brightness from the camera |
| `voice_output` | 2026-09-15 | An answer spoken to completion from the bundled app |
| `conversation` | 2026-09-14 | Typed and spoken questions answered by the on-device model |
| `on_device_model` | 2026-09-14 | 5.6 s for a one-line answer |
| `perception_observability` | 2026-09-20 | 893 samples in 15 minutes: 65.6% of KUE's uncertainty was an AMBIGUOUS measurement, 16.9% a live pipeline being late, 2.5% stale, 0 contradictions. Its own model answer measured at 13.9 s prefill + 3.4 s generation |
| `perception_observability` | 2026-09-20 | 893 samples in 15 minutes: 65.6% of KUE's uncertainty was an AMBIGUOUS measurement, 16.9% a live pipeline being late, 2.5% stale, 0 contradictions. Its own model answer measured at 13.9 s prefill + 3.4 s generation |
<!-- /registry:LIVE_VERIFIED -->

Two cautions. These were seen in the build of that date; later builds changed
code around several of them and have not been re-checked live. And a live
sighting of a capability is not a live sighting of every path through it —
the partly verified list says which paths are missing.

## 5. PARTIAL capabilities, and what has only partly been seen

### Partly seen working on this Mac

<!-- registry:PARTLY_LIVE_VERIFIED -->
| Capability | Seen | Not yet seen |
|---|---|---|
| `identity_matching` | Matches the enrolled owner at the desk | **A steady match — FAILED live:** 304 non-matching measurements of the owner on 2026-09-16, each followed by a match. Rejecting another person: never measured |
| `owner_session` | Owner reached LEVEL_2 from the camera; a spoken and a typed action ran under it; a second face locked it | **Steady access — FAILED live:** 755 access changes in one hour on 2026-09-16, after the 2026-09-14 fix |
| `os_authentication` | Touch ID present; both policies available | A Touch ID prompt completed through KUE for an action |
| `body_hand_pose` | Upper body detected (4 of 4 joints) | A hand overlapping the face |
| `microphone` | Push-to-talk: spoken requests transcribed; one opened an app (2026-09-15) | Release on pause/kill/lock; a refused or revoked microphone permission |
| `storage_inspection` | 37,055 entries in 838 ms through the app's transaction; the sheet withheld the report while Locked | The report shown to the owner in the window |
| `storage_cleanup` | Two installers found, moved to the real Trash and put back through the executor | A move from the window with the owner recognised and a real Touch ID prompt |
| `computer_automation` | From the window (event log): a spoken request opened an app, a typed request opened a document after confirmation, a folder opened — each succeeded with read-back. KueAct opened Chrome, Calculator and a PDF | Quit, switch, links, notifications, and files in ~/KUE, from the window |
<!-- /registry:PARTLY_LIVE_VERIFIED -->

**Where this evidence came from.** Besides the docs and commit history, KUE's
own event log on this Mac (`~/Library/Application Support/Lantern/lantern.sqlite3`,
read-only; only the running app writes to it, never a test) was read on
2026-09-16. It holds no words and no targets, only kinds and outcomes. It
showed two things the documents did not: actions run end to end from the window
on 2026-09-15, and identity flapping that the 2026-09-14 fix did not remove.

### Checked only by automated tests

<!-- registry:TEST_VERIFIED_ONLY -->
| Capability | Why not live |
|---|---|
| `wake_word` | Hands-free listening has run only on audio files through the real transcriber (105/105). Never with the open microphone in a real room |
| `arithmetic` | Built 2026-09-16; not tried in the app |
| `failure_reporting` | No live record of each failure state reaching the window |
| `thermal_adaptation` | No thermal pressure or Low Power Mode has occurred during a session |
| `personal_memory` | Built 2026-09-23: what the owner asks KUE to remember, what it did and checked, and forgetting. Survives a restart in a store test; not yet driven in the app |
<!-- /registry:TEST_VERIFIED_ONLY -->

Also TEST_VERIFIED_ONLY, but not separate capabilities in the registry: the
safety boundary for requests, the intent router, goals and plans, the
cleanup-goal flow, the explanation spoken aloud, the "Computer," typed prefix,
the KUE rename of every sentence KUE says, `KUE.app` (built and signed
2026-09-16, never opened — the app running on this Mac is still the older
`Lantern.app`) and the owner scripts (`build-kue.sh`, `status-kue.sh` and
`backup-kue.sh` have run; `run-kue.sh` and `test-kue.sh` have not).

PARTIAL (exists, with material limits): `identity_matching` (reject side
unmeasured), `resource_monitoring` (no GPU, Neural Engine or energy),
`scene_recognition` (whole-frame labels only), `microphone`
(push-to-talk; hands-free is separate), `wake_word` (off by default; English;
name only, not person), `voice_output` (one macOS voice; no critical-safety
producer), `conversation` (small on-device model: short, 5–15 s, can be wrong;
every answer checked against the registry), `arithmetic` (+ − × ÷ % and
brackets only).

## 6. NOT_IMPLEMENTED capabilities

<!-- registry:NOT_APPLICABLE -->
| Capability | Why |
|---|---|
| `speaker_identity` | No Apple API; needs a third-party model the owner has not approved (section 10) |
| `external_model` | No cloud model and no network code; policy v1 refuses personal context to one (section 11) |
| `internet_research` | No network code exists (section 12) |
| `memory_recall` | Events are stored but cannot be asked about. What the owner tells KUE to keep is a different capability, and that one exists (section 15) |
| `in_app_control` | Clicking, typing and reading other apps needs Accessibility, which KUE does not request (section 13) |
| `proactive_assistance` | No reminders or background actions (section 16) |
| `pattern_recognition` | Events are not mined |
| `screen_context` | KUE never reads the screen |
| `purchasing` | Needs web action plus confirmation and macOS authentication per purchase |
| `messaging` | Needs a sending path plus confirmation per message |
| `permanent_deletion` | Deliberately never: the Trash is as far as KUE goes |
| `emotion_reading` | Deliberately never: facial geometry does not show emotion |
| `calendar` | No EventKit code, no Reminders code, no calendar permission requested. A read-only helper exists on a side branch, unbuilt and never run |
<!-- /registry:NOT_APPLICABLE -->

Also not implemented, with no registry row yet because nothing exists to
describe: a Claude or other external model provider (section 11), calendar
access (section 14), onboarding (section 17), voice enrollment, GUEST and
RESTRICTED_GUEST identity states, echo cancellation.

## 7. Security model

The authority chain is fixed and outside any model:

```
REQUEST → SAFETY BOUNDARY (rule; refuses before anything else) → INTENT (rule)
        → AUTHORIZATION (identity session + levels; per step, at the moment it runs)
        → GOAL / STEP → TRANSACTION → ACTION BROKER (closed allowlist) → EXECUTOR
        → VERIFICATION (read back) → RECORD
```

Standing invariants, each enforced in code and covered by tests (details and
test names in [KUE_STATUS.md](KUE_STATUS.md#security-invariants--current-standing)):
unknown operation, classification, action or authority = DENY; a model cannot
grant permission, disable or recover the kill switch, change privacy policy or
authorize anyone (models have no path to those commands); wake ≠ identity ≠
authorization; failed verification is never success (`ActionRecord::finish`
downgrades an unverified success); a plan inherits no authority — every step is
authorized when it runs; the Trash is the only removal; KUE moves only files it
found and showed.

Known security gaps: helpers are ad-hoc signed, so anyone with write access to
the bundle could replace one; Keychain is not used; a change of enrolled
fingerprints is not detected; standard error is not routed through the privacy
policy; the safety boundary and intent router match phrases, so a reworded
dangerous request can reach the on-device model (which still has no authority);
the KueAct hand-off is contained, not firewalled. No threat model document
exists yet. (Fixed 2026-09-16: data placed in a model prompt could forge a turn;
see section 11.)

## 8. Privacy model

Policy v1 (`core/src/privacy.rs`), versioned code with no runtime override
except adding refusals. Every data kind has a classification; an unknown one
is denied at compile time. Classifications in use: LOCAL_ONLY, DERIVED_ONLY,
USER_APPROVAL_REQUIRED (interface only), NEVER_STORE, NEVER_COLLECT. There is no
CLOUD_ALLOWED data today: the model router refuses EXTERNAL for everything.
Every destination — memory, the window, the on-device model, speech, the action
path — takes a sealed clearance, and each decision is written to a ledger.

Raw camera frames, face crops, audio, keystrokes, clipboard, screen contents,
window titles and document contents are never collected or kept. Transcripts
never enter context, events or memory. Face enrollment keeps derived
descriptors only.

## 9. Identity model

One identity session (`core/src/authz.rs`) for everything. States: NO_PERSON,
UNKNOWN_PERSON, IDENTITY_UNCERTAIN, AUTHORIZED_USER,
AUTHORIZED_USER_LOW_CONFIDENCE, MULTIPLE_PEOPLE, AUTHENTICATION_REQUIRED,
LOCKED. It starts LOCKED; the owner is established by a measured face match or
Touch ID; 10 s unseen → owner left, 60 s → LOCKED; a stranger or a second face
locks at once and revokes Touch ID. Levels 0–4: level 2 for personal answers
and low/medium actions, level 3 (Touch ID or password) for high-risk actions
and enrollment, level 4 (a finger on the sensor, single-use, bound to the
operation) for critical ones.

Face identity is a factor, not authentication, and it is not equivalent to
Apple's. Rejecting other people has never been measured. Voice gives no level
at all.

**FAILED live: identity is not steady.** The fix of 2026-09-14 (unmeasurable
frames no longer count against the owner) holds in tests, but the event log
shows access still changing hundreds of times an hour with the owner at the
desk: 755 changes between 09:00 and 10:00 on 2026-09-16. The leading cause on
that day, 304 times, was a single measurement that did not match the owner,
followed about a second later by one that did; low capture quality came next.
By design one non-matching measurement denies owner operations at once, so
every such frame drops access to LEVEL_0 — which also makes answers wait and
actions refuse. The distances behind those measurements are not stored (face
measurements never reach memory), so the cause inside the matcher cannot be
read from the log; it needs a measurement session with the owner at the camera.
Any change must keep "conflicting identity = do not assume owner" true.

## 10. Voice model

| Stage | Status |
|---|---|
| Microphone permission | Asked by macOS the first time KUE uses the microphone; KUE never grants it |
| Push-to-talk (Speak) | PARTLY_LIVE_VERIFIED |
| Listening for "Computer" (hands-free) | TEST_VERIFIED_ONLY; **off by default** |
| Speech recognition | On this Mac (SpeechAnalyzer); verified on audio files |
| Speaker identity | NOT_IMPLEMENTED — deferred on purpose |
| Face + voice fusion | Designed ([KUE_SPEAKER_IDENTITY.md](KUE_SPEAKER_IDENTITY.md)); not built |
| KUE hearing its own voice | Its own sentences are recognised and ignored (tested on audio files). Echo cancellation: not implemented |
| Speaking | LIVE_VERIFIED |

So today the owner still presses Speak or turns listening on. "Computer, check
my storage" hands-free has never been heard in a real room.

**Speaker identity research.** Written up as a decision record,
[KUE_SPEAKER_MODEL_DECISION.md](KUE_SPEAKER_MODEL_DECISION.md): candidates,
licences of code and weights, dataset terms, redistribution, size, runtime,
privacy, and why published error rates are not KUE's. It recommends WeSpeaker
ResNet34-LM (ONNX, 26.5 MB, weights CC-BY-4.0) for personal use on this Mac
only, after Phase 1, and lists ten yes-or-no decisions for the owner. It found
VoxCeleb's own licence file and web pages disagree about what is licensed and
how, which blocks distributing any VoxCeleb-trained model. No model has been
downloaded, and the registry is unchanged. The speaker row no longer calls this
capability deliberately excluded: it waits on the owner's decision.

## 11. Claude integration status

**Claude, or any external model: NOT_IMPLEMENTED** (`external_model`). No API
client, no key storage, no network code. The model router refuses an external
model for every kind of personal context under policy v1.

**The boundary one would sit behind: built 2026-09-16, TEST_VERIFIED_ONLY**
(`core/src/model.rs`), in front of the on-device model that exists today:

- *Provider interface.* A model is asked only through `model::ask`, which
  admits a prompt only if the firewall cleared it for that model's destination
  under the current policy. Providers receive an `Admitted` prompt, which only
  `model::ask` can make, so no provider can skip the check. The on-device model
  process implements it, and its raw line-writer is now private.
- *Context sanitizer.* Every value placed in a prompt — app names, events, the
  owner's words, earlier answers — is folded onto one line and length-capped.
  Before this, a model answer containing a line "Owner: …" became a turn the
  owner never took in the next prompt.
- *Output validator.* Every answer — while it streams and when it finishes — is
  cut where the model starts writing the owner's next turn, corrected for claims
  of capabilities KUE lacks (as before), and now also corrected for claims of
  having done something KUE never does ("I've sent the email", "I booked it",
  "I deleted them"). A true account of what KUE's actions did ("I opened Safari
  for you", which the model can read in its context) is left uncorrected, and a
  test holds that through the real prompt path. The check is by words: a claim
  worded differently can pass it.
- Model output still has no path to a request, an action or a grant.

Six deliberate code breaks (sanitizer removed from context fields and from
earlier answers, destination check removed, action-claim check removed,
invented-turn cut removed, a verb KUE's actions produce treated as false) were
each caught by a test. The owner's current question is folded onto one line but
never shortened; earlier turns are capped at 400 characters and earlier
answers at 800.

Needs the owner before an external model can run: an Anthropic API key, and a
privacy-policy decision that some sanitized context may leave this Mac. The
design investigation of 2026-09-16 was cut off before writing anything.

## 12. Web agent status

**NOT_IMPLEMENTED.** `core/src/agent.rs` holds the stage order (search → fetch →
extract → normalize → compare → corroborate → evidence → synthesize), a status
read from the registry, and `UntrustedText` — a type for outside text that
cannot be handed back as if the owner had said it. That is not progress toward
browsing. Research requests are answered by rule: "I can't go online."

## 13. Computer-use status

**Allowlisted actions: PARTLY_LIVE_VERIFIED** — open, quit and switch apps;
open links; open a folder or document by name; list a folder; create folders
and files and read or move files inside ~/KUE; notifications; move found files to
the Trash and put them back. Each goes through authorization, the firewall, the
executor and a read-back.

**Operating inside other apps — click, type, scroll, navigate, read what is on
screen: NOT_IMPLEMENTED.** It needs macOS Accessibility, which KUE does not
request. `agent.rs` defines the observe → understand → plan → act → observe →
verify loop order only.

## 14. Calendar status

**NOT_IMPLEMENTED.** A calendar helper was started on 2026-09-16 in a separate
worktree (`kue/calendar-helper`) and cut off by the usage limit: 515 lines of
Swift source (event minimisation, a time window, keys, a command format) with no
package, no executable, no build and no tests. Preserved as a work-in-progress
commit on that branch. It has never run and has never asked for calendar
access.

## 15. Memory status

**Local event memory: LIVE_VERIFIED.** Events and allowlisted snapshots in
SQLite, policy-gated, erasable all at once (Touch ID, level 4).

**Personal memory — commitments, preferences, decisions, facts, recall:
NOT_IMPLEMENTED.** KUE cannot remember something it is told and cannot answer a
question about the past. "What did I do yesterday" is answered by rule with
that.

## 16. Proactivity status

**NOT_IMPLEMENTED.** KUE never speaks or acts unprompted. It depends on
calendar, memory and a reliable identity signal, none of which are ready.

## 17. UI/UX status

The redesigned window (one presence, one activity stream, trust signals,
Diagnostics behind a control) was seen running on 2026-09-15; three defects seen
there were fixed, and **the fixed window has not been seen running.**
Everything since — the goal waiting line, step sentences, proof in the
Capabilities sheet — is TEST_VERIFIED_ONLY.

Against the directive: the Speak button and text box are still the primary
input; hands-free listening is off by default; there is no first-launch
onboarding, no proactive or result cards in the new form, and no
Understanding/Thinking/Acting/Verifying indicators (VERIFYING is not shown on
purpose: verification happens in the same moment as execution, so an indicator
would be fake progress). A UI redesign was started on 2026-09-16 in the
`kue/ui-redesign` worktree and cut off before any commit.

The window already renders one state projection decided in core
(`core/src/surface.rs`); React holds no business logic and no authority.

## 18. Git and repository status

- **Local only.** No remote. Nothing has been pushed anywhere.
- **Branches:** `kue/runtime-safety` is the working line, well ahead of
  `main`, which it contains (a fast-forward is possible; not done). Five
  `kue/*` branches from the cut-off agents, four with no commits of their own.
  Two stale 2026-09-14 branches (`claude/personal-ai-prototype-continue-47ecde`,
  `worktree-agent-a26e37e863db25089`) already merged. `backup/main-uncommitted-identity-check`
  holds uncommitted work saved from `main` on 2026-09-14, not merged.
- **Tags:** `kue-phase0-2026-09-16`, local, at the commit that adds this document.
- **Ignored:** build output, secrets and certificates, databases, the kill latch,
  audio. Checked: no tracked file is a database, key, recording or credential.
- **Personal data** lives in `~/Library/Application Support/Lantern`, outside
  the repository (enrollment, database, lock).
- **Tools:** builds and tests from the terminal (`scripts/build-kue.sh`,
  `test-kue.sh`, `run-kue.sh`, `status-kue.sh`). No VS Code dependency.
- **Recommended, needing the owner's yes:** a *private* GitHub repository as an
  off-machine backup (nothing is created or pushed without that authorization);
  until then, `scripts/backup-kue.sh <folder>` writes a single-file git bundle
  of every branch to a folder the owner chooses.

## 19. Current blockers

**Live failure, top of the list:** identity flapping (section 9). It denies
the owner's own requests every few seconds.

Owner-only — these cannot be done or decided for the owner:

1. **Live verification at the Mac** (Phase 1 and the PARTLY rows above): open
   microphone in a real room, KUE's own voice, other voices, video, pause, kill,
   lock, microphone denial and revocation; an owner-authorized command from the
   window; a real Touch ID prompt for a move; the populated storage sheet.
2. **Speaker model:** the ten decisions in
   [KUE_SPEAKER_MODEL_DECISION.md](KUE_SPEAKER_MODEL_DECISION.md), starting with
   whether to download the recommended model at all.
3. **Claude:** an API key, and the privacy decision to let sanitized context
   leave this Mac.
4. **Calendar:** granting calendar access when KUE first asks.
5. **Private GitHub repository:** authorization to create and push.
6. **Installing KUE.app** into Applications, and a Developer ID certificate
   (ad-hoc signing resets the camera permission on some rebuilds, and the
   bundle-identifier rename should wait for real signing).

Engineering, not blocked on the owner:

7. `KUE.app` is built (2026-09-16, ad-hoc signed, bundle identifier unchanged so
   macOS permissions carry over) but has never been opened; the running app is
   the older `Lantern.app`.
8. The five background agents started earlier on 2026-09-16 all stopped at a
   usage limit; the calendar sources are preserved on their branch. The speaker
   research was redone from their saved sources and is merged.
9. The full shell test suite has never been run in one go (some of its tests
   take over the camera, screen or speakers).

## 20. Next three implementation slices

In dependency order, each audit → design → implement → test → live verify →
document → commit.

1. **Identity stability, then the Phase 1 live pass.** With the owner at the
   camera and consenting: an instrumented session that records, for each
   measurement, only derived numbers (distances to the enrollment, capture
   quality, track continuity — no images) to find why the owner's own face
   fails to match about once every few seconds; a fix that keeps a real
   conflict denying; then the same session repeated to show the access-change
   rate falls. Then the Phase 1 checks in `KUE.app`: hands-free "Computer" in a
   real room with each interference case; pause, kill and lock while listening;
   microphone denial and revocation; the storage review and a Touch ID move from
   the window. Each result recorded from the event log, not from memory.
2. **Claude as a provider behind the boundary (Phase 3b), once the owner
   decides.** The boundary exists (section 11). What remains needs the owner's
   key and a privacy-policy decision: which context kinds, if any, may go to an
   external model, stored where (Keychain), with what the owner sees before
   anything is sent. Until then an external provider stays refused.
3. **Speaker identity (Phase 2), once the owner decides.** The decision record
   is written. After Phase 1 and the owner's yes: download the named file,
   verify its checksum, build the front end, enroll, and measure false rejection
   on this Mac — and false acceptance only if the owner decides how an impostor
   set is gathered.
