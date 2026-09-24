# KUE product phase — report

Against the nineteen headings the brief asks for. Limitations are stated, not
hidden; where something is not verified, it says so and says why.

Branch `kue/runtime-safety`, from `4c53c80` to the head of this phase.

---

## 1. What I inspected

The whole repository before changing anything: `src/` (App, eleven rail panels,
the conversation, 278 lines of types, 477 lines of ad-hoc CSS), the 36 IPC
commands in `src-tauri/src/lib.rs`, `core/` (engine, authz, privacy,
transaction, actions, apps, folders, task, router, conversation, voice),
`docs/KUE_STATUS.md`, `docs/KUE_RENAME_PLAN.md`, the README, and the 246 core
tests then passing.

I also ran it. The app was live (pid 70676, release bundle) and I screenshotted
it in its paused state, measured the palette's contrast, and checked the
accessibility tree.

Then the field: Microsoft's 18 HAX guidelines verbatim, Google PAIR on
calibrated trust and error taxonomy, Claude in Chrome's permission model and the
~93% approval-rate finding, ChatGPT agent, Apple's Siri/Apple Intelligence
direction and the macOS privacy indicators, Raycast, Perplexity, Arc, visionOS,
and the Humane/Rabbit post-mortems. Sources are labelled primary or secondary in
`KUE_DESIGN_RESEARCH.md`; three pages returned 403 and are marked secondary
rather than cited as if I had read them.

## 2. What was wrong

Recorded in full in `KUE_PRODUCT_UX_AUDIT.md` (sections A–U). The short form:

**It was an inspector, not a product.** Eleven panels of engineering state down
the right side. Evidence arithmetic ("Why — the full arithmetic") in the primary
column. A raw JSON dump of the context object. `LEVEL_2`, `a3`,
`REQUIRES_CONFIRMATION`, `PRIVACY_DENIED`, head-pose angles and descriptor
distances on the face of the normal experience.

**Two untrue statements were shipping.** The unknowns list said "Anything you
say — there is no microphone capture in this build", and `MIC_UNAVAILABLE`
reported `NOT_IMPLEMENTED` for the same reason. Push-to-talk capture has existed
since the voice phase. Both were false in the shipping build, in exactly the
category the brief forbids.

**A truthfulness regression was one rename away.** The checker that corrects a
model answer claiming a capability KUE lacks held its own phrase table, matched
to capabilities by name. Renaming a capability would have silently switched the
correction off, and no test would have caught it, because the checker's tests
build their own rows.

**A slow action's result was thrown away.** Age was measured from `created_at` —
when an action was *proposed* — so anything that took longer than the twelve-
second outcome window was stale the instant it finished, and its verified result
never appeared.

**Two more couplings keyed off English prose.** The capability answer decided
what was "deliberately excluded" from whether a note began with the word
"Deliberately". The claim phrases `"click on"` and `"type into"` meant a correct
answer — "You can click on Confirm" — would be corrected as wrong.

**Accessibility failed, measurably.** `--ink-3` 3.53:1 and `--ink-4` 2.16:1
against the window background, both below WCAG AA. Zero `:focus-visible` rules
in the entire app. No `prefers-reduced-motion`. One ARIA role. A global
`user-select: none` that made KUE's own error text unselectable.

**No design system.** Eleven font sizes from 9px to 30px, 24 inline styles, one
accent colour doing four different jobs.

## 3. What I changed

Seven commits. In order:

1. The four documents (audit, research, UX architecture, design system).
2. `core/src/capabilities.rs` — the capability registry, owning the claim
   phrases; both false microphone claims fixed; `ConditionStatus::Unknown` added
   so an unchecked permission is reported as unchecked.
3. `core/src/surface.rs` — the interface projection, with its invariants as
   tests.
4. `updated_at` on action records; the two prose couplings removed; one shared
   rule for "the current action".
5. The redesigned window, `src/tokens.css`, the sheets, Diagnostics as a mode.
6. The three defects the first live run exposed (below), and the broker's
   runtime capability gate.

## 4. What was already working

Most of it, and I left it alone. The privacy firewall and its sealed
clearances. The kill switch and its latch. The access session, the 15-second
unmeasured hold and the 1-second face-gap grace. The transaction pipeline with
atomic claim and verified success. The Action Broker's allowlist and target
policy. App resolution against installed apps. Folder opening and listing.
Multi-step requests. The voice output stack — narration, speech gate, firewall,
queue, `kue-voice` — which I did not touch at all. The model router and the
overclaim correction (I moved where its phrases live, not what it does).

