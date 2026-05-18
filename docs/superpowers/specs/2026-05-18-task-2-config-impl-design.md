# Task 2 Config Implementation — Design Spec

**Spec ID:** tuxlink-4mt
**Date:** 2026-05-18 (v2 — post-spec-adrev revision)
**Author:** agent `fox-cove-towhee`
**Status:** revised — 4 Claude adrev rounds (R1-R4) applied; Codex R5 deferred to plan-review phase (quota-exhausted, per memory `feedback_codex_quota_gotcha`)
**Branch:** `bd-tuxlink-4mt/task-2-config-impl` (worktree off `feat/v0.0.1`)
**Closes via deliverable:** the PR that merges this spec's implementation into `feat/v0.0.1`

---

## 1. Why this spec exists

Two plan amendments — [AMD-1 (2026-05-17)](../../plans/2026-04-22-tuxlink-v0.0.1-plan.md) and AMD-11 (2026-05-18) — replaced Task 2's flat `Config` schema with a nested shape and dropped the `winlink_password_present` field. **The amendments shipped as plan-text only. The code in [`src-tauri/src/config.rs`](../../../src-tauri/src/config.rs) was never updated** — it still carries the pre-AMD-1 flat schema with `winlink_password_present`.

This gap was caught by the 2026-05-18 wizard-cluster plan-review-cycle:
- **R1 P0-1** (friction lens, Claude): `config.rs` is pre-AMD-1.
- **R1 P0-3** (friction lens, Claude): `validate_identity()` doesn't exist.
- **R4 P0 #2** (cross-provider, Codex): cross-validated R1's finding.
- **R3 P0-2** (coverage lens, Claude): wizard plan calls `write_config_atomic()` which also doesn't exist.

This spec closes that gap. It is a **HARD prerequisite** for the wizard-cluster impl plan (`tuxlink-ln3`). Beyond fixing the immediate gap, it codifies the gap *class* as a pitfalls entry (`DRIFT-1`) so future plan amendments file the paired bd issue at amendment time.

### 1.1 v2 revision changes from v1

This v2 supersedes v1 (committed 2ae9abb) after the 4-round Claude spec adrev cycle:

| Round | Findings | Top P0 applied in v2 |
|---|---|---|
| R1 friction | 16 (3 P0, 6 P1, 5 P2, 2 P3) | `validate_identity` signature; Cargo.toml single block; `deserialize_schema_version` body inlined |
| R2 contract | 12 (3 P0, 4 P1, 4 P2, 1 P3) | `read_config` in-scope; atomic-write atomicity scoped; `ConfigValidationError::InvalidIdentity { field, rule }` |
| R3 coverage | 15 (4 P0, 5 P1, 4 P2, 2 P3) | Offline-rejects-callsign rule; DRIFT-1 → §2; per-variant round-trip tests |
| R4 failure-mode | 15 (3 P0, 8 P1, 3 P2, 2 P3) | Parent-dir fsync; symlink detection; cross-process docs; probe-read error distinguishes NotFound |

Codex R5 hit ChatGPT-mode quota at 20:43 UTC; will retry during plan-review R4 slot per the memory.

---

## 2. Scope

### 2.1 In scope

1. Replace flat `Config` struct in `src-tauri/src/config.rs` with the AMD-1 nested shape: `Config { schema_version, wizard_completed, connect: ConnectConfig, identity: IdentityConfig, privacy: PrivacyConfig, pat_mbo_address }`.
2. Drop `winlink_password_present` per AMD-11.
3. Implement `validate_identity(s: &str) -> bool` per AMD-1's loose-validator semantics. **(v2: signature changed from `Result<(), String>` per R1 P0-1 + R2 P0-1 — see §3.2.)**
4. Implement `validate_identity_describe(s: &str) -> Option<&'static str>` — companion that returns a static error-string slug for the first rule violated, or `None` if the input passes. Used by `Config::validate` to synthesize structured errors.
5. Implement `Config::validate(&self) -> Result<(), ConfigValidationError>` for cross-field rules: CMS-path-requires-callsign AND **offline-path-rejects-callsign (v2 add per R3 P0-2)**, plus identity-field validation via `validate_identity`.
6. Implement `read_config() -> Result<Config, ConfigReadError>` — **v2 add per R2 P0-2 + R3 P0-1**. Required by wizard plan line 525 + line 617.
7. Implement `write_config_atomic(config: &Config) -> Result<(), ConfigWriteError>` — same-directory tempfile + persist + **parent-dir fsync (v2 add per R2 P0-3 + R4 P0-1)** + schema-version mismatch refusal + **symlink-detection (v2 add per R4 P0-2)** + **probe-read error distinguishes NotFound from other I/O (v2 add per R4 P1-4)**.
8. Replace the 4 flat-schema tests in `src-tauri/tests/config_test.rs` with a **20-test suite** (v2: grew from 11 — added enum round-trip tests + offline-rejects-callsign + Some("") handling + probe-read error class tests + read_config tests).
9. Add `thiserror = "1"` + `tempfile = "3"` to `[dependencies]`; add `serial_test = "3"` to `[dev-dependencies]` for env-var-mutating tests (v2 per R1 P1-6).
10. Add `DRIFT-1` pitfalls entry to `docs/pitfalls/implementation-pitfalls.md` as **§2 "Plan & Documentation Discipline"** (replacing the existing EXAMPLE-DOMAIN-2 stub) — **v2 resolution per R3 P0-3** (SCOPE-1 already shipped to §1; §10 of v1 was based on stale state).
11. Update the plan body's Task 2 "Pre-amendment shape (historical)" section to cite `tuxlink-4mt` as the implementing bd issue.

