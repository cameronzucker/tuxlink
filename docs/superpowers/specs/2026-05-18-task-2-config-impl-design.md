# Task 2 Config Implementation — Design Spec

**Spec ID:** tuxlink-4mt
**Date:** 2026-05-18
**Author:** agent `fox-cove-towhee`
**Status:** draft — pre-adversarial-review
**Branch:** `bd-tuxlink-4mt/task-2-config-impl` (worktree off `feat/v0.0.1`)
**Closes via deliverable:** the PR that merges this spec's implementation into `feat/v0.0.1`

---

## 1. Why this spec exists

Two plan amendments — [AMD-1 (2026-05-17)](../../plans/2026-04-22-tuxlink-v0.0.1-plan.md) and AMD-11 (2026-05-18) — replaced Task 2's flat `Config` schema with a nested shape and dropped the `winlink_password_present` field. **The amendments shipped as plan-text only. The code in [`src-tauri/src/config.rs`](../../../src-tauri/src/config.rs) was never updated** — it still carries the pre-AMD-1 flat schema with `winlink_password_present`.

This gap was caught by the 2026-05-18 wizard-cluster plan-review-cycle:
- **R1 P0-1** (friction lens, Claude): `config.rs` is pre-AMD-1; wizard plan Phase 3.2 step 4 builds a `Config { ... }` literal with a struct shape that doesn't exist.
- **R1 P0-3** (friction lens, Claude): `validate_identity()` cited by wizard plan Phase 3.2 step 3 does not exist in `config.rs`.
- **R4 P0 #2** (cross-provider, Codex): cross-validated R1's finding — "`src-tauri/src/config.rs` is still the older flat shape with `winlink_password_present`, so Phase 3 would not compile if execution began from this base."
- **R3 P0-2** (coverage lens, Claude): wizard plan calls `write_config_atomic()` which also does not exist.

This spec closes that gap. It is a **HARD prerequisite** for the wizard-cluster impl plan (`tuxlink-ln3`).

Beyond fixing the immediate gap, this spec codifies the gap *class* as a pitfalls entry (`DRIFT-1`) so future plan amendments file the paired bd issue at amendment time, not in a downstream plan-review.

---

## 2. Scope

### 2.1 In scope

1. Replace flat `Config` struct in `src-tauri/src/config.rs` with the AMD-1 nested shape: `Config { schema_version, wizard_completed, connect: ConnectConfig, identity: IdentityConfig, privacy: PrivacyConfig, pat_mbo_address }`.
2. Drop `winlink_password_present` per AMD-11 (the keyring is single source of truth post-cred-refactor).
3. Implement `validate_identity(s: &str) -> Result<(), String>` per AMD-1's loose-validator semantics.
4. Implement `Config::validate(&self) -> Result<(), ConfigValidationError>` for cross-field rules (CMS path requires callsign; identity fields run through `validate_identity`).
5. Implement `write_config_atomic(config: &Config) -> Result<(), ConfigWriteError>` — same-directory tempfile + persist (atomic rename on POSIX), with schema-version-downgrade refusal per wizard plan-review R3 P0-2.
6. Replace the 4 flat-schema tests in `src-tauri/tests/config_test.rs` with an 11-test suite (6 from the AMD-1-updated plan body + 5 new tests for `write_config_atomic` and AMD-11 drift defense).
7. Add `thiserror = "1"` to `[dependencies]`; promote `tempfile = "3"` from `[dev-dependencies]` to `[dependencies]`.
8. Add `DRIFT-1` pitfalls entry to `docs/pitfalls/implementation-pitfalls.md` codifying the AMD-cascade discipline.
9. Update the plan body's Task 2 "Pre-amendment shape (historical)" section to reference `tuxlink-4mt` as the implementing bd issue.

### 2.2 Out of scope

