/** What the window is allowed to put on screen.
 *
 * The invariants live in core, where the decisions are made, and are tested
 * there. These test the other half: that the window renders what core said and
 * invents nothing — including the one invariant that is genuinely the window's,
 * because it is about what a person can see.
 *
 * These render to static markup, so they need no DOM. The components under test
 * are prop-driven and have no effects; anything with IPC is left to live
 * verification, and the report says so.
 */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ActivityLine, Attention, Presence, TrustStrip } from "./components/Presence";
import { isKilled } from "./killed";
import { MemoryList, STATUS_PILL } from "./components/Sheets";
import { StorageBody } from "./components/Storage";
import { Perception } from "./components/Perception";
import { Plan, Runtime, Thread } from "./components/Runtime";
import type { PerceptionReport } from "./components/Perception";
import type { Candidate, StorageView } from "./components/Storage";
import type { Surface } from "./surface";

const SURFACE: Surface = {
  system: "RUNNING",
  system_sentence: "KUE is running.",
  presence: "RECOGNISED",
  presence_sentence: "I recognise you.",
  presence_detail: null,
  activity: "IDLE",
  activity_sentence: null,
  activity_action_id: null,
  activity_verified: false,
  trust: {
    camera: { state: "ON", word: "Camera on" },
    microphone: { state: "OFF", word: "Microphone off" },
    computer: { state: "ON", word: "Sees which app is in front" },
    external_ai: { state: "OFF", word: "Nothing leaves this Mac" },
    memory: { state: "ON", word: "Keeping what it works out" },
    kue: { state: "ON", word: "KUE is running" },
  },
  context: [],
  attention: [],
};

const render = (el: React.ReactElement) => renderToStaticMarkup(el);

describe("the window claims nothing core did not say", () => {
  it("shows the verified mark only when core recorded a verification", () => {
    const done = { ...SURFACE, activity: "DONE" as const,
      activity_sentence: "Google Chrome is open.", activity_verified: true };
    expect(render(<ActivityLine s={done} />)).toContain("verified");

    // The same sentence, the same state name, without core's permission.
    const unconfirmed = { ...done, activity_verified: false };
    expect(render(<ActivityLine s={unconfirmed} />)).not.toContain("verified");
  });

  it("says only what core put in the sentence", () => {
    const acting = { ...SURFACE, activity: "ACTING" as const,
      activity_sentence: "Opening Google Chrome…" };
    const html = render(<ActivityLine s={acting} />);
    expect(html).toContain("Opening Google Chrome…");
    // No invented progress, percentage or reassurance.
    expect(html).not.toMatch(/%|almost|nearly|should be/i);
  });

  it("shows nothing at all when KUE is doing nothing", () => {
    const html = render(<ActivityLine s={SURFACE} />);
    expect(html).not.toMatch(/>[^<]*[A-Za-z][^<]*</);
  });

  it("puts core's words on every trust signal, never a colour alone", () => {
    const html = render(<TrustStrip s={SURFACE} onOpen={() => {}} />);
    for (const word of ["Camera on", "Microphone off", "Sees which app is in front",
                        "Nothing leaves this Mac", "Keeping what it works out"]) {
      expect(html).toContain(word);
    }
  });

  it("stops claiming anything about the sensors once the projection is stale", () => {
    const html = render(<TrustStrip s={SURFACE} stale onOpen={() => {}} />);
    expect(html).not.toContain("Camera on");
    expect(html).toContain("not sure");
  });

  it("never says it is listening on its own", () => {
    // The window has no path to this state: Activity comes from core, which
    // will not produce LISTENING unless the microphone report is live.
    const listening = { ...SURFACE, activity: "LISTENING" as const, activity_sentence: "Listening…" };
    expect(render(<ActivityLine s={listening} />)).toContain("Listening…");
    expect(render(<ActivityLine s={SURFACE} />)).not.toContain("Listening");
  });

  it("reads the presence sentence out, and gives it to a screen reader too", () => {
    const html = render(<Presence s={{ ...SURFACE, presence: "UNCERTAIN",
      presence_sentence: "I'm not certain it's you." }} />);
    expect(html).toContain("I&#x27;m not certain it&#x27;s you.");
    expect(html).toContain('aria-label="I&#x27;m not certain it&#x27;s you."');
  });
});

