# Foundation slice — perception observability: evidence

| | |
|---|---|
| Date | 2026-09-20 |
| Repository | `main` = `kue/runtime-safety`, one canonical line |
| Code under test | `03d2af1` and the three commits before it |
| Build | `KUE.app`, release, ad-hoc signed, built and **run** from this branch |
| Raw output | [`kue-evidence-2026-09-20.txt`](kue-evidence-2026-09-20.txt), produced by `./scripts/evidence-kue.sh` |
| Who was at the Mac | The owner was present for part of the window (a face is confirmed in 50 % of samples). **No seated protocol was run** — the numbers below are from ordinary use, not from the 30-minute test the roadmap asks for |

## Test counts

| Suite | Result | Note |
|---|---|---|
| Rust core (`cargo test -p lantern-core`) | **416 passed, 0 failed** | was 388 before this slice |
| macOS shell (`cargo test -p lantern`) | **10 passed, 0 failed, 6 ignored** | the 6 are the opt-in live tests |
| Live tests run by hand | **4 of 6**: storage measurement (passes, repaired), Trash round trip (passes), on-device model (passes, ×4), cleared-context answer (passes) | not run: open-Chrome and the Tampa flow — both take over the screen |
| Window (`vitest`) | **24 passed** | was 21 |
| TypeScript (`tsc --noEmit`) | 0 errors | |

Ignored tests are not evidence of anything: two remain unrun and are named above.

## What was built

| Piece | Where | Live? |
|---|---|---|
| Eight measurement states, decided from facts | `core/src/measurement.rs` | **yes** — all but `CAMERA_KILLED` observed today |
| Sensing heartbeat, 1 Hz, sent whether or not a frame was produced | `sensing/…/PipelineMonitor.swift`, `Protocol.swift`, `Camera.swift`, `main.swift` | **yes** — verified from the real binary before the app was built, and in the app since |
| Per-stage timings, with the model's phase on the same timeline | `core/src/telemetry.rs`, shell instrumentation | **yes** |
| Both records classified and cleared by the privacy firewall | `core/src/privacy.rs` (`PERCEPTION_TIMING`, `STAGE_TIMING`, both `LOCAL_ONLY`) | **yes** |
| Two bounded tables | `core/src/store.rs` | **yes** |
| Diagnostics panel | `src/components/Perception.tsx` | **built and type-checked; not seen on screen** |

## The four questions this slice existed to answer

### 1. Does model inference actually delay perception? **Yes — measured, from KUE's own process.**

KUE answered a question during the window. Its own instrumentation recorded:

| Stage | Duration |
|---|---|
| `MODEL_PREFILL` | **13,893 ms** |
| `MODEL_GENERATION` | 3,353 ms |

**81 % of that answer was spent reading the prompt, before a single token came
back.** This is the first time the 20-second answer has been split into parts.

At the same moments, the perception samples carry `model_phase = MODEL_PREFILL`
together with `measurement_age_ms` of 3,091 and 3,159 — past the 3.0 s staleness
limit that demotes identity.

### 2. How long are the stalls, and where are they? **~3 s, and in two different places.**

Nine moments crossed 2.5 s of measurement age in the window. They split into two
mechanisms, which the instrument can now tell apart:

| Mechanism | Example | Vision analysis | Capture gap |
|---|---|---|---|
| **Analysis stall** — one Vision call takes seconds | 13:06:53 | **3,201 ms** | 38 ms |
| **Delivery stall** — frames stop arriving | 13:04:27 | 16 ms | **790 ms** (3,528 ms in an earlier run) |

Four deliberate model runs (the on-device model in a *separate* process, as a
load test) reproduced this every time: **4 of 4 runs pushed the measurement age
past 3.0 s**, with maxima of 3,320 / 3,417 / 3,416 / 3,082 ms. Baseline with the
model idle: worst measurement age 268 ms, worst Vision analysis 56 ms.

The audit's hypothesis was "Vision stalls". The evidence says: *sometimes
Vision, sometimes frame delivery, and the stall lands within 400 ms of the
3-second limit either way.*

### 3. Does the pipeline stay alive during those stalls? **Yes.**

Of the samples with no current measurement, **89 were `MEASUREMENT_DELAYED`
(the pipeline proved itself alive) and 10 were `MEASUREMENT_STALE`.** That is
the distinction this slice was built to make, and it holds: when the model is
working, KUE is not blind — it is *late*, and now it can say so.

### 4. What actually costs the owner their session? **Not lateness. Ambiguity.**

893 samples, 15 minutes. Of every moment KUE was `IDENTITY_UNCERTAIN`:

| Why KUE was unsure | Share |
|---|---|
| **`MEASUREMENT_AMBIGUOUS`** — measured, and the descriptors fell between the accept and reject thresholds | **65.6 %** |
| `MEASUREMENT_DELAYED` — a live pipeline being late | 16.9 % |
| `MEASUREMENT_FRESH` — measured fine, but the claim was not (yet) earned | 15.0 % |
| `MEASUREMENT_STALE` | 2.5 % |
| `MEASUREMENT_CONFLICT` — a reading that said somebody else | **0** |

**Not once in fifteen minutes did a measurement say "this is not the owner".**
Access still changed 115 times (460/hour at that rate). KUE is not losing the
owner because it sees somebody else, and mostly not because it is late: it is
losing the owner because two thirds of its measurements settle nothing.

Capture quality was **0.263 when fresh and 0.249 when ambiguous** — nearly
identical, so quality is not what separates the two. The threshold band itself,
or the descriptors feeding it, is where the next investigation goes.

Faces per frame: 806 single, 75 none, **0 multiple** — no false multi-face
detection in this window (F5 not reproduced).

## Database growth — measured

| | Before (2026-09-19) | After a day with the new tables |
|---|---|---|
| File | 61.1 MB | 61.1 MB (unchanged — the new tables used free pages) |
| Of which reusable free space | 47.8 MB | falling as the new tables fill it |
| events | 8,937 | 9,258 |
| snapshots | 5,000 (capped) | 5,000 (capped) |
| perception samples | — | ~1/s, ~105 bytes each ≈ **9 MB/day**, capped at 172,800 rows (2 days) |
| stage timings | — | after filtering, a few hundred a day; **before** filtering it measured 316,000/day |

The stage-timing filter was added because the instrument showed the instrument
was the problem: the pump's tick and context build run four times a second and
are almost always under a millisecond. Percentiles over all of them are still
computed in memory; only spans ≥ 50 ms and the once-per-request stages are
written down.

`context_snapshots` remains the largest table at 10.3 MB, and while KUE is
paused it is pure churn — see `docs/KUE_PAUSE_CONTRACT.md`.

## Privacy

- Both new kinds are classified (`LOCAL_ONLY`) and pass the firewall before
  reaching the store; an unknown kind is a compile error, as before.
- Span operation ids are stripped to identifier characters **in code**, so a
  question or a path cannot become a span id. Ids seen in the live table:
  `tick`, `frame`, `ask`, `q1`.
- Perception rows were inspected directly: states, levels, ages, durations,
  counts, a track number and a capture-quality figure. No descriptor, no
  distance, no image, no audio, no file name. A test asserts the same.
- The production privacy ledger still shows the firewall denying
  `BODY_JOINT_POSITIONS` to local memory, unchanged.

## What was NOT verified

| Not verified | Why |
|---|---|
| The seated 30-minute protocol | Needs the owner to sit deliberately; the numbers above are ordinary use |
| The reject side (a second person) | Needs a consenting second person |
| The Diagnostics panel on screen | Type-checked and unit-tested; nobody has looked at it running |
| Pause and kill on this build | Pause needs the window; kill needs Touch ID to recover — both the owner's |
| `MODEL_QUEUE` timings | The phase is entered on send; queueing inside the helper is not separately visible |
| `CAMERA_KILLED` state live | Would require engaging the kill switch |
| Two live tests | Open-Chrome and the Tampa flow take over the screen |

## Failures found while doing this

| # | Found | Fixed |
|---|---|---|
| 1 | The live storage test asserted wording removed weeks earlier; `#[ignore]` tests are run by nothing | Repaired and anchored to the registry (`e896194`) |
| 2 | The first instrument filed 18 of 30 samples as `MEASUREMENT_CONFLICT` when they were ambiguous — evidence that would have argued for looser thresholds | `MEASUREMENT_AMBIGUOUS` separated (`888eb03`) |
| 3 | The instrument reported `MEASUREMENT_STALE` for a pipeline that had just finished a 3.2 s analysis | Liveness now counts a recent analysis (`0487730`) |
| 4 | Stage timings would have written 316,000 rows/day of "nothing happened" | Filtered to slow and rare stages |
| 5 | KUE had been paused for 2½ days, writing 8,470 identical snapshots a day and evicting its own observed history | **Documented, not fixed** — `docs/KUE_PAUSE_CONTRACT.md` |

## Unresolved questions

1. What is in the ambiguous band? Two thirds of KUE's uncertainty lives there,
   and nothing yet says whether it is the FeaturePrint descriptor, the geometry
   ratio, the 1.15× accept threshold, or the camera's low capture quality
   (0.25 average, against a 0.20 floor).
2. Is the delivery stall the camera, the system, or contention? Distinguishing
   them needs `powermetrics` while a model runs — the owner's to run.
3. What separates the owner from a stranger? Still unmeasured. Until it is, the
   accept threshold cannot be moved in either direction with evidence.
4. Why did 15 % of uncertain moments have a perfectly fresh measurement? Most
   likely the three-frame confirmation and the carry lapsing; the record does
   not yet name which, and the next slice should make it say so.
