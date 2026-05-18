# Task 2 Config Implementation — Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `src-tauri/src/config.rs` to the AMD-1 nested schema + drop `winlink_password_present` per AMD-11 + add `validate_identity`, `Config::validate`, `read_config`, and `write_config_atomic` so the wizard cluster impl (`tuxlink-ln3`) can compile against the post-AMD code surface.

**Architecture:** Single file (`src-tauri/src/config.rs`, ~250 LOC). Public surface = `Config` struct + 4 sub-structs + 3 enums + 3 typed error enums + 5 free functions (`validate_identity`, `validate_identity_describe`, `read_config`, `write_config_atomic`, `config_path`). All consumers use the qualified `tuxlink_lib::config::*` path; no top-level re-exports. Tests in `src-tauri/tests/config_test.rs` (~450 LOC, 34 tests). TDD throughout: red → impl → green → commit.

**Plan version:** v2 (post 3-round Claude plan-review-cycle; Codex R4 deferred per ChatGPT quota gotcha — see memory `feedback_codex_quota_gotcha`). Critical P0s applied: DRIFT-1 relocated from §2 (stale stub-assumption) to NEW §3; Rust syntax fixed in Phase 5 tests 20+21; preservation asserts added to refusal tests; RAII XdgGuard for env-var safety; EACCES test added for ConfigReadError::Io; word-boundary grep in Phase 7; testing-pitfalls.md sibling bd issue task added.

**Tech Stack:** Rust 2021, serde 1.x + serde_json 1.x, thiserror 1.x, tempfile 3.x (promoted to runtime), serial_test 3.x (dev-only — gates env-var-mutating tests).

**Spec of record:** [`docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md`](../specs/2026-05-18-task-2-config-impl-design.md) (commit a36233f, v2). When this plan and the spec disagree, the spec wins for design rationale; the plan wins for executable code (the plan inlines complete code; the spec is reference).

**Closes via deliverable:** PR that merges the implementation into `feat/v0.0.1`. Closes bd issue `tuxlink-4mt`.

---

## Mandatory Per-Task Preamble

BEFORE starting work on any task in this plan:

1. Read the skill at `.claude/skills/test-driven-development/` (or invoke `/test-driven-development`).
2. Read [`docs/pitfalls/testing-pitfalls.md`](../../pitfalls/testing-pitfalls.md).
3. Follow TDD: write failing test → run (verify RED) → write minimal implementation → run (verify GREEN) → commit.
4. Pick your moniker (run `python3 .claude/scripts/get_agent_moniker.py` from the repo root) and substitute it for `<SESSION-MONIKER>` in all commit templates below.

BEFORE marking any task complete:
1. Review your tests against [`docs/pitfalls/testing-pitfalls.md`](../../pitfalls/testing-pitfalls.md).
2. Verify test coverage of the fix (are error paths tested? edge cases?).
3. Run `cd src-tauri && cargo test --test config_test` and confirm green.
4. Commit before moving to the next task.

After every PHASE (logical group of tasks): carefully review the batch of work from multiple perspectives. Minimum three review rounds; if you still find substantive issues in the third review, keep going until no findings remain.

---

## Pipeline status

All phases ⬜ Pending; flip to ✅ as each commits.

| Phase | Status | Notes | Test count after phase |
|---|---|---|---|
| 0 — Setup + Cargo.toml | ⬜ Pending | First task; prerequisite for compile-checks | (no tests yet) |
| 1 — `validate_identity` + describe-helper | ⬜ Pending | Leaf function; no deps on rest of the file | 6 |
| 2 — Nested `Config` types | ⬜ Pending | Foundation for Phase 3-5 | 15 (6 + 9) |
| 3 — `Config::validate` | ⬜ Pending | Depends on Phase 2 | 21 (+6) |
| 4 — `read_config` + `ConfigReadError` | ⬜ Pending | Depends on Phase 2 + 3 | 27 (+6 including EACCES) |
| 5 — `write_config_atomic` + `ConfigWriteError` | ⬜ Pending | Depends on Phase 2 + 3; most complex | 34 (+7) |
| 6 — Pitfalls DRIFT-1 (§3) + plan body cite | ⬜ Pending | Docs only — DRIFT-1 lands as NEW §3 of implementation-pitfalls.md (NOT replacing the substantive §2 Safety-Stack Coordination) | 34 |
| 7 — Final verification + PR + sibling bd issue | ⬜ Pending | Gate to merge | 34 |

---

## Phase 0 — Setup + Cargo.toml

### Task 0.1: Verify worktree state + update Cargo.toml dependencies

**Files:**
- Read: `src-tauri/src/config.rs` (current shipped flat schema; baseline)
- Read: `src-tauri/src/lib.rs` (verify `pub mod config;` exists)
- Modify: `src-tauri/Cargo.toml` (add thiserror + promote tempfile + add serial_test)

- [ ] **Step 1: Verify branch and current config.rs state**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-4mt-task-2-config-impl
git status                                         # should show "On branch bd-tuxlink-4mt/task-2-config-impl"
wc -l src-tauri/src/config.rs                     # should be 54 (pre-AMD-1 flat schema)
grep -c 'winlink_password_present' src-tauri/src/config.rs    # should be 1 (the field still exists)
grep -c 'validate_identity' src-tauri/src/config.rs           # should be 0 (function does not exist)
grep -c 'write_config_atomic' src-tauri/src/config.rs         # should be 0 (function does not exist)
grep -c 'pub mod config;' src-tauri/src/lib.rs               # should be 1 (module already exposed)
```

If any of those don't match, STOP and investigate — the prerequisite state has drifted.

- [ ] **Step 2: Update Cargo.toml (ONLY the two dep tables; leave everything else)**

Read `src-tauri/Cargo.toml`. The file has these top-level sections: `[package]`, `[build-dependencies]`, `[dependencies]`, `[dev-dependencies]`, `[lib]`, `[[bin]]`. **DO NOT touch `[package]`, `[build-dependencies]`, `[lib]`, or `[[bin]]`.** Modify ONLY:

1. **In the existing `[dependencies]` table**: ADD two new lines below the existing entries (do not reorder or change existing entries):
   ```toml
   thiserror = "1"            # NEW — for ConfigValidationError + ConfigReadError + ConfigWriteError
   tempfile = "3"             # PROMOTED from [dev-dependencies] below
   ```

2. **In the existing `[dev-dependencies]` table**: REMOVE the line `tempfile = "3"` (it was promoted above), and ADD:
   ```toml
   serial_test = "3"          # NEW — gates env-var-mutating tests
   ```
   Final `[dev-dependencies]` content:
   ```toml
   [dev-dependencies]
   mockito = "1.5"
   serial_test = "3"
   ```

The other tables (`[package]`, `[build-dependencies]`, `[lib]`, `[[bin]]`) MUST remain byte-identical to before. Verify with `git diff src-tauri/Cargo.toml` — the diff should show ONLY adds inside `[dependencies]`, ONLY one removal + one add inside `[dev-dependencies]`, and nothing else.

- [ ] **Step 3: Verify the manifest parses (without dep resolution)**

```bash
cd src-tauri && cargo metadata --no-deps --format-version 1 > /dev/null 2>&1 && echo "TOML OK" || echo "TOML BROKEN"
```

Expected: `TOML OK`. This parses Cargo.toml's structure without resolving any deps (no network, no compile). If you see "duplicate key `dependencies`" or "manifest parse error" — fix the TOML structure before proceeding.

(Why not `cargo check`? `cargo check` triggers dep resolution + compile; on a fresh worktree it'll fail because thiserror/serial_test aren't in the registry cache, masking the actual goal of verifying TOML validity.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "$(cat <<'EOF'
build(config): tuxlink-4mt Phase 0 — add thiserror + tempfile + serial_test deps

thiserror (NEW, [dependencies]) — for ConfigValidationError + ConfigReadError +
ConfigWriteError variants. tempfile (PROMOTED to [dependencies]) — needed by
write_config_atomic's runtime path. serial_test (NEW, [dev-dependencies]) —
gates env-var-mutating tests against parallel-test races per spec §6 + R1 P1-6.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 1 — `validate_identity` + describe-helper (foundation)

### Task 1.1: Write failing tests for validate_identity + describe-helper

**Files:**
- Modify: `src-tauri/tests/config_test.rs` (will be REPLACED wholesale at end of Phase 2; for now we APPEND tests to the existing file)

- [ ] **Step 1: REPLACE the existing test file content**

Replace `src-tauri/tests/config_test.rs` entirely with this content (we'll add to it through later phases):

```rust
// Tests for tuxlink-4mt — see docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md §6
// for the full 24-test matrix and the design rationale.

use tuxlink_lib::config::{validate_identity, validate_identity_describe};

// ============================================================================
// Phase 1 — validate_identity + describe-helper (loose-validator rules)
// ============================================================================

#[test]
fn test_validate_identity_loose_rules_accept() {
    assert!(validate_identity("W4PHS"));
    assert!(validate_identity("W4PHS-7"));
    assert!(validate_identity("EOC-1"));
    assert!(validate_identity("BAOFENG-FM-01"));
    assert!(validate_identity("LabBench-3"));
    assert!(validate_identity("W"));                    // 1 char OK
    assert!(validate_identity(&"X".repeat(32)));        // exactly 32 chars OK
}

