//! Supervisor for the Swift sensing process.
//!
//! The Rust core owns this process end to end. The UI has no path to the camera:
//! it can only invoke a Tauri command, which asks the core, which writes a line
//! to this child's stdin. That is the whole of the control flow, by design.

use lantern_core::sensor::parse_line;
use lantern_core::Engine;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

pub struct Sensing {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    pub last_error: Option<String>,
}

impl Sensing {
    pub fn new() -> Self {
        Sensing { child: None, stdin: None, last_error: None }
    }

    pub fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Sends one command line to the sensing layer.
    pub fn send(&mut self, json: &str) -> Result<(), String> {
        let s = self.stdin.as_mut().ok_or_else(|| "sensing layer is not running".to_string())?;
        writeln!(s, "{json}").map_err(|e| format!("could not reach the sensing layer: {e}"))?;
        s.flush().map_err(|e| format!("could not reach the sensing layer: {e}"))
    }

    pub fn shutdown(&mut self) {
        let _ = self.send(r#"{"cmd":"shutdown"}"#);
        self.stdin = None;
        if let Some(c) = &mut self.child {
            // Give it a moment to tear the capture session down cleanly, then insist.
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = c.kill();
            let _ = c.wait();
        }
        self.child = None;
    }
}

/// Finds LanternSense.app.
///
/// It must be an .app bundle: macOS refuses to show a camera permission prompt
/// for a bare executable, so a loose binary would fail closed with no
/// explanation. Verified on macOS 26.
pub fn locate_sensing_app(resource_dir: Option<PathBuf>) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("LanternSense.app/Contents/MacOS/lantern-sense"));
    }
    // Development layout, relative to the built binary.
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..6 {
            if let Some(dir) = &d {
                candidates.push(dir.join("../Resources/LanternSense.app/Contents/MacOS/lantern-sense"));
                candidates.push(dir.join("sensing/bundle/LanternSense.app/Contents/MacOS/lantern-sense"));
                d = dir.parent().map(|p| p.to_path_buf());
            }
        }
    }
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }
    Err(format!(
        "LanternSense.app was not found. Build it with ./sensing/build.sh. Looked in:\n{}",
        candidates.iter().map(|c| format!("  {}", c.display())).collect::<Vec<_>>().join("\n")
    ))
}

/// Starts the sensing process and wires its stdout into the engine.
pub fn spawn(
    exe: PathBuf,
    engine: Arc<Mutex<Engine>>,
    sensing: Arc<Mutex<Sensing>>,
) -> Result<(), String> {
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start the sensing layer at {}: {e}", exe.display()))?;

    let stdout = child.stdout.take().ok_or("sensing layer produced no stdout")?;
    let stderr = child.stderr.take().ok_or("sensing layer produced no stderr")?;
    let stdin = child.stdin.take().ok_or("sensing layer accepted no stdin")?;

    {
        let mut s = sensing.lock().unwrap();
        s.child = Some(child);
        s.stdin = Some(stdin);
        s.last_error = None;
    }

    // Protocol reader.
    {
        let engine = Arc::clone(&engine);
        let sensing = Arc::clone(&sensing);
        std::thread::spawn(move || {
            let r = BufReader::new(stdout);
            for line in r.lines() {
                let line = match line { Ok(l) => l, Err(_) => break };
                // Unparseable lines are framework noise, not protocol. Skip them.
                if let Some(msg) = parse_line(&line) {
                    if let Ok(mut e) = engine.lock() {
                        e.ingest(msg, now());
                    }
                }
            }
            // stdout closed: the sensing layer has exited.
            if let Ok(mut e) = engine.lock() {
                e.set_sensing_process_up(false, now());
            }
            if let Ok(mut s) = sensing.lock() {
                s.stdin = None;
            }
        });
    }

    // Diagnostics channel. Never parsed, only surfaced on failure.
    {
        let sensing = Arc::clone(&sensing);
        std::thread::spawn(move || {
            let r = BufReader::new(stderr);
            for line in r.lines().map_while(Result::ok) {
                eprintln!("[sense] {line}");
                if line.to_lowercase().contains("error") {
                    if let Ok(mut s) = sensing.lock() {
                        s.last_error = Some(line);
                    }
                }
            }
        });
    }

    Ok(())
}

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// This process's own CPU time (user + system, seconds) and physical memory
/// footprint (bytes), from getrusage and proc_pid_rusage.
pub fn own_usage() -> Option<(f64, u64)> {
    // SAFETY: both calls write only into the zeroed structs passed to them.
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
            return None;
        }
        let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1_000_000.0;
        let cpu = secs(ru.ru_utime) + secs(ru.ru_stime);

        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        let rc = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            &mut info as *mut libc::rusage_info_v2 as *mut libc::rusage_info_t,
        );
        if rc != 0 {
            return None;
        }
        Some((cpu, info.ri_phys_footprint))
    }
}
