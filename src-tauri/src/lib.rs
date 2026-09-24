//! Lantern — Tauri desktop shell and IPC boundary.
//!
//! This layer owns the window and the command surface, and nothing else.
//! It does not perceive, and it does not reason. It forwards UI intent to the
//! core and pushes the core's context object to the UI.

mod auth;
mod broker;
mod mind;
mod sensing;
mod speech;

use lantern_core::actions::ActionRecord;
use lantern_core::transaction;
use lantern_core::authz::{AccessState, AuthLevel, Decision, OsAuthKind, Operation};
use lantern_core::conversation::{Conversation, TurnOutcome};
use lantern_core::voice::speaker::SpeechStop;
use lantern_core::voice::{reference, InputSource, Priority, VoiceSettings};
use speech::{Speech, Voice, VoiceOut};
use lantern_core::router::{self, Availability, ModelId, ModelTask};
use mind::Mind;
use lantern_core::model::{ModelProvider, ModelRequest};
use lantern_core::config::Config;
use lantern_core::context::{Capability, ContextObject};
use lantern_core::privacy::{Firewall, PRIVACY_POLICY_VERSION};
use lantern_core::pump::MemoryPump;
use lantern_core::runtime::{Principal, RuntimeState, LATCH_FILE_NAME};
use lantern_core::store::Store;
use lantern_core::Engine;
use sensing::{now, Sensing};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// Cloned to hand to a worker thread; every field is a handle to the same
/// engine, firewall, book and voice, never a copy of them.
#[derive(Clone)]
pub struct AppState {
    engine: Arc<Mutex<Engine>>,
    /// Every write to local memory passes through this. The store accepts
    /// nothing it did not clear.
    firewall: Arc<Mutex<Firewall>>,
    sensing: Arc<Mutex<Sensing>>,
    store: Arc<Mutex<Option<Store>>>,
    sensing_exe: Arc<Mutex<Option<PathBuf>>>,
    startup_note: Arc<Mutex<String>>,
    /// The on-device language model process. Started on first question.
    mind: Arc<Mutex<Mind>>,
    mind_exe: Option<PathBuf>,
    /// This session's conversation. Never written to local memory.
    conversation: Arc<Mutex<Conversation>>,
    /// This session's actions. Targets (apps, links, paths) are shown here and
    /// never written to memory or sent to a model.
    actions: Arc<Mutex<transaction::ActionBook>>,
    /// KUE's voice: the narrator, the speech queue, and the kue-voice process.
    /// The only path to a speaker.
    voice: Voice,
    /// Whether KUE listens for its name, and what for. The owner's setting,
    /// off until they turn it on.
    wake: Arc<Mutex<WakeSetting>>,
    /// Where a plan the model is writing is delivered: the request it belongs
    /// to, and the channel waiting for it. One at a time.
    plan_wait: Arc<Mutex<Option<(String, std::sync::mpsc::Sender<Result<String, String>>)>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WakeSetting {
    pub enabled: bool,
    pub phrase: String,
}

fn support_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let d = PathBuf::from(home).join("Library/Application Support/Lantern");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Config precedence: your own override, then the bundled file, then built-in defaults.
fn load_config(resource_dir: Option<PathBuf>) -> (Config, String) {
    let user = support_dir().join("lantern.toml");
    if user.exists() {
        return Config::load(&user);
    }
    if let Some(r) = resource_dir {
        let bundled = r.join("lantern.toml");
        if bundled.exists() {
            return Config::load(&bundled);
        }
    }
    for rel in ["config/lantern.toml", "../config/lantern.toml", "../../config/lantern.toml"] {
        let p = PathBuf::from(rel);
        if p.exists() {
            return Config::load(&p);
        }
    }
    (Config::default_config(), "using built-in defaults".into())
}

// MARK: - Commands

#[tauri::command]
fn get_context(state: tauri::State<AppState>) -> ContextObject {
    state.engine.lock().unwrap().build_context(now())
}

#[tauri::command]
fn get_capabilities(state: tauri::State<AppState>) -> Vec<Capability> {
    state.engine.lock().unwrap().capabilities()
}

/// The Capabilities sheet: what KUE can do, in the owner's words, with whether
/// each can be used right now. Availability is decided from live facts — a
/// permission macOS has not granted, a process that is not running — so the
/// sheet never states a capability is ready when it is not. A permission KUE
/// has not been able to check reads as unchecked, never as granted.
#[tauri::command]
fn get_capability_sheet(state: tauri::State<AppState>) -> Vec<lantern_core::capabilities::CapabilityView> {
    let granted = |p: &str| match p {
        "AUTHORIZED" => Some(true),
        "DENIED" | "RESTRICTED" => Some(false),
        _ => None,
    };
    let voice_available = state.voice.exe.is_some();
    let model_available = state.mind_exe.is_some();
    let e = state.engine.lock().unwrap();
    let facts = lantern_core::capabilities::RuntimeFacts {
        killed: e.is_killed(),
        paused: e.is_paused(),
        sensing_running: e.sensing_process_up(),
        camera_granted: granted(e.camera_permission()),
        microphone_granted: e.microphone_permission().and_then(granted),
        // The same macOS grant as the microphone in this build; reported
        // separately so the sheet can say which one is missing.
        speech_granted: e.microphone_permission().and_then(granted),
        notifications_granted: None,
        // KUE neither requests nor uses Accessibility control, so it is never
        // granted here. The sheet says so rather than implying it is coming.
        accessibility_granted: Some(false),
        model_available,
        voice_available,
    };
    lantern_core::capabilities::sheet(&facts)
}

#[tauri::command]
fn startup_note(state: tauri::State<AppState>) -> String {
    state.startup_note.lock().unwrap().clone()
}

// MARK: Authorization
//
// Commands from this window are stamped Principal::Owner. Each sensitive one
// passes `gate` first: the core decides; if it asks for OS authentication,
// macOS prompts; the core decides again. At most one prompt per request.

fn gate(engine: &Mutex<Engine>, op: Operation) -> Result<(), String> {
    transaction::gate(engine, op, &|kind, op| auth::authenticate(kind, op.prompt()), &now)
        .map_err(|reason| format!("Not authorized: {reason}"))
}

/// Touch ID or password, for a LEVEL_3 grant that lasts until it expires or the session locks.
#[tauri::command(async)]
fn authenticate(state: tauri::State<AppState>) -> Result<String, String> {
    let result = auth::authenticate(OsAuthKind::Strong, "unlock KUE at LEVEL_3");
    state.engine.lock().unwrap().record_os_auth(OsAuthKind::Strong, None, &result, now());
    if result == "SUCCESS" { Ok("Authenticated by macOS.".into()) }
    else { Err(format!("Not authenticated: macOS did not confirm ({result}).")) }
}

#[tauri::command]
fn lock_session(state: tauri::State<AppState>) {
    state.engine.lock().unwrap().lock_session(now());
    state.voice.pump(now());
}

#[tauri::command(async)]
fn auth_hardware() -> serde_json::Value {
    auth::probe()
}

// MARK: Conversation — the on-device model behind the router and firewall

const MODEL_NAME: &str = "Apple on-device model";

/// What the interface may show. The conversation is visible only at LEVEL_2
/// or above and never while killed; below that it is withheld, not sent.
fn conversation_payload(engine: &Mutex<Engine>, mind: &Mutex<Mind>, conversation: &Mutex<Conversation>) -> serde_json::Value {
    let t = now();
    let (killed, access) = { let e = engine.lock().unwrap(); (e.is_killed(), e.access_block(t)) };
    let visible = !killed && access.level >= AuthLevel::Level2;
    let (running, availability, reason) = {
        let mut m = mind.lock().unwrap();
        (m.is_running(), m.availability.clone(), m.unavailable_reason.clone())
    };
    let c = conversation.lock().unwrap();
    serde_json::json!({
        "visible": visible,
        // Two sentences for the same fact: the plain one the window shows, and
        // the precise one Diagnostics and the event log keep. The window used
        // to show the precise one, which put LEVEL_2 in front of a person who
        // only wanted to ask a question.
        "withheld_said": if visible { None } else if killed { Some("I'm stopped, so I can't talk.".to_string()) }
            else { Some("I need to be sure it's you before we talk. Show your face, or use Touch ID.".to_string()) },
        "withheld_because": if visible { None } else if killed { Some("KUE is killed.".to_string()) }
            else { Some(format!("The conversation needs LEVEL_2 — you, confirmed by the camera — or Touch ID. Current: {}.", access.detail)) },
        "conversation": if visible { serde_json::to_value(&*c).unwrap_or_default() }
            else { serde_json::json!({ "exchanges": [], "pending": null, "cleared_because": c.cleared_because }) },
        "model": { "name": MODEL_NAME, "running": running, "availability": availability, "reason": reason },
    })
}

fn emit_conversation(app: &tauri::AppHandle, engine: &Mutex<Engine>, mind: &Mutex<Mind>, conversation: &Mutex<Conversation>) {
    let _ = app.emit("lantern://conversation", conversation_payload(engine, mind, conversation));
}

/// Starts the model process, unless KUE is killed or it is already running.
type PlanWait = Arc<Mutex<Option<(String, std::sync::mpsc::Sender<Result<String, String>>)>>>;

fn start_mind(
    exe: PathBuf, engine: &Arc<Mutex<Engine>>, mind: &Arc<Mutex<Mind>>,
    conversation: &Arc<Mutex<Conversation>>, voice: Option<Voice>, app: Option<tauri::AppHandle>,
    plan_wait: Option<PlanWait>,
) -> Result<(), String> {
    if engine.lock().unwrap().is_killed() {
        return Err("KUE is killed. The on-device model cannot start.".into());
    }
    if mind.lock().unwrap().is_running() { return Ok(()); }
    let (conv_m, eng_m, mind_m, app_m) = (Arc::clone(conversation), Arc::clone(engine), Arc::clone(mind), app.clone());
    let plan_m = plan_wait.clone();
    let (conv_x, eng_x, mind_x, app_x) = (Arc::clone(conversation), Arc::clone(engine), Arc::clone(mind), app);
    mind::spawn(exe, Arc::clone(mind), move |v| {
        let id = v["id"].as_str().unwrap_or_default().to_string();
        let t = now();
        // A plan the model is writing goes to whoever asked for it, and NEVER
        // into the conversation: it is JSON, it is not an answer to anybody,
        // and it is not shown until it has been checked.
        if id.starts_with(PLAN_ID_PREFIX) {
            let finished = match v["type"].as_str() {
                Some("answer") => Some(Ok(v["text"].as_str().unwrap_or_default().to_string())),
                Some("failed") => Some(Err(v["reason"].as_str().unwrap_or("GENERATION_FAILED").to_string())),
                _ => None,
            };
            if let (Some(result), Some(waiting)) = (finished, plan_m.as_ref()) {
                if let Ok(mut held) = waiting.lock() {
                    if held.as_ref().is_some_and(|(want, _)| *want == id) {
                        if let Some((_, tx)) = held.take() { let _ = tx.send(result); }
                    }
                }
            }
            return;
        }
        match v["type"].as_str() {
            Some("partial") => {
                // The first token ends prefill and begins generation. Later
                // tokens change nothing: the phase is already generating.
                if let Ok(mut e) = eng_m.lock() {
                    if e.telemetry().phase() == lantern_core::telemetry::ModelPhase::ModelPrefill {
                        e.telemetry().model_phase(lantern_core::telemetry::ModelPhase::ModelGenerating, &id, t);
                    }
                }
                conv_m.lock().unwrap().partial(&id, v["text"].as_str().unwrap_or(""))
            }
            Some("answer") => {
                if let Ok(mut e) = eng_m.lock() {
                    e.telemetry().model_phase(lantern_core::telemetry::ModelPhase::ModelIdle, &id, t);
                    // The helper's own account of where the time went. Measured
                    // there because only the model process can see the boundary
                    // between building a session, reading the prompt and
                    // producing text. Times only.
                    let ms = |k: &str| v[k].as_f64().filter(|x| *x > 0.0);
                    let span = |stage, start_ms: f64, dur: f64| lantern_core::telemetry::Span::new(
                        stage, &id, t - (start_ms + dur) / 1000.0, t - start_ms / 1000.0,
                        lantern_core::telemetry::SpanOutcome::Ok);
                    let gen = ms("generationMs").unwrap_or(0.0);
                    let prefill = ms("prefillMs").unwrap_or(0.0);
                    if gen > 0.0 {
                        e.telemetry().record(span(lantern_core::telemetry::Stage::ModelGeneration, 0.0, gen));
                    }
                    if prefill > 0.0 {
                        e.telemetry().record(span(lantern_core::telemetry::Stage::ModelPrefill, gen, prefill));
                    }
                    if let Some(create) = ms("sessionCreateMs") {
                        e.telemetry().record(span(lantern_core::telemetry::Stage::ModelQueue, gen + prefill, create));
                    }
                }
                // Checked against the capability table, and against what KUE
                // verified: a model may explain a confirmed action, never doubt it.
                let (caps, verified) = {
                    let e = eng_m.lock().unwrap();
                    (e.capabilities(), e.facts().verified(t).into_iter().cloned().collect::<Vec<_>>())
                };
                let held = conv_m.lock().unwrap().finish_checked_against(&id, TurnOutcome::Answered,
                    v["text"].as_str().unwrap_or(""), MODEL_NAME, &caps, &verified.iter().collect::<Vec<_>>(), t);
                if held {
                    eng_m.lock().unwrap().record_model_event(
                        format!("The on-device model answered in {:.1}s.", v["seconds"].as_f64().unwrap_or(0.0)), None, t);
                    // The request is finished with the answer as shown — after
                    // the capability and verified-fact checks, not before.
                    let shown = conv_m.lock().unwrap().exchanges.last().map(|x| x.answer.clone()).unwrap_or_default();
                    transaction::answered(&eng_m, true, &shown, t);
                    speak_last_answer(voice.as_ref(), &conv_m, t);
                }
            }
            Some("failed") => {
                if let Ok(mut e) = eng_m.lock() {
                    e.telemetry().model_phase(lantern_core::telemetry::ModelPhase::ModelIdle, &id, t);
                }
                let code = v["reason"].as_str().unwrap_or("GENERATION_FAILED").to_string();
                let text = match code.as_str() {
                    "GUARDRAIL" | "REFUSED" => "Apple's on-device model declined to answer this.",
                    "CONTEXT_TOO_LARGE" => "The question and context were too long for the on-device model.",
                    "MODEL_UNAVAILABLE" => "The on-device model is not available right now.",
                    "UNSUPPORTED_LANGUAGE" => "The on-device model does not support this language.",
                    "CANCELLED" => "Cancelled.",
                    _ => "The on-device model could not produce an answer.",
                };
                let held = conv_m.lock().unwrap().finish(&id, TurnOutcome::Failed, text, MODEL_NAME, t);
                if held {
                    eng_m.lock().unwrap().record_model_event("The on-device model did not answer.".into(), Some(code), t);
                    transaction::answered(&eng_m, false, text, t);
                }
            }
            _ => {}
        }
        if let Some(a) = &app_m { emit_conversation(a, &eng_m, &mind_m, &conv_m); }
    }, move || {
        {
            let mut c = conv_x.lock().unwrap();
            if let Some(p) = c.pending.clone() {
                c.finish(&p.id, TurnOutcome::Failed, "The on-device model process stopped.", MODEL_NAME, now());
            }
        }
        if let Some(a) = &app_x { emit_conversation(a, &eng_x, &mind_x, &conv_x); }
    })?;
    for _ in 0..60 {
        if mind.lock().unwrap().availability.is_some() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

/// The newest answer, through KUE's speech pipeline (spoken only if you asked by voice,
/// or asked for answers aloud; the gate and firewall decide the rest).
fn speak_last_answer(voice: Option<&Voice>, conversation: &Mutex<Conversation>, t: f64) {
    let Some(v) = voice else { return };
    let last = conversation.lock().ok().and_then(|c| c.exchanges.last().cloned());
    if let Some(x) = last { v.on_answer(&x, t); }
}

/// What a spoken request does once KUE has heard its name — with nobody
/// clicking anything.
///
/// It is the SAME path a typed request takes: the deterministic command parser
/// first, and if the words are not a command, the question path. Nothing here
/// grants anything. A voice that woke KUE is still a voice: the transaction
/// authorizes against the access session, so a request made while KUE is
/// locked, or by someone it does not recognise, is refused exactly as it would
/// be if it had been typed.
fn handle_spoken_request(app: &tauri::AppHandle, text: &str) {
    use tauri::Manager;
    let state = app.state::<AppState>();
    if let Err(e) = receive_input(text.to_string(), Some("VOICE".into()), app.clone(), state) {
        eprintln!("[kue] spoken request: {e}");
    }
}

/// What happened to one input, as the window may see it. No target, no path:
/// the window fetches action records through the firewall as it always has.
#[derive(serde::Serialize)]
struct Heard {
    /// NEW_ACTION · QUESTION · CONFIRMED · CANCELLED · REVISED · CLARIFY · ANSWERED · REFERENCE
    kind: &'static str,
    request_id: Option<String>,
    /// KUE's reply, when the pipeline produced one (a revision, a question back).
    said: Option<String>,
}

/// THE front door. Spoken after the wake word, spoken after a button, typed —
/// every input enters here, is understood in the context of what is open, and
/// takes the same governed path. The window makes one call; the hands-free
/// listener makes the same one.
#[tauri::command(async)]
fn receive_input(text: String, source: Option<String>, app: tauri::AppHandle, state: tauri::State<AppState>)
    -> Result<Heard, String>
{
    use lantern_core::pipeline::Transport;
    let source = source.unwrap_or_else(|| "TEXT".into());
    let transport = if source == "VOICE" { Transport::Voice } else { Transport::Typed };
    let phrase = state.wake.lock().map(|w| w.phrase.clone()).unwrap_or_default();
    let (_, text_only) = lantern_core::intent::split_invocation(&text, &phrase);
    let text_only = text_only.to_string();

    // Picking among document matches ("the older one") keeps its own path.
    if let Some(r @ reference::Reference::Choose(_)) = reference::interpret(&text_only) {
        let max_age = state.voice.out.lock().unwrap().spoken_confirmation_seconds;
        let outcome = with_actions(Some(&app), &state, |rt| transaction::resolve_reference(rt, r, max_age));
        if let Some(m) = &outcome.message { state.voice.say_plain(m, "reference", Priority::AuthorizationRequired, now()); }
        return Ok(Heard { kind: "REFERENCE", request_id: None, said: outcome.message });
    }
    if transport == Transport::Voice && reference::interpret(&text_only) == Some(reference::Reference::Cancel) {
        // You are speaking over KUE: it stops.
        state.voice.stop(SpeechStop::StoppedByYou, now());
    }

    // The request whose answer the model is writing, if any: a "stop" that
    // ends it also stops the model, so a late answer is neither shown as
    // current nor spoken.
    let answering = state.engine.lock().ok().and_then(|e| e.requests().answering());
    let received = with_actions(Some(&app), &state, |rt| transaction::receive(rt, transport, &text_only));
    use transaction::Received;
    let heard = match received {
        Received::New { request_id, record: Some(_) } => Heard { kind: "NEW_ACTION", request_id: Some(request_id), said: None },
        Received::New { request_id, record: None } => {
            // A yes or a stop the conversation could not place — an action
            // waiting from before this conversation existed. The older path
            // still owns it, so it is asked there rather than sent to a model.
            if let Some(r @ (reference::Reference::Confirm | reference::Reference::Cancel)) = reference::interpret(&text_only) {
                let max_age = state.voice.out.lock().unwrap().spoken_confirmation_seconds;
                let outcome = with_actions(Some(&app), &state, |rt| transaction::resolve_reference(rt, r, max_age));
                if let Some(m) = &outcome.message { state.voice.say_plain(m, "reference", Priority::AuthorizationRequired, now()); }
                return Ok(Heard { kind: "REFERENCE", request_id: Some(request_id), said: outcome.message });
            }
            ask(text, Some(source.clone()), None, app.clone(), state)?;
            Heard { kind: "QUESTION", request_id: Some(request_id), said: None }
        }
        Received::Confirmed { request_id, record } => {
            let said = record.as_ref().err().cloned();
            Heard { kind: "CONFIRMED", request_id: Some(request_id), said }
        }
        // KUE's replies below were recorded as turns and spoken by the core,
        // through the speech gate and the firewall; the window shows them.
        Received::Cancelled { request_id, said } => {
            if answering.as_deref() == Some(request_id.as_str()) { stop_pending_answer(&state); }
            Heard { kind: "CANCELLED", request_id: Some(request_id), said: Some(said) }
        }
        Received::Revised { request_id, said } => Heard { kind: "REVISED", request_id: Some(request_id), said: Some(said) },
        Received::Clarify { request_id, question } => Heard { kind: "CLARIFY", request_id: Some(request_id), said: Some(question) },
        Received::Answered { said } => Heard { kind: "ANSWERED", request_id: None, said: Some(said) },
    };
    emit_actions(&app);
    Ok(heard)
}

/// What KUE is doing, for the window to project: the one runtime state, the
/// request in flight, what is open, and the recent conversation. KUE's own
/// sentences and the owner's words — never a path, never a target.
#[tauri::command]
fn get_runtime(state: tauri::State<AppState>) -> serde_json::Value {
    let listening = false;
    let e = match state.engine.lock() { Ok(e) => e, Err(_) => return serde_json::json!({}) };
    // The conversation, the plan and what was said in them are the owner's:
    // the same bar as the conversation view (LEVEL_2, not killed). Otherwise
    // only what state KUE is in — never the words, the plan or the request.
    let visible = !e.is_killed() && e.access_block(now()).level >= AuthLevel::Level2;
    if !visible {
        return serde_json::json!({
            "state": e.kue_state(listening),
            "withheld": if e.is_killed() { "I'm stopped, so I can't talk." }
                        else { "I need to be sure it's you before we talk. Show your face, or use Touch ID." },
        });
    }
    let p = e.requests();
    let active = p.active().map(|r| serde_json::json!({
        "request_id": r.ids.request_id, "goal_id": r.ids.goal_id, "tool_execution_id": r.ids.tool_execution_id,
        "verification_id": r.ids.verification_id, "transport": r.transport, "state": r.state.tag(),
        "decision": r.decision,
    }));
    let turns: Vec<_> = p.dialogue.turns().iter().rev().take(20).rev().map(|t| serde_json::json!({
        "kind": t.kind, "said": t.said, "at": t.at, "request_id": t.request_id,
    })).collect();
    // The plan the live request is following: its goal, read as a plan, with
    // what the owner is asked and when. Read with the engine released, so the
    // two locks are never held together.
    let goal_id = p.active().and_then(|r| r.ids.goal_id.clone());
    let open_plan = match p.dialogue.open() {
        Some(lantern_core::dialogue::Open::Plan { plan_id, .. }) => Some(plan_id.clone()),
        _ => None,
    };
    let (state_now, conversation_id, open, thread, now_line, refused) = (e.kue_state(listening),
        p.dialogue.conversation_id.clone(), serde_json::to_value(p.dialogue.open()).unwrap_or_default(),
        p.thread(30), p.now_line(), p.refused_transitions);
    drop(e);
    // The plan the live request is following: the one waiting for a yes, or
    // the one running as a goal. A plan WAITING has no goal yet, so it is
    // found by what is open — without this the owner is asked to agree to a
    // plan the window cannot show them.
    //
    // `steps` is what each step would actually do, named. The owner reads it,
    // because the plan they are shown is the plan they are agreeing to. It
    // names their own files, so it goes to their window and nowhere else:
    // never to a model, never into a spoken line.
    // What the owner has told KUE that shaped what is being shown — their own
    // sentences, not ids. A goal KUE filtered names them; a model plan names
    // what was put in front of the model.
    // Gathered under the book's lock and looked up afterwards: the engine is
    // never locked while the book is held.
    let shaped_by: Vec<String>;
    let plan = {
        let book = state.actions.lock().ok();
        let held = book.as_ref().and_then(|b| {
            open_plan.as_deref().and_then(|id| b.plan(id))
                .or_else(|| goal_id.as_deref().and_then(|g| b.plans().iter().find(|h| h.goal_id.as_deref() == Some(g))))
        });
        match held {
            // A running plan is read from its goal, so the step states are the
            // live ones; what each step does still comes from the held plan.
            Some(h) => {
                let live = h.goal_id.as_deref().and_then(|g| book.as_ref().and_then(|b| b.goal(g)))
                    .map(lantern_core::plan::Plan::of).unwrap_or_else(|| h.plan.clone());
                let from_goal = h.goal_id.as_deref().and_then(|g| book.as_ref().and_then(|b| b.goal(g)))
                    .map(|g| g.memory_refs.clone()).unwrap_or_default();
                shaped_by = if from_goal.is_empty() { h.memory_refs.clone() } else { from_goal };
                Some(serde_json::json!({ "plan": &live, "preview": lantern_core::plan::preview(&live),
                                         "steps": h.step_details(), "version": h.version,
                                         "revision_of": h.revision_of, "waiting": !h.approved() }))
            }
            None => {
                shaped_by = goal_id.as_deref().and_then(|g| book.as_ref().and_then(|b| b.goal(g)))
                    .map(|g| g.memory_refs.clone()).unwrap_or_default();
                goal_id.and_then(|g| book.as_ref()?.goal(&g).map(lantern_core::plan::Plan::of))
                    .map(|pl| serde_json::json!({ "plan": &pl, "preview": lantern_core::plan::preview(&pl),
                                                  "steps": Vec::<String>::new(), "version": 1,
                                                  "revision_of": None::<String>, "waiting": false }))
            }
        }
    };
    // Their own sentences, never the ids: what the owner reads is what they
    // said, and what they can go and look at in "What I remember".
    let plan = plan.map(|mut p| {
        let used: Vec<String> = match state.engine.lock() {
            Ok(e) => shaped_by.iter().filter_map(|id| e.memory().get(id).filter(|m| m.is_current(now())))
                .map(|m| m.statement.clone()).collect(),
            Err(_) => Vec::new(),
        };
        p["memory_used"] = serde_json::json!(used);
        p
    });
    serde_json::json!({
        "plan": plan,
        "state": state_now,
        "conversation_id": conversation_id,
        "active": active,
        "open": open,
        "turns": turns,
        // The conversation as the main window shows it: labels decided in core.
        "thread": thread,
        "now": now_line,
        "refused_transitions": refused,
    })
}

/// Ask Lantern. Authorization first (LEVEL_2, or Touch ID), then the router,
/// then the firewall builds the only prompt the model will see.
#[tauri::command(async)]
fn ask(question: String, source: Option<String>, invocation: Option<String>, app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let input = InputSource::from_tag(source.as_deref().unwrap_or("TEXT"));
    // "Computer, what is 17% of 840?" typed: the invocation is recorded, not classified.
    let phrase = state.wake.lock().map(|w| w.phrase.clone()).unwrap_or_default();
    let (typed_invocation, question) = match invocation {
        Some(_) => (invocation, question.as_str()),
        None => lantern_core::intent::split_invocation(&question, &phrase),
    };
    let invocation = typed_invocation;
    let q = question.trim();
    if q.is_empty() { return Err("Ask something first.".into()); }
    if q.chars().count() > 2000 { return Err("That question is too long (2,000 characters at most).".into()); }
    if state.conversation.lock().unwrap().is_busy() {
        return Err("KUE is still answering the previous question.".into());
    }
    // Refused by what it asks for, before authorization and before any model:
    // the model is not the safety boundary, and who is asking does not matter.
    let t_screen = now();
    let screened = lantern_core::safety::screen(q);
    record_span(&state.engine, lantern_core::telemetry::Stage::SafetyScreen, "ask", t_screen,
                if screened.is_some() { lantern_core::telemetry::SpanOutcome::Refused }
                else { lantern_core::telemetry::SpanOutcome::Ok });
    if let Some(refusal) = screened {
        let t = now();
        state.conversation.lock().unwrap().answer_directly_from(q, refusal.reason,
            lantern_core::safety::SAFETY_BOUNDARY_SOURCE, input, t)?;
        state.engine.lock().unwrap().record_refusal(refusal.concern, t);
        speak_last_answer(Some(&state.voice), &state.conversation, t);
        emit_conversation(&app, &state.engine, &state.mind, &state.conversation);
        return Ok(());
    }
    gate(&state.engine, Operation::AskModelWithPersonalContext)?;

    // What is being asked for, by rule. Whatever KUE can answer without a model —
    // arithmetic, the capability list, what it cannot do, what it needs to know
    // first — is answered here and never reaches one. Only a question goes on.
    let t_intent = now();
    let understanding = lantern_core::intent::classify(q, input, invocation.as_deref());
    record_span(&state.engine, lantern_core::telemetry::Stage::IntentRouting, "ask", t_intent,
                lantern_core::telemetry::SpanOutcome::Ok);
    let lantern_core::intent::Understanding::Understood(intent) = understanding else {
        return Err("Refused by KUE's safety boundary.".into());
    };
    {
        let t = now();
        let mut e = state.engine.lock().unwrap();
        let preview = intent.operation.map(|op| e.preview_authorization(op, t));
        let status = lantern_core::intent::status(&intent, e.is_killed(), preview.as_ref());
        e.record_intent(&intent.summary(status), t);
    }
    // The governed decision for a question: KUE answers it. Recorded against the
    // request the input created, so the runtime — not the window — knows.
    let _ = transaction::answering(&state.engine, now());
    let answer_by_rule = |sentence: &str, source: &str| -> Result<(), String> {
        let t = now();
        transaction::answered(&state.engine, true, sentence, t);
        state.conversation.lock().unwrap().answer_directly_from(q, sentence, source, input, t)?;
        speak_last_answer(Some(&state.voice), &state.conversation, t);
        emit_conversation(&app, &state.engine, &state.mind, &state.conversation);
        Ok(())
    };
    use lantern_core::intent::Work;
    match intent.work {
        Work::Answer { sentence, source, .. } => return answer_by_rule(&sentence, source),
        Work::Ask { sentence, goal } => {
            // A broad request ("clean my computer") opens a goal that waits for what is missing. Nothing runs.
            if let Some(bp) = goal {
                let tag = if input == InputSource::Voice { "VOICE" } else { "TEXT" };
                with_actions(Some(&app), &state, |rt| transaction::start_goal(rt, bp, tag));
            }
            return answer_by_rule(&sentence, lantern_core::intent::INTENT_ROUTER_SOURCE);
        }
        // "What can you do?" is answered from the capability table, never by the
        // model, which was measured claiming capabilities Lantern does not have.
        Work::CapabilityList => {
            let caps = state.engine.lock().unwrap().capabilities();
            answer_by_rule(&lantern_core::conversation::capability_answer(&caps), lantern_core::conversation::CAPABILITY_LIST_SOURCE)?;
            state.engine.lock().unwrap().record_model_event(
                "Answered a question about KUE's capabilities from its capability list, without a model.".into(), None, now());
            return Ok(());
        }
        // Commands are planned by the transaction before a question is asked. A
        // command that arrives here was not planned, and is not a question.
        Work::Act(_) | Work::Goal(_) => return Err("That is a request to act, and it was not planned. Nothing was done.".into()),
        // A request to put the owner's things in order that no rule can carry
        // out. The model is asked for a PLAN, on a worker thread so the window
        // stays live, and what comes back is checked before anyone sees it.
        Work::ModelPlan => return plan_with_model(q, input, &app, &state),
        Work::Model => {}
    }

    let exe = state.mind_exe.clone().ok_or("The on-device model process (lantern-mind) is not installed.")?;
    start_mind(exe, &state.engine, &state.mind, &state.conversation, Some(state.voice.clone()), Some(app.clone()), Some(Arc::clone(&state.plan_wait)))?;
    let (availability, reason) = { let m = state.mind.lock().unwrap(); (m.availability.clone(), m.unavailable_reason.clone()) };
    let route = router::route(ModelTask::ConversationWithContext, &Decision::Allow,
        Availability { apple_on_device: availability.as_deref() == Some("AVAILABLE"), external_configured: false });
    if route.chosen != Some(ModelId::AppleOnDevice) {
        let why: Vec<String> = route.considered.iter().map(|c| format!("{:?} {}", c.model, c.outcome)).collect();
        return Err(format!("No model may answer: {}.{}", why.join("; "), reason.map(|r| format!(" {r}")).unwrap_or_default()));
    }

    let t = now();
    let ctx = state.engine.lock().unwrap().build_context(t);
    let history = state.conversation.lock().unwrap().history();
    // What KUE knows, newest first, each sentence carrying its own state. The
    // firewall clears the prompt; the facts arrive already classified.
    let known = {
        let e = state.engine.lock().unwrap();
        let mut known = e.facts().for_model(t, 12);
        // What the owner has told KUE that bears on THIS question — at most a
        // few, chosen by the words they share with it. The model never sees
        // the memory KUE keeps, only what was asked about; a candidate is not
        // offered at all, and each carries the state it is in.
        let relevant: Vec<String> = e.memory().matching(q, t).into_iter().take(5)
            .map(|m| format!("[{} the owner told KUE] {}", m.state.tag(), m.statement)).collect();
        known.extend(relevant);
        known
    };
    let t_clear = now();
    let cleared = state.firewall.lock().unwrap().clear_model_context_with(&ctx, &history, q, &known, t);
    record_span(&state.engine, lantern_core::telemetry::Stage::PrivacyClearance, "ask", t_clear,
                if cleared.is_some() { lantern_core::telemetry::SpanOutcome::Ok }
                else { lantern_core::telemetry::SpanOutcome::Refused });
    let cleared = cleared
        .ok_or("The privacy firewall refused to send this question to the on-device model.")?;
    let withheld: Vec<_> = cleared.value().withheld.iter().map(|w| w.kind.tag()).collect();
    let withheld_count = withheld.len();
    let id = state.conversation.lock().unwrap().begin_from(q, input, withheld, t)?;
    // The model is reached only through `model::ask`, which admits the prompt exactly as cleared.
    let sent = lantern_core::model::ask(&mut *state.mind.lock().unwrap(), &ModelRequest::new(id.clone(), cleared));
    if let Err(e) = sent {
        state.conversation.lock().unwrap().finish(&id, TurnOutcome::Failed, &e, MODEL_NAME, now());
        emit_conversation(&app, &state.engine, &state.mind, &state.conversation);
        return Err(e);
    }
    {
        // The model's phase, on the same timeline as the perception samples.
        // MODEL_PREFILL is the stretch between the prompt leaving here and the
        // first token coming back — the phase suspected of starving Vision, and
        // the reason this timeline exists at all.
        let mut e = state.engine.lock().unwrap();
        e.telemetry().model_phase(lantern_core::telemetry::ModelPhase::ModelPrefill, &id, t);
        e.record_model_event(
            format!("Asked the on-device model; the prompt passed the privacy firewall with {withheld_count} kind(s) withheld."),
            None, t);
    }
    emit_conversation(&app, &state.engine, &state.mind, &state.conversation);
    Ok(())
}

/// The id a plan request is sent under, so its answer is never mistaken for a
/// question's.
const PLAN_ID_PREFIX: &str = "plan-";
/// How long KUE waits for a model to write a plan. Measured on this Mac: a
/// cold model takes 7-17 s to its first token, a warm one about 4.
const PLAN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Asks the on-device model for a plan, and offers it to the owner if it
/// checks out. Runs on its own thread: the model takes seconds, and the
/// window must keep working.
///
/// The model never acts. What comes back is JSON, parsed strictly and checked
/// against the declared tools (`transaction::offer_plan`); if it does not
/// check out, the owner is told what happened and nothing is held.
fn plan_with_model(goal: &str, input: InputSource, app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    let exe = state.mind_exe.clone().ok_or("The on-device model process (lantern-mind) is not installed.")?;
    start_mind(exe, &state.engine, &state.mind, &state.conversation, Some(state.voice.clone()),
               Some(app.clone()), Some(Arc::clone(&state.plan_wait)))?;
    let (availability, reason) = { let m = state.mind.lock().unwrap(); (m.availability.clone(), m.unavailable_reason.clone()) };
    let route = router::route(ModelTask::ConversationWithContext, &Decision::Allow,
        Availability { apple_on_device: availability.as_deref() == Some("AVAILABLE"), external_configured: false });
    if route.chosen != Some(ModelId::AppleOnDevice) {
        return Err(format!("No model may write a plan: {}.{}",
            route.considered.iter().map(|c| format!("{:?} {}", c.model, c.outcome)).collect::<Vec<_>>().join("; "),
            reason.map(|r| format!(" {r}")).unwrap_or_default()));
    }
    let t = now();
    // The governed decision, recorded: KUE is planning this, not answering it.
    let rid = transaction::planning(&state.engine, t).unwrap_or_default();
    // What the owner has already told KUE that bears on THIS goal — at most
    // three, chosen by the words they share with it. The model is shown their
    // sentences; which memories those were is recorded by KUE when the plan
    // comes back, never taken from what the model wrote.
    let (remembered, memory_refs): (Vec<String>, Vec<String>) = {
        let e = state.engine.lock().unwrap();
        e.memory().matching(goal, t).into_iter().take(3)
            .map(|m| (m.statement.clone(), m.id.clone())).unzip()
    };
    let cleared = state.firewall.lock().unwrap().clear_plan_request_with(goal, &remembered, t)
        .ok_or("The privacy firewall refused to send this to the on-device model.")?;
    if !remembered.is_empty() {
        state.engine.lock().unwrap().record_action_event(
            format!("Asked the on-device model for a plan, with {} thing(s) the owner has told KUE that bear on it.",
                    remembered.len()), t);
    }
    transaction::note_turn(&state.engine, transaction::WORKING_OUT_A_PLAN, &rid, t);
    emit_conversation(app, &state.engine, &state.mind, &state.conversation);

    let id = format!("{PLAN_ID_PREFIX}{rid}");
    let (tx, rx) = std::sync::mpsc::channel();
    *state.plan_wait.lock().unwrap() = Some((id.clone(), tx));
    lantern_core::model::ask(&mut *state.mind.lock().unwrap(), &lantern_core::model::ModelRequest::new(id, cleared))?;
    state.engine.lock().unwrap().record_model_event(
        "Asked the on-device model for a plan. It was given KUE's tool list and the owner's words, and nothing else.".into(), None, t);

    let (owned, app, tag) = ((*state).clone(), app.clone(), if input == InputSource::Voice { "VOICE" } else { "TEXT" });
    let refs = memory_refs.clone();
    std::thread::spawn(move || {
        let outcome = rx.recv_timeout(PLAN_TIMEOUT);
        owned.plan_wait.lock().map(|mut w| *w = None).ok();
        let t = now();
        match outcome {
            Ok(Ok(json)) => {
                let executor = broker::locate_act().is_some();
                with_actions(Some(&app), &owned, |rt|
                    transaction::offer_plan_informed_by(rt, &rid, &json, "on-device model", tag, executor, &refs));
            }
            other => {
                // Said as what happened: the model gave nothing, or gave it too late.
                let why = match other {
                    Ok(Err(code)) => format!("{} The model stopped: {code}.", transaction::NO_PLAN_PRODUCED),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) =>
                        format!("{} The model did not finish within {} seconds.", transaction::NO_PLAN_PRODUCED, PLAN_TIMEOUT.as_secs()),
                    _ => format!("{} The model process stopped.", transaction::NO_PLAN_PRODUCED),
                };
                owned.engine.lock().unwrap().record_model_event("The on-device model produced no plan.".into(), None, t);
                transaction::note_turn(&owned.engine, &why, &rid, t);
                owned.voice.say_plain(&why, "plan", lantern_core::voice::Priority::ActionResult, t);
                let _ = owned.engine.lock().unwrap().requests_mut()
                    .walk_to(&rid, lantern_core::pipeline::RequestState::Failed, t, Some("No plan was produced."));
            }
        }
        emit_conversation(&app, &owned.engine, &owned.mind, &owned.conversation);
        emit_actions(&app);
    });
    Ok(())
}

#[tauri::command]
fn cancel_ask(app: tauri::AppHandle, state: tauri::State<AppState>) {
    stop_pending_answer(&state);
    emit_conversation(&app, &state.engine, &state.mind, &state.conversation);
}

/// Stops the model's answer in progress. Finished as CANCELLED first, so an
/// answer that arrives anyway is not held, shown as current, or spoken.
fn stop_pending_answer(state: &AppState) {
    let pending = state.conversation.lock().unwrap().pending.clone();
    if let Some(p) = pending {
        let _ = ModelProvider::cancel(&mut *state.mind.lock().unwrap(), &p.id);
        state.conversation.lock().unwrap().finish(&p.id, TurnOutcome::Failed, "Cancelled.", MODEL_NAME, now());
    }
}

/// Records one finished stage. Times and an opaque id only — `Span::new`
/// strips anything else, so no question or file name can reach the record.
fn record_span(engine: &Mutex<Engine>, stage: lantern_core::telemetry::Stage, op: &str,
               start: f64, outcome: lantern_core::telemetry::SpanOutcome) {
    if let Ok(mut e) = engine.lock() {
        e.telemetry().record(lantern_core::telemetry::Span::new(stage, op, start, now(), outcome));
    }
}

/// Push-to-talk. Refused while killed or paused; what is heard grants nothing.
#[tauri::command(async)]
fn listen_start(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::Listen)?;
    if state.engine.lock().unwrap().is_paused() {
        return Err("KUE is paused. Resume sensing to use the microphone.".into());
    }
    // You are about to speak: KUE stops talking and drops what it was going to say.
    state.voice.stop(SpeechStop::UserSpeaking, now());
    send_unless_killed(&state, &format!(r#"{{"cmd":"listen_start","maxSeconds":30,"silenceSeconds":{LISTEN_SILENCE_SECONDS}}}"#))
}

#[tauri::command]
fn listen_stop(state: tauri::State<AppState>) -> Result<(), String> {
    state.sensing.lock().unwrap().send(r#"{"cmd":"listen_stop"}"#)
}

#[tauri::command]
fn listen_cancel(state: tauri::State<AppState>) -> Result<(), String> {
    state.sensing.lock().unwrap().send(r#"{"cmd":"listen_cancel"}"#)
}

// MARK: Command pipeline and Action Broker

/// The earlier default cut a spoken command in half on this Mac: "Can you
/// open…" stopped at the pause before the name, and the rest was lost.
const LISTEN_SILENCE_SECONDS: f64 = 2.5;

fn home_dir() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())) }

/// Tells the interface the action list changed. Carries no record: the list is
/// fetched through `get_actions`, the one path gated by level and firewall.
fn emit_actions(app: &tauri::AppHandle) {
    let _ = app.emit("lantern://actions", ());
}

/// Runs `f` with the transaction runtime: the engine's authorization, the
/// firewall, this session's actions, macOS authentication and KueAct. The
/// transaction logic itself lives in `lantern_core::transaction`.
fn with_actions<R>(app: Option<&tauri::AppHandle>, state: &AppState, f: impl FnOnce(&transaction::Runtime) -> R) -> R {
    let targets = transaction::Targets::for_home(&home_dir());
    let clock = || now();
    let authenticate = |kind: OsAuthKind, op: Operation| auth::authenticate(kind, op.prompt());
    let execute_os = |req: &transaction::ExecutorRequest| broker::run_os_action(req);
    let measure_volume = |p: &std::path::Path| measure_volume(p);
    let changed = || if let Some(a) = app { emit_actions(a) };
    // Called with no core lock held; the record is already cleared for the window.
    let narrate = |rec: &ActionRecord| state.voice.on_action(rec, now());
    // A goal step's own sentence, through the same speech gate as any, and the
    // firewall for the kinds of data it declares.
    let say = |text: &str, carries: &[lantern_core::privacy::DataKind]|
        state.voice.say_carrying(text, carries, "goal", lantern_core::voice::Priority::ActionResult, now());
    let rt = transaction::Runtime {
        engine: &state.engine, firewall: &state.firewall, book: &state.actions, targets: &targets,
        now: &clock, authenticate: &authenticate, execute_os: &execute_os, changed: &changed, narrate: &narrate,
        measure_volume: &measure_volume, say: &say,
    };
    f(&rt)
}

/// What the volume holding `path` reports, from macOS `statfs`. Two different
/// numbers, and KUE keeps them apart: `available` is what this user may still
/// write, while the used figure counts every block in use, reserved ones
/// included. Finder's "available" can be larger than either, because Finder
/// counts space it believes it could purge; KUE reports the measurement it
/// took, and the window says which it is.
fn measure_volume(path: &std::path::Path) -> Result<lantern_core::storage::VolumeUsage, String> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut buf) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let block = buf.f_bsize as u64;
    let capacity = (buf.f_blocks as u64).saturating_mul(block);
    let free = (buf.f_bfree as u64).saturating_mul(block);
    Ok(lantern_core::storage::VolumeUsage {
        capacity,
        available: (buf.f_bavail as u64).saturating_mul(block),
        used: capacity.saturating_sub(free),
    })
}

