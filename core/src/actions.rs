//! KUE command pipeline and Action Broker policy.
//!
//!   TEXT / VOICE (transcript) INPUT
//!   → COMMAND PARSER (deterministic; no model)  → INTENT (an allowlisted ActionKind)
//!   → IDENTITY + AUTHORIZATION (authz, by risk) → POLICY (targets, permitted folder)
//!   → PLAN → CONFIRMATION if required → BROKER executes → VERIFICATION → EVENT
//!
//! Anything the parser does not recognise is not an action. There is no
//! "run this command" action, no shell, and no path for a model to propose or
//! execute one: unknown action = DENY by construction.
//!
//! File actions are confined to permitted roots (default `~/KUE`), checked
//! lexically and again after resolving symlinks. Nothing is ever deleted or
//! overwritten.
//!
//! OPEN_DOCUMENT is the one action that reaches outside `~/KUE`, and only to
//! OPEN a document you name, never to change one: it searches Desktop,
//! Documents, Downloads and `~/KUE` for document types only (never an app,
//! script or installer), shows you the exact file, and waits for Confirm.
//!
//! Every target — app name, link, path, notification text, matched document —
//! is ACTION_TARGET data. It passes the privacy firewall before a document
//! search runs and before any record reaches the interface; the events local
//! memory keeps about an action never contain it.

use crate::authz::Operation;
use crate::privacy::{Cleared, Destination};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionKind {
    OpenApplication { name: String },
    CloseApplication { name: String },
    FocusApplication { name: String },
    OpenUrl { url: String },
    CreateDirectory { path: String },
    CreateFile { path: String, text: String },
    ReadPermittedFile { path: String },
    MovePermittedFile { from: String, to: String },
    ShowNotification { title: String, body: String },
    /// `query` is what you asked for; `path` is the file chosen for you to confirm.
    OpenDocument { query: String, path: Option<String> },
    /// A folder you named, in Desktop, Documents, Downloads or ~/KUE, opened in Finder.
    /// `scope` is the document folder you named it in, if any; `path` the folder found.
    OpenDirectory { query: String, scope: Option<String>, path: Option<String> },
    /// What is directly inside a folder: names, kinds and dates, never contents.
    /// `filter` is what to look for ("resumes"); empty lists everything.
    ListDirectory { query: String, filter: String, scope: Option<String>, path: Option<String> },
    /// Takes stock of what is using room: the volume's totals, and the names,
    /// sizes and dates in the folders you allowed. Reads no file's contents and
    /// changes nothing.
    InspectStorage,
    /// Files you chose from what KUE found, moved to the Trash one at a time,
    /// each checked afterwards. A move, never a delete: everything stays until
    /// you empty the Trash, and KUE can put any of it back.
    MoveToTrash { paths: Vec<String> },
    /// Puts back what a move to the Trash moved, each to exactly where it came
    /// from. Refused for anything whose old place is now taken.
    RestoreFromTrash { items: Vec<TrashedItem> },
}

/// One file KUE moved: where it was, and where macOS put it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashedItem {
    pub original: String,
    pub trashed: String,
}

/// The most files one move to the Trash may carry. A number a person can
/// actually look at before agreeing to it.
pub const MAX_TRASHED: usize = 50;

impl ActionKind {
    pub fn tag(&self) -> &'static str {
        match self {
            ActionKind::OpenApplication { .. } => "OPEN_APPLICATION",
            ActionKind::CloseApplication { .. } => "CLOSE_APPLICATION",
            ActionKind::FocusApplication { .. } => "FOCUS_APPLICATION",
            ActionKind::OpenUrl { .. } => "OPEN_URL",
            ActionKind::CreateDirectory { .. } => "CREATE_DIRECTORY",
            ActionKind::CreateFile { .. } => "CREATE_FILE",
            ActionKind::ReadPermittedFile { .. } => "READ_PERMITTED_FILE",
            ActionKind::MovePermittedFile { .. } => "MOVE_PERMITTED_FILE",
            ActionKind::ShowNotification { .. } => "SHOW_NOTIFICATION",
            ActionKind::OpenDocument { .. } => "OPEN_DOCUMENT",
            ActionKind::OpenDirectory { .. } => "OPEN_DIRECTORY",
            ActionKind::ListDirectory { .. } => "LIST_DIRECTORY",
            ActionKind::InspectStorage => "INSPECT_STORAGE",
            ActionKind::MoveToTrash { .. } => "MOVE_TO_TRASH",
            ActionKind::RestoreFromTrash { .. } => "RESTORE_FROM_TRASH",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            ActionKind::OpenApplication { name } => format!("Open {name}"),
            ActionKind::CloseApplication { name } => format!("Quit {name}"),
            ActionKind::FocusApplication { name } => format!("Switch to {name}"),
            ActionKind::OpenUrl { url } => format!("Open {url} in your default browser"),
            ActionKind::CreateDirectory { path } => format!("Create the folder {path}"),
            ActionKind::CreateFile { path, text } =>
                format!("Create the file {path}{}", if text.is_empty() { String::new() } else { format!(" ({} characters)", text.chars().count()) }),
            ActionKind::ReadPermittedFile { path } => format!("Show the contents of {path}"),
            ActionKind::MovePermittedFile { from, to } => format!("Move {from} to {to}"),
            ActionKind::ShowNotification { body, .. } => format!("Show a notification: {body}"),
            ActionKind::OpenDocument { path: Some(p), .. } => format!("Open {}", tilde(p)),
            ActionKind::OpenDocument { query, path: None } => format!("Find and open a document matching “{query}”"),
            ActionKind::OpenDirectory { path: Some(p), .. } => format!("Open the folder {} in Finder", tilde(p)),
            ActionKind::OpenDirectory { query, scope, path: None } => format!("Find and open a folder matching “{query}”{}", in_scope(scope)),
            ActionKind::ListDirectory { filter, path: Some(p), .. } =>
                format!("List {} in {}", if filter.is_empty() { "everything".to_string() } else { format!("“{filter}”") }, tilde(p)),
            ActionKind::ListDirectory { filter, query, scope, path: None } =>
                format!("List {} in a folder matching “{query}”{}", if filter.is_empty() { "everything".to_string() } else { format!("“{filter}”") }, in_scope(scope)),
            ActionKind::InspectStorage => "Take stock of what is using room on this Mac".to_string(),
            ActionKind::MoveToTrash { paths } =>
                format!("Move {} to the Trash", count_files(paths.len())),
            ActionKind::RestoreFromTrash { items } =>
                format!("Put {} back where {} came from", count_files(items.len()), if items.len() == 1 { "it" } else { "they" }),
        }
    }
}

