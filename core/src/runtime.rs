//! KUE runtime control: the kill switch.
//!
//! PAUSE stops observation and can be undone with one click. KILL is different:
//!
//!  * it is a LATCH written to disk before anything else happens, so a crash, a
//!    force-quit or a relaunch comes back KILLED, not running;
//!  * while killed nothing observes, nothing is analysed and nothing is written
//!    to local memory;
//!  * anyone may pull it — the owner, the model, an automation, or a file
//!    created from outside the app — because stopping is always safe;
//!  * only the OWNER may undo it, in two deliberate steps (begin, then
//!    complete), from Lantern's own window. A model, an automation, or a latch
//!    deleted from outside cannot recover it.
//!
//! Fail closed: if the latch cannot be read, KUE is treated as killed. If a
//! kill cannot be saved, KUE is still killed in memory and says, loudly, that a
//! relaunch would not stay killed.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const LATCH_FILE_NAME: &str = "KILLED";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeState {
    #[serde(rename = "KUE_RUNNING")]
    Running,
    #[serde(rename = "KUE_PAUSED")]
    Paused,
    #[serde(rename = "KUE_KILLED")]
    Killed,
    #[serde(rename = "KUE_RECOVERING")]
    Recovering,
}

impl RuntimeState {
    pub fn label(self) -> &'static str {
        match self {
            RuntimeState::Running => "KUE_RUNNING",
            RuntimeState::Paused => "KUE_PAUSED",
            RuntimeState::Killed => "KUE_KILLED",
            RuntimeState::Recovering => "KUE_RECOVERING",
        }
    }
}

/// Who is asking. The shell stamps OWNER only on commands that come from a
/// gesture in Lantern's own window; everything else is stamped with what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Principal {
    /// A direct gesture in Lantern's own window.
    Owner,
    /// Any language model, local or otherwise.
    Model,
    /// Scheduled or proactive behaviour inside KUE.
    Automation,
    /// Something outside the app, e.g. a latch file created from a terminal.
    External,
}

impl Principal {
    pub fn label(self) -> &'static str {
        match self {
            Principal::Owner => "OWNER",
            Principal::Model => "MODEL",
            Principal::Automation => "AUTOMATION",
            Principal::External => "EXTERNAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KillRecord {
    pub at: f64,
    pub by: Principal,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Only the owner may recover a killed runtime.
    NotOwner(Principal),
    /// There is nothing to recover from.
    NotKilled,
    /// Recovery must be begun before it can be completed.
    RecoveryNotBegun,
    /// The latch could not be removed, so the runtime stays killed.
    LatchNotCleared(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NotOwner(p) => write!(f, "Refused: only the owner can recover KUE, not {}.", p.label()),
            Refusal::NotKilled => write!(f, "KUE is not killed."),
            Refusal::RecoveryNotBegun => write!(f, "Recovery has not been begun."),
            Refusal::LatchNotCleared(e) => write!(f, "The kill latch could not be removed ({e}); KUE stays killed."),
        }
    }
}

#[derive(Debug)]
pub struct KillSwitch {
    latch: Option<PathBuf>,
    killed: Option<KillRecord>,
    recovering: bool,
    /// Set when the latch could not be written: killed now, but not durably.
    latch_error: Option<String>,
}

impl KillSwitch {
    /// No latch on disk. For tests and for a runtime whose support directory
    /// cannot be found — which is reported, not hidden.
    pub fn in_memory() -> Self {
        KillSwitch { latch: None, killed: None, recovering: false, latch_error: None }
    }

