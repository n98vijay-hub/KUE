# Lantern

A private personal AI that observes your immediate context on your Mac — and is
honest about the limits of what it can actually tell.

This is a working prototype, not a demo. Every number on screen comes from a real
sensor or from arithmetic you can redo by hand. Nothing is simulated, and
anything not implemented says so in the interface.

---

## What it actually does

```
camera ──▶ Apple Vision ──▶ identity ──▶ evidence ──▶ context object ──▶ interface
              ▲                             ▲
   NSWorkspace│ HID idle timer              │ temporal event stream
```

| Signal | How |
|---|---|
| Camera capture | AVFoundation, analysed in memory, never recorded |
| Face detection | Vision face rectangles + 76-point landmarks |
| Head pose | Vision yaw / pitch / roll, in degrees |
| Capture quality | Vision's own per-face quality score |
| Tracking continuity | IoU tracker with stable track IDs |
| Identity | Two descriptors that must agree (see below) |
| Frontmost app | `NSWorkspace` — app identity only |
| Recent input | HID idle timer — seconds since last event, nothing else |
| Temporal context | Change-triggered event log with stability tracking |
| Confidence | Deterministic arithmetic over weighted evidence |
| Memory | Local SQLite: events with the evidence behind each conclusion, and context snapshots |
| Failure states | Every named failure state is listed with a status: active, clear, not applicable, or not implemented |
| Cost | CPU time and memory footprint of both processes, thermal state, Low Power Mode, battery |
| Identity check | Measures whether your enrolled samples are separable from samples of another person |

## Running it

```bash
./scripts/build-app.sh
open "target/release/bundle/macos/KUE.app"   # or ./scripts/run-kue.sh
```

**Lantern must be run as a bundled `.app`.** macOS will not show a camera
permission prompt for an unbundled binary — it fails closed and reports "denied"
with no dialog at all. `tauri dev` runs an unbundled binary, so the camera will
not work there. This was verified empirically on macOS 26; see
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

Because the app is ad-hoc signed, **macOS re-asks for camera permission after
every rebuild** (the signature hash changes, so TCC treats it as a new app).

```bash
./scripts/build-app.sh --debug   # faster build while iterating
cargo test -p lantern-core       # 108 tests, no camera required
cargo test -p lantern            # 2 end-to-end tests against the real Swift binary (camera never started)
./sensing/build.sh && open sensing/bundle/LanternSense.app \
  --args --selftest --duration 20 --out /tmp/sense.jsonl   # sensing layer alone
cargo run -p lantern-core --example replay -- /tmp/sense.jsonl   # replay a recording through the real engine
```

Only one copy of Lantern can hold local memory at a time. A second copy — a debug
build left running while you open a release build, say — reports
`STORAGE_UNAVAILABLE` rather than writing the same database. `build-app.sh`
warns when an old copy is still running.

## Enrolling your face

Open Lantern, then use **Capture sample** in the Identity panel five times,
varying your head angle between captures. Add more samples later in different
lighting — see the measured finding below for why that matters. **Undo last
sample** removes a capture you did not mean to take.

## Honest limits

**Identity matching is weak biometrics.** Lantern compares two things:

1. **Landmark geometry** — ratios between facial landmarks, normalised for roll
   and scale.
2. **Vision FeaturePrint** — Apple's *general image similarity* embedding,
   computed on a tight face crop. It is not a face-identity model.

A claim in either direction requires **both descriptors to agree**. When they
conflict, Lantern reports `IDENTITY_UNCERTAIN` and says why.

**A measured result from this machine.** With the face crop padded by 0.25, the
FeaturePrint distance for the *same face* drifted to **4.5×** the enrollment
spread across a 20-minute lighting change, while landmark geometry held at
**0.91×**. Under the original weighted-average rule this produced
`UNKNOWN_PERSON` — the system accused its own owner of being a stranger.

Two things changed as a result: the crop was tightened to 0.08, which brought the
FeaturePrint to **0.63×** (a 7× improvement), and no single descriptor may make a
claim alone. Both are pinned by regression tests.

