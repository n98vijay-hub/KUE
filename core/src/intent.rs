//! What the owner wants, as a type — decided by rule, before anything runs.
//!
//!   "Computer, check my storage."
//!     → STORAGE_ANALYZE · VOICE · invocation "computer" · LOCAL_CAPABILITY
//!       · capability storage_inspection · operation ACTION_LOW_RISK
//!       · executed LOCAL · verification REQUIRED
//!
//! The router decides WHAT is wanted. It does not decide whether it may be
//! done (the engine does, per operation, when the step runs), and it does not
//! do it (the transaction and the Action Broker do). Classifying reads only the
//! sentence: no folder, no file, no model, no network.
//!
//! Not `router.rs`. That one decides which model may answer a question; this
//! one decides whether a model is needed at all.
//!
//! Order, and why:
//!   1. the safety boundary — a request to weaken KUE is refused before
//!      anything else looks at it;
//!   2. the command parser (`task::plan`) — the 15 allowlisted actions, and
//!      plans of them;
//!   3. requests KUE understands but answers by rule: arithmetic, the
//!      capability list, and every kind of request it cannot carry out (web
//!      research, purchases, messages, deletion, typing into apps) — which a
//!      model would otherwise answer as if it had done them;
//!   4. questions, for the model.
//!
//! Unknown operation = DENY: a sentence that starts with a verb that acts on
//! the world, and that no rule above recognises, is UNKNOWN and answered by
//! rule — nothing done, no model.

use crate::actions::{self, ActionKind, Risk};
use crate::authz::{Decision as AuthDecision, Operation};
use crate::capabilities;
use crate::context::CapabilityStatus;
use crate::goal::{self, Authority, Blueprint};
use crate::safety::{self, Refusal};
use crate::task::{self, Plan};
use crate::voice::InputSource;
use serde::Serialize;

/// Shown as the `model` of an answer the router wrote by rule.
pub const INTENT_ROUTER_SOURCE: &str = "KUE's intent router · no model";
/// Shown as the `model` of a calculation.
pub const ARITHMETIC_SOURCE: &str = "KUE's arithmetic · no model";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntentKind {
    OpenApplication,
    QuitApplication,
    SwitchApplication,
    OpenWebsite,
    OpenFolder,
    ListFolder,
    FindDocument,
    CreateFolder,
    CreateFile,
    ReadFile,
    MoveFile,
    ShowNotification,
    StorageStatus,
    StorageAnalyze,
    StorageExplain,
    StorageCleanup,
    /// "Clean my computer": a cleanup whose object is missing.
    CleanUpUnspecified,
    UndoTrash,
    Calculate,
    SystemStatus,
    ContextQuery,
    MemoryQuery,
    /// Asking KUE to remind the owner of something. Not implemented.
    Reminder,
    /// Asking about the owner's calendar. Not implemented.
    CalendarQuery,
    CapabilityQuery,
    WebResearch,
    WebComparison,
    ComputerTask,
    DeleteFiles,
    Purchase,
    SendMessage,
    /// Several commands in one sentence.
    MultiStep,
    /// A command verb with nothing to act on: "Can you open?"
    IncompleteCommand,
    /// A question for the model.
    Conversation,
    /// A request to act that no rule recognises.
    Unknown,
}

