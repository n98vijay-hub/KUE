//! Short replies that refer to what is already happening: "yes", "go ahead",
//! "cancel that", "stop", "the older one".
//!
//! Recognised by exact phrase, deterministically — never by a model, and never
//! by finding a word inside a longer sentence ("don't stop the music" is not a
//! stop). A reference is an instruction about the action that is waiting; what
//! it leads to goes through the same transaction as the window's buttons
//! (`transaction::resolve_reference`), so a spoken "yes" re-authorizes exactly
//! as Confirm does and grants nothing by itself.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Reference {
    /// Confirm the one action waiting for confirmation.
    Confirm,
    /// Stop speaking, and cancel the action waiting for confirmation, if any.
    Cancel,
    /// Pick a different match for the document request that is waiting.
    Choose(Choice),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Choice {
    Newest,
    /// One older than the match selected now.
    Older,
    /// One newer than the match selected now.
    Newer,
    Oldest,
    /// First, second, third… (1-based).
    Position(usize),
}

fn normalize(text: &str) -> String {
    let t: String = text.to_lowercase().chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' }).collect();
    let mut words: Vec<&str> = t.split_whitespace().collect();
    const LEAD: [&str; 7] = ["kue", "lantern", "hey", "okay", "ok", "actually", "please"];
    while words.len() > 1 && LEAD.contains(&words[0]) { words.remove(0); }
    while words.len() > 1 && words.last() == Some(&"please") { words.pop(); }
    words.join(" ")
}

const CONFIRM: [&str; 14] = ["yes", "yeah", "yep", "yes please", "go ahead", "do it", "confirm", "confirmed",
    "yes go ahead", "yes do it", "sure", "sure go ahead", "please do", "that one"];
const CANCEL: [&str; 16] = ["stop", "cancel", "cancel that", "cancel it", "stop that", "stop it", "never mind",
    "nevermind", "no", "no thanks", "no thank you", "don't", "dont", "don't do it", "not now", "forget it"];

fn choice(words: &str) -> Option<Choice> {
    let w = words.strip_prefix("no ").unwrap_or(words);
    let w = w.strip_prefix("open ").unwrap_or(w);
    let w = w.strip_prefix("the ").unwrap_or(w);
    let w = w.strip_suffix(" one").or_else(|| w.strip_suffix(" instead")).unwrap_or(w);
    let w = w.strip_suffix(" one").unwrap_or(w);
    Some(match w {
        "newest" | "latest" | "most recent" => Choice::Newest,
        "first" => Choice::Position(1),
        "older" | "other" | "next" | "previous" => Choice::Older,
        "newer" => Choice::Newer,
        "oldest" | "last" => Choice::Oldest,
        "second" => Choice::Position(2),
        "third" => Choice::Position(3),
        "fourth" => Choice::Position(4),
        "fifth" => Choice::Position(5),
        "sixth" => Choice::Position(6),
        _ => return None,
    })
}

/// The reference a whole utterance makes, or None (it is a command or a question).
pub fn interpret(text: &str) -> Option<Reference> {
    let n = normalize(text);
    if n.is_empty() { return None; }
    if let Some(c) = choice(&n) { return Some(Reference::Choose(c)); }
    if CONFIRM.contains(&n.as_str()) { return Some(Reference::Confirm); }
    if CANCEL.contains(&n.as_str()) { return Some(Reference::Cancel); }
    None
}

/// The index `choice` picks among `count` matches, newest first, when `current`
/// is selected. None when there is no such match.
pub fn pick(choice: Choice, current: usize, count: usize) -> Option<usize> {
    if count == 0 { return None; }
    match choice {
        Choice::Newest => Some(0),
        Choice::Oldest => Some(count - 1),
        Choice::Older => (current + 1 < count).then_some(current + 1),
        Choice::Newer => current.checked_sub(1),
        Choice::Position(p) => (p >= 1 && p <= count).then(|| p - 1),
    }
}

// MARK: - Speaking about files

/// A number as it is said: "two", not "2". Above twelve, digits.
pub fn spoken_count(n: usize) -> String {
    const WORDS: [&str; 13] = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"];
    WORDS.get(n).map(|w| w.to_string()).unwrap_or_else(|| n.to_string())
}

/// "resume" → "resumes". Plain English plurals only; a word already ending in s is left alone.
pub fn plural(noun: &str) -> String {
    let n = noun.trim();
    if n.ends_with('s') { n.to_string() }
    else if n.ends_with('y') && !n.ends_with("ay") && !n.ends_with("ey") && !n.ends_with("oy") { format!("{}ies", &n[..n.len() - 1]) }
    else { format!("{n}s") }
}