#[test]
fn test_validate_identity_loose_rules_reject_each_class() {
    assert!(!validate_identity(""), "empty rejected");
    assert!(!validate_identity("W4 PHS"), "internal whitespace rejected");
    assert!(!validate_identity(&"X".repeat(33)), ">32 chars rejected");
    assert!(!validate_identity("W4PHS\x07"), "non-ASCII-printable (BEL) rejected");
    assert!(!validate_identity("W4PHS\x7F"), "DEL rejected");
    assert!(!validate_identity("Ünïcödë"), "non-ASCII rejected");
}

#[test]
fn test_validate_identity_describe_returns_first_rule_violated() {
    // Rule order per spec §3.2: empty → ASCII → whitespace → length
    assert_eq!(validate_identity_describe(""), Some("must not be empty"));
    assert_eq!(validate_identity_describe("Ünï"), Some("must be ASCII-printable"));
    assert_eq!(validate_identity_describe("W4 PHS"), Some("must not contain whitespace"));
    assert_eq!(validate_identity_describe(&"X".repeat(33)), Some("must be ≤32 chars"));
}

#[test]
fn test_validate_identity_describe_precedence_multi_violation() {
    // Per plan-review R2 P2-3: test PRECEDENCE — inputs violating multiple rules
    // should produce the FIRST-rule error. R2 P1-3's actionable-error-first claim
    // is the load-bearing semantic; regression that swapped rule order (e.g., length
    // first) would pass single-violation tests but fail these.
    // 40-char string containing whitespace → whitespace fires before length.
    let ws_long: String = std::iter::repeat("X ").take(20).collect();
    assert_eq!(validate_identity_describe(&ws_long), Some("must not contain whitespace"),
        "whitespace check must fire before length check");
    // 40-char non-ASCII string → ASCII fires before length.
    let non_ascii_long: String = std::iter::repeat("Ü").take(40).collect();
    assert_eq!(validate_identity_describe(&non_ascii_long), Some("must be ASCII-printable"),
        "ASCII check must fire before length check");
}

#[test]
fn test_validate_identity_describe_returns_none_on_accept() {
    assert_eq!(validate_identity_describe("W4PHS"), None);
    assert_eq!(validate_identity_describe("EOC-1"), None);
    assert_eq!(validate_identity_describe(&"X".repeat(32)), None);
}

#[test]
fn test_validate_identity_consistency_with_describe() {
    // validate_identity == validate_identity_describe(s).is_none()
    for s in &["W4PHS", "EOC-1", "", "W4 PHS", "Ünï", &"X".repeat(33)] {
        let by_bool = validate_identity(s);
        let by_describe = validate_identity_describe(s).is_none();
        assert_eq!(by_bool, by_describe, "consistency violation for input {:?}", s);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail (RED)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | head -40
```

Expected: compile error — `validate_identity` and `validate_identity_describe` not found in `tuxlink_lib::config`. This is the expected red-stage failure.

- [ ] **Step 3: Implement `validate_identity` + describe-helper**

Replace `src-tauri/src/config.rs` entirely with this Phase 1 content (we'll grow it through later phases):

```rust
//! Tuxlink configuration types + validators + atomic-write surface.
//!
//! Spec: docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md
//! bd issue: tuxlink-4mt

// Phase 1: validate_identity + describe-helper.
// Phase 2 will add the nested Config struct + sub-structs + helpers.

/// Loose identity validator. Matches Express's `hs30.htm` "checked for basic syntax" semantics:
/// non-empty + ASCII-printable + no internal whitespace + ≤32 chars (in that order so the most
/// actionable error fires first). The CMS is authoritative for actual callsign / tactical-address
/// acceptance.
///
/// Returns `true` if `s` passes ALL rules; `false` otherwise. Use [`validate_identity_describe`]
/// to obtain the first-violated-rule slug for error synthesis.
pub fn validate_identity(s: &str) -> bool {
    validate_identity_describe(s).is_none()
}

/// Returns `Some(static-rule-slug)` for the FIRST rule violated, or `None` if input passes all rules.
/// Rule order: empty → ASCII → whitespace → length (most-actionable first per spec adrev R2 P1-3 + R4 P1-2).
pub fn validate_identity_describe(s: &str) -> Option<&'static str> {
    if s.is_empty() { return Some("must not be empty"); }
    if s.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) { return Some("must be ASCII-printable"); }
    if s.chars().any(char::is_whitespace) { return Some("must not contain whitespace"); }
    if s.chars().count() > 32 { return Some("must be ≤32 chars"); }
    None
}

/// Resolve the config file path. Honors XDG_CONFIG_HOME, falls back to
/// ~/.config/tuxlink/config.json.
pub fn config_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").expect("HOME must be set");
            std::path::PathBuf::from(home).join(".config")
        });
    base.join("tuxlink").join("config.json")
}
```

Note: the previous flat-schema `Config` struct + `deserialize_schema_version` + `deserialize_nonempty_string` helpers are REMOVED. This will break any callers that referenced the old `Config` — verify with `grep -rn 'use tuxlink_lib::config::Config' src-tauri/`:

```bash
grep -rn 'use tuxlink_lib::config' src-tauri/src/ src-tauri/tests/ 2>&1 || echo "no callers"
```

Expected: only the test file imports the new `validate_identity` items; no other source-side caller exists (verified during spec phase). `pat_process.rs` takes a `PathBuf`, not the `Config` struct.

- [ ] **Step 4: Run the tests to verify they pass (GREEN)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | tail -10
```

Expected output:
```
running 5 tests
test test_validate_identity_loose_rules_accept ... ok
test test_validate_identity_loose_rules_reject_each_class ... ok
test test_validate_identity_describe_returns_first_rule_violated ... ok
test test_validate_identity_describe_returns_none_on_accept ... ok
test test_validate_identity_consistency_with_describe ... ok

test result: ok. 5 passed; 0 failed
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/tests/config_test.rs
git commit -m "$(cat <<'EOF'
feat(config): tuxlink-4mt Phase 1 — validate_identity + describe-helper

Loose validator per AMD-1: nonempty + ASCII-printable + no whitespace + ≤32
chars (rule order: most-actionable-first per spec adrev R2 P1-3 + R4 P1-2).
Bool return matches shipped wizard spec line 151 consumer. Companion
validate_identity_describe returns the first-violated-rule slug for
Config::validate's structured error synthesis.

REMOVES the old flat-schema Config struct from config.rs; subsequent phases
rebuild the nested AMD-1 + AMD-11 shape. No source-side callers of the old
Config remain (pat_process.rs takes PathBuf, not Config).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2 — Nested `Config` types + sub-structs + `deny_unknown_fields` + drift defense

### Task 2.1: Write failing tests for nested deserialize + enum round-trips + drift defense

**Files:**
- Modify: `src-tauri/tests/config_test.rs` (APPEND tests for Phase 2)

- [ ] **Step 1: APPEND tests for Phase 2**

Add the following test block to the END of `src-tauri/tests/config_test.rs`:

```rust
// ============================================================================
// Phase 2 — Nested Config types + deserialize + AMD-11 drift defense
// ============================================================================

use tuxlink_lib::config::{
    Config, ConnectConfig, IdentityConfig, PrivacyConfig,
    CmsTransport, GpsState, PositionPrecision,
    CONFIG_SCHEMA_VERSION,
};

#[test]
fn test_deserialize_minimal_cms_config() {
    let json = r#"{
        "schema_version": 1,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": "EM75xx"},
        "privacy": {"gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid"},
        "pat_mbo_address": "W4PHS@winlink.org"
    }"#;
    let config: Config = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
    assert!(config.wizard_completed);
    assert!(config.connect.connect_to_cms);
    assert_eq!(config.connect.transport, CmsTransport::CmsSsl);
    assert_eq!(config.identity.callsign.as_deref(), Some("W4PHS"));
    assert!(config.identity.identifier.is_none());
    assert_eq!(config.identity.grid.as_deref(), Some("EM75xx"));
    assert_eq!(config.privacy.gps_state, GpsState::BroadcastAtPrecision);
    assert_eq!(config.privacy.position_precision, PositionPrecision::FourCharGrid);
    assert_eq!(config.pat_mbo_address.as_deref(), Some("W4PHS@winlink.org"));
}

#[test]
fn test_deserialize_offline_config() {
    let json = r#"{
        "schema_version": 1,
        "wizard_completed": true,
        "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
        "identity": {"callsign": null, "identifier": "EOC-1", "grid": "EM75"},
        "privacy": {"gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    let config: Config = serde_json::from_str(json).expect("offline config must deserialize");
    assert!(!config.connect.connect_to_cms);
    assert!(config.identity.callsign.is_none());
    assert_eq!(config.identity.identifier.as_deref(), Some("EOC-1"));
    assert!(config.pat_mbo_address.is_none());
}

#[test]
fn test_reject_wrong_schema_version() {
    let json = r#"{
        "schema_version": 99,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    let result: Result<Config, _> = serde_json::from_str(json);
    assert!(result.is_err(), "unexpected schema version must fail to deserialize");
}

#[test]
fn test_reject_amd11_dropped_field_winlink_password_present() {
    // Stale top-level field MUST be rejected by deny_unknown_fields on Config.
    // The pre-AMD-1 flat schema had winlink_password_present at the TOP LEVEL.
    let json = r#"{
        "schema_version": 1,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null,
        "winlink_password_present": true
    }"#;
    let result: Result<Config, _> = serde_json::from_str(json);
    assert!(result.is_err(), "AMD-11-dropped field at top level must hard-fail via deny_unknown_fields");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("winlink_password_present"),
        "error message must mention the stale field: {err}");
}

