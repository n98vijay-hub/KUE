//! The on-device language model process (mind/), managed by the shell.
//!
//! It holds no sensor handle. It receives only a prompt that has already been
//! cleared by the privacy firewall for Destination::LocalModel, and it can only
//! return text. It is terminated by the kill switch and never started while
//! killed.

use lantern_core::model::{Admitted, ModelProvider};
use lantern_core::router::ModelId;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

pub struct Mind {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    /// AVAILABLE, UNAVAILABLE, or None before the process has said hello.
    pub availability: Option<String>,
    pub unavailable_reason: Option<String>,
}

impl Mind {
    pub fn new() -> Self {
        Mind { child: None, stdin: None, availability: None, unavailable_reason: None }
    }

    /// The model process's pid, when one is running. Used to prove there is
    /// exactly one: two answering at once were measured at twice the latency.
    pub fn pid(&self) -> Option<u32> { self.child.as_ref().map(|c| c.id()) }

    pub fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Private: the process is reached only through `ModelProvider::deliver`,
    /// which takes nothing but a prompt `model::ask` admitted for this Mac's model.
    fn send(&mut self, v: &serde_json::Value) -> Result<(), String> {
        let s = self.stdin.as_mut().ok_or_else(|| "the on-device model process is not running".to_string())?;
        writeln!(s, "{v}").and_then(|_| s.flush()).map_err(|e| format!("could not reach the on-device model: {e}"))
    }

    pub fn shutdown(&mut self) {
        let _ = self.send(&serde_json::json!({ "cmd": "shutdown" }));
        self.stdin = None;
        if let Some(c) = &mut self.child {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = c.kill();
            let _ = c.wait();
        }
        self.child = None;
        self.availability = None;
    }
}

impl ModelProvider for Mind {
    fn model(&self) -> ModelId { ModelId::AppleOnDevice }

    fn deliver(&mut self, request: Admitted<'_>) -> Result<(), String> {
        let p = request.prompt();
        let line = serde_json::json!({ "cmd": "ask", "id": request.id, "instructions": p.instructions, "prompt": p.prompt });
        self.send(&line)
    }

    fn cancel(&mut self, id: &str) -> Result<(), String> {
        self.send(&serde_json::json!({ "cmd": "cancel", "id": id }))
    }
}

pub fn locate() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidates = vec![exe.with_file_name("lantern-mind")];
    let mut d = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..6 {
        if let Some(dir) = &d {
            candidates.push(dir.join("mind/bin/lantern-mind"));
            d = dir.parent().map(|p| p.to_path_buf());
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mind/bin/lantern-mind"));
    candidates.into_iter().find(|p| p.exists())
}

/// Starts the process. `on_message` receives every protocol message except
/// hello/status, which update availability here. `on_exit` runs when it stops.
pub fn spawn(
    exe: PathBuf,
    mind: Arc<Mutex<Mind>>,
    on_message: impl Fn(serde_json::Value) + Send + 'static,
    on_exit: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let mut child = Command::new(&exe).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("could not start the on-device model at {}: {e}", exe.display()))?;
    let stdout = child.stdout.take().ok_or("model process produced no stdout")?;
    let stderr = child.stderr.take().ok_or("model process produced no stderr")?;
    let stdin = child.stdin.take().ok_or("model process accepted no stdin")?;
    {
        let mut m = mind.lock().unwrap();
        m.child = Some(child);
        m.stdin = Some(stdin);
        m.availability = None;
    }
    {
        let mind = Arc::clone(&mind);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                match v.get("type").and_then(|t| t.as_str()) {
                    Some("hello") | Some("status") => {
                        if let Ok(mut m) = mind.lock() {
                            m.availability = v.get("availability").and_then(|a| a.as_str()).map(String::from);
                            m.unavailable_reason = v.get("reason").and_then(|a| a.as_str()).map(String::from);
                        }
                    }
                    _ => on_message(v),
                }
            }
            if let Ok(mut m) = mind.lock() { m.stdin = None; m.availability = None; }
            on_exit();
        });
    }
    // Diagnostics only. Never parsed and never forwarded: the model's stderr
    // could echo prompt text, and diagnostic logs may carry no LOCAL_ONLY data.
    std::thread::spawn(move || { for _ in BufReader::new(stderr).lines().map_while(Result::ok) {} });
    Ok(())
}