### 2.2 Out of scope

- **Migration code for pre-AMD-1 flat configs.** Per AMD-1's "zero shipped users" punt.
- **AuxAddr / multi-callsign keyring entries.** Per AMD-13's single-callsign scope.
- **Settings UI surface** for post-wizard editing of these fields.
- **Wizard's `wizard_persist_cms` and `wizard_persist_offline` Tauri commands.** Those belong to `tuxlink-ln3`.
- **`validate_grid(s)` cross-field grid format validator.** v2 declines per R2 P2-2 — the wizard's TypeScript validator owns grid format checking; if downstream consumers need a Rust-side grid validator, file a separate bd issue. Reason: the grid validator's regex is wizard-domain knowledge (AMD-1's Maidenhead semantics) and adding it here would expand scope beyond AMD-1+AMD-11 cascade closure.
- **Callsign canonical-form (uppercase, trimmed) enforcement at deserialize time.** v2 declines per R2 P2-1 — the wizard normalizes at write time; hand-edited config that has non-canonical callsign passes validation but then the keyring lookup falls through. Worth a pitfalls note in a future revision, not a Config::validate rule today.
- **Flock-based cross-process write serialization.** v2 declines per R4 P0-3 — instead, this spec DOCUMENTS the single-instance assumption (§3.4 + §5) and recommends a startup guard that's a separate bd issue (out of scope here).
- **Multi-MB hostile-config size guard on read.** R1 P2-5 + R4 P1-3 noted potential CPU burn on hostile inputs. v0.0.1 threat model is operator-local; out of scope.

### 2.3 Dependency map

This spec is upstream of:
- `tuxlink-ln3` (wizard impl plan — HARD blocker)
- `tuxlink-756` (Task 3 PatProcess amendment — soft dependency; reads Config via `read_config`)

**Co-blockers for the wizard impl plan that this spec does NOT resolve (per R3 P2-1):**
- Wizard plan Task 1.4: pin the `keyring` crate version (3.6.x feature-flagged or keyring-core path per wizard plan-review R1 P1-2 + Codex R4 P0 #1). This is wizard-domain; tuxlink-4mt unblocks Phase 3 + Phase 4 of the wizard plan, NOT Phase 1.

bd dep edges: `tuxlink-ln3` already blocks-on `tuxlink-4mt`.

---

## 3. Design

### 3.1 Public surface (v2)

```rust
// All in src-tauri/src/config.rs.
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
    // winlink_password_present REMOVED per AMD-11.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectConfig {
    pub connect_to_cms: bool,
    pub transport: CmsTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]                    // v2 per R1 P1-1 — defensive no-op locking wire format
pub enum CmsTransport { CmsSsl, Telnet }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    #[serde(deserialize_with = "deserialize_optional_nonempty_string")]   // v2 per R4 P1-1 — maps "" → None
    pub callsign: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_nonempty_string")]
    pub identifier: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_nonempty_string")]
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]                    // v2 — defensive
pub enum GpsState { Off, LocalUiOnly, BroadcastAtPrecision }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]                    // v2 — defensive
pub enum PositionPrecision { FourCharGrid, SixCharGrid }

#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("CMS path requires identity.callsign to be set")]
    CmsPathMissingCallsign,
    #[error("offline path must NOT have identity.callsign set (use identity.identifier instead)")]
    OfflinePathHasCallsign,                            // v2 add per R3 P0-2
    #[error("invalid identity field `{field}`: {rule}")]
    InvalidIdentity { field: &'static str, rule: &'static str },   // v2 per R2 P1-2 — preserves field identity for wizard mapping
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigValidationError>;
}

// v2 surface — split signature per R1 P0-1 + R2 P0-1.
// `validate_identity` returns bool, matching the wizard spec's call site (`!validate_identity(&callsign)`).
// `validate_identity_describe` returns the first-violated-rule slug, used by `Config::validate` to synthesize structured errors.
pub fn validate_identity(s: &str) -> bool;
pub fn validate_identity_describe(s: &str) -> Option<&'static str>;

pub fn config_path() -> std::path::PathBuf;            // unchanged from shipped

#[derive(Debug, thiserror::Error)]
pub enum ConfigReadError {                             // v2 add per R2 P0-2 + R3 P0-1
    #[error("config file not found at {path}")]
    NotFound { path: std::path::PathBuf },
    #[error("io error reading {path}: {source}")]
    Io { path: std::path::PathBuf, #[source] source: std::io::Error },
    #[error("config deserialize failed: {source}")]
    Serde { #[source] source: serde_json::Error },
    #[error("config failed semantic validation: {source}")]
    Validation { #[source] source: ConfigValidationError },
}

pub fn read_config() -> Result<Config, ConfigReadError>;

#[derive(Debug, thiserror::Error)]
pub enum ConfigWriteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config serialize failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("refuse to overwrite existing config with schema_version {existing} (this binary supports v{ours}): mismatch — either downgrade (existing > ours) or backward-incompat (existing < ours)")]
    SchemaVersionMismatch { existing: u32, ours: u32 },        // v2 renamed per R4 P1-5 — covers both directions
    #[error("refuse to overwrite existing config at {path}: file is a symlink (target: {target:?})")]
    ExistingFileIsSymlink { path: std::path::PathBuf, target: Option<std::path::PathBuf> },  // v2 per R4 P0-2
    #[error("config path {path} cannot be probed: {source}")]
    ProbeReadFailed { path: std::path::PathBuf, #[source] source: std::io::Error },          // v2 per R4 P1-4 — non-NotFound errors from probe-read
    #[error("config path {path} has no parent directory")]
    NoParentDirectory { path: std::path::PathBuf },            // v2 per R2 P2-3 — typed instead of .expect() panic
}

pub fn write_config_atomic(config: &Config) -> Result<(), ConfigWriteError>;

// PRIVATE helpers — placed directly above write_config_atomic per Rust convention.
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
    // v2 per R4 P1-1: map JSON `null` → None; map JSON `""` → None (treat empty-string as missing);
    // map non-empty string → Some(s). Removes the Some("") ambiguity that R4 flagged.
    let opt = <Option<String>>::deserialize(d)?;
    Ok(opt.filter(|s| !s.is_empty()))
}

#[derive(serde::Deserialize)]
struct SchemaVersionProbe { schema_version: u32 }
```

**Stability contract (v2 per R2 P1-1):** the `Display` strings on every `ConfigWriteError`, `ConfigReadError`, and `ConfigValidationError` variant are **STABLE PUBLIC SURFACE** — the wizard's error-UX mapping table interpolates them into operator-visible messages via `format!("{e}")`. Any future change to a Display string is a breaking change for the wizard's UX tests.

**`#[from]` discipline (v2 per R4 P3-1):** the `#[from]` attributes on `ConfigWriteError::Io` and `ConfigWriteError::Serde` are intentional. Future contributors adding new variants via `#[from]` must consider whether the type-inference engine could produce ambiguous From-chain resolution.

**Re-exports (v2 per R1 P2-3):** All public surface lives under `tuxlink_lib::config::*`. No top-level re-exports needed (wizard spec uses the qualified path).

### 3.2 `validate_identity` — loose validator with bool signature

Per AMD-1: non-empty + no internal whitespace + ≤32 chars + ASCII-printable. Per R2 P1-3 + R4 P1-2, rule ordering matters for operator UX — ASCII-check fires first to give the most actionable error.

```rust
/// Loose identity validator. Matches Express's hs30.htm "checked for basic syntax" semantics:
/// non-empty + ASCII-printable + no internal whitespace + ≤32 chars (in that order so the most
/// actionable error fires first). The CMS is authoritative for actual callsign / tactical-address
/// acceptance.
///
/// Returns `true` if `s` passes ALL rules; `false` otherwise. Use `validate_identity_describe`
/// to obtain the first-violated-rule slug for error synthesis.
pub fn validate_identity(s: &str) -> bool {
    validate_identity_describe(s).is_none()
}

/// Returns `Some(static-rule-slug)` for the FIRST rule violated, or `None` if input passes all rules.
/// Rule order: empty → ASCII → whitespace → length (most-actionable first per R2 P1-3 + R4 P1-2).
pub fn validate_identity_describe(s: &str) -> Option<&'static str> {
    if s.is_empty() { return Some("must not be empty"); }
    if s.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) { return Some("must be ASCII-printable"); }
    if s.chars().any(char::is_whitespace) { return Some("must not contain whitespace"); }
    if s.chars().count() > 32 { return Some("must be ≤32 chars"); }
    None
}
```

`★ Insight ─────────────────────────────────────`
**The signature split addresses R1 P0-1 + R2 P0-1 without losing information.** The wizard spec at line 151 (`!validate_identity(&callsign)`) consumes `bool`; that consumer is shipped and the v2 signature matches. The describe-helper preserves the per-rule error string for `Config::validate`'s structured error synthesis. The plan body's `Result<(), String>` form is split into two specialized functions — one optimized for the wizard's negation pattern, one for error synthesis.
`─────────────────────────────────────────────────`

### 3.3 `Config::validate` — cross-field validation (v2)

```rust
impl Config {
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Rule 1 — CMS path requires callsign.
        if self.connect.connect_to_cms && self.identity.callsign.is_none() {
            return Err(ConfigValidationError::CmsPathMissingCallsign);
        }

        // Rule 2 (v2 add per R3 P0-2) — offline path must NOT have callsign.
        if !self.connect.connect_to_cms && self.identity.callsign.is_some() {
            return Err(ConfigValidationError::OfflinePathHasCallsign);
        }

        // Rule 3 — identity fields pass validate_identity loose rules.
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

**Validation invariants explicitly NOT in this spec** (deferred per §2.2):
- Callsign canonical form (`s == s.trim().to_uppercase()`).
- Grid Maidenhead format (regex `^[A-R]{2}[0-9]{2}([a-x]{2})?$`).
- `pat_mbo_address` RFC 822-ish shape (Pat owns this).

**`write_config_atomic` and `Config::validate` relationship (v2 per R1 P1-3):** `write_config_atomic` does NOT auto-call `Config::validate`. Callers (wizard, future read-then-modify paths) are responsible for invoking validation before passing a `Config` to `write_config_atomic`. Rationale: tests and future internal users may want to write deliberately-malformed configs to exercise edge cases; auto-validation would block. Documented choice; not a defense-in-depth gap.

### 3.4 `write_config_atomic` (v2)

```rust
pub fn write_config_atomic(config: &Config) -> Result<(), ConfigWriteError> {
    let path = config_path();
    let parent = path.parent()
        .ok_or_else(|| ConfigWriteError::NoParentDirectory { path: path.clone() })?;
    std::fs::create_dir_all(parent)?;

    // Symlink-detection (v2 per R4 P0-2): refuse to silently replace a symlink.
    // Operators using dotfiles workflows symlink config.json; rename(2) replaces the
    // symlink itself, not its target, silently breaking their backup flow. Detect and abort.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() {
            return Err(ConfigWriteError::ExistingFileIsSymlink {
                path: path.clone(),
                target: std::fs::read_link(&path).ok(),
            });
        }
    }

    // Schema-version mismatch refusal (v2 renamed from Downgrade per R4 P1-5).
    // Covers both directions: existing file with schema_version != ours blocks the write.
    // Tolerates unparseable bytes (first-run + corruption-recovery cases).
    match std::fs::read(&path) {
        Ok(bytes) => {
            if let Ok(probe) = serde_json::from_slice::<SchemaVersionProbe>(&bytes) {
                if probe.schema_version != CONFIG_SCHEMA_VERSION {
                    return Err(ConfigWriteError::SchemaVersionMismatch {
                        existing: probe.schema_version, ours: CONFIG_SCHEMA_VERSION,
                    });
                }
            }
            // Unparseable bytes: silently overwrite (corruption recovery). Documented in §3.5.
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // First-run case — proceed with write. Not an error.
        }
        Err(e) => {
            // v2 per R4 P1-4: EACCES, EIO, etc. — abort with typed error rather than silently
            // proceeding (which would have destroyed an unreadable existing file).
            return Err(ConfigWriteError::ProbeReadFailed { path: path.clone(), source: e });
        }
    }

    // Same-directory tempfile → atomic persist on local POSIX FS.
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(tmp.as_file(), config)?;
    tmp.as_file().sync_all()?;  // file data durable
    let persisted_path = tmp.persist(&path)
        .map_err(|e| ConfigWriteError::Io(e.error))?;

    // Parent-dir fsync (v2 per R2 P0-3 + R4 P0-1) — rename(2) is atomic but not DURABLE
    // until the parent directory's metadata flushes. tempfile::persist does not do this for you.
    // Open parent and sync_all() to issue fsync(2) on the directory inode.
    let parent_dir = std::fs::File::open(parent)?;
    parent_dir.sync_all()?;
    drop(persisted_path);  // satisfy unused-result lint without consuming the path
    Ok(())
}
```

**Atomicity contract scope (v2 per R2 P0-3):**

- **Guarantees:** atomic + durable on local POSIX filesystems (ext4, btrfs, xfs, APFS) where `$XDG_CONFIG_HOME` and the target file are on the same filesystem AND the same BTRFS subvolume.
- **NOT guaranteed:** NFS, FUSE filesystems, Lustre, GlusterFS — `rename(2)` semantics vary by FS implementation; the wizard's transactional pair (keyring + config) should not assume atomicity on these.
- **BTRFS subvolume boundary:** if `parent` and the target file span a BTRFS subvolume boundary, `tempfile::persist` falls back to copy-then-delete, which is NOT atomic. Detection is not feasible at write time; documented as an operator constraint.

**Single-instance assumption (v2 per R4 P0-3):** This spec assumes one tuxlink instance writes to `~/.config/tuxlink/config.json` at a time. Cross-process serialization (`flock(2)` on a sibling `.config.lock` or a startup guard refusing to launch when a peer is detected) is out of scope; if multi-instance support is added later, file a separate bd issue. Today's failure mode: two concurrent writers both return `Ok(())`; whichever rename lands second wins; first writer's data is silently lost.

**Backup-tool .tmp visibility (v2 per R4 P1-7):** `NamedTempFile::new_in(parent)` creates a file under `~/.config/tuxlink/` with the default tempfile naming pattern (`.tmpXXXXXX`). Backup tools watching the directory may briefly capture this file. The tempfile is short-lived (microseconds) and is removed atomically by `persist`'s rename. Documented as expected behavior; no startup-cleanup machinery in v0.0.1.

### 3.5 `read_config` (v2 add)

```rust
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

