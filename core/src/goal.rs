//! Goals and plans: what KUE is trying to get done, as steps it can check.
//!
//!   GOAL  what the owner wants ("find why my storage is full")
//!   CONSTRAINTS  what must hold throughout (nothing deleted; nothing changes
//!         before the owner chooses; only the approved folders are read)
//!   PLAN  steps, in order
//!   STEP  preconditions · the authorization it will need · what it does ·
//!         what success looks like · what it observed · how that was verified
//!         · why it failed, if it did · a recovery, if one is defined
//!   DECISION  after every step: start the next, wait, stop, or finish
//!
//! This module is data and rules only. It runs nothing, reads nothing and
//! authorizes nothing. `transaction` drives a goal: it authorizes each step
//! with the engine when that step runs, executes through the Action Broker or
//! in KUE's own process, and reports what happened here as events.
//!
//! Rules the state machine enforces rather than hopes for:
//!
//! * A step is COMPLETED only from VERIFYING, with the evidence stated. There
//!   is no path from RUNNING to COMPLETED.
//! * FAILED, BLOCKED and CANCELLED are final. A later step never starts after
//!   one of them unless the plan names a recovery step for it.
//! * A step's authorization requirement is a function of the step alone. What
//!   the owner said to start the goal authorizes nothing later.
//! * A step KUE cannot do has the authority NOT_IMPLEMENTED, which no
//!   authorization satisfies.

use crate::actions::{self, ActionKind, Risk};
use crate::authz::Operation;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GoalKind {
    /// Several commands in one sentence: "open Safari, then open Notes".
    RunCommands,
    /// "Find my resume and open it."
    FindAndOpenDocument,
    /// "Why is my storage full?"
    ExplainStorage,
    /// "Clean my storage." Look first, explain, recommend, and change nothing
    /// until the owner chooses.
    CleanUpStorage,
    /// "Clean my computer." What to clean is missing, so KUE asks.
    CleanUpUnspecified,
}

impl GoalKind {
    pub fn tag(self) -> &'static str {
        match self {
            GoalKind::RunCommands => "RUN_COMMANDS",
            GoalKind::FindAndOpenDocument => "FIND_AND_OPEN_DOCUMENT",
            GoalKind::ExplainStorage => "EXPLAIN_STORAGE",
            GoalKind::CleanUpStorage => "CLEAN_UP_STORAGE",
            GoalKind::CleanUpUnspecified => "CLEAN_UP_UNSPECIFIED",
        }
    }
}

/// What must hold for the whole goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Constraint {
    /// Nothing is deleted. The strongest change is a move to the Trash, which can be undone.
    NoDeletion,
    /// Nothing on the Mac changes before the owner chooses it.
    OwnerApprovesChanges,
    /// Only Desktop, Documents, Downloads and ~/KUE are read.
    ApprovedFoldersOnly,
    /// Nothing leaves this Mac.
    StaysOnThisMac,
    /// A step starts only after the one before it was verified.
    StopOnFailure,
}

/// Information the goal cannot proceed without, which only the owner has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Missing {
    /// "Clean my computer": clean what?
    WhatToClean,
}

/// What a step does.
#[derive(Debug, Clone, PartialEq)]
pub enum StepKind {
    /// One allowlisted action, through the transaction and the Action Broker.
    Act(ActionKind),
    /// Search the document folders for a document. The first half of OPEN_DOCUMENT.
    FindDocument { query: String },
    /// Open the document the find step found, once the owner confirms. The second half.
    OpenFoundDocument,
    /// Say why storage is used, from what the storage pass measured. No model.
    ExplainFindings,
    /// Say what is worth reviewing, each with its evidence. Moves nothing.
    RecommendReview,
    /// Wait for the owner to choose. Nothing moves before this step completes.
    WaitForApproval,
    /// Move what the owner chose to the Trash, through the Action Broker.
    MoveChosenToTrash,
    /// Say what happened, from the record of the step before. Changes nothing.
    ReportOutcome,
    /// Ask what the owner means. Nothing else happens until they say.
    AskWhatToClean,
    // Declared so that their authorization is a stated fact rather than an
    // omission. No plan in this build produces them.
    DeletePermanently,
    Purchase,
    SendMessage,
}

