//! Requests of more than one step: "Open the Tampa folder on Desktop and tell me
//! what resumes are inside."
//!
//!   SENTENCE → CLAUSES ("and", "then") → each clause an allowlisted action
//!   → STEP 1 planned, authorized, confirmed if its risk needs it, executed, verified
//!   → only then STEP 2, with what step 1 found (the folder it opened), and so on.
//!
//! Every step is an ordinary action: its own privacy check, authorization,
//! confirmation and verification. A step runs only after the step before it
//! SUCCEEDED; if one does not, the rest are recorded as not run and never start.
//!
//! A request with a step KUE cannot do is not started at all. Typing into, or
//! reading from, another app's window ("open Calculator and add 2 and 2") needs
//! macOS Accessibility control, which KUE does not have: rather than open
//! Calculator and stop, KUE says so and does nothing.
//!
//! Step states, as the window shows them: PLANNED (no record yet), then the
//! record's own: AUTHORIZED, RUNNING (EXECUTING), SUCCEEDED, FAILED,
//! UNKNOWN_RESULT, CANCELLED, and BLOCKED (not run because an earlier step did not succeed).

use crate::actions::{self, ActionKind};
use crate::folders;
use serde::Serialize;

/// At most this many steps in one request.
pub const MAX_STEPS: usize = 4;

/// Starts the reason of a step that was not run.
pub const TASK_STOPPED: &str = "TASK_STOPPED";

#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    Single(ActionKind),
    Steps(Vec<ActionKind>),
    /// A multi-step request with a step KUE cannot do. Nothing is started.
    Unsupported { reason: &'static str },
    /// More than one thing is being asked for, and the grammar covers only
    /// part of it: the rest would be swallowed into a name. Nothing is done by
    /// rule — it goes to the planner, which proposes steps the owner approves.
    SeveralThings,
}

pub const CANNOT_TYPE_INTO_APPS: &str = "I can't do all of that yet, so I haven't started any of it. Opening apps, folders and \
    documents works, but typing into another app or reading what it shows isn't built: it needs macOS Accessibility control, \
    which KUE doesn't have.";

/// Verbs that act inside another app's window. Recognised only so that a request
/// using one is refused as a whole instead of half done.
const IN_APP_VERBS: [&str; 22] = ["calculate", "compute", "sum", "add", "subtract", "multiply", "divide", "type", "enter",
    "write", "press", "click", "tap", "scroll", "fill", "submit", "search", "select", "copy", "paste", "drag", "play"];

/// One clause: a folder request first ("folder" is a stronger signal than an app name), then the action parser.
pub fn parse_one(text: &str) -> Option<ActionKind> {
    if crate::storage::parse_storage_request(text) {
        return Some(ActionKind::InspectStorage);
    }
    if let Some((query, scope)) = folders::parse_open_folder(text) {
        return Some(ActionKind::OpenDirectory { query, scope, path: None });
    }
    if let Some((query, filter, scope)) = folders::parse_listing(text) {
        return Some(ActionKind::ListDirectory { query, filter, scope, path: None });
    }
    actions::parse_command(text)
}

/// Splits at "and", "and then", "then". Byte positions come from an ASCII
/// lower-casing, so they are valid in the original text.
fn clauses(text: &str) -> Vec<String> {
    const SEPARATORS: [&str; 6] = [", and then ", " and then ", ", then ", " then ", ", and ", " and "];
    let mut out = Vec::new();
    let mut rest = text.trim().trim_end_matches(['.', '!', '?']).to_string();
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = SEPARATORS.iter().filter_map(|s| lower.find(s).map(|i| (i, s.len()))).min_by_key(|(i, _)| *i);
        match next {
            Some((i, len)) if out.len() + 1 < MAX_STEPS + 1 => {
                out.push(rest[..i].trim().to_string());
                rest = rest[i + len..].trim().to_string();
            }
            _ => { out.push(rest.trim().to_string()); break; }
        }
    }
    out.retain(|c| !c.is_empty());
    out
}

