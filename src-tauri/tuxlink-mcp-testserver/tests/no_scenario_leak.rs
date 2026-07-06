//! CI grep-gate (tuxlink-cnz5o, Rust Task 5): scenario harness code MUST NOT
//! leak into the production-linked crates.
//!
//! The sim-harness is additive + test-mode-gated: all scenario code lives in
//! `tuxlink-mcp-testserver` and `d3zwe`. `src-tauri/src` (the Tauri monolith) and
//! `src-tauri/tuxlink-mcp-core` are production-linked and must stay clean of the
//! scenario tokens below. This test walks those trees and fails if any forbidden
//! token appears — the mechanical enforcement of the "additive only" constraint,
//! run under the existing workspace `cargo test` with no CI-YAML change.

use std::fs;
use std::path::{Path, PathBuf};

/// Tokens that only ever appear in the scenario harness. If one shows up in a
/// prod crate, scenario code leaked across the boundary.
const FORBIDDEN: &[&str] = &[
    "TUXLINK_TEST_SCENARIO",
    "load_fixture",
    "resolve_scenario",
    "ScenarioStatus",
    "ScenarioStation",
    "fixture_json_schema",
];

/// Resolve `src-tauri/` from this crate's manifest dir
/// (`src-tauri/tuxlink-mcp-testserver`).
fn src_tauri_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testserver crate has a parent (src-tauri)")
        .to_path_buf()
}

/// Recursively collect every `.rs` file under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip build artifacts.
            if path.file_name().map(|n| n == "target").unwrap_or(false) {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

#[test]
fn scenario_code_absent_from_prod_crates() {
    let root = src_tauri_root();
    let prod_dirs = [root.join("src"), root.join("tuxlink-mcp-core").join("src")];

    let mut leaks: Vec<String> = Vec::new();
    for dir in &prod_dirs {
        let mut files = Vec::new();
        rust_files(dir, &mut files);
        for file in files {
            let contents = match fs::read_to_string(&file) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for token in FORBIDDEN {
                if contents.contains(token) {
                    leaks.push(format!("{}: contains `{token}`", file.display()));
                }
            }
        }
    }

    assert!(
        leaks.is_empty(),
        "scenario harness code leaked into prod crates:\n{}",
        leaks.join("\n")
    );
}
