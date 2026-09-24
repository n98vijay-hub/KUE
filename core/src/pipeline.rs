//! The governed request pipeline: one lifecycle for every request, however it
//! arrived.
//!
//! ```text
//!  voice · typed · proactive · computer context · scheduled
//!                          │
//!                        Input ──▶ dialogue::interpret (does it answer what is open?)
//!                          │
//!  RECEIVED → UNDERSTANDING → CHECKING → DECIDING ─┬─▶ ANSWERING ──────────────▶ COMPLETED
//!                                                  ├─▶ ASKING ───▶ WAITING_FOR_USER
//!                                                  ├─▶ SUGGESTING ─▶ WAITING_FOR_USER | COMPLETED
//!                                                  ├─▶ PLANNING ──▶ AUTHORIZING | WAITING_FOR_USER
//!                                                  └─▶ AUTHORIZING ─▶ ACTING ─▶ VERIFYING ─▶ COMPLETED
//!                                                                                        ├─▶ UNCERTAIN
//!                                                                                        └─▶ FAILED
//!  WAITING_FOR_USER ─ confirm ─▶ AUTHORIZING      (the confirm is re-authorized, never trusted)
//!                  ─ correct ─▶ PLANNING         (the plan is revised, not replaced)
//!                  ─ answer  ─▶ DECIDING
//!  any live state  ─▶ CANCELLED | REFUSED | BLOCKED | PAUSED | KILLED
//! ```
//!
//! WHY IT EXISTS. KUE's state lived in several places — the transaction book,
//! the goal, the conversation box, the window's own flags — and the window
//! reconstructed "what is KUE doing" from them. This makes one owner: the
//! runtime holds the state, every transport enters the same way, and the window
//! only projects it.
//!
//! WHAT IT DOES NOT DO. It decides nothing about permission. `Decision::Act`
//! means "the runtime will try to act", after which authorization, the Touch ID
//! prompt and verification happen exactly where they always did. The model may
//! reason and propose; this records what the governed runtime decided.
//!
//! An illegal transition is REFUSED and reported, never forced: a pipeline that
//! can jump from RECEIVED to COMPLETED would let a request claim it finished
//! without ever acting or verifying.

use crate::dialogue::{Dialogue, Interpretation, Open, TurnKind};
use serde::Serialize;

/// How a request arrived. The pipeline behaves identically for all of them;
/// the transport is recorded so the owner can see what started something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Transport {
    /// Spoken, after the wake word or push-to-talk.
    Voice,
    /// Typed into the window.
    Typed,
    /// Raised by KUE itself. Reserved: no proactive producer exists yet.
    Proactive,
    /// Raised by something KUE observed about the computer. Reserved.
    ComputerContext,
    /// Raised by a schedule. Reserved: KUE has no scheduler yet.
    Scheduled,
}

/// Where one request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RequestState {
    Received,
    Understanding,
    Checking,
    Deciding,
    Answering,
    Asking,
    Suggesting,
    Planning,
    Authorizing,
    Acting,
    Verifying,
    WaitingForUser,
    Completed,
    /// It ran, and KUE could not verify the result. Never reported as done.
    Uncertain,
    Failed,
    /// Refused by a rule: safety, authorization, privacy, or "KUE cannot do that".
    Refused,
    /// Could not proceed: a precondition did not hold.
    Blocked,
    Cancelled,
    Paused,
    Killed,
}

impl RequestState {
    pub fn is_terminal(self) -> bool {
        matches!(self, RequestState::Completed | RequestState::Uncertain | RequestState::Failed
                     | RequestState::Refused | RequestState::Blocked | RequestState::Cancelled | RequestState::Killed)
    }

    pub fn tag(self) -> &'static str {
        match self {
            RequestState::Received => "RECEIVED", RequestState::Understanding => "UNDERSTANDING",
            RequestState::Checking => "CHECKING", RequestState::Deciding => "DECIDING",
            RequestState::Answering => "ANSWERING", RequestState::Asking => "ASKING",
            RequestState::Suggesting => "SUGGESTING", RequestState::Planning => "PLANNING",
            RequestState::Authorizing => "AUTHORIZING", RequestState::Acting => "ACTING",
            RequestState::Verifying => "VERIFYING", RequestState::WaitingForUser => "WAITING_FOR_USER",
            RequestState::Completed => "COMPLETED", RequestState::Uncertain => "UNCERTAIN",
            RequestState::Failed => "FAILED", RequestState::Refused => "REFUSED",
            RequestState::Blocked => "BLOCKED", RequestState::Cancelled => "CANCELLED",
            RequestState::Paused => "PAUSED", RequestState::Killed => "KILLED",
        }
    }
}

/// Every legal step. Anything not here is refused.
pub fn legal(from: RequestState, to: RequestState) -> bool {
    use RequestState::*;
    if from.is_terminal() { return false; }
    // The owner's gestures and the rules can end any live request.
    if matches!(to, Cancelled | Refused | Blocked | Killed) { return true; }
    // Pause holds any live request; resume returns it to where it was, which the
    // pipeline checks separately.
    if to == Paused { return from != Paused; }
    matches!((from, to),
        (Received, Understanding)
        | (Understanding, Checking)
        | (Understanding, Uncertain)
        | (Checking, Deciding)
        | (Deciding, Answering | Asking | Suggesting | Planning | Authorizing)
        | (Answering, Completed | Failed)
        | (Asking, WaitingForUser)
        | (Suggesting, WaitingForUser | Completed)
        | (Planning, Authorizing | WaitingForUser | Answering)
        | (Authorizing, Acting | WaitingForUser)
        | (WaitingForUser, Authorizing | Planning | Deciding)
        | (Acting, Verifying | Failed)
        | (Verifying, Completed | Uncertain | Failed)
        | (Verifying, Planning)          // a goal's next step
    )
}

