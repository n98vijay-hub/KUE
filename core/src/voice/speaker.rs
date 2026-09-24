//! The speech queue: the one path from a cleared sentence to a voice provider.
//!
//! Nothing reaches a provider except through `SpeechController::submit`, which
//! accepts only a `Cleared<SpeechDraft>` — a value only the privacy firewall can
//! make, after `policy::clear_for_speech`. The window has no command that
//! speaks text; it can only stop speech and change how KUE sounds.
//!
//! One sentence is with the provider at a time, so the queue — its order, what
//! is dropped, and each request's state — is decided here, in core, not by the
//! synthesizer:
//!
//!   QUEUED → SPEAKING → COMPLETED
//!                     ↘ CANCELLING → CANCELLED      (stopped, interrupted)
//!                     ↘ FAILED                      (provider error, never started)
//!   QUEUED → CANCELLED (superseded, stopped, kill, pause, lock, you speaking)
//!   refused by the gate, or at dispatch → BLOCKED
//!
//! and the controller is IDLE when nothing is queued or being said.
//!
//! What stops speech, and what it drops:
//!   KILLED         everything, at once; nothing new is accepted while killed.
//!   PAUSED         everything; while paused only results and questions for you
//!                  may be said, never progress or answers.
//!   LOCKED         everything, when the session stops being yours (locked, a stranger,
//!                  a second person). A dip in identity confidence is not a lock: what
//!                  is being said finishes, and a sentence built from your data waits
//!                  up to OWNER_WAIT_SECONDS for LEVEL_2 before it is dropped, unsaid.
//!   USER_SPEAKING  everything: the microphone went live.
//!   TASK_CANCELLED what was queued for that action.
//!   a newer sentence for the same action replaces one still waiting (SUPERSEDED);
//!   AUTHORIZATION_REQUIRED or CRITICAL_SAFETY cuts off anything lower (INTERRUPTED).
//!
//! Every request ends in exactly one audit entry, which carries no text.

use super::policy::{microphone_busy, session_not_yours, SpeechRefusal};
use super::provider::{choose_voice, VoiceChoice, VoiceInfo, VoiceProviderId};
use super::{Priority, SpeechDraft, VoiceSettings};
use crate::authz::AuthLevel;
use crate::engine::Engine;
use crate::privacy::Cleared;
use serde::Serialize;

/// A voice: something that can say one sentence now and stop.
pub trait SpeechProvider {
    fn id(&self) -> VoiceProviderId;
    /// Whether it can speak at all (its process is running).
    fn is_available(&mut self) -> bool;
    /// Starts one sentence now. `interrupt` cuts off whatever it is saying.
    fn speak(&mut self, request_id: &str, text: &str, voice: Option<&str>, rate: f64, volume: f64, interrupt: bool) -> Result<(), String>;
    /// Stops speaking now and drops anything it holds.
    fn cancel(&mut self);
    fn is_speaking(&self) -> bool;
    fn available_voices(&self) -> Vec<VoiceInfo>;
    fn select_voice(&self, requested: Option<&str>, language: &str) -> VoiceChoice {
        choose_voice(&self.available_voices(), requested, language)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpeechState {
    Idle,
    Queued,
    Speaking,
    Cancelling,
    Cancelled,
    Completed,
    Blocked,
    Failed,
}

impl SpeechState {
    pub fn is_terminal(self) -> bool {
        matches!(self, SpeechState::Cancelled | SpeechState::Completed | SpeechState::Blocked | SpeechState::Failed)
    }
}

/// Why a request did not complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpeechStop {
    Killed,
    Paused,
    Locked,
    UserSpeaking,
    TaskCancelled,
    StoppedByYou,
    Superseded,
    Interrupted,
    Duplicate,
    QueueFull,
    /// Refused by the speech gate (`policy::clear_for_speech`).
    Refused(SpeechRefusal),
    /// Needed the owner when it was due to be said, and the owner was not confirmed.
    OwnerRequired,
    ProviderUnavailable,
    ProviderFailed(String),
    /// Sent, and the provider never said it started.
    ProviderDidNotStart,
    ProviderStopped,
}

/// One sentence KUE was asked to say. Its text is held only until it ends and
/// is never serialized: the window and the audit see what it was about, not what it said.
#[derive(Debug, Clone, Serialize)]
pub struct SpeechRequest {
    /// "s7". Unique for this launch.
    pub id: String,
    /// The action this narrates ("a3"), or "answer".
    pub topic: String,
    pub priority: Priority,
    pub state: SpeechState,
    pub stop: Option<SpeechStop>,
    pub provider: Option<VoiceProviderId>,
    pub requested_at: f64,
    pub sent_at: Option<f64>,
    pub ended_at: Option<f64>,
    #[serde(skip)]
    text: String,
    #[serde(skip)]
    needs_owner: bool,
}

/// What the audit records for a request that ended. No text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeechAudit {
    pub request_id: String,
    pub topic: String,
    pub priority: Priority,
    pub state: SpeechState,
    pub stop: Option<SpeechStop>,
    pub at: f64,
}

