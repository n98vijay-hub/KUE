//! One action, from request to verified result, as a transaction.
//!
//!   PROPOSED → PRIVACY_CHECKED → AUTHORIZED → REQUIRES_CONFIRMATION
//!     → (Confirm) REAUTHORIZING → REAUTHORIZED → EXECUTING → SUCCEEDED (verified)
//!
//! Every way an action can stop is explicit and has its own message:
//!   DENIED                 not authorized when planned, a target refused by policy, or KUE killed
//!   NO_MATCHES             no document matched the request
//!   PRIVACY_DENIED         the privacy firewall refused the target
//!   AUTHORIZATION_EXPIRED  authorized when planned, not any more when confirmed
//!   FAILED · CANCELLED · UNKNOWN_RESULT
//!
//! This module is the only route from a request to the executor. Authorization
//! comes from the engine's access session, never from the interface, the
//! record, or a model; Confirm re-authorizes from scratch; the kill switch is
//! checked once more immediately before anything happens on the Mac.

use crate::actions::{self, ActionKind, ActionRecord, ActionState, ActionStep, DocumentQuery, DocumentRoots, PermittedRoots, TaskStepRef, TrashedItem};
use crate::goal::{Authority, Blueprint, Decision as GoalDecision, Goal, GoalKind, GoalState, StepEvent, StepKind, StepState};
use crate::intent::{self, Understanding, Work};
use crate::task::{TaskStepView, TaskView};
use crate::voice::InputSource;
use crate::apps::{AppCatalog, AppResolution};
use crate::authz::{AuthLevel, Decision, OsAuthKind, Operation};
use crate::engine::Engine;
use crate::privacy::{DataKind, Destination, Firewall};
use crate::storage::{self, StorageRequest, StorageReport, VolumeUsage};
use crate::runtime::Principal;
use crate::voice::narration::{live_status, LiveStatus};
use crate::voice::reference::{self, Reference};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const AUTHORIZATION_EXPIRED: &str = "Authorization expired. Please re-authenticate.";
pub const PRIVACY_DENIED: &str = "The privacy firewall refused this action's target. Nothing was searched or run.";
/// KUE was asked to move a file it had not found and shown.
pub const NOT_OFFERED: &str = "KUE only moves files it found and showed you. Take a look at your storage first, then choose from what it found.";
pub const KILLED: &str = "KUE is killed. Nothing runs until you recover it.";
pub const WITHHELD: &str = "The privacy firewall withheld this action's details from the window.";
pub const CANCELLED_BY_YOU: &str = "Cancelled by you.";
/// Starts every refusal for want of authorization (narration keys on it).
pub const AUTHORIZATION_REQUIRED: &str = "Authorization required";

// MARK: - This session's actions

/// This session's action records. Held in memory only; targets are never
/// written to local memory or sent to a model.
#[derive(Debug, Default)]
pub struct ActionBook {
    records: Vec<ActionRecord>,
    next: u64,
    /// Requests of more than one step, as goals. In memory for this session only.
    goals: Vec<Goal>,
    next_goal: u64,
    /// The folder most recently opened or listed: what "inside" and "it" refer to.
    last_folder: Option<String>,
    /// The last time KUE took stock of storage, for the window to show. In
    /// memory for this session only: it names your files, and its
    /// classification (StorageInventory) keeps it out of local memory.
    last_storage: Option<crate::storage::StorageReport>,
    /// What KUE's last move to the Trash moved, and where macOS put each one,
    /// so it can be undone exactly. This session only.
    last_trashed: Vec<TrashedItem>,
    /// Plans that were proposed and checked, waiting for the owner's yes or
    /// already approved. In memory for this session only: a proposal names
    /// files, so it is never written down or sent to a model.
    plans: Vec<HeldPlan>,
    next_plan: u64,
}

/// A checked plan, held between "here is what I'd do" and "do it".
///
/// The steps the owner was shown are `plan.steps`; the actions that would run
/// are private and are never shown, serialized or spoken. Approving binds to
/// the plan's id, and the goal it becomes is built from exactly these actions
/// — so a step that was not in the approved plan cannot be added to it later.
#[derive(Debug, Clone)]
pub struct HeldPlan {
    pub plan: crate::plan::Plan,
    /// Who proposed it (a model's name, or KUE itself).
    pub proposer: String,
    /// VOICE or TEXT, for the goal it becomes.
    pub source: String,
    /// The goal that carries it out, once approved.
    pub goal_id: Option<String>,
    /// The proposal as written, kept to check the moment again at approval.
    /// Private: it names the owner's files.
    json: String,
    executor_present: bool,
    actions: Vec<ActionKind>,
    /// The plan this one was changed from, and how many versions in it is.
    /// A changed plan is a new plan: this is provenance, never authority.
    pub revision_of: Option<String>,
    pub version: u32,
    /// The memories that were put in front of whoever proposed this — ATTACHED
    /// BY KUE after retrieval, never read out of the proposal. A model writing
    /// a field like this would be refused by the parser, which takes no
    /// unknown fields.
    pub memory_refs: Vec<String>,
}

impl HeldPlan {
    pub fn approved(&self) -> bool { !self.plan.approved_by.is_empty() }
    pub fn step_tools(&self) -> Vec<&'static str> { self.plan.steps.iter().map(|s| s.step).collect() }

    /// What each step would actually do, named. This is the plan the owner
    /// agrees to, so the owner is shown it; it names the owner's own files, so
    /// it is never sent to a model and never spoken into a room.
    pub fn step_details(&self) -> Vec<String> { self.actions.iter().map(|a| a.describe()).collect() }

    /// Where the plan would write, as the plan names it — relative to KUE's
    /// own folder unless it says otherwise. For a caller that has to know
    /// which places a plan would touch before it runs.
    pub fn step_paths(&self) -> Vec<String> {
        self.actions.iter().filter_map(|a| match a {
            ActionKind::CreateDirectory { path } | ActionKind::CreateFile { path, .. } => Some(path.clone()),
            ActionKind::MovePermittedFile { to, .. } => Some(to.clone()),
            _ => None,
        }).collect()
    }

    /// The words that mean each step, for matching "don't create the folder"
    /// to the step that creates a folder. Built here, from the plan KUE holds.
    pub fn step_words(&self) -> Vec<crate::dialogue::StepWords> {
        const NOT_A_NAME: [&str; 21] = ["the", "a", "an", "in", "to", "from", "of", "and", "on", "it", "its",
                                        "with", "your", "contents", "characters", "matching", "everything",
                                        "default", "browser", "show", "kue"];
        self.actions.iter().enumerate().map(|(i, a)| {
            let mut words: Vec<String> = a.describe()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 1 && !w.chars().all(|c| c.is_numeric()))
                .map(str::to_lowercase)
                .filter(|w| !NOT_A_NAME.contains(&w.as_str()))
                .collect();
            words.sort();
            words.dedup();
            crate::dialogue::StepWords { index: i, words }
        }).collect()
    }
}

impl ActionBook {
    pub fn new() -> Self { Self::default() }
    pub fn next_id(&mut self) -> String { self.next += 1; format!("a{}", self.next) }
    pub fn records(&self) -> &[ActionRecord] { &self.records }
    pub fn get(&self, id: &str) -> Option<ActionRecord> { self.records.iter().find(|r| r.id == id).cloned() }
    pub fn last_folder(&self) -> Option<&str> { self.last_folder.as_deref() }
    /// The last time KUE took stock of storage, for the window. None until it has.
    pub fn last_storage(&self) -> Option<&StorageReport> { self.last_storage.as_ref() }
    /// What the last move to the Trash moved. Empty when there is nothing to undo.
    pub fn last_trashed(&self) -> &[TrashedItem] { &self.last_trashed }

    /// This session's goals, oldest first.
    pub fn goals(&self) -> &[Goal] { &self.goals }
    /// Plans proposed this session, newest last.
    pub fn plans(&self) -> &[HeldPlan] { &self.plans }
    pub fn plan(&self, id: &str) -> Option<&HeldPlan> { self.plans.iter().find(|p| p.plan.plan_id == id) }
    fn plan_mut(&mut self, id: &str) -> Option<&mut HeldPlan> { self.plans.iter_mut().find(|p| p.plan.plan_id == id) }
    /// The approved plan a goal is carrying out, if it is carrying one out.
    fn approved_plan_of_goal(&mut self, goal_id: &str) -> Option<&mut HeldPlan> {
        self.plans.iter_mut().find(|p| p.goal_id.as_deref() == Some(goal_id) && !p.plan.approved_by.is_empty())
    }
    /// Holds a checked plan. At most four are kept; the oldest finished one goes first.
    fn hold_plan(&mut self, mut held: HeldPlan) -> String {
        self.next_plan += 1;
        let id = format!("p{}", self.next_plan);
        held.plan.plan_id = id.clone();
        if self.plans.len() >= 4 {
            if let Some(i) = self.plans.iter().position(|p| p.goal_id.is_some() || p.plan.state == crate::plan::PlanState::Cancelled) {
                self.plans.remove(i);
            } else { self.plans.remove(0); }
        }
        self.plans.push(held);
        id
    }
    pub fn goal(&self, id: &str) -> Option<&Goal> { self.goals.iter().find(|g| g.id == id) }
    fn goal_mut(&mut self, id: &str) -> Option<&mut Goal> { self.goals.iter_mut().find(|g| g.id == id) }

    /// The id the next goal will get. Used to tie an approved plan to its goal
    /// BEFORE the goal starts — the first step runs inside `start_goal`, and it
    /// has to know it belongs to an approved plan.
    fn next_goal_id(&self) -> String { format!("g{}", self.next_goal + 1) }

    fn new_goal(&mut self, bp: Blueprint, source: &str, now: f64) -> String {
        self.next_goal += 1;
        let id = format!("g{}", self.next_goal);
        self.goals.push(Goal::new(id.clone(), source, bp, now));
        // At most ten; the oldest that has ended goes first.
        if self.goals.len() > 10 {
            let i = self.goals.iter().position(|g| !g.is_open()).unwrap_or(0);
            self.goals.remove(i);
        }
        id
    }

    /// Whether the last storage report was made for a goal created at `created_at`.
    fn report_since(&self, created_at: f64) -> bool {
        self.last_storage.as_ref().is_some_and(|r| r.measured_at >= created_at)
    }

    /// The goals as the window shows them. A step that drives an action record
    /// takes its state from that record, as it always has; any other step from
    /// the plan's own state machine. No targets.
    pub fn task_views(&self) -> Vec<TaskView> {
        self.goals.iter().map(|g| {
            let mut stopped = false;
            let steps = g.steps.iter().map(|s| {
                let by_record = matches!(s.kind, StepKind::Act(_) | StepKind::MoveChosenToTrash);
                let rec = if by_record {
                    self.records.iter().rev().find(|r| r.task.as_ref().is_some_and(|x| x.task_id == g.id && x.index == s.index))
                } else { None };
                let state = match rec.map(|r| r.state) {
                    Some(st) => match st {
                        ActionState::Proposed => "PLANNED",
                        ActionState::Authorized => "AUTHORIZED",
                        ActionState::RequiresConfirmation | ActionState::RequiresStrongAuth | ActionState::Reauthorizing => "WAITING_FOR_CONFIRMATION",
                        ActionState::Executing => "RUNNING",
                        ActionState::Succeeded => "SUCCEEDED",
                        ActionState::Failed | ActionState::PartiallySucceeded | ActionState::NoMatches => "FAILED",
                        ActionState::UnknownResult => "UNKNOWN_RESULT",
                        ActionState::Cancelled => "CANCELLED",
                        ActionState::Denied | ActionState::AuthorizationExpired | ActionState::PrivacyDenied => "BLOCKED",
                    },
                    None => match s.state {
                        StepState::Pending | StepState::Ready if stopped => "BLOCKED",
                        StepState::Pending | StepState::Ready => "PLANNED",
                        StepState::Running | StepState::Verifying => "RUNNING",
                        StepState::WaitingForUser => "WAITING_FOR_YOU",
                        StepState::WaitingForAuthorization => "WAITING_FOR_CONFIRMATION",
                        StepState::Completed => "SUCCEEDED",
                        StepState::Failed => "FAILED",
                        StepState::Blocked => "BLOCKED",
                        StepState::Cancelled => "CANCELLED",
                    },
                };
                if !matches!(state, "PLANNED" | "AUTHORIZED" | "WAITING_FOR_CONFIRMATION" | "WAITING_FOR_YOU" | "RUNNING" | "SUCCEEDED") { stopped = true; }
                TaskStepView { index: s.index, kind: s.kind.tag(), state, step: s.state, said: s.said.clone(),
                               action_id: rec.map(|r| r.id.clone()).or_else(|| s.action_id.clone()) }
            }).collect();
            let waiting = g.waiting_for_user().map(|s| match s.kind {
                StepKind::WaitForApproval => WAITING_FOR_YOUR_CHOICE,
                StepKind::AskWhatToClean => WAITING_FOR_WHAT_TO_CLEAN,
                _ => WAITING_FOR_YOUR_CONFIRMATION,
            });
            TaskView { id: g.id.clone(), goal: g.kind.tag(), state: g.state(), steps, waiting }
        }).collect()
    }

    pub fn put(&mut self, r: ActionRecord) {
        if let Some(existing) = self.records.iter_mut().find(|x| x.id == r.id) { *existing = r; }
        else { self.records.push(r); if self.records.len() > 30 { self.records.remove(0); } }
    }

    /// Cancels every action still waiting for confirmation, and every goal
    /// still open. Returns how many of either.
    pub fn cancel_pending(&mut self, because: &str) -> usize {
        let last = self.records.iter().filter_map(|r| r.updated_at)
            .chain(self.goals.iter().filter_map(|g| g.transitions.last().map(|t| t.at)))
            .fold(0.0, f64::max);
        self.cancel_pending_at(because, last)
    }

    pub fn cancel_pending_at(&mut self, because: &str, now: f64) -> usize {
        let mut n = 0;
        for r in self.records.iter_mut().filter(|r| r.state.is_waiting() || r.state == ActionState::Proposed) {
            r.finish(ActionState::Cancelled, Some(because.to_string()), None);
            n += 1;
        }
        // Nothing a goal was waiting for survives a kill or a lock either.
        for g in self.goals.iter_mut().filter(|g| g.is_open()) {
            g.cancel(because, now);
            n += 1;
        }
        n
    }

    /// Takes a waiting document request out of waiting to give it another of
    /// its matches. Atomic like `claim`: a Confirm racing this finds it not
    /// waiting and runs nothing, least of all the old target.
    pub fn take_for_retarget(&mut self, id: &str, choice: &str) -> Result<ActionRecord, String> {
        let rec = self.records.iter_mut().find(|r| r.id == id).ok_or("That action no longer exists.")?;
        if !rec.state.is_waiting() {
            return Err(format!("That action is {}, not waiting for confirmation.", tag(rec.state)));
        }
        let ActionKind::OpenDocument { query, .. } = &rec.action else { return Err("Only a document request offers choices.".into()) };
        if !rec.choices.iter().any(|c| c == choice) { return Err("That file was not one of the matches offered.".into()); }
        rec.action = ActionKind::OpenDocument { query: query.clone(), path: Some(choice.to_string()) };
        rec.description = rec.action.describe();
        rec.state = ActionState::Proposed;
        rec.reason = None;
        rec.awaiting_since = None;
        rec.steps = vec![ActionStep::Proposed];
        Ok(rec.clone())
    }

    /// Takes a waiting action for confirmation. Atomic under the book's lock, so
    /// a second Confirm of the same action is refused rather than run twice.
    /// A `choice` must be one of the matches this action offered.
    pub fn claim(&mut self, id: &str, choice: Option<String>) -> Result<ActionRecord, String> {
        let rec = self.records.iter_mut().find(|r| r.id == id).ok_or("That action no longer exists.")?;
        if !rec.state.is_waiting() {
            return Err(format!("That action is {}, not waiting for confirmation.", tag(rec.state)));
        }
        let must_choose = rec.needs_choice();
        if must_choose && choice.is_none() { return Err(choose_message(rec).into()); }
        if let Some(c) = choice {
            if !rec.choices.contains(&c) { return Err("That was not one of the matches offered.".into()); }
            rec.action = match &rec.action {
                ActionKind::OpenDocument { query, .. } => ActionKind::OpenDocument { query: query.clone(), path: Some(c) },
                ActionKind::OpenDirectory { query, scope, .. } if must_choose =>
                    ActionKind::OpenDirectory { query: query.clone(), scope: scope.clone(), path: Some(c) },
                ActionKind::ListDirectory { query, filter, scope, .. } if must_choose =>
                    ActionKind::ListDirectory { query: query.clone(), filter: filter.clone(), scope: scope.clone(), path: Some(c) },
                other if must_choose => other.with_app_name(&c),
                _ => return Err("Only a document, or a name that matched several apps or folders, offers choices.".into()),
            };
            rec.choices.clear();
            rec.description = rec.action.describe();
        }
        rec.state = ActionState::Reauthorizing;
        Ok(rec.clone())
    }
}

fn choose_message(rec: &ActionRecord) -> &'static str {
    if rec.action.app_name().is_some() { CHOOSE_AN_APP } else { CHOOSE_A_FOLDER }
}

fn tag<T: Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

// MARK: - Target policy

/// Where actions may reach: `~/KUE` for files, the document folders for OPEN_DOCUMENT.
pub struct Targets {
    pub permitted: PermittedRoots,
    pub documents: DocumentRoots,
    /// The applications installed on this Mac, which an app name is resolved against.
    pub apps: AppCatalog,
    pub home: PathBuf,
}

impl Targets {
    pub fn for_home(home: &Path) -> Self {
        Targets { permitted: PermittedRoots::default_for_home(home), documents: DocumentRoots::default_for_home(home),
                  apps: AppCatalog::scan(&AppCatalog::default_dirs(home)), home: home.to_path_buf() }
    }

    /// Which installed app an app action names. None for actions that name no app.
    pub fn resolve_app(&self, kind: &ActionKind) -> Option<AppResolution> {
        match kind {
            ActionKind::OpenApplication { name } | ActionKind::CloseApplication { name } | ActionKind::FocusApplication { name } =>
                Some(self.apps.resolve(name)),
            _ => None,
        }
    }

    /// Checks and normalises an action's targets. Err is a policy refusal.
    pub fn check(&self, kind: &ActionKind) -> Result<ActionKind, String> {
        Ok(match kind {
            ActionKind::OpenApplication { name } => ActionKind::OpenApplication { name: actions::validate_app_name(name)? },
            ActionKind::CloseApplication { name } => ActionKind::CloseApplication { name: actions::validate_app_name(name)? },
            ActionKind::FocusApplication { name } => ActionKind::FocusApplication { name: actions::validate_app_name(name)? },
            ActionKind::OpenUrl { url } => ActionKind::OpenUrl { url: actions::validate_url(url)? },
            ActionKind::CreateDirectory { path } | ActionKind::ReadPermittedFile { path } | ActionKind::CreateFile { path, .. } => {
                self.permitted.resolve(path, &self.home)?;
                kind.clone()
            }
            ActionKind::MovePermittedFile { from, to } => {
                self.permitted.resolve(from, &self.home)?;
                self.permitted.resolve(to, &self.home)?;
                kind.clone()
            }
            ActionKind::ShowNotification { title, body } => {
                if body.trim().is_empty() { return Err("A notification needs some text.".into()); }
                ActionKind::ShowNotification { title: title.clone(), body: body.clone() }
            }
            ActionKind::OpenDocument { query, path: Some(p) } => {
                let checked = self.documents.validate(p)?;
                ActionKind::OpenDocument { query: query.clone(), path: Some(checked.to_string_lossy().to_string()) }
            }
            ActionKind::OpenDocument { path: None, .. } => return Err("No document was chosen.".into()),
            ActionKind::OpenDirectory { query, scope, path: Some(p) } => {
                let checked = self.documents.validate_folder(p)?;
                ActionKind::OpenDirectory { query: query.clone(), scope: scope.clone(), path: Some(checked.to_string_lossy().to_string()) }
            }
            ActionKind::ListDirectory { query, filter, scope, path: Some(p) } => {
                let checked = self.documents.validate_folder(p)?;
                ActionKind::ListDirectory { query: query.clone(), filter: filter.clone(), scope: scope.clone(), path: Some(checked.to_string_lossy().to_string()) }
            }
            ActionKind::OpenDirectory { path: None, .. } | ActionKind::ListDirectory { path: None, .. } => return Err("No folder was chosen.".into()),
            // Names no target: the folders it reads are fixed, and it reads
            // nothing else. There is nothing to check or normalise.
            ActionKind::InspectStorage => ActionKind::InspectStorage,
            // Every path is checked again here, one at a time, against the same
            // folders a document search is limited to. The stronger rule — that
            // KUE only moves what it found and showed you — is in `propose_kind`,
            // where the report it came from can be consulted.
            ActionKind::MoveToTrash { paths } => {
                if paths.is_empty() { return Err("Nothing was selected.".into()); }
                if paths.len() > actions::MAX_TRASHED {
                    return Err(format!("That is more than {} files at once.", actions::MAX_TRASHED));
                }
                let mut checked = Vec::new();
                for p in paths {
                    let ok = self.documents.validate_movable(p)?;
                    let ok = ok.to_string_lossy().to_string();
                    if !checked.contains(&ok) { checked.push(ok); }
                }
                ActionKind::MoveToTrash { paths: checked }
            }
            ActionKind::RestoreFromTrash { items } => {
                if items.is_empty() { return Err("There is nothing to put back.".into()); }
                for i in items {
                    if !i.trashed.starts_with('/') || !i.original.starts_with('/') {
                        return Err("A file is put back by its full location.".into());
                    }
                }
                kind.clone()
            }
        })
    }
}

// MARK: - The executor boundary

/// What the executor process (KueAct) receives for one action.
///
/// The verb is its only command-line argument. Every target — app name, link,
/// path, notification text — travels as one JSON line on its standard input,
/// so none of it appears in the process table where any process of this user
/// could read it. The executor writes nothing to disk and logs nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutorRequest {
    pub verb: &'static str,
    payload: serde_json::Value,
}