impl StepKind {
    pub fn tag(&self) -> &'static str {
        match self {
            StepKind::Act(a) => a.tag(),
            StepKind::FindDocument { .. } => "FIND_DOCUMENT",
            StepKind::OpenFoundDocument => "OPEN_DOCUMENT",
            StepKind::ExplainFindings => "EXPLAIN_FINDINGS",
            StepKind::RecommendReview => "RECOMMEND_REVIEW",
            StepKind::WaitForApproval => "WAIT_FOR_APPROVAL",
            StepKind::MoveChosenToTrash => "MOVE_TO_TRASH",
            StepKind::ReportOutcome => "REPORT",
            StepKind::AskWhatToClean => "ASK_WHAT_TO_CLEAN",
            StepKind::DeletePermanently => "DELETE_PERMANENTLY",
            StepKind::Purchase => "PURCHASE",
            StepKind::SendMessage => "SEND_MESSAGE",
        }
    }

    /// What success looks like, said before the step runs.
    pub fn expected(&self) -> &'static str {
        match self {
            StepKind::Act(_) => "The action succeeds and records what it verified.",
            StepKind::FindDocument { .. } => "At least one matching document, inside the document folders, that exists now.",
            StepKind::OpenFoundDocument => "The document you confirm is handed to its app, and the executor reports it open.",
            StepKind::ExplainFindings => "An explanation whose every number matches what the storage pass measured.",
            StepKind::RecommendReview => "Every finding offered carries its evidence and its caution, and none suggests deleting.",
            StepKind::WaitForApproval => "You choose files, every one of them from this report.",
            StepKind::MoveChosenToTrash => "Each chosen file is in the Trash, checked one at a time.",
            StepKind::ReportOutcome => "A report of what the move did, from its record.",
            StepKind::AskWhatToClean => "You say what to clean.",
            StepKind::DeletePermanently | StepKind::Purchase | StepKind::SendMessage => "Not implemented.",
        }
    }
}

/// Who may let a step run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "authority", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Authority {
    /// The engine decides this operation when the step runs.
    Operation { operation: Operation },
    /// Reads nothing new and changes nothing: it waits, asks, or reports what
    /// an earlier step that was authorized produced.
    NoneNeeded,
    /// No implementation exists, so no authorization lets it run.
    NotImplemented { capability: &'static str },
}

/// Whether the owner confirms this particular step, beyond being authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confirmation {
    None,
    /// The owner presses Confirm, or says yes.
    Owner,
    /// The owner confirms, and macOS confirms it is them (Touch ID or password).
    OwnerAndMacos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Requirement {
    pub authority: Authority,
    pub confirmation: Confirmation,
}

fn confirmation_for(risk: Risk) -> Confirmation {
    match risk {
        Risk::Low => Confirmation::None,
        Risk::Medium => Confirmation::Owner,
        Risk::High | Risk::Critical => Confirmation::OwnerAndMacos,
    }
}

/// A step's requirement, from the step alone. Exhaustive: a new step kind
/// cannot exist without one.
pub fn requirement(kind: &StepKind) -> Requirement {
    let op = |a: &ActionKind| Authority::Operation { operation: actions::operation_for(actions::risk(a)) };
    let document = ActionKind::OpenDocument { query: String::new(), path: None };
    let trash = ActionKind::MoveToTrash { paths: Vec::new() };
    match kind {
        StepKind::Act(a) => Requirement { authority: op(a), confirmation: confirmation_for(actions::risk(a)) },
        // The search is authorized exactly as OPEN_DOCUMENT's search is.
        StepKind::FindDocument { .. } => Requirement { authority: op(&document), confirmation: Confirmation::None },
        StepKind::OpenFoundDocument => Requirement { authority: op(&document), confirmation: confirmation_for(actions::risk(&document)) },
        // Reads what the storage pass found, which names your files: the owner's level, as the storage view.
        StepKind::ExplainFindings | StepKind::RecommendReview =>
            Requirement { authority: Authority::Operation { operation: Operation::ActionLowRisk }, confirmation: Confirmation::None },
        StepKind::WaitForApproval | StepKind::ReportOutcome | StepKind::AskWhatToClean =>
            Requirement { authority: Authority::NoneNeeded, confirmation: Confirmation::None },
        StepKind::MoveChosenToTrash => Requirement { authority: op(&trash), confirmation: confirmation_for(actions::risk(&trash)) },
        StepKind::DeletePermanently => Requirement {
            authority: Authority::NotImplemented { capability: "permanent_deletion" }, confirmation: Confirmation::OwnerAndMacos },
        StepKind::Purchase => Requirement {
            authority: Authority::NotImplemented { capability: "purchasing" }, confirmation: Confirmation::OwnerAndMacos },
        StepKind::SendMessage => Requirement {
            authority: Authority::NotImplemented { capability: "messaging" }, confirmation: Confirmation::Owner },
    }
}

/// What must be true before a step may start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Precondition {
    /// Every earlier step is COMPLETED.
    EarlierStepsCompleted,
    /// The step before has ended, however it ended. Only a report of it needs no more.
    PreviousStepEnded,
    /// A storage pass made for this goal exists.
    StorageReportForThisGoal,
}

