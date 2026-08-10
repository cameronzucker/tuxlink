//! ADR 0027 parity-manifest enforcement (tuxlink-ybf9f) — test-only module.
//!
//! Prose could not hold the ADR 0025 parity invariant (the 2026-07-21
//! favorites/transport gaps); this module is the mechanical layer, same
//! posture as the destructive-git hook. It pins four invariants:
//!
//! 1. **Bidirectional completeness** — every command registered in
//!    `generate_handler!` is classified in `docs/parity/parity-manifest.json`
//!    and vice versa (a rename cannot leave a stale entry).
//! 2. **Mapping liveness** — every `mcp`/`mcp-field` path names a tool that
//!    actually exists in the router source.
//! 3. **Authority defense** — an agent path on an `operator-authority`
//!    command is a FAILURE (ADR 0024), generalizing the routines router's
//!    pinned-closed guard.
//! 4. **Tool budget** — the router's tool count equals the manifest's
//!    `tool_budget`; growing the tool surface requires editing the budget in
//!    the same PR (the operator-owned counter-ratchet against schema tax).
//!
//! Source-text parsing (not runtime reflection) is deliberate: the
//! registration list and `#[tool(name = "...")]` attributes are literal,
//! and parsing them keeps this module free of router-construction fixtures.
//! The frontend half (every `invoke('…')` literal must be classified) lives
//! in `src/parityManifest.test.ts`.

#![cfg(test)]

use std::collections::BTreeMap;

const MANIFEST: &str = include_str!("../../docs/parity/parity-manifest.json");
const LIB_RS: &str = include_str!("lib.rs");
const ROUTER_RS: &str = include_str!("../tuxlink-mcp-core/src/router.rs");
const AGENTS_GUIDE: &str = include_str!("../../docs/mcp-knowledge/agents-guide.md");

#[derive(serde::Deserialize)]
struct Manifest {
    tool_budget: usize,
    commands: BTreeMap<String, Entry>,
}

#[derive(serde::Deserialize)]
struct Entry {
    class: String,
    #[serde(default)]
    mcp: Option<String>,
    #[serde(default, rename = "mcp-field")]
    mcp_field: Option<String>,
    #[serde(default)]
    finding: Option<String>,
    #[serde(default)]
    pending: Option<String>,
}

fn manifest() -> Manifest {
    serde_json::from_str(MANIFEST).expect("parity-manifest.json parses")
}