impl ExecutorRequest {
    /// None for actions that do not leave KUE's own process (file actions).
    pub fn for_action(kind: &ActionKind) -> Option<Self> {
        use serde_json::json;
        let (verb, payload) = match kind {
            ActionKind::OpenApplication { name } => ("open-app", json!({ "name": name })),
            ActionKind::FocusApplication { name } => ("focus-app", json!({ "name": name })),
            ActionKind::CloseApplication { name } => ("close-app", json!({ "name": name })),
            ActionKind::OpenUrl { url } => ("open-url", json!({ "url": url })),
            ActionKind::ShowNotification { title, body } => ("notify", json!({ "title": title, "body": body })),
            ActionKind::OpenDocument { path: Some(p), .. } => ("open-file", json!({ "path": p })),
            ActionKind::OpenDirectory { path: Some(p), .. } => ("open-folder", json!({ "path": p })),
            _ => return None,
        };
        Some(ExecutorRequest { verb, payload })
    }

    /// One file to the Trash. A move to the Trash is made one file at a time,
    /// each verified on its own, so a batch where some fail cannot be reported
    /// as one that worked.
    pub fn trash(path: &str) -> Self {
        ExecutorRequest { verb: "trash", payload: serde_json::json!({ "path": path }) }
    }

    /// One file back where it came from.
    pub fn untrash(item: &TrashedItem) -> Self {
        ExecutorRequest { verb: "untrash", payload: serde_json::json!({ "from": item.trashed, "to": item.original }) }
    }

    pub fn argv(&self) -> Vec<String> { vec![self.verb.to_string()] }

    pub fn stdin_line(&self) -> String {
        let mut s = serde_json::json!({ "v": 1, "verb": self.verb, "target": self.payload }).to_string();
        s.push('\n');
        s
    }
}

/// What the executor reported. `landed` is the one piece of machine-readable
/// detail: where a file ended up in the Trash, so putting it back is exact
/// rather than a guess at its name.
#[derive(Debug, Clone, PartialEq)]
pub struct Execution {
    pub state: ActionState,
    pub reason: Option<String>,
    pub verification: Option<String>,
    pub landed: Option<String>,
}

impl From<(ActionState, Option<String>, Option<String>)> for Execution {
    fn from((state, reason, verification): (ActionState, Option<String>, Option<String>)) -> Self {
        Execution { state, reason, verification, landed: None }
    }
}

// MARK: - The transaction

/// Everything a transaction touches. The shell supplies real locks, macOS
/// authentication and the KueAct runner; tests supply their own.
pub struct Runtime<'a> {
    pub engine: &'a Mutex<Engine>,
    pub firewall: &'a Mutex<Firewall>,
    pub book: &'a Mutex<ActionBook>,
    pub targets: &'a Targets,
    pub now: &'a dyn Fn() -> f64,
    /// macOS LocalAuthentication for one operation; returns its result code.
    /// Called with no lock held.
    pub authenticate: &'a dyn Fn(OsAuthKind, Operation) -> String,
    /// Runs one request through the executor process.
    pub execute_os: &'a dyn Fn(&ExecutorRequest) -> Execution,
    /// Measures the volume a path lives on. The shell asks macOS; core never
    /// does, and an error here means the totals are absent, never guessed.
    pub measure_volume: &'a dyn Fn(&Path) -> Result<VolumeUsage, String>,
    /// Told whenever a record changes, so the interface can fetch the list.
    pub changed: &'a dyn Fn(),
    /// Given each new state of a record, as the window may show it (redacted
    /// when the firewall refuses its target), for narration. Called with no
    /// lock held.
    pub narrate: &'a dyn Fn(&ActionRecord),
    /// A sentence of KUE's about a goal step that is not an action (the
    /// answer to "why is my storage full?"), for its voice and speech gate,
    /// with the kinds of data it carries so the firewall checks them again at
    /// the speakers. Names no file. Called with no lock held.
    pub say: &'a dyn Fn(&str, &[DataKind]),
}

/// Authorizes `op` for the owner. When the engine asks for OS authentication,
/// macOS prompts (with no lock held) and the engine decides again. Err is the
/// reason, from the engine.
pub fn gate(engine: &Mutex<Engine>, op: Operation, authenticate: &dyn Fn(OsAuthKind, Operation) -> String,
            now: &dyn Fn() -> f64) -> Result<(), String> {
    let first = engine.lock().unwrap().authorize(op, Principal::Owner, now());
    let kind = match first {
        Decision::Allow => return Ok(()),
        Decision::Deny(reason) => return Err(reason),
        Decision::NeedsStrongAuth => OsAuthKind::Strong,
        Decision::NeedsPhysicalConfirmation => OsAuthKind::Physical,
    };
    let result = authenticate(kind, op);
    let mut e = engine.lock().unwrap();
    e.record_os_auth(kind, Some(op), &result, now());
    if result != "SUCCESS" {
        return Err(format!("macOS did not confirm ({result})."));
    }
    match e.authorize(op, Principal::Owner, now()) {
        Decision::Allow => Ok(()),
        Decision::Deny(reason) => Err(reason),
        other => Err(format!("still not authorized after authentication ({other:?})")),
    }
}

enum Memo { Refused, Unplanned }

/// Stops a transaction: records the end state, a target-free event, and hands
/// back what the interface may show.
fn end(rt: &Runtime, mut rec: ActionRecord, state: ActionState, reason: String, memo: Memo) -> ActionRecord {
    rec.finish(state, Some(reason), None);
    let summary = match memo { Memo::Refused => rec.refused_summary(), Memo::Unplanned => rec.unplanned_summary() };
    rt.engine.lock().unwrap().record_action_event(summary, (rt.now)());
    publish(rt, rec)
}

/// Stores a record's new state, tells the window and the narrator, and returns
/// what the window may show.
fn publish(rt: &Runtime, mut rec: ActionRecord) -> ActionRecord {
    // Every state change passes through here, so this is the one place that
    // knows when the record last moved.
    rec.updated_at = Some((rt.now)());
    rt.book.lock().unwrap().put(rec.clone());
    (rt.changed)();
    // The runtime's request moves with its record, whatever moved the record.
    sync_request(rt, &rec);
    let shown = for_interface(rt, rec.clone());
    (rt.narrate)(&shown);
    advance(rt, &rec);
    if let Some(t) = &rec.task { sync_goal(rt, &t.task_id); }
    shown
}

// MARK: - Goals

/// What the window says a goal is waiting for.
pub const WAITING_FOR_YOUR_CHOICE: &str = "Choose what to move to the Trash. Nothing moves until you do.";
pub const WAITING_FOR_WHAT_TO_CLEAN: &str = "Tell me what to clean.";
pub const WAITING_FOR_YOUR_CONFIRMATION: &str = "Waiting for you to confirm.";

/// Opens a goal and runs it until a step waits, the plan stops, or it
/// finishes. Returns the first action record a step produced, if any.
pub fn start_goal(rt: &Runtime, bp: Blueprint, source: &str) -> Option<ActionRecord> {
    start_goal_for(rt, bp, source, "")
}

/// The same, told what the owner actually said. A clean-up consults what they
/// have told KUE before about cleaning up; `said` is how KUE knows which
/// memories bear on it, and nothing else uses it.
pub fn start_goal_for(rt: &Runtime, bp: Blueprint, source: &str, said: &str) -> Option<ActionRecord> {
    let now = (rt.now)();
    let (id, kind, n, replaced) = {
        let mut book = rt.book.lock().unwrap();
        let mut replaced = Vec::new();
        // "Clean my storage" answers "clean what?".
        if bp.kind == GoalKind::CleanUpStorage {
            for g in book.goals.iter_mut().filter(|g| g.kind == GoalKind::CleanUpUnspecified && g.is_open()) {
                g.cancel("Replaced by a request to clean your storage.", now);
                g.outcome_recorded = true;
                replaced.push(g.kind.tag());
            }
        }
        let (kind, n) = (bp.kind, bp.steps.len());
        (book.new_goal(bp, if source == "VOICE" { "VOICE" } else { "TEXT" }, now), kind, n, replaced)
    };
    {
        let mut e = rt.engine.lock().unwrap();
        if let Some((rid, _)) = served() { e.requests_mut().bind(&rid, Some(&id), Some(&id), None, None); }
        for r in replaced { e.record_action_event(format!("Goal {r}: cancelled, replaced by a more specific request."), now); }
        e.record_action_event(format!("Goal {}: planned, {n} step{}.", kind.tag(), if n == 1 { "" } else { "s" }), now);
    }
    // What the owner has already told KUE about cleaning up, applied before
    // the plan is laid out — so they do not have to say it again.
    if kind == GoalKind::CleanUpStorage { apply_standing_wishes(rt, &id, said, now); }
    (rt.changed)();
    drive(rt, &id)
}

/// After a record bound to a goal step changes: bring that step up to date,
/// then let the goal decide what happens next.
fn advance(rt: &Runtime, rec: &ActionRecord) {
    let Some(step) = &rec.task else { return };
    sync(rt, &step.task_id, step.index, rec);
    drive(rt, &step.task_id);
}

enum Claim { Claimed, PreconditionFailed, Taken }

/// PENDING → READY, once, under the book's lock, if the step's preconditions
/// hold. A step already claimed is left to whoever claimed it.
fn claim_step(rt: &Runtime, goal_id: &str, i: usize, now: f64) -> Claim {
    let mut book = rt.book.lock().unwrap();
    let Some(created) = book.goal(goal_id).map(|g| g.created_at) else { return Claim::Taken };
    let has_report = book.report_since(created);
    let Some(g) = book.goal_mut(goal_id) else { return Claim::Taken };
    if g.steps[i].state != StepState::Pending { return Claim::Taken; }
    match g.preconditions_hold(i, has_report) {
        Ok(()) => { let _ = g.apply(i, StepEvent::PreconditionsMet, now); Claim::Claimed }
        Err(why) => { let _ = g.apply(i, StepEvent::PreconditionFailed(why), now); Claim::PreconditionFailed }
    }
}

/// Decides and starts steps until one waits, the plan stops, or it finishes.
fn drive(rt: &Runtime, goal_id: &str) -> Option<ActionRecord> {
    let mut first = None;
    for _ in 0..32 {
        let now = (rt.now)();
        let decision = match rt.book.lock().unwrap().goal(goal_id) { Some(g) => g.decide(), None => break };
        let i = match decision {
            GoalDecision::Start(i) => i,
            GoalDecision::Recover { from, to } => {
                if let Some(g) = rt.book.lock().unwrap().goal_mut(goal_id) { g.block_after(from, now); }
                to
            }
            GoalDecision::Wait(_) => break,
            GoalDecision::Stop { at } => { conclude(rt, goal_id, Some(at), now); break }
            GoalDecision::Finished => { conclude(rt, goal_id, None, now); break }
        };
        match claim_step(rt, goal_id, i, now) {
            Claim::Claimed => if let Some(rec) = run_goal_step(rt, goal_id, i) { first.get_or_insert(rec); },
            Claim::PreconditionFailed => { record_step(rt, goal_id, i, now); continue }
            Claim::Taken => break,
        }
    }
    first
}

/// Records how a goal ended, once, and blocks what did not run.
fn conclude(rt: &Runtime, goal_id: &str, stopped_at: Option<usize>, now: f64) {
    let summary = {
        let mut book = rt.book.lock().unwrap();
        let Some(g) = book.goal_mut(goal_id) else { return };
        if let Some(at) = stopped_at { g.block_after(at, now); }
        if g.outcome_recorded { return; }
        g.outcome_recorded = true;
        match stopped_at {
            None => format!("Goal {}: completed.", g.kind.tag()),
            Some(at) => format!("Goal {}: {} at step {} ({}, {}).", g.kind.tag(),
                if g.state() == GoalState::Cancelled { "cancelled" } else { "stopped" }, at + 1, g.steps[at].kind.tag(), g.steps[at].state.tag()),
        }
    };
    rt.engine.lock().unwrap().record_action_event(summary, now);
    (rt.changed)();
}

/// Applies events to one step, in order, through the state machine.
fn step_events(rt: &Runtime, goal_id: &str, i: usize, events: Vec<StepEvent>, now: f64) {
    let mut book = rt.book.lock().unwrap();
    let Some(g) = book.goal_mut(goal_id) else { return };
    for e in events {
        if g.apply(i, e, now).is_err() { break; }
    }
}

/// A target-free event for a step that has ended.
fn record_step(rt: &Runtime, goal_id: &str, i: usize, now: f64) {
    let line = {
        let book = rt.book.lock().unwrap();
        let Some(g) = book.goal(goal_id) else { return };
        let s = &g.steps[i];
        if !s.state.is_final() { return; }
        format!("Goal {} step {} ({}): {}.", g.kind.tag(), i + 1, s.kind.tag(), s.state.tag())
    };
    rt.engine.lock().unwrap().record_action_event(line, now);
}

/// Runs one claimed step.
/// A step of a plan the owner approved, whose own gate is the owner's word, is
/// carried by that approval: the owner read this exact step in the plan and
/// said yes to it, and asking again for each step would train them to say yes
/// without reading.
///
/// What the approval does NOT carry: anything macOS must confirm. A high or
/// critical step still waits and asks for Touch ID at the moment it runs,
/// inside an approved plan exactly as outside one. Authorization itself is
/// unchanged — `confirm` re-authorizes, and the step is still verified.
fn confirmed_by_the_plan(rt: &Runtime, goal_id: &str, i: usize, rec: ActionRecord) -> ActionRecord {
    if rec.state != ActionState::RequiresConfirmation { return rec; }
    let approved = rt.book.lock().unwrap().plans().iter()
        .any(|h| h.goal_id.as_deref() == Some(goal_id) && !h.plan.approved_by.is_empty());
    if !approved { return rec; }
    let now = (rt.now)();
    match confirm(rt, &rec.id, None) {
        Ok(done) => {
            if let Some(h) = rt.book.lock().unwrap().approved_plan_of_goal(goal_id) {
                h.plan.approved_by.push(crate::plan::Approval { by: crate::plan::Approver::Owner, at: now, step: i });
            }
            done
        }
        // It could not be confirmed now (the moment changed): it stays waiting,
        // as it would have without a plan.
        Err(_) => rec,
    }
}

fn run_goal_step(rt: &Runtime, goal_id: &str, i: usize) -> Option<ActionRecord> {
    let now = (rt.now)();
    let (kind, source, n, folder, earlier_action, chosen) = {
        let book = rt.book.lock().unwrap();
        let g = book.goal(goal_id)?;
        // The folder an earlier step of this goal opened or listed, else the last one used.
        let earlier = book.records.iter().rev()
            .filter(|r| r.task.as_ref().is_some_and(|x| x.task_id == goal_id && x.index < i) && r.state == ActionState::Succeeded)
            .find_map(|r| folder_of(&r.action));
        (g.steps[i].kind.clone(), g.source.clone(), g.steps.len(), earlier.or(book.last_folder.clone()),
         i.checked_sub(1).and_then(|p| g.steps[p].action_id.clone()), g.chosen.clone())
    };
    let step = Some(TaskStepRef { task_id: goal_id.to_string(), index: i, of: n });
    // An action step meets the kill switch inside its transaction, and leaves a
    // record saying so. Every other step meets it here.
    let acts = matches!(kind, StepKind::Act(_) | StepKind::FindDocument { .. } | StepKind::MoveChosenToTrash);
    if !acts && rt.engine.lock().unwrap().is_killed() {
        step_events(rt, goal_id, i, vec![StepEvent::AuthorizationDenied(KILLED.into())], now);
        record_step(rt, goal_id, i, now);
        return None;
    }
    match kind {
        StepKind::Act(action) => {
            let action = match action {
                ActionKind::ListDirectory { query, filter, scope: None, path: None } if query.is_empty() =>
                    ActionKind::ListDirectory { query, filter, scope: None, path: folder },
                other => other,
            };
            let rec = propose_kind(rt, action, &source, step);
            Some(confirmed_by_the_plan(rt, goal_id, i, rec))
        }
        StepKind::FindDocument { query } => Some(propose_kind(rt, ActionKind::OpenDocument { query, path: None }, &source, step)),
        // The same transaction as the find: it is waiting for your Confirm.
        StepKind::OpenFoundDocument => {
            let rec = earlier_action.and_then(|id| rt.book.lock().unwrap().get(&id));
            match rec {
                Some(rec) => {
                    let mut book = rt.book.lock().unwrap();
                    if let Some(g) = book.goal_mut(goal_id) {
                        g.steps[i].action_id = Some(rec.id.clone());
                        follow(g, i, act_target(&rec), &rec, now);
                    }
                }
                None => step_events(rt, goal_id, i, vec![StepEvent::Authorized,
                    StepEvent::Failed("There is no record of the document the find step found.".into())], now),
            }
            None
        }
        StepKind::MoveChosenToTrash => Some(propose_kind(rt, ActionKind::MoveToTrash { paths: chosen }, &source, step)),
        StepKind::ExplainFindings | StepKind::RecommendReview => { run_local(rt, goal_id, i, &kind, now); None }
        StepKind::WaitForApproval | StepKind::AskWhatToClean => {
            step_events(rt, goal_id, i, vec![StepEvent::Authorized, StepEvent::NeedsOwner], now);
            None
        }
        StepKind::ReportOutcome => { report_outcome(rt, goal_id, i, earlier_action, now); None }
        StepKind::DeletePermanently | StepKind::Purchase | StepKind::SendMessage => {
            step_events(rt, goal_id, i, vec![StepEvent::AuthorizationDenied(
                format!("{} is not implemented, so no authorization lets it run.", kind.tag()))], now);
            record_step(rt, goal_id, i, now);
            None
        }
    }
}

/// A step KUE does in its own process from what an earlier step measured:
/// authorized now, cleared by the firewall now, then observed and verified.
fn run_local(rt: &Runtime, goal_id: &str, i: usize, kind: &StepKind, now: f64) {
    let (op, goal_kind) = {
        let book = rt.book.lock().unwrap();
        let Some(g) = book.goal(goal_id) else { return };
        (match g.steps[i].requirement.authority { Authority::Operation { operation } => Some(operation), _ => None }, g.kind)
    };
    let Some(op) = op else {
        step_events(rt, goal_id, i, vec![StepEvent::AuthorizationDenied("This step names no operation, so it does not run.".into())], now);
        return record_step(rt, goal_id, i, now);
    };
    // Authorized by the engine for this step, now. Nothing earlier counts.
    let decision = rt.engine.lock().unwrap().authorize(op, Principal::Owner, now);
    match decision {
        Decision::Allow => step_events(rt, goal_id, i, vec![StepEvent::Authorized], now),
        Decision::Deny(r) => {
            step_events(rt, goal_id, i, vec![StepEvent::AuthorizationDenied(format!("{AUTHORIZATION_REQUIRED}: {r}"))], now);
            return record_step(rt, goal_id, i, now);
        }
        Decision::NeedsStrongAuth | Decision::NeedsPhysicalConfirmation => {
            step_events(rt, goal_id, i, vec![StepEvent::NeedsAuthorization, StepEvent::AuthorizationDenied(format!(
                "{AUTHORIZATION_REQUIRED}: this step needs more than KUE can confirm right now, and a step that only explains does not ask macOS."))], now);
            return record_step(rt, goal_id, i, now);
        }
    }
    let Some(report) = rt.book.lock().unwrap().last_storage().cloned() else {
        step_events(rt, goal_id, i, vec![StepEvent::Failed("The storage report is gone.".into())], now);
        return record_step(rt, goal_id, i, now);
    };
    // The firewall, asked again for this step: an explanation names folders, a
    // recommendation names files.
    let cleared = {
        let mut fw = rt.firewall.lock().unwrap();
        match kind {
            StepKind::ExplainFindings => fw.clear_storage_summary(report.summary.clone(), now).is_some(),
            _ => fw.clear_storage_inventory(StorageRequest { areas: rt.targets.documents.names() }, now).is_some(),
        }
    };
    if !cleared {
        step_events(rt, goal_id, i, vec![StepEvent::AuthorizationDenied(PRIVACY_DENIED.into())], now);
        return record_step(rt, goal_id, i, now);
    }
    let (said, observed, verdict) = match kind {
        StepKind::ExplainFindings => (report.explanation(),
            format!("Read the report: {} folder(s), {} finding(s).", report.summary.areas.len(), report.found), report.check_explanation()),
        _ => (report.recommendation(),
            format!("Read {} finding(s) of {}.", report.candidates.len(), report.found), report.check_recommendation()),
    };
    let verified = verdict.is_ok();
    step_events(rt, goal_id, i, vec![StepEvent::Observed(observed), match verdict {
        Ok(evidence) => StepEvent::Verified(evidence),
        Err(why) => StepEvent::VerificationFailed(why),
    }], now);
    {
        let mut book = rt.book.lock().unwrap();
        if let Some(g) = book.goal_mut(goal_id) {
            if verified { g.steps[i].said = Some(said.clone()); }
            // Nothing to choose from: the rest of a cleanup is not needed.
            if verified && *kind == StepKind::RecommendReview && report.found == 0 {
                g.finish_early(i + 1, "Nothing was found to move.", now);
            }
        }
    }
    record_step(rt, goal_id, i, now);
    (rt.changed)();
    // The answer to "why is my storage full?" is this sentence, so it is said.
    if verified && goal_kind == GoalKind::ExplainStorage && *kind == StepKind::ExplainFindings { (rt.say)(&said, &[DataKind::StorageSummary]); }
}

/// Reports what the move did, from its record. Changes nothing.
fn report_outcome(rt: &Runtime, goal_id: &str, i: usize, move_action: Option<String>, now: f64) {
    let rec = move_action.and_then(|id| rt.book.lock().unwrap().get(&id));
    let mut events = vec![StepEvent::Authorized];
    let mut said = None;
    match rec {
        Some(r) if !r.state.is_waiting() && !matches!(r.state, ActionState::Proposed | ActionState::Authorized
            | ActionState::Executing | ActionState::Reauthorizing) => {
            events.push(StepEvent::Observed(format!("{} {}", r.action.tag(), tag(r.state))));
            events.push(StepEvent::Verified(format!("The move's record is final ({}), and this report is read from it.", tag(r.state))));
            said = r.sentences.as_ref().map(|s| s.on_screen.clone());
        }
        _ => {
            events.push(StepEvent::Observed("No final record of the move.".into()));
            events.push(StepEvent::VerificationFailed("There is no final record of the move to report.".into()));
        }
    }
    step_events(rt, goal_id, i, events, now);
    if let Some(s) = said {
        if let Some(g) = rt.book.lock().unwrap().goal_mut(goal_id) { g.steps[i].said = Some(s); }
    }
    record_step(rt, goal_id, i, now);
}

