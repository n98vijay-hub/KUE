//! What KUE's storage pass finds on this Mac, without the app.
//!
//!   cargo run --example take_stock
//!
//! Reads names, sizes and dates in Desktop, Documents, Downloads and ~/KUE.
//! Opens no file, changes nothing. What it prints is what the window would show.

use lantern_core::actions::DocumentRoots;
use lantern_core::privacy::Firewall;
use lantern_core::storage::{self, StorageRequest};

fn main() {
    let home = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"));
    let roots = DocumentRoots::default_for_home(&home);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();

    let permit = Firewall::new()
        .clear_storage_inventory(StorageRequest { areas: roots.names() }, now)
        .expect("the policy allows an inventory for the window");
    let started = std::time::Instant::now();
    let inventory = storage::take_inventory(&roots, &permit, now);
    let report = storage::analyze(&inventory, None, Some("not measured by this example".into()), now);

    println!("{}", report.headline());
    for line in report.area_lines() { println!("  {line}"); }
    for line in report.limits() { println!("  ! {line}"); }
    println!("\n{}", report.finding());
    for category in report.categories() {
        println!("\n{}", category.heading());
        for c in report.candidates.iter().filter(|c| c.category == category).take(5) {
            println!("  {} — {}", c.name, storage::size_words(c.size));
            println!("     evidence: {}", c.evidence);
            println!("     {:?}: {}", c.basis, c.reason);
        }
    }
    println!("\nverification: {}", report.verification());
    println!("took {} ms", started.elapsed().as_millis());
}