/// What kind of file, as said: "a PDF". None for kinds with no plain name.
pub fn spoken_kind(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path).extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "pdf" => "a PDF",
        "doc" | "docx" => "a Word document",
        "pages" => "a Pages document",
        "txt" | "md" | "rtf" => "a text document",
        "key" | "ppt" | "pptx" => "a presentation",
        "numbers" | "xls" | "xlsx" | "csv" => "a spreadsheet",
        "png" | "jpg" | "jpeg" | "heic" => "an image",
        _ => return None,
    })
}

/// A file's name as KUE may say it aloud: the name only — never its folder,
/// path or extension — with separators read as spaces. None when the name
/// looks like an identifier rather than words (a UUID, a hash, a long number),
/// which is noise when spoken and can be a private reference.
pub fn spoken_file_name(path: &str) -> Option<String> {
    let stem = std::path::Path::new(path).file_stem()?.to_str()?;
    // A hidden file has no name to say.
    if stem.starts_with('.') { return None; }
    let words: Vec<String> = stem.split(|c: char| c == '_' || c == '-' || c == '.' || c.is_whitespace())
        .filter(|w| !w.is_empty()).map(String::from).collect();
    if words.is_empty() || words.len() > 8 { return None; }
    let identifier_like = |w: &str| {
        let digits = w.chars().filter(|c| c.is_ascii_digit()).count();
        let hex = w.chars().all(|c| c.is_ascii_hexdigit());
        digits >= 5 || (w.len() >= 8 && hex && digits > 0) || w.len() > 24
    };
    if words.iter().any(|w| identifier_like(w) || !w.chars().all(|c| c.is_alphanumeric() || "'&()".contains(c))) {
        return None;
    }
    let name = words.join(" ");
    (name.chars().count() <= 60).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_replies_are_references_and_sentences_are_not() {
        for s in ["Yes.", "yes please", "Go ahead!", "KUE, do it", "okay go ahead", "That one."] {
            assert_eq!(interpret(s), Some(Reference::Confirm), "{s}");
        }
        for s in ["Stop", "KUE stop", "Cancel that.", "never mind", "No.", "no thanks"] {
            assert_eq!(interpret(s), Some(Reference::Cancel), "{s}");
        }
        assert_eq!(interpret("Actually open the older one."), Some(Reference::Choose(Choice::Older)));
        assert_eq!(interpret("No, the older one"), Some(Reference::Choose(Choice::Older)));
        assert_eq!(interpret("the second one"), Some(Reference::Choose(Choice::Position(2))));
        assert_eq!(interpret("the newest one"), Some(Reference::Choose(Choice::Newest)));
        for s in ["open my resume", "don't stop the music", "yes, and open Safari too", "stop the timer in Clock",
                  "is the older one bigger?", "What can you do?", "no idea", "open", ""] {
            assert_eq!(interpret(s), None, "{s}");
        }
    }

    #[test]
    fn files_are_named_without_folders_extensions_or_identifiers() {
        assert_eq!(spoken_file_name("/Users/v/Desktop/Tampa/Vijay_Resume.pdf").as_deref(), Some("Vijay Resume"));
        assert_eq!(spoken_file_name("/Users/v/Documents/resume-2024 final.docx").as_deref(), Some("resume 2024 final"));
        for noise in ["/x/3F2504E0-4F89-11D3-9A0C-0305E82C3301.pdf", "/x/scan_20240912_104455.pdf",
                      "/x/d41d8cd98f00b204e9800998ecf8427e.pdf", "/x/.pdf", "/x/résumé<script>.pdf"] {
            assert_eq!(spoken_file_name(noise), None, "{noise}");
        }
        assert_eq!(spoken_kind("/x/a.PDF"), Some("a PDF"));
        assert_eq!(spoken_kind("/x/a.bin"), None);
        assert_eq!((plural("resume"), plural("summary"), plural("notes")), ("resumes".into(), "summaries".into(), "notes".into()));
        assert_eq!((spoken_count(2), spoken_count(13)), ("two".into(), "13".into()));
    }

    #[test]
    fn picking_stays_inside_the_matches() {
        assert_eq!(pick(Choice::Older, 0, 3), Some(1));
        assert_eq!(pick(Choice::Older, 2, 3), None, "already the oldest");
        assert_eq!(pick(Choice::Newer, 0, 3), None);
        assert_eq!(pick(Choice::Oldest, 0, 3), Some(2));
        assert_eq!(pick(Choice::Position(4), 0, 3), None);
        assert_eq!(pick(Choice::Position(0), 0, 3), None);
        assert_eq!(pick(Choice::Newest, 2, 0), None);
    }
}
