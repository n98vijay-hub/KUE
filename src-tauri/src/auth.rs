//! Runs kue-auth, the LocalAuthentication helper, and reads back what macOS said.
//!
//! KUE never handles a password or a fingerprint. macOS draws the prompt; the
//! helper returns SUCCESS or the reason it failed, and only SUCCESS is ever
//! turned into a grant (by the core, not here).

use lantern_core::authz::OsAuthKind;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn locate() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidates = vec![exe.with_file_name("kue-auth")];
    let mut d = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..6 {
        if let Some(dir) = &d {
            candidates.push(dir.join("auth/bin/kue-auth"));
            d = dir.parent().map(|p| p.to_path_buf());
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../auth/bin/kue-auth"));
    candidates.into_iter().find(|p| p.exists())
}

fn run(args: &[&str], timeout: Duration) -> Result<serde_json::Value, String> {
    let exe = locate().ok_or("The authentication helper (kue-auth) is not installed.")?;
    let mut child = Command::new(exe).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|e| format!("could not start the authentication helper: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The authentication prompt timed out.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut out = String::new();
    child.stdout.take().ok_or("no output")?.read_to_string(&mut out).map_err(|e| e.to_string())?;
    serde_json::from_str(out.trim()).map_err(|_| "The authentication helper returned an unreadable answer.".into())
}

/// What the helper reports about this Mac's authentication hardware.
pub fn probe() -> serde_json::Value {
    run(&["probe"], Duration::from_secs(5)).unwrap_or_else(|e| serde_json::json!({ "result": "UNAVAILABLE", "reason": e }))
}

/// Asks macOS to authenticate. Returns the result code: "SUCCESS", or why not.
pub fn authenticate(kind: OsAuthKind, reason: &str) -> String {
    let level = match kind { OsAuthKind::Strong => "strong", OsAuthKind::Physical => "physical" };
    match run(&[level, reason], Duration::from_secs(120)) {
        Ok(v) => match v.get("result").and_then(|r| r.as_str()) {
            Some("SUCCESS") => "SUCCESS".into(),
            _ => v.get("reason").and_then(|r| r.as_str()).unwrap_or("FAILED").to_string(),
        },
        Err(e) => format!("HELPER_ERROR: {e}"),
    }
}
