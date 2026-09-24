# KUE design research

Research into how current AI and computing products communicate state, action,
uncertainty, permission and privacy — and what KUE should take from each. Done
2026-09-15 for the KUE product redesign.

**On sources.** Entries marked **[primary]** come from the vendor's own
documentation or from published guidelines. Entries marked **[secondary]** come
from press or independent write-ups and are treated as reports, not facts.
Nothing here is a measurement of KUE, and no number from a vendor's marketing is
repeated as if it were one.

---

## 1. Principles, not products

### Microsoft — 18 Guidelines for Human-AI Interaction **[primary]**

The 2019 CHI guidelines, still the most usable checklist for an assistant.
Verbatim, the ones KUE is currently failing:

| # | Guideline | KUE today |
|---|---|---|
| 1 | Make clear what the system can do | A 31-row engineering table |
| 2 | Make clear how well the system can do it | Buried in per-row caveats |
| 7 | Support efficient invocation | No hotkey, no push-to-talk gesture |
| 8 | Support efficient dismissal | No way to dismiss a suggestion or stop a read-aloud except a text link |
| 9 | Support efficient correction | "the older one" by voice only |
| 10 | Scope services when in doubt | Good: ambiguous app names ask; documents offer choices |
| 11 | Make clear why the system did what it did | Only as raw evidence arithmetic |
| 12 | Remember recent interactions | Session only, and cleared on lock |
| 15 | Encourage granular feedback | Absent |
| 16 | Convey the consequences of user actions | Partly: risk chips, confirmation |
| 17 | Provide global controls | Pause, kill, erase — good, but scattered |
| 18 | Notify users about changes | Absent |

**KUE takes:** guidelines 1, 2, 9, 11, 18 as product requirements — a capability
registry the interface, the voice and the model all read from; an explanation
for every action; an efficient correction path.
**KUE must not copy:** the checklist as a UI. These are properties, not panels.

### Google PAIR — Explainability + Trust, Errors + Graceful Failure **[primary]**

- Trust should be **calibrated**, not maximised: ability, reliability,
  benevolence. Both over-trust and algorithm aversion are failure modes.
- **Show confidence only when it is actionable.** Numeric percentages are the
  riskiest form; categories or n-best alternatives are usually better.
- Explanations should be **partial and tied to an action** the user took, with
  progressive disclosure for the curious — not a full algorithmic account.
- Error taxonomy: **system-limit**, **context** (working as intended, but the
  user's expectation was different) and **background** errors (nobody notices).
- Errors should say what the system needs and offer a way forward; hand control
  back to the user; be "safe, boring, and a natural part of the product".

**KUE takes:** confidence as words, not percentages, in the normal experience
(the arithmetic stays in Diagnostics); every failure names the next step;
context errors ("I stopped because someone else appeared") get first-class
sentences.
**KUE must not copy:** thumbs-up/down telemetry as the feedback channel — KUE's
feedback must be a local correction, not a rating sent anywhere.

## 2. Agent interfaces that act on a computer

### ChatGPT agent **[secondary, vendor help pages unreachable at audit time]**

Reported patterns: a running log of each step while it works; explicit
permission before consequential actions (purchases); "takeover" for logins where
the agent stops capturing the screen while the person types; pause/stop/take
over at any time.

**KUE takes:** the running log of real steps (KUE already has a step trail — it
just renders as tags); "stop" always available; a hand-back for anything KUE
must not do itself (passwords, payments) rather than attempting it.
**KUE must not copy:** a virtual browser the user watches. KUE acts on the real
Mac, where the user's own eyes are the display.

### Claude in Chrome / Claude computer use **[primary + secondary]**

Two modes: *ask before acting* (approve a plan that names the sites and the
approach; then autonomous within it, still pausing at irreversible actions:
purchase, account creation, download) and *act without asking*. Anthropic's own
engineering note on auto-mode reports that **users approve ~93% of permission
prompts**, i.e. per-action prompting alone is not a safety mechanism —
approval fatigue is real. **[secondary report of a primary claim]**

**KUE takes:** the distinction between *approving a plan* and *approving an
irreversible step* — KUE's risk tiers already encode it (LOW runs, MEDIUM asks,
HIGH asks + Touch ID). And the warning: confirmation dialogs must be **rare and
meaningful**, or they become a reflex. KUE's answer is that authority sits in
core (identity, level, policy), with confirmation as the last step, not the only
one.
**KUE must not copy:** a global "act without asking" switch. In KUE that would
mean the model choosing scope, which the architecture forbids.