**Consumers (cross-referenced from wizard plan):**
- `wizard_persist_offline` (wizard plan line 525): `crate::config::read_config().ok()` — uses `.ok()` to fold any error into `None` (first-run, malformed, etc.) and falls through to a fresh wizard.
- `get_wizard_completed` Tauri command (wizard plan line 617): same `.ok()` pattern.

**deny_unknown_fields + read_config:** A config.json carrying an AMD-11-style drift field (e.g., `winlink_password_present`) will fail at the `from_slice` line with `ConfigReadError::Serde`. The wizard's `.ok()` pattern means the operator sees an "unconfigured" state and the wizard re-runs — clean recovery via the drift defense.

### 3.6 `deny_unknown_fields` — AMD-11 drift defense (v2 clarification)

All structs carry `#[serde(deny_unknown_fields)]`. The defense fires on the **read path** (via `read_config`), not the write path (the `SchemaVersionProbe` is a narrow read for downgrade refusal only). Per R2 P1-3: drift via stale fields surfaces when something tries to deserialize the existing config, NOT when `write_config_atomic` overwrites it.

Tradeoff explicitly accepted: forward-incompatible. Adding a new field to `Config` in a future schema version (`schema_version=2`) means an older binary fails to deserialize the new config. Mitigation: `deserialize_schema_version` rejects anything ≠ 1 at the strict-deserialize boundary, which catches this case before unknown-field analysis. `deny_unknown_fields` is belt-and-suspenders within a single schema version.

