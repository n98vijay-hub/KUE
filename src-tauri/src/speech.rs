//! The speech output process (voice/, `kue-voice`), managed by the shell, and
//! the one pipeline that reaches it.
//!
//! It receives only sentences KUE Core cleared, one at a time, from the core's
//! speech queue (`lantern_core::voice::speaker`), one JSON line each on its
//! standard input, and reports when speech starts, finishes or is cut off. It
//! holds no sensor handle. It is terminated by the kill switch and never
//! started while killed.
//!
//!   action record / answer → Narrator → speech gate + firewall → SpeechController → Speech (kue-voice)
//!
//! The window can stop speech and change voice settings. It has no command that speaks.

use lantern_core::actions::{ActionRecord, ActionState};
use lantern_core::conversation::Exchange;
use lantern_core::privacy::Firewall;
use lantern_core::voice::narration::Narrator;
use lantern_core::voice::policy::clear_for_speech;
use lantern_core::voice::provider::{ProviderAvailability, VoiceInfo, VoiceProviderId};
use lantern_core::voice::speaker::{ProviderEvent, SpeechController, SpeechGate, SpeechProvider, SpeechStop};
use lantern_core::voice::SpeechDraft;
use lantern_core::Engine;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
pub struct Speech {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    /// Which process the reader threads belong to; a replaced process's thread goes quiet.
    generation: u64,
    /// The installed voices, from the process's hello. Empty until then.
    pub voices: Vec<VoiceInfo>,
    /// The sentence being spoken, by request id.
    pub speaking: Option<String>,
    /// Sent but not yet started: id → when it was sent.
    requested: Vec<(String, Instant)>,
    /// Time from sending a sentence to the synthesizer starting it, most recent first (at most 20).
    pub start_latency_ms: Vec<f64>,
    pub spoken: u64,
    pub cut_off: u64,
    pub failures: u64,
    pub last_failure: Option<String>,
}

impl Speech {
    pub fn new() -> Self { Self::default() }

    pub fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)) && self.stdin.is_some(),
            None => false,
        }
    }

    fn send(&mut self, v: &serde_json::Value) -> Result<(), String> {
        let s = self.stdin.as_mut().ok_or_else(|| "the speech process is not running".to_string())?;
        writeln!(s, "{v}").and_then(|_| s.flush()).map_err(|e| format!("could not reach the speech process: {e}"))
    }

    pub fn shutdown(&mut self) {
        let _ = self.send(&serde_json::json!({ "cmd": "shutdown" }));
        self.stdin = None;
        if let Some(c) = &mut self.child {
            // Killed outright: the kill switch does not wait for a sentence to end.
            let _ = c.kill();
            let _ = c.wait();
        }
        self.child = None;
        self.speaking = None;
        self.requested.clear();
    }

    /// Updates the process record and returns the report for the speech queue.
    fn on_message(&mut self, v: &serde_json::Value) -> Option<(String, ProviderEvent)> {
        let id = v["id"].as_str().unwrap_or_default().to_string();
        let event = match v["type"].as_str() {
            Some("hello") => {
                self.voices = serde_json::from_value(v["voices"].clone()).unwrap_or_default();
                return None;
            }
            Some("started") => {
                if let Some(i) = self.requested.iter().position(|(r, _)| *r == id) {
                    let (_, at) = self.requested.remove(i);
                    self.start_latency_ms.insert(0, at.elapsed().as_secs_f64() * 1000.0);
                    self.start_latency_ms.truncate(20);
                }
                self.speaking = Some(id.clone());
                ProviderEvent::Started
            }
            Some(t @ ("finished" | "cancelled" | "failed")) => {
                self.requested.retain(|(r, _)| *r != id);
                if self.speaking.as_deref() == Some(id.as_str()) { self.speaking = None; }
                match t {
                    "finished" => { self.spoken += 1; ProviderEvent::Finished }
                    "cancelled" => { self.cut_off += 1; ProviderEvent::Cancelled }
                    _ => {
                        self.failures += 1;
                        self.last_failure = v["reason"].as_str().map(String::from);
                        ProviderEvent::Failed(self.last_failure.clone().unwrap_or_else(|| "FAILED".into()))
                    }
                }
            }
            _ => return None,
        };
        Some((id, event))
    }
}

