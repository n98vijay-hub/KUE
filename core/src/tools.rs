//! What KUE can DO, declared.
//!
//! The capability registry describes KUE to a person. This describes it to the
//! runtime: every operation the governed pipeline may execute, with the
//! contract it executes under — what it takes, what it returns, what it touches,
//! how risky it is, who may authorize it, what must be true first, what runs it,
//! how its result is checked, and how it is undone.
//!
//! THREE RULES, each enforced by a test rather than by hope:
//!
//! 1. **Only what exists is declared.** Every declaration names a capability
//!    row, and that row may not be NOT_IMPLEMENTED. There is no declaration for
//!    web search, typing, clicking or calendar, because KUE cannot do them.
//! 2. **Nothing executes undeclared.** Every action tag the broker allows has
//!    exactly one declaration, and the broker refuses anything without one.
//! 3. **Nothing is written twice.** Risk comes from `actions::risk`, the
//!    authorization operation from `actions::operation_for`, whether a confirm
//!    is needed from `actions::needs_confirmation`, and the level from
//!    `authz::requirement`. A declaration can never disagree with the code
//!    that actually decides, because it does not hold its own copy.
//!
//! This is also the list a model will one day *choose from* — it may propose a
//! tool by id and arguments; the governed runtime decides whether it runs. No
//! model reaches any of this today.

use crate::actions::{self, ActionKind, Risk};
use crate::authz::Operation;
use crate::capabilities::{self, Proof};
use crate::context::CapabilityStatus;
use crate::privacy::DataKind;
use serde::Serialize;

/// The type of one input or output field, for a caller — or a future model —
/// that must construct a valid call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldType {
    Text,
    /// A path the runtime resolves and re-checks; never trusted as given.
    Path,
    /// Several paths, each re-checked.
    Paths,
    /// A name resolved against something installed or found.
    Name,
    Url,
    Number,
    Bool,
    /// A sentence written by KUE for the owner.
    Sentence,
    /// A structured report produced by KUE.
    Report,
}