#[test]
fn test_deny_unknown_fields_on_each_substruct() {
    // Unknown field on ConnectConfig must fail.
    let json_connect = r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl", "extra_field": "x"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_connect).is_err(),
        "unknown field on ConnectConfig must fail");

    // Unknown field on IdentityConfig must fail.
    let json_id = r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null, "extra": "x"},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_id).is_err(),
        "unknown field on IdentityConfig must fail");

    // Unknown field on PrivacyConfig must fail.
    let json_priv = r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid", "extra": "x"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_priv).is_err(),
        "unknown field on PrivacyConfig must fail");
}

#[test]
fn test_cms_transport_both_variants_round_trip() {
    // Per plan-review R2 P2-2: iterate BOTH variants, not just Telnet.
    // CmsSsl is implicitly deserialized in many other tests but its SERIALIZE-AS-PascalCase
    // contract is unlocked without an explicit check.
    for (variant, name) in [
        (CmsTransport::CmsSsl, "CmsSsl"),
        (CmsTransport::Telnet, "Telnet"),
    ] {
        let json = format!(r#"{{
            "schema_version": 1, "wizard_completed": true,
            "connect": {{"connect_to_cms": true, "transport": "{}"}},
            "identity": {{"callsign": "W4PHS", "identifier": null, "grid": null}},
            "privacy": {{"gps_state": "Off", "position_precision": "FourCharGrid"}},
            "pat_mbo_address": null
        }}"#, name);
        let config: Config = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {name} must deserialize: {e}"));
        assert_eq!(config.connect.transport, variant);
        let out = serde_json::to_string(&config).unwrap();
        assert!(out.contains(&format!("\"{name}\"")),
            "serialized form must use PascalCase variant {name}: {out}");
    }
}

#[test]
fn test_gps_state_three_variants_round_trip() {
    for (variant, name) in [
        (GpsState::Off, "Off"),
        (GpsState::LocalUiOnly, "LocalUiOnly"),
        (GpsState::BroadcastAtPrecision, "BroadcastAtPrecision"),
    ] {
        let json = format!(r#"{{
            "schema_version": 1, "wizard_completed": true,
            "connect": {{"connect_to_cms": false, "transport": "CmsSsl"}},
            "identity": {{"callsign": null, "identifier": "X", "grid": null}},
            "privacy": {{"gps_state": "{}", "position_precision": "FourCharGrid"}},
            "pat_mbo_address": null
        }}"#, name);
        let config: Config = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {name} must deserialize: {e}"));
        assert_eq!(config.privacy.gps_state, variant);
        let out = serde_json::to_string(&config).unwrap();
        assert!(out.contains(&format!("\"{name}\"")), "serialize must use PascalCase: {out}");
    }
}

#[test]
fn test_position_precision_two_variants_round_trip() {
    for (variant, name) in [
        (PositionPrecision::FourCharGrid, "FourCharGrid"),
        (PositionPrecision::SixCharGrid, "SixCharGrid"),
    ] {
        let json = format!(r#"{{
            "schema_version": 1, "wizard_completed": true,
            "connect": {{"connect_to_cms": false, "transport": "CmsSsl"}},
            "identity": {{"callsign": null, "identifier": "X", "grid": null}},
            "privacy": {{"gps_state": "Off", "position_precision": "{}"}},
            "pat_mbo_address": null
        }}"#, name);
        let config: Config = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {name} must deserialize: {e}"));
        assert_eq!(config.privacy.position_precision, variant);
    }
}

#[test]
fn test_empty_string_identity_field_normalizes_to_none() {
    // Spec §3.1: deserialize_optional_nonempty_string maps "" → None.
    // This is the offline-mode-when-operator-types-then-clears case.
    let json = r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
        "identity": {"callsign": "", "identifier": "EOC-1", "grid": ""},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": ""
    }"#;
    let config: Config = serde_json::from_str(json).expect("must deserialize");
    assert!(config.identity.callsign.is_none(), "empty callsign should normalize to None");
    assert_eq!(config.identity.identifier.as_deref(), Some("EOC-1"));
    assert!(config.identity.grid.is_none(), "empty grid should normalize to None");
    assert!(config.pat_mbo_address.is_none(), "empty pat_mbo_address should normalize to None");
}
```

- [ ] **Step 2: Run the tests to verify they fail (RED)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | head -40
```

Expected: compile error — `Config`, `ConnectConfig`, `IdentityConfig`, etc. not found in `tuxlink_lib::config`.

- [ ] **Step 3: Implement Phase 2 — append to `src-tauri/src/config.rs`**

Add this block to `src-tauri/src/config.rs` ABOVE the existing `pub fn validate_identity` (place the Phase 2 types near the top of the file for readability; the validators stay where Phase 1 put them):

```rust
use serde::{Deserialize, Deserializer, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u32,
    pub wizard_completed: bool,
    pub connect: ConnectConfig,
    pub identity: IdentityConfig,
    pub privacy: PrivacyConfig,
    pub pat_mbo_address: Option<String>,
    // winlink_password_present REMOVED per AMD-11; deny_unknown_fields catches drift.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectConfig {
    /// Set by wizard Task 9. False = offline-only deployment.
    pub connect_to_cms: bool,
    /// Per the transport-visibility anti-pattern: always explicit, never auto-selected.
    pub transport: CmsTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CmsTransport {
    /// Port 8773, TLS-wrapped. v0.0.1 default.
    CmsSsl,
    /// Port 8772, plaintext. For networks blocking port 8773.
    Telnet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// Required when connect_to_cms = true (enforced by Config::validate).
    /// Loose validator per validate_identity(): nonempty + no whitespace + ≤32 + ASCII-printable.
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub callsign: Option<String>,
    /// Free-form station identifier for offline-mode operators. Same loose-validator rules.
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub identifier: Option<String>,
    /// Maidenhead grid, stored at full 6-char precision when known. Broadcast precision is
    /// governed by PrivacyConfig.position_precision (per Principle 7).
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GpsState {
    /// No GPS device read at all.
    Off,
    /// GPS read locally; never broadcast.
    LocalUiOnly,
    /// Default. GPS read + broadcast at the chosen precision.
    BroadcastAtPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PositionPrecision {
    /// Default. Broadcasts 4-char Maidenhead (~1°).
    FourCharGrid,
    /// Opt-in. Broadcasts full 6-char (~5km).
    SixCharGrid,
}

fn deserialize_schema_version<'de, D>(d: D) -> Result<u32, D::Error>
where D: Deserializer<'de>
{
    let v = u32::deserialize(d)?;
    if v != CONFIG_SCHEMA_VERSION {
        return Err(serde::de::Error::custom(format!(
            "unsupported config schema_version {} (expected {})",
            v, CONFIG_SCHEMA_VERSION
        )));
    }
    Ok(v)
}

fn deserialize_optional_nonempty_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where D: Deserializer<'de>
{
    // Maps JSON `null` → None; maps JSON `""` → None (treat empty-string as missing);
    // maps non-empty string → Some(s). Eliminates Some("") ambiguity per spec adrev R4 P1-1.
    let opt = <Option<String>>::deserialize(d)?;
    Ok(opt.filter(|s| !s.is_empty()))
}
```

- [ ] **Step 4: Run the tests to verify they pass (GREEN)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | tail -20
```

Expected: 14 tests pass (5 from Phase 1 + 9 new from Phase 2).

If any FAIL — investigate:
- `test_reject_amd11_dropped_field_winlink_password_present` failing: verify `Config` has `#[serde(deny_unknown_fields)]`.
- `test_cms_transport_telnet_variant_round_trips` failing on serialize: verify `CmsTransport` has `#[serde(rename_all = "PascalCase")]`.
- `test_empty_string_identity_field_normalizes_to_none` failing: verify each `Option<String>` field in `IdentityConfig` has the `#[serde(deserialize_with = ..., default)]` attribute.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/tests/config_test.rs
git commit -m "$(cat <<'EOF'
feat(config): tuxlink-4mt Phase 2 — nested Config types per AMD-1 + AMD-11 drift defense

Nested shape per AMD-1 (docs/design/v0.0.1-ux-mockups.md §6): ConnectConfig
governs CMS-vs-offline + explicit transport (CmsSsl/Telnet); IdentityConfig
holds callsign+identifier+grid (all optional, "" → None via custom deserializer
per spec adrev R4 P1-1); PrivacyConfig governs 3-state GPS + 2-state position
precision per Principle 7.

AMD-11 drift defense: deny_unknown_fields on every sub-struct hard-fails any
stale winlink_password_present field at deserialize time (test 10).

PascalCase serde rename on enums locks the wire format against future
good-taste snake_case refactors (per spec adrev R1 P1-1).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3 — `Config::validate` cross-field rules

### Task 3.1: Write failing tests + implement Config::validate

**Files:**
- Modify: `src-tauri/tests/config_test.rs` (APPEND Phase 3 tests)
- Modify: `src-tauri/src/config.rs` (APPEND `ConfigValidationError` + `impl Config::validate`)

- [ ] **Step 1: APPEND Phase 3 tests**

Add to the END of `src-tauri/tests/config_test.rs`:

```rust
// ============================================================================
// Phase 3 — Config::validate (cross-field rules)
// ============================================================================

use tuxlink_lib::config::ConfigValidationError;

fn make_config(
    connect_to_cms: bool,
    callsign: Option<&str>,
    identifier: Option<&str>,
) -> Config {
    // Helper: build a Config via deserialization to ensure all the deserialize
    // attributes (rename_all, deny_unknown_fields, etc.) are honored.
    // Empty-string → None happens via deserialize_optional_nonempty_string.
    let json = format!(r#"{{
        "schema_version": 1, "wizard_completed": false,
        "connect": {{"connect_to_cms": {}, "transport": "CmsSsl"}},
        "identity": {{
            "callsign": {},
            "identifier": {},
            "grid": null
        }},
        "privacy": {{"gps_state": "Off", "position_precision": "FourCharGrid"}},
        "pat_mbo_address": null
    }}"#,
        connect_to_cms,
        match callsign { Some(s) => format!("\"{s}\""), None => "null".into() },
        match identifier { Some(s) => format!("\"{s}\""), None => "null".into() },
    );
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("test fixture must deserialize: {e}\nJSON: {json}"))
}

#[test]
fn test_validate_cms_path_requires_callsign() {
    let config = make_config(true, None, None);
    let err = config.validate().unwrap_err();
    assert!(matches!(err, ConfigValidationError::CmsPathMissingCallsign));
}

#[test]
fn test_validate_offline_path_rejects_callsign() {
    let config = make_config(false, Some("W4PHS"), None);
    let err = config.validate().unwrap_err();
    assert!(matches!(err, ConfigValidationError::OfflinePathHasCallsign));
}

#[test]
fn test_validate_offline_with_identifier_only_accepts() {
    let config = make_config(false, None, Some("EOC-1"));
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_cms_with_callsign_accepts() {
    let config = make_config(true, Some("W4PHS"), None);
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_invalid_identity_propagates_field() {
    // Callsign with whitespace → InvalidIdentity { field: "callsign", rule: "must not contain whitespace" }
    // Note: deserialize_optional_nonempty_string accepts the non-empty whitespace-containing input;
    // we have to build the Config bypassing the deserializer to construct this case directly.
    let mut config = make_config(true, Some("W4PHS"), None);
    config.identity.callsign = Some("W4 PHS".into());
    let err = config.validate().unwrap_err();
    match err {
        ConfigValidationError::InvalidIdentity { field, rule } => {
            assert_eq!(field, "callsign");
            assert_eq!(rule, "must not contain whitespace");
        }
        other => panic!("expected InvalidIdentity, got {other:?}"),
    }

    // Identifier with bad char → InvalidIdentity { field: "identifier", rule: "must be ASCII-printable" }
    let mut config = make_config(false, None, Some("EOC-1"));
    config.identity.identifier = Some("EOC\x07".into());
    let err = config.validate().unwrap_err();
    match err {
        ConfigValidationError::InvalidIdentity { field, rule } => {
            assert_eq!(field, "identifier");
            assert_eq!(rule, "must be ASCII-printable");
        }
        other => panic!("expected InvalidIdentity, got {other:?}"),
    }
}

#[test]
fn test_validation_error_display_strings_stable() {
    // Per spec §3.1: Display strings are STABLE PUBLIC SURFACE for ALL THREE error enums
    // (ConfigValidationError, ConfigReadError, ConfigWriteError). The wizard interpolates
    // them into operator-visible messages via format!("{e}"). Any future change is a
    // breaking change for the wizard's UX tests. Plan-review R2 P0-2 + R3 P1-3 caught
    // earlier under-coverage (3 of 12 variants tested); v2 of this test covers all 3 enums.
    let e = ConfigValidationError::CmsPathMissingCallsign;
    assert_eq!(e.to_string(), "CMS path requires identity.callsign to be set");

    let e = ConfigValidationError::OfflinePathHasCallsign;
    assert_eq!(e.to_string(),
        "offline path must NOT have identity.callsign set (use identity.identifier instead)");

    let e = ConfigValidationError::InvalidIdentity { field: "callsign", rule: "must not be empty" };
    assert_eq!(e.to_string(), "invalid identity field `callsign`: must not be empty");
}
```

**ADDITIONAL Phase 3 test** — append to the test file in Phase 3 alongside `test_validation_error_display_strings_stable`. Will need imports from Phase 4 + Phase 5 (`ConfigReadError`, `ConfigWriteError`) — since this plan executes phases sequentially and these tests are added in the test file AFTER Phase 5 ships the variants, place this test in Phase 5 instead (at the same time the `ConfigWriteError` variants are added). Same logic for `ConfigReadError` after Phase 4.

(Implementer note: split the Display-stability tests across phases. Phase 3 ships the `ConfigValidationError` test above; Phase 4 ships a `ConfigReadError` Display test; Phase 5 ships a `ConfigWriteError` Display test. Each phase's test asserts only its phase's enum to keep phase-ordering compile-clean.)

- [ ] **Step 2: Run the tests to verify they fail (RED)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | head -30
```

Expected: compile error — `ConfigValidationError` not found; `Config::validate` not found.

- [ ] **Step 3: Append `ConfigValidationError` + `impl Config::validate` to `src-tauri/src/config.rs`**

Add at the end of `src-tauri/src/config.rs` (after the existing validators):

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("CMS path requires identity.callsign to be set")]
    CmsPathMissingCallsign,
    #[error("offline path must NOT have identity.callsign set (use identity.identifier instead)")]
    OfflinePathHasCallsign,
    #[error("invalid identity field `{field}`: {rule}")]
    InvalidIdentity { field: &'static str, rule: &'static str },
}

impl Config {
    /// Cross-field semantic validation (can't be expressed via serde deserialize-with).
    /// Callers (wizard's `wizard_persist_cms`, `read_config`) invoke after deserialization.
    /// NOT auto-called by `write_config_atomic` — caller responsibility per spec §3.3.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.connect.connect_to_cms && self.identity.callsign.is_none() {
            return Err(ConfigValidationError::CmsPathMissingCallsign);
        }
        if !self.connect.connect_to_cms && self.identity.callsign.is_some() {
            return Err(ConfigValidationError::OfflinePathHasCallsign);
        }
        if let Some(ref c) = self.identity.callsign {
            if let Some(rule) = validate_identity_describe(c) {
                return Err(ConfigValidationError::InvalidIdentity { field: "callsign", rule });
            }
        }
        if let Some(ref i) = self.identity.identifier {
            if let Some(rule) = validate_identity_describe(i) {
                return Err(ConfigValidationError::InvalidIdentity { field: "identifier", rule });
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass (GREEN)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | tail -10
```

Expected: 20 tests pass (5 from Phase 1 + 9 from Phase 2 + 6 new from Phase 3).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/tests/config_test.rs
git commit -m "$(cat <<'EOF'
feat(config): tuxlink-4mt Phase 3 — Config::validate + ConfigValidationError

Cross-field validation with BOTH orthogonality rules per AMD-1 (per spec
adrev R3 P0-2): CMS-requires-callsign AND offline-rejects-callsign. The
half-enforcement in earlier drafts would have reintroduced the exact
ambiguity AMD-1's nested shape eliminated.

InvalidIdentity carries {field, rule} (per spec adrev R2 P1-2) so the wizard's
WizardError::InvalidInput { field } mapping can preserve field identity.
Display strings declared STABLE PUBLIC SURFACE per spec §3.1.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 4 — `read_config` + `ConfigReadError`

### Task 4.1: Write failing tests + implement read_config

**Files:**
- Modify: `src-tauri/tests/config_test.rs` (APPEND Phase 4 tests with `#[serial]`)
- Modify: `src-tauri/src/config.rs` (APPEND `ConfigReadError` + `read_config`)

- [ ] **Step 1: APPEND Phase 4 tests**

Add to the END of `src-tauri/tests/config_test.rs`:

