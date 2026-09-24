# KUE UX architecture

The product architecture between KUE's runtime and the person using it. It
defines what the interface may say, where each sentence comes from, and what it
must never claim. Nothing here changes KUE's security architecture: the window
remains a projection of core state and a source of owner gestures, never an
authority.

Companion documents: `KUE_PRODUCT_UX_AUDIT.md` (what is wrong today),
`KUE_DESIGN_RESEARCH.md` (what the field has learned),
`KUE_DESIGN_SYSTEM.md` (how it looks).

---

## 1. Mental model

**"KUE is here with me, it knows what it can see, it does what I allow, and it
tells me what actually happened."**

Seven user-facing concepts. Everything on screen belongs to exactly one.

| Concept | The user's question | Where the truth comes from |
|---|---|---|
| **Presence** | Is KUE here, and does it know me? | `authz::AccessSession` (state, level), `identity`, runtime state |
| **Context** | What does it understand right now? | context object: identity, frontmost app, activity inference, sensors |
| **Thought** | Is it thinking, and about what? | conversation pending, model router decision |
| **Action** | What is it doing for me? | `transaction::ActionList`, `task` steps |
| **Result** | Did it work? | `ActionRecord.state` + `verification`, never inferred |
| **Memory** | What does it keep? | store counts, remembered events, privacy ledger |
| **Trust** | What can it sense, and what is it allowed to do? | sensors block, runtime, privacy policy, permissions |

Engineering vocabulary — levels, bases, transaction states, policy versions,
descriptor distances — is **not** a user-facing concept. It lives in
Diagnostics, unchanged and complete.

### The six levels of depth (brief §44)

| Level | Surface |
|---|---|
| 1 What KUE is doing for me now | Presence line + activity line + the newest moment in the stream |
| 2 What KUE understands | Context column (3–5 sentences) |
| 3 What KUE remembers | Memory sheet |
| 4 What KUE can do | Capabilities sheet (registry) |
| 5 Why KUE did that | "How did this go?" on a moment; Why on an event |
| 6 Technical diagnostics | Diagnostics mode (every existing panel, intact) |

## 2. Information architecture

One window. No tab bar. One primary surface, five sheets, one mode.

```
┌───────────────────────────────────────────────────────────────┐
│  KUE      I recognise you.            ◐ camera ◐ mic ◑ …  ⋯  │  presence + trust
├───────────────────────────────────────────────────────────────┤
│                                                │              │
│   stream of moments                            │  context     │
│   ─ you: open my resume                        │  (what KUE   │
│   ─ KUE: I found your résumé.                  │   understands│
│        [ Open it ] [ Not that one ]            │   right now) │
│   ─ KUE: Opening it… ▸ how did this go?        │              │
│   ─ KUE: It's open in Preview.  ✓ verified     │              │
│                                                │              │
├───────────────────────────────────────────────────────────────┤
│  ● Listening…                    [ ask or tell KUE… ]  [hold] │  activity + composer
└───────────────────────────────────────────────────────────────┘
```

- **Stream** — one timeline for questions, answers and actions. An action is a
  *moment*: one human sentence, an inline decision when one is needed, and a
  disclosure that reveals the transaction (steps, verification, risk, id).
- **Context column** — 3–5 sentences, each tagged *observed* or *inferred*.
  Collapses into a sheet below 1,040 px.
- **Presence line** — who KUE thinks is there, in words.
- **Trust strip** — camera · microphone · computer awareness · external AI ·
  memory, each with a word, not only a colour. Opens the Trust sheet.
- **Activity line** — what KUE is doing this second (§4), with a stop control
  whenever something can be stopped.
- **Sheets** — Trust & privacy · Memory · Capabilities · Identity · Voice.
  Opened from the thing they explain; they dim the stream, never replace it.
- **Diagnostics** — a mode (⌘⇧D or the ⋯ menu) that mounts today's panels
  unchanged: Access, Sensing, Identity, In view, Identity check, Recent events,
  Failure states, Cost, Privacy ledger, capability table, context JSON.
- **Killed** — replaces everything. Unmistakable, with the record and the
  two-step recovery.

