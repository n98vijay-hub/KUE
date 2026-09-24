//! Replays a recorded sensing stream through the real parser and engine.
//!
//!   cargo run -p lantern-core --example replay -- /tmp/sense.jsonl [--context]
//!
//! Record a stream with the sensing layer's self-test:
//!
//!   open sensing/bundle/LanternSense.app --args --selftest --duration 60 --out /tmp/sense.jsonl
//!
//! Every line is parsed exactly as the app parses it, and the engine's clock is
//! the timestamp carried by the stream, so a replay reproduces what the app
//! would have concluded. Nothing here touches a device or the local database.

use lantern_core::config::Config;
use lantern_core::sensor::{parse_line, SensorMessage};
use lantern_core::Engine;
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.iter().find(|a| !a.starts_with("--")).expect("usage: replay <stream.jsonl> [--context]");
    let print_context = args.iter().any(|a| a == "--context");
    let text = std::fs::read_to_string(path).expect("could not read stream");

    // The stream's own `hello` marks the sensing layer as up, exactly as in the app.
    let mut engine = Engine::new(Config::default_config(), "replay".into());

    let mut parsed: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut unparsed = Vec::new();
    let mut clock = 0.0_f64;

    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() { continue; }
        let Some(msg) = parse_line(line) else {
            unparsed.push((n + 1, line.chars().take(120).collect::<String>()));
            continue;
        };
        let kind = match &msg {
            SensorMessage::Hello { .. } => "hello",
            SensorMessage::Status { .. } => "status",
            SensorMessage::Perception { ts, .. } => { clock = *ts; "perception" }
            SensorMessage::Computer { ts, .. } => { clock = *ts; "computer" }
            SensorMessage::EnrollCaptured { .. } => "enrollCaptured",
            SensorMessage::Devices { .. } => "devices",
            SensorMessage::ProbeCaptured { .. } => "probeCaptured",
            SensorMessage::SeparationReport { .. } => "separationReport",
            SensorMessage::AnalysisFailed { ts, .. } => { clock = *ts; "analysisFailed" }
            SensorMessage::Pose { ts, .. } => { clock = *ts; "pose" }
            SensorMessage::Scene { ts, .. } => { clock = *ts; "scene" }
            SensorMessage::Health { .. } => "health",
            SensorMessage::SenseHealth { .. } => "senseHealth",
            SensorMessage::Voice { ts, .. } => { clock = *ts; "voice" }
            SensorMessage::Wake { ts, .. } => { clock = *ts; "wake" }
            SensorMessage::Transcript { .. } => "transcript",
            SensorMessage::Error { .. } => "error",
            SensorMessage::Pong { .. } => "pong",
        };
        *parsed.entry(kind).or_default() += 1;
        engine.ingest(msg, clock);
    }

    println!("parsed:   {parsed:?}");
    println!("unparsed: {}", unparsed.len());
    for (n, l) in unparsed.iter().take(10) {
        println!("  line {n}: {l}");
    }

    let events = engine.recent_events(100_000);
    let mut by_kind: BTreeMap<String, u32> = BTreeMap::new();
    for e in &events { *by_kind.entry(format!("{:?}", e.kind)).or_default() += 1; }
    println!("events:   {} total {by_kind:?}", events.len());
    for e in events.iter().rev() {
        println!("  {:>14.2}  {:<22} {}", e.ts, format!("{:?}", e.kind), e.summary);
    }

    if print_context {
        println!("{}", serde_json::to_string_pretty(&engine.build_context(clock)).unwrap());
    }
}