// MARK: - Step states

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StepState {
    Pending,
    Ready,
    Running,
    WaitingForUser,
    WaitingForAuthorization,
    Verifying,
    Completed,
    Failed,
    Blocked,
    Cancelled,
}

impl StepState {
    pub fn is_final(self) -> bool {
        matches!(self, StepState::Completed | StepState::Failed | StepState::Blocked | StepState::Cancelled)
    }
    pub fn tag(self) -> &'static str {
        match self {
            StepState::Pending => "PENDING", StepState::Ready => "READY", StepState::Running => "RUNNING",
            StepState::WaitingForUser => "WAITING_FOR_USER", StepState::WaitingForAuthorization => "WAITING_FOR_AUTHORIZATION",
            StepState::Verifying => "VERIFYING", StepState::Completed => "COMPLETED", StepState::Failed => "FAILED",
            StepState::Blocked => "BLOCKED", StepState::Cancelled => "CANCELLED",
        }
    }
}

/// What happened to a step. Reasons and evidence name no file, app or folder:
/// they may be shown and recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepEvent {
    PreconditionsMet,
    PreconditionFailed(String),
    /// The engine allowed it now, or the step needs no authorization.
    Authorized,
    NeedsAuthorization,
    AuthorizationDenied(String),
    NeedsOwner,
    OwnerAnswered,
    Observed(String),
    Verified(String),
    VerificationFailed(String),
    Failed(String),
    Cancelled(String),
    /// A step before it ended without completing.
    EarlierStepStopped,
    /// The plan does not need it after all ("nothing was found to move").
    NotNeeded(String),
}

impl StepEvent {
    pub fn tag(&self) -> &'static str {
        match self {
            StepEvent::PreconditionsMet => "PRECONDITIONS_MET", StepEvent::PreconditionFailed(_) => "PRECONDITION_FAILED",
            StepEvent::Authorized => "AUTHORIZED", StepEvent::NeedsAuthorization => "NEEDS_AUTHORIZATION",
            StepEvent::AuthorizationDenied(_) => "AUTHORIZATION_DENIED", StepEvent::NeedsOwner => "NEEDS_OWNER",
            StepEvent::OwnerAnswered => "OWNER_ANSWERED", StepEvent::Observed(_) => "OBSERVED",
            StepEvent::Verified(_) => "VERIFIED", StepEvent::VerificationFailed(_) => "VERIFICATION_FAILED",
            StepEvent::Failed(_) => "FAILED", StepEvent::Cancelled(_) => "CANCELLED",
            StepEvent::EarlierStepStopped => "EARLIER_STEP_STOPPED", StepEvent::NotNeeded(_) => "NOT_NEEDED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: StepState,
    pub event: &'static str,
}

/// The step state machine. Everything not listed is refused.
pub fn next_state(from: StepState, event: &StepEvent) -> Result<StepState, IllegalTransition> {
    use StepEvent as E;
    use StepState::*;
    let to = match (from, event) {
        (s, _) if s.is_final() => None,
        (Pending, E::PreconditionsMet) => Some(Ready),
        (Pending | Ready, E::PreconditionFailed(_) | E::EarlierStepStopped) => Some(Blocked),
        (Pending | Ready, E::NotNeeded(_)) => Some(Cancelled),
        (Ready | WaitingForAuthorization, E::Authorized) => Some(Running),
        (Ready | Running, E::NeedsAuthorization) => Some(WaitingForAuthorization),
        (Ready | Running | WaitingForUser | WaitingForAuthorization, E::AuthorizationDenied(_)) => Some(Blocked),
        (Running, E::NeedsOwner) => Some(WaitingForUser),
        (WaitingForUser, E::OwnerAnswered) => Some(Running),
        (Running, E::Observed(_)) => Some(Verifying),
        // The only way to COMPLETED, and only with evidence.
        (Verifying, E::Verified(evidence)) if !evidence.trim().is_empty() => Some(Completed),
        (Verifying, E::VerificationFailed(_)) => Some(Failed),
        (Running | WaitingForUser | WaitingForAuthorization, E::Failed(_)) => Some(Failed),
        (Ready | Running | WaitingForUser | WaitingForAuthorization | Verifying, E::Cancelled(_)) => Some(Cancelled),
        _ => None,
    };
    to.ok_or(IllegalTransition { from, event: event.tag() })
}

// MARK: - Steps, plans, goals

#[derive(Debug, Clone)]
pub struct Step {
    pub index: usize,
    pub kind: StepKind,
    pub requirement: Requirement,
    pub preconditions: Vec<Precondition>,
    pub expected: &'static str,
    pub state: StepState,
    pub observation: Option<String>,
    pub verification: Option<String>,
    pub failure: Option<String>,
    /// The step to run if this one ends without completing. None: the plan stops.
    pub recovery: Option<usize>,
    /// The action record this step drives, when it drives one.
    pub action_id: Option<String>,
    /// KUE's sentence about what this step established, for the window. Names no file.
    pub said: Option<String>,
}

/// One change of a step's state, kept so the goal can say how it got where it is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Transition {
    pub step: usize,
    pub from: StepState,
    pub to: StepState,
    pub event: &'static str,
    pub at: f64,
}

