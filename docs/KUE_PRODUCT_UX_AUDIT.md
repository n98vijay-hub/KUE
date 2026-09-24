# KUE product experience audit

What the product feels like today, measured against what KUE is meant to be. The
runtime underneath is strong; the experience in front of it is a developer's
console for that runtime. This document is deliberately unflattering: praise for
work that functions is not the same as a product that is ready.

Audited on 2026-09-15, branch `kue/runtime-safety` at `4c53c80`, against the
running release build (pid 70676) and the whole of `src/` (2,618 lines across
`App.tsx`, 9 components, `types.ts`, `styles.css`), plus the IPC surface
(36 commands in `src-tauri/src/lib.rs`), the capability list
(`core/src/engine.rs::capabilities`, 31 entries), `docs/KUE_STATUS.md`,
`docs/ARCHITECTURE.md`, `README.md` and the test suite (246 core tests).

---

## A. The current experience

One window, always the same layout, no navigation and no modes.

| Region | What is in it |
|---|---|
| Title bar (46 px) | Word­mark "Lantern" + a dot; **Restart sensing** (when down), **Pause everything**, **Kill** |
| Main column (max 860 px) | Banners (IPC failure, alarms, sensing down, camera permission, notes) → hero (identity chip, activity sentence, identity detail, pause veil) → **Ask Lantern** (action cards, conversation turns, input, voice meter, model line, voice settings) → confidence meter → "Why — the full arithmetic" (evidence rows with weight × strength × reliability) → "Observed" → "Signals that disagree" → "Prediction" → "What Lantern does not know" → "Context object · schema v9" with the raw JSON |
| Right rail (372 px, fixed) | Access · Sensing · Identity · In view (skeleton figure) · Identity check · Recent events · Failure states · What Lantern costs · Privacy firewall · Local memory · What this build actually does (31 capability rows) |
| Killed | Replaces the main column with a killed screen and a two-step recovery |

Everything on screen is real — that is the system's great strength and it should
not be lost. But the screen is organised the way the *engine* is organised, not
the way a person thinks.

The window I captured while auditing was in the PAUSED state. It said "Paused"
four times: in the hero sentence, in the hero detail, in a warm veil below it,
and again under "Why". It also said, in the list of things KUE does not know:
"Anything you say — there is no microphone capture in this build" — which has
been false since the voice work landed (`core/src/engine.rs:1607`, and the same
claim in the MICROPHONE_UNAVAILABLE condition at `:1789`).

## B. What is good, and must survive the redesign

1. **Nothing is faked.** Every number traces to a sensor or to arithmetic the
   screen shows you. This is rarer than it sounds and it is KUE's identity.
2. **Limits are stated where the claim is made** — the identity caveat sits
   inside the identity panel, not in a footnote.
3. **Failure states are first-class**: named, listed, and shown as CLEAR or
   NOT_APPLICABLE rather than hidden when they are not happening.
4. **The kill screen is genuinely good product design**: unmistakable state,
   the record of who killed it and when, two-step owner-only recovery, and the
   out-of-band `touch` command shown honestly.
5. **The action card already carries the whole transaction** (risk, state,
   steps, verification), and confirmation is a real decision point.
6. **Restraint in colour**: one warm accent, four semantic tones, no gradients
   beyond a single meter fill, no neon.
7. **Voice output is decided in core**, not in the window — the window can only
   stop speech or change how it sounds. The right architecture is already there.

## C. What feels like developer tooling

- **The rail is an inspector.** Eleven stacked panels, each a key–value table.
  It is the shape of a debugger, not of an assistant.
- **"Why — the full arithmetic"** is in the primary column, above everything a
  person actually came for. `weight 3.00 × strength 0.750 × reliability 0.85`
  is a unit test rendered as UI.
- **The raw context object** (schema v9, ~1,500 lines of JSON) is one click from
  the main surface, with the config file path printed underneath.
- **Levels, states and tags leak everywhere**: `LEVEL_2`, `AUTHORIZED_USER`,
  `IDENTITY_UNCERTAIN`, `MEASURED_MATCH`, `REQUIRES_CONFIRMATION`,
  `OPEN_DOCUMENT`, `a3`, `proposed → privacy checked → authorized → …`,
  `IPC_FAILURE`, `STORAGE_UNAVAILABLE`, `PENDING_CORROBORATION`.
- **The capability list is a 31-row release-notes table** with paragraphs of
  caveats, sorted by subsystem. Row 22 is a 90-word paragraph about the speech
  gate. Nobody reads this; the model is handed the same text.
- **Identity is presented as biometrics**: `0.29× spread`, "decision bands
  accept ≤ 1.15 · reject ≥ 2.50", a 4-pip enrollment meter, and a panel whose
  purpose is to measure the matcher against another person.
- **"What Lantern costs"** — CPU percentages, footprint in MB, thermal state —
  is engineering telemetry in the middle of the product.
- **Operation requirement table** ("show what each operation requires") lists
  internal operation names.

## D. What feels unfinished

- **No first run.** A new owner sees the same eleven panels, with an
  un-enrolled identity and no path told to them. Enrollment is a button called
  "Capture sample" in a rail panel, pressed five times.
