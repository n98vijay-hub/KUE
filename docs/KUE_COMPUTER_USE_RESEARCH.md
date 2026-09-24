# KUE computer use: research

Written 2026-09-17 on branch `kue/computer-use-research` (from `0263e59`).
**Research only.** No code, configuration, permission or other document was
changed to write this, nothing was installed, and nothing that controls an app
or synthesises input was run. Every KUE capability below that is not already in
the repository is a **proposal**. `in_app_control` and `screen_context` are
NOT_IMPLEMENTED in the registry today, and this document does not change that.

The question: what KUE would need to operate other applications on the owner's
MacBook Air (macOS 26, Apple silicon) safely, through the owner's six stages —

1. open or identify an app and verify it is frontmost;
2. inspect its accessibility tree (buttons, text fields, menus);
3. click, type, scroll, press keys;
4. read the visible result, detect the change, verify;
5. multi-step workflows;
6. adaptive workflows —

and what current macOS and current computer-use agents actually offer.

## How to read this document

Every substantive statement carries one tag:

- **FACT** — read in a source, cited inline. Sources are not equal, so the tag
  says which kind:
  - **FACT (doc)** — Apple reference documentation, Apple Support, or the
    vendor's own documentation, read on 2026-09-17.
  - **FACT (DTS)** — a statement by Apple Developer Technical Support (Quinn
    "The Eskimo!") on the Apple Developer Forums. Apple staff, but not reference
    documentation; it can be wrong or out of date, and several load-bearing
    points below rest only on it because the reference pages for those
    functions have no discussion text.
  - **FACT (paper)** — a research paper's own abstract or text.
  - **FACT (repo)** — this repository at `0263e59`, with file and line.
- **DESIGN DECISION** — proposed for KUE, with the reason.
- **EXPERIMENT** — must be measured on the owner's Mac before anything relies
  on it; numbered E1–E22 in section 8.
- **UNRESOLVED LIMITATION** — no known way around it; numbered U1–U18 in
  section 9.
- **UNVERIFIED** marks a point whose source could not be read, or was read only
  in a secondary source (press, blogs, GitHub issues).

Two caveats on the sources themselves:

- Several Apple Support pages served the **macOS 27** version of the page on
  2026-09-17. The owner's Mac runs **macOS 26**. Where a support page is cited,
  its version is stated; behaviour on macOS 26 is assumed the same only where
  noted, and E-numbered checks exist for anything that matters.
- Apple's documentation site renders in JavaScript; its pages were read through
  the same page's JSON data endpoint
  (`developer.apple.com/tutorials/data/documentation/…json`). The citations use
  the ordinary page URL. Some fetches returned a summary rather than raw text;
  short quotes below are only those that came back as quoted text, and
  everything else is paraphrased.

---

## 1. Scope, and what exists in KUE today

### 1.1 What KUE already does with other apps

- **FACT (repo)** — The executor, `KueAct.app`, performs one allowlisted verb
  per process: `open-app`, `focus-app`, `close-app`, `open-url`, `notify`,
  `open-file`, `open-folder`, `trash`, `untrash`. Its header states there is no
  shell, no AppleScript and no arbitrary argument
  (`act/Sources/KueAct/main.swift:1-17`).
- **FACT (repo)** — Stage 1 already exists for opening and switching:
  `open-app`/`focus-app` launch through `NSWorkspace.openApplication`, wait up
  to 15 s for `isFinishedLaunching`, then poll
  `NSWorkspace.frontmostApplication` for up to 3 s. `focus-app` that does not
  become frontmost reports UNKNOWN_RESULT; `open-app` reports SUCCEEDED with
  "frontmost" in the evidence only when it was
  (`act/Sources/KueAct/main.swift:82-104`).
- **FACT (repo)** — KueAct refuses to quit KUE's own processes, Finder, Dock,
  loginwindow, SystemUIServer and WindowManager
  (`act/Sources/KueAct/main.swift:36-37`).
- **FACT (repo)** — `computer_automation` is REAL and PARTLY_LIVE_VERIFIED
  (opening an app, a document and a folder seen from the window). `in_app_control`
  is NOT_IMPLEMENTED with risk HIGH, permission Accessibility, privacy kinds
  WindowTitle and ScreenContent. `screen_context` is NOT_IMPLEMENTED: "KUE never
  reads your screen" (`core/src/capabilities.rs:644`, `:1003`, `:1026`;
  `docs/KUE_MASTER_STATUS.md` §13).
- **FACT (repo)** — A request such as "type hello into Notes" or "open
  Calculator and add 2 and 2" is classified COMPUTER_TASK and answered by rule as
  not implemented, whole, before any step starts (`core/src/intent.rs:463-465`,
  tests at `:628-631`; `core/src/task.rs:12-15`).
- **FACT (repo)** — LanternSense reads *seconds since the last input event*
  through `CGEventSource.secondsSinceLastEventType(.hidSystemState, …)`. It reads
  a time, not keys (`sensing/Sources/LanternSense/main.swift:126-131`).
- **FACT (repo)** — The shell reports `accessibility_granted: Some(false)`
  unconditionally, and the only thing KUE does about any permission is open the
  System Settings pane for the owner (`src-tauri/src/lib.rs:131-133`,
  `open_permission_settings`, `:953-968`). No code in the repository calls an
  `AXUIElement` function or posts a `CGEvent`.

### 1.2 The gates a new in-app action would have to pass

- **FACT (repo)** — `ActionKind` is a closed enum of 15 kinds; `risk()` is
  exhaustive; `needs_confirmation` is true for every risk except LOW; an unknown
  risk tag is DENY (`core/src/actions.rs:31-62`, `:145-147`, `:172-208`).
- **FACT (repo)** — `Risk::Critical` and `Operation::ActionCriticalRisk` exist
  and no current action uses them. Critical requires LEVEL_4: a fresh, single-use
  OS authentication bound to the operation (`core/src/authz.rs:169-172`,
  `docs/KUE_MASTER_STATUS.md` §9).
- **FACT (repo)** — `ActionRecord::finish` turns SUCCEEDED without verification
  evidence into UNKNOWN_RESULT (`core/src/actions.rs:365-371`).
- **FACT (repo)** — Goals: each step names its own authority and confirmation
  (`goal::requirement`); `Purchase`, `SendMessage` and `DeletePermanently` are
  declared with NOT_IMPLEMENTED authority so their requirement is a stated fact;
  a plan is at most 4 steps (`core/src/goal.rs:187-209`, `core/src/task.rs:26`).
- **FACT (repo)** — `agent.rs` holds the loop order OBSERVE → UNDERSTAND →
  PLAN → ACT → OBSERVE_AGAIN → VERIFY, its status read from `in_app_control`,
  and `UntrustedText`, which has `len`, `is_empty` and `mentions(needle) -> bool`
  and **no accessor that returns its words**; its `Debug` prints only length and
  origin (`core/src/agent.rs:51-69`, `:94-120`).
- **FACT (repo)** — Privacy policy v1 classifies `Keystroke`, `TypedText`,
  `ClipboardContent`, `WindowTitle`, `ScreenContent`, `Url` and `DocumentName`
  as **NEVER_COLLECT** (`core/src/privacy.rs:205`).
- **FACT (repo)** — The kill switch is checked immediately before an action
  executes and between files of a move to the Trash
  (`core/src/transaction.rs:1252-1256`, `:1294`). **Once `kue-act` is running,
  nothing interrupts it:** the broker polls the child every 50 ms for up to 90 s
  and never consults the kill switch (`src-tauri/src/broker.rs:48-60`).
- **FACT (repo)** — Every helper is ad-hoc signed (`codesign --force --sign -`),
  without the hardened runtime option and with no entitlements file
  (`act/build.sh:26-27`, `scripts/build-app.sh:55-84`). The master status lists
  ad-hoc signing as a known gap and notes that rebuilds have reset the camera
  permission (`docs/KUE_MASTER_STATUS.md` §7, §19 item 6).
- **FACT (repo)** — The target of an action travels to KueAct as one JSON line
  on standard input, never in `argv`; KueAct does not check who started it
  (`core/src/transaction.rs:334-375`, `src-tauri/src/broker.rs:37-47`,
  `act/Sources/KueAct/main.swift:69-79`).

### 1.3 What that means for this research

- **DESIGN DECISION** — Stage 1 is not the first thing to build: it exists.
  The first new work is stage 2, and it is blocked by two owner decisions,
  not by engineering: granting Accessibility, and a privacy-policy change,
  because policy v1 forbids collecting the very thing stage 2 reads (§6.3).
- **DESIGN DECISION** — The in-flight kill gap (§1.2) must be closed *before*
  any verb that synthesises input exists, because an input sequence is the first
  executor work where "already running" means "still changing another app".

---

## 2. macOS mechanisms

### 2.1 Accessibility API (`AXUIElement`, `AXObserver`)

