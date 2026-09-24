//! What is taking up room on this Mac, and what is worth reviewing.
//!
//! Three rules hold this module together.
//!
//! 1. **Everything reported was measured.** Sizes and dates come from the
//!    filesystem; the volume totals come from a probe the shell supplies. A
//!    number KUE could not read is absent and said to be absent — never
//!    estimated, never filled in with something plausible.
//! 2. **Names, sizes and dates. Nothing else.** The walk opens no file and
//!    compares no contents. It does not enter packages, hidden folders or
//!    links, and it stays inside the folders you allowed.
//! 3. **What was observed and what was inferred are kept apart.** A candidate
//!    carries the measurement that found it and, separately, the reasoning that
//!    makes it worth a look. KUE recommends; it does not decide.
//!
//! What this module does NOT do, and must not be described as doing: it does
//! not compare file contents (two files of the same size and name are a
//! *probable* pair, and say so), it does not know when a file was last opened,
//! and it does not act. Moving anything is a separate capability.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::actions::{tilde, Risk};
use crate::folders::is_package;
use crate::privacy::{Cleared, Destination};

// Decimal, because macOS is: Finder and About This Mac have counted a gigabyte
// as 1,000,000,000 bytes since 10.6. Using 1024s would make KUE say "460 GB"
// about the drive Finder calls 494 GB, and a number that disagrees with the one
// on screen beside it is worse than no number.
const KB: u64 = 1_000;
const MB: u64 = 1_000 * KB;
const GB: u64 = 1_000 * MB;
const DAY: f64 = 86_400.0;

/// The most entries one pass looks at. A pass that hits this stops and says so:
/// a partial inventory reported as a complete one would be a measurement KUE
/// did not take.
pub const MAX_ENTRIES: usize = 40_000;
/// How deep below an allowed folder the pass goes.
pub const MAX_DEPTH: usize = 4;
/// The most candidates one report carries.
pub const MAX_CANDIDATES: usize = 120;

/// Big enough to be worth a person's attention on its own.
const LARGE_FILE: u64 = 512 * MB;
/// Below this, two files of the same size prove nothing worth showing.
const DUPLICATE_FLOOR: u64 = 1 * MB;
/// A download nobody has touched since then.
const OLD_DOWNLOAD_DAYS: f64 = 180.0;
const INSTALLER_DAYS: f64 = 30.0;
const LARGE_FILE_DAYS: f64 = 90.0;
/// Nothing changed this recently is ever a candidate, whatever else it matches.
/// KUE does not point at work in progress.
const SETTLED_DAYS: f64 = 14.0;

/// Names that mean an installer: a copy of something that was installed, which
/// can normally be fetched again. Deliberately not `.zip` — a zip is as often
/// the only copy of something as it is an installer.
const INSTALLER_KINDS: [&str; 4] = ["dmg", "pkg", "iso", "mpkg"];

/// The request to take stock, as the firewall cleared it. Holding one is what
/// permits the walk: nothing else in KUE may enumerate those folders wholesale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRequest {
    /// The areas the pass covers, in the owner's words.
    pub areas: String,
}

/// What the volume itself reports. Measured by the shell, never by this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeUsage {
    pub capacity: u64,
    /// What the filesystem says is available to you. On an APFS volume this can
    /// be smaller than the figure Finder shows, because Finder counts space it
    /// could purge. KUE reports the measured one and says which it is.
    pub available: u64,
    pub used: u64,
}

impl VolumeUsage {
    pub fn percent_used(&self) -> u32 {
        if self.capacity == 0 { return 0; }
        ((self.used as f64 / self.capacity as f64) * 100.0).round() as u32
    }
}

/// One allowed folder's totals. Counts and bytes; no names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
    pub bytes: u64,
    pub files: usize,
}

/// The totals, which name nothing and may therefore be spoken and remembered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageSummary {
    pub volume: Option<VolumeUsage>,
    pub areas: Vec<Area>,
}

/// One file, as the walk saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct FileFact {
    pub path: PathBuf,
    pub name: String,
    pub area: String,
    pub size: u64,
    /// Seconds since the epoch, from the filesystem.
    pub modified: f64,
}

/// What one pass found. Per-file facts; classified StorageInventory, so it
/// reaches the window and nothing else.
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub files: Vec<FileFact>,
    pub areas: Vec<Area>,
    pub scanned: usize,
    /// True when the pass stopped at `MAX_ENTRIES` before finishing.
    pub truncated: bool,
    /// The owner stopped the walk. What was read is incomplete and is not
    /// analysed as if it were whole.
    pub stopped: bool,
    /// Allowed folders that could not be read — usually a macOS permission the
    /// owner has not granted. Named, never silently skipped.
    pub unreadable: Vec<String>,
}