## 5. What I did not touch

The Swift sensing layer. `KueAct.app`. `kue-auth`. `kue-voice`. The privacy
policy version (still v1, and nothing here needs v2). The matcher. The eleven
Diagnostics panels — mounted unchanged behind the mode toggle, deliberately:
rewriting working panels to match a new design system is risk without benefit,
and Diagnostics is allowed to look like an instrument.

I also did not do the Lantern → KUE rename. `KUE_RENAME_PLAN.md` is explicit
that it must not be a search-and-replace, and it touches bundle identifiers,
signing and the data directory. The registry now makes the *display* half safe
to do — which was the blocker the plan named — but it is its own piece of work.

## 6. New UX architecture

`docs/KUE_UX_ARCHITECTURE.md`. Seven user-facing concepts (presence, context,
thought, action, result, memory, trust) and the six levels of depth the brief
asks for. One window: presence line, activity line, the stream, a context
column, an always-visible trust strip, five sheets, and Diagnostics as a mode.
Presence, activity, trust, capability, memory, voice and error models each
enumerate their states with the producer of each. The thirty journeys are listed
with their status.

## 7. New UI architecture

The authority boundary the brief requires: the window renders a projection and
sends gestures back. It composes no sentence about KUE, maps no state name to
English, and decides nothing about what KUE is doing. `get_surface` is the only
source of KUE's states; `ActionList.said` carries core's sentence for each
action. Privileged work still goes through the same IPC commands into the same
core paths — the UI gained no authority it did not have.

## 8. New design system

`docs/KUE_DESIGN_SYSTEM.md` and `src/tokens.css`. Every colour measured, not
asserted: `--ink-3` 5.67:1 and `--ink-4` 4.69:1 replace the two that failed;
all four ink values now pass AA. One accent per job (presence, affirm, caution,
alert, absent, focus). Seven type steps, none below 11px. One spacing scale.
`:focus-visible` on everything. `prefers-reduced-motion` reducing every
animation, including the two indeterminate indicators. Text selection restored.
Two indeterminate indicators exist and both mean "a real process is running
whose duration is unknown"; there are no skeletons and no progress bars toward
percentages KUE does not have.

## 9. New components

`Presence`, `TrustStrip`, `ActivityLine`, `Attention` (`src/components/Presence.tsx`);
`Sheet`, `TrustSheet`, `CapabilitiesSheet`, `MemorySheet` (`src/components/Sheets.tsx`);
`Diagnostics` (the existing panels, moved intact). The action card became a
moment: one sentence, a decision row when one is needed, and a "How did this go?"
disclosure holding the record. The voice controls moved into a disclosure of
their own.

## 10. New state model

`core/src/surface.rs`. `SystemState`, `Presence` (11), `Activity` (14), `Signal`
and `Trust` (6 signals), `ContextLine`, `Attention`. Every state has a real
producer. `VERIFYING` is deliberately absent: verification happens inside the
executor in the same step as execution, so nothing could produce it, and a phase
with no producer is a progress indicator for work that is not happening. It
joins the enum when a primitive verifies separately.

## 11. Capability registry status

**REAL.** `core/src/capabilities.rs`, 32 capabilities. Each carries id, internal
name, the name a person reads, the sheet sentence, the spoken sentence, limits,
status, risk, authorization, the data kinds it touches, the macOS permissions it
needs, how success is verified, whether it is excluded on purpose, the action
tags it covers, and the phrases that amount to claiming it.

Wired, today, to: the capability panel and the model's context (`rows()`), the
spoken capability answer, the overclaim checker (`claim_phrases`), the
Capabilities sheet (`get_capability_sheet`, with availability from live facts),
and the Action Broker, which refuses at runtime any action not covered by an
implemented capability.

Availability is conservative: an unchecked permission reports unavailable, never
available; a denied one names the Settings pane only the owner can use.
Accessibility control reports as not granted, because KUE neither requests nor
uses it.

## 12. Computer-control status

Unchanged, and still **NOT_IMPLEMENTED** for in-app control. `ENTER_TEXT`,
`PRESS_KEY`, `CLICK_TARGET`, `SCROLL`, `NAVIGATE`, `SUBMIT`,
`READ_VISIBLE_RESULT`, `WAIT_FOR_STATE` and `VERIFY_APPLICATION_STATE` do not
exist. What does exist — opening, quitting and switching apps, links, folders,
listings, documents by name, files inside `~/KUE`, notifications — is unchanged
and still verified after every execution.