### Claude background computer use on macOS **[secondary]**

A "working on your computer" status with elapsed time and an expandable detail
view; the capability is off by default and permission-gated in settings.

**KUE takes:** one calm status line with elapsed time, expandable to the real
steps; capability off until granted, and the grant explained where it is needed.

## 3. Assistants and system UI

### Apple — Siri / Apple Intelligence **[primary]**

- Activity is shown as an **ambient glow at the edge of the screen**, not a
  character or an avatar.
- Fluid switching between **typing and speaking**, with context carried across.
- Privacy is stated as a **property of where processing happens** (on device;
  Private Cloud Compute with independent verification), not as a settings page.
- In 2026: on-screen awareness by explicit gesture (control-click, shutter
  button) — the user points at what the assistant may look at.

**KUE takes:** ambient, non-anthropomorphic presence; type/speak parity; privacy
as a visible property of the running system; "what KUE may look at" as an
explicit, owner-initiated act.
**KUE must not copy:** the glow itself (borrowed identity), or implying a cloud
tier KUE does not have.

### macOS privacy indicators **[primary]**

A green dot for camera, orange for microphone, next to Control Center; one
indicator at a time, camera wins. Tiny, always-on, never explains itself.

**KUE takes:** always-visible sensor truth, and the discipline that an indicator
must be **on exactly when the sensor is on** — KUE's own strip must be derived
from the sensing process's reported state, never from the UI's intent.
**KUE must not copy:** colour-only signalling. KUE's strip carries words too.

### Raycast **[secondary]**

Keyboard-first command surface: hotkey → type → act. Tight spacing, dark
surfaces, restrained polish; "search + act" rather than search.

**KUE takes:** invocation by keyboard as a first-class path; a composer that
accepts a request or a question without the user choosing a mode first.
**KUE must not copy:** density as an aesthetic. KUE is an ambient presence, not
a power-user launcher; it should breathe more than Raycast does.

### Perplexity **[secondary]**

Progressive phases while working (searching → reading → writing) and citations
in the reading flow, with the source panel beside the answer.

**KUE takes:** phases that correspond to work that is really happening, and
"evidence in the flow" — KUE's equivalent of a citation is *what it observed*
and *what it verified*.
**KUE must not copy:** performing phases for latency's sake. If a KUE step takes
4 ms, it is not a step worth animating.

### Arc / The Browser Company **[secondary]**

Chrome-less main area, everything in an auto-hiding sidebar; figure-ground used
to make content the figure.

**KUE takes:** the main area belongs to the conversation; controls and context
recede until wanted.
**KUE must not copy:** hiding functionality behind discovery. KUE's trust
controls must never be hidden.

### visionOS / spatial **[secondary]**

Depth by material and dimming rather than by chrome; dim the background to focus
without losing context; heavier type weights for legibility on complex
backgrounds.

**KUE takes:** layering by surface and dimming for focus (sheets dim the
stream), a "spatial" feel from restraint rather than from 3-D decoration.
**KUE must not copy:** glass and blur as a look. On a dark Mac window this
becomes noise.

## 4. What the failures teach

### Humane AI Pin, Rabbit R1 **[secondary]**

Both shipped an ambient promise the runtime could not keep: slow responses,
unclear state, capability claimed in demos and absent in use. The consistent
verdict is that the products broke the user's mental model and had no honest
feedback when they could not do something.

**KUE takes:** the product rule that follows from KUE's own architecture — *say
only what is true, and say it immediately*. An assistant that admits "I can't
type into other apps yet" is trusted; one that stalls silently is abandoned.
**KUE must not copy:** the demo-first posture. KUE's capability registry exists
to make a demo of an unimplemented capability impossible.

## 5. Voice quality: options and what they would cost

KUE's speech architecture is provider-neutral (`SpeechProvider` in
`core/src/voice/speaker.rs`); today one provider is implemented: macOS
`AVSpeechSynthesizer` in the `kue-voice` process.

**Measured on this Mac (2026-09-15):** every installed English voice reports
`DEFAULT` quality. Samantha (en-US, compact) is the only en-US female
non-novelty voice installed. First-utterance start latency measured at ~130 ms
warm through the real process at volume 0.