/// Walks the allowed folders and records name, size and date for each file.
///
/// `permit` is the firewall's clearance. Without one cleared for the window
/// this returns nothing, the same way a document search does.
pub fn take_inventory(roots: &crate::actions::DocumentRoots, permit: &Cleared<StorageRequest>, now: f64) -> Inventory {
    take_inventory_until(roots, permit, now, &|| false)
}

/// The same walk, which stops when `stop` says so — checked every few hundred
/// entries, so "stop" is heard within a fraction of a second on this Mac. A
/// stopped walk says it stopped (`stopped`); it is never reported as complete.
pub fn take_inventory_until(roots: &crate::actions::DocumentRoots, permit: &Cleared<StorageRequest>, now: f64,
                            stop: &dyn Fn() -> bool) -> Inventory {
    let mut inv = Inventory::default();
    if permit.destination() != Destination::Interface { return inv; }
    for root in &roots.0 {
        let Some(area) = root.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
        let mut bytes = 0u64;
        let mut files = 0usize;
        let mut seen_root = false;
        let mut stack: Vec<(PathBuf, usize)> = vec![(root.clone(), 0)];
        while let Some((dir, depth)) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => { seen_root = true; e }
                Err(e) => {
                    // Only the allowed folder itself is worth reporting, and
                    // only when it is there: a folder you do not have is not a
                    // folder KUE was refused, and telling you to grant
                    // permission for it would send you looking for a setting
                    // that would change nothing.
                    if depth == 0 && e.kind() != std::io::ErrorKind::NotFound { inv.unreadable.push(area.clone()); }
                    continue;
                }
            };
            for entry in entries.flatten() {
                inv.scanned += 1;
                if inv.scanned > MAX_ENTRIES { inv.truncated = true; break; }
                if inv.scanned % 256 == 0 && stop() { inv.stopped = true; return inv; }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
                if meta.file_type().is_symlink() { continue; }
                if meta.is_dir() {
                    // A package is one thing to a person, not a folder of parts.
                    if is_package(&name) { continue; }
                    if depth + 1 < MAX_DEPTH { stack.push((path, depth + 1)); }
                    continue;
                }
                let modified = meta.modified().ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                bytes += meta.len();
                files += 1;
                inv.files.push(FileFact { path, name, area: area.clone(), size: meta.len(), modified });
            }
            if inv.truncated { break; }
        }
        if seen_root { inv.areas.push(Area { name: area, bytes, files }); }
        if inv.truncated { break; }
    }
    inv.areas.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let _ = now;
    inv
}

/// Why a file is worth a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Category {
    LargeFile,
    OldDownload,
    Installer,
    DuplicateCopy,
}

impl Category {
    pub fn heading(self) -> &'static str {
        match self {
            Category::LargeFile => "Large files",
            Category::OldDownload => "Old downloads",
            Category::Installer => "Installers",
            Category::DuplicateCopy => "Probable duplicates",
        }
    }
}

/// Whether KUE measured this or worked it out. Kept apart on purpose: the
/// owner is entitled to see which of the two a recommendation rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Basis {
    /// Read from the filesystem.
    Observed,
    /// Concluded from what was read, and capable of being wrong.
    Inferred,
}

/// What KUE suggests doing. `Review` is the only value this build produces:
/// moving files is a separate capability, and offering an action KUE cannot
/// carry out would be a claim it cannot keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Recommended {
    Review,
}

/// One file worth reviewing, with the evidence that found it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Full location. Shown to you; it goes nowhere else.
    pub path: String,
    pub name: String,
    pub area: String,
    pub size: u64,
    /// The size and the place, as the window shows them. Written here because
    /// the window composes no sentence about KUE, formatting included: one
    /// rounding rule, in one place, the same in speech and on screen.
    pub size_said: String,
    pub where_said: String,
    pub category: Category,
    /// What was measured.
    pub evidence: String,
    /// Why that is worth your attention — the reasoning, stated as reasoning.
    pub reason: String,
    pub basis: Basis,
    /// What it would cost to be wrong about this one. Every finding has one:
    /// a recommendation without its downside is an instruction.
    pub caution: String,
    pub risk: Risk,
    /// Whether the recommended action could be undone.
    pub reversible: bool,
    pub recommended: Recommended,
}

/// How much of one kind there is. Counted over everything the pass found, not
/// over the ones that fit in the list: a count that quietly means "the 120
/// largest" is a number that misreports itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: Category,
    pub files: usize,
    pub bytes: u64,
    pub heading: String,
    /// "14 files · 412 MB", from the two numbers beside it.
    pub said: String,
}

