//! Resolves app names against the apps installed on this Mac, as KUE does.
//!   cargo run -p lantern-core --example resolve_app -- Chrome Microsoft Photoshop
use lantern_core::apps::AppCatalog;

fn main() {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let catalog = AppCatalog::scan(&AppCatalog::default_dirs(&home));
    println!("{} installed apps", catalog.len());
    for name in std::env::args().skip(1) {
        println!("{name:>12} → {}", serde_json::to_string(&catalog.resolve(&name)).unwrap());
    }
}