impl IntentKind {
    pub fn tag(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Category {
    /// Answered by a rule, with nothing sensed or acted on.
    Deterministic,
    /// One of KUE's own capabilities on this Mac.
    LocalCapability,
    /// Needs a language model.
    ModelReasoning,
    /// Needs the internet.
    WebResearch,
    /// Needs to act inside another app or a website.
    ComputerInteraction,
    /// Changes something, spends something or speaks for the owner: it needs a
    /// confirmation or authentication beyond being recognised.
    AuthorizationSensitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Execution {
    /// In KUE's own process.
    Local,
    /// Through the Action Broker's executor process (KueAct).
    ActionBroker,
    /// The on-device model, through the model router and the privacy firewall.
    OnDeviceModel,
    /// Nothing runs.
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationNeed {
    /// The result is checked before it is reported as done.
    Required,
    /// A model's answer cannot be verified; it is checked for claims KUE cannot back.
    NotApplicable,
    /// Nothing runs, so there is nothing to verify.
    Nothing,
}

/// Whether a routed intent can be carried out now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "reason", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RouteStatus {
    Available,
    Partial,
    NotImplemented,
    Denied(String),
    RequiresAuthentication,
}

/// What happens next.
#[derive(Debug, Clone, PartialEq)]
pub enum Work {
    /// One allowlisted action, through the transaction.
    Act(ActionKind),
    /// A plan of steps, through the transaction, step by step.
    Goal(Blueprint),
    /// Answered by rule, without a model.
    Answer { sentence: String, source: &'static str, verification: Option<String> },
    /// The answer is the capability list, which the shell reads from the registry.
    CapabilityList,
    /// KUE needs something only the owner can say before it can plan. When a
    /// goal applies, it is opened and waits.
    Ask { sentence: String, goal: Option<Blueprint> },
    /// A question for the model.
    Model,
    /// A request to DO something that no rule of KUE's can carry out as it
    /// stands. The model is asked for a PLAN — never for an answer, and never
    /// to act: what comes back is a proposal, checked against the declared
    /// tools before the owner is shown anything (`plan::validate`).
    ModelPlan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    pub kind: IntentKind,
    pub source: InputSource,
    /// The invocation that preceded the request, when there was one.
    pub invocation: Option<String>,
    /// The first is the primary one.
    pub categories: Vec<Category>,
    /// The registry row this intent depends on. None: answered by rule from KUE itself.
    pub capability: Option<&'static str>,
    /// What the first act needs from the engine. Not a grant.
    pub operation: Option<Operation>,
    pub execution: Execution,
    pub verification: VerificationNeed,
    pub work: Work,
}

/// An intent without its words or targets: safe to show and to record.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntentSummary {
    pub kind: IntentKind,
    pub source: InputSource,
    pub invocation: Option<String>,
    pub categories: Vec<Category>,
    pub capability: Option<&'static str>,
    pub operation: Option<Operation>,
    pub execution: Execution,
    pub verification: VerificationNeed,
    pub status: RouteStatus,
}

pub enum Understanding {
    /// The safety boundary refused it. Nothing else looked at it.
    Refused(Refusal),
    Understood(Intent),
}

// MARK: - Words

const POLITE: [&str; 17] = ["please ", "can you ", "could you ", "would you ", "will you ", "i want you to ", "i need you to ",
    "go ahead and ", "just ", "hey ", "ok ", "okay ", "computer, ", "kue, ", "kue ", "lantern, ", "lantern "];
const PRONOUNS: [&str; 10] = ["it", "that", "this", "them", "those", "these", "one", "that one", "this one", "the one"];
const DELETE: [&str; 8] = ["delete", "erase", "remove", "wipe", "shred", "trash", "uninstall", "destroy"];
const PURCHASE: [&str; 9] = ["buy", "purchase", "order", "book", "pay", "checkout", "reserve", "subscribe", "rent"];
const SEND: [&str; 9] = ["send", "email", "e-mail", "text", "message", "reply", "forward", "tweet", "post"];
const IN_APP: [&str; 12] = ["type", "click", "press", "scroll", "fill", "submit", "select", "drag", "paste", "copy", "log", "sign"];
/// Verbs that act on the world. A sentence that begins with one and matches no
/// rule is UNKNOWN, and nothing is done.
const ACTING: [&str; 26] = ["install", "download", "upload", "restart", "reboot", "shut", "shutdown", "rename", "print", "share",
    "schedule", "set", "turn", "enable", "disable", "mute", "unmute", "change", "update", "connect", "disconnect", "format",
    "backup", "sync", "record", "empty"];
const WEB_PHRASES: [&str; 14] = ["search the web", "search the internet", "search online", "look up", "look it up", "google ",
    "on the internet", "online for", "find online", "latest news", "the news", "weather", "stock price", "news about"];
const WEB_TOPICS: [&str; 6] = ["flight", "flights", "hotel", "hotels", "airfare", "tickets"];
const COMPARISON: [&str; 8] = ["cheapest", "lowest price", "best price", "best deal", "compare prices", "price of", "prices", "compare"];
/// Asking KUE to remember a time, or to tell the owner at one. KUE has no
/// reminders and no calendar, and this is how it says so in milliseconds
/// instead of after thirty seconds of a model deciding the same thing.
const REMINDER: [&str; 10] = ["remind me", "remind her", "remind him", "set a reminder", "put a reminder",
    "add a reminder", "make a reminder", "reminder for", "reminder to", "remind us"];
const CALENDAR: [&str; 12] = ["my calendar", "the calendar", "on my schedule", "my schedule", "calendar event",
    "add an event", "schedule a meeting", "my meetings", "my next meeting", "book a meeting",
    "put it in my calendar", "put this on my calendar"];
const MEMORY: [&str; 9] = ["what did i ", "do you remember", "remember when", "remember that", "remember what", "what was i doing",
    "last time i", "what have i been", "remember this"];
const SYSTEM: [&str; 10] = ["battery", "thermal", "overheating", "too hot", "running hot", "cpu", "low power",
    "how is my mac", "how's my mac", "how is my computer"];
const CONTEXT: [&str; 10] = ["what app", "which app", "what am i doing", "am i at", "who is here", "is anyone here",
    "what do you see", "can you see me", "do you recognise me", "do you recognize me"];
const STORAGE_WORDS: [&str; 6] = ["storage", "disk", "drive", "space", "hard drive", "ssd"];
/// Asking KUE to put the owner's things in order: the verbs, and what they act
/// on. Both are needed — "sort out my thoughts" is not work on this Mac, and
/// "my files" on its own is not a request.
const TIDY_VERBS: [&str; 14] = ["organize", "organise", "tidy", "sort", "arrange", "declutter", "file away",
    "clean up", "clear out", "group", "rearrange", "reorganize", "reorganise", "make room"];
const OWNED_THINGS: [&str; 16] = ["file", "files", "folder", "folders", "desktop", "downloads", "documents",
    "screenshot", "screenshots", "photo", "photos", "pdf", "pdfs", "notes", "installers", "duplicates"];
const COMPUTER_WORDS: [&str; 7] = ["computer", "mac", "macbook", "laptop", "system", "pc", "machine"];

fn normalise(text: &str) -> String {
    let t: String = text.to_lowercase().chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' || c == '%' || c == '-' { c } else { ' ' }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn without_polite(mut t: &str) -> &str {
    loop {
        let before = t;
        for p in POLITE { if let Some(r) = t.strip_prefix(p) { t = r.trim_start(); } }
        if t == before { return t; }
    }
}

/// The clauses of a sentence, each without its polite lead-in.
fn clauses(t: &str) -> Vec<String> {
    t.split(" and then ").flat_map(|c| c.split(" then ")).flat_map(|c| c.split(" and "))
        .map(|c| without_polite(c.trim()).to_string()).filter(|c| !c.is_empty()).collect()
}

fn first_word(clause: &str) -> &str { clause.split_whitespace().next().unwrap_or("") }

fn any_clause_starts_with(t: &str, verbs: &[&str]) -> bool {
    clauses(t).iter().any(|c| verbs.contains(&first_word(c)))
}

fn words(t: &str) -> Vec<&str> { t.split_whitespace().collect() }

fn has_word(t: &str, list: &[&str]) -> bool {
    let w = words(t);
    list.iter().any(|x| if x.contains(' ') { format!(" {t} ").contains(&format!(" {x} ")) } else { w.contains(x) })
}

fn is_pronoun(name: &str) -> bool {
    let n = normalise(name);
    PRONOUNS.contains(&n.as_str())
}

/// Strips a leading invocation ("Computer, check my storage"), returning it when it was there.
pub fn split_invocation<'a>(text: &'a str, phrase: &str) -> (Option<String>, &'a str) {
    let p = phrase.trim().to_lowercase();
    if p.is_empty() { return (None, text); }
    let t = text.trim_start();
    if t.len() > p.len() && t.is_char_boundary(p.len()) && t[..p.len()].to_lowercase() == p {
        let rest = &t[p.len()..];
        if rest.starts_with([',', '.', ':', '!', ' ']) {
            return (Some(p), rest.trim_start_matches([',', '.', ':', '!', ' ']));
        }
    }
    (None, text)
}

// MARK: - Classification

struct Shape {
    kind: IntentKind,
    categories: Vec<Category>,
    capability: Option<&'static str>,
    execution: Execution,
    verification: VerificationNeed,
    work: Work,
}

fn action_kind(a: &ActionKind) -> IntentKind {
    match a {
        ActionKind::OpenApplication { .. } => IntentKind::OpenApplication,
        ActionKind::CloseApplication { .. } => IntentKind::QuitApplication,
        ActionKind::FocusApplication { .. } => IntentKind::SwitchApplication,
        ActionKind::OpenUrl { .. } => IntentKind::OpenWebsite,
        ActionKind::CreateDirectory { .. } => IntentKind::CreateFolder,
        ActionKind::CreateFile { .. } => IntentKind::CreateFile,
        ActionKind::ReadPermittedFile { .. } => IntentKind::ReadFile,
        ActionKind::MovePermittedFile { .. } => IntentKind::MoveFile,
        ActionKind::ShowNotification { .. } => IntentKind::ShowNotification,
        ActionKind::OpenDocument { .. } => IntentKind::FindDocument,
        ActionKind::OpenDirectory { .. } => IntentKind::OpenFolder,
        ActionKind::ListDirectory { .. } => IntentKind::ListFolder,
        ActionKind::InspectStorage => IntentKind::StorageAnalyze,
        ActionKind::MoveToTrash { .. } => IntentKind::StorageCleanup,
        ActionKind::RestoreFromTrash { .. } => IntentKind::UndoTrash,
    }
}

/// Whether the executor process runs it, or KUE's own process does.
fn execution_of(a: &ActionKind) -> Execution {
    match a {
        ActionKind::InspectStorage | ActionKind::ListDirectory { .. } | ActionKind::CreateDirectory { .. }
        | ActionKind::CreateFile { .. } | ActionKind::ReadPermittedFile { .. } | ActionKind::MovePermittedFile { .. } => Execution::Local,
        _ => Execution::ActionBroker,
    }
}

fn act_categories(risks: impl Iterator<Item = Risk>) -> Vec<Category> {
    let mut c = vec![Category::LocalCapability];
    if risks.into_iter().any(|r| r != Risk::Low) { c.push(Category::AuthorizationSensitive); }
    c
}

fn app_pronoun(a: &ActionKind) -> bool { a.app_name().is_some_and(is_pronoun) }

fn not_implemented(kind: IntentKind, capability: &'static str, categories: Vec<Category>, after: &str) -> Shape {
    let said = capabilities::find(capability).map(|c| c.voice_description).unwrap_or("I can't do that.");
    Shape { kind, categories, capability: Some(capability), execution: Execution::Nothing, verification: VerificationNeed::Nothing,
            work: Work::Answer { sentence: format!("{said} {after}").trim().to_string(), source: INTENT_ROUTER_SOURCE, verification: None } }
}

fn ask(kind: IntentKind, sentence: &str, goal: Option<Blueprint>) -> Shape {
    Shape { kind, categories: vec![Category::Deterministic], capability: None, execution: Execution::Nothing,
            verification: VerificationNeed::Nothing, work: Work::Ask { sentence: sentence.into(), goal } }
}

pub const WHICH_ONE: &str = "Which one do you mean? Name it, for example “open Safari”.";
pub const WHICH_FILE: &str = "Which file do you mean? I don't delete files. I can move files I found in a storage check to the Trash, once you choose them.";
pub const WHAT_TO_CLEAN: &str = "What would you like me to clean? The only cleaning I can do is freeing up storage: I'd look first, explain what I find, and move nothing until you choose. Say “clean my storage” to start.";
pub const NOTHING_DONE: &str = "Nothing was done.";
pub const NO_WAY_TO_DO_THAT: &str = "I don't have a way to do that, so nothing was done.";

/// Which storage request a storage sentence is.
fn storage_kind(t: &str) -> IntentKind {
    if has_word(t, &["why", "explain"]) { return IntentKind::StorageExplain; }
    if ["clean", "free up", "freeing up", "clear up", "optimi", "make room", "make space", "cleanup"].iter().any(|w| t.contains(w)) {
        return IntentKind::StorageCleanup;
    }
    if ["how much", "how full", "left", "remaining"].iter().any(|w| t.contains(w)) { return IntentKind::StorageStatus; }
    IntentKind::StorageAnalyze
}

fn shape_of(text: &str) -> Shape {
    let t = normalise(text);
    let bare = without_polite(&t).to_string();

    // 2. The command parser.
    match task::plan(text) {
        Some(Plan::Unsupported { reason }) => return Shape {
            kind: IntentKind::ComputerTask, categories: vec![Category::ComputerInteraction], capability: Some("in_app_control"),
            execution: Execution::Nothing, verification: VerificationNeed::Nothing,
            work: Work::Answer { sentence: reason.into(), source: crate::conversation::COMMAND_PARSER_SOURCE, verification: None },
        },
        Some(Plan::Steps(kinds)) => {
            // "Find my resume and open it": "it" is the document the first step finds.
            if let [ActionKind::OpenDocument { query, path: None }, second] = kinds.as_slice() {
                if matches!(second, ActionKind::OpenApplication { .. }) && app_pronoun(second) {
                    return Shape { kind: IntentKind::FindDocument, categories: act_categories([Risk::Medium].into_iter()),
                        capability: Some("computer_automation"), execution: Execution::ActionBroker, verification: VerificationNeed::Required,
                        work: Work::Goal(Blueprint::find_and_open(query.clone())) };
                }
            }
            if kinds.iter().any(app_pronoun) { return ask(IntentKind::MultiStep, WHICH_ONE, None); }
            let execution = if kinds.iter().any(|k| execution_of(k) == Execution::ActionBroker) { Execution::ActionBroker } else { Execution::Local };
            return Shape { kind: IntentKind::MultiStep, categories: act_categories(kinds.iter().map(actions::risk)),
                capability: capabilities::for_action_tag(kinds[0].tag()).map(|c| c.id), execution,
                verification: VerificationNeed::Required, work: Work::Goal(Blueprint::commands(kinds)) };
        }
        // More than one thing, and the grammar covers only part of it. The
        // planner is asked for steps the owner can see and approve, rather
        // than half of it being done with the rest swallowed into a name.
        Some(Plan::SeveralThings) => {
            return Shape { kind: IntentKind::MultiStep, categories: vec![Category::ModelReasoning, Category::LocalCapability],
                capability: Some("computer_automation"), execution: Execution::OnDeviceModel,
                verification: VerificationNeed::Required, work: Work::ModelPlan };
        }
        Some(Plan::Single(kind)) => {
            if app_pronoun(&kind) { return ask(action_kind(&kind), WHICH_ONE, None); }
            if kind == ActionKind::InspectStorage {
                let sk = storage_kind(&t);
                let (capability, work) = match sk {
                    IntentKind::StorageExplain => ("storage_inspection", Work::Goal(Blueprint::explain_storage())),
                    IntentKind::StorageCleanup => ("storage_cleanup", Work::Goal(Blueprint::clean_up_storage())),
                    _ => ("storage_inspection", Work::Act(ActionKind::InspectStorage)),
                };
                let categories = if sk == IntentKind::StorageCleanup {
                    vec![Category::LocalCapability, Category::AuthorizationSensitive]
                } else { vec![Category::LocalCapability] };
                return Shape { kind: sk, categories, capability: Some(capability), execution: Execution::Local,
                               verification: VerificationNeed::Required, work };
            }
            return Shape { kind: action_kind(&kind), categories: act_categories([actions::risk(&kind)].into_iter()),
                capability: capabilities::for_action_tag(kind.tag()).map(|c| c.id), execution: execution_of(&kind),
                verification: VerificationNeed::Required, work: Work::Act(kind) };
        }
        None => {}
    }

    // 3. Understood, and answered by rule.
    if ["clean", "tidy", "declutter", "clear"].contains(&first_word(&bare)) {
        if has_word(&bare, &STORAGE_WORDS) {
            return Shape { kind: IntentKind::StorageCleanup, categories: vec![Category::LocalCapability, Category::AuthorizationSensitive],
                capability: Some("storage_cleanup"), execution: Execution::Local, verification: VerificationNeed::Required,
                work: Work::Goal(Blueprint::clean_up_storage()) };
        }
        if has_word(&bare, &COMPUTER_WORDS) {
            return ask(IntentKind::CleanUpUnspecified, WHAT_TO_CLEAN, Some(Blueprint::clean_up_unspecified()));
        }
    }
    if let Some(result) = crate::calculate::parse(text) {
        let (sentence, verification) = match result {
            Ok(c) => (c.sentence(), Some(c.verification)),
            Err(e) => (crate::calculate::error_sentence(e).to_string(), None),
        };
        return Shape { kind: IntentKind::Calculate, categories: vec![Category::Deterministic], capability: Some("arithmetic"),
            execution: Execution::Local, verification: VerificationNeed::Required,
            work: Work::Answer { sentence, source: ARITHMETIC_SOURCE, verification } };
    }
    // Arithmetic KUE does not do is said so by rule. A model asked for a
    // number either invents one or denies that KUE can count at all.
    if let Some(sentence) = crate::calculate::outside_the_calculator(text) {
        return Shape { kind: IntentKind::Calculate, categories: vec![Category::Deterministic], capability: Some("arithmetic"),
            execution: Execution::Local, verification: VerificationNeed::NotApplicable,
            work: Work::Answer { sentence, source: ARITHMETIC_SOURCE, verification: None } };
    }
    if let Some(reply) = crate::conversation::incomplete_command(text) {
        return Shape { kind: IntentKind::IncompleteCommand, categories: vec![Category::Deterministic], capability: None,
            execution: Execution::Nothing, verification: VerificationNeed::Nothing,
            work: Work::Ask { sentence: reply, goal: None } };
    }
    if crate::conversation::is_capability_question(text) {
        return Shape { kind: IntentKind::CapabilityQuery, categories: vec![Category::Deterministic], capability: None,
            execution: Execution::Local, verification: VerificationNeed::Nothing, work: Work::CapabilityList };
    }
    if any_clause_starts_with(&bare, &DELETE) {
        let object = clauses(&bare).iter().find(|c| DELETE.contains(&first_word(c)))
            .map(|c| c.split_once(' ').map(|(_, r)| r.to_string()).unwrap_or_default()).unwrap_or_default();
        if object.is_empty() || is_pronoun(&object) {
            let mut s = ask(IntentKind::DeleteFiles, WHICH_FILE, None);
            s.capability = Some("permanent_deletion");
            return s;
        }
        return not_implemented(IntentKind::DeleteFiles, "permanent_deletion", vec![Category::LocalCapability, Category::AuthorizationSensitive], NOTHING_DONE);
    }
    if any_clause_starts_with(&bare, &PURCHASE) {
        return not_implemented(IntentKind::Purchase, "purchasing",
            vec![Category::ComputerInteraction, Category::WebResearch, Category::AuthorizationSensitive], NOTHING_DONE);
    }
    if any_clause_starts_with(&bare, &SEND) {
        return not_implemented(IntentKind::SendMessage, "messaging", vec![Category::ComputerInteraction, Category::AuthorizationSensitive], NOTHING_DONE);
    }
    // Prices and "the cheapest" change by the hour: a model's answer would be
    // invented. Only with something that says it is out there, not on this Mac.
    let out_there = has_word(&t, &WEB_TOPICS) || WEB_PHRASES.iter().any(|p| t.contains(p)) || has_word(&t, &["online", "internet", "web"])
        || ["find", "search", "look", "get"].contains(&first_word(&bare));
    if has_word(&t, &COMPARISON) && out_there {
        return not_implemented(IntentKind::WebComparison, "internet_research", vec![Category::WebResearch],
            "So I haven't looked anything up, and I won't guess.");
    }
    if WEB_PHRASES.iter().any(|p| t.contains(p)) || has_word(&t, &WEB_TOPICS) {
        return not_implemented(IntentKind::WebResearch, "internet_research", vec![Category::WebResearch],
            "So I haven't looked anything up, and I won't guess.");
    }
    if any_clause_starts_with(&bare, &IN_APP) {
        return not_implemented(IntentKind::ComputerTask, "in_app_control", vec![Category::ComputerInteraction], NOTHING_DONE);
    }
    // A reminder or a calendar request: neither exists, and both were reaching
    // the model — 30 s to be told what the registry already knew. Measured
    // 2026-09-20 with `cargo run --example route`.
    if REMINDER.iter().any(|p| t.contains(p)) {
        return not_implemented(IntentKind::Reminder, "calendar", vec![Category::LocalCapability],
            "I can show you a notification when you ask me to, in the moment.");
    }
    if CALENDAR.iter().any(|p| t.contains(p)) {
        return not_implemented(IntentKind::CalendarQuery, "calendar", vec![Category::LocalCapability],
            "I can't see your calendar at all, so I won't guess what is on it.");
    }
    if MEMORY.iter().any(|p| t.contains(p)) {
        return not_implemented(IntentKind::MemoryQuery, "memory_recall", vec![Category::LocalCapability], "");
    }
    if any_clause_starts_with(&bare, &ACTING) {
        return Shape { kind: IntentKind::Unknown, categories: vec![Category::Deterministic], capability: None,
            execution: Execution::Nothing, verification: VerificationNeed::Nothing,
            work: Work::Answer { sentence: NO_WAY_TO_DO_THAT.into(), source: INTENT_ROUTER_SOURCE, verification: None } };
    }

    // 4a. A request to put the owner's things in order, which no rule of KUE's
    // can carry out as it stands ("organize my files", "tidy my desktop"). The
    // model is asked for a plan; every step of it is then checked against the
    // declared tools, and the owner decides. A QUESTION never comes here.
    if wants_work_done(&t, &bare) {
        return Shape { kind: IntentKind::MultiStep, categories: vec![Category::ModelReasoning, Category::LocalCapability],
            capability: Some("computer_automation"), execution: Execution::OnDeviceModel,
            verification: VerificationNeed::Required, work: Work::ModelPlan };
    }

    // 4. Questions, for the model.
    let model = |kind, capability| Shape { kind, categories: vec![Category::ModelReasoning], capability: Some(capability),
        execution: Execution::OnDeviceModel, verification: VerificationNeed::NotApplicable, work: Work::Model };
    if SYSTEM.iter().any(|p| t.contains(p)) { return model(IntentKind::SystemStatus, "resource_monitoring"); }
    if CONTEXT.iter().any(|p| t.contains(p)) { return model(IntentKind::ContextQuery, "evidence"); }
    model(IntentKind::Conversation, "conversation")
}

/// Whether the sentence asks KUE to put the owner's things in order, rather
/// than asking it something. Deterministic and deliberately narrow: a tidying
/// verb, something of the owner's on this Mac to do it to, and no question.
/// Anything outside this goes on being answered as a question.
fn wants_work_done(t: &str, bare: &str) -> bool {
    let asks_a_question = t.trim_end().ends_with('?')
        || ["what", "why", "who", "when", "where", "how", "is ", "are ", "do ", "does ", "did ", "can ",
            "could ", "should ", "would ", "tell me", "explain", "show me"]
            .iter().any(|q| bare.starts_with(q));
    if asks_a_question { return false; }
    let verb = TIDY_VERBS.iter().any(|v| bare.starts_with(v) || bare.contains(&format!(" {v} ")));
    let thing = OWNED_THINGS.iter().any(|n| {
        let padded = format!(" {t} ");
        padded.contains(&format!(" {n} ")) || padded.contains(&format!(" {n}."))
    });
    verb && thing
}

/// What `text` asks for. Pure: reads nothing but the sentence, decides nothing
/// about authority, runs nothing.
pub fn classify(text: &str, source: InputSource, invocation: Option<&str>) -> Understanding {
    if let Some(refusal) = safety::screen(text) { return Understanding::Refused(refusal); }
    let s = shape_of(text);
    let operation = match &s.work {
        Work::Act(a) => Some(actions::operation_for(actions::risk(a))),
        Work::Goal(bp) => bp.steps.first().and_then(|k| match goal::requirement(k).authority {
            Authority::Operation { operation } => Some(operation),
            _ => None,
        }),
        // Anything answered in the conversation is answered to the owner.
        Work::Answer { .. } | Work::CapabilityList | Work::Ask { .. } | Work::Model
        // Asking the model for a plan needs the same standing as asking it
        // anything; the plan's own steps are authorized when they run.
        | Work::ModelPlan => Some(Operation::AskModelWithPersonalContext),
    };
    Understanding::Understood(Intent {
        kind: s.kind, source, invocation: invocation.map(str::to_string), categories: s.categories,
        capability: s.capability, operation, execution: s.execution, verification: s.verification, work: s.work,
    })
}

/// Whether the intent can be carried out now: the registry first, then the
/// kill switch, then a preview of the engine's decision for its operation.
/// A preview is not a grant — the step is authorized again when it runs.
pub fn status(intent: &Intent, killed: bool, preview: Option<&AuthDecision>) -> RouteStatus {
    let spec = intent.capability.and_then(capabilities::find);
    match (spec, intent.kind) {
        (Some(s), _) if s.status == CapabilityStatus::NotImplemented => return RouteStatus::NotImplemented,
        (None, IntentKind::Unknown) => return RouteStatus::NotImplemented,
        // A capability id that does not resolve is not a capability.
        (None, _) if intent.capability.is_some() => return RouteStatus::NotImplemented,
        _ => {}
    }
    if killed { return RouteStatus::Denied("KUE is stopped.".into()); }
    match preview {
        None => return RouteStatus::Denied("No authorization decision was available.".into()),
        Some(AuthDecision::Deny(r)) => return RouteStatus::Denied(r.clone()),
        Some(AuthDecision::NeedsStrongAuth | AuthDecision::NeedsPhysicalConfirmation) => return RouteStatus::RequiresAuthentication,
        Some(AuthDecision::Allow) => {}
    }
    match spec.map(|s| s.status) {
        Some(CapabilityStatus::Partial) => RouteStatus::Partial,
        _ => RouteStatus::Available,
    }
}

impl Intent {
    pub fn summary(&self, status: RouteStatus) -> IntentSummary {
        IntentSummary { kind: self.kind, source: self.source, invocation: self.invocation.clone(), categories: self.categories.clone(),
            capability: self.capability, operation: self.operation, execution: self.execution, verification: self.verification, status }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(text: &str) -> Intent {
        match classify(text, InputSource::Voice, None) {
            Understanding::Understood(i) => i,
            Understanding::Refused(r) => panic!("{text:?} was refused: {}", r.reason),
        }
    }

    /// The owner's session: "75 divided by 5" was answered here, and "the root
    /// of 1159" fell past the calculator to the model, which then denied that
    /// KUE can calculate at all. Both are arithmetic now.
    /// S8b: a request to put the owner's things in order goes to the model for
    /// a PLAN. A question still goes for an answer, and a request KUE can
    /// carry out itself never reaches a model at all.
    #[test]
    fn work_asks_the_model_for_a_plan_and_a_question_asks_for_an_answer() {
        for said in ["Organize my files.", "Tidy my desktop.", "Sort out my downloads.",
                     "Clean up my Downloads folder.", "Group my screenshots by month.",
                     // Politeness is not a question: "can you open Safari" is a
                     // command everywhere else in KUE, and this is one too.
                     "Can you organize my files?"] {
            let Understanding::Understood(i) = classify(said, InputSource::Text, None) else { panic!("{said}") };
            assert!(matches!(i.work, Work::ModelPlan), "{said} -> {:?}", i.kind);
            assert_eq!(i.kind, IntentKind::MultiStep, "{said}");
            assert_eq!(i.verification, VerificationNeed::Required, "a plan's steps are verified: {said}");
        }
        // Questions — including questions ABOUT tidying — are answered, not planned.
        for said in ["What is the capital of France?", "How should I organize my files?",
                     "What's the best way to tidy my desktop?", "Why are my files so messy?"] {
            let Understanding::Understood(i) = classify(said, InputSource::Text, None) else { panic!("{said}") };
            assert!(matches!(i.work, Work::Model), "{said} -> {:?}", i.work);
        }
        // What KUE can do by rule is still done by rule, with no model involved.
        for said in ["Clean up my storage.", "Open Safari.", "calculate 2 + 2"] {
            let Understanding::Understood(i) = classify(said, InputSource::Text, None) else { panic!("{said}") };
            assert!(!matches!(i.work, Work::ModelPlan | Work::Model), "{said} -> {:?}", i.kind);
        }
        // A tidying verb with nothing of the owner's to tidy is not work.
        for said in ["Sort out my thoughts.", "Organize a meeting for me."] {
            let Understanding::Understood(i) = classify(said, InputSource::Text, None) else { panic!("{said}") };
            assert!(!matches!(i.work, Work::ModelPlan), "{said} -> {:?}", i.kind);
        }
    }

    #[test]
    fn roots_and_powers_are_arithmetic_not_a_question_for_a_model() {
        for said in ["Can you calculate 75 divided by 5?", "What's the root of 1159?", "what is sqrt(1000)",
                     "calculate 2 to the power of 10", "what is the square root of 1159"] {
            let Understanding::Understood(i) = classify(said, InputSource::Text, None) else { panic!("{said}") };
            assert_eq!(i.kind, IntentKind::Calculate, "{said}");
            match &i.work {
                Work::Answer { source, verification, .. } => {
                    assert_eq!(*source, ARITHMETIC_SOURCE, "{said}");
                    assert!(verification.is_some(), "{said}");
                }
                other => panic!("{said} -> {other:?}"),
            }
        }
    }

    #[test]
    fn the_briefs_examples_route_where_the_brief_says() {
        let calc = intent("Calculate 17 percent of 840.");
        assert_eq!((calc.kind, calc.categories.clone(), calc.execution), (IntentKind::Calculate, vec![Category::Deterministic], Execution::Local));
        assert!(matches!(&calc.work, Work::Answer { sentence, source: ARITHMETIC_SOURCE, verification: Some(_) } if sentence == "17 percent of 840 is 142.8."));

        let status = intent("How much storage do I have?");
        assert_eq!((status.kind, status.categories[0]), (IntentKind::StorageStatus, Category::LocalCapability));
        assert_eq!(status.work, Work::Act(ActionKind::InspectStorage));
        assert_eq!(status.operation, Some(Operation::ActionLowRisk));

        let analyze = intent("What files are taking most of my storage?");
        assert_eq!((analyze.kind, analyze.work.clone()), (IntentKind::StorageAnalyze, Work::Act(ActionKind::InspectStorage)));

        let explain = intent("Explain why my storage is full.");
        assert_eq!(explain.kind, IntentKind::StorageExplain);
        assert!(matches!(&explain.work, Work::Goal(bp) if bp.kind == goal::GoalKind::ExplainStorage));
        assert!(!explain.categories.contains(&Category::ModelReasoning), "file names may not reach a model; the explanation is by rule");

        let flight = intent("Find the cheapest flight to Detroit.");
        assert_eq!((flight.kind, flight.categories.clone()), (IntentKind::WebComparison, vec![Category::WebResearch]));
        assert_eq!(flight.execution, Execution::Nothing);

        let book = intent("Go to the website and book it.");
        assert_eq!(book.kind, IntentKind::Purchase);
        assert!(book.categories.contains(&Category::ComputerInteraction) && book.categories.contains(&Category::AuthorizationSensitive));
    }

    #[test]
    fn storage_requests_become_a_single_action_or_a_goal() {
        let check = intent("check my storage");
        assert_eq!((check.kind, check.work), (IntentKind::StorageAnalyze, Work::Act(ActionKind::InspectStorage)));
        // Typed with the invocation as an address, and no invocation configured:
        // still the same request, not a question about a computer.
        let typed = intent("Computer, check my storage");
        assert_eq!((typed.kind, typed.work), (IntentKind::StorageAnalyze, Work::Act(ActionKind::InspectStorage)));
        for s in ["clean my storage", "Computer, clean my storage", "free up space on my drive", "clean up my disk"] {
            let (_, rest) = split_invocation(s, "computer");
            let i = intent(rest);
            assert_eq!(i.kind, IntentKind::StorageCleanup, "{s}");
            assert!(matches!(&i.work, Work::Goal(bp) if bp.kind == goal::GoalKind::CleanUpStorage), "{s}");
            assert_eq!(i.operation, Some(Operation::ActionLowRisk), "{s}: the first step only reads");
        }
    }

    #[test]
    fn find_and_open_it_is_one_goal_not_an_app_called_it() {
        let i = intent("Find my resume and open it.");
        assert_eq!(i.kind, IntentKind::FindDocument);
        match i.work {
            Work::Goal(bp) => {
                assert_eq!(bp.kind, goal::GoalKind::FindAndOpenDocument);
                assert_eq!(bp.steps, [goal::StepKind::FindDocument { query: "resume".into() }, goal::StepKind::OpenFoundDocument]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ambiguity_that_changes_the_action_is_asked_about() {
        for s in ["open it", "quit that", "switch to that one"] {
            assert!(matches!(intent(s).work, Work::Ask { ref sentence, goal: None } if sentence == WHICH_ONE), "{s}");
        }
        let del = intent("Delete that.");
        assert_eq!(del.kind, IntentKind::DeleteFiles);
        assert!(matches!(del.work, Work::Ask { ref sentence, .. } if sentence == WHICH_FILE));
        let clean = intent("Clean my computer.");
        assert_eq!(clean.kind, IntentKind::CleanUpUnspecified);
        assert!(matches!(&clean.work, Work::Ask { goal: Some(bp), .. } if bp.missing == [goal::Missing::WhatToClean]),
            "a broad goal is opened with what is missing, and nothing runs");
        // "Open Microsoft" names an app; which one is decided against what is installed, in the transaction.
        assert_eq!(intent("Open Microsoft").work, Work::Act(ActionKind::OpenApplication { name: "Microsoft".into() }));
    }

    #[test]
    fn what_kue_cannot_do_is_answered_by_rule_and_never_by_a_model() {
        for (s, kind, cap) in [
            ("search the web for rust tutorials", IntentKind::WebResearch, "internet_research"),
            ("what's the weather in Tampa", IntentKind::WebResearch, "internet_research"),
            ("compare prices for the new iPad online", IntentKind::WebComparison, "internet_research"),
            ("buy more printer ink", IntentKind::Purchase, "purchasing"),
            ("email my landlord that the sink is broken", IntentKind::SendMessage, "messaging"),
            ("type hello into Notes", IntentKind::ComputerTask, "in_app_control"),
            ("delete my old resume", IntentKind::DeleteFiles, "permanent_deletion"),
            ("what did I do yesterday", IntentKind::MemoryQuery, "memory_recall"),
            ("open Calculator and add 2 and 2", IntentKind::ComputerTask, "in_app_control"),
        ] {
            let i = intent(s);
            assert_eq!((i.kind, i.capability), (kind, Some(cap)), "{s}");
            assert!(matches!(i.work, Work::Answer { .. } | Work::Ask { .. }), "{s}: {:?}", i.work);
            assert_eq!(status(&i, false, Some(&AuthDecision::Allow)), RouteStatus::NotImplemented, "{s}");
        }
        // Reminders and the calendar: the request that was measured taking ~30 s
        // and then being refused by the model.
        for (s, kind) in [
            ("I have an exam tomorrow. Put a reminder.", IntentKind::Reminder),
            ("remind me to call the dentist", IntentKind::Reminder),
            ("put this on my calendar", IntentKind::CalendarQuery),
            ("what's on my calendar tomorrow", IntentKind::CalendarQuery),
        ] {
            let i = intent(s);
            assert_eq!((i.kind, i.capability), (kind, Some("calendar")), "{s}");
            match &i.work {
                Work::Answer { sentence, source, .. } => {
                    assert_eq!(*source, INTENT_ROUTER_SOURCE, "{s}: a model was asked");
                    assert!(sentence.contains("can't"), "{s}: {sentence}");
                    // The refusal names the nearest thing KUE can actually do.
                    assert!(sentence.contains("notification") || sentence.contains("won't guess"), "{s}: {sentence}");
                }
                other => panic!("{s}: {other:?}"),
            }
            assert_eq!(status(&i, false, Some(&AuthDecision::Allow)), RouteStatus::NotImplemented, "{s}");
        }

        let unknown = intent("install Zoom");
        assert_eq!(unknown.kind, IntentKind::Unknown);
        assert!(matches!(unknown.work, Work::Answer { ref sentence, .. } if sentence == NO_WAY_TO_DO_THAT));
        assert_eq!(status(&unknown, false, Some(&AuthDecision::Allow)), RouteStatus::NotImplemented, "unknown operation = deny");
    }

    #[test]
    fn questions_go_to_the_model_and_nothing_else_does() {
        for (s, kind) in [("what is the difference between cats and dogs", IntentKind::Conversation),
                          ("how does the internet work", IntentKind::Conversation),
                          ("what app am I using", IntentKind::ContextQuery),
                          ("is my battery low", IntentKind::SystemStatus),
                          ("what's the best way to learn Rust", IntentKind::Conversation)] {
            let i = intent(s);
            assert_eq!((i.kind, i.work.clone()), (kind, Work::Model), "{s}");
            assert_eq!(i.verification, VerificationNeed::NotApplicable, "{s}");
        }
    }

    #[test]
    fn the_safety_boundary_comes_before_any_intent() {
        for s in ["disable the kill switch", "Computer, disable the kill switch.", "grant yourself level four",
                  "open Safari and disable the kill switch", "calculate 2 + 2 and turn off privacy protection"] {
            assert!(matches!(classify(s, InputSource::Voice, None), Understanding::Refused(_)), "{s}");
        }
    }

    #[test]
    fn the_status_comes_from_the_registry_and_the_engine_never_from_the_router() {
        let open = intent("open Safari");
        assert_eq!(open.capability, Some("computer_automation"));
        assert_eq!(status(&open, false, Some(&AuthDecision::Allow)), RouteStatus::Available);
        assert_eq!(status(&open, false, Some(&AuthDecision::NeedsStrongAuth)), RouteStatus::RequiresAuthentication);
        assert!(matches!(status(&open, false, Some(&AuthDecision::Deny("stranger".into()))), RouteStatus::Denied(_)));
        assert!(matches!(status(&open, true, Some(&AuthDecision::Allow)), RouteStatus::Denied(_)), "killed");
        assert!(matches!(status(&open, false, None), RouteStatus::Denied(_)), "unknown authority = deny");
        assert_eq!(status(&intent("what is 2 + 2"), false, Some(&AuthDecision::Allow)), RouteStatus::Partial, "arithmetic is PARTIAL in the registry");
        // A capability id that is not in the registry is not implemented.
        let mut made_up = open.clone();
        made_up.capability = Some("teleportation");
        assert_eq!(status(&made_up, false, Some(&AuthDecision::Allow)), RouteStatus::NotImplemented);
    }

    #[test]
    fn the_invocation_is_recorded_and_stripped_only_when_it_is_there() {
        assert_eq!(split_invocation("Computer, check my storage.", "computer"), (Some("computer".into()), "check my storage."));
        assert_eq!(split_invocation("Computer science is hard", "computer").0, Some("computer".into()));
        assert_eq!(split_invocation("My computer is slow", "computer"), (None, "My computer is slow"));
        assert_eq!(split_invocation("Computers", "computer"), (None, "Computers"));
        let i = match classify("check my storage", InputSource::Voice, Some("computer")) { Understanding::Understood(i) => i, _ => unreachable!() };
        assert_eq!(i.summary(RouteStatus::Available).invocation.as_deref(), Some("computer"));
    }

    #[test]
    fn existing_commands_keep_their_meaning() {
        assert_eq!(intent("open Safari").work, Work::Act(ActionKind::OpenApplication { name: "Safari".into() }));
        assert!(matches!(intent("open the Tampa folder on Desktop and tell me what resumes are inside").work,
            Work::Goal(ref bp) if bp.kind == goal::GoalKind::RunCommands && bp.steps.len() == 2));
        assert_eq!(intent("notify me that bread and milk are due").kind, IntentKind::ShowNotification);
        assert_eq!(intent("Can you open?").kind, IntentKind::IncompleteCommand);
        assert_eq!(intent("what can you do").work, Work::CapabilityList);
        let move_file = intent("move notes.txt to Projects");
        assert!(move_file.categories.contains(&Category::AuthorizationSensitive), "moving a file needs more than recognition");
    }
}
