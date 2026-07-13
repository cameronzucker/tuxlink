//! Guards the filesystem-vs-registry boundary.
//!
//! `BUNDLED_TOPICS` is hand-maintained while `src/help/topics.ts` auto-globs via
//! `import.meta.glob`. That asymmetry let docs/user-guide/36-off-air-space-weather.md
//! exist on disk, render in the sidebar, and be absent from the FTS index — so
//! `docs_search` could not find it. A test comparing the registry against ITSELF
//! (`len == TOPICS.len()`) can never catch that. This one crosses the boundary.

use crate::search::docs_bundle::BUNDLED_TOPICS;
use crate::search::docs_index::DocSource;
use std::collections::HashSet;
use std::path::PathBuf;

/// Repo root, derived from the crate manifest dir (`src-tauri/`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn dir_for(source: DocSource) -> PathBuf {
    let sub = match source {
        DocSource::UserGuide => "docs/user-guide",
        DocSource::Knowledge => "docs/knowledge",
        DocSource::McpKnowledge => "docs/mcp-knowledge",
    };
    repo_root().join(sub)
}

/// Every `.md` on disk in every indexed source dir must be registered.
#[test]
fn every_markdown_file_on_disk_is_registered() {
    let registered: HashSet<String> = BUNDLED_TOPICS
        .iter()
        .map(|t| t.slug.to_string())
        .collect();

    let mut missing: Vec<String> = Vec::new();

    for source in [DocSource::UserGuide, DocSource::Knowledge, DocSource::McpKnowledge] {
        let dir = dir_for(source);
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("utf-8 file stem")
                .to_string();
            if !registered.contains(&stem) {
                missing.push(format!("{}/{stem}.md", dir.display()));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "these markdown files exist on disk but are NOT in BUNDLED_TOPICS, so they are \
         absent from docs_fts and unfindable by docs_search/docs_read:\n  {}",
        missing.join("\n  ")
    );
}

/// Slugs are the retrieval key; duplicates would make `docs_read` ambiguous.
#[test]
fn registered_slugs_are_unique() {
    let mut seen = HashSet::new();
    for t in BUNDLED_TOPICS {
        assert!(seen.insert(t.slug), "duplicate slug in BUNDLED_TOPICS: {}", t.slug);
    }
}
