# Lantern → KUE rename plan

The code is KUE's foundation but still carries the name Lantern in about 80 files.
This plan says what renaming each kind of name would break, so that it happens in
deliberate steps rather than as one search-and-replace. **Nothing here has been
renamed yet.** New code written for KUE (`kue-voice`, `kue-act`, `kue-auth`, the
`KUE_*` runtime states, `core/src/voice`, `core/src/task.rs`) already uses KUE.

Inventory taken 2026-09-15 on branch `kue/runtime-safety`.

## Classifications

| Class | Meaning |
|---|---|
| **SAFE_TO_RENAME** | Text or internal names. Renaming changes nothing on disk, in macOS, or across processes. Tests and a build prove it. |
| **PERMISSION_SENSITIVE** | macOS attaches a privacy (TCC) grant to it. Renaming revokes camera, microphone, speech-recognition or folder access; the owner must grant it again. |
| **SIGNING_SENSITIVE** | Part of a code-signing identity that another component checks, or that macOS uses to recognise the app. Renaming breaks those checks unless every reference moves together. |
| **DATA_MIGRATION_REQUIRED** | Names a location where the owner's data lives. Renaming without moving the data makes KUE start empty, or leaves the kill latch behind. |
| **DEFERRED** | Could be renamed, but not worth the churn or risk now. |

## Inventory

### PERMISSION_SENSITIVE