/// The one rule for "the action KUE is on right now", used by the spoken
/// status, the window's live line and the interface projection alike.
///
/// Something in progress wins over a newer finished one; otherwise the newest
/// record that changed recently enough to still be worth reporting. There was
/// briefly a second rule in `surface`, which could have named a different
/// action on the same screen — exactly what one projection exists to prevent.
///
/// Age is measured from `updated_at`: `created_at` is when the action was
/// proposed, and a slow one (a document search, a Touch ID wait, a step of a
/// task) would be stale the instant it finished.
pub fn current<'a>(records: &'a [ActionRecord], now: f64, outcome_seconds: f64) -> Option<&'a ActionRecord> {
    let busy = |r: &&ActionRecord| r.state.is_waiting()
        || matches!(r.state, ActionState::Reauthorizing | ActionState::Executing);
    records.iter().rev().find(busy).or_else(|| records.iter().rev()
        .find(|r| now - r.updated_at.unwrap_or(r.created_at) <= outcome_seconds))
}

/// The brief's nine actions, plus OPEN_DOCUMENT, OPEN_DIRECTORY and LIST_DIRECTORY,
/// added at the owner's request. All three only read or open; none changes a file.
pub const ALLOWLIST: [&str; 15] = ["OPEN_APPLICATION", "CLOSE_APPLICATION", "FOCUS_APPLICATION", "OPEN_URL",
    "CREATE_DIRECTORY", "CREATE_FILE", "READ_PERMITTED_FILE", "MOVE_PERMITTED_FILE", "SHOW_NOTIFICATION", "OPEN_DOCUMENT",
    "OPEN_DIRECTORY", "LIST_DIRECTORY", "INSPECT_STORAGE", "MOVE_TO_TRASH", "RESTORE_FROM_TRASH"];

fn count_files(n: usize) -> String { format!("{n} file{}", if n == 1 { "" } else { "s" }) }

fn in_scope(scope: &Option<String>) -> String {
    scope.as_ref().map(|s| format!(" in {s}")).unwrap_or_default()
}

/// How an executor marks a failure caused by a macOS permission that is not
/// granted (for example Notifications). KUE never works around it; it says so.
pub const PERMISSION_REQUIRED: &str = "PERMISSION_REQUIRED";

/// Shows a path under your home folder as `~/…`.
pub fn tilde(p: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && p.starts_with(&format!("{h}/")) => format!("~{}", &p[h.len()..]),
        _ => p.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Risk { Low, Medium, High, Critical }

/// Exhaustive: a new action cannot exist without a risk.
pub const fn risk(kind: &ActionKind) -> Risk {
    match kind {
        ActionKind::ShowNotification { .. } | ActionKind::FocusApplication { .. } | ActionKind::OpenApplication { .. } => Risk::Low,
        // Open a folder in Finder, or read the names in one: nothing runs and nothing changes.
        ActionKind::OpenDirectory { .. } | ActionKind::ListDirectory { .. } => Risk::Low,
        // Reads sizes and dates. Opens nothing, moves nothing, changes nothing.
        ActionKind::InspectStorage => Risk::Low,
        // Leaves Lantern: the browser fetches it. Writes: something new appears on disk.
        // A document opens one of your files in another app, which may run its content.
        ActionKind::OpenUrl { .. } | ActionKind::CreateDirectory { .. } | ActionKind::CreateFile { .. }
        | ActionKind::ReadPermittedFile { .. } | ActionKind::OpenDocument { .. } => Risk::Medium,
        // Puts a file back exactly where it was, and refuses if anything is there.
        ActionKind::RestoreFromTrash { .. } => Risk::Medium,
        // Can lose unsaved work, or moves your files. Moving to the Trash is
        // reversible and still HIGH: they are your files, and the owner should
        // be asked in the strongest way KUE has before any of them move.
        ActionKind::CloseApplication { .. } | ActionKind::MovePermittedFile { .. }
        | ActionKind::MoveToTrash { .. } => Risk::High,
    }
}

pub const fn operation_for(r: Risk) -> Operation {
    match r {
        Risk::Low => Operation::ActionLowRisk,
        Risk::Medium => Operation::ActionMediumRisk,
        Risk::High => Operation::ActionHighRisk,
        Risk::Critical => Operation::ActionCriticalRisk,
    }
}

/// MEDIUM and above wait for an explicit Confirm.
pub const fn needs_confirmation(r: Risk) -> bool { !matches!(r, Risk::Low) }

/// Unknown risk tag = DENY.
pub fn risk_from_tag(tag: &str) -> Option<Risk> {
    match tag { "LOW" => Some(Risk::Low), "MEDIUM" => Some(Risk::Medium), "HIGH" => Some(Risk::High), "CRITICAL" => Some(Risk::Critical), _ => None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionState {
    Proposed, Authorized, RequiresConfirmation, RequiresStrongAuth, Denied, Executing,
    Succeeded, Failed, PartiallySucceeded, Cancelled, UnknownResult,
    /// Confirm was pressed; authorization is being checked again.
    Reauthorizing,
    /// Authorized when planned, not any more when confirmed. Nothing ran.
    AuthorizationExpired,
    /// The privacy firewall refused the action's target. Nothing ran.
    PrivacyDenied,
    /// No document matched the request. Nothing ran.
    NoMatches,
}

impl ActionState {
    /// Waiting for the owner's Confirm.
    pub fn is_waiting(self) -> bool { matches!(self, ActionState::RequiresConfirmation | ActionState::RequiresStrongAuth) }
}

/// The checkpoints an action passed, in order. Kept with the record so the card
/// can show how far it got and why it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionStep {
    Proposed, PrivacyChecked, Authorized, AwaitingConfirmation, Reauthorized, Executing, Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub id: String,
    pub source: String,
    pub action: ActionKind,
    pub description: String,
    pub risk: Risk,
    pub state: ActionState,
    pub reason: Option<String>,
    /// What was checked after execution. Required for SUCCEEDED.
    pub verification: Option<String>,
    /// For READ_PERMITTED_FILE only: shown to you, never stored or sent to a model.
    pub output: Option<String>,
    pub created_at: f64,
    /// When this record last changed state. `created_at` is when it was
    /// proposed, which for a slow action — a document search, a Touch ID wait,
    /// a step of a task — is long before it finished. Anything asking "did this
    /// just happen?" must ask this, or a slow action's outcome is stale the
    /// moment it arrives.
    #[serde(default)]
    pub updated_at: Option<f64>,
    /// For OPEN_DOCUMENT: other matching files you may pick instead, newest first.
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub steps: Vec<ActionStep>,
    /// When this action (or its current target) started waiting for your
    /// confirmation. A spoken "yes" only confirms a recent request.
    #[serde(default)]
    pub awaiting_since: Option<f64>,
    /// The multi-step request this action is one step of, if any.
    #[serde(default)]
    pub task: Option<TaskStepRef>,
    /// KUE's own sentences for this outcome, computed from verified state at
    /// the moment it happened — for actions whose result cannot be described
    /// from their kind alone. INSPECT_STORAGE's numbers are the case: "31.6 GB
    /// worth reviewing" is arithmetic over what was measured, and there is
    /// nowhere else it could come from. Never a model's words.
    #[serde(default)]
    pub sentences: Option<Sentences>,
}

/// One outcome, said two ways. Speech reaches further than a screen, so the
/// spoken form is written separately rather than trimmed from the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentences {
    pub on_screen: String,
    pub aloud: String,
}

/// Step `index` (0-based) of `of` in task `task_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStepRef {
    pub task_id: String,
    pub index: usize,
    pub of: usize,
}