```rust
// ============================================================================
// Phase 4 — read_config + ConfigReadError
// ============================================================================

use tuxlink_lib::config::{read_config, ConfigReadError, config_path};

/// Helper: scope XDG_CONFIG_HOME to a fresh temp dir for the duration of `f`.
/// Uses RAII guard so prior env value is RESTORED even if `f` panics (per plan-review
/// R1 P1-1 + R2 P1-3 — panic during a test would otherwise orphan the env var and
/// cascade failures into subsequent tests). Use with #[serial_test::serial] to avoid
/// concurrent-process races.
struct XdgGuard {
    prior: Option<std::ffi::OsString>,
    _tmp: tempfile::TempDir,
}
impl Drop for XdgGuard {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(p) => std::env::set_var("XDG_CONFIG_HOME", p),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}

fn with_xdg_temp<R>(f: impl FnOnce(&std::path::Path) -> R) -> R {
    let tmp = tempfile::tempdir().expect("must create tempdir");
    let path = tmp.path().to_owned();
    let prior = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", &path);
    let _guard = XdgGuard { prior, _tmp: tmp };
    f(&path)
    // _guard drops here, restoring prior env value (even on panic from `f`)
}

#[test]
#[serial_test::serial]
fn test_read_config_not_found_returns_typed_error() {
    with_xdg_temp(|_| {
        let err = read_config().unwrap_err();
        assert!(matches!(err, ConfigReadError::NotFound { .. }));
    });
}

#[test]
#[serial_test::serial]
fn test_read_config_serde_returns_typed_error_on_malformed_json() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not valid json").unwrap();
        let err = read_config().unwrap_err();
        assert!(matches!(err, ConfigReadError::Serde { .. }));
    });
}

#[test]
#[serial_test::serial]
fn test_read_config_validation_runs_after_deserialize() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Valid JSON shape but offline-with-callsign — should fail validation.
        std::fs::write(&path, r#"{
            "schema_version": 1, "wizard_completed": true,
            "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
            "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
            "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
            "pat_mbo_address": null
        }"#).unwrap();
        let err = read_config().unwrap_err();
        match err {
            ConfigReadError::Validation { source: ConfigValidationError::OfflinePathHasCallsign } => {}
            other => panic!("expected Validation(OfflinePathHasCallsign), got {other:?}"),
        }
    });
}

#[test]
#[serial_test::serial]
fn test_read_config_happy_path() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{
            "schema_version": 1, "wizard_completed": true,
            "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
            "identity": {"callsign": "W4PHS", "identifier": null, "grid": "EM75"},
            "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
            "pat_mbo_address": "W4PHS@winlink.org"
        }"#).unwrap();
        let config = read_config().expect("happy path must succeed");
        assert!(config.wizard_completed);
        assert_eq!(config.identity.callsign.as_deref(), Some("W4PHS"));
    });
}

#[test]
#[serial_test::serial]
fn test_config_path_uses_xdg_config_home_when_set() {
    with_xdg_temp(|xdg| {
        let path = config_path();
        assert_eq!(path, xdg.join("tuxlink").join("config.json"));
    });
}

#[test]
#[serial_test::serial]
#[cfg(unix)]
fn test_read_config_eacces_returns_io_variant_not_notfound() {
    // ConfigReadError::Io variant per spec §3.1 — fires when std::fs::read returns
    // a non-NotFound error (EACCES, EIO, etc). Symmetric with the write-side
    // ProbeReadFailed coverage. Added per plan-review R3 P0-2.
    use std::os::unix::fs::PermissionsExt;
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema_version": 1}"#).unwrap();
        let mut perm = std::fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o000);
        std::fs::set_permissions(&path, perm).unwrap();

        let err = read_config().unwrap_err();
        assert!(matches!(err, ConfigReadError::Io { .. }),
            "EACCES on read MUST be Io variant, not NotFound: {err:?}");

        // Restore permissions so tempdir cleanup works.
        let mut perm = std::fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&path, perm).unwrap();
    });
}
```

- [ ] **Step 2: Run the tests to verify they fail (RED)**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | head -30
```

Expected: compile error — `read_config` and `ConfigReadError` not found.

- [ ] **Step 3: Append `ConfigReadError` + `read_config` to `src-tauri/src/config.rs`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigReadError {
    #[error("config file not found at {path}")]
    NotFound { path: std::path::PathBuf },
    #[error("io error reading {path}: {source}")]
    Io { path: std::path::PathBuf, #[source] source: std::io::Error },
    #[error("config deserialize failed: {source}")]
    Serde { #[source] source: serde_json::Error },
    #[error("config failed semantic validation: {source}")]
    Validation { #[source] source: ConfigValidationError },
}

/// Read + parse + validate the config at `config_path()`. Returns typed errors per spec §3.5.
/// Consumers: wizard plan line 525 (wizard_persist_offline) + line 617 (get_wizard_completed) —
/// both use `.ok()` to fold any error into None (first-run, malformed, etc.) and fall through
/// to a fresh wizard run.
pub fn read_config() -> Result<Config, ConfigReadError> {
    let path = config_path();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConfigReadError::NotFound { path });
        }
        Err(e) => return Err(ConfigReadError::Io { path, source: e }),
    };
    let config: Config = serde_json::from_slice(&bytes)
        .map_err(|source| ConfigReadError::Serde { source })?;
    config.validate()
        .map_err(|source| ConfigReadError::Validation { source })?;
    Ok(config)
}
```

- [ ] **Step 4: Run tests to verify GREEN**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | tail -10
```

Expected: 25 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/tests/config_test.rs
git commit -m "$(cat <<'EOF'
feat(config): tuxlink-4mt Phase 4 — read_config + ConfigReadError

In-scope per spec adrev R2 P0-2 + R3 P0-1: wizard plan line 525 + 617
literally call crate::config::read_config(); previous "defer to sibling bd
issue" framing was an unfiled intention that would have left the wizard
plan compile-failing.

ConfigReadError variants distinguish first-run (NotFound) from corruption
(Serde) from semantic violation (Validation wraps ConfigValidationError),
giving consumers (wizard's .ok() fold) clean recovery paths.

#[serial_test::serial] applied unconditionally to env-var-mutating tests
per spec adrev R1 P1-6 (the "if observed flaky" hedge was guaranteed
flakiness in CI).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 5 — `write_config_atomic` + `ConfigWriteError`

### Task 5.1: Write failing tests for write_config_atomic (happy + refusal paths)

**Files:**
- Modify: `src-tauri/tests/config_test.rs` (APPEND Phase 5 tests)

- [ ] **Step 1: APPEND Phase 5 tests**

Add to the END of `src-tauri/tests/config_test.rs`:

```rust
// ============================================================================
// Phase 5 — write_config_atomic + ConfigWriteError
// ============================================================================

use tuxlink_lib::config::{write_config_atomic, ConfigWriteError};

fn make_valid_cms_config() -> Config {
    serde_json::from_str(r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": "EM75"},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": "W4PHS@winlink.org"
    }"#).unwrap()
}

#[test]
#[serial_test::serial]
fn test_write_atomic_first_run_creates_file() {
    with_xdg_temp(|xdg| {
        let config = make_valid_cms_config();
        write_config_atomic(&config).expect("first-run write must succeed");
        let path = xdg.join("tuxlink").join("config.json");
        assert!(path.exists(), "config file must exist after write");
        // Round-trip: read_config should deserialize it back.
        let roundtrip = read_config().expect("written file must read back");
        assert_eq!(roundtrip.identity.callsign.as_deref(), Some("W4PHS"));
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_overwrites_v1_file() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema_version": 1, "old": "value"}"#).unwrap();
        let config = make_valid_cms_config();
        write_config_atomic(&config).expect("v1-overwrite must succeed");
        let roundtrip = read_config().expect("post-overwrite must read back");
        assert!(roundtrip.wizard_completed);
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_refuses_schema_version_mismatch_future() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let preserved = br#"{"schema_version": 99, "future": "shape"}"#;
        std::fs::write(&path, preserved).unwrap();
        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::SchemaVersionMismatch { existing: 99, ours: 1 } => {}
            other => panic!("expected SchemaVersionMismatch{{99,1}}, got {other:?}"),
        }
        // Original file MUST be preserved.
        let current = std::fs::read(&path).unwrap();
        assert_eq!(current, preserved);
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_refuses_schema_version_mismatch_past() {
    // Spec §3.4 SchemaVersionMismatch covers BOTH directions (renamed from Downgrade
    // per adrev R4 P1-5). A schema_version=0 file (hypothetical historical artifact)
    // also blocks the write rather than silently overwriting.
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema_version": 0, "ancient": "shape"}"#).unwrap();
        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::SchemaVersionMismatch { existing: 0, ours: 1 } => {}
            other => panic!("expected SchemaVersionMismatch{{0,1}}, got {other:?}"),
        }
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_overwrites_unparseable_file() {
    // Corruption-recovery semantics: malformed-JSON existing file does NOT block the write.
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"\x00\x01\x02 totally not json").unwrap();
        let config = make_valid_cms_config();
        write_config_atomic(&config).expect("unparseable existing file must NOT block");
        let roundtrip = read_config().expect("post-overwrite must read back");
        assert!(roundtrip.wizard_completed);
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_refuses_existing_symlink() {
    use std::os::unix::fs::symlink;
    with_xdg_temp(|xdg| {
        let cfg_path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        // Create a real target file + a symlink at the config path.
        let target = xdg.join("dotfiles-config.json");
        std::fs::write(&target, br#"{"original": "data"}"#).unwrap();
        symlink(&target, &cfg_path).unwrap();
        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::ExistingFileIsSymlink { path: ref p, target: ref t } => {
                assert_eq!(p, &cfg_path);
                assert_eq!(t.as_deref(), Some(target.as_path()));
            }
            other => panic!("expected ExistingFileIsSymlink, got {other:?}"),
        }
        // PRESERVATION CONTRACT (spec §6): both the symlink itself AND its target must survive.
        assert!(
            std::fs::symlink_metadata(&cfg_path).unwrap().file_type().is_symlink(),
            "symlink itself must survive refusal"
        );
        assert_eq!(
            std::fs::read_link(&cfg_path).unwrap(),
            target,
            "symlink must still point to target"
        );
        let target_content = std::fs::read(&target).unwrap();
        assert_eq!(target_content, br#"{"original": "data"}"#);
    });
}

#[test]
#[serial_test::serial]
#[cfg(unix)]
fn test_write_atomic_probe_read_eacces_fails_typed() {
    use std::os::unix::fs::PermissionsExt;
    with_xdg_temp(|xdg| {
        let cfg_path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        let original = br#"{"schema_version": 1, "preserved": true}"#;
        std::fs::write(&cfg_path, original).unwrap();

        // Capture file content BEFORE the chmod 000 (since we'll need to verify preservation
        // after the failed write, and chmod 000 means we can't read it then).
        // Re-open with chmod 0o400, capture, then chmod 0o000 so the probe fails.
        let mut perm = std::fs::metadata(&cfg_path).unwrap().permissions();
        perm.set_mode(0o000);
        std::fs::set_permissions(&cfg_path, perm).unwrap();

        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::ProbeReadFailed { path: ref p, .. } => {
                assert_eq!(p, &cfg_path);
            }
            other => panic!("expected ProbeReadFailed, got {other:?}"),
        }

        // PRESERVATION CONTRACT (spec §6): original file content unchanged after refusal.
        // Restore read permission to verify, then chmod 600 for tempdir cleanup.
        let mut perm = std::fs::metadata(&cfg_path).unwrap().permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&cfg_path, perm).unwrap();
        let preserved = std::fs::read(&cfg_path).unwrap();
        assert_eq!(preserved, original, "original file content must be preserved on ProbeReadFailed refusal");
    });
}
```

- [ ] **Step 2: Run tests to verify RED**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | head -30
```

Expected: compile error — `write_config_atomic` and `ConfigWriteError` not found.

- [ ] **Step 3: Append `ConfigWriteError` + `write_config_atomic` + helpers to `src-tauri/src/config.rs`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigWriteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config serialize failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("refuse to overwrite existing config with schema_version {existing} (this binary supports v{ours}): mismatch — either downgrade (existing > ours) or backward-incompat (existing < ours)")]
    SchemaVersionMismatch { existing: u32, ours: u32 },
    #[error("refuse to overwrite existing config at {path:?}: file is a symlink (target: {target:?})")]
    ExistingFileIsSymlink { path: std::path::PathBuf, target: Option<std::path::PathBuf> },
    #[error("config path {path:?} cannot be probed: {source}")]
    ProbeReadFailed { path: std::path::PathBuf, #[source] source: std::io::Error },
    #[error("config path {path:?} has no parent directory")]
    NoParentDirectory { path: std::path::PathBuf },
}