| Name | Where | What renaming breaks |
|---|---|---|
| `dev.lantern.desktop` | `src-tauri/tauri.conf.json` (`identifier`), `scripts/build-app.sh` (codesign `--identifier`) | The camera, microphone and speech-recognition grants, and the Desktop/Documents/Downloads folder grants, belong to this bundle identifier (the sensing helper's requests are attributed to its responsible process, which is this app). A new identifier is a new app to TCC: every permission prompt appears again. Also the single-instance and LaunchServices identity. |
| `dev.lantern.sense` | `sensing/build.sh` (Info.plist `CFBundleIdentifier`, codesign), `scripts/build-app.sh` | The nested sensing app's own TCC identity (camera, microphone). |
| `NSCameraUsageDescription` / `NSMicrophoneUsageDescription` / `NSSpeechRecognitionUsageDescription` / `NS*FolderUsageDescription` strings that say "Lantern" | `src-tauri/Info.plist`, `sensing/build.sh`, `scripts/build-app.sh` | Only the text of the prompt. Safe on its own, but must change **together with** the identifier, or the prompt names an app the owner does not see. |

Note: ad-hoc signing (current) already resets the camera grant on some rebuilds.
Moving to a real Developer ID identity is the right moment to change identifiers,
since the grants have to be given again then anyway.

### SIGNING_SENSITIVE

| Name | Where | What renaming breaks |
|---|---|---|
| `dev.lantern.act` | `act/build.sh` (Info.plist, codesign), `scripts/build-app.sh`, `act/Sources/KueAct/main.swift` (`protectedBundles`) | KueAct refuses to quit bundles in `protectedBundles` — it will not quit KUE itself. Renaming the identifiers without updating that set would let "quit Lantern" quit KUE's own processes. |
| `dev.lantern.auth`, `dev.lantern.voice`, `dev.lantern.mind` | `auth/build.sh`, `voice/build.sh`, `mind/build.sh`, `scripts/build-app.sh` | Codesign identifiers of the helper executables. Nothing checks them at runtime today; a future check (the shell verifying a helper's signature before trusting it — listed as missing in `KUE_STATUS.md` row 8) would. Rename with that check, not before. |
| `dev.lantern.desktop`, `dev.lantern.sense` in `protectedBundles` | `act/Sources/KueAct/main.swift` | As above. |
| Executable and bundle names `lantern`, `LanternSense.app`, `lantern-sense`, `lantern-mind` | `src-tauri/Cargo.toml` (`name = "lantern"`), `sensing/build.sh`, `mind/build.sh`, `scripts/build-app.sh`, `src-tauri/src/sensing.rs` (`locate_sensing_app`), `src-tauri/src/mind.rs` (`locate`), the `pgrep` in `build-app.sh` | The shell finds its helpers by these paths; the outer bundle's signature covers them. Rename file names and every locator in one change, then rebuild and re-sign. |

### DATA_MIGRATION_REQUIRED

| Name | Where | What renaming breaks |
|---|---|---|
| `~/Library/Application Support/Lantern/` | `src-tauri/src/lib.rs` (`support_dir`) | The local memory database, the owner's config override and the kill latch live here. A new folder name starts KUE with no memory **and no kill latch** — a killed KUE would come back running. Migration must move the latch first (or refuse to start if the old latch exists), then the database, then the config. |
| `lantern.sqlite3` | `src-tauri/src/lib.rs`, `core/src/store.rs` (tests) | The events, snapshots and privacy ledger. Same migration. |
| `lantern.toml` | `config/lantern.toml` (bundled), `src-tauri/src/lib.rs` (`load_config`) | The owner's override in Application Support would stop being read. |
| Face enrollment descriptors | `sensing/Sources/LanternSense/main.swift` (`supportDirectory()` → `Application Support/Lantern`) | Enrollment would be lost; identity would read NO ENROLLMENT until re-enrolled. |

### SAFE_TO_RENAME

| Name | Where |
|---|---|
| User-visible text "Lantern" (window title, headings, notes, capability descriptions, spoken sentences that say "Lantern") | `src/**/*.tsx`, `index.html`, `core/src/engine.rs` (capability notes), `core/src/conversation.rs` (`CAPABILITY_LIST_SOURCE`, `COMMAND_PARSER_SOURCE`) — note tests assert some of these strings, so rename test expectations with them |
| IPC event names `lantern://context`, `lantern://conversation`, `lantern://actions`, `lantern://transcript` | `src-tauri/src/lib.rs`, `src/App.tsx`, `src/components/Conversation.tsx` — internal to one app; rename both sides in one commit |
| Dispatch queue labels `dev.lantern.sense.*`, `dev.lantern.voice.out`, `dev.lantern.mind.out` | Swift sources — debugging labels only |
| Swift target directories `LanternSense`, `LanternMind` | `sensing/Sources/`, `mind/Sources/` — build inputs only (the *output* names are SIGNING_SENSITIVE above) |
| Crate names `lantern-core`, `lantern_lib` | `core/Cargo.toml`, `src-tauri/Cargo.toml`, every `use lantern_core::` — compile-time only; large diff, no runtime effect |
| Log prefix `[lantern]` | `src-tauri/src/lib.rs` |
| `productName: "Lantern"` | `src-tauri/tauri.conf.json` — changes `Lantern.app` to `KUE.app`; safe for TCC (the identifier is what counts) but moves the bundle path, so the build script's paths and `pgrep` change with it |
| The notification title `"Lantern"` | `core/src/actions.rs` (`ShowNotification` default title) |

### DEFERRED

| Name | Why |
|---|---|
| Repository and folder name `MY PERSONAL AI ASSISTANT`, git history, the worktree branch names | No functional effect; renaming disrupts open worktrees and sessions. |
| `docs/ARCHITECTURE.md` and historical sections of `docs/KUE_STATUS.md` that describe Lantern as built | Records of what was measured under that name; rewrite only when the architecture document is next revised. |
| `LATCH_FILE_NAME = "KILLED"` | Already name-neutral. |
| The capability-panel wording that calls the app Lantern in answers to "What can you do?" | Rename with the SAFE_TO_RENAME text, but only after checking the model-answer correction logic, which matches capability names. |

## Order of operations

1. **SAFE_TO_RENAME** text, IPC event names and log prefixes, in one commit with its tests. No permission or data effect.
2. **Crate names**, in one mechanical commit. Build and test.
3. **Data migration code** for `Application Support/Lantern` → `Application Support/KUE`: move the kill latch first (a latch in either location means KILLED), then the database and config; refuse to start if both folders exist with different latches. Test with a killed latch.
4. **Executable and bundle file names** together with every locator, then rebuild.
5. **Bundle identifiers and signing identities** together — `dev.lantern.*` → `dev.kue.*`, `protectedBundles` in the same commit — ideally when moving to a Developer ID certificate. Tell the owner beforehand that camera, microphone, speech recognition and folder access will be asked for again.

No step is to be done by a global search-and-replace.