describe("what needs the owner", () => {
  const item = (over: Partial<Surface["attention"][number]>) => ({
    kind: "PERMISSION" as const, sentence: "I need camera access.", action_id: null,
    permission: null, ...over,
  });

  it("offers the Settings control from the permission field, not from the wording", () => {
    const html = render(<Attention item={item({ permission: "CAMERA" })} busy={false}
      onConfirm={() => {}} onCancel={() => {}} onSettings={() => {}} />);
    expect(html).toContain("Open Settings");

    // Reworded sentence, same field — the control survives.
    const reworded = render(<Attention item={item({ permission: "CAMERA", sentence: "I can't see." })}
      busy={false} onConfirm={() => {}} onCancel={() => {}} onSettings={() => {}} />);
    expect(reworded).toContain("Open Settings");

    // The microphone gets one too, which it previously did not.
    const mic = render(<Attention item={item({ permission: "MICROPHONE" })} busy={false}
      onConfirm={() => {}} onCancel={() => {}} onSettings={() => {}} />);
    expect(mic).toContain("Open Settings");
  });

  it("offers a confirmation only for an action that has one to give", () => {
    const confirm = render(<Attention item={item({ kind: "CONFIRM", action_id: "a1",
      sentence: "Quit Notes?" })} busy={false} onConfirm={() => {}} onCancel={() => {}} onSettings={() => {}} />);
    expect(confirm).toContain("Go ahead");
    expect(confirm).toContain("Not now");

    // An item with no action behind it offers no way to confirm one.
    const orphan = render(<Attention item={item({ kind: "CONFIRM", action_id: null })} busy={false}
      onConfirm={() => {}} onCancel={() => {}} onSettings={() => {}} />);
    expect(orphan).not.toContain("Go ahead");
  });
});

describe("the kill state is unmistakable", () => {
  const running = { system: "RUNNING" };
  const ctxRunning = { runtime: { state: "KUE_RUNNING" } };

  it("shows stopped when either source says stopped", () => {
    expect(isKilled({ system: "STOPPED" }, ctxRunning)).toBe(true);
    expect(isKilled({ system: "RECOVERING" }, ctxRunning)).toBe(true);
    expect(isKilled(running, { runtime: { state: "KUE_KILLED" } })).toBe(true);
    expect(isKilled(running, { runtime: { state: "KUE_RECOVERING" } })).toBe(true);
  });

  it("cannot be hidden by a projection that stopped arriving", () => {
    // The defect this exists for: the projection freezes on its last good
    // value while context events keep coming, and the window goes on showing a
    // running KUE — trust strip included — after it was stopped.
    const frozen = { system: "RUNNING" };
    expect(isKilled(frozen, { runtime: { state: "KUE_KILLED" } })).toBe(true);
  });

  it("does not claim stopped while both sources say otherwise", () => {
    expect(isKilled(running, ctxRunning)).toBe(false);
    expect(isKilled({ system: "PAUSED" }, { runtime: { state: "KUE_PAUSED" } })).toBe(false);
  });

  it("treats a missing source as no evidence either way", () => {
    expect(isKilled(null, ctxRunning)).toBe(false);
    expect(isKilled(null, { runtime: { state: "KUE_KILLED" } })).toBe(true);
  });
});

describe("the capabilities sheet speaks English", () => {
  /** The values core actually serialises (`CapabilityStatus`, SCREAMING_SNAKE_CASE). */
  const SHIPPED = ["REAL", "PARTIAL", "SIMULATED", "PLACEHOLDER", "NOT_IMPLEMENTED"] as const;

  it("has a plain word for every status core can send", () => {
    // The defect this exists for: the map was keyed "Real"/"NotImplemented",
    // so every lookup missed and the sheet printed the enum name itself —
    // "NOT_IMPLEMENTED" — at a person.
    for (const status of SHIPPED) {
      const entry = STATUS_PILL[status];
      expect(entry, `no word for ${status}`).toBeDefined();
      const [, word] = entry;
      expect(word).not.toMatch(/_|^[A-Z]{2,}$/);
    }
  });
});