/// Atomic single-write of `config` to `config_path()`. Returns typed errors per spec §3.4.
///
/// Atomicity contract scope: local POSIX FS (ext4/btrfs/xfs/APFS) where target file +
/// tempfile are on the same FS AND the same BTRFS subvolume. NFS / FUSE / Lustre semantics
/// undefined; BTRFS subvolume-boundary case lapses atomicity silently.
///
/// Single-instance assumption: ONE tuxlink instance writes at a time. Cross-process
/// serialization (flock) out of scope for v0.0.1; concurrent writers both return Ok and
/// the last rename wins.
///
/// Backup-tool .tmp visibility: NamedTempFile creates a file like `.tmpXXXXXX` in the
/// parent directory. Backup tools (Time Machine, rclone, restic) watching the directory
/// may briefly capture this file. The tempfile is short-lived (microseconds) and removed
/// atomically by persist's rename. Expected behavior; no startup-cleanup machinery.
///
/// NoParentDirectory variant is defensive — config_path()'s `XDG_CONFIG_HOME/tuxlink/...`
/// composition makes this unreachable in practice; declared for future config_path()
/// refactors that may relax the path structure.
///
/// Does NOT auto-call `config.validate()` — caller responsibility per spec §3.3.
pub fn write_config_atomic(config: &Config) -> Result<(), ConfigWriteError> {
    let path = config_path();
    let parent = path.parent()
        .ok_or_else(|| ConfigWriteError::NoParentDirectory { path: path.clone() })?;
    std::fs::create_dir_all(parent)?;

    // Symlink-detection (spec §3.4 per adrev R4 P0-2): refuse to silently replace a symlink.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() {
            return Err(ConfigWriteError::ExistingFileIsSymlink {
                path: path.clone(),
                target: std::fs::read_link(&path).ok(),
            });
        }
    }

    // Schema-version mismatch refusal (both directions per adrev R4 P1-5).
    // Tolerates unparseable bytes (first-run + corruption-recovery cases).
    // Distinguishes NotFound (proceed) from other I/O errors (abort) per adrev R4 P1-4.
    match std::fs::read(&path) {
        Ok(bytes) => {
            if let Ok(probe) = serde_json::from_slice::<SchemaVersionProbe>(&bytes) {
                if probe.schema_version != CONFIG_SCHEMA_VERSION {
                    return Err(ConfigWriteError::SchemaVersionMismatch {
                        existing: probe.schema_version,
                        ours: CONFIG_SCHEMA_VERSION,
                    });
                }
            }
            // Unparseable bytes: silently overwrite (corruption recovery).
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // First-run case — proceed with write. Not an error.
        }
        Err(e) => {
            // EACCES, EIO, etc. — abort with typed error rather than silently
            // proceeding (which would have destroyed an unreadable existing file).
            return Err(ConfigWriteError::ProbeReadFailed { path: path.clone(), source: e });
        }
    }

    // Same-directory tempfile → atomic persist on local POSIX FS.
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(tmp.as_file(), config)?;
    tmp.as_file().sync_all()?;  // file data durable
    tmp.persist(&path).map_err(|e| ConfigWriteError::Io(e.error))?;

    // Parent-dir fsync per adrev R2 P0-3 + R4 P0-1: rename(2) is atomic but not DURABLE
    // until the parent directory's metadata flushes. tempfile::persist does not do this.
    let parent_dir = std::fs::File::open(parent)?;
    parent_dir.sync_all()?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct SchemaVersionProbe { schema_version: u32 }
```

- [ ] **Step 4: Run tests to verify GREEN**

```bash
cd src-tauri && cargo test --test config_test 2>&1 | tail -15
```

Expected: 32 tests pass (5 + 9 + 6 + 5 + 7 = 32).

If `test_write_atomic_refuses_existing_symlink` fails on a non-Unix host: that's a test infrastructure gap, not a bug. The test gated with `#[cfg(unix)]` if needed (most dev environments are Unix; CI is Linux).

If `test_write_atomic_probe_read_eacces_fails_typed` fails: chmod 000 may not propagate on tmpfs in some setups; check the test environment.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/tests/config_test.rs
git commit -m "$(cat <<'EOF'
feat(config): tuxlink-4mt Phase 5 — write_config_atomic + ConfigWriteError

Atomic single-write with seven defenses per spec §3.4 + adrev R2 P0-3 + R4
P0-1 + R4 P0-2 + R4 P1-4 + R4 P1-5:
- Same-directory tempfile + tempfile::persist (atomic rename on local POSIX)
- File-data fsync via sync_all() before persist (durable contents)
- Parent-directory fsync after persist (durable rename metadata; tempfile
  crate does NOT do this for you — silent crash-loss without it)
- Symlink-detection: refuse to silently replace a symlink (dotfiles workflow
  safety per adrev R4 P0-2)
- SchemaVersionMismatch covers both directions (downgrade AND backward) per
  adrev R4 P1-5
- ProbeReadFailed distinguishes EACCES/EIO from NotFound per adrev R4 P1-4
  (NotFound proceeds with write; other errors abort)
- NoParentDirectory typed error replaces .expect() panic per adrev R2 P2-3

Single-instance assumption documented; flock cross-process serialization
out-of-scope for v0.0.1 (separate bd issue per spec §2.2). Atomicity bounds
explicitly NFS-undefined + BTRFS-subvolume-boundary-lapses (spec §3.4).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 6 — Pitfalls DRIFT-1 + plan body historical-cite

### Task 6.1: Add DRIFT-1 to implementation-pitfalls.md as new §3 (NOT §2; §2 is already substantive)

**CRITICAL CORRECTION from plan-review R1 P0-2 + R2 P2-1 + R3 P0-1:** the spec's §5 framing (which said "replace the EXAMPLE-DOMAIN-2 stub at §2") was based on stale state. Verified 2026-05-18: pitfalls §2 is fully populated with `# Section 2: Safety-Stack Coordination and Cross-Component Parity` (HOOK-1, LEASE-1, PARITY-1; shipped via PR #39). The TOC row + section header + section body for §2 MUST NOT BE TOUCHED. DRIFT-1 lands as a NEW §3.

**Files:**
- Modify: `docs/pitfalls/implementation-pitfalls.md` (ADD a new §3 section + new TOC row + Appendix B summary row)

- [ ] **Step 1: Verify the current state matches expectations**

```bash
grep -n "^# Section 2:\|^# Section 3:\|EXAMPLE-DOMAIN" docs/pitfalls/implementation-pitfalls.md
```

Expected: ONE hit on `# Section 2: Safety-Stack Coordination and Cross-Component Parity` (around line 284) and ZERO hits on `# Section 3:` or `EXAMPLE-DOMAIN-*`. If the output differs, STOP and investigate — the pitfalls state has changed since plan-write.

- [ ] **Step 2: Add the TOC row for §3**

In `docs/pitfalls/implementation-pitfalls.md`, find the table-of-contents row for §2 (around line 29):
```markdown
| 2 | [Safety-Stack Coordination and Cross-Component Parity](#2-safety-stack-coordination-and-cross-component-parity) | ... | HOOK-1, LEASE-1, PARITY-1 | §2.C |
```

Insert a NEW row IMMEDIATELY AFTER it:
```markdown
| 3 | [Plan and Documentation Discipline](#3-plan-and-documentation-discipline) | Any plan / spec amendment, especially when an AMENDMENT marker (AMD-N) lands in a previously-shipped task's plan body | DRIFT-1 | §3.C |
```

- [ ] **Step 3: Add the §3 section body AFTER the §2 review checklist**