/// Where a step's action record says the step is.
enum Target { Stay, Running, Owner, Macos, Verified(String), Unverified(String), Failed(String), Blocked(String), Cancelled(String) }

fn act_target(rec: &ActionRecord) -> Target {
    let why = |what: &str| format!("{} {}: {what}.", rec.action.tag(), tag(rec.state));
    match rec.state {
        ActionState::Proposed => Target::Stay,
        ActionState::Authorized | ActionState::Executing => Target::Running,
        ActionState::RequiresConfirmation => Target::Owner,
        ActionState::RequiresStrongAuth | ActionState::Reauthorizing => Target::Macos,
        ActionState::Succeeded if rec.verification.is_some() => Target::Verified(format!("{} recorded what it verified.", rec.action.tag())),
        ActionState::Succeeded => Target::Unverified(why("it reported success but recorded no verification")),
        ActionState::UnknownResult => Target::Unverified(why("the result could not be verified")),
        ActionState::Failed | ActionState::PartiallySucceeded | ActionState::NoMatches => Target::Failed(why("it did not succeed")),
        ActionState::Denied | ActionState::AuthorizationExpired | ActionState::PrivacyDenied => Target::Blocked(why("it was not allowed to run")),
        ActionState::Cancelled => Target::Cancelled(why("cancelled")),
    }
}

/// The find half of OPEN_DOCUMENT: done once a document was found and checked.
fn find_target(rec: &ActionRecord, found_ok: bool) -> Target {
    let found = matches!(&rec.action, ActionKind::OpenDocument { path: Some(_), .. }) && !rec.choices.is_empty();
    match rec.state {
        ActionState::Proposed | ActionState::Authorized => Target::Stay,
        ActionState::NoMatches => Target::Failed("No matching document was found in the document folders.".into()),
        _ if found && found_ok => Target::Verified(format!(
            "{} matching document(s) inside the document folders; the one proposed exists and is a document KUE may open.", rec.choices.len())),
        _ if found => Target::Unverified("The document found is no longer there, or is not one KUE may open.".into()),
        ActionState::Cancelled => Target::Cancelled("Cancelled before a document was found.".into()),
        _ => Target::Blocked(format!("OPEN_DOCUMENT {}: the search did not run.", tag(rec.state))),
    }
}

/// The next event that moves a step toward its target, if any. Only legal
/// transitions are ever proposed; `Goal::apply` refuses anything else.
fn hop(state: StepState, target: &Target, rec: &ActionRecord) -> Option<StepEvent> {
    use StepState as S;
    use Target as T;
    let observed = || StepEvent::Observed(format!("{} {}", rec.action.tag(), tag(rec.state)));
    match (state, target) {
        (_, T::Stay) => None,
        (s, _) if s.is_final() => None,
        // A step starts only when its goal starts it.
        (S::Pending, _) => None,
        (S::Ready | S::Running | S::WaitingForUser | S::WaitingForAuthorization, T::Blocked(r)) => Some(StepEvent::AuthorizationDenied(r.clone())),
        (S::Ready | S::Running | S::WaitingForUser | S::WaitingForAuthorization | S::Verifying, T::Cancelled(r)) => Some(StepEvent::Cancelled(r.clone())),
        (S::Ready, T::Macos) => Some(StepEvent::NeedsAuthorization),
        (S::Ready, _) => Some(StepEvent::Authorized),
        (S::Running, T::Running) => None,
        (S::Running, T::Owner) => Some(StepEvent::NeedsOwner),
        (S::Running, T::Macos) => Some(StepEvent::NeedsAuthorization),
        (S::Running, T::Verified(_) | T::Unverified(_)) => Some(observed()),
        (S::Running | S::WaitingForUser | S::WaitingForAuthorization, T::Failed(r)) => Some(StepEvent::Failed(r.clone())),
        (S::WaitingForUser, T::Owner) => None,
        (S::WaitingForUser, _) => Some(StepEvent::OwnerAnswered),
        (S::WaitingForAuthorization, T::Macos) => None,
        (S::WaitingForAuthorization, _) => Some(StepEvent::Authorized),
        (S::Verifying, T::Verified(e)) => Some(StepEvent::Verified(e.clone())),
        (S::Verifying, T::Unverified(r) | T::Failed(r)) => Some(StepEvent::VerificationFailed(r.clone())),
        (S::Verifying, _) => None,
        // Final states were handled first.
        (S::Completed | S::Failed | S::Blocked | S::Cancelled, _) => None,
    }
}

/// Walks a step through the state machine to where its record says it is.
fn follow(g: &mut Goal, i: usize, target: Target, rec: &ActionRecord, now: f64) {
    for _ in 0..8 {
        let Some(event) = hop(g.steps[i].state, &target, rec) else { return };
        if g.apply(i, event, now).is_err() { return; }
    }
}

/// Brings the step (or, for a document, both halves) bound to `rec` up to date.
fn sync(rt: &Runtime, goal_id: &str, index: usize, rec: &ActionRecord) {
    let now = (rt.now)();
    let found_ok = match &rec.action {
        ActionKind::OpenDocument { path: Some(p), .. } => rec.choices.contains(p) && rt.targets.documents.validate(p).is_ok(),
        _ => false,
    };
    let ended = {
        let mut book = rt.book.lock().unwrap();
        let Some(g) = book.goal_mut(goal_id) else { return };
        if index >= g.steps.len() { return; }
        if g.steps[index].action_id.is_none() { g.steps[index].action_id = Some(rec.id.clone()); }
        let before: Vec<StepState> = g.steps.iter().map(|s| s.state).collect();
        if matches!(g.steps[index].kind, StepKind::FindDocument { .. }) {
            follow(g, index, find_target(rec, found_ok), rec, now);
            if g.steps.get(index + 1).is_some_and(|s| s.kind == StepKind::OpenFoundDocument && s.state != StepState::Pending) {
                follow(g, index + 1, act_target(rec), rec, now);
            }
        } else {
            follow(g, index, act_target(rec), rec, now);
        }
        g.steps.iter().zip(before).filter(|(s, b)| s.state.is_final() && !b.is_final()).map(|(s, _)| s.index).collect::<Vec<_>>()
    };
    for i in ended { record_step(rt, goal_id, i, now); }
}
fn with_folder(kind: &ActionKind, path: &str) -> ActionKind {
    match kind {
        ActionKind::OpenDirectory { query, scope, .. } => ActionKind::OpenDirectory { query: query.clone(), scope: scope.clone(), path: Some(path.into()) },
        ActionKind::ListDirectory { query, filter, scope, .. } =>
            ActionKind::ListDirectory { query: query.clone(), filter: filter.clone(), scope: scope.clone(), path: Some(path.into()) },
        other => other.clone(),
    }
}

/// The one folder in the document folders named exactly `name`, searched only when
/// authorized and cleared like any folder search. None when there is not exactly one.
fn folder_named(rt: &Runtime, name: &str, op: Operation) -> Option<String> {
    gate(rt.engine, op, rt.authenticate, rt.now).ok()?;
    let cleared = rt.firewall.lock().unwrap().clear_document_search(DocumentQuery::new(name), (rt.now)())?;
    let found = rt.targets.documents.find_folders(&cleared, None, 2);
    match found.as_slice() {
        [one] if crate::folders::exact_name(one, name) => Some(one.to_string_lossy().to_string()),
        _ => None,
    }
}

fn folder_of(kind: &ActionKind) -> Option<String> {
    match kind {
        ActionKind::OpenDirectory { path: Some(p), .. } | ActionKind::ListDirectory { path: Some(p), .. } => Some(p.clone()),
        _ => None,
    }
}

/// A record on its way to the window: cleared by the firewall, or reduced to
/// its kind and state.
fn for_interface(rt: &Runtime, rec: ActionRecord) -> ActionRecord {
    match rt.firewall.lock().unwrap().clear_actions_for_interface(vec![rec.clone()], (rt.now)()) {
        Some(c) => c.into_value().pop().unwrap_or_else(|| rec.redacted()),
        // The state stays true — an action that ran is not reported as refused —
        // but nothing that could name the target is shown.
        None => ActionRecord {
            reason: Some(if rec.state == ActionState::PrivacyDenied { PRIVACY_DENIED } else { WITHHELD }.into()),
            ..rec.redacted()
        },
    }
}

/// Plans a typed or spoken sentence. None: not a command — the conversation
/// answers it, by rule or by the model. A LOW-risk action that is already
/// authorized runs at once; anything else waits for Confirm or stops with a
/// reason. A request of several steps becomes a goal, run step by step.
pub fn propose(rt: &Runtime, text: &str, source: &str) -> Option<ActionRecord> {
    // A request refused by what it asks for is never planned, even when part of
    // it would parse ("open Safari and disable the kill switch"). The
    // conversation says why (`safety::screen`).
    if crate::safety::screen(text).is_some() { return None; }
    let Understanding::Understood(intent) = intent::classify(text, InputSource::from_tag(source), None) else { return None };
    if matches!(intent.work, Work::Act(_) | Work::Goal(_)) { note_intent(rt, &intent); }
    match intent.work {
        Work::Act(kind) => Some(propose_kind(rt, kind, source, None)),
        Work::Goal(bp) => start_goal_for(rt, bp, source, text),
        // Answered in the conversation — a refusal of a request KUE cannot do
        // included — before anything starts.
        // Answered, asked about, or taken to the model — as a question, or for
        // a plan the owner will be shown. None of them start anything here.
        Work::Answer { .. } | Work::CapabilityList | Work::Ask { .. } | Work::Model | Work::ModelPlan => None,
    }
}

/// Records what a request was understood as, with where it stands now: the
/// registry, the kill switch, and a preview of the engine's decision. The
/// preview grants nothing; every step is authorized again when it runs.
pub fn note_intent(rt: &Runtime, i: &intent::Intent) {
    let now = (rt.now)();
    let mut e = rt.engine.lock().unwrap();
    let preview = i.operation.map(|op| e.preview_authorization(op, now));
    let status = intent::status(i, e.is_killed(), preview.as_ref());
    e.record_intent(&i.summary(status), now);
}

/// Plans one action: privacy, target resolution, authorization, then it runs,
/// waits for Confirm or stops with a reason.
fn propose_kind(rt: &Runtime, parsed: ActionKind, source: &str, task: Option<TaskStepRef>) -> ActionRecord {
    let now = (rt.now)();
    let risk = actions::risk(&parsed);
    let op = actions::operation_for(risk);
    let id = rt.book.lock().unwrap().next_id();
    let mut rec = ActionRecord {
        id, source: if source == "VOICE" { "VOICE" } else { "TEXT" }.into(), description: parsed.describe(),
        action: parsed.clone(), risk, state: ActionState::Proposed, reason: None, verification: None, output: None,
        created_at: now, updated_at: Some(now), choices: Vec::new(), steps: vec![ActionStep::Proposed], awaiting_since: None, task,
        sentences: None,
    };
    // The request this is for knows it from the moment it exists, so the
    // runtime follows it through every state — not only once it has ended.
    if let Some((rid, _)) = served() {
        rt.engine.lock().unwrap().requests_mut().bind(&rid, None, None, Some(&rec.id), None);
    }

    // Nothing runs under a capability KUE does not claim to have. The
    // allowlist already decides which kinds exist; this is the second half of
    // the same rule — the capability registry is what KUE tells you, out loud
    // and on screen, that it can do, and an action must be covered by one of
    // those. A kind added to the parser without a capability stops here rather
    // than quietly becoming a thing KUE does but never says it does.
    match crate::capabilities::for_action_tag(parsed.tag()) {
        Some(cap) if cap.status != crate::context::CapabilityStatus::NotImplemented => {}
        _ => return end(rt, rec, ActionState::Denied,
            format!("{AUTHORIZATION_REQUIRED}: {} is not something KUE can do.", parsed.tag()),
            Memo::Refused),
    }
    // And it must be a DECLARED tool: one with a written contract — inputs,
    // preconditions, executor, verifier, rollback (tools.rs). The registry says
    // what KUE can do; the declaration says how it does it. Without both,
    // nothing runs.
    if crate::tools::declared(parsed.tag()).is_none() {
        return end(rt, rec, ActionState::Denied,
            format!("{AUTHORIZATION_REQUIRED}: {} has no declared contract, so KUE will not run it.", parsed.tag()),
            Memo::Refused);
    }

    // The target exists now, in memory. It may be shown to you and go nowhere else.
    if !rt.firewall.lock().unwrap().check(DataKind::ActionTarget, Destination::Interface, now).is_allow() {
        let redacted = rec.redacted();
        return end(rt, redacted, ActionState::PrivacyDenied, PRIVACY_DENIED.into(), Memo::Refused);
    }
    rec.step(ActionStep::PrivacyChecked);

    // KUE moves nothing it did not find and show you first. The selection has
    // to be of the report the window is looking at — a path from anywhere else,
    // however well formed, is refused here, before authorization is even asked
    // for. This is the rule that keeps "move these files" from becoming "move
    // any file", whatever sends it.
    if let ActionKind::MoveToTrash { paths } = &rec.action {
        if !rt.book.lock().unwrap().last_storage().is_some_and(|r| r.offered(paths)) {
            return end(rt, rec, ActionState::Denied, NOT_OFFERED.into(), Memo::Refused);
        }
    }

    // A document request must be authorized before your folders are searched.
    if let ActionKind::OpenDocument { query, path: None } = &parsed {
        if let Err(r) = gate(rt.engine, op, rt.authenticate, rt.now) {
            return end(rt, rec, ActionState::Denied, format!("{AUTHORIZATION_REQUIRED} before searching: {r}"), Memo::Refused);
        }
        rec.step(ActionStep::Authorized);
        let Some(cleared) = rt.firewall.lock().unwrap().clear_document_search(DocumentQuery::new(query), (rt.now)()) else {
            return end(rt, rec, ActionState::PrivacyDenied, PRIVACY_DENIED.into(), Memo::Refused);
        };
        let found = rt.targets.documents.find(&cleared, 6);
        let Some(best) = found.first() else {
            let msg = format!("No document matching “{query}” was found in {}. Only document files are searched \
                (PDF, Word, Pages, text, spreadsheets, presentations, images), three folders deep.", rt.targets.documents.names());
            return end(rt, rec, ActionState::NoMatches, msg, Memo::Unplanned);
        };
        rec.action = ActionKind::OpenDocument { query: query.clone(), path: Some(best.to_string_lossy().to_string()) };
        rec.choices = found.iter().map(|p| p.to_string_lossy().to_string()).collect();
    }

    // A folder request must be authorized before your folders are searched, too.
    let folder_query = match &rec.action {
        ActionKind::OpenDirectory { query, scope, path: None } | ActionKind::ListDirectory { query, scope, path: None, .. } =>
            Some((query.clone(), scope.clone())),
        _ => None,
    };
    // "Inside" and "it" mean the folder most recently opened or listed.
    let last_folder = rt.book.lock().unwrap().last_folder.clone();
    let folder_query = match (folder_query, last_folder) {
        (Some((q, None)), Some(f)) if q.is_empty() && matches!(rec.action, ActionKind::ListDirectory { .. }) => {
            rec.action = with_folder(&rec.action, &f);
            None
        }
        (fq, _) => fq,
    };
    if let Some((query, scope)) = folder_query {
        if query.is_empty() && scope.is_none() {
            return end(rt, rec, ActionState::NoMatches, format!("{NEED_A_FOLDER}: no folder is open or named. Name it, \
                for example “what resumes are in the Tampa folder on Desktop”."), Memo::Unplanned);
        }
        if let Err(r) = gate(rt.engine, op, rt.authenticate, rt.now) {
            return end(rt, rec, ActionState::Denied, format!("{AUTHORIZATION_REQUIRED} before searching: {r}"), Memo::Refused);
        }
        rec.step(ActionStep::Authorized);
        let Some(cleared) = rt.firewall.lock().unwrap().clear_document_search(DocumentQuery::new(&query), (rt.now)()) else {
            return end(rt, rec, ActionState::PrivacyDenied, PRIVACY_DENIED.into(), Memo::Refused);
        };
        let found = rt.targets.documents.find_folders(&cleared, scope.as_deref(), 6);
        match found.as_slice() {
            [] => {
                let msg = format!("{FOLDER_NOT_FOUND}: no folder matching “{query}” was found in {}, three folders deep.",
                    scope.clone().unwrap_or_else(|| rt.targets.documents.names()));
                return end(rt, rec, ActionState::NoMatches, msg, Memo::Unplanned);
            }
            [one] => rec.action = with_folder(&rec.action, &one.to_string_lossy()),
            many => rec.choices = many.iter().map(|p| p.to_string_lossy().to_string()).collect(),
        }
    }

    // An app name is resolved against the apps installed on this Mac: one app, a
    // choice for you to make, or none — never a guess.
    match rt.targets.resolve_app(&rec.action) {
        Some(AppResolution::Found { app }) => rec.action = rec.action.with_app_name(&app.name),
        Some(AppResolution::Ambiguous { candidates }) => rec.choices = candidates.into_iter().map(|a| a.name).collect(),
        Some(AppResolution::NotFound) if matches!(rec.action, ActionKind::OpenApplication { .. }) => {
            let name = rec.action.app_name().unwrap_or_default().to_string();
            // No app by that name: "Open Tampa" may mean a folder, if exactly one is named exactly that.
            if let Some(folder) = folder_named(rt, &name, op) {
                rec.action = ActionKind::OpenDirectory { query: name.to_lowercase(), scope: None, path: Some(folder) };
                rec.description = rec.action.describe();
            } else {
            return end(rt, rec, ActionState::NoMatches, format!("{APP_NOT_FOUND}: no installed application is named “{name}”. \
                Searched {} apps in /Applications, /System/Applications and ~/Applications.", rt.targets.apps.len()), Memo::Unplanned);
            }
        }
        // Quit and switch act on running apps, which KueAct looks up by name itself.
        Some(AppResolution::NotFound) | None => {}
    }

    let must_choose = rec.needs_choice();
    // Checked after the choice is made, when there is one to make.
    if !must_choose {
        match rt.targets.check(&rec.action) {
            Err(e) => return end(rt, rec, ActionState::Denied, e, Memo::Refused),
            Ok(normalised) => { rec.action = normalised; rec.description = rec.action.describe(); }
        }
    }

    let decision = rt.engine.lock().unwrap().authorize(op, Principal::Owner, (rt.now)());
    match decision {
        Decision::Deny(r) => return end(rt, rec, ActionState::Denied, format!("{AUTHORIZATION_REQUIRED}: {r}"), Memo::Refused),
        Decision::Allow => {
            rec.step(ActionStep::Authorized);
            if must_choose {
                rec.state = ActionState::RequiresConfirmation;
                rec.reason = Some(choose_message(&rec).into());
            } else if !actions::needs_confirmation(risk) {
                rec.state = ActionState::Authorized;
                return execute(rt, rec);
            }
            rec.state = ActionState::RequiresConfirmation;
        }
        Decision::NeedsStrongAuth => {
            rec.state = ActionState::RequiresStrongAuth;
            rec.reason = Some("Confirming will ask macOS for Touch ID or your password.".into());
        }
        Decision::NeedsPhysicalConfirmation => {
            rec.state = ActionState::RequiresStrongAuth;
            rec.reason = Some("Confirming will ask for your finger on Touch ID.".into());
        }
    }
    rec.step(ActionStep::AwaitingConfirmation);
    rec.awaiting_since = Some((rt.now)());
    publish(rt, rec)
}

/// Confirm. Nothing from planning is trusted: targets are checked again and
/// authorization is decided again, now, by the engine.
pub fn confirm(rt: &Runtime, id: &str, choice: Option<String>) -> Result<ActionRecord, String> {
    let mut rec = rt.book.lock().unwrap().claim(id, choice)?;
    (rt.changed)();
    (rt.narrate)(&for_interface(rt, rec.clone()));
    if rt.engine.lock().unwrap().is_killed() {
        return Ok(end(rt, rec, ActionState::Denied, KILLED.into(), Memo::Refused));
    }
    if let Err(e) = rt.targets.check(&rec.action) {
        return Ok(end(rt, rec, ActionState::Denied, e, Memo::Refused));
    }
    // The report is consulted again, at the moment of moving. Between asking
    // and confirming sits Touch ID — seconds, or minutes — and in that time the
    // owner may have worked on one of these files, which takes it straight out
    // of the findings. Checking only when the request was made would move a
    // file KUE would no longer offer.
    if let ActionKind::MoveToTrash { paths } = &rec.action {
        if !rt.book.lock().unwrap().last_storage().is_some_and(|r| r.offered(paths)) {
            return Ok(end(rt, rec, ActionState::Denied, NOT_OFFERED.into(), Memo::Refused));
        }
        // And nothing the owner told KUE to leave alone, however it got here.
        if excluded_by_owner(rt, paths) {
            return Ok(end(rt, rec, ActionState::Denied, LEFT_ALONE.into(), Memo::Refused));
        }
    }
    if let Err(r) = gate(rt.engine, actions::operation_for(rec.risk), rt.authenticate, rt.now) {
        return Ok(end(rt, rec, ActionState::AuthorizationExpired, format!("{AUTHORIZATION_EXPIRED} ({r})"), Memo::Refused));
    }
    rec.step(ActionStep::Reauthorized);
    rec.state = ActionState::Authorized;
    Ok(execute(rt, rec))
}

pub fn cancel(rt: &Runtime, id: &str) {
    let cancelled = {
        let mut book = rt.book.lock().unwrap();
        book.get(id).filter(|r| r.state.is_waiting() || r.state == ActionState::Proposed).map(|mut rec| {
            rec.finish(ActionState::Cancelled, Some(CANCELLED_BY_YOU.into()), None);
            book.put(rec.clone());
            rec
        })
    };
    (rt.changed)();
    if let Some(rec) = cancelled {
        sync_request(rt, &rec);
        (rt.narrate)(&for_interface(rt, rec.clone()));
        advance(rt, &rec);
        if let Some(t) = &rec.task { sync_goal(rt, &t.task_id); }
    }
}