- **No onboarding for permissions.** Camera has a banner. Microphone has a
  sentence. Accessibility — needed for the next phase of computer control — has
  nothing at all.
- **The composer's empty state is a 70-word paragraph** listing example
  commands in one run-on sentence.
- **Multi-step tasks render as one grey line of text** (`Steps · 1. OPEN
  DIRECTORY — succeeded → 2. LIST DIRECTORY — running`) above the cards.
- **Answers and actions are two different visual languages** in the same box:
  actions are cards with chips, answers are quoted text with meta lines.
- **The voice settings live in a sentence** of inline `<select>`s inside a
  footnote paragraph.
- **The window does not respond to size.** Rail is fixed at 372 px, the minimum
  window is 940 px, and nothing collapses; at small sizes the main column is
  squeezed while the rail keeps its width.
- **Speaking has no visual presence.** KUE can talk, but the only sign is a
  word in a footnote and a "Stop speaking" link.

## E. What is confusing

- **Two names.** The window says "Lantern", the voice line says "KUE voice",
  the kill screen says "KUE is killed", the runtime states are `KUE_*`. A first
  look cannot tell whether this is one product or two.
- **Two pause-shaped controls** in the title bar: "Pause everything" (sensing)
  and "Kill" (everything, persistent). Their difference is explained only in a
  tooltip on the kill button.
