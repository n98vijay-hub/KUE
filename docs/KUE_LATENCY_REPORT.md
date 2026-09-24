# KUE latency — S2a: where the seconds actually go

**2026-09-20 · measured on this Mac, against the real on-device model.**

The previous slice removed 48 % of the prompt and latency did not move. This one
asked where the time is, and the answer is uncomfortable: **almost all of it is
inside Apple's model, and almost none of it is under KUE's control.**

## BASELINE

| | |
|---|---|
| Owner-visible answer time (KUE's own event log, n=31) | p50 **19.9 s**, p90 31.4 s, max 107.2 s |
| KUE's own first stage split (2026-09-20) | prefill **13,893 ms**, generation 3,353 ms |
| Model process | started on demand, one per shell, none running while idle |
| Session | a new `LanguageModelSession(instructions:)` **per question**, no prewarm |

## MEASUREMENTS

The helper now reports its own parts — session creation, prefill (session ready
→ first token) and generation (first → last token). Times only; no prompt text,
no answer text. Everything below drove the real binary over its real protocol.

| Stage | Measured |
|---|---|
| `MODEL_SESSION_CREATE` | **0.1 – 50 ms** — negligible, and 0.1 ms when a session was built in advance |
| `MODEL_PREFILL` | **428 ms – 17,036 ms** for the *same* work |
| `MODEL_GENERATION` | typically **~40 ms**; occasionally 3–10 s |
| `MODEL_TEARDOWN` | not measurable separately; the process outlives the answer |
| `PROMPT_PREPARATION` + `CONTEXT_BUILD` | < 1 ms (core telemetry, live) |
| `TOTAL_TIME_TO_FIRST_RESPONSE` | ≈ prefill; everything else is noise beside it |

**The shape that matters.** Ten consecutive identical questions in one process,
machine otherwise ordinary:

```
 1  13,246 ms      ← cold
 2   3,774 ms
 3   7,321 ms
 4   3,909 ms
 5–10  3,615 – 3,890 ms   ← steady state
```

And in an earlier burst, after the machine had been answering for some minutes:
**537, 546, 632, 641 ms.** The same prompt, the same code, 25× faster.

So the model has at least three regimes: **cold ≈ 7–17 s**, **warm ≈ 3.6–3.9 s**,
**hot ≈ 0.6 s**. KUE, asking one question every few minutes, lives in the first.

## EXPERIMENTS

| # | Hypothesis | Method | Result |
|---|---|---|---|
| E1 | Prompt size drives prefill | Same question with and without the 1,400-char capability table, alternating, ×3 | **11.5 s vs 13.3 s** — inside the noise |
| E2 | Prompt *and* instruction size drive it | Sweep: 60 / 400 / 1,200-char prompts × with/without the 1,176-char instructions, interleaved ×3 | 60-char prompts took **6.8–17.0 s**; 1,200-char prompts took **3.7–7.2 s**. **No size effect survives the variance** |
| E3 | Apple caches on the prompt prefix, so KUE's volatile context defeats it | Identical prompts ×6, volatile-early ×6, volatile-late ×6 | **All ~7 s.** No prefix effect |
| E4 | Session creation is expensive | Time `LanguageModelSession(instructions:)` | **0.1–50 ms.** Not the cost |
| E5 | `prewarm()` with a realistic lead helps | Fresh process, warm + 2 s or 5 s lead, vs no warm, ×3 | 7.4 s / 8.0 s / 6.8 s medians — **no material gain** |
| E6 | Two model processes contend | Same test in two binaries at once vs one | **22.3 s vs 11.5 s — latency doubles** |
| E7 | KUE spawns duplicates | Read `start_mind`; call it six times | One process; guarded. Now a regression test |

## BOTTLENECK

**Prefill, inside Apple's FoundationModels, governed by how recently the model
has been used.** Not the prompt, not the instructions, not the session, not
prefix caching, not KUE's own code paths — each of which was measured and
excluded.

The one factor KUE demonstrably controls is **contention**: two processes
answering at once doubles the wait, and that is now prevented by construction
and asserted by a test.

## NEGATIVE RESULTS

Stated plainly so nobody spends a week on them again:

1. **Cutting the prompt does not make KUE faster.** (It was still worth doing —
   less data to a model is right on its own terms — but not for speed.)
2. **Instructions are not the cost either.** Removing all 1,176 characters
   changed nothing measurable.
3. **Reordering the prompt for prefix caching does nothing.** There is no
   observable prefix cache.
4. **Prewarming a session is worth ~19 ms**, not seconds. The `warm` command
   exists and is honest about that.
5. **Session-per-question is not the problem.** Reusing a session would also be
   wrong: a `LanguageModelSession` accumulates its turns, and KUE decides what
   history a question may see. Each question gets a fresh session on purpose.

## RECOMMENDATION

| Priority | Action | Expected |
|---|---|---|
| 1 | **Answer without the model wherever a rule can.** A question never asked costs 0 ms. Reminders and calendar moved from ~30 s to ~1 ms this way | Largest real win, already partly banked |
| 2 | **Keep one model owner.** Never two answering processes | Prevents a measured 2× regression |
| 3 | **Do not build a keep-warm loop yet.** Warmth comes from *actually answering*, repeatedly. Holding it would mean running the model continuously — competing with Vision for the same silicon, which is the identity failure | Deliberate non-action |
| 4 | Tell the owner the truth in the interface: the first question after a quiet period is slow because the model is cold | Product honesty, not a speed-up |
| 5 | When a second provider is eventually considered, this is the number it must beat: **3.6 s warm, 7–17 s cold, on-device** | Evidence for a later decision |

**Expected improvement from this slice: none in the model path, by design.** The
honest gain is that the next person does not repeat E1–E5.

## REGRESSION RISKS

- The helper now reports three extra numbers per answer. They are times; they
  pass the same privacy classification as every other stage timing.
- The `warm` command builds a session in advance. It is one-use and
  instruction-matched, so it cannot carry one question's history into another —
  asserted by construction (`takeSpare` requires identical instructions and
  removes the spare).
- The single-owner test reads `lib.rs` to assert one spawn site. If the file is
  split by concern (planned), that assertion must move with the spawner.

## WHAT THE OWNER COULD SETTLE

`sudo powermetrics --samplers ane_power,gpu_power -i 1000 -n 15` during one
answer would say whether prefill is ANE-bound, GPU-bound or waiting. It needs a
password, so it is the owner's to run.