### Responsive rules

| Width | Behaviour |
|---|---|
| ≥ 1,240 px | Stream + context column + full trust strip with words |
| 1,040–1,240 px | Context column narrows; trust strip keeps words |
| < 1,040 px | Context becomes a sheet; trust strip becomes icons + labels on focus/hover, still readable by VoiceOver |
| < 820 px | Presence sentence truncates to its first clause; composer keeps full width |

Nothing stretches: the stream has a maximum measure of 720 px and centres.

## 3. Presence model

Derived in core from access state, identity state and runtime state. One state,
one sentence, one optional detail.

| State | Sentence | When |
|---|---|---|
| `RECOGNISED` | "I recognise you." | AUTHORIZED_USER |
| `RECOGNISED_HELD` | "I recognise you — I can't see your face clearly this moment." | AUTHORIZED_USER while the unmeasured hold carries it |
| `UNCERTAIN` | "I'm not certain it's you." | IDENTITY_UNCERTAIN / LOW_CONFIDENCE |
| `SOMEONE_ELSE` | "Someone I don't know is here." | UNKNOWN_PERSON |
| `MORE_THAN_ONE` | "There's more than one person here." | MULTIPLE_PEOPLE |
| `NOBODY` | "Nobody is in view." | NO_PERSON |
| `LOCKED` | "Locked. Show your face, or use Touch ID." | LOCKED / AUTHENTICATION_REQUIRED |
| `NOT_WATCHING` | "I'm not watching — the camera is off." | paused, sensing down, camera stopped |
| `NOT_ENROLLED` | "I don't know your face yet." | enrolled samples < required |
| `CAMERA_BLOCKED` | "macOS is blocking the camera." | permission DENIED |
| `STOPPED` | "I'm stopped." | KUE_KILLED / KUE_RECOVERING |

Uncertainty is never hidden and never dramatised. No percentage, no biometric
number, no face box — those stay in Diagnostics.

## 4. Activity model

What KUE is doing *now*, in one line. Every state has a real producer; a state
with no producer does not exist in the enum (brief §39: no fake progress).

| State | Sentence pattern | Producer |
|---|---|---|
| `IDLE` | (line hidden) | nothing else true |
| `LISTENING` | "Listening…" | sensing reports a live microphone (fresh STARTING/LISTENING) |
| `UNDERSTANDING` | "Working out what you said…" | voice state FINISHING (transcribing) |
| `THINKING` | "Thinking on this Mac… {n}s" | conversation pending |
| `CHECKING_ACCESS` | "Checking it's still you." | action record REAUTHORIZING |
| `WAITING_FOR_YOU` | "{question} — I need your OK." | record REQUIRES_CONFIRMATION |
| `WAITING_FOR_MACOS` | "macOS is asking for Touch ID." | record REQUIRES_STRONG_AUTH |
| `ACTING` | "{Opening Google Chrome}… I'll check it worked." | record EXECUTING |
| `SPEAKING` | "Speaking." (only when no action is running) | speech controller SPEAKING |
| `DONE` | "{Google Chrome is open.} ✓ verified" | record SUCCEEDED, within 12 s |
| `UNCERTAIN_RESULT` | "I can't confirm that finished." | record UNKNOWN_RESULT |
| `FAILED` | "{I couldn't open it.}" | record FAILED |
| `BLOCKED` | "{I need you to authenticate first.}" | DENIED / AUTHORIZATION_EXPIRED / PRIVACY_DENIED |
| `NOT_FOUND` | "I couldn't find it." | NO_MATCHES |

**`VERIFYING` is deliberately absent.** Today verification happens inside the
executor, in the same step as execution, so there is no moment where KUE is
verifying and not acting. The state joins this table when a primitive verifies
separately (the in-app primitives of §33 will).

Precedence: STOPPED (system) > LISTENING/UNDERSTANDING > action states >
THINKING > SPEAKING > DONE/result > IDLE.

## 5. Trust model

Always visible, always in words, always derived from what the *sensing process
reports*, never from what the window intends.

