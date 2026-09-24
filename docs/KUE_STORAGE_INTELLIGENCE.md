# KUE storage intelligence

What KUE does when the owner says "my storage is getting full", and why each
step is built the way it is.

Status of each part is marked **IMPLEMENTED**, **PARTIAL** or **PLANNED**.
Nothing here describes something that does not exist in the build.

---

## The problem

Asked about storage, KUE used to answer the way anything with a language model
answers: delete unused apps, empty the Trash, check large files. True, useless,
and not a product. The owner already knows what one does about storage; what
they do not know is **what is actually on their Mac**.

So the capability is not advice. It is a measurement, a set of findings drawn
from it by fixed rules, and the evidence for each one.

## The loop

    INSPECT → ANALYSE → FIND PATTERNS → EXPLAIN → RECOMMEND
      → (the owner decides) → [ACT] → [VERIFY] → REPORT

Every step is **IMPLEMENTED**. What is deliberately *not* implemented is
deletion: KUE moves files to the Trash and can put them back, and nothing in
the build empties the Trash or deletes anything. That is its own registry row —
`permanent_deletion`, NOT_IMPLEMENTED, deliberate — and the review sheet states
it in the registry's own words rather than wording of its own.

## 1. Inspect — IMPLEMENTED

`core/src/storage.rs`, `take_inventory`.

**The volume.** `statfs`, through the shell (`measure_volume` in
`src-tauri/src/lib.rs`), never core. Two numbers are kept apart: `available` is
what this user may still write; `used` counts every block in use. Finder's
"available" can be larger than either, because Finder counts space it believes
it could purge — so KUE reports the measurement it took and labels it.

Sizes are **decimal** (1 GB = 1,000,000,000 bytes) because macOS is: a KUE that
said "460 GB" about the drive Finder calls 494 GB would be wrong in the only way
the owner can check.

Verified against `df` on this Mac by
`the_volume_measurement_is_the_one_macos_reports`, which fails if the two ever
disagree by more than a percent of the drive.

**The folders.** One pass over Desktop, Documents, Downloads and `~/KUE` —
the same four the document capabilities already use, which are the four macOS
grants the owner has given. Four levels deep. Never into a package, a hidden
folder or a symlink. It records name, size and modification date; it opens no
file. It stops at 40,000 entries and **says so** when it does.

A folder that cannot be read is named, with the Settings pane that grants it. A
folder that does not exist is not: telling the owner to grant permission for a
folder they do not have would send them after a setting that changes nothing.
(That distinction was a real defect, found by running the pass on this Mac.)

## 2. Analyse — IMPLEMENTED

`analyze` is pure: the same inventory always gives the same report, and nothing
in it touches the disk. No model is involved at any point.

| Finding | Rule | Basis |
|---|---|---|
| Probable duplicate | Same name **and** exactly the same size as another file; the newest copy is kept, the rest are candidates; ≥ 1 MB | Inferred |
| Installer | `.dmg`, `.pkg`, `.iso`, `.mpkg`, unchanged for ≥ 30 days | Inferred |
| Old download | In Downloads, unchanged for ≥ 180 days | Inferred |
| Large file | ≥ 512 MB, unchanged for ≥ 90 days | Observed |

Nothing changed in the last **14 days** is ever a candidate: KUE does not point
at work in progress. `.zip` is deliberately not an installer kind — a zip is as
often the only copy of something as it is a copy of something installed.

**Contents are never compared.** Duplicate detection is stage 1 (size) and
stage 2 (name and date) of the three the brief describes; content hashing is
**PLANNED** and will need its own data kind and its own authorization, because
reading the inside of files in Desktop and Documents is a bigger capability than
reading their names. Until then every duplicate says, in its own text, that
contents were not compared.

macOS does not reliably record when a file was last *opened*, so no finding
rests on that. "Old" means "unchanged since", and the sentence says which.

## 3. Explain — IMPLEMENTED

Every candidate carries four things, and the sheet shows them in this order:

1. **What it is** — name, size, and the folder holding it.
2. **What was measured** — `evidence`, only facts read from the filesystem.
3. **What KUE worked out** — `reason`, labelled *seen* or *worked out*
   (`Basis::Observed` / `Basis::Inferred`).
4. **What it would cost to be wrong** — `caution`. Every finding has one. A
   recommendation without its downside is an instruction.

Counts are counted over **everything found**, not over the hundred and twenty
that fit in the list — a count that quietly means "the largest 120" is a number
that misreports itself. When the list is shorter than the findings, the sheet
says so.

## 4. Recommend — IMPLEMENTED

`Recommended::Review` is the only value a finding carries: KUE offers it for
review, and the owner decides. Nothing is pre-selected, and nothing happens
until they choose.

## 5. Act — IMPLEMENTED

`MOVE_TO_TRASH`, through the Action Broker and `KueAct`, using macOS's own
`trashItem(at:resultingItemURL:)`. Finder's Put Back works afterwards, because
it is the same move Finder makes. `removeItem` appears nowhere in KUE.

Five rules, each enforced in code rather than described:

1. **KUE moves nothing it did not find and show you.** A path the last report
   did not offer is refused in `propose_kind`, before authorization is asked
   for — whatever sends it, including a future model. This is the rule that
   keeps "move these files" from becoming "move any file".

   It is checked **twice**: when the request is made, and again at the moment of
   moving. Between the two sits Touch ID — seconds, or minutes — and in that
   time the owner may have worked on one of those files, which takes it
   straight out of the findings. Checking only at the first would move a file
   KUE would no longer offer. (Found by asking the question; the first
   implementation checked once, and a test now holds the second.)
2. **HIGH risk**: owner session, explicit selection, confirmation, and macOS
   authentication. Reversible, and still the strongest ask KUE has, because
   they are the owner's files.
3. **Bounds**: at most 50 at once; files only, never folders; never a package,
   a link, a hidden file, anything in `~/Library`, or anything outside the four
   allowed folders. Checked in core *and* again in the executor.
4. **One file at a time, each verified on its own** — the original gone, the
   Trash URL present. Where each one landed is recorded (`landed`), because
   macOS renames on a name collision and a guessed name could not be undone.
5. **Never claims the space back.** The Trash is on the same volume: "Moved 2
   files to the Trash — 140 MB. Nothing is deleted: the space comes back when
   you empty the Trash, which is yours to do."

A batch is never reported by its last file. Two of three moving is `FAILED`,
with the one that did not named in the reason and a verification that still
says truthfully what did happen.

If one of the chosen files has moved or been renamed since the report, the
**whole batch is refused** and that file is named — a deliberate choice over
moving the rest: the world changed under the request, so KUE stops and lets the
owner look again rather than performing most of something they asked for once.

`RESTORE_FROM_TRASH` puts every file back exactly where it came from, from the
recorded location, refusing any whose old place is now taken.

After a successful move, the report forgets what moved — a list still offering
a file KUE has already moved is a list that is lying — and the totals it took
at the time stand, with the sheet saying how long ago that was.

## Privacy — IMPLEMENTED

Two new data kinds, and the classification is the enforcement:

| Kind | Class | Interface | Memory | Any model |
|---|---|---|---|---|
| `StorageInventory` — per-file names, locations, sizes, dates | `UserApprovalRequired` | shown | **denied** | **denied** |
| `StorageSummary` — volume totals and area totals; names nothing | `LocalOnly` | shown | allowed | local only |

An inventory is deliberately **not** `ActionTarget`. An ActionTarget is what the
owner named — an app, a link, a document they asked to open. An inventory is
what KUE found by walking folders the owner named nothing in, which is the
gathering `DocumentName` forbids when KUE does it on its own initiative.
Recording that difference as a classification, rather than a paragraph, is what
keeps storage findings out of memory.

The decision matrix is unchanged, so the policy stays **v1**: no datum already
cleared changes meaning, and `Store::legacy_snapshot_count` — which treats every
snapshot below the current version as pre-firewall — keeps its meaning too.

The pass runs only behind a clearance (`clear_storage_inventory`), the report is
re-checked against the firewall on **every** fetch rather than once when it was
made, and `get_storage` hands over nothing while KUE is stopped or below
LEVEL_2.

## Authorization — IMPLEMENTED

`INSPECT_STORAGE` is LOW risk: it reads sizes and dates and changes nothing, so
it runs once the owner is authorized, like a folder listing. It is refused
outright for a stranger at the camera and while KUE is killed, and those
refusals are tested.

Moving files, when it exists, will be HIGH: owner session, explicit selection,
confirmation, and per-item verification.

## Voice — IMPLEMENTED

The spoken sentence is arithmetic over the measured report:

> "I checked your storage. You're using 90 percent of the drive. I found about
> 5 gigabytes worth reviewing, including 14 probable duplicates and 20 old
> installers. Want to look at them?"

It carries **no file names**. Speech reaches further than a screen, and the
names are on the screen. The whole sentence still goes through the existing
speech gate and the firewall like any other.

## What this is measured to cost

On this Mac: 37,055 entries across three folders in **838 ms** through the app's
own transaction, no file opened.

## What has been verified on this Mac

- The volume figures equal what `df` reports (an ordinary test, not opt-in).
- A full pass: 494.4 GB drive, 90% in use, 804 findings, 4.7 GB, in 838 ms.
- The review sheet opened in the running build and **withheld** the report,
  because identity was Locked: *"KUE shows this to you, and only to you."*
- The whole move: KUE made two installers on the Desktop, found them in its own
  report, moved both to the real Trash — macOS renamed one on collision — and
  put both back. Stand-ins, named in the test: identity, and the Touch ID answer.
- **Not verified**: the populated sheet on screen. That needs the owner's face
  or Touch ID, which is theirs to give.

## Known limits

- Only the four allowed folders. Not `~/Library`, not `/Applications`, not any
  other volume. Sizing an app means recursing into packages, and "unused app"
  would need last-used dates macOS does not give.
- Contents are never compared, so duplicates are probable, never proven.
- No last-opened date, so "old" is "unchanged since".
- 40,000 entries per pass, then it stops and says so.
- The list shows the 120 largest findings; the counts cover all of them.
- Nothing is deleted, ever. Emptying the Trash is the owner's, in Finder.
- The owner chooses every file that moves. KUE selects nothing on their behalf,
  and nothing here is automatic.
