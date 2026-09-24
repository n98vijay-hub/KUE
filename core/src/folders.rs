//! Folders you name: finding one, opening it in Finder, and saying what is inside.
//!
//!   "Open the Tampa folder on Desktop"      → OPEN_DIRECTORY  query "tampa", scope Desktop
//!   "What resumes are inside?"               → LIST_DIRECTORY  filter "resumes", the folder just used
//!   "What's in the Tampa folder?"            → LIST_DIRECTORY  everything, folder "tampa"
//!
//! Scopes are the document folders and nothing else: USER_DESKTOP, USER_DOCUMENTS,
//! USER_DOWNLOADS and USER_KUE (`~/KUE`). A folder is found by name, at most
//! three levels deep, never through a link, never inside a hidden folder or a
//! package. Listing reads names, kinds and modification dates (READ_METADATA) —
//! never a file's contents — and changes nothing.
//!
//! What is found is ACTION_TARGET data: it passes the privacy firewall before the
//! search runs and before it reaches the window, and never reaches memory or a model.

use crate::actions::{name_matches, DocumentQuery, DocumentRoots};
use crate::privacy::{Cleared, Destination};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The folders a folder request may name as its scope.
pub const SCOPES: [(&str, &str); 4] = [("desktop", "Desktop"), ("documents", "Documents"), ("downloads", "Downloads"), ("kue", "KUE")];

/// Words that say nothing about which folder.
const FOLDER_FILLER: [&str; 12] = ["the", "my", "a", "folder", "folders", "directory", "called", "named", "please", "of", "this", "that"];
/// Words that mean "the folder we were just talking about".
const PRONOUNS: [&str; 5] = ["it", "there", "inside", "that", "this"];

fn fold_words(s: &str) -> Vec<String> {
    s.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>()
        .split_whitespace().map(String::from).collect()
}

/// Splits "the Tampa folder on my Desktop" into ("tampa", Some("Desktop")).
/// Returns None for a phrase that names no folder.
pub fn folder_phrase(rest: &str) -> Option<(String, Option<String>)> {
    let mut words = fold_words(rest);
    let mut scope = None;
    // A trailing "on/in (my/the) desktop (folder)".
    if let Some(i) = words.iter().rposition(|w| w == "on" || w == "in") {
        let tail: Vec<&String> = words[i + 1..].iter().filter(|w| !FOLDER_FILLER.contains(&w.as_str())).collect();
        if let [one] = tail.as_slice() {
            if let Some((_, dir)) = SCOPES.iter().find(|(k, _)| *k == one.as_str()) {
                scope = Some(dir.to_string());
                words.truncate(i);
            }
        }
    }
    let query: Vec<String> = words.into_iter().filter(|w| !FOLDER_FILLER.contains(&w.as_str())).collect();
    // "my Downloads folder" is the scope itself.
    if let ([one], None) = (query.as_slice(), &scope) {
        if let Some((_, dir)) = SCOPES.iter().find(|(k, _)| *k == one.as_str()) { return Some((String::new(), Some(dir.to_string()))); }
    }
    if query.is_empty() {
        // "open my Desktop folder": the scope itself is the folder.
        return scope.map(|s| (String::new(), Some(s)));
    }
    Some((query.join(" "), scope))
}

/// "what resumes are inside the Tampa folder" → Some((filter "resumes", location "the Tampa folder")).
fn listing(text: &str) -> Option<(String, String)> {
    let l = text.to_lowercase().replace('’', "'");
    let l = l.trim();
    for p in ["tell me ", "show me ", "list for me "] {
        if let Some(r) = l.strip_prefix(p) { return listing(r); }
    }
    for p in ["what's inside", "what is inside", "whats inside", "what's in", "what is in", "whats in", "list the contents of", "list everything in"] {
        if let Some(r) = l.strip_prefix(p) {
            if !(r.is_empty() || r.starts_with(' ')) { continue; }
            return Some((String::new(), r.trim().to_string()));
        }
    }
    for (p, joins) in [("what ", [" are inside", " are in", " is inside", " is in"]), ("which ", [" are inside", " are in", " is inside", " is in"])] {
        if let Some(r) = l.strip_prefix(p) {
            for j in joins {
                if let Some(i) = r.find(j) {
                    let tail = &r[i + j.len()..];
                    if !(tail.is_empty() || tail.starts_with(' ')) { continue; }
                    return Some((r[..i].trim().to_string(), tail.trim().to_string()));
                }
            }
        }
    }
    if let Some(r) = l.strip_prefix("list ") {
        for j in [" inside ", " in "] {
            if let Some(i) = r.find(j) { return Some((r[..i].trim_start_matches("the ").trim().to_string(), r[i + j.len()..].trim().to_string())); }
        }
    }
    None
}