    /// Reads the latch. Present → killed. Unreadable → killed (fail closed).
    pub fn with_latch(path: impl Into<PathBuf>, now: f64) -> Self {
        let path = path.into();
        let mut ks = KillSwitch { latch: Some(path.clone()), killed: None, recovering: false, latch_error: None };
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                let text = std::fs::read_to_string(&path).ok();
                let record = text.as_deref()
                    .and_then(|s| serde_json::from_str::<KillRecord>(s).ok())
                    .unwrap_or(KillRecord {
                        at: now,
                        by: Principal::External,
                        reason: match text.as_deref().map(str::trim) {
                            Some("") => "The kill latch file was created outside the app.".into(),
                            _ => "A kill latch exists but its contents could not be read; treated as killed.".into(),
                        },
                    });
                ks.killed = Some(record);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                ks.killed = Some(KillRecord {
                    at: now,
                    by: Principal::External,
                    reason: format!("The kill latch could not be checked ({e}); failing closed."),
                });
            }
        }
        ks
    }

    pub fn latch_path(&self) -> Option<&Path> { self.latch.as_deref() }
    pub fn record(&self) -> Option<&KillRecord> { self.killed.as_ref() }
    pub fn latch_error(&self) -> Option<&str> { self.latch_error.as_deref() }
    pub fn is_recovering(&self) -> bool { self.recovering }

    /// True while killed OR recovering: in both, nothing runs.
    pub fn is_killed(&self) -> bool { self.killed.is_some() }

    pub fn state(&self, paused: bool) -> RuntimeState {
        match (&self.killed, self.recovering, paused) {
            (Some(_), true, _) => RuntimeState::Recovering,
            (Some(_), false, _) => RuntimeState::Killed,
            (None, _, true) => RuntimeState::Paused,
            (None, _, false) => RuntimeState::Running,
        }
    }

    /// Anyone may kill. Idempotent: a second kill keeps the first record.
    /// Returns Err only if the latch could not be saved — the runtime is killed
    /// regardless.
    pub fn kill(&mut self, by: Principal, reason: impl Into<String>, now: f64) -> Result<(), String> {
        self.recovering = false;
        if self.killed.is_none() {
            self.killed = Some(KillRecord { at: now, by, reason: reason.into() });
        }
        self.persist()
    }

    /// Owner only. The runtime stays killed while recovering.
    pub fn begin_recovery(&mut self, by: Principal) -> Result<(), Refusal> {
        if by != Principal::Owner { return Err(Refusal::NotOwner(by)); }
        if self.killed.is_none() { return Err(Refusal::NotKilled); }
        self.recovering = true;
        Ok(())
    }

    /// Owner only. Abandons a begun recovery; stays killed.
    pub fn cancel_recovery(&mut self, by: Principal) -> Result<(), Refusal> {
        if by != Principal::Owner { return Err(Refusal::NotOwner(by)); }
        self.recovering = false;
        Ok(())
    }

    /// Owner only, after `begin_recovery`. Removes the latch first; if it
    /// cannot be removed the runtime stays killed.
    pub fn complete_recovery(&mut self, by: Principal) -> Result<KillRecord, Refusal> {
        if by != Principal::Owner { return Err(Refusal::NotOwner(by)); }
        if self.killed.is_none() { return Err(Refusal::NotKilled); }
        if !self.recovering { return Err(Refusal::RecoveryNotBegun); }
        if let Some(p) = &self.latch {
            match std::fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    self.recovering = false;
                    return Err(Refusal::LatchNotCleared(e.to_string()));
                }
            }
        }
        self.recovering = false;
        self.latch_error = None;
        Ok(self.killed.take().expect("checked above"))
    }

    /// Called periodically. A latch that appears from outside kills the
    /// runtime. A latch that disappears from outside does NOT recover it: the
    /// latch is written back.
    pub fn poll(&mut self, now: f64) -> bool {
        let Some(path) = self.latch.clone() else { return false };
        let present = !matches!(std::fs::symlink_metadata(&path), Err(ref e) if e.kind() == std::io::ErrorKind::NotFound);
        match (self.killed.is_some(), present) {
            (false, true) => {
                *self = KillSwitch::with_latch(path, now);
                true
            }
            (true, false) => {
                let _ = self.persist();
                false
            }
            _ => false,
        }
    }

    fn persist(&mut self) -> Result<(), String> {
        let (Some(path), Some(record)) = (self.latch.clone(), self.killed.clone()) else {
            if self.latch.is_none() {
                let msg = "No kill latch location: KUE is killed now, but a relaunch would not stay killed.".to_string();
                self.latch_error = Some(msg.clone());
                return Err(msg);
            }
            return Ok(());
        };
        let result = (|| -> std::io::Result<()> {
            let tmp = path.with_extension("tmp");
            {
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(serde_json::to_string(&record).unwrap_or_default().as_bytes())?;
                f.sync_all()?;
            }
            std::fs::rename(&tmp, &path)?;
            if let Some(dir) = path.parent() {
                if let Ok(d) = std::fs::File::open(dir) { let _ = d.sync_all(); }
            }
            Ok(())
        })();
        match result {
            Ok(()) => { self.latch_error = None; Ok(()) }
            Err(e) => {
                let msg = format!("The kill latch could not be saved ({e}): KUE is killed now, but a relaunch would not stay killed.");
                self.latch_error = Some(msg.clone());
                Err(msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kue-kill-{tag}-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_kill_survives_a_relaunch() {
        let d = dir("relaunch");
        let latch = d.join(LATCH_FILE_NAME);
        let mut ks = KillSwitch::with_latch(&latch, 1.0);
        assert_eq!(ks.state(false), RuntimeState::Running);
        ks.kill(Principal::Owner, "test", 2.0).unwrap();
        drop(ks);

        let relaunched = KillSwitch::with_latch(&latch, 99.0);
        assert_eq!(relaunched.state(false), RuntimeState::Killed);
        let r = relaunched.record().unwrap();
        assert_eq!((r.at, r.by, r.reason.as_str()), (2.0, Principal::Owner, "test"));
    }

    #[test]
    fn only_the_owner_can_recover() {
        let d = dir("owner");
        let mut ks = KillSwitch::with_latch(d.join(LATCH_FILE_NAME), 1.0);
        ks.kill(Principal::Model, "the model may stop KUE", 1.0).unwrap();
        for p in [Principal::Model, Principal::Automation, Principal::External] {
            assert_eq!(ks.begin_recovery(p), Err(Refusal::NotOwner(p)));
            assert_eq!(ks.complete_recovery(p), Err(Refusal::NotOwner(p)));
            assert!(ks.is_killed());
        }
    }

    #[test]
    fn recovery_takes_two_deliberate_steps_and_stays_killed_between_them() {
        let d = dir("two-step");
        let latch = d.join(LATCH_FILE_NAME);
        let mut ks = KillSwitch::with_latch(&latch, 1.0);
        ks.kill(Principal::Owner, "t", 1.0).unwrap();
        assert_eq!(ks.complete_recovery(Principal::Owner), Err(Refusal::RecoveryNotBegun));
        ks.begin_recovery(Principal::Owner).unwrap();
        assert_eq!(ks.state(false), RuntimeState::Recovering);
        assert!(ks.is_killed(), "recovering must still block everything");
        assert!(latch.exists(), "the latch stays until recovery completes");
        ks.complete_recovery(Principal::Owner).unwrap();
        assert_eq!(ks.state(false), RuntimeState::Running);
        assert!(!latch.exists());
        assert_eq!(KillSwitch::with_latch(&latch, 5.0).state(false), RuntimeState::Running);
    }

    #[test]
    fn a_latch_created_outside_the_app_kills_it() {
        let d = dir("external");
        let latch = d.join(LATCH_FILE_NAME);
        let mut ks = KillSwitch::with_latch(&latch, 1.0);
        assert!(!ks.poll(2.0));
        std::fs::write(&latch, "").unwrap(); // e.g. `touch KILLED` from a terminal
        assert!(ks.poll(3.0));
        assert_eq!(ks.state(false), RuntimeState::Killed);
        assert_eq!(ks.record().unwrap().by, Principal::External);
        assert_eq!(ks.record().unwrap().reason, "The kill latch file was created outside the app.");
    }

    #[test]
    fn deleting_the_latch_outside_the_app_does_not_recover() {
        let d = dir("delete");
        let latch = d.join(LATCH_FILE_NAME);
        let mut ks = KillSwitch::with_latch(&latch, 1.0);
        ks.kill(Principal::Owner, "t", 1.0).unwrap();
        std::fs::remove_file(&latch).unwrap();
        ks.poll(2.0);
        assert!(ks.is_killed());
        assert!(latch.exists(), "the latch is written back");
    }

    #[test]
    fn an_unreadable_latch_location_fails_closed() {
        let d = dir("closed");
        // A path whose parent is a regular file cannot be checked: ENOTDIR.
        let file = d.join("not-a-dir");
        std::fs::write(&file, "x").unwrap();
        let ks = KillSwitch::with_latch(file.join(LATCH_FILE_NAME), 1.0);
        assert_eq!(ks.state(false), RuntimeState::Killed);
    }

    #[test]
    fn a_kill_that_cannot_be_saved_still_kills_and_says_so() {
        let d = dir("unsaved");
        let file = d.join("not-a-dir");
        std::fs::write(&file, "x").unwrap();
        let mut ks = KillSwitch::in_memory();
        ks.latch = Some(file.join(LATCH_FILE_NAME));
        let err = ks.kill(Principal::Owner, "t", 1.0).unwrap_err();
        assert!(err.contains("would not stay killed"));
        assert!(ks.is_killed());
        assert!(ks.latch_error().is_some());
    }

    #[test]
    fn a_second_kill_keeps_the_first_record_and_ends_recovery() {
        let mut ks = KillSwitch::in_memory();
        let _ = ks.kill(Principal::Owner, "first", 1.0);
        ks.begin_recovery(Principal::Owner).unwrap();
        let _ = ks.kill(Principal::Model, "second", 2.0);
        assert_eq!(ks.state(false), RuntimeState::Killed);
        assert_eq!(ks.record().unwrap().reason, "first");
    }
}