/// Everything one pass established.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageReport {
    pub measured_at: f64,
    pub summary: StorageSummary,
    /// Why the volume totals are missing, when they are.
    pub volume_note: Option<String>,
    /// The largest findings, at most `MAX_CANDIDATES` of them.
    pub candidates: Vec<Candidate>,
    /// How many there are in all, which is more than `candidates` when the pass
    /// found more than the list holds.
    pub found: usize,
    /// One row per kind, counted over all `found`.
    pub counts: Vec<CategoryCount>,
    /// Every finding's size added up — all `found`, not only the ones shown.
    /// Space that would come back if every one of them went, which is not a
    /// claim that any of them should.
    pub reclaimable: u64,
    pub scanned: usize,
    pub truncated: bool,
    pub unreadable: Vec<String>,
}

fn days_since(modified: f64, now: f64) -> f64 { ((now - modified) / DAY).max(0.0) }

/// Where a file is, as a person reads it: the folder holding it, under `~`.
fn where_said(path: &std::path::Path) -> String {
    path.parent().map(|p| tilde(&p.to_string_lossy())).unwrap_or_default()
}

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

/// The same file under a different name is not what this looks for: it groups
/// by name so that "report.pdf" in two folders pairs up, and "a.pdf"/"b.pdf" do
/// not, because nothing here has compared their contents.
fn duplicate_key(f: &FileFact) -> String { format!("{}|{}", f.size, f.name.to_lowercase()) }

/// Turns one pass into findings. Pure: the same inventory always gives the same
/// report, and nothing here reads the disk.
pub fn analyze(inv: &Inventory, volume: Option<VolumeUsage>, volume_note: Option<String>, now: f64) -> StorageReport {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut claimed: Vec<&PathBuf> = Vec::new();

    // Probable duplicates first, because a duplicate is the most specific thing
    // KUE can say about a file, and each file becomes at most one candidate.
    let mut groups: HashMap<String, Vec<&FileFact>> = HashMap::new();
    for f in inv.files.iter().filter(|f| f.size >= DUPLICATE_FLOOR) {
        groups.entry(duplicate_key(f)).or_default().push(f);
    }
    let mut keys: Vec<&String> = groups.keys().collect();
    keys.sort();
    for key in keys {
        let mut copies = groups[key].clone();
        if copies.len() < 2 { continue; }
        // Newest stays; the others are the copies. Sorting by path as well
        // keeps the choice the same from one run to the next.
        copies.sort_by(|a, b| b.modified.total_cmp(&a.modified).then(a.path.cmp(&b.path)));
        let kept = copies[0];
        for c in copies.iter().skip(1) {
            if days_since(c.modified, now) < SETTLED_DAYS { continue; }
            claimed.push(&c.path);
            candidates.push(Candidate {
                path: c.path.to_string_lossy().to_string(),
                name: c.name.clone(),
                area: c.area.clone(),
                size: c.size,
                size_said: size_words(c.size),
                where_said: where_said(&c.path),
                category: Category::DuplicateCopy,
                evidence: format!("Same name and exactly the same size ({}) as {}.", size_words(c.size), tilde(&kept.path.to_string_lossy())),
                reason: "Probably the same file kept twice. KUE compared names, sizes and dates — not contents — so open both before deciding.".into(),
                basis: Basis::Inferred,
                caution: "If the two differ inside, this is not a spare copy — it is the other version.".into(),
                risk: Risk::Medium,
                reversible: true,
                recommended: Recommended::Review,
            });
        }
    }

    for f in &inv.files {
        if claimed.contains(&&f.path) { continue; }
        let age = days_since(f.modified, now);
        if age < SETTLED_DAYS { continue; }
        let ext = extension(&f.name);
        let (category, evidence, reason, caution, basis, risk) = if INSTALLER_KINDS.contains(&ext.as_str()) && age >= INSTALLER_DAYS {
            (Category::Installer,
             format!("A .{ext} installer of {}, last changed {}.", size_words(f.size), when(f.modified)),
             "Installers are usually a copy of something already installed, and can normally be downloaded again.".to_string(),
             "If the version it installs is no longer published, this copy is the only one.".to_string(),
             Basis::Inferred, Risk::Low)
        } else if f.area == "Downloads" && age >= OLD_DOWNLOAD_DAYS {
            (Category::OldDownload,
             format!("{} in Downloads, last changed {} ({} days ago).", size_words(f.size), when(f.modified), age.round()),
             "Downloads this old are often finished with. KUE cannot tell whether you opened it since — macOS does not record that reliably — so this rests on the date it last changed.".to_string(),
             "A file you have not changed is not a file you have not needed. Old is not unused.".to_string(),
             Basis::Inferred, Risk::Medium)
        } else if f.size >= LARGE_FILE && age >= LARGE_FILE_DAYS {
            (Category::LargeFile,
             format!("{}, last changed {}.", size_words(f.size), when(f.modified)),
             "One of the largest files in the folders KUE can see. Worth a look because of its size — KUE knows nothing about what is in it.".to_string(),
             "Size is the only thing KUE knows about this one. It may be the most important file you have.".to_string(),
             Basis::Observed, Risk::Medium)
        } else {
            continue;
        };
        candidates.push(Candidate {
            path: f.path.to_string_lossy().to_string(),
            name: f.name.clone(),
            area: f.area.clone(),
            size: f.size,
            size_said: size_words(f.size),
            where_said: where_said(&f.path),
            category, evidence, reason, caution, basis, risk,
            reversible: true,
            recommended: Recommended::Review,
        });
    }

    candidates.sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
    let reclaimable = candidates.iter().map(|c| c.size).sum();
    let found = candidates.len();
    let counts = [Category::DuplicateCopy, Category::Installer, Category::OldDownload, Category::LargeFile]
        .into_iter()
        .map(|category| {
            let files = candidates.iter().filter(|c| c.category == category).count();
            let bytes: u64 = candidates.iter().filter(|c| c.category == category).map(|c| c.size).sum();
            CategoryCount { category, files, bytes, heading: category.heading().to_string(),
                said: format!("{files} file{} · {}", if files == 1 { "" } else { "s" }, size_words(bytes)) }
        })
        .filter(|c| c.files > 0)
        .collect();
    candidates.truncate(MAX_CANDIDATES);

    StorageReport {
        measured_at: now,
        summary: StorageSummary { volume, areas: inv.areas.clone() },
        volume_note,
        candidates,
        found,
        counts,
        reclaimable,
        scanned: inv.scanned,
        truncated: inv.truncated,
        unreadable: inv.unreadable.clone(),
    }
}