### 3.7 Module structure

All in `src-tauri/src/config.rs` (~250 LOC post-revision; up from ~150 in v1 due to symlink/parent-fsync/read_config additions). No sub-modules. `src-tauri/src/lib.rs` already has `pub mod config;` — no change.

---

## 4. Error UX for the consuming layer (v2)

`write_config_atomic` + `read_config` return typed errors that the wizard maps to `WizardError` variants. Stable Display strings flow through `format!("{e}")` into operator-visible messages.

| Variant | Wizard maps to | Operator-visible UX |
|---|---|---|
| `ConfigWriteError::Io(_)` | `WizardError::ConfigWrite { detail: e.to_string() }` | "Could not save your settings to `<path>`. Details: io error: Permission denied" |
| `ConfigWriteError::SchemaVersionMismatch` | `WizardError::Other { detail: e.to_string() }` | "Your config file is from a different schema version (v2; this binary expects v1). Run the matching version or remove `~/.config/tuxlink/config.json` to start fresh." |
| `ConfigWriteError::ExistingFileIsSymlink` | `WizardError::Other { detail: e.to_string() }` | "Tuxlink refused to overwrite `<path>` because it's a symlink to `<target>`. If you're using a dotfiles workflow, edit the target file directly OR remove the symlink." |
| `ConfigWriteError::ProbeReadFailed` | `WizardError::ConfigWrite { detail }` | "Could not read the existing config to check for version conflicts. Details: io error: Permission denied. (Tuxlink refused to silently overwrite it.)" |
| `ConfigWriteError::NoParentDirectory` | `WizardError::ConfigWrite { detail }` | "Config path has no parent directory. Check XDG_CONFIG_HOME / HOME env vars." |
| `ConfigReadError::NotFound` | (wizard treats as first-run; renders Step 1) | (no error UX; wizard launches normally) |
| `ConfigReadError::Serde` | `WizardError::Other { detail }` | "Existing config is malformed: <serde detail>. Edit or remove `<path>` and re-run the wizard." |
| `ConfigReadError::Validation` | (wizard treats as malformed; same as Serde class) | same as Serde class |
| `ConfigValidationError::CmsPathMissingCallsign` | `WizardError::InvalidInput { field: "callsign" }` | "CMS connection requires a callsign." |
| `ConfigValidationError::OfflinePathHasCallsign` | `WizardError::InvalidInput { field: "callsign" }` | "Offline mode shouldn't have a callsign set. Use the offline identifier field instead." |
| `ConfigValidationError::InvalidIdentity { field, rule }` | `WizardError::InvalidInput { field }` + log rule | "<field>: <rule>" (e.g., "callsign: must not contain whitespace") |

