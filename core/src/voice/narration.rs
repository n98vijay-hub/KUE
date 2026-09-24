//! The execution narrator: turns an action's real state into a short sentence.
//!
//! Every sentence is written here, by KUE, from the record's state — never by a
//! model and never from the executor's free text (which can name a path). Each
//! moment is said at most once per action, the same sentence is not repeated
//! within a few seconds, and verbosity decides which moments are said at all.
//!
//! Narration follows what actually happened. Steps that take microseconds
//! (the privacy check, a folder search) are not performed as speech; "Opening
//! it now" is said only once an action is executing, and success only once the
//! record is SUCCEEDED — which already requires verification evidence. What a
//! success sentence claims is limited to what the executor verified: a
//! document or link is "handed to its app", not "on your screen", because
//! whether the window shows it is not verified.

use super::reference::{plural, spoken_count, spoken_file_name, spoken_kind};
use super::{InputSource, Priority, SpeechDraft, Verbosity};
use crate::actions::{ActionKind, ActionRecord, ActionState, ActionStep, PERMISSION_REQUIRED};
use crate::conversation::{Exchange, TurnOutcome, CAPABILITY_LIST_SOURCE, COMMAND_PARSER_SOURCE};
use crate::privacy::DataKind;
use crate::transaction::{AUTHORIZATION_REQUIRED, CANCELLED_BY_YOU, KILLED, NEED_A_FOLDER};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

/// The moments of an action that can be said out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Moment {
    CheckingAccess,
    WaitingForConfirmation,
    WaitingForStrongAuth,
    Executing,
    Succeeded,
    Failed,
    UnknownResult,
    Cancelled,
    AuthorizationRequired,
    AuthorizationExpired,
    PrivacyDenied,
    NoMatches,
    NotAllowed,
}

impl Moment {
    /// None for states that are not said (PROPOSED, AUTHORIZED, a kill or lock).
    pub fn of(rec: &ActionRecord) -> Option<Moment> {
        let reason = rec.reason.as_deref().unwrap_or("");
        Some(match rec.state {
            ActionState::Proposed | ActionState::Authorized => return None,
            ActionState::Reauthorizing => Moment::CheckingAccess,
            ActionState::RequiresConfirmation => Moment::WaitingForConfirmation,
            ActionState::RequiresStrongAuth => Moment::WaitingForStrongAuth,
            ActionState::Executing => Moment::Executing,
            ActionState::Succeeded => Moment::Succeeded,
            ActionState::Failed | ActionState::PartiallySucceeded => Moment::Failed,
            ActionState::UnknownResult => Moment::UnknownResult,
            // Cancelled by a lock or a kill is not announced: the session is no longer yours.
            ActionState::Cancelled if reason == CANCELLED_BY_YOU => Moment::Cancelled,
            ActionState::Cancelled => return None,
            ActionState::AuthorizationExpired => Moment::AuthorizationExpired,
            ActionState::PrivacyDenied => Moment::PrivacyDenied,
            ActionState::NoMatches => Moment::NoMatches,
            ActionState::Denied if reason.starts_with(KILLED) => return None,
            ActionState::Denied if reason.starts_with(AUTHORIZATION_REQUIRED) => Moment::AuthorizationRequired,
            ActionState::Denied => Moment::NotAllowed,
        })
    }

    fn said_at(self, verbosity: Verbosity, rec: &ActionRecord) -> bool {
        match self {
            Moment::CheckingAccess => verbosity >= Verbosity::Detailed,
            // A LOW action runs straight away; its outcome follows within a
            // second, so at BRIEF only the outcome is said.
            Moment::Executing => verbosity >= Verbosity::Normal
                || (verbosity >= Verbosity::Brief && rec.steps.contains(&ActionStep::AwaitingConfirmation)),
            _ => verbosity >= Verbosity::Brief,
        }
    }
}

