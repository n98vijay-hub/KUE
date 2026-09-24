//! Which installed application a name means.
//!
//!   "Chrome" → INSTALLED APP "Google Chrome" (com.google.Chrome) → OPEN_APPLICATION "Google Chrome"
//!
//! Nothing here is a hardcoded list of apps. The catalog is read from the
//! application folders on this Mac, and a name is matched against it by fixed
//! rules — never by a model:
//!
//!   1. the app's name (or bundle identifier) exactly, ignoring case and ".app";
//!   2. otherwise apps whose name contains every word you said, as whole words;
//!   3. otherwise apps with a word starting with each word you said (3+ letters);
//!
//! and within 2 or 3 the app with the fewest words you did not say wins, if
//! exactly one does ("Chrome": Google Chrome over Chrome Remote Desktop). A tie
//! is AMBIGUOUS — KUE asks which one. No match is NOT_FOUND — KUE says so and
//! never guesses.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledApp {
    /// The bundle's name, as Finder shows it without ".app".
    pub name: String,
    pub bundle_id: Option<String>,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppResolution {
    Found { app: InstalledApp },
    Ambiguous { candidates: Vec<InstalledApp> },
    NotFound,
}

/// The most apps offered when a name is ambiguous.
pub const MAX_CANDIDATES: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct AppCatalog {
    apps: Vec<InstalledApp>,
}

