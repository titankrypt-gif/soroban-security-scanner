//! Golden JSON and SARIF snapshots for the regression corpus (#66).
//!
//! Every corpus positive fixture is rendered through the same public report
//! functions the CLI uses. The committed files under `fixtures/snapshots/` are
//! compared byte-for-byte. Regenerate them deliberately:
//!
//! ```bash
//! UPDATE_SNAPSHOTS=1 cargo test -p soroban-scan-core --test snapshots
//! ```
//!
//! Review the diff before committing: a snapshot change is a machine-readable
//! contract change, not a formatting accident.

use std::path::{Path, PathBuf};

use soroban_scan_core::config::ScanConfig;
use soroban_scan_core::engine;
use soroban_scan_core::registry::RuleRegistry;
use soroban_scan_core::report;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn update_requested() -> bool {
    matches!(
        std::env::var("UPDATE_SNAPSHOTS").as_deref(),
        Ok("1") | Ok("true")
    )
}

fn check_or_update(directory: &Path, name: &str, actual: &str) -> bool {
    let path = directory.join(name);
    if update_requested() {
        std::fs::create_dir_all(directory).expect("create snapshot directory");
        std::fs::write(&path, actual).expect("write snapshot");
        return true;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("missing snapshot {}: {error}", path.display()));
    assert_eq!(
        expected,
        actual,
        "snapshot {} changed; review the report diff and regenerate with \
         UPDATE_SNAPSHOTS=1 if the change is intentional",
        path.display()
    );
    true
}

#[test]
fn corpus_json_and_sarif_snapshots_match() {
    let registry = RuleRegistry::with_default_rules();
    let config = ScanConfig::default();
    let corpus = fixtures().join("corpus");
    let snapshots = fixtures().join("snapshots");
    let mut checked = 0;

    for metadata in registry.metadata_sorted() {
        let directory = corpus.join(metadata.id);
        let (json, sarif) = if metadata.id == "SS-015" {
            let manifest = std::fs::read_to_string(directory.join("positive.toml"))
                .unwrap_or_else(|_| panic!("missing positive.toml for {}", metadata.id));
            let outcome = engine::scan_manifest_str("Cargo.toml", &manifest, &config, &registry)
                .expect("manifest fixture must parse");
            (
                report::render_json(&outcome).expect("render JSON"),
                report::render_sarif(&outcome, &registry),
            )
        } else {
            let source = std::fs::read_to_string(directory.join("positive.rs"))
                .unwrap_or_else(|_| panic!("missing positive.rs for {}", metadata.id));
            let file_name = format!("{}/positive.rs", metadata.id);
            let outcome = engine::scan_source_str(&source, &file_name, &config, &registry);
            (
                report::render_json(&outcome).expect("render JSON"),
                report::render_sarif(&outcome, &registry),
            )
        };

        check_or_update(&snapshots, &format!("{}.json", metadata.id), &json);
        check_or_update(&snapshots, &format!("{}.sarif", metadata.id), &sarif);
        checked += 1;
    }

    assert_eq!(checked, 18, "expected snapshots for all built-in rules");
}