- **Migration code for pre-AMD-1 flat configs.** Per AMD-1's own note: "`schema_version` stays at 1 (no shipped users to migrate)." A pre-AMD-1 `config.json` on disk fails to deserialize; operators in test environments wipe and re-wizard. Explicit punt, not omission.
- **`read_config()` from disk.** Consumers of `Config` at runtime (e.g., Task 1.5's `get_wizard_completed` Tauri command, Task 3's PatProcess rendering of Pat's config from tuxlink's) live in their own bd issues. This spec ships the *types* and the *write path*; the *read path* is the caller's responsibility.
- **AuxAddr / multi-callsign keyring entries.** Per AMD-13's single-callsign scope; multi-account operators provision additional `(service="tuxlink-pat", account=AUX)` via `secret-tool` manually.
- **Settings UI surface** for post-wizard editing of these fields. Future bd issue when Task 12+ shell lands.
- **Wizard's `wizard_persist_cms` and `wizard_persist_offline` Tauri commands.** Those belong to `tuxlink-ln3` (the wizard impl) and CALL the surface this spec ships.

### 2.3 Dependency map

This spec is upstream of:
- `tuxlink-ln3` (wizard impl plan — HARD blocker; wizard `wizard_persist_cms` calls `write_config_atomic` + `validate_identity`)
- `tuxlink-756` (Task 3 PatProcess amendment — softer dependency; PatProcess will need to READ tuxlink's `Config` to render Pat's config.json, but that's `read_config`, not this spec's surface)

bd dep edges: `tuxlink-ln3` already blocks-on `tuxlink-4mt` (added 2026-05-18 during plan-review).

---

## 3. Design

### 3.1 Public surface

The full Rust public surface this spec exposes from `src-tauri/src/config.rs`:

```rust
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

// Nested AMD-1 schema. deny_unknown_fields is intentional — see §3.5.
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
    // winlink_password_present is REMOVED per AMD-11.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectConfig {
    pub connect_to_cms: bool,
    pub transport: CmsTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmsTransport { CmsSsl, Telnet }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    pub callsign: Option<String>,
    pub identifier: Option<String>,
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpsState { Off, LocalUiOnly, BroadcastAtPrecision }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionPrecision { FourCharGrid, SixCharGrid }

#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("CMS path requires identity.callsign to be set")]
    CmsPathMissingCallsign,
    #[error("invalid identity: {0}")]
    InvalidIdentity(String),
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigValidationError>;
}

pub fn validate_identity(s: &str) -> Result<(), String>;

pub fn config_path() -> std::path::PathBuf;     // unchanged from shipped

#[derive(Debug, thiserror::Error)]
pub enum ConfigWriteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("refuse to overwrite config with schema_version {existing} (this binary supports v{ours}) — would downgrade")]
    SchemaVersionDowngrade { existing: u32, ours: u32 },
}

pub fn write_config_atomic(config: &Config) -> Result<(), ConfigWriteError>;
```

The struct fields, enum variants, and validator semantics are quoted verbatim from the plan body's Task 2 (post-AMD-1) — see [docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md](../../plans/2026-04-22-tuxlink-v0.0.1-plan.md) lines ~280-620. This spec adds: `deny_unknown_fields`, `write_config_atomic`, `ConfigWriteError`.

### 3.2 `validate_identity` — loose validator

Per AMD-1: non-empty + no internal whitespace + ≤32 chars + ASCII-printable. The CMS is authoritative; this validator matches Express's `hs30.htm` "checked for basic syntax" semantics. Acceptance: standard callsigns (`W4PHS`), callsigns with SSID (`W4PHS-7`), tactical strings (`EOC-1`, `BAOFENG-FM-01`, `LabBench-3`). Rejection: empty, whitespace-internal (`W4 PHS`), >32 chars, non-ASCII-printable.

```rust
pub fn validate_identity(s: &str) -> Result<(), String> {
    if s.is_empty() { return Err("must not be empty".into()); }
    if s.chars().count() > 32 { return Err(format!("must be ≤32 chars (got {})", s.chars().count())); }
    if s.chars().any(char::is_whitespace) { return Err("must not contain whitespace".into()); }
    if s.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) { return Err("must be ASCII-printable".into()); }
    Ok(())
}
```

Codex R4 P3 (validator-message-mismatch finding) is preempted: error strings name the actual rule violated, not a generic "invalid identifier."