| Signal | States | Source |
|---|---|---|
| Camera | On · Off · Blocked by macOS · Not running | `sensors.camera_state`, permission |
| Microphone | Off · Listening · Blocked by macOS · Unavailable | voice block |
| Computer awareness | On · Paused · Off | `computer_sampling_reported` |
| External AI | Blocked (policy v1) — with the reason on the sheet | privacy policy: no kind may reach EXTERNAL_MODEL |
| Memory | Keeping events · Not keeping (stopped) · Unavailable | store status, runtime |
| KUE | Running · Paused · Stopped | runtime state |

Rules: no signal is ever shown "on" while the underlying process says
otherwise; pause and stop are one click from the strip; the sheet explains, in
one paragraph each, what is kept, what is never collected, and what is
deletable — with the privacy ledger available in Diagnostics.

## 6. Capability model

One registry in core (`core/src/capabilities.rs`) is the single source of truth
for what KUE says it can do — in the window, in speech, in the model's context,
and in the answer to "what can you do?".

Each capability carries: `id`, `internal_name`, `user_name`, `user_description`,
`voice_description`, `status`, `availability` (computed at runtime from
permissions and processes), `risk`, `authorization`, `privacy` (the data kinds
it touches), `permissions` (macOS grants it needs), `verification` (how success
is established), `limits`, and `claim_phrases` — the wordings that count as
claiming this capability, used by the model-answer checker.

| Status | Meaning |
|---|---|
| `LIVE_VERIFIED` | Implemented, and exercised on this Mac with recorded evidence |
| `IMPLEMENTED` | Implemented and covered by automated tests; not yet exercised live |
| `PARTIAL` | Works within stated limits |
| `NOT_IMPLEMENTED` | Does not exist. KUE says so plainly |
| `UNAVAILABLE` | Implemented but not usable right now (permission missing, process down) — with the reason |

Invariants (tested): every `ActionKind` maps to a capability that is at least
`IMPLEMENTED`; no `NOT_IMPLEMENTED` capability has an executable path; every
claim phrase resolves to a registry id; the capability answer, the model's
context line and the Capabilities sheet are generated from the registry, so they
cannot disagree.

## 7. Memory model

Not a database screen. Three questions:

1. **What KUE remembers** — recent conclusions in sentences, with when.
2. **Why it kept them** — the provenance already stored with each event.
3. **What it never keeps** — camera frames, audio, keystrokes, clipboard,
   window titles, URLs, document names it was not asked to open, and the
   contents of files it opens for you.

Controls: erase everything (exists), and — when the core supports it — erase by
day, by kind, or by entity. The UI must not offer deletion the core cannot
perform; today it offers exactly one, and says that plainly.

## 8. Voice model

Voice is a rendering of the same state, not a second brain.