/// LIST_DIRECTORY from a sentence, or None. `location` is empty when the sentence
/// points at the folder already in use ("what resumes are inside?").
pub fn parse_listing(text: &str) -> Option<(String, String, Option<String>)> {
    let (filter, location) = listing(text.trim().trim_end_matches(['.', '?', '!']))?;
    let filter = fold_words(&filter).into_iter().filter(|w| !["the", "my", "any", "all", "files", "file", "documents", "document", "items", "things"].contains(&w.as_str()))
        .collect::<Vec<_>>().join(" ");
    let loc_words = fold_words(&location);
    let points_back = loc_words.iter().all(|w| PRONOUNS.contains(&w.as_str()) || FOLDER_FILLER.contains(&w.as_str()));
    if points_back { return Some((String::new(), filter, None)); }
    // "what is in front of the camera" is a question, not a folder: a location must
    // say "folder" or name a scope, unless the sentence also names what to look for.
    let names_folder = loc_words.iter().any(|w| w == "folder" || w == "directory" || SCOPES.iter().any(|(k, _)| k == w));
    if !names_folder && (filter.is_empty() || loc_words.len() > 3) { return None; }
    let (query, scope) = folder_phrase(&location)?;
    Some((query, filter, scope))
}

/// OPEN_DIRECTORY from a sentence, or None. It must say "folder" (or name a scope):
/// "open Tampa" alone is an app name, resolved as a folder only when no app matches.
pub fn parse_open_folder(text: &str) -> Option<(String, Option<String>)> {
    let l = text.trim().trim_end_matches(['.', '!']).to_lowercase().replace('’', "'");
    let rest = ["open ", "show ", "show me ", "go to ", "reveal "].iter().find_map(|p| l.strip_prefix(p))?;
    let words = fold_words(rest);
    if !words.iter().any(|w| w == "folder" || w == "directory") { return None; }
    folder_phrase(rest)
}

impl DocumentRoots {
    fn scoped(&self, scope: Option<&str>) -> Vec<PathBuf> {
        match scope {
            Some(s) => self.0.iter().filter(|r| r.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(s))).cloned().collect(),
            None => self.0.clone(),
        }
    }

    /// Folders whose name matches every word of `query`, exact names first, then
    /// newest first; at most `limit`. An empty query with a scope is that scope's folder itself.
    pub fn find_folders(&self, query: &Cleared<DocumentQuery>, scope: Option<&str>, limit: usize) -> Vec<PathBuf> {
        const MAX_DEPTH: usize = 3;
        const MAX_SCANNED: usize = 20_000;
        if query.destination() != Destination::Interface { return Vec::new(); }
        let roots = self.scoped(scope);
        let q = query.value().as_str().trim().to_lowercase();
        if q.is_empty() { return roots.into_iter().filter(|r| r.is_dir()).take(limit).collect(); }
        let mut scanned = 0usize;
        let mut hits: Vec<(bool, SystemTime, PathBuf)> = Vec::new();
        let mut stack: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
        while let Some((dir, depth)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                scanned += 1;
                if scanned > MAX_SCANNED { break; }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.contains('.') && is_package(&name) { continue; }
                let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
                if !meta.is_dir() || meta.file_type().is_symlink() { continue; }
                if name_matches(&name, &q) {
                    let exact = fold_words(&name).join(" ") == q;
                    hits.push((exact, meta.modified().unwrap_or(UNIX_EPOCH), entry.path()));
                }
                if depth + 1 < MAX_DEPTH { stack.push((entry.path(), depth + 1)); }
            }
        }
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        // Exact names win outright: "Tampa" is not also "Tampa 2019 backup".
        if hits.first().is_some_and(|h| h.0) { hits.retain(|h| h.0); }
        hits.into_iter().take(limit).map(|(_, _, p)| p).collect()
    }

    /// A folder chosen for OPEN_DIRECTORY or LIST_DIRECTORY, checked again: a real
    /// folder (not a link, not a package), inside one of the roots.
    pub fn validate_folder(&self, path: &str) -> Result<PathBuf, String> {
        let p = PathBuf::from(path);
        if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("A folder is opened by its full location.".into());
        }
        let meta = std::fs::symlink_metadata(&p).map_err(|_| "That folder no longer exists.".to_string())?;
        if meta.file_type().is_symlink() || !meta.is_dir() { return Err("That is not a folder.".into()); }
        if p.file_name().is_some_and(|n| is_package(&n.to_string_lossy())) { return Err("Apps and packages are not opened as folders.".into()); }
        let real = std::fs::canonicalize(&p).map_err(|e| e.to_string())?;
        let inside = self.0.iter().any(|r| std::fs::canonicalize(r).map(|rr| real.starts_with(rr)).unwrap_or(false));
        if !inside { return Err(format!("Only folders in {} may be opened or listed.", self.names())); }
        Ok(p)
    }

    /// What is directly inside a folder that matches `filter` (every word, plural-
    /// insensitive; empty = everything), newest first, at most `limit`. Hidden
    /// items and links are skipped. Reads names, kinds and dates — never contents.
    pub fn list_folder(&self, path: &str, filter: &str, limit: usize) -> Result<Vec<Entry>, String> {
        let folder = self.validate_folder(path)?;
        let entries = std::fs::read_dir(&folder).map_err(|e| format!("Could not read that folder: {e}"))?;
        let mut out: Vec<Entry> = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else { continue };
            if meta.file_type().is_symlink() { continue; }
            if !filter.trim().is_empty() && !name_matches(&name, filter) { continue; }
            out.push(Entry { path: e.path(), folder: meta.is_dir(), modified: meta.modified().unwrap_or(UNIX_EPOCH) });
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.path.cmp(&b.path)));
        out.truncate(limit);
        Ok(out)
    }
}