/// Extract the registered command names from `generate_handler![...]`,
/// comment-stripped. Mirrors the manifest generator.
fn registered_commands() -> Vec<String> {
    let start = LIB_RS
        .find("generate_handler![")
        .expect("generate_handler! present in lib.rs")
        + "generate_handler![".len();
    // "])" not "]": a stray `]` inside a body comment must not truncate
    // the list (the closing of generate_handler![...] is always "])").
    let end = start
        + LIB_RS[start..]
            .find("])")
            .expect("generate_handler! closes");
    let body: String = LIB_RS[start..end]
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    body.split(',')
        .map(|c| c.trim().rsplit("::").next().unwrap_or("").to_string())
        .filter(|c| {
            !c.is_empty()
                && c.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
        .collect()
}

fn router_tools() -> Vec<String> {
    let mut tools = Vec::new();
    let mut rest = ROUTER_RS;
    while let Some(i) = rest.find("#[tool(") {
        rest = &rest[i + 7..];
        if let Some(n) = rest.find("name = \"") {
            let after = &rest[n + 8..];
            if let Some(e) = after.find('"') {
                // Only accept if the name = appears close to the attribute
                // head (same attribute, not a later one).
                if n < 200 {
                    tools.push(after[..e].to_string());
                }
            }
        }
    }
    tools.sort();
    tools.dedup();
    tools
}

#[test]
fn every_registered_command_is_classified_and_vice_versa() {
    let m = manifest();
    let mut registered = registered_commands();
    registered.sort();
    registered.dedup();

    let unclassified: Vec<_> = registered
        .iter()
        .filter(|c| !m.commands.contains_key(*c))
        .collect();
    assert!(
        unclassified.is_empty(),
        "commands registered but not classified in docs/parity/parity-manifest.json \
         (ADR 0027: every new command lands classified): {unclassified:?}"
    );

    let stale: Vec<_> = m
        .commands
        .keys()
        .filter(|c| !registered.contains(c))
        .collect();
    assert!(
        stale.is_empty(),
        "manifest entries with no registered command (renamed/removed?): {stale:?}"
    );
}

#[test]
fn capability_entries_carry_exactly_one_agent_path_and_terminal_classes_none() {
    let m = manifest();
    for (name, e) in &m.commands {
        let paths = [
            e.mcp.is_some(),
            e.mcp_field.is_some(),
            e.finding.is_some(),
            e.pending.is_some(),
        ]
        .iter()
        .filter(|p| **p)
        .count();
        match e.class.as_str() {
            "capability" => assert_eq!(
                paths, 1,
                "{name}: capability entries carry exactly one agent path"
            ),
            "chrome" | "presentation" => assert_eq!(
                paths, 0,
                "{name}: terminal class `{}` must not carry an agent path",
                e.class
            ),
            "operator-authority" => assert_eq!(
                paths, 0,
                "{name}: operator-authority MUST NOT gain an agent path — \
                 ADR 0024 authority parity is defended here; if you meant to \
                 expose this, the ADR conversation comes first"
            ),
            other => panic!("{name}: unknown class `{other}`"),
        }
    }
}

#[test]
fn mcp_paths_name_live_tools() {
    let m = manifest();
    let tools = router_tools();
    for (name, e) in &m.commands {
        if let Some(tool) = &e.mcp {
            assert!(
                tools.contains(tool),
                "{name}: mcp path `{tool}` is not a live router tool"
            );
        }
        if let Some(field_path) = &e.mcp_field {
            let tool = field_path.split('.').next().unwrap_or("");
            assert!(
                tools.contains(&tool.to_string()),
                "{name}: mcp-field path `{field_path}` names non-live tool `{tool}`"
            );
        }
    }
}

#[test]
fn pending_entries_carry_bd_ids() {
    let m = manifest();
    for (name, e) in &m.commands {
        if let Some(p) = &e.pending {
            assert!(
                p.starts_with("tuxlink-")
                    && p.len() > "tuxlink-".len()
                    && p["tuxlink-".len()..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.'),
                "{name}: pending value `{p}` is not a bd id — new gaps are \
                 tracked at birth (ADR 0027)"
            );
        }
    }
}

#[test]
fn terminal_classes_cannot_shadow_live_tools() {
    // Codex adrev 2026-07-21 P2: a chrome/presentation classification on a
    // command whose NAME is a live MCP tool hides real agent capability
    // from the manifest and skips its liveness check — the 2026-07-21
    // backfill shipped three of these (platform_info, get_wizard_completed,
    // session_log_snapshot). Terminal classes must not shadow live tools;
    // operator-authority is exempt HERE because its own anti-mapping test
    // already fails if someone maps it, and an authority command sharing a
    // tool name would be caught by review of that louder failure.
    let m = manifest();
    let tools = router_tools();
    for (name, e) in &m.commands {
        if matches!(e.class.as_str(), "chrome" | "presentation") {
            assert!(
                !tools.contains(name),
                "{name}: classified `{}` but a live MCP tool of the same                  name exists — classify as capability with an mcp path",
                e.class
            );
        }
    }
}

#[test]
fn tool_budget_matches_router() {
    let m = manifest();
    let tools = router_tools();
    assert_eq!(
        tools.len(),
        m.tool_budget,
        "router tool count ({}) != manifest tool_budget ({}) — growing the \
         tool surface is a deliberate, operator-visible act: edit the budget \
         in the same PR and justify the schema tax (ADR 0027)",
        tools.len(),
        m.tool_budget
    );
}

// ----- Agents-guide drift gate (tuxlink-9n4cr) -------------------------------
//
// The guide claims to describe "the full MCP tool surface"; before this gate
// it silently covered 56 of 92 (the entire routines tier was invisible) and
// asserted "these four taint" while the registry tainted five. The tool COUNT
// was already pinned above; these pin the guide's COVERAGE and the taint
// enumeration to the same registry source text.

/// The guide names every registered tool, backticked. No exemption list on
/// purpose: a tool worth registering is worth one line of agent-facing
/// documentation, in the same PR that adds it.
#[test]
fn agents_guide_names_every_router_tool() {
    let missing: Vec<String> = router_tools()
        .into_iter()
        .filter(|t| !AGENTS_GUIDE.contains(&format!("`{t}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/mcp-knowledge/agents-guide.md never names these registered \
         tools (backticked): {missing:?} — the guide claims the full surface, \
         so document each (a tier-list mention is enough) in the same PR"
    );
}

/// Every tainting tool (per the router's `TAINTING_TOOLS` const, which also
/// builds the server `instructions` string) is named in the guide, and the
/// stale closed-list phrasing that shipped wrong cannot come back.
#[test]
fn agents_guide_taint_enumeration_matches_registry() {
    for tool in tuxlink_mcp_core::router::TAINTING_TOOLS {
        assert!(
            AGENTS_GUIDE.contains(&format!("`{tool}`")),
            "tainting tool `{tool}` is not named in agents-guide.md — the \
             taint list shipped wrong once (routines_journal_get was the \
             undocumented fifth); keep the guide's enumeration in sync with \
             router::TAINTING_TOOLS"
        );
    }
    assert!(
        !AGENTS_GUIDE.contains("These four return"),
        "agents-guide.md has regressed to the stale 'These four' taint \
         phrasing — the registry taints {} tools; enumerate from \
         router::TAINTING_TOOLS",
        tuxlink_mcp_core::router::TAINTING_TOOLS.len()
    );
    // The const is the source the runtime instructions are BUILT from; if a
    // new `guard.taint(...)` call site lands without extending it, this
    // count check against the router source text catches the drift. Scoped
    // to the handler region — the router's own test module below
    // `#[cfg(test)]` taints fixtures freely and must not count.
    let handler_src = ROUTER_RS.split("#[cfg(test)]").next().unwrap_or(ROUTER_RS);
    let taint_call_sites = handler_src.matches(".taint(tuxlink_security::TaintReason::").count();
    assert_eq!(
        taint_call_sites,
        tuxlink_mcp_core::router::TAINTING_TOOLS.len(),
        "router.rs has {} `guard.taint(...)` call sites but TAINTING_TOOLS \
         lists {} tools — extend the const (and the guide) in the same change",
        taint_call_sites,
        tuxlink_mcp_core::router::TAINTING_TOOLS.len()
    );
}