impl ActionKind {
    /// The app an application action names.
    pub fn app_name(&self) -> Option<&str> {
        match self {
            ActionKind::OpenApplication { name } | ActionKind::CloseApplication { name } | ActionKind::FocusApplication { name } => Some(name),
            _ => None,
        }
    }

    /// The same application action, naming `name` instead.
    pub fn with_app_name(&self, name: &str) -> ActionKind {
        match self {
            ActionKind::OpenApplication { .. } => ActionKind::OpenApplication { name: name.into() },
            ActionKind::CloseApplication { .. } => ActionKind::CloseApplication { name: name.into() },
            ActionKind::FocusApplication { .. } => ActionKind::FocusApplication { name: name.into() },
            other => other.clone(),
        }
    }

    /// The same kind with every target emptied.
    pub fn redacted(&self) -> ActionKind {
        let e = String::new;
        match self {
            ActionKind::OpenApplication { .. } => ActionKind::OpenApplication { name: e() },
            ActionKind::CloseApplication { .. } => ActionKind::CloseApplication { name: e() },
            ActionKind::FocusApplication { .. } => ActionKind::FocusApplication { name: e() },
            ActionKind::OpenUrl { .. } => ActionKind::OpenUrl { url: e() },
            ActionKind::CreateDirectory { .. } => ActionKind::CreateDirectory { path: e() },
            ActionKind::CreateFile { .. } => ActionKind::CreateFile { path: e(), text: e() },
            ActionKind::ReadPermittedFile { .. } => ActionKind::ReadPermittedFile { path: e() },
            ActionKind::MovePermittedFile { .. } => ActionKind::MovePermittedFile { from: e(), to: e() },
            ActionKind::ShowNotification { .. } => ActionKind::ShowNotification { title: e(), body: e() },
            ActionKind::OpenDocument { .. } => ActionKind::OpenDocument { query: e(), path: None },
            ActionKind::OpenDirectory { .. } => ActionKind::OpenDirectory { query: e(), scope: None, path: None },
            ActionKind::ListDirectory { .. } => ActionKind::ListDirectory { query: e(), filter: e(), scope: None, path: None },
            // Names nothing in the first place, so there is nothing to redact.
            ActionKind::InspectStorage => ActionKind::InspectStorage,
            ActionKind::MoveToTrash { .. } => ActionKind::MoveToTrash { paths: Vec::new() },
            ActionKind::RestoreFromTrash { .. } => ActionKind::RestoreFromTrash { items: Vec::new() },
        }
    }
}

impl ActionRecord {
    /// The record with its target, matches, reason, verification and output
    /// removed: what may be shown when the target itself may not.
    pub fn redacted(&self) -> ActionRecord {
        ActionRecord {
            action: self.action.redacted(),
            description: format!("{} (target withheld by the privacy firewall)", self.action.tag()),
            reason: None, verification: None, output: None, choices: Vec::new(), sentences: None,
            ..self.clone()
        }
    }

    /// Waiting for you to pick one of several matches before anything can run:
    /// an app name that matched several apps, or a folder name that matched several folders.
    pub fn needs_choice(&self) -> bool {
        !self.choices.is_empty() && match &self.action {
            ActionKind::OpenApplication { .. } | ActionKind::CloseApplication { .. } | ActionKind::FocusApplication { .. } => true,
            ActionKind::OpenDirectory { path, .. } | ActionKind::ListDirectory { path, .. } => path.is_none(),
            _ => false,
        }
    }

    pub fn step(&mut self, s: ActionStep) {
        if self.steps.last() != Some(&s) { self.steps.push(s); }
    }
    /// SUCCEEDED requires verification evidence; without it the state is UNKNOWN_RESULT.
    pub fn finish(&mut self, state: ActionState, reason: Option<String>, verification: Option<String>) {
        self.state = if state == ActionState::Succeeded && verification.as_deref().map(str::is_empty).unwrap_or(true) {
            ActionState::UnknownResult
        } else { state };
        self.reason = reason;
        self.verification = verification;
    }

    // The only text local memory keeps about an action, and through recent
    // events the only text a model can see. Kind, risk, source and state —
    // never the target, the reason (which can name a path) or any output.

    pub fn event_summary(&self) -> String {
        format!("Action {} ({:?} risk, {}): {:?}.", self.action.tag(), self.risk, self.source, self.state)
    }

    pub fn refused_summary(&self) -> String { format!("Action {} refused.", self.action.tag()) }

