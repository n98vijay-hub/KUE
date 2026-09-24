# KUE design system

A small, strict system. Every value here exists because something in
`KUE_UX_ARCHITECTURE.md` needs it; nothing is included for completeness. The
system replaces the 11 ad-hoc font sizes, 24 inline styles and untested colour
choices recorded in `KUE_PRODUCT_UX_AUDIT.md` §P.

Principle: **KUE is an instrument, not an interface.** It is quiet, it is dark,
it does not blink, and nothing on screen is larger than it deserves to be. The
only element allowed to draw the eye is the thing that needs the owner.

---

## 1. Colour

Dark only, deliberately: KUE runs alongside the owner's work, at night, with a
camera watching a face. A bright window is a light source pointed at the person.

Every ratio below is measured against `--bg` `#0a0b0e` with the WCAG 2 formula.

### Surfaces

| Token | Value | Use |
|---|---|---|
| `--bg` | `#0a0b0e` | Window |
| `--bg-raised` | `rgba(255,255,255,0.028)` | Cards, moments, sheets |
| `--bg-raised-2` | `rgba(255,255,255,0.052)` | A card on a sheet; hover |
| `--bg-sunken` | `rgba(0,0,0,0.22)` | Composer, code, disclosure bodies |
| `--hairline` | `rgba(255,255,255,0.072)` | Structural division |
| `--hairline-soft` | `rgba(255,255,255,0.042)` | Division inside a group |
| `--scrim` | `rgba(6,7,9,0.62)` | Behind a sheet |

### Ink — all pass WCAG AA for body text (≥ 4.5:1)

| Token | Value | Ratio | Use |
|---|---|---|---|
| `--ink` | `#e9e6e2` | 15.82:1 | The sentence that matters |
| `--ink-2` | `#9d9892` | 6.88:1 | Supporting sentences, labels |
| `--ink-3` | `#8e8981` | 5.67:1 | Timestamps, captions, meta (**was `#6a6862`, 3.53:1 — failed**) |
| `--ink-4` | `#807b74` | 4.69:1 | The dimmest text permitted (**was `#4a4844`, 2.16:1 — failed**) |

Nothing dimmer than `--ink-4` may carry text. Dimmer values are for borders and
fills only, and `--ink-4` is never used for anything the owner must read to act.

### Meaning

One accent, one job each. The audit found a single warm accent doing four
different jobs; these separate them.

| Token | Value | Ratio | Means |
|---|---|---|---|
| `--presence` | `#f0b46a` | 10.72:1 | KUE is here and attending. Presence only — never a button, never a link |
| `--presence-dim` | `rgba(240,180,106,0.14)` | — | Fill behind a presence element |
| `--affirm` | `#86c79c` | 9.99:1 | Verified success. **Only** with recorded verification |
| `--caution` | `#d4ab6a` | 9.21:1 | Needs the owner; unconfirmed; unknown result |
| `--alert` | `#d98a83` | 7.42:1 | Failed, denied, blocked |
| `--absent` | `#7e8390` | 5.19:1 | Off, unavailable, not implemented |
| `--focus` | `#8ab4f8` | 9.34:1 | Keyboard focus ring. Used for nothing else |

Colour never carries meaning alone (audit §Q): every state also has a word, and
where a shape helps, a distinct glyph. A colour-blind owner and a VoiceOver user
read the same state from the same text.

### Killed

The kill surface inverts the system: `--bg` `#160d0d`, ink `#f0e4e2`, a single
`--alert` rule at 2 px. No other surface uses that background, so "stopped" is
unmistakable at a glance across the room (journey 21).

## 2. Typography

System font only (`-apple-system` → SF Pro Text), because KUE ships no fonts and
must not fetch any. Seven steps, replacing the current eleven.

