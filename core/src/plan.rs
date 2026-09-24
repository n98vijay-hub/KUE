//! A plan: the steps KUE will take toward a goal, as one object that can be
//! checked before anything runs, shown to the owner, approved, and followed.
//!
//! TWO SOURCES, ONE CONTRACT
//!
//! - KUE's own plans are its goals (goal.rs). `Plan::of` reads a goal as a
//!   plan. Nothing is stored twice: risk comes from `actions::risk`, the
//!   authorization from `goal::requirement`, what counts as done from
//!   `StepKind::expected`, how a step is undone from the declared tool.
//! - A model may one day PROPOSE a plan. `parse` reads a proposal strictly,
//!   and `validate` checks every step against the declared tools (tools.rs):
//!   the tool exists, its input matches its schema, it can run now, it can be
//!   authorized, the data it touches may go where it goes, its preconditions
//!   can hold, it has a verifier and a declared rollback, and the steps depend
//!   only on earlier ones. A proposal that fails is REJECTED; one that is
//!   sound but cannot run now is BLOCKED, with the reason. Risk is never taken
//!   from a proposal — a field for it is refused.
//!
//! NO MODEL IS CONNECTED TO THIS IN THIS BUILD. Nothing here executes
//! anything, and a validated proposal is not wired to the runtime.
//!
//! VALIDATION IS NOT AUTHORIZATION, AND APPROVAL IS NOT EITHER. `validate`
//! only previews authorization; `approval` says what the owner is asked
//! before the plan proceeds. Each step is still authorized by the engine,
//! confirmed and verified when it runs, exactly as if it had been asked for on
//! its own. A plan-level "yes" is added to that, never used in its place.

use crate::actions::{self, ActionKind, Risk, TrashedItem};
use crate::authz::Operation;
use crate::goal::{self, Authority, Confirmation, Goal, GoalState, StepKind};
use crate::privacy::{DataKind, Destination};
use crate::tools::{self, Conditions, FieldType, Rollback};
use serde::Serialize;

/// The most steps a proposal may have. A plan a person can read before
/// agreeing to it.
pub const MAX_STEPS: usize = 12;