    pub fn unplanned_summary(&self) -> String { format!("Action {} could not be planned.", self.action.tag()) }
}

// MARK: - Parser

fn strip_prefixes(s: &str) -> &str {
    let mut t = s.trim();
    loop {
        let lower = t.to_lowercase();
        let before = t;
        for p in ["please ", "computer, ", "lantern, ", "lantern ", "kue, ", "kue ", "can you ", "could you ", "would you ", "hey ", "ok "] {
            if lower.starts_with(p) { t = t[p.len()..].trim_start(); break; }
        }
        if t == before { return t; }
    }
}

fn after<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let lower = text.to_lowercase();
    prefixes.iter().find(|p| lower.starts_with(*p)).map(|p| text[p.len()..].trim()).filter(|r| !r.is_empty())
}

fn strip_article(s: &str) -> &str {
    let lower = s.to_lowercase();
    for a in ["the app ", "the application ", "app ", "application ", "the "] {
        if lower.starts_with(a) { return s[a.len()..].trim(); }
    }
    s
}

fn looks_like_url(s: &str) -> bool {
    let l = s.to_lowercase();
    if l.starts_with("http://") || l.starts_with("https://") { return true; }
    !l.contains(' ') && l.contains('.') && {
        let host = l.split('/').next().unwrap_or("");
        let tld = host.rsplit('.').next().unwrap_or("");
        tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()) && host.len() > tld.len() + 1
    }
}

/// Split "X to Y" on the LAST " to " / " into ".
fn split_to(s: &str) -> Option<(String, String)> {
    let l = s.to_lowercase();
    let (i, n) = [" into ", " to "].iter().filter_map(|sep| l.rfind(sep).map(|i| (i, sep.len()))).max_by_key(|x| x.0)?;
    let (a, b) = (s[..i].trim(), s[i + n..].trim());
    (!a.is_empty() && !b.is_empty()).then(|| (a.to_string(), b.to_string()))
}

/// Parses one sentence into an allowlisted action, or None. Never guesses
/// beyond the grammar: an unrecognised sentence is a question, not a command.
pub fn parse_command(input: &str) -> Option<ActionKind> {
    let text = strip_prefixes(input.trim().trim_end_matches(['.', '!', '?']));
    let clean = |s: &str| s.trim().trim_matches(['"', '\'', '“', '”']).to_string();

    if let Some(rest) = after(text, &["show a notification saying ", "show a notification that ", "show a notification ",
        "send me a notification saying ", "send me a notification ", "notify me that ", "notify me to ", "notify me "]) {
        return Some(ActionKind::ShowNotification { title: "KUE".into(), body: clean(rest) });
    }
    if let Some(rest) = after(text, &["create a folder called ", "create a folder named ", "create folder ", "create a folder ",
        "make a folder called ", "make a folder named ", "make a folder ", "new folder ", "create a directory called ",
        "create a directory named ", "create directory ", "make a directory "]) {
        return Some(ActionKind::CreateDirectory { path: clean(rest) });
    }
    if let Some(rest) = after(text, &["create a text file called ", "create a text file named ", "create a file called ",
        "create a file named ", "create a text file ", "create a file ", "create file ", "make a file called ",
        "make a file named ", "make a file ", "new file "]) {
        let l = rest.to_lowercase();
        for sep in [" containing ", " with the text ", " with text ", " saying ", " that says ", " with "] {
            if let Some(i) = l.find(sep) {
                return Some(ActionKind::CreateFile { path: clean(&rest[..i]), text: clean(&rest[i + sep.len()..]) });
            }
        }
        return Some(ActionKind::CreateFile { path: clean(rest), text: String::new() });
    }
    if let Some(rest) = after(text, &["read the file ", "read file ", "show me the file ", "show the file ", "what's in the file ",
        "what is in the file "]) {
        return Some(ActionKind::ReadPermittedFile { path: clean(rest) });
    }
    if let Some(rest) = after(text, &["move the file ", "move file ", "move "]) {
        let (from, to) = split_to(rest)?;
        return Some(ActionKind::MovePermittedFile { from: clean(&from), to: clean(&to) });
    }
    if let Some(rest) = after(text, &["open any of my ", "open one of my ", "open all of my ", "open my ", "find my ",
        "find and open ", "open the document called ", "open the document named ", "open the document ",
        "open document ", "open a document called ", "open the file called ", "open the file named ",
        "open the file ", "open file ", "open the pdf ", "open pdf "]) {
        return document_query(rest).map(|query| ActionKind::OpenDocument { query, path: None });
    }
    if let Some(rest) = after(text, &["go to ", "visit ", "browse to ", "open the website ", "open website "]) {
        return looks_like_url(rest).then(|| ActionKind::OpenUrl { url: clean(rest) });
    }
    if let Some(rest) = after(text, &["quit ", "close ", "exit "]) {
        return Some(ActionKind::CloseApplication { name: clean(strip_article(rest)) });
    }
    if let Some(rest) = after(text, &["switch to ", "focus on ", "focus ", "bring up ", "go back to "]) {
        return Some(ActionKind::FocusApplication { name: clean(strip_article(rest)) });
    }
    if let Some(rest) = after(text, &["open ", "launch ", "start "]) {
        let rest = strip_article(rest);
        // "open my" names nothing: not an application called "my".
        document_query(rest)?;
        // "resume.pdf" also looks like a web address; a document extension wins.
        if has_document_extension(rest) {
            return document_query(rest).map(|query| ActionKind::OpenDocument { query, path: None });
        }
        return Some(if looks_like_url(rest) { ActionKind::OpenUrl { url: clean(rest) } }
                    else { ActionKind::OpenApplication { name: clean(rest) } });
    }
    None
}

// MARK: - Documents

/// Types OPEN_DOCUMENT may open. Never an application, script, installer,
/// disk image or anything else that runs on opening.
pub const DOCUMENT_EXTENSIONS: [&str; 20] = ["pdf", "doc", "docx", "pages", "rtf", "txt", "md", "odt",
    "key", "ppt", "pptx", "numbers", "xls", "xlsx", "csv", "png", "jpg", "jpeg", "heic", "epub"];

fn extension_of(name: &str) -> Option<String> {
    let n = name.trim().trim_matches(['"', '\'']);
    let (stem, ext) = n.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_lowercase())
}