const ALL_STATES: [RequestState; 20] = {
    use RequestState::*;
    [Received, Understanding, Checking, Deciding, Answering, Asking, Suggesting, Planning, Authorizing,
     Acting, Verifying, WaitingForUser, Completed, Uncertain, Failed, Refused, Blocked, Cancelled, Paused, Killed]
};

/// The shortest sequence of legal steps from one state to another, excluding
/// the start. Paused is never passed through on the way somewhere else.
fn shortest_legal_path(from: RequestState, to: RequestState) -> Option<Vec<RequestState>> {
    use std::collections::VecDeque;
    let mut prev: Vec<(RequestState, RequestState)> = Vec::new();
    let mut seen = vec![from];
    let mut q = VecDeque::from([from]);
    while let Some(s) = q.pop_front() {
        if s == to { break; }
        for next in ALL_STATES {
            if next == RequestState::Paused || seen.contains(&next) || !legal(s, next) { continue; }
            // Only the target may be terminal: a path never ends early on the way.
            if next.is_terminal() && next != to { continue; }
            seen.push(next);
            prev.push((next, s));
            q.push_back(next);
        }
    }
    if !seen.contains(&to) { return None; }
    let mut path = vec![to];
    let mut cur = to;
    while let Some(&(_, p)) = prev.iter().find(|(n, _)| *n == cur) {
        if p == from { break; }
        path.push(p);
        cur = p;
    }
    path.reverse();
    Some(path)
}

/// What the governed runtime decided to do with a request. Recorded, because
/// "why did KUE ask me instead of doing it?" deserves an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    /// Say something: a fact, a calculation, a refusal that names an alternative.
    Answer,
    /// Something needed is missing; ask for it.
    Ask,
    /// Offer a next step without taking it.
    Suggest,
    /// Several steps are needed; lay them out.
    Plan,
    /// One declared tool, through authorization and verification.
    Act,
    /// A rule refused it.
    Refuse,
}

/// The ids that tie a request to its conversation and its work.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Ids {
    pub conversation_id: String,
    pub request_id: String,
    pub goal_id: Option<String>,
    pub plan_id: Option<String>,
    pub tool_execution_id: Option<String>,
    pub verification_id: Option<String>,
}

/// One request and its whole history.
#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub ids: Ids,
    pub transport: Transport,
    pub state: RequestState,
    pub decision: Option<Decision>,
    /// Every state it passed through, with when.
    pub trace: Vec<(RequestState, f64)>,
    /// The state to return to on resume.
    held: Option<RequestState>,
    /// Why it ended where it did, in words the owner can read.
    pub why: Option<String>,
}

/// A transition that was refused.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IllegalTransition {
    pub request_id: String,
    pub from: RequestState,
    pub to: RequestState,
}

/// What the whole runtime is doing — the one state the window, the voice, the
/// logs and any automation read. Derived; never set by the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeState {
    Idle, Listening, Understanding, Checking, Asking, Planning, Authorizing, Acting, Verifying,
    Completed, Failed, Blocked, Uncertain, WaitingForUser, Recovering, Paused, Killed,
}

/// Facts from outside the pipeline that outrank it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlay {
    pub killed: bool,
    pub paused: bool,
    pub recovering: bool,
    /// The microphone is open for the owner right now.
    pub listening: bool,
}

/// The conversation and the requests in it.
#[derive(Debug, Clone, Serialize)]
pub struct Pipeline {
    pub dialogue: Dialogue,
    requests: Vec<Request>,
    next: u64,
    max_requests: usize,
    /// Refused transitions, counted: a pipeline that is asked to do something
    /// illegal has a bug somewhere, and that should be visible.
    pub refused_transitions: u64,
}

impl Pipeline {
    pub fn new(conversation_id: impl Into<String>) -> Pipeline {
        Pipeline { dialogue: Dialogue::new(conversation_id), requests: Vec::new(), next: 1,
                   max_requests: 40, refused_transitions: 0 }
    }