### 3.3 `Config::validate` — cross-field

Per plan body:

```rust
impl Config {
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.connect.connect_to_cms && self.identity.callsign.is_none() {
            return Err(ConfigValidationError::CmsPathMissingCallsign);
        }
        if let Some(ref c) = self.identity.callsign {
            validate_identity(c).map_err(ConfigValidationError::InvalidIdentity)?;
        }
        if let Some(ref i) = self.identity.identifier {
            validate_identity(i).map_err(ConfigValidationError::InvalidIdentity)?;
        }
        Ok(())
    }
}
```

Not auto-called by `Deserialize`; consumers (wizard's `wizard_persist_cms`, future `read_config`) explicitly invoke after deserialization. This matches the spec body's pattern: serde validates shape, `validate()` enforces semantics.

### 3.4 `write_config_atomic` — atomic single-write with downgrade refusal

```rust
pub fn write_config_atomic(config: &Config) -> Result<(), ConfigWriteError> {
    let path = config_path();
    let parent = path.parent()
        .expect("config_path always has a parent (XDG_CONFIG_HOME/tuxlink/...)");
    std::fs::create_dir_all(parent)?;

    // Downgrade refusal (wizard plan-review R3 P0-2 + Codex R4 P0 #2 spirit).
    // We probe ONLY for schema_version; tolerate any shape mismatch or unparseable
    // bytes by proceeding with the write (first-run + corruption-recovery cases).
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(probe) = serde_json::from_slice::<SchemaVersionProbe>(&bytes) {
            if probe.schema_version > CONFIG_SCHEMA_VERSION {
                return Err(ConfigWriteError::SchemaVersionDowngrade {
                    existing: probe.schema_version,
                    ours: CONFIG_SCHEMA_VERSION,
                });
            }
        }
    }

    // Same-directory tempfile → atomic persist (POSIX rename(2) within one filesystem).
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(tmp.as_file(), config)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path).map_err(|e| ConfigWriteError::Io(e.error))?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct SchemaVersionProbe { schema_version: u32 }
```

Three design calls:

1. **Same-directory tempfile via `NamedTempFile::new_in(parent)`** — keeps the tempfile on the same filesystem as the target. `persist` falls through to `rename(2)`, which is atomic within one filesystem. Cross-filesystem rename silently degrades to copy-then-delete, which is NOT atomic. Letting `tempfile` use the OS temp dir would break atomicity on systems where `/tmp` is `tmpfs` and `$XDG_CONFIG_HOME` is on `/home`.

2. **Downgrade refusal tolerates unparseable existing bytes.** First-run case: `std::fs::read` fails (file absent) → proceed with write. Corruption-recovery case: file present but malformed JSON → `from_slice` fails → proceed with write. Only a valid JSON parse with `schema_version > 1` blocks. This avoids the "operator's file got corrupted → wizard now refuses to overwrite forever" failure mode.

3. **`fsync` before `persist`.** `sync_all()` ensures the file's data hits disk before the rename. Without it, a crash between the tempfile write and the rename could leave the rename pointing at an empty file. This is paranoid but cheap.

### 3.5 `deny_unknown_fields` — AMD-11 drift defense

All structs in this spec carry `#[serde(deny_unknown_fields)]`. The immediate motivation: a `config.json` on disk that still has the AMD-11-removed `winlink_password_present` field should hard-fail deserialization, surfacing the drift at load time. Without `deny_unknown_fields`, serde silently ignores unknown fields and the operator/agent can't tell whether their config is up-to-date.

Tradeoff: forward-incompatible. A future AMD that adds a new field to `Config` means an older binary fails to deserialize the new config. Mitigation: schema_version is the explicit forward-incompatibility marker — when adding new fields, bump `CONFIG_SCHEMA_VERSION`, and `deserialize_schema_version` already refuses anything ≠ 1. So `deny_unknown_fields` is belt-and-suspenders within a single schema version.

This is intentional. AMD-1's note "`schema_version` stays at 1 (no shipped users)" confirms we're not optimizing for cross-version JSON tolerance in v0.0.1.

### 3.6 Module structure

All of the above lives in a single `src-tauri/src/config.rs` file. No sub-modules. The file is ~150 LOC; splitting into `config/mod.rs` + `config/validate.rs` + `config/write.rs` would be premature.

`src-tauri/src/lib.rs` already exposes `pub mod config;` — no change needed.

### 3.7 What `tuxlink-4mt` does NOT touch in `pat_process.rs`

The current shipped `pat_process.rs` takes a `config_path: PathBuf` (line 9), not the `Config` struct. Type changes to `Config` therefore do NOT ripple into `pat_process.rs` through the call signature. `pat_process.rs` is updated separately by `tuxlink-756` (which adds the "render Pat's config.json from tuxlink's config" code path).

This isolation is convenient: `tuxlink-4mt` and `tuxlink-756` are theoretically parallelizable (different files, no shared types-in-flux), but per the operator's pacing call, this session does `tuxlink-4mt` first.

---

## 4. Error UX for the consuming layer

`write_config_atomic` returns four error classes. Mapping for the wizard's `wizard_persist_cms` consumer:

| `ConfigWriteError` | Wizard maps to | Operator-visible UX |
|---|---|---|
| `Io(_)` (permission, disk full, ENOSPC) | `WizardError::ConfigWrite { detail }` | "Could not save your settings to `<path>`. Free up disk space or check the directory's permissions." |
| `Serde(_)` (impossible if serde implementation is sound; would indicate a bug) | `WizardError::Other` | "Internal serialization failure. Please file an issue." |
| `SchemaVersionDowngrade { existing, ours }` | `WizardError::Other { detail }` | "Your config file is from a newer version of tuxlink (v{existing}); this binary is v{ours}. Run the newer version or remove `~/.config/tuxlink/config.json` to start fresh." |

The wizard's mapping table belongs in the wizard spec, not here — this spec just guarantees the error variants and their `Display` strings.

---

## 5. Pitfalls discipline — DRIFT-1

Add to [`docs/pitfalls/implementation-pitfalls.md`](../../../docs/pitfalls/implementation-pitfalls.md) under a NEW section "Plan & Documentation Discipline" (the existing `EXAMPLE-DOMAIN-1` stub on `task-amd-main-ui` is unrelated WIP — see §10 below).

```markdown
### DRIFT-1: Plan-text AMENDMENT does not auto-cascade to the code it amends

**The Flaw:** A plan amendment (`> AMENDMENT 2026-MM-DD (AMD-N).`) lands in `docs/plans/*.md` documenting a change to a previously-shipped task's contract. The plan body is updated. The code that the prior task shipped is NOT updated — the AMD is a description of intent, not a code change. Subsequent plans that assume the AMD shipped find the codebase in the pre-AMD shape and fail to compile.

**Why It Matters:** AMDs are cheap (a markdown edit + commit). Code amendments are expensive (bd issue + full pipeline). The asymmetry tempts operators to ship the AMD without the bd issue, especially when the AMD is conceptually simple. The gap is invisible until a downstream task tries to use the new contract — at which point a plan-review-cycle catches it (best case, like wizard-cluster R1 P0-1) or impl ships compile-failing code (worst case).

**The Fix:** Every AMD MUST ship with a paired bd issue if the prior task is "shipped." Two acceptable forms:
1. **Code-bearing AMD:** the AMD body cites the bd issue tracking the code-impl side: "AMD-N. ... Bd issue tracking the code-impl side: tuxlink-XYZ."
2. **Prose-only AMD:** state explicitly that there's no code surface: "AMD-N (prose-only; no code impact)."

Either way, the discipline question is asked at amendment time, not delegated to a future plan-review.

**The Lesson:** The 2026-05-18 wizard-cluster plan-review caught this gap class via R1 P0-1 + R1 P0-3 + Codex R4 P0 #2 (cross-validated across providers). `tuxlink-4mt` retroactively cleared AMD-1 + AMD-11's code-impl gap. The fix is to never accumulate the gap in the first place.

**Review checklist** (added to Section 1 checklist): every PR that lands an AMD must enumerate the AMD-N IDs in the PR description, and for each cite either (a) the bd issue tracking the code-impl side, or (b) "prose-only; no code impact."
```

This is a cross-cutting discipline pitfall, not a code-domain one — it belongs in a new section. The SCOPE-1 entry currently uncommitted on `task-amd-main-ui` (about RMS Express vs RMS Trimode) is also cross-cutting but its own domain — both can coexist as separate sections.

---

## 6. Test plan

All tests live in `src-tauri/tests/config_test.rs`. The existing 4 flat-schema tests are replaced wholesale.

| # | Test name | Source / lens | Asserts |
|---|---|---|---|
| 1 | `test_deserialize_minimal_cms_config` | plan body, AMD-11-adjusted | nested CMS-path JSON round-trips |
| 2 | `test_deserialize_offline_config` | plan body, AMD-11-adjusted | offline JSON round-trips |
| 3 | `test_reject_wrong_schema_version` | plan body | `schema_version: 99` rejected by deserializer |
| 4 | `test_cms_path_requires_callsign` | plan body | `Config::validate` fires `CmsPathMissingCallsign` |
| 5 | `test_identity_validator_loose` | plan body | each `validate_identity` rule accepts/rejects expected inputs |
| 6 | `test_config_path_uses_xdg_config_home_when_set` | shipped (preserved) | XDG path resolution unchanged |
| 7 | `test_reject_winlink_password_present_field` | **NEW** — AMD-11 drift defense | JSON carrying stale `winlink_password_present` hard-fails via `deny_unknown_fields` |
| 8 | `test_write_atomic_first_run_creates_file` | **NEW** — `write_config_atomic` happy path | absent file → file present after write, contents round-trip-deserialize |
| 9 | `test_write_atomic_overwrites_v1_file` | **NEW** | existing same-version file is replaced |
| 10 | `test_write_atomic_refuses_schema_version_downgrade` | **NEW** — plan-review R3 P0-2 | JSON with `schema_version: 99` blocks the write; original file preserved |
| 11 | `test_write_atomic_overwrites_unparseable_file` | **NEW** — corruption-recovery semantics | malformed-JSON existing file does NOT block the write |

Tests 7-11 use `tempfile::TempDir` + `std::env::set_var("XDG_CONFIG_HOME", tmp.path().to_str().unwrap())` to redirect `config_path()` into a per-test directory. Cleanup is automatic via `TempDir::drop`. The env-var manipulation is not parallel-safe across tests; if Rust's default test parallelism causes contention, gate with `serial_test::serial` (added as a dev-dependency). For now, all tests are written assuming serial execution within their `mod` — Rust's `cargo test` runs tests in parallel by default, so any env-var-mutating test that races is a real risk.

**Mitigation chosen:** Use unique per-test sub-directories via `TempDir::new().unwrap().path().join(format!("test-{}", test_name))` so each test's `XDG_CONFIG_HOME` resolves to a distinct path. The env-var is set per-test (still racy if two tests run in parallel and both call `config_path()` at the same time), but the *file paths* don't collide. The remaining race is on the env-var read inside `config_path()`; if observed flaky, promote to `#[serial]`.

Codex R4 P3 "Mock outcome alternation makes tests order-dependent" applies here in spirit — tests should not depend on shared mutable state. The env-var is the shared state; the mitigation is to scope it per-test.

---

## 7. File changes inventory

| File | Action | Approximate LOC |
|---|---|---|
| `src-tauri/src/config.rs` | REWRITE | ~150 (was 54) |
| `src-tauri/tests/config_test.rs` | REWRITE | ~180 (was 56) |
| `src-tauri/Cargo.toml` | UPDATE — add thiserror, promote tempfile | ~2 lines |
| `src-tauri/src/lib.rs` | NO CHANGE | — |
| `docs/pitfalls/implementation-pitfalls.md` | ADD — DRIFT-1 entry + new section | ~40 lines |
| `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` | UPDATE — Task 2 "Pre-amendment shape (historical)" subsection cites tuxlink-4mt as implementer | ~3 lines |
| `docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md` | THIS FILE | ~400 |
| `docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md` | TO BE CREATED (Phase 3 of build-robust-features) | — |

---

## 8. Cargo.toml deltas

```toml
[dependencies]
# … existing …
thiserror = "1"            # NEW — for ConfigValidationError + ConfigWriteError

[dependencies]
# … existing …
tempfile = "3"             # PROMOTED — from [dev-dependencies] to [dependencies]; needed by write_config_atomic

[dev-dependencies]
# tempfile entry REMOVED — promoted above
# … rest unchanged …
```

`thiserror` is stable + tiny + standard in the Rust ecosystem; no review concern. `tempfile` promotion is the change that warrants attention — it adds a runtime dep that ships with the binary instead of just being a test concern. Tradeoff accepted: writing the atomic-file machinery by hand (manual `OpenOptions` + `link` + `unlink`) is error-prone and tempfile's `NamedTempFile::persist` is the idiomatic implementation. The crate is small and dependency-free.

---

## 9. Concerns the 5-round adrev should attack

These are the design calls I'm least confident about, prioritized for adversarial review:

1. **`deny_unknown_fields` on every sub-struct.** Over-strict? Are there practical use cases (operator hand-editing the JSON, future binary tolerating older configs) where silent unknown-field tolerance would be more humane? Codex P3 will probably surface this. My current call: drift defense > tolerance; AMD-1's "no shipped users" backs this up. But worth challenging.

2. **`write_config_atomic` parent-dir vs path-resolution race.** `std::fs::create_dir_all(parent)` then `NamedTempFile::new_in(parent)` — what if another process deletes `parent` between the two calls? My current call: tolerate (treat as unrecoverable; the operator's keyring also broke). But maybe the right call is to retry once.

3. **`SchemaVersionDowngrade` tolerance of unparseable bytes.** I picked tolerance for corruption-recovery, but Codex R4 P1 ("snapshot rollback swallows too many keyring errors") is a sibling concern in spirit: too-permissive recovery semantics. Should `SchemaVersionDowngrade`-class errors include "existing file present but unparseable" as a separate variant? Currently no.

4. **`fsync` before `persist`.** Defensive but slow. Alternative: skip fsync and trust the OS pagecache. For config-write this is operator-visible state; the cost of a missed write on crash (operator re-runs wizard) is low. Maybe drop fsync.

5. **`validate_identity` is `pub fn`, not `Config::validate_identity`.** The plan body has it as `pub fn` (free-standing) — I matched the plan. But cross-cutting use from `wizard.rs` would suggest it could be a method or in a `validators` sub-module. Current call: match plan; revisit if wizard impl finds friction.

6. **No `read_config` in this spec.** Wizard plan does NOT call `read_config`; it calls `write_config_atomic` only. But the orphan-keyring startup check (wizard spec §5.3, plan-review R3 P0-1 + Codex R4 P1) needs to READ the current config. Should `read_config` ship in `tuxlink-4mt` too, or as a separate bd issue? Current call: out of scope here; defer to a sibling issue. Adrev may push back.

7. **DRIFT-1 placement.** New section vs extension of an existing one. The existing pitfalls doc structure (Section 0 RADIO + Section 1 stub EXAMPLE-DOMAIN-1 + Section 2 stub EXAMPLE-DOMAIN-2) means adding a Section 1 "Plan & Documentation Discipline" would COLLIDE with the in-flight SCOPE-1 on `task-amd-main-ui`. Need to coordinate: this spec lands SCOPE-1 in §1 and DRIFT-1 in §2? Or DRIFT-1 in §3? See §10.

8. **`tempfile` promotion.** Should `write_config_atomic` instead use stdlib-only `OpenOptions` + manual rename? Tradeoff: more LOC + manual handling of `O_EXCL` race; less dependency. Current call: tempfile.

---

## 10. Coordination with uncommitted state on `task-amd-main-ui`

There is uncommitted in-progress work on the main checkout's `task-amd-main-ui` branch that adds **SCOPE-1** (a different cross-cutting pitfall, about RMS Express vs RMS Trimode) as Section 1 in `implementation-pitfalls.md`. That work is NOT in any handoff doc and predates the maple-magpie-oak session.

**This spec's DRIFT-1 entry needs a section number that doesn't collide.** Two options:

- **Coordinate at PR time:** if SCOPE-1 lands first as §1, DRIFT-1 lands here as §2 (renaming the existing EXAMPLE-DOMAIN-2 stub or supplementing it). If DRIFT-1 lands first, vice versa.
- **Defer DRIFT-1 to a separate PR:** ship the code surface in this PR; ship DRIFT-1 in a follow-up that's coordinated with whatever happens to SCOPE-1.

Decision deferred to plan-write phase. The shape doesn't change — DRIFT-1 will land. The numbering is the only open question.

---

## 11. Open questions resolved (defaults baked in, see §9 for adrev attack surface)

| # | Question | Default chosen | Why |
|---|---|---|---|
| 1 | `deny_unknown_fields`? | Yes (all sub-structs) | AMD-11 drift defense; AMD-1's "no shipped users" backs the cost |
| 2 | DRIFT-1 placement | New section "Plan & Documentation Discipline" | Cross-cutting docs discipline; not a code-domain entry |
| 3 | Pre-AMD-1 flat-config migration | Hard break (no shim) | AMD-1's own punt: "zero shipped users" |
| 4 | thiserror + tempfile dep tier | Both to `[dependencies]` (tempfile promoted) | Required by runtime path |
| 5 | Test suite shape | 11-test wholesale replace | Existing 4 are pre-AMD-1; partial migration adds confusion |
| 6 | `read_config` in scope? | No — defer to sibling bd issue | Wizard plan calls only `write_config_atomic`; orphan-keyring read is a separate concern |
| 7 | fsync before persist? | Yes (default) | Cheap and defensive; revisit if observable startup latency |
| 8 | `validate_identity` method-or-fn? | Free-standing `pub fn` | Matches plan body verbatim |

---

## 12. Pipeline ahead

1. ✅ Brainstorm (this document)
2. **NEXT:** 5-round cross-provider adrev (4 Claude lenses + 1 Codex) targeting §9's attack surface + the spec body
3. Spec revision applying P0/P1 findings
4. Plan write via `writing-plans` skill → `docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md`
5. 4-round plan-review cycle (R1 friction + R2 contract + R3 coverage Claude + R4 cross-provider Codex)
6. Plan revision
7. TDD impl via subagent-driven-development in this worktree
8. `cargo test --test config_test` is the gate (no browser smoke — no UI surface)
9. Pitfalls PR description enumerates AMD-N → bd-issue mappings (DRIFT-1's own discipline)
10. PR opens against `feat/v0.0.1`; merges close `tuxlink-4mt` via deliverable

Per memory `feedback_no_carveout_on_cross_provider_adrev`: no upstream skip even though the design is largely settled by AMD-1 + AMD-11.

---

## 13. References

- **Plan body Task 2 (post-AMD-1):** `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` lines ~280-690
- **AMD-1 amendment:** `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` line 283
- **AMD-11 amendment:** `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` line 285
- **Wizard cluster spec (downstream consumer):** `docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md`
- **Wizard cluster plan-review R1 P0-1 + P0-3:** `dev/adversarial/2026-05-18-wizard-plan-review-R1-friction-claude.md` (gitignored; in `worktrees/bd-tuxlink-ln3-wizard-cluster-spec/`)
- **Wizard cluster plan-review R3 P0-2:** `dev/adversarial/2026-05-18-wizard-plan-review-R3-coverage-claude.md` (gitignored)
- **Wizard cluster plan-review R4 (Codex):** `dev/adversarial/2026-05-18-wizard-plan-review-R4-cross-provider-codex.md` (gitignored)
- **Cred-handling spec (sibling that established the keyring contract):** `docs/superpowers/specs/2026-05-18-cred-handling-design.md`
- **bd issue:** `tuxlink-4mt` (P1, HARD blocker on `tuxlink-ln3`)
