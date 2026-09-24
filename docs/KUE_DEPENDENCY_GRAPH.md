# KUE dependency graph — what must be reliable before what

**2026-09-20.** Derived from measurements, not from a feature wish-list. An
arrow means *"the thing above is unreliable or unsafe until the thing below
is"*. Where the evidence contradicts the obvious order, the evidence wins and
the reason is stated.

```
                    PROACTIVE  ADAPTIVE
                          ▲      ▲
              ┌───────────┴──────┴───────────┐
              │        MEMORY + CALENDAR      │
              └───────────────▲───────────────┘
                              │
         ┌────────────────────┴────────────────────┐
         │   WEB RESEARCH        COMPUTER USE      │
         └────────────────────▲────────────────────┘
                              │
                      DYNAMIC PLANNING
                              ▲
                      GOVERNED TOOLS
                              ▲
              ┌───────────────┴───────────────┐
              │  AMBIENT VOICE                │
              └───────────────▲───────────────┘
                              │
   ┌──────────────────────────┴──────────────────────────┐
   │  FAST INTENT + LATENCY        VERIFIED-FACT GROUNDING │
   └──────────────────────────▲──────────────────────────┘
                              │
                      IDENTITY STABILITY
                              ▲
                      OBSERVABILITY  ✅ landed 2026-09-20
                              ▲
                      REPOSITORY TRUTH  ✅ landed 2026-09-20
```

## The edges, and why each one is real

| This | needs | Because — measured |
|---|---|---|
| Anything | repository truth | Until 2026-09-20 the newest analysis existed only inside Claude worktrees and `main` was the pre-rename prototype. Work started from the wrong tree rebuilds the wrong product |
| Identity stability | observability | The prevailing theory was "stale camera data". The instrument showed 65.6 % of uncertainty is *ambiguity* and 0 % is contradiction. Without it, the fix would have been aimed at 17 % of the problem |
| Identity stability | latency | 4 of 4 model runs pushed the measurement age past 3.0 s. Prefill is not merely slow, it is an identity cause |
| **Latency** | **nothing** | ← **the only upper-layer work with no unmet dependency.** 48 % of the prompt is a capability dump sent on every question; fast-path routing needs no camera, no owner, no second person |
| Verified-fact grounding | nothing | The action pipeline already produces verified results; only the type and the grounding rule are missing. Independent of identity |
| Ambient voice | identity + latency | Directedness leans on speaker identity and conversation state; a 20 s answer makes a spoken loop unusable; a session that drops 460 times an hour refuses spoken requests |
| Governed tools | verified-fact grounding | A tool result that the model can contradict is not a tool result |
| Dynamic planning | governed tools | A planner needs declared inputs, outputs, preconditions and verifiers to plan against |
| Web research | planning + tools + governance | Untrusted input entering a system whose planner is five fixed templates |
| Computer use | planning + tools + a threat model | Highest-consequence capability; Accessibility is the largest single increase in what a mistake costs |
| Memory | privacy retention decisions | Memory on unclear boundaries is a surveillance archive by another name |
| Proactivity | memory + calendar + identity | Interrupting the owner on an identity that changes 460 times an hour |
| Speaker identity | a live microphone + the owner's model-licence decision | Owner-blocked, and voice can only ever corroborate |

## Owner-blocked vs. buildable

| Owner-blocked (cannot be *accepted* without the owner) | Buildable now |
|---|---|
| Identity acceptance criteria (< 10 changes/hour seated; 0 false accepts with a second person) | Identity **forensics** — why a measurement is ambiguous |
| Hands-free in a real room | Latency: prompt diet, fast-path routing |
| Touch ID completing an action | Verified-fact grounding |
| Kill / pause on this build | Tool declarations |
| Speaker model licence decision | Runtime state machine; `lib.rs` split |

**Consequence for sequencing:** the identity *fix* cannot be accepted this week,
but identity *forensics* can be built and will collect evidence during ordinary
use. Meanwhile latency and grounding are unblocked, high-value, and one of them
(latency) is itself an identity cause. That is why the next slices are latency
and forensics in parallel rather than "fix identity".

## What this graph forbids today

Claude/cloud reasoning · web research · Accessibility · proactivity · calendar ·
speaker identity. Each multiplies the consequences of an unstable identity and a
20-second answer path, and each has an unmet dependency above.
