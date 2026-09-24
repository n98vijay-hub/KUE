//! Action targets through the privacy firewall.
//!
//! An app name, a link, a path, notification text or a matched document is
//! ACTION_TARGET data: shown to you in the action card, and nowhere else. These
//! tests follow a distinctive target through every path an action touches —
//! the document search, the interface, the events table on disk, and the
//! prompt a model receives — and check the ledger records each decision.

use lantern_core::actions::{ActionKind, ActionRecord, ActionState, DocumentQuery, DocumentRoots, Risk};
use lantern_core::config::Config;
use lantern_core::engine::Engine;
use lantern_core::privacy::{DataKind, Destination, Firewall};
use lantern_core::store::Store;

const SENTINEL: &str = "zq-target-sentinel";

fn every_kind_with_sentinel_targets() -> Vec<ActionKind> {
    let s = SENTINEL.to_string();
    vec![
        ActionKind::OpenApplication { name: s.clone() },
        ActionKind::CloseApplication { name: s.clone() },
        ActionKind::FocusApplication { name: s.clone() },
        ActionKind::OpenUrl { url: format!("https://{s}.example") },
        ActionKind::CreateDirectory { path: s.clone() },
        ActionKind::CreateFile { path: s.clone(), text: s.clone() },
        ActionKind::ReadPermittedFile { path: s.clone() },
        ActionKind::MovePermittedFile { from: s.clone(), to: s.clone() },
        ActionKind::ShowNotification { title: s.clone(), body: s.clone() },
        ActionKind::OpenDocument { query: s.clone(), path: Some(format!("/Users/x/Desktop/{s}.pdf")) },
    ]
}

fn record(action: ActionKind, state: ActionState) -> ActionRecord {
    ActionRecord {
        id: "a1".into(), source: "VOICE".into(), description: format!("Open {SENTINEL}"), action, risk: Risk::Medium,
        state, reason: Some(format!("{SENTINEL} no longer exists.")), verification: Some(format!("{SENTINEL} running")),
        output: Some(SENTINEL.into()), created_at: 0.0, updated_at: None, choices: vec![format!("/Users/x/Desktop/{SENTINEL}.pdf")], steps: vec![], awaiting_since: None, task: None, sentences: None,
    }
}

fn ledger_row<'a>(rows: &'a [serde_json::Value], kind: &str, dest: &str) -> Option<&'a serde_json::Value> {
    rows.iter().find(|r| r["kind"] == kind && r["destination"] == dest)
}

#[test]
fn action_targets_may_reach_the_interface_and_no_other_destination() {
    let mut fw = Firewall::new();
    assert!(fw.check(DataKind::ActionTarget, Destination::Interface, 0.0).is_allow());
    for dest in [Destination::LocalMemory, Destination::LocalModel, Destination::ExternalModel, Destination::DiagnosticLog] {
        assert!(!fw.check(DataKind::ActionTarget, dest, 0.0).is_allow(), "ACTION_TARGET -> {dest:?}");
    }
    // A document KUE would gather on its own is still refused everywhere.
    for dest in Destination::ALL {
        assert!(!fw.check(DataKind::DocumentName, dest, 0.0).is_allow(), "DOCUMENT_NAME -> {dest:?}");
    }
}

#[test]
fn a_document_search_runs_only_on_a_cleared_query_and_the_decision_is_on_the_ledger() {
    let home = std::env::temp_dir().join(format!("kue-action-privacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("Desktop")).unwrap();
    std::fs::write(home.join(format!("Desktop/{SENTINEL} resume.pdf")), b"x").unwrap();

    let mut fw = Firewall::new();
    let cleared = fw.clear_document_search(DocumentQuery::new("resume"), 1.0).expect("an owner request may be searched for display");
    assert_eq!(cleared.destination(), Destination::Interface);
    let found = DocumentRoots::default_for_home(&home).find(&cleared, 6);
    assert_eq!(found.len(), 1);

    let store = Store::open_in_memory(100, 100).unwrap();
    store.record_ledger(&fw.take_ledger(2.0).expect("the search decision was recorded")).unwrap();
    let rows = store.ledger().unwrap();
    let row = ledger_row(&rows, "ACTION_TARGET", "INTERFACE").expect("the search is on the audit ledger");
    assert_eq!(row["decision"], "ALLOW");
    assert_eq!(row["class"], "USER_APPROVAL_REQUIRED");
    let text = serde_json::to_string(&rows).unwrap();
    assert!(!text.contains("resume") && !text.contains(SENTINEL), "the ledger holds no query and no file name");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn records_for_the_interface_pass_the_firewall_and_are_ledgered() {
    let mut fw = Firewall::new();
    let recs = every_kind_with_sentinel_targets().into_iter().map(|k| record(k, ActionState::RequiresConfirmation)).collect::<Vec<_>>();
    let cleared = fw.clear_actions_for_interface(recs.clone(), 5.0).expect("the interface may show action targets");
    assert_eq!(cleared.destination(), Destination::Interface);
    assert_eq!(cleared.into_value().len(), recs.len());
    let ledger = serde_json::to_value(fw.take_ledger(6.0).unwrap().value()).unwrap();
    let rows = ledger.as_array().unwrap();
    assert_eq!(ledger_row(rows, "ACTION_TARGET", "INTERFACE").unwrap()["count"], 1, "one decision per clearance");
}

#[test]
fn what_memory_keeps_about_an_action_never_names_its_target() {
    let mut engine = Engine::new(Config::default_config(), "test".into());
    let store = Store::open_in_memory(1000, 100).unwrap();
    let mut fw = Firewall::new();
    let mut t = 1.0;
    for kind in every_kind_with_sentinel_targets() {
        for state in [ActionState::Succeeded, ActionState::Denied, ActionState::Failed] {
            let rec = record(kind.clone(), state);
            for summary in [rec.event_summary(), rec.refused_summary(), rec.unplanned_summary()] {
                assert!(!summary.contains(SENTINEL) && !summary.contains("example") && !summary.contains("Desktop"),
                    "{summary}");
                engine.record_action_event(summary, t);
                t += 1.0;
            }
        }
    }
    let events = engine.events_since(0);
    assert!(events.len() >= 30, "control: the action events were recorded");
    for ev in &events {
        if let Some(c) = fw.clear_event(ev, t) { store.record_event(&c).unwrap(); }
    }
    let stored = serde_json::to_string(&store.history(1000).unwrap()).unwrap();
    assert!(stored.contains("OPEN_DOCUMENT") && stored.contains("SHOW_NOTIFICATION"), "control: kinds are kept");
    assert!(!stored.contains(SENTINEL), "the events table holds an action target:\n{stored}");

    // Recent events are part of what a model is shown; the target is not among them.
    let prompt = fw.clear_model_context(&engine.build_context(t), &[], "What did you just do?", t).unwrap();
    assert!(prompt.value().prompt.contains("Action OPEN_DOCUMENT"), "control: action events reach the prompt");
    assert!(!prompt.value().prompt.contains(SENTINEL), "an action target reached the model prompt");
}