describe("the storage sheet shows what KUE measured, and nothing it did not", () => {
  const CANDIDATE: Candidate = {
    path: "/Users/x/Downloads/Docker.dmg",
    name: "Docker.dmg",
    area: "Downloads",
    size_said: "571 MB",
    where_said: "~/Downloads",
    category: "INSTALLER",
    evidence: "A .dmg installer of 571 MB, last changed 2026-07-27.",
    reason: "Installers are usually a copy of something already installed.",
    basis: "INFERRED",
    caution: "If the version it installs is no longer published, this copy is the only one.",
    risk: "LOW",
    reversible: true,
    recommended: "REVIEW",
  };

  const VIEW: StorageView = {
    withheld: null,
    can_undo: 0,
    said: {
      headline: "90% of the drive is in use — 51.6 GB free of 494.4 GB.",
      finding: "4.7 GB worth reviewing in Downloads, Desktop, Documents: 20 installers.",
      areas: ["Downloads — 5.6 GB in 9875 files"],
      limits: ["Showing the 120 largest of 804 findings; the totals above count all 804."],
      measured: "Measured just now.",
      cannot: "KUE never deletes anything. It moves files to the Trash, where you can put them back.",
    },
    report: {
      measured_at: 1,
      summary: { volume: { capacity: 494_353_338_368, available: 51_573_960_704, used: 442_779_377_664 }, areas: [] },
      candidates: [CANDIDATE],
      found: 804,
      counts: [{ category: "INSTALLER", files: 20, bytes: 4_700_000_000, heading: "Installers", said: "20 files · 4.7 GB" }],
      reclaimable: 4_700_000_000,
      scanned: 37055,
      truncated: false,
      unreadable: [],
    },
  };

  const render = (view: StorageView | null) =>
    renderToStaticMarkup(<StorageBody view={view} busy={false} failed="" onCheck={() => {}} />);

  it("puts core's sentences on screen and composes none of its own", () => {
    const html = render(VIEW);
    expect(html).toContain("90% of the drive is in use — 51.6 GB free of 494.4 GB.");
    expect(html).toContain("4.7 GB worth reviewing");
    expect(html).toContain("Showing the 120 largest of 804 findings");
    // The window knows the raw byte counts. It must never turn them into a
    // size of its own: one rounding rule lives in core, and this is how it
    // stays there.
    expect(html).not.toMatch(/494353338368|442779377664|4700000000/);
  });

  it("shows each finding's measurement, its reasoning, and what it would cost to be wrong", () => {
    const html = render(VIEW);
    expect(html).toContain(CANDIDATE.evidence);
    expect(html).toContain(CANDIDATE.reason);
    expect(html).toContain(CANDIDATE.caution);
    expect(html).toContain("worked out");
    expect(html).toContain("if I&#x27;m wrong");
  });

  it("offers only the action KUE has, and says what it never does", () => {
    const html = render(VIEW);
    expect(html).toContain("KUE never deletes anything.");
    // Moving is real now; deleting is not, and the sheet must not imply it.
    for (const verb of ["Delete", "Erase", "Empty the Trash", "Remove"]) {
      expect(html).not.toContain(verb);
    }
    expect(html).toContain("Choose what to move");
  });

  it("asks before anything moves, and only for what was chosen", () => {
    const nothing = render(VIEW);
    expect(nothing).toContain("disabled");
    expect(nothing).not.toContain("Touch ID");

    const one = renderToStaticMarkup(
      <StorageBody view={VIEW} busy={false} failed="" onCheck={() => {}}
                   chosen={[CANDIDATE.path]} onChoose={() => {}} onTrash={() => {}} onUndo={() => {}} />);
    expect(one).toContain("Move 1 to the Trash");
    expect(one).toContain("KUE will ask you to confirm, and macOS will ask for Touch ID, before anything moves.");
  });

  it("offers to put back what KUE moved, and only when there is something to put back", () => {
    expect(render(VIEW)).not.toContain("Put back");
    expect(render({ ...VIEW, can_undo: 3 })).toContain("Put back the 3 I moved");
  });

  it("says why there is nothing rather than showing an empty sheet", () => {
    expect(render({ report: null, said: null, withheld: "KILLED", can_undo: 0 }))
      .toContain("KUE is stopped. It is reading nothing about this Mac.");
    expect(render({ report: null, said: null, withheld: "AUTHORIZATION_REQUIRED", can_undo: 0 }))
      .toContain("Face the camera, or use Touch ID");
    // No report yet: an offer to look, not a blank.
    expect(render({ report: null, said: null, withheld: null, can_undo: 0 })).toContain("Take a look");
  });

  it("shows no findings when core sent a report without its sentences", () => {
    // The projection is the authority. A report whose sentences are missing is
    // a projection the window cannot narrate, so it narrates nothing.
    const html = render({ ...VIEW, said: null });
    expect(html).not.toContain("Docker.dmg");
    expect(html).toContain("Take a look");
  });
});