/// What to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Start(usize),
    /// A step is in progress or waiting for the owner or macOS.
    Wait(usize),
    /// A step ended without completing and its recovery step has not run.
    Recover { from: usize, to: usize },
    /// A step ended without completing. Nothing after it runs.
    Stop { at: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GoalState {
    Planned,
    InProgress,
    WaitingForUser,
    WaitingForAuthorization,
    Completed,
    /// A step failed or was blocked; the rest did not run.
    Stopped,
    Cancelled,
}

/// A plan before it has an id: what `intent` hands to `transaction`.
#[derive(Debug, Clone, PartialEq)]
pub struct Blueprint {
    pub kind: GoalKind,
    pub constraints: Vec<Constraint>,
    pub steps: Vec<StepKind>,
    pub missing: Vec<Missing>,
}

impl Blueprint {
    pub fn commands(actions: Vec<ActionKind>) -> Blueprint {
        Blueprint { kind: GoalKind::RunCommands, constraints: vec![Constraint::StopOnFailure],
                    steps: actions.into_iter().map(StepKind::Act).collect(), missing: Vec::new() }
    }

    pub fn find_and_open(query: String) -> Blueprint {
        Blueprint { kind: GoalKind::FindAndOpenDocument,
                    constraints: vec![Constraint::ApprovedFoldersOnly, Constraint::OwnerApprovesChanges, Constraint::StopOnFailure],
                    steps: vec![StepKind::FindDocument { query }, StepKind::OpenFoundDocument], missing: Vec::new() }
    }

    pub fn explain_storage() -> Blueprint {
        Blueprint { kind: GoalKind::ExplainStorage,
                    constraints: vec![Constraint::ApprovedFoldersOnly, Constraint::StaysOnThisMac, Constraint::NoDeletion, Constraint::StopOnFailure],
                    steps: vec![StepKind::Act(ActionKind::InspectStorage), StepKind::ExplainFindings], missing: Vec::new() }
    }

    pub fn clean_up_storage() -> Blueprint {
        Blueprint { kind: GoalKind::CleanUpStorage,
                    constraints: vec![Constraint::NoDeletion, Constraint::OwnerApprovesChanges, Constraint::ApprovedFoldersOnly,
                                      Constraint::StaysOnThisMac, Constraint::StopOnFailure],
                    steps: vec![StepKind::Act(ActionKind::InspectStorage), StepKind::ExplainFindings, StepKind::RecommendReview,
                                StepKind::WaitForApproval, StepKind::MoveChosenToTrash, StepKind::ReportOutcome],
                    missing: Vec::new() }
    }

    pub fn clean_up_unspecified() -> Blueprint {
        Blueprint { kind: GoalKind::CleanUpUnspecified,
                    constraints: vec![Constraint::NoDeletion, Constraint::OwnerApprovesChanges],
                    steps: vec![StepKind::AskWhatToClean], missing: vec![Missing::WhatToClean] }
    }
}

#[derive(Debug, Clone)]
pub struct Goal {
    pub id: String,
    pub kind: GoalKind,
    /// VOICE or TEXT.
    pub source: String,
    pub constraints: Vec<Constraint>,
    pub missing: Vec<Missing>,
    pub steps: Vec<Step>,
    pub transitions: Vec<Transition>,
    pub created_at: f64,
    /// Steps from this index on are not needed, and the goal finishes before them.
    pub finished_early_at: Option<usize>,
    /// What the owner chose at WAIT_FOR_APPROVAL. In memory only; never shown
    /// through a goal view, never recorded.
    pub chosen: Vec<String>,
    /// Whether the goal's end has been recorded as an event, so it is recorded once.
    pub outcome_recorded: bool,
    /// What the owner told KUE to leave alone ("don't touch the installers",
    /// "leave the PDF"). In memory only. Enforced at the moment of moving, not
    /// only when the list is shown: a correction binds.
    pub excluded: Vec<crate::dialogue::Facet>,
    /// "Only the duplicates": when set, nothing else is offered.
    pub only: Option<crate::dialogue::Facet>,
    /// Each correction's state before it was applied, so "go back" undoes
    /// exactly the last one.
    pub revisions: Vec<(Vec<crate::dialogue::Facet>, Option<crate::dialogue::Facet>)>,
    /// The memories that shaped this goal before the owner said anything about
    /// it — a standing preference of theirs, applied by KUE and attached here
    /// so "why did you leave those out?" is answered from what they said, not
    /// from a guess. Ids only; the memory itself lives in the memory book.
    pub memory_refs: Vec<String>,
}

impl Goal {
    pub fn new(id: String, source: &str, bp: Blueprint, now: f64) -> Goal {
        let n = bp.steps.len();
        let steps = bp.steps.into_iter().enumerate().map(|(index, kind)| {
            let preconditions = match &kind {
                StepKind::ReportOutcome => vec![Precondition::PreviousStepEnded],
                StepKind::ExplainFindings | StepKind::RecommendReview | StepKind::WaitForApproval =>
                    vec![Precondition::EarlierStepsCompleted, Precondition::StorageReportForThisGoal],
                _ if index == 0 => Vec::new(),
                _ => vec![Precondition::EarlierStepsCompleted],
            };
            // A failed move is still reported: the report reads the move's record and changes nothing.
            let recovery = match &kind {
                StepKind::MoveChosenToTrash if index + 1 < n => Some(index + 1),
                _ => None,
            };
            Step { index, requirement: requirement(&kind), expected: kind.expected(), kind, preconditions,
                   state: StepState::Pending, observation: None, verification: None, failure: None, recovery,
                   action_id: None, said: None }
        }).collect();
        Goal { id, kind: bp.kind, source: source.to_string(), constraints: bp.constraints, missing: bp.missing, steps,
               transitions: Vec::new(), created_at: now, finished_early_at: None, chosen: Vec::new(),
               outcome_recorded: false, excluded: Vec::new(), only: None, revisions: Vec::new(),
               memory_refs: Vec::new() }
    }

    /// Applies one event to one step, through the state machine.
    pub fn apply(&mut self, index: usize, event: StepEvent, now: f64) -> Result<StepState, IllegalTransition> {
        let step = &mut self.steps[index];
        let from = step.state;
        let to = next_state(from, &event)?;
        match &event {
            StepEvent::Observed(o) => step.observation = Some(o.clone()),
            StepEvent::Verified(v) => step.verification = Some(v.clone()),
            StepEvent::PreconditionFailed(r) | StepEvent::AuthorizationDenied(r) | StepEvent::VerificationFailed(r)
            | StepEvent::Failed(r) | StepEvent::Cancelled(r) | StepEvent::NotNeeded(r) => step.failure = Some(r.clone()),
            StepEvent::EarlierStepStopped => step.failure = Some("An earlier step did not complete, so this one did not start.".into()),
            _ => {}
        }
        step.state = to;
        self.transitions.push(Transition { step: index, from, to, event: event.tag(), at: now });
        Ok(to)
    }

    /// Whether step `index`'s preconditions hold, given the facts only the caller has.
    pub fn preconditions_hold(&self, index: usize, storage_report_for_this_goal: bool) -> Result<(), String> {
        for p in &self.steps[index].preconditions {
            match p {
                Precondition::EarlierStepsCompleted if self.steps[..index].iter().any(|s| s.state != StepState::Completed) =>
                    return Err("An earlier step has not completed.".into()),
                Precondition::PreviousStepEnded if index > 0 && !self.steps[index - 1].state.is_final() =>
                    return Err("The step before has not ended.".into()),
                Precondition::StorageReportForThisGoal if !storage_report_for_this_goal =>
                    return Err("There is no storage report from this request.".into()),
                _ => {}
            }
        }
        Ok(())
    }

    /// What to do next. Pure: the same goal always gives the same decision.
    pub fn decide(&self) -> Decision {
        for (i, s) in self.steps.iter().enumerate() {
            if self.finished_early_at.is_some_and(|f| i >= f) { return Decision::Finished; }
            match s.state {
                StepState::Completed => continue,
                StepState::Pending => return Decision::Start(i),
                StepState::Ready | StepState::Running | StepState::Verifying
                | StepState::WaitingForUser | StepState::WaitingForAuthorization => return Decision::Wait(i),
                StepState::Failed | StepState::Blocked | StepState::Cancelled => {
                    return match s.recovery {
                        Some(r) if self.steps[r].state == StepState::Pending => Decision::Recover { from: i, to: r },
                        Some(r) if !self.steps[r].state.is_final() => Decision::Wait(r),
                        _ => Decision::Stop { at: i },
                    };
                }
            }
        }
        Decision::Finished
    }

    pub fn state(&self) -> GoalState {
        match self.decide() {
            Decision::Finished => GoalState::Completed,
            Decision::Start(0) => GoalState::Planned,
            Decision::Start(_) | Decision::Recover { .. } => GoalState::InProgress,
            Decision::Wait(i) => match self.steps[i].state {
                StepState::WaitingForUser => GoalState::WaitingForUser,
                StepState::WaitingForAuthorization => GoalState::WaitingForAuthorization,
                _ => GoalState::InProgress,
            },
            Decision::Stop { at } if self.steps[at].state == StepState::Cancelled => GoalState::Cancelled,
            Decision::Stop { .. } => GoalState::Stopped,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.state(), GoalState::Planned | GoalState::InProgress | GoalState::WaitingForUser | GoalState::WaitingForAuthorization)
    }

    /// Blocks every step after `at` that has not started, except a recovery step still to run.
    pub fn block_after(&mut self, at: usize, now: f64) {
        let recovery = self.steps[at].recovery;
        for i in at + 1..self.steps.len() {
            if Some(i) == recovery { continue; }
            if matches!(self.steps[i].state, StepState::Pending | StepState::Ready) {
                let _ = self.apply(i, StepEvent::EarlierStepStopped, now);
            }
        }
    }

    /// The plan needs nothing from `from` on: those steps are cancelled as not needed.
    pub fn finish_early(&mut self, from: usize, why: &str, now: f64) {
        for i in from..self.steps.len() {
            if matches!(self.steps[i].state, StepState::Pending | StepState::Ready) {
                let _ = self.apply(i, StepEvent::NotNeeded(why.to_string()), now);
            }
        }
        self.finished_early_at = Some(from);
    }

    /// Cancels every step that has not ended: the one in progress is cancelled,
    /// the ones not started are blocked.
    pub fn cancel(&mut self, why: &str, now: f64) {
        for i in 0..self.steps.len() {
            match self.steps[i].state {
                StepState::Pending => { let _ = self.apply(i, StepEvent::EarlierStepStopped, now); }
                s if !s.is_final() => { let _ = self.apply(i, StepEvent::Cancelled(why.to_string()), now); }
                _ => {}
            }
        }
    }

    /// The step waiting for the owner, if any.
    pub fn waiting_for_user(&self) -> Option<&Step> {
        self.steps.iter().find(|s| s.state == StepState::WaitingForUser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(bp: Blueprint) -> Goal { Goal::new("g1".into(), "VOICE", bp, 100.0) }

    /// Runs one step through to COMPLETED the only way the machine allows.
    fn complete(g: &mut Goal, i: usize) {
        for e in [StepEvent::PreconditionsMet, StepEvent::Authorized, StepEvent::Observed("seen".into()), StepEvent::Verified("checked".into())] {
            g.apply(i, e, 101.0).unwrap();
        }
    }

    #[test]
    fn completed_is_reached_only_through_verification_with_evidence() {
        use StepState::*;
        for from in [Pending, Ready, Running, WaitingForUser, WaitingForAuthorization] {
            assert!(next_state(from, &StepEvent::Verified("ok".into())).is_err(), "{from:?} → COMPLETED without VERIFYING");
        }
        assert!(next_state(Verifying, &StepEvent::Verified("   ".into())).is_err(), "no evidence, no completion");
        assert_eq!(next_state(Verifying, &StepEvent::Verified("read back".into())), Ok(Completed));
        assert_eq!(next_state(Verifying, &StepEvent::VerificationFailed("did not match".into())), Ok(Failed));
        // Every state, every event: nothing but VERIFIED from VERIFYING reaches COMPLETED.
        let events = [StepEvent::PreconditionsMet, StepEvent::PreconditionFailed("x".into()), StepEvent::Authorized,
            StepEvent::NeedsAuthorization, StepEvent::AuthorizationDenied("x".into()), StepEvent::NeedsOwner, StepEvent::OwnerAnswered,
            StepEvent::Observed("x".into()), StepEvent::Verified("x".into()), StepEvent::VerificationFailed("x".into()),
            StepEvent::Failed("x".into()), StepEvent::Cancelled("x".into()), StepEvent::EarlierStepStopped, StepEvent::NotNeeded("x".into())];
        for from in [Pending, Ready, Running, WaitingForUser, WaitingForAuthorization, Verifying, Completed, Failed, Blocked, Cancelled] {
            for e in &events {
                if next_state(from, e) == Ok(Completed) {
                    assert!(from == Verifying && matches!(e, StepEvent::Verified(_)), "{from:?} + {} → COMPLETED", e.tag());
                }
            }
        }
    }

    #[test]
    fn final_states_are_final() {
        use StepState::*;
        for from in [Completed, Failed, Blocked, Cancelled] {
            for e in [StepEvent::PreconditionsMet, StepEvent::Authorized, StepEvent::OwnerAnswered, StepEvent::Observed("x".into()),
                      StepEvent::Verified("x".into()), StepEvent::Cancelled("x".into())] {
                assert!(next_state(from, &e).is_err(), "{from:?} + {}", e.tag());
            }
        }
    }

    #[test]
    fn a_failed_step_stops_the_plan_and_later_steps_are_blocked_not_started() {
        let mut g = goal(Blueprint::commands(vec![
            ActionKind::OpenApplication { name: "Safari".into() }, ActionKind::OpenApplication { name: "Notes".into() },
            ActionKind::OpenApplication { name: "Mail".into() }]));
        assert_eq!(g.state(), GoalState::Planned);
        assert_eq!(g.decide(), Decision::Start(0));
        complete(&mut g, 0);
        assert_eq!(g.decide(), Decision::Start(1));
        g.apply(1, StepEvent::PreconditionsMet, 102.0).unwrap();
        g.apply(1, StepEvent::Authorized, 102.0).unwrap();
        g.apply(1, StepEvent::Failed("did not become frontmost".into()), 102.0).unwrap();
        assert_eq!(g.decide(), Decision::Stop { at: 1 }, "FAILED never leads to the next step");
        g.block_after(1, 103.0);
        assert_eq!(g.steps[2].state, StepState::Blocked);
        assert_eq!(g.state(), GoalState::Stopped);
        assert!(!g.is_open());
    }

    #[test]
    fn unverified_is_not_completed_and_stops_the_plan() {
        let mut g = goal(Blueprint::find_and_open("resume".into()));
        g.apply(0, StepEvent::PreconditionsMet, 101.0).unwrap();
        g.apply(0, StepEvent::Authorized, 101.0).unwrap();
        g.apply(0, StepEvent::Observed("2 matches".into()), 101.0).unwrap();
        g.apply(0, StepEvent::VerificationFailed("the chosen file is no longer there".into()), 101.0).unwrap();
        assert_eq!(g.decide(), Decision::Stop { at: 0 });
        assert_eq!(g.steps[1].state, StepState::Pending);
        g.block_after(0, 102.0);
        assert_eq!(g.steps[1].state, StepState::Blocked);
    }

    #[test]
    fn waiting_for_the_owner_and_for_macos_are_states_of_their_own() {
        let mut g = goal(Blueprint::clean_up_storage());
        for i in 0..3 { complete(&mut g, i); }
        assert_eq!(g.decide(), Decision::Start(3));
        g.apply(3, StepEvent::PreconditionsMet, 101.0).unwrap();
        g.apply(3, StepEvent::Authorized, 101.0).unwrap();
        g.apply(3, StepEvent::NeedsOwner, 101.0).unwrap();
        assert_eq!(g.state(), GoalState::WaitingForUser);
        assert_eq!(g.decide(), Decision::Wait(3), "nothing moves while the owner has not chosen");
        assert_eq!(g.waiting_for_user().map(|s| s.index), Some(3));
        g.apply(3, StepEvent::OwnerAnswered, 102.0).unwrap();
        g.apply(3, StepEvent::Observed("chose 2".into()), 102.0).unwrap();
        g.apply(3, StepEvent::Verified("both were offered".into()), 102.0).unwrap();
        g.apply(4, StepEvent::PreconditionsMet, 102.0).unwrap();
        g.apply(4, StepEvent::NeedsAuthorization, 102.0).unwrap();
        assert_eq!(g.state(), GoalState::WaitingForAuthorization);
    }

    #[test]
    fn a_defined_recovery_runs_and_the_goal_still_reports_the_failure() {
        let mut g = goal(Blueprint::clean_up_storage());
        for i in 0..4 { complete(&mut g, i); }
        g.apply(4, StepEvent::PreconditionsMet, 101.0).unwrap();
        g.apply(4, StepEvent::Authorized, 101.0).unwrap();
        g.apply(4, StepEvent::Failed("1 of 3 did not move".into()), 101.0).unwrap();
        assert_eq!(g.decide(), Decision::Recover { from: 4, to: 5 }, "the report is the plan's stated recovery");
        g.block_after(4, 101.0);
        assert_eq!(g.steps[5].state, StepState::Pending, "the recovery step is not blocked");
        assert_eq!(g.preconditions_hold(5, true), Ok(()));
        complete(&mut g, 5);
        assert_eq!(g.decide(), Decision::Stop { at: 4 });
        assert_eq!(g.state(), GoalState::Stopped, "a report of a failure is not a success");
    }

    #[test]
    fn cancelling_cancels_what_is_running_and_blocks_what_has_not_started() {
        let mut g = goal(Blueprint::clean_up_storage());
        complete(&mut g, 0);
        g.apply(1, StepEvent::PreconditionsMet, 101.0).unwrap();
        g.apply(1, StepEvent::Authorized, 101.0).unwrap();
        g.cancel("KUE was killed.", 102.0);
        assert_eq!(g.steps[1].state, StepState::Cancelled);
        assert!(g.steps[2..].iter().all(|s| s.state == StepState::Blocked));
        assert_eq!(g.state(), GoalState::Cancelled);
        assert_eq!(g.steps[0].state, StepState::Completed, "what was done stays recorded as done");
    }

    #[test]
    fn finishing_early_is_completion_not_failure() {
        let mut g = goal(Blueprint::clean_up_storage());
        for i in 0..3 { complete(&mut g, i); }
        g.finish_early(3, "Nothing was found to move.", 101.0);
        assert_eq!(g.decide(), Decision::Finished);
        assert_eq!(g.state(), GoalState::Completed);
        assert!(g.steps[3..].iter().all(|s| s.state == StepState::Cancelled && s.failure.as_deref() == Some("Nothing was found to move.")));
    }

    #[test]
    fn preconditions_are_checked_not_assumed() {
        let g = goal(Blueprint::explain_storage());
        assert!(g.preconditions_hold(1, true).is_err(), "the storage pass has not completed");
        let mut g = goal(Blueprint::explain_storage());
        complete(&mut g, 0);
        assert!(g.preconditions_hold(1, false).is_err(), "no report from this request");
        assert_eq!(g.preconditions_hold(1, true), Ok(()));
    }

    #[test]
    fn authorization_is_per_step_and_comes_from_the_step_alone() {
        let r = |k: StepKind| requirement(&k);
        assert_eq!(r(StepKind::Act(ActionKind::InspectStorage)).authority,
            Authority::Operation { operation: Operation::ActionLowRisk }, "reading storage: the owner's level");
        let trash = r(StepKind::MoveChosenToTrash);
        assert_eq!(trash.authority, Authority::Operation { operation: Operation::ActionHighRisk }, "moving files: stronger");
        assert_eq!(trash.confirmation, Confirmation::OwnerAndMacos);
        assert_eq!(r(StepKind::DeletePermanently).authority, Authority::NotImplemented { capability: "permanent_deletion" });
        let buy = r(StepKind::Purchase);
        assert_eq!(buy.authority, Authority::NotImplemented { capability: "purchasing" });
        assert_eq!(buy.confirmation, Confirmation::OwnerAndMacos, "a purchase would need explicit confirmation");
        let send = r(StepKind::SendMessage);
        assert_eq!(send.authority, Authority::NotImplemented { capability: "messaging" });
        assert_eq!(send.confirmation, Confirmation::Owner, "a message would need explicit confirmation");

        // A plan never inherits authority: the same step needs the same thing in
        // every goal, whatever the sentence that started it.
        let a = goal(Blueprint::clean_up_storage());
        let b = goal(Blueprint::commands(vec![ActionKind::InspectStorage]));
        assert_eq!(a.steps[0].requirement, b.steps[0].requirement);
        for g in [goal(Blueprint::clean_up_storage()), goal(Blueprint::find_and_open("x".into())),
                  goal(Blueprint::explain_storage()), goal(Blueprint::clean_up_unspecified())] {
            for s in &g.steps {
                assert_eq!(s.requirement, requirement(&s.kind), "{}", s.kind.tag());
                if matches!(s.kind, StepKind::Act(_) | StepKind::FindDocument { .. } | StepKind::OpenFoundDocument | StepKind::MoveChosenToTrash) {
                    assert!(matches!(s.requirement.authority, Authority::Operation { .. }), "{} acts, so it names an operation", s.kind.tag());
                }
            }
        }
    }

    #[test]
    fn the_brief_plan_for_storage_is_represented_in_order() {
        let g = goal(Blueprint::clean_up_storage());
        assert_eq!(g.steps.iter().map(|s| s.kind.tag()).collect::<Vec<_>>(),
            ["INSPECT_STORAGE", "EXPLAIN_FINDINGS", "RECOMMEND_REVIEW", "WAIT_FOR_APPROVAL", "MOVE_TO_TRASH", "REPORT"]);
        assert!(g.constraints.contains(&Constraint::NoDeletion) && g.constraints.contains(&Constraint::OwnerApprovesChanges));
        let ask = goal(Blueprint::clean_up_unspecified());
        assert_eq!(ask.missing, [Missing::WhatToClean]);
    }
}