| Token | Size / line | Weight | Use |
|---|---|---|---|
| `--t-presence` | 22 / 1.30 | 500 | The presence sentence |
| `--t-lead` | 17 / 1.45 | 400 | KUE's answer; the sentence of a moment |
| `--t-body` | 13.5 / 1.55 | 400 | Default |
| `--t-support` | 12.5 / 1.5 | 400 | Context column, descriptions |
| `--t-meta` | 11.5 / 1.45 | 400 | Timestamps, captions |
| `--t-label` | 11 / 1.3 | 600, `0.06em` | Section labels, uppercase |
| `--t-mono` | 12 / 1.5 | 400, SF Mono | Diagnostics only |

Rules: no size below 11 px anywhere (the audit found 9 px); measure caps at
72 characters; numerals `font-variant-numeric: tabular-nums` wherever a value
changes in place, so nothing jitters; monospace never appears in the normal
experience.

## 3. Space

An 8 px base with two half-steps. Six tokens, and no arbitrary pixel values in
components.

`--s-1` 4 · `--s-2` 8 · `--s-3` 12 · `--s-4` 16 · `--s-5` 24 · `--s-6` 32 ·
`--s-7` 48

| Region | Padding |
|---|---|
| Window edge | `--s-5` |
| Moment card | `--s-4` |
| Between moments | `--s-4` |
| Inside a group | `--s-2` |
| Sheet | `--s-5` `--s-5` `--s-6` |
| Title bar | 0 `--s-4` 0 86px (traffic lights) |

Grid: one column for the stream (max 720 px, centred) plus a 320 px context
column with a `--s-6` gutter. Nothing stretches to fill.

## 4. Shape and elevation

`--radius` 14 (cards, sheets) · `--radius-sm` 9 (chips, buttons, inputs) ·
`--radius-pill` 999 (status pills only).

Elevation is border and background, not shadow. One shadow exists in the whole
system — the sheet's `0 18px 48px rgba(0,0,0,0.45)` — because a sheet is the
only thing that floats. Backdrop blur is reserved for the title bar and the
sheet scrim.

## 5. Components

Each component's states are the ones core can actually produce.

**Presence line** — glyph + sentence + optional detail. The glyph is a 7 px dot
in `--presence` when recognised, a hollow 7 px ring in `--ink-3` when uncertain,
and a 7 px dot in `--absent` when nobody is there or KUE is not watching. The
glow (`box-shadow 0 0 10px`) appears only for `RECOGNISED`.

**Trust chip** — icon + word, e.g. `Camera on`, `Mic off`, `Nothing leaves this
Mac`. Never a bare icon; never green-for-good (the camera being *on* is not
good news, it is a fact). Pressing one opens the Trust sheet at that row.

**Moment** — the stream unit. A left gutter (24 px) carries a speaker glyph, the
body carries one `--t-lead` sentence, and below it at most one row of controls
and one disclosure. A moment never contains a table.

**Decision row** — when core reports `REQUIRES_CONFIRMATION`, the moment grows
one row: the question in `--ink`, then the affirmative button, then the
alternative. The affirmative is the only filled button in the system
(`--presence-dim` fill, `--ink` text, 1 px `--presence` border at 40 %). Nothing
else is filled, so the one thing waiting for the owner is the one thing that
looks pressable. Irreversible verbs never auto-focus.

**Disclosure** (`▸ How did this go?`) — a `--t-meta` control that reveals the
record: what was asked, what was authorised, the steps and their states, what
was verified and how, and the action id. Closed by default; its content is
plain sentences, not JSON.

**Sheet** — enters from the right at 420 px (full width below 820 px), over the
scrim, with the title, a close control, and `Esc` bound. Focus moves in on open
and returns to the opener on close. Sheets never nest.

**Composer** — a single-line field that grows to 4 lines, with a push-to-talk
control on the left. The field is `--bg-sunken` with a `--hairline` border,
`--focus` ring on focus. While the microphone is live the border is `--presence`
and the placeholder reads "Listening…" — and that is driven by the sensing
process's own report, never by the click that started it.

**Status pill** — used in Diagnostics and on the Capabilities sheet:
`LIVE_VERIFIED` `--affirm`, `IMPLEMENTED` `--ink-2`, `PARTIAL` `--caution`,
`UNAVAILABLE` `--caution`, `NOT_IMPLEMENTED` `--absent`. Text always present.