    fn id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}{}", self.next);
        self.next += 1;
        id
    }

    /// An utterance arrives, from any transport. It is recorded as a turn, and
    /// either answers what is open or becomes a new request. Returns the
    /// interpretation and the request it now belongs to.
    pub fn receive(&mut self, transport: Transport, said: &str, now: f64) -> (Interpretation, String) {
        let interpretation = crate::dialogue::interpret(said, self.dialogue.open());
        let had_open = self.dialogue.open().is_some();
        // "Stop" with nothing waiting on the owner, while something is running:
        // it stops what is running. Otherwise a bare stop would become a new
        // request — or a question to a model.
        let interpretation = match interpretation {
            Interpretation::NewRequest if crate::voice::reference::interpret(said)
                == Some(crate::voice::reference::Reference::Cancel) =>
                match self.active().map(|r| r.ids.request_id.clone()) {
                    Some(rid) => Interpretation::Cancellation { request_id: rid },
                    None => Interpretation::NewRequest,
                },
            other => other,
        };
        let kind = match &interpretation {
            Interpretation::Confirmation { .. } => TurnKind::UserConfirmation,
            Interpretation::Cancellation { .. } => TurnKind::UserCancellation,
            Interpretation::Correction { .. } | Interpretation::Clarify { .. }
            | Interpretation::PlanChange { .. } => TurnKind::UserCorrection,
            Interpretation::NewRequest | Interpretation::AboutWork { .. } | Interpretation::UndoLast
            | Interpretation::Memory(_) => TurnKind::UserSpeech,
        };
        let request_id = match &interpretation {
            // A question about the work belongs to the work it asks about: the
            // live request, or failing that the last one. It starts nothing.
            // A question about the work, or about what KUE keeps, belongs to
            // the work it asks about: the live request, or failing that the
            // last one. Neither starts anything.
            Interpretation::AboutWork { .. } | Interpretation::Memory(_) => self.active().or_else(|| self.requests.last())
                .map(|r| r.ids.request_id.clone()).unwrap_or_default(),
            Interpretation::NewRequest | Interpretation::UndoLast => {
                let rid = self.id("r");
                let ids = Ids { conversation_id: self.dialogue.conversation_id.clone(), request_id: rid.clone(), ..Default::default() };
                if self.requests.len() >= self.max_requests {
                    // Make room by dropping the oldest FINISHED request. If every
                    // request is still live, the oldest is first ended — CANCELLED,
                    // with the reason — so nothing disappears while it looks
                    // unfinished, and nothing waits forever unseen.
                    let i = match self.requests.iter().position(|r| r.state.is_terminal()) {
                        Some(i) => i,
                        None => {
                            let old = &mut self.requests[0];
                            old.state = RequestState::Cancelled;
                            old.trace.push((RequestState::Cancelled, now));
                            old.why = Some("Superseded by newer requests before anything was done.".into());
                            if self.dialogue.open().is_some_and(|o| o.request_id() == old.ids.request_id) {
                                self.dialogue.set_open(None);
                            }
                            0
                        }
                    };
                    self.requests.remove(i);
                }
                self.requests.push(Request { ids, transport, state: RequestState::Received, decision: None,
                                             trace: vec![(RequestState::Received, now)], held: None, why: None });
                rid
            }
            Interpretation::Confirmation { request_id } | Interpretation::Cancellation { request_id }
            | Interpretation::Correction { request_id, .. } | Interpretation::Clarify { request_id, .. }
            | Interpretation::PlanChange { request_id, .. } => request_id.clone(),
        };
        self.dialogue.say(kind, said, now, Some(&request_id));
        // Cancelling what waits on the owner ends it now. Stopping what is
        // RUNNING is decided by the runtime: work that changes something may be
        // past the point where stopping it is honest (transaction.rs).
        if had_open && matches!(interpretation, Interpretation::Cancellation { .. }) {
            let _ = self.advance(&request_id, RequestState::Cancelled, now, Some("You cancelled it."));
            self.dialogue.set_open(None);
        }
        (interpretation, request_id)
    }

    pub fn get(&self, request_id: &str) -> Option<&Request> {
        self.requests.iter().find(|r| r.ids.request_id == request_id)
    }

    fn get_mut(&mut self, request_id: &str) -> Option<&mut Request> {
        self.requests.iter_mut().find(|r| r.ids.request_id == request_id)
    }

    /// Moves a request. An illegal move is refused, counted, and reported; the
    /// request stays where it was.
    pub fn advance(&mut self, request_id: &str, to: RequestState, now: f64, why: Option<&str>)
        -> Result<(), IllegalTransition>
    {
        let Some(r) = self.requests.iter_mut().find(|r| r.ids.request_id == request_id) else {
            self.refused_transitions += 1;
            return Err(IllegalTransition { request_id: request_id.into(), from: RequestState::Received, to });
        };
        if !legal(r.state, to) {
            let err = IllegalTransition { request_id: request_id.into(), from: r.state, to };
            self.refused_transitions += 1;
            return Err(err);
        }
        r.state = to;
        r.trace.push((to, now));
        if let Some(w) = why { r.why = Some(w.to_string()); }
        // What was open for this request closes when it ends, and when it stops
        // waiting on the owner: once a confirmed move is underway, "stop" is
        // about the move, not an answer to a question no longer being asked.
        if to != RequestState::WaitingForUser
            && self.dialogue.open().is_some_and(|o| o.request_id() == request_id) {
            self.dialogue.set_open(None);
        }
        Ok(())
    }

    /// Records the governed decision, and moves to the state it implies.
    pub fn decide(&mut self, request_id: &str, d: Decision, now: f64) -> Result<(), IllegalTransition> {
        let to = match d {
            Decision::Answer => RequestState::Answering,
            Decision::Ask => RequestState::Asking,
            Decision::Suggest => RequestState::Suggesting,
            Decision::Plan => RequestState::Planning,
            Decision::Act => RequestState::Authorizing,
            Decision::Refuse => RequestState::Refused,
        };
        self.advance(request_id, to, now, None)?;
        if let Some(r) = self.get_mut(request_id) { r.decision = Some(d); }
        Ok(())
    }

    /// Moves a request that is about to act into ACTING, recording the decision
    /// to act on the way if nothing recorded it yet — work that runs straight
    /// away decides and acts in one call, and "why?" must still have an answer.
    pub fn begin_acting(&mut self, request_id: &str, now: f64) {
        let Some(r) = self.get(request_id) else { return };
        if r.decision.is_none() {
            if matches!(r.state, RequestState::Received | RequestState::Understanding | RequestState::Checking | RequestState::Deciding) {
                let _ = self.walk_to(request_id, RequestState::Deciding, now, None);
                let _ = self.decide(request_id, Decision::Act, now);
            } else if let Some(r) = self.get_mut(request_id) {
                r.decision = Some(Decision::Act);
            }
        }
        let _ = self.walk_to(request_id, RequestState::Acting, now, None);
    }

    /// Ties a request to a goal, a plan, a tool execution or a verification.
    pub fn bind(&mut self, request_id: &str, goal: Option<&str>, plan: Option<&str>,
                tool_execution: Option<&str>, verification: Option<&str>) {
        if let Some(r) = self.get_mut(request_id) {
            if let Some(g) = goal { r.ids.goal_id = Some(g.into()); }
            if let Some(p) = plan { r.ids.plan_id = Some(p.into()); }
            if let Some(x) = tool_execution { r.ids.tool_execution_id = Some(x.into()); }
            if let Some(v) = verification { r.ids.verification_id = Some(v.into()); }
        }
    }

    /// Opens something for the owner to answer. Only one thing is ever open.
    pub fn wait_for(&mut self, open: Open, now: f64) -> Result<(), IllegalTransition> {
        let rid = open.request_id().to_string();
        self.advance(&rid, RequestState::WaitingForUser, now, None)?;
        self.dialogue.set_open(Some(open));
        Ok(())
    }

    /// KUE says something in the conversation.
    pub fn say(&mut self, kind: TurnKind, said: &str, request_id: Option<&str>, now: f64) {
        debug_assert!(!kind.by_owner(), "KUE cannot speak in the owner's voice");
        let ids = request_id.and_then(|rid| self.get(rid)).map(|r| r.ids.clone());
        self.dialogue.say(kind, said, now, request_id);
        if let Some(ids) = ids {
            self.dialogue.tag_last(ids.goal_id.as_deref(), ids.tool_execution_id.as_deref(), ids.verification_id.as_deref());
        }
    }

    /// The kill switch: every live request ends KILLED and nothing stays open.
    pub fn kill(&mut self, now: f64) {
        for r in self.requests.iter_mut().filter(|r| !r.state.is_terminal()) {
            r.state = RequestState::Killed;
            r.trace.push((RequestState::Killed, now));
            r.why = Some("KUE was stopped.".into());
        }
        self.dialogue.close_everything();
    }

    /// Pause holds every live request where it is and closes what was open:
    /// nothing is confirmed across a pause.
    pub fn pause(&mut self, now: f64) {
        for r in self.requests.iter_mut().filter(|r| !r.state.is_terminal() && r.state != RequestState::Paused) {
            r.held = Some(r.state);
            r.state = RequestState::Paused;
            r.trace.push((RequestState::Paused, now));
        }
        self.dialogue.close_everything();
    }

    /// Resume returns each held request — except one that was waiting for the
    /// owner: whatever it was waiting for was closed by the pause, so it ends
    /// CANCELLED and has to be asked for again.
    pub fn resume(&mut self, now: f64) {
        for r in self.requests.iter_mut().filter(|r| r.state == RequestState::Paused) {
            let back = r.held.take().unwrap_or(RequestState::Received);
            let to = if back == RequestState::WaitingForUser { RequestState::Cancelled } else { back };
            r.state = to;
            r.trace.push((to, now));
            if to == RequestState::Cancelled { r.why = Some("Paused while waiting for you; ask again.".into()); }
        }
    }

    /// The live request a tool execution belongs to.
    pub fn by_tool_execution(&self, tool_execution_id: &str) -> Option<String> {
        self.requests.iter().rev()
            .find(|r| !r.state.is_terminal() && r.ids.tool_execution_id.as_deref() == Some(tool_execution_id))
            .map(|r| r.ids.request_id.clone())
    }

    /// The live request a goal belongs to.
    pub fn by_goal(&self, goal_id: &str) -> Option<String> {
        self.requests.iter().rev()
            .find(|r| !r.state.is_terminal() && r.ids.goal_id.as_deref() == Some(goal_id))
            .map(|r| r.ids.request_id.clone())
    }

    /// Moves a request to `target` along the shortest LEGAL path, recording
    /// every state on the way. Used when the work itself moved on — an action
    /// that ran, verified and finished inside one call — so the trace still
    /// shows AUTHORIZING → ACTING → VERIFYING rather than a jump. If no legal
    /// path exists, nothing moves and the refusal is counted.
    pub fn walk_to(&mut self, request_id: &str, target: RequestState, now: f64, why: Option<&str>)
        -> Result<(), IllegalTransition>
    {
        let Some(from) = self.get(request_id).map(|r| r.state) else {
            self.refused_transitions += 1;
            return Err(IllegalTransition { request_id: request_id.into(), from: RequestState::Received, to: target });
        };
        if from == target { return Ok(()); }
        let path = shortest_legal_path(from, target).ok_or_else(|| {
            IllegalTransition { request_id: request_id.into(), from, to: target }
        });
        let path = match path { Ok(p) => p, Err(e) => { self.refused_transitions += 1; return Err(e); } };
        let n = path.len();
        for (i, s) in path.into_iter().enumerate() {
            self.advance(request_id, s, now, if i + 1 == n { why } else { None })?;
        }
        Ok(())
    }

    /// The request a question is being answered for: the newest live request
    /// that was received and checked but has no decision yet. KUE answers one
    /// question at a time, so this is unambiguous.
    pub fn awaiting_decision(&self) -> Option<String> {
        self.requests.iter().rev()
            .find(|r| r.decision.is_none() && matches!(r.state, RequestState::Checking | RequestState::Deciding))
            .map(|r| r.ids.request_id.clone())
    }

    /// The request whose answer is being produced right now, if any.
    pub fn answering(&self) -> Option<String> {
        self.requests.iter().rev()
            .find(|r| r.state == RequestState::Answering)
            .map(|r| r.ids.request_id.clone())
    }

    /// The newest request that is still live.
    pub fn active(&self) -> Option<&Request> {
        self.requests.iter().rev().find(|r| !r.state.is_terminal())
    }

    pub fn requests(&self) -> &[Request] { &self.requests }

    /// The one runtime state. The overlay outranks everything: a killed KUE is
    /// KILLED whatever a request thinks, a paused one PAUSED.
    pub fn state(&self, o: Overlay) -> RuntimeState {
        use RequestState as R;
        use RuntimeState as S;
        if o.killed { return S::Killed; }
        if o.recovering { return S::Recovering; }
        if o.paused { return S::Paused; }
        if o.listening { return S::Listening; }
        let Some(r) = self.active() else {
            // Nothing live: say how the last one ended, briefly, or idle.
            return match self.requests.last().map(|r| r.state) {
                Some(R::Uncertain) => S::Uncertain,
                Some(R::Failed) => S::Failed,
                Some(R::Blocked) => S::Blocked,
                _ => S::Idle,
            };
        };
        match r.state {
            R::Received | R::Understanding => S::Understanding,
            R::Checking | R::Deciding => S::Checking,
            R::Answering | R::Suggesting => S::Checking,
            R::Asking => S::Asking,
            R::Planning => S::Planning,
            R::Authorizing => S::Authorizing,
            R::Acting => S::Acting,
            R::Verifying => S::Verifying,
            R::WaitingForUser => S::WaitingForUser,
            R::Paused => S::Paused,
            // Terminal states are never active; listed so the match stays exhaustive.
            R::Completed => S::Completed, R::Uncertain => S::Uncertain, R::Failed => S::Failed,
            R::Refused | R::Blocked => S::Blocked, R::Cancelled => S::Idle, R::Killed => S::Killed,
        }
    }
}