/// Gives a waiting document request a different one of its matches. The new
/// file is a new target: privacy, target policy and authorization are decided
/// again from nothing, and it waits for a fresh confirmation. Nothing runs here.
pub fn retarget(rt: &Runtime, id: &str, choice: &str) -> Result<ActionRecord, String> {
    let mut rec = rt.book.lock().unwrap().take_for_retarget(id, choice)?;
    let now = (rt.now)();
    if rt.engine.lock().unwrap().is_killed() {
        return Ok(end(rt, rec, ActionState::Denied, KILLED.into(), Memo::Refused));
    }
    if !rt.firewall.lock().unwrap().check(DataKind::ActionTarget, Destination::Interface, now).is_allow() {
        return Ok(end(rt, rec, ActionState::PrivacyDenied, PRIVACY_DENIED.into(), Memo::Refused));
    }
    rec.step(ActionStep::PrivacyChecked);
    match rt.targets.check(&rec.action) {
        Err(e) => return Ok(end(rt, rec, ActionState::Denied, e, Memo::Refused)),
        Ok(normalised) => { rec.action = normalised; rec.description = rec.action.describe(); }
    }
    // A statement of its own, so the engine lock is released before `end` records an event.
    let decision = rt.engine.lock().unwrap().authorize(actions::operation_for(rec.risk), Principal::Owner, (rt.now)());
    match decision {
        Decision::Deny(r) => return Ok(end(rt, rec, ActionState::Denied, format!("{AUTHORIZATION_REQUIRED}: {r}"), Memo::Refused)),
        Decision::Allow => { rec.step(ActionStep::Authorized); rec.state = ActionState::RequiresConfirmation; }
        Decision::NeedsStrongAuth | Decision::NeedsPhysicalConfirmation => rec.state = ActionState::RequiresStrongAuth,
    }
    rec.step(ActionStep::AwaitingConfirmation);
    rec.awaiting_since = Some((rt.now)());
    Ok(publish(rt, rec))
}

/// What a spoken or typed reference ("yes", "cancel that", "the older one") did.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceOutcome {
    /// The action it acted on, as the window may show it.
    pub record: Option<ActionRecord>,
    /// A sentence for you when nothing was acted on, or why not. Carries no data.
    pub message: Option<String>,
}

impl ReferenceOutcome {
    fn said(m: &str) -> Self { ReferenceOutcome { record: None, message: Some(m.to_string()) } }
}

pub const NOTHING_WAITING: &str = "There's nothing waiting for your confirmation.";
/// More than one installed app matches the name: you choose; KUE does not guess.
pub const CHOOSE_AN_APP: &str = "More than one app matches that name. Choose the one you mean.";
/// No installed app matches the name.
pub const APP_NOT_FOUND: &str = "APP_NOT_FOUND";
/// More than one folder matches the name.
pub const CHOOSE_A_FOLDER: &str = "More than one folder matches that name. Choose the one you mean.";
/// No folder matches the name.
pub const FOLDER_NOT_FOUND: &str = "FOLDER_NOT_FOUND";
/// "What's inside?" with no folder open or named.
pub const NEED_A_FOLDER: &str = "NEED_A_FOLDER";
pub const MORE_THAN_ONE_WAITING: &str = "More than one request is waiting. Choose the one you mean on screen.";
pub const STRONG_AUTH_ON_SCREEN: &str = "That one needs Touch ID or your password. Confirm it on screen.";
pub const TOO_OLD_TO_CONFIRM: &str = "That request has waited too long to confirm by voice. Confirm it on screen, or ask again.";
pub const CANNOT_STOP_RUNNING: &str = "It's already running, and I can't safely stop it partway.";
pub const NO_SUCH_MATCH: &str = "There isn't a match like that.";

/// Acts on a reference to the action waiting for confirmation. Every path is
/// the window's own: `confirm` (which re-authorizes), `cancel`, `retarget`.
/// A spoken "yes" confirms only a single, recent request that needs no more
/// than the camera's LEVEL_2; one needing Touch ID is confirmed on screen.
pub fn resolve_reference(rt: &Runtime, reference: Reference, max_age_seconds: f64) -> ReferenceOutcome {
    let now = (rt.now)();
    let (waiting, running): (Vec<ActionRecord>, bool) = {
        let book = rt.book.lock().unwrap();
        (book.records().iter().filter(|r| r.state.is_waiting()).cloned().collect(),
         book.records().iter().any(|r| matches!(r.state, ActionState::Executing | ActionState::Reauthorizing)))
    };
    let newest = waiting.iter().max_by(|a, b| a.awaiting_since.unwrap_or(a.created_at)
        .total_cmp(&b.awaiting_since.unwrap_or(b.created_at)));
    match reference {
        Reference::Cancel => match newest {
            Some(r) => {
                cancel(rt, &r.id);
                let after = rt.book.lock().unwrap().get(&r.id);
                ReferenceOutcome { record: after.map(|x| for_interface(rt, x)), message: None }
            }
            None if running => ReferenceOutcome::said(CANNOT_STOP_RUNNING),
            None => ReferenceOutcome { record: None, message: None },
        },
        Reference::Confirm | Reference::Choose(_) if waiting.is_empty() => ReferenceOutcome::said(NOTHING_WAITING),
        Reference::Confirm | Reference::Choose(_) if waiting.len() > 1 => ReferenceOutcome::said(MORE_THAN_ONE_WAITING),
        Reference::Confirm => {
            let r = &waiting[0];
            if r.needs_choice() { return ReferenceOutcome::said(choose_message(r)); }
            if r.state == ActionState::RequiresStrongAuth { return ReferenceOutcome::said(STRONG_AUTH_ON_SCREEN); }
            if now - r.awaiting_since.unwrap_or(r.created_at) > max_age_seconds { return ReferenceOutcome::said(TOO_OLD_TO_CONFIRM); }
            match confirm(rt, &r.id, None) {
                Ok(rec) => ReferenceOutcome { record: Some(rec), message: None },
                Err(e) => ReferenceOutcome::said(&e),
            }
        }
        // Among apps, only a position means anything ("the second one"); "older" does not.
        Reference::Choose(reference::Choice::Position(p)) if waiting[0].needs_choice() => {
            let r = &waiting[0];
            if r.state == ActionState::RequiresStrongAuth { return ReferenceOutcome::said(STRONG_AUTH_ON_SCREEN); }
            if now - r.awaiting_since.unwrap_or(r.created_at) > max_age_seconds { return ReferenceOutcome::said(TOO_OLD_TO_CONFIRM); }
            let Some(choice) = p.checked_sub(1).and_then(|i| r.choices.get(i)).cloned() else { return ReferenceOutcome::said(NO_SUCH_MATCH) };
            match confirm(rt, &r.id, Some(choice)) {
                Ok(rec) => ReferenceOutcome { record: Some(rec), message: None },
                Err(e) => ReferenceOutcome::said(&e),
            }
        }
        Reference::Choose(c) => {
            let r = &waiting[0];
            let ActionKind::OpenDocument { path: Some(current), .. } = &r.action else { return ReferenceOutcome::said(NO_SUCH_MATCH) };
            let at = r.choices.iter().position(|x| x == current).unwrap_or(0);
            let Some(i) = reference::pick(c, at, r.choices.len()) else { return ReferenceOutcome::said(NO_SUCH_MATCH) };
            if i == at { return ReferenceOutcome { record: Some(for_interface(rt, r.clone())), message: None }; }
            match retarget(rt, &r.id, &r.choices[i].clone()) {
                Ok(rec) => ReferenceOutcome { record: Some(rec), message: None },
                Err(e) => ReferenceOutcome::said(&e),
            }
        }
    }
}

/// Runs an authorized action and records the verified outcome.
fn execute(rt: &Runtime, mut rec: ActionRecord) -> ActionRecord {
    // The last check, immediately before anything happens on the Mac.
    let stopped = {
        let mut e = rt.engine.lock().unwrap();
        if e.is_killed() { drop(e); return end(rt, rec, ActionState::Denied, KILLED.into(), Memo::Refused); }
        match served() {
            // The owner said "stop" after asking for this and before it ran.
            Some((_, epoch)) if e.stopped_since(epoch) => true,
            // From here on, "stop" meets work that is underway: the request is
            // ACTING before the lock is released, so no "stop" can arrive in
            // between and be told nothing will change.
            Some((rid, _)) => { e.requests_mut().begin_acting(&rid, (rt.now)()); false }
            None => false,
        }
    };
    if stopped { return end(rt, rec, ActionState::Cancelled, STOPPED_BY_YOU.into(), Memo::Refused); }
    rec.state = ActionState::Executing;
    rec.step(ActionStep::Executing);
    publish(rt, rec.clone());
    let (st, reason, verification, output) = match &rec.action {
        ActionKind::CreateDirectory { .. } | ActionKind::CreateFile { .. } | ActionKind::ReadPermittedFile { .. }
        | ActionKind::MovePermittedFile { .. } => actions::execute_file_action(&rec.action, &rt.targets.permitted, &rt.targets.home),
        // Read in this process: names, kinds and dates, never contents. What was read is the verification.
        ActionKind::ListDirectory { filter, path: Some(p), .. } => match rt.targets.documents.list_folder(p, filter, LIST_LIMIT) {
            Ok(entries) => {
                rec.choices = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
                let what = if filter.is_empty() { "item(s)".to_string() } else { format!("item(s) matching “{filter}”") };
                let output = if entries.is_empty() { format!("Nothing in {} matches.", actions::tilde(p)) }
                    else { entries.iter().map(|e| e.line()).collect::<Vec<_>>().join("\n") };
                (ActionState::Succeeded, None, Some(format!("read {}: {} {what}, newest first", actions::tilde(p), entries.len())), Some(output))
            }
            Err(e) => (ActionState::Failed, Some(e), None, None),
        },
        // Read in this process too, and for the same reason: nothing leaves KUE,
        // and what was read is the verification. The firewall clears the pass
        // before a single folder is opened.
        ActionKind::InspectStorage => match take_stock(rt) {
            Ok(report) => {
                let out = report_lines(&report);
                rec.sentences = Some(actions::Sentences { on_screen: report.finding(), aloud: report.spoken() });
                let verification = report.verification();
                rt.book.lock().unwrap().last_storage = Some(report);
                (ActionState::Succeeded, None, Some(verification), Some(out))
            }
            // Stopped by the owner is a cancellation, not a failure.
            Err(e) if e == STOPPED_BY_YOU => (ActionState::Cancelled, Some(e), None, None),
            Err(e) => (ActionState::Failed, Some(e), None, None),
        },
        // One file at a time, each checked on its own. A batch is never
        // reported by the state of the last one in it: three of forty failing
        // must not read as success, and the record says which.
        ActionKind::MoveToTrash { paths } => {
            let mut moved: Vec<TrashedItem> = Vec::new();
            let mut refused: Vec<String> = Vec::new();
            for path in paths {
                if rt.engine.lock().unwrap().is_killed() { break; }
                let done = (rt.execute_os)(&ExecutorRequest::trash(path));
                match (done.state, done.landed) {
                    (ActionState::Succeeded, Some(landed)) =>
                        moved.push(TrashedItem { original: path.clone(), trashed: landed }),
                    (state, _) => refused.push(format!("{} — {}", file_name(path),
                        done.reason.unwrap_or_else(|| format!("{state:?} with no reason given")))),
                }
            }
            let size = rt.book.lock().unwrap().last_storage().map(|r| r.size_of(
                &moved.iter().map(|m| m.original.clone()).collect::<Vec<_>>())).unwrap_or(0);
            let (on_screen, aloud) = storage::moved_sentences(moved.len(), paths.len(), size);
            rec.sentences = Some(actions::Sentences { on_screen, aloud });
            let verification = (!moved.is_empty()).then(|| format!("moved {} of {} to the Trash, each checked: {}",
                moved.len(), paths.len(),
                moved.iter().map(|m| format!("{} → {}", file_name(&m.original), actions::tilde(&m.trashed)))
                    .collect::<Vec<_>>().join("; ")));
            {
                let mut book = rt.book.lock().unwrap();
                book.last_trashed = moved.clone();
                let done: Vec<String> = moved.iter().map(|m| m.original.clone()).collect();
                if let Some(report) = book.last_storage.as_mut() { report.forget(&done); }
            }
            let state = if moved.len() == paths.len() { ActionState::Succeeded } else { ActionState::Failed };
            let reason = (!refused.is_empty()).then(|| format!("{} did not move: {}", refused.len(), refused.join("; ")));
            (state, reason, verification, None)
        }
        ActionKind::RestoreFromTrash { items } => {
            let mut back = 0usize;
            let mut refused: Vec<String> = Vec::new();
            for item in items {
                let done = (rt.execute_os)(&ExecutorRequest::untrash(item));
                if done.state == ActionState::Succeeded { back += 1; }
                else { refused.push(format!("{} — {}", file_name(&item.original),
                    done.reason.unwrap_or_else(|| "no reason given".into()))); }
            }
            let (on_screen, aloud) = storage::restored_sentences(back, items.len());
            rec.sentences = Some(actions::Sentences { on_screen, aloud });
            if back > 0 { rt.book.lock().unwrap().last_trashed.clear(); }
            let state = if back == items.len() { ActionState::Succeeded } else { ActionState::Failed };
            let verification = (back > 0).then(|| format!("{back} of {} back where they came from, each checked", items.len()));
            let reason = (!refused.is_empty()).then(|| format!("{} did not go back: {}", refused.len(), refused.join("; ")));
            (state, reason, verification, None)
        }
        other => match ExecutorRequest::for_action(other) {
            Some(req) => { let e = (rt.execute_os)(&req); (e.state, e.reason, e.verification, None) }
            None => (ActionState::Failed, Some("No executor handles this action.".into()), None, None),
        },
    };
    rec.finish(st, reason, verification);
    rec.output = output;
    if rec.state == ActionState::Succeeded {
        rec.step(ActionStep::Verified);
        // What KUE did, and confirmed. This is the one route to a VERIFIED
        // fact: it requires the read-back the executor produced, and a model
        // has no way to reach it (facts.rs). Without this, a model describing a
        // confirmed action as uncertain has nothing to contradict it.
        if let Some(proof) = rec.verification.as_deref().and_then(crate::facts::Verification::of) {
            let tag = rec.action.tag();
            rt.engine.lock().unwrap().record_fact(crate::facts::Fact::verified(
                &format!("action:{tag}"),
                &format!("KUE performed {tag} and verified it"),
                crate::privacy::DataKind::EventRecord,
                (rt.now)(), &tag, proof));
        }
        if let Some(folder) = folder_of(&rec.action) { rt.book.lock().unwrap().last_folder = Some(folder); }
    }
    rt.engine.lock().unwrap().record_action_event(rec.event_summary(), (rt.now)());
    publish(rt, rec)
}