pub fn has_document_extension(name: &str) -> bool {
    extension_of(name).is_some_and(|e| DOCUMENT_EXTENSIONS.contains(&e.as_str()))
}

/// Words that say nothing about which document.
const QUERY_FILLER: [&str; 14] = ["my", "the", "a", "an", "any", "of", "one", "all", "file", "files",
    "document", "documents", "please", "latest"];

fn fold(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c {
        'é' | 'è' | 'ê' | 'ë' => 'e', 'á' | 'à' | 'â' | 'ä' => 'a', 'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' => 'o', 'ú' | 'ù' | 'û' | 'ü' => 'u',
        c if c.is_alphanumeric() => c, _ => ' ',
    }).collect()
}

/// The words of a document request, or None when nothing names a document.
pub fn document_query(rest: &str) -> Option<String> {
    let words: Vec<String> = fold(rest).split_whitespace()
        .filter(|w| !QUERY_FILLER.contains(w)).map(str::to_string).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn singular(w: &str) -> &str {
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") { &w[..w.len() - 1] } else { w }
}

/// Whether a file name matches every word of a query (plural-insensitive).
pub fn name_matches(file_name: &str, query: &str) -> bool {
    let name = fold(file_name);
    let tokens: Vec<&str> = name.split_whitespace().collect();
    query.split_whitespace().all(|q| {
        let q = singular(q);
        tokens.iter().any(|t| t.contains(q) || singular(t) == q)
    })
}

/// The words of a document request, as you typed or said them. Searching
/// needs one cleared by `Firewall::clear_document_search`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentQuery(String);

impl DocumentQuery {
    pub fn new(query: &str) -> Self { DocumentQuery(query.to_string()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone)]
pub struct DocumentRoots(pub Vec<PathBuf>);

impl DocumentRoots {
    pub fn default_for_home(home: &Path) -> Self {
        DocumentRoots(["Desktop", "Documents", "Downloads", "KUE"].iter().map(|d| home.join(d)).collect())
    }

    pub fn names(&self) -> String {
        self.0.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect::<Vec<_>>().join(", ")
    }

    /// Documents matching `query`, newest first, at most `limit`. Walks at most
    /// three folders deep, never follows links, never enters hidden folders or
    /// packages (apps, libraries), and stops after `MAX_SCANNED` entries.
    ///
    /// Runs only with a query the firewall cleared for the interface: the
    /// matches exist to be shown to you and go nowhere else. Names that do not
    /// match are compared in memory and dropped — never kept, shown or sent.
    pub fn find(&self, query: &Cleared<DocumentQuery>, limit: usize) -> Vec<PathBuf> {
        const MAX_DEPTH: usize = 3;
        const MAX_SCANNED: usize = 20_000;
        if query.destination() != Destination::Interface { return Vec::new(); }
        let query = query.value().as_str();
        let mut scanned = 0usize;
        let mut hits: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        let mut stack: Vec<(PathBuf, usize)> = self.0.iter().map(|r| (r.clone(), 0)).collect();
        while let Some((dir, depth)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                scanned += 1;
                if scanned > MAX_SCANNED { break; }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
                if meta.file_type().is_symlink() { continue; }
                if meta.is_dir() {
                    if depth + 1 < MAX_DEPTH && extension_of(&name).is_none() { stack.push((entry.path(), depth + 1)); }
                } else if meta.is_file() && has_document_extension(&name) && name_matches(&name, query) {
                    hits.push((meta.modified().unwrap_or(std::time::UNIX_EPOCH), entry.path()));
                }
            }
        }
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        hits.into_iter().take(limit).map(|(_, p)| p).collect()
    }

    /// Checks a chosen file again before it is opened: it must still be a real
    /// document file (not a link) inside one of the roots.
    /// A file KUE may MOVE, which is a different question from one it may open.
    /// Opening runs a file's contents through another app, so only documents
    /// qualify; moving does not open anything, and the files worth clearing —
    /// installers above all — are exactly the ones opening refuses. What still
    /// holds: inside the folders you allowed, a real file, never a link, never
    /// a package, never hidden.
    pub fn validate_movable(&self, path: &str) -> Result<PathBuf, String> {
        let p = PathBuf::from(path);
        if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("A file is moved by its full location.".into());
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') { return Err("Hidden files are left alone.".into()); }
        if crate::folders::is_package(&name) { return Err("Apps and packages are not moved this way.".into()); }
        let meta = std::fs::symlink_metadata(&p).map_err(|_| format!("{} is no longer there.", tilde(path)))?;
        if meta.file_type().is_symlink() || !meta.is_file() { return Err(format!("{} is not a regular file.", tilde(path))); }
        let real = std::fs::canonicalize(&p).map_err(|e| e.to_string())?;
        let inside = self.0.iter().any(|r| std::fs::canonicalize(r).map(|rr| real.starts_with(rr)).unwrap_or(false));
        if !inside { return Err(format!("Only files in {} may be moved.", self.names())); }
        Ok(p)
    }

    pub fn validate(&self, path: &str) -> Result<PathBuf, String> {
        let p = PathBuf::from(path);
        if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("A document is opened by its full location.".into());
        }
        if !has_document_extension(path) {
            return Err("Only documents may be opened this way — not applications, scripts or installers.".into());
        }
        let meta = std::fs::symlink_metadata(&p).map_err(|_| format!("{} no longer exists.", tilde(path)))?;
        if meta.file_type().is_symlink() || !meta.is_file() { return Err(format!("{} is not a regular file.", tilde(path))); }
        let real = std::fs::canonicalize(&p).map_err(|e| e.to_string())?;
        let inside = self.0.iter().any(|r| std::fs::canonicalize(r).map(|rr| real.starts_with(rr)).unwrap_or(false));
        if !inside { return Err(format!("Only documents in {} may be opened.", self.names())); }
        Ok(p)
    }
}

// MARK: - Policy on targets