/// Verbs that begin an instruction. A clause starting with one is another
/// thing being asked for, never part of the name before it.
const TAIL_VERBS: [&str; 26] = ["put", "add", "create", "make", "write", "move", "save", "copy", "name", "call",
    "open", "show", "list", "send", "notify", "read", "set", "organize", "organise", "sort", "tidy", "clean",
    "rename", "fill", "delete", "remove"];

/// What a parsed action would use as a NAME — a folder, a file, an app, a
/// query. Free text the owner dictated (a notification's words, a file's
/// contents) is not a name and is left out: "and" belongs in it.
fn named(kind: &ActionKind) -> Vec<&str> {
    match kind {
        ActionKind::CreateDirectory { path } | ActionKind::ReadPermittedFile { path } => vec![path],
        ActionKind::CreateFile { path, .. } => vec![path],
        ActionKind::MovePermittedFile { from, to } => vec![from, to],
        ActionKind::OpenApplication { name } | ActionKind::CloseApplication { name } | ActionKind::FocusApplication { name } => vec![name],
        ActionKind::OpenDocument { query, .. } | ActionKind::OpenDirectory { query, .. } => vec![query],
        ActionKind::ListDirectory { query, .. } => vec![query],
        ActionKind::OpenUrl { .. } | ActionKind::ShowNotification { .. } | ActionKind::InspectStorage
        | ActionKind::MoveToTrash { .. } | ActionKind::RestoreFromTrash { .. } => vec![],
    }
}

/// Whether parsing the whole sentence as one command swallowed a second
/// instruction into a name. "Make a folder called Reports and put a note in
/// it" is two things, and the folder is not called "Reports and put a note in
/// it"; "notify me that bread and milk are due" is one thing whose words
/// happen to contain "and".
fn swallowed_an_instruction(kind: &ActionKind) -> bool {
    named(kind).iter().any(|name| {
        let lower = format!(" {} ", name.to_lowercase());
        [" and ", " then "].iter().any(|sep| match lower.split_once(sep) {
            Some((_, tail)) => tail.split_whitespace().next().is_some_and(|v| TAIL_VERBS.contains(&v)),
            None => false,
        })
    })
}

fn in_app_verb(clause: &str) -> bool {
    let l = clause.trim().to_lowercase();
    let first = l.split_whitespace().next().unwrap_or("");
    IN_APP_VERBS.contains(&first)
}

/// The plan for a sentence, or None when it is not a command (a question for the model).
pub fn plan(text: &str) -> Option<Plan> {
    let parts = clauses(text);
    if parts.len() >= 2 && parts.len() <= MAX_STEPS {
        let mut steps = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            match parse_one(part) {
                Some(kind) => steps.push(kind),
                // "…and add 2 and 2": the first part was a command; this part acts inside an app.
                None if i > 0 && !steps.is_empty() && in_app_verb(part) => {
                    return Some(Plan::Unsupported { reason: CANNOT_TYPE_INTO_APPS });
                }
                None => { steps.clear(); break; }
            }
        }
        if steps.len() == parts.len() { return Some(Plan::Steps(steps)); }
    }
    // One command — including one whose own text says "and" ("notify me that
    // bread and milk are due"). But a name that carries another instruction is
    // not a name: the sentence asks for more than this grammar can do, and
    // doing the first half of it would be worse than not starting.
    let single = parse_one(text)?;
    if parts.len() >= 2 && swallowed_an_instruction(&single) { return Some(Plan::SeveralThings); }
    Some(Plan::Single(single))
}

/// A step as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskStepView {
    pub index: usize,
    pub kind: &'static str,
    /// PLANNED · AUTHORIZED · WAITING_FOR_CONFIRMATION · WAITING_FOR_YOU · RUNNING · SUCCEEDED · FAILED
    /// · UNKNOWN_RESULT · CANCELLED · BLOCKED — from the step's action record when it has one.
    pub state: &'static str,
    /// The plan step's own state (`goal::StepState`).
    pub step: crate::goal::StepState,
    /// What this step established, in KUE's words. Names no file.
    pub said: Option<String>,
    pub action_id: Option<String>,
}