/// One item in a folder listing.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub folder: bool,
    pub modified: SystemTime,
}

impl Entry {
    pub fn line(&self) -> String {
        let name = self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        format!("{name}{} — modified {}", if self.folder { "/" } else { "" }, date(self.modified))
    }
}

/// Whether a folder's own name is exactly `name`, ignoring case and punctuation.
pub fn exact_name(path: &Path, name: &str) -> bool {
    path.file_name().is_some_and(|n| fold_words(&n.to_string_lossy()) == fold_words(name))
}

pub fn is_package(name: &str) -> bool {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    ["app", "bundle", "framework", "photoslibrary", "musiclibrary", "pkg", "plugin", "kext"].contains(&ext.as_str())
}

/// "2026-09-15" in UTC, from a file time.
pub fn date(t: SystemTime) -> String {
    let days = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64;
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_sentences_become_folder_requests_and_questions_do_not() {
        assert_eq!(parse_open_folder("Open the Tampa folder on Desktop"), Some(("tampa".into(), Some("Desktop".into()))));
        assert_eq!(parse_open_folder("open the folder called Tax Returns in my Documents"), Some(("tax returns".into(), Some("Documents".into()))));
        assert_eq!(parse_open_folder("open my Downloads folder"), Some(("".into(), Some("Downloads".into()))));
        assert_eq!(parse_listing("What’s inside the Tampa folder?"), Some(("tampa".into(), "".into(), None)), "a curly apostrophe, as speech recognition writes it");
        assert_eq!(parse_open_folder("open Tampa"), None, "no \"folder\": an app name first");
        assert_eq!(parse_open_folder("open Safari"), None);

        assert_eq!(parse_listing("tell me what resumes are inside"), Some(("".into(), "resumes".into(), None)));
        assert_eq!(parse_listing("What resumes are inside?"), Some(("".into(), "resumes".into(), None)));
        assert_eq!(parse_listing("what's in the Tampa folder"), Some(("tampa".into(), "".into(), None)));
        assert_eq!(parse_listing("what PDFs are in Tampa"), Some(("tampa".into(), "pdfs".into(), None)));
        assert_eq!(parse_listing("list the resumes in the Tampa folder on Desktop"), Some(("tampa".into(), "resumes".into(), Some("Desktop".into()))));
        assert_eq!(parse_listing("what's inside it"), Some(("".into(), "".into(), None)));
        for question in ["what is in front of the camera", "what is in my calendar today", "what am I doing", "who is in the room"] {
            assert_eq!(parse_listing(question), None, "{question}");
        }
    }

    #[test]
    fn dates_are_civil_dates() {
        assert_eq!(date(UNIX_EPOCH), "1970-01-01");
        assert_eq!(date(UNIX_EPOCH + std::time::Duration::from_secs(1_789_493_809)), "2026-09-15");
        assert_eq!(date(UNIX_EPOCH + std::time::Duration::from_secs(951_782_400)), "2000-02-29");
    }
}