/// Parses text or a transcript. Not a command → Ok(None), and the interface
/// asks the model instead. A command becomes a plan that is refused, waits for
/// confirmation, or (LOW risk, already authorized) runs now.
#[tauri::command(async)]
fn propose_command(text: String, source: Option<String>, app: tauri::AppHandle, state: tauri::State<AppState>)
    -> Result<Option<ActionRecord>, String>
{
    // "Computer, open Safari" typed is the same command as said after the name.
    let phrase = state.wake.lock().map(|w| w.phrase.clone()).unwrap_or_default();
    let (_, text) = lantern_core::intent::split_invocation(&text, &phrase);
    Ok(with_actions(Some(&app), &state, |rt| transaction::propose(rt, text, source.as_deref().unwrap_or("TEXT"))))
}

/// Confirm. Re-checks targets and re-authorizes now; see `transaction::confirm`.
#[tauri::command(async)]
fn confirm_action(id: String, choice: Option<String>, app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<ActionRecord, String> {
    with_actions(Some(&app), &state, |rt| transaction::confirm(rt, &id, choice))
}

#[tauri::command]
fn cancel_action(id: String, app: tauri::AppHandle, state: tauri::State<AppState>) {
    with_actions(Some(&app), &state, |rt| transaction::cancel(rt, &id))
}

/// The action list, or why it is withheld (KILLED, AUTHORIZATION_REQUIRED, PRIVACY_DENIED).
#[tauri::command]
fn get_actions(state: tauri::State<AppState>) -> transaction::ActionList {
    with_actions(None, &state, transaction::list)
}

/// Everything the window may show, decided in the core (`lantern_core::surface`).
///
/// The window renders this and sends gestures back. It composes no sentence
/// about KUE and infers no state of its own, so it cannot claim KUE is
/// listening, acting or finished unless something actually reported it.
///
/// Built from records the core already cleared for the window: a withheld list
/// contributes nothing, exactly as it shows nothing.
/// The last time KUE took stock of storage. Re-checked on every fetch: killed,
/// below LEVEL_2 or refused by the firewall all mean nothing is handed over.
/// Takes stock now, at the owner's asking. The same action the spoken and
/// typed requests raise, through the same authorization, privacy and
/// verification path — the control is a shortcut to it, not a way around it.
#[tauri::command(async)]
fn check_storage(app: tauri::AppHandle, state: tauri::State<AppState>) -> transaction::StorageView {
    with_actions(Some(&app), &state, |rt| { transaction::propose_inspection(rt, "TEXT"); });
    with_actions(None, &state, transaction::storage)
}

/// Files the owner selected from the last report, for the Trash. The core
/// refuses any path that report did not offer, then asks for confirmation and
/// macOS authentication like any HIGH-risk action — this command starts that,
/// it does not perform it.
#[tauri::command(async)]
fn trash_selected(paths: Vec<String>, app: tauri::AppHandle, state: tauri::State<AppState>) -> ActionRecord {
    with_actions(Some(&app), &state, |rt| transaction::propose_trash(rt, paths.clone(), "TEXT"))
}

/// Undo: everything the last move to the Trash moved, back where it came from.
#[tauri::command(async)]
fn undo_trash(app: tauri::AppHandle, state: tauri::State<AppState>) -> Option<ActionRecord> {
    with_actions(Some(&app), &state, |rt| transaction::propose_restore(rt, "TEXT"))
}

#[tauri::command]
fn get_storage(state: tauri::State<AppState>) -> transaction::StorageView {
    with_actions(None, &state, transaction::storage)
}

/// What KUE knows about its own machinery, for Diagnostics. Times, counts and
/// categories — the same record it writes to local memory, nothing more.
#[tauri::command]
fn get_perception(state: tauri::State<AppState>) -> serde_json::Value {
    use lantern_core::telemetry::Stage;
    let t = now();
    let mut e = match state.engine.lock() { Ok(e) => e, Err(_) => return serde_json::json!({}) };
    let measurement = e.measurement_state(t);
    let health = e.pipeline_health().cloned();
    let stages: Vec<_> = [Stage::VisionAnalyze, Stage::CoreTick, Stage::ContextBuild, Stage::SafetyScreen,
                          Stage::IntentRouting, Stage::PrivacyClearance, Stage::ModelQueue,
                          Stage::ModelPrefill, Stage::ModelGeneration]
        .into_iter().filter_map(|st| e.telemetry().summary(st)).collect();
    let phase = e.telemetry().phase();
    let (dropped_spans, dropped_samples) = e.telemetry().dropped();
    drop(e);
    let footprint = state.store.lock().ok().and_then(|g| g.as_ref().and_then(|st| st.footprint().ok()));
    serde_json::json!({
        "measurement": measurement.tag(),
        "measurementSaid": measurement.said(),
        "pipeline": health,
        "modelPhase": phase.tag(),
        "stages": stages,
        "dropped": { "spans": dropped_spans, "samples": dropped_samples },
        "store": footprint,
    })
}

#[tauri::command]
fn get_surface(state: tauri::State<AppState>) -> lantern_core::surface::Surface {
    let t = now();
    // The pipeline's lock order is out → speech → engine → firewall, so the
    // voice is read first and released before the engine is touched.
    let speaking = {
        let out = state.voice.out.lock().unwrap();
        out.controller.state() == lantern_core::voice::speaker::SpeechState::Speaking
    };
    let thinking_seconds = state.conversation.lock().unwrap().pending.as_ref()
        .map(|p| (t - p.started_at).max(0.0));
    let memory_writable = state.store.lock().unwrap().is_some();
    let actions = with_actions(None, &state, transaction::list);
    let wake = state.wake.lock().unwrap().clone();

    let e = state.engine.lock().unwrap();
    let access = e.access_block(t);
    let computer = e.computer_now(t);
    let activity = e.activity_sentence(t);
    let inputs = lantern_core::surface::Inputs {
        runtime: e.runtime_state(),
        paused: e.is_paused(),
        access: &access,
        sensing_up: e.sensing_process_up(),
        camera_state: e.camera_state(),
        camera_permission: e.camera_permission(),
        enrolled_samples: e.enrollment_stats().sample_count,
        required_samples: e.samples_needed_for_identity(),
        microphone_state: e.voice_state(),
        // The same judgement the speech gate makes, from the same clock: a
        // LISTENING the sensing layer has not refreshed is not believed.
        microphone_live: lantern_core::voice::policy::microphone_busy(&e, t),
        microphone_permission: e.microphone_permission(),
        // From the live report and the conditions, never from the setting: what
        // the owner asked for is not what is running.
        waiting_for_name: lantern_core::voice::wake::listening_now(e.wake(), &e.wake_conditions(wake.enabled), t),
        wake_phrase: &wake.phrase,
        speaking,
        computer_sampling: e.computer_sampling_reported(),
        memory_writable,
        thinking_seconds,
        records: &actions.records,
        goal_waiting: actions.tasks.iter().rev().find_map(|t| t.waiting),
        frontmost_app: computer.as_ref().and_then(|(n, _)| n.as_deref()),
        idle_seconds: computer.as_ref().map(|(_, i)| *i),
        activity_inference: activity.as_deref(),
        people_in_view: e.people_in_view(),
        now: t,
    };
    lantern_core::surface::project(&inputs)
}

/// A whole spoken reply that refers to what is already happening: "yes", "cancel that",
/// "the older one". Null when it is not one (a command or a question). Every path is
/// the window's own (`transaction::resolve_reference`): a spoken yes re-authorizes and
/// grants nothing by itself. What KUE answers is a fixed sentence with no data.
#[tauri::command(async)]
fn voice_reference(text: String, app: tauri::AppHandle, state: tauri::State<AppState>) -> Option<transaction::ReferenceOutcome> {
    let r = reference::interpret(&text)?;
    if r == reference::Reference::Cancel { state.voice.stop(SpeechStop::StoppedByYou, now()); }
    let max_age = state.voice.out.lock().unwrap().spoken_confirmation_seconds;
    let outcome = with_actions(Some(&app), &state, |rt| transaction::resolve_reference(rt, r, max_age));
    if let Some(m) = &outcome.message { state.voice.say_plain(m, "reference", Priority::AuthorizationRequired, now()); }
    Some(outcome)
}

/// KUE's voice for the window: the queue's state, the settings, and the installed
/// voices. No spoken text: requests carry what they were about, not what they said.
#[tauri::command]
fn get_speech(state: tauri::State<AppState>) -> serde_json::Value {
    let out = state.voice.out.lock().unwrap();
    let mut sp = state.voice.speech.lock().unwrap();
    use lantern_core::voice::speaker::SpeechProvider;
    let choice = sp.select_voice(out.controller.settings.voice.as_deref(), &out.controller.language);
    serde_json::json!({
        "provider": { "id": "MACOS_NATIVE", "installed": state.voice.exe.is_some(), "running": sp.is_running(),
                      "spoken": sp.spoken, "cut_off": sp.cut_off, "failures": sp.failures, "last_failure": sp.last_failure,
                      "start_latency_ms": sp.start_latency_ms.first() },
        "external_provider": "NOT_CONFIGURED",
        "settings": out.controller.settings,
        "voice": choice.voice, "voice_why": choice.why,
        "voices": sp.voices.iter().filter(|v| !v.personal).collect::<Vec<_>>(),
        "status": out.controller.status(),
    })
}

/// How KUE sounds and how much it says. Clamped by core; none of it widens what
/// may be spoken, so it needs no authorization.
#[tauri::command]
fn set_voice_settings(settings: VoiceSettings, state: tauri::State<AppState>) -> VoiceSettings {
    let mut out = state.voice.out.lock().unwrap();
    out.controller.settings = settings.clamped();
    out.controller.settings.clone()
}

/// Stop speaking. Always allowed, whoever asks.
/// Whether KUE listens for its name, and what for. Turning it on opens the
/// microphone; the window says so while it is open, and every other condition
/// (pause, kill, permission, the sensing layer) still has to hold.
#[tauri::command]
fn set_wake(enabled: bool, phrase: Option<String>, state: tauri::State<AppState>) -> WakeSetting {
    let mut w = state.wake.lock().unwrap();
    w.enabled = enabled;
    if let Some(p) = phrase {
        let p = p.trim().to_string();
        if !p.is_empty() { w.phrase = p; }
    }
    // Off means off now, not at the next tick.
    if !enabled {
        let _ = state.sensing.lock().unwrap().send(r#"{"cmd":"wake_stop"}"#);
        state.engine.lock().unwrap().forget_wake();
    } else {
        // On re-reads what macOS says about the microphone, so access the owner
        // granted in System Settings since the last report counts. The start
        // itself is the pump's decision, like every other start.
        state.engine.lock().unwrap().wake_turned_on();
        let _ = state.sensing.lock().unwrap().send(r#"{"cmd":"status"}"#);
    }
    w.clone()
}

#[tauri::command]
fn get_wake(state: tauri::State<AppState>) -> serde_json::Value {
    let setting = state.wake.lock().unwrap().clone();
    let e = state.engine.lock().unwrap();
    let conditions = e.wake_conditions(setting.enabled);
    let report = e.wake();
    let listening = lantern_core::voice::wake::listening_now(report, &conditions, now());
    serde_json::json!({
        "enabled": setting.enabled,
        "phrase": setting.phrase,
        "listening": listening,
        "said": if listening { report.state.said(&report.phrase) } else { lantern_core::voice::wake::WakeState::Off.said(&setting.phrase) },
        "why_not": conditions.why_not(),
    })
}

#[tauri::command]
fn stop_speaking(state: tauri::State<AppState>) {
    state.voice.stop(SpeechStop::StoppedByYou, now());
}

#[tauri::command]
fn get_conversation(state: tauri::State<AppState>) -> serde_json::Value {
    conversation_payload(&state.engine, &state.mind, &state.conversation)
}

/// PAUSE. Stops the camera AND computer-activity sampling at the sensing layer,
/// not just in the UI.
///
/// The order matters in both directions:
///  * pause — the sensing layer is told first, then the core discards state, so
///    there is no window in which the UI reads "paused" while sampling continues;
///  * resume — the core un-pauses first, so the first fresh reading is accepted
///    rather than mistaken for a reading that arrived after pause.
#[tauri::command]
fn set_paused(paused: bool, state: tauri::State<AppState>) -> Result<(), String> {
    let r = apply_pause(&state.engine, &state.sensing, paused);
    // Pause stops speech and drops its queue now, not on the next tick.
    state.voice.pump(now());
    r
}

/// The whole of pause and resume, independent of Tauri so it can be exercised
/// against the real sensing layer in a test.
fn apply_pause(engine: &Mutex<Engine>, sensing: &Mutex<Sensing>, paused: bool) -> Result<(), String> {
    if !paused && engine.lock().unwrap().is_killed() {
        return Err("KUE is killed. Resume cannot undo a kill; use Recover.".into());
    }
    if paused {
        {
            let mut s = sensing.lock().unwrap();
            if s.is_running() {
                s.send(r#"{"cmd":"pause"}"#)?;
            }
        }
        engine.lock().unwrap().set_paused(true, now());
        return Ok(());
    }

    if !sensing.lock().unwrap().is_running() {
        return Err("The sensing layer is not running. Use Restart sensing.".into());
    }
    let rates = {
        let mut e = engine.lock().unwrap();
        e.set_paused(false, now());
        e.rates_json()
    };
    sensing.lock().unwrap().send(&format!(r#"{{"cmd":"resume",{rates}}}"#))
}

/// Sends a command to the sensing layer, unless KUE is killed.
fn send_unless_killed(state: &AppState, json: &str) -> Result<(), String> {
    if state.engine.lock().unwrap().is_killed() {
        return Err("KUE is killed. Nothing reaches the sensing layer until you recover.".into());
    }
    state.sensing.lock().unwrap().send(json)
}

#[tauri::command(async)]
fn enroll_capture(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::EnrollmentCapture)?;
    send_unless_killed(&state, r#"{"cmd":"enroll_capture"}"#)
}

/// Captures a sample of someone who is NOT you, into a separate store, so the
/// matcher can be measured without corrupting your enrolled profile.
#[tauri::command(async)]
fn probe_capture(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::ProbeCapture)?;
    send_unless_killed(&state, r#"{"cmd":"probe_capture"}"#)
}

#[tauri::command(async)]
fn probe_reset(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::ProbeReset)?;
    send_unless_killed(&state, r#"{"cmd":"probe_reset"}"#)
}

#[tauri::command(async)]
fn probe_report(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::ProbeReport)?;
    send_unless_killed(&state, r#"{"cmd":"probe_report"}"#)
}

/// Removes the most recent enrollment sample(s) — the way back from capturing
/// the wrong face into your own profile.
#[tauri::command(async)]
fn enroll_undo(count: u32, state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::EnrollmentUndo)?;
    send_unless_killed(&state, &format!(r#"{{"cmd":"enroll_undo","count":{count}}}"#))
}

#[tauri::command(async)]
fn enroll_reset(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::EnrollmentReset)?;
    send_unless_killed(&state, r#"{"cmd":"enroll_reset"}"#)
}

#[tauri::command]
fn restart_sensing(state: tauri::State<AppState>) -> Result<(), String> {
    let exe = state.sensing_exe.lock().unwrap().clone()
        .ok_or_else(|| "The sensing layer could not be located on disk.".to_string())?;
    start_sensing(exe, &state.engine, &state.sensing)
}

/// Starts (or restarts) the sensing layer and its camera. Refused while killed:
/// this is the one place sensing is started, so it is the one place to guard.
fn start_sensing(exe: PathBuf, engine: &Arc<Mutex<Engine>>, sensing: &Arc<Mutex<Sensing>>) -> Result<(), String> {
    if engine.lock().unwrap().is_killed() {
        return Err("KUE is killed. Sensing cannot start until you recover.".into());
    }
    sensing.lock().unwrap().shutdown();
    sensing::spawn(exe, Arc::clone(engine), Arc::clone(sensing))?;
    let rates = engine.lock().unwrap().rates_json();
    std::thread::sleep(std::time::Duration::from_millis(250));
    // Checked again: a kill may have landed while the process was starting.
    if engine.lock().unwrap().is_killed() {
        sensing.lock().unwrap().shutdown();
        return Err("KUE was killed while sensing was starting; it has been stopped.".into());
    }
    engine.lock().unwrap().set_paused(false, now());
    sensing.lock().unwrap().send(&format!(r#"{{"cmd":"start",{rates}}}"#))
}

// MARK: Kill switch
//
// Every command here comes from a gesture in Lantern's own window, so it is
// stamped Principal::Owner. No model has a path to these commands.

/// KILL. The latch is saved first, readings are discarded, then the sensing
/// process is terminated — not paused — so the camera is released.
fn apply_kill(engine: &Mutex<Engine>, sensing: &Mutex<Sensing>, mind: &Mutex<Mind>, voice: &Voice, by: Principal, reason: &str) -> Result<(), String> {
    let saved = engine.lock().unwrap().kill(by, reason, now());
    // Speech first: it is the one output a person hears after pressing Kill.
    voice.kill(now());
    // The microphone stops with everything else. Shutting the sensing layer
    // down takes the listener with it, but not for a moment or two, and KUE
    // must not report itself as listening for even that long.
    engine.lock().unwrap().forget_wake();
    let _ = sensing.lock().unwrap().send(r#"{"cmd":"wake_stop"}"#);
    sensing.lock().unwrap().shutdown();
    mind.lock().unwrap().shutdown();
    saved
}

#[tauri::command]
fn kill_kue(reason: Option<String>, state: tauri::State<AppState>) -> Result<(), String> {
    state.conversation.lock().unwrap().clear("KUE was killed.");
    apply_kill(&state.engine, &state.sensing, &state.mind, &state.voice, Principal::Owner,
        reason.as_deref().unwrap_or("Kill switch pressed in KUE's window."))
}

#[tauri::command]
fn begin_recovery(state: tauri::State<AppState>) -> Result<(), String> {
    state.engine.lock().unwrap().begin_recovery(Principal::Owner, now()).map_err(|r| r.to_string())
}

#[tauri::command]
fn cancel_recovery(state: tauri::State<AppState>) -> Result<(), String> {
    state.engine.lock().unwrap().cancel_recovery(Principal::Owner).map_err(|r| r.to_string())
}

/// Clears the latch, then starts sensing again — explicitly, never on its own.
/// Requires fresh Touch ID or password: whoever is at the window is not
/// necessarily the owner, and the camera is off, so it cannot tell.
#[tauri::command(async)]
fn complete_recovery(state: tauri::State<AppState>) -> Result<(), String> {
    gate(&state.engine, Operation::RecoverFromKill)?;
    state.engine.lock().unwrap().complete_recovery(Principal::Owner, now()).map_err(|r| r.to_string())?;
    if let Err(e) = state.voice.start() { eprintln!("[lantern] speech output not started: {e}"); }
    let exe = state.sensing_exe.lock().unwrap().clone()
        .ok_or_else(|| "Recovered, but the sensing layer could not be located on disk.".to_string())?;
    start_sensing(exe, &state.engine, &state.sensing)
}

#[tauri::command(async)]
fn erase_memory(state: tauri::State<AppState>) -> Result<String, String> {
    gate(&state.engine, Operation::EraseMemory)?;
    let s = state.store.lock().unwrap();
    match s.as_ref() {
        Some(st) => st.erase_all().map(|_| "Local memory erased.".to_string())
            .map_err(|e| e.to_string()),
        None => Err("Local memory is not open.".into()),
    }
}

/// What KUE keeps about the owner, for the window. The owner's own memory,
/// so it meets the same bar as the conversation: LEVEL_2, and not killed.
/// Each row carries why it is kept and when, because "why do you remember
/// that?" should be answerable by looking as well as by asking.
#[tauri::command]
fn get_memories(state: tauri::State<AppState>) -> serde_json::Value {
    let t = now();
    let e = match state.engine.lock() { Ok(e) => e, Err(_) => return serde_json::json!({}) };
    let visible = !e.is_killed() && e.access_block(t).level >= AuthLevel::Level2;
    if !visible {
        return serde_json::json!({
            "visible": false,
            "withheld": if e.is_killed() { "I'm stopped, so I can't show you what I keep." }
                        else { "I need to be sure it's you before I show you what I keep. Show your face, or use Touch ID." },
        });
    }
    let (current, past) = e.memory().counts(t);
    let kept: Vec<_> = e.memory().current(t).into_iter().map(|m| serde_json::json!({
        "id": m.id,
        "class": m.class.tag(),
        "heading": m.class.heading(),
        "statement": m.statement,
        "state": m.state_at(t).tag(),
        "why": m.provenance,
        "when": lantern_core::storage::when(m.created_at),
        "expires": m.valid_for.is_some(),
    })).collect();
    serde_json::json!({ "visible": true, "kept": kept, "current": current, "past": past })
}

/// The owner forgetting one memory, by its id, from the window. The words go
/// here and from the store on the next pass — the same path as forgetting it
/// out loud.
#[tauri::command]
fn forget_memory(id: String, state: tauri::State<AppState>) -> Result<String, String> {
    let t = now();
    {
        let e = state.engine.lock().map_err(|_| "the engine lock was poisoned")?;
        if !e.is_killed() && e.access_block(t).level < AuthLevel::Level2 {
            return Err(lantern_core::transaction::NOT_SURE_ITS_YOU.into());
        }
        if let Some(no) = e.memory_refusal() { return Err(no.into()); }
    }
    let mut e = state.engine.lock().map_err(|_| "the engine lock was poisoned")?;
    match e.memory_mut().forget(&id, t) {
        Some(gone) => {
            e.record_action_event(format!("Memory {id}: forgotten at the owner's request, from the window."), t);
            Ok(format!("Forgotten: {}", gone.statement))
        }
        None => Err("I don't have that one any more.".into()),
    }
}

#[tauri::command]
fn storage_stats(state: tauri::State<AppState>) -> serde_json::Value {
    let s = state.store.lock().unwrap();
    match s.as_ref() {
        Some(st) => serde_json::json!({
            "open": true,
            "path": support_dir().join("lantern.sqlite3").to_string_lossy(),
            "events": st.event_count().unwrap_or(0),
            "snapshots": st.snapshot_count().unwrap_or(0),
        }),
        None => serde_json::json!({ "open": false }),
    }
}

/// The privacy firewall's state: policy version, decision totals for this
/// launch, the audit ledger, and how many pre-firewall snapshots remain.
#[tauri::command]
fn privacy_status(state: tauri::State<AppState>) -> serde_json::Value {
    let (allowed, denied) = state.firewall.lock().map(|f| f.totals()).unwrap_or((0, 0));
    let s = state.store.lock().unwrap();
    let (legacy, ledger) = match s.as_ref() {
        Some(st) => (st.legacy_snapshot_count().ok(), st.ledger().unwrap_or_default()),
        None => (None, vec![]),
    };
    serde_json::json!({
        "policy_version": PRIVACY_POLICY_VERSION,
        "decisions_allowed": allowed,
        "decisions_denied": denied,
        "legacy_snapshots": legacy,
        "ledger": ledger,
    })
}

/// Deletes snapshots written before the privacy firewall existed, and compacts
/// the database so their content is removed from disk. Explicit owner action only.
#[tauri::command(async)]
fn purge_legacy_snapshots(state: tauri::State<AppState>) -> Result<String, String> {
    gate(&state.engine, Operation::PurgeLegacySnapshots)?;
    let s = state.store.lock().unwrap();
    let st = s.as_ref().ok_or_else(|| "Local memory is not open.".to_string())?;
    let n = st.purge_legacy_snapshots().map_err(|e| e.to_string())?;
    Ok(format!("Deleted {n} snapshot(s) written before the privacy firewall, and compacted local memory."))
}

/// Opens the macOS Camera privacy pane so a denied permission can be fixed.
#[tauri::command]
fn open_camera_settings() -> Result<(), String> {
    open_permission_settings("CAMERA".into())
}

/// Opens the System Settings pane for one permission. KUE never changes a
/// permission — only the owner can, in macOS's own interface — so this takes
/// them there and stops.
#[tauri::command]
fn open_permission_settings(permission: String) -> Result<(), String> {
    let pane = match permission.as_str() {
        "CAMERA" => "Privacy_Camera",
        "MICROPHONE" => "Privacy_Microphone",
        "SPEECH_RECOGNITION" => "Privacy_SpeechRecognition",
        "ACCESSIBILITY" => "Privacy_Accessibility",
        "NOTIFICATIONS" => "Notifications",
        // Unknown permission = do nothing, rather than open something arbitrary.
        other => return Err(format!("KUE has no Settings pane for {other}.")),
    };
    std::process::Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// MARK: - Run

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let resource_dir = app.path().resource_dir().ok();
            let (config, config_note) = load_config(resource_dir.clone());
            let max_events = config.storage.max_events;
            let max_snapshots = config.storage.max_snapshots;
            let snapshot_every = config.storage.snapshot_interval_seconds as f64;

            let engine = Arc::new(Mutex::new(Engine::new(config.clone(), config_note)));
            let sensing = Arc::new(Mutex::new(Sensing::new()));

            let (store, open_error) = match Store::open(&support_dir().join("lantern.sqlite3"), max_events, max_snapshots) {
                Ok(s) => (Some(s), None),
                Err(e) => {
                    eprintln!("[lantern] local memory unavailable: {e}");
                    (None, Some(format!("the database could not be opened: {e}")))
                }
            };
            // The kill latch is read before anything is started.
            let launched_killed = engine.lock().unwrap()
                .attach_kill_latch(support_dir().join(LATCH_FILE_NAME), now()) == RuntimeState::Killed;
            {
                let mut e = engine.lock().unwrap();
                e.set_storage_status(open_error.clone(), now());
                // Read before sensing starts, so everything here predates this launch.
                if let Some(st) = store.as_ref() {
                    if let Ok(h) = st.history(40) { e.set_remembered(h); }
                    // What KUE keeps about the owner's world, from the last run
                    // and every run before it. A row it cannot read is left
                    // where it is and counted, never guessed at.
                    match st.memories() {
                        Ok((rows, unreadable)) => {
                            let kept = rows.len();
                            e.memory_mut().load(rows);
                            if kept > 0 || unreadable > 0 {
                                e.record_action_event(format!(
                                    "Memory: {kept} kept from before this launch{}.",
                                    if unreadable > 0 { format!(", {unreadable} row(s) KUE could not read and left alone") } else { String::new() }),
                                    now());
                            }
                        }
                        Err(err) => eprintln!("[lantern] memories could not be read: {err}"),
                    }
                }
            }
            let store = Arc::new(Mutex::new(store));

            let mut note = String::new();
            let exe = match sensing::locate_sensing_app(resource_dir) {
                Ok(p) => {
                    if launched_killed {
                        note = "KUE launched KILLED: the kill latch is set, so nothing was started.".into();
                    } else if let Err(e) = start_sensing(p.clone(), &engine, &sensing) {
                        note = e;
                    }
                    Some(p)
                }
                Err(e) => {
                    note = e;
                    None
                }
            };

            let firewall = Arc::new(Mutex::new(Firewall::new()));
            let mind = Arc::new(Mutex::new(Mind::new()));
            let conversation = Arc::new(Mutex::new(Conversation::new()));
            let action_book = Arc::new(Mutex::new(transaction::ActionBook::new()));
            let voice_cfg = config.voice.clone();
            let voice = Voice {
                engine: Arc::clone(&engine), firewall: Arc::clone(&firewall),
                out: Arc::new(Mutex::new(VoiceOut {
                    narrator: lantern_core::voice::narration::Narrator::new(),
                    controller: lantern_core::voice::speaker::SpeechController::new(voice_cfg.settings(), voice_cfg.language.clone()),
                    spoken_confirmation_seconds: voice_cfg.spoken_confirmation_seconds,
                })),
                speech: Arc::new(Mutex::new(Speech::new())),
                exe: speech::locate(),
            };
            if !launched_killed {
                if let Err(e) = voice.start() { eprintln!("[lantern] speech output not started: {e}"); }
            }
            let wake_setting = Arc::new(Mutex::new(WakeSetting {
                enabled: voice_cfg.wake_enabled,
                phrase: voice_cfg.wake_phrase.clone(),
            }));
            app.manage(AppState {
                engine: Arc::clone(&engine),
                firewall: Arc::clone(&firewall),
                sensing: Arc::clone(&sensing),
                store: Arc::clone(&store),
                sensing_exe: Arc::new(Mutex::new(exe)),
                startup_note: Arc::new(Mutex::new(note)),
                mind: Arc::clone(&mind),
                mind_exe: mind::locate(),
                conversation: Arc::clone(&conversation),
                actions: Arc::clone(&action_book),
                voice: voice.clone(),
                wake: Arc::clone(&wake_setting),
                plan_wait: Arc::new(Mutex::new(None)),
            });

            // Context pump: builds the context object and pushes it to the UI.
            // Also persists new events and periodic snapshots to local memory.
            let handle = app.handle().clone();
            let pump_sensing = Arc::clone(&sensing);
            let pump_wake = Arc::clone(&wake_setting);
            let pump_mind = Arc::clone(&mind);
            let action_book_pump = Arc::clone(&action_book);
            let firewall = Arc::clone(&firewall);
            let initial_rates = engine.lock().unwrap().rates_json();
            std::thread::spawn(move || {
                let mut memory = MemoryPump::new(snapshot_every);
                let mut tick: u64 = 0;
                let mut applied_rates = initial_rates;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let t = now();
                    tick += 1;

                    // Authorization timers lapse on the clock, not only when a frame lands.
                    let access_now = engine.lock().map(|mut e| { e.tick_access(t); (e.is_killed(), e.access_block(t).state) });

                    // The conversation belongs to the owner's session. When the session
                    // stops being theirs, it is cleared and any answer in flight cancelled.
                    let clear_because = match access_now {
                        Ok((true, _)) => Some("KUE was killed."),
                        Ok((_, AccessState::Locked)) => Some("The session locked."),
                        Ok((_, AccessState::UnknownPerson)) => Some("Someone who is not you appeared."),
                        Ok((_, AccessState::MultiplePeople)) => Some("More than one person appeared."),
                        _ => None,
                    };
                    if let Some(because) = clear_because {
                        // "Pending actions stopped": nothing waits for confirmation across a kill or lock.
                        let cancelled = action_book_pump.lock().map(|mut b| b.cancel_pending_at(because, t)).unwrap_or(0);
                        if cancelled > 0 { emit_actions(&handle); }
                        let pending = conversation.lock().ok().and_then(|c| c.pending.clone());
                        let had = conversation.lock().map(|c| !c.exchanges.is_empty() || c.pending.is_some()).unwrap_or(false);
                        if let Some(p) = pending {
                            let _ = pump_mind.lock().map(|mut m| ModelProvider::cancel(&mut *m, &p.id));
                        }
                        if had {
                            if let Ok(mut c) = conversation.lock() { c.clear(because); }
                            emit_conversation(&handle, &engine, &pump_mind, &conversation);
                        }
                    }

                    // A latch created outside the app (e.g. from a terminal) kills KUE.
                    let killed_from_outside = engine.lock().map(|mut e| e.poll_kill_latch(t)).unwrap_or(false);
                    if killed_from_outside {
                        voice.kill(t);
                        if let Ok(mut s) = pump_sensing.lock() { s.shutdown(); }
                        if let Ok(mut m) = pump_mind.lock() { m.shutdown(); }
                    }
                    // Speech obeys kill, pause, lock and a live microphone on the clock too.
                    voice.pump(t);

                    // Listening for its name: started and stopped from the same
                    // conditions every tick, so the microphone follows the
                    // owner's setting, pause, kill and the sensing layer's own
                    // life without anyone having to remember to stop it.
                    {
                        let setting = pump_wake.lock().map(|w| w.clone()).unwrap_or(WakeSetting { enabled: false, phrase: String::new() });
                        let step = match engine.lock() {
                            Ok(mut e) => e.wake_step(setting.enabled, t),
                            Err(_) => continue,
                        };
                        use lantern_core::voice::wake::WakeStep;
                        let command = match step {
                            WakeStep::Listen =>
                                Some(serde_json::json!({ "cmd": "wake_start", "phrase": setting.phrase }).to_string()),
                            WakeStep::Stop => Some(r#"{"cmd":"wake_stop"}"#.to_string()),
                            // Heard its name and nothing else. The sentence that
                            // follows is caught by the ordinary listening
                            // session — the one the window shows and the one
                            // whose transcripts the owner can see. Through the
                            // same gate the button goes through: a wake is not
                            // a way around anything.
                            WakeStep::Capture => match gate(&engine, Operation::Listen) {
                                // The owner is about to speak, so KUE stops talking.
                                Ok(()) => { voice.stop(SpeechStop::UserSpeaking, t);
                                    Some(format!(r#"{{"cmd":"listen_start","maxSeconds":30,"silenceSeconds":{LISTEN_SILENCE_SECONDS}}}"#)) }
                                Err(_) => None,
                            },
                            WakeStep::Nothing => None,
                        };
                        if let Some(command) = command {
                            if let Ok(mut s) = pump_sensing.lock() { let _ = s.send(&command); }
                        }
                    }

                    // A request that arrived by voice runs the same way a typed
                    // one does: the command parser first, then a question.
                    // Being heard is not being authorized.
                    let spoken = engine.lock().map(|mut e| e.take_wake_requests()).unwrap_or_default();
                    for request in spoken {
                        handle_spoken_request(&handle, &request);
                    }

                    // Measure this process every 5s, alongside the sensing layer's own report.
                    if tick % 10 == 0 {
                        if let (Some((cpu, footprint)), Ok(mut e)) = (sensing::own_usage(), engine.lock()) {
                            e.record_shell_usage(t, cpu, footprint);
                        }
                    }

                    // Apply the core's analysis-rate policy when it changes.
                    let (want, paused) = match engine.lock() {
                        Ok(e) => (e.rates_json(), e.is_paused()),
                        Err(_) => continue,
                    };
                    if !paused && want != applied_rates {
                        if let Ok(mut s) = pump_sensing.lock() {
                            if s.send(&format!(r#"{{"cmd":"set_fps",{want}}}"#)).is_ok() {
                                applied_rates = want;
                            }
                        }
                    }

                    // Every write passes the privacy firewall; none happens while killed.
                    // A failed write is reported to the core, which surfaces STORAGE_UNAVAILABLE.
                    let storage_error: Option<String> = match (store.lock(), firewall.lock(), engine.lock()) {
                        (Err(_), _, _) => Some("the storage lock was poisoned".into()),
                        (_, Err(_), _) => Some("the privacy firewall lock was poisoned; nothing was written".into()),
                        (_, _, Err(_)) => continue,
                        (Ok(guard), Ok(mut fw), Ok(mut e)) => match guard.as_ref() {
                            None => open_error.clone(),
                            Some(st) => {
                                // One row of evidence about this moment, if one is
                                // due. Observation only — it reads the engine's
                                // state and changes none of it.
                                e.sample_measurement(t);
                                memory.tick(&mut e, &mut fw, st, t).err()
                            }
                        },
                    };
                    let (ctx, heard) = match engine.lock() {
                        Ok(mut e) => {
                            e.set_storage_status(storage_error, t);
                            let t_ctx = now();
                            let ctx = e.build_context(t);
                            let done = now();
                            e.telemetry().record(lantern_core::telemetry::Span::new(
                                lantern_core::telemetry::Stage::ContextBuild, "tick", t_ctx, done,
                                lantern_core::telemetry::SpanOutcome::Ok));
                            // The pump's own pass, so a slow tick is visible as a
                            // slow tick rather than as a mysterious gap.
                            e.telemetry().record(lantern_core::telemetry::Span::new(
                                lantern_core::telemetry::Stage::CoreTick, "tick", t, done,
                                lantern_core::telemetry::SpanOutcome::Ok));
                            (ctx, e.take_transcripts())
                        }
                        Err(_) => continue,
                    };
                    // What you said goes to the interface's conversation box and nowhere else.
                    for (session, text, is_final) in heard {
                        let _ = handle.emit("lantern://transcript",
                            serde_json::json!({ "session": session, "text": text, "is_final": is_final }));
                    }

                    let _ = handle.emit("lantern://context", &ctx);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_context,
            get_surface,
            get_perception,
            get_runtime,
            receive_input,
            get_capabilities,
            get_capability_sheet,
            startup_note,
            set_paused,
            enroll_capture,
            enroll_reset,
            enroll_undo,
            probe_capture,
            probe_reset,
            probe_report,
            restart_sensing,
            erase_memory,
            get_memories,
            forget_memory,
            storage_stats,
            privacy_status,
            purge_legacy_snapshots,
            open_camera_settings,
            open_permission_settings,
            kill_kue,
            begin_recovery,
            cancel_recovery,
            complete_recovery,
            authenticate,
            lock_session,
            auth_hardware,
            ask,
            cancel_ask,
            get_conversation,
            listen_start,
            listen_stop,
            listen_cancel,
            propose_command,
            confirm_action,
            cancel_action,
            get_actions,
            get_storage,
            check_storage,
            set_wake,
            get_wake,
            trash_selected,
            undo_trash,
            voice_reference,
            get_speech,
            set_voice_settings,
            stop_speaking,
        ])
        .on_window_event(|window, event| {
            // Closing the window must stop the camera, not orphan the sensing process.
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.voice.speech.lock().unwrap().shutdown();
                    state.sensing.lock().unwrap().shutdown();
                    state.mind.lock().unwrap().shutdown();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running KUE");
}

#[cfg(test)]
mod tests {
    //! End-to-end tests against the REAL Swift sensing binary.
    //!
    //! The camera is never started (no `start` command is sent), so these run
    //! without a camera permission and never switch a camera on. They exercise
    //! the same pause path the interface uses, through the real process and
    //! protocol. Build the sensing layer first: ./sensing/build.sh

    use super::*;
    use std::time::Duration;

    /// LIVE: the measurement KUE reports is the one macOS reports. Compared
    /// against `df`, which reads the same volume through a different tool — if
    /// the two ever disagree, the number KUE puts on screen is wrong.
    #[test]
    fn the_volume_measurement_is_the_one_macos_reports() {
        let home = home_dir();
        let measured = measure_volume(&home).expect("this Mac has a volume");
        let df = std::process::Command::new("/bin/df").arg("-k").arg(&home).output().expect("df runs");
        let text = String::from_utf8_lossy(&df.stdout);
        let row = text.lines().nth(1).expect("df prints a row");
        let cols: Vec<&str> = row.split_whitespace().collect();
        let blocks: u64 = cols[1].parse().expect("1K-blocks");
        let available: u64 = cols[3].parse().expect("available");

        assert_eq!(measured.capacity, blocks * 1024, "capacity disagrees with df: {row}");
        // Free space moves between the two calls; a percent of the drive is not
        // drift, it is a different number.
        let drift = (measured.available as i64 - (available * 1024) as i64).unsigned_abs();
        assert!(drift < measured.capacity / 100, "available disagrees with df by {drift} bytes: {row}");
        assert!(measured.used <= measured.capacity && measured.available <= measured.capacity);
        assert!(measured.percent_used() <= 100);
        eprintln!("volume: {} capacity, {} available, {}% used",
            lantern_core::storage::size_words(measured.capacity),
            lantern_core::storage::size_words(measured.available), measured.percent_used());
    }

    fn sensing_exe() -> Option<PathBuf> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../sensing/bundle/LanternSense.app/Contents/MacOS/lantern-sense");
        p.exists().then_some(p)
    }

    #[test]
    fn pause_stops_every_reading_from_the_real_sensing_layer() {
        let Some(exe) = sensing_exe() else {
            eprintln!("skipped: sensing layer not built (run ./sensing/build.sh)");
            return;
        };
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        let sensing = Arc::new(Mutex::new(Sensing::new()));
        sensing::spawn(exe, Arc::clone(&engine), Arc::clone(&sensing)).expect("spawn sensing layer");

        // Running: computer-activity readings arrive about once a second.
        std::thread::sleep(Duration::from_millis(2500));
        let ctx = engine.lock().unwrap().build_context(now());
        assert_eq!(ctx.sensors.sensing_process, "RUNNING");
        assert!(ctx.computer.frontmost_app.is_some() || ctx.computer.idle_seconds.is_some(),
            "expected live computer context before pausing");
        assert_eq!(ctx.sensors.computer_sampling_reported, Some(true));

        // Paused, and held well past the in-flight grace window.
        apply_pause(&engine, &sensing, true).unwrap();
        std::thread::sleep(Duration::from_millis(5000));
        let ctx = engine.lock().unwrap().build_context(now());
        assert!(ctx.sensors.paused);
        assert_eq!(ctx.activity.label, "PAUSED");
        assert_eq!(ctx.sensors.readings_after_pause, 0,
            "the real sensing layer sent readings after pause took effect");
        assert_eq!(ctx.sensors.computer_sampling_reported, Some(false),
            "the sensing layer must report that sampling stopped");
        assert_eq!(ctx.computer.frontmost_app, None);
        assert_eq!(ctx.computer.idle_seconds, None);
        assert!(ctx.contradictions.is_empty(), "unexpected: {:?}", ctx.contradictions);

        // Resumed: readings flow again, and none is mistaken for a late one.
        apply_pause(&engine, &sensing, false).unwrap();
        std::thread::sleep(Duration::from_millis(2500));
        let ctx = engine.lock().unwrap().build_context(now());
        assert!(!ctx.sensors.paused);
        assert_eq!(ctx.sensors.computer_sampling_reported, Some(true));
        assert!(ctx.computer.idle_seconds.is_some(), "computer context must return after resume");
        assert_eq!(ctx.sensors.readings_after_pause, 0);

        // Self-measurement kept running through the pause, and measured its cost.
        let r = &ctx.resources;
        assert!(r.sensing.footprint_mb.unwrap_or(0.0) > 1.0, "sensing footprint: {:?}", r.sensing);
        assert!(r.sensing.cpu_percent.is_some(), "two health samples should give a CPU rate");
        assert!(r.paused_seconds_measured > 0.0, "the cost of pause must be measured");
        eprintln!("measured while paused: sensing CPU {:?}% over {:.0}s, footprint {:.1} MB",
            r.sensing_cpu_paused_percent, r.paused_seconds_measured, r.sensing.footprint_mb.unwrap());

        sensing.lock().unwrap().shutdown();
    }

    /// One model, one owner.
    ///
    /// Two model processes answering at once were measured at 22.3 s against
    /// 11.5 s for one (2026-09-20) — contention is the largest latency effect
    /// KUE actually controls. This asserts the shell cannot create a second
    /// one: `start_mind` is the only path, and it returns early when a model
    /// process is already running.
    #[test]
    fn the_shell_owns_exactly_one_model_process() {
        let Some(exe) = mind::locate() else { eprintln!("skipped: mind not built"); return; };
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        let mind_ = Arc::new(Mutex::new(Mind::new()));
        let conversation = Arc::new(Mutex::new(Conversation::new()));

        start_mind(exe.clone(), &engine, &mind_, &conversation, None, None, None).expect("the first start works");
        let first = mind_.lock().unwrap().pid();
        assert!(mind_.lock().unwrap().is_running());

        // Five more attempts, as five components each deciding they need a model.
        for _ in 0..5 {
            start_mind(exe.clone(), &engine, &mind_, &conversation, None, None, None).expect("a second start is a no-op, not an error");
        }
        assert_eq!(mind_.lock().unwrap().pid(), first, "a second model process was created");

        // And the source has exactly one spawn site, so this cannot be
        // circumvented by a component calling the spawner directly.
        // The needle is assembled at runtime, or this line would match itself.
        let needle = format!("{}{}", "mind::", "spawn(exe");
        let me = include_str!("lib.rs");
        assert_eq!(me.matches(needle.as_str()).count(), 1,
            "the model process is spawned in more than one place; one owner is the point");

        mind_.lock().unwrap().shutdown();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!mind_.lock().unwrap().is_running(), "shutdown leaves no model process behind");
    }

    #[test]
    fn kill_terminates_the_real_sensing_process_and_nothing_can_restart_it() {
        let Some(exe) = sensing_exe() else {
            eprintln!("skipped: sensing layer not built (run ./sensing/build.sh)");
            return;
        };
        let dir = std::env::temp_dir().join(format!("kue-shell-kill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let latch = dir.join(LATCH_FILE_NAME);
        let _ = std::fs::remove_file(&latch);

        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        engine.lock().unwrap().attach_kill_latch(&latch, now());
        let sensing = Arc::new(Mutex::new(Sensing::new()));
        sensing::spawn(exe.clone(), Arc::clone(&engine), Arc::clone(&sensing)).expect("spawn sensing layer");
        std::thread::sleep(Duration::from_millis(2000));
        assert!(sensing.lock().unwrap().is_running());

        let mind = Arc::new(Mutex::new(Mind::new()));
        let conversation = Arc::new(Mutex::new(Conversation::new()));
        let mind_exe = mind::locate();
        if let Some(m) = &mind_exe {
            start_mind(m.clone(), &engine, &mind, &conversation, None, None, None).expect("start the model process");
            assert!(mind.lock().unwrap().is_running());
        }

        let voice = test_voice(&engine);
        if voice.exe.is_some() {
            voice.start().expect("start kue-voice");
            assert!(voice.speech.lock().unwrap().is_running());
        }

        apply_kill(&engine, &sensing, &mind, &voice, Principal::Owner, "e2e").unwrap();
        assert!(!mind.lock().unwrap().is_running(), "the model process is terminated too");
        assert!(!voice.speech.lock().unwrap().is_running(), "and the speech process");
        if voice.exe.is_some() {
            assert!(voice.start().is_err(), "which cannot start while killed");
            assert!(!voice.speech.lock().unwrap().is_running());
        }
        if let Some(m) = &mind_exe {
            assert!(start_mind(m.clone(), &engine, &mind, &conversation, None, None, None).is_err(), "and cannot start while killed");
        }
        assert!(!sensing.lock().unwrap().is_running(), "the process is terminated, not paused");
        assert!(latch.exists());
        std::thread::sleep(Duration::from_millis(1500));
        let ctx = engine.lock().unwrap().build_context(now());
        assert_eq!(ctx.runtime.state, RuntimeState::Killed);
        assert_eq!(ctx.sensors.sensing_process, "DOWN");
        assert_eq!(ctx.computer.frontmost_app, None);

        // Every path that could start observation again is refused.
        assert!(apply_pause(&engine, &sensing, false).is_err());
        assert!(start_sensing(exe.clone(), &engine, &sensing).is_err());
        assert!(!sensing.lock().unwrap().is_running());
        assert!(engine.lock().unwrap().complete_recovery(Principal::Model, now()).is_err());

        // A relaunch reads the latch and comes back killed.
        let relaunched = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        assert_eq!(relaunched.lock().unwrap().attach_kill_latch(&latch, now()), RuntimeState::Killed);
        assert!(start_sensing(exe, &relaunched, &Arc::new(Mutex::new(Sensing::new()))).is_err());

        engine.lock().unwrap().begin_recovery(Principal::Owner, now()).unwrap();
        engine.lock().unwrap().complete_recovery(Principal::Owner, now()).unwrap();
        assert!(!latch.exists());
    }

    #[test]
    fn the_gate_refuses_without_ever_prompting_when_the_core_denies() {
        // Killed: enrollment is denied outright. If the gate prompted, this test
        // would hang on a Touch ID dialog instead of returning.
        let engine = Mutex::new(Engine::new(Config::default_config(), "test".into()));
        let _ = engine.lock().unwrap().kill(Principal::Owner, "t", now());
        let err = gate(&engine, Operation::EnrollmentCapture).unwrap_err();
        assert!(err.contains("KUE is killed"), "{err}");
        assert!(gate(&engine, Operation::Kill).is_ok(), "stopping is always allowed");
    }

    #[test]
    fn the_authentication_helper_reports_this_macs_hardware() {
        if auth::locate().is_none() {
            eprintln!("skipped: kue-auth not built (run ./auth/build.sh)");
            return;
        }
        let p = auth::probe();
        assert_eq!(p["result"], "PROBE", "{p}");
        eprintln!("authentication hardware: {p}");
    }

    /// Slow (the on-device model takes seconds), so it is opt-in:
    /// cargo test -- --ignored a_real_question
    #[test]
    #[ignore]
    fn a_real_question_is_answered_on_device_from_the_cleared_context_only() {
        let Some(exe) = mind::locate() else { eprintln!("skipped: mind not built"); return; };
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        engine.lock().unwrap().ingest(lantern_core::sensor::SensorMessage::Computer {
            ts: now(), frontmost_app: lantern_core::sensor::FrontmostApp { name: Some("Xcode".into()), bundle_id: None },
            idle_seconds: 2.0 }, now());
        let mind_ = Arc::new(Mutex::new(Mind::new()));
        let conversation = Arc::new(Mutex::new(Conversation::new()));
        start_mind(exe, &engine, &mind_, &conversation, None, None, None).unwrap();
        if mind_.lock().unwrap().availability.as_deref() != Some("AVAILABLE") {
            eprintln!("skipped: on-device model unavailable: {:?}", mind_.lock().unwrap().unavailable_reason);
            return;
        }
        let t = now();
        let ctx = engine.lock().unwrap().build_context(t);
        let mut fw = Firewall::new();
        let cleared = fw.clear_model_context(&ctx, &[], "Which app is frontmost right now?", t).unwrap();
        let id = conversation.lock().unwrap().begin("Which app is frontmost right now?", vec![], t).unwrap();
        lantern_core::model::ask(&mut *mind_.lock().unwrap(), &ModelRequest::new(id, cleared)).unwrap();
        let started = std::time::Instant::now();
        while conversation.lock().unwrap().is_busy() && started.elapsed() < Duration::from_secs(90) {
            std::thread::sleep(Duration::from_millis(200));
        }
        let c = conversation.lock().unwrap().clone();
        let ex = c.exchanges.last().expect("an answer or a failure within 90s");
        eprintln!("on-device answer in {:.1}s ({:?}): {}", ex.seconds, ex.outcome, ex.answer);
        assert_eq!(ex.outcome, TurnOutcome::Answered);
        assert!(ex.answer.contains("Xcode"), "the answer should use the cleared context: {}", ex.answer);
        mind_.lock().unwrap().shutdown();
    }

    /// Diagnostic for what the real model says to a question, with the prompt it
    /// was given. Opt-in: LANTERN_Q="What can you do?" cargo test -- --ignored ask_the_real_model --nocapture
    #[test]
    /// Does the prompt's size actually drive the wait?
    ///
    /// 81 % of an answer's wall time was prefill (13,893 ms against 3,353 ms of
    /// generation, 2026-09-20), and the capability table was 48 % of the prompt.
    /// Removing it is only worth something if prefill tracks prompt size — so
    /// this asks the REAL on-device model the same question twice: once with the
    /// prompt as it is now, once with the removed table put back verbatim.
    ///
    ///     cargo test -p lantern --lib model_latency_against_prompt_size -- --ignored --nocapture
    ///
    /// `--lib` matters: without it the same test runs in both the lib and the bin
    /// target at once, two model processes contend, and every number doubles —
    /// which is itself a measurement (2026-09-20: 22.3 s contended against
    /// 11.5 s alone).
    ///
    /// Result, 2026-09-20: mean 11.5 s short against 13.3 s long, for 1,400
    /// characters of difference — a small effect inside a large variance. The
    /// wait is NOT mostly the prompt.
    #[test]
    #[ignore]
    fn model_latency_against_prompt_size() {
        let Some(exe) = mind::locate() else { eprintln!("skipped: mind not built"); return; };
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        let mind_ = Arc::new(Mutex::new(Mind::new()));
        let conversation = Arc::new(Mutex::new(Conversation::new()));
        start_mind(exe, &engine, &mind_, &conversation, None, None, None).unwrap();
        if mind_.lock().unwrap().availability.as_deref() != Some("AVAILABLE") { eprintln!("skipped: model unavailable"); return; }

        // The table as it used to be sent: every row, with its status.
        let old_table = lantern_core::capabilities::rows().iter()
            .map(|c| format!("{}: {:?}", c.name, c.status)).collect::<Vec<_>>().join("; ");
        let question = "In one sentence, what is the capital of France?";
        let mut results: Vec<(&str, f64)> = Vec::new();

        // Alternating, so a warm model does not flatter whichever went second.
        for round in 0..3 {
            for (label, padded) in [("short (now)", false), ("long (old table)", true)] {
                let t = now();
                let ctx = engine.lock().unwrap().build_context(t);
                let mut fw = Firewall::new();
                let cleared = fw.clear_model_context(&ctx, &[], question, t).unwrap();
                let chars = cleared.value().prompt.len() + if padded { old_table.len() } else { 0 };
                let q = if padded { format!("{question}\n(context: {old_table})") } else { question.to_string() };
                let cleared = fw.clear_model_context(&ctx, &[], &q, t).unwrap();
                let id = conversation.lock().unwrap().begin(&q, vec![], t).unwrap();
                lantern_core::model::ask(&mut *mind_.lock().unwrap(), &ModelRequest::new(id, cleared)).unwrap();
                let started = std::time::Instant::now();
                while conversation.lock().unwrap().is_busy() && started.elapsed() < Duration::from_secs(120) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                let ex = conversation.lock().unwrap().exchanges.last().cloned().expect("an answer");
                eprintln!("round {} · {label:<16} · prompt ~{chars} chars · {:.1}s · {:?}", round + 1, ex.seconds, ex.outcome);
                results.push((label, ex.seconds));
                conversation.lock().unwrap().clear("next");
            }
        }
        let mean = |l: &str| {
            let v: Vec<f64> = results.iter().filter(|(k, _)| *k == l).map(|(_, s)| *s).collect();
            v.iter().sum::<f64>() / v.len() as f64
        };
        eprintln!("\nMEAN short {:.1}s · MEAN long {:.1}s", mean("short (now)"), mean("long (old table)"));
        eprintln!("If these are within noise, prompt size is NOT what makes the wait, and the");
        eprintln!("next latency work is model start-up and prewarming, not the prompt.");
        mind_.lock().unwrap().shutdown();
    }

    #[ignore]
    fn ask_the_real_model() {
        let Some(exe) = mind::locate() else { eprintln!("skipped: mind not built"); return; };
        let question = std::env::var("LANTERN_Q").unwrap_or_else(|_| "What can you do?".into());
        let runs: usize = std::env::var("LANTERN_RUNS").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        engine.lock().unwrap().ingest(lantern_core::sensor::SensorMessage::Computer {
            ts: now(), frontmost_app: lantern_core::sensor::FrontmostApp { name: Some("Safari".into()), bundle_id: None },
            idle_seconds: 1.0 }, now());
        let mind_ = Arc::new(Mutex::new(Mind::new()));
        let conversation = Arc::new(Mutex::new(Conversation::new()));
        start_mind(exe, &engine, &mind_, &conversation, None, None, None).unwrap();
        if mind_.lock().unwrap().availability.as_deref() != Some("AVAILABLE") { eprintln!("skipped: model unavailable"); return; }
        for run in 0..runs {
            let t = now();
            let ctx = engine.lock().unwrap().build_context(t);
            let mut fw = Firewall::new();
            let cleared = fw.clear_model_context(&ctx, &[], &question, t).unwrap();
            if run == 0 { eprintln!("--- PROMPT ---\n{}\n--- END PROMPT ---", cleared.value().prompt); }
            let id = conversation.lock().unwrap().begin(&question, vec![], t).unwrap();
            lantern_core::model::ask(&mut *mind_.lock().unwrap(), &ModelRequest::new(id, cleared)).unwrap();
            let started = std::time::Instant::now();
            while conversation.lock().unwrap().is_busy() && started.elapsed() < Duration::from_secs(90) {
                std::thread::sleep(Duration::from_millis(200));
            }
            let ex = conversation.lock().unwrap().exchanges.last().cloned().expect("an answer");
            eprintln!("run {} · {:.1}s · {:?}: {}", run + 1, ex.seconds, ex.outcome, ex.answer);
            for c in &ex.corrections { eprintln!("      correction: {c}"); }
            conversation.lock().unwrap().clear("next run");
        }
        mind_.lock().unwrap().shutdown();
    }

    fn test_voice(engine: &Arc<Mutex<Engine>>) -> Voice {
        let mut settings = VoiceSettings::default();
        // Silent on the test machine's speakers; the synthesizer still reports start and finish.
        settings.volume = 0.0;
        Voice {
            engine: Arc::clone(engine), firewall: Arc::new(Mutex::new(Firewall::new())),
            out: Arc::new(Mutex::new(VoiceOut {
                narrator: lantern_core::voice::narration::Narrator::new(),
                controller: lantern_core::voice::speaker::SpeechController::new(settings, "en-US"),
                spoken_confirmation_seconds: 60.0,
            })),
            speech: Arc::new(Mutex::new(Speech::new())),
            exe: speech::locate(),
        }
    }

    fn wait_for(seconds: f64, cond: impl Fn() -> bool) -> bool {
        let end = std::time::Instant::now() + Duration::from_secs_f64(seconds);
        while std::time::Instant::now() < end { if cond() { return true; } std::thread::sleep(Duration::from_millis(20)); }
        cond()
    }

    /// Through the REAL kue-voice process (AVSpeechSynthesizer), at volume 0:
    /// the core queue sends one sentence, the synthesizer reports it started and
    /// finished, and the request completes with the default voice.
    #[test]
    fn the_real_macos_voice_speaks_a_cleared_sentence_and_the_kill_switch_cuts_it_off() {
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "test".into())));
        let voice = test_voice(&engine);
        if voice.exe.is_none() { eprintln!("skipped: kue-voice not built (run ./voice/build.sh)"); return; }
        voice.start().expect("start kue-voice");
        assert!(wait_for(3.0, || !voice.speech.lock().unwrap().voices.is_empty()), "the process lists its voices");
        {
            let out = voice.out.lock().unwrap();
            let sp = voice.speech.lock().unwrap();
            use lantern_core::voice::speaker::SpeechProvider;
            let choice = sp.select_voice(None, "en-US");
            eprintln!("kue-voice default: {:?} — {}", choice.voice.as_ref().map(|v| &v.name), choice.why);
            assert_eq!(choice.voice.expect("an installed female voice").gender, lantern_core::voice::provider::VoiceGender::Female);
            drop(out);
        }

        // A sentence with no data: an unconfirmed session may hear it.
        use lantern_core::voice::speaker::SpeechState;
        voice.say_plain("There's nothing waiting for your confirmation.", "reference", Priority::AuthorizationRequired, now());
        let id = voice.out.lock().unwrap().controller.status().speaking.expect("sent").id;
        let state = |id: &str| voice.out.lock().unwrap().controller.request(id).map(|r| r.state);
        // 20 s, not 10: measured 2026-09-23, this failed once directly after
        // the on-device model had been running, when the synthesizer took
        // longer than ten seconds to start. What is being tested is that a
        // cleared sentence IS spoken and recorded, not how fast this Mac is.
        assert!(wait_for(20.0, || state(&id) == Some(SpeechState::Completed)), "completed: {:?}", state(&id));
        let started = voice.speech.lock().unwrap().start_latency_ms.first().copied();
        eprintln!("synthesizer start latency: {started:?} ms");
        assert!(voice.engine.lock().unwrap().recent_events(20).iter().any(|e| e.summary.contains(&format!("Speech {id} for reference (AUTHORIZATION_REQUIRED): COMPLETED."))));

        // Kill mid-sentence: the process is terminated and nothing more is said.
        voice.say_plain("This is a long sentence that the kill switch has to cut off before it can finish being spoken aloud.", "a1", Priority::ActionResult, now());
        let long = voice.out.lock().unwrap().controller.status().speaking.expect("sent").id;
        assert!(wait_for(5.0, || state(&long) == Some(SpeechState::Speaking)), "speaking: {:?}", state(&long));
        let t = now();
        let _ = engine.lock().unwrap().kill(Principal::Owner, "test", t);
        voice.kill(t);
        assert_eq!(state(&long), Some(SpeechState::Cancelled));
        assert!(!voice.speech.lock().unwrap().is_running(), "the speech process is gone");
        voice.say_plain("Anything.", "a2", Priority::CriticalSafety, now());
        voice.pump(now());
        assert!(voice.start().is_err());
        let spoken_after = voice.out.lock().unwrap().controller.status();
        assert!(spoken_after.speaking.is_none() && spoken_after.queued.is_empty(), "{spoken_after:?}");
    }

    /// React cannot reach a speaker except through core: the window's code has no
    /// Web Speech call, and no command accepts text to speak.
    #[test]
    fn the_window_has_no_path_to_a_speaker_that_bypasses_core() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src");
        let mut stack = vec![root];
        let mut checked = 0;
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() { stack.push(p); continue; }
                if !matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx")) { continue; }
                let code = std::fs::read_to_string(&p).unwrap();
                checked += 1;
                for banned in ["speechSynthesis", "SpeechSynthesisUtterance", "AudioContext", "new Audio("] {
                    assert!(!code.contains(banned), "{} uses {banned}", p.display());
                }
            }
        }
        assert!(checked > 5, "the window's source was found ({checked} files)");

        let me = include_str!("lib.rs");
        let handlers = me.split("generate_handler![").nth(1).unwrap().split(']').next().unwrap();
        let names: Vec<&str> = handlers.split(',').map(|n| n.trim()).filter(|n| !n.is_empty()).collect();
        assert!(names.contains(&"stop_speaking") && names.contains(&"set_voice_settings") && names.contains(&"get_speech"));
        for n in &names {
            assert!(!(n.contains("speak") || n.contains("say") || n.contains("utter") || n.contains("tts")) || *n == "stop_speaking",
                "{n} looks like a command that speaks");
        }
        // Settings from the window are clamped and cannot silence the gate.
        let s = VoiceSettings { speed: 50.0, volume: 7.0, ..VoiceSettings::default() }.clamped();
        assert!(s.speed <= VoiceSettings::MAX_SPEED && s.volume <= 1.0);
    }

    #[test]
    fn a_dangerous_request_is_refused_before_authorization_and_before_any_model() {
        // `ask` is where a request that is not a command goes to a model. The
        // safety boundary has to come first in it — before the authorization
        // gate (who asks does not matter), before the capability answer, and
        // before the model process is started or sent anything.
        let me = include_str!("lib.rs");
        let body = me.split("fn ask(question: String").nth(1).expect("ask exists")
            .split("\n}\n").next().unwrap();
        let at = |needle: &str| body.find(needle).unwrap_or_else(|| panic!("{needle} is not in ask"));
        let screen = at("lantern_core::safety::screen(q)");
        for later in ["gate(&state.engine, Operation::AskModelWithPersonalContext)", "intent::classify(q",
                      "answer_by_rule(", "start_goal(", "plan_with_model(", "start_mind(", "router::route(",
                      "clear_model_context", "model::ask("] {
            assert!(screen < at(later), "the safety boundary comes after {later}");
        }
        // The model process is reached only through the provider boundary: the
        // raw line-writer is private to mind.rs, so this is the one way in.
        assert!(!me.contains("\"cmd\": \"ask\""), "a prompt is sent to the model outside ModelProvider");
        // And a spoken request after a wake takes the same path as everything
        // else: the one front door, which understands it in context first.
        let spoken = me.split("fn handle_spoken_request(").nth(1).unwrap().split("\n}\n").next().unwrap();
        assert!(spoken.contains("receive_input("), "the hands-free path has its own route");
        let door = me.split("fn receive_input(").nth(1).unwrap().split("\n}\n").next().unwrap();
        assert!(door.contains("transaction::receive(") && door.contains("ask(text"));
    }

    /// The app's state for an opt-in live test: real engine, firewall, action book, KueAct
    /// and kue-voice (volume 0). Identity is the one stand-in: owner-matching face
    /// measurements fed by the harness instead of the camera.
    /// The camera's part, for a test: the owner sitting in view. Identity
    /// evidence goes stale on purpose — it is a measurement, not a memory — so
    /// a test that waits (for a model, say) feeds it again before the moment
    /// that needs the owner, which is what a real camera does continuously.
    fn feed_owner_frames(e: &mut Engine) {
        use lantern_core::sensor::*;
        for _ in 0..12 {
            let t = now();
            e.ingest(SensorMessage::Perception { ts: t, face_count: 1, frame_seq: 1, processed_fps: 4.0, faces: vec![FaceMeasurement {
                track_id: "T1".into(), frames_tracked: 10, track_age_seconds: 4.0, detection_confidence: 0.9,
                bounding_box: BBox { x: 0.3, y: 0.2, w: 0.3, h: 0.4 }, roll_deg: Some(0.0), yaw_deg: Some(2.0), pitch_deg: Some(3.0),
                capture_quality: Some(0.45), landmarks_available: true, geometry_distance: Some(0.07),
                feature_print_distance: Some(0.09), descriptor_status: "OK".into() }] }, t);
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    fn live_state() -> (AppState, Voice) {
        use lantern_core::sensor::*;
        let engine = Arc::new(Mutex::new(Engine::new(Config::default_config(), "live".into())));
        {
            let mut e = engine.lock().unwrap();
            e.set_sensing_process_up(true, now());
            e.ingest(SensorMessage::Status {
                camera: CameraStatus { state: "RUNNING".into(), permission: "AUTHORIZED".into(), ..Default::default() },
                sensing_active: true, computer_sampling_active: Some(true),
                enrollment: EnrollmentStats { sample_count: 5, geometry_self_p95: Some(0.1849), feature_print_self_p95: Some(0.1361), ..Default::default() },
                microphone_permission: None,
            }, now());
            feed_owner_frames(&mut e);
            assert_eq!(e.access_block(now()).level, AuthLevel::Level2, "harness identity");
        }
        let voice = test_voice(&engine);
        if voice.exe.is_some() { voice.start().unwrap(); }
        let state = AppState {
            engine: Arc::clone(&engine), firewall: Arc::clone(&voice.firewall), sensing: Arc::new(Mutex::new(Sensing::new())),
            store: Arc::new(Mutex::new(None)), sensing_exe: Arc::new(Mutex::new(None)), startup_note: Arc::new(Mutex::new(String::new())),
            mind: Arc::new(Mutex::new(Mind::new())), mind_exe: None, conversation: Arc::new(Mutex::new(Conversation::new())),
            actions: Arc::new(Mutex::new(transaction::ActionBook::new())), voice: voice.clone(),
            plan_wait: Arc::new(Mutex::new(None)),
            wake: Arc::new(Mutex::new(WakeSetting { enabled: false, phrase: "computer".into() })),
        };
        (state, voice)
    }

    /// Opt-in, and acts on this Mac: opens a folder named Tampa on YOUR Desktop in
    /// Finder and lists the resumes in it, through the same core transaction, real
    /// folder search, real KueAct executor and real kue-voice (volume 0) the app uses.
    /// The only stand-in is identity: the harness feeds owner-matching face
    /// measurements instead of the camera. Needs ~/Desktop/Tampa.
    ///   cargo test -p lantern -- --ignored the_tampa_request --nocapture
    #[test]
    #[ignore]
    fn the_tampa_request_runs_on_this_macs_real_folders_finder_and_voice() {
        if !home_dir().join("Desktop/Tampa").is_dir() { eprintln!("skipped: no ~/Desktop/Tampa"); return; }
        let (state, voice) = live_state();
        let started = std::time::Instant::now();
        let first = with_actions(None, &state, |rt| transaction::propose(rt,
            "Open the Tampa folder on Desktop and tell me what resumes are inside", "VOICE")).expect("a plan");
        eprintln!("planned and run in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
        let records = state.actions.lock().unwrap().records().to_vec();
        for r in &records {
            eprintln!("{} {} {:?} — verification: {:?} — reason: {:?} — {} match(es)",
                r.id, r.action.tag(), r.state, r.verification, r.reason, r.choices.len());
        }
        assert_eq!(first.action.tag(), "OPEN_DIRECTORY");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].state, lantern_core::actions::ActionState::Succeeded);
        assert!(records[0].verification.as_deref().unwrap().contains("Finder"));
        assert_eq!(records[1].state, lantern_core::actions::ActionState::Succeeded);
        let tasks = state.actions.lock().unwrap().task_views();
        eprintln!("steps: {:?}", tasks[0].steps.iter().map(|s| (s.kind, s.state)).collect::<Vec<_>>());

        // What KUE said, through the real synthesizer.
        if voice.exe.is_some() {
            let done = |v: &Voice| v.out.lock().unwrap().controller.state() == lantern_core::voice::speaker::SpeechState::Idle;
            let end = std::time::Instant::now() + Duration::from_secs(40);
            while !done(&voice) && std::time::Instant::now() < end { voice.pump(now()); std::thread::sleep(Duration::from_millis(100)); }
            let st = voice.out.lock().unwrap().controller.status();
            for r in st.recent.iter().rev() { eprintln!("speech {} for {} ({:?}): {:?} {:?}", r.id, r.topic, r.priority, r.state, r.stop); }
            use lantern_core::voice::speaker::{SpeechState, SpeechStop};
            // Progress that a result replaced before its turn is superseded, not said late.
            assert!(st.recent.iter().all(|r| r.state == SpeechState::Completed
                || (r.state == SpeechState::Cancelled && r.stop == Some(SpeechStop::Superseded))), "{st:?}");
            assert!(st.recent.iter().any(|r| r.topic == records[1].id && r.state == SpeechState::Completed), "the listing result was said");
            voice.speech.lock().unwrap().shutdown();
        }
    }

    /// S8, on this Mac: a goal is spoken, a plan is laid out and waits, the
    /// owner says "do it", and three declared tools run in ~/KUE — each
    /// authorized, executed and read back by the same transaction the app uses.
    /// It writes only inside a folder it creates under ~/KUE and removes it
    /// afterwards.
    ///
    /// Stand-ins: identity (the harness feeds owner-matching face
    /// measurements instead of the camera) and the proposer (no model is
    /// connected; the plan is handed in as a proposal would be). Everything
    /// else — the front door, the dialogue, validation against the declared
    /// tools, approval, execution, verification — is the real thing.
    ///   cargo test -p lantern -- --ignored s8_plan --nocapture
    #[test]
    #[ignore]
    fn s8_plan_runs_three_declared_tools_in_kue_on_this_mac() {
        use lantern_core::transaction::PlanProposed;
        let (state, voice) = live_state();
        let dir = home_dir().join("KUE").join(format!("s8-live-{}", std::process::id()));
        let file = dir.join("notes.txt");
        let json = format!(r#"{{"goal":"set up today's notes","steps":[
            {{"tool":"CREATE_DIRECTORY","input":{{"path":"{}"}}}},
            {{"tool":"CREATE_FILE","input":{{"path":"{}","text":"S8 live check"}},"after":[0]}},
            {{"tool":"READ_PERMITTED_FILE","input":{{"path":"{}"}},"after":[1]}}]}}"#,
            dir.to_string_lossy(), file.to_string_lossy(), file.to_string_lossy());

        // The owner asks; KUE has no plan of its own for this, so a proposal is
        // offered against that request.
        let received = with_actions(None, &state, |rt| transaction::receive(rt, lantern_core::pipeline::Transport::Voice, "Set up a folder for today's notes."));
        let rid = match received { transaction::Received::New { request_id, .. } => request_id, other => panic!("{other:?}") };
        let started = std::time::Instant::now();
        let offered = with_actions(None, &state, |rt| transaction::offer_plan(rt, &rid, &json, "live-harness", "VOICE", true));
        let PlanProposed::Waiting { plan_id, lines } = offered else { panic!("{offered:?}") };
        for l in &lines { eprintln!("KUE: {l}"); }
        assert!(!dir.exists(), "nothing ran before the owner said so");

        // "Do it" — and only then does anything happen.
        let confirmed = with_actions(None, &state, |rt| transaction::receive(rt, lantern_core::pipeline::Transport::Voice, "Do it."));
        eprintln!("approved and run in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
        assert!(matches!(confirmed, transaction::Received::Confirmed { .. }), "{confirmed:?}");

        let records = state.actions.lock().unwrap().records().to_vec();
        for r in &records {
            eprintln!("{} {} {:?} — verification: {:?} — reason: {:?}", r.id, r.action.tag(), r.state, r.verification, r.reason);
        }
        // What is on disk, not what KUE said about it.
        assert!(dir.is_dir(), "the folder exists");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "S8 live check");
        assert_eq!(records.len(), 3, "three steps, one record each");
        for r in &records {
            assert_eq!(r.state, lantern_core::actions::ActionState::Succeeded, "{}: {:?}", r.action.tag(), r.reason);
            assert!(r.verification.is_some(), "{} was read back", r.action.tag());
        }
        let held = state.actions.lock().unwrap().plan(&plan_id).cloned().expect("the plan");
        assert!(held.approved() && held.goal_id.is_some());
        let steps = state.actions.lock().unwrap().task_views();
        eprintln!("steps: {:?}", steps[0].steps.iter().map(|s| (s.kind, s.state)).collect::<Vec<_>>());
        for line in state.engine.lock().unwrap().recent_events(12).iter().rev() { eprintln!("event: {}", line.summary); }

        // Its own files, removed by the test — KUE never deletes anything.
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
        if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
    }

    /// S8b, on this Mac: the real on-device model is asked for a plan, and what
    /// it writes goes through the real parser and validator. Nothing runs —
    /// this stops at the plan being offered, or refused.
    ///
    /// Prints the raw JSON and the verdict, because what the model actually
    /// returns is the thing worth knowing.
    ///   cargo test -p lantern -- --ignored s8b_plan_from_the_real_model --nocapture
    #[test]
    #[ignore]
    fn s8b_plan_from_the_real_model() {
        use lantern_core::transaction::PlanProposed;
        let Some(exe) = mind::locate() else { eprintln!("skipped: mind not built"); return };
        let (state, voice) = live_state();
        start_mind(exe, &state.engine, &state.mind, &state.conversation, None, None, Some(Arc::clone(&state.plan_wait))).unwrap();
        if state.mind.lock().unwrap().availability.as_deref() != Some("AVAILABLE") {
            eprintln!("skipped: model unavailable: {:?}", state.mind.lock().unwrap().unavailable_reason);
            return;
        }
        let goal = std::env::var("KUE_GOAL").unwrap_or_else(|_| "Make a folder called Reports in my KUE folder and put a note in it.".into());
        let t = now();

        // S10: something the owner told KUE that bears on this goal. What is
        // tested here is KUE's part — that the sentence reaches the prompt and
        // that the plan records which memory was put in front of the model.
        // Whether the model FOLLOWS it is the model's business, and KUE
        // validates whatever comes back either way.
        let remembered = "Put the date in any note you write for me";
        {
            let mut e = state.engine.lock().unwrap();
            let id = e.memory_mut().id(t);
            let m = lantern_core::memory::Memory::told(&id, lantern_core::memory::MemoryClass::Preference,
                &lantern_core::memory::subject_of(lantern_core::memory::MemoryClass::Preference, remembered),
                remembered, lantern_core::privacy::DataKind::OwnerMessage, t, remembered);
            e.memory_mut().remember(m, false, t);
        }
        let (relevant, memory_refs): (Vec<String>, Vec<String>) = {
            let e = state.engine.lock().unwrap();
            e.memory().matching(&goal, t).into_iter().take(3).map(|m| (m.statement.clone(), m.id.clone())).unzip()
        };
        eprintln!("memories put in front of the model: {relevant:?}");
        assert!(relevant.iter().any(|s| s == remembered), "the preference did not come back from retrieval");
        let cleared = state.firewall.lock().unwrap().clear_plan_request_with(&goal, &relevant, t).expect("cleared");
        assert!(cleared.value().prompt.contains(remembered), "the prompt does not carry what the owner said");
        assert!(cleared.value().prompt.contains("WHAT THE OWNER HAS TOLD KUE BEFORE"), "it is not labelled as theirs");
        eprintln!("--- PROMPT ---\n{}\n{}\n--- END ---", cleared.value().instructions, cleared.value().prompt);

        let (tx, rx) = std::sync::mpsc::channel();
        let id = format!("{PLAN_ID_PREFIX}live");
        *state.plan_wait.lock().unwrap() = Some((id.clone(), tx));
        let started = std::time::Instant::now();
        lantern_core::model::ask(&mut *state.mind.lock().unwrap(), &lantern_core::model::ModelRequest::new(id, cleared)).unwrap();
        let answer = rx.recv_timeout(Duration::from_secs(180));
        eprintln!("model answered in {:.1}s", started.elapsed().as_secs_f64());
        let json = match answer {
            Ok(Ok(text)) => { eprintln!("--- MODEL WROTE ---\n{text}\n--- END ---"); text }
            other => { eprintln!("no plan: {other:?}"); state.mind.lock().unwrap().shutdown(); return; }
        };

        // Through the real front door, the real parser and the real validator.
        // The goal is phrased so no rule of KUE's acts on it by itself: what
        // runs, if anything, is the model's plan after the owner agrees to it.
        let received = with_actions(None, &state, |rt| transaction::receive(rt, lantern_core::pipeline::Transport::Typed, &goal));
        let rid = match received {
            transaction::Received::New { request_id, record: None } => request_id,
            other => { state.mind.lock().unwrap().shutdown(); panic!("a rule acted on the goal by itself: {other:?}") }
        };
        let outcome = with_actions(None, &state, |rt| transaction::offer_plan_informed_by(
            rt, &rid, &json, "on-device model", "TEXT", broker::locate_act().is_some(), &memory_refs));
        let plan_id = match &outcome {
            PlanProposed::Refused { said, problems } => {
                eprintln!("REFUSED before anything ran: {said}");
                for p in problems { eprintln!("  {p:?}"); }
                assert!(state.actions.lock().unwrap().records().is_empty(), "nothing ran");
                state.mind.lock().unwrap().shutdown();
                if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
                return;
            }
            PlanProposed::Waiting { plan_id, lines } => {
                eprintln!("PLAN {plan_id} — offered to the owner:");
                for l in lines { eprintln!("  KUE: {l}"); }
                plan_id.clone()
            }
        };
        let held = state.actions.lock().unwrap().plan(&plan_id).cloned().expect("the plan is held");
        // Attached by KUE after retrieval — never read out of what the model wrote.
        assert_eq!(held.memory_refs, memory_refs, "the plan does not record what was put in front of the model");
        eprintln!("  tools: {:?}", held.step_tools());
        for d in held.step_details() { eprintln!("  step: {d}"); }
        assert!(!held.approved(), "nothing is approved by proposing it");
        assert!(state.actions.lock().unwrap().records().is_empty(), "nothing ran before the owner said so");

        // Only KUE's own folder, and only steps that write or read inside it,
        // are carried out by an unattended test. Anything else is left for a
        // person to decide, and the plan is dropped.
        const IN_KUE: [&str; 3] = ["CREATE_DIRECTORY", "CREATE_FILE", "READ_PERMITTED_FILE"];
        if !held.step_tools().iter().all(|t| IN_KUE.contains(t)) {
            eprintln!("not run by this test: it asks for {:?}, which is a person's decision", held.step_tools());
            with_actions(None, &state, |rt| transaction::cancel_plan(rt, &plan_id));
            state.mind.lock().unwrap().shutdown();
            if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
            return;
        }

        // A name this test's own earlier run left behind is cleared, and only
        // when clearing it can destroy nothing: an EMPTY folder. Anything with
        // something in it, or any file, belongs to the owner — the run stops
        // rather than touching it, and says so.
        let kue_root = home_dir().join("KUE");
        let resolve = |path: &str| if std::path::Path::new(path).is_absolute() { PathBuf::from(path) } else { kue_root.join(path) };
        let named: Vec<PathBuf> = held.step_paths().iter().map(|p| resolve(p)).collect();
        for p in &named {
            if p.is_dir() { let _ = std::fs::remove_dir(p); }
            if p.exists() {
                eprintln!("not run: {} is already there and is the owner's, not this test's", p.display());
                with_actions(None, &state, |rt| transaction::cancel_plan(rt, &plan_id));
                state.mind.lock().unwrap().shutdown();
                if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
                return;
            }
        }

        // A correction, against a plan the MODEL wrote — the one S8b path that
        // had never met real model output. The changed plan is written back
        // out as proposal text, re-read and re-checked from the beginning, and
        // held under a new identity: the plan the owner was looking at cannot
        // be started any more, and the new one needs its own yes.
        let (plan_id, held) = if held.step_tools().len() >= 2 {
            feed_owner_frames(&mut state.engine.lock().unwrap());
            let changed = with_actions(None, &state, |rt| transaction::receive(rt, lantern_core::pipeline::Transport::Typed, "Don't do step two."));
            match changed {
                transaction::Received::Revised { said, .. } => {
                    eprintln!("CHANGED: {said}");
                    let now_held = state.actions.lock().unwrap().plans().iter()
                        .find(|h| h.plan.state != lantern_core::plan::PlanState::Cancelled && !h.approved())
                        .cloned().expect("a plan is held after the change");
                    let old = state.actions.lock().unwrap().plan(&plan_id).cloned().expect("the plan it came from");
                    assert_eq!(old.plan.state, lantern_core::plan::PlanState::Cancelled, "the plan it came from is cancelled");
                    assert_ne!(now_held.plan.plan_id, plan_id, "a changed plan is a different plan");
                    assert_eq!(now_held.version, old.version + 1);
                    assert_eq!(now_held.revision_of.as_deref(), Some(plan_id.as_str()));
                    assert_eq!(now_held.step_tools().len(), old.step_tools().len() - 1, "one step fewer");
                    assert!(!now_held.approved());
                    eprintln!("PLAN {} (version {}) — {:?}", now_held.plan.plan_id, now_held.version, now_held.step_tools());
                    for d in now_held.step_details() { eprintln!("  step: {d}"); }
                    // The old plan's id cannot start anything, by its own state.
                    let refused = with_actions(None, &state, |rt| transaction::approve_plan(rt, &plan_id));
                    eprintln!("old plan approved? {refused:?}");
                    assert!(refused.is_err(), "the plan the owner changed away from must not start");
                    (now_held.plan.plan_id.clone(), now_held)
                }
                other => { eprintln!("the correction was not applied: {other:?}"); (plan_id, held) }
            }
        } else { (plan_id, held) };
        assert!(state.actions.lock().unwrap().records().is_empty(), "a correction runs nothing");

        // The owner is still sitting there when they say yes. The model took
        // tens of seconds, and identity evidence is a measurement of NOW: the
        // harness feeds the camera's part again rather than KUE trusting an
        // old reading, which it is right to refuse.
        feed_owner_frames(&mut state.engine.lock().unwrap());
        assert_eq!(state.engine.lock().unwrap().access_block(now()).level, AuthLevel::Level2);

        // "Do it" — the owner's word, through the same front door, binding to
        // exactly the plan that was shown.
        let started = std::time::Instant::now();
        let confirmed = with_actions(None, &state, |rt| transaction::receive(rt, lantern_core::pipeline::Transport::Typed, "Do it."));
        eprintln!("approved and run in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
        assert!(matches!(confirmed, transaction::Received::Confirmed { .. }), "{confirmed:?}");

        let records = state.actions.lock().unwrap().records().to_vec();
        for r in &records {
            eprintln!("{} {} {:?} — verification: {:?} — reason: {:?}", r.id, r.action.tag(), r.state, r.verification, r.reason);
        }
        assert_eq!(records.len(), held.step_tools().len(), "one record per approved step");
        for r in &records {
            assert_eq!(r.state, lantern_core::actions::ActionState::Succeeded, "{}: {:?}", r.action.tag(), r.reason);
            assert!(r.verification.as_ref().is_some_and(|v| !v.trim().is_empty()), "{} was read back", r.action.tag());
        }
        // What is on disk, not what the model or KUE said about it. A plan
        // names a place relative to KUE's own folder — the model is never told
        // where that is — so the check resolves it the way the runtime does.
        let kue = home_dir().join("KUE");
        let here = |path: &str| if std::path::Path::new(path).is_absolute() { PathBuf::from(path) } else { kue.join(path) };
        let mut made: Vec<PathBuf> = Vec::new();
        for r in &records {
            match &r.action {
                lantern_core::actions::ActionKind::CreateDirectory { path } => {
                    let p = here(path);
                    assert!(p.is_dir(), "the folder the plan named is not there: {}", p.display());
                    made.push(p);
                }
                lantern_core::actions::ActionKind::CreateFile { path, text } => {
                    let p = here(path);
                    assert_eq!(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())), text);
                    made.push(p);
                }
                _ => {}
            }
        }
        eprintln!("on disk: {:?}", made);
        let after = state.actions.lock().unwrap().plan(&plan_id).cloned().expect("the plan");
        assert!(after.approved() && after.goal_id.is_some(), "the plan records the owner's approval and its goal");

        // S9: what KUE did, kept — because it read the world back. And what
        // the MODEL wrote is not kept, in any form: the plan's words were its,
        // and none of them became something KUE remembers as true.
        {
            use lantern_core::memory::{MemoryClass, MemoryState};
            let e = state.engine.lock().unwrap();
            let kept: Vec<_> = e.memory().all().iter()
                .map(|m| (m.class, m.state, m.source, m.statement.clone())).collect();
            for (class, state, source, statement) in &kept { eprintln!("memory: {class:?} {state:?} {source:?} — {statement}"); }
            let work: Vec<_> = kept.iter().filter(|(c, ..)| *c == MemoryClass::Work).collect();
            assert_eq!(work.len(), 1, "the plan KUE carried out and checked is kept once");
            assert_eq!(work[0].1, MemoryState::Verified);
            assert_eq!(work[0].2, lantern_core::facts::FactSource::Action, "kept because KUE acted, not because a model spoke");
            assert!(kept.iter().any(|(c, s, ..)| *c == MemoryClass::Decision && *s == MemoryState::Confirmed),
                    "the owner's approval is kept");
            assert!(!kept.iter().any(|(.., source, _)| *source == lantern_core::facts::FactSource::Model),
                    "a model's words became memory: {kept:?}");
            // And nothing KUE kept names a file: policy allows the owner to be
            // shown those, and allows nobody to store them.
            for (.., statement) in &kept {
                assert!(!statement.contains('/') && !statement.contains(".txt"), "a memory names a file: {statement}");
            }
        }
        for line in state.engine.lock().unwrap().recent_events(14).iter().rev() { eprintln!("event: {}", line.summary); }

        // Only what this run created, and only inside KUE's own folder.
        made.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        for p in made {
            assert!(p.starts_with(&kue), "refusing to remove anything outside ~/KUE: {}", p.display());
            let _ = if p.is_dir() { std::fs::remove_dir(&p) } else { std::fs::remove_file(&p) };
        }
        state.mind.lock().unwrap().shutdown();
        if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
    }

    /// S9, on this Mac, across two processes: what the owner asks KUE to keep
    /// is written to a real SQLite file through the real firewall and the real
    /// pump, and a SECOND PROCESS — this same test binary, run again — opens
    /// that file, finds the memory, answers a question from it, and forgets
    /// it. A memory that did not outlive the process that made it would not be
    /// memory.
    ///
    /// Phase two runs as its own process so the restart is real, not a fresh
    /// object in the same one.
    #[test]
    #[ignore]
    fn s9_memory_on_this_mac_survives_the_process_that_made_it() {
        use lantern_core::pump::MemoryPump;
        let dir = std::env::temp_dir().join("kue-s9-live");
        let _ = std::fs::create_dir_all(&dir);
        let db = dir.join(format!("memory-{}.sqlite3", std::process::id()));
        let _ = std::fs::remove_file(&db);

        // PHASE ONE — the owner tells KUE something, and the pump writes it.
        let (state, voice) = live_state();
        let store = Store::open(&db, 500, 20).expect("a real store on this Mac");
        let asked = with_actions(None, &state, |rt|
            transaction::receive(rt, lantern_core::pipeline::Transport::Typed, "Remember that I prefer PDF reports."));
        let said = match asked { transaction::Received::Answered { said } => said, other => panic!("{other:?}") };
        eprintln!("KUE: {said}");
        assert_eq!(said, "I'll remember that you prefer PDF reports.");
        {
            let mut e = state.engine.lock().unwrap();
            let mut fw = state.firewall.lock().unwrap();
            MemoryPump::new(600.0).tick(&mut e, &mut fw, &store, now()).expect("written");
        }
        drop(store);
        eprintln!("wrote {}", db.display());

        // PHASE TWO — a different process, the same file.
        let out = std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .args(["--exact", "--ignored", "--nocapture", "tests::s9_second_process_reads_what_the_first_one_kept"])
            .env("KUE_S9_DB", &db)
            .output().expect("the second process runs");
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        eprintln!("--- second process ---
{stdout}{stderr}--- end ---");
        assert!(out.status.success(), "the second process could not read what the first one kept");

        let _ = std::fs::remove_file(&db);
        if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
    }

    /// The second half of the test above: run by it, with KUE_S9_DB set.
    #[test]
    #[ignore]
    fn s9_second_process_reads_what_the_first_one_kept() {
        let Ok(db) = std::env::var("KUE_S9_DB") else { eprintln!("skipped: not the second process"); return };
        use lantern_core::pump::MemoryPump;
        let store = Store::open(std::path::Path::new(&db), 500, 20).expect("opens the file the first process wrote");
        let (rows, unreadable) = store.memories().expect("reads it back");
        eprintln!("second process found {} memories ({unreadable} unreadable)", rows.len());
        assert_eq!(unreadable, 0);
        assert!(!rows.is_empty(), "nothing survived the first process");

        let (state, voice) = live_state();
        state.engine.lock().unwrap().memory_mut().load(rows);

        // It answers from it, in the owner's own words, with no model at all.
        let ask = |q: &str| match with_actions(None, &state, |rt|
            transaction::receive(rt, lantern_core::pipeline::Transport::Typed, q)) {
            transaction::Received::Answered { said } => said,
            other => panic!("{q:?} → {other:?}"),
        };
        let recalled = ask("What do you remember about my report preferences?");
        eprintln!("KUE: {recalled}");
        assert_eq!(recalled, "You prefer PDF reports.");
        let why = ask("Why do you remember that?");
        eprintln!("KUE: {why}");
        assert!(why.contains("you said: “I prefer PDF reports”"), "{why}");

        // And forgetting reaches the file: the row goes.
        let forgotten = ask("Forget that I prefer PDF reports.");
        eprintln!("KUE: {forgotten}");
        assert!(forgotten.starts_with("Forgotten:"), "{forgotten}");
        {
            let mut e = state.engine.lock().unwrap();
            let mut fw = state.firewall.lock().unwrap();
            MemoryPump::new(600.0).tick(&mut e, &mut fw, &store, now()).expect("the removal is written");
        }
        let (rows, _) = store.memories().expect("read back");
        assert!(rows.is_empty(), "the words are gone from the file, not blanked in it");
        eprintln!("the file holds {} memories after forgetting", rows.len());
        if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
    }

    /// S10 on this Mac: a preference the owner states once changes the plan KUE
    /// proposes for a LATER request, over real files, through the real storage
    /// walk, the real KueAct executor and the real Trash — and changes nothing
    /// about approval, authorization or verification.
    ///
    /// The home is made of KUE's own files, so nothing here can reach anything
    /// of the owner's. Touch ID is the one stand-in; it needs a finger.
    #[test]
    #[ignore]
    fn s10_memory_changes_the_plan_on_this_mac_and_nothing_else() {
        use lantern_core::memory::{MemoryClass, MemoryState};
        use lantern_core::pipeline::Transport;
        use transaction::Received;
        let (state, voice) = live_state();
        let home = home_dir().join("KUE/live-memory");
        let _ = std::fs::remove_dir_all(&home);
        let file = |rel: &str, size: usize, days: u64| {
            let p = home.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![b'k'; size]).unwrap();
            let old = std::time::SystemTime::now() - Duration::from_secs(days * 86_400);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(old).unwrap();
            p
        };
        let installer = file("Downloads/kue-live-installer.dmg", 3_000_000, 200);
        let statement = file("Downloads/kue-live-old-statement.pdf", 2_000_000, 300);
        let copy_a = file("Documents/kue-live-deck.key", 1_500_000, 100);
        let copy_b = file("Desktop/kue-live-deck.key", 1_500_000, 40);

        let targets = transaction::Targets::for_home(&home);
        let clock = || now();
        let authenticate = |_: OsAuthKind, _: Operation| "SUCCESS".to_string();
        let execute_os = |req: &transaction::ExecutorRequest| broker::run_os_action(req);
        let measure = |p: &std::path::Path| measure_volume(p);
        let (changed, narrate) = (|| {}, |_: &ActionRecord| {});
        let say = |_: &str, _: &[lantern_core::privacy::DataKind]| {};
        let rt = transaction::Runtime {
            engine: &state.engine, firewall: &state.firewall, book: &state.actions, targets: &targets,
            now: &clock, authenticate: &authenticate, execute_os: &execute_os, changed: &changed, narrate: &narrate,
            measure_volume: &measure, say: &say,
        };
        let hear = |said: &str| transaction::receive(&rt, Transport::Typed, said);
        let last = || state.engine.lock().unwrap().requests().dialogue.turns().last()
            .map(|t| t.said.clone()).unwrap_or_default();

        // TEST 1 — the owner says it once. And something irrelevant, which
        // must stay out of the clean-up entirely (TEST 8).
        let Received::Answered { said } = hear("Remember that I don't want installers included when cleaning my storage.") else { panic!() };
        eprintln!("KUE: {said}");
        let Received::Answered { .. } = hear("Remember that I prefer PDF reports.") else { panic!() };
        assert_eq!(state.engine.lock().unwrap().memory().current(now()).len(), 2);

        // TEST 2 — a later request, with no mention of installers.
        let Received::New { request_id: rid, .. } = hear("Clean up my storage.") else { panic!() };
        let plan_said = last();
        eprintln!("KUE: {plan_said}");
        assert!(plan_said.contains("I left out the installer, because you told me:"), "{plan_said}");
        assert!(!plan_said.to_lowercase().contains("pdf"), "an unrelated preference reached the clean-up: {plan_said}");
        let refs = state.actions.lock().unwrap().goals().last().map(|g| g.memory_refs.clone()).unwrap_or_default();
        assert_eq!(refs.len(), 1, "one memory shaped it, and it is recorded");

        // "Why?" — from what they said, not from a guess.
        let Received::Answered { said: why } = hear("Why?") else { panic!() };
        eprintln!("KUE: {why}");
        assert!(why.contains("Because you told me:") && why.contains("installers"), "{why}");

        // TEST 3, 4, 5 — approval, execution and verification are untouched.
        // The storage READ has run — that is how the plan exists. Nothing has
        // been moved, which is the part that waits for the owner.
        assert!(!state.actions.lock().unwrap().records().iter()
                    .any(|r| matches!(r.action, lantern_core::actions::ActionKind::MoveToTrash { .. })),
                "a move ran before the owner said so");
        assert!(statement.exists() && copy_a.exists() && copy_b.exists(), "something moved before the owner said so");
        let Received::Confirmed { record, .. } = hear("Do it.") else { panic!("not confirmed: {}", last()) };
        let moved = record.expect("the move ran");
        eprintln!("moved: {:?} — verification: {:?}", moved.state, moved.verification);
        assert_eq!(moved.state, lantern_core::actions::ActionState::Succeeded, "{:?}", moved.reason);
        assert!(moved.verification.as_deref().unwrap_or("").contains("/.Trash/"), "{:?}", moved.verification);
        assert_eq!(state.engine.lock().unwrap().requests().get(&rid).map(|r| r.state),
                   Some(lantern_core::pipeline::RequestState::Completed));

        // What is on disk: the installer the preference protected is still there.
        assert!(installer.exists(), "a file the owner's preference protected was moved");
        assert!(!statement.exists(), "the old download was not moved");
        assert!(copy_a.exists() != copy_b.exists(), "exactly one copy of the deck moved");

        // TEST 5 (continued) — and the outcome became memory, because it verified.
        {
            let e = state.engine.lock().unwrap();
            let kept: Vec<_> = e.memory().all().iter().map(|m| (m.class, m.state, m.statement.clone())).collect();
            for (c, s, t) in &kept { eprintln!("memory: {c:?} {s:?} — {t}"); }
            assert!(kept.iter().any(|(c, s, _)| *c == MemoryClass::Work && *s == MemoryState::Verified),
                    "what KUE did and checked was not kept: {kept:?}");
            assert!(kept.iter().all(|(.., t)| !t.contains("kue-live-")), "a memory names a file: {kept:?}");
        }

        // TEST 6 — the owner changes their mind, in plain words.
        let Received::Answered { said } = hear("Actually, include installers in storage cleanup from now on.") else { panic!() };
        eprintln!("KUE: {said}");
        assert!(said.contains("That replaces what you told me before."), "{said}");

        // TEST 7 — the next clean-up follows the new preference.
        let _ = hear("Clean up my storage.");
        let plan_said = last();
        eprintln!("KUE: {plan_said}");
        assert!(!plan_said.contains("I left out the installer"), "the old preference still applied: {plan_said}");
        let _ = hear("Cancel that.");
        assert!(installer.exists(), "nothing more was moved");

        // Its own files put back and removed; KUE never deletes anything.
        if let Some(undo) = transaction::propose_restore(&rt, "TEXT") {
            let _ = transaction::confirm(&rt, &undo.id, None);
        }
        let _ = std::fs::remove_dir_all(&home);
        if voice.exe.is_some() { voice.speech.lock().unwrap().shutdown(); }
    }

    /// The wake boundary, over audio made on this Mac: `say` speaks a line, the
    /// real sensing binary transcribes it and applies the real matcher, and the
    /// only thing that comes back is a decision.
    ///
    /// Not opt-in: it needs no microphone, no camera and no permission, and it
    /// is the test that would catch the matcher drifting away from what the
    /// recogniser actually returns.
    #[test]
    fn the_wake_boundary_hears_its_name_in_speech_made_on_this_mac() {
        // `--wake-stream` is the LIVE listener's path with a file for a
        // microphone: the same gate, recogniser options and decision, audio at
        // microphone pace, and no end of input. An earlier version of this test
        // used the finished-file path, which knows where the input ends; it
        // passed while the live listener, measured this way, woke for none of
        // fifteen invocations. And it used one voice, which hid a second
        // defect that four other voices showed.
        let Some(sense) = sensing_exe() else { eprintln!("skipped: sensing layer not built"); return; };
        let gate = std::process::Command::new(&sense).arg("--wake-gate-check").output().expect("the sensing binary runs");
        let gate: serde_json::Value = serde_json::from_slice(&gate.stdout).unwrap_or(serde_json::json!({}));
        assert_eq!(gate["ok"], serde_json::json!(true), "the wake gate: {gate}");

        let dir = std::env::temp_dir().join(format!("kue-wake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = |voice: &str, text: &str, name: &str| -> Option<PathBuf> {
            let p = dir.join(format!("{voice}-{name}.aiff"));
            let ok = std::process::Command::new("/usr/bin/say")
                .args(["-v", voice, "-o", &p.to_string_lossy(), text]).status().ok()?.success();
            ok.then_some(p)
        };
        let decide = |path: &PathBuf| -> serde_json::Value {
            let out = std::process::Command::new(&sense)
                .args(["--wake-stream", &path.to_string_lossy(), "--wake-phrase", "computer"])
                .output().expect("the sensing binary runs");
            serde_json::from_slice(&out.stdout).unwrap_or(serde_json::json!({}))
        };

        for voice in ["Samantha", "Daniel"] {
            let Some(spoken) = clip(voice, "Computer, check my storage.", "pos") else {
                eprintln!("skipped: `say` is unavailable"); return;
            };
            let heard = decide(&spoken);
            eprintln!("{voice} spoken → woke {} rest {:?} after {}", heard["woke"], heard["rest"], heard["wokeAfterSpeechEnded"]);
            assert_eq!(heard["woke"], serde_json::json!(true), "{voice}: the invocation was not heard: {heard}");
            // What followed it comes back as the request, so the owner says it once.
            assert_eq!(heard["rest"], serde_json::json!("check my storage"), "{voice}: {heard}");

            // A sentence that merely contains the word does not wake it: the
            // invocation has to come first.
            if let Some(about) = clip(voice, "My computer is slow today.", "neg") {
                let quiet = decide(&about);
                assert_eq!(quiet["woke"], serde_json::json!(false), "{voice}: ordinary speech woke KUE: {quiet}");
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Opt-in, and reads this Mac: takes stock of storage through the same core
    /// transaction the app uses, then prints what the window would render — the
    /// real volume, the real folders, the real findings. Reads names, sizes and
    /// dates only; opens no file and changes nothing. The only stand-in is
    /// identity: the harness feeds owner-matching face measurements.
    ///   cargo test -p lantern -- --ignored storage_live --nocapture
    #[test]
    #[ignore]
    fn storage_live_measures_this_mac_and_the_window_gets_only_what_it_may_show() {
        let (state, voice) = live_state();
        let started = std::time::Instant::now();
        let rec = with_actions(None, &state, |rt| transaction::propose(rt, "my storage is getting full", "VOICE"))
            .expect("a storage request is a plan");
        eprintln!("{} {:?} in {:.0} ms", rec.action.tag(), rec.state, started.elapsed().as_secs_f64() * 1000.0);
        eprintln!("verification: {:?}", rec.verification);
        eprintln!("on screen:\n{}", rec.output.as_deref().unwrap_or("—"));
        eprintln!("aloud: {}", rec.sentences.as_ref().map(|s| s.aloud.clone()).unwrap_or_default());

        assert_eq!(rec.action.tag(), "INSPECT_STORAGE");
        assert_eq!(rec.state, lantern_core::actions::ActionState::Succeeded, "{:?}", rec.reason);

        let view = with_actions(None, &state, transaction::storage);
        let report = view.report.expect("the window is handed the report");
        let said = view.said.expect("with KUE's own sentences");
        eprintln!("headline: {}", said.headline);
        for a in &said.areas { eprintln!("  {a}"); }
        for l in &said.limits { eprintln!("  ! {l}"); }
        eprintln!("{}", said.finding);
        for c in &report.counts { eprintln!("  {} — {}", c.heading, c.said); }
        for c in report.candidates.iter().take(3) {
            eprintln!("  · {} ({}) in {} — {}", c.name, c.size_said, c.where_said, c.evidence);
        }

        // The volume is this Mac's, not a number from anywhere else.
        let v = report.summary.volume.expect("statfs answered");
        assert!(v.capacity > 0 && v.available <= v.capacity && v.percent_used() <= 100);
        assert!(said.headline.contains(&lantern_core::storage::size_words(v.capacity)));
        // Every finding is inside a folder the owner allowed.
        let roots = lantern_core::actions::DocumentRoots::default_for_home(&home_dir());
        for c in &report.candidates {
            assert!(roots.0.iter().any(|r| std::path::Path::new(&c.path).starts_with(r)), "outside the allowed folders: {}", c.path);
            assert!(!c.caution.is_empty(), "a finding with no downside stated: {}", c.path);
            assert_eq!(c.recommended, lantern_core::storage::Recommended::Review, "this build only reviews");
        }
        // Nothing KUE cannot act on is offered as something it can. The limit is
        // QUOTED from the capability registry rather than written in the window,
        // so this asserts both: that it is the registry's own sentence, and that
        // the sentence still says deletion is not a thing KUE does.
        //
        // (This assertion had rotted: it expected wording removed weeks earlier,
        // and nothing noticed because `#[ignore]` tests are run by nothing. The
        // structural half below cannot rot the same way.)
        let row = lantern_core::capabilities::find("permanent_deletion").expect("the registry row exists");
        assert_eq!(said.cannot, row.ui_description);
        assert_eq!(row.status, lantern_core::context::CapabilityStatus::NotImplemented,
            "permanent deletion is deliberately absent; if that changes, this flow changes with it");
        let limit = said.cannot.to_lowercase();
        assert!(limit.contains("never deletes") && limit.contains("trash"),
            "the sentence must still state the limit: {}", said.cannot);
        // And what it says out loud names no file.
        let aloud = rec.sentences.as_ref().unwrap().aloud.clone();
        for c in report.candidates.iter().take(20) {
            assert!(!aloud.contains(&c.name), "a file name was spoken: {aloud}");
        }
        voice.speech.lock().unwrap().shutdown();
    }

    /// Opt-in, and MOVES FILES on this Mac — its own, which it creates first:
    /// two installers in ~/Desktop/kue-live-check, dated old enough for KUE to
    /// find them. It then runs the whole path with the real core, the real
    /// KueAct and the real Trash: take stock → choose → move, one at a time,
    /// each checked → put them back → and confirm they are back.
    ///
    /// Stand-ins, named rather than hidden: identity (harness face measurements)
    /// and the Touch ID answer (a stub says the owner authenticated, so the test
    /// does not raise a prompt on a Mac nobody is sitting at). Everything else —
    /// the report, the checks, the move, the verification — is the real thing.
    ///   cargo test -p lantern -- --ignored trash_live --nocapture
    #[test]
    #[ignore]
    fn conversation_live_clean_up_then_a_correction_then_do_it_on_the_real_trash() {
        // The owner's exchange, live: the real storage walk, the real KueAct
        // executor and the real Trash — over a home made of KUE's OWN files, so
        // "do it" can only ever reach what this test created. Touch ID is the
        // one stand-in (it needs a finger), exactly as in the test above.
        use lantern_core::pipeline::{RequestState, Transport};
        use transaction::Received;
        let (state, voice) = live_state();
        let home = home_dir().join("KUE/live-conversation");
        let _ = std::fs::remove_dir_all(&home);
        let file = |rel: &str, size: usize, days: u64| {
            let p = home.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![b'k'; size]).unwrap();
            let old = std::time::SystemTime::now() - Duration::from_secs(days * 86_400);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(old).unwrap();
            p
        };
        let installer = file("Downloads/kue-live-installer.dmg", 3_000_000, 200);
        let statement = file("Downloads/kue-live-old-statement.pdf", 2_000_000, 300);
        let copy_a = file("Documents/kue-live-deck.key", 1_500_000, 100);
        let copy_b = file("Desktop/kue-live-deck.key", 1_500_000, 40);

        let targets = transaction::Targets::for_home(&home);
        let clock = || now();
        let authenticate = |_: OsAuthKind, _: Operation| "SUCCESS".to_string();
        let execute_os = |req: &transaction::ExecutorRequest| broker::run_os_action(req);
        let measure = |p: &std::path::Path| measure_volume(p);
        let (changed, narrate) = (|| {}, |_: &ActionRecord| {});
        let say = |_: &str, _: &[lantern_core::privacy::DataKind]| {};
        let rt = transaction::Runtime {
            engine: &state.engine, firewall: &state.firewall, book: &state.actions, targets: &targets,
            now: &clock, authenticate: &authenticate, execute_os: &execute_os, changed: &changed, narrate: &narrate,
            measure_volume: &measure, say: &say,
        };
        let turns = || state.engine.lock().unwrap().requests().dialogue.turns().iter()
            .map(|t| format!("{:?}: {}", t.kind, t.said)).collect::<Vec<_>>();

        let hear = |said: &str| transaction::receive(&rt, Transport::Voice, said);
        let Received::New { request_id: rid, .. } = hear("Clean up my storage.") else { panic!("not a new request") };
        // Two corrections, one after the other, each binding.
        let Received::Revised { said, .. } = hear("Leave the installer.") else { panic!("not a revision: {:?}", turns()) };
        assert_eq!(said, "Okay. I'll exclude the installer.");
        let Received::Revised { said, .. } = hear("Actually leave the old download too.") else { panic!("{:?}", turns()) };
        assert_eq!(said, "Okay. I'll exclude both.");

        // S9: a correction to THIS clean-up is kept as a note about it, and
        // never as how the owner always wants storage cleaned. The difference
        // matters on real files: one expires, the other would not.
        {
            use lantern_core::memory::{MemoryClass, MemoryState};
            let e = state.engine.lock().unwrap();
            let kept: Vec<_> = e.memory().all().iter().map(|m| (m.class, m.state, m.statement.clone(), m.valid_for)).collect();
            for (c, st, said, for_) in &kept { eprintln!("memory: {c:?} {st:?} (for {for_:?}) — {said}"); }
            assert!(kept.iter().all(|(c, ..)| *c == MemoryClass::TaskNote),
                    "a correction became something more than a note about this task: {kept:?}");
            assert!(kept.iter().all(|(_, s, _, for_)| *s == MemoryState::Confirmed && for_.is_some()),
                    "a note about one task must expire: {kept:?}");
        }
        assert_eq!(turns().last().unwrap(), "KueSuggestion: That leaves one probable duplicate.");
        let Received::Answered { said } = hear("What's left?") else { panic!("{:?}", turns()) };
        assert_eq!(said, "One probable duplicate left, waiting for you.");
        let Received::Confirmed { record, .. } = hear("Okay, do it.") else { panic!("not a confirmation: {:?}", turns()) };
        let moved = record.expect("the move ran");
        for t in turns() { eprintln!("  {t}"); }
        assert_eq!(moved.state, lantern_core::actions::ActionState::Succeeded, "{:?}", moved.reason);

        // What the owner said to leave alone is still there; the one duplicate
        // is in the real Trash, per the executor's own read-back.
        assert!(installer.exists(), "the installer was moved after the owner said not to");
        assert!(statement.exists(), "the old download was moved after the owner said not to");
        assert!(copy_a.exists() != copy_b.exists(), "exactly one copy of the deck moved");
        assert!(moved.verification.as_deref().unwrap_or("").contains("/.Trash/"), "{:?}", moved.verification);
        let r = state.engine.lock().unwrap().requests().get(&rid).cloned().unwrap();
        assert_eq!(r.state, RequestState::Completed);
        assert!(r.ids.verification_id.is_some());
        let Received::Answered { said } = hear("What did you just do?") else { panic!() };
        assert!(said.starts_with("Done."), "{said}");

        // And back, exactly where it came from — asked for in words, confirmed in words.
        let Received::Answered { said } = hear("Can you undo that?") else { panic!() };
        assert!(said.starts_with("Yes — I can put back the one file I moved"), "{said}");
        let Received::New { record: Some(undo), .. } = hear("Put them back.") else { panic!("{:?}", turns()) };
        let back = if undo.state.is_waiting() {
            let Received::Confirmed { record, .. } = hear("Yes.") else { panic!("{:?}", turns()) };
            record.expect("confirm")
        } else { undo };
        assert_eq!(back.state, lantern_core::actions::ActionState::Succeeded, "{:?}", back.reason);
        assert!(copy_a.exists() && copy_b.exists(), "the copy did not come back");

        std::fs::remove_dir_all(&home).ok();
        voice.speech.lock().unwrap().shutdown();
    }

    #[test]
    #[ignore]
    fn trash_live_moves_kues_own_files_to_the_real_trash_and_puts_them_back() {
        use lantern_core::actions::ActionState;
        let (state, voice) = live_state();
        let folder = home_dir().join("Desktop/kue-live-check");
        std::fs::create_dir_all(&folder).expect("make the test folder");
        let mut paths = Vec::new();
        // Big enough to be among the largest findings on a real Mac — the
        // report shows the 120 biggest, so a 3 MB test file is honestly ranked
        // out of it (which is how this test found that out).
        for (name, size) in [("kue-live-check-one.dmg", 80_000_000usize), ("kue-live-check-two.dmg", 60_000_000)] {
            let p = folder.join(name);
            std::fs::write(&p, vec![b'k'; size]).expect("write the test file");
            let old = std::time::SystemTime::now() - Duration::from_secs(200 * 86_400);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(old).unwrap();
            paths.push(p.to_string_lossy().to_string());
        }

        // KUE finds them itself. Nothing is moved that a report did not offer.
        let targets = transaction::Targets::for_home(&home_dir());
        let clock = || now();
        let authenticate = |_: OsAuthKind, _: Operation| "SUCCESS".to_string();
        let execute_os = |req: &transaction::ExecutorRequest| broker::run_os_action(req);
        let measure = |p: &std::path::Path| measure_volume(p);
        let changed = || {};
        let narrate = |_: &ActionRecord| {};
        let say = |_: &str, _: &[lantern_core::privacy::DataKind]| {};
        let rt = transaction::Runtime {
            engine: &state.engine, firewall: &state.firewall, book: &state.actions, targets: &targets,
            now: &clock, authenticate: &authenticate, execute_os: &execute_os, changed: &changed, narrate: &narrate,
            measure_volume: &measure, say: &say,
        };

        let looked = transaction::propose(&rt, "my storage is getting full", "TEXT").expect("a storage request");
        assert_eq!(looked.state, ActionState::Succeeded, "{:?}", looked.reason);
        let report = state.actions.lock().unwrap().last_storage().cloned().unwrap();
        for p in &paths {
            assert!(report.candidates.iter().any(|c| &c.path == p), "KUE did not offer {p}");
        }

        // Chosen, checked, moved — one file at a time.
        let asked = transaction::propose_trash(&rt, paths.clone(), "TEXT");
        assert_eq!(asked.state, ActionState::RequiresStrongAuth, "{:?}", asked.reason);
        let moved = transaction::confirm(&rt, &asked.id, None).expect("confirm");
        eprintln!("{:?} — {:?}", moved.state, moved.verification);
        eprintln!("said: {}", moved.sentences.as_ref().unwrap().on_screen);
        assert_eq!(moved.state, ActionState::Succeeded, "{:?}", moved.reason);
        for p in &paths {
            assert!(!std::path::Path::new(p).exists(), "still where it was: {p}");
        }
        assert!(moved.verification.as_deref().unwrap().contains("/.Trash/"), "{:?}", moved.verification);
        assert!(!moved.sentences.as_ref().unwrap().aloud.contains("freed"), "no space was claimed back");

        // And back again, exactly where they came from.
        let undo = transaction::propose_restore(&rt, "TEXT").expect("something to undo");
        let back = transaction::confirm(&rt, &undo.id, None).expect("confirm");
        eprintln!("{:?} — {:?}", back.state, back.verification);
        assert_eq!(back.state, ActionState::Succeeded, "{:?}", back.reason);
        for p in &paths {
            assert!(std::path::Path::new(p).exists(), "did not come back: {p}");
        }

        std::fs::remove_dir_all(&folder).ok();
        voice.speech.lock().unwrap().shutdown();
    }

    /// Opt-in, and acts on this Mac: brings Google Chrome to the front.
    ///   cargo test -p lantern -- --ignored open_chrome_live --nocapture
    #[test]
    #[ignore]
    fn open_chrome_live_resolves_the_installed_app_and_the_executor_verifies_it() {
        let (state, voice) = live_state();
        let r = with_actions(None, &state, |rt| transaction::propose(rt, "Open Chrome", "VOICE")).expect("a plan");
        eprintln!("{} {:?} {:?} — verification: {:?} — reason: {:?}", r.action.tag(), r.action, r.state, r.verification, r.reason);
        assert_eq!(r.action.app_name(), Some("Google Chrome"));
        assert_eq!(r.state, lantern_core::actions::ActionState::Succeeded);
        assert!(r.verification.as_deref().unwrap().starts_with("com.google.Chrome running as pid"));
        voice.speech.lock().unwrap().shutdown();
    }

    #[test]
    fn the_shell_measures_its_own_resource_use() {
        let (cpu, footprint) = sensing::own_usage().expect("getrusage and proc_pid_rusage");
        assert!(cpu >= 0.0);
        assert!(footprint > 1024 * 1024, "footprint of a running test process: {footprint}");
    }
}