| Option | What it would give | What it costs | Policy standing |
|---|---|---|---|
| **Apple Enhanced / Premium voices** (owner downloads in System Settings → Accessibility → Spoken Content) | Noticeably better prosody, same API, same process, no network | A download the **owner** must start; KUE must never do it | Allowed today; KUE already prefers higher quality automatically |
| **Local neural TTS** (e.g. Kokoro-class 80M–1B models via ONNX/MLX) **[secondary: quality and speed claims are vendors'/blogs']** | Conversational prosody offline; streaming possible | A new helper process, model weights (~80 MB–2 GB), CPU/GPU cost, and an evaluation on *this* Mac before any claim | Allowed in principle (on-device); needs measurement, not marketing |
| **Hosted TTS** (ElevenLabs, OpenAI, Cartesia…) | Best prosody today, streaming | Text leaves the Mac | **Refused by policy v1** for anything carrying owner data; `provider::route` already tests this refusal |

**Recommendation:** do not claim a "natural" voice until it has been heard on
this Mac. The honest next step is (a) tell the owner how to add an Enhanced or
Premium system voice, which KUE will then choose on its own, and (b) evaluate
one local neural provider behind the existing trait with measured latency and a
listening comparison — not a swap on faith.

## 6. Computer control: what the research says about the next phase

**[secondary, and must be verified on this Mac before anything is built]**

- `AXPress` on Chrome-rendered content can return success while nothing
  happens — the browser's accessibility shim acknowledges the action without
  forwarding it. A verification step is therefore **mandatory**, not optional.
- Setting a value in web inputs may need `AXValue`, then legacy attribute
  names, per app.
- Synthetic `CGEvent` clicks are the fallback for browsers, and are exactly the
  kind of blind coordinate action KUE's brief forbids as a default.
- Accessibility control is gated by a TCC grant the owner must give in System
  Settings; there is no programmatic bypass (and KUE must never seek one).

**Consequences for KUE's design:** every in-app primitive must return
`UNKNOWN_RESULT` rather than success when its verification cannot read the
expected state; semantic targets (AX role + label) are the primary path;
coordinates are a last resort, bounded to the target element's own frame, and
recorded as such in the action record.

---

## Sources

Primary: [Microsoft HAX Toolkit — Guidelines for Human-AI Interaction](https://www.microsoft.com/en-us/haxtoolkit/library/) ·
[Google PAIR — Explainability + Trust](https://pair.withgoogle.com/guidebook-v2/chapter/explainability-trust/) ·
[Google PAIR — Errors + Graceful Failure](https://pair.withgoogle.com/guidebook-v2/chapter/errors-failing/) ·
[Apple Newsroom — Siri AI (June 2026)](https://www.apple.com/newsroom/2026/06/apple-introduces-siri-ai-a-profoundly-more-capable-and-personal-assistant/) ·
[Apple — Apple Intelligence](https://www.apple.com/apple-intelligence/) ·
[Claude in Chrome permissions guide](https://support.claude.com/en/articles/12902446-claude-in-chrome-permissions-guide) ·
[Anthropic engineering — Claude Code auto mode](https://anthropic.com/engineering/claude-code-auto-mode)

Secondary: [OpenAI — ChatGPT agent](https://openai.com/index/introducing-chatgpt-agent/) ·
[explainx — Claude background computer use on macOS](https://explainx.ai/blog/claude-background-computer-use-cowork-code-macos-september-2026) ·
[MacRumors — menu bar privacy dots](https://www.macrumors.com/how-to/menu-bar-dot-explanation/) ·
[Perplexity interface teardown](https://aiuxplayground.com/teardowns/perplexity/citations/) ·
[Arc design analysis](https://blakecrosley.com/guides/design/arc) ·
[visionOS design guide](https://think.design/blog/the-complete-guide-to-designing-for-visionos/) ·
[Humane AI Pin / Rabbit R1 post-mortems](https://www.digitalapplied.com/blog/ai-product-failures-2026-sora-humane-rabbit-lessons) ·
[macOS accessibility write-path pitfalls](https://t8r.tech/t/macos-accessibility-ui-tree) ·
[Local TTS model comparisons](https://www.codesota.com/guides/tts-models)