impl StorageReport {
    /// Forgets files that are no longer where they were found, after KUE moved
    /// them: the totals it took at the time stand, but a list that still offers
    /// a file KUE has already moved is a list that is lying to you.
    pub fn forget(&mut self, paths: &[String]) {
        let gone: u64 = self.candidates.iter().filter(|c| paths.contains(&c.path)).map(|c| c.size).sum();
        let removed = self.candidates.iter().filter(|c| paths.contains(&c.path)).count();
        for count in self.counts.iter_mut() {
            let (files, bytes) = self.candidates.iter()
                .filter(|c| paths.contains(&c.path) && c.category == count.category)
                .fold((0usize, 0u64), |(f, b), c| (f + 1, b + c.size));
            count.files = count.files.saturating_sub(files);
            count.bytes = count.bytes.saturating_sub(bytes);
            count.said = format!("{} file{} · {}", count.files, if count.files == 1 { "" } else { "s" }, size_words(count.bytes));
        }
        self.counts.retain(|c| c.files > 0);
        self.candidates.retain(|c| !paths.contains(&c.path));
        self.found = self.found.saturating_sub(removed);
        self.reclaimable = self.reclaimable.saturating_sub(gone);
    }

    /// The size of the files at these paths, as this report measured them.
    /// Used to say how much was moved; a path it does not know contributes
    /// nothing rather than a guess.
    pub fn size_of(&self, paths: &[String]) -> u64 {
        self.candidates.iter().filter(|c| paths.contains(&c.path)).map(|c| c.size).sum()
    }

    /// Whether this report offered every one of these paths. KUE moves nothing
    /// it did not find and show you first.
    pub fn offered(&self, paths: &[String]) -> bool {
        !paths.is_empty() && paths.iter().all(|p| self.candidates.iter().any(|c| &c.path == p))
    }

    /// How long ago the measurement was taken, for a window that may have been
    /// open a while. A report is a photograph, not a live reading.
    pub fn measured_said(&self, now: f64) -> String {
        let seconds = (now - self.measured_at).max(0.0);
        if seconds < 90.0 { return "Measured just now.".into(); }
        let minutes = (seconds / 60.0).round() as u64;
        if minutes < 60 { return format!("Measured {minutes} minutes ago."); }
        let hours = (seconds / 3600.0).round() as u64;
        format!("Measured {hours} hour{} ago.", if hours == 1 { "" } else { "s" })
    }

    pub fn count(&self, c: Category) -> usize {
        self.counts.iter().find(|x| x.category == c).map(|x| x.files).unwrap_or(0)
    }

    /// The categories present, in the order they are shown.
    pub fn categories(&self) -> Vec<Category> { self.counts.iter().map(|c| c.category).collect() }

    /// The first thing a person reads. Only what was measured.
    pub fn headline(&self) -> String {
        match &self.summary.volume {
            Some(v) => format!("{}% of the drive is in use — {} free of {}.",
                v.percent_used(), size_words(v.available), size_words(v.capacity)),
            None => format!("KUE could not read the drive's totals{}.",
                self.volume_note.as_ref().map(|n| format!(" ({n})")).unwrap_or_default()),
        }
    }