impl SpeechAudit {
    pub fn summary(&self) -> String {
        let stop = match &self.stop {
            Some(s) => format!(" ({})", serde_json::to_value(s).ok()
                .map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).unwrap_or_default()),
            None => String::new(),
        };
        format!("Speech {} for {} ({}): {}{stop}.", self.request_id, self.topic,
            tag(&self.priority), tag(&self.state))
    }
}

fn tag<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|x| x.as_str().map(String::from)).unwrap_or_default()
}

/// The runtime facts the queue obeys, read from the engine at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechGate {
    pub killed: bool,
    pub paused: bool,
    /// LEVEL_2 or above: the owner, confirmed.
    pub owner_present: bool,
    /// The session is not the owner's: locked, someone unknown, or more than one person.
    /// Not a brief dip in identity confidence (a head turn), which only makes speech wait.
    pub session_locked: bool,
    pub microphone_busy: bool,
}

impl SpeechGate {
    pub fn of(engine: &Engine, now: f64) -> SpeechGate {
        SpeechGate {
            killed: engine.is_killed(),
            paused: engine.is_paused(),
            owner_present: engine.access_block(now).level >= AuthLevel::Level2,
            session_locked: session_not_yours(&engine.access_block(now)),
            microphone_busy: microphone_busy(engine, now),
        }
    }
}

/// A provider that has not said a sent sentence started within this long has failed.
pub const START_TIMEOUT_SECONDS: f64 = 10.0;
/// A provider that has not confirmed a stop within this long is taken as stopped.
pub const CANCEL_TIMEOUT_SECONDS: f64 = 3.0;
pub const MAX_QUEUED: usize = 6;
/// A sentence built from your data waits this long for you to be confirmed again
/// after a dip in identity confidence; then it is dropped, unsaid.
pub const OWNER_WAIT_SECONDS: f64 = 8.0;
const HISTORY: usize = 40;

/// How a provider reports a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEvent {
    Started,
    Finished,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeechStatus {
    pub state: SpeechState,
    pub speaking: Option<SpeechRequest>,
    pub queued: Vec<SpeechRequest>,
    pub recent: Vec<SpeechRequest>,
}

pub struct SpeechController {
    pub settings: VoiceSettings,
    pub language: String,
    next: u64,
    requests: Vec<SpeechRequest>,
    audit: Vec<SpeechAudit>,
    last: Option<SpeechGate>,
    /// The words of sentences sent to the voice, by request id, kept until
    /// `echo::ECHO_WINDOW_SECONDS` after each ends so KUE can recognise its own
    /// voice coming back through the microphone. Memory only; never serialized.
    own: Vec<(String, crate::voice::echo::OwnSentence)>,
}

impl SpeechController {
    pub fn new(settings: VoiceSettings, language: impl Into<String>) -> Self {
        SpeechController { settings, language: language.into(), next: 0, requests: Vec::new(), audit: Vec::new(), last: None,
            own: Vec::new() }
    }

    fn new_id(&mut self) -> String { self.next += 1; format!("s{}", self.next) }

    /// The one in flight: with the provider, not yet ended.
    fn in_flight(&self) -> Option<usize> {
        self.requests.iter().position(|r| matches!(r.state, SpeechState::Speaking | SpeechState::Cancelling)
            || (r.state == SpeechState::Queued && r.sent_at.is_some()))
    }

    pub fn state(&self) -> SpeechState {
        match self.in_flight().map(|i| self.requests[i].state) {
            Some(SpeechState::Cancelling) => SpeechState::Cancelling,
            Some(_) => SpeechState::Speaking,
            None if self.requests.iter().any(|r| r.state == SpeechState::Queued) => SpeechState::Queued,
            None => SpeechState::Idle,
        }
    }

