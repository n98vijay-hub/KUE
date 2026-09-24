# KUE performance profile

Opened 2026-09-17. Measured on the owner's MacBook Air (macOS 26, Apple
silicon). Tags: **MEASURED** (with how) · **NOT INSTRUMENTED** (no number exists
yet) · **ESTABLISHED** · **HYPOTHESIS** · **DESIGN OPTION**.

## Summary

A spoken question takes **20–35 s** to answer today, and up to 107 s. Almost
all of that is the on-device model **before it produces its first word**.
Everything KUE itself does between the end of speech and the model — speech
finalisation, the intent router, context, the privacy firewall, the prompt —
takes **under 0.6 s**. The model's prompt-reading time grows with the prompt,
and while it reads, the camera's face analysis nearly stops
(`KUE_IDENTITY_STABILITY.md` §2), so slowness and identity loss are one problem.

## 1. The request path, stage by stage — MEASURED where possible

Source: KUE's event log (timestamps to the millisecond) for eight spoken
questions, 2026-09-16 and 2026-09-17. Words are not stored, so the questions
themselves are unknown.

| Stage | Measured | How |
|---|---|---|
| Voice detection → wake | **Never run live.** Hands-free listening has not been turned on in the app (0 "Listening for" events ever) | event log |
| Speech capture (push-to-talk start → stop) | 4.2–9.4 s | event log; includes the owner speaking **plus a fixed 2.5 s of silence** before capture ends |
| Speech recognition finalisation + intent router + reaching `ask` | 0.18–0.53 s (stop → "Understood") | event log, 2026-09-16 21:48 onward |
| Context, privacy firewall, prompt, hand-off to the model | ≤ 0.001 s on 2026-09-17 (0.1–1.0 s earlier, which included starting the model process) | event log ("Understood" → "Asked") |
| **Model: prompt sent → answer** | **20.3, 34.7, 31.4 s** (2026-09-17); 12.4–75.3 s (2026-09-16) | event log |
| Model history since 2026-09-14 | 31 answers: min 9.6 s, typical ~20 s, max 107.2 s | event log |
| Speech output | answer → speech finished 2.5–20.5 s | event log — **this is KUE speaking the answer, not delay**. When speech *starts* is NOT INSTRUMENTED |
| Wake detection latency, recognition latency per word, IPC per message, window render | NOT INSTRUMENTED | — |

## 2. The on-device model, isolated — MEASURED

Driven directly (`KUE.app/Contents/MacOS/lantern-mind`), no camera, KUE paused,
with KUE's real instructions (1,172 characters) and a real context prompt
(1,424 characters, printed by the opt-in test `ask_the_real_model`).

**Through KUE's own opt-in test:** 19.1, 11.0, 15.4 s for one-sentence answers.

**First text vs whole answer:**

| Question | First text | Whole answer | Length |
|---|---|---|---|
| What am I doing right now? | 11.16 s | 11.2 s | 39 chars |
| What can you tell me about my storage? | 11.33 s | 11.4 s | 37 chars |
| Explain what KUE is in two sentences. | 22.85 s | 23.2 s | 226 chars |

**ESTABLISHED:** the wait is before the first text. For short answers the text
arrives all at once at the end, so streaming it to the window or to speech
would not make these answers start sooner.

**Prompt size (two runs each; noisy):**

| Instructions | Prompt | First text |
|---|---|---|
| full (1,172 c) | full context incl. capability list (1,424 c) | 10.72 s, 10.29 s |
| full | context without the capability line (603 c) | 7.37 s, 3.79 s |
| full | the question only (38 c) | 4.30 s, 6.85 s |
| one sentence (36 c) | the question only | 12.73 s, 8.28 s (longer answers) |

**HYPOTHESIS (direction supported, not yet precise):** the prompt is the main
lever. The single capability line — 41 capabilities with their status, 820
characters — accounts for most of the difference between 10.5 s and 4–7 s.
Variance is high; a run of ≥10 per condition is needed before quoting a number.

**With the camera's face analysis running at the same time:** first text took
14.4–19.6 s instead of 10.3–11.3 s — the contention slows both sides.

## 3. Sensing under model load — MEASURED

Vision per frame (face landmarks + feature print, synthetic 1280×720 image):
alone p50 24 ms, max 96 ms; while the model reads its prompt, back-to-back
~3,000 ms frames — about 0.33 frames a second instead of 4. Details and the
experiments that ruled out CPU priority are in `KUE_IDENTITY_STABILITY.md` §2.

## 4. Bottlenecks, ranked — from the measurements

1. **On-device model prompt reading: 4–23 s before any text**, longer under
   load, occasionally 75–107 s. ESTABLISHED.
2. **Every question goes to the model.** All eight spoken questions measured
   were routed as conversation. Questions KUE could answer from what it
   already knows ("what am I working on?", "can you open files?", storage,
   status) pay the full model cost. ESTABLISHED for these eight; which
   questions they were is unknown.
3. **The prompt carries the whole capability list** on every question.
   HYPOTHESIS (§2), and the model still answered "I don't have the capability
   to open files or applications directly" with that list in front of it
   (owner's screenshot) — the cost buys no accuracy there.
4. **2.5 s of fixed silence** before push-to-talk capture ends. ESTABLISHED
   (setting in the sensing layer).
5. **Speech starts only after the whole answer.** ESTABLISHED from code; for
   long answers this matters, for short ones §2 shows it does not.
6. **Model contention stops face analysis**, so a slow answer also costs
   identity. ESTABLISHED.

Not bottlenecks (MEASURED): the intent router, context assembly, the privacy
firewall, prompt building and the hand-off — together under 0.6 s.

## 5. What to measure next — instrumentation, DESIGN (not built)

One latency record per request in the core, with monotonic timestamps and no
words: capture start, speech end detected, transcript final, intent decided
(with kind), authorization decided, context built, prompt cleared, model
request sent, first partial, answer, first speech sample queued, speech
started, window updated (reported back by the window). Kept in memory, shown
in Diagnostics as a per-stage breakdown of the last 20 requests; the event
log keeps only the totals. Plus the wake path once hands-free is on: voice
onset → name decided → request captured.

## 6. Options — DESIGN OPTIONS, to be chosen from measurements

- **Answer by rule whatever can be answered by rule:** capability questions
  ("can you open files?") from the registry; "what am I working on / what's in
  front" from context; status questions from state. Zero model time, zero
  identity cost.
- **Shrink the prompt:** drop the capability list (capability questions no
  longer need it; the answer checker still corrects overclaims); send only the
  context lines a question needs. Measure first text with ≥10 runs per variant.
- **Keep the model process and a session warm** between questions
  (FoundationModels offers session prewarming) — EXPERIMENT: first text with a
  prewarmed session vs a new session per question (today: new per question).
- **Show the real state at once**: "Thinking — on-device model" the moment the
  prompt is sent, with elapsed time; never a fake progress bar.
- **Shorter end-of-speech wait** for push-to-talk, measured against cut-off
  sentences; hands-free already ends an utterance on its own gate.
- **An external model** (Claude) for questions the owner permits to leave the
  Mac: removes on-device contention for those questions; an owner decision
  (privacy policy, key), not a performance fix to make silently.

## 7. Targets (proposed, for the owner to confirm)

| Request | Target to first visible result |
|---|---|
| Deterministic command (open, storage status, arithmetic) | < 1 s after speech ends |
| Question answered by rule | < 1 s |
| Question needing the on-device model | state shown < 0.5 s; first text < 5 s |
| Long operation (storage pass, research) | real progress shown < 1 s |
