# KUE speaker identity — research and design

Phase 4 of the 2026-09-16 brief. **Nothing here is implemented.** Two things
stand in front of it, and both are the owner's:

1. **The live microphone path has not been verified** (Phase 1). The brief puts
   that first, and a speaker check built on an unverified capture path would be
   measured on nothing.
2. **macOS has no speaker-identity API** (below). Building one needs a model
   KUE does not have, and bringing one in is a decision about a download and a
   licence — not something to do quietly.

**IMPLEMENTED** · **PLANNED** · **DECISION NEEDED**

---

## The separation, restated

| Question | Answered by | Status |
|---|---|---|
| Was there a voice? | Energy gate (`WakeGate`) | IMPLEMENTED |
| Was the name said first? | `Wake.swift` + matcher | IMPLEMENTED (not live verified) |
| What was said? | `SpeechTranscriber`, on this Mac | IMPLEMENTED |
| **Who said it?** | — | **NOT IMPLEMENTED** |
| May it be done? | `authz.rs`, from the face identity session and macOS authentication | IMPLEMENTED |

A transcript identifies nobody, and nothing in KUE treats one as if it did.

## What this Mac offers — checked against the SDK on this Mac

Searched the macOS SDK (`xcrun --show-sdk-path`) for speaker, diarization,
voiceprint and embedding APIs in Speech, SoundAnalysis, AVFAudio, AVFoundation,
CoreML and CreateML:

- **Speech** — transcription only. No speaker attribution, no diarization.
- **SoundAnalysis** — `SNClassifySoundRequest` classifies ~300 kinds of sound
  ("speech" among them). It says there is a voice, not whose. `SNFeaturePrint`
  exists only as a private symbol in `SoundAnalysis.tbd`; it is not in the
  public interface and cannot be relied on.
- **CreateML** — `audioFeaturePrint` exists to train sound *classifiers*. An
  owner-versus-everyone-else classifier needs recordings of everyone else, and
  a general audio embedding is not built to separate speakers.
- **AVFAudio / AVFoundation** — "speaker" means loudspeaker.
- **LocalAuthentication** — Touch ID and passwords. There is no voice factor,
  and nothing KUE builds becomes one.

**Conclusion: there is no Apple API that says who is speaking.** Face identity
in KUE uses Vision's general image feature print plus geometry, measured for
separation on this Mac; there is no audio equivalent in the public SDK.

## Options

### A. A pretrained speaker-embedding model, run locally — RECOMMENDED, DECISION NEEDED

A network that maps an utterance to a fixed-length vector; same speaker →
nearby vectors. Enrollment stores a few vectors; a check compares one.

- **Candidate:** WeSpeaker ECAPA-TDNN. Toolkit Apache-2.0
  ([wenet-e2e/wespeaker](https://github.com/wenet-e2e/wespeaker)); the
  published checkpoint is **CC-BY-4.0**, trained on VoxCeleb2 dev (5,994
  speakers), 192-dimension embedding, reported EER 0.78 % on VoxCeleb1-O with
  large-margin fine-tuning and AS-Norm
  ([model card](https://huggingface.co/Wespeaker/wespeaker-ecapa-tdnn512-LM)).
- **What it needs:** downloading third-party weights (tens of MB); converting
  to Core ML (or running ONNX) inside the sensing process; a fixed front end
  (80-dim log-mel filterbanks, 16 kHz); a calibration set to choose accept /
  reject thresholds; and cohort vectors for score normalisation.
- **Licence caution:** the weights' licence is CC-BY-4.0, but VoxCeleb is
  audio of public figures collected from YouTube, and the dataset's own terms
  should be read before this ships in a product. Not settled here.
- **What its numbers are not:** VoxCeleb EER is celebrities on YouTube, not
  this room, this microphone, the owner's voice or replayed recordings. KUE
  would measure its own separation on this Mac, the way face identity did.

### B. Classical features (MFCC statistics, GMM) — possible without a download, NOT RECOMMENDED

Runs with Accelerate alone. Without a background model trained on many speakers
it separates voices poorly, and it would be weakest exactly where it matters —
similar voices, a cold, a different microphone. Measuring it on `say` voices
would be flattering and meaningless: synthetic voices differ far more than
people do.

### C. A cloud speaker-recognition service — REJECTED

Raw audio leaving the Mac. The brief's privacy rules and KUE's firewall rule it
out by default.

## Design, when A is chosen — PLANNED

Mirrors face enrollment (`sensing/Sources/LanternSense/Enrollment.swift`), which
keeps derived descriptors in the sensing layer and never sends images anywhere.

**Where it runs.** Inside `lantern-sense`, on the audio of the utterance that
was just recognised — the same buffers the wake boundary already holds for one
utterance. Two paths need it: the utterance that carried the invocation, and
the listening session that catches a request after a bare invocation (a
different person may say the request than said the name).

**What leaves the sensing layer.** A verdict, never a vector, never audio:

    VOICE_MATCHED | VOICE_UNKNOWN | VOICE_UNCERTAIN | VOICE_UNMEASURABLE
    + score (internal), model id + version, enrollment version,
      speech seconds measured, quality reason

`VOICE_UNMEASURABLE` covers too little speech (under ~1.5 s is unreliable for
these models), clipping, too much noise, and KUE's own voice playing
(`KUE_ECHO_AND_SELF_WAKE.md`).

**Enrollment.** Several utterances (at least five, a few seconds each, prompted
sentences), read by the owner. Requires the same authorization face enrollment
does — LEVEL_3 by Touch ID, owner gesture in KUE's window — and re-enrollment
and reset likewise (reset at LEVEL_4). Stored: the embedding vectors, model id
and version, creation time, sample count. **Not stored: audio, transcripts.**
An enrollment made by a different model version is discarded, not compared.

**Fusion — into the existing identity session, not a second one.** `authz.rs`
already receives an `IdentityObservation` (identity + basis). Voice becomes a
second factor in that observation:

| Face | Voice | Fused | Access |
|---|---|---|---|
| owner (measured) | matched | OWNER_CONFIRMED | as face today |
| owner (measured) | unknown | IDENTITY_UNCERTAIN (conflict) | owner operations denied |
| owner (measured) | uncertain / unmeasurable / none | as face today | as face today — voice adds nothing |
| unknown | matched | IDENTITY_UNCERTAIN (conflict) | denied |
| unknown / none | unknown | UNKNOWN_PERSON | denied |
| no face | matched | AUTHORIZED_USER_LOW_CONFIDENCE | **no more than LEVEL_1** — voice alone never reaches personal data or actions |
| two people | anything | MULTIPLE_PEOPLE | denied |
| stale (older than a few seconds) | — | ignored, not trusted | — |

Rules that do not bend:

- A voice match never raises access above what the face session allows, and
  never replaces Touch ID for LEVEL_3–4.
- A conflict is never resolved by picking the stronger-looking factor.
- The verdict is about the utterance it came from; it expires quickly.
- A voice that matches during KUE's own speech is unmeasurable, not a match.
- Replay (a recording of the owner) is **not** defended against by an
  embedding. That is a known limitation, stated in the window's language:
  "KUE recognises your voice; that is not the same as proving it's you."

## Decision needed from the owner

1. Verify the live microphone (Phase 1) — nothing above is measurable before it.
2. Approve (or not) downloading a speaker-embedding model, after reading its
   licence and VoxCeleb's terms. Named candidate: WeSpeaker ECAPA-TDNN,
   CC-BY-4.0.
3. Accept the stated ceiling: voice is a factor that can deny or corroborate,
   never one that grants more than presence on its own.
