# KUE UX product specification

2026-09-17. A specification, not a build: nothing here is implemented unless
the "Today" column says so. Earlier design work this builds on:
[KUE_UX_ARCHITECTURE.md](KUE_UX_ARCHITECTURE.md),
[KUE_DESIGN_SYSTEM.md](KUE_DESIGN_SYSTEM.md) (tokens in `src/tokens.css`),
[KUE_PRODUCT_UX_AUDIT.md](KUE_PRODUCT_UX_AUDIT.md).

## 1. What it should feel like

"I have a personal computer intelligence working with me" — calm, present,
voice-first, and always honest about what it is doing. Not a dashboard, not a
chat window, not a diagnostics console.

Rules that shape every screen:

1. **The window renders the core's projection and nothing else.** Every state
   and every sentence comes from `core/src/surface.rs` (`get_surface`). React
   chooses layout, colour and motion; it never invents a state, a sentence or
   a capability. (Today: true, and tested.)
2. **No fake progress.** A state is shown only while the runtime is in it.
   Elapsed time is shown where waiting is real ("Thinking on this Mac… 12s").
3. **Capability words come from the registry** — never from a model, never
   hard-coded in the window.
4. **Engineering detail lives behind Diagnostics.** A person never sees
   descriptor distances, state-machine names, subsystem names, firewall
   internals or raw events unless they open it.

## 2. The primary screen

```
┌──────────────────────────────────────────────────────────────────────────┐
│ KUE                                          ● Listening for "Computer"  ⏸ ■│   ← presence + control (pause, kill)
│                                                                          │
│ Good afternoon, Vijay.                                                   │   ← greeting only if the owner gave a name
│                                                                          │
│ ┌ NOW ────────────────────────────────────────────────────────────────┐ │
│ │ You're in Chrome · Storage 90% full · Meeting in 54 min              │ │   ← context, each line seen or worked out
│ └──────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│ ┌ KUE ─────────────────────────────────────────────────────────────────┐ │
│ │ “Computer, find my latest resume.”                        you, 10:48 │ │   ← the request, as heard
│ │ SEARCHING FILES  Looking in Desktop, Documents, Downloads, KUE…      │ │   ← live state
│ │ FOUND  3 resume files                                                │ │
│ │ Resume_September_2026.docx — changed yesterday                       │ │   ← result
│ │ [Open]  [Show the others]                                            │ │
│ └──────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│ RECENTLY                                                                 │
│ 10:42  Opened Chrome · verified                                          │   ← KUE's own actions, from the record
│ 10:45  Moved 2 installers to the Trash · verified · Undo                 │
│                                                                          │
│ [ Type instead… ]                                          Diagnostics   │   ← typing is the fallback
└──────────────────────────────────────────────────────────────────────────┘
```

- **Presence** is one line at the top, not the page's headline. "I recognise
  you" stops dominating; when identity is the problem, the line says so and
  offers Touch ID.
