# KUE speaker model — decision record

Written 2026-09-16 on branch `kue/speaker-model-decision`, from public web
pages read on that date. **Speaker identity is NOT_IMPLEMENTED in KUE. No model,
weights, dataset, package or binary has been downloaded.** Nothing here has been
run, converted, measured or seen working on this Mac; nothing in this document
is LIVE_VERIFIED or TEST_VERIFIED_ONLY.

This record exists so the owner can approve or refuse one download: a
speaker-embedding model that would run on this Mac, inside the sensing layer
(`lantern-sense`), under the design in
[KUE_SPEAKER_IDENTITY.md](KUE_SPEAKER_IDENTITY.md). It is not legal advice. The
licence questions below are recorded as found, including where the sources
contradict each other; they are not settled here.

**How facts are marked.** Every fact names its source page. A fact marked
**UNVERIFIED** could not be confirmed from a primary page (the page did not say
it, the page could not be read, or two sources disagree). Quotes are kept
under 15 words and attributed; everything else is summary. All pages were read
through a fetch tool that summarises; licence names for the Hugging Face models
are corroborated by the Hugging Face API, but exact wording — above all the
VoxCeleb `license.txt` wording in section 3.1 — rests on a single summarised
read and must be re-read at the primary page before anyone relies on it.

---

## 1. The decision in one paragraph

The best-supported candidate is **WeSpeaker ResNet34-LM** (ONNX file, 26.5 MB,
weights labelled CC-BY-4.0, trained on VoxCeleb2 dev only, not gated, no key,
no network at runtime). The weights licence is not the hard part. The hard part
is that **VoxCeleb's own pages state its terms inconsistently** (section 3), so
the CC-BY-4.0 label on the weights rests on a dataset licence that is not
cleanly stated. That risk is small and acceptable for the owner's personal use
on this one Mac; it is unresolved for bundling the weights inside an app given
to other people. Independently of all of this, **nothing can be measured until
the live microphone path (Phase 1) is verified**, so the download should wait
for that, and voice stays a factor that can deny or corroborate, never one that
grants.

## 2. Constraints this decision must respect

From [KUE_SPEAKER_IDENTITY.md](KUE_SPEAKER_IDENTITY.md) and
[KUE_MASTER_STATUS.md](KUE_MASTER_STATUS.md) sections 8–10:

- **Verdict-only output.** The sensing layer emits `VOICE_MATCHED`,
  `VOICE_UNKNOWN`, `VOICE_UNCERTAIN` or `VOICE_UNMEASURABLE` with metadata —
  never a vector, never audio.
- **Voice never grants.** A voice match never raises access above what the face
  session allows, never replaces Touch ID for LEVEL_3–4, and voice alone with no
  face reaches no more than LEVEL_1. A conflict is never resolved by picking the
  stronger-looking factor.
- **No network, no cloud.** The privacy policy has no CLOUD_ALLOWED data and the
  model router refuses EXTERNAL. Raw audio leaving the Mac is rejected by
  policy. A licence server or usage-metering call at runtime is also a network
  dependency the policy does not allow.
- **Nothing stored but derived descriptors.** Enrollment keeps embedding vectors,
  model id and version; not audio, not transcripts.
- **Phase 1 first.** The live microphone path is PARTLY_LIVE_VERIFIED
  (push-to-talk) and TEST_VERIFIED_ONLY (hands-free). A speaker check built on
  an unverified capture path would be measured on nothing.
- **Voice gives no level today** (master status §9). Speaker identity is
  NOT_IMPLEMENTED (§10).

### Two questions with different exposure

The owner is really facing two decisions, and they should not be merged:

| | (a) Personal use | (b) Distribution |
|---|---|---|
| What | Download the weights and run them on this Mac, for the owner's own voice | Bundle the weights inside a signed KUE.app given to other people |
| Licence exposure | Low: no redistribution happens | Attribution duties apply; dataset-terms conflict (section 3) becomes material |
| Decided here? | Yes — this record asks for (a) | **No** — deferred; needs the conflict in section 3 resolved by someone qualified |

---

## 3. The training data and its terms — recorded as a conflict

Every serious candidate except Picovoice Eagle (undisclosed data) was trained
on VoxCeleb; TitaNet adds LDC corpora. This section is the most
decision-critical part of the record.

### 3.1 VoxCeleb: four sources, four different statements

| Source | What it says | Scope |
|---|---|---|
| `files/license.txt` — https://www.robots.ox.ac.uk/~vgg/data/voxceleb/files/license.txt | "The data is covered under a Creative Commons Attribution 4.0 International license" (VGG). The words "commercial" and "research" **do not appear** in the file. Copyright of original and cropped videos "remains with the original owners". Downloading implies following "the same conditions" for redistribution. Provided "AS IS". | "The data" — not itemised |
| VoxCeleb1 page — https://www.robots.ox.ac.uk/~vgg/data/voxceleb/vox1.html | Metadata licensed under **CC BY-SA 4.0** (Attribution-**ShareAlike**). URLs, timestamps, audio and identifying metadata are "no longer available from this website". | **Metadata only** |
| VoxCeleb2 page — https://www.robots.ox.ac.uk/~vgg/data/voxceleb/vox2.html | Metadata licensed under **CC BY-SA 4.0**. Audio, video, URLs/timestamps and identifying metadata "no longer available from this website". | **Metadata only** |
| KAIST mirror — https://mm.kaist.ac.kr/datasets/voxceleb/ | "available to download for research purposes" under CC BY 4.0; copyright "remains with the original owners of the video". | Download, research framing |