    pub fn request(&self, id: &str) -> Option<&SpeechRequest> { self.requests.iter().find(|r| r.id == id) }

    pub fn status(&self) -> SpeechStatus {
        SpeechStatus {
            state: self.state(),
            speaking: self.in_flight().map(|i| self.requests[i].clone()),
            queued: self.requests.iter().filter(|r| r.state == SpeechState::Queued && r.sent_at.is_none()).cloned().collect(),
            recent: self.requests.iter().rev().filter(|r| r.state.is_terminal()).take(10).cloned().collect(),
        }
    }

    /// What KUE is saying or said within the echo window, as words. Older
    /// sentences are dropped here.
    pub fn own_speech(&mut self, now: f64) -> Vec<crate::voice::echo::OwnSentence> {
        self.own.retain(|(_, s)| s.is_recent(now));
        self.own.iter().map(|(_, s)| s.clone()).collect()
    }

    /// Audit entries for requests that ended since the last call.
    pub fn take_audit(&mut self) -> Vec<SpeechAudit> { std::mem::take(&mut self.audit) }

    fn end(&mut self, i: usize, state: SpeechState, stop: Option<SpeechStop>, now: f64) {
        let r = &mut self.requests[i];
        if r.state.is_terminal() { return; }
        r.state = state;
        r.stop = stop;
        r.ended_at = Some(now);
        r.text.clear();
        let rid = r.id.clone();
        if let Some((_, s)) = self.own.iter_mut().find(|(id, _)| *id == rid) { s.ended_at = Some(now); }
        self.audit.push(SpeechAudit { request_id: r.id.clone(), topic: r.topic.clone(), priority: r.priority,
            state, stop: r.stop.clone(), at: now });
    }

    fn prune(&mut self) {
        while self.requests.len() > HISTORY {
            match self.requests.iter().position(|r| r.state.is_terminal()) {
                Some(i) => { self.requests.remove(i); }
                None => break,
            }
        }
    }

    /// Records a sentence the gate refused, so a refusal is audited like any other.
    pub fn refused(&mut self, draft: &SpeechDraft, why: SpeechRefusal, now: f64) -> String {
        let id = self.new_id();
        self.requests.push(SpeechRequest { id: id.clone(), topic: draft.topic.clone(), priority: draft.priority,
            state: SpeechState::Queued, stop: None, provider: None, requested_at: now, sent_at: None, ended_at: None,
            text: String::new(), needs_owner: false });
        let i = self.requests.len() - 1;
        self.end(i, SpeechState::Blocked, Some(SpeechStop::Refused(why)), now);
        self.prune();
        id
    }

    /// Queues a cleared sentence and says it when its turn comes. Returns its request id.
    pub fn submit(&mut self, sentence: Cleared<SpeechDraft>, provider_id: VoiceProviderId, gate: SpeechGate,
                  provider: &mut dyn SpeechProvider, now: f64) -> String {
        let d = sentence.into_value();
        let id = self.new_id();
        self.requests.push(SpeechRequest { id: id.clone(), topic: d.topic.clone(), priority: d.priority,
            state: SpeechState::Queued, stop: None, provider: Some(provider_id), requested_at: now, sent_at: None,
            ended_at: None, needs_owner: !d.carries.is_empty(), text: d.text.clone() });
        let new = self.requests.len() - 1;

        // Checked here as well as in the gate: the queue never holds speech while killed.
        if gate.killed { self.end(new, SpeechState::Blocked, Some(SpeechStop::Killed), now); return id; }
        if gate.paused && d.priority < Priority::ActionResult {
            self.end(new, SpeechState::Blocked, Some(SpeechStop::Paused), now);
            return id;
        }
        let duplicate = self.requests[..new].iter().any(|r| !r.state.is_terminal() && r.text == d.text);
        if duplicate { self.end(new, SpeechState::Blocked, Some(SpeechStop::Duplicate), now); return id; }

        // A newer sentence about the same action replaces one still waiting.
        let stale: Vec<usize> = (0..new).filter(|&i| {
            let r = &self.requests[i];
            r.state == SpeechState::Queued && r.sent_at.is_none() && r.topic == d.topic
        }).collect();
        for i in stale { self.end(i, SpeechState::Cancelled, Some(SpeechStop::Superseded), now); }

        let waiting: Vec<usize> = (0..self.requests.len())
            .filter(|&i| self.requests[i].state == SpeechState::Queued && self.requests[i].sent_at.is_none()).collect();
        if waiting.len() > MAX_QUEUED {
            // The lowest priority, oldest first.
            let drop = *waiting.iter().min_by_key(|&&i| (self.requests[i].priority, self.requests[i].requested_at as i64)).unwrap();
            self.end(drop, SpeechState::Cancelled, Some(SpeechStop::QueueFull), now);
        }

        let interrupting = self.in_flight().filter(|&i| d.priority >= Priority::AuthorizationRequired
            && self.requests[i].priority < d.priority && self.requests[i].state != SpeechState::Cancelling
            && !gate.microphone_busy && (!self.requests[new].needs_owner || gate.owner_present));
        if let Some(i) = interrupting {
            // The provider cuts it off as it starts the new sentence.
            self.end(i, SpeechState::Cancelled, Some(SpeechStop::Interrupted), now);
            let r = self.requests.iter().position(|r| r.id == id).unwrap();
            self.dispatch(r, gate, provider, true, now);
            if self.requests[r].sent_at.is_none() {
                // The new one may not be said after all; the old one is still stopped.
                provider.cancel();
                self.pump(gate, provider, now);
            }
        } else {
            self.pump(gate, provider, now);
        }
        self.prune();
        id
    }

