//! What KUE actually sends the on-device model, and how big each part is.
//!
//! Measured because 81 % of an answer's wall time was spent before the first
//! token (13,893 ms of prefill against 3,353 ms of generation, 2026-09-20), and
//! prefill cost is driven by how much the model has to read. This prints the
//! parts so the next change is aimed rather than guessed.
//!
//!     cargo run -p lantern-core --example prompt_size

use lantern_core::config::Config;
use lantern_core::privacy::{Firewall, MODEL_INSTRUCTIONS};
use lantern_core::sensor::*;
use lantern_core::Engine;

fn main() {
    let mut e = Engine::new(Config::default_config(), "measure".into());
    e.set_sensing_process_up(true, 0.0);
    e.ingest(SensorMessage::Status {
        camera: CameraStatus { state: "RUNNING".into(), permission: "AUTHORIZED".into(),
                               device_name: Some("FaceTime HD Camera".into()), device_id: Some("d".into()), detail: None },
        sensing_active: true, computer_sampling_active: Some(true),
        enrollment: EnrollmentStats { sample_count: 8, created_at: Some(0.0), feature_print_revision: Some(1),
            geometry_self_p95: Some(0.18), geometry_self_mean: Some(0.10), geometry_self_max: Some(0.18),
            feature_print_self_p95: Some(0.13), feature_print_self_mean: Some(0.09), feature_print_self_max: Some(0.13),
            yaw_spread_deg: Some(21.0), pitch_spread_deg: Some(13.9) },
        microphone_permission: Some("AUTHORIZED".into()),
    }, 1.0);
    e.ingest(SensorMessage::Computer { ts: 1.0,
        frontmost_app: FrontmostApp { name: Some("Safari".into()), bundle_id: Some("com.apple.Safari".into()) },
        idle_seconds: 3.0 }, 1.0);

    let ctx = e.build_context(2.0);
    let mut fw = Firewall::new();
    let cleared = fw.clear_model_context(&ctx, &[], "what is 17 percent of 840", 2.0)
        .expect("the firewall cleared a prompt");
    let p = cleared.value();

    let instr = MODEL_INSTRUCTIONS.len();
    let body = p.prompt.len();
    let caps = p.context.capabilities.as_ref().map(|v| v.join("; ").len()).unwrap_or(0);
    let events = p.context.recent_events.as_ref().map(|v| v.join(" | ").len()).unwrap_or(0);

    println!("instructions      {instr:>6} chars  (~{} tokens)", instr / 4);
    println!("prompt body       {body:>6} chars  (~{} tokens)", body / 4);
    println!("  of which:");
    println!("    capabilities  {caps:>6} chars  (~{} tokens)", caps / 4);
    println!("    recent events {events:>6} chars");
    println!("total sent        {:>6} chars  (~{} tokens)", instr + body, (instr + body) / 4);
    println!("withheld kinds    {:>6}", p.withheld.len());
    println!();
    println!("--- the prompt, as the model receives it ---");
    println!("{}", p.prompt);
}