The main page (https://www.robots.ox.ac.uk/~vgg/data/voxceleb/) says the audio
can be requested by filling a form; the form's own terms were not visible and
are **UNVERIFIED**.

**Privacy notice** (https://www.robots.ox.ac.uk/~vgg/terms/url-lists-privacy-notice.html):

- Controller: the Visual Geometry Group, Department of Engineering Science, at
  the University (of Oxford).
- Lawful basis: the UK GDPR Article 14(5)(b) exemption for scientific research,
  where notifying each person would involve disproportionate effort.
- Purpose: framed as research; the notice does not mention commercial use.
- Data subjects: may contact VGG to ask for removal if the URLs infringe their
  privacy.
- The notice says **nothing** about obligations on people who downloaded the
  data, and nothing about models trained on it. Whether a removal request
  would reach a trained model is **UNVERIFIED**.
- No retention period is stated.

Dataset content, per `license.txt`: VoxCeleb1 is over 100,000 utterances from
1,251 celebrities and VoxCeleb2 over a million utterances from 6,112
celebrities, extracted from YouTube videos; speakers span many ethnicities,
accents, professions and ages.

**What model publishers infer from this:**

- WeSpeaker (https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md):
  a pretrained model "follows the license of it's corresponding dataset", and
  for VoxCeleb that is CC BY 4.0.
- SpeechBrain and 3D-Speaker label their VoxCeleb-trained weights **Apache-2.0**
  instead — same data, a different licence label.
- Praat (https://praat.org/manual/VoxCeleb_CC-BY-4_0_license.html) ships the
  WeSpeaker ResNet34-LM model with a CC-BY-4.0 attribution page. This shows how
  another desktop application handles it; it is not evidence that the handling
  is legally sound.

**What this means, stated carefully:**

1. The CC-BY-4.0 label on the WeSpeaker weights derives from a dataset licence
   that the dataset's current pages state differently: BY vs BY-SA, and "the
   data" vs "metadata" only. Which statement governs is **UNVERIFIED**.
2. If BY-SA applied and model weights counted as an adaptation of the data, a
   share-alike duty could reach the weights. Whether weights are an adaptation
   of a dataset is an open legal question — **UNVERIFIED**.
3. None of the VoxCeleb licences can grant rights the publisher does not hold:
   the audio's copyright stays with the YouTube uploaders. Whether training on
   it, and shipping the result, needs their permission is **UNVERIFIED** and
   not settled here.
4. The "research purposes" framing appears on the mirror and in the privacy
   notice but not in `license.txt`. Whether commercial use is permitted is
   **UNVERIFIED**.
5. For decision (a), personal use on this Mac with no redistribution, these
   questions carry little practical exposure. For decision (b) they are
   material and unresolved.

### 3.2 LDC corpora (TitaNet only)

NVIDIA's TitaNet-Large card lists Fisher, Switchboard and SRE (2004–2010) among
its training data; these are Linguistic Data Consortium corpora. The LDC
agreement PDF (https://catalog.ldc.upenn.edu/license/ldc-non-members-agreement.pdf)
could not be read (the fetch returned image data; the LDC site timed out). A
search-engine summary of that agreement states that non-member use is limited
to non-commercial education, research and technology development, and that a
commercial product requires For-Profit membership before release. **UNVERIFIED**
from the PDF itself. Whether NVIDIA held a For-Profit licence, and whether those
terms reach users of a trained model, is **UNVERIFIED**. This asymmetry — not
accuracy — is the reason to prefer the VoxCeleb-only models.

### 3.3 Other datasets, for completeness

- **CN-Celeb** (https://www.openslr.org/82/): Attribution-ShareAlike 4.0
  International. Relevant only to WeSpeaker's CN-Celeb (Mandarin) models, which
  are not candidates.
- **3D-Speaker dataset** (https://3dspeaker.github.io/): metadata "available to
  download" under CC BY-SA 4.0, while the page footer says "All rights
  reserved". Relevant only to models trained on it; not candidates.
- **3D-Speaker ~200k-speaker Mandarin set**: the ERes2NetV2 and CAM++
  `zh-cn-16k-common` model cards describe it as about 200k speakers; no public
  source or terms were found — **UNVERIFIED** provenance. Not candidates.
- **LibriSpeech** (https://www.openslr.org/12/): CC BY 4.0. **MUSAN**
  (https://www.openslr.org/17/): CC BY 4.0. **RIR and noise database**
  (https://www.openslr.org/28/): Apache 2.0. These appear as augmentation or
  extra training data in some recipes.

---

## 4. Candidates

### 4.1 WeSpeaker ResNet34-LM — RECOMMENDED (for personal use, after Phase 1)

1. **Source.** https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM,
   published by the WeSpeaker project's Hugging Face organisation. Revision
   `f0c48c298fd835726c27956a5d617bad7115627e`, last modified 2024-05-06
   (Hugging Face API). Files: `voxceleb_resnet34_LM.onnx` 26.5 MB, `avg_model`
   45.1 MB (PyTorch checkpoint), `config.yaml` 1.67 kB, `README.md` 3.15 kB.
   WeSpeaker's `pretrained.md` also links a wenet.org.cn zip and a Hugging Face
   path spelled `Wespeaker/wespeaker-resnet34-LM`; whether that path redirects to
   the same repository is **UNVERIFIED**. A third-party ONNX export of the same
   model exists in the sherpa-onnx `speaker-recongition-models` release
   (`wespeaker_en_voxceleb_resnet34_LM.onnx`, 26,530,550 bytes); it is not the
   publisher's file.
2. **Licence.** Code: Apache-2.0 (https://github.com/wenet-e2e/wespeaker).
   Weights: **CC-BY-4.0** (model card tag; confirmed by the Hugging Face API),
   on the basis that the model follows its dataset's licence — see section 3.1.
3. **Training data.** VoxCeleb2 dev, 5,994 speakers (model card). `config.yaml`
   has `num_class: 17982` (3 × 5,994) while this fine-tune stage's config has
   `speed_perturb: false`; that is consistent with the class count carrying over
   from a base training run that used speed perturbation — inference,
   **UNVERIFIED**. Terms: section 3.1.
4. **Redistribution.** Not gated (API: `gated: false`). No key, no click-through,
   no activation. CC BY 4.0 permits copying and redistribution, including
   commercially, with attribution: credit, a link to the licence, and an
   indication of changes (converting the file to Core ML would be a change to
   indicate). Whether bundling inside a signed, notarised app is compatible with
   CC BY 4.0 — for example, whether app packaging counts as a restricting
   "technological measure" under the licence — is **UNVERIFIED** and a question
   for decision (b). The dataset conflict in section 3.1 also governs (b) and is
   unresolved.
5. **Size and format.** 6.63M parameters (model card); 256-dimension embedding;
   pooling TSTP. ONNX 26.5 MB (consistent with float32 weights) and PyTorch
   45.1 MB. No Core ML file from the publisher. Conversion paths: section 5.
6. **Inference.** 16 kHz mono (WeSpeaker CLI default resample rate 16,000).
   80-dimension Kaldi-style log-mel filterbank, 25 ms frames, 10 ms shift
   (`config.yaml`). The WeSpeaker CLI subtracts the per-utterance feature mean
   over time when CMN is on. The exported ONNX graph takes input `feats` shaped
   [batch, frames, 80] with a dynamic frame axis and outputs `embs`; it also
   subtracts a fixed mean vector from the *embedding* inside the graph
   (`wespeaker/bin/export_onnx.py`, opset 14). These are two different
   normalisations and both must be reproduced correctly. `num_frms: 600` in the
   config is a **training crop length** (6 s), not a minimum input. No minimum
   speech duration is published; the ~1.5 s floor in KUE_SPEAKER_IDENTITY.md is
   an assumption, **UNVERIFIED**. Compute and latency on Apple silicon: not
   measured, **UNVERIFIED**. The model file itself makes no network call.
7. **Privacy.** Runs fully offline once the file is on disk; no licence server,
   no telemetry in the model. (The chosen runtime's behaviour is a separate
   check — section 5.) What would be stored: a few 256-float enrollment vectors,
   model id and revision, enrollment version, creation time. Those vectors are
   biometric-derived: if copied off the Mac they could be used to test whether a
   recording is the owner's voice, so they belong in the sensing layer beside
   the face descriptors, never in memory, context or logs.
8. **Reported accuracy.** EER (%) on VoxCeleb1 cleaned trial lists, from the
   model card:

   | Large-margin fine-tune | AS-Norm | Vox1-O | Vox1-E | Vox1-H |
   |---|---|---|---|---|
   | no | no | 0.867 | 1.049 | 1.959 |
   | no | yes | 0.787 | 0.964 | 1.726 |
   | **yes** | **no** | **0.797** | **0.937** | **1.695** |
   | yes | yes | 0.723 | 0.867 | 1.532 |

   The downloadable checkpoint is the large-margin one. KUE has no AS-Norm
   cohort, so the **bold row** is the most honest published baseline. Why none
   of these numbers is KUE's accuracy: section 6.

### 4.2 WeSpeaker ECAPA-TDNN512-LM — runner-up (named in the earlier design)

1. **Source.** https://huggingface.co/Wespeaker/wespeaker-ecapa-tdnn512-LM,
   described on its card as an official WeSpeaker model. Revision
   `a2f3dcb1c8702caccc7a55ceb57f5e8d1842112b`, last modified 2024-05-06. Files:
   `voxceleb_ECAPA512_LM.onnx` 24.9 MB, `avg_model.pt` 38.7 MB, `config.yaml`
   1.54 kB, `README.md` 3.23 kB. Not in the sherpa-onnx release asset list.
2. **Licence.** Code Apache-2.0; weights **CC-BY-4.0** (card tag, API confirmed).
3. **Training data.** VoxCeleb2 dev, 5,994 speakers; `num_class: 17982`,
   `speed_perturb: true`. Terms: section 3.1.
4. **Redistribution.** As 4.1: not gated, no key, CC BY 4.0 attribution; dataset
   conflict unresolved for (b).
5. **Size and format.** 6.19M parameters, 1.04G FLOPs (the input length the FLOPs
   figure assumes is not stated — **UNVERIFIED**); 192-dimension embedding;
   pooling ASTP. ONNX 24.9 MB, PyTorch 38.7 MB. No Core ML file.
6. **Inference.** As 4.1: 16 kHz, 80-dim fbank, 25/10 ms. Training crop
   `num_frms: 200` (2 s) — again a crop length, not a minimum. Apple-silicon
   compute **UNVERIFIED**. No network at runtime.
7. **Privacy.** As 4.1; 192-float vectors.
8. **Reported accuracy.** EER (%), VoxCeleb1 cleaned:

   | Large-margin fine-tune | AS-Norm | Vox1-O | Vox1-E | Vox1-H |
   |---|---|---|---|---|
   | no | no | 1.069 | 1.209 | 2.310 |
   | no | yes | 0.957 | 1.128 | 2.105 |
   | **yes** | **no** | **0.878** | **1.072** | **2.007** |
   | yes | yes | 0.782 | 1.005 | 1.824 |

   Worse than ResNet34-LM on every list in every row. Section 6 applies.

### 4.3 3D-Speaker CAM++ (VoxCeleb, English) — second runner-up

1. **Source.** https://www.modelscope.cn/models/iic/speech_campplus_sv_en_voxceleb_16k,
   published by the `iic` organisation on ModelScope for the 3D-Speaker project
   (https://github.com/modelscope/3D-Speaker). File `campplus_voxceleb.bin`,
   29,357,703 bytes, file revision `032b8131a7ad812f87061955ca974c99060c5a03`
   committed 2024-06-24 (ModelScope files API). Third-party ONNX export in
   sherpa-onnx: `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx`,
   29,596,978 bytes. (WeSpeaker also trains its own VoxCeleb CAM++; that is a
   different checkpoint.)
2. **Licence.** Code Apache-2.0 (3D-Speaker README). Weights: "Apache License
   2.0" (ModelScope metadata and README) — a different label from WeSpeaker's
   for the same VoxCeleb2 data. The label does not settle section 3.1.
3. **Training data.** VoxCeleb2, 5,994 speakers (README).
4. **Redistribution.** Apache 2.0: keep licence and notices. Whether ModelScope
   downloads need an account, and ModelScope's own platform terms, are
   **UNVERIFIED** (the file listing API answered without a login).
5. **Size and format.** 7.18M parameters, 1.72G FLOPs; PyTorch `.bin` only from
   the publisher; ONNX only from a third party. Conversion needs the 3D-Speaker
   model code or trust in the third-party export.
6. **Inference.** 16 kHz, 80-dimension fbank (README). Reported real-time factor
   0.013 on a single CPU thread, hardware unstated — not Apple silicon,
   **UNVERIFIED** for this Mac. No network at runtime.
7. **Privacy.** Offline; embeddings only (dimension not recorded here —
   **UNVERIFIED**).
8. **Reported accuracy.** EER 0.73 % / 0.89 % / 1.76 % on VoxCeleb1-O / E / H
   (README). Whether score normalisation was applied is **UNVERIFIED**.

### 4.4 SpeechBrain ECAPA-TDNN (spkrec-ecapa-voxceleb) — not recommended (format)

1. **Source.** https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb, published
   by SpeechBrain. Revision `0f99f2d0ebe89ac095bcc5903c4dd8f72b367286`, last
   modified 2025-02-18. Files: `embedding_model.ckpt` 83.3 MB, `classifier.ckpt`
   5.53 MB, `mean_var_norm_emb.ckpt` 1.92 kB, `hyperparams.yaml` 1.92 kB,
   `label_encoder.txt` 129 kB, examples; about 89.1 MB total.
2. **Licence.** Weights **apache-2.0** (card tag, API confirmed) — the cleanest
   licence label of any candidate, though it rests on the same VoxCeleb data.
   Toolkit code licence not fetched — **UNVERIFIED** here.
3. **Training data.** VoxCeleb1 + VoxCeleb2 training data (card). Terms: 3.1.
4. **Redistribution.** Not gated. Apache 2.0 notices. The card asks for a
   citation if used "for your research or business".
5. **Size and format.** Parameter count not stated on the card; the 83.3 MB
   checkpoint is consistent with roughly 20M float32 parameters — inference,
   **UNVERIFIED**. **PyTorch `.ckpt` only; no ONNX, no Core ML.** The
   hyperparameter file binds the checkpoint to SpeechBrain's Python classes, so
   any conversion needs SpeechBrain and PyTorch installed on a development
   machine to trace the model.
6. **Inference.** Trained on 16 kHz single-channel audio (card); 80 mel bins
   (`hyperparams.yaml`). The card notes its own code resamples and selects mono.
   Apple-silicon compute **UNVERIFIED**. No network at runtime.
7. **Privacy.** Offline; embeddings only.
8. **Reported accuracy.** EER 0.80 % on VoxCeleb1-test (cleaned) (card). The card
   disclaims any warranty for performance on other datasets.

### 4.5 NVIDIA TitaNet — not recommended (training-data terms)

**TitaNet-Large**

1. **Source.** https://huggingface.co/nvidia/speakerverification_en_titanet_large,
   NVIDIA. Revision `0dc382f40121a5fbd34db10a2bb04d826c2be6a8`, last modified
   2023-11-14. File `speakerverification_en_titanet_large.nemo`, 102 MB.
   Third-party ONNX in sherpa-onnx: `nemo_en_titanet_large.onnx`,
   101,405,493 bytes.
2. **Licence.** Weights **CC-BY-4.0** (card; API confirmed). NeMo toolkit
   Apache 2.0 (https://github.com/NVIDIA-NeMo/NeMo).
3. **Training data.** VoxCeleb-1, VoxCeleb-2, Fisher, Switchboard, LibriSpeech,
   SRE (2004–2010) (card). LDC terms: section 3.2 — **the reason it is not
   recommended**.
4. **Redistribution.** Not gated. CC BY 4.0 attribution. LDC question unresolved.
5. **Size and format.** About 23M parameters (card). `.nemo` archive; export to
   ONNX needs the NeMo toolkit on a development machine, or trust in the
   third-party export. About four times the file size of the WeSpeaker models.
6. **Inference.** 16 kHz mono (card). Apple-silicon compute **UNVERIFIED**. No
   network at runtime.
7. **Privacy.** Offline; embeddings only.
8. **Reported accuracy.** EER 0.66 % on the VoxCeleb1 cleaned trial list (card);
   the paper (https://arxiv.org/abs/2110.04410) reports 0.68 %.

**TitaNet-Small**

1. **Source.** NGC, https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/titanet_small,
   version 1.19.0, updated 2023-06-07, compressed size 35.77 MB. Third-party
   ONNX in sherpa-onnx: `nemo_en_titanet_small.onnx`, 40,257,283 bytes.
2. **Licence.** A different story from TitaNet-Large: "covered by the license of
   the NeMo Toolkit" (NGC), and downloading means accepting its terms. The page
   shows sign-in prompts; whether sign-in is required is **UNVERIFIED**.
3. **Training data.** VoxCeleb 1 and 2 dev, RIR noise, Fisher, Switchboard,
   LibriSpeech (NGC). LDC question as above.
4. **Redistribution.** NGC terms plus the NeMo licence — **UNVERIFIED** in detail.
5. **Size.** **UNVERIFIED — the sources contradict each other.** The NGC text
   describes "TitaNet-L" with 6.4M parameters on the TitaNet-S page; the paper's
   abstract says TitaNet-S has about 6M parameters; the 40.3 MB third-party ONNX
   is inconsistent with ~6M float32 parameters (~24 MB).
6. **Inference.** 16 kHz mono (NGC). No network at runtime.
7. **Privacy.** Offline; embeddings only.
8. **Reported accuracy.** The NGC page quotes 1.08 % EER on a VoxCeleb clean trial
   list, but in the same sentence that mislabels the model — **UNVERIFIED**.

### 4.6 Picovoice Eagle — REJECTED (licence server and usage metering)

1. **Source.** https://github.com/Picovoice/eagle and
   https://picovoice.ai/docs/eagle/, Picovoice. Model files (`.pv`) under
   `lib/common`. Version not recorded — **UNVERIFIED**.
2. **Licence.** Repository Apache-2.0. The engine library and model file terms
   are not stated on the repository page — **UNVERIFIED**. Picovoice's FAQ
   (https://picovoice.ai/docs/faq/general/) says it is a B2B company with "no
   dedicated free or paid plans for personal or non-commercial use"; the Free
   Trial is "a one-time offer". The pricing page could not be read.
3. **Training data.** Not disclosed — **UNVERIFIED**.
4. **Redistribution.** Requires a secret AccessKey per account; an app cannot
   carry a key for other people without a commercial agreement (inference from
   the key being secret and plan terms — **UNVERIFIED** in detail).
5. **Size and format.** Proprietary `.pv` model plus native library; macOS
   x86_64 and arm64; Swift, C and Python SDKs. Picovoice's own benchmark gives a
   model size of 4.48 MB.
6. **Inference.** Audio processing is local ("All voice processing runs
   locally", Eagle docs). But the README says you need "internet connectivity to
   validate your AccessKey with Picovoice license servers". The FAQ lists Eagle
   among engines whose usage is counted per second of audio processed; how that
   count reaches Picovoice is **UNVERIFIED**.
7. **Privacy.** A licence-server call and usage metering are network
   dependencies. KUE's privacy policy has no CLOUD_ALLOWED data and the router
   refuses EXTERNAL; a sensing process that must phone a vendor to work does not
   fit that model. **Rejected on policy grounds.**
8. **Reported accuracy.** Picovoice's benchmark
   (https://github.com/Picovoice/speaker-recognition-benchmark) reports EER
   0.18 % for Eagle, 0.49 % for pyannote and 0.70 % for SpeechBrain, but the
   README does not name the corpus or how trials were built. **The number is not
   usable as evidence** and should not be cited later.

### 4.7 Dropped or noted briefly

- **pyannote/embedding** (https://huggingface.co/pyannote/embedding): MIT, trained
  on VoxCeleb, **gated** — "You need to agree to share your contact information".
  An older model with no advantage over 4.1; the gate is a needless disclosure.
- **pyannote/wespeaker-voxceleb-resnet34-LM**
  (https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM): cc-by-4.0, a
  wrapper around the same WeSpeaker ResNet34-LM weights for `pyannote.audio`. Its
  gating was not checked through the API (**UNVERIFIED**); take the weights from
  the WeSpeaker repository instead.
- **Resemblyzer** (https://github.com/resemble-ai/Resemblyzer): Apache-2.0 code;
  `pretrained.pt` 17,090,379 bytes; 256-dimension GE2E embedding; 40-channel mel,
  16 kHz, 1.6 s partials. Training data not stated in the repository —
  **UNVERIFIED**. Dropped: unknown provenance.
- **3D-Speaker Mandarin models** (ERes2NetV2, CAM++ `zh-cn-16k-common`): Apache
  2.0 label, trained on an undisclosed ~200k-speaker Mandarin set (section 3.3).
  Dropped: unknown provenance.
- **WeSpeaker CN-Celeb models**: CN-Celeb is CC BY-SA 4.0, Mandarin. Dropped.
- **FluidAudio** (https://github.com/FluidInference/FluidAudio): Apache-2.0 Swift
  package that runs a WeSpeaker embedding (which variant: **UNVERIFIED**)
  converted to Core ML by a third party. It **downloads models from Hugging Face
  at runtime** unless offline mode is set. Not a candidate as-is; useful only as
  evidence that a Core ML conversion of a WeSpeaker embedding is feasible.
- **sherpa-onnx** (https://github.com/k2-fsa/sherpa-onnx): Apache-2.0 runtime
  with Swift and macOS support; hosts third-party ONNX exports whose release note
  says each model has its own licence. A possible reference implementation, not
  a model source.
- **Cloud speaker-recognition services**: rejected by policy (raw audio would
  leave the Mac). Not evaluated.

---

## 5. Running it on this Mac — what each path involves

Nothing here has been tried. Each path brings its own downloads, and each of
those is a separate approval.

**Path A — ONNX Runtime inside `lantern-sense`.** Uses the publisher's ONNX file
unchanged, which keeps attribution simple and avoids conversion errors. ONNX
Runtime's Core ML execution provider needs macOS 10.15 or later, sends the
operations Core ML supports to Apple hardware, and runs the rest on the CPU
(https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html).
Costs: a native library dependency (source, version and size to be stated at the
time — **UNVERIFIED**), bridging its C API into Swift, and a check of the
library's own telemetry behaviour on macOS (**UNVERIFIED**).

**Path B — Core ML conversion.** Core ML Tools does not convert ONNX: the
`onnx-coreml` converter is "frozen and no longer updated or maintained"
(Core ML Tools FAQ), and the unified API converts from PyTorch and TensorFlow.
So the path is: the PyTorch checkpoint (`avg_model`, 45.1 MB) plus WeSpeaker's
model code, traced with PyTorch, converted with Core ML Tools into a compiled
model shipped with the sensing helper. That needs Python, PyTorch, Core ML Tools
and the WeSpeaker code on a development machine. A variable-length time axis
needs flexible input shapes; whether those run on the Neural Engine on macOS is
**UNVERIFIED** (the Core ML Tools docs mention a reshape hint for iOS 17.4).
The converted model is a changed form of the licensed material, to be marked as
such. Output must be compared numerically with the ONNX or PyTorch output on the
same input.

**Either path — the front end is where accuracy is silently lost.** The model
consumes filterbank features, not audio, so KUE must reproduce WeSpeaker's
Kaldi-compatible `fbank` exactly in Swift/Accelerate: window type (a
per-model setting in WeSpeaker's CLI), pre-emphasis, DC removal, energy floor,
log, per-utterance mean subtraction, and waveform scale. Two specific traps:
the training config has `dither: 1.0` while the CLI's inference call shows no
dither; and Kaldi-style fbank is conventionally fed int16-scaled samples, but
the fetched summary of WeSpeaker's CLI showed no scaling step — **UNVERIFIED**,
must be read in the source. A mismatch here raises no error; it only makes
every score worse.

---

## 6. What the published numbers are not

**EER is not a false-acceptance rate.** Equal error rate is the single point on
a trial list where false acceptance and false rejection happen to be equal. It
is threshold-free and symmetric. KUE needs the opposite: a threshold chosen for
a very low false-acceptance rate, accepting more false rejections. No
threshold, and no FAR or FRR at any threshold, can be read off an EER.

**The best rows need a cohort KUE does not have.** AS-Norm normalises each score
against a cohort of other speakers' embeddings, taken from training data. KUE
would have no such cohort without downloading more data (another decision). The
large-margin-without-AS-Norm rows (4.1: 0.797 / 0.937 / 1.695) are the fairer
baseline, and even they are not KUE's accuracy.

**The domain is different.** VoxCeleb is celebrities in YouTube videos, with
unknown microphones, codecs and noise. KUE is one owner, a MacBook Air's built-in microphones,
one or a few rooms, short commands, and KUE's own synthetic voice playing from
the same machine (see [KUE_ECHO_AND_SELF_WAKE.md](KUE_ECHO_AND_SELF_WAKE.md)).
How the model does on the owner's language and accent is unmeasured.

**What KUE would have to measure itself, on this Mac:**

1. **Front-end and model parity** — the same audio gives the same embedding as
   the reference implementation, within a stated tolerance.
2. **Owner genuine trials** — held-out owner utterances against the enrollment,
   across sessions and days, times of day, distance from the Mac, background
   noise, a cold or a tired voice, and short utterances; this gives the false
   rejection rate at a candidate threshold.
3. **KUE speaking** — utterances captured while KUE's own voice plays must come
   out `VOICE_UNMEASURABLE`, never `VOICE_MATCHED`.
4. **Impostor trials** — utterances from people who are not the owner, through
   the same microphone and room, to estimate false acceptance. **This is a real
   blocker: the owner does not have an impostor set and cannot easily build
   one.** Options each need their own decision: consenting people recording on
   this Mac, or downloading a public speech corpus under its own licence.
   Synthetic `say` voices are not a substitute: they differ far more than people
   do and would flatter the result.
5. **Statistical honesty** — with zero false accepts in N independent impostor
   trials, the 95 % upper bound on the false-acceptance rate is roughly 3/N. A
   claim of "under 1 %" needs about 300 independent impostor trials with no
   accepts, and trials from only a few impostors are not independent. A small
   impostor set can therefore only support a weak claim — which is exactly why
   voice must stay a factor that can deny or corroborate, never grant.
6. **Replay is not defended.** A phone speaker playing a recording of the owner,
   or a synthesised clone of the owner's voice, can match. No model in this
   record includes anti-spoofing. This is a known limitation, not a test to pass.

---

## 7. Comparison

| Candidate | Weights licence | Training data | Data-terms risk | Gate / key | Network at runtime | Publisher formats | File | Params / dim | Reported EER (benchmark) | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| **WeSpeaker ResNet34-LM** | CC-BY-4.0 | VoxCeleb2 dev | VoxCeleb conflict (3.1) | none | none | ONNX, PyTorch | 26.5 MB ONNX | 6.63M / 256 | 0.797 / 0.937 / 1.695 % Vox1-O/E/H, no AS-Norm | **Recommended** (personal use, after Phase 1) |
| WeSpeaker ECAPA512-LM | CC-BY-4.0 | VoxCeleb2 dev | VoxCeleb conflict | none | none | ONNX, PyTorch | 24.9 MB ONNX | 6.19M / 192 | 0.878 / 1.072 / 2.007 %, no AS-Norm | Runner-up |
| 3D-Speaker CAM++ VoxCeleb | Apache-2.0 (label) | VoxCeleb2 | VoxCeleb conflict | account **UNVERIFIED** | none | PyTorch `.bin` (ONNX third-party) | 29.4 MB | 7.18M / **UNVERIFIED** | 0.73 / 0.89 / 1.76 %, norm **UNVERIFIED** | Second runner-up |
| SpeechBrain ECAPA | Apache-2.0 | VoxCeleb1+2 | VoxCeleb conflict | none | none | PyTorch `.ckpt` only | 83.3 MB | ~20M **UNVERIFIED** | 0.80 % Vox1-test cleaned | Not recommended (format) |
| NVIDIA TitaNet-Large | CC-BY-4.0 | VoxCeleb1+2, Fisher, Switchboard, LibriSpeech, SRE | VoxCeleb + **LDC** (3.2) | none | none | `.nemo` (ONNX third-party) | 102 MB | 23M | 0.66 % Vox1 cleaned | Not recommended (data terms, size) |
| NVIDIA TitaNet-Small | NeMo toolkit licence (NGC) | VoxCeleb1+2 dev, Fisher, Switchboard, LibriSpeech | VoxCeleb + LDC | NGC terms, sign-in **UNVERIFIED** | none | `.nemo` (ONNX third-party) | 35.77 MB compressed | **UNVERIFIED** (sources conflict) | 1.08 % **UNVERIFIED** | Not recommended |
| Picovoice Eagle | Proprietary engine/model **UNVERIFIED**; repo Apache-2.0 | Undisclosed | Unknown | **AccessKey**, B2B plans | **Licence server; usage metered** | `.pv` + native library | 4.48 MB (vendor) | undisclosed | 0.18 % on unnamed corpus — unusable | **Rejected** (policy) |
| Cloud services | — | — | — | — | raw audio leaves the Mac | — | — | — | — | **Rejected** (policy) |

---

## 8. RECOMMENDATION

**Recommended candidate: WeSpeaker ResNet34-LM** — the publisher's ONNX file
`voxceleb_resnet34_LM.onnx` (26.5 MB) from
https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM at revision
`f0c48c298fd835726c27956a5d617bad7115627e`, weights CC-BY-4.0, **for the owner's
personal use on this Mac only**, **downloaded only after the live microphone
path (Phase 1) is verified**. Bundling it in a distributed app is **not**
recommended until the VoxCeleb terms conflict (section 3.1) is resolved.

Reasons:

1. **Cleanest training-data story among the usable models.** VoxCeleb2 dev only;
   no LDC corpora (unlike TitaNet), no undisclosed data (unlike Eagle and the
   Mandarin 200k models).
2. **Fits the privacy model.** Not gated, no account, no key, no licence server,
   no network at runtime.
3. **The publisher ships ONNX.** It can run without KUE converting anything
   (Path A), and the file KUE runs is the file the publisher licensed.
   SpeechBrain and 3D-Speaker publish PyTorch only; TitaNet publishes `.nemo`.
4. **Best reported numbers of the VoxCeleb-only publisher-ONNX options** —
   better than ECAPA512-LM on all three VoxCeleb1 lists in every row compared
   (large-margin, no AS-Norm: 0.797 / 0.937 / 1.695 vs 0.878 / 1.072 / 2.007),
   at a similar size (26.5 vs 24.9 MB, 6.63M vs 6.19M parameters). These are
   still not KUE's accuracy (section 6).
5. **More independent users of the same weights** — Praat, pyannote's wrapper,
   sherpa-onnx's export — which gives more reference implementations to check
   KUE's front end against.

**Why this changes the earlier design's pick.** KUE_SPEAKER_IDENTITY.md named
ECAPA-TDNN512-LM. The two share licence, data, gating and format; the evidence
gathered here favours ResNet34-LM on reported accuracy and on reuse, at a cost
of 1.6 MB and 64 more floats per stored vector. ECAPA512-LM remains an
acceptable alternative if the owner prefers to keep the earlier choice.

**What this recommendation does not do.** It does not implement anything, does
not change KUE's registry, does not make voice a factor that grants access, and
does not settle the dataset licence question.

---

## 9. DECISIONS FOR THE OWNER

Each is a separate yes or no.

1. **Phase 1 first.** Accept that no speaker model is downloaded until the live
   microphone path is verified on this Mac.
2. **Approve the download** of `voxceleb_resnet34_LM.onnx` (26.5 MB),
   `config.yaml` (1.67 kB) and `README.md` (3.15 kB) from
   https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM at revision
   `f0c48c298fd835726c27956a5d617bad7115627e`, weights under **CC-BY-4.0**, for
   personal use on this Mac only. (Not the 45.1 MB PyTorch checkpoint unless
   decision 4 chooses Core ML.)
3. **Accept the dataset-terms risk for personal use:** VoxCeleb's `license.txt`
   says CC BY 4.0 over "the data" with no research or commercial wording; its
   current pages say CC BY-SA 4.0 over metadata only; a mirror and the privacy
   notice frame it as research; the audio's copyright stays with the YouTube
   uploaders. This is unresolved and is **not** accepted for distribution.
4. **Choose the runtime path**, each bringing its own later download approval:
   (A) ONNX Runtime library inside `lantern-sense`, or (B) Core ML conversion on
   a development machine (Python, PyTorch, Core ML Tools, WeSpeaker code, and
   the 45.1 MB checkpoint).
5. **Accept the ceiling:** voice can deny or corroborate, never grants more than
   presence alone; voice without a face reaches no more than LEVEL_1; voice
   never replaces Touch ID for LEVEL_3–4; a conflict denies.
6. **Accept the known limitations:** replay and voice cloning are not defended;
   published EER will never be presented as KUE's accuracy; KUE reports only
   what it measures on this Mac.
7. **Decide how, or whether, an impostor set is built** (consenting people
   recording on this Mac, or a public corpus under its own licence). Without
   one, KUE can report false rejections of the owner but can say nothing about
   false acceptance of others.
8. **Accept that enrollment embeddings are biometric-derived data**, stored on
   this Mac in the sensing layer and handled as the face descriptors are:
   enrollment at LEVEL_3, reset at LEVEL_4, never in memory, context, logs or
   the window.
9. **Confirm the rejections:** Picovoice Eagle (licence server and usage
   metering), cloud services (raw audio leaves the Mac), and TitaNet (LDC-corpus
   training terms).
10. **Distribution is a separate, later decision**, not made by approving 2.

## 10. What happens after approval (steps, no code)

1. **Phase 1:** the live microphone path is verified on this Mac. Nothing below
   starts before this.
2. **Download only the approved files**, from the pinned revision. Record each
   file's SHA-256, the revision, the date and the source URL. Write the
   attribution record: WeSpeaker as author, the CC BY 4.0 licence link, the
   VoxCeleb citation, and a note of any change (for example, a Core ML
   conversion).
3. **Runtime approval (decision 4):** the chosen path's own downloads are stated
   (source, version, size) and approved one by one.
4. **Front-end parity:** KUE's filterbank features for a fixed set of audio files
   are compared with the reference implementation's features for the same
   files. The reference run itself needs an approved tool, stated at that time.
5. **Embedding parity:** KUE's embeddings for those files match the reference
   within a stated tolerance.
6. **Enrollment on this Mac:** the owner reads prompted sentences, authorised at
   LEVEL_3; only vectors, model id and revision are kept.
7. **Measurement on this Mac:** owner genuine trials across sessions and
   conditions; KUE-speaking trials; impostor trials per decision 7; a threshold
   chosen for low false acceptance; false-acceptance and false-rejection counts
   with their bounds written down; replay recorded as a known failure.
8. **Only then:** verdicts wired into the existing identity session per the
   fusion table in KUE_SPEAKER_IDENTITY.md, off by default. Tests, parity checks
   and recorded audio files move the registry no further than
   TEST_VERIFIED_ONLY; LIVE_VERIFIED only after verdicts are seen working through
   KUE's own app, with the real microphone, in a real room.
9. **Distribution (decision 10)** revisited separately, with section 3 resolved.

---

## Sources (read 2026-09-16)

Models
- https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM (card, tree, `config.yaml`, API)
- https://huggingface.co/Wespeaker/wespeaker-ecapa-tdnn512-LM (card, tree, `config.yaml`, API)
- https://github.com/wenet-e2e/wespeaker — `docs/pretrained.md`, `wespeaker/bin/export_onnx.py`, `wespeaker/cli/speaker.py`
- https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb (card, tree, `hyperparams.yaml`, API)
- https://huggingface.co/nvidia/speakerverification_en_titanet_large (card, tree, API)
- https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/titanet_small
- https://arxiv.org/abs/2110.04410 (TitaNet)
- https://github.com/NVIDIA-NeMo/NeMo
- https://github.com/modelscope/3D-Speaker
- https://www.modelscope.cn/models/iic/speech_campplus_sv_en_voxceleb_16k (README, files API)
- https://www.modelscope.cn/models/iic/speech_eres2netv2_sv_zh-cn_16k-common (README)
- https://www.modelscope.cn/models/iic/speech_campplus_sv_zh-cn_16k-common (README)
- https://huggingface.co/pyannote/embedding
- https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM
- https://github.com/resemble-ai/Resemblyzer
- https://github.com/Picovoice/eagle, https://picovoice.ai/docs/eagle/, https://picovoice.ai/docs/faq/general/
- https://github.com/Picovoice/speaker-recognition-benchmark
- https://github.com/k2-fsa/sherpa-onnx and its `speaker-recongition-models` release (GitHub API)
- https://github.com/FluidInference/FluidAudio

Datasets and terms
- https://www.robots.ox.ac.uk/~vgg/data/voxceleb/ , `vox1.html`, `vox2.html`, `files/license.txt`
- https://www.robots.ox.ac.uk/~vgg/terms/url-lists-privacy-notice.html
- https://mm.kaist.ac.kr/datasets/voxceleb/
- https://praat.org/manual/VoxCeleb_CC-BY-4_0_license.html
- https://www.openslr.org/82/ (CN-Celeb), https://www.openslr.org/12/ (LibriSpeech), https://www.openslr.org/17/ (MUSAN), https://www.openslr.org/28/ (RIR and noise)
- https://3dspeaker.github.io/
- https://catalog.ldc.upenn.edu/license/ldc-non-members-agreement.pdf (not readable; search summary only)

Runtime
- https://apple.github.io/coremltools/docs-guides/source/faqs.html
- https://apple.github.io/coremltools/docs-guides/source/unified-conversion-api.html
- https://apple.github.io/coremltools/docs-guides/source/flexible-inputs.html
- https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html