/// A name the owner SAID, safe to say back. None for typed requests, redacted
/// records, and targets never read aloud (links, paths, file text, notifications).
fn spoken_name(rec: &ActionRecord) -> Option<String> {
    if rec.source != "VOICE" { return None; }
    let name = match &rec.action {
        ActionKind::OpenApplication { name } | ActionKind::CloseApplication { name } | ActionKind::FocusApplication { name } => name.clone(),
        ActionKind::OpenDocument { query, .. } => format!("your {query}"),
        // The folder's own name once it is found (never its location), else what you said.
        ActionKind::OpenDirectory { query, scope, path } => match (path.as_deref().and_then(spoken_file_name), query.as_str(), scope) {
            (Some(n), _, _) => format!("the {n} folder"),
            (None, "", Some(s)) => format!("your {s} folder"),
            (None, q, _) if !q.is_empty() => format!("the {q} folder"),
            _ => return None,
        },
        _ => return None,
    };
    let n = name.trim();
    (!n.is_empty() && n != "your").then(|| n.to_string())
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// What a folder listing found, as said: how many, and the newest by name.
/// Names only for a request you spoke; a typed one is pointed to the screen.
fn listing_sentence(rec: &ActionRecord, voice: bool) -> String {
    let ActionKind::ListDirectory { filter, .. } = &rec.action else { return String::new() };
    let n = rec.choices.len();
    let f = filter.trim();
    let singular = if f.len() > 3 && f.ends_with('s') && !f.ends_with("ss") { &f[..f.len() - 1] } else { f };
    let many = if f.is_empty() { "items".to_string() } else { plural(singular) };
    if !voice {
        return match n {
            0 => "Nothing in that folder matches.".into(),
            1 => "I found one matching item. It's listed on screen.".into(),
            _ => format!("I found {} matching items. They're listed on screen.", spoken_count(n)),
        };
    }
    let newest = rec.choices.first().map(|p| match (spoken_file_name(p), spoken_kind(p)) {
        (Some(name), Some(kind)) => format!("{name}, {kind}"),
        (Some(name), None) => name,
        _ => String::new(),
    }).unwrap_or_default();
    match (n, f.is_empty(), newest.is_empty()) {
        (0, true, _) => "It's empty.".into(),
        (0, false, _) => format!("There are no {many} in it."),
        (1, true, false) => format!("There's one item: {newest}."),
        (1, false, false) => format!("I found one {singular}: {newest}."),
        (1, _, true) => format!("I found one {}. It's listed on screen.", if f.is_empty() { "item" } else { singular }),
        (_, _, false) => format!("I found {} {many}. The latest one is {newest}.", spoken_count(n)),
        (_, _, true) => format!("I found {} {many}. They're listed on screen.", spoken_count(n)),
    }
}

/// The sentence for a moment, and whether it names the target.
/// "one file", "three files".
pub fn files(n: usize) -> String {
    if n == 1 { "one file".into() } else { format!("{} files", spoken_count(n)) }
}

fn sentence(m: Moment, rec: &ActionRecord) -> Option<(String, bool)> {
    let name = spoken_name(rec);
    let listed = matches!(rec.action, ActionKind::ListDirectory { .. }) && rec.source == "VOICE" && !rec.choices.is_empty();
    let named = name.is_some() || listed;
    let folder = matches!(rec.action, ActionKind::OpenDirectory { .. } | ActionKind::ListDirectory { .. });
    let doc = matches!(rec.action, ActionKind::OpenDocument { .. });
    let voice = rec.source == "VOICE";
    let text = match m {
        Moment::CheckingAccess => "Checking your access.".to_string(),
        Moment::WaitingForConfirmation if doc => {
            let ask = if voice { " Should I open it?" } else { " Confirm to open it." };
            let query = match &rec.action { ActionKind::OpenDocument { query, .. } => query.trim().to_string(), _ => String::new() };
            let selected = match &rec.action { ActionKind::OpenDocument { path: Some(p), .. } => Some(p.as_str()), _ => None };
            let count = rec.choices.len();
            match (&name, selected) {
                // Spoken, several matches: how many, and the newest by its name (never its folder or extension).
                (Some(_), Some(p)) if count > 1 && !query.is_empty() => {
                    let newest = match (spoken_file_name(p), spoken_kind(p)) {
                        (Some(f), Some(k)) => format!("The newest is {f}, {k}."),
                        (Some(f), None) => format!("The newest is {f}."),
                        _ => "The newest one is selected.".into(),
                    };
                    format!("I found {} {}. {newest}{ask}", spoken_count(count), plural(&query))
                }
                (Some(n), _) => format!("I found {n}.{ask}"),
                (None, _) if count > 1 => format!("I found a matching document. The newest one is selected.{ask}"),
                (None, _) => format!("I found a matching document.{ask}"),
            }
        }
        // An app name that matched more than one installed app: a question, not a guess.
        Moment::WaitingForConfirmation if folder && rec.needs_choice() => {
            if voice { format!("I found {} folders with that name. Choose one on screen.", spoken_count(rec.choices.len())) }
            else { "More than one folder matches that name. Choose one on screen.".into() }
        }
        Moment::WaitingForConfirmation if rec.action.app_name().is_some() && !rec.choices.is_empty() => {
            if voice {
                let list = match rec.choices.as_slice() {
                    [a, b] => format!("{a} or {b}"),
                    many => format!("{}, or {}", many[..many.len() - 1].join(", "), many[many.len() - 1]),
                };
                format!("Which one do you mean: {list}?")
            } else {
                "More than one app matches that name. Choose one on screen.".into()
            }
        }
        Moment::WaitingForConfirmation => {
            let what = match (&rec.action, &name) {
                (ActionKind::CloseApplication { .. }, Some(n)) => format!("quit {n}"),
                (ActionKind::CloseApplication { .. }, None) => "quit that app".into(),
                (ActionKind::OpenUrl { .. }, _) => "open that link".into(),
                (ActionKind::CreateDirectory { .. }, _) => "create that folder".into(),
                (ActionKind::CreateFile { .. }, _) => "create that file".into(),
                (ActionKind::ReadPermittedFile { .. }, _) => "read that file".into(),
                (ActionKind::MovePermittedFile { .. }, _) => "move that file".into(),
                (ActionKind::ShowNotification { .. }, _) => "show that notification".into(),
                (_, Some(n)) => format!("open {n}"),
                _ => "do that".into(),
            };
            if voice { format!("Should I {what}?") } else { format!("I need your confirmation to {what}.") }
        }
        // A move to the Trash says how many first, in the words the
        // conversation shows: "Moving one file to the Trash. Touch ID is required."
        Moment::WaitingForStrongAuth => match &rec.action {
            ActionKind::MoveToTrash { paths } => format!("Moving {} to the Trash. {}",
                files(paths.len()), crate::transaction::TOUCH_ID_REQUIRED),
            _ => "That needs Touch ID or your password. Confirm it on screen.".into(),
        },
        Moment::Executing => match (&rec.action, &name) {
            (ActionKind::OpenApplication { .. }, Some(n)) => format!("Opening {n}."),
            (ActionKind::CloseApplication { .. }, Some(n)) => format!("Quitting {n}."),
            (ActionKind::FocusApplication { .. }, Some(n)) => format!("Switching to {n}."),
            (ActionKind::OpenDirectory { .. }, Some(n)) => format!("Opening {n}."),
            (ActionKind::OpenDirectory { .. }, None) => "Opening the folder.".into(),
            (ActionKind::ListDirectory { .. }, _) => "Looking inside.".into(),
            (ActionKind::InspectStorage, _) => "Checking your storage.".into(),
            (ActionKind::MoveToTrash { .. }, _) => "Moving them to the Trash.".into(),
            (ActionKind::RestoreFromTrash { .. }, _) => "Putting them back.".into(),
            (ActionKind::OpenApplication { .. } | ActionKind::OpenDocument { .. } | ActionKind::OpenUrl { .. }, _) => "Opening it now.".into(),
            _ => "Working on it.".into(),
        },
        Moment::Succeeded => match (&rec.action, &name) {
            (ActionKind::OpenApplication { .. }, Some(n)) => format!("{} is open.", capitalized(n)),
            (ActionKind::OpenApplication { .. }, None) => "The app is open.".into(),
            (ActionKind::FocusApplication { .. }, Some(n)) => format!("Switched to {n}."),
            (ActionKind::FocusApplication { .. }, None) => "Switched.".into(),
            (ActionKind::CloseApplication { .. }, Some(n)) => format!("{} has quit.", capitalized(n)),
            (ActionKind::CloseApplication { .. }, None) => "The app has quit.".into(),
            (ActionKind::OpenDocument { .. }, Some(n)) => format!("I've handed {n} to its app."),
            (ActionKind::OpenDocument { .. }, None) => "I've handed the document to its app.".into(),
            (ActionKind::OpenUrl { .. }, _) => "I've handed the link to your browser.".into(),
            (ActionKind::CreateDirectory { .. }, _) => "The folder is created.".into(),
            (ActionKind::CreateFile { .. }, _) => "The file is created.".into(),
            (ActionKind::ReadPermittedFile { .. }, _) => "It's on screen.".into(),
            (ActionKind::MovePermittedFile { .. }, _) => "The file is moved.".into(),
            (ActionKind::ShowNotification { .. }, _) => "The notification is delivered.".into(),
            (ActionKind::OpenDirectory { .. }, Some(n)) => format!("{} is open.", capitalized(n)),
            (ActionKind::OpenDirectory { .. }, None) => "The folder is open.".into(),
            (ActionKind::ListDirectory { .. }, _) => listing_sentence(rec, voice),
            // The numbers are the outcome, and they exist only in the record
            // core wrote when it measured them. Without them KUE says it
            // looked, and claims nothing about what it found.
            // The numbers, and what became of each file, exist only in the
            // record core wrote when it did the work.
            (ActionKind::InspectStorage | ActionKind::MoveToTrash { .. } | ActionKind::RestoreFromTrash { .. }, _) =>
                match (&rec.sentences, &rec.action) {
                    (Some(s), _) if voice => s.aloud.clone(),
                    (Some(s), _) => s.on_screen.clone(),
                    (None, ActionKind::MoveToTrash { .. }) => "The files are in the Trash.".into(),
                    (None, ActionKind::RestoreFromTrash { .. }) => "They're back where they were.".into(),
                    (None, _) => "I've taken stock of your storage.".into(),
                },
        },
        Moment::Failed if permission_required(rec) => "I need macOS permission to do that.".into(),
        Moment::Failed if doc => "I found it, but macOS didn't open it.".into(),
        Moment::Failed => match (&rec.action, &name) {
            (ActionKind::OpenApplication { .. }, Some(n)) => format!("I couldn't open {n}."),
            (ActionKind::FocusApplication { .. }, Some(n)) => format!("I couldn't switch to {n}."),
            (ActionKind::CloseApplication { .. }, Some(n)) => format!("{} didn't quit.", capitalized(n)),
            (ActionKind::OpenDirectory { .. }, Some(n)) => format!("I couldn't open {n}."),
            _ => "I couldn't complete that.".into(),
        },
        Moment::UnknownResult => "I can't confirm whether that finished.".into(),
        Moment::Cancelled => "Cancelled.".into(),
        Moment::AuthorizationRequired => "I need you to authenticate first.".into(),
        Moment::AuthorizationExpired => "Your authorization expired. Please authenticate again.".into(),
        Moment::PrivacyDenied => "I can't safely handle that request.".into(),
        Moment::NoMatches if folder && rec.reason.as_deref().is_some_and(|r| r.starts_with(NEED_A_FOLDER)) =>
            "Which folder? Say its name.".into(),
        Moment::NoMatches if folder => match (&rec.action, voice) {
            (ActionKind::OpenDirectory { query, .. } | ActionKind::ListDirectory { query, .. }, true) if !query.is_empty() =>
                format!("I couldn't find a folder called {query}."),
            _ => "I couldn't find that folder.".into(),
        },
        Moment::NoMatches => match (&rec.action, &name) {
            (ActionKind::OpenApplication { .. }, Some(n)) => format!("I couldn't find an app called {n}."),
            (ActionKind::OpenApplication { .. }, None) => "I couldn't find that app.".into(),
            (_, Some(n)) => format!("I couldn't find {n}."),
            (_, None) => "I couldn't find a matching file.".into(),
        },
        Moment::NotAllowed => "I'm not allowed to do that.".into(),
    };
    Some((text, named))
}

/// A failure the executor attributed to a macOS permission (`actions::PERMISSION_REQUIRED`).
fn permission_required(rec: &ActionRecord) -> bool {
    rec.reason.as_deref().is_some_and(|r| r.starts_with(PERMISSION_REQUIRED))
}

/// What a moment's sentence is built from.
fn carries(m: Moment, rec: &ActionRecord, named: bool) -> Vec<DataKind> {
    // Sentences that say only that you must authenticate, or that something was
    // refused or cancelled, carry no data and may be said to whoever is there.
    if matches!(m, Moment::CheckingAccess | Moment::Cancelled | Moment::AuthorizationRequired
        | Moment::AuthorizationExpired | Moment::PrivacyDenied) || (m == Moment::Failed && permission_required(rec)) {
        return Vec::new();
    }
    // Anything else says what happened to one of your actions (as an event
    // would), and sometimes what it was for or how many matches there were.
    let mut k = vec![DataKind::EventRecord];
    if named || rec.choices.len() > 1 { k.push(DataKind::ActionTarget); }
    k
}

const REPEAT_WINDOW_SECONDS: f64 = 4.0;

#[derive(Debug, Default)]
pub struct Narrator {
    said: BTreeMap<String, Moment>,
    recent: VecDeque<(String, f64)>,
}

impl Narrator {
    pub fn new() -> Self { Self::default() }

    /// A sentence for this record's state, or None if it is not to be said.
    /// Give it the record as the window may show it, not the raw one.
    pub fn on_action(&mut self, rec: &ActionRecord, verbosity: Verbosity, now: f64) -> Option<SpeechDraft> {
        let m = Moment::of(rec)?;
        if self.said.get(&rec.id) == Some(&m) { return None; }
        // A new moment for this action replaces the old one either way, so a
        // moment skipped at this verbosity is not said later by accident.
        self.said.insert(rec.id.clone(), m);
        if !m.said_at(verbosity, rec) { return None; }
        let (text, named) = sentence(m, rec)?;
        let priority = match m {
            Moment::CheckingAccess | Moment::Executing => Priority::ActionProgress,
            Moment::WaitingForConfirmation | Moment::WaitingForStrongAuth | Moment::AuthorizationRequired
            | Moment::AuthorizationExpired => Priority::AuthorizationRequired,
            Moment::Succeeded | Moment::Failed | Moment::UnknownResult | Moment::Cancelled | Moment::PrivacyDenied
            | Moment::NoMatches | Moment::NotAllowed => Priority::ActionResult,
        };
        self.fresh(SpeechDraft::new(text, &carries(m, rec, named), priority, rec.id.clone()), now)
    }

    /// An action picked again after it had already been narrated (a different
    /// match chosen): its moments may be said again.
    pub fn forget(&mut self, action_id: &str) { self.said.remove(action_id); }

    /// A finished answer, spoken when you asked by voice (or asked for typed
    /// answers to be read). A corrected answer is followed by its correction.
    pub fn on_answer(&mut self, x: &Exchange, speak_typed: bool, verbosity: Verbosity, now: f64) -> Option<SpeechDraft> {
        if verbosity == Verbosity::Silent || (x.source != InputSource::Voice && !speak_typed) { return None; }
        let (text, kinds): (String, &[DataKind]) = match x.outcome {
            TurnOutcome::Failed => (x.answer.clone(), &[]),
            TurnOutcome::Answered if x.model == COMMAND_PARSER_SOURCE => (x.answer.clone(), &[]),
            TurnOutcome::Answered if x.model == CAPABILITY_LIST_SOURCE => (x.answer.clone(), &[DataKind::SensorState]),
            TurnOutcome::Answered => {
                let mut t = x.answer.clone();
                for c in &x.corrections { t.push(' '); t.push_str(c); }
                (t, &[DataKind::ModelAnswer])
            }
        };
        self.fresh(SpeechDraft::new(text, kinds, Priority::GeneralInformation, "answer"), now)
    }

    fn fresh(&mut self, d: SpeechDraft, now: f64) -> Option<SpeechDraft> {
        while self.recent.front().is_some_and(|(_, t)| now - t > REPEAT_WINDOW_SECONDS) { self.recent.pop_front(); }
        if d.text.trim().is_empty() || self.recent.iter().any(|(t, _)| *t == d.text) { return None; }
        self.recent.push_back((d.text.clone(), now));
        Some(d)
    }
}

/// The sentence for a record's current state, for the window's live status
/// line. Uses the same words as speech, with no de-duplication or verbosity.
pub fn describe(rec: &ActionRecord) -> Option<String> {
    Moment::of(rec).and_then(|m| sentence(m, rec)).map(|(t, _)| t)
}

/// The window's one-line status: what KUE is doing with your latest action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveStatus {
    /// CHECKING_ACCESS · WAITING_FOR_CONFIRMATION · EXECUTING · DONE · CANCELLED · NEEDS_ATTENTION
    pub phase: &'static str,
    pub line: String,
    pub action_id: String,
}

/// From records the window may already show, picked by `actions::current` — the
/// same rule the interface projection uses.
pub fn live_status(records: &[ActionRecord], now: f64) -> Option<LiveStatus> {
    // One rule for "the current action", shared with the interface projection,
    // so the spoken line and the window never name different actions.
    let r = crate::actions::current(records, now, crate::surface::OUTCOME_SECONDS)
        .filter(|r| Moment::of(r).is_some())?;
    let phase = match r.state {
        ActionState::Reauthorizing => "CHECKING_ACCESS",
        ActionState::RequiresConfirmation | ActionState::RequiresStrongAuth => "WAITING_FOR_CONFIRMATION",
        ActionState::Executing => "EXECUTING",
        ActionState::Succeeded => "DONE",
        ActionState::Cancelled => "CANCELLED",
        _ => "NEEDS_ATTENTION",
    };
    Some(LiveStatus { phase, line: describe(r)?, action_id: r.id.clone() })
}