fn words(s: &str) -> Vec<String> {
    let lower = s.trim().to_lowercase();
    let lower = lower.strip_suffix(".app").unwrap_or(&lower);
    lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// CFBundleIdentifier from an XML Info.plist. A binary plist yields None: the
/// identifier is a convenience for matching and display, never required.
fn bundle_id(app: &Path) -> Option<String> {
    let bytes = std::fs::read(app.join("Contents/Info.plist")).ok()?;
    if bytes.starts_with(b"bplist") { return None; }
    let text = String::from_utf8(bytes).ok()?;
    let after = &text[text.find("<key>CFBundleIdentifier</key>")? + 29..];
    let start = after.find("<string>")? + 8;
    let end = after[start..].find("</string>")?;
    let id = after[start..start + end].trim();
    (!id.is_empty() && id.len() < 200).then(|| id.to_string())
}

impl AppCatalog {
    /// Where applications are installed for this user.
    pub fn default_dirs(home: &Path) -> Vec<PathBuf> {
        vec![PathBuf::from("/Applications"), PathBuf::from("/Applications/Utilities"),
             PathBuf::from("/System/Applications"), PathBuf::from("/System/Applications/Utilities"),
             home.join("Applications")]
    }

    /// Reads `*.app` bundles in `dirs` and one folder level below them
    /// (e.g. /Applications/WhatsApp.localized/WhatsApp.app). Nothing is opened or run.
    pub fn scan(dirs: &[PathBuf]) -> Self {
        let mut apps: Vec<InstalledApp> = Vec::new();
        let mut add = |p: &Path| {
            let Some(name) = p.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".app")) else { return };
            if apps.iter().any(|a| a.name.eq_ignore_ascii_case(name)) { return; }
            apps.push(InstalledApp { name: name.to_string(), bundle_id: bundle_id(p), path: p.to_path_buf() });
        };
        for d in dirs {
            let Ok(entries) = std::fs::read_dir(d) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "app") { add(&p); continue; }
                if p.is_dir() && !p.is_symlink() {
                    if let Ok(inner) = std::fs::read_dir(&p) {
                        for i in inner.flatten() {
                            if i.path().extension().is_some_and(|x| x == "app") { add(&i.path()); }
                        }
                    }
                }
            }
        }
        apps.sort_by(|a, b| a.name.cmp(&b.name));
        AppCatalog { apps }
    }

    /// A catalog of named apps, for tests and for callers that already know the list.
    pub fn from_names(names: &[&str]) -> Self {
        AppCatalog { apps: names.iter().map(|n| InstalledApp { name: n.to_string(), bundle_id: None, path: PathBuf::new() }).collect() }
    }

    pub fn len(&self) -> usize { self.apps.len() }
    pub fn is_empty(&self) -> bool { self.apps.is_empty() }

    pub fn resolve(&self, query: &str) -> AppResolution {
        let q = words(query);
        if q.is_empty() { return AppResolution::NotFound; }
        let exact: Vec<&InstalledApp> = self.apps.iter()
            .filter(|a| words(&a.name) == q || a.bundle_id.as_deref().is_some_and(|b| b.eq_ignore_ascii_case(query.trim())))
            .collect();
        if let [one] = exact.as_slice() { return AppResolution::Found { app: (*one).clone() }; }

        let whole = |a: &InstalledApp| { let w = words(&a.name); q.iter().all(|x| w.contains(x)) };
        let prefix = |a: &InstalledApp| {
            let w = words(&a.name);
            q.iter().all(|x| x.chars().count() >= 3 && w.iter().any(|y| y.starts_with(x.as_str())))
        };
        for rule in [&whole as &dyn Fn(&InstalledApp) -> bool, &prefix] {
            let hits: Vec<&InstalledApp> = self.apps.iter().filter(|a| rule(a)).collect();
            if hits.is_empty() { continue; }
            let extra = |a: &InstalledApp| words(&a.name).len().saturating_sub(q.len());
            let fewest = hits.iter().map(|a| extra(a)).min().unwrap_or(0);
            let mut best: Vec<&InstalledApp> = hits.iter().copied().filter(|a| extra(a) == fewest).collect();
            best.sort_by(|a, b| a.name.cmp(&b.name));
            return match best.as_slice() {
                [one] => AppResolution::Found { app: (*one).clone() },
                many => AppResolution::Ambiguous { candidates: many.iter().take(MAX_CANDIDATES).map(|a| (*a).clone()).collect() },
            };
        }
        AppResolution::NotFound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names as installed on this Mac (abridged).
    fn mac() -> AppCatalog {
        AppCatalog::from_names(&["Calculator", "Chrome Remote Desktop", "Google Chrome", "Keynote", "Keynote Creator Studio",
            "Microsoft Excel", "Microsoft PowerPoint", "Microsoft Teams", "Microsoft Word", "Notes", "Safari",
            "Visual Studio Code", "zoom.us", "Grammarly Desktop", "Grammarly for Safari"])
    }

    fn found(r: AppResolution) -> String {
        match r { AppResolution::Found { app } => app.name, other => panic!("{other:?}") }
    }

    #[test]
    fn a_name_resolves_to_the_installed_app_it_means() {
        let c = mac();
        assert_eq!(found(c.resolve("Chrome")), "Google Chrome", "fewer words you did not say");
        assert_eq!(found(c.resolve("google chrome")), "Google Chrome");
        assert_eq!(found(c.resolve("calculator.app")), "Calculator");
        assert_eq!(found(c.resolve("Keynote")), "Keynote", "exact beats longer names");
        assert_eq!(found(c.resolve("word")), "Microsoft Word");
        assert_eq!(found(c.resolve("code")), "Visual Studio Code");
        assert_eq!(c.resolve("vs code"), AppResolution::NotFound, "\"vs\" is not a word of any app name");
    }

    #[test]
    fn a_tie_is_asked_about_and_an_unknown_name_is_not_guessed() {
        let c = mac();
        match c.resolve("microsoft") {
            AppResolution::Ambiguous { candidates } => assert_eq!(candidates.len(), 4),
            other => panic!("{other:?}"),
        }
        assert_eq!(found(c.resolve("grammarly")), "Grammarly Desktop", "one extra word beats two");
        assert_eq!(c.resolve("Photoshop"), AppResolution::NotFound);
        assert_eq!(c.resolve("ca"), AppResolution::NotFound, "two letters match nothing by prefix");
        assert_eq!(c.resolve(""), AppResolution::NotFound);
        assert_eq!(found(c.resolve("calc")), "Calculator");
    }

    #[test]
    fn the_catalog_is_read_from_the_application_folders_on_this_mac() {
        let c = AppCatalog::scan(&AppCatalog::default_dirs(Path::new("/nonexistent-home")));
        if c.is_empty() { eprintln!("no application folders here"); return; }
        match c.resolve("Calculator") {
            AppResolution::Found { app } => assert!(app.path.ends_with("Calculator.app"), "{:?}", app.path),
            other => panic!("Calculator is part of macOS: {other:?}"),
        }
    }
}
