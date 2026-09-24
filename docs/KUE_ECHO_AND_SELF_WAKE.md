# KUE hearing itself — echo and self-wake

Phase 4B. Research and design. **Nothing in this document is implemented
unless its row says IMPLEMENTED.**

**IMPLEMENTED** · **PARTIAL** · **PLANNED** · **KNOWN LIMITATION**

---

## The problem, as the code stands

    kue-voice (its own process)          lantern-sense (its own process)
    AVSpeechSynthesizer → default output  AVAudioEngine input tap → gate → recogniser → matcher
            │                                        ▲
            └──────── speakers → air → microphone ───┘

- KUE's voice leaves the Mac through the speakers and comes back through the
  microphone. Nothing between the two knows the other exists.
- The name listener is **not** stopped while KUE speaks. That is deliberate:
  interrupting KUE by name is an intended feature, and the brief rules out
  "ignore everything while speaking" for that reason.
- So KUE can, in principle, hear its own sentence. It wakes only if that
  sentence *begins* with the invocation (the matcher's start-of-utterance rule)
  — which makes self-waking unlikely, not impossible. Anything KUE reads aloud
  that it did not write (a file name, a model's answer) could begin with it,
  and the invocation is configurable.

What already exists:

| Piece | Status |
|---|---|
| A listening session (button, or after a bare invocation) stops KUE's speech first | IMPLEMENTED |
| KUE does not start speaking while the microphone is live (fresh report) | IMPLEMENTED |
| Wake while KUE is speaking is heard and acted on (barge-in by name) | PARTIAL — the wiring exists; never exercised in a room |
| A wake that is KUE's own sentence does nothing (L2, below) | IMPLEMENTED — automated, not live verified |

## Measured without a microphone (2026-09-16)

KUE's sentences, rendered in KUE's own voice (Samantha) and run through the
listener's own code path (`--wake-stream`):

| KUE says | Result |
|---|---|
| "Computer Science notes, PDF document." | **WOKE**, request "science notes pdf document" |
| "Computer activity: I can see which app is in front." | **WOKE**, request "activity i can see which app is in front" |
| "There's one item: Computer Science notes, PDF document." | no wake |
| "Your storage is 90% full. The largest folder is Downloads." | no wake |
| "Computers can't read your mind, but I can tell you what I see." | no wake |
| "Denied. I can't switch off or weaken the kill switch…" | no wake |

By reading: none of KUE's fixed sentences begins with "Computer" ("computer
automation" is read aloud only inside a list, never first in a sentence — now a
test), but narration puts file and app names first in places, and a model's
answer can begin with anything. **So KUE can wake itself** — on the recogniser
path. Whether that audio reaches the
microphone loud enough through this room is NOT RUN.

Before L2, such a wake would have become a request (going to the parser, then
the model) or, for a bare name, opened a listening session — which stops KUE's
speech, so KUE would cut itself off.

## What the platform offers — with sources

### Voice processing I/O (acoustic echo cancellation)

`AVAudioInputNode.setVoiceProcessingEnabled(_:)` (macOS 10.15+). From the SDK
header on this Mac (`AVFAudio.framework/Headers/AVAudioIONode.h`):

> If enabled, the input node does signal processing on the incoming audio
> (taking out any of the audio that is played from the device at a given time
> from the incoming audio).
>
> Voice processing requires both input and output nodes to be in the voice
> processing mode. … can only be enabled or disabled when the engine is in a
> stopped state.

Also available: `isVoiceProcessingBypassed`, `isVoiceProcessingAGCEnabled`,
`isVoiceProcessingInputMuted`, `setMutedSpeechActivityEventListener` (macOS 14+),
and `voiceProcessingOtherAudioDuckingConfiguration` (macOS 14+).

**The open question is what "played from the device" covers.** The echo
canceller needs a reference signal. Practitioner reports describe that reference
as the voice-processing unit's **own output bus** — audio the same engine plays —
and describe AEC "failing silently" when nothing is played through it
([field report, 2026](https://barock.dev/2026/04/22/why-your-ios-voice-agent-still-hears-itself),
iOS). Apple's forum reply on macOS voice processing repeats the both-nodes
requirement and does not say whether other processes' audio is removed
([Apple Developer Forums 733733](https://developer.apple.com/forums/thread/733733)).

KUE's speech is played by **another process**. So whether turning voice
processing on in `lantern-sense` removes KUE's voice is **unknown**, and on the
evidence available the safer assumption is that it does not. This is measured,
not assumed — see *Measurement plan*.

The same report measures an echo tail of roughly 200–500 ms after playback
stops in ordinary rooms (longer over Bluetooth), and treats voice processing as
subtraction that leaves residue, not as a guarantee.

### Rendering speech into the listener's own engine

`AVSpeechSynthesizer.write(_:toBufferCallback:)` renders speech into PCM buffers
instead of the speakers. `kue-voice` already uses it (to measure a voice without
playing it). Played through an `AVAudioPlayerNode` on the **same** engine whose
input has voice processing on, KUE's voice becomes the canceller's reference by
construction — the documented configuration, not the uncertain one.

### Speech detection

The Speech framework on macOS 26 has `SpeechDetector`, a `SpeechModule` with
`DetectionOptions(sensitivityLevel:)` and a `speechDetected` result. It says
whether there is speech. It does not say whose, and does not help with echo on
its own.

### What none of these do

None of these APIs identify a speaker. Echo cancellation removes a known
signal; it does not know who the owner is. Speaker identity is Phase 4C and a
separate problem.

## Design — layers, cheapest and most certain first

### L1. KUE never begins a sentence with its own invocation — PARTIAL

**As built:** a test holds every spoken registry sentence and the "what can you
do?" answer to never begin a sentence with "computer" (the default invocation).
**Not built:** rewording at the speech gate for file names, app names, model
answers or a changed invocation — those rely on L2.

Planned design: deterministic, no audio. Before a sentence is spoken, if it begins with the
configured invocation (by the matcher's own rule), it is reworded ("Your
computer…" rather than "Computer…") or prefixed. Covers KUE's fixed narration
by test over the templates, and model or file-derived text at the speech gate.
It closes the most direct self-wake path and costs nothing.

### L2. A wake that repeats what KUE is saying is KUE — IMPLEMENTED (automated)

The core knows the text of the request in flight (`SpeechController`, held in
memory, never logged). A wake that arrives while KUE is speaking, or within
6 s after, whose words match the sentence is recorded as **heard itself** and
does nothing.

- **Barge-in survives.** The owner saying "Computer, stop" over KUE does not
  match KUE's sentence, so it wakes.
- **No audio is kept.** The comparison is between words in memory, dropped
  6 s after the sentence ends.
- **It fails safe in the right direction.** A false "heard itself" costs the
  owner a repeat; a missed one costs a request that is still authorized like
  any other.

**As built** (`core/src/voice/echo.rs`): the speech controller keeps the words
of each sentence it sends to the voice until 6 s after it ends (measured: a
wake arrives ~1.9 s after its audio ends), in memory only, and hands them to the
engine. A wake whose words — the invocation and what followed — appear in order
for at least 75 % in one of those sentences records "Heard its own voice say its
name. Nothing was done." (no words) and does nothing: no request, no listening
session. Tested with the real speech queue and the rests the harness measured;
the owner's "computer, stop" during a sentence still becomes a request; the
words are gone after the window; a mutant disabling the check is caught.

**Privacy change, stated:** the words of what KUE said used to be dropped the
moment it finished saying them; now they are held up to 6 s longer, in memory.

**Stated cost:** the owner saying *only* the name while KUE is saying a
sentence that contains it is taken for KUE. The name with a request is not.

**Not covered:** another device saying "Computer, …" (a podcast) — never KUE's
sentence, so it wakes, as it always has.

### L3. Echo cancellation where the reference is certain — PLANNED

Speech rendered with `write(_:toBufferCallback:)` and played through the same
voice-processing engine that captures the microphone. This means one process
owns both the microphone and KUE's voice — a real architectural change (today
`kue-voice` is separate so that kill can terminate it). Not to be started until
the measurement below says whether the cheaper configuration already works.

### L4. Ducking, not muting — PLANNED

While KUE speaks, the gate's threshold can rise so that residual echo is less
likely to open a segment, without closing the microphone. A threshold is a
guess until measured in a room; no number is chosen here.

### Rejected

- **Ignore the microphone while KUE speaks.** Rules out interruption, which the
  brief requires.
- **Treat any wake during speech as the owner.** A wake is not identity, and
  this would make KUE's own voice able to start requests.
- **Keep audio to compare against.** Nothing in L1–L4 needs recorded audio.

## Measurement plan — needs the owner, the microphone, and a room

1. **Cross-process cancellation.** `say` a sentence beginning with the
   invocation through the speakers while the listener runs, voice processing off
   then on. Record wakes and recognised words (harness only). Tells us whether
   L3 needs the architectural change.
2. **Self-wake rate as built.** KUE speaks its real narration for a series of
   actions with the listener on. Count wakes.
3. **Barge-in.** The owner says "Computer, stop" while KUE speaks. Count
   heard / not heard.
4. **Echo tail.** Energy above the gate threshold after KUE's speech ends, by
   milliseconds.

## Status

| Claim | Status |
|---|---|
| KUE cannot wake itself | **Not claimed.** L2 stops KUE acting on its own sentence (automated). The acoustic path in a room is NOT RUN. |
| Echo cancellation | NOT IMPLEMENTED |
| Owner's voice distinguished from KUE's | By words only (L2): KUE's own sentence vs anything else. Not acoustic, not speaker identity. |
| Barge-in by name | PARTIAL — wired, not exercised live |