impl SpeechProvider for Speech {
    fn id(&self) -> VoiceProviderId { VoiceProviderId::MacosNative }
    fn is_available(&mut self) -> bool { self.is_running() }
    fn speak(&mut self, request_id: &str, text: &str, voice: Option<&str>, rate: f64, volume: f64, interrupt: bool) -> Result<(), String> {
        self.send(&serde_json::json!({
            "cmd": "speak", "id": request_id, "text": text, "voice": voice.unwrap_or(""), "rate": rate, "volume": volume,
            "interrupt": interrupt,
        }))?;
        self.requested.push((request_id.to_string(), Instant::now()));
        Ok(())
    }
    fn cancel(&mut self) {
        if self.stdin.is_some() { let _ = self.send(&serde_json::json!({ "cmd": "stop" })); }
    }
    fn is_speaking(&self) -> bool { self.speaking.is_some() }
    fn available_voices(&self) -> Vec<VoiceInfo> { self.voices.clone() }
}

pub fn locate() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidates = vec![exe.with_file_name("kue-voice")];
    let mut d = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..6 {
        if let Some(dir) = &d {
            candidates.push(dir.join("voice/bin/kue-voice"));
            d = dir.parent().map(|p| p.to_path_buf());
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../voice/bin/kue-voice"));
    candidates.into_iter().find(|p| p.exists())
}

/// Starts the process and waits briefly for its list of voices.
/// `on_report` runs, with no lock held, for every report about a sentence and
/// once with None when the process ends.
pub fn spawn(exe: PathBuf, speech: Arc<Mutex<Speech>>, on_report: impl Fn(Option<(String, ProviderEvent)>) + Send + 'static)
    -> Result<(), String>
{
    let generation;
    let mut child = Command::new(&exe).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("could not start speech output at {}: {e}", exe.display()))?;
    let stdout = child.stdout.take().ok_or("speech process produced no stdout")?;
    let stderr = child.stderr.take().ok_or("speech process produced no stderr")?;
    let stdin = child.stdin.take().ok_or("speech process accepted no stdin")?;
    {
        let mut s = speech.lock().unwrap();
        s.shutdown();
        s.generation += 1;
        generation = s.generation;
        s.child = Some(child);
        s.stdin = Some(stdin);
        s.voices.clear();
    }
    {
        let speech = Arc::clone(&speech);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                let report = speech.lock().ok().filter(|s| s.generation == generation).and_then(|mut s| s.on_message(&v));
                if report.is_some() { on_report(report); }
            }
            // Only if no newer process has replaced this one.
            let current = speech.lock().map(|mut s| {
                let current = s.generation == generation;
                if current { s.stdin = None; s.speaking = None; s.requested.clear(); }
                current
            }).unwrap_or(false);
            if current { on_report(None); }
        });
    }
    // Diagnostics only. Never parsed and never forwarded: a framework could
    // echo the text being spoken.
    std::thread::spawn(move || { for _ in BufReader::new(stderr).lines().map_while(Result::ok) {} });
    for _ in 0..40 {
        if !speech.lock().unwrap().voices.is_empty() { break; }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Ok(())
}

// MARK: - The pipeline

/// What KUE says and the queue it says it from. One per launch.
pub struct VoiceOut {
    pub narrator: Narrator,
    pub controller: SpeechController,
    /// A spoken "yes" confirms only a request that has waited at most this long.
    pub spoken_confirmation_seconds: f64,
}

/// Everything the pipeline touches. Locks are always taken in this order:
/// out → speech → engine → firewall. Nothing that holds the engine or the
/// firewall calls in here.
#[derive(Clone)]
pub struct Voice {
    pub engine: Arc<Mutex<Engine>>,
    pub firewall: Arc<Mutex<Firewall>>,
    pub out: Arc<Mutex<VoiceOut>>,
    pub speech: Arc<Mutex<Speech>>,
    /// Where kue-voice is. None: not installed, and nothing is spoken.
    pub exe: Option<PathBuf>,
}

impl Voice {
    /// Starts kue-voice, unless KUE is killed. Its reports feed the queue.
    pub fn start(&self) -> Result<(), String> {
        if self.engine.lock().unwrap().is_killed() {
            return Err("KUE is killed. Speech output cannot start until you recover.".into());
        }
        let exe = self.exe.clone().ok_or("Speech output (kue-voice) is not installed.")?;
        if self.speech.lock().unwrap().is_running() { return Ok(()); }
        let me = self.clone();
        spawn(exe, Arc::clone(&self.speech), move |report| me.on_report(report, crate::sensing::now()))?;
        // Checked again: a kill may have landed while it was starting.
        if self.engine.lock().unwrap().is_killed() {
            self.speech.lock().unwrap().shutdown();
            return Err("KUE was killed while speech output was starting; it has been stopped.".into());
        }
        Ok(())
    }

    fn record_audit(&self, out: &mut VoiceOut, t: f64) {
        let audit = out.controller.take_audit();
        if audit.is_empty() { return; }
        if let Ok(mut e) = self.engine.lock() {
            for a in audit { e.record_speech_event(a.summary(), t); }
        }
    }