- **The Speak button leaves the primary layout** once hands-free listening is
  on and working. It stays reachable (keyboard shortcut, and inside "Type
  instead…") for accessibility and for when listening is off.
- **"What I can do / What I remember / What I can sense"** buttons move into a
  single "About KUE" sheet; capabilities are discovered by asking or through
  onboarding, not a button row.
- **"Recently"** shows only what KUE did or what the owner did that KUE can
  attest to from its own record (apps opened by KUE, actions, meetings from
  the calendar once it exists). It does not become a surveillance timeline:
  the owner's own app switching is shown only if the owner turns that on.

## 3. Request states

The owner's list, against the states the core can already produce
(`surface.rs::Activity`):

| Owner's state | Core state today | Producer today | Missing |
|---|---|---|---|
| LISTENING | `LISTENING` | live microphone report | the hands-free variant: "Listening for 'Computer'" as a quiet presence line, not an activity |
| UNDERSTANDING | `UNDERSTANDING` | transcript being finalised | — |
| CHECKING | `CHECKING_ACCESS` | re-authorization before an action | "checking" for local lookups (file search, storage pass) as a stage name |
| THINKING | `THINKING` ("Thinking on this Mac… 12s") | model request pending | name the model when an external one exists |
| RESEARCHING | — | none: web research does not exist | built with the web agent; must show sources as they arrive |
| ASKING | `WAITING_FOR_YOU` with a question | goals waiting for the owner, choices | — |
| ACTING | `ACTING` | executor running | per-step progress for plans |
| VERIFYING | — **on purpose** | verification is instant today, so showing it would be fake | becomes real with the computer agent, whose read-back takes observable time |
| COMPLETED | `DONE` (verified) | a verified action record | — |
| BLOCKED | `BLOCKED` | refused by authorization or policy | say *who* can unblock it (Touch ID, a setting, the owner's decision) |
| UNCERTAIN | `UNCERTAIN_RESULT` | success could not be verified | — |
| WAITING FOR YOU | `WAITING_FOR_YOU`, `WAITING_FOR_MACOS` | confirmation, Touch ID | — |

Also: `SPEAKING`, `FAILED`, `NOT_FOUND` exist and stay.

## 4. Cards

**Action card** (a goal waiting): what was found, from the report, and what
will not happen without the owner.
> I found 17 large files that haven't changed in a year. About 4.7 GB.
> Nothing moves until you choose. **[Review files]**

**Result card**: what happened, verified, and what the owner can do.
> Done. Moved 2 installers to the Trash. 2.1 GB comes back when the Trash is
> emptied. **[Undo]**
(The "comes back when emptied" sentence is required: KUE never claims space
was recovered.)

**Proactive card** (only once calendar, memory and proactivity exist — not
before): reason, evidence and a way out, every time.
> Your interview with Company X is in an hour. Earlier you said you wanted to
> prepare. What do you want to focus on?
> **[Prepare with me] [Something else] [Not now]** · *Why am I seeing this?*

**Research card** (with the web agent): question, sources as they are read,
the comparison, what is uncertain, and nothing bought or sent without an
explicit confirmation card.

## 5. Capability language — one source

Sentences about what KUE can do come from `capabilities.rs`, combining
`status`, `proof` and `availability`:

| Registry | Owner reads |
|---|---|
| implemented, `LIVE_VERIFIED`, available | (nothing extra — it just works) |
| implemented, not live verified | "Available — not yet checked on this Mac." |
| implemented, `FAILED_LIVE` | "Available, but not working reliably yet: <what failed>." |
| needs a macOS permission not granted | "Needs macOS permission: <pane>." |
| `NOT_IMPLEMENTED` | "Not available yet." |
| deliberately excluded | "KUE doesn't do this, on purpose." |

**The model never describes capabilities.** "Can you open files?", "can you
search the web?" and similar are answered by rule from the registry, and model
answers are checked for both over-claims (today) and under-claims (not yet —
the "I don't have the capability to open files" answer must be caught).

## 6. Diagnostics

Opened deliberately; everything engineering goes here: identity measurements
and separation, access state machine and reasons, evidence and confidence,
environment readings, the privacy ledger, event stream, process and model
status, latency per stage (once instrumented), the identity stability harness
(once built). Today `Diagnostics.tsx` already holds most of these.

## 7. First launch — onboarding

Ask for each permission only at the step that needs it; every step can be
skipped and done later; nothing is enabled by default that the owner did not
choose.

1. **Welcome.** "Hi. I'm KUE. Before I can help, I need to know who I'm helping
   — and you need to know what I can see, hear and keep."
2. **What KUE can see, hear, store; what stays on this Mac; what may leave
   (today: nothing).** Generated from the privacy policy and the registry.
3. **Your name** (optional; used only for greeting).
4. **Camera** → macOS prompt → **face enrollment** (samples, derived
   descriptors only, Touch ID to save).
5. **Identity check** — live separation shown plainly: "I recognised you in 18
   of 20 checks."
6. **Microphone and speech recognition** → macOS prompts → a test phrase.
7. **Wake phrase**: "Say 'Computer' when you want me." A live test of the name
   and one request; hands-free listening turned on only if the owner chooses.
8. **Voice samples** — only once speaker identity exists and its model is
   approved; until then this step says it is not available.
9. **Pause, kill and authorization** — shown, and tried once.
10. **Optional, each asked only when first needed later:** Accessibility (for
    operating apps), calendar, web research and external models (with exactly
    what would leave the Mac).
11. **Try it:** "Computer, what can you do?" — answered from the registry,
    demonstrating only live-verified capabilities.

## 8. When someone else is there

- Unknown person: "I don't recognise you, so personal things are locked." No
  names, no memory, no recent activity, no files shown.
- More than one face: personal content hidden from the window at once;
  sensitive actions require Touch ID from the owner.
- Never phrased as proof: face recognition is not Apple's authentication and
  the window never says it is.

## 9. What the window is today, against this spec

| Area | Today | Gap |
|---|---|---|
| State-driven projection | Yes (`surface.rs`, tested) | new stages above |
| Presence | Large "I recognise you" headline | demote to a line; identity flapping makes it flicker (fix identity first) |
| Input | Text box + Speak button primary; hands-free toggle below, off every launch | hands-free primary, persisted, onboarding consent |
| Context column "What I understand" | Sentences from core, labelled seen/worked out | merge into "Now" |
| Capabilities | Sheet from registry, now with proof | wording table in §5; under-claim check |
| Conversation | Chat-like card | request → live stage → result card |
| Recently | — | from action records |
| Proactive, research, memory | — | depend on those systems |
| Onboarding | — | §7 |
| Diagnostics | Separate view | keep; add latency and harness |

## 10. Acceptance, for each UX slice

Built from real runtime state only; `tsc` and window tests pass; seen on this
Mac in `KUE.app` with a screenshot the owner takes or approves; every sentence
traceable to `surface.rs` or the registry; the owner can reach pause and kill
in one click from every screen.