pub fn validate_url(url: &str) -> Result<String, String> {
    let u = url.trim();
    let lower = u.to_lowercase();
    // A scheme is letters/digits/+.- followed by ':' that is not a port number.
    let has_other_scheme = lower.find(':').map(|i| {
        let (scheme, rest) = (&lower[..i], &lower[i + 1..]);
        !scheme.is_empty() && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
            && !rest.chars().next().is_some_and(|c| c.is_ascii_digit())
    }).unwrap_or(false);
    let full = if lower.starts_with("https://") || lower.starts_with("http://") { u.to_string() }
        else if has_other_scheme {
            return Err("Only http and https links may be opened; other schemes can launch programs.".into());
        } else { format!("https://{u}") };
    if full.chars().any(|c| c.is_whitespace() || c.is_control()) { return Err("A link cannot contain spaces.".into()); }
    let host = full.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') { return Err("That link has no valid host.".into()); }
    Ok(full)
}

pub fn validate_app_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 80 { return Err("Name an application.".into()); }
    if n.contains('/') || n.contains("..") || n.chars().any(|c| c.is_control()) {
        return Err("An application is named, not given as a path.".into());
    }
    Ok(n.to_string())
}

#[derive(Debug, Clone)]
pub struct PermittedRoots(pub Vec<PathBuf>);

impl PermittedRoots {
    pub fn default_for_home(home: &Path) -> Self { PermittedRoots(vec![home.join("KUE")]) }

    /// Resolves a name or path into a location inside a permitted root. A bare
    /// name goes in the first root. `..`, and anything that resolves outside —
    /// including through a symlink — is refused.
    pub fn resolve(&self, input: &str, home: &Path) -> Result<PathBuf, String> {
        let raw = input.trim();
        if raw.is_empty() { return Err("No file or folder was named.".into()); }
        if raw.chars().any(|c| c.is_control()) { return Err("That name contains control characters.".into()); }
        let expanded = if let Some(rest) = raw.strip_prefix("~/") { home.join(rest) } else { PathBuf::from(raw) };
        let root0 = self.0.first().ok_or("No permitted folder is configured.")?;
        let candidate = if expanded.is_absolute() { expanded } else { root0.join(expanded) };
        if candidate.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("Paths may not contain '..'.".into());
        }
        let root = self.0.iter().find(|r| candidate.starts_with(r))
            .ok_or_else(|| format!("Only files inside {} may be touched.", self.0.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", ")))?;
        if candidate == *root { return Err("The permitted folder itself cannot be the target.".into()); }
        // Symlinks: the deepest existing ancestor INSIDE the root must still be
        // inside it. The walk stops at the root itself — above it there is
        // nothing to check, and walking past it refused every path inside a
        // permitted folder that did not exist yet (found by the S8 plan tests:
        // a plan that creates ~/KUE/notes before ~/KUE exists).
        let real_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        let mut probe = candidate.clone();
        loop {
            if let Ok(real) = std::fs::canonicalize(&probe) {
                if !real.starts_with(&real_root) {
                    return Err("That path leads outside the permitted folder through a link.".into());
                }
                break;
            }
            if probe == *root || !probe.pop() || probe.as_os_str().is_empty() { break; }
        }
        Ok(candidate)
    }
}

// MARK: - File executors (with verification)

fn ensure_root(roots: &PermittedRoots) -> Result<Option<String>, String> {
    let root = roots.0.first().ok_or("No permitted folder is configured.")?;
    if root.is_dir() { return Ok(None); }
    std::fs::create_dir_all(root).map_err(|e| format!("Could not create the permitted folder {}: {e}", root.display()))?;
    Ok(Some(format!("created the permitted folder {}", root.display())))
}

pub const MAX_READ_BYTES: u64 = 64 * 1024;
pub const MAX_WRITE_CHARS: usize = 20_000;