**The reject side has never been validated.** Thresholds are calibrated from the
spread of your own enrolled samples. Lantern has no data on anyone else's face,
so `UNKNOWN_PERSON` is an unvalidated heuristic and the interface says so. The
**Identity check** panel can measure it: with consent, capture a few samples of
someone who is not you (kept apart from your enrollment), and it reports whether
the two sets separate on each descriptor. Until that is done it reads
`INSUFFICIENT_DATA`.

**Your own samples are spread wide on the FeaturePrint.** Measured across the 15
enrolled samples on this machine: FeaturePrint distance between two of *your
own* samples has median 0.37 and max 0.66; landmark geometry has median 0.098
and max 0.217. Because thresholds are expressed as multiples of that spread, a
wide self-spread means a wide accept band — which is the side that has not been
tested against another person.

**Identity flicker, measured.** On a real session Vision regularly misses a face
for a single frame. Before any fix, 102 identity changes were recorded in ~518s
of camera time. Two changes followed, each measured:

1. A match is *carried* through frames where the face cannot be measured (for
   up to `hold_unmeasurable_seconds`). A measurement that disagrees still demotes
   at once.
2. The interface still shows every state change the instant it happens, but an
   identity state is only **written to memory once it has held** for
   `event_min_seconds`, dated from when it began. States that came and went in
   between are counted in the next recorded event, never silently dropped.

After (1) alone, 107 identity changes were recorded in 623s: 67 of 117 states held
under one second, and `NO_FACE` states lasted a median 0.29s — one missed frame.

**An open trade-off.** `NO_FACE` still applies immediately, because a stale
"confirmed" after the lens is covered would be a lie. The price is that a
one-frame detection miss drops the displayed identity for that frame. Holding
`NO_FACE` for a frame or two would steady the display, at the cost of briefly
claiming a face that is gone. That choice is yours; it is not made here.

**What Lantern cannot know**, and states plainly: what you are thinking or
feeling, whether you are focused, what is on your screen, or anything you say.

## Privacy

- **A privacy firewall enforces every write to local memory** (KUE priority 1,
  first increment). Every kind of data is classified; the store accepts only
  values the firewall cleared; memory keeps an allowlisted summary, so body
  joint positions, hand positions and per-frame face measurements are used live
  and never stored. Each decision goes into a payload-free audit ledger. See
  [docs/KUE_STATUS.md](docs/KUE_STATUS.md) for what is and is not enforced yet.
- Deleting memory removes the bytes from disk (`secure_delete`, WAL truncation,
  `VACUUM`), verified by reading the raw database files in tests.
- No network code. Lantern makes no requests of any kind, and the webview's
  Content Security Policy permits only its own assets and the IPC bridge.
- Camera frames exist in memory for one analysis pass, then are released. No
  frame, crop, or image is ever written to disk.
- Enrollment stores **derived descriptors only** — a vector of ratios and an
  opaque embedding. Neither can be rendered back into a photograph.
- Input detection reads *seconds since the last HID event*. It is structurally
  incapable of capturing keystrokes or typed text — there is no API path from a
  scalar idle timer to content.
- No window titles, document names, URLs, or clipboard contents are read.
- **Pause** is enforced in the sensing layer, not just in the UI: the capture
  session is torn down (the camera indicator light goes out), computer-activity
  sampling is cancelled, and the core discards every reading it held. A reading
  that arrives after pause took effect is discarded and reported as a
  contradiction — pause is checked, not assumed. Measured against the real
  binary: 0 readings in 5s of pause, camera `STOPPED` 0.2s after pausing, and
  the paused sensing layer used 0.07% CPU. Only self-measurement (Lantern's own
  CPU and memory, thermal state) continues while paused; it observes nothing
  about you.
- Every remembered conclusion carries its evidence and confidence at the time it
  was drawn, so "why did it think that?" is answerable later from the database.

## Layout

```
sensing/     Swift — owns the camera and Apple Vision, and measures its own cost. Emits measurements only.
core/        Rust — temporal state, evidence, confidence, failure states, context object, storage.
src-tauri/   Tauri — window and IPC. Does not perceive and does not reason.
src/         React — draws the context object. Has no path to any sensor.
config/      Every threshold and weight, in one TOML file.
```

The layering is enforced by the type system: `core::sensor` has no field capable
of expressing a conclusion, and the UI has no command that reaches a device.
