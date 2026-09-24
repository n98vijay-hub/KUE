# KUE voice architecture

Input, invocation, and what a voice is worth. Output (narration, the speech
queue, `kue-voice`) is in `KUE_STATUS.md` row 5.

**IMPLEMENTED** · **PARTIAL** · **PLANNED**

---

## The five states, kept apart

The brief's separation, and the code's:

| State | Means | Where it lives | Status |
|---|---|---|---|
| VOICE_ACTIVITY | Sound energy above a threshold | `Voice.swift` / `Wake.swift` | IMPLEMENTED |
| INVOCATION_HEARD | The name was said, first, in this utterance | `Wake.swift` | PARTIAL (measured below) |
| SPEECH_RECOGNIZED | Words, on this Mac | `SpeechTranscriber` | IMPLEMENTED |
| SPEAKER_VERIFIED | **Who** spoke | — | **NOT IMPLEMENTED** |
| OWNER_PRESENT / AUTHORIZED | The access session | `authz.rs` | IMPLEMENTED |

The fourth does not exist, and nothing in KUE pretends otherwise. A transcript
is not an identity; an invocation is not an identity either. A wake starts a
request, and the request is authorized against the access session exactly as a
typed one is — including being refused while KUE is locked.

## The wake boundary — IMPLEMENTED

`sensing/Sources/LanternSense/Wake.swift`.

    microphone → voice energy → (only then) a recogniser for THIS utterance
               → the pause that ends it → its final results → matcher → ONE DECISION

Properties, in the order they matter:

1. **One decision leaves.** The only thing emitted is whether the invocation was
   heard and, if it was, what followed it in the same breath. There is no code
   path from the wake listener to a transcript of anything else. It is a
   separate file so that this is checkable by reading it.
2. **Silence is never recognised.** Buffers reach the recogniser only while
   there is voice energy, with 0.5 s of pre-roll held back so a first word is
   not clipped. After 1.5 s of quiet the utterance is over: its recogniser is
   finished, the decision is taken over its final results, and the next
   utterance gets a new recogniser. Why it is built this way is measured below
   (*The live listener, measured*).
3. **Nothing is recorded.** Audio is analysed in memory and released. No file,
   no buffer kept, nothing off this Mac.
4. **No presence archive.** The listener reports "still here" every five seconds
   and says nothing about when anyone spoke. An earlier draft emitted a message
   each time voice energy started; that is a record of when people were talking
   in the room, and it was removed before it shipped.
5. **It stops with everything else.** Pause, kill, the sensing layer going away,
   or the owner's setting. The core forgets the listener wherever it forgets the
   microphone, so a window cannot keep claiming an open microphone.
