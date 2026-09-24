# mind/ — on-device language model process

**Status: REAL — connected behind the privacy firewall and model router.**

A Swift process wrapping Apple's on-device foundation model (`FoundationModels`,
`SystemLanguageModel.default`). It holds no camera, microphone or screen handle
and can only return text.

## How it is connected

The three conditions set when it was disconnected are now met:

1. Every prompt is built by `privacy::Firewall::clear_model_context`, which
   returns a sealed `Cleared<ModelPrompt>` for `Destination::LocalModel` from
   allowlisted kinds only (tests: `core/tests/privacy_scenarios.rs`).
2. The kill switch terminates it, and it cannot start while killed
   (`kill_terminates_the_real_sensing_process_and_nothing_can_restart_it`).
3. It is reached only through `core::router::route`, after the
   `AskModelWithPersonalContext` authorization gate (LEVEL_2 or Touch ID).

The conversation lives in memory for the session only, is cleared when the
session locks, a stranger appears, or KUE is killed, and is never written to
local memory. The model's stderr is drained and discarded, never logged.

Measured on this Mac: a one-line answer from a cleared context in 5.6 s
(`cargo test -- --ignored a_real_question`, run from `src-tauri/`).

## Building it standalone

```bash
./mind/build.sh   # output in mind/bin/, which is gitignored
```
