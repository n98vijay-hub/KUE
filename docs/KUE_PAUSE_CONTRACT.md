# The pause contract

**2026-09-20.** Written because KUE was found paused since 2026-09-17 11:26,
still running, having written 5,000 identical context snapshots and not one
event in two and a half days. Nothing about that was a lie — but nothing about
it was useful either, and one part of it quietly destroys history.

**No behaviour was changed by this document.** It states what pause does today,
what it should do, where those differ, what the fix would be and how it would be
proved. The fix is deliberately not implemented in this slice.

## CURRENT BEHAVIOUR — measured, not assumed

| What | What actually happens | Evidence |
|---|---|---|
| Sensing | Stops completely. The `AVCaptureSession` is torn down (inputs and outputs removed, not merely stopped), computer-activity sampling is cancelled, pending enrollment and probe captures are abandoned, the camera light goes out | `main.swift` pause handler; `pause_stops_every_reading_from_the_real_sensing_layer` drives the real Swift binary and passes |
| Readings | A reading that arrives while paused is dropped at ingest; one arriving later than the grace window is counted as a contradiction | `engine.rs::reading_while_paused` |
| Identity | Reset; the context reports `NOT_OBSERVING` and activity `PAUSED` | context snapshots read on 2026-09-19 |
| Microphone | Push-to-talk and the name listener are both stopped | `main.swift`, `wake_step` |
| Model | An answer already being generated is **not** cancelled | `lib.rs` pause path |
| Speech | Stops, and the queue is dropped | `lib.rs` |
| Events | None are produced, because nothing changes state | 0 events between 2026-09-17 11:26 and 2026-09-19 23:38 |
| **Snapshots** | **Continue at the full rate — one every 10 s, 8,470 a day** | 5,000 snapshot rows spanning 2026-09-18 13:05 → 2026-09-19 23:38, every one `NOT_OBSERVING / PAUSED` |
| Resume | Needs no authorization at all (LEVEL_0) | `authz.rs` |
| Persistence of the pause itself | None. Pause is in-memory; a relaunch starts observing again | no pause field in the store |

## INTENDED BEHAVIOUR

The product's own sentence is *"KUE is paused. Nothing is being sensed."*
(`surface.rs`). Read strictly, pause is a promise about **sensing**, and that
promise is kept — verified against the real sensing binary.

What pause should additionally mean, and does not say anywhere today:

1. **Pause is the owner's gesture, not a fault.** Every layer should describe it
   that way. (The new `CAMERA_PAUSED` measurement state does this.)
2. **A paused KUE should not consume the owner's history.** Local memory is a
   bounded window; filling it with "nothing happened" evicts the record of when
   something did.
3. **A paused KUE should be visibly paused after a restart**, and should say for
   how long. Two and a half days passed with no one noticing.

## DISCREPANCY

| # | Discrepancy | Consequence, measured |
|---|---|---|
| D1 | Snapshots are written at full rate while paused | 8,470 rows a day of one repeated state. The snapshot table is capped at 5,000 rows, so **about 14 hours of pause evicts the entire observed history**. The snapshots from the sessions of 09-14 to 09-17 — the ones the identity investigation would want — are already gone, replaced by identical `PAUSED` rows |
| D2 | Pause produces no periodic record of *itself* | A three-day pause and a three-minute pause look the same afterwards: two events, far apart |
| D3 | Pause does not survive a restart | The owner's decision to stop watching is undone by a relaunch, silently |
| D4 | Resume is LEVEL_0 | Anyone at the Mac can turn the camera back on after the owner paused it (also recorded as S5 in the security matrix) |

D1 is the one that destroys information. D2–D4 are honesty and safety gaps,
recorded here so the pause contract is decided once rather than drifting.

## PROPOSED FIX — not implemented

1. **Snapshot on transition, then stop.** Write one snapshot when pause takes
   effect (so the record shows exactly when observing stopped), then write no
   further snapshots until the state changes. Resume writes another.
2. **A heartbeat of pause, not a stream of it.** If a periodic record is wanted,
   one row an hour carrying "still paused, since T" answers every question the
   8,470 rows answer, at 0.4 % of the cost.
3. **Say it after a restart.** If KUE starts and the owner had paused it, the
   window should say so rather than silently starting the camera — which needs
   pause to be persisted, and needs a deliberate decision about whether a
   restart should resume sensing at all.
4. **Decide the authorization level for resume** (unchanged by this slice).

Why not simply stop all persistence while paused: snapshots exist so the window,
the model and any later analysis can reconstruct what KUE understood at a moment
in time. Pause is part of that record. The waste is the *repetition*, not the
recording.

## TEST

To be written with the fix, and to fail before it:

| Test | Asserts |
|---|---|
| `a_paused_kue_writes_one_snapshot_for_the_transition_and_then_stops` | Exactly one snapshot after pause takes effect; none for the next N intervals; one again on resume |
| `a_long_pause_does_not_evict_the_observed_history` | With a snapshot cap of 10, an hour of simulated pause leaves the pre-pause snapshots in place |
| `the_pause_record_says_how_long` | The hourly pause row carries the time pause began |
| `pause_still_stops_every_reading` | The existing real-binary test, unchanged — the fix may not weaken the sensing promise |

Until those exist and pass, the behaviour in the first table is what KUE does.