6. **Turning it on is what asks macOS.** The sensing layer reports the
   microphone permission from the moment it starts. Access nobody has asked for
   does not stop the attempt — the attempt puts macOS's own prompt in front of
   the owner. A refusal stops it and is not re-asked on a timer; turning the
   setting on again re-reads the permission. (Until 2026-09-16 the listener
   waited for a grant that only the listener could ask for, so on a Mac where
   KUE's microphone had never been used, the setting did nothing.)
7. **It is visible while it runs.** The always-visible trust strip reads
   *Listening for “computer”*, derived from the live report and the same
   freshness rule the speech gate uses — never from the setting.
8. **A wake ends the session, and the core starts the next one.** The boundary
   stops itself when it wakes — it hands the microphone over and says so once.
   Nothing inside the sensing layer restarts it. The decision to listen again is
   taken in `core/src/voice/wake.rs` (`next_step`) and acted on by the shell
   every half second, which is what makes hands-free work more than once per
   launch. It is a state machine, not a background loop: every start and every
   stop follows from the conditions.

## Choosing the invocation — MEASURED

Spoken by five macOS voices (Samantha, Daniel, Karen, Moira, Tessa) through
`say`, transcribed by the real binary on this Mac, matched by the real matcher.

**What this table measures:** the recogniser and the matcher, on finished
files (`--wake-file` — final results, end of input known). It chose the
phrase. It did **not** measure the live listener, which never knows where its
input ends; that is the next section, and the two gave very different answers.

| Phrase | Heard | False wakes |
|---|---|---|
| **“computer”** | **25 / 25** | **0 / 80** |
| “hello kue” | 14 / 25 | 0 / 60 |
| “KUE” alone | 6 / 15 | 2 / 18 |
| “Hey KUE” | 0 / 3 | — |

**“KUE” cannot be heard reliably, and this is not fixable by tuning.** It is one
syllable, and a homophone of *Q*, *cue* and *queue*. The transcriber returned
“Q”, “Okay”, “Oh”, “Who”, “Quay”, “Colloquy”, “Thank you”, and sometimes nothing
at all. Biasing the recogniser toward the phrase with `AnalysisContext.contextualStrings`
changed **not one transcript** in the corpus. The failures where the phrase
became an ordinary word are the dangerous ones: accepting “okay” or “thank you”
as an invocation would wake KUE constantly.

So the invocation is **configurable**, defaults to the phrase that measured
best, and the owner can pick another. The Voice controls offer the three
measured above.

### The rule that keeps the room out

The invocation must be at the **start** of the utterance. A name in the middle
is someone talking *about* KUE, not *to* it. That one rule is why 80 lines of
ordinary speech — “the queue is long today”, “can you check the queue for me?”,
“my computer is slow”, “computers are expensive” — produced zero false wakes.

Matching is done over the utterance's opening letters rather than token by
token, because the transcriber sometimes runs the words together (“Helloqui,
check my storage”) and sometimes splits them (“Hello, Q.”).

Confidence is a **string-match** confidence: 1.0 when the name was written as
itself, 0.8 when it arrived as one of the sounds it shares with other words. It
is not acoustic and not biometric, and it says nothing about who spoke.

## The live listener, measured — `--wake-stream`

`lantern-sense --wake-stream <clip>` runs a clip through the live listener's own
code — the same gate, the same per-utterance recogniser, the same decision —
with a second of quiet before it, audio delivered at microphone pace, four
quiet seconds after, and **no end of input**, because a room does not end.
It needs no microphone and no permission. Same five voices.

**As it was built, the live listener would not have woken.** 0 of 15
invocations with a request, and 0 of 5 bare invocations. Three separate defects,
each found by measurement:

| Defect | What happened | Fix |
|---|---|---|
| The recogniser never saw an utterance end | The gate drops silence — which is what the recogniser uses to decide an utterance is over — and input never ends, so it returned **nothing** until a second utterance arrived. | The gate reports the pause that ends an utterance, and that utterance's recogniser is finished there. |
| Waking on a provisional result lost the request | Once results flowed, a provisional "Computer" arrived ~50 ms before the final "Computer, check my storage." for 4 voices in 5. The wake handed over the microphone with an empty request. | Provisional results are not requested; only final ones are decided on. It costs no time — both arrived in the same burst. |
| The next utterance lost its first word | On a recogniser that had already finished one utterance, the next began "....", so "My computer is slow today. … Computer, check my storage." missed the invocation for 2 voices in 5. Explicit timestamps made it worse. | One recogniser per utterance. The first utterance a recogniser hears was clean every time. |

Also found and fixed: the pre-roll trimmed against a length it computed once,
so it held anywhere from 0.1 to 0.5 s and could clip a first word. The
`--wake-gate-check` self-check now fails on that code.

**After the fixes, 105 of 105 expected outcomes** (21 clips × 5 voices):

| Clips | Result |
|---|---|
| "Computer, <request>." — five requests | **25 / 25** woke, request intact |
| "My computer is slow today." … 2.5 s … "Computer, check my storage." | **5 / 5** woke on the second, request intact |
| "Computer." … 0.7 s … "Check my storage." | **5 / 5** woke, request intact — the two results are one utterance |
| "Computer." alone | **5 / 5** woke with no request (the listening session then opens) |
| Thirteen ordinary sentences — "my computer is slow", "can you check the queue for me?", "thank you", "okay, sounds good", "check my storage" without the name… | **65 / 65** ignored |

**Latency:** the wake arrives a median **1.85 s** after the speaker stops
(1.74–2.21 s) — 1.5 s of that is the pause that ends the utterance.

**KNOWN LIMITATION — measured, not fixed:** "Computer science is hard." wakes
KUE, 5 / 5, with the request "science is hard". The rule is "the invocation
comes first", and here it does. What follows is still only a request —
parsed, authorized and verified like a typed one — but it is a wake nobody
meant.

**Edge, measured:** speech that begins at the very first sample the listener
receives — no quiet at all before it — lost a bare "Computer." for 2 voices in
5 (a request in the same breath still woke 5 / 5). With 0.2 s of quiet first,
10 / 10. A microphone delivers room sound before anyone speaks, so this bites
only if the name is said the instant the listener starts (`--lead 0`).

**Also by design:** a wake decided in the moment after the listener is stopped
— pause, kill, the button taking the microphone — is dropped, not delivered.

**What this is not:** `say` voices in digital silence. No room, no
reverberation, no background noise, no real microphone, no accents but the
five voices'. It is evidence about the code path, not about the room.

## What happens after a wake — IMPLEMENTED

    heard → (words in the same breath?) → yes: that is the request
                                        → no:  an ordinary listening session opens,
                                               and the next sentence is the request (12 s)
    request → the deterministic command parser → an action, authorized and verified
            → or, if it is not a command → the question path
    then   → the name listener starts again

No model is consulted about whether something is a command. A late sentence
after a bare invocation is not a request: a wake from across the room must not
capture a sentence spoken a minute later to somebody else.

The session that catches the sentence after a bare invocation is the ordinary
listening session — the one the window shows, whose transcripts the owner can
see, and which goes through the same authorization gate as the button. A wake is
not a way around anything, including the parts of KUE that make listening
visible. While it runs, the name listener is stopped: one microphone, one user
of it. If it never opens, or it opens and closes on silence because nobody
followed up, the name listener comes back within three seconds — a bare
invocation cannot leave KUE deaf.

**This step is new, and it is the part of the wake path that has never run.**
Until now the documentation described it as implemented while nothing opened
that session: the engine would wait for a transcript that only a test ever
delivered. The test passed because it injected the transcript directly, which is
a test whose shape did not match the claim above it. The decision to open the
session is now covered by tests; the wiring that turns it into a `listen_start`
has not been exercised, live or automated.

## What is NOT built

- **Speaker identity (PLANNED).** No voice embedding, no speaker match, no voice
  factor in the access session. A future one would be a *factor*, never
  equivalent to Touch ID: Apple's biometric authentication is backed by the
  Secure Enclave, and an application-level voice match is not the same thing and
  must never be described as if it were.
- **Voice enrollment (PLANNED).** Nothing captures voice samples today.
- **An acoustic keyword spotter (PLANNED).** The wake boundary is Apple's
  transcriber plus a string match, which is why the numbers above are what they
  are. A trained spotter would measure differently, and would need its own
  model and its own evidence.
- **Barge-in while KUE is speaking (PARTIAL).** Speech stops when a listening
  session starts, including the one a bare invocation opens. The name listener
  is *not* stopped while KUE talks — deliberately, so the owner can interrupt —
  and the consequence is that KUE's own voice reaches its own microphone. The
  matcher only wakes on an utterance that BEGINS with the invocation, which is
  what makes self-waking unlikely rather than impossible — and the invocation is
  configurable, so an owner could choose a word KUE itself says. This has not
  been exercised in a real room, so "KUE cannot wake itself" is not a claim being
  made here. Echo cancellation is not implemented.

## Verified on this Mac

- **The live listener's code path, without a microphone** — 105 / 105, above,
  run by `--wake-stream`. The shell test
  `the_wake_boundary_hears_its_name_in_speech_made_on_this_mac` runs the same
  harness on two voices, plus `--wake-gate-check` (pre-roll after every buffer,
  no silence reaching the recogniser, the end of an utterance reported once,
  and the decision over an utterance's results).
- The phrase measurement above, over 105 finished-file clips.
- **Turning it on where the microphone was never asked for asks macOS**, and a
  refusal is not re-asked on a timer
  (`turning_the_name_listener_on_puts_the_question_to_macos_on_a_mac_that_was_never_asked`).
  Automated, without a microphone.
- **The listener starts again after every wake**, and the sentence that follows
  a bare invocation is caught once, not asked for every half second
  (`the_name_listener_starts_again_after_every_wake`, and the policy's own tests
  in `core/src/voice/wake.rs`). Automated, without a microphone.
- **NOT verified:** the sentence-after-a-bare-invocation path end to end. The
  core's decision is tested; the shell command it produces has never run.
- **NOT verified:** the live microphone path. The open microphone, a real room,
  the owner's own voice, and KUE's voice reaching its own microphone
  (`KUE_ECHO_AND_SELF_WAKE.md`) have not been exercised on this Mac.