I did not start the Accessibility work in this phase, for two reasons worth
stating plainly: it needs a TCC grant only the owner can give, in a dialog I
must not touch, and ad-hoc signing resets that grant on every rebuild. It should
be a session with the owner present. The registry now carries the honest
placeholder — "I can't type into other apps yet" — with the permission named.

## 13. Voice status

Unchanged; I touched none of the pipeline. Output is **PARTIAL** as before
(narration → gate → firewall → queue → `kue-voice`, female default, no external
or trained voice). Input is **PARTIAL**: push-to-talk, audio never stored, the
live microphone path still unverified on this Mac.

Two things did change around it. The surface will not report listening unless
`microphone_busy` — the same judgement the speech gate uses — says the report is
live, so the window cannot claim to be listening when it is not. And the voice
controls are behind a disclosure rather than a row of selects in the
conversation.

The better-TTS work in the brief is **not done**. The research records what is
available (all installed English voices on this Mac are DEFAULT quality;
Samantha is the only non-novelty en-US female; Enhanced/Premium voices must be
downloaded by the owner, which I must not do for them).

## 14. Privacy status

Policy still v1, unchanged, and nothing in this phase needed a change. No new
data kind, no new destination, no new clearance path. The surface is built from
records the core already cleared for the window; a withheld list contributes
nothing to it, exactly as it shows nothing.

The trust strip states policy v1's actual position — nothing may reach a model
outside this Mac — as a fact, not a setting, because that is what it is.

Unchanged limitations: no encryption at rest, no deletion by day/kind/entity
(erase-all only, which the Memory sheet says rather than offering a control that
does not exist), and the diagnostic log path is still not routed through the
firewall.

## 15. Security status

No security boundary moved. The window gained no authority: every privileged
operation still goes through the same commands into the same core gates.
Authorization still comes from the access session, never from the interface, a
record, or a model. Kill still dominates — and the projection enforces it in
what the window may show, with a test that no signal reads "on" while KUE is
stopped.

One boundary got stronger: the Action Broker now refuses any action not covered
by an implemented capability, so a kind added to the parser without a capability
cannot quietly become something KUE does but never says it does.

## 16. Tests

274 core tests pass, plus the shell and e2e binaries (11 test binaries, all
green). New this phase: 17 surface tests, 12 capability tests, 3 conversation
tests, 2 engine tests, 1 transaction test.

Mutation-checked, each confirmed to fail when the guard is removed:

| Mutation | Test that caught it |
|---|---|
| Believe `SUCCEEDED` without verification | `success_is_never_shown_without_a_verified_record` |
| Read the microphone state string instead of the live report | `listening_is_never_shown_unless_the_microphone_is_actually_live` |
| Let sensors keep reporting while KUE is stopped | `stopped_overrides_everything_and_no_sensor_reads_on` |
| Measure an action's age from `created_at` (the shipped bug) | `a_slow_action_still_gets_to_report_what_it_did` |
| Delete a capability's claim phrases | `the_checker_fires_against_the_capabilities_kue_actually_ships` |
| Rename a capability (must *not* break anything) | same test — still passes, which is the point |

**UI tests: 13** (`npm test`, vitest, static rendering — the components under
test are prop-driven and have no effects, so no DOM is needed). They cover the
half that is not core's: the verified mark appears only when core recorded a
verification; the activity line says only what core put in the sentence and
shows nothing when KUE is doing nothing; every trust signal carries core's word
rather than a colour alone; a stale projection stops the window claiming
anything about the sensors; the Settings control comes from the permission
field, not from the wording; and the kill state cannot be hidden by a projection
that stopped arriving. Three mutation checks:

| Mutation | Test that caught it |
|---|---|
| Show the verified mark regardless of what core recorded | `shows the verified mark only when core recorded a verification` |
| Keep showing the last trust signals when the projection is stale | `stops claiming anything about the sensors once the projection is stale` |
| Derive the kill state from the projection alone (the shipped bug) | 3 of the 4 kill tests |

**Gap that remains:** nothing renders `App` itself, because it reaches for IPC
on mount, so the composition — which sheet opens, what Diagnostics mounts, the
killed surface — is verified by type-checking and by eye, not by test.

## 17. Live verification

**LIVE_VERIFIED on this Mac.** The redesigned window built, launched from the
bundled app, and rendered from real runtime state: presence "Locked." with
"Show your face, or use Touch ID", the trust strip reading *Camera on ·
Microphone not yet checked · Sees which app is in front · Nothing leaves this
Mac · Keeping what it works out*, the context column, the Diagnostics control,
and the conversation gated behind identity. Running it is how three defects were
found that reading the code did not show:

1. The trust strip wrapped out of the 46px title bar at 1200px.
2. The context column was showing "Head pose — yaw -20°, pitch 9°, roll 6°;
   capture quality 0.19" and "1 face(s) detected".
3. The conversation told a person who wanted to ask a question that it "needs
   LEVEL_2 — you, confirmed by the camera — or Touch ID. Current: Locked."

**NOT VERIFIED.** The build containing the fixes for those three has not been
seen running. It compiles, passes every test, and is installed and running right
now — but the window is on a different desktop Space from the one in use, and a
window cannot be moved into a full-screen Space, so I could not capture it.
I am not going to describe a screen I have not seen. The fixes are covered by
tests (`context_is_sentences_a_person_reads_never_measurements` rejects any
context line containing a measurement); the *visual* result is unconfirmed.

**Also not verified live:** every sheet (Trust, Capabilities, Memory, Context),
Diagnostics mode, the killed surface in the new shell, keyboard navigation, and
`prefers-reduced-motion`. These are new code paths that have been type-checked
and built but not exercised by a person or by me.

**One measured observation, cause unknown.** From 15:47 the camera reported
`STARTING` and did not reach `RUNNING`. I guessed re-signing had reset the
bundle's camera grant, which was wrong, and the checks that disproved it are
worth recording:

* The *previous* release bundle — binary untouched, grant working an hour
  earlier — showed the same `STARTING`.
* There was no stale sensing process holding the device: exactly one
  `lantern-sense`, belonging to the app that was running.
* It resolved on its own. Measured transitions: 15:41:06 → 15:41:13 (7s, a
  normal start), 15:47:48 → never (that instance was stopped before it
  resolved), 15:56:57 → 15:59:44 (**167s**).

So camera start was delayed by minutes twice and then succeeded. I do not know
why. I considered "another application was holding the camera", but macOS lets
several clients hold a camera at once, so that explanation does not carry the
weight I first gave it, and I am not asserting it.

What this did show is that KUE behaves correctly while its camera has not
started: `camera_state != RUNNING` makes the presence line read "I'm not
watching — the camera is off" and the trust strip read "Camera not running",
rather than claiming to watch or going silent. The user's app is sensing
normally as of the end of this phase (camera `RUNNING`, identity confirmed).

**State of this Mac when I finished.** The app running is the *previous* release
build, which I restarted after stopping the new one — so what is on screen is
pre-phase code, including the two false microphone claims this phase fixed. The
new build is committed and tested but not installed at the path the owner
launches. Installing it is `./scripts/build-app.sh`, and macOS will very likely
ask for camera access again afterwards, because re-signing changes the code
identity TCC keys on. That prompt is the owner's to answer; I neither can nor
should.

## 18. Remaining limitations

- The last increment has not been seen running (§17).
- UI tests cover the presentational components and the kill derivation, not the
  composition in `App` (§16).
- Journeys 1–3 (first launch, permission onboarding, guided enrollment) are not
  built. Permission and enrollment prompts exist as attention items; a first-run
  welcome does not.
- In-app computer control does not exist (§12).
- Voice quality is unimproved; the live microphone path is still unverified.
- Memory can only be erased entirely.
- The Voice sheet is a disclosure, not a sheet.
- The window is still called Lantern, in the title bar, the bundle id, the data
  directory and much of the prose. The rename is planned, not done.
- The Diagnostics panels still say "Lantern" and still show raw engineering
  state — by design, but it means the rename touches them too.
- `Availability::Degraded` is produced but nothing in the UI distinguishes it
  from available yet.

## 19. Recommended next phase

In this order:

1. **See the fixed build running**, and walk every sheet, Diagnostics, the
   killed surface and keyboard navigation. Ten minutes of looking will find
   things tests cannot, as it already did once this phase.
2. **The rename**, following `KUE_RENAME_PLAN.md` by risk class, now that the
   claim phrases can no longer be broken by it. Display strings first, as their
   own commit; the data directory and bundle identifiers deliberately last.
3. **First run and permission onboarding** — journeys 1–3. This is the largest
   remaining hole in the experience, and it is entirely honest work: the states
   already exist in the projection.
4. **Extend the UI tests to `App` itself**, with the IPC boundary stubbed, so
   the composition is covered and not only the pieces.
5. **Accessibility control**, in a session with the owner present, because the
   TCC grant is theirs to give and ad-hoc signing resets it. Semantic targets
   first, coordinates last and bounded, verification mandatory — the research
   records why.
6. **Voice quality**, starting with a measurement rather than a swap.