    /// Sends request `i` to the provider, ends it if it may not be said, or leaves
    /// it waiting for the owner to be confirmed again.
    fn dispatch(&mut self, i: usize, gate: SpeechGate, provider: &mut dyn SpeechProvider, interrupt: bool, now: f64) {
        if self.requests[i].needs_owner && !gate.owner_present {
            if gate.session_locked || now - self.requests[i].requested_at > OWNER_WAIT_SECONDS {
                return self.end(i, SpeechState::Blocked, Some(SpeechStop::OwnerRequired), now);
            }
            return;
        }
        if gate.paused && self.requests[i].priority < Priority::ActionResult {
            return self.end(i, SpeechState::Blocked, Some(SpeechStop::Paused), now);
        }
        if !provider.is_available() {
            return self.end(i, SpeechState::Failed, Some(SpeechStop::ProviderUnavailable), now);
        }
        let voice = provider.select_voice(self.settings.voice.as_deref(), &self.language).voice.map(|v| v.identifier);
        let (rate, volume) = (self.settings.provider_rate(), self.settings.volume);
        let (id, text) = (self.requests[i].id.clone(), self.requests[i].text.clone());
        match provider.speak(&id, &text, voice.as_deref(), rate, volume, interrupt) {
            Ok(()) => {
                self.requests[i].sent_at = Some(now);
                self.own.push((id.clone(), crate::voice::echo::OwnSentence::new(&text, now)));
            }
            Err(e) => self.end(i, SpeechState::Failed, Some(SpeechStop::ProviderFailed(e)), now),
        }
    }

    /// Applies the runtime state and sends the next sentence when the provider is free.
    /// Call it on the shell's clock, and after anything that changes the gate.
    pub fn pump(&mut self, gate: SpeechGate, provider: &mut dyn SpeechProvider, now: f64) {
        let before = self.last.replace(gate);
        if gate.killed { return self.stop_all(SpeechStop::Killed, provider, now); }
        // Transitions only: a sentence accepted in the current state is not stopped by it.
        if gate.paused && before.is_some_and(|b| !b.paused) { self.stop_all(SpeechStop::Paused, provider, now); }
        if gate.session_locked && before.is_some_and(|b| !b.session_locked) { self.stop_all(SpeechStop::Locked, provider, now); }
        if gate.microphone_busy && before.is_some_and(|b| !b.microphone_busy) { self.stop_all(SpeechStop::UserSpeaking, provider, now); }

        if let Some(i) = self.in_flight() {
            let r = &self.requests[i];
            match r.state {
                SpeechState::Queued if now - r.sent_at.unwrap_or(now) > START_TIMEOUT_SECONDS => {
                    provider.cancel();
                    self.end(i, SpeechState::Failed, Some(SpeechStop::ProviderDidNotStart), now);
                }
                SpeechState::Cancelling if now - r.ended_at.or(r.sent_at).unwrap_or(now) > CANCEL_TIMEOUT_SECONDS => {
                    let stop = r.stop.clone();
                    self.end(i, SpeechState::Cancelled, stop, now);
                }
                _ => return,
            }
        }
        if gate.microphone_busy { return; }
        for next in self.queued_in_order() {
            self.dispatch(next, gate, provider, false, now);
            if self.in_flight().is_some() { break; }
        }
    }