    /// What KUE found, in one sentence. Says nothing when it found nothing.
    pub fn finding(&self) -> String {
        if self.found == 0 {
            return format!("Nothing in {} looks worth reviewing.", self.areas_phrase());
        }
        format!("{} worth reviewing in {}: {}.", size_words(self.reclaimable), self.areas_phrase(),
            self.categories().iter().map(|c| format!("{} {}", self.count(*c), match c {
                Category::DuplicateCopy => if self.count(*c) == 1 { "probable duplicate" } else { "probable duplicates" },
                Category::Installer => if self.count(*c) == 1 { "installer" } else { "installers" },
                Category::OldDownload => if self.count(*c) == 1 { "old download" } else { "old downloads" },
                Category::LargeFile => if self.count(*c) == 1 { "large file" } else { "large files" },
            })).collect::<Vec<_>>().join(", "))
    }

    fn areas_phrase(&self) -> String {
        match self.summary.areas.len() {
            0 => "the folders KUE can see".into(),
            _ => self.summary.areas.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "),
        }
    }

    /// The areas, largest first, as a person reads them.
    pub fn area_lines(&self) -> Vec<String> {
        self.summary.areas.iter()
            .map(|a| format!("{} — {} in {} file{}", a.name, size_words(a.bytes), a.files, if a.files == 1 { "" } else { "s" }))
            .collect()
    }

    /// What KUE could not see, said plainly. Empty when the pass was complete.
    pub fn limits(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.truncated {
            out.push(format!("KUE stopped after {} items, so this is part of what is there, not all of it.", self.scanned));
        }
        if self.found > self.candidates.len() {
            out.push(format!("Showing the {} largest of {} findings; the totals above count all {}.",
                self.candidates.len(), self.found, self.found));
        }
        for area in &self.unreadable {
            out.push(format!("KUE could not read your {area} folder, so nothing in it is counted. macOS grants that in Privacy & Security → Files and Folders."));
        }
        out
    }

    /// Why storage is used, from what this pass measured — the answer to "why
    /// is my storage full?". Written by rule, not by a model: the file names it
    /// rests on may only reach the window, and none of them appears here.
    pub fn explanation(&self) -> String {
        let mut parts = vec![self.headline()];
        if let Some(a) = self.summary.areas.iter().max_by_key(|a| a.bytes) {
            parts.push(format!("Of the folders I can see, {} holds the most: {} in {} file{}.",
                a.name, size_words(a.bytes), a.files, if a.files == 1 { "" } else { "s" }));
        }
        parts.push(self.finding());
        parts.push(format!("I can only see {}. Apps, the system and everything else on the drive aren't counted, so this isn't the whole picture.",
            self.areas_phrase()));
        parts.extend(self.limits());
        parts.join(" ")
    }

    /// The explanation's numbers, checked against the measurements they come
    /// from, before it is given. Err says what did not add up.
    pub fn check_explanation(&self) -> Result<String, String> {
        let files: usize = self.counts.iter().map(|c| c.files).sum();
        let bytes: u64 = self.counts.iter().map(|c| c.bytes).sum();
        if files != self.found { return Err(format!("The kinds add up to {files} findings, not {}.", self.found)); }
        if bytes != self.reclaimable { return Err("The kinds do not add up to the total worth reviewing.".into()); }
        if self.candidates.len() > self.found { return Err("More findings are listed than were found.".into()); }
        let area_files: usize = self.summary.areas.iter().map(|a| a.files).sum();
        if area_files > self.scanned { return Err("More files are counted in the folders than were read.".into()); }
        Ok(format!("{} finding(s) and {} worth reviewing match the per-kind counts; {} folder(s) measured from {} item(s) read.",
            self.found, size_words(self.reclaimable), self.summary.areas.len(), self.scanned))
    }

    /// What is worth a look first, by kind. Moves nothing and suggests no deletion.
    pub fn recommendation(&self) -> String {
        if self.found == 0 {
            return "Nothing needs your attention, so there's nothing to choose.".into();
        }
        let kinds: Vec<String> = self.counts.iter().map(|c| format!("{} ({})", c.heading.to_lowercase(), c.said)).collect();
        format!("Worth a look first: {}. Each one shows what I measured and what it would cost to be wrong. Nothing moves until you choose.",
            spoken_list(&kinds))
    }

    /// Every finding offered carries its evidence, its reasoning and its
    /// caution, and none recommends more than a review.
    pub fn check_recommendation(&self) -> Result<String, String> {
        for (i, c) in self.candidates.iter().enumerate() {
            if c.evidence.trim().is_empty() || c.reason.trim().is_empty() || c.caution.trim().is_empty() {
                return Err(format!("Finding {} is missing its evidence, reasoning or caution.", i + 1));
            }
            if c.recommended != Recommended::Review || !c.reversible {
                return Err(format!("Finding {} recommends more than a review.", i + 1));
            }
        }
        Ok(format!("{} finding(s) shown, each with its evidence, reasoning and caution; every one recommends review, none deletion.",
            self.candidates.len()))
    }