// MARK: - The conversation as the window shows it
//
// The window renders this and computes nothing: every label is decided here,
// from the turn's kind and the state of the request it belongs to, and every
// sentence is KUE's own or the owner's.

/// How one entry of the conversation is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Shown {
    UserTurn, KueTurn, Question, Suggestion, Progress,
    /// A plan is being worked out. Real waiting, and it can be slow.
    Planning,
    WaitingForConfirmation, WaitingForSelection,
    /// A checked plan is laid out and waiting for the owner.
    WaitingForPlan,
    Authorizing, Acting, Verifying,
    Result, Unverified, Blocked, Failed, Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShownTurn {
    pub shown: Shown,
    pub said: String,
    pub at: f64,
    pub request_id: Option<String>,
}

impl Pipeline {
    /// The last `limit` turns, each with how it is shown. An outcome is shown
    /// as a RESULT only when it verified: a turn about work that could not be
    /// verified is UNVERIFIED, one that was refused BLOCKED, and so on — by the
    /// request's own state, never by the wording.
    pub fn thread(&self, limit: usize) -> Vec<ShownTurn> {
        let turns = self.dialogue.turns();
        let start = turns.len().saturating_sub(limit);
        turns.iter().enumerate().skip(start).map(|(i, t)| {
            let state = t.request_id.as_deref().and_then(|r| self.get(r)).map(|r| r.state);
            let after_cancel = i > 0 && turns[i - 1].kind == TurnKind::UserCancellation
                && turns[i - 1].request_id == t.request_id;
            let shown = match t.kind {
                k if k.by_owner() => Shown::UserTurn,
                TurnKind::KueResponse if after_cancel => Shown::Cancelled,
                TurnKind::KueResponse => Shown::KueTurn,
                TurnKind::KueQuestion => Shown::Question,
                TurnKind::KueSuggestion => Shown::Suggestion,
                TurnKind::ToolProgress | TurnKind::GoalProgress => Shown::Progress,
                TurnKind::VerificationResult => Shown::Result,
                TurnKind::ToolResult => match state {
                    Some(RequestState::Uncertain) => Shown::Unverified,
                    Some(RequestState::Refused | RequestState::Blocked) => Shown::Blocked,
                    Some(RequestState::Cancelled | RequestState::Killed) => Shown::Cancelled,
                    _ => Shown::Failed,
                },
                _ => Shown::KueTurn,
            };
            ShownTurn { shown, said: t.said.clone(), at: t.at, request_id: t.request_id.clone() }
        }).collect()
    }