**What it can do**

- **FACT (doc)** — The AXUIElement header is the API assistive apps use to
  "communicate with and control accessible applications running in macOS". Each
  UI element is an `AXUIElementRef` (a CFTypeRef).
  [AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)
- **FACT (doc)** — Reading: `AXUIElementCreateApplication(pid)`,
  `AXUIElementCreateSystemWide()`, `CopyAttributeNames`, `CopyAttributeValue(s)`,
  `CopyMultipleAttributeValues`, `CopyParameterizedAttributeValue`,
  `GetAttributeValueCount`, `GetPid`. Acting: `CopyActionNames`,
  `PerformAction`, `IsAttributeSettable`, `SetAttributeValue`. Hit-testing:
  `CopyElementAtPosition`. Same page.
- **FACT (doc)** — `AXUIElementCopyElementAtPosition` hit-tests by window
  z-order in top-left screen coordinates; passed the system-wide element it is not
  limited to one app.
  [AXUIElementCopyElementAtPosition](https://developer.apple.com/documentation/applicationservices/1462077-axuielementcopyelementatposition)
- **FACT (doc)** — `AXUIElementSetAttributeValue` accepts property-list types,
  AXUIElement, AXValue, text markers and attributed strings; it can fail with
  attribute-unsupported, illegal-argument or cannot-complete.
  [AXUIElementSetAttributeValue](https://developer.apple.com/documentation/applicationservices/1460434-axuielementsetattributevalue)
- **FACT (doc)** — `AXObserverCreate(pid, callback, &observer)` creates an
  observer for one application; notifications are registered per element with
  `AXObserverAddNotification` and delivered through a run-loop source
  (`AXObserverGetRunLoopSource`).
  [AXObserverCreate](https://developer.apple.com/documentation/applicationservices/1460133-axobservercreate),
  [AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)
- **FACT (doc)** — The notifications an observer can receive
  (`valueChanged`, `focusedUIElementChanged`, `focusedWindowChanged`,
  `mainWindowChanged`, `titleChanged`, `created`, `uiElementDestroyed`,
  `layoutChanged`, …) are **posted by the application that owns the element**,
  through `NSAccessibility.post(element:notification:)`.
  [NSAccessibility.Notification](https://developer.apple.com/documentation/appkit/nsaccessibility-swift.struct/notification)
  → an observer sees only what the target app chooses to post (U5).

**Errors, timeouts, and why a retry is dangerous**

- **FACT (doc)** — Messaging functions can return `kAXErrorCannotComplete`
  when messaging fails or the target app is unresponsive or waiting for user
  input; `kAXErrorAPIDisabled` when the API is disabled; `kAXErrorNotImplemented`
  when a process does not fully support accessibility.
  [AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)
- **FACT (doc)** — For `AXUIElementPerformAction`, cannot-complete can occur
  because apps do modal work inside the action and miss the timeout; Apple
  states this "does not necessarily mean that the function has failed".
  [AXUIElementPerformAction](https://developer.apple.com/documentation/applicationservices/1462091-axuielementperformaction)
- **FACT (doc)** — `AXUIElementSetMessagingTimeout` sets a per-element timeout,
  or the whole process's timeout when given the system-wide element; 0 resets.
  [AXUIElementSetMessagingTimeout](https://developer.apple.com/documentation/applicationservices/1459345-axuielementsetmessagingtimeout)
- **DESIGN DECISION** — A press that returns cannot-complete is **never
  re-sent**. It may already have happened (the doc says so). KUE re-observes and
  either finds the expected post-condition (SUCCEEDED) or reports
  UNKNOWN_RESULT. This is the in-app form of "unverified is not done" (U4).

**Permission**

- **FACT (doc)** — `AXIsProcessTrustedWithOptions` returns whether the current
  process is a trusted accessibility client. With `kAXTrustedCheckOptionPrompt`
  the user is informed if it is not; the prompt is asynchronous and does not
  change the return value.
  [AXIsProcessTrustedWithOptions](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)
- **FACT (doc, macOS 27 page)** — When a third-party app tries to control the
  Mac through accessibility features, macOS alerts and the user must grant it in
  System Settings › Privacy & Security › Accessibility. Apple advises granting
  only apps you know and trust, and says such access also exposes contacts,
  calendar and other information.
  [Allow accessibility apps to access your Mac](https://support.apple.com/guide/mac-help/allow-accessibility-apps-to-access-your-mac-mh43185/mac)
- **FACT (doc, archived 2016)** — Accessibility control is off by default and
  is enabled by the user app by app; that guide also says admin credentials are
  required. [Mac Automation Scripting Guide — Automating the User Interface](https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/AutomatetheUserInterface.html)
- **UNRESOLVED LIMITATION (U7)** — Nothing read describes a way to grant
  Accessibility for *one target app*. The grant is to the client, for every app.
  A per-app allowlist in KUE is KUE's own rule, enforced by KUE's code.

**Sandbox, hardened runtime, signing, helpers**

- **FACT (DTS)** — App Sandbox, in general, blocks the Accessibility APIs; DTS
  does not support them in sandboxed apps.
  [thread 789896](https://developer.apple.com/forums/thread/789896),
  [thread 780626](https://developer.apple.com/forums/thread/780626)
- **FACT (doc)** — The hardened runtime's resource-access entitlements are
  audio input, camera, location, contacts, calendars, photos and Apple Events;
  Accessibility is not among them.
  [Hardened Runtime](https://developer.apple.com/documentation/security/hardened-runtime)
- **FACT (repo)** — KUE is neither sandboxed nor hardened (§1.2). Neither
  blocks the AX API for it; only TCC does.
- **FACT (DTS)** — TCC identifies code by its code signature; a broken or
  changed signature can cause TCC problems.
  [thread 703188](https://developer.apple.com/forums/thread/703188)
  → with ad-hoc signing every rebuild changes the signature (E2, U8).
- **FACT (DTS)** — TCC relies on *responsible code*: when an app's helper
  triggers a privacy prompt, the intent is that the app's name appears, the
  decision is recorded for the whole app, and it shows under the app's name in
  System Settings. The system decides this by heuristics that usually work and
  can break (for example a child that daemonises itself).
  [On File System Permissions, revised 2025-11-04](https://developer.apple.com/forums/thread/678819)
- **FACT (DTS)** — When some privacy settings change in System Settings, macOS
  offers to quit and relaunch the app; a developer in the same thread reports
  preflight results not changing live.
  [thread 727984](https://developer.apple.com/forums/thread/727984)
- **EXPERIMENT (E1)** — *Not established by any source read:* whether
  `KueAct.app` (its own bundle, `dev.lantern.act`, `LSUIElement`, spawned by the
  KUE app with `std::process::Command`) is attributed to KUE for Accessibility,
  whether the grant then makes `AXIsProcessTrusted()` true **inside KueAct**, and
  whether it also makes the main KUE process (and so its web view's host) trusted.
  The whole executor design depends on this answer.
- **EXPERIMENT (E3)** — Whether KueAct started from Terminal (not from KUE) is
  trusted. If it is, any process of this user could borrow KUE's grant by
  writing one JSON line to KueAct: a confused deputy at the OS level (§5).

**Coverage outside native AppKit**

- **FACT (doc)** — Electron apps enable their accessibility tree when they
  detect assistive technology, and a third-party client can switch it on by
  setting `AXManualAccessibility` with `AXUIElementSetAttributeValue`.
  [Electron — Accessibility](https://www.electronjs.org/docs/latest/tutorial/accessibility)
- **UNVERIFIED** — How much of Safari's and Chrome's web content is exposed to
  an AX client on macOS 26 was not found in a primary source read (E19).
- **UNRESOLVED LIMITATION (U3)** — Apps that draw their own controls (games,
  canvases, some cross-platform toolkits) may expose little or nothing.

**Secure (password) fields**

- **FACT (doc)** — AppKit defines a secure-text-field accessibility subrole
  (`AXSecureTextField`).
  [NSAccessibility.Subrole.secureTextField](https://developer.apple.com/documentation/appkit/nsaccessibility-swift.struct/subrole/securetextfield)
  The page does not say what reading its value returns.
- **UNVERIFIED** — Whether `AXValue` of a secure field returns nothing, a mask
  or plaintext on macOS 26. Only third-party GitHub issues were found, which
  warn against reading it. (E5, U14)
- **DESIGN DECISION** — KUE refuses by *subrole*, whatever AX would return: it
  never reads `AXValue`, never sets a value, never focuses and never posts a key
  to an element whose role or subrole is a secure field, or whose app has secure
  event input enabled (§2.2). A refused step is DENIED, not UNKNOWN.

### 2.2 Synthesised input (`CGEvent`)

- **FACT (doc)** — `CGEvent.post(tap:)` posts an event immediately before the
  event taps at that location, and the event passes through them.
  [CGEvent.post(tap:)](https://developer.apple.com/documentation/coregraphics/cgevent/post(tap:))
- **FACT (doc)** — `CGEvent.postToPid(_:)` exists (macOS 10.11+);
  `CGPreflightPostEventAccess`, `CGPreflightListenEventAccess` exist (10.15+).
  Their reference pages carry **no discussion text** — the documentation does
  not state which privilege each checks.
  [postToPid](https://developer.apple.com/documentation/coregraphics/cgevent/posttopid(_:)),
  [CGPreflightPostEventAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightposteventaccess()),
  [CGPreflightListenEventAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightlisteneventaccess())
- **FACT (DTS)** — Posting events needs its own *Post Event* privilege
  (`CGPreflightPostEventAccess` / `CGRequestPostEventAccess`), not the full
  Accessibility privilege. It appears in System Settings under Accessibility but
  is limited to posting. Posting a `CGEvent` makes the system show the TCC alert.
  It is compatible with App Sandbox.
  [thread 730441](https://developer.apple.com/forums/thread/730441),
  [thread 789896](https://developer.apple.com/forums/thread/789896)
- **FACT (DTS)** — *Listening* to keyboard events uses a different privilege,
  Input Monitoring, through `CGEventTap`
  (`CGPreflightListenEventAccess` / `CGRequestListenEventAccess`).
  [thread 744440](https://developer.apple.com/forums/thread/744440),
  [thread 789896](https://developer.apple.com/forums/thread/789896)
- **DESIGN DECISION** — KUE never requests Input Monitoring and never creates a
  listening event tap. It posts; it does not listen. `Keystroke` stays
  NEVER_COLLECT.
- **FACT (doc, archived 2007)** — Secure event input
  (`EnableSecureEventInput`) stops keyboard events reaching keyboard-intercept
  processes, including event taps, system-wide while enabled, even when that
  process is in the background; `NSSecureTextField` turns it on only while
  needed; `IsSecureEventInputEnabled` reports it.
  [TN2150: Using Secure Event Input Fairly](https://developer.apple.com/library/archive/technotes/tn2150/_index.html)
  The note is about *intercepting* keys, not *posting* them, and is marked no
  longer updated.
- **EXPERIMENT (E6)** — Whether `IsSecureEventInputEnabled()` called in KueAct
  reports another app's secure input on macOS 26. Detection only: KUE never
  tests whether a posted key reaches a secure field.
- **UNRESOLVED LIMITATION (U2)** — A posted keyboard event goes to whatever
  holds keyboard focus when it is delivered. KUE can check focus immediately
  before posting, but a window that appears, or an owner's click, between the
  check and delivery redirects the keys. No source read offers an atomic
  "type into this element" for arbitrary apps.
- **DESIGN DECISION** — Prefer setting `AXValue` on the verified element
  (§2.1) over posting keys; post keys only where the element's value is not
  settable (E10), in short chunks, re-reading the focused element before every
  chunk (E13).

### 2.3 Apple Events and AppleScript

- **FACT (doc)** — An app that sends Apple Events must declare
  `NSAppleEventsUsageDescription`. Apple's own example of the risk: an app
  automating Mail could reach Mail's personal data indirectly.
  [NSAppleEventsUsageDescription](https://developer.apple.com/documentation/bundleresources/information-property-list/nsappleeventsusagedescription)
- **FACT (doc)** — The hardened-runtime entitlement
  `com.apple.security.automation.apple-events` lets an app *prompt* for
  permission to send Apple events to other apps; it is not needed for events to
  itself or to apps with the same team ID.
  [Apple Events entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.automation.apple-events)
- **FACT (doc)** — `AEDeterminePermissionToAutomateTarget(target, eventClass,
  eventID, askUserIfNeeded)` checks permission for one *target* and one event,
  optionally asking; its page has no discussion text.
  [AEDeterminePermissionToAutomateTarget](https://developer.apple.com/documentation/coreservices/3025784-aedeterminepermissiontoautomatet)
- **FACT (doc, macOS 27 page)** — System Settings › Privacy & Security ›
  Automation lists each app, and for each the apps it may control, to turn on or
  off. [Allow apps to control other apps](https://support.apple.com/guide/mac-help/allow-apps-to-control-other-apps-mchl07817563/mac)
- **FACT (doc, archived 2016)** — Scripting dictionaries are app-specific and
  partial; "UI scripting" (clicks and keystrokes through System Events) relies on
  the accessibility frameworks and the same per-app enablement.
  [Mac Automation Scripting Guide](https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/AutomatetheUserInterface.html)
- **EXPERIMENT (E17)** — Whether the per-target prompt appears once per target
  app for KUE, and what `AEDeterminePermissionToAutomateTarget` returns with
  `askUserIfNeeded = false` before any prompt.
- **DESIGN DECISION** — Apple Events are **not** in the first stages. Where a
  target app has a scripting dictionary for the exact operation (a named command
  with a checkable result), it is a better executor path than the UI, but each
  command would be its own allowlisted verb with fixed parameters — never a
  script string. KueAct's rule "no AppleScript" stays until such a verb is
  proposed on its own.

### 2.4 App Intents and Shortcuts

- **FACT (doc)** — App Intents let an app express its actions and data so
  Siri, Spotlight, Shortcuts, widgets and Apple Intelligence can use them.
  [App Intents](https://developer.apple.com/documentation/appintents)
  The overview read does not describe one third-party app invoking another app's
  intents directly (UNVERIFIED); the documented user path is Shortcuts.
- **FACT (doc, macOS 27 page)** — The `shortcuts` command runs, lists, views
  and signs shortcuts; `run` takes `--input-path` and `--output-path`/
  `--output-type`, exits 0 on success and 1 on error; a shortcut that shows an
  alert or asks for input pauses the command.
  [Run shortcuts from the command line](https://support.apple.com/guide/shortcuts-mac/run-shortcuts-from-the-command-line-apd455c82f02/mac)
- **FACT (repo)** — KUE has no shell and forbids "run this command" by
  construction (`core/src/actions.rs:8-10`). Running `shortcuts` would need a
  dedicated executor verb that starts that one binary with fixed arguments.
- **UNRESOLVED LIMITATION** (part of U6) — A shortcut does whatever it
  contains, and KUE cannot inspect that. Exit code 0 says the shortcut finished,
  not that its effect happened.
- **DESIGN DECISION** — Shortcuts come after stage 3, as `RUN_SHORTCUT { name }`
  for names on an owner-kept list, HIGH risk by default, verified only by a
  result the shortcut writes where KUE can check it (for example a file in
  `~/KUE`). E18 first.

### 2.5 NSWorkspace and NSRunningApplication

- **FACT (doc)** — `NSWorkspace.didActivateApplicationNotification` is posted
  when an app is about to be activated, with the `NSRunningApplication` in
  `userInfo`; it arrives only on the workspace's own notification center.
  [didActivateApplicationNotification](https://developer.apple.com/documentation/appkit/nsworkspace/didactivateapplicationnotification)
- **FACT (doc)** — `NSRunningApplication.activate(from:options:)` (macOS 14+)
  returns whether the system allowed the request, and does not guarantee
  activation; cooperative activation expects the active app to yield first.
  [activate(from:options:)](https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(from:options:))
- **FACT (repo)** — KueAct already uses `openApplication` and
  `frontmostApplication` and has been seen opening Chrome, Calculator and a PDF
  on this Mac with read-back, with no privacy prompt recorded
  (`docs/KUE_MASTER_STATUS.md` §5).
- **EXPERIMENT (E14)** — How often activation of the target succeeds when KUE's
  own window is frontmost, under cooperative activation.

### 2.6 ScreenCaptureKit and Vision text recognition

- **FACT (doc)** — ScreenCaptureKit captures displays, apps and windows;
  `SCContentFilter` can select a single window or a display excluding apps;
  `SCScreenshotManager` takes single frames; `SCContentSharingPicker` is a
  system picker for choosing what to share. The framework asks apps to request
  Screen Recording with `NSScreenCaptureUsageDescription`.
  [ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- **FACT (doc)** — Apple's sample notes the system prompts for Screen Recording
  on first run and the app must restart after the grant.
  [Capturing screen content in macOS](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos)
- **FACT (doc)** — The Persistent Content Capture entitlement is for VNC apps
  and is granted by Apple on request.
  [Persistent Content Capture](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.persistent-content-capture)
- **FACT (doc, macOS 27 page)** — The owner manages this under Privacy &
  Security › Screen & System Audio Recording, and what a third-party app
  collects that way is governed by that app's own terms.
  [Control access to screen and system audio recording](https://support.apple.com/guide/mac-help/control-access-screen-system-audio-recording-mchld6aa7d23/mac)
- **UNVERIFIED** — Since macOS 15, periodic re-confirmation prompts for apps
  that capture without the system picker. Found only in press reports
  (9to5Mac, 2024-08-14), not in an Apple source read (E15).
- **FACT (doc)** — Vision's `RecognizeTextRequest` (macOS 15+) recognises text
  in an image, with a speed/accuracy level, language selection and custom words.
  [RecognizeTextRequest](https://developer.apple.com/documentation/vision/recognizetextrequest)
  The page read does not state, in quoted text, that it never uses the network
  (E16).
- **DESIGN DECISION** — **No screen capture in any stage of this plan.** A
  frame holds everything visible: other apps, notifications, other people's
  messages, passwords shown in plaintext. `ScreenContent` is NEVER_COLLECT under
  policy v1, `screen_context` says KUE never reads the screen, and OCR text gives
  no element to act on and no semantic state to verify. Apps with no usable
  accessibility stay out of reach (U3).

### 2.7 Browser control

- **FACT (doc)** — Chromium's page for third-party Mac apps says JavaScript
  injection through AppleScript is (or will be) off by default in Chrome, and the
  user re-enables it at View › Developer › Allow JavaScript from Apple Events; it
  recommends an extension with native messaging instead.
  [Chromium — Information for third-party applications on Mac](https://www.chromium.org/developers/applescript/)
- **UNVERIFIED** — Safari's equivalent Develop-menu setting was not described in
  any Apple page read (E22, read-only).
- **FACT (doc)** — Safari's WebDriver: enable once with `safaridriver --enable`
  (possibly with `sudo` after an upgrade). Automation runs in separate windows
  that start clean and cannot reach Safari's history or AutoFill; a transparent
  glass pane blocks the user's input to them; one session at a time.
  [Testing with WebDriver in Safari](https://developer.apple.com/documentation/webkit/testing-with-webdriver-in-safari),
  [About WebDriver for Safari](https://developer.apple.com/documentation/webkit/about-webdriver-for-safari)
- **FACT (doc)** — From Chrome 136, `--remote-debugging-port`/`-pipe` are
  ignored for the default Chrome data directory and need a non-standard
  `--user-data-dir`; the reason is cookie theft by infostealers (2025-03-17).
  [Changes to remote debugging switches](https://developer.chrome.com/blog/remote-debugging-port)
- **FACT (doc)** — Playwright downloads its own browser builds (hundreds of MB,
  into `~/Library/Caches/ms-playwright`), can drive installed branded Chrome or
  Edge, and cannot drive branded Safari.
  [Playwright — Browsers](https://playwright.dev/docs/browsers)
- **FACT (doc)** — Playwright MCP acts on the page's accessibility tree rather
  than pixels and says of itself that it is not a security boundary.
  [microsoft/playwright-mcp](https://github.com/microsoft/playwright-mcp)
- **DESIGN DECISION** — Browser control is **out of scope** here. Every route
  either (a) runs in an isolated session that is not the owner's signed-in
  browser (safaridriver, CDP with a separate profile, Playwright), (b) needs a
  developer setting that widens the browser's attack surface (JavaScript from
  Apple Events), or (c) needs extra processes and downloads KUE forbids. And web
  pages are the most hostile input there is. It belongs to the web-agent phase,
  which `docs/KUE_AGENT_ARCHITECTURE.md` already orders first and gives its own
  session and adversarial corpus. Stages 2–4 exclude browsers from the app
  allowlist.

---

## 3. Comparison

Legend: ✔ yes · ◐ partly · ✘ no. "Fits KUE's executor" = a fixed verb in
one-shot KueAct, with a checkable post-condition, without a shell, network or
download.

| Mechanism | Read UI | Act | Verify | Permission (who grants) | Privacy cost | Reliability | Fits KUE's executor? |
|---|---|---|---|---|---|---|---|
| AX read (`CopyAttributeValue`, tree walk) | ✔ roles, subroles, states; labels/values as the app supplies them | ✘ | ✔ re-read | Accessibility (owner, System Settings) — for all apps | Medium–high: labels and values can be any on-screen text; roles/counts alone are low | ◐ depends on each app's AX; cannot-complete when busy | ✔ (E1 decides which process) |
| AX act (`PerformAction`, `SetAttributeValue`) | — | ✔ press, set value, raise, show menu | ✔ re-read; ◐ ambiguous on cannot-complete | Accessibility | Same as read | ◐ semantic, survives layout changes; unsupported actions in some apps | ✔ |
| `AXObserver` notifications | ◐ change events | ✘ | ◐ only what the app posts | Accessibility | Low if only kinds are kept | ◐ app-dependent (E9) | ◐ needs a run loop for the step's lifetime |
| `CGEvent` post (keys, clicks, scroll) | ✘ | ✔ anything a user can do | ✘ by itself — needs AX read-back | Post Event (DTS), shown under Accessibility | Low to send; the text typed is the owner's | ◐ focus races (U2); coordinates break on layout | ◐ only paired with AX verification |
| `CGEventTap` listen | keys typed | ✘ | ✘ | Input Monitoring | **Very high** (keystrokes) | — | ✘ **never** |
| Apple Events (scripting dictionary) | ◐ app model, not UI | ✔ named commands | ✔ often via the same dictionary | Automation, per target app (owner) | Medium: can reach app data (Mail example) | ✔ where the dictionary covers it | ◐ one fixed verb per command, later |
| UI scripting via System Events | ✔ | ✔ | ◐ | Accessibility + Automation | as AX | ◐ fragile | ✘ script strings; use AX directly |
| App Intents via `shortcuts run` | ✘ | ✔ what the shortcut does | ◐ exit code only | Shortcuts' own and each action's prompts (E18) | Depends on the shortcut | ◐ opaque contents | ◐ fixed verb, owner-listed names |
| NSWorkspace / NSRunningApplication | frontmost app, launch state | ✔ open, activate, quit | ✔ | none observed | Low (app identity) | ✔ (activation not guaranteed) | ✔ exists |
| ScreenCaptureKit + Vision OCR | ◐ pixels → text | ✘ | ◐ weak | Screen Recording (owner) | **Very high**: everything visible | ◐ OCR errors, no elements | ✘ policy v1 |
| safaridriver (WebDriver) | ✔ DOM | ✔ | ✔ | one-time `--enable` (admin) | Isolated session | ✔ in its own windows | ✘ not the owner's session; web phase |
| Chrome DevTools Protocol | ✔ DOM | ✔ | ✔ | launch flags + separate profile | High if a profile with data is used | ✔ | ✘ separate process; not the default profile |
| Playwright | ✔ DOM / a11y | ✔ | ✔ | Node + browser download | as CDP | ✔ | ✘ downloads, network |
| Screenshot loop with a cloud model (e.g. Anthropic computer use) | ✔ pixels | ✔ via your own input code | ◐ model judges screenshots | Screen Recording + Post Event; an API key | **Highest**: every screenshot leaves the Mac | ◐ (§4) | ✘ `external_model` refused under policy v1 |

---

## 4. Computer-use agent approaches

### 4.1 Where the field is (benchmarks)

- **FACT (paper)** — OSWorld (2024): 369 tasks on real desktops (Ubuntu,
  Windows, macOS); humans completed over 72.36%, the best model then 12.24%,
  mainly failing at GUI grounding and operational knowledge.
  [arXiv:2404.07972](https://arxiv.org/abs/2404.07972)
  Its experiments compare screenshot, accessibility-tree, combined and
  set-of-marks observations; the per-modality numbers were read only through a
  summarising fetch and are not repeated here (UNVERIFIED detail). They are two
  years old; no current leaderboard page could be read (the fetch was
  redirected), so no current state-of-the-art figure is claimed.
- **FACT (paper)** — macOSWorld (2025): 202 multilingual tasks across 30 apps,
  28 macOS-only, with a safety subset testing susceptibility to deception
  attacks; proprietary agents exceeded 30% success, open-source ones stayed
  under 5%, and non-English interfaces degraded results.
  [arXiv:2506.04135](https://arxiv.org/abs/2506.04135)
- **FACT (paper)** — UFO2 (Windows, 2025) fuses the platform accessibility API
  (UI Automation) with vision-based parsing and prefers native app APIs where
  they exist.
  [arXiv:2504.14603](https://arxiv.org/abs/2504.14603)

### 4.2 Screenshot-based loop (Anthropic computer use)

- **FACT (doc)** — The developer's application runs the loop: the model asks
  for actions (screenshot, zoom, clicks, drag, mouse move, scroll, type, key,
  hold key, wait); the application performs them in its own environment and
  returns results, screenshots as images; coordinates are screenshot pixels and
  must be rescaled (Retina included). Current toolset
  `computer_toolset_20260801`.
  [Computer use tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- **FACT (doc)** — Documented risks: the model may follow instructions found
  in content (web pages, images) over the developer's; precautions recommended
  are a dedicated VM or container with minimal privileges, no access to
  sensitive data such as logins, an allowlist of internet domains, and a human
  confirming decisions with real-world consequences (cookies, financial
  transactions, terms of service). Classifiers scan tool results, screenshots
  included, and steer the model to check with the user; opt-out exists. End users
  should be told of the risks and consent. Dropdowns and scrollbars can be
  difficult; keyboard shortcuts are suggested. Same page.
- **FACT (doc)** — Anthropic reports about 1% attack success for Claude Opus 4.5
  against an internal adaptive attacker in browser use, and states that "No
  browser agent is immune to prompt injection" (2025-11-24).
  [Mitigating the risk of prompt injections in browser use](https://www.anthropic.com/research/prompt-injection-defenses)
- **Cost to KUE** — **DESIGN DECISION:** not adopted. It requires (1) Screen
  Recording, (2) `external_model`, NOT_IMPLEMENTED and refused for personal
  context by policy v1, and (3) sending every screenshot — whatever is on the
  owner's screen at that moment — off the Mac. Its own documentation's first
  mitigation is a dedicated VM, and KUE's whole purpose is the owner's real Mac.
  Reliability is also bound to pixels: layout, scaling and pop-ups move targets.

### 4.3 Accessibility-tree agents

- **FACT (doc)** — Playwright MCP gives the model structured accessibility
  snapshots instead of pixels, so no vision model is needed and targets are
  unambiguous element references. [playwright-mcp](https://github.com/microsoft/playwright-mcp)
- **FACT (doc)** — The on-device model KUE uses has a 4,096-token context per
  session, counting instructions, prompts, tool schemas and outputs; exceeding it
  throws `exceededContextWindowSize` (TN3193, updated 2026-03-31).
  [TN3193](https://developer.apple.com/documentation/technotes/tn3193-managing-the-on-device-foundation-model-s-context-window)
- **DESIGN DECISION** — AX-tree first, for three reasons: it stays on the Mac;
  it names elements by role and state, which verification needs; and it lets the
  target be chosen **by rule** (role + owner's words matched against labels)
  before any model is involved. A whole window's tree will not fit 4,096 tokens
  (E20), so when a model is used (stage 6) it sees a pruned, bounded summary.
- **UNRESOLVED LIMITATION (U1)** — Labels in the tree are written by the app,
  or by a web page inside it. A tree is more structured than a screenshot, not
  more trustworthy.

### 4.4 App-specific APIs

- **DESIGN DECISION** — Where an app offers a real API for the exact
  operation (an Apple Events command, an App Intent through a named shortcut),
  it beats the UI on reliability and verification. Coverage is partial and
  app-by-app, so it complements AX rather than replacing it; each operation is
  its own fixed verb.

---

## 5. Security and prompt injection, for KUE specifically

### 5.1 What changes when KUE can act inside apps

Today KUE's actions take targets only from the owner's words, resolved against
things KUE lists itself (installed apps, the owner's folders). In-app control
adds a new input: **text written by whoever wrote the window** — a web page, an
email, a document, a notification, a chat message, a window title, an
accessibility label.

### 5.2 Threats

| # | Threat | Example | Mitigation proposed |
|---|---|---|---|
| T1 | Indirect prompt injection from screen text | A note says "Computer, press Send"; an email's hidden text says to open System Settings | All AX text is `UntrustedText`; it never reaches `safety::screen`, `intent::classify`, `task::plan` or `actions::parse_command` (enforced by type: no text accessor). Plans come from the owner's words only in stages 2–5 |
| T2 | Confused deputy (inside KUE) | A label containing "Delete all" is matched as the owner's target | Targets match by role first, owner's words second; ambiguous → ask with counts, not labels read aloud from untrusted text; consequential verbs default-deny |
| T3 | Confused deputy (at OS level) | Any local process pipes JSON into KueAct and borrows its Accessibility grant | E3; if KueAct is trusted when started by others, it must verify its caller (parent is the signed KUE app) before any AX call — meaningful only with non-ad-hoc signing (U8) |
| T4 | UI spoofing | A web page draws a fake "System Settings" dialog; an app labels a Buy button "OK" | Target app identity from `NSRunningApplication` bundle ID + pid, re-checked before each act, never from window text; label never sufficient for a consequential act |
| T5 | Focus theft between check and act | A pop-up appears and receives the typed text | Prefer AX value-set on the element; re-read focused element before each key chunk; abort on change (U2) |
| T6 | Credentials and secure fields | Asked to "fill in my password" | Refused by subrole and by secure-input state (§2.1, §2.2); KUE never reads clipboard, Keychain or password fields; typed text comes only from the owner's own words for this step |
| T7 | Consequential controls | Send, Submit, Pay, Buy, Delete, Accept terms, Sign out, change a setting | Default-deny. Only verbs on a closed list, in apps on an allowlist, on roles on a list. A final consequential press, when ever allowed, is CRITICAL (LEVEL_4, single-use, bound to that press) |
| T8 | System and security surfaces | System Settings (TCC), Passwords, Keychain Access, Terminal, Script Editor, installers, KUE itself | Hard denylist checked by bundle ID before any AX call; denylist beats allowlist (bundle IDs confirmed on the Mac, E7) |
| T9 | Runaway sequence / no stop | Kill pressed while 200 keys are being posted | Broker checks the kill switch every poll and terminates the child; KueAct posts in chunks with no held modifiers (E12) |
| T10 | Exfiltration through the model | Screen text copied into a prompt, later an external model | UI text is a new data kind the firewall must clear per destination; never `EXTERNAL`; on-device only after an owner decision (§6.3) |
| T11 | Reading what the owner types | Keystroke logging "to learn" | Never Input Monitoring, never a listening tap; `Keystroke` stays NEVER_COLLECT |

- **FACT (doc)** — OWASP LLM01:2025 separates direct from indirect prompt
  injection and recommends least privilege, human approval for high-risk
  operations, segregating external content and adversarial testing.
  [OWASP LLM01:2025](https://genai.owasp.org/llmrisk/llm01-prompt-injection/)
- **DESIGN DECISION** — `UntrustedText` keeps its shape: no method that returns
  its words. What in-app control needs from it is **predicates**
  (`mentions`, and a proposed exact `label_equals`), both returning `bool`. That
  is enough to pick an element the owner named and to verify, without an escape
  hatch.
- **DESIGN DECISION** — The model (stage 6) can only choose among element
  handles KUE's observation produced and verbs the broker allows. It never
  outputs coordinates, key codes, free text to type (the text is the owner's), or
  an app to open outside the allowlist.
- **DESIGN DECISION** — Before stage 3 ships, an adversarial corpus for in-app
  control: TextEdit/Notes documents and local test windows containing injected
  instructions, fake dialogs, misleading labels, a secure field with a
  "normal"-looking label, and focus-stealing pop-ups. Each case asserts that
  nothing beyond the owner's request happened.

### 5.3 Boundaries KUE never crosses

- **DESIGN DECISION** — KUE never turns on, off or changes any TCC permission
  or System Settings value for the owner. It may open the pane (as today) and it
  may call `AXIsProcessTrustedWithOptions` with the prompt option **only when the
  owner explicitly asks to set up app control** — that shows macOS's own alert,
  which only points to System Settings. System Settings is on the denylist, so KUE
  cannot click its own grant.
- **DESIGN DECISION** — Never bypass, disable or probe around secure event
  input; never read, type into or focus a secure field.
- **DESIGN DECISION** — Never read keystrokes (no Input Monitoring), never
  read the clipboard, never capture the screen.
- **DESIGN DECISION** — Never use private or undocumented API to change
  responsibility attribution (for example the `responsibility_spawnattrs_setdisclaim`
  technique described by
  [Qt, 2022](https://www.qt.io/blog/the-curious-case-of-the-responsible-process),
  a third-party blog) or TCC entitlements.

---

## 6. Proposed design, on the existing pipeline

### 6.1 Where each part sits

```
INTENT         intent::classify → COMPUTER_TASK, parsed by rule into
               {app, operation, owner's target words, owner's text}      (owner's words: OwnerMessage/ActionTarget)
AUTHORIZATION  per step, at the moment it runs: operation_for(risk)       (unchanged engine)
PRIVACY        firewall clears UI_STRUCTURE / UI_LABEL for the destination (new kinds, §6.3)
ACTION         broker → KueAct verb, one step, bundle ID + element path on stdin
OBSERVATION    KueAct AX read → roles/states/counts (+ labels as UntrustedText in core, stage 2+)
VERIFICATION   predicate over a fresh AX read (§6.5) → Succeeded | UnknownResult
RESULT         ActionRecord::finish; events: kind, risk, source, state — never a label or value
```

- **DESIGN DECISION** — All AX and `CGEvent` calls live in KueAct only. The
  Tauri process and the React window never link or call them (a source test can
  hold that, as the existing source-order tests do). If E1 shows the grant
  attaches to the whole KUE app, the main process will *hold* the privilege
  without *using* it (U16).
- **DESIGN DECISION** — KueAct stays one process per step. An `AXUIElementRef`
  cannot outlive the process, so an element handle crosses steps as an
  **element path**: `{bundle ID, pid, window index, list of (role, subrole,
  index among siblings)}` plus an observation number. KueAct re-resolves it and
  refuses (FAILED, `STALE_ELEMENT`) if the pid, role or path no longer match. A
  longer-lived session process is considered only at stage 5 if E8 shows
  per-step startup is too slow.

### 6.2 New ActionKinds and risk (proposal)

| ActionKind (proposed) | KueAct verb | Mechanism | Risk → operation | Confirmation | Stage |
|---|---|---|---|---|---|
| `INSPECT_APP_UI { app }` | `inspect-ui` | AX read, roles/subroles/counts only | LOW → ACTION_LOW_RISK (LEVEL_2) | none | 2a |
| `FIND_UI_ELEMENT { app, role, words }` | `find-ui` | AX read incl. labels, matched in KueAct by predicate | MEDIUM (reads labels, like FIND_DOCUMENT) | none | 2b |
| `PRESS_UI_ELEMENT { app, element, expected }` | `press-ui` | `AXPress` | MEDIUM for allowlisted benign roles (tab, disclosure, a View-menu item); **refused** otherwise | owner | 3 |
| `SET_UI_TEXT { app, element, text }` | `set-ui-text` | `AXValue` set | MEDIUM (text into a field, not submitted) | owner | 3 |
| `TYPE_TEXT { app, element, text }` | `type-text` | `CGEvent` keys after AX focus check | HIGH (focus race, U2) | owner + macOS | 3 (only where E10 shows set fails) |
| `PRESS_KEYS { app, combo }` | `press-keys` | `CGEvent`, combo from a closed list (Tab, arrows, Escape, ⌘Z) | LOW–MEDIUM by combo; Return/⌘Return/⌘S/⌘Q/⌘W/⌘⌫ not on the list | by risk | 3 |
| `SCROLL_UI { app, element, direction }` | `scroll-ui` | AX scroll-bar value / scroll action | LOW | none | 3 |
| `SELECT_MENU_ITEM { app, path }` | `menu-ui` | AX menu bar, `AXPress` | by item: Undo MEDIUM; anything that saves, sends, deletes or quits refused | by risk | 3–4 |
| `UNDO_LAST_UI_EDIT { app }` | `menu-ui` | Edit › Undo when `AXEnabled` | MEDIUM | owner | 4 |
| `PRESS_CONSEQUENTIAL { app, element }` | — | — | CRITICAL (LEVEL_4, single-use, bound to this press) — **not proposed for any stage here**; needs its own decision, like `messaging` and `purchasing` | owner + macOS | — |
| `RUN_SHORTCUT { name }` | `run-shortcut` | `/usr/bin/shortcuts run <name>` | HIGH | owner + macOS | after 4 |

- **DESIGN DECISION** — Confirmation attaches to a **step or a sequence**,
  never to each input event: `needs_confirmation` derives from risk alone
  (`core/src/actions.rs:203`), so per-keystroke confirmation would be unusable.
  The owner confirms "Type 23 characters into the body of the front TextEdit
  document" once; the kill switch and Cancel stop it at any time.
- **DESIGN DECISION** — A new registry row per slice (e.g. `app_ui_structure`,
  PARTIAL) rather than flipping `in_app_control`, which stays NOT_IMPLEMENTED
  until stage 3 exists and is live-verified. The registry gate keeps refusing any
  kind whose row is not implemented.
- **DESIGN DECISION** — App allowlist for stages 2–4: TextEdit, Notes,
  Calculator (native, low stakes). Denylist always: System Settings, Passwords,
  Keychain Access, Terminal, Script Editor, Installer, Shortcuts (editor),
  KUE/Lantern and its helpers, loginwindow, and every browser until the web
  phase. Changing either list is a code change, not a runtime setting.

### 6.3 Privacy: the owner decision stage 2 needs

- **FACT (repo)** — `ScreenContent` and `WindowTitle` are NEVER_COLLECT
  (`core/src/privacy.rs:205`), and `screen_context` tells the owner KUE never
  reads the screen (`core/src/capabilities.rs:644-662`).
- **DESIGN DECISION** — Stage 2a is designed to need **no** policy change:
  `UI_STRUCTURE` holds only roles, subroles, booleans (enabled, focused,
  settable) and counts — no label, value, help text or window title ever leaves
  KueAct. Proposed class: LOCAL_ONLY, destinations Interface and Speech; not
  memory, not any model.
- **DESIGN DECISION** — Stage 2b onward needs a **policy v2 decision by the
  owner**, of the same kind as the external-model decision in master status §11:
  a new `UI_LABEL` kind (labels, titles and descriptions of controls in
  allowlisted apps; never `AXValue` of text content, never window titles),
  classified USER_APPROVAL_REQUIRED — shown to the owner, used in KueAct and core
  only as `UntrustedText` predicates, never stored, and never given to any model
  until a further decision for the on-device model only. Until the owner decides,
  stages 2b–6 stay refused.

### 6.4 Observation

- **DESIGN DECISION** — Walk from `AXFocusedWindow` of the target app, depth ≤
  12, elements ≤ 500 (tuned by E8), per-element messaging timeout 1 s set with
  `AXUIElementSetMessagingTimeout`, whole-step deadline 5 s (the broker's 90 s
  ceiling is far too long for in-app verbs). A partial walk is reported as
  partial, never as the whole window.
- **DESIGN DECISION** — Secure fields are counted by subrole and nothing else
  is read from them.

### 6.5 Verification, per verb

Verification is a **predicate over a fresh AX read**, returned by KueAct as
evidence made only of roles, counts, lengths and booleans. A predicate that does
not hold within its timeout → UNKNOWN_RESULT; nothing is retried blindly.

| Verb | Before acting (preconditions) | Success predicate (after) | Timeout | On cannot-complete | Reversible? |
|---|---|---|---|---|---|
| `inspect-ui` | pid frontmost, bundle ID allowlisted, trusted | second read: same pid, same focused-window role, total count within ±5% | 1 s | UNKNOWN_RESULT | n/a (reads) |
| `find-ui` | as above | exactly one element satisfies role + predicate; else ASK with count | 2 s | UNKNOWN_RESULT | n/a |
| `press-ui` | path re-resolves; role/subrole as expected; `AXEnabled`; action in `CopyActionNames` | the verb's declared post-condition: a window/sheet of expected role appeared, or `AXValue`/`AXSelected`/`AXExpanded` changed, or the menu closed (AXObserver where the app posts, poll otherwise) | 2 s | **no re-press**; re-observe; post-condition or UNKNOWN_RESULT | app-specific |
| `set-ui-text` | element settable, not secure, focused element unchanged | `AXValue` character count == expected, and KueAct's equality check returns true (the value itself never leaves KueAct) | 1 s | re-read; never re-set if the value already matches | Undo if Edit › Undo enabled (E21) |
| `type-text` | as above + secure input off (E6) + focus re-read before each chunk | as `set-ui-text`; abort mid-way if focus changes → PARTIALLY_SUCCEEDED with count typed | 1 s/chunk | n/a | not guaranteed: typing appends |
| `press-keys` | combo on the closed list; focus as expected | combo-specific (⌘Z: value length changed back; Tab: focused element changed) | 1 s | re-observe | by combo |
| `scroll-ui` | scroll area exists | scroll-bar `AXValue` changed in the right direction | 1 s | re-observe | yes (scroll back) |

- **DESIGN DECISION** — Idempotency: every act carries the observation number
  it was planned against; KueAct refuses to act if the fresh read differs in
  what the precondition names. A retry is a new step with a new observation.
- **UNRESOLVED LIMITATION (U9, U10)** — Undo is an app feature, not a
  guarantee; what has been sent, submitted, bought or deleted in another app
  cannot be taken back by KUE. That is why those controls are refused.

### 6.6 Kill switch during input

- **DESIGN DECISION** — The broker's poll loop reads the kill switch on every
  tick and terminates the child at once
  (`src-tauri/src/broker.rs:48-60` today does not). KueAct posts keys in chunks
  of at most a few characters, never holds a modifier down across events (flags
  ride on each key event), and a killed step records CANCELLED with how many
  characters were delivered as far as KueAct had reported. E12 measures kill to
  last-event latency and checks for stuck modifiers.
- **UNRESOLVED LIMITATION (U9)** — Events already delivered stay delivered.

### 6.7 What the owner must grant or decide

1. **Accessibility** for the entry macOS shows (KUE or its helper — E1), in
   System Settings, by hand. Not Input Monitoring. Not Screen Recording. Not
   Automation (until an Apple Events verb is proposed).
2. **Post Event** if macOS asks separately when stage 3 first posts a key (E11).
3. **Privacy policy v2**: whether `UI_LABEL` may exist (§6.3), before stage 2b.
4. **Signing**: a Developer ID certificate before the grant is expected to
   survive rebuilds and before the caller check in T3 means anything (E2, U8;
   master status §19 item 6).
5. Which apps join the allowlist after TextEdit, Notes and Calculator.

---

## 7. Stage-by-stage plan

Each stage: audit → design → implement → test → live verify → document →
commit, and the next starts only after the previous is live-verified.

| Stage | Owner's stage | Delivers | Needs first | Live acceptance (summary) |
|---|---|---|---|---|
| 1 | Open/identify, verify frontmost | **Exists** (`open-app`, `focus-app`). Add: in-flight kill (§6.6) | — | Kill during a slow `open-app` → child gone ≤ 1 s, record not SUCCEEDED |
| 2a | Inspect tree (structure) | `INSPECT_APP_UI` — **first vertical slice**, below | Accessibility grant | Below |
| 2b | Inspect tree (labels) | `FIND_UI_ELEMENT` by role + owner's words | Policy v2 decision; E5, E7 | "Find the Bold button in TextEdit" → exactly one AXButton; label never in events; injected label text changes nothing |
| 3 | Click, type, scroll, keys | `SET_UI_TEXT`, `PRESS_UI_ELEMENT` (benign roles), `SCROLL_UI`, `PRESS_KEYS` (closed list); `TYPE_TEXT` only if E10 requires | Adversarial corpus; E6, E9, E10, E11, E12, E13 | "Type 'shopping list' into a new TextEdit document" → Confirm → value length matches, equality true, Undo restores; a focus-stealing window mid-sequence → aborted, PARTIALLY_SUCCEEDED; kill mid-sequence → no stuck keys |
| 4 | Read result, detect change, verify | Per-verb predicates as a library; AXObserver where apps post; `UNDO_LAST_UI_EDIT` | E9, E21 | Each verb's predicate fails when the effect is suppressed (mutation checks), and passes live in each allowlisted app |
| 5 | Multi-step workflows | Fixed goal blueprints of stage-3 verbs, each step authorized and verified at its moment; stop on anything but verified success | E8 latency | "Open Notes, make a new note, type X" (3 steps) runs, and stops at step 2 when Notes is on a different account/view |
| 6 | Adaptive workflows | On-device model proposes the next step from a bounded role+label summary; broker validates against the allowlists | Policy decision for model destination; E20; corpus passes | Injected on-screen instructions in every corpus case produce no extra act; model proposals outside allowlists refused and recorded |

### 7.1 The first vertical slice

**Slice 2a — INSPECT_APP_UI: read the structure of an allowlisted app's front
window, roles and counts only.**

The owner asks, typed or spoken, "Computer, what's in the TextEdit window?" →
`safety::screen` → `intent::classify` (a new COMPUTER_TASK sub-kind) → registry
gate on a new `app_ui_structure` row (PARTIAL; `in_app_control` stays
NOT_IMPLEMENTED) → authorization of ACTION_LOW_RISK (LEVEL_2) at the step →
firewall clears a new `UI_STRUCTURE` data kind for the Interface and Speech only →
kill switch checked → broker starts KueAct verb `inspect-ui` with the bundle ID on
standard input, and terminates it if KUE is killed mid-walk → KueAct: the app must
be running, frontmost, on the allowlist (TextEdit, Notes, Calculator) and not on
the denylist, else DENIED before any AX call; `AXIsProcessTrustedWithOptions`
without the prompt option, else FAILED `PERMISSION_REQUIRED`; messaging timeout 1 s;
walk `AXFocusedWindow` to depth 12 / 500 elements; return counts per
`AXRole`/`AXSubrole`, whether a focused element exists, whether any secure field
is present, and whether the walk was complete — no `AXTitle`, `AXDescription`,
`AXValue`, `AXHelp` or window title is read into the result → verification: a
second walk from a fresh `AXUIElementCreateApplication` within 1 s gives the same
pid, still frontmost, the same focused-window role and a total count within ±5%
→ SUCCEEDED, otherwise UNKNOWN_RESULT → the window shows "TextEdit's front window:
1 text area, 7 menus, 3 buttons"; the event log records kind, risk, source and
state only. A separate owner command, "set up app control", is the only thing
that ever calls `AXIsProcessTrustedWithOptions` with the prompt option.

**Live acceptance test on the Mac** (owner present; results read from the event
log, not from memory):

1. Before any grant: ask → FAILED `PERMISSION_REQUIRED`; no system alert
   appears; System Settings unchanged; the event row holds kind and state only.
2. Owner says "set up app control" → macOS's alert appears; the owner grants in
   System Settings › Privacy & Security › Accessibility by hand. **Record the name
   of the entry that appeared** (E1).
3. Owner opens a new TextEdit document and types a sentinel word → ask →
   SUCCEEDED; counts include at least one text area and the menu bar; the
   sentinel is absent from `lantern.sqlite3` (read-only query), from the
   interface payload and from anything KueAct printed.
4. System Settings frontmost → DENIED before any AX call.
5. Owner away until LOCKED → DENIED before KueAct starts.
6. Kill during a walk → no SUCCEEDED record; KueAct gone within 1 s (measure).
7. Owner revokes the grant → the next ask is FAILED `PERMISSION_REQUIRED` (E4).
8. Rebuild ad-hoc, repeat step 3 → record whether the grant survived (E2).
9. From Terminal, run `kue-act inspect-ui` with the same input line → record
   whether it is trusted (E3). **If it is, the slice does not ship** until the
   executor verifies its caller.
10. Twenty asks → record p50/p95 duration and any cannot-complete (E8).

Pass: 1, 3, 4, 5, 6 and 7 behave exactly as stated; 2, 8, 9 and 10 are recorded
as measurements.

---

## 8. Experiments

All on the owner's Mac, macOS 26, with the owner present. None is run by this
research. None enables a permission on the owner's behalf.

| # | Question | How | Recorded |
|---|---|---|---|
| E1 | Which process does the Accessibility grant belong to? | On "set up app control", KueAct (spawned by KUE) calls `AXIsProcessTrustedWithOptions(prompt: true)`; owner grants the entry shown. Then log `AXIsProcessTrusted()` from KueAct and, read-only, from the main process | Entry name; trusted in KueAct y/n; trusted in main process y/n |
| E2 | Does the grant survive a rebuild? | Ad-hoc rebuild, repeat E1's check without re-granting; later repeat with Developer ID signing | Trusted after rebuild y/n; whether a stale entry remains |
| E3 | Can another process borrow the grant? | Start KueAct `inspect-ui` from Terminal | Trusted y/n; which app macOS would prompt for |
| E4 | Does revocation apply to the next one-shot process? | Revoke in System Settings while KUE runs; ask again | First ask after revocation: trusted y/n |
| E5 | What does AX return for a secure field? | A local test app with an `NSSecureTextField` holding a dummy string; read `AXRole`, `AXSubrole`, and whether `AXValue` is empty/masked/plain (compared in-process, printed as a category only) | Category per app type (AppKit, one Electron app) |
| E6 | Can KueAct detect secure input in another app? | Focus the test secure field, call `IsSecureEventInputEnabled()` from KueAct | true/false; never post keys in this test |
| E7 | AX coverage and bundle IDs of candidate apps | `inspect-ui`-style walk of TextEdit, Notes, Calculator, Finder, one Electron app; read bundle IDs of denylisted apps from their bundles | Element counts, % with titles, % pressable, % settable values; bundle IDs |
| E8 | AX read latency | 20 walks per app, depth/element caps as §6.4 | p50/p95 ms, cannot-complete count |
| E9 | Are AX notifications reliable? | Observe `AXValueChanged`, `AXFocusedUIElementChanged`, `AXWindowCreated` around owner-made edits | Delivered within 1 s: rate per app |
| E10 | Does setting `AXValue` behave like typing? | Set a text area's value in TextEdit/Notes; check the document is marked edited, Undo is enabled, and the app keeps the change after focus moves | y/n per app |
| E11 | Does posting keys need a separate grant? | First `CGEvent` post from KueAct after E1 | Alert shown y/n; entry name |
| E12 | Kill latency during input | Kill after the first chunk of a long dummy string into a test document | ms from kill to last delivered character; any stuck modifier |
| E13 | Focus theft detection | Owner clicks another window mid-sequence | Aborted before the next chunk y/n; characters misdelivered |
| E14 | Activation under cooperative activation | `focus-app` while KUE's window is frontmost, 20 times | Success rate |
| E15 | Screen-recording re-prompts on macOS 26 | Only if screen capture is ever proposed | Prompt observed, interval |
| E16 | Vision OCR offline | Only if OCR is ever proposed: run with networking off | Works offline y/n |
| E17 | Automation consent per target | Only if an Apple Events verb is proposed: `AEDeterminePermissionToAutomateTarget(askUserIfNeeded: false)` for one target | Status before/after one prompt |
| E18 | `shortcuts run` from a helper | Run an owner-made no-op shortcut from a KueAct verb | Prompts, attribution, exit code |
| E19 | Web content exposure in browsers | AX walk of Safari/Chrome showing a local test page | Elements exposed (web phase only) |
| E20 | Tree size vs the on-device model | Token count of role-only and role+label summaries for E7's windows | Tokens vs 4,096 |
| E21 | Undo availability after KUE's edit | Read Edit › Undo `AXEnabled` after `set-ui-text` | y/n per app |
| E22 | Safari's JavaScript-from-Apple-Events setting | Owner looks at Safari's Develop menu; nothing is changed | Present y/n; state |

---

## 9. Unresolved limitations

- **U1** — Accessibility labels are supplied by the app, or by a web page
  inside it; they can lie. Nothing authenticates what a control does.
- **U2** — Posted keys go to whatever has focus at delivery; the focus check
  and the delivery cannot be made atomic for arbitrary apps.
- **U3** — Apps with little or no accessibility (custom-drawn UIs, games,
  some toolkits) are out of reach without screen capture, which this plan
  excludes.
- **U4** — `kAXErrorCannotComplete` on a press is ambiguous by Apple's own
  statement; some steps will end UNKNOWN_RESULT.
- **U5** — AX notifications arrive only if the target app posts them.
- **U6** — KUE cannot reliably tell a consequential control from a benign one
  by its label; default-deny keeps KUE safe and also keeps it limited. A
  shortcut's contents are opaque to KUE.
- **U7** — The Accessibility grant covers every app; KUE's app allowlist is
  self-imposed.
- **U8** — With ad-hoc signing, grant durability is doubtful, a replaced helper
  would inherit trust, and a caller check proves little.
- **U9** — The kill switch stops what has not been delivered; it cannot recall
  what has.
- **U10** — Undo is app-specific and not guaranteed; sending, submitting,
  buying and deleting in another app are irreversible to KUE.
- **U11** — Every browser route is either an isolated session, a developer
  setting that widens attack surface, or a download KUE forbids; the owner's
  signed-in browsing is reachable only through AX or JavaScript from Apple
  Events.
- **U12** — 4,096 tokens on the on-device model: whole windows do not fit.
- **U13** — Prompt injection is unsolved industry-wide; Anthropic says no
  browser agent is immune. KUE's defence is structural (no path from screen text
  to a command), which limits what adaptive workflows can ever do.
- **U14** — What AX returns for a secure field on macOS 26 is unverified.
- **U15** — Several Apple Support pages read were the macOS 27 versions; the
  Mac runs macOS 26. The macOS 26.0 release notes read list no Accessibility,
  TCC, ScreenCaptureKit or Apple Events changes.
- **U16** — If the grant is recorded for the whole KUE app (E1), the main
  process and the process hosting the web view hold the privilege even though
  only KueAct uses it.
- **U17** — Labels are localized; matching the owner's words to labels by rule
  is language-dependent (macOSWorld found non-English interfaces degrade agents).
- **U18** — Stage 2b onward is blocked on an owner privacy decision; without
  it KUE can describe a window's structure but not find a named control.

---

## Sources

All read on **2026-09-17**. Apple Developer pages were read through their JSON
data endpoints.

**Apple reference documentation and Apple Support**

- https://developer.apple.com/documentation/applicationservices/axuielement_h
- https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions
- https://developer.apple.com/documentation/applicationservices/1462091-axuielementperformaction
- https://developer.apple.com/documentation/applicationservices/1460434-axuielementsetattributevalue
- https://developer.apple.com/documentation/applicationservices/1459345-axuielementsetmessagingtimeout
- https://developer.apple.com/documentation/applicationservices/1460133-axobservercreate
- https://developer.apple.com/documentation/applicationservices/1462077-axuielementcopyelementatposition
- https://developer.apple.com/documentation/appkit/nsaccessibility-swift.struct/notification
- https://developer.apple.com/documentation/appkit/nsaccessibility-swift.struct/subrole/securetextfield
- https://developer.apple.com/documentation/coregraphics/cgevent/post(tap:)
- https://developer.apple.com/documentation/coregraphics/cgevent/posttopid(_:)
- https://developer.apple.com/documentation/coregraphics/cgpreflightposteventaccess() (no discussion text)
- https://developer.apple.com/documentation/coregraphics/cgpreflightlisteneventaccess() (no discussion text)
- https://developer.apple.com/library/archive/technotes/tn2150/_index.html (archived, 2007-06-08)
- https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/AutomatetheUserInterface.html (archived, 2016-06-13)
- https://developer.apple.com/documentation/bundleresources/information-property-list/nsappleeventsusagedescription
- https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.automation.apple-events
- https://developer.apple.com/documentation/coreservices/3025784-aedeterminepermissiontoautomatet (no discussion text)
- https://developer.apple.com/documentation/security/hardened-runtime
- https://developer.apple.com/documentation/appintents
- https://developer.apple.com/documentation/appkit/nsworkspace/didactivateapplicationnotification
- https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(from:options:)
- https://developer.apple.com/documentation/screencapturekit
- https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos
- https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.persistent-content-capture
- https://developer.apple.com/documentation/vision/recognizetextrequest
- https://developer.apple.com/documentation/technotes/tn3193-managing-the-on-device-foundation-model-s-context-window (updated 2026-03-31)
- https://developer.apple.com/documentation/webkit/testing-with-webdriver-in-safari
- https://developer.apple.com/documentation/webkit/about-webdriver-for-safari
- https://developer.apple.com/documentation/macos-release-notes/macos-26-release-notes
- https://support.apple.com/guide/mac-help/allow-accessibility-apps-to-access-your-mac-mh43185/mac (macOS 27 version served)
- https://support.apple.com/guide/mac-help/allow-apps-to-control-other-apps-mchl07817563/mac (macOS 27 version served)
- https://support.apple.com/guide/mac-help/control-access-screen-system-audio-recording-mchld6aa7d23/mac (macOS 27 version served)
- https://support.apple.com/guide/shortcuts-mac/run-shortcuts-from-the-command-line-apd455c82f02/mac (macOS 27 version served)
- https://support.apple.com/guide/safari/use-the-developer-tools-in-the-develop-menu-sfri20948/mac (did not describe the Apple Events setting)

**Apple Developer Technical Support, Apple Developer Forums**

- https://developer.apple.com/forums/thread/678819 — "On File System Permissions", responsible code (revised 2025-11-04)
- https://developer.apple.com/forums/thread/703188 — TCC identifies code by signature
- https://developer.apple.com/forums/thread/727984 — relaunch after privacy changes; preflight not live (developer report)
- https://developer.apple.com/forums/thread/730441 — Post Event privilege for synthetic events
- https://developer.apple.com/forums/thread/744440 — listen/post preflight functions vs Accessibility
- https://developer.apple.com/forums/thread/780626 — sandbox: Input Monitoring yes, Accessibility no
- https://developer.apple.com/forums/thread/789896 — sandbox blocks AX; posting limited privilege
- https://developer.apple.com/forums/thread/721441 — responsible code in System Settings attribution
- https://developer.apple.com/forums/thread/100414 — Safari JavaScript from Apple Events (users only; no Apple staff statement)

**Other vendors' documentation**

- https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool
- https://www.anthropic.com/research/prompt-injection-defenses (2025-11-24)
- https://www.chromium.org/developers/applescript/
- https://developer.chrome.com/blog/remote-debugging-port (2025-03-17)
- https://playwright.dev/docs/browsers
- https://github.com/microsoft/playwright-mcp
- https://www.electronjs.org/docs/latest/tutorial/accessibility
- https://genai.owasp.org/llmrisk/llm01-prompt-injection/ (2025)

**Papers**

- https://arxiv.org/abs/2404.07972 — OSWorld (v2, 2024-05-30); https://arxiv.org/html/2404.07972 read through a summarising fetch only
- https://arxiv.org/abs/2506.04135 — macOSWorld (revised 2025-10-18)
- https://arxiv.org/abs/2504.14603 — UFO2 (2025-04-25)

**Secondary, used only where marked UNVERIFIED or as context**

- https://www.qt.io/blog/the-curious-case-of-the-responsible-process (2022-02-04)
- https://9to5mac.com/2024/08/14/macos-sequoia-screen-recording-prompt-monthly/ (search result only; not fetched)
- GitHub issues on AX secure-field handling (search results only; not relied on)

**Could not be read**

- https://os-world.github.io/ — redirected; current leaderboard not read
- https://developer.apple.com/documentation/appintents/making-onscreen-content-available-to-siri-and-apple-intelligence — HTTP 404 at the address tried