- **"Right now" + an activity inference as the headline.** The largest text on
  screen is a guess about what the owner is doing ("Present, not interacting —
  an inference, not an observation"), not anything KUE is doing for them.
- **The identity chip and the access chip disagree by design** (identity stays
  uncertain while authorization is held), with no explanation on the surface —
  the reason lives in a rail row called "Identity basis".
- **Error text is raw**: `Not authorized: …`, `IPC_FAILURE`, and a regex that
  guesses whether a message is an error by looking for the words "error", "not
  running", "could not", "fail", "denied", "unavailable".
- **"What Lantern does not know"** mixes real limits ("what you are thinking")
  with stale ones ("no microphone capture in this build").

## F. What exposes implementation detail (and should not)

| On screen now | What the user needs instead |
|---|---|
| `AUTHORIZED_USER · LEVEL_2`, `IDENTITY_UNCERTAIN`, basis `MEASURED_MATCH` | "I recognise you." / "I'm not certain it's you." |
| `REQUIRES_CONFIRMATION`, `REAUTHORIZING`, `AUTHORIZATION_EXPIRED` | "I need your OK before I open it." / "Checking it's still you." / "That waited too long — confirm again." |
| `OPEN_DOCUMENT · from text · a3 · proposed → privacy checked → …` | "Opening your résumé." + a "how did this go?" disclosure |
| `0.47× spread`, accept/reject bands, pips | "Recognised you just now." (numbers in Diagnostics) |
| `ACTION_TARGET → INTERFACE · 116×` | "What I looked at, and where it went." |
| CPU %, MB, thermal state | Diagnostics only |
| `schema v9` + raw JSON + config path | Diagnostics only |
| 31-row capability table with engineering notes | "What I can do", grouped, in the owner's words |

## G. What should disappear from the normal experience

Not be deleted — **moved to Diagnostics** (§22 of the brief): evidence
arithmetic, confidence percentages, the context object, cost/resources, the
in-view skeleton figure, the identity-check probe panel, the privacy ledger
table, failure-state list, operation requirements, sensor key–values, action
ids, step trails, risk tags, policy versions, and the raw event timeline.

## H. Missing interactions

- Voice as a first-class input: press-and-hold or a hotkey, not a "Speak"
  button that must be found next to a text box.
- Interrupting KUE while it speaks (barge-in exists in core; there is no
  visible affordance except a text link).
- Keyboard: no shortcuts at all (no ⌘K to focus the composer, no Escape to
  cancel, no ⌘. to stop).
- Re-asking, editing or repeating a request; re-running a failed action.
- Choosing between matches without reading full paths.
- Undo/step-back after a wrong interpretation ("no, the other one" exists by
  voice only).
- Any way to ask "why did you do that?" about a specific action.

## I. Missing states

- First run / not enrolled / enrollment in progress.
- Permission needed (microphone, Accessibility, notifications, folders).
- KUE speaking; KUE interrupted.
- Thinking with elapsed time and a way to stop (exists for the model only).
- Action waiting on *you* vs waiting on *macOS* (Touch ID) — both render the
  same.
- Reconnecting after IPC failure; sensing restarting.
- Offline / model unavailable as a normal state rather than an error string.
- Empty memory vs memory unavailable.

## J. Missing information architecture

There is one screen and a rail. Everything competes for the same attention.
There is no hierarchy of "now / understanding / memory / capability / why /
diagnostics" — the brief's §44 levels do not exist in the UI.

## K. Missing onboarding

No welcome, no explanation of what KUE is, no guided enrollment, no permission
story, no first command suggestion, no "here's what I can do today" moment.

## L. Missing trust indicators

The information exists (camera state, permission, paused, ledger, kill state)
but there is no single, glanceable answer to "what is KUE sensing right now?".
A person has to read three panels in the rail to assemble it.

## M. Missing privacy controls

- No per-signal switch (camera on/off independently of the microphone).
- No plain-language "what is kept, for how long, and how to delete it".
- Deletion is all-or-nothing (`erase_memory`), matching the core's limits —
  but the UI does not say what is deletable and what is not.
- The USER_APPROVAL_REQUIRED flow (the policy's own hook for targets) has no UI.

## N. Missing action feedback

An action's progress is a chip changing text. There is no sense of an agent
working: no start, no during, no finish, nothing that draws the eye to the one
thing that changed. Speech now narrates it, and the window does not.

## O. Missing verification feedback

Verification is the heart of KUE's promise and it renders as a grey line:
`Verified: handed to Preview (pid 64893), frontmost; whether its window shows
the file is not verified`. The distinction between SUCCEEDED and UNKNOWN_RESULT
— which the core enforces rigorously — is two words in a chip.

## P. Missing error recovery

Errors are strings in a dismissible banner. None of them offers the next step:
no "Authenticate now" button on an authorization failure, no "Open Settings" for
the microphone, no "Try again" on a failed action, no "Ask me differently" on
NO_MATCHES.

## Q. Missing personalisation

No preferences beyond voice settings; no learned phrasing; no way to correct an
interpretation and have it stick; no user-visible memory of past requests.

## R. Missing AI interaction patterns

Measured against the research (see `KUE_DESIGN_RESEARCH.md`): no capability
disclosure at the point of need, no scoped uncertainty ("I found two — which?"
exists only in cards), no progressive explanation, no plan preview for
multi-step requests, no per-step confirmation model, no interruption, no
feedback loop, no notification of new capability.

## S. Design-system problems

- **No system.** 477 lines of CSS with ad-hoc values: 11 font sizes between
  9 px and 30 px, inline `style={{…}}` in 24 places, three shades of hairline,
  spacing values from 2 to 34 px with no scale.
- **Type is too small**: 9 px tags, 10.5 px notes, 11 px meta — below macOS
  norms and unreadable at a glance.
- **The single accent (warm amber) does four jobs**: brand mark, primary
  button, confidence meter, and "paused" — so "paused" looks like a highlight.
- **Chips carry the visual weight of buttons** without being clickable.
- **No elevation model, no motion language** (two transitions exist), no icon
  set, no focus style.

## T. Accessibility problems

- **Contrast below AA**: `--ink-3` (#6a6862) = **3.53:1** and `--ink-4`
  (#4a4844) = **2.16:1** on the background; both are used for body-sized text
  (notes, meta, timeline detail). AA needs 4.5:1.
- **No focus styles**: `:focus-visible` appears zero times; keyboard users
  cannot see where they are.
- **No ARIA**: one `role="img"` in the whole app; no live regions, so state
  changes (an action succeeding, KUE speaking) are silent to VoiceOver.
- **`user-select: none` globally** — answers cannot be copied.
- **No `prefers-reduced-motion`** handling.
- **State by colour alone** in several chips (tone classes with identical
  shape), and the accessibility tree is so thin that a system-level walk of the
  window returned almost no actionable elements (verified with an
  accessibility inspection of the running window during this audit).

## U. Recommended redesign (summary — detail in `KUE_UX_ARCHITECTURE.md`)

1. **One presence, not a dashboard.** The top of the window answers, in one
   sentence each: is KUE running, does it know me, what is it doing.
2. **The stream is the product.** Requests, answers and actions share one
   timeline of "moments", each with a human sentence, an inline decision when
   one is needed, and a "how did this go?" disclosure that reveals the
   transaction.
3. **A trust strip** — camera, microphone, computer awareness, external AI,
   memory — always visible, always readable in words, never colour alone.
4. **Context is a quiet column**, not a rail of inspectors: three to five
   sentences about what KUE currently understands, each labelled observed or
   inferred.
5. **Sheets, not tabs**, for Memory, Capabilities, Trust & privacy, Identity,
   Voice. Opened from the thing they explain.
6. **Diagnostics is a mode**, not a panel: everything in §G moves there intact.
7. **The capability registry becomes the single source of truth** for what KUE
   says it can do — in the window, in speech, and in the model's context.
8. **Every state the UI can show must be derived in core** and tested, so the
   window cannot claim listening, acting or success that did not happen.

---

## Addendum — after the storage-intelligence phase

The audit's central finding was that KUE was an inspector rather than a product:
it showed its own state well and the owner's situation not at all. The window
work fixed the presentation; it did not, on its own, give KUE anything new to
say.

Storage intelligence is the first answer to "what does KUE know about my
computer that I don't": it measures the drive and the allowed folders, groups
what it finds by fixed rules, shows the evidence for each finding beside the
reasoning and the cost of being wrong, and — with the owner choosing each file —
moves what they pick to the Trash, reversibly and verified one file at a time.

`docs/KUE_STORAGE_INTELLIGENCE.md` records what it does and does not do.
`docs/KUE_INTELLIGENCE_LAYERS.md` places it: layer 1 and 2 are real for storage
and nothing else, and layer 8 (proactivity) remains deliberately unbuilt.
