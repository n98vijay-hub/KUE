# Decision gate — what the evidence says about identity

**2026-09-20 · `main` @ `0762cce` · evidence:
`docs/evidence/KUE_FOUNDATION_OBSERVABILITY_2026-09-20.md`**

The perception-observability slice is complete and has run on this Mac. **The
identity carry fix is not implemented, thresholds are unchanged, and no
authorization semantics moved** — as instructed. This is the report that gate
asks for.

One sentence first, because it changes the plan: **KUE is not losing its owner
to contradiction, and mostly not to lateness. It is losing them to measurements
that settle nothing.**

## The twelve questions

**1. How frequently does Vision actually become delayed?**
In 15 minutes of ordinary use with the owner present: 89 of 893 samples (10 %)
had no current measurement while the pipeline was proven alive; a further 10
(1 %) were stale. Nine moments crossed 2.5 s of measurement age. With the model
deliberately loaded four times, **4 of 4 runs** pushed the age past 3.0 s.
With the model idle, the worst age in 100 s was **268 ms**.

**2. How long are the delays?**
Worst measurement age 3,417 ms. The stall sits within ~400 ms of the 3.0 s
staleness limit in every case — it is exactly the size that trips the demotion.

**3. Do delays correlate with model prefill?**
**Yes, and it is now measured from KUE's own process, not inferred.** Its own
answer recorded `MODEL_PREFILL` 13,893 ms and `MODEL_GENERATION` 3,353 ms; two
samples during that prefill carry ages of 3,091 and 3,159 ms. The separate
load test reproduced it 4 times out of 4. *Caveat:* one in-app answer is one
data point for the in-app case; the load tests used a second process.

**4. Does the sensing pipeline remain alive during model inference?**
**Yes.** 89 DELAYED against 10 STALE. The heartbeat kept arriving, capture kept
running, and analyses kept completing. The stall is in one long Vision call
(3.2 s) or in frame delivery (up to 3.5 s) — not in a dead pipeline.

**5. How often does a fresh *contradictory* identity measurement occur?**
**Zero times in 893 samples.** Not once did a reading say "this is somebody
else". This is the single most important number in the report: the demotions
that cost the owner 460 access changes an hour are not evidence-driven.

**6. How often is uncertainty caused only by a missing or delayed measurement?**
16.9 % delayed + 2.5 % stale = **19.4 %**. The rest: **65.6 % ambiguous**
(measured, between the thresholds) and 15.0 % fresh-but-unconfirmed.

**7. How often do false multiple-person detections occur?**
**Zero** in this window: 806 single-face frames, 75 with none, none with more.
F5 is not reproduced and stays open rather than closed.

**8. What is the safest possible temporal identity policy?**
One that treats *lateness from a proven-live pipeline* as the absence of new
evidence rather than as contrary evidence — and nothing more. Concretely: an
**already-earned** match may survive a bounded delay while (a) the heartbeat is
fresh, (b) capture is running, (c) the track is unchanged, and (d) no contrary
measurement has arrived. It must be bounded by wall-clock, not by frames.

**9. What can safely be carried?**
Only a claim that was earned from measurements, across `MEASUREMENT_DELAYED`,
on the same track, for a bounded time. Nothing may be carried across
`MEASUREMENT_CONFLICT`, `NO_PERSON`, a new track, `CAMERA_UNAVAILABLE`,
`CAMERA_PAUSED`, `CAMERA_KILLED`, or `MEASUREMENT_STALE` — stale means nothing
proves another measurement is coming, which is exactly when assuming is unsafe.
**Ambiguity may never promote**: a measurement that settles nothing is not a
reason to grant, only — at most — a reason not to revoke immediately.

**10. What must immediately revoke authorization?**
Unchanged from today and not up for negotiation: a measured conflict, a second
face, a stranger, a new track, the camera stopping, sensing dying, pause, kill,
and the session's own lapse timers. Fail-closed stays fail-closed.

**11. What should remain fail-closed?**
Everything above, plus: unknown measurement state, unknown capability, unknown
operation, unknown data classification. And the rule this slice was careful to
honour — **observation may never become authorization.** A test runs identical
inputs with and without the new instrument and asserts the decisions are
byte-identical.

**12. What is the next smallest engineering slice?**
Not the carry fix. **The ambiguous band is 3.4× bigger than the delay problem**,
and changing the carry rule would leave two thirds of the failure untouched
while spending the one change budget that identity gets.

The next slice is: **record *why* a measurement was ambiguous** — which
descriptor, which ratio, how far outside the accept band — and nothing else.
That is one field on the sample, no decision changes, and it turns "65.6 %
ambiguous" into a cause. It is also the prerequisite for touching any threshold,
because a threshold moved without knowing which descriptor is responsible is a
guess with the owner's security as the stake.

The carry-across-delay fix (worth ~17 % of the failure, already designed in
question 9, already covered by tests that currently assert today's behaviour)
should land **second**, and only with the seated protocol run before and after.

## What this slice deliberately did not do

- No threshold changed. No carry duration changed. No authorization level
  changed. No second-person handling changed. No face-match logic touched.
- The pause fix is documented and not implemented (`docs/KUE_PAUSE_CONTRACT.md`).
- Nothing was deleted: not a branch, not a worktree, not a row.

## What the owner still has to do

| # | Test | What it settles |
|---|---|---|
| L1 | Sit deliberately for 30 minutes, ask five questions, then run `./scripts/evidence-kue.sh 1800` | Turns 15 minutes of ordinary use into the protocol the roadmap gates on |
| L2 | Run one action of each kind from the window on this build | Eight capability rows are still YELLOW for want of it |
| L3 | Complete a Touch ID prompt for a Trash move from the storage sheet | Never once observed end to end |
| L4 | Kill, relaunch, recover | Never verified on `KUE.app` |
| L6 | The Identity Check with a consenting second person | The reject side has never been measured; until it is, no threshold may move |
| — | `powermetrics` during a model answer | Whether the delivery stall is contention, the camera, or the system |

Until L1 and L6 are done, the next slice can be *built* but its acceptance
criteria cannot be *met*.