    /// What KUE is doing right now, in one line, when it is doing something the
    /// owner should see: waiting on them, on macOS, acting, or checking. (An
    /// answer being written shows itself, as it streams.)
    pub fn now_line(&self) -> Option<ShownTurn> {
        let r = self.active()?;
        let (shown, said) = match (self.dialogue.open(), r.state) {
            (Some(Open::Confirmation { .. }), _) => (Shown::WaitingForConfirmation, "Waiting for you to confirm — say “do it” or “cancel”."),
            // "Do it" only when there is a set KUE would move; otherwise the
            // choice is the sheet's.
            (Some(Open::Selection { proposed, .. }), _) if proposed.is_empty() =>
                (Shown::WaitingForSelection, "Waiting for you — choose in the storage sheet, or tell me what to leave out."),
            (Some(Open::Selection { .. }), _) => (Shown::WaitingForSelection, "Waiting for you — say “do it”, or what to leave out."),
            (Some(Open::Question { .. }), _) => (Shown::Question, "Waiting for your answer."),
            // A plan is laid out. Nothing happens until the owner says so, and
            // what they can say is said here rather than left to be guessed.
            (Some(Open::Plan { .. }), _) =>
                (Shown::WaitingForPlan, "Waiting for you — say “do it”, tell me what to change, or “cancel”."),
            // Working out a plan is the slowest thing KUE does: the model is
            // reading the whole tool catalogue. It says so, and says nothing
            // about how far along it is, because it does not know.
            (None, RequestState::Planning) => (Shown::Planning, "Working out a plan on this Mac."),
            (None, RequestState::Authorizing) => (Shown::Authorizing, "Waiting for macOS to confirm it's you."),
            (None, RequestState::Acting) => (Shown::Acting, "Working on it."),
            (None, RequestState::Verifying) => (Shown::Verifying, "Checking that it actually happened."),
            _ => return None,
        };
        Some(ShownTurn { shown, said: said.into(), at: r.trace.last().map(|(_, t)| *t).unwrap_or(0.0),
                         request_id: Some(r.ids.request_id.clone()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RequestState as R;

    #[test]
    fn the_thread_labels_outcomes_by_the_request_state_not_the_words() {
        let mut p = Pipeline::new("c1");
        let (_, rid) = p.receive(Transport::Typed, "open Safari", 1.0);
        let _ = p.walk_to(&rid, R::Acting, 1.1, None);
        assert_eq!(p.now_line().map(|n| n.shown), Some(Shown::Acting));
        let _ = p.walk_to(&rid, R::Uncertain, 1.2, None);
        // Even a sentence that sounds like success is not shown as a result.
        p.say(TurnKind::ToolResult, "Safari is open.", Some(&rid), 1.3);
        let shown: Vec<_> = p.thread(10).into_iter().map(|t| t.shown).collect();
        assert_eq!(shown, [Shown::UserTurn, Shown::Unverified]);
        assert!(p.now_line().is_none(), "nothing is live");

        let (_, r2) = p.receive(Transport::Typed, "move these to the Trash", 2.0);
        let _ = p.walk_to(&r2, R::Refused, 2.1, None);
        p.say(TurnKind::ToolResult, "I can't do that: no.", Some(&r2), 2.2);
        assert_eq!(p.thread(1)[0].shown, Shown::Blocked);
    }

    fn to(p: &mut Pipeline, rid: &str, states: &[RequestState], t: &mut f64) {
        for s in states { *t += 0.1; p.advance(rid, *s, *t, None).unwrap_or_else(|e| panic!("{e:?}")); }
    }

    #[test]
    fn a_request_cannot_claim_to_have_finished_without_acting_and_verifying() {
        let mut p = Pipeline::new("c1");
        let (_, rid) = p.receive(Transport::Typed, "move these to the Trash", 1.0);
        for skip in [R::Completed, R::Acting, R::Verifying] {
            assert!(p.advance(&rid, skip, 1.1, None).is_err(), "RECEIVED → {skip:?} was allowed");
        }
        assert_eq!(p.refused_transitions, 3, "each refused jump is counted, not silently ignored");
        let mut t = 1.0;
        to(&mut p, &rid, &[R::Understanding, R::Checking, R::Deciding], &mut t);
        p.decide(&rid, Decision::Act, 2.0).unwrap();
        // From AUTHORIZING, completion still needs acting and verifying.
        assert!(p.advance(&rid, R::Completed, 2.1, None).is_err());
        to(&mut p, &rid, &[R::Acting, R::Verifying, R::Completed], &mut t);
        assert_eq!(p.get(&rid).unwrap().trace.len(), 8);
        // And a finished request stays finished.
        assert!(p.advance(&rid, R::Acting, 9.0, None).is_err());
    }

    #[test]
    fn every_transport_enters_the_same_pipeline() {
        for tr in [Transport::Voice, Transport::Typed, Transport::Proactive, Transport::ComputerContext, Transport::Scheduled] {
            let mut p = Pipeline::new("c");
            let (i, rid) = p.receive(tr, "check my storage", 1.0);
            assert_eq!(i, Interpretation::NewRequest);
            let r = p.get(&rid).unwrap();
            assert_eq!((r.state, r.transport), (R::Received, tr));
            assert!(legal(R::Received, R::Understanding));
        }
    }

    /// The exchange the owner described, end to end, as the runtime sees it.
    #[test]
    fn clean_up_my_downloads_then_do_it_then_done() {
        use crate::dialogue::{Facet, Open};
        use crate::storage::Category;
        let mut p = Pipeline::new("c1");
        let mut t = 0.0;

        // "Clean up my Downloads."
        let (_, rid) = p.receive(Transport::Voice, "Clean up my Downloads.", t);
        to(&mut p, &rid, &[R::Understanding, R::Checking, R::Deciding], &mut t);
        p.decide(&rid, Decision::Plan, t).unwrap();
        p.bind(&rid, Some("g1"), Some("p1"), None, None);
        // The plan inspected and laid out candidates; KUE waits for a choice.
        p.say(TurnKind::KueResponse, "I found 18 candidates. 4 are recent, so I excluded them. 14 appear safe to move to the Trash.", Some(&rid), t);
        p.wait_for(Open::Selection { request_id: rid.clone(), goal_id: "g1".into(),
            facets: vec![Facet::Storage(Category::Installer), Facet::Storage(Category::OldDownload)], proposed: vec![] }, t).unwrap();
        assert_eq!(p.state(Overlay::default()), RuntimeState::WaitingForUser);

        // "Do it." — a confirmation of the open plan, not a new request.
        let (i, same) = p.receive(Transport::Voice, "Do it.", t + 1.0);
        assert_eq!(i, Interpretation::Confirmation { request_id: rid.clone() });
        assert_eq!(same, rid, "\"do it\" belongs to the request it answers");
        // Confirmation goes to AUTHORIZING — where Touch ID is asked for. It is
        // never a shortcut past it.
        p.advance(&rid, R::Authorizing, t + 1.1, None).unwrap();
        p.say(TurnKind::KueQuestion, "Before I move them, macOS requires Touch ID.", Some(&rid), t + 1.2);
        assert_eq!(p.state(Overlay::default()), RuntimeState::Authorizing);

        p.bind(&rid, None, None, Some("x1"), None);
        to(&mut p, &rid, &[R::Acting, R::Verifying], &mut t);
        p.bind(&rid, None, None, None, Some("v1"));
        p.say(TurnKind::VerificationResult, "Done. I verified all 14 are in Trash.", Some(&rid), t);
        p.advance(&rid, R::Completed, t, None).unwrap();

        let ids = &p.get(&rid).unwrap().ids;
        assert_eq!((ids.conversation_id.as_str(), ids.goal_id.as_deref(), ids.plan_id.as_deref(),
                    ids.tool_execution_id.as_deref(), ids.verification_id.as_deref()),
                   ("c1", Some("g1"), Some("p1"), Some("x1"), Some("v1")));
        // The last thing KUE said carries every id of the work it reports.
        let last = p.dialogue.turns().last().unwrap();
        assert_eq!((last.kind, last.verification_id.as_deref()), (TurnKind::VerificationResult, Some("v1")));
        assert_eq!(p.state(Overlay::default()), RuntimeState::Idle);
    }

    #[test]
    fn a_correction_revises_the_plan_instead_of_starting_over() {
        use crate::dialogue::{Facet, Open, Revision};
        use crate::storage::Category;
        let mut p = Pipeline::new("c1");
        let mut t = 0.0;
        let (_, rid) = p.receive(Transport::Voice, "Clean up my Downloads.", t);
        to(&mut p, &rid, &[R::Understanding, R::Checking, R::Deciding], &mut t);
        p.decide(&rid, Decision::Plan, t).unwrap();
        p.wait_for(Open::Selection { request_id: rid.clone(), goal_id: "g1".into(),
            facets: vec![Facet::Storage(Category::Installer), Facet::Storage(Category::OldDownload)], proposed: vec![] }, t).unwrap();

        let (i, same) = p.receive(Transport::Voice, "No, don't touch the installers.", t + 1.0);
        assert_eq!(i, Interpretation::Correction { request_id: rid.clone(),
                                                   revision: Revision::Exclude(vec![Facet::Storage(Category::Installer)]) });
        assert_eq!(same, rid, "a correction belongs to the plan it corrects");
        assert_eq!(p.requests().len(), 1, "no new request was started");
        // The runtime revises: back to PLANNING on the same request.
        p.advance(&rid, R::Planning, t + 1.1, Some("Revised: installers left out.")).unwrap();
        assert_eq!(p.dialogue.turns().iter().filter(|t| t.kind == TurnKind::UserCorrection).count(), 1);
    }

    #[test]
    fn kill_ends_every_live_request_and_closes_what_was_open() {
        use crate::dialogue::Open;
        let mut p = Pipeline::new("c1");
        let (_, a) = p.receive(Transport::Typed, "open Safari", 1.0);
        let (_, b) = p.receive(Transport::Voice, "check my storage", 1.0);
        let mut t = 1.0;
        to(&mut p, &b, &[R::Understanding, R::Checking, R::Deciding], &mut t);
        p.decide(&b, Decision::Act, t).unwrap();
        p.wait_for(Open::Confirmation { request_id: b.clone(), tool_execution_id: "x".into() }, t).unwrap();

        p.kill(5.0);
        for rid in [&a, &b] { assert_eq!(p.get(rid).unwrap().state, R::Killed); }
        assert!(p.dialogue.open().is_none(), "nothing waits across a kill");
        // "Do it" after a kill confirms nothing.
        assert_eq!(p.receive(Transport::Voice, "do it", 6.0).0, Interpretation::NewRequest);
        assert_eq!(p.state(Overlay { killed: true, ..Default::default() }), RuntimeState::Killed);
        // And a killed request cannot be revived.
        assert!(p.advance(&b, R::Acting, 7.0, None).is_err());
    }

    #[test]
    fn nothing_is_confirmed_across_a_pause() {
        use crate::dialogue::Open;
        let mut p = Pipeline::new("c1");
        let (_, rid) = p.receive(Transport::Voice, "move them to the Trash", 1.0);
        let mut t = 1.0;
        to(&mut p, &rid, &[R::Understanding, R::Checking, R::Deciding], &mut t);
        p.decide(&rid, Decision::Act, t).unwrap();
        p.wait_for(Open::Confirmation { request_id: rid.clone(), tool_execution_id: "x".into() }, t).unwrap();

        p.pause(3.0);
        assert_eq!(p.state(Overlay { paused: true, ..Default::default() }), RuntimeState::Paused);
        assert!(p.dialogue.open().is_none());
        p.resume(4.0);
        assert_eq!(p.get(&rid).unwrap().state, R::Cancelled,
            "a confirmation that was pending before a pause is not waiting after it");
        assert_eq!(p.receive(Transport::Voice, "yes", 5.0).0, Interpretation::NewRequest);
    }

    #[test]
    fn a_result_kue_could_not_verify_is_never_completed() {
        let mut p = Pipeline::new("c1");
        let (_, rid) = p.receive(Transport::Typed, "quit Notes", 1.0);
        let mut t = 1.0;
        to(&mut p, &rid, &[R::Understanding, R::Checking, R::Deciding], &mut t);
        p.decide(&rid, Decision::Act, t).unwrap();
        to(&mut p, &rid, &[R::Acting, R::Verifying, R::Uncertain], &mut t);
        assert_eq!(p.state(Overlay::default()), RuntimeState::Uncertain);
        assert!(p.advance(&rid, R::Completed, t + 1.0, None).is_err(),
            "UNCERTAIN may not be upgraded to COMPLETED after the fact");
    }

    #[test]
    fn walking_to_a_state_records_every_legal_step_and_refuses_shortcuts() {
        let mut p = Pipeline::new("c1");
        let (_, rid) = p.receive(Transport::Typed, "open Safari", 1.0);
        p.walk_to(&rid, R::Completed, 2.0, Some("Safari is open and in front.")).unwrap();
        let states: Vec<_> = p.get(&rid).unwrap().trace.iter().map(|(s, _)| *s).collect();
        assert_eq!(states, vec![R::Received, R::Understanding, R::Checking, R::Deciding, R::Answering, R::Completed],
            "the shortest legal path from RECEIVED to COMPLETED answers; acting would pass through VERIFYING");
        // From AUTHORIZING, completion necessarily passes through ACTING and VERIFYING.
        let (_, act) = p.receive(Transport::Typed, "quit Notes", 3.0);
        p.walk_to(&act, R::Authorizing, 3.1, None).unwrap();
        p.walk_to(&act, R::Completed, 3.2, None).unwrap();
        let tail: Vec<_> = p.get(&act).unwrap().trace.iter().rev().take(3).map(|(s, _)| *s).collect();
        assert_eq!(tail, vec![R::Completed, R::Verifying, R::Acting]);
        // And a terminal request goes nowhere.
        assert!(p.walk_to(&act, R::Acting, 4.0, None).is_err());
    }

    #[test]
    fn the_overlay_outranks_every_request() {
        let mut p = Pipeline::new("c1");
        let (_, rid) = p.receive(Transport::Typed, "open Safari", 1.0);
        p.advance(&rid, R::Understanding, 1.1, None).unwrap();
        assert_eq!(p.state(Overlay::default()), RuntimeState::Understanding);
        assert_eq!(p.state(Overlay { listening: true, ..Default::default() }), RuntimeState::Listening);
        assert_eq!(p.state(Overlay { paused: true, listening: true, ..Default::default() }), RuntimeState::Paused);
        assert_eq!(p.state(Overlay { killed: true, paused: true, ..Default::default() }), RuntimeState::Killed);
        assert_eq!(p.state(Overlay { recovering: true, ..Default::default() }), RuntimeState::Recovering);
    }

    #[test]
    fn the_pipeline_is_bounded_and_prefers_to_forget_finished_work() {
        let mut p = Pipeline::new("c1");
        let (_, keep) = p.receive(Transport::Typed, "open Safari", 0.0);
        let mut t = 0.0;
        // Forty finished requests…
        for i in 0..40 {
            let (_, rid) = p.receive(Transport::Typed, "what is 2 plus 2", i as f64);
            to(&mut p, &rid, &[R::Understanding, R::Checking, R::Deciding], &mut t);
            p.decide(&rid, Decision::Answer, t).unwrap();
            p.advance(&rid, R::Completed, t, None).unwrap();
        }
        assert!(p.requests().len() <= 40, "unbounded");
        assert!(p.get(&keep).is_some(), "a live request was dropped while finished ones remained");

        // …and when every request is live, the oldest is ended with a reason
        // before it goes, never dropped while it looks unfinished.
        let mut q = Pipeline::new("c2");
        for i in 0..100 { q.receive(Transport::Typed, "hello", i as f64); }
        assert!(q.requests().len() <= 40);
    }
}