/// A goal as the window shows it: its kind, its state, and each step. No targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskView {
    pub id: String,
    pub goal: &'static str,
    pub state: crate::goal::GoalState,
    pub steps: Vec<TaskStepView>,
    /// What KUE is waiting for, when it is waiting for the owner.
    pub waiting: Option<&'static str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sentence_asking_for_more_than_one_thing_is_not_done_by_halves() {
        // Live 2026-09-23: this made a folder named "Reports in my KUE folder
        // and put a note in it". It is two things, and belongs to the planner.
        for said in ["Make a folder called Reports in my KUE folder and put a note in it.",
                     "create a folder called Reports and put a note in it",
                     "make a folder called Trip and put the tickets in it"] {
            assert_eq!(plan(said), Some(Plan::SeveralThings), "{said}");
        }
        // One thing whose own words contain "and" is still that one thing.
        assert_eq!(plan("notify me that bread and milk are due"),
            Some(Plan::Single(ActionKind::ShowNotification { title: "KUE".into(), body: "bread and milk are due".into() })));
        assert_eq!(plan("create a folder called Rock and Roll"),
            Some(Plan::Single(ActionKind::CreateDirectory { path: "Rock and Roll".into() })));
        assert_eq!(plan("create a file called notes.txt containing milk and bread"),
            Some(Plan::Single(ActionKind::CreateFile { path: "notes.txt".into(), text: "milk and bread".into() })));
        // A clause that asks KUE to act inside another app is refused as a
        // whole, and says so — that check comes first and is unchanged.
        for said in ["create a folder called Trip then add a list", "create a folder called Notes and write the date in it"] {
            assert!(matches!(plan(said), Some(Plan::Unsupported { .. })), "{said}");
        }
        // Several things KUE CAN parse are still a task of its own, with a
        // step for each — nothing is swallowed, so nothing goes to a model.
        assert!(matches!(plan("open Safari and open Notes"), Some(Plan::Steps(_))));
        assert!(matches!(plan("make a folder called Archive and move the old files into it"), Some(Plan::Steps(_))));
    }

    #[test]
    fn a_sentence_of_several_commands_is_a_plan_of_steps() {
        let p = plan("Open the Tampa folder on Desktop and tell me what resumes are inside").unwrap();
        assert_eq!(p, Plan::Steps(vec![
            ActionKind::OpenDirectory { query: "tampa".into(), scope: Some("Desktop".into()), path: None },
            ActionKind::ListDirectory { query: "".into(), filter: "resumes".into(), scope: None, path: None },
        ]));
        assert_eq!(plan("open Safari, then open Notes").unwrap(), Plan::Steps(vec![
            ActionKind::OpenApplication { name: "Safari".into() }, ActionKind::OpenApplication { name: "Notes".into() }]));
    }

    #[test]
    fn a_step_kue_cannot_do_stops_the_whole_request_before_it_starts() {
        for s in ["Open Calculator and sum 2 + 2", "open calculator and then calculate 2+2", "open Notes and type hello",
                  "Open Calculator and add 2 and 2"] {
            assert_eq!(plan(s), Some(Plan::Unsupported { reason: CANNOT_TYPE_INTO_APPS }), "{s}");
        }
    }

    #[test]
    fn and_inside_one_command_is_not_a_second_step() {
        assert_eq!(plan("notify me that bread and milk are due"),
            Some(Plan::Single(ActionKind::ShowNotification { title: "KUE".into(), body: "bread and milk are due".into() })));
        assert_eq!(plan("what is the difference between cats and dogs"), None, "a question, not a command");
        assert_eq!(plan("open Safari"), Some(Plan::Single(ActionKind::OpenApplication { name: "Safari".into() })));
    }
}