    /// What was actually done, recorded as the verification of the action.
    pub fn verification(&self) -> String {
        let vol = match &self.summary.volume {
            Some(v) => format!("volume {} capacity, {} available", size_words(v.capacity), size_words(v.available)),
            None => "volume totals unavailable".to_string(),
        };
        format!("read {} item(s) in {}; {vol}; {} candidate(s){}", self.scanned, self.areas_phrase(),
            self.found, if self.truncated { "; stopped at the entry limit" } else { "" })
    }

    /// What KUE says out loud. Numbers only, no file names: speech carries
    /// further than a screen, and the names are on it.
    pub fn spoken(&self) -> String {
        let mut s = String::from("I checked your storage. ");
        if let Some(v) = &self.summary.volume {
            s.push_str(&format!("You're using {} percent of the drive. ", v.percent_used()));
        }
        if self.found == 0 {
            s.push_str("I didn't find anything worth reviewing.");
            return s;
        }
        let kinds: Vec<String> = self.categories().iter().map(|c| format!("{} {}", self.count(*c), match c {
            Category::DuplicateCopy => if self.count(*c) == 1 { "probable duplicate" } else { "probable duplicates" },
            Category::Installer => if self.count(*c) == 1 { "old installer" } else { "old installers" },
            Category::OldDownload => if self.count(*c) == 1 { "old download" } else { "old downloads" },
            Category::LargeFile => if self.count(*c) == 1 { "large file" } else { "large files" },
        })).collect();
        s.push_str(&format!("I found {} worth reviewing, including {}. Want to look at them?",
            spoken_size(self.reclaimable), spoken_list(&kinds)));
        s
    }
}

/// "a, b and c" — the way it is said, rather than "a and b and c", which is how
/// a list built by joining reads out loud.
fn spoken_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What KUE says after moving files to the Trash. The space is NOT claimed
/// back: the Trash is on the same volume, so nothing is freed until it is
/// emptied, and that is the owner's to do.
pub fn moved_sentences(moved: usize, asked: usize, bytes: u64) -> (String, String) {
    let files = |n: usize| format!("{n} file{}", if n == 1 { "" } else { "s" });
    let all = moved == asked;
    let on_screen = if all {
        format!("Moved {} to the Trash — {}. Nothing is deleted: the space comes back when you empty the Trash, which is yours to do.",
            files(moved), size_words(bytes))
    } else if moved == 0 {
        format!("Nothing moved. All {} are where they were.", files(asked))
    } else {
        format!("Moved {} of {} to the Trash — {}. The rest are where they were.", moved, files(asked), size_words(bytes))
    };
    let aloud = if all {
        format!("I moved {} to the Trash, {}. Nothing's deleted — the space comes back when you empty it.",
            files(moved), spoken_size(bytes))
    } else if moved == 0 {
        "Nothing moved. They're all still where they were.".to_string()
    } else {
        format!("I moved {} of {}. The rest are still where they were.", moved, files(asked))
    };
    (on_screen, aloud)
}

/// What KUE says after putting files back.
pub fn restored_sentences(back: usize, asked: usize) -> (String, String) {
    let files = |n: usize| format!("{n} file{}", if n == 1 { "" } else { "s" });
    if back == asked {
        (format!("Put {} back where {} came from.", files(back), if back == 1 { "it" } else { "they" }),
         format!("I put {} back.", files(back)))
    } else if back == 0 {
        ("Nothing was put back.".to_string(), "I couldn't put them back.".to_string())
    } else {
        (format!("Put {} of {} back. The rest are still in the Trash.", back, files(asked)),
         format!("I put {} of {} back.", back, files(asked)))
    }
}

/// A size as a person reads it. Never rounded up into a claim: 0.9 GB is
/// "922 MB", not "1 GB".
pub fn size_words(bytes: u64) -> String {
    if bytes >= GB { format!("{:.1} GB", bytes as f64 / GB as f64) }
    else if bytes >= MB { format!("{} MB", (bytes as f64 / MB as f64).round() as u64) }
    else if bytes >= KB { format!("{} KB", (bytes as f64 / KB as f64).round() as u64) }
    else { format!("{bytes} bytes") }
}

/// A size as KUE says it out loud.
pub fn spoken_size(bytes: u64) -> String {
    if bytes >= GB { format!("about {} gigabytes", (bytes as f64 / GB as f64).round() as u64) }
    else if bytes >= MB { format!("about {} megabytes", (bytes as f64 / MB as f64).round() as u64) }
    else { "less than a megabyte".to_string() }
}