impl FieldType {
    pub fn tag(self) -> &'static str {
        match self {
            FieldType::Text => "text", FieldType::Path => "path", FieldType::Paths => "list of paths",
            FieldType::Name => "name", FieldType::Url => "url", FieldType::Number => "number",
            FieldType::Bool => "true or false", FieldType::Sentence => "sentence", FieldType::Report => "report",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Field {
    pub name: &'static str,
    pub ty: FieldType,
    pub required: bool,
    pub note: &'static str,
}

const fn f(name: &'static str, ty: FieldType, required: bool, note: &'static str) -> Field {
    Field { name, ty, required, note }
}

/// What must hold before a tool may run. Checked by the runtime at execution,
/// not only when the request was understood — the world can change in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Precondition {
    /// KUE is not killed. Every tool.
    NotKilled,
    /// KUE is not paused. Every tool that touches the Mac.
    NotPaused,
    /// The executor helper is present in the bundle.
    ExecutorPresent,
    /// The application is installed, resolved by name.
    ApplicationInstalled,
    /// The target lies inside Desktop, Documents, Downloads or ~/KUE, after
    /// resolving links.
    InsideAllowedFolders,
    /// The target lies inside ~/KUE, KUE's own folder.
    InsideKueFolder,
    /// The file was found by KUE's own storage check, and is still there, still
    /// a regular file, and still inside the folder it was found in.
    OfferedByStorageCheck,
    /// Moved to the Trash by KUE this session, and its old place is empty.
    PreviouslyTrashedByKue,
    /// http or https only.
    WebAddress,
}

/// What performs a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind", content = "detail")]
pub enum Executor {
    /// A verb of the separate KueAct helper, which does one thing and reports
    /// what it saw afterwards.
    KueAct(&'static str),
    /// Rust code in the core, inside KUE's own process.
    InProcess(&'static str),
    /// Arithmetic and rules: no system call at all.
    Rule(&'static str),
}

/// How a tool is undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind", content = "detail")]
pub enum Rollback {
    /// Undone by another declared tool.
    By(&'static str),
    /// Nothing changed, so there is nothing to undo.
    NothingChanged,
    /// It cannot be undone by KUE, and says why.
    NotPossible(&'static str),
}

/// What the owner — and the runtime — may believe about a tool right now.
/// Derived from evidence, never set by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolStatus {
    /// Code exists; nothing has checked it.
    Implemented,
    /// Automated tests pass; it has not been seen working on this Mac.
    Tested,
    /// Seen working on this Mac, with a record.
    LiveVerified,
    /// Part of it has been seen working; part has not.
    Partial,
}

/// One declared tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    /// Stable. For an action, the broker's own tag.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub input: &'static [Field],
    pub output: &'static [Field],
    /// The registry row whose proof and status this tool answers to.
    pub capability: &'static str,
    /// Every kind of data the tool reads or produces. The privacy firewall, not
    /// this list, is what enforces handling; this is what the owner is told.
    pub privacy: &'static [DataKind],
    pub preconditions: &'static [Precondition],
    pub executor: Executor,
    /// What is read back after it runs, and what counts as it having happened.
    pub verifier: &'static str,
    pub rollback: Rollback,
    /// Seen working on this Mac: the date and what was seen, from the event
    /// log or a live test. None means nobody has seen it work.
    pub live: Option<(&'static str, &'static str)>,
    pub version: u32,
}

impl ToolSpec {
    /// A representative call, so risk is computed by `actions::risk` — the
    /// function the broker uses — rather than copied into this table.
    fn sample(&self) -> Option<ActionKind> {
        let s = String::new;
        Some(match self.id {
            "OPEN_APPLICATION" => ActionKind::OpenApplication { name: s() },
            "CLOSE_APPLICATION" => ActionKind::CloseApplication { name: s() },
            "FOCUS_APPLICATION" => ActionKind::FocusApplication { name: s() },
            "OPEN_URL" => ActionKind::OpenUrl { url: s() },
            "CREATE_DIRECTORY" => ActionKind::CreateDirectory { path: s() },
            "CREATE_FILE" => ActionKind::CreateFile { path: s(), text: s() },
            "READ_PERMITTED_FILE" => ActionKind::ReadPermittedFile { path: s() },
            "MOVE_PERMITTED_FILE" => ActionKind::MovePermittedFile { from: s(), to: s() },
            "SHOW_NOTIFICATION" => ActionKind::ShowNotification { title: s(), body: s() },
            "OPEN_DOCUMENT" => ActionKind::OpenDocument { query: s(), path: None },
            "OPEN_DIRECTORY" => ActionKind::OpenDirectory { query: s(), scope: None, path: None },
            "LIST_DIRECTORY" => ActionKind::ListDirectory { query: s(), filter: s(), scope: None, path: None },
            "INSPECT_STORAGE" => ActionKind::InspectStorage,
            "MOVE_TO_TRASH" => ActionKind::MoveToTrash { paths: vec![] },
            "RESTORE_FROM_TRASH" => ActionKind::RestoreFromTrash { items: vec![] },
            _ => return None,
        })
    }

    /// The risk the broker will assign. None for a tool that is not an action.
    pub fn risk(&self) -> Option<Risk> { self.sample().map(|k| actions::risk(&k)) }

    /// The authorization operation the runtime will ask for. A rule-only tool
    /// needs none beyond KUE running.
    pub fn operation(&self) -> Option<Operation> { self.risk().map(actions::operation_for) }

    /// Whether the owner must explicitly confirm before it runs.
    pub fn needs_confirmation(&self) -> bool { self.risk().map(actions::needs_confirmation).unwrap_or(false) }

    /// What may be believed about it, from the evidence the registry and the
    /// declaration hold. Never higher than the capability row it answers to.
    pub fn status(&self) -> ToolStatus {
        let row = capabilities::find(self.capability);
        let row_partial = row.is_some_and(|r| r.status == CapabilityStatus::Partial);
        match (self.live, row.map(|r| r.proof)) {
            (Some(_), _) if row_partial => ToolStatus::Partial,
            (Some(_), _) => ToolStatus::LiveVerified,
            (None, Some(Proof::TestVerifiedOnly | Proof::PartlyLiveVerified { .. } | Proof::LiveVerified { .. })) => ToolStatus::Tested,
            (None, _) => ToolStatus::Implemented,
        }
    }
}

/// Why a tool cannot run right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "state", content = "reason")]
pub enum Availability {
    Available,
    Unavailable(String),
}

/// What the runtime knows that decides availability.
#[derive(Debug, Clone, Copy, Default)]
pub struct Conditions {
    pub killed: bool,
    pub paused: bool,
    pub executor_present: bool,
}

impl ToolSpec {
    /// Whether this tool can run now. Separate from authorization: an available
    /// tool still has to be authorized, confirmed and verified.
    pub fn availability(&self, c: Conditions) -> Availability {
        use Precondition::*;
        if c.killed && self.preconditions.contains(&NotKilled) {
            return Availability::Unavailable("KUE is stopped.".into());
        }
        if c.paused && self.preconditions.contains(&NotPaused) {
            return Availability::Unavailable("KUE is paused.".into());
        }
        if !c.executor_present && self.preconditions.contains(&ExecutorPresent) {
            return Availability::Unavailable("The part of KUE that acts on this Mac is missing.".into());
        }
        Availability::Available
    }
}

use Precondition as P;
use FieldType as T;

const ACT: &[Precondition] = &[P::NotKilled, P::NotPaused, P::ExecutorPresent];

/// Every tool KUE has. Nothing else may execute.
pub static TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "INSPECT_STORAGE",
        name: "Check storage",
        description: "Measures the drive and walks Desktop, Documents, Downloads and ~/KUE for what is using space. Reads sizes and dates; opens nothing, moves nothing.",
        input: &[],
        output: &[f("report", T::Report, true, "volume totals, findings by category, each with evidence and a caution"),
                  f("summary", T::Sentence, true, "what KUE says about it, naming no file")],
        capability: "storage_inspection",
        privacy: &[DataKind::StorageInventory, DataKind::StorageSummary],
        preconditions: &[P::NotKilled, P::NotPaused],
        executor: Executor::InProcess("storage::take_inventory"),
        verifier: "The volume measurement comes from statfs and is compared with the walk; a folder that could not be read is named, and a partial pass says so.",
        rollback: Rollback::NothingChanged,
        live: Some(("2026-09-20", "820 findings, 5.0 GB, 27k files measured on this Mac by the live test")),
        version: 1,
    },
    ToolSpec {
        id: "MOVE_TO_TRASH",
        name: "Move to Trash",
        description: "Moves files the owner chose from KUE's own storage check to the Trash, one at a time. Never deletes. Refuses anything the check did not offer.",
        input: &[f("paths", T::Paths, true, "each must have been offered by the most recent storage check")],
        output: &[f("landed", T::Paths, true, "where each file now is in the Trash, as macOS named it"),
                  f("summary", T::Sentence, true, "how many moved; never a claim about space freed")],
        capability: "storage_cleanup",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::OfferedByStorageCheck],
        executor: Executor::KueAct("trash"),
        verifier: "Each file is absent from where it was AND present in the Trash; anything else is FAILED or UNKNOWN_RESULT for that file.",
        rollback: Rollback::By("RESTORE_FROM_TRASH"),
        live: Some(("2026-09-20", "Round trip on KUE's own files through the executor passed live; the window path with Touch ID has not been seen")),
        version: 1,
    },
    ToolSpec {
        id: "RESTORE_FROM_TRASH",
        name: "Put back",
        description: "Puts back files KUE moved to the Trash, exactly where they were. Refuses if something now occupies that place.",
        input: &[f("items", T::Paths, true, "each must have been moved by KUE this session")],
        output: &[f("summary", T::Sentence, true, "how many are back")],
        capability: "storage_cleanup",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::PreviouslyTrashedByKue],
        executor: Executor::KueAct("untrash"),
        verifier: "Each file is present at its old place and absent from the Trash.",
        rollback: Rollback::By("MOVE_TO_TRASH"),
        live: Some(("2026-09-20", "Restored KUE's own files in the live round trip")),
        version: 1,
    },
    ToolSpec {
        id: "OPEN_APPLICATION",
        name: "Open an app",
        description: "Opens an installed application by name.",
        input: &[f("name", T::Name, true, "resolved against the applications installed on this Mac")],
        output: &[f("summary", T::Sentence, true, "which app is now in front")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::ApplicationInstalled],
        executor: Executor::KueAct("open-app"),
        verifier: "The application is running and frontmost.",
        rollback: Rollback::By("CLOSE_APPLICATION"),
        live: Some(("2026-09-15", "7 successes recorded in the event log, spoken and typed (Lantern-era build)")),
        version: 1,
    },
    ToolSpec {
        id: "CLOSE_APPLICATION",
        name: "Quit an app",
        description: "Asks an application to quit, the ordinary way. Never force-quits, and never quits KUE itself.",
        input: &[f("name", T::Name, true, "a running application")],
        output: &[f("summary", T::Sentence, true, "whether it quit, or is still asking to save")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::ApplicationInstalled],
        executor: Executor::KueAct("close-app"),
        verifier: "The application is no longer running; if it is still open (for example asking to save), UNKNOWN_RESULT.",
        rollback: Rollback::By("OPEN_APPLICATION"),
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "FOCUS_APPLICATION",
        name: "Switch to an app",
        description: "Brings a running application to the front.",
        input: &[f("name", T::Name, true, "a running application")],
        output: &[f("summary", T::Sentence, true, "which app is now in front")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: ACT,
        executor: Executor::KueAct("focus-app"),
        verifier: "The application is frontmost.",
        rollback: Rollback::NothingChanged,
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "OPEN_URL",
        name: "Open a web address",
        description: "Hands an http or https address to the default browser. KUE itself fetches nothing.",
        input: &[f("url", T::Url, true, "http or https only")],
        output: &[f("summary", T::Sentence, true, "which browser opened it")],
        capability: "computer_automation",
        privacy: &[DataKind::Url],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::WebAddress],
        executor: Executor::KueAct("open-url"),
        verifier: "A browser is frontmost after the hand-off. What the page then does is not KUE's to verify.",
        rollback: Rollback::NotPossible("A page, once opened, has been fetched by the browser."),
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "OPEN_DOCUMENT",
        name: "Open a document",
        description: "Finds a document by name in the allowed folders and opens it in its default app, after the owner confirms which one.",
        input: &[f("query", T::Text, true, "the words the owner used"),
                 f("path", T::Path, false, "the match the owner confirmed")],
        output: &[f("summary", T::Sentence, true, "which document is open, and in what")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::InsideAllowedFolders],
        executor: Executor::KueAct("open-file"),
        verifier: "The document's app is frontmost after opening.",
        rollback: Rollback::NotPossible("An opened document may already have been read or changed by its app."),
        live: Some(("2026-09-15", "A typed request opened a document after confirmation (Lantern-era build)")),
        version: 1,
    },
    ToolSpec {
        id: "OPEN_DIRECTORY",
        name: "Open a folder",
        description: "Opens a folder in Finder: Desktop, Documents, Downloads, ~/KUE, or a folder found inside them.",
        input: &[f("query", T::Text, true, "the folder's name"),
                 f("scope", T::Text, false, "which allowed folder to look inside")],
        output: &[f("summary", T::Sentence, true, "which folder is open")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::ExecutorPresent, P::InsideAllowedFolders],
        executor: Executor::KueAct("open-folder"),
        verifier: "Finder is frontmost after opening.",
        rollback: Rollback::NothingChanged,
        live: Some(("2026-09-15", "A folder opened from the window (Lantern-era build)")),
        version: 1,
    },
    ToolSpec {
        id: "LIST_DIRECTORY",
        name: "List a folder",
        description: "Reads the names, kinds and dates in one allowed folder, at most fifty. Opens nothing.",
        input: &[f("query", T::Text, true, "the folder's name"),
                 f("filter", T::Text, false, "what kind of item to list")],
        output: &[f("items", T::Report, true, "names, kinds and dates")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::InsideAllowedFolders],
        executor: Executor::InProcess("transaction::list_directory"),
        verifier: "The listing is what the file system returned at that moment.",
        rollback: Rollback::NothingChanged,
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "CREATE_DIRECTORY",
        name: "Make a folder in ~/KUE",
        description: "Creates a folder inside ~/KUE, and nowhere else.",
        input: &[f("path", T::Path, true, "a name inside KUE's folder, like Reports or Reports/July — never a full path")],
        output: &[f("summary", T::Sentence, true, "that the folder exists")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::InsideKueFolder],
        executor: Executor::InProcess("actions::execute_file_action"),
        verifier: "The folder exists afterwards.",
        rollback: Rollback::NotPossible("KUE does not delete, so a created folder stays until the owner removes it."),
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "CREATE_FILE",
        name: "Write a file in ~/KUE",
        description: "Creates a new text file inside ~/KUE with the owner's words. Refuses to overwrite.",
        input: &[f("path", T::Path, true, "a name inside KUE's folder, like Reports/note.txt — never a full path, and not existing yet"),
                 f("text", T::Text, true, "what to write")],
        output: &[f("summary", T::Sentence, true, "that the file exists with that content")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::InsideKueFolder],
        executor: Executor::InProcess("actions::execute_file_action"),
        verifier: "The file exists afterwards and its content reads back equal.",
        rollback: Rollback::NotPossible("KUE does not delete, so a created file stays until the owner removes it."),
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "READ_PERMITTED_FILE",
        name: "Read a file in ~/KUE",
        description: "Reads a text file inside ~/KUE and shows it to the owner.",
        input: &[f("path", T::Path, true, "a name inside KUE's folder, like Reports/note.txt — never a full path")],
        output: &[f("text", T::Text, true, "shown to the owner only")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::InsideKueFolder],
        executor: Executor::InProcess("actions::execute_file_action"),
        verifier: "What was read is what the file system returned.",
        rollback: Rollback::NothingChanged,
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "MOVE_PERMITTED_FILE",
        name: "Move a file within ~/KUE",
        description: "Moves a file from one place in ~/KUE to another. Refuses to overwrite.",
        input: &[f("from", T::Path, true, "a name inside KUE's folder, like old/note.txt — never a full path"),
                 f("to", T::Path, true, "a name inside KUE's folder, not existing yet")],
        output: &[f("summary", T::Sentence, true, "where it now is")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: &[P::NotKilled, P::NotPaused, P::InsideKueFolder],
        executor: Executor::InProcess("actions::execute_file_action"),
        verifier: "Absent at the old path and present at the new one.",
        rollback: Rollback::By("MOVE_PERMITTED_FILE"),
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "SHOW_NOTIFICATION",
        name: "Show a notification",
        description: "Shows a macOS notification now. Not a reminder: KUE cannot schedule one for later.",
        input: &[f("title", T::Text, false, "defaults to KUE"), f("body", T::Text, true, "the words")],
        output: &[f("summary", T::Sentence, true, "that it was delivered, or that notifications are not allowed")],
        capability: "computer_automation",
        privacy: &[DataKind::ActionTarget],
        preconditions: ACT,
        executor: Executor::KueAct("notify"),
        verifier: "macOS accepted the notification; PERMISSION_REQUIRED if notifications are off for KUE.",
        rollback: Rollback::NotPossible("A notification, once shown, has been seen."),
        live: None,
        version: 1,
    },
    ToolSpec {
        id: "CALCULATE",
        name: "Calculate",
        description: "Arithmetic worked out here, in exact fractions: numbers, + − × ÷, percentages, brackets, whole-number powers and square roots. A root that is not exact is bracketed between two fractions that are squared to prove its digits, and the answer is said as about. The result is read back before it is said.",
        input: &[f("expression", T::Text, true, "in words or symbols")],
        output: &[f("result", T::Number, true, "exact, then rounded for speech"),
                  f("summary", T::Sentence, true, "the sentence KUE says")],
        capability: "arithmetic",
        privacy: &[DataKind::OwnerMessage],
        preconditions: &[P::NotKilled],
        executor: Executor::Rule("calculate::evaluate"),
        verifier: "The result is parsed back from the sentence and compared with the exact value; a root is proved by squaring the two fractions it lies between.",
        rollback: Rollback::NothingChanged,
        live: None,
        version: 2,
    },
];

/// The tools a plan may use, written for a model to read: the id, what it
/// does, and the exact fields of its input. Built from the declarations, so a
/// model is never told about a tool the runtime would refuse to run — and a
/// tool it invents anyway is caught by `plan::validate`.
///
/// Says nothing about the owner: no paths, no files, no context. Tools that
/// change or remove things are listed with the others; risk, authorization and
/// confirmation are the runtime's, and a proposal that states any of them is
/// refused unread.
pub fn catalogue() -> String {
    let mut out = String::new();
    for t in TOOLS {
        let fields: Vec<String> = t.input.iter()
            .map(|f| format!("{}: {} ({}{})", f.name, f.ty.tag(), if f.required { "required" } else { "optional" },
                             if f.note.is_empty() { String::new() } else { format!(", {}", f.note) }))
            .collect();
        out.push_str(&format!("- {} — {} Input: {}\n", t.id, t.description,
                              if fields.is_empty() { "none".to_string() } else { fields.join("; ") }));
    }
    out
}

/// The declaration for an id, or None — and None means the runtime does not run it.
pub fn declared(id: &str) -> Option<&'static ToolSpec> { TOOLS.iter().find(|t| t.id == id) }

/// Tools as a person — or later a model — reads them: id, what it does, what it
/// takes, how risky, whether it asks first, and what is known about it. Only
/// declared tools appear, so nothing absent can be offered.
#[derive(Debug, Clone, Serialize)]
pub struct ToolView {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub input: &'static [Field],
    pub risk: Option<Risk>,
    pub asks_first: bool,
    pub status: ToolStatus,
    pub availability: Availability,
    pub rollback: Rollback,
}

pub fn views(c: Conditions) -> Vec<ToolView> {
    TOOLS.iter().map(|t| ToolView {
        id: t.id, name: t.name, description: t.description, input: t.input,
        risk: t.risk(), asks_first: t.needs_confirmation(), status: t.status(),
        availability: t.availability(c), rollback: t.rollback,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_the_broker_allows_is_declared_exactly_once_and_nothing_else_is() {
        for tag in actions::ALLOWLIST {
            assert_eq!(TOOLS.iter().filter(|t| t.id == tag).count(), 1, "{tag} is allowed but not declared exactly once");
        }
        for t in TOOLS {
            if t.sample().is_some() {
                assert!(actions::ALLOWLIST.contains(&t.id), "{} is declared as an action the broker does not allow", t.id);
            }
        }
    }

    #[test]
    fn only_what_exists_is_declared() {
        for t in TOOLS {
            let row = capabilities::find(t.capability)
                .unwrap_or_else(|| panic!("{} answers to capability {} which does not exist", t.id, t.capability));
            assert_ne!(row.status, CapabilityStatus::NotImplemented,
                "{} is declared, but its capability {} is NOT_IMPLEMENTED", t.id, t.capability);
        }
        // And the things KUE cannot do have no declaration to be chosen by.
        for absent in ["WEB_SEARCH", "BROWSER_NAVIGATION", "TYPE_TEXT", "CLICK_TARGET",
                       "CALENDAR_READ", "CALENDAR_CREATE", "SEND_MESSAGE", "DELETE_FILE", "READ_DOCUMENT"] {
            assert!(declared(absent).is_none(), "{absent} is declared, and KUE cannot do it");
        }
    }

    #[test]
    fn risk_and_authorization_come_from_the_code_that_decides_them() {
        for t in TOOLS {
            let Some(kind) = t.sample() else { continue };
            assert_eq!(t.risk(), Some(actions::risk(&kind)), "{}", t.id);
            assert_eq!(t.operation(), Some(actions::operation_for(actions::risk(&kind))), "{}", t.id);
            assert_eq!(t.needs_confirmation(), actions::needs_confirmation(actions::risk(&kind)), "{}", t.id);
        }
        let trash = declared("MOVE_TO_TRASH").unwrap();
        assert_eq!(trash.risk(), Some(Risk::High), "moving the owner's files is HIGH, reversible or not");
        assert!(trash.needs_confirmation());
        assert_eq!(trash.operation(), Some(Operation::ActionHighRisk), "which needs Touch ID or the password");
    }

    #[test]
    fn a_tool_that_changes_something_says_how_it_is_undone() {
        for t in TOOLS {
            if let Rollback::By(other) = t.rollback {
                assert!(declared(other).is_some(), "{} is undone by {other}, which is not declared", t.id);
            }
            if matches!(t.risk(), Some(Risk::High)) {
                assert!(!matches!(t.rollback, Rollback::NothingChanged), "{} is HIGH risk and claims to change nothing", t.id);
            }
        }
        assert_eq!(declared("MOVE_TO_TRASH").unwrap().rollback, Rollback::By("RESTORE_FROM_TRASH"));
    }

    #[test]
    fn a_tool_is_never_believed_more_than_its_evidence() {
        for t in TOOLS {
            let row = capabilities::find(t.capability).unwrap();
            if t.live.is_none() {
                assert_ne!(t.status(), ToolStatus::LiveVerified, "{} claims live verification with no record", t.id);
            }
            if row.status == CapabilityStatus::Partial {
                assert_ne!(t.status(), ToolStatus::LiveVerified, "{} is more certain than its capability", t.id);
            }
        }
        // Concretely: quitting apps and notifications have never been seen working.
        assert_eq!(declared("CLOSE_APPLICATION").unwrap().status(), ToolStatus::Tested);
        assert_eq!(declared("SHOW_NOTIFICATION").unwrap().status(), ToolStatus::Tested);
        assert_eq!(declared("OPEN_APPLICATION").unwrap().status(), ToolStatus::LiveVerified);
    }

    #[test]
    fn kill_and_pause_make_tools_unavailable_and_say_why() {
        let open = declared("OPEN_APPLICATION").unwrap();
        let ok = Conditions { killed: false, paused: false, executor_present: true };
        assert_eq!(open.availability(ok), Availability::Available);
        assert!(matches!(open.availability(Conditions { killed: true, ..ok }), Availability::Unavailable(_)));
        assert!(matches!(open.availability(Conditions { paused: true, ..ok }), Availability::Unavailable(_)));
        assert!(matches!(open.availability(Conditions { executor_present: false, ..ok }), Availability::Unavailable(_)));
        // Every tool obeys the kill switch.
        for t in TOOLS {
            assert!(t.preconditions.contains(&Precondition::NotKilled), "{} ignores the kill switch", t.id);
            assert!(matches!(t.availability(Conditions { killed: true, ..ok }), Availability::Unavailable(_)), "{}", t.id);
        }
    }

    #[test]
    fn the_declarations_read_as_a_contract_and_hold_no_content() {
        let v = serde_json::to_string(&views(Conditions { executor_present: true, ..Default::default() })).unwrap();
        assert!(v.contains("\"MOVE_TO_TRASH\"") && v.contains("\"asks_first\":true"));
        for t in TOOLS {
            assert!(!t.description.is_empty() && !t.verifier.is_empty(), "{} has an empty contract", t.id);
        }
    }
}
