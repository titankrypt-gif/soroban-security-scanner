//! Enforces crate and module dependency boundaries (#76).
//!
//! The documented "may depend on" column in
//! `docs/architecture/workspace-layout.md` is the source of truth: every
//! production `use crate::<module>` edge must be allowed by that table. Test
//! modules and the test-only `rules::test_support` helper are excluded.
//!
//! Deliberately failing examples (do not uncomment):
//! ```ignore
//! // In soroban-scan-core/src/lib.rs or any core module:
//! use soroban_scan_cli::main as cli;
//! // In soroban-scan-core/src/engine.rs:
//! use crate::report::render;
//! ```
//! The first would make `core_does_not_depend_on_cli_crate` fail; the second
//! would make `module_dependencies_follow_the_documented_direction` fail.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn core_modules() -> BTreeSet<String> {
    read("crates/soroban-scan-core/src/lib.rs")
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("pub mod ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(str::to_string)
        })
        .collect()
}

fn documented_allowed_edges(
    layout: &str,
    modules: &BTreeSet<String>,
) -> BTreeMap<String, BTreeSet<String>> {
    let section = layout
        .split("## Top-level modules (`soroban-scan-core`)")
        .nth(1)
        .expect("workspace layout must contain the top-level module table")
        .split("## Subsystem submodules")
        .next()
        .expect("top-level module section must end before subsystem submodules");

    let mut allowed = BTreeMap::new();
    for line in section.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let columns: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if columns.len() < 3 {
            continue;
        }
        let owner = columns[0].trim_matches('`');
        if !modules.contains(owner) {
            continue;
        }
        let deps = columns[2]
            .split('`')
            .skip(1)
            .step_by(2)
            .map(str::trim)
            .filter(|candidate| modules.contains(*candidate))
            .map(str::to_string)
            .collect();
        allowed.insert(owner.to_string(), deps);
    }
    allowed
}

fn collect_rust_files(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
    {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn scan_crate_edges(modules: &BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let src = repo_root().join("crates/soroban-scan-core/src");
    let mut files = Vec::new();
    collect_rust_files(&src, &mut files);

    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in files {
        let relative = path.strip_prefix(&src).expect("file under src");
        let first = relative
            .components()
            .next()
            .expect("non-empty relative path")
            .as_os_str()
            .to_string_lossy()
            .to_string();
        let owner = first.trim_end_matches(".rs").to_string();
        if owner == "lib" || !modules.contains(&owner) {
            continue;
        }
        // `rules::test_support` is declared under `#[cfg(test)]`.
        if relative.file_name().and_then(|name| name.to_str()) == Some("test_support.rs") {
            continue;
        }

        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        // Test modules live at the end of the file in this crate; ignoring the
        // test half keeps test-only helpers from being treated as production
        // dependencies. Inline `#[cfg(test)]` blocks are treated the same way.
        let production = source.split("#[cfg(test)]").next().unwrap_or("");
        for line in production.lines() {
            let code = line.split("//").next().unwrap_or("");
            scan_code(code, &owner, modules, &mut edges);
        }
    }
    edges
}

fn scan_code(
    code: &str,
    owner: &str,
    modules: &BTreeSet<String>,
    edges: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let mut cursor = 0;
    while let Some(offset) = code[cursor..].find("crate::") {
        let start = cursor + offset + "crate::".len();
        let rest = &code[start..];
        let candidate: String = rest
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if candidate.is_empty() {
            cursor = start;
            continue;
        }
        if candidate != owner && modules.contains(&candidate) {
            edges
                .entry(owner.to_string())
                .or_default()
                .insert(candidate.clone());
        }
        cursor = start + candidate.len();
    }

    // `use crate::{finding, model};` has no repeated `crate::` prefix.
    if let Some(offset) = code.find("crate::{") {
        let rest = &code[offset + "crate::{".len()..];
        if let Some(end) = rest.find('}') {
            for item in rest[..end].split(',') {
                let candidate = item
                    .trim()
                    .trim_start_matches("self::")
                    .split("::")
                    .next()
                    .unwrap_or("")
                    .trim();
                if candidate != owner && modules.contains(candidate) {
                    edges
                        .entry(owner.to_string())
                        .or_default()
                        .insert(candidate.to_string());
                }
            }
        }
    }
}

#[test]
fn core_does_not_depend_on_cli_crate() {
    let cargo = read("crates/soroban-scan-core/Cargo.toml");
    for forbidden in ["soroban-scan-cli", "xtask"] {
        assert!(
            !cargo.contains(forbidden),
            "soroban-scan-core/Cargo.toml must not depend on {forbidden}"
        );
    }
}

#[test]
fn module_dependencies_follow_the_documented_direction() {
    let modules = core_modules();
    assert!(!modules.is_empty(), "expected pub modules in src/lib.rs");
    let layout = read("docs/architecture/workspace-layout.md");
    let allowed = documented_allowed_edges(&layout, &modules);
    assert_eq!(
        allowed.len(),
        modules.len(),
        "workspace-layout.md must document every public module"
    );

    let edges = scan_crate_edges(&modules);
    for (owner, dependencies) in &edges {
        for dependency in dependencies {
            assert!(
                allowed
                    .get(owner)
                    .is_some_and(|deps| deps.contains(dependency)),
                "{owner} uses crate::{dependency}, but workspace-layout.md does not \
                 allow that edge; update the module or the documented direction"
            );
        }
    }

    // The analysis pipeline is one-way: rules and the engine must not reach
    // into the renderers.
    for owner in ["engine", "rules"] {
        assert!(
            !edges.get(owner).is_some_and(|deps| deps.contains("report")),
            "{owner} must not depend on report"
        );
    }
}