    /// One draft through the gate and the firewall into the queue. Returns the request id.
    fn say(&self, out: &mut VoiceOut, draft: SpeechDraft, t: f64) -> String {
        let mut sp = self.speech.lock().unwrap();
        let available = ProviderAvailability { macos_native: sp.is_running(), external_configured: false };
        let (decision, gate) = {
            let e = self.engine.lock().unwrap();
            let mut fw = self.firewall.lock().unwrap();
            (clear_for_speech(&e, &mut fw, draft.clone(), &out.controller.settings, available, t), SpeechGate::of(&e, t))
        };
        let id = match decision {
            Ok((cleared, provider)) => out.controller.submit(cleared, provider, gate, &mut *sp, t),
            Err(why) => out.controller.refused(&draft, why, t),
        };
        drop(sp);
        self.record_audit(out, t);
        id
    }

    /// Narrates an action's new state (given as the window may show it).
    pub fn on_action(&self, rec: &ActionRecord, t: f64) {
        let mut out = self.out.lock().unwrap();
        if rec.state == ActionState::Cancelled {
            // What was queued about it no longer applies.
            let mut sp = self.speech.lock().unwrap();
            out.controller.cancel_topic(&rec.id, &mut *sp, t);
        }
        let verbosity = out.controller.settings.verbosity;
        if let Some(d) = out.narrator.on_action(rec, verbosity, t) { self.say(&mut out, d, t); }
        self.record_audit(&mut out, t);
    }

    /// Speaks a finished answer when you asked by voice (or asked for answers aloud).
    pub fn on_answer(&self, x: &Exchange, t: f64) {
        let mut out = self.out.lock().unwrap();
        let (typed, verbosity) = (out.controller.settings.speak_typed_answers, out.controller.settings.verbosity);
        if let Some(d) = out.narrator.on_answer(x, typed, verbosity, t) { self.say(&mut out, d, t); }
    }

    /// A sentence written by KUE with no data ("There's nothing waiting for your confirmation.").
    pub fn say_plain(&self, text: &str, topic: &str, priority: lantern_core::voice::Priority, t: f64) {
        let mut out = self.out.lock().unwrap();
        self.say(&mut out, SpeechDraft::new(text, &[], priority, topic), t);
    }

    /// A sentence written by KUE from data, declaring the kinds it carries so
    /// the firewall decides again whether they may be spoken.
    pub fn say_carrying(&self, text: &str, carries: &[lantern_core::privacy::DataKind], topic: &str, priority: lantern_core::voice::Priority, t: f64) {
        let mut out = self.out.lock().unwrap();
        self.say(&mut out, SpeechDraft::new(text, carries, priority, topic), t);
    }

    /// Applies the runtime state (kill, pause, lock, microphone) and moves the queue on.
    pub fn pump(&self, t: f64) {
        let mut out = self.out.lock().unwrap();
        let mut sp = self.speech.lock().unwrap();
        let gate = SpeechGate::of(&self.engine.lock().unwrap(), t);
        out.controller.pump(gate, &mut *sp, t);
        drop(sp);
        self.share_own_speech(&mut out, t);
        self.record_audit(&mut out, t);
    }

    /// Hands the engine the words KUE is saying or just said, so a wake that is
    /// KUE's own voice coming back is recognised (`voice::echo`).
    fn share_own_speech(&self, out: &mut VoiceOut, t: f64) {
        let own = out.controller.own_speech(t);
        if let Ok(mut e) = self.engine.lock() { e.set_own_speech(own); }
    }

    /// A report from kue-voice, or None when its process ended.
    pub fn on_report(&self, report: Option<(String, ProviderEvent)>, t: f64) {
        let mut out = self.out.lock().unwrap();
        let mut sp = self.speech.lock().unwrap();
        let gate = SpeechGate::of(&self.engine.lock().unwrap(), t);
        match report {
            Some((id, ev)) => out.controller.on_provider_event(&id, ev, gate, &mut *sp, t),
            None => out.controller.provider_gone(t),
        }
        drop(sp);
        self.share_own_speech(&mut out, t);
        self.record_audit(&mut out, t);
    }

    /// Stops speech now and drops the queue.
    pub fn stop(&self, why: SpeechStop, t: f64) {
        let mut out = self.out.lock().unwrap();
        let mut sp = self.speech.lock().unwrap();
        out.controller.stop_all(why, &mut *sp, t);
        drop(sp);
        self.record_audit(&mut out, t);
    }

    /// The kill switch: speech stops, the queue is dropped, the process is terminated.
    pub fn kill(&self, t: f64) {
        let mut out = self.out.lock().unwrap();
        let mut sp = self.speech.lock().unwrap();
        out.controller.stop_all(SpeechStop::Killed, &mut *sp, t);
        sp.shutdown();
        drop(sp);
        self.record_audit(&mut out, t);
    }
}