    /// Waiting requests, highest priority first; among equals, first asked.
    fn queued_in_order(&self) -> Vec<usize> {
        let mut q: Vec<usize> = (0..self.requests.len())
            .filter(|&i| self.requests[i].state == SpeechState::Queued && self.requests[i].sent_at.is_none()).collect();
        q.sort_by(|&a, &b| self.requests[b].priority.cmp(&self.requests[a].priority)
            .then(self.requests[a].requested_at.total_cmp(&self.requests[b].requested_at)));
        q
    }

    /// A report from the provider about one of its sentences.
    pub fn on_provider_event(&mut self, request_id: &str, event: ProviderEvent, gate: SpeechGate,
                             provider: &mut dyn SpeechProvider, now: f64) {
        let Some(i) = self.requests.iter().position(|r| r.id == request_id) else { return };
        let state = self.requests[i].state;
        match event {
            ProviderEvent::Started if state.is_terminal() => {
                // Stopped before the provider got to it: stop it again — unless a
                // newer sentence is already with the provider, which the stop would cut.
                // (While killed nothing is in flight, so this always stops it.)
                if self.in_flight().is_none() { provider.cancel(); }
            }
            ProviderEvent::Started if state == SpeechState::Queued => self.requests[i].state = SpeechState::Speaking,
            ProviderEvent::Started => {}
            ProviderEvent::Finished if state == SpeechState::Cancelling => {
                let stop = self.requests[i].stop.clone();
                self.end(i, SpeechState::Cancelled, stop, now);
            }
            ProviderEvent::Finished => self.end(i, SpeechState::Completed, None, now),
            ProviderEvent::Cancelled => {
                let stop = self.requests[i].stop.clone().or(Some(SpeechStop::ProviderStopped));
                self.end(i, SpeechState::Cancelled, stop, now);
            }
            ProviderEvent::Failed(why) => self.end(i, SpeechState::Failed, Some(SpeechStop::ProviderFailed(why)), now),
        }
        self.pump(gate, provider, now);
    }

    /// The provider's process ended: nothing it held will be said.
    pub fn provider_gone(&mut self, now: f64) {
        if let Some(i) = self.in_flight() {
            let stop = self.requests[i].stop.clone();
            match self.requests[i].state {
                SpeechState::Cancelling => self.end(i, SpeechState::Cancelled, stop, now),
                _ => self.end(i, SpeechState::Failed, Some(SpeechStop::ProviderStopped), now),
            }
        }
    }

    /// Stops what is being said and drops everything queued.
    pub fn stop_all(&mut self, why: SpeechStop, provider: &mut dyn SpeechProvider, now: f64) {
        let flight = self.in_flight();
        let queued: Vec<usize> = (0..self.requests.len())
            .filter(|&i| self.requests[i].state == SpeechState::Queued && Some(i) != flight).collect();
        for i in queued { self.end(i, SpeechState::Cancelled, Some(why.clone()), now); }
        if let Some(i) = flight { self.cut_off(i, why, provider, now); }
    }

    /// Stops and drops what was queued for one action (its task was cancelled).
    pub fn cancel_topic(&mut self, topic: &str, provider: &mut dyn SpeechProvider, now: f64) {
        let flight = self.in_flight();
        let hits: Vec<usize> = (0..self.requests.len())
            .filter(|&i| self.requests[i].topic == topic && !self.requests[i].state.is_terminal()).collect();
        for i in hits {
            if Some(i) == flight { self.cut_off(i, SpeechStop::TaskCancelled, provider, now); }
            else { self.end(i, SpeechState::Cancelled, Some(SpeechStop::TaskCancelled), now); }
        }
    }

    fn cut_off(&mut self, i: usize, why: SpeechStop, provider: &mut dyn SpeechProvider, now: f64) {
        if self.requests[i].state == SpeechState::Cancelling { return; }
        provider.cancel();
        if why == SpeechStop::Killed {
            // The kill switch terminates the provider's process; it will not report.
            return self.end(i, SpeechState::Cancelled, Some(why), now);
        }
        let r = &mut self.requests[i];
        r.state = SpeechState::Cancelling;
        r.stop = Some(why);
        r.ended_at = Some(now);
    }
}