/// Takes stock of storage: the firewall first, then the volume, then one pass
/// over the allowed folders. Err is a refusal or a failure, never a partial
/// report presented as whole.
fn take_stock(rt: &Runtime) -> Result<StorageReport, String> {
    let now = (rt.now)();
    let request = StorageRequest { areas: rt.targets.documents.names() };
    // Two clearances, because they are two different things: the per-file
    // inventory may only reach the window, while the totals name nothing and
    // may be spoken. Either refusal stops the whole pass.
    let permit = {
        let mut fw = rt.firewall.lock().unwrap();
        let permit = fw.clear_storage_inventory(request, now).ok_or(PRIVACY_DENIED)?;
        fw.clear_storage_summary(storage::StorageSummary { volume: None, areas: Vec::new() }, now).ok_or(PRIVACY_DENIED)?;
        permit
    };
    let epoch = served().map(|(_, e)| e).unwrap_or_else(|| rt.engine.lock().map(|e| e.stop_epoch()).unwrap_or(0));
    let stop = || rt.engine.lock().map(|e| e.stopped_since(epoch)).unwrap_or(true);
    let inventory = storage::take_inventory_until(&rt.targets.documents, &permit, now, &stop);
    if inventory.stopped { return Err(STOPPED_BY_YOU.into()); }
    let (volume, note) = match (rt.measure_volume)(&rt.targets.home) {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    Ok(storage::analyze(&inventory, volume, note, now))
}

/// The report as the window shows it: the totals, what KUE could not see, and
/// each finding with its evidence.
fn report_lines(r: &StorageReport) -> String {
    let mut out = vec![r.headline()];
    out.extend(r.area_lines());
    out.extend(r.limits());
    out.push(r.finding());
    out.join("\n")
}

/// A file's own name, for saying what happened to it without repeating its
/// whole location.
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

/// The most items one LIST_DIRECTORY reports.
pub const LIST_LIMIT: usize = 50;

/// The action list for the window, or why it is withheld. Withheld records are
/// kept, not cancelled: they reappear when you are confirmed again, and Confirm
/// re-authorizes anyway. Kill and lock cancel them separately.
#[derive(Debug, Clone, Serialize)]
pub struct ActionList {
    pub visible: bool,
    /// KILLED · AUTHORIZATION_REQUIRED · PRIVACY_DENIED, when not visible.
    pub withheld: Option<&'static str>,
    pub withheld_because: Option<String>,
    pub records: Vec<ActionRecord>,
    /// One line on the latest action, from the records shown (never from withheld ones).
    pub live: Option<LiveStatus>,
    /// KUE's own sentence for each record shown, in the same order — what the
    /// window puts on screen. The window composes no sentence about KUE, so an
    /// action it cannot narrate is an action it shows by its description alone,
    /// never one it describes in words of its own.
    pub said: Vec<ActionLine>,
    /// Requests of several steps: each step's kind and state, no targets.
    pub tasks: Vec<TaskView>,
}

/// One record's sentence, and how far it got.
#[derive(Debug, Clone, Serialize)]
pub struct ActionLine {
    pub id: String,
    pub said: String,
    /// WORKING · WAITING · DONE · UNCERTAIN · WRONG — what the window may show
    /// as a state, decided here rather than from the state name's spelling.
    pub tone: &'static str,
    /// True only for a SUCCEEDED record that recorded what it verified.
    pub verified: bool,
}

fn lines(records: &[ActionRecord]) -> Vec<ActionLine> {
    records.iter().filter_map(|r| {
        let tone = match r.state {
            ActionState::Executing | ActionState::Reauthorizing => "WORKING",
            ActionState::RequiresConfirmation | ActionState::RequiresStrongAuth => "WAITING",
            ActionState::Succeeded if r.verification.is_some() => "DONE",
            // A success with nothing recorded as verified is not a success.
            ActionState::Succeeded | ActionState::UnknownResult => "UNCERTAIN",
            ActionState::Failed | ActionState::PartiallySucceeded | ActionState::Denied
            | ActionState::AuthorizationExpired | ActionState::PrivacyDenied
            | ActionState::NoMatches => "WRONG",
            ActionState::Proposed | ActionState::Authorized | ActionState::Cancelled => return None,
        };
        Some(ActionLine {
            id: r.id.clone(),
            said: crate::voice::narration::describe(r)?,
            tone,
            verified: r.state == ActionState::Succeeded && r.verification.is_some(),
        })
    }).collect()
}

/// The last storage report, for the window to show, under the same rules as
/// the action list: nothing while KUE is stopped, nothing below LEVEL_2, and
/// the firewall asked again on every fetch rather than once when it was made.
/// `withheld` says why there is nothing, so the window never has to guess.
#[derive(Debug, Clone, Serialize)]
pub struct StorageView {
    pub report: Option<StorageReport>,
    pub withheld: Option<&'static str>,
    /// KUE's own sentences about this report. The window arranges them; it
    /// writes none of them, and with no report there are none to write.
    pub said: Option<StorageSaid>,
    /// How many files the last move to the Trash moved and could put back. 0
    /// when there is nothing to undo.
    pub can_undo: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageSaid {
    pub headline: String,
    pub finding: String,
    pub areas: Vec<String>,
    pub limits: Vec<String>,
    /// How long ago this was measured. A report is a photograph, not a reading.
    pub measured: String,
    /// What KUE cannot do with what it found, in the registry's own words, so
    /// the window cannot promise more than KUE has.
    pub cannot: String,
}

pub fn storage(rt: &Runtime) -> StorageView {
    let now = (rt.now)();
    let hidden = |code| StorageView { report: None, withheld: Some(code), said: None, can_undo: 0 };
    let (killed, access) = { let e = rt.engine.lock().unwrap(); (e.is_killed(), e.access_block(now)) };
    if killed { return hidden("KILLED"); }
    if access.level < AuthLevel::Level2 { return hidden("AUTHORIZATION_REQUIRED"); }
    let (last, can_undo) = { let b = rt.book.lock().unwrap(); (b.last_storage().cloned(), b.last_trashed().len()) };
    let Some(report) = last else {
        return StorageView { report: None, withheld: None, said: None, can_undo };
    };
    let request = StorageRequest { areas: rt.targets.documents.names() };
    match rt.firewall.lock().unwrap().clear_storage_inventory(request, now) {
        Some(_) => StorageView {
            said: Some(StorageSaid { headline: report.headline(), finding: report.finding(),
                areas: report.area_lines(), limits: report.limits(), measured: report.measured_said(now),
                cannot: crate::capabilities::find("permanent_deletion").map(|c| c.ui_description.to_string()).unwrap_or_default() }),
            report: Some(report),
            withheld: None,
            can_undo,
        },
        None => hidden("PRIVACY_DENIED"),
    }
}

/// Files the owner picked out of the last report, for the Trash. The paths are
/// checked against that report before anything else happens.
///
/// When a cleanup goal is waiting for this choice, the choice is its answer:
/// verified against the report, and only then does its move step start.
pub fn propose_trash(rt: &Runtime, paths: Vec<String>, source: &str) -> ActionRecord {
    let now = (rt.now)();
    let waiting = {
        let book = rt.book.lock().unwrap();
        book.goals.iter().rev()
            .find_map(|g| g.waiting_for_user().filter(|s| g.kind == GoalKind::CleanUpStorage && s.kind == StepKind::WaitForApproval)
                .map(|s| (g.id.clone(), s.index)))
    };
    if let Some((goal_id, i)) = waiting {
        let offered = rt.book.lock().unwrap().last_storage().is_some_and(|r| r.offered(&paths));
        let verdict = if offered {
            StepEvent::Verified("Every chosen file was offered by this request's storage report.".into())
        } else {
            StepEvent::VerificationFailed(NOT_OFFERED.into())
        };
        {
            let mut book = rt.book.lock().unwrap();
            if let Some(g) = book.goal_mut(&goal_id) {
                if offered { g.chosen = paths.clone(); }
            }
        }
        step_events(rt, &goal_id, i, vec![StepEvent::OwnerAnswered,
            StepEvent::Observed(format!("You chose {} file{}.", paths.len(), if paths.len() == 1 { "" } else { "s" })), verdict], now);
        record_step(rt, &goal_id, i, now);
        if let Some(rec) = drive(rt, &goal_id) { return rec; }
        // The choice did not verify, so the goal stopped. The move is refused
        // exactly as it would be on its own.
    }
    propose_kind(rt, ActionKind::MoveToTrash { paths }, source, None)
}

/// Checks a plan a model proposed against the declared tools and this moment:
/// the engine's authorization PREVIEW (grants nothing, consumes nothing), the
/// firewall's decision without recording one, the target policy, the latest
/// storage check. Runs nothing, records nothing, and is connected to no model
/// in this build.
///
/// `executor_present`: whether the KueAct helper is in the bundle — the shell
/// knows; the core does not.
pub fn validate_proposal(rt: &Runtime, json: &str, proposer: &str, executor_present: bool) -> crate::plan::Verdict {
    use crate::plan::{self, Verdict};
    let now = (rt.now)();
    let proposal = match plan::parse(json) {
        Ok(p) => p,
        Err(problem) => return Verdict::Rejected(vec![problem]),
    };
    let (conditions, plan_id) = {
        let e = rt.engine.lock().unwrap();
        (crate::tools::Conditions { killed: e.is_killed(), paused: e.is_paused(), executor_present },
         format!("proposal-{}", e.requests().requests().len() + 1))
    };
    let authorization = |op: Operation| rt.engine.lock().unwrap().preview_authorization(op, now);
    let targets = |k: &ActionKind| rt.targets.check(k);
    let fw = rt.firewall.lock().unwrap();
    let privacy = |k: DataKind, d: Destination| fw.would_allow(k, d);
    let book = rt.book.lock().unwrap();
    let ctx = plan::Context { conditions, authorization: &authorization, targets: &targets, privacy: &privacy,
                              storage: book.last_storage(), trashed: book.last_trashed() };
    plan::validate(&proposal, proposer, &plan_id, now, &ctx)
}

/// What happened to a proposed plan.
#[derive(Debug, Clone)]
pub enum PlanProposed {
    /// Checked and sound. KUE has said what it would do and waits for the
    /// owner; nothing has run.
    Waiting { plan_id: String, lines: Vec<String> },
    /// Not proposed to the owner at all: wrong, or impossible right now.
    /// `said` is KUE's sentence; `problems` are for Diagnostics.
    Refused { said: String, problems: Vec<crate::plan::Problem> },
}

pub const PLAN_NOT_HELD: &str = "I don't have that plan any more. Ask me again and I'll lay it out.";
pub const PLAN_ALREADY_APPROVED: &str = "You already approved that one.";
pub const PLAN_PAUSED: &str = "KUE is paused, so I won't start it. Resume and say it again.";
pub const PLAN_CHANGED: &str = "Something changed since I showed you that plan, so I didn't start it. Ask me again and I'll lay out a fresh one.";
pub const PLAN_NOT_ACTIONS: &str = "That plan has a step I can't carry out as an action yet, so I didn't start it.";
pub const PLAN_CANCELLED: &str = "Cancelled. Nothing was done.";
pub const PLAN_WAS_CANCELLED: &str = "You cancelled that plan. Ask me again and I'll lay out a fresh one.";
/// An approved plan whose first step produced no action record at all.
pub const PLAN_NOTHING_STARTED: &str = "I approved that, but nothing started. Nothing was changed.";

/// One sentence for the owner about why a plan was not proposed. Names tools
/// and steps — KUE's own vocabulary — and never a file.
fn why_refused(problems: &[crate::plan::Problem]) -> String {
    use crate::plan::Problem as P;
    let step = |i: &usize| format!("step {}", i + 1);
    let first = match problems.first() {
        None => return "I can't do that.".into(),
        Some(p) => match p {
            P::Unreadable(_) => "that plan wasn't written in a shape I can read".to_string(),
            P::Empty => "that plan had no steps".to_string(),
            P::TooManySteps(n) => format!("that plan had {n} steps, which is more than I'll take at once"),
            P::UnknownTool { step: i, .. } => format!("{} asks for something I don't have", step(i)),
            P::MissingInput { step: i, .. } | P::WrongType { step: i, .. } =>
                format!("{} doesn't give me what that step needs", step(i)),
            P::UnexpectedInput { step: i, .. } => format!("{} carries something that step doesn't take", step(i)),
            P::BadDependency { step: i, .. } => format!("{} depends on a step that comes later", step(i)),
            P::NoVerifier { step: i } => format!("I couldn't check what {} did", step(i)),
            P::Incomplete { why, .. } => why.trim_end_matches('.').to_string(),
            P::Unavailable { why, .. } => why.trim_end_matches('.').to_string(),
            P::NotAuthorized { .. } => "I'm not sure it's you right now".to_string(),
            P::PrivacyDenied { .. } => "the privacy firewall wouldn't allow part of it".to_string(),
            P::PreconditionFailed { why, .. } => why.trim_end_matches('.').to_string(),
        },
    };
    let more = match problems.len() { 1 => String::new(), n => format!(" ({} other problem{}.)", n - 1, if n == 2 { "" } else { "s" }) };
    format!("I can't do that plan: {first}.{more}")
}

/// A plan someone proposed, checked against the declared tools and this
/// moment, then either laid out for the owner or refused.
///
/// Nothing runs here. A sound plan waits as `Open::Plan`; the owner's "do it"
/// reaches `approve_plan`, and every step is still authorized, confirmed and
/// verified by the transaction when it runs.
pub fn propose_plan(rt: &Runtime, json: &str, proposer: &str, source: &str, executor_present: bool) -> PlanProposed {
    propose_version(rt, json, proposer, source, executor_present, None, &[])
}

/// The same, recording which of the owner's memories were put in front of the
/// proposer. They are provenance — what KUE showed, so it can say later why a
/// plan looks as it does — and they grant nothing.
pub fn propose_plan_informed_by(rt: &Runtime, json: &str, proposer: &str, source: &str, executor_present: bool,
                                memory_refs: &[String]) -> PlanProposed {
    propose_version(rt, json, proposer, source, executor_present, None, memory_refs)
}

/// The same checks, for a plan that came from one the owner changed. The
/// lineage is recorded and shown; it grants nothing. A changed plan is
/// validated from the beginning, exactly as the model's own text was.
fn propose_version(rt: &Runtime, json: &str, proposer: &str, source: &str, executor_present: bool,
                   from: Option<(String, u32)>, memory_refs: &[String]) -> PlanProposed {
    let now = (rt.now)();
    let problems = match validate_proposal(rt, json, proposer, executor_present) {
        crate::plan::Verdict::Valid { plan, actions } => {
            // Every step must be an action this build can run.
            let kinds: Vec<ActionKind> = actions.iter().flatten().cloned().collect();
            if kinds.len() != plan.steps.len() {
                rt.engine.lock().unwrap().record_action_event(
                    format!("Plan by {proposer}: refused, a step is not an action ({} of {} steps).", kinds.len(), plan.steps.len()), now);
                return PlanProposed::Refused { said: PLAN_NOT_ACTIONS.into(), problems: Vec::new() };
            }
            let steps = plan.steps.len();
            let lines = crate::plan::preview(&plan).lines;
            let (revision_of, version) = match &from {
                Some((id, v)) => (Some(id.clone()), v + 1),
                None => (None, 1),
            };
            let held = HeldPlan { plan, proposer: proposer.to_string(), source: source.to_string(), goal_id: None,
                                  json: json.to_string(), executor_present, actions: kinds, revision_of, version,
                                  memory_refs: memory_refs.to_vec() };
            let plan_id = rt.book.lock().unwrap().hold_plan(held);
            rt.engine.lock().unwrap().record_action_event(match &from {
                Some((old, _)) => format!("Plan {plan_id}: {old} as you changed it, {steps} step(s), checked again from the beginning.                                            {old} is not approved and cannot be started."),
                None => format!("Plan {plan_id}: proposed by {proposer}, {steps} step(s), checked and waiting for you."),
            }, now);
            return PlanProposed::Waiting { plan_id, lines };
        }
        crate::plan::Verdict::Rejected(p) | crate::plan::Verdict::Blocked(p) => p,
    };
    let tag = problems.first().map(|p| serde_json::to_value(p).ok()
        .and_then(|v| v.get("problem").and_then(|x| x.as_str().map(str::to_string)))
        .unwrap_or_default()).unwrap_or_default();
    rt.engine.lock().unwrap().record_action_event(
        format!("Plan by {proposer}: refused before anything ran ({tag}, {} problem(s)).", problems.len()), now);
    PlanProposed::Refused { said: why_refused(&problems), problems }
}

/// Lays a checked plan before the owner: KUE says what it would do, the plan
/// waits as the one open thing, and the request records that KUE decided to
/// PLAN rather than act. Nothing runs until the owner says so.
pub fn offer_plan(rt: &Runtime, request_id: &str, json: &str, proposer: &str, source: &str,
                  executor_present: bool) -> PlanProposed {
    offer_plan_informed_by(rt, request_id, json, proposer, source, executor_present, &[])
}

/// The same, recording which of the owner's memories were put in front of the
/// proposer — so KUE can say afterwards what shaped the plan it is showing.
pub fn offer_plan_informed_by(rt: &Runtime, request_id: &str, json: &str, proposer: &str, source: &str,
                              executor_present: bool, memory_refs: &[String]) -> PlanProposed {
    let now = (rt.now)();
    let outcome = propose_plan_informed_by(rt, json, proposer, source, executor_present, memory_refs);
    match &outcome {
        PlanProposed::Waiting { plan_id, lines } => {
            {
                let mut e = rt.engine.lock().unwrap();
                let p = e.requests_mut();
                let _ = p.walk_to(request_id, RequestState::Deciding, now, None);
                let _ = p.decide(request_id, PipelineDecision::Plan, now);
                let _ = p.walk_to(request_id, RequestState::Planning, now, Some("A plan was proposed and checked."));
            }
            for l in lines { say(rt, TurnKind::KueResponse, l, request_id); }
            speak(rt, &lines.join(" "), &[]);
            let open = Open::Plan { request_id: request_id.to_string(), plan_id: plan_id.clone() };
            let _ = rt.engine.lock().unwrap().requests_mut().wait_for(open, now);
        }
        PlanProposed::Refused { said, .. } => {
            walk(rt, request_id, RequestState::Refused, Some("The plan did not check out."));
            say(rt, TurnKind::KueResponse, said, request_id);
            speak(rt, said, &[]);
        }
    }
    outcome
}

/// The owner's yes to exactly one plan. Checks the moment again — an approval
/// is for now, not for when the plan was shown — and then starts it as a goal
/// built from precisely the steps that were approved.
///
/// This is added to each step's own authorization, never used in its place:
/// the goal's steps are authorized, confirmed and verified as they run.
pub fn approve_plan(rt: &Runtime, plan_id: &str) -> Result<Option<ActionRecord>, String> {
    let now = (rt.now)();
    let Some((json, proposer, source, executor_present, approved, cancelled, approved_actions)) = ({
        let b = rt.book.lock().unwrap();
        b.plan(plan_id).map(|h| (h.json.clone(), h.proposer.clone(), h.source.clone(), h.executor_present,
                                 h.approved() || h.goal_id.is_some(),
                                 h.plan.state == crate::plan::PlanState::Cancelled, h.actions.clone()))
    }) else { return Err(PLAN_NOT_HELD.into()) };
    if approved { return Err(PLAN_ALREADY_APPROVED.into()); }
    if cancelled { return Err(PLAN_WAS_CANCELLED.into()); }
    {
        let e = rt.engine.lock().unwrap();
        if e.is_killed() { return Err(KILLED.into()); }
        if e.is_paused() { return Err(PLAN_PAUSED.into()); }
        // The plan is the owner's to approve — checked here too, so no caller
        // can start one on someone else's say-so.
        if e.access_block(now).level < AuthLevel::Level2 { return Err(NOT_SURE_ITS_YOU.into()); }
    }
    // The same checks as when it was proposed, against this moment.
    let (steps_now, actions) = match validate_proposal(rt, &json, &proposer, executor_present) {
        crate::plan::Verdict::Valid { plan, actions } => (plan.steps, actions),
        crate::plan::Verdict::Rejected(p) | crate::plan::Verdict::Blocked(p) => {
            rt.book.lock().unwrap().plan_mut(plan_id).map(|h| h.plan.state = crate::plan::PlanState::Cancelled);
            rt.engine.lock().unwrap().record_action_event(
                format!("Plan {plan_id}: not started, it no longer checks out ({} problem(s)).", p.len()), now);
            return Err(why_refused(&p));
        }
    };
    // Exactly what the owner was shown: the same tools, in the same order, on
    // the same targets. Anything else is a different plan and needs its own yes.
    let kinds: Vec<ActionKind> = actions.into_iter().flatten().collect();
    if kinds.len() != steps_now.len() { return Err(PLAN_NOT_ACTIONS.into()); }
    if kinds != approved_actions {
        rt.book.lock().unwrap().plan_mut(plan_id).map(|h| h.plan.state = crate::plan::PlanState::Cancelled);
        rt.engine.lock().unwrap().record_action_event(format!("Plan {plan_id}: not started, its steps are not the ones you saw."), now);
        return Err(PLAN_CHANGED.into());
    }

    rt.engine.lock().unwrap().record_action_event(
        format!("Plan {plan_id}: approved by the owner, {} step(s). Each step is still authorized when it runs.", kinds.len()), now);
    // Kept, so "why did you do that?" has an answer after the conversation has
    // moved on: the owner agreed to this plan, on this day. The tools, never
    // the targets.
    {
        let tools: Vec<String> = steps_now.iter().map(|s| s.step.to_lowercase().replace('_', " ")).collect();
        let mut e = rt.engine.lock().unwrap();
        if e.memory_refusal().is_none() {
            let id = e.memory_mut().id(now);
            let m = crate::memory::Memory::decided(&id, &format!("decision:{plan_id}"),
                &format!("You approved a plan of {} step{}: {}", tools.len(),
                         if tools.len() == 1 { "" } else { "s" }, tools.join(", ")),
                now, &format!("you approved plan {plan_id}")).about(plan_id);
            let _ = e.memory_mut().remember(m, false, now);
        }
    }
    // Recorded before it starts: its first step runs inside `start_goal`, and
    // the approval has to be on the plan by then.
    {
        let mut book = rt.book.lock().unwrap();
        let goal_id = book.next_goal_id();
        if let Some(h) = book.plan_mut(plan_id) {
            h.goal_id = Some(goal_id);
            h.plan.state = crate::plan::PlanState::Running;
            h.plan.approved_by.push(crate::plan::Approval { by: crate::plan::Approver::Owner, at: now, step: 0 });
        }
    }
    let record = start_goal(rt, Blueprint::commands(kinds), &source);
    // What the goal actually got, in case anything else made one first.
    if let Some(actual) = record.as_ref().and_then(|r| r.task.as_ref().map(|t| t.task_id.clone()))
        .or_else(|| rt.book.lock().unwrap().goals().last().map(|g| g.id.clone())) {
        if let Some(h) = rt.book.lock().unwrap().plan_mut(plan_id) { h.goal_id = Some(actual); }
    }
    Ok(record)
}

/// The owner's no. The plan is dropped; nothing it named was touched.
pub fn cancel_plan(rt: &Runtime, plan_id: &str) {
    let now = (rt.now)();
    if let Some(h) = rt.book.lock().unwrap().plan_mut(plan_id) {
        if h.goal_id.is_none() { h.plan.state = crate::plan::PlanState::Cancelled; }
    }
    rt.engine.lock().unwrap().record_action_event(format!("Plan {plan_id}: cancelled by the owner before anything ran."), now);
}


pub const PLAN_RUNNING: &str = "That one's already started, so I won't change it underneath itself. Say “stop”, and ask me again.";
pub const PLAN_NOTHING_LEFT: &str = "That takes out every step, so there's no plan left. I've dropped it, and nothing was changed.";
pub const PLAN_NO_VERSION_BEFORE: &str = "That's the first version of this plan — there's nothing to go back to.";
pub const PLAN_NO_DESTINATION: &str = "That plan doesn't put anything anywhere, so there's no destination to change.";

/// The owner changing a plan they have not agreed to.
///
/// A changed plan is a DIFFERENT plan. It is read and checked from the
/// beginning against the declarations and this moment, it is held under a new
/// identity, and it needs its own yes. The approval of the plan it came from
/// does not reach it — nothing carries over but the owner's words — and the
/// plan the owner is looking at is always the plan a "do it" would start.
///
/// A change can only take steps out or move where the plan writes. There is
/// no way for a correction to add a step: a step nobody proposed has never
/// been checked, and KUE does not invent one.
pub fn change_plan(rt: &Runtime, request_id: &str, plan_id: &str, said: &str) -> Received {
    use crate::dialogue::PlanChange;
    let now = (rt.now)();
    let Some(held) = rt.book.lock().unwrap().plan(plan_id).cloned() else {
        return plan_says(rt, request_id, PLAN_NOT_HELD);
    };
    // A plan that is running is past changing: its first steps may already
    // have happened, and a "plan" that no longer describes what was done is
    // worse than no plan.
    if held.approved() || held.goal_id.is_some() { return plan_says(rt, request_id, PLAN_RUNNING); }
    if held.plan.state == crate::plan::PlanState::Cancelled { return plan_says(rt, request_id, PLAN_WAS_CANCELLED); }

    let change = crate::dialogue::plan_change(said, &held.step_words());
    // What the changed plan would say. Nothing is held, approved or run here.
    let (json, from) = match &change {
        // Not understood well enough to act on. The plan is untouched and
        // still waiting: an unclear correction never silently narrows a plan.
        PlanChange::Ask(question) => {
            say(rt, TurnKind::KueQuestion, question, request_id);
            speak(rt, question, &[]);
            rt.engine.lock().unwrap().record_action_event(
                format!("Plan {plan_id}: a change was asked about, not applied. It is unchanged and still waiting."), now);
            return Received::Clarify { request_id: request_id.to_string(), question: question.clone() };
        }
        PlanChange::GoBack => {
            let before = held.revision_of.as_ref()
                .and_then(|id| rt.book.lock().unwrap().plan(id).map(|p| (p.json.clone(), p.version)));
            let Some((json, version)) = before else { return plan_says(rt, request_id, PLAN_NO_VERSION_BEFORE) };
            // Going back is itself a change: the earlier text is proposed
            // again, checked again, and given an identity of its own.
            (json, Some((plan_id.to_string(), held.version.max(version))))
        }
        PlanChange::Drop(_) | PlanChange::Destination(_) => {
            let Ok(proposal) = crate::plan::parse(&held.json) else {
                return plan_says(rt, request_id, PLAN_NOT_HELD);
            };
            let edited = match &change {
                PlanChange::Drop(indices) => crate::plan::without_steps(&proposal, indices),
                PlanChange::Destination(name) => crate::plan::into_destination(&proposal, name),
                _ => unreachable!("the arm above matches only these two"),
            };
            match edited {
                Ok(p) => (crate::plan::text_of(&p), Some((plan_id.to_string(), held.version))),
                Err(why) => {
                    // Nothing left to do is the one refusal that ends the plan:
                    // the owner took every step out of it.
                    if why == crate::plan::EditRefused::NothingLeft {
                        cancel_plan(rt, plan_id);
                        rt.engine.lock().unwrap().requests_mut().dialogue.set_open(None);
                        walk(rt, request_id, RequestState::Cancelled, Some("Every step was taken out of the plan."));
                        return plan_says(rt, request_id, PLAN_NOTHING_LEFT);
                    }
                    return plan_says(rt, request_id, &why_not_changed(&why));
                }
            }
        }
    };

    // A changed plan keeps the provenance of the one it came from: the same
    // memories were in front of whoever proposed it.
    let refs = held.memory_refs.clone();
    match propose_version(rt, &json, &held.proposer, &held.source, held.executor_present, from, &refs) {
        PlanProposed::Waiting { plan_id: new_id, lines } => {
            // The plan the owner was looking at stops existing the moment the
            // one they asked for is sound. Approving the old one is refused
            // from here on, by its own state — not by remembering to check.
            cancel_plan_superseded(rt, plan_id, &new_id);
            // The request goes back through planning: it was waiting on the
            // owner, KUE worked out a different plan, and now it waits again —
            // on THAT plan. Without this the request never leaves the state it
            // is in, and what is open would still be the plan that is gone.
            walk(rt, request_id, RequestState::Planning, Some("The owner changed the plan."));
            for l in &lines { say(rt, TurnKind::KueResponse, l, request_id); }
            speak(rt, &lines.join(" "), &[]);
            let open = Open::Plan { request_id: request_id.to_string(), plan_id: new_id };
            rt.engine.lock().unwrap().requests_mut().wait_for(open, now)
                .expect("a request that was waiting on a plan can wait on the plan that replaced it");
            Received::Revised { request_id: request_id.to_string(), said: lines.join(" ") }
        }
        // The change made a plan KUE cannot do. The one the owner has stays
        // exactly as it was, still waiting.
        PlanProposed::Refused { said, .. } => {
            rt.engine.lock().unwrap().record_action_event(
                format!("Plan {plan_id}: the change did not check out, so it is unchanged and still waiting."), now);
            plan_says(rt, request_id, &said)
        }
    }
}

fn why_not_changed(why: &crate::plan::EditRefused) -> String {
    use crate::plan::EditRefused as E;
    match why {
        E::NoSuchStep(n) => format!("There's no step {n} in that plan."),
        E::NothingLeft => PLAN_NOTHING_LEFT.to_string(),
        E::NoDestination => PLAN_NO_DESTINATION.to_string(),
        E::Ambiguous => crate::dialogue::WHICH_PLACE.to_string(),
        E::BadName(name) => format!("I can only put things inside KUE's own folder, so “{name}” isn't somewhere I can write."),
    }
}

/// The plan the owner replaced. Cancelled, with what replaced it recorded, so
/// a later "yes" to it is refused by the plan's own state.
fn cancel_plan_superseded(rt: &Runtime, plan_id: &str, by: &str) {
    let now = (rt.now)();
    if let Some(h) = rt.book.lock().unwrap().plan_mut(plan_id) {
        if h.goal_id.is_none() { h.plan.state = crate::plan::PlanState::Cancelled; }
    }
    rt.engine.lock().unwrap().record_action_event(
        format!("Plan {plan_id}: replaced by {by} before anything ran. It cannot be approved, and your yes to it would not start {by}."), now);
}

fn plan_says(rt: &Runtime, request_id: &str, said: &str) -> Received {
    say(rt, TurnKind::KueResponse, said, request_id);
    speak(rt, said, &[]);
    Received::Answered { said: said.to_string() }
}

/// Undo: everything the last move to the Trash moved, back where it came from.
/// None when there is nothing to put back.
pub fn propose_restore(rt: &Runtime, source: &str) -> Option<ActionRecord> {
    let items = rt.book.lock().unwrap().last_trashed().to_vec();
    (!items.is_empty()).then(|| propose_kind(rt, ActionKind::RestoreFromTrash { items }, source, None))
}

/// The window's "check my storage" control. A gesture, not a sentence: it
/// starts the one action it names and nothing else, so the window cannot
/// propose an action of its own choosing.
pub fn propose_inspection(rt: &Runtime, source: &str) -> ActionRecord {
    propose_kind(rt, ActionKind::InspectStorage, source, None)
}

pub fn list(rt: &Runtime) -> ActionList {
    let now = (rt.now)();
    let (killed, access) = { let e = rt.engine.lock().unwrap(); (e.is_killed(), e.access_block(now)) };
    let hidden = |code, why: String| ActionList { visible: false, withheld: Some(code), withheld_because: Some(why), records: Vec::new(), live: None, said: Vec::new(), tasks: Vec::new() };
    if killed { return hidden("KILLED", KILLED.into()); }
    if access.level < AuthLevel::Level2 {
        return hidden("AUTHORIZATION_REQUIRED", format!(
            "Actions are shown at LEVEL_2 — you, confirmed by the camera — or with Touch ID. Current: {} at {}. {}",
            tag(access.state), tag(access.level), access.detail));
    }
    let (records, tasks) = { let b = rt.book.lock().unwrap(); (b.records().to_vec(), b.task_views()) };
    if records.is_empty() { return ActionList { visible: true, withheld: None, withheld_because: None, records, live: None, said: Vec::new(), tasks }; }
    // A statement of its own: the firewall lock is released before the status is built.
    let cleared = rt.firewall.lock().unwrap().clear_actions_for_interface(records, now);
    match cleared {
        Some(c) => {
            let records = c.into_value();
            ActionList { visible: true, withheld: None, withheld_because: None,
                live: live_status(&records, now), said: lines(&records), records, tasks }
        }
        None => hidden("PRIVACY_DENIED", "The privacy firewall withheld the action list.".into()),
    }
}

// MARK: - The governed request pipeline
//
// One front door for every transport — spoken after the wake word, spoken after
// a button, typed, and later proactive or scheduled. `receive` decides what an
// utterance refers to (dialogue.rs), and then does exactly what the existing
// paths do: a confirmation calls `confirm`, which re-authorizes and asks for
// Touch ID; a new request calls `propose`. Nothing here is a shortcut past
// authorization or verification — it is the same governance, reached the same
// way from anywhere.

use crate::dialogue::{Facet, FileType, Interpretation, Open, Revision, TurnKind, WorkQuestion};
use crate::pipeline::{Decision as PipelineDecision, RequestState, Transport};

/// What happened to one utterance.
#[derive(Debug, Clone)]
pub enum Received {
    /// Something new. `record` is the action or goal step it planned; None
    /// means it is a question or an answer-by-rule, which the caller continues.
    New { request_id: String, record: Option<ActionRecord> },
    /// It confirmed what was open. The record went through re-authorization,
    /// Touch ID where required, execution and verification — like any confirm.
    Confirmed { request_id: String, record: Result<ActionRecord, String> },
    /// It cancelled what was waiting, or stopped what was running. `said` is
    /// KUE's reply — including when a change already underway cannot be stopped.
    Cancelled { request_id: String, said: String },
    /// It corrected the open plan. `said` is KUE's reply.
    Revised { request_id: String, said: String },
    /// It looked like a correction KUE could not place. `question` is asked.
    Clarify { request_id: String, question: String },
    /// A question about KUE's own work, answered from runtime state.
    Answered { said: String },
}

/// Said before a move that needs Touch ID, after how many files: "Moving one
/// file to the Trash. Touch ID is required."
pub const TOUCH_ID_REQUIRED: &str = "Touch ID is required.";
pub const STOPPED_BY_YOU: &str = "Stopped, as you asked. Nothing was changed.";
/// The reply to anything about the owner's work from someone KUE is not sure
/// of — the same words the conversation uses when it is withheld.
pub const NOT_SURE_ITS_YOU: &str = "I need to be sure it's you before we talk. Show your face, or use Touch ID.";
pub const LEFT_ALONE: &str = "You told me to leave some of these alone, so none of them were moved.";
pub const CHOOSE_IN_SHEET: &str = "That is more than I move at once. Choose which in the storage sheet.";
pub const COULD_NOT_VERIFY: &str = "I attempted that, but I couldn't verify the result.";

/// The one entry point for an utterance, from any transport.
pub fn receive(rt: &Runtime, transport: Transport, text: &str) -> Received {
    let now = (rt.now)();
    let source = if transport == Transport::Voice { "VOICE" } else { "TEXT" };
    // What is open, and what is running, are read BEFORE the utterance is
    // recorded: a cancellation closes them, and the dispatch needs to know.
    let (open, running, epoch, recognized, (interpretation, rid)) = {
        let mut e = rt.engine.lock().unwrap();
        let open = e.requests().dialogue.open().cloned();
        let running = e.requests().active().map(|r| (r.ids.request_id.clone(), r.ids.tool_execution_id.clone()));
        // The same bar as the conversation itself: the owner, confirmed by the
        // camera (LEVEL_2), and KUE not killed. UNKNOWN IDENTITY ≠ OWNER.
        let recognized = !e.is_killed() && e.access_block(now).level >= crate::authz::AuthLevel::Level2;
        (open, running, e.stop_epoch(), recognized, e.requests_mut().receive(transport, text, now))
    };
    // What KUE has been doing, and the owner's plan in progress, are the
    // owner's. Someone KUE is not sure of may stop or cancel — that is the safe
    // direction — but may not hear about the work, change the plan, say "do
    // it" to it, or undo it. Nothing changes; they are told why.
    let owners_only = match &interpretation {
        // What KUE keeps about the owner is the owner's: theirs to add to,
        // ask about, and forget. Nobody else's.
        Interpretation::Memory(_) => true,
        Interpretation::AboutWork { .. } | Interpretation::Correction { .. } | Interpretation::Clarify { .. }
        | Interpretation::PlanChange { .. } | Interpretation::UndoLast => true,
        Interpretation::Confirmation { .. } => matches!(open, Some(Open::Selection { .. } | Open::Plan { .. })),
        _ => false,
    };
    if owners_only && !recognized {
        // Why KUE will not: because it is stopped, or because it is not sure
        // who is there. Two different things, and it says which.
        let killed = rt.engine.lock().unwrap().is_killed();
        let said = if killed { crate::engine::MEMORY_KILLED } else { NOT_SURE_ITS_YOU };
        let target = if rid.is_empty() { None } else { Some(rid.as_str()) };
        rt.engine.lock().unwrap().requests_mut().say(TurnKind::KueResponse, said, target, now);
        if matches!(interpretation, Interpretation::UndoLast) {
            walk(rt, &rid, RequestState::Refused, Some("Not sure it's the owner."));
        }
        speak(rt, said, &[]);
        return Received::Answered { said: said.into() };
    }
    // What this utterance starts or confirms runs on behalf of `rid`, and a
    // "stop" said after this moment reaches it.
    let _serving = match interpretation {
        Interpretation::NewRequest | Interpretation::Confirmation { .. } | Interpretation::UndoLast => Some(serving(&rid, epoch)),
        _ => None,
    };

    match interpretation {
        Interpretation::NewRequest => {
            walk(rt, &rid, RequestState::Checking, None);
            let record = propose(rt, text, source);
            // No rule of KUE's acts on it, and it is a question KUE can answer
            // from what the owner has told it. Memory comes before a model:
            // what they said themselves is better than anything worked out,
            // and it costs nothing.
            if record.is_none() && recognized {
                if let Some(said) = from_memory(rt, text, now) {
                    say(rt, TurnKind::KueResponse, &said, &rid);
                    walk(rt, &rid, RequestState::Deciding, None);
                    let _ = rt.engine.lock().unwrap().requests_mut().decide(&rid, PipelineDecision::Answer, now);
                    walk(rt, &rid, RequestState::Completed, Some("Answered from what the owner has said."));
                    return Received::Answered { said };
                }
            }
            if let Some(rec) = &record {
                // Bound when it was created, and synced by every publish since;
                // binding again only fills in what an older path left out.
                bind_record(rt, &rid, rec);
                if let Some(t) = &rec.task { sync_goal(rt, &t.task_id); }
                // Stopped while it was being prepared: what it would have
                // waited for is withdrawn, not left for a later "yes".
                let stopped = rt.engine.lock().unwrap().requests().get(&rid).is_some_and(|r| r.state == RequestState::Cancelled);
                if stopped && rec.state.is_waiting() { cancel(rt, &rec.id); }
            }
            Received::New { request_id: rid, record }
        }

        Interpretation::Confirmation { .. } => match open {
            Some(Open::Confirmation { tool_execution_id, .. }) => {
                let record = confirm(rt, &tool_execution_id, None);
                Received::Confirmed { request_id: rid, record }
            }
            // "Do it" to a laid-out selection confirms exactly the set KUE
            // announced, and then asks macOS for Touch ID like any move.
            Some(Open::Selection { proposed, .. }) if !proposed.is_empty() => {
                let n = proposed.len();
                let rec = propose_trash(rt, proposed, source);
                bind_record(rt, &rid, &rec);
                if let Some(t) = &rec.task { sync_goal(rt, &t.task_id); }
                if rec.state.is_waiting() {
                    note(rt, TurnKind::KueResponse, &format!("Moving {} to the Trash. {TOUCH_ID_REQUIRED}", files(n)), &rid);
                    sync_request(rt, &rec);
                    let record = confirm(rt, &rec.id, None);
                    Received::Confirmed { request_id: rid, record }
                } else {
                    sync_request(rt, &rec);
                    Received::Confirmed { request_id: rid, record: Ok(rec) }
                }
            }
            Some(Open::Selection { .. }) => {
                say(rt, TurnKind::KueQuestion, CHOOSE_IN_SHEET, &rid);
                Received::Clarify { request_id: rid, question: CHOOSE_IN_SHEET.into() }
            }
            // "Do it" to a plan approves that plan, and only it. Every step
            // still asks for what it needs as it runs.
            Some(Open::Plan { plan_id, .. }) => {
                walk(rt, &rid, RequestState::Authorizing, Some("The owner approved the plan."));
                match approve_plan(rt, &plan_id) {
                    Ok(record) => {
                        rt.engine.lock().unwrap().requests_mut().dialogue.set_open(None);
                        if let Some(rec) = &record {
                            bind_record(rt, &rid, rec);
                            if let Some(t) = &rec.task { sync_goal(rt, &t.task_id); }
                            sync_request(rt, rec);
                        }
                        if let Some(goal_id) = rt.book.lock().unwrap().plan(&plan_id).and_then(|h| h.goal_id.clone()) {
                            rt.engine.lock().unwrap().requests_mut().bind(&rid, Some(&goal_id), Some(&plan_id), None, None);
                        }
                        match record {
                            Some(rec) => Received::Confirmed { request_id: rid, record: Ok(rec) },
                            None => Received::Confirmed { request_id: rid, record: Err(PLAN_NOTHING_STARTED.into()) },
                        }
                    }
                    Err(said) => {
                        rt.engine.lock().unwrap().requests_mut().dialogue.set_open(None);
                        walk(rt, &rid, RequestState::Refused, Some("The plan did not start."));
                        say(rt, TurnKind::KueResponse, &said, &rid);
                        speak(rt, &said, &[]);
                        Received::Answered { said }
                    }
                }
            }
            _ => Received::New { request_id: rid, record: None },
        },

        Interpretation::Cancellation { .. } => {
            let said: String = match open {
                Some(Open::Confirmation { tool_execution_id, .. }) => { cancel(rt, &tool_execution_id); "Cancelled. Nothing was changed.".into() }
                Some(Open::Selection { goal_id, .. }) => { stop_waiting_goal(rt, &goal_id, "You cancelled it."); "Cancelled. Nothing was moved.".into() }
                Some(Open::Question { .. }) => "Okay, never mind.".into(),
                Some(Open::Plan { plan_id, .. }) => { cancel_plan(rt, &plan_id); PLAN_CANCELLED.into() }
                // Nothing waiting on the owner: stop what is RUNNING — if it
                // can honestly be stopped.
                None => stop_running(rt, &rid, running.as_ref().and_then(|r| r.1.as_deref())),
            };
            say(rt, TurnKind::KueResponse, &said, &rid);
            Received::Cancelled { request_id: rid, said }
        }

        Interpretation::Correction { revision, .. } => {
            let Some(Open::Selection { goal_id, .. }) = open else {
                return Received::New { request_id: rid, record: None };
            };
            let said = revise(rt, &goal_id, revision);
            walk(rt, &rid, RequestState::Planning, Some("Revised by the owner."));
            say_storage(rt, TurnKind::KueResponse, &said, &rid);
            open_selection_if_waiting(rt, &rid);
            Received::Revised { request_id: rid, said }
        }

        // A change to the plan that is waiting. What it changes is worked out
        // against that plan, and whatever comes of it — a new plan, a
        // question, or a refusal — the old plan never runs.
        Interpretation::PlanChange { said, .. } => match open {
            Some(Open::Plan { plan_id, .. }) => change_plan(rt, &rid, &plan_id, &said),
            _ => Received::New { request_id: rid, record: None },
        },

        Interpretation::Clarify { question, .. } => {
            say(rt, TurnKind::KueQuestion, &question, &rid);
            Received::Clarify { request_id: rid, question }
        }

        Interpretation::Memory(ask) => {
            let said = memory_reply(rt, ask, now);
            let target = if rid.is_empty() { None } else { Some(rid.as_str()) };
            rt.engine.lock().unwrap().requests_mut().say(TurnKind::KueResponse, &said, target, now);
            speak(rt, &said, &[]);
            Received::Answered { said }
        }

        Interpretation::AboutWork { question } => {
            let (said, carries) = answer_about_work(rt, question, open.as_ref());
            let target = if rid.is_empty() { None } else { Some(rid.as_str()) };
            rt.engine.lock().unwrap().requests_mut().say(TurnKind::KueResponse, &said, target, now);
            speak(rt, &said, carries);
            Received::Answered { said }
        }

        Interpretation::UndoLast => {
            walk(rt, &rid, RequestState::Checking, None);
            match propose_restore(rt, source) {
                Some(rec) => {
                    bind_record(rt, &rid, &rec);
                    let n = match &rec.action { ActionKind::RestoreFromTrash { items } => items.len(), _ => 0 };
                    if rec.state.is_waiting() {
                        say_storage(rt, TurnKind::KueQuestion, &format!("I can put back the {} I moved. Say “yes” to confirm.", files(n)), &rid);
                    }
                    sync_request(rt, &rec);
                    Received::New { request_id: rid, record: Some(rec) }
                }
                None => {
                    let said = "There's nothing I can undo: I haven't moved anything I could put back.";
                    say(rt, TurnKind::KueResponse, said, &rid);
                    walk(rt, &rid, RequestState::Completed, None);
                    Received::Answered { said: said.into() }
                }
            }
        }
    }
}

/// "Stop" while something runs, with nothing waiting on the owner. KUE stops
/// what can honestly be stopped, and says plainly what it cannot: a change
/// already underway is not reported as stopped.
fn stop_running(rt: &Runtime, rid: &str, tool_execution_id: Option<&str>) -> String {
    let now = (rt.now)();
    let state = rt.engine.lock().unwrap().requests().get(rid).map(|r| r.state);
    let record = tool_execution_id.and_then(|id| rt.book.lock().unwrap().records.iter().find(|r| r.id == id).cloned());
    let tool = record.as_ref().and_then(|r| crate::tools::declared(r.action.tag()));
    let changes_nothing = tool.is_some_and(|t| t.rollback == crate::tools::Rollback::NothingChanged);
    match state {
        // The change is being made, or has been and is being checked. Stopping
        // it partway is not something KUE can promise.
        Some(RequestState::Acting) if !changes_nothing =>
            return format!("I can't stop that partway — {} is already underway. I'll tell you exactly how it ends.",
                           tool.map(|t| t.name.to_lowercase()).unwrap_or_else(|| "the change".into())),
        Some(RequestState::Verifying) =>
            return "It has already run; I'm checking the result. I'll tell you what I find.".into(),
        Some(RequestState::Authorizing) =>
            return "macOS is asking for Touch ID now. Cancel that prompt and nothing will change.".into(),
        _ => {}
    }
    // Stoppable: before anything acted, or a read that changes nothing. The
    // running walk hears it within a fraction of a second; the goal ends; the
    // request is stopped.
    rt.engine.lock().unwrap().request_stop();
    let goal = rt.engine.lock().unwrap().requests().get(rid).and_then(|r| r.ids.goal_id.clone());
    let changed = goal.as_deref().is_some_and(|g| goal_changed_something(rt, g));
    if let Some(g) = &goal {
        let ended = {
            let mut book = rt.book.lock().unwrap();
            match book.goal_mut(g) {
                Some(goal) if goal.is_open() => { goal.cancel("You stopped it.", now); goal.outcome_recorded = true; Some(goal.kind.tag()) }
                _ => None,
            }
        };
        if let Some(kind) = ended {
            rt.engine.lock().unwrap().record_action_event(format!("Goal {kind}: cancelled — you stopped it."), now);
            (rt.changed)();
        }
    }
    walk(rt, rid, RequestState::Cancelled, Some("You stopped it."));
    if changed { "Stopped. Nothing further will run; what already finished stays as it is.".into() }
    else { "Stopped. Nothing was changed.".into() }
}

/// Whether any step of this goal verified a change on the Mac.
fn goal_changed_something(rt: &Runtime, goal_id: &str) -> bool {
    rt.book.lock().unwrap().records.iter()
        .filter(|r| r.task.as_ref().is_some_and(|t| t.task_id == goal_id))
        .filter(|r| matches!(r.state, ActionState::Succeeded | ActionState::PartiallySucceeded | ActionState::UnknownResult))
        .any(|r| crate::tools::declared(r.action.tag()).is_none_or(|t| t.rollback != crate::tools::Rollback::NothingChanged))
}

thread_local! {
    /// The request the current call chain works for, and the stop count when
    /// its utterance arrived. Set by `receive` for as long as it runs: one
    /// utterance, one thread, start to finish.
    static SERVING: std::cell::RefCell<Option<(String, u64)>> = const { std::cell::RefCell::new(None) };
}

/// Restores what was being served before, so a nested utterance on the same
/// thread never leaves the outer one unserved.
struct Serving(Option<(String, u64)>);
impl Drop for Serving {
    fn drop(&mut self) { let before = self.0.take(); SERVING.with(|s| *s.borrow_mut() = before); }
}

fn serving(rid: &str, epoch: u64) -> Serving {
    Serving(SERVING.with(|s| s.borrow_mut().replace((rid.to_string(), epoch))))
}

fn served() -> Option<(String, u64)> { SERVING.with(|s| s.borrow().clone()) }

use crate::voice::narration::files;
use crate::voice::reference::spoken_count;

/// The request a question belongs to has been decided: KUE will answer it —
/// by rule, or with the model. Recorded so "why did it answer instead of
/// acting?" has an answer.
/// The governed decision for a request that needs steps: KUE will PLAN it.
/// Recorded against the request the input created, so the runtime — not the
/// window — knows a plan is being worked out, and the window can say so.
pub fn planning(engine: &Mutex<Engine>, now: f64) -> Option<String> {
    let mut e = engine.lock().unwrap();
    let p = e.requests_mut();
    let rid = p.awaiting_decision()?;
    let _ = p.walk_to(&rid, RequestState::Deciding, now, None);
    let _ = p.decide(&rid, PipelineDecision::Plan, now);
    let _ = p.walk_to(&rid, RequestState::Planning, now, Some("Working out a plan."));
    Some(rid)
}

/// KUE's own sentence in the conversation, from the shell. The turn is
/// recorded against `rid`; nothing is spoken here.
pub fn note_turn(engine: &Mutex<Engine>, said: &str, rid: &str, now: f64) {
    engine.lock().unwrap().requests_mut().say(TurnKind::KueResponse, said, Some(rid), now);
}

/// What KUE says while a model is working out a plan. Said before the wait, so
/// the window is never blank while nothing visible happens.
pub const WORKING_OUT_A_PLAN: &str = "Working out a plan…";
/// The model did not produce one. Said as what happened, not as a refusal.
pub const NO_PLAN_PRODUCED: &str = "No plan was produced.";

pub fn answering(engine: &Mutex<Engine>, now: f64) -> Option<String> {
    let mut e = engine.lock().unwrap();
    let p = e.requests_mut();
    let rid = p.awaiting_decision()?;
    let _ = p.walk_to(&rid, RequestState::Deciding, now, None);
    let _ = p.decide(&rid, PipelineDecision::Answer, now);
    Some(rid)
}

/// The answer arrived, or failed. KUE's own reply is recorded as its turn.
/// Nothing here claims more than the answer did: a failed answer is FAILED.
pub fn answered(engine: &Mutex<Engine>, ok: bool, said: &str, now: f64) {
    let mut e = engine.lock().unwrap();
    let p = e.requests_mut();
    let Some(rid) = p.answering() else { return };
    p.say(TurnKind::KueResponse, said, Some(&rid), now);
    let _ = p.walk_to(&rid, if ok { RequestState::Completed } else { RequestState::Failed }, now, None);
}

fn walk(rt: &Runtime, rid: &str, to: RequestState, why: Option<&str>) {
    let _ = rt.engine.lock().unwrap().requests_mut().walk_to(rid, to, (rt.now)(), why);
}

/// KUE's own reply in the conversation: recorded as its turn, and spoken
/// through the same speech gate and firewall as every sentence, declaring what
/// it carries. Outcomes of actions are NOT said here — the narrator speaks
/// those from the verified record, so nothing is said twice and nothing is
/// said as done before it verified.
fn say(rt: &Runtime, kind: TurnKind, said: &str, rid: &str) {
    note(rt, kind, said, rid);
    speak(rt, said, &[]);
}

/// The same, for a sentence about the storage plan: it names kinds and counts
/// of the owner's files (never a name), and declares so.
fn say_storage(rt: &Runtime, kind: TurnKind, said: &str, rid: &str) {
    note(rt, kind, said, rid);
    speak(rt, said, STORAGE);
}

const STORAGE: &[DataKind] = &[DataKind::StorageSummary];

/// Recorded as KUE's turn, not spoken — the narrator already says it.
fn note(rt: &Runtime, kind: TurnKind, said: &str, rid: &str) {
    rt.engine.lock().unwrap().requests_mut().say(kind, said, Some(rid), (rt.now)());
}

/// Speaks a conversational sentence, declaring what it carries so the
/// firewall decides again. A sentence that carries nothing is not silenced by
/// a refusal about something else. Called with no core lock held.
fn speak(rt: &Runtime, said: &str, carries: &[DataKind]) {
    (rt.say)(said, carries);
}

fn bind_record(rt: &Runtime, rid: &str, rec: &ActionRecord) {
    let goal = rec.task.as_ref().map(|t| t.task_id.clone());
    let mut e = rt.engine.lock().unwrap();
    let p = e.requests_mut();
    p.bind(rid, goal.as_deref(), goal.as_deref(), Some(&rec.id), None);
}

/// Moves the request a record belongs to so it matches the record. Called
/// from `publish` and `cancel`, the two places every record change passes —
/// so no transport, button or goal step can move an action without the runtime
/// knowing. A record that belongs to no request (an older path) is left alone.
fn sync_request(rt: &Runtime, rec: &ActionRecord) {
    use ActionState as A;
    let now = (rt.now)();
    let mut e = rt.engine.lock().unwrap();
    let p = e.requests_mut();
    let Some(rid) = p.by_tool_execution(&rec.id) else { return };
    // A request the owner stopped stays stopped: work that finished anyway
    // (a read that completed as "stop" arrived) changes nothing about that.
    if p.get(&rid).is_some_and(|r| r.state.is_terminal()) { return; }
    let step_of_goal = rec.task.is_some();
    // An earlier step's success never pulls a goal back from waiting on the
    // owner: the goal has moved on, and it decides.
    if step_of_goal && rec.state == ActionState::Succeeded
        && p.get(&rid).is_some_and(|r| r.state == RequestState::WaitingForUser) { return; }
    let (target, turn) = match rec.state {
        // One step of a goal verified: the goal goes on, and `sync_goal`
        // decides whether the request is finished, waiting, or stopped.
        A::Succeeded if step_of_goal => (RequestState::Planning, Some(TurnKind::GoalProgress)),
        A::Proposed | A::Authorized | A::Reauthorizing => (RequestState::Authorizing, None),
        A::RequiresConfirmation | A::RequiresStrongAuth => (RequestState::WaitingForUser, None),
        A::Executing => (RequestState::Acting, Some(TurnKind::ToolProgress)),
        A::Succeeded => (RequestState::Completed, Some(TurnKind::VerificationResult)),
        // Ran, and KUE cannot say it worked: never "done".
        A::PartiallySucceeded | A::UnknownResult => (RequestState::Uncertain, Some(TurnKind::ToolResult)),
        A::Failed => (RequestState::Failed, Some(TurnKind::ToolResult)),
        A::Denied | A::PrivacyDenied | A::AuthorizationExpired => (RequestState::Refused, Some(TurnKind::ToolResult)),
        // Nothing to act on: a precondition failed, nothing ran.
        A::NoMatches => (RequestState::Blocked, Some(TurnKind::ToolResult)),
        A::Cancelled => (RequestState::Cancelled, None),
    };
    // The decision this record represents, recorded once.
    if p.get(&rid).is_some_and(|r| r.decision.is_none() && matches!(r.state, RequestState::Checking | RequestState::Deciding)) {
        let _ = p.walk_to(&rid, RequestState::Deciding, now, None);
        let _ = p.decide(&rid, PipelineDecision::Act, now);
    }
    if rec.state == ActionState::Succeeded {
        p.bind(&rid, None, None, None, Some(&format!("v-{}", rec.id)));
        // A goal step passes through VERIFYING on its way back to planning.
        if step_of_goal { let _ = p.walk_to(&rid, RequestState::Verifying, now, None); }
    }
    let _ = p.walk_to(&rid, target, now, rec.reason.as_deref());
    if target == RequestState::WaitingForUser {
        p.dialogue.set_open(Some(Open::Confirmation { request_id: rid.clone(), tool_execution_id: rec.id.clone() }));
    }
    if let Some(kind) = turn {
        // Only KUE's own sentence about the outcome. Never the target or the
        // executor's raw output, and never "done" for what did not verify.
        let reason = rec.reason.clone().unwrap_or_default();
        let said = match rec.state {
            A::Executing => format!("{}…", crate::tools::declared(rec.action.tag()).map(|t| t.name).unwrap_or("Working")),
            A::PartiallySucceeded | A::UnknownResult => format!("{COULD_NOT_VERIFY} {reason}").trim().to_string(),
            A::Denied | A::PrivacyDenied | A::AuthorizationExpired | A::NoMatches if !reason.is_empty() =>
                format!("I can't do that: {}", reason.trim_end_matches('.')) + ".",
            _ => rec.sentences.as_ref().map(|s| s.on_screen.clone()).unwrap_or_else(|| rec.event_summary()),
        };
        p.say(kind, &said, Some(&rid), now);
    }
}

/// Moves the request a goal belongs to so it matches the goal, once the goal
/// has been driven: finished, stopped, cancelled, or waiting for the owner.
fn sync_goal(rt: &Runtime, goal_id: &str) {
    let Some(rid) = rt.engine.lock().unwrap().requests().by_goal(goal_id) else { return };
    let Some(state) = rt.book.lock().unwrap().goal(goal_id).map(|g| g.state()) else { return };
    match state {
        GoalState::Completed => {
            // Said only because every step verified: the goal cannot be
            // COMPLETED otherwise. The sentence is the goal's own report,
            // computed from what was read back, never from what was intended.
            let outcome = rt.book.lock().unwrap().goal(goal_id)
                .and_then(|g| g.steps.iter().rev().find_map(|s| s.said.clone()));
            if let Some(o) = outcome {
                // The step already reported this sentence as progress; saying it
                // twice is noise. The report is promoted to the verified result.
                let done = format!("Done. {o}");
                let promoted = rt.engine.lock().unwrap().requests_mut().dialogue
                    .promote_last(&o, TurnKind::VerificationResult, &done);
                if !promoted { note(rt, TurnKind::VerificationResult, &done, &rid); }
            }
            walk(rt, &rid, RequestState::Completed, Some("Every step verified."));
            keep_what_was_done(rt, goal_id, (rt.now)());
        }
        GoalState::Stopped => walk(rt, &rid, RequestState::Failed, Some("A step did not verify, so the rest did not run.")),
        GoalState::Cancelled => walk(rt, &rid, RequestState::Cancelled, None),
        GoalState::WaitingForUser => open_selection_if_waiting(rt, &rid),
        _ => {}
    }
}

/// A goal that finished, kept: what KUE did, because it did it and read the
/// world back. Named by the TOOLS it used and how many steps — never by the
/// files it touched, which policy allows KUE to show the owner and not to
/// store. Nothing is inferred from it: "you wanted a folder there" is not
/// something this knows, and it does not pretend to.
fn keep_what_was_done(rt: &Runtime, goal_id: &str, now: f64) {
    let (kind, tools, proof) = {
        let book = rt.book.lock().unwrap();
        let Some(g) = book.goal(goal_id) else { return };
        let tools: Vec<&'static str> = g.steps.iter().filter_map(|s| crate::plan::tool_name(&s.kind)).collect();
        // The proof the action pipeline produced. Without one there is no
        // verified memory, and KUE keeps nothing.
        let proof = book.records.iter()
            .filter(|r| r.task.as_ref().is_some_and(|t| t.task_id == goal_id) && r.state == ActionState::Succeeded)
            .filter_map(|r| r.verification.as_deref()).last()
            .and_then(crate::facts::Verification::of);
        (g.kind.tag(), tools, proof)
    };
    let (Some(proof), false) = (proof, tools.is_empty()) else { return };
    let mut e = rt.engine.lock().unwrap();
    if e.memory_refusal().is_some() { return; }
    let id = e.memory_mut().id(now);
    let what = format!("KUE carried out a {} plan of {} step{}: {}",
                       kind.to_lowercase().replace('_', " "), tools.len(),
                       if tools.len() == 1 { "" } else { "s" },
                       tools.iter().map(|t| t.to_lowercase().replace('_', " ")).collect::<Vec<_>>().join(", "));
    let m = crate::memory::Memory::verified(&id, &format!("work:{goal_id}"), &what,
                                            crate::privacy::DataKind::EventRecord, now, kind, proof)
        .about(goal_id);
    let _ = e.memory_mut().remember(m, false, now);
    e.record_action_event(format!("Memory {id}: kept as WORK — a plan KUE carried out and checked."), now);
}

/// What a standing preference took out of this plan, and why — as one
/// sentence, or nothing when memory changed nothing. The count comes from the
/// report; the reason is the owner's own words, never KUE's paraphrase.
fn memory_left_out(rt: &Runtime, goal_id: &str, refs: &[String], now: f64) -> String {
    if refs.is_empty() { return String::new(); }
    // The goal's exclusions and how many of each the report found, read once
    // and copied, so the book is not held while the memory book is read.
    let (excluded, counts): (Vec<crate::dialogue::Facet>, Vec<(crate::dialogue::Facet, usize)>) = {
        let book = rt.book.lock().unwrap();
        let (Some(goal), Some(report)) = (book.goal(goal_id), book.last_storage()) else { return String::new() };
        let excluded = goal.excluded.clone();
        let counts = excluded.iter()
            .map(|f| (*f, report.candidates.iter().filter(|c| matches(*f, c)).count()))
            .collect();
        (excluded, counts)
    };
    let e = rt.engine.lock().unwrap();
    let mut said: Vec<String> = Vec::new();
    for id in refs {
        let Some(m) = e.memory().get(id).filter(|m| m.is_current(now)) else { continue };
        let Some(wish) = crate::dialogue::storage_wish(&m.statement) else { continue };
        if !excluded.contains(&wish.facet) { continue; }
        let n = counts.iter().find(|(f, _)| *f == wish.facet).map(|(_, n)| *n).unwrap_or(0);
        if n == 0 { continue; }
        let what = if n == 1 { format!("the {}", singular_of(wish.facet)) } else { format!("the {n} {}", plural_of(wish.facet)) };
        said.push(format!(" I left out {what}, because you told me: “{}”.", m.statement.trim_end_matches('.')));
    }
    said.join("")
}

fn singular_of(f: crate::dialogue::Facet) -> String {
    match f { crate::dialogue::Facet::Storage(c) => singular(c).to_string(), other => format!("{:?}", other).to_lowercase() }
}

fn plural_of(f: crate::dialogue::Facet) -> String {
    match f { crate::dialogue::Facet::Storage(c) => c.heading().to_lowercase(), other => format!("{:?}", other).to_lowercase() }
}

pub const WISHES_DISAGREE: &str = "I have two things you've told me that disagree about that, so I've left them both out of it. Tell me which one holds and I'll keep that.";

/// What the owner has told KUE that bears on THIS clean-up, applied before
/// anything is offered to them.
///
/// Memory is context, never authority: this changes what KUE PROPOSES, and
/// nothing else. Every step still authorizes, the owner still approves, and
/// the move is still verified. Only a CONFIRMED preference of theirs counts —
/// a candidate has been agreed to by nobody, and a note about a different task
/// belongs to that task.
///
/// When two saved preferences disagree about the same kind of file, KUE
/// applies neither and says so: choosing one silently is the thing not to do.
fn apply_standing_wishes(rt: &Runtime, goal_id: &str, said: &str, now: f64) {
    use crate::memory::{MemoryClass, MemoryState};
    let wishes: Vec<(crate::dialogue::StorageWish, String, String)> = {
        let e = rt.engine.lock().unwrap();
        e.memory().matching(said, now).into_iter()
            .filter(|m| m.class == MemoryClass::Preference && m.state_at(now) == MemoryState::Confirmed)
            .filter_map(|m| crate::dialogue::storage_wish(&m.statement)
                .map(|w| (w, m.id.clone(), m.statement.clone())))
            .collect()
    };
    if wishes.is_empty() { return; }

    // Two preferences about the same kind of file that pull opposite ways.
    let mut disagree: Vec<String> = Vec::new();
    for (w, _, statement) in &wishes {
        if wishes.iter().any(|(o, _, s)| o.facet == w.facet && o.include != w.include && s != statement) {
            disagree.push(statement.clone());
        }
    }
    let rid = served().map(|(r, _)| r).unwrap_or_default();
    if !disagree.is_empty() {
        disagree.sort();
        disagree.dedup();
        let question = format!("{WISHES_DISAGREE} You've told me: {}.",
                               disagree.iter().map(|s| format!("“{}”", s.trim_end_matches('.'))).collect::<Vec<_>>().join(", and "));
        if !rid.is_empty() { say(rt, TurnKind::KueQuestion, &question, &rid); }
        rt.engine.lock().unwrap().record_action_event(
            format!("Goal {goal_id}: two saved preferences disagree, so neither was applied."), now);
        return;
    }

    let leave_out: Vec<(crate::dialogue::Facet, String, String)> = wishes.into_iter()
        .filter(|(w, _, _)| !w.include)
        .map(|(w, id, statement)| (w.facet, id, statement))
        .collect();
    if leave_out.is_empty() { return; }
    {
        let mut book = rt.book.lock().unwrap();
        if let Some(g) = book.goal_mut(goal_id) {
            for (facet, id, _) in &leave_out {
                if !g.excluded.contains(facet) { g.excluded.push(*facet); }
                if !g.memory_refs.contains(id) { g.memory_refs.push(id.clone()); }
            }
        }
    }
    let mut e = rt.engine.lock().unwrap();
    for (_, id, _) in &leave_out { e.memory_mut().mention(id); }
    e.record_action_event(format!("Goal {goal_id}: {} standing preference(s) of yours applied before anything was offered.",
                                  leave_out.len()), now);
}

/// Whether a storage candidate matches something the owner named.
fn matches(f: Facet, c: &storage::Candidate) -> bool {
    match f {
        Facet::Storage(cat) => c.category == cat,
        Facet::FileType(t) => FileType::of(&c.name) == Some(t),
    }
}

/// Whether the owner's corrections to this goal leave a candidate in the plan.
fn keeps(goal: &Goal, c: &storage::Candidate) -> bool {
    goal.only.is_none_or(|o| matches(o, c)) && !goal.excluded.iter().any(|f| matches(*f, c))
}

/// The plan as it stands: what KUE proposes to move, counted by kind in words.
fn current_selection(book: &ActionBook, goal_id: &str) -> Option<(Vec<String>, Vec<String>, Vec<Facet>)> {
    let goal = book.goal(goal_id)?;
    let report = book.last_storage()?;
    let kept: Vec<&storage::Candidate> = report.candidates.iter().filter(|c| keeps(goal, c)).collect();
    let mut counts: Vec<(storage::Category, usize)> = Vec::new();
    for c in &kept {
        match counts.iter_mut().find(|(k, _)| *k == c.category) {
            Some((_, n)) => *n += 1,
            None => counts.push((c.category, 1)),
        }
    }
    // One style of number per sentence: words when every count is small,
    // digits for all of them when any is not ("22 installers, 11 duplicates",
    // never "22 installers and eleven duplicates").
    let words = counts.iter().all(|(_, n)| *n <= 12);
    let parts = counts.iter().map(|(c, n)| {
        let count = if words { if *n == 1 { "one".to_string() } else { spoken_count(*n) } } else { n.to_string() };
        format!("{count} {}", if *n == 1 { singular(*c).to_string() } else { c.heading().to_lowercase() })
    }).collect();
    // Everything the owner may name: every kind and file type the report
    // found — including what they left out, so "only the installers" can
    // bring one back.
    let mut facets: Vec<Facet> = Vec::new();
    for c in &report.candidates {
        for f in [Some(Facet::Storage(c.category)), FileType::of(&c.name).map(Facet::FileType)].into_iter().flatten() {
            if !facets.contains(&f) { facets.push(f); }
        }
    }
    Some((kept.iter().map(|c| c.path.clone()).collect(), parts, facets))
}

/// When a storage-cleanup goal stops to let the owner choose, the conversation
/// opens a selection: exactly the set KUE proposes, and what the owner can name
/// to change it. The first time, KUE says what it found; after a correction,
/// only what is left.
fn open_selection_if_waiting(rt: &Runtime, rid: &str) {
    let now = (rt.now)();
    let (goal_id, already_open) = {
        let e = rt.engine.lock().unwrap();
        let Some(goal_id) = e.requests().get(rid).and_then(|r| r.ids.goal_id.clone()) else { return };
        let open = matches!(e.requests().dialogue.open(), Some(Open::Selection { goal_id: g, .. }) if *g == goal_id);
        (goal_id, open)
    };
    // Laid out once. The goal is synced from more than one place; the owner
    // hears the plan once, and again only after it changes.
    if already_open { return; }
    let (proposed, parts, facets, revised, narrowed, refs) = {
        let book = rt.book.lock().unwrap();
        let Some(goal) = book.goal(&goal_id) else { return };
        let waiting = goal.kind == GoalKind::CleanUpStorage
            && goal.waiting_for_user().is_some_and(|s| s.kind == StepKind::WaitForApproval);
        if !waiting { return; }
        let revised = !goal.revisions.is_empty();
        let narrowed = !goal.excluded.is_empty() || goal.only.is_some();
        let refs = goal.memory_refs.clone();
        let Some((proposed, parts, facets)) = current_selection(&book, &goal_id) else { return };
        (proposed, parts, facets, revised, narrowed, refs)
    };
    // What a standing preference of theirs took out before they saw anything.
    // Said in the same breath as the plan, with the reason, because a plan
    // changed for a reason the owner cannot see is a plan they cannot check.
    let because = memory_left_out(rt, &goal_id, &refs, now);

    if proposed.is_empty() {
        say_storage(rt, TurnKind::KueResponse, if revised { "With those left out, nothing is left to move." }
            else { "I found nothing I would suggest moving." }, rid);
        walk(rt, rid, RequestState::Completed, Some("Nothing to move."));
        return;
    }
    let n = proposed.len();
    let what = reference_list(&parts);
    // The report lists at most its largest findings; everything KUE offers to
    // move comes from that list. When there were more, it says so, rather than
    // counting the list as if it were everything (the storage line above it
    // counts all of them).
    let listed = rt.book.lock().unwrap().last_storage()
        .and_then(|r| (r.found > r.candidates.len()).then_some((r.candidates.len(), r.found)));
    let lead = if revised && narrowed {
        format!("That leaves {what}.")
    } else if revised {
        format!("That's all {}: {what}.", files(n))
    } else if let Some((shown, found)) = listed {
        format!("I found {found} files worth reviewing. The {shown} largest are in the storage sheet: {what}.")
    } else {
        format!("I found {} worth reviewing: {what}.", files(n))
    };
    let said = if n > actions::MAX_TRASHED {
        format!("{lead}{because} {CHOOSE_IN_SHEET}")
    } else if revised {
        format!("{lead}{because}")
    } else {
        format!("{lead}{because} I can move {} to the Trash — say “do it”, or tell me what to leave out.",
                if n == 1 { "it" } else { "them" })
    };
    let proposed = if n > actions::MAX_TRASHED { Vec::new() } else { proposed };
    {
        let mut e = rt.engine.lock().unwrap();
        let p = e.requests_mut();
        p.say(TurnKind::KueSuggestion, &said, Some(rid), now);
        let _ = p.walk_to(rid, RequestState::Planning, now, None);
        let _ = p.wait_for(Open::Selection { request_id: rid.to_string(), goal_id, facets, proposed }, now);
    }
    speak(rt, &said, STORAGE);
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn reference_list(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn facet_said(book: &ActionBook, f: Facet) -> String {
    let n = book.last_storage().map(|r| r.candidates.iter().filter(|c| matches(f, c)).count()).unwrap_or(0);
    match f {
        Facet::Storage(cat) => if n == 1 { format!("the {}", singular(cat)) } else { format!("the {} {}", spoken_count(n), cat.heading().to_lowercase()) },
        Facet::FileType(t) => format!("the {}", t.said(n)),
    }
}

/// Applies the owner's correction to the goal, and says what changed. Each
/// correction is kept, so "go back" undoes exactly the last one.
/// How long a note about one task is worth anything. Long enough to finish
/// what is being done, short enough that it never becomes a standing rule
/// nobody agreed to.
const TASK_NOTE_SECONDS: f64 = 4.0 * 3600.0;

/// A correction the owner made to the work in hand, kept as a note about THIS
/// task and nothing more. "Don't include the installers" is an instruction for
/// this cleanup; "I never want installers included" is a preference, and only
/// the owner saying so in those terms makes one (`memory_reply`).
fn keep_task_note(rt: &Runtime, goal_id: &str, what: &str, now: f64) {
    let mut e = rt.engine.lock().unwrap();
    if e.memory_refusal().is_some() { return; }
    let id = e.memory_mut().id(now);
    let statement = format!("For this clean-up, you asked me to leave out {what}");
    let m = crate::memory::Memory::told(&id, crate::memory::MemoryClass::TaskNote,
        &crate::memory::subject_of(crate::memory::MemoryClass::TaskNote, &statement), &statement,
        crate::privacy::DataKind::OwnerMessage, now, what)
        .valid_for(TASK_NOTE_SECONDS)
        .about(goal_id);
    let _ = e.memory_mut().remember(m, false, now);
}

fn revise(rt: &Runtime, goal_id: &str, revision: Revision) -> String {
    let mut book = rt.book.lock().unwrap();
    let before = book.goal(goal_id).map(|g| (g.excluded.clone(), g.only));
    let Some(before) = before else { return "I've lost track of that plan.".into() };
    // What the owner asked to be left out of THIS task, for the note below.
    let mut excluded_said: Option<String> = None;
    let said = match &revision {
        Revision::Exclude(facets) => {
            let named: Vec<String> = facets.iter().map(|f| facet_said(&book, *f)).collect();
            excluded_said = Some(named.join(" and "));
            let total_after = {
                let mut all = before.0.clone();
                for f in facets { if !all.contains(f) { all.push(*f); } }
                all.len()
            };
            if let Some(g) = book.goal_mut(goal_id) {
                for f in facets { if !g.excluded.contains(f) { g.excluded.push(*f); } }
            }
            if facets.len() == 1 && total_after == 2 {
                "Okay. I'll exclude both.".to_string()
            } else if facets.len() == 1 && total_after > 2 {
                format!("Okay. I'll exclude {} as well.", named[0])
            } else {
                format!("Okay. I'll exclude {}.", reference_list(&named))
            }
        }
        Revision::OnlyInclude(f) => {
            let named = facet_said(&book, *f);
            if let Some(g) = book.goal_mut(goal_id) { g.only = Some(*f); g.excluded.clear(); }
            format!("Okay — only {named}.")
        }
        Revision::IncludeAll => {
            if let Some(g) = book.goal_mut(goal_id) { g.excluded.clear(); g.only = None; }
            "Okay — all of them again.".to_string()
        }
        // No change, so nothing to go back to: the plan is said as it is.
        Revision::LeaveNothingOut => return if before.0.is_empty() && before.1.is_none() {
            "Okay — nothing left out.".into()
        } else {
            "Okay — nothing else left out.".into()
        },
        Revision::Undo => {
            let last = book.goal_mut(goal_id).and_then(|g| g.revisions.pop());
            return match last {
                Some((ex, only)) => {
                    if let Some(g) = book.goal_mut(goal_id) { g.excluded = ex; g.only = only; }
                    "Okay — I've undone that change.".into()
                }
                None => "There's no change to go back to.".into(),
            };
        }
    };
    if let Some(g) = book.goal_mut(goal_id) { g.revisions.push(before); }
    drop(book);
    // Kept as a note about this task, which expires — never as a standing
    // preference, which only the owner's own words can make.
    if let Some(what) = excluded_said { keep_task_note(rt, goal_id, &what, (rt.now)()); }
    said
}

fn singular(cat: storage::Category) -> &'static str {
    match cat {
        storage::Category::Installer => "installer",
        storage::Category::DuplicateCopy => "probable duplicate",
        storage::Category::OldDownload => "old download",
        storage::Category::LargeFile => "large file",
    }
}

// MARK: - What KUE keeps
//
// The owner's memory, in the conversation: keep this, what do you keep, forget
// that, why. Every sentence here is composed from what KUE holds; no model is
// asked, and nothing is written that the engine's own rules refuse.

pub const NOTHING_ABOUT_THAT: &str = "I don't have anything about that in my memory.";
pub const NOTHING_TO_FORGET: &str = "I don't have anything about that to forget.";
pub const NOTHING_KEPT_YET: &str = "I'm not keeping anything about you yet. Tell me to remember something and I will.";
pub const WHICH_MEMORY: &str = "I'm not sure which one you mean. Ask me what I remember about something, and I'll tell you where it came from.";

/// The owner's words, said back about them: "I prefer PDFs" → "You prefer
/// PDFs". Mechanical and narrow; a word it cannot turn around is kept exactly
/// as they said it.
fn about_the_owner(said: &str) -> String {
    let swap = |bare: &str| match bare {
        "i" | "me" => Some("you"), "my" => Some("your"), "mine" => Some("yours"),
        "myself" => Some("yourself"), "i'm" | "im" => Some("you're"), "i've" | "ive" => Some("you've"),
        "i'd" => Some("you'd"), "i'll" => Some("you'll"), "am" => Some("are"),
        _ => None,
    };
    let out: Vec<String> = said.split_whitespace().map(|w| {
        let bare = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase();
        match swap(&bare) {
            Some(s) => w.to_lowercase().replacen(&bare, s, 1),
            None => w.to_string(),
        }
    }).collect();
    let joined = out.join(" ");
    let mut c = joined.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(joined)
}

/// Which kind of thing the owner just asked KUE to keep, by the words they
/// used. Deliberately narrow: what KUE cannot place is a FACT about their
/// world, the class that claims the least about how they want things done.
fn class_of(statement: &str) -> crate::memory::MemoryClass {
    use crate::memory::MemoryClass as C;
    let t = format!(" {} ", statement.to_lowercase());
    let has = |words: &[&str]| words.iter().any(|w| t.contains(&format!(" {w} ")));
    // A sentence KUE can read as a standing wish about cleaning up is a
    // preference, whatever words it used to say it.
    if crate::dialogue::storage_wish(statement).is_some() && crate::dialogue::is_standing(statement) { return C::Preference; }
    if has(&["prefer", "prefers", "rather", "always", "never", "instead"])
        || t.contains(" i like ") || t.contains("don't want") || t.contains("dont want")
        || t.contains(" i want you to ") { return C::Preference; }
    if has(&["working", "preparing", "applying", "building", "goal", "deadline", "aiming"]) { return C::Goal; }
    C::Fact
}

/// KUE's reply to anything about what it keeps. Composed here, from the
/// memory book; a model is never asked what KUE remembers.
fn memory_reply(rt: &Runtime, ask: crate::dialogue::MemoryAsk, now: f64) -> String {
    use crate::dialogue::MemoryAsk;
    use crate::memory::{subject_of, Memory, MemoryClass, Remembered};
    let mut e = rt.engine.lock().unwrap();
    match ask {
        MemoryAsk::Keep { statement, replacing } => {
            if let Some(no) = e.memory_refusal() { return no.to_string(); }
            let said = about_the_owner(&statement);
            let class = class_of(&statement);
            let id = e.memory_mut().id(now);
            // The owner's own words: OWNER_MESSAGE, which policy allows to
            // stay on this Mac and nowhere else.
            let m = Memory::told(&id, class, &subject_of(class, &said), &said,
                                 crate::privacy::DataKind::OwnerMessage, now, &statement);
            let outcome = e.memory_mut().remember(m, replacing, now);
            match &outcome {
                Remembered::Kept(id) | Remembered::Replaced { id, .. } => {
                    let id = id.clone();
                    e.memory_mut().mention(&id);
                    let replaced = matches!(outcome, Remembered::Replaced { .. });
                    e.record_action_event(format!("Memory {id}: kept as {} because the owner said so{}.",
                        class.tag(), if replaced { ", replacing what they said before" } else { "" }), now);
                    // "I'll remember that you prefer PDFs" — but "I'll remember
                    // TO include installers", because that is how the owner
                    // said it: as something to do, not something that is so.
                    let joined = format!("{} {}", connector(&said), sentence(&lower_first(&said)));
                    if replaced { format!("I'll remember {joined} That replaces what you told me before.") }
                    else { format!("I'll remember {joined}") }
                }
                Remembered::AlreadyKnown(id) => {
                    let id = id.clone();
                    e.memory_mut().mention(&id);
                    format!("I already have that: {}", sentence(&said))
                }
                Remembered::Conflicts { with } => {
                    let old = with.first().and_then(|id| e.memory().get(id)).map(|m| m.statement.clone()).unwrap_or_default();
                    format!("You told me before that {} If that's changed, say “remember that {} instead” and I'll replace it.",
                            sentence(&lower_first(&old)), statement.trim().trim_end_matches('.'))
                }
            }
        }

        MemoryAsk::Recall { about } if about.is_empty() => {
            let current = e.memory().current(now);
            if current.is_empty() { return NOTHING_KEPT_YET.into(); }
            let mut by_class: Vec<(MemoryClass, Vec<String>)> = Vec::new();
            for m in current.iter().take(12) {
                match by_class.iter_mut().find(|(c, _)| *c == m.class) {
                    Some((_, v)) => v.push(m.statement.trim_end_matches('.').to_string()),
                    None => by_class.push((m.class, vec![m.statement.trim_end_matches('.').to_string()])),
                }
            }
            let parts: Vec<String> = by_class.iter()
                .map(|(c, v)| format!("{} — {}", c.heading(), v.join("; ")))
                .collect();
            format!("Here's what I keep. {}.", parts.join(". "))
        }

        MemoryAsk::Recall { about } => {
            let found: Vec<(String, String)> = e.memory().matching(&about, now).into_iter().take(3)
                .map(|m| (m.id.clone(), m.statement.clone())).collect();
            if found.is_empty() { return NOTHING_ABOUT_THAT.into(); }
            e.memory_mut().mention(&found[0].0);
            found.iter().map(|(_, s)| sentence(s)).collect::<Vec<_>>().join(" ")
        }

        MemoryAsk::Forget { about } => {
            if let Some(no) = e.memory_refusal() { return no.to_string(); }
            // The one it fits best, when the words point at one. Otherwise
            // everything they could mean, to be asked about.
            let found: Vec<(String, String)> = match e.memory().best(&about, now) {
                Some(m) => vec![(m.id.clone(), m.statement.clone())],
                None => e.memory().matching(&about, now).into_iter()
                    .map(|m| (m.id.clone(), m.statement.clone())).collect(),
            };
            match found.as_slice() {
                [] => NOTHING_TO_FORGET.into(),
                [(id, statement)] => {
                    let (id, statement) = (id.clone(), statement.clone());
                    e.memory_mut().forget(&id, now);
                    e.record_action_event(format!("Memory {id}: forgotten at the owner's request. The words are gone."), now);
                    format!("Forgotten: {} It's gone from what I keep.", sentence(&lower_first(&statement)))
                }
                // More than one fits. KUE does not choose which of the owner's
                // memories to destroy.
                many => {
                    let list: Vec<String> = many.iter().take(4)
                        .map(|(_, s)| format!("“{}”", s.trim_end_matches('.'))).collect();
                    format!("I have more than one thing about that: {}. Which should I forget? Say it in the words I used.",
                            list.join(", "))
                }
            }
        }

        MemoryAsk::Why => match e.memory().last_mentioned() {
            None => WHICH_MEMORY.into(),
            Some(m) => {
                let when = crate::storage::when(m.created_at);
                m.why_kept(&format!("On {when}"))
            }
        },
    }
}

/// The words a question starts with when it is asking KUE something rather
/// than telling it to do something.
const ASKING: [&str; 12] = ["what", "which", "who", "when", "where", "why", "how", "do you", "does",
                            "should", "am i", "is my"];

/// An answer to a question out of what the owner has told KUE, when there is
/// one. Never a guess and never a model: these are their own words, said back
/// with where they came from.
fn from_memory(rt: &Runtime, text: &str, now: f64) -> Option<String> {
    let t = text.trim().to_lowercase();
    let asking = t.contains('?') || ASKING.iter().any(|w| t.starts_with(w));
    if !asking { return None; }
    let mut e = rt.engine.lock().unwrap();
    let found: Vec<(String, String)> = e.memory().matching(text, now).into_iter().take(3)
        .map(|m| (m.id.clone(), m.statement.clone())).collect();
    let (first, _) = found.first()?;
    let first = first.clone();
    e.memory_mut().mention(&first);
    Some(format!("From what you've told me: {}",
                 found.iter().map(|(_, s)| sentence(s)).collect::<Vec<_>>().join(" ")))
}

/// "that" for something that IS so, "to" for something to do. A sentence that
/// opens with a verb is an instruction, and reads as one.
fn connector(statement: &str) -> &'static str {
    const DOING: [&str; 14] = ["include", "exclude", "leave", "skip", "use", "keep", "always", "never",
                               "put", "move", "clean", "ignore", "add", "avoid"];
    let first = statement.split_whitespace().next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    if DOING.contains(&first.as_str()) { "to" } else { "that" }
}

/// One sentence, ending in a full stop.
fn sentence(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() { return String::new(); }
    if t.ends_with(['.', '!', '?']) { t.to_string() } else { format!("{t}.") }
}

/// "You prefer PDFs" → "you prefer PDFs", for the middle of a sentence.
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A question about KUE's own work, answered from what the runtime holds —
/// never by asking a model to reconstruct it.
///
/// Each answer declares what it carries: counts of the owner's files by kind,
/// or an outcome a storage step reported, go out only as STORAGE_SUMMARY.
fn answer_about_work(rt: &Runtime, q: WorkQuestion, open: Option<&Open>) -> (String, &'static [DataKind]) {
    struct Seen { state: RequestState, why: Option<String>, decision: Option<PipelineDecision>,
                  tool: Option<&'static str>, said_last: Option<String>,
                  asked: Option<String>, confirmed: bool }
    // The request asked about: the live one, else the last one that ended.
    let (active, last) = {
        let e = rt.engine.lock().unwrap();
        let p = e.requests();
        let book = rt.book.lock().unwrap();
        let seen = |r: &crate::pipeline::Request| Seen {
            state: r.state, why: r.why.clone(), decision: r.decision,
            tool: r.ids.tool_execution_id.as_ref()
                .and_then(|id| book.records.iter().find(|x| &x.id == id))
                .and_then(|x| crate::tools::declared(x.action.tag())).map(|t| t.name),
            // KUE's last sentence about this request that was not a reply to a
            // question: the outcome, the progress, or what it proposed.
            said_last: p.dialogue.turns().iter().rev()
                .filter(|t| t.request_id.as_deref() == Some(&r.ids.request_id) && !t.kind.by_owner())
                .map(|t| t.said.clone()).next(),
            // The owner's own words that started it, and whether they said yes.
            asked: p.dialogue.turns().iter()
                .find(|t| t.request_id.as_deref() == Some(&r.ids.request_id) && t.kind == TurnKind::UserSpeech)
                .map(|t| t.said.clone()),
            confirmed: p.dialogue.turns().iter()
                .any(|t| t.request_id.as_deref() == Some(&r.ids.request_id) && t.kind == TurnKind::UserConfirmation),
        };
        (p.active().map(seen), p.requests().iter().rev().find(|r| r.state.is_terminal()).map(seen))
    };
    let selection = match open {
        Some(Open::Selection { goal_id, .. }) => current_selection(&rt.book.lock().unwrap(), goal_id),
        _ => None,
    };
    let trashed = rt.book.lock().unwrap().last_trashed().len();
    // What a standing preference took out of the plan in hand, if any: the
    // owner's sentence, and the kind of file it left out.
    let shaped_by: Vec<(String, String)> = {
        let goal_id = rt.engine.lock().unwrap().requests().active().and_then(|r| r.ids.goal_id.clone());
        let refs = goal_id.and_then(|g| rt.book.lock().unwrap().goal(&g).map(|x| x.memory_refs.clone())).unwrap_or_default();
        let e = rt.engine.lock().unwrap();
        let at = (rt.now)();
        refs.iter().filter_map(|id| e.memory().get(id).filter(|m| m.is_current(at)))
            .filter_map(|m| crate::dialogue::storage_wish(&m.statement)
                .map(|w| (m.statement.clone(), plural_of(w.facet))))
            .collect()
    };
    let doing = |t: Option<&str>| t.map(|n| n.to_lowercase()).unwrap_or_else(|| "what you asked".into());
    let plain = |s: String| (s, &[] as &'static [DataKind]);
    let counted = |s: String| (s, STORAGE);
    match q {
        WorkQuestion::WhatAreYouDoing => match (active.as_ref(), &selection) {
            (Some(a), Some((p, parts, _))) if a.state == RequestState::WaitingForUser || a.state == RequestState::Planning =>
                counted(format!("I'm waiting for you to decide about {}: {}.", files(p.len()), reference_list(parts))),
            (Some(a), _) => plain(match a.state {
                RequestState::WaitingForUser => "I'm waiting for you to confirm what I proposed.".into(),
                RequestState::Authorizing => "I'm waiting for macOS to confirm it's you.".into(),
                RequestState::Acting => format!("I'm working on it: {}.", doing(a.tool)),
                RequestState::Verifying => "I'm checking that it actually happened.".into(),
                RequestState::Answering => "I'm working out the answer to your question.".into(),
                _ => "I'm working out what to do next.".into(),
            }),
            (None, _) => plain("Nothing right now.".into()),
        },
        WorkQuestion::Why => plain(match active.as_ref().or(last.as_ref()) {
            // A plan a standing preference of theirs shaped: the reason is
            // their own words, said back to them.
            Some(a) if a.state == RequestState::WaitingForUser && !shaped_by.is_empty() =>
                format!("Because you told me: {}. That's why I left {} out of it.",
                        shaped_by.iter().map(|(s, _)| format!("“{}”", s.trim_end_matches('.'))).collect::<Vec<_>>().join(", and "),
                        shaped_by.iter().map(|(_, w)| w.clone()).collect::<Vec<_>>().join(" and ")),
            Some(a) if a.state == RequestState::WaitingForUser && selection.is_some() =>
                "Because moving your files is your call. I won't move any until you say so, and macOS will ask for Touch ID.".into(),
            Some(a) if a.state == RequestState::WaitingForUser =>
                "Because this changes something on your Mac, so it waits for your go-ahead.".into(),
            // Finished: the reason is the owner's request, in their words.
            Some(Seen { state: RequestState::Completed, asked: Some(asked), confirmed, decision, .. })
                if *decision != Some(PipelineDecision::Answer) =>
                format!("Because you asked — “{}”{}", asked.trim_end_matches(['.', '!', '?']),
                        if *confirmed { " — and confirmed it." } else { "." }),
            // Refused, failed, stopped: the runtime's own reason.
            Some(Seen { why: Some(why), .. }) => why.clone(),
            Some(Seen { decision: Some(PipelineDecision::Answer), .. }) => "Because you asked a question, so I answered rather than acted.".into(),
            Some(Seen { decision: Some(_), tool: Some(t), .. }) => format!("Because your request needed {}.", t.to_lowercase()),
            _ => "I haven't done anything that needs a reason yet.".into(),
        }),
        WorkQuestion::HowMany => match &selection {
            Some((p, parts, _)) if !p.is_empty() => counted(format!("{}: {}.", capitalized(&files(p.len())), reference_list(parts))),
            Some(_) => plain("None — you've left them all out.".into()),
            None if trashed > 0 => counted(format!("The last move put {} in the Trash.", files(trashed))),
            None => plain("There's nothing I'm counting right now.".into()),
        },
        WorkQuestion::WhatsLeft => match &selection {
            Some((p, parts, _)) if !p.is_empty() => counted(format!("{} left, waiting for you.", capitalized(&reference_list(parts)))),
            Some(_) => plain("Nothing — you've left everything out.".into()),
            None => plain(match active.as_ref() {
                Some(a) => format!("Still {}. Nothing else is queued.", doing(a.tool)),
                None => "Nothing is waiting.".into(),
            }),
        },
        // What the LAST request came to, by its state — never an older result.
        // Its sentence may be a storage report, so it is declared as one.
        WorkQuestion::WhatDidYouDo => match last.as_ref() {
            None => plain("I haven't done anything yet in this conversation.".into()),
            Some(l) => counted(match l.state {
                RequestState::Cancelled if l.decision == Some(PipelineDecision::Answer) && l.tool.is_none() =>
                    "You asked me something, and you stopped me before I answered. Nothing was changed.".into(),
                RequestState::Cancelled => match (l.tool, &l.said_last) {
                    (Some(t), Some(s)) if s.starts_with("Stopped.") =>
                        format!("I started to {}, and you stopped it.{}", t.to_lowercase(), &s["Stopped.".len()..]),
                    (_, Some(s)) => s.clone(),
                    (_, None) => "You stopped it before anything ran. Nothing was changed.".into(),
                },
                RequestState::Completed | RequestState::Uncertain | RequestState::Failed
                | RequestState::Refused | RequestState::Blocked => l.said_last.clone()
                    .unwrap_or_else(|| format!("{} — it ended {}.", doing(l.tool), l.state.tag().to_lowercase())),
                RequestState::Killed => "You stopped KUE with the kill switch. Nothing further ran.".into(),
                _ => "Nothing has finished yet.".into(),
            }),
        },
        WorkQuestion::CanYouUndo => if trashed > 0 {
            counted(format!("Yes — I can put back the {} I moved, exactly where they were. Say “put them back”.", files(trashed)))
        } else {
            plain("There's nothing I can undo: I haven't moved anything I could put back.".into())
        },
    }
}

/// Ends a goal that is waiting for the owner, because the owner said stop.
fn stop_waiting_goal(rt: &Runtime, goal_id: &str, why: &str) {
    let now = (rt.now)();
    let waiting = rt.book.lock().unwrap().goal(goal_id).and_then(|g| g.waiting_for_user().map(|s| s.index));
    if let Some(i) = waiting {
        step_events(rt, goal_id, i, vec![StepEvent::Cancelled(why.into())], now);
        record_step(rt, goal_id, i, now);
    }
}

/// Whether any of these paths is something the owner told a live cleanup goal
/// to leave alone. Checked at the moment of moving: a correction binds, it
/// does not merely change what the list shows.
fn excluded_by_owner(rt: &Runtime, paths: &[String]) -> bool {
    let book = rt.book.lock().unwrap();
    let Some(report) = book.last_storage() else { return false };
    book.goals.iter()
        .filter(|g| g.kind == GoalKind::CleanUpStorage && g.state() != GoalState::Completed
                    && (!g.excluded.is_empty() || g.only.is_some()))
        .any(|g| paths.iter().any(|p| report.candidates.iter().any(|c| &c.path == p && !keeps(g, c))))
}