Find the end of `# Section 2: Safety-Stack Coordination and Cross-Component Parity` — specifically the last `---` separator that follows its review checklist (around line 396). INSERT the entire §3 section AFTER that `---` separator. (Do not modify the §2 content; only add new content following it.) Insert:

   ```markdown
   # Section 3: Plan and Documentation Discipline

   > **Reader context:** I'm proposing, reviewing, or amending a plan document (`docs/plans/*.md`, `docs/superpowers/specs/*.md`, `docs/superpowers/plans/*.md`) and I need to know what discipline applies to the amendment lifecycle to prevent the implementation from drifting out of sync with what the plan says.

   ---

   ### DRIFT-1: Plan-text AMENDMENT does not auto-cascade to the code it amends

   **The Flaw:** A plan amendment (`> AMENDMENT 2026-MM-DD (AMD-N).`) lands in `docs/plans/*.md` documenting a change to a previously-shipped task's contract. The plan body is updated. The code that the prior task shipped is NOT updated — the AMD is a description of intent, not a code change. Subsequent plans that assume the AMD shipped find the codebase in the pre-AMD shape and fail to compile.

   **Why It Matters:** AMDs are cheap (a markdown edit + commit). Code amendments are expensive (bd issue + full pipeline). The asymmetry tempts operators to ship the AMD without the bd issue, especially when the AMD is conceptually simple. The gap is invisible until a downstream task tries to use the new contract — at which point a plan-review-cycle catches it (best case, like wizard-cluster R1 P0-1) or impl ships compile-failing code (worst case).

   **The Fix:** Every AMD MUST ship with a paired bd issue if the prior task is "shipped." Two acceptable forms:
   1. **Code-bearing AMD:** the AMD body cites the bd issue tracking the code-impl side: "AMD-N. ... Bd issue tracking the code-impl side: tuxlink-XYZ."
   2. **Prose-only AMD:** state explicitly that there's no code surface: "AMD-N (prose-only; no code impact)."

   The discipline question is asked at amendment time, not delegated to a future plan-review.

   **The Lesson:** The 2026-05-18 wizard-cluster plan-review caught this gap class via R1 P0-1 + R1 P0-3 + Codex R4 P0 #2 (cross-validated across providers). `tuxlink-4mt` retroactively cleared AMD-1 + AMD-11's code-impl gap. The fix is to never accumulate the gap in the first place. Cross-spec amendments inherit the same discipline: when amending a SHIPPED SPEC (e.g., the wizard cluster spec landed via PR #62), file a paired bd issue tracking any consumer that needs updating.

   ---

   ### Section 3 Review Checklist

   - [ ] **Check derived from DRIFT-1** — Any PR that lands an AMENDMENT in a plan or spec includes either (a) a cited bd issue tracking the code-impl side, or (b) explicit "prose-only; no code impact" framing. Verify by searching the PR body for AMENDMENT markers and confirming each carries the cite or the explicit punt.
   - [ ] **Check derived from DRIFT-1** — When amending a shipped spec (e.g., adding fields to `WizardError` or changing a function signature in `validate_identity`), the PR identifies every downstream consumer (via `grep -r 'consumer-symbol'`) and files paired bd issues for each consumer that needs adaptation.
   - [ ] **Check derived from DRIFT-1** — Pipeline cycles for code amendments inherited from plan amendments use the FULL build-robust-features pipeline (brainstorm → 5-round adrev → spec → 4-round plan-review → revision → TDD impl) — the discipline of `tuxlink-4mt` itself. Skipping upstream phases to "ship the AMD-cascade fix faster" defeats the purpose of catching the gap class.

   ---
   ```

- [ ] **Step 4: Add a row to Appendix B's "Validated Pitfalls Summary" table (if present)**

```bash
grep -n "Unified Summary Table\|Validated Pitfalls Summary" docs/pitfalls/implementation-pitfalls.md
```

If a summary table exists in Appendix B, add a row for DRIFT-1:
```markdown
| DRIFT-1 | Plan-text amendments don't auto-cascade to code | HIGH | VALIDATED | §3 Plan and Documentation Discipline |
```

- [ ] **Step 5: Verify the pitfalls file renders without broken anchors**

```bash
grep -c "DRIFT-1\|Plan and Documentation Discipline\|#3-plan-and-documentation" docs/pitfalls/implementation-pitfalls.md
```

Expected: at least 4 hits (TOC + section header + entry header + review-checklist mention; +1 more if Appendix B row added). Also verify §2 was NOT modified:

```bash
grep -c "Safety-Stack Coordination\|HOOK-1\|LEASE-1\|PARITY-1" docs/pitfalls/implementation-pitfalls.md
```

Expected: same count as before the edit (≥7 — TOC + section header + 3 entry headers + checklist mentions). If lower, §2 was accidentally modified.

- [ ] **Step 6: Commit**

```bash
git add docs/pitfalls/implementation-pitfalls.md
git commit -m "$(cat <<'EOF'
docs(pitfalls): tuxlink-4mt — DRIFT-1 (Plan and Documentation Discipline) as §3

ADDS a new §3 (does NOT modify §2 Safety-Stack Coordination, which already
shipped via PR #39 and remains intact). Codifies the AMD-cascade discipline
that caused tuxlink-4mt itself. Cross-validated by 2026-05-18 wizard-cluster
plan-review R1 P0-1 + R1 P0-3 + Codex R4 P0 #2: plan-text amendments (AMD-N)
shipped without paired bd issues for the code-impl side, leaving the
downstream wizard plan compile-failing against the post-AMD code surface.

The placement decision (§3, NOT §2 as v1 plan called for) is itself a DRIFT-1
near-miss: the spec assumed §2 was an EXAMPLE-DOMAIN-2 stub; plan-review
caught the stale read; this commit corrects to §3 so substantive §2 content
is preserved.

Discipline going forward: every AMENDMENT marker in a plan/spec body must
cite either a paired bd issue (for code-bearing AMDs) or "prose-only; no code
impact" (for prose-only). Cross-spec amendments inherit the same rule.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 6.2: Update plan body Task 2 "Pre-amendment shape (historical)" to cite tuxlink-4mt

**Files:**
- Modify: `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` (locate Task 2 historical subsection)

- [ ] **Step 1: Find the historical subsection**

```bash
grep -n "Pre-amendment shape (historical)" docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md
```

Expected: one match, around line 689 of the plan.

- [ ] **Step 2: Append a citation paragraph after the historical-shape JSON block**

In `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md`, find the verbatim sentence anchor (use `grep -n` to locate the exact line; line numbers shift over time so anchor on the sentence text, not the line number):

```bash
grep -n "Nesting + optionality solves these without bumping" docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md
```

Expected: one match. The matching paragraph ends with:
> "Nesting + optionality solves these without bumping `schema_version` since v0.0.1 has shipped to zero users."

Insert a blank line + the following paragraph IMMEDIATELY AFTER that sentence (and before the trailing `---` that ends the Task 2 section):

```markdown

**Implementation tracking (added 2026-05-18, bd issue `tuxlink-4mt`):** The post-AMD-1 + post-AMD-11 code surface defined above shipped via the implementation plan at [`docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md`](../superpowers/plans/2026-05-18-task-2-config-impl-plan.md) (closes bd issue tuxlink-4mt). The 2026-05-18 wizard-cluster plan-review caught the original gap (R1 P0-1 + R1 P0-3 + Codex R4 P0 #2 cross-validated) — code in `src-tauri/src/config.rs` was still pre-AMD-1 flat-schema at the time of the AMDs because plan-text amendments do not auto-cascade. Pitfalls entry DRIFT-1 codifies the discipline so this gap class does not recur.
```

- [ ] **Step 3: Commit**

```bash
git add docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md
git commit -m "$(cat <<'EOF'
docs(plan): cite tuxlink-4mt as Task 2 historical-shape implementer

Closes the AMD-cascade discipline loop per DRIFT-1: AMD-1 + AMD-11
implementations now have an explicit bd-issue citation in the plan body,
so future plan readers can trace amendment → implementation directly.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 7 — Final verification + PR

### Task 7.1: Full test suite + cargo build

**Files:** none modified

- [ ] **Step 1: Run the full test suite + build**

```bash
cd src-tauri
cargo test --test config_test 2>&1 | tail -15
cargo build --quiet 2>&1 | head -20
cd ..
```

Expected: 32 tests pass; `cargo build` produces no warnings/errors. If `cargo build` warns about unused imports or `dead_code`, investigate — the new public surface should be exercised by the tests.

- [ ] **Step 2: Verify no AMD-11 drift remains + DRIFT-1 + functions present (use word boundaries)**

```bash
# AMD-11 drift defense
grep -c 'winlink_password_present' src-tauri/src/config.rs                                # MUST be 0
grep -c 'winlink_password_present' src-tauri/tests/config_test.rs                         # MUST be ≥1 (only in the AMD-11 drift-defense test)

# Public function presence — use word boundaries to avoid double-counting (per plan-review R1 P1-4):
grep -q 'pub fn validate_identity\b' src-tauri/src/config.rs || echo "MISSING: validate_identity"
grep -q 'pub fn validate_identity_describe\b' src-tauri/src/config.rs || echo "MISSING: validate_identity_describe"
grep -q 'pub fn read_config\b' src-tauri/src/config.rs || echo "MISSING: read_config"
grep -q 'pub fn write_config_atomic\b' src-tauri/src/config.rs || echo "MISSING: write_config_atomic"
grep -q 'pub fn config_path\b' src-tauri/src/config.rs || echo "MISSING: config_path"
grep -c 'pub mod config;' src-tauri/src/lib.rs                                            # MUST be 1

# DRIFT-1 + Section 3 presence in pitfalls (per plan-review R3 P3-2 — Phase 7 needs to verify docs landed):
grep -c 'DRIFT-1\|Plan and Documentation Discipline\|#3-plan-and-documentation' docs/pitfalls/implementation-pitfalls.md  # MUST be ≥4

# Plan body historical-cite (per plan-review R3 P3-2):
grep -c 'tuxlink-4mt' docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md                        # MUST be ≥1

# §2 of pitfalls MUST NOT have been modified (cross-check Safety-Stack content survived):
grep -c 'Safety-Stack Coordination\|HOOK-1\|LEASE-1\|PARITY-1' docs/pitfalls/implementation-pitfalls.md  # MUST be ≥7
```

If any check fails — STOP and investigate before opening the PR. Each "MISSING:" echo or count mismatch indicates a gap.

### Task 7.2: Push branch + open PR

- [ ] **Step 1: Push the branch**

```bash
git status                                       # confirm clean working tree
git log --oneline -10                            # confirm Phase 0-6 commits all present
git push -u origin bd-tuxlink-4mt/task-2-config-impl
```

- [ ] **Step 2: Open the PR**

```bash
gh pr create --base feat/v0.0.1 --head bd-tuxlink-4mt/task-2-config-impl \
  --title "[<SESSION-MONIKER>] feat(config): tuxlink-4mt — Task 2 config impl (AMD-1 + AMD-11 cascade fix)" \
  --body "$(cat <<'EOF'
## Summary

Closes bd issue `tuxlink-4mt`. Implements the post-AMD-1 nested config schema + drops `winlink_password_present` per AMD-11 + adds the validators and atomic-write surface that downstream wizard cluster (`tuxlink-ln3`) requires.

Spec: [`docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md`](docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md) (v2, post-4-round Claude spec adrev)
Plan: [`docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md`](docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md)

### Pipeline summary (per memory `feedback_no_carveout_on_cross_provider_adrev`)

- Brainstorm + spec v1 → 2ae9abb
- 4-round Claude spec adrev (R1 friction + R2 contract + R3 coverage + R4 failure-mode) → 58 findings (13 P0)
- Codex R5 spec adrev — deferred to plan-review R4 slot per quota gotcha (`feedback_codex_quota_gotcha`)
- Spec revision v2 → a36233f (applied all P0s + critical P1s)
- Plan write → [link]
- 4-round plan-review (R1-R3 Claude + R4 Codex) → [N findings]
- Plan revision → [SHA]
- TDD impl via subagent-driven-development → Phase 0-7 commits in this PR

### What landed (file inventory)

| File | Change |
|---|---|
| `src-tauri/src/config.rs` | REWRITE — nested AMD-1 shape + validators + read_config + write_config_atomic |
| `src-tauri/tests/config_test.rs` | REWRITE — 32-test suite (replaces 4 flat-schema tests) |
| `src-tauri/Cargo.toml` | thiserror + tempfile to runtime deps; serial_test to dev-deps |
| `docs/pitfalls/implementation-pitfalls.md` | §2 Plan & Documentation Discipline + DRIFT-1 entry |
| `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` | Task 2 historical section cites tuxlink-4mt |
| `docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md` | spec doc (v2) |
| `docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md` | plan doc |

### AMD-cascade discipline (DRIFT-1)

This PR codifies DRIFT-1 as a new pitfalls entry. **Every AMD in a plan/spec body must cite either (a) the paired bd issue for the code-impl side, or (b) "prose-only; no code impact."** This PR retroactively clears AMD-1 (2026-05-17) and AMD-11 (2026-05-18) via tuxlink-4mt.

### Test plan

- [x] `cd src-tauri && cargo test --test config_test` — all 32 tests pass
- [x] `cd src-tauri && cargo build` — no warnings/errors
- [x] `grep -c 'winlink_password_present' src-tauri/src/config.rs` returns 0 (AMD-11 drift cleared)
- [ ] Wizard cluster spec at line 151 (`!validate_identity(&callsign)`) compiles against this PR's `validate_identity: bool` signature — verified by downstream wizard impl plan (`tuxlink-ln3`) after this PR merges.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

### Task 7.3: File the testing-pitfalls.md sibling bd issue (per plan-review R3 P0-4)

The spec deferred adding a DRIFT-1 companion entry to `testing-pitfalls.md` ("Add DRIFT-1 verification recipe to testing-pitfalls.md") to a sibling bd issue. The plan-review caught that the spec said "follow-up bd issue" but the bd issue was never actually filed — making the deferral a forgotten intention. File it here before PR merge so the discipline loop closes.

- [ ] **Step 1: Create the sibling bd issue**

```bash
bd create --title "Add DRIFT-1 verification recipe to testing-pitfalls.md" \
  --type=task --priority=2 \
  --description="Per tuxlink-4mt spec §5 + §11 #21: testing-pitfalls.md needs a verification recipe paired with DRIFT-1 in implementation-pitfalls.md (shipped via tuxlink-4mt PR #<NUM>). Likely recipe: 'grep -E \"AMENDMENT [0-9]{4}-[0-9]{2}-[0-9]{2} \\(AMD-\" docs/plans/*.md' should match the number of bd issues whose body cites AMD-N (file or close the gap). Verify testing-pitfalls.md state first — may also be in stub state from project init. Cites tuxlink-4mt as the parent that deferred this companion entry."
```

Note the returned bd issue ID (e.g., `tuxlink-xyz`). Cite it in the PR body.

### Task 7.4: Close tuxlink-4mt via deliverable

- [ ] **Step 1: After PR merges, close the bd issue**

```bash
# Wait for PR to merge (operator action).
# Then:
bd close tuxlink-4mt --reason="PR #<NUM> merged at <SHA>. Task 2 config impl (AMD-1 nested schema + AMD-11 drop + validate_identity + Config::validate + read_config + write_config_atomic) + DRIFT-1 pitfalls entry (as §3) shipped. Sibling testing-pitfalls.md companion tracked at tuxlink-xyz."
```

- [ ] **Step 2: Verify the bd dep edge is cleared**

```bash
bd show tuxlink-ln3 | grep -A 2 'DEPENDS ON'
```

Expected: `tuxlink-4mt` no longer in `DEPENDS ON` of `tuxlink-ln3` (or shows as closed).

---

## Self-Review (per writing-plans skill)

### Spec coverage check

Going through spec §2.1 items 1-11:

| Spec item | Plan phase | ✓ |
|---|---|---|
| 1. Replace flat Config → nested AMD-1 | Phase 2 | ✓ |
| 2. Drop winlink_password_present | Phase 2 (deny_unknown_fields + test 10) | ✓ |
| 3. validate_identity → bool | Phase 1 | ✓ |
| 4. validate_identity_describe | Phase 1 | ✓ |
| 5. Config::validate (both rules + identity validation) | Phase 3 | ✓ |
| 6. read_config + ConfigReadError | Phase 4 | ✓ |
| 7. write_config_atomic + ConfigWriteError + symlink + parent-fsync + EACCES + schema mismatch | Phase 5 | ✓ |
| 8. 20+ test suite (grew to 32) | Phases 1-5 | ✓ |
| 9. thiserror + tempfile + serial_test deps | Phase 0 | ✓ |
| 10. DRIFT-1 pitfalls entry → §2 | Phase 6 task 6.1 | ✓ |
| 11. Plan body historical-cite update | Phase 6 task 6.2 | ✓ |

All 11 in-scope items have task coverage.

### Placeholder scan

No "TBD", "TODO", "implement later", "fill in details", "Similar to Task N" patterns in the plan. All code blocks are complete and self-contained. Commit templates have placeholders (`<SESSION-MONIKER>`, `<NUM>`, `<SHA>`) that are EXPECTED — the Mandatory Per-Task Preamble directs the implementer to substitute them.

### Type consistency

- `Config` struct fields match across Phase 2 (definition) + Phase 3 (validate) + Phase 4 (read_config) + Phase 5 (write_config_atomic).
- `ConfigValidationError::InvalidIdentity { field, rule }` shape consistent between Phase 3 (definition) and Phase 4 test (validation propagation).
- `ConfigWriteError::ExistingFileIsSymlink { path, target }` field names consistent between Phase 5 definition and Phase 5 test pattern-matching.
- `validate_identity` return type `bool` consistent between Phase 1 definition + Phase 3 usage in `Config::validate`.
- `read_config` signature `Result<Config, ConfigReadError>` consistent between Phase 4 definition + Phase 5 usage in `test_write_atomic_first_run_creates_file` (round-trip via read).

No drift detected.

---

## Execution handoff

Plan complete and saved to [`docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md`](2026-05-18-task-2-config-impl-plan.md). Three execution options:

**1. Subagent-Driven (recommended)** — parent dispatches a fresh subagent per task, reviews between tasks, fast iteration. Each subagent gets a self-contained brief and one phase's tasks. Best for keeping the parent's context window healthy across 7 phases.

**2. Inline Execution** — parent executes tasks in this session using `superpowers:executing-plans`. Batch execution with checkpoints for review. Burns parent context faster but reduces dispatch overhead.

**3. Single-subagent-end-to-end** — dispatch one subagent with the entire plan and have it execute Phases 1-7 sequentially. Simplest but loses the per-phase checkpointing.

**Recommendation given the operator's "Full pipeline this session" pacing call (memory `feedback_no_atomic_decisions_to_operator`):** Option 1, subagent-driven. The plan's 7 phases naturally divide between subagent dispatches; parent reviews + applies Codex post-subagent review per memory `feedback_codex_post_subagent_review`.