/// Runs a file action. Returns (state, reason, verification, output).
pub fn execute_file_action(kind: &ActionKind, roots: &PermittedRoots, home: &Path)
    -> (ActionState, Option<String>, Option<String>, Option<String>)
{
    use ActionState::*;
    let fail = |r: String| (Failed, Some(r), None, None);
    let created_root = match kind {
        ActionKind::CreateDirectory { .. } | ActionKind::CreateFile { .. } => match ensure_root(roots) {
            Ok(n) => n, Err(e) => return fail(e),
        },
        _ => None,
    };
    let prefix = |v: String| match &created_root { Some(n) => format!("{n}; {v}"), None => v };
    match kind {
        ActionKind::CreateDirectory { path } => {
            let p = match roots.resolve(path, home) { Ok(p) => p, Err(e) => return (Denied, Some(e), None, None) };
            if p.exists() { return fail(format!("{} already exists; nothing was changed.", p.display())); }
            if let Err(e) = std::fs::create_dir(&p) { return fail(format!("Could not create {}: {e}", p.display())); }
            match std::fs::metadata(&p) {
                Ok(m) if m.is_dir() => (Succeeded, None, Some(prefix(format!("{} exists and is a folder", p.display()))), None),
                _ => (UnknownResult, Some("The folder could not be found after creating it.".into()), None, None),
            }
        }
        ActionKind::CreateFile { path, text } => {
            let p = match roots.resolve(path, home) { Ok(p) => p, Err(e) => return (Denied, Some(e), None, None) };
            if text.chars().count() > MAX_WRITE_CHARS { return (Denied, Some("That text is too long for one file action.".into()), None, None); }
            let written = (|| -> std::io::Result<()> {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&p)?;
                f.write_all(text.as_bytes())?;
                f.sync_all()
            })();
            if let Err(e) = written {
                return fail(if e.kind() == std::io::ErrorKind::AlreadyExists {
                    format!("{} already exists; it was not overwritten.", p.display())
                } else { format!("Could not create {}: {e}", p.display()) });
            }
            match std::fs::read(&p) {
                Ok(back) if back == text.as_bytes() =>
                    (Succeeded, None, Some(prefix(format!("{} read back: {} bytes, identical", p.display(), back.len()))), None),
                Ok(back) => (PartiallySucceeded, Some("The file exists but its contents differ from what was written.".into()),
                    Some(format!("read back {} bytes, expected {}", back.len(), text.len())), None),
                Err(e) => (UnknownResult, Some(format!("The file could not be read back: {e}")), None, None),
            }
        }
        ActionKind::ReadPermittedFile { path } => {
            let p = match roots.resolve(path, home) { Ok(p) => p, Err(e) => return (Denied, Some(e), None, None) };
            let meta = match std::fs::metadata(&p) { Ok(m) => m, Err(e) => return fail(format!("{}: {e}", p.display())) };
            if !meta.is_file() { return fail(format!("{} is not a file.", p.display())); }
            if meta.len() > MAX_READ_BYTES { return (Denied, Some(format!("{} is larger than {} KB.", p.display(), MAX_READ_BYTES / 1024)), None, None); }
            match std::fs::read(&p) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => (Succeeded, None, Some(format!("read {} bytes from {}", s.len(), p.display())), Some(s)),
                    Err(_) => fail("That file is not text.".into()),
                },
                Err(e) => fail(format!("Could not read {}: {e}", p.display())),
            }
        }
        ActionKind::MovePermittedFile { from, to } => {
            let src = match roots.resolve(from, home) { Ok(p) => p, Err(e) => return (Denied, Some(e), None, None) };
            let mut dst = match roots.resolve(to, home) { Ok(p) => p, Err(e) => return (Denied, Some(e), None, None) };
            let meta = match std::fs::symlink_metadata(&src) { Ok(m) => m, Err(e) => return fail(format!("{}: {e}", src.display())) };
            if meta.file_type().is_symlink() { return (Denied, Some("Links are not moved.".into()), None, None); }
            if dst.is_dir() { if let Some(n) = src.file_name() { dst = dst.join(n); } }
            if dst.exists() { return fail(format!("{} already exists; nothing was overwritten.", dst.display())); }
            let size = meta.len();
            if let Err(e) = std::fs::rename(&src, &dst) { return fail(format!("Could not move: {e}")); }
            let gone = !src.exists();
            let there = std::fs::symlink_metadata(&dst).map(|m| m.len() == size || m.is_dir()).unwrap_or(false);
            match (gone, there) {
                (true, true) => (Succeeded, None, Some(format!("{} no longer exists; {} exists with the same size", src.display(), dst.display())), None),
                (false, true) => (PartiallySucceeded, Some("The destination exists but so does the source.".into()), None, None),
                _ => (UnknownResult, Some("The moved item could not be found at the destination.".into()), None, None),
            }
        }
        _ => fail("Not a file action.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parser_recognises_the_allowlist_and_nothing_else() {
        assert_eq!(parse_command("Open Safari"), Some(ActionKind::OpenApplication { name: "Safari".into() }));
        assert_eq!(parse_command("please launch the app Notes."), Some(ActionKind::OpenApplication { name: "Notes".into() }));
        assert_eq!(parse_command("open apple.com"), Some(ActionKind::OpenUrl { url: "apple.com".into() }));
        assert_eq!(parse_command("go to https://example.org/a?b=c"), Some(ActionKind::OpenUrl { url: "https://example.org/a?b=c".into() }));
        assert_eq!(parse_command("quit Xcode"), Some(ActionKind::CloseApplication { name: "Xcode".into() }));
        assert_eq!(parse_command("switch to Mail"), Some(ActionKind::FocusApplication { name: "Mail".into() }));
        assert_eq!(parse_command("create a folder called Projects"), Some(ActionKind::CreateDirectory { path: "Projects".into() }));
        assert_eq!(parse_command("create a file called notes.txt containing buy milk"),
            Some(ActionKind::CreateFile { path: "notes.txt".into(), text: "buy milk".into() }));
        assert_eq!(parse_command("read the file notes.txt"), Some(ActionKind::ReadPermittedFile { path: "notes.txt".into() }));
        assert_eq!(parse_command("move notes.txt to Projects"),
            Some(ActionKind::MovePermittedFile { from: "notes.txt".into(), to: "Projects".into() }));
        assert_eq!(parse_command("notify me that the build finished"),
            Some(ActionKind::ShowNotification { title: "KUE".into(), body: "the build finished".into() }));
        let doc = |q: &str| Some(ActionKind::OpenDocument { query: q.into(), path: None });
        // The request that failed on this Mac, spoken: it became OPEN_APPLICATION "any of my resume".
        assert_eq!(parse_command("Open any of my resume"), doc("resume"));
        assert_eq!(parse_command("Lantern, open my resume."), doc("resume"));
        // The invocation the owner actually says, and the name, typed as an address.
        assert_eq!(parse_command("Computer, open my resume."), doc("resume"));
        assert_eq!(parse_command("KUE, open my resume."), doc("resume"));
        // Only as an address: without the comma it is an ordinary word.
        assert_ne!(parse_command("computer science notes"), doc("resume"));
        assert_eq!(parse_command("open my latest resumes"), doc("resumes"));
        assert_eq!(parse_command("open resume.pdf"), doc("resume pdf"), "a document, not https://resume.pdf");
        assert_eq!(parse_command("open the file Budget 2026.xlsx"), doc("budget 2026 xlsx"));
        assert_eq!(parse_command("open my"), None);
        for question in ["what am I doing?", "who is in front of the camera", "run rm -rf /", "delete everything",
                         "execute ls", "grant me level 4", "disable the kill switch", "sudo open Safari", ""] {
            assert_eq!(parse_command(question), None, "{question:?} must not become an action");
        }
    }

    #[test]
    fn every_action_has_an_allowlisted_tag_and_a_risk() {
        let samples = [
            ActionKind::OpenApplication { name: "a".into() }, ActionKind::CloseApplication { name: "a".into() },
            ActionKind::FocusApplication { name: "a".into() }, ActionKind::OpenUrl { url: "a".into() },
            ActionKind::CreateDirectory { path: "a".into() }, ActionKind::CreateFile { path: "a".into(), text: "".into() },
            ActionKind::ReadPermittedFile { path: "a".into() }, ActionKind::MovePermittedFile { from: "a".into(), to: "b".into() },
            ActionKind::ShowNotification { title: "a".into(), body: "b".into() },
            ActionKind::OpenDocument { query: "a".into(), path: None },
            ActionKind::OpenDirectory { query: "a".into(), scope: None, path: None },
            ActionKind::ListDirectory { query: "a".into(), filter: "".into(), scope: None, path: None },
            ActionKind::InspectStorage,
            ActionKind::MoveToTrash { paths: vec!["a".into()] },
            ActionKind::RestoreFromTrash { items: vec![TrashedItem { original: "a".into(), trashed: "b".into() }] },
        ];
        for s in &samples { assert!(ALLOWLIST.contains(&s.tag())); let _ = risk(s); }
        assert_eq!(samples.len(), ALLOWLIST.len());
        assert_eq!(risk_from_tag("EXTREME"), None);
    }

    #[test]
    fn urls_are_http_only() {
        assert_eq!(validate_url("apple.com").unwrap(), "https://apple.com");
        assert_eq!(validate_url("localhost.test:8080/x").unwrap(), "https://localhost.test:8080/x");
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "vnc://host", "x-apple.systempreferences:com.apple",
                    "https://user@evil.com", "ssh://h", "https://"] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
    }

    fn sandbox(tag: &str) -> (PathBuf, PermittedRoots) {
        let home = std::env::temp_dir().join(format!("kue-act-{tag}-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&home).unwrap();
        let roots = PermittedRoots::default_for_home(&home);
        (home, roots)
    }

    #[test]
    fn paths_outside_the_permitted_folder_are_refused_including_through_links() {
        let (home, roots) = sandbox("escape");
        for bad in ["../x", "/etc/hosts", "~/Documents/secret.txt", "a/../../b"] {
            assert!(roots.resolve(bad, &home).is_err(), "{bad}");
        }
        std::fs::create_dir_all(home.join("KUE")).unwrap();
        std::fs::create_dir_all(home.join("outside")).unwrap();
        std::os::unix::fs::symlink(home.join("outside"), home.join("KUE/link")).unwrap();
        assert!(roots.resolve("link/file.txt", &home).is_err(), "a symlink out of the root is refused");
        assert_eq!(roots.resolve("notes.txt", &home).unwrap(), home.join("KUE/notes.txt"));
    }

    #[test]
    fn file_actions_verify_and_never_overwrite() {
        let (home, roots) = sandbox("files");
        let create = ActionKind::CreateFile { path: "a.txt".into(), text: "hello".into() };
        let (state, _, verification, _) = execute_file_action(&create, &roots, &home);
        assert_eq!(state, ActionState::Succeeded);
        assert!(verification.unwrap().contains("identical"));
        let (state, reason, _, _) = execute_file_action(&create, &roots, &home);
        assert_eq!(state, ActionState::Failed);
        assert!(reason.unwrap().contains("not overwritten"));
        assert_eq!(std::fs::read_to_string(home.join("KUE/a.txt")).unwrap(), "hello");

        let (state, _, _, out) = execute_file_action(&ActionKind::ReadPermittedFile { path: "a.txt".into() }, &roots, &home);
        assert_eq!((state, out.as_deref()), (ActionState::Succeeded, Some("hello")));

        assert_eq!(execute_file_action(&ActionKind::CreateDirectory { path: "Docs".into() }, &roots, &home).0, ActionState::Succeeded);
        let mv = ActionKind::MovePermittedFile { from: "a.txt".into(), to: "Docs".into() };
        assert_eq!(execute_file_action(&mv, &roots, &home).0, ActionState::Succeeded);
        assert!(home.join("KUE/Docs/a.txt").exists() && !home.join("KUE/a.txt").exists());

        let outside = ActionKind::CreateFile { path: "/tmp/kue-should-not-exist.txt".into(), text: "x".into() };
        assert_eq!(execute_file_action(&outside, &roots, &home).0, ActionState::Denied);
        assert!(!Path::new("/tmp/kue-should-not-exist.txt").exists());
    }

    #[test]
    fn documents_are_found_by_name_newest_first_and_nothing_else_is() {
        let (home, _) = sandbox("docs");
        let docs = DocumentRoots::default_for_home(&home);
        for d in ["Desktop", "Documents/Jobs/2025", "Downloads", "Desktop/Tools.app/Contents", "Documents/.hidden"] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        let write = |rel: &str| std::fs::write(home.join(rel), b"x").unwrap();
        write("Documents/Jobs/2025/Vijay_Resume_old.docx");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write("Desktop/vijay resume.pdf");
        write("Desktop/resume.command");            // runs on opening: never a document
        write("Desktop/Tools.app/Contents/resume.pdf"); // inside an app package
        write("Documents/.hidden/resume.pdf");      // hidden folder
        write("Downloads/notes.txt");
        std::os::unix::fs::symlink(home.join("Documents/Jobs/2025/Vijay_Resume_old.docx"), home.join("Desktop/resume-link.docx")).unwrap();

        let mut fw = crate::privacy::Firewall::new();
        let mut q = |s: &str| fw.clear_document_search(DocumentQuery::new(s), 0.0).expect("cleared for the interface");
        let found = docs.find(&q("resume"), 8);
        assert_eq!(found, vec![home.join("Desktop/vijay resume.pdf"), home.join("Documents/Jobs/2025/Vijay_Resume_old.docx")],
            "newest first; no script, package, hidden or linked file");
        assert!(docs.find(&q("resumes"), 8).len() == 2, "plural-insensitive");
        assert!(docs.find(&q("invoice"), 8).is_empty());

        assert!(docs.validate(home.join("Desktop/vijay resume.pdf").to_str().unwrap()).is_ok());
        for bad in ["Desktop/resume.command", "Desktop/resume-link.docx"] {
            assert!(docs.validate(home.join(bad).to_str().unwrap()).is_err(), "{bad}");
        }
        std::fs::write(home.join("outside.pdf"), b"x").unwrap();
        assert!(docs.validate(home.join("outside.pdf").to_str().unwrap()).is_err(), "outside the document folders");
        assert!(docs.validate("relative/resume.pdf").is_err());
    }

    #[test]
    fn success_without_verification_is_reported_as_unknown() {
        let mut r = ActionRecord { id: "a".into(), source: "TEXT".into(), action: ActionKind::OpenApplication { name: "x".into() },
            description: "".into(), risk: Risk::Low, state: ActionState::Executing, reason: None, verification: None,
            output: None, created_at: 0.0, updated_at: None, choices: vec![], steps: vec![], awaiting_since: None,
            task: None, sentences: None };
        r.finish(ActionState::Succeeded, None, None);
        assert_eq!(r.state, ActionState::UnknownResult);
        r.finish(ActionState::Succeeded, None, Some("pid 42 running".into()));
        assert_eq!(r.state, ActionState::Succeeded);
    }
}