**Buttons** — three kinds only: *affirmative* (above), *quiet* (transparent,
`--hairline` border, `--ink-2`), *plain* (text only, `--ink-2`, underline on
hover). Destructive controls (Stop KUE) use `--alert` text on a quiet button and
require the existing two-step confirmation.

## 6. Motion

Motion exists to show causality and continuity, never to entertain, and never to
imply progress that isn't happening (brief §39).

| Purpose | Duration / curve |
|---|---|
| State word change | 120 ms opacity |
| Moment enters the stream | 180 ms, 6 px rise, `cubic-bezier(0.2,0,0,1)` |
| Sheet in / out | 220 / 160 ms, same curve |
| Presence glyph change | 400 ms — slow, because presence changes are meaningful |
| Disclosure open | 160 ms height |

Two indeterminate indicators exist, and both mean "a real process is running and
its duration is unknown": the thinking dot (a 1.6 s opacity pulse on the
presence glyph) and the acting bar (a 2 px indeterminate sweep under the
moment). **No progress bar advances toward a percentage we do not have.** No
skeleton screens: KUE shows nothing rather than a shape pretending to be data.

`@media (prefers-reduced-motion: reduce)` — every animation above becomes an
instant state change; the pulse and sweep become a static `--caution` dot with
the word beside it. This is a hard requirement (the audit found none).

## 7. Accessibility

Non-negotiable, all of it currently missing (audit §Q):

- **Focus** — `:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px }`
  globally, on every interactive element. No `outline: none` anywhere.
- **Keyboard** — every action reachable without a pointer. `Tab` order follows
  the visual order; `⌘K` focuses the composer; `Esc` closes a sheet; `⌘⇧D`
  toggles Diagnostics; `⌥Space` (held) is push-to-talk. Buttons are `<button>`,
  not `div`.
- **Live regions** — the activity line is `aria-live="polite"`; a moment needing
  the owner is `aria-live="assertive"`; the killed surface takes focus and is
  announced once.
- **Names** — every glyph-only control has an `aria-label`; every status glyph
  has text beside it or an accessible name; the presence glyph is
  `role="img"` with the presence sentence as its label.
- **Selection** — the global `user-select: none` is removed. KUE's sentences,
  error text and action ids must be selectable; only the title bar's drag region
  keeps it.
- **Text scaling** — the layout survives 200 % text: all sizes in the type scale
  are `rem`-relative, and no container has a fixed height that clips text.
- **Contrast** — every pair in §1 is measured, not assumed, and a test asserts
  the four ink tokens stay ≥ 4.5:1 against `--bg`.

## 8. Voice of the interface

The words are part of the design system, because in KUE the words *are* the
product.

- First person, present tense, plain: "I recognise you." "I couldn't open it."
- Never claim more than the record supports: "It's open" only after verification;
  otherwise "I can't confirm that finished."
- Never apologise twice, never use exclamation marks, never use "Oops".
- Uncertainty is stated, not softened: "I'm not certain it's you."
- No engineering vocabulary in the normal experience: no `LEVEL_2`, no
  `PRIVACY_DENIED`, no descriptor distances, no policy version numbers.
- Limits are stated at the point of the claim, not in a footnote.
- KUE never describes its own feelings, and never claims to infer the owner's
  from a face.

## 9. Implementation rules

1. Tokens live in `src/tokens.css`; components may use **only** tokens. A CI
   grep fails the build on a raw hex value or a raw px font-size in `src/`.
2. No inline `style` attributes except for values computed at runtime from core
   state (a width, a count) — the audit's 24 static ones all become classes.
3. No component decides what state means. It receives a state from the `surface`
   projection and renders it. Mapping state → colour is one shared table.
4. Diagnostics keeps its current dense look and its existing panels; it is
   allowed to be an instrument. It is not retro-fitted to this system beyond the
   token swap for contrast.
