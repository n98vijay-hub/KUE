//! Where a sentence goes, and whether it reaches the model.
//!
//!     cargo run -q -p lantern-core --example route -- "put a reminder for my exam"
//!
//! Built because a request KUE cannot do — "I have an exam tomorrow, put a
//! reminder" — was observed taking ~30 s and then saying it was not
//! implemented. Anything KUE cannot do should be refused by rule, in
//! milliseconds, naming the nearest thing it can do.

use lantern_core::intent::{classify, status, Understanding, Work};
use lantern_core::voice::InputSource;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sentences: Vec<String> = if args.is_empty() {
        [
            "I have an exam tomorrow. Put a reminder.",
            "remind me to call the dentist",
            "put this on my calendar",
            "what's on my calendar tomorrow",
            "what is 17 percent of 840",
            "what can you do",
            "check my storage",
            "open Safari",
            "search the web for rust tutorials",
            "type hello into Notes",
            "I need to finish this proposal",
        ].iter().map(|s| s.to_string()).collect()
    } else { args };

    for s in sentences {
        let t0 = std::time::Instant::now();
        let u = classify(&s, InputSource::Text, None);
        let micros = t0.elapsed().as_micros();
        match u {
            Understanding::Refused(r) => println!("{:<44} REFUSED BY SAFETY  ({micros} µs)  {}", s, r.reason),
            Understanding::Understood(i) => {
                let reaches_model = matches!(i.work, Work::Model | Work::ModelPlan);
                let what = match &i.work {
                    Work::Model => "→ THE MODEL".to_string(),
                    Work::ModelPlan => "→ THE MODEL, for a plan".to_string(),
                    Work::Answer { source, .. } => format!("answered by rule ({source})"),
                    Work::Ask { .. } => "asks a question back".to_string(),
                    Work::Act(a) => format!("acts: {a:?}").chars().take(40).collect(),
                    Work::Goal(b) => format!("goal: {:?}", b.kind),
                    Work::CapabilityList => "capability list, no model".to_string(),
                };
                println!("{:<44} {:<18} {:<34} {:?} ({micros} µs){}",
                    s, format!("{:?}", i.kind), what,
                    status(&i, false, None),
                    if reaches_model { "   ⟵ slow path" } else { "" });
            }
        }
    }
}