The exact mapping lives in the wizard spec, not here. This table documents the CONTRACT — Display strings are stable; wizard relies on `format!("{e}")` interpolation; `field` from `InvalidIdentity` threads to `WizardError::InvalidInput`'s `field` discriminant.

---

## 5. Pitfalls discipline — DRIFT-1 (v2 placement resolved)

Add to [`docs/pitfalls/implementation-pitfalls.md`](../../../docs/pitfalls/implementation-pitfalls.md) as **§2 "Plan & Documentation Discipline"**, replacing the existing EXAMPLE-DOMAIN-2 stub at line ~284 of the file. SCOPE-1 is already in §1 (verified 2026-05-18 against the worktree's pitfalls.md per R3 P0-3); §2 is the natural slot.

```markdown
# Section 2: Plan & Documentation Discipline

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

### Section 2 Review Checklist

- [ ] **Check derived from DRIFT-1** — Any PR that lands an AMENDMENT in a plan or spec includes either (a) a cited bd issue tracking the code-impl side, or (b) explicit "prose-only; no code impact" framing. Verify by searching the PR body for AMENDMENT markers and confirming each carries the cite or the explicit punt.
- [ ] **Check derived from DRIFT-1** — When amending a shipped spec (e.g., adding fields to `WizardError` or changing a function signature in `validate_identity`), the PR identifies every downstream consumer (via `grep -r 'consumer-symbol'`) and files paired bd issues for each consumer that needs adaptation.
- [ ] **Check derived from DRIFT-1** — Pipeline cycles for code amendments inherited from plan amendments use the FULL build-robust-features pipeline (brainstorm → 5-round adrev → spec → 4-round plan-review → revision → TDD impl) — the discipline of `tuxlink-4mt` itself. Skipping upstream phases to "ship the AMD-cascade fix faster" defeats the purpose of catching the gap class.

---
```

The corresponding entry in `testing-pitfalls.md` (per R3 P1-4) is filed as a follow-up bd issue: "Add DRIFT-1 verification recipe to testing-pitfalls.md" — testing-pitfalls.md is likely in a similar stub state and addressing it is out of scope for tuxlink-4mt.

---

## 6. Test plan (v2 — 20 tests)

Test file: `src-tauri/tests/config_test.rs`. The existing 4 flat-schema tests are replaced wholesale. `#[serial_test::serial]` applied UNCONDITIONALLY to every test that mutates `XDG_CONFIG_HOME` (v2 per R1 P1-6 — no "if observed flaky" deferral).

| # | Test name | Source | Asserts |
|---|---|---|---|
| 1 | `test_deserialize_minimal_cms_config` | plan body, AMD-11-adjusted | nested CMS-path JSON round-trips; `winlink_password_present` ABSENT |
| 2 | `test_deserialize_offline_config` | plan body, AMD-11-adjusted | offline JSON round-trips |
| 3 | `test_reject_wrong_schema_version` | plan body | `schema_version: 99` rejected by `deserialize_schema_version` |
| 4 | `test_validate_cms_path_requires_callsign` | plan body | `Config::validate` fires `CmsPathMissingCallsign` |
| 5 | `test_validate_offline_path_rejects_callsign` | **v2 add per R3 P0-2** | `Config::validate` fires `OfflinePathHasCallsign` |
| 6 | `test_validate_invalid_identity_propagates_field` | **v2 add per R2 P1-2** | malformed callsign vs malformed identifier produce `InvalidIdentity { field, rule }` with right field |
| 7 | `test_validate_identity_loose_rules` | plan body + R2 P1-3 rule-order | `validate_identity` accepts canonical examples + rejects each rule violation with the right `describe` slug; rule order: empty → ASCII → whitespace → length |
| 8 | `test_validate_identity_describe_accepts_passes` | **v2 add** | `validate_identity_describe(s)` returns `None` for accepted inputs (round-trip check vs `validate_identity` bool) |
| 9 | `test_config_path_uses_xdg_config_home_when_set` | shipped (preserved) | XDG path resolution unchanged. `#[serial_test::serial]` |
| 10 | `test_reject_amd11_dropped_field_winlink_password_present` | **v2 renamed from test 7 per R4 P1-6** + inlined fixture per R1 P1-5 | JSON with stale top-level `winlink_password_present: true` field hard-fails via `deny_unknown_fields` |
| 11 | `test_deny_unknown_fields_on_each_substruct` | **v2 add per R3 P2-2** | unknown field on `ConnectConfig`, `IdentityConfig`, `PrivacyConfig` each independently rejects |
| 12 | `test_cms_transport_telnet_variant_round_trips` | **v2 add per R3 P1-1** | `transport: "Telnet"` deserializes correctly |
| 13 | `test_gps_state_three_variants_round_trip` | **v2 add per R3 P1-2** | all three `GpsState` variants serialize + deserialize symmetrically |
| 14 | `test_position_precision_two_variants_round_trip` | **v2 add per R3 P1-2** | both `PositionPrecision` variants round-trip |
| 15 | `test_empty_string_identity_field_normalizes_to_none` | **v2 add per R4 P1-1** | `callsign: ""` in JSON deserializes to `Some(None)`-equivalent (via `deserialize_optional_nonempty_string`); offline-rejects-callsign rule then NOT fired |
| 16 | `test_write_atomic_first_run_creates_file` | spec | absent file → file present after write, contents round-trip via `read_config`. `#[serial]` |
| 17 | `test_write_atomic_overwrites_v1_file` | spec | existing same-version file is replaced. `#[serial]` |
| 18 | `test_write_atomic_refuses_schema_version_mismatch` | **v2 renamed from Downgrade per R4 P1-5** + tests both directions (v0 + v99) | JSON with `schema_version: 99` OR `schema_version: 0` blocks the write; existing file preserved. `#[serial]` |
| 19 | `test_write_atomic_overwrites_unparseable_file` | spec | malformed-JSON existing file does NOT block. `#[serial]` |
| 20 | `test_write_atomic_refuses_existing_symlink` | **v2 add per R4 P0-2** | existing `config.json` is a symlink → `ExistingFileIsSymlink` returned; symlink + target unchanged. `#[serial]` |
| 21 | `test_write_atomic_probe_read_eacces_fails_typed` | **v2 add per R4 P1-4** | existing file with restrictive permissions (chmod 000) → `ProbeReadFailed`; original file preserved. `#[serial]` |
| 22 | `test_read_config_not_found_returns_typed_error` | **v2 add per §3.5** | empty XDG dir → `ConfigReadError::NotFound`. `#[serial]` |
| 23 | `test_read_config_serde_returns_typed_error_on_malformed_json` | **v2 add per §3.5** | malformed bytes → `ConfigReadError::Serde`. `#[serial]` |
| 24 | `test_read_config_validation_runs_after_deserialize` | **v2 add per §3.5** | valid JSON shape but offline-with-callsign → `ConfigReadError::Validation`. `#[serial]` |

(24 tests, not 20 — count grew in revision as I traced findings.)

**Test 16 + parent-dir fsync verification:** v2's parent-dir fsync (§3.4) is hard to unit-test directly without crash-simulation. Test 16 verifies that `write_config_atomic` returns `Ok(())` after the parent-dir-fsync attempt; if the parent isn't readable, `File::open(parent)?` propagates `Io` error and the test will fail. The actual durability is observable only on power loss; documented as expected.

**Test fixtures (v2 per R1 P1-5):** Test 10's fixture is inlined:
```rust
let json = r#"{
    "schema_version": 1, "wizard_completed": true,
    "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
    "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
    "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
    "pat_mbo_address": null,
    "winlink_password_present": true
}"#;
```

The stale field is at the TOP LEVEL (where the pre-AMD-1 schema had it), not nested — this is where the real drift surfaces.

---

## 7. File changes inventory (v2)

| File | Action | LOC est. |
|---|---|---|
| `src-tauri/src/config.rs` | REWRITE | ~250 (v1 estimated ~150) |
| `src-tauri/tests/config_test.rs` | REWRITE | ~350 (v1 estimated ~180; +read_config + symlink + EACCES + 4 new validator tests) |
| `src-tauri/Cargo.toml` | UPDATE — single `[dependencies]` block (v2 per R1 P0-2) | ~3 lines |
| `src-tauri/src/lib.rs` | NO CHANGE | — |
| `docs/pitfalls/implementation-pitfalls.md` | UPDATE — replace EXAMPLE-DOMAIN-2 stub with §2 Plan & Documentation Discipline + DRIFT-1 | ~50 lines |
| `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` | UPDATE — Task 2 historical section cites tuxlink-4mt | ~3 lines |
| `docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md` | THIS FILE (v2 supersedes v1) | ~600 |
| `docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md` | TO BE CREATED (Phase 3 of build-robust-features) | — |

---

## 8. Cargo.toml deltas (v2 — single [dependencies] block)

The CURRENT shipped `src-tauri/Cargo.toml` has one `[dependencies]` table and one `[dev-dependencies]` table. v2 modifies both — single change per table:

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
tauri-plugin-fs = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json", "blocking", "multipart"] }
tokio = { version = "1", features = ["full"] }
nix = { version = "0.28", features = ["signal", "process"] }
thiserror = "1"            # NEW — for ConfigValidationError + ConfigReadError + ConfigWriteError
tempfile = "3"             # PROMOTED from [dev-dependencies]; needed by write_config_atomic

[dev-dependencies]
mockito = "1.5"
serial_test = "3"          # NEW (v2 per R1 P1-6) — gates env-var-mutating tests against parallel-test races
# tempfile entry REMOVED — promoted above
```

---

## 9. Concerns the plan-review (and deferred Codex R5) should attack

After 4 rounds of Claude adrev applied to v2, the remaining open design calls worth Codex pushback:

1. **Symlink-detection-and-refuse vs silently-follow.** v2 chose detect+refuse with explicit error (R4 P0-2). Defensible alternative: follow the symlink (matches `cp` default behavior; operator's dotfiles workflow keeps working). Open: which is the right default for an operator-facing config-write?

2. **`Some("")` normalized to `None` via custom deserializer.** v2 added `deserialize_optional_nonempty_string` (R4 P1-1). Side effect: operator can no longer distinguish "intentionally blank" from "absent" — both surface as `None`. Defensible alternative: keep `Some("")` and add `EmptyIdentity` variant. Adrev may prefer the explicit variant.

3. **Single-instance assumption documented, no flock.** v2 declines `flock(2)` (R4 P0-3). Operator multi-window-instance scenarios rare in v0.0.1 but possible if Settings UI lands in v0.1. Codex may push for the flock anyway.

4. **`SchemaVersionMismatch` blocks both directions.** v2 widened from `Downgrade` (R4 P1-5). Side effect: a v0 config (hypothetical historical artifact) blocks the write rather than silently overwriting. Open: should the spec distinguish "intentional pre-AMD-1 punt" from "future-version block" with different error variants?

5. **`validate_identity` returning `bool` + describe-helper pair.** v2 chose the split (R1 P0-1 + R2 P0-1). Alternative: ship `Result<(), &'static str>` (the original-with-static-string-rules) which gives the bool via `.is_ok()` and the rule via `.err()`. Cleaner; symmetric with `Result<(), ConfigValidationError>` elsewhere. Codex may prefer this.

6. **`Config::validate` not auto-called by `write_config_atomic`.** v2 documented as caller-responsibility (R1 P1-3). Defense-in-depth alternative: auto-validate, with a `ConfigWriteError::ValidationFailed` variant. Decision: caller-responsibility (less surprising; tests may want to write malformed configs).

7. **DRIFT-1 testing-pitfalls.md companion entry deferred to sibling bd issue.** v2 punted (R3 P1-4). Plan-review may push back.

---

## 10. Open questions resolved (v2 — defaults baked in; superseding v1 §10 + §11)

| # | Question | v2 default | Rationale |
|---|---|---|---|
| 1 | `validate_identity` return type | `bool` + companion `validate_identity_describe(s) -> Option<&'static str>` | Matches shipped wizard spec (R1 P0-1 + R2 P0-1); split preserves error-string info for `Config::validate` |
| 2 | `read_config` in scope? | Yes, ship in this PR | Wizard plan line 525 + 617 literally call it (R2 P0-2 + R3 P0-1); deferring was an unfiled intention |
| 3 | Parent-dir fsync after persist | Yes (open `parent`, `sync_all()`) | Atomic ≠ durable on POSIX (R2 P0-3 + R4 P0-1) |
| 4 | FS scope bounds | Local POSIX (ext4/btrfs/xfs/APFS); NFS undefined; BTRFS subvolume boundary breaks atomicity | R2 P0-3 |
| 5 | Symlink existing config | Refuse with typed error (`ExistingFileIsSymlink`) | More defensive than silent follow (R4 P0-2) |
| 6 | Cross-process serialization | Document single-instance; no flock | v0.0.1 scope (R4 P0-3); future bd issue for multi-instance |
| 7 | Probe-read error handling | Distinguish NotFound (proceed) from other Err (`ProbeReadFailed`) | R4 P1-4 — silently destroying unreadable files is unacceptable |
| 8 | Schema version probe shape | `SchemaVersionMismatch` (both directions; v0 + v99 both block) | R4 P1-5 — align probe with `deserialize_schema_version` strictness |
| 9 | `Config::validate` orthogonality | Both rules: CMS-requires-callsign AND offline-rejects-callsign | R3 P0-2 — half-enforcement undermines AMD-1 |
| 10 | `ConfigValidationError::InvalidIdentity` shape | `{ field: &'static str, rule: &'static str }` | R2 P1-2 — preserves field identity for wizard's `InvalidInput { field }` |
| 11 | `Some("")` normalization | `deserialize_optional_nonempty_string` maps to `None` | R4 P1-1 — eliminates ambiguity |
| 12 | DRIFT-1 placement | §2 of pitfalls (replacing EXAMPLE-DOMAIN-2 stub) | R3 P0-3 — SCOPE-1 already shipped to §1 |
| 13 | Enum serde rename_all | `#[serde(rename_all = "PascalCase")]` defensive no-op | R1 P1-1 — locks wire format against good-taste refactors |
| 14 | Error enum Serialize derive | NO Serialize; wizard maps via `.to_string()` not `#[from]` | R1 P1-2 — keeps surface minimal |
| 15 | Display string stability | Stable public surface; documented in §3.1 | R2 P1-1 |
| 16 | Test parallel-safety | `serial_test = "3"` unconditional `#[serial]` on env-var tests | R1 P1-6 — "if observed flaky" was guaranteed flakiness |
| 17 | `write_config_atomic` calls `Config::validate`? | No; caller responsibility, documented | R1 P1-3 |
| 18 | thiserror + tempfile dep tier | Both `[dependencies]`; tempfile promoted; serial_test added to dev-deps | R1 P0-2 — single `[dependencies]` block |
| 19 | Cross-language callsign canonical form (validate at deserialize?) | Defer to separate bd issue (out of scope per §2.2) | R2 P2-1 — wizard normalizes; hand-edited drift is a pitfalls note |
| 20 | Grid format validator | Defer to separate bd issue | R2 P2-2 — wizard's TS validator owns this |
| 21 | DRIFT-1 testing-pitfalls.md companion | Defer to sibling bd issue | R3 P1-4 — testing-pitfalls.md likely also stub |

---

## 11. Pipeline ahead

1. ✅ Brainstorm (v1 spec at 2ae9abb)
2. ✅ 4-round Claude spec adrev (R1-R4); Codex R5 deferred to plan-review per quota gotcha
3. ✅ Spec revision (this v2)
4. **NEXT:** Commit v2 spec + push (in this worktree)
5. Plan write via `writing-plans` skill → `docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md`
6. 4-round plan-review cycle (R1 friction + R2 contract + R3 coverage Claude + R4 cross-provider Codex; if Codex still quota'd at that point, defer R4 again and surface)
7. Plan revision
8. TDD impl via subagent-driven-development in this worktree
9. `cd src-tauri && cargo test --test config_test` is the gate (no browser smoke — no UI surface)
10. PR opens against `feat/v0.0.1`; merges close `tuxlink-4mt` via deliverable

Per memory `feedback_no_carveout_on_cross_provider_adrev`: no upstream skip even though the design is largely settled by AMD-1 + AMD-11 + 4 rounds of adrev. Per memory `feedback_codex_quota_gotcha`: Codex R5 deferral here is capacity-defer, not choice-skip — will run at plan-review's R4 slot.

---

## 12. References

- **Plan body Task 2 (post-AMD-1):** `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` lines ~280-690
- **AMD-1 amendment:** line 283
- **AMD-11 amendment:** line 285
- **Wizard cluster spec (downstream consumer):** `docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md` (line 151 for `validate_identity` call site)
- **Wizard cluster plan (Phase 1 + 3 + 4 consumers):** `docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md` (line 525 + 617 + 1087 + 1092 for `crate::config::*` calls)
- **Spec adrev R1 (friction, Claude):** `dev/adversarial/2026-05-18-task-2-config-spec-adrev-R1-friction-claude.md`
- **Spec adrev R2 (contract, Claude):** `dev/adversarial/2026-05-18-task-2-config-spec-adrev-R2-contract-claude.md`
- **Spec adrev R3 (coverage, Claude):** `dev/adversarial/2026-05-18-task-2-config-spec-adrev-R3-coverage-claude.md`
- **Spec adrev R4 (failure-mode, Claude):** `dev/adversarial/2026-05-18-task-2-config-spec-adrev-R4-failure-mode-claude.md`
- **Spec adrev R5 (Codex, deferred — quota):** `dev/adversarial/2026-05-18-task-2-config-spec-adrev-R5-cross-provider-codex.md` (54 lines; quota error captured)
- **Wizard cluster plan-review R1-R4** (cross-referenced in §1): `dev/adversarial/2026-05-18-wizard-plan-review-R[1-4]-*.md` in `worktrees/bd-tuxlink-ln3-wizard-cluster-spec/`
- **Cred-handling spec (sibling):** `docs/superpowers/specs/2026-05-18-cred-handling-design.md`
- **bd issue:** `tuxlink-4mt` (P1, HARD blocker on `tuxlink-ln3`)
