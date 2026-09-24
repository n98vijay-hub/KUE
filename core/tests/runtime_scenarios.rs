//! Kill switch: end-to-end scenarios through the public API.
//!
//! KUE invariant 3: "LLM cannot disable the kill switch." And from the brief:
//! "KUE must NOT automatically restart after a kill. Recovery must require
//! explicit user action."

use lantern_core::config::Config;
use lantern_core::engine::Engine;
use lantern_core::privacy::Firewall;
use lantern_core::pump::{MemoryPump, MemoryTick};
use lantern_core::runtime::{Principal, Refusal, RuntimeState, LATCH_FILE_NAME};
use lantern_core::sensor::*;
use lantern_core::store::Store;
use std::path::PathBuf;

fn latch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kue-rt-{tag}-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(LATCH_FILE_NAME)
}

fn observing_engine(latch_path: &PathBuf) -> Engine {
    let mut e = Engine::new(Config::default_config(), "test".into());
    assert_eq!(e.attach_kill_latch(latch_path, 0.0), RuntimeState::Running);
    e.set_sensing_process_up(true, 0.0);
    e.ingest(SensorMessage::Status {
        camera: CameraStatus { state: "RUNNING".into(), permission: "AUTHORIZED".into(), ..Default::default() },
        sensing_active: true, computer_sampling_active: Some(true),
        enrollment: EnrollmentStats::default(),
        microphone_permission: None,
    }, 0.0);
    e
}

fn computer(t: f64) -> SensorMessage {
    SensorMessage::Computer {
        ts: t, frontmost_app: FrontmostApp { name: Some("Safari".into()), bundle_id: Some("com.apple.Safari".into()) },
        idle_seconds: 1.0,
    }
}

#[test]
fn killing_discards_readings_and_refuses_new_ones() {
    let l = latch("discard");
    let mut e = observing_engine(&l);
    e.ingest(computer(1.0), 1.0);
    assert_eq!(e.build_context(1.0).computer.frontmost_app.as_deref(), Some("Safari"));

    e.kill(Principal::Owner, "test", 2.0).unwrap();
    assert_eq!(e.build_context(2.0).computer.frontmost_app, None);
    e.ingest(computer(3.0), 3.0);
    let ctx = e.build_context(3.0);
    assert_eq!(ctx.computer.frontmost_app, None, "a reading after the kill is dropped");
    assert_eq!(ctx.runtime.state, RuntimeState::Killed);
    assert_eq!(ctx.runtime.killed_by, Some(Principal::Owner));
}

#[test]
fn resume_cannot_undo_a_kill() {
    let l = latch("resume");
    let mut e = observing_engine(&l);
    e.kill(Principal::Owner, "test", 1.0).unwrap();
    e.set_paused(false, 2.0);
    assert_eq!(e.runtime_state(), RuntimeState::Killed);
    e.ingest(computer(3.0), 3.0);
    assert_eq!(e.build_context(3.0).computer.frontmost_app, None);
}

#[test]
fn a_model_can_kill_but_never_recover() {
    let l = latch("model");
    let mut e = observing_engine(&l);
    e.kill(Principal::Model, "the model asked to stop", 1.0).unwrap();
    assert_eq!(e.begin_recovery(Principal::Model, 2.0), Err(Refusal::NotOwner(Principal::Model)));
    assert_eq!(e.complete_recovery(Principal::Model, 2.0), Err(Refusal::NotOwner(Principal::Model)));
    assert_eq!(e.begin_recovery(Principal::Automation, 2.0), Err(Refusal::NotOwner(Principal::Automation)));
    assert_eq!(e.runtime_state(), RuntimeState::Killed);
}

#[test]
fn a_relaunch_after_a_kill_comes_back_killed_and_observes_nothing() {
    let l = latch("relaunch");
    {
        let mut e = observing_engine(&l);
        e.kill(Principal::Owner, "before quitting", 1.0).unwrap();
    }
    let mut relaunched = Engine::new(Config::default_config(), "test".into());
    assert_eq!(relaunched.attach_kill_latch(&l, 50.0), RuntimeState::Killed);
    relaunched.set_sensing_process_up(true, 50.0);
    relaunched.ingest(computer(51.0), 51.0);
    let ctx = relaunched.build_context(51.0);
    assert_eq!(ctx.computer.frontmost_app, None);
    assert_eq!(ctx.runtime.reason.as_deref(), Some("before quitting"));
}

#[test]
fn nothing_is_written_to_local_memory_while_killed() {
    let l = latch("memory");
    let mut e = observing_engine(&l);
    let store = Store::open_in_memory(1000, 1000).unwrap();
    let mut fw = Firewall::new();
    let mut pump = MemoryPump::new(1.0);

    e.ingest(computer(1.0), 1.0);
    assert_eq!(pump.tick(&mut e, &mut fw, &store, 2.0).unwrap(), MemoryTick::Written);
    let (events, snaps) = (store.event_count().unwrap(), store.snapshot_count().unwrap());
    assert!(events > 0 && snaps > 0, "control: an observing engine does write");

    e.kill(Principal::Owner, "t", 3.0).unwrap();
    for i in 0..10 {
        let t = 4.0 + i as f64 * 2.0;
        e.ingest(computer(t), t);
        assert_eq!(pump.tick(&mut e, &mut fw, &store, t).unwrap(), MemoryTick::BlockedByKillSwitch);
    }
    e.begin_recovery(Principal::Owner, 30.0).unwrap();
    assert_eq!(pump.tick(&mut e, &mut fw, &store, 31.0).unwrap(), MemoryTick::BlockedByKillSwitch);
    assert_eq!((store.event_count().unwrap(), store.snapshot_count().unwrap()), (events, snaps));

    e.complete_recovery(Principal::Owner, 32.0).unwrap();
    assert_eq!(pump.tick(&mut e, &mut fw, &store, 33.0).unwrap(), MemoryTick::Written);
    let history = store.history(10).unwrap();
    assert!(history.iter().any(|h| h.summary.starts_with("Recovered by the owner")),
        "the recovery, carrying the kill record, is what gets written");
    assert!(!history.iter().any(|h| h.summary.starts_with("KILLED")),
        "nothing from inside the killed period is written");
}

#[test]
fn recovery_leaves_kue_running_and_the_latch_gone() {
    let l = latch("recover");
    let mut e = observing_engine(&l);
    e.kill(Principal::Owner, "t", 1.0).unwrap();
    assert!(l.exists());
    e.begin_recovery(Principal::Owner, 2.0).unwrap();
    assert_eq!(e.runtime_state(), RuntimeState::Recovering);
    e.complete_recovery(Principal::Owner, 3.0).unwrap();
    assert_eq!(e.runtime_state(), RuntimeState::Running);
    assert!(!l.exists());
    e.ingest(computer(4.0), 4.0);
    assert_eq!(e.build_context(4.0).computer.frontmost_app.as_deref(), Some("Safari"));
}