// MARK: - The plan

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StepState {
    Pending,
    Ready,
    /// Started; may be waiting on the owner or on macOS (`waiting`).
    Running,
    /// Ran, and what it did was read back and matched.
    Succeeded,
    Failed,
    /// Could not start: a precondition, an authorization or the firewall.
    Blocked,
    Cancelled,
    /// Never needed to run: an earlier step stopped the plan, or the plan
    /// finished without it.
    Skipped,
    /// Ran, and what was read back did not match. Never counted as done.
    VerificationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Waiting { Owner, Macos }

/// Who put the plan forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "by", content = "detail")]
pub enum Proposer {
    /// One of KUE's own plans, written in code (goal.rs).
    Kue(&'static str),
    /// A model. Its plan is a proposal until it validates, and the governed
    /// runtime decides every step regardless.
    Model(String),
}

/// Who agreed to it, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Approver {
    /// The owner said yes, or chose.
    Owner,
    /// The owner said yes, and macOS confirmed it was them.
    OwnerAndMacos,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Approval {
    pub by: Approver,
    pub at: f64,
    /// The step the approval was given at.
    pub step: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanState {
    /// Not started.
    Planned,
    Running,
    /// Waiting for the owner to choose or confirm.
    WaitingForOwner,
    /// Waiting for macOS to confirm it is the owner.
    WaitingForMacos,
    /// Every step succeeded, and every one was verified.
    Completed,
    /// A step did not succeed; nothing after it ran.
    Stopped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStep {
    pub index: usize,
    /// The step, as a tag: the declared tool's id, or KUE's own step kind.
    pub step: &'static str,
    /// The declared tool this step runs, when it runs one.
    pub tool: Option<&'static str>,
    pub state: StepState,
    pub waiting: Option<Waiting>,
    pub depends_on: Vec<usize>,
    /// From `actions::risk`; None for a step that is not an action.
    pub risk: Option<Risk>,
    pub authority: Authority,
    pub confirmation: Confirmation,
    /// What must be read back for it to count as done.
    pub verification: &'static str,
    pub rollback: Rollback,
    /// KUE's sentence about what the step established. Names no file.
    pub said: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub plan_id: String,
    pub goal_id: Option<String>,
    pub created_at: f64,
    pub steps: Vec<PlanStep>,
    /// The highest risk of any step.
    pub risk: Option<Risk>,
    /// The strongest authorization any step needs.
    pub required_authorization: Option<Operation>,
    pub current_step: Option<usize>,
    pub state: PlanState,
    pub proposed_by: Proposer,
    pub approved_by: Vec<Approval>,
    pub verification_requirements: Vec<&'static str>,
    /// Whether everything the plan changes can be undone by KUE.
    pub reversible: bool,
}

/// The declared tool a goal step runs, if it runs one. Public because what a
/// finished goal DID is written down by the transaction, and the tools are
/// what it may say (memory.rs keeps no targets).
pub fn tool_name(kind: &StepKind) -> Option<&'static str> { tool_of(kind) }

/// The declared tool a goal step runs, if it runs one.
fn tool_of(kind: &StepKind) -> Option<&'static str> {
    match kind {
        StepKind::Act(a) => Some(a.tag()),
        StepKind::FindDocument { .. } | StepKind::OpenFoundDocument => Some("OPEN_DOCUMENT"),
        StepKind::MoveChosenToTrash => Some("MOVE_TO_TRASH"),
        _ => None,
    }
}

fn risk_of(kind: &StepKind) -> Option<Risk> {
    match kind {
        StepKind::Act(a) => Some(actions::risk(a)),
        StepKind::FindDocument { .. } | StepKind::OpenFoundDocument =>
            Some(actions::risk(&ActionKind::OpenDocument { query: String::new(), path: None })),
        StepKind::MoveChosenToTrash => Some(actions::risk(&ActionKind::MoveToTrash { paths: Vec::new() })),
        _ => None,
    }
}

fn operation_rank(op: Operation) -> u8 {
    match op {
        Operation::ActionLowRisk => 1, Operation::ActionMediumRisk => 2,
        Operation::ActionHighRisk => 3, Operation::ActionCriticalRisk => 4, _ => 0,
    }
}

impl Plan {
    /// A goal, read as a plan. Every field is derived from the goal and the
    /// declarations; nothing is kept here that the goal does not already hold.
    pub fn of(g: &Goal) -> Plan {
        let storage_step = g.steps.iter().position(|s| s.kind == StepKind::Act(ActionKind::InspectStorage));
        let steps: Vec<PlanStep> = g.steps.iter().map(|s| {
            let last = g.transitions.iter().rev().find(|t| t.step == s.index);
            let state = match s.state {
                goal::StepState::Pending => StepState::Pending,
                goal::StepState::Ready => StepState::Ready,
                goal::StepState::Running | goal::StepState::Verifying
                | goal::StepState::WaitingForUser | goal::StepState::WaitingForAuthorization => StepState::Running,
                goal::StepState::Completed => StepState::Succeeded,
                goal::StepState::Failed if last.is_some_and(|t| t.event == "VERIFICATION_FAILED") => StepState::VerificationFailed,
                goal::StepState::Failed => StepState::Failed,
                goal::StepState::Blocked if last.is_some_and(|t| t.event == "EARLIER_STEP_STOPPED") => StepState::Skipped,
                goal::StepState::Blocked => StepState::Blocked,
                goal::StepState::Cancelled if last.is_some_and(|t| t.event == "NOT_NEEDED") => StepState::Skipped,
                goal::StepState::Cancelled => StepState::Cancelled,
            };
            let waiting = match s.state {
                goal::StepState::WaitingForUser => Some(Waiting::Owner),
                goal::StepState::WaitingForAuthorization => Some(Waiting::Macos),
                _ => None,
            };
            let mut depends_on: Vec<usize> = Vec::new();
            for p in &s.preconditions {
                match p {
                    goal::Precondition::EarlierStepsCompleted => depends_on.extend(0..s.index),
                    goal::Precondition::PreviousStepEnded => depends_on.extend(s.index.checked_sub(1)),
                    goal::Precondition::StorageReportForThisGoal => depends_on.extend(storage_step.filter(|i| *i < s.index)),
                }
            }
            depends_on.sort_unstable();
            depends_on.dedup();
            let tool = tool_of(&s.kind);
            PlanStep {
                index: s.index, step: s.kind.tag(), tool, state, waiting, depends_on,
                risk: risk_of(&s.kind), authority: s.requirement.authority, confirmation: s.requirement.confirmation,
                verification: s.expected,
                // A step that runs no tool reads, explains, waits or reports:
                // it changes nothing.
                rollback: tool.and_then(tools::declared).map(|t| t.rollback).unwrap_or(Rollback::NothingChanged),
                said: s.said.clone(),
            }
        }).collect();

        // The owner's approvals, read from how the steps got where they are:
        // a choice made at a step that waited for them, and macOS confirming
        // them at a step that waited for it.
        let approved_by = g.transitions.iter().filter_map(|t| match (t.from, t.event) {
            (goal::StepState::WaitingForUser, "OWNER_ANSWERED") => Some(Approval { by: Approver::Owner, at: t.at, step: t.step }),
            (goal::StepState::WaitingForAuthorization, "AUTHORIZED") => Some(Approval { by: Approver::OwnerAndMacos, at: t.at, step: t.step }),
            _ => None,
        }).collect();

        let state = match g.state() {
            GoalState::Planned => PlanState::Planned,
            GoalState::InProgress => PlanState::Running,
            GoalState::WaitingForUser => PlanState::WaitingForOwner,
            GoalState::WaitingForAuthorization => PlanState::WaitingForMacos,
            GoalState::Completed => PlanState::Completed,
            GoalState::Stopped => PlanState::Stopped,
            GoalState::Cancelled => PlanState::Cancelled,
        };
        let current_step = match g.decide() {
            goal::Decision::Start(i) | goal::Decision::Wait(i) => Some(i),
            goal::Decision::Recover { to, .. } => Some(to),
            goal::Decision::Stop { .. } | goal::Decision::Finished => None,
        };
        summarize(g.id.clone(), Some(g.id.clone()), g.created_at, steps, current_step, state,
                  Proposer::Kue(g.kind.tag()), approved_by)
    }
}

fn summarize(plan_id: String, goal_id: Option<String>, created_at: f64, steps: Vec<PlanStep>, current_step: Option<usize>,
             state: PlanState, proposed_by: Proposer, approved_by: Vec<Approval>) -> Plan {
    let risk = steps.iter().filter_map(|s| s.risk).max();
    let required_authorization = steps.iter().filter_map(|s| match s.authority {
        Authority::Operation { operation } => Some(operation),
        _ => None,
    }).max_by_key(|op| operation_rank(*op));
    let verification_requirements = steps.iter().map(|s| s.verification).collect();
    let reversible = steps.iter().all(|s| !matches!(s.rollback, Rollback::NotPossible(_)));
    Plan { plan_id, goal_id, created_at, steps, risk, required_authorization, current_step, state,
           proposed_by, approved_by, verification_requirements, reversible }
}

// MARK: - What the owner is asked before a plan proceeds

/// What a plan asks of the owner, from the risk and reversibility of its
/// steps: MAXIMUM SAFE AUTONOMY, MINIMUM UNNECESSARY INTERRUPTION — and never
/// less than each step's own confirmation, which still happens when it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "ask", content = "detail")]
pub enum Gate {
    /// Every step is low risk and changes nothing that cannot be undone: it
    /// proceeds, and says what it did.
    Proceed,
    /// Something in it needs the owner's word: one yes for the whole plan,
    /// before anything runs.
    ConfirmPlan,
    /// A consequential step: asked at that step, with macOS confirming it is
    /// the owner (Touch ID or password).
    ConfirmAtStep(usize),
    /// A critical step: a fresh, single-use physical confirmation at that step.
    StrongAuthAtStep(usize),
    /// A step no authorization can let run.
    Refuse(String),
}

/// Every gate the plan must pass, in order. Additive: the plan-level yes does
/// not stand in for any step's own confirmation or authorization.
pub fn approval(plan: &Plan) -> Vec<Gate> {
    let mut gates = Vec::new();
    for s in &plan.steps {
        if let Authority::NotImplemented { capability } = s.authority {
            return vec![Gate::Refuse(format!("KUE cannot do {} — {capability} is not implemented.", s.step.to_lowercase().replace('_', " ")))];
        }
    }
    let irreversible = plan.steps.iter().any(|s| matches!(s.rollback, Rollback::NotPossible(_)) && s.tool.is_some());
    let medium = plan.steps.iter().any(|s| s.risk == Some(Risk::Medium));
    if medium || irreversible { gates.push(Gate::ConfirmPlan); }
    for s in &plan.steps {
        match s.risk {
            Some(Risk::High) => gates.push(Gate::ConfirmAtStep(s.index)),
            Some(Risk::Critical) => gates.push(Gate::StrongAuthAtStep(s.index)),
            _ => {}
        }
    }
    if gates.is_empty() { gates.push(Gate::Proceed); }
    gates
}

/// What the owner may say to a plan preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Reply { Yes, No, Change, Ask, Cancel }

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    /// KUE's sentences, in order. No file names, no paths.
    pub lines: Vec<String>,
    pub gates: Vec<Gate>,
    pub replies: &'static [Reply],
}

/// The plan, said before it runs: the steps by what they do, what the owner
/// will be asked and when, and whether it can be undone.
pub fn preview(plan: &Plan) -> Preview {
    let gates = approval(plan);
    let names: Vec<String> = plan.steps.iter().map(|s| match s.tool.and_then(tools::declared) {
        Some(t) => t.name.to_lowercase(),
        None => s.step.to_lowercase().replace('_', " "),
    }).collect();
    let mut lines = vec![match names.len() {
        1 => format!("I'd {}.", names[0]),
        n => format!("I'd do this in {} steps: {}.", crate::voice::reference::spoken_count(n), names.join(", then ")),
    }];
    for g in &gates {
        match g {
            Gate::Proceed => lines.push("Nothing here changes anything you can't get back, so I'd go ahead and tell you what I did.".into()),
            Gate::ConfirmPlan => lines.push("I'll wait for your yes before starting.".into()),
            Gate::ConfirmAtStep(i) => lines.push(format!("Step {} changes your files, so macOS will ask for Touch ID right before it.", i + 1)),
            Gate::StrongAuthAtStep(i) => lines.push(format!("Step {} needs your fingerprint or password at that moment, and only for it.", i + 1)),
            Gate::Refuse(why) => { lines = vec![why.clone()]; break; }
        }
    }
    if !matches!(gates.first(), Some(Gate::Refuse(_))) {
        lines.push(if plan.reversible { "Everything it changes can be undone.".into() }
                   else { "Some of it can't be undone once it's done.".into() });
    }
    Preview { lines, gates, replies: &[Reply::Yes, Reply::No, Reply::Change, Reply::Ask, Reply::Cancel] }
}

// MARK: - A proposed plan: read strictly, checked against the declarations

/// A plan as a model would write it. Nothing in it is trusted: not the tool,
/// not the input, not the order. There is no field for risk or authorization —
/// those are the runtime's, and a proposal carrying one is refused.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub goal: String,
    pub steps: Vec<ProposedStep>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedStep {
    pub tool: String,
    #[serde(default)]
    pub input: serde_json::Map<String, serde_json::Value>,
    /// Indices of EARLIER steps this one needs.
    #[serde(default)]
    pub after: Vec<usize>,
}

/// Why a proposal was not accepted. REJECTED problems mean the proposal is
/// wrong; BLOCKED ones mean it is sound but cannot run now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "problem", content = "detail")]
pub enum Problem {
    Unreadable(String),
    Empty,
    TooManySteps(usize),
    UnknownTool { step: usize, tool: String },
    MissingInput { step: usize, field: &'static str },
    UnexpectedInput { step: usize, field: String },
    WrongType { step: usize, field: &'static str, expected: FieldType },
    BadDependency { step: usize, on: usize },
    NoVerifier { step: usize },
    /// It depends on something no step and nothing KUE holds provides.
    Incomplete { step: usize, why: String },
    Unavailable { step: usize, why: String },
    NotAuthorized { step: usize, why: String },
    PrivacyDenied { step: usize, kind: DataKind },
    PreconditionFailed { step: usize, why: String },
}

impl Problem {
    /// Whether the proposal itself is wrong (REJECTED) rather than the moment
    /// (BLOCKED).
    pub fn rejects(&self) -> bool {
        !matches!(self, Problem::Unavailable { .. } | Problem::NotAuthorized { .. }
                        | Problem::PrivacyDenied { .. } | Problem::PreconditionFailed { .. })
    }
}

#[derive(Debug, Clone)]
pub enum Verdict {
    /// Every check passed. The plan, with the risk the RUNTIME assigned.
    Valid { plan: Plan, actions: Vec<Option<ActionKind>> },
    Rejected(Vec<Problem>),
    Blocked(Vec<Problem>),
}

/// Reads a proposal. Anything not in the shape above — an extra field, a
/// "risk", a "confirmed": true — is unreadable, not ignored.
pub fn parse(json: &str) -> Result<Proposal, Problem> {
    serde_json::from_str(json).map_err(|e| Problem::Unreadable(e.to_string()))
}

/// What the runtime knows that validation needs. Supplied by the caller from
/// live state; validation reads it and changes nothing.
pub struct Context<'a> {
    pub conditions: Conditions,
    /// The engine's preview of an operation: grants nothing, consumes nothing.
    pub authorization: &'a dyn Fn(Operation) -> crate::authz::Decision,
    /// Whether an action's targets pass the target policy (`Targets::check`).
    pub targets: &'a dyn Fn(&ActionKind) -> Result<ActionKind, String>,
    /// Whether the firewall would let `kind` go to `destination` now
    /// (`Firewall::would_allow`): asked, not recorded.
    pub privacy: &'a dyn Fn(DataKind, Destination) -> bool,
    /// The most recent storage check, if one exists.
    pub storage: Option<&'a crate::storage::StorageReport>,
    /// What KUE moved to the Trash and may put back.
    pub trashed: &'a [TrashedItem],
}

/// Checks a proposal against the declared tools and the moment. Every problem
/// is reported, not only the first.
pub fn validate(p: &Proposal, proposer: &str, plan_id: &str, now: f64, ctx: &Context) -> Verdict {
    let mut problems = Vec::new();
    if p.steps.is_empty() { return Verdict::Rejected(vec![Problem::Empty]); }
    if p.steps.len() > MAX_STEPS { return Verdict::Rejected(vec![Problem::TooManySteps(p.steps.len())]); }

    let mut steps = Vec::new();
    let mut kinds: Vec<Option<ActionKind>> = Vec::new();
    for (i, s) in p.steps.iter().enumerate() {
        for &on in &s.after {
            if on >= i { problems.push(Problem::BadDependency { step: i, on }); }
        }
        let Some(spec) = tools::declared(&s.tool) else {
            problems.push(Problem::UnknownTool { step: i, tool: s.tool.clone() });
            kinds.push(None);
            continue;
        };
        // The input, against the declared schema: nothing missing, nothing extra, every type right.
        for field in s.input.keys() {
            if !spec.input.iter().any(|f| f.name == field) {
                problems.push(Problem::UnexpectedInput { step: i, field: field.clone() });
            }
        }
        for f in spec.input {
            match s.input.get(f.name) {
                None if f.required => problems.push(Problem::MissingInput { step: i, field: f.name }),
                None => {}
                Some(v) if !typed(v, f.ty) => problems.push(Problem::WrongType { step: i, field: f.name, expected: f.ty }),
                Some(_) => {}
            }
        }
        if spec.verifier.trim().is_empty() { problems.push(Problem::NoVerifier { step: i }); }
        let kind = action_of(spec.id, &s.input, ctx);

        // The moment: can it run, may it be authorized, may its data go where it goes.
        if let tools::Availability::Unavailable(why) = spec.availability(ctx.conditions) {
            problems.push(Problem::Unavailable { step: i, why });
        }
        if let Some(op) = spec.operation() {
            if let crate::authz::Decision::Deny(why) = (ctx.authorization)(op) {
                problems.push(Problem::NotAuthorized { step: i, why });
            }
        }
        for &(kind, dest) in clearances(spec.id) {
            if !(ctx.privacy)(kind, dest) {
                problems.push(Problem::PrivacyDenied { step: i, kind });
            }
        }
        // Its preconditions, against what earlier steps provide and what KUE holds.
        let earlier: Vec<&str> = p.steps[..i].iter().map(|x| x.tool.as_str()).collect();
        for pre in spec.preconditions {
            match pre {
                tools::Precondition::OfferedByStorageCheck => {
                    let paths = match &kind { Some(ActionKind::MoveToTrash { paths }) => paths.clone(), _ => Vec::new() };
                    let checked_first = earlier.contains(&"INSPECT_STORAGE");
                    let offered = ctx.storage.is_some_and(|r| !paths.is_empty() && r.offered(&paths));
                    if !checked_first && !offered {
                        problems.push(Problem::Incomplete { step: i,
                            why: "It moves files no storage check offered, and no step checks first.".into() });
                    }
                    if paths.len() > actions::MAX_TRASHED {
                        problems.push(Problem::PreconditionFailed { step: i,
                            why: format!("More than {} files at once.", actions::MAX_TRASHED) });
                    }
                }
                tools::Precondition::PreviouslyTrashedByKue => {
                    if matches!(&kind, Some(ActionKind::RestoreFromTrash { items }) if items.is_empty()) {
                        problems.push(Problem::PreconditionFailed { step: i,
                            why: "Nothing named was moved to the Trash by KUE.".into() });
                    }
                }
                _ => {}
            }
        }
        // The target policy, for what can be checked before it runs.
        if let Some(k) = &kind {
            let needs_check = !matches!(k, ActionKind::MoveToTrash { .. } | ActionKind::RestoreFromTrash { .. } | ActionKind::InspectStorage);
            if needs_check {
                if let Err(why) = (ctx.targets)(k) { problems.push(Problem::PreconditionFailed { step: i, why }); }
            }
        }

        let risk = spec.risk();
        let confirmation = match risk {
            None | Some(Risk::Low) => Confirmation::None,
            Some(Risk::Medium) => Confirmation::Owner,
            Some(Risk::High | Risk::Critical) => Confirmation::OwnerAndMacos,
        };
        steps.push(PlanStep {
            index: i, step: spec.id, tool: Some(spec.id), state: StepState::Pending, waiting: None,
            depends_on: s.after.clone(), risk,
            authority: spec.operation().map(|operation| Authority::Operation { operation }).unwrap_or(Authority::NoneNeeded),
            confirmation, verification: spec.verifier, rollback: spec.rollback, said: None,
        });
        kinds.push(kind);
    }

    if problems.iter().any(Problem::rejects) {
        return Verdict::Rejected(problems.into_iter().filter(Problem::rejects).collect());
    }
    if !problems.is_empty() { return Verdict::Blocked(problems); }
    let plan = summarize(plan_id.to_string(), None, now, steps, Some(0), PlanState::Planned,
                         Proposer::Model(proposer.to_string()), Vec::new());
    Verdict::Valid { plan, actions: kinds }
}

/// What the runtime clears through the firewall when this tool runs — the
/// real clearance points, not the kinds a tool is described as touching (a
/// link is never collected as such; the action card carries it as the
/// action's target). Kept beside `transaction::take_stock` and
/// `transaction::for_interface`, which make these clearances.
fn clearances(tool: &str) -> &'static [(DataKind, Destination)] {
    const RECORD: &[(DataKind, Destination)] = &[(DataKind::ActionTarget, Destination::Interface)];
    const STORAGE: &[(DataKind, Destination)] = &[(DataKind::ActionTarget, Destination::Interface),
        (DataKind::StorageInventory, Destination::Interface), (DataKind::StorageSummary, Destination::Interface)];
    match tool {
        "INSPECT_STORAGE" => STORAGE,
        "CALCULATE" => &[(DataKind::OwnerMessage, Destination::Interface)],
        _ => RECORD,
    }
}

fn typed(v: &serde_json::Value, ty: FieldType) -> bool {
    use serde_json::Value;
    match ty {
        FieldType::Text | FieldType::Path | FieldType::Name | FieldType::Url | FieldType::Sentence =>
            matches!(v, Value::String(s) if !s.trim().is_empty()),
        FieldType::Paths => matches!(v, Value::Array(a) if a.iter().all(|x| matches!(x, Value::String(s) if !s.trim().is_empty()))),
        FieldType::Number => v.is_number(),
        FieldType::Bool => v.is_boolean(),
        // KUE writes reports; nobody hands one in.
        FieldType::Report => false,
    }
}

/// The action a validated step would be, built by the runtime from the input.
fn action_of(id: &str, input: &serde_json::Map<String, serde_json::Value>, ctx: &Context) -> Option<ActionKind> {
    let text = |k: &str| input.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let paths = |k: &str| input.get(k).and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>());
    Some(match id {
        "INSPECT_STORAGE" => ActionKind::InspectStorage,
        "MOVE_TO_TRASH" => ActionKind::MoveToTrash { paths: paths("paths")? },
        // Only what KUE itself moved can be put back, and the runtime supplies
        // where it went: a proposal names only where it came from.
        "RESTORE_FROM_TRASH" => {
            let wanted = paths("items")?;
            ActionKind::RestoreFromTrash { items: ctx.trashed.iter().filter(|t| wanted.contains(&t.original)).cloned().collect() }
        }
        "OPEN_APPLICATION" => ActionKind::OpenApplication { name: text("name")? },
        "CLOSE_APPLICATION" => ActionKind::CloseApplication { name: text("name")? },
        "FOCUS_APPLICATION" => ActionKind::FocusApplication { name: text("name")? },
        "OPEN_URL" => ActionKind::OpenUrl { url: text("url")? },
        "OPEN_DOCUMENT" => ActionKind::OpenDocument { query: text("query")?, path: text("path") },
        "OPEN_DIRECTORY" => ActionKind::OpenDirectory { query: text("query")?, scope: text("scope"), path: None },
        "LIST_DIRECTORY" => ActionKind::ListDirectory { query: text("query")?, filter: text("filter").unwrap_or_default(), scope: None, path: None },
        "CREATE_DIRECTORY" => ActionKind::CreateDirectory { path: text("path")? },
        "CREATE_FILE" => ActionKind::CreateFile { path: text("path")?, text: text("text").unwrap_or_default() },
        "READ_PERMITTED_FILE" => ActionKind::ReadPermittedFile { path: text("path")? },
        "MOVE_PERMITTED_FILE" => ActionKind::MovePermittedFile { from: text("from")?, to: text("to")? },
        "SHOW_NOTIFICATION" => ActionKind::ShowNotification { title: text("title").unwrap_or_else(|| "KUE".into()), body: text("body")? },
        _ => return None,
    })
}

// MARK: - Changing a plan the owner has not agreed to

/// Why a change could not be applied. When one of these comes back the plan
/// is left exactly as it was: not narrowed, not dropped, still waiting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "refused", content = "detail")]
pub enum EditRefused {
    /// The owner named a step number the plan does not have (1-based).
    NoSuchStep(usize),
    /// Taking those steps out would leave no plan at all.
    NothingLeft,
    /// Nothing in the plan writes anywhere, so it has no destination.
    NoDestination,
    /// The plan writes in more than one place, and which was meant is a guess.
    Ambiguous,
    /// The name given is not one KUE may write to.
    BadName(String),
}

/// The proposal text for a changed plan.
///
/// These functions produce a PROPOSAL, not a plan: what comes back goes
/// through `parse` and `validate` again exactly as the model's own text did,
/// and the plan it becomes has its own identity and needs its own yes. An
/// edit can only take steps out or move where the plan writes — there is no
/// way here to add a step, a tool, or an input the owner was never shown.
pub fn text_of(p: &Proposal) -> String {
    serde_json::to_string(p).unwrap_or_default()
}

/// Takes steps out by the positions they hold now, and renumbers what the
/// steps that remain depend on. A step that waited for a dropped one simply
/// stops naming it: the dependency was an order to run in, and the thing it
/// waited for is not going to happen.
pub fn without_steps(p: &Proposal, drop: &[usize]) -> Result<Proposal, EditRefused> {
    if let Some(&bad) = drop.iter().find(|&&i| i >= p.steps.len()) {
        return Err(EditRefused::NoSuchStep(bad + 1));
    }
    let keep: Vec<usize> = (0..p.steps.len()).filter(|i| !drop.contains(i)).collect();
    if keep.is_empty() { return Err(EditRefused::NothingLeft); }
    let moved: Vec<Option<usize>> = (0..p.steps.len())
        .map(|old| keep.iter().position(|&k| k == old)).collect();
    let steps = keep.iter().map(|&old| {
        let s = &p.steps[old];
        ProposedStep {
            tool: s.tool.clone(),
            input: s.input.clone(),
            after: s.after.iter().filter_map(|&a| moved.get(a).copied().flatten()).collect(),
        }
    }).collect();
    Ok(Proposal { goal: p.goal.clone(), steps })
}

/// Every path a proposal writes or reads, as (step, field, value, is a folder
/// the plan makes). Only the fields the DECLARATIONS call a path: a plan
/// cannot invent one.
fn path_fields(p: &Proposal) -> Vec<(usize, &'static str, String, bool)> {
    let mut found = Vec::new();
    for (i, s) in p.steps.iter().enumerate() {
        let Some(spec) = tools::declared(&s.tool) else { continue };
        let makes_folder = s.tool == "CREATE_DIRECTORY";
        for f in spec.input.iter().filter(|f| f.ty == tools::FieldType::Path) {
            if let Some(v) = s.input.get(f.name).and_then(|v| v.as_str()) {
                found.push((i, f.name, v.to_string(), makes_folder));
            }
        }
    }
    found
}

/// The folders a path sits in. A folder the plan MAKES counts as one of them:
/// "Reports" and "Reports/note.txt" are both inside Reports, which is what
/// makes Reports the plan's destination.
fn folders_of(value: &str, makes_folder: bool) -> Vec<&str> {
    let parts: Vec<&str> = value.split('/').collect();
    if makes_folder { parts } else { parts[..parts.len().saturating_sub(1)].to_vec() }
}

/// Moves everything the plan writes to a different place inside KUE's folder.
///
/// The destination is the one folder every path in the plan sits under. When
/// the plan makes that folder itself, changing the destination renames it —
/// which is what "put it in Archive instead" means for a plan that was going
/// to make Reports. When the paths sit nowhere in common, everything the plan
/// writes goes inside the new folder, keeping its shape.
///
/// Where that cannot be worked out — paths spelled out in full that share no
/// folder — this refuses rather than choosing: guessing which of someone's
/// folders they meant is the mistake not to make. Whatever comes back is a
/// PROPOSAL, checked again from the beginning, and `resolve` still decides
/// where each name really lands.
pub fn into_destination(p: &Proposal, dest: &str) -> Result<Proposal, EditRefused> {
    let name = dest.trim().trim_end_matches('/');
    if name.is_empty() || name.starts_with('/') || name.starts_with('~')
        || name.split('/').any(|c| c == ".." || c == "." || c.is_empty())
        || name.chars().any(|c| c.is_control()) {
        return Err(EditRefused::BadName(dest.trim().to_string()));
    }
    let paths = path_fields(p);
    if paths.is_empty() { return Err(EditRefused::NoDestination); }

    // The longest run of folders every path shares.
    let mut common: Vec<String> = folders_of(&paths[0].2, paths[0].3).iter().map(|s| s.to_string()).collect();
    for (_, _, value, makes_folder) in &paths[1..] {
        let mine = folders_of(value, *makes_folder);
        let shared = common.iter().zip(mine.iter()).take_while(|(a, b)| a.as_str() == **b).count();
        common.truncate(shared);
    }
    let absolute = paths.iter().any(|(_, _, v, _)| v.starts_with('/'));
    // Nothing in common: everything goes inside the new folder as it is. A
    // path spelled out in full has no "as it is" to keep, so that is refused.
    if common.iter().all(|c| c.is_empty()) {
        if absolute { return Err(EditRefused::Ambiguous); }
        common.clear();
    }

    let mut out = p.clone();
    for (i, field, value, makes_folder) in paths {
        let parts: Vec<&str> = value.split('/').collect();
        let rest = parts[common.len().min(parts.len())..].join("/");
        let _ = makes_folder;
        let moved = if rest.is_empty() { name.to_string() } else { format!("{name}/{rest}") };
        out.steps[i].input.insert(field.to_string(), serde_json::Value::String(moved));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz::Decision as Authz;

    fn ctx<'a>(authorization: &'a dyn Fn(Operation) -> Authz, targets: &'a dyn Fn(&ActionKind) -> Result<ActionKind, String>) -> Context<'a> {
        Context { conditions: Conditions { killed: false, paused: false, executor_present: true },
                  authorization, targets, privacy: &policy, storage: None, trashed: &[] }
    }
    fn policy(kind: DataKind, dest: Destination) -> bool { crate::privacy::Firewall::new().would_allow(kind, dest) }
    fn allow(_: Operation) -> Authz { Authz::Allow }
    fn any_target(k: &ActionKind) -> Result<ActionKind, String> { Ok(k.clone()) }

    fn check(json: &str) -> Verdict {
        let p = parse(json).expect("readable");
        validate(&p, "test-model", "p1", 1.0, &ctx(&allow, &any_target))
    }

    /// The text of a changed plan is what gets checked again at approval, so
    /// what a proposal means must survive being written back out. If these
    /// ever drift, the plan the owner saw and the plan that runs could differ
    /// — which is the one thing this whole spine exists to prevent.
    #[test]
    fn a_proposal_written_back_out_means_the_same_thing() {
        for json in [
            r#"{"goal":"notes","steps":[{"tool":"CREATE_DIRECTORY","input":{"path":"Reports"}},
                {"tool":"CREATE_FILE","input":{"path":"Reports/a.txt","text":"hi"},"after":[0]}]}"#,
            r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE"}]}"#,
            r#"{"goal":"unicode ✓","steps":[{"tool":"SHOW_NOTIFICATION","input":{"body":"done ✓","title":"KUE"}}]}"#,
        ] {
            let once = parse(json).expect("readable");
            let twice = parse(&text_of(&once)).expect("what KUE writes, KUE can read");
            assert_eq!(once, twice, "{json}");
        }
    }

    /// Taking a step out renumbers what the steps after it wait for. A step
    /// that waited for the one removed stops naming it: the thing it waited
    /// for is not going to happen.
    #[test]
    fn taking_a_step_out_renumbers_what_the_rest_wait_for() {
        let p = parse(r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE"},
            {"tool":"CREATE_DIRECTORY","input":{"path":"a"},"after":[0]},
            {"tool":"CREATE_FILE","input":{"path":"a/b.txt","text":"hi"},"after":[0,1]}]}"#).unwrap();

        let without_first = without_steps(&p, &[0]).unwrap();
        assert_eq!(without_first.steps.len(), 2);
        assert_eq!(without_first.steps[0].after, Vec::<usize>::new(), "it waited for a step that is gone");
        assert_eq!(without_first.steps[1].after, vec![0], "and the one it still waits for moved up");

        let without_middle = without_steps(&p, &[1]).unwrap();
        assert_eq!(without_middle.steps[1].after, vec![0], "the step that remains keeps the dependency it kept");

        // Every remaining dependency points backwards, which is what the
        // validator requires of any proposal.
        for out in [&without_first, &without_middle] {
            for (i, s) in out.steps.iter().enumerate() {
                assert!(s.after.iter().all(|&a| a < i), "step {i} waits for {:?}", s.after);
            }
        }
        assert_eq!(without_steps(&p, &[0, 1, 2]), Err(EditRefused::NothingLeft));
        assert_eq!(without_steps(&p, &[7]), Err(EditRefused::NoSuchStep(8)));
    }

    /// Where a plan writes can be changed; what it writes cannot be invented.
    #[test]
    fn a_destination_moves_every_path_together_or_asks() {
        let notes = parse(r#"{"goal":"x","steps":[{"tool":"CREATE_DIRECTORY","input":{"path":"Reports"}},
            {"tool":"CREATE_FILE","input":{"path":"Reports/a.txt","text":"hi"},"after":[0]}]}"#).unwrap();
        let moved = into_destination(&notes, "Archive").unwrap();
        assert_eq!(moved.steps[0].input["path"], "Archive");
        assert_eq!(moved.steps[1].input["path"], "Archive/a.txt");
        assert_eq!(moved.steps[1].input["text"], "hi", "what it writes is untouched");
        assert_eq!(moved.steps[1].after, vec![0], "and so is the order");

        // Spelled out in full, the same plan moves the same way.
        let full = parse(r#"{"goal":"x","steps":[{"tool":"CREATE_DIRECTORY","input":{"path":"/Users/x/KUE/Reports"}},
            {"tool":"CREATE_FILE","input":{"path":"/Users/x/KUE/Reports/a.txt","text":"hi"},"after":[0]}]}"#).unwrap();
        let moved = into_destination(&full, "Archive").unwrap();
        assert_eq!(moved.steps[0].input["path"], "Archive");
        assert_eq!(moved.steps[1].input["path"], "Archive/a.txt");

        // Nowhere to put it, nowhere it agrees on, and names KUE may not write.
        let no_paths = parse(r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE"}]}"#).unwrap();
        assert_eq!(into_destination(&no_paths, "Archive"), Err(EditRefused::NoDestination));
        let scattered = parse(r#"{"goal":"x","steps":[{"tool":"CREATE_FILE","input":{"path":"/a/x.txt","text":"1"}},
            {"tool":"CREATE_FILE","input":{"path":"/b/y.txt","text":"2"}}]}"#).unwrap();
        assert_eq!(into_destination(&scattered, "Archive"), Err(EditRefused::Ambiguous));
        for bad in ["/etc", "~/Desktop", "../..", "", "  ", "a/../../b"] {
            assert!(matches!(into_destination(&notes, bad), Err(EditRefused::BadName(_))), "{bad:?} was allowed");
        }
    }

    #[test]
    fn a_valid_proposal_gets_the_runtimes_risk_not_its_own() {
        let v = check(r#"{"goal":"clean up","steps":[{"tool":"INSPECT_STORAGE"},
            {"tool":"MOVE_TO_TRASH","input":{"paths":["/Users/x/Downloads/a.dmg"]},"after":[0]}]}"#);
        let Verdict::Valid { plan, .. } = v else { panic!("{v:?}") };
        assert_eq!(plan.risk, Some(Risk::High), "risk is actions::risk, whatever was proposed");
        assert_eq!(plan.required_authorization, Some(Operation::ActionHighRisk));
        assert_eq!(plan.proposed_by, Proposer::Model("test-model".into()));
        assert!(plan.approved_by.is_empty(), "a proposal is approved by nobody");
        assert_eq!(approval(&plan), vec![Gate::ConfirmAtStep(1)]);
        assert!(plan.reversible);
    }

    #[test]
    fn o_a_tool_that_does_not_exist_is_rejected() {
        let Verdict::Rejected(p) = check(r#"{"goal":"x","steps":[{"tool":"DELETE_EVERYTHING"}]}"#) else { panic!() };
        assert_eq!(p, vec![Problem::UnknownTool { step: 0, tool: "DELETE_EVERYTHING".into() }]);
        // Things KUE genuinely cannot do are not declared, so they are unknown tools too.
        for tool in ["WEB_SEARCH", "SEND_MESSAGE", "TYPE_TEXT", "CLICK", "RUN_SHELL"] {
            let v = check(&format!(r#"{{"goal":"x","steps":[{{"tool":"{tool}"}}]}}"#));
            assert!(matches!(v, Verdict::Rejected(_)), "{tool}");
        }
    }

    #[test]
    fn p_an_action_that_cannot_be_authorized_is_blocked_not_run() {
        let deny = |_: Operation| Authz::Deny("identity is uncertain".into());
        let p = parse(r#"{"goal":"x","steps":[{"tool":"OPEN_APPLICATION","input":{"name":"Safari"}}]}"#).unwrap();
        let v = validate(&p, "m", "p1", 1.0, &ctx(&deny, &any_target));
        let Verdict::Blocked(problems) = v else { panic!("{v:?}") };
        assert_eq!(problems, vec![Problem::NotAuthorized { step: 0, why: "identity is uncertain".into() }]);
    }

    #[test]
    fn q_an_incomplete_plan_is_rejected() {
        // Moves files without a storage check first, and none exists.
        let Verdict::Rejected(p) = check(r#"{"goal":"x","steps":[{"tool":"MOVE_TO_TRASH","input":{"paths":["/a"]}}]}"#) else { panic!() };
        assert!(matches!(p[0], Problem::Incomplete { step: 0, .. }), "{p:?}");
        // A required input missing, an extra one, a wrong type.
        let Verdict::Rejected(p) = check(r#"{"goal":"x","steps":[{"tool":"OPEN_URL","input":{"link":"https://a.b"}}]}"#) else { panic!() };
        assert!(p.contains(&Problem::UnexpectedInput { step: 0, field: "link".into() }));
        assert!(p.contains(&Problem::MissingInput { step: 0, field: "url" }));
        let Verdict::Rejected(p) = check(r#"{"goal":"x","steps":[{"tool":"MOVE_TO_TRASH","input":{"paths":"/a"}}]}"#) else { panic!() };
        assert!(p.contains(&Problem::WrongType { step: 0, field: "paths", expected: FieldType::Paths }));
        // A dependency on itself or on a later step.
        let Verdict::Rejected(p) = check(r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE","after":[0]}]}"#) else { panic!() };
        assert_eq!(p, vec![Problem::BadDependency { step: 0, on: 0 }]);
        assert!(matches!(check(r#"{"goal":"x","steps":[]}"#), Verdict::Rejected(p) if p == vec![Problem::Empty]));
    }

    #[test]
    fn a_proposal_cannot_carry_its_own_risk_or_approval() {
        for json in [r#"{"goal":"x","risk":"LOW","steps":[{"tool":"INSPECT_STORAGE"}]}"#,
                     r#"{"goal":"x","steps":[{"tool":"MOVE_TO_TRASH","confirmed":true,"input":{"paths":["/a"]}}]}"#,
                     r#"{"goal":"x","approved_by":"OWNER","steps":[{"tool":"INSPECT_STORAGE"}]}"#] {
            assert!(matches!(parse(json), Err(Problem::Unreadable(_))), "{json}");
        }
    }

    #[test]
    fn a_step_whose_data_the_firewall_refuses_is_blocked_and_asking_records_nothing() {
        let mut fw = crate::privacy::Firewall::new();
        fw.refuse_additionally(DataKind::StorageInventory, Destination::Interface);
        let before = fw.totals();
        let privacy = |k: DataKind, d: Destination| fw.would_allow(k, d);
        let p = parse(r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE"}]}"#).unwrap();
        let cx = Context { privacy: &privacy, ..ctx(&allow, &any_target) };
        let Verdict::Blocked(problems) = validate(&p, "m", "p1", 1.0, &cx) else { panic!() };
        assert_eq!(problems, vec![Problem::PrivacyDenied { step: 0, kind: DataKind::StorageInventory }]);
        assert_eq!(fw.totals(), before, "validating a plan is not a clearance and is not recorded as one");
    }

    #[test]
    fn nothing_proposed_runs_while_killed_or_paused() {
        let p = parse(r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE"}]}"#).unwrap();
        for c in [Conditions { killed: true, paused: false, executor_present: true },
                  Conditions { killed: false, paused: true, executor_present: true }] {
            let cx = Context { conditions: c, ..ctx(&allow, &any_target) };
            assert!(matches!(validate(&p, "m", "p1", 1.0, &cx), Verdict::Blocked(_)));
        }
    }

    #[test]
    fn the_gates_follow_risk_and_reversibility_and_are_never_fewer_than_the_steps_own() {
        let plan = |json: &str| match check(json) { Verdict::Valid { plan, .. } => plan, v => panic!("{v:?}") };
        // Safe and reversible: it proceeds.
        assert_eq!(approval(&plan(r#"{"goal":"x","steps":[{"tool":"INSPECT_STORAGE"},{"tool":"OPEN_APPLICATION","input":{"name":"Notes"}}]}"#)),
                   vec![Gate::Proceed]);
        // Medium: one yes for the whole plan.
        assert_eq!(approval(&plan(r#"{"goal":"x","steps":[{"tool":"CREATE_DIRECTORY","input":{"path":"~/KUE/a"}},
            {"tool":"CREATE_FILE","input":{"path":"~/KUE/a/b.txt","text":"hi"},"after":[0]}]}"#)), vec![Gate::ConfirmPlan]);
        // Medium and high: the plan-level yes, AND the high step's own confirmation.
        let g = approval(&plan(r#"{"goal":"x","steps":[{"tool":"OPEN_URL","input":{"url":"https://apple.com"}},
            {"tool":"CLOSE_APPLICATION","input":{"name":"Notes"}}]}"#));
        assert_eq!(g, vec![Gate::ConfirmPlan, Gate::ConfirmAtStep(1)]);
        let pv = preview(&plan(r#"{"goal":"x","steps":[{"tool":"OPEN_URL","input":{"url":"https://apple.com"}}]}"#));
        assert!(pv.lines.iter().any(|l| l == "Some of it can't be undone once it's done."), "{:?}", pv.lines);
        assert_eq!(pv.replies, &[Reply::Yes, Reply::No, Reply::Change, Reply::Ask, Reply::Cancel]);
    }

    #[test]
    fn a_kue_goal_reads_as_a_plan_without_a_second_copy() {
        let mut g = Goal::new("g1".into(), "VOICE", goal::Blueprint::clean_up_storage(), 5.0);
        let plan = Plan::of(&g);
        assert_eq!(plan.plan_id, "g1");
        assert_eq!(plan.proposed_by, Proposer::Kue("CLEAN_UP_STORAGE"));
        assert_eq!(plan.state, PlanState::Planned);
        assert_eq!(plan.steps.len(), 6);
        assert_eq!(plan.risk, Some(Risk::High), "the move to the Trash");
        assert_eq!(plan.steps[4].tool, Some("MOVE_TO_TRASH"));
        assert_eq!(plan.steps[4].rollback, Rollback::By("RESTORE_FROM_TRASH"));
        assert!(plan.steps[4].depends_on.contains(&0), "the move depends on the storage check");
        assert!(plan.reversible);
        let gates = approval(&plan);
        assert_eq!(gates, vec![Gate::ConfirmAtStep(4)]);
        // A step that ran and whose read-back did not match is VERIFICATION_FAILED, never done.
        use goal::StepEvent as E;
        for e in [E::PreconditionsMet, E::Authorized, E::Observed("walked".into()), E::VerificationFailed("totals disagree".into())] {
            g.apply(0, e, 6.0).unwrap();
        }
        g.block_after(0, 6.0);
        let plan = Plan::of(&g);
        assert_eq!(plan.steps[0].state, StepState::VerificationFailed);
        assert_eq!(plan.steps[1].state, StepState::Skipped);
        assert_eq!(plan.state, PlanState::Stopped);
        assert_ne!(plan.state, PlanState::Completed);
    }
}