- **In:** push-to-talk (button, ⌥Space hold, or the composer's mic control).
  While the microphone is live, KUE does not speak.
- **Out:** narration from the action's recorded state through the speech gate,
  the firewall and the core queue (already built).
- **Interruption:** starting to speak stops KUE; "stop"/"cancel that" cancels a
  waiting action; the window's Stop is always available while speaking.
- **Parity:** every sentence KUE speaks has a visible counterpart in the
  stream — but the visible one may name a target the spoken one omits (speech
  carries further than a screen; see `privacy.rs::clear_utterance`).

## 9. Error model

Every failure gets: a sentence in the owner's language, the reason if it helps,
and a next step that is a real control.

| Internal | Sentence | Next step |
|---|---|---|
| `DENIED` (authorization) | "I need to be sure it's you first." | Touch ID button |
| `AUTHORIZATION_EXPIRED` | "That waited too long — I checked again and couldn't confirm it's you." | Ask again |
| `PRIVACY_DENIED` | "I can't safely handle that request." | Open Trust sheet |
| `NO_MATCHES` | "I couldn't find anything matching that." | Edit request |
| `FAILED` | "I couldn't complete that." + what the executor reported | Try again |
| `UNKNOWN_RESULT` | "I can't confirm that finished." | Check yourself / try again |
| `CANCELLED` (lock/kill) | "I stopped — the session is no longer yours." | (none) |
| Task step blocked | "I stopped after step {n}; the rest didn't run." | Retry from step {n} |
| `UNSUPPORTED` (in-app typing) | "I can't type into other apps yet." | Capabilities sheet |
| Permission missing | "I need {permission} for that, and macOS has to grant it." | Open the exact Settings pane |
| Sensing down / IPC failure | "I've lost contact with my own senses." | Restart sensing |
| Model unavailable | "My on-device model isn't available right now." | Retry / explanation |

No raw code appears in the normal experience; every message above keeps its code
in Diagnostics and in the event log.

## 10. Journeys

Implemented in this phase unless marked. "Shows" is what the person sees.

| # | Journey | Shows | Status |
|---|---|---|---|
| 1 | First launch | Welcome moment in the stream: what KUE is, what it needs, one suggested request | new |
| 2 | Permission onboarding | Per-permission moment with why/what it enables/what it does not, and the Settings button | new (camera, mic, notifications, Accessibility as *not granted* state) |
| 3 | Owner enrollment | Guided moment: "Let me learn your face" → capture progress → done | new |
| 4 | Owner recognised | Presence "I recognise you." | new wording |
| 5 | Owner uncertain | "I'm not certain it's you." + what it limits | new |
| 6 | Another person | "There's more than one person here." + actions withheld | new |
| 7 | Locked | Presence LOCKED + Touch ID control | new |
| 8 | Simple question | Stream answer with source line ("on this Mac") | reworked |
| 9–11 | Open app / folder / list folder | Moment: "Opening Google Chrome… → Google Chrome is open ✓" | reworked |
| 12 | Multi-step request | One moment with numbered steps, each with its own state | reworked |
| 13 | Accessibility needed | "I can't type into other apps yet" + what it would need | new (honest) |
| 14 | Computer interaction | — | NOT_IMPLEMENTED (next phase) |
| 15 | Action succeeds | "✓ verified" + what was verified on disclosure | reworked |
| 16 | Action fails | Failure sentence + try again | reworked |
| 17 | Verification fails | "I can't confirm that finished." | reworked |
| 18–19 | Interruption / barge-in | Speech stops; activity line shows it | exists in core, now visible |
| 20 | Paused | Presence "I'm not watching", trust strip all off | reworked |
| 21 | Killed | Killed surface | kept |
| 22 | Recovery | Two-step recovery | kept |
| 23 | External AI unavailable | "Everything runs on this Mac; nothing goes out." | new wording |
| 24 | Capability not implemented | Plain sentence + Capabilities sheet | registry-driven |
| 25 | "What can you do?" | Registry answer, grouped | registry-driven |
| 26 | "What are you doing?" | Activity line, stated back | new |
| 27 | "What do you remember about me?" | Memory sheet summary | new |
| 28 | "Why did you do that?" | The moment's disclosure: evidence, authorization, verification | new |
| 29 | "What information did you use?" | The prompt's cleared context, in words + what was withheld | new |
| 30 | "Are you watching me?" | Trust strip + sheet, answered from sensor state | new |

## 11. State model for the interface

One projection, computed in core (`core/src/surface.rs`), delivered by one
command (`get_surface`) and pushed on change. The window renders it and sends
gestures back. It contains:

```
system    RUNNING | PAUSED | STOPPED | RECOVERING        + sentence
presence  (§3)                                           + sentence, detail
activity  (§4)                                           + sentence, action id, steps
trust     camera, microphone, computer, external_ai, memory, kue  (§5)
context   [ {sentence, basis: OBSERVED | INFERRED} ]      (3–5)
attention [ {kind, sentence, action: control} ]           (needs the owner)
voice     microphone state, speaking, output availability
moments   stream entries (questions, answers, actions) already cleared by the firewall
```

Rules the projection enforces (each a test):

1. It cannot report success without `SUCCEEDED` **and** verification text.
2. It cannot report listening unless the sensing process reports a live mic.
3. It cannot report acting unless a record is EXECUTING.
4. `STOPPED` overrides everything; no sensor reads "on" while killed.
5. Unknown stays unknown: no identity claim when identity is uncertain.
6. It carries no target the firewall refused, and no engineering tag.
