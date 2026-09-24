//! The Action Broker's executor boundary: runs KueAct for one request.
//!
//! Policy and the transaction live in `lantern_core::actions` and
//! `lantern_core::transaction`. This module only starts `kue-act` with the
//! request's verb as its single argument and the target on standard input, so
//! no app name, link, path or notification text appears in the process table.

use lantern_core::actions::ActionState;
use lantern_core::transaction::{Execution, ExecutorRequest};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn locate_act() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidates = Vec::new();
    let mut d = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..6 {
        if let Some(dir) = &d {
            candidates.push(dir.join("../Resources/KueAct.app/Contents/MacOS/kue-act"));
            candidates.push(dir.join("act/bundle/KueAct.app/Contents/MacOS/kue-act"));
            d = dir.parent().map(|p| p.to_path_buf());
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../act/bundle/KueAct.app/Contents/MacOS/kue-act"));
    candidates.into_iter().find(|p| p.exists())
}

/// Runs one application/link/notification/document request through kue-act.
pub fn run_os_action(req: &ExecutorRequest) -> Execution {
    let fail = |why: String| Execution { state: ActionState::Failed, reason: Some(why), verification: None, landed: None };
    let unknown = |why: String| Execution { state: ActionState::UnknownResult, reason: Some(why), verification: None, landed: None };
    let Some(exe) = locate_act() else {
        return fail("The action executor (KueAct) is not installed.".into());
    };
    let mut child = match Command::new(exe).args(req.argv()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
        Ok(c) => c,
        Err(e) => return fail(format!("Could not start the action executor: {e}")),
    };
    // The target, once, then end of input.
    let wrote = child.stdin.take().map(|mut s| s.write_all(req.stdin_line().as_bytes())).unwrap_or(Ok(()));
    if let Err(e) = wrote {
        let _ = child.kill();
        let _ = child.wait();
        return fail(format!("Could not hand the request to the action executor: {e}"));
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(90) => {
                let _ = child.kill();
                let _ = child.wait();
                return unknown("The action did not report back within 90 seconds.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return unknown(e.to_string()),
        }
    }
    let mut out = String::new();
    if let Some(mut s) = child.stdout.take() { let _ = s.read_to_string(&mut out); }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(out.trim()) else {
        return unknown("The action executor returned an unreadable result.".into());
    };
    let state = match v["result"].as_str() {
        Some("SUCCEEDED") => ActionState::Succeeded,
        Some("FAILED") => ActionState::Failed,
        _ => ActionState::UnknownResult,
    };
    Execution {
        state,
        reason: v["reason"].as_str().map(String::from),
        verification: v["verification"].as_str().map(String::from),
        landed: v["landed"].as_str().map(String::from),
    }
}