describe("how the measuring is going", () => {
  const HEALTH = {
    ts: 100, capture_running: true, vision_busy: true, loop_alive: true,
    last_capture_at: 99, last_analyzed_at: 96,
    analyze_ms_last: 2810, analyze_ms_p50: 44, analyze_ms_max: 3120,
    capture_gap_ms_max: 3100, frames_captured: 412, frames_analyzed: 370, frames_dropped: 42,
  };

  const render = (r: PerceptionReport) => renderToStaticMarkup(<Perception report={r} />);

  it("names a delay as a delay, and never as uncertainty about a person", () => {
    const html = render({
      measurement: "MEASUREMENT_DELAYED",
      measurementSaid: "The camera is working, but the last reading is late.",
      pipeline: HEALTH, modelPhase: "MODEL_PREFILL", stages: [],
    });
    expect(html).toContain("MEASUREMENT DELAYED");
    expect(html).toContain("The camera is working, but the last reading is late.");
    expect(html).toContain("model prefill");
    expect(html).not.toContain("UNCERTAIN");
    // The measured facts behind the claim are shown, not asserted.
    expect(html).toContain("2810 ms last");
    expect(html).toContain("42 superseded before analysis");
    expect(html).toContain("longest gap 3100 ms");
  });

  it("shows nothing rather than a zero when nothing has been measured", () => {
    const html = render({ measurement: "CAMERA_PAUSED", measurementSaid: "Paused, so nothing is being measured.", pipeline: null });
    expect(html).toContain("The sensing layer has not reported its pipeline yet.");
    expect(html).not.toContain("0 ms");
    expect(html).not.toContain("Stages, since this launch");
  });

  it("reports what local memory costs, including the space it is holding empty", () => {
    const html = render({
      measurement: "MEASUREMENT_FRESH", pipeline: HEALTH,
      store: { events: 8937, snapshots: 5000, perception_samples: 120, stage_timings: 44,
               ledger_rows: 41, file_bytes: 61_140_992, reusable_bytes: 47_845_376 },
    });
    expect(html).toContain("61 MB");
    expect(html).toContain("120 perception samples");
    expect(html).toContain("48 MB of the file is free space it will reuse");
  });
});

describe("what KUE is doing", () => {
  it("projects the core's state and conversation, and composes none of its own", () => {
    const html = renderToStaticMarkup(<Runtime report={{
      state: "WAITING_FOR_USER",
      active: { request_id: "r1", state: "WAITING_FOR_USER", decision: "PLAN", transport: "VOICE",
                goal_id: "g1", tool_execution_id: null, verification_id: null },
      open: { kind: "SELECTION", facets: [{ kind: "STORAGE", value: "INSTALLER" }] },
      turns: [
        { kind: "USER_SPEECH", said: "Clean up my storage.", at: 1, request_id: "r1" },
        { kind: "KUE_SUGGESTION", said: "3 files are worth reviewing.", at: 2, request_id: "r1" },
      ],
    }} />);
    expect(html).toContain("WAITING FOR USER");
    expect(html).toContain("decided to plan");
    expect(html).toContain("Waiting for you: selection (installer)");
    expect(html).toContain(">You<");
    expect(html).toContain("3 files are worth reviewing.");
  });

  it("says plainly when nothing is in progress", () => {
    const html = renderToStaticMarkup(<Runtime report={{ state: "IDLE", active: null, turns: [] }} />);
    expect(html).toContain("No request in progress.");
  });
});