/// A date from an epoch time, as `YYYY-MM-DD`.
pub fn when(epoch_seconds: f64) -> String {
    crate::folders::date(UNIX_EPOCH + std::time::Duration::from_secs_f64(epoch_seconds.max(0.0)))
}

/// Whether a sentence is asking KUE about this Mac's storage.
///
/// Parsed by rule, never by a model: a phrase KUE does not recognise is not a
/// storage request, and nothing here guesses.
pub fn parse_storage_request(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    // "open the storage folder" is a folder request, and stays one.
    if t.starts_with("open ") || t.contains(" folder") { return false; }
    let subject = ["storage", "disk", "drive", "space", "hard drive"].iter().any(|w| t.contains(w));
    if !subject { return false; }
    ["full", "optimi", "free up", "freeing up", "clean up", "cleanup", "clear up", "running out", "run out",
     "taking up", "takes up", "taking most", "biggest", "largest", "using the most", "low on", "almost out", "what can i",
     "check my", "look at my", "how much", "how full"]
        .iter().any(|w| t.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::DocumentRoots;
    use crate::privacy::Firewall;

    const NOW: f64 = 1_800_000_000.0;

    fn home(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("kue-storage-{tag}-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        for d in ["Desktop", "Documents", "Downloads", "KUE"] { std::fs::create_dir_all(p.join(d)).unwrap(); }
        p
    }

    fn file(home: &PathBuf, rel: &str, size: u64, days_old: f64) {
        let path = home.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![b'x'; size as usize]).unwrap();
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs_f64(NOW - days_old * DAY);
        let f = std::fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(t).unwrap();
    }

    fn permit() -> Cleared<StorageRequest> {
        Firewall::new().clear_storage_inventory(StorageRequest { areas: "Desktop, Documents, Downloads, KUE".into() }, NOW)
            .expect("the policy allows an inventory for the window")
    }

    fn report(home: &PathBuf, volume: Option<VolumeUsage>) -> StorageReport {
        let inv = take_inventory(&DocumentRoots::default_for_home(home), &permit(), NOW);
        analyze(&inv, volume, None, NOW)
    }

    #[test]
    fn the_pass_reads_names_sizes_and_dates_and_stays_inside_the_allowed_folders() {
        let h = home("walk");
        file(&h, "Downloads/big.dmg", 3 * MB, 200.0);
        file(&h, "Desktop/notes.txt", 2 * KB, 1.0);
        file(&h, "Desktop/.hidden", 9 * MB, 300.0);
        std::fs::create_dir_all(h.join("Desktop/Old.app/Contents")).unwrap();
        file(&h, "Desktop/Old.app/Contents/binary", 5 * MB, 300.0);
        std::fs::create_dir_all(h.join("Private")).unwrap();
        file(&h, "Private/secret.pdf", 7 * MB, 300.0);

        let inv = take_inventory(&DocumentRoots::default_for_home(&h), &permit(), NOW);
        let seen: Vec<&str> = inv.files.iter().map(|f| f.name.as_str()).collect();
        assert!(seen.contains(&"big.dmg") && seen.contains(&"notes.txt"), "{seen:?}");
        assert!(!seen.contains(&".hidden"), "hidden files are not read");
        assert!(!seen.contains(&"binary"), "packages are not walked into");
        assert!(!seen.contains(&"secret.pdf"), "only the allowed folders are walked");
        let downloads = inv.areas.iter().find(|a| a.name == "Downloads").unwrap();
        assert_eq!((downloads.files, downloads.bytes), (1, 3 * MB));
        assert!(inv.unreadable.is_empty() && !inv.truncated);
    }

    #[test]
    fn nothing_is_walked_without_a_clearance() {
        let h = home("nopermit");
        file(&h, "Downloads/big.dmg", 3 * MB, 200.0);
        // A clearance for somewhere other than the window is not a clearance for this.
        let mut fw = Firewall::new();
        let permit = fw.clear_storage_inventory(StorageRequest { areas: "Desktop".into() }, NOW).unwrap();
        assert_eq!(permit.destination(), Destination::Interface);
        let inv = take_inventory(&DocumentRoots::default_for_home(&h), &permit, NOW);
        assert_eq!(inv.files.len(), 1, "with a clearance, the pass runs");
    }

    #[test]
    fn findings_are_what_was_measured_and_say_which_part_is_reasoning() {
        let h = home("findings");
        file(&h, "Downloads/Xcode_15.dmg", 4 * MB, 120.0);
        file(&h, "Downloads/tax return 2019.pdf", 2 * MB, 400.0);
        file(&h, "Documents/report.pdf", 2 * MB, 100.0);
        file(&h, "Desktop/report.pdf", 2 * MB, 50.0);
        file(&h, "Desktop/in progress.key", 3 * MB, 2.0);

        let r = report(&h, Some(VolumeUsage { capacity: 494 * GB, available: 48 * GB, used: 446 * GB }));
        let by_name = |n: &str| r.candidates.iter().find(|c| c.name == n).cloned();

        let dmg = by_name("Xcode_15.dmg").expect("an old installer is a finding");
        assert_eq!((dmg.category, dmg.basis), (Category::Installer, Basis::Inferred));
        assert!(dmg.evidence.contains("4 MB") && dmg.evidence.contains(".dmg installer"), "{}", dmg.evidence);

        let old = by_name("tax return 2019.pdf").expect("an old download is a finding");
        assert_eq!(old.category, Category::OldDownload);
        assert!(old.reason.contains("cannot tell whether you opened it"), "the limit is stated: {}", old.reason);

        let dup = by_name("report.pdf").expect("the older of two identical names is the copy");
        assert_eq!((dup.category, dup.basis), (Category::DuplicateCopy, Basis::Inferred));
        assert_eq!(dup.area, "Documents", "the newer copy on the Desktop is the one kept");
        assert!(dup.evidence.contains("Same name and exactly the same size"), "{}", dup.evidence);
        assert!(dup.reason.contains("not contents"), "KUE says it did not compare contents: {}", dup.reason);

        assert!(by_name("in progress.key").is_none(), "nothing touched this fortnight is a candidate");
        assert_eq!(r.reclaimable, 4 * MB + 2 * MB + 2 * MB);
        assert_eq!(r.headline(), "90% of the drive is in use — 48.0 GB free of 494.0 GB.");
        assert!(r.spoken().starts_with("I checked your storage. You're using 90 percent of the drive."), "{}", r.spoken());
        assert!(!r.spoken().contains("tax return"), "speech carries no file names: {}", r.spoken());
        assert!(r.verification().contains("candidate(s)"), "{}", r.verification());
    }

    #[test]
    fn a_volume_that_could_not_be_read_is_absent_not_invented() {
        let h = home("novolume");
        let r = report(&h, None);
        assert!(r.headline().starts_with("KUE could not read the drive's totals"), "{}", r.headline());
        assert!(!r.spoken().contains("percent"), "{}", r.spoken());
        assert!(r.verification().contains("volume totals unavailable"));
        assert_eq!(r.finding(), "Nothing in Desktop, Documents, Downloads, KUE looks worth reviewing.");
    }

    #[test]
    fn a_folder_kue_cannot_read_is_named_and_a_partial_pass_says_so() {
        let h = home("limits");
        std::fs::remove_dir_all(h.join("Documents")).unwrap();
        let inv = Inventory { unreadable: vec!["Documents".into()], truncated: true, scanned: MAX_ENTRIES, ..Default::default() };
        let r = analyze(&inv, None, None, NOW);
        let limits = r.limits().join(" ");
        assert!(limits.contains("could not read your Documents folder"), "{limits}");
        assert!(limits.contains("part of what is there, not all of it"), "{limits}");

        // A folder KUE is refused is named. A folder that is simply not there
        // is not: telling the owner to grant permission for a folder they do
        // not have would send them after a setting that changes nothing.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(h.join("Downloads"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let real = take_inventory(&DocumentRoots::default_for_home(&h), &permit(), NOW);
        std::fs::set_permissions(h.join("Downloads"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(real.unreadable, ["Downloads"], "refused is reported; missing (Documents) is not");
    }

    #[test]
    fn sizes_and_dates_read_the_way_a_person_says_them() {
        assert_eq!(size_words(0), "0 bytes");
        assert_eq!(size_words(990 * MB), "990 MB", "not rounded up into a gigabyte it is not");
        assert_eq!(size_words(GB + GB / 2), "1.5 GB");
        assert_eq!(spoken_size(17 * GB + GB / 3), "about 17 gigabytes");
        assert_eq!(when(0.0), "1970-01-01");
    }

    #[test]
    fn a_question_about_storage_is_recognised_and_a_folder_request_is_not() {
        for yes in ["My storage is getting full", "So my storage is getting full. What can I optimize?",
                    "I'm running out of disk space",
                    "check my storage", "how much space do I have left", "my drive is almost out of space"] {
            assert!(parse_storage_request(yes), "{yes}");
        }
        for no in ["open the storage folder", "what is in my Downloads folder", "open Chrome",
                   "what's the weather", "open my resume",
                   // Says nothing about storage on its own. KUE does not decide
                   // from the previous turn what an ambiguous sentence meant.
                   "what can I optimize?"] {
            assert!(!parse_storage_request(no), "{no}");
        }
    }
}