describe("the conversation is the one core keeps", () => {
  const thread = [
    { shown: "USER_TURN", said: "Clean up my storage.", at: 1, request_id: "r1" },
    { shown: "SUGGESTION", said: "I found three files worth reviewing: one installer, one probable duplicate and one old download.", at: 2, request_id: "r1" },
    { shown: "USER_TURN", said: "Leave the installer.", at: 3, request_id: "r1" },
    { shown: "KUE_TURN", said: "Okay. I'll exclude the installer.", at: 4, request_id: "r1" },
    { shown: "UNVERIFIED", said: "I attempted that, but I couldn't verify the result.", at: 5, request_id: "r2" },
  ];

  it("shows each turn with core's label and core's words, in order", () => {
    const html = render(<Thread report={{ thread }} />);
    const order = thread.map((t) => html.indexOf(t.said.replace(/'/g, "&#x27;")));
    expect(order.every((i) => i >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(html).toContain("KUE suggests");
    expect(html).toContain("Not verified");
  });

  it("marks only a verified result as verified", () => {
    expect(render(<Thread report={{ thread }} />)).not.toContain("\u2713");
    const done = [...thread, { shown: "RESULT", said: "Done. Moved 1 file to the Trash.", at: 6, request_id: "r1" }];
    const html = render(<Thread report={{ thread: done }} />);
    expect(html.match(/\u2713/g)?.length).toBe(1);
    expect(html).toContain("Verified");
  });

  it("says what KUE waits for, in core's sentence, and nothing when idle", () => {
    const now = { shown: "WAITING_FOR_SELECTION", said: "Waiting for you — say “do it”, or what to leave out.", at: 7, request_id: "r1" };
    const html = render(<Thread report={{ thread, now }} />);
    expect(html).toContain(now.said);
    expect(html).toContain("Waiting for you");
    expect(render(<Thread report={{}} />)).toBe("");
  });
});

describe("a plan is shown as core read it", () => {
  it("shows the preview lines and says when nothing is approved yet", () => {
    const plan = {
      plan: { plan_id: "g1", state: "WAITING_FOR_OWNER", risk: "HIGH", reversible: true,
              steps: [{ index: 0, step: "INSPECT_STORAGE", state: "SUCCEEDED", risk: "LOW" },
                      { index: 1, step: "MOVE_TO_TRASH", state: "PENDING", risk: "HIGH" }],
              approved_by: [], proposed_by: { by: "KUE", detail: "CLEAN_UP_STORAGE" } },
      preview: { lines: ["I'd do this in two steps: check storage, then move to trash.",
                         "Step 2 changes your files, so macOS will ask for Touch ID right before it."] },
      steps: ["Take stock of storage", "Move 2 files to the Trash"],
      version: 1, revision_of: null, waiting: true,
    };
    const html = render(<Runtime report={{ state: "WAITING_FOR_USER", plan }} />);
    expect(html).toContain("not yet approved");
    expect(html).toContain("macOS will ask for Touch ID");
    expect(html).toContain("2. move to trash — pending");
  });
});

describe("the plan the owner is agreeing to is shown in full", () => {
  const waiting = {
    plan: { plan_id: "p2", state: "PLANNED", risk: "MEDIUM", reversible: false,
            steps: [{ index: 0, step: "CREATE_DIRECTORY", state: "PENDING", risk: "MEDIUM" },
                    { index: 1, step: "CREATE_FILE", state: "PENDING", risk: "MEDIUM" }],
            approved_by: [], proposed_by: { by: "MODEL", detail: "ON_DEVICE_MODEL" } },
    preview: { lines: ["I'd do this in two steps: make a folder in ~/KUE, then write a file in ~/KUE.",
                       "I'll wait for your yes before starting."] },
    steps: ["Create the folder Archive", "Create the file Archive/today.txt (5 characters)"],
    version: 2, revision_of: "p1", waiting: true,
  };

  it("names what each step would really do, not just the tool", () => {
    const html = render(<Plan report={{ state: "WAITING_FOR_USER", plan: waiting }} />);
    // The approval boundary: the owner can see the folder and the file they
    // are agreeing to, not only that a folder and a file are involved.
    expect(html).toContain("Create the folder Archive");
    expect(html).toContain("Create the file Archive/today.txt");
    expect(html).toContain("wait for your yes before starting.");
    expect(html).toContain("Plan — waiting for you");
  });

  it("says it is a version the owner changed, and which plan it came from", () => {
    const html = render(<Plan report={{ state: "WAITING_FOR_USER", plan: waiting }} />);
    expect(html).toContain("version 2");
    expect(html).toContain("you changed p1");
    expect(html).toContain("on device model");
  });

  it("calls a step done only once core says it was read back", () => {
    const states = ["PENDING", "RUNNING", "SUCCEEDED", "VERIFICATION_FAILED", "FAILED"];
    const html = render(<Plan report={{ plan: { ...waiting, waiting: false,
      plan: { ...waiting.plan,
              steps: states.map((state, index) => ({ index, step: "CREATE_FILE", state, risk: "LOW" })) },
      steps: states.map((s) => `Create the file ${s.toLowerCase()}.txt`) } }} />);
    expect(html).toContain("to do");
    expect(html).toContain("doing it now");
    expect(html).toContain("verified");
    expect(html).toContain("not verified");
    // Nothing is ever called done, finished or complete: a step reads as
    // verified because core verified it, or it does not read as done at all.
    expect(html).not.toMatch(/\bdone\b/i);
    expect(html).not.toMatch(/\bcomplete/i);
    expect(html).not.toMatch(/\bfinished\b/i);
  });

  it("shows nothing at all when there is no plan", () => {
    expect(render(<Plan report={{ state: "IDLE" }} />)).toBe("");
  });
});

describe("the owner's conversation is withheld from someone KUE is not sure of", () => {
  it("shows why, and no words, plan or request", () => {
    const html = render(<Runtime report={{ state: "IDLE", withheld: "I need to be sure it's you before we talk. Show your face, or use Touch ID." }} />);
    expect(html).toContain("I need to be sure it");
    expect(html).not.toContain("Plan ");
    expect(render(<Thread report={{ state: "IDLE", withheld: "x" }} />)).toBe("");
  });
});

describe("the window's own characters", () => {
  // Found live 2026-09-22: a backslash-u-2713 escape showed on screen as six characters. The production build
  // leaves a \u escape in JSX text as the six characters; the test build turns
  // it into the character, so no render test can see it. The source is checked.
  it("writes no \\u escape in JSX text", () => {
    const sources = import.meta.glob(["./components/*.tsx", "./App.tsx"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
    expect(Object.keys(sources).length).toBeGreaterThan(5);
    const offending: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      text.split("\n").forEach((line: string, i: number) => {
        const outsideStrings = line.replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`/g, "");
        // JSX text, and a JSX attribute string (title="\u00b7"), which is not a
        // JavaScript string either and shows the escape as written.
        const inAttribute = /[A-Za-z]="[^"]*\\u[0-9a-fA-F]{4}/.test(line);
        if (/\\u[0-9a-fA-F]{4}/.test(outsideStrings) || inAttribute) offending.push(`${file}:${i + 1}`);
      });
    }
    expect(offending).toEqual([]);
  });
});

describe("what KUE keeps, on screen", () => {
  it("shows each memory with why it is kept, and never invents a reason", () => {
    const kept = [
      { id: "m1", class: "PREFERENCE", heading: "How you like things done", statement: "You prefer PDF reports",
        state: "CONFIRMED", why: "you said: “I prefer PDF reports”", when: "2026-09-23", expires: false },
      { id: "m2", class: "TASK_NOTE", heading: "Just for now", statement: "For this clean-up, you asked me to leave out the installers",
        state: "CONFIRMED", why: "you said: “the installers”", when: "2026-09-23", expires: true },
      { id: "m3", class: "FACT", heading: "About you and your work", statement: "You work on the SAP project",
        state: "CONFIRMED", why: "", when: "2026-09-23", expires: false },
    ];
    const html = render(<MemoryList memory={{ visible: true, kept, current: 3, past: 2 }} onForget={() => {}} />);
    expect(html).toContain("You prefer PDF reports");
    expect(html).toContain("you said: “I prefer PDF reports”");
    expect(html).toContain("only for the task in hand");
    // A memory with no provenance says so rather than making one up.
    expect(html).toContain("KUE has no record of why this was kept");
    // What is no longer current is counted, never read back as current.
    expect(html).toContain("2 older");
    expect(html).not.toContain("You prefer DOCX");
  });

  it("shows nothing of what KUE keeps to someone it is not sure of", () => {
    const withheld = "I need to be sure it's you before I show you what I keep. Show your face, or use Touch ID.";
    const html = render(<MemoryList memory={{ visible: false, withheld }} onForget={() => {}} />);
    expect(html).toContain("I need to be sure it");
    expect(html).not.toContain("prefer");
  });
});

describe("when memory changed what KUE proposes", () => {
  const plan = {
    plan: { plan_id: "g1", state: "WAITING_FOR_OWNER", risk: "HIGH", reversible: true,
            steps: [{ index: 0, step: "MOVE_TO_TRASH", state: "PENDING", risk: "HIGH" }],
            approved_by: [], proposed_by: { by: "KUE", detail: "CLEAN_UP_STORAGE" } },
    preview: { lines: ["I'd move to trash."] }, steps: ["Move 12 files to the Trash"],
    version: 1, revision_of: null, waiting: true,
  };

  it("says so in the owner's own words, and shows no ids", () => {
    const html = render(<Plan report={{ state: "WAITING_FOR_USER",
      plan: { ...plan, memory_used: ["You don't want installers included when cleaning your storage"] } }} />);
    expect(html).toContain("KUE used one thing you told it");
    expect(html).toContain("You don&#x27;t want installers included when cleaning your storage");
    expect(html).not.toContain("m1-");
  });

  it("says nothing when memory changed nothing", () => {
    const html = render(<Plan report={{ state: "WAITING_FOR_USER", plan: { ...plan, memory_used: [] } }} />);
    expect(html).not.toContain("you told it");
  });
});
