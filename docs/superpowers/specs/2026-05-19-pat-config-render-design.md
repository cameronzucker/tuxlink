# Pat Config Render at PatProcess Spawn — Design Spec

**Spec ID:** tuxlink-756
**Date:** 2026-05-19 (pre-adrev v1)
**Author:** agent `badger-oak-dahlia`
**Status:** pre-adrev — awaiting Codex cross-provider round (no-carveout floor per [`feedback_no_carveout_on_cross_provider_adrev`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_no_carveout_on_cross_provider_adrev.md))
**Branch:** `bd-tuxlink-756/pat-config-render` (worktree off `feat/v0.0.1` post-PR#67-merge)
**Closes via deliverable:** the PR that merges this spec's implementation into `feat/v0.0.1`
**Discipline:** tightly-scoped per the bd-issue body's "design is settled by cred-handling spec + AMD-13 + this gap" framing; per-round scope is tight, ≥1 Codex round mandatory.

---

## 1. Why this spec exists

The pre-cred-refactor reality: Pat's config.json was operator-managed or pre-existing; `PatProcess::spawn` accepted a `config_path: PathBuf` in `PatSpawnOptions` and passed it via `--config <path>` to the Pat binary. The file's existence was the caller's responsibility.

The post-cred-refactor (`tuxlink-mib`, shipped [tuxlink-pat#2](https://github.com/cameronzucker/tuxlink-pat/pull/2) + [tuxlink#59](https://github.com/cameronzucker/tuxlink/pull/59)) reality: Pat's config struct no longer carries `SecureLoginPassword` or `AuxAddr.Password`; passwords come from the OS keyring at runtime. **The wizard writes the keyring entry + tuxlink's `config.json`, but does NOT write Pat's `config.json`.** AMD-13 makes this explicit.

The resulting gap:
1. Wizard completes → tuxlink config written, keyring entry written.
2. User opens tuxlink → tuxlink spawns Pat via `PatProcess::spawn`.
3. Pat reads `--config <path>` → **file does not exist** (wizard never wrote it).
4. Pat starts with empty defaults → no callsign, no locator → nonfunctional.

Codex R5 P1 on the wizard cluster spec adrev (§7.3 follow-up item 4) surfaced this and filed `tuxlink-756`. The wizard cluster spec §3 (lines 179-181) explicitly defers the fix to here: *"the wizard does NOT write Pat's config directly — that's Task 3 (`PatProcess`, `tuxlink-b9d`)'s responsibility: when Task 3 spawns Pat, it RENDERS Pat's config from tuxlink's config (the wizard's persisted state) + the keyring callsign convention."*

This spec closes that gap. It introduces a new module `pat_config.rs` that owns Pat-config rendering, and amends `PatProcess::spawn` to call it before exec.

---

## 2. Scope

### 2.1 In scope

1. New module `src-tauri/src/pat_config.rs` containing:
   - `render_pat_config(tuxlink_config: &crate::config::Config) -> Result<String, PatConfigError>` — returns Pat's `config.json` as a JSON string. Pure function (no I/O), enables unit-testing.
   - `write_pat_config_atomic(tuxlink_config: &crate::config::Config, dest: &Path) -> Result<(), PatConfigError>` — calls `render_pat_config`, then writes atomically to `dest` (same-directory tempfile + persist + parent-dir fsync; mirrors `config::write_config_atomic`).
   - `PatConfigError` typed-error enum (variants: `MissingRequiredField`, `RenderFailed`, `Io`, `OfflineModeNoConfigNeeded`).
   - `pub const PAT_CONFIG_SCHEMA_FIELDS` — documented set of Pat config keys this renderer populates (for self-documentation + future-drift detection).
2. Amend `src-tauri/src/pat_process.rs`:
   - `PatProcess::spawn` calls `write_pat_config_atomic` BEFORE exec, writing to `opts.config_path`.
   - Existing `config_path: PathBuf` field semantics change from "caller-provided existing file" to "destination for the rendered config." Add a `tuxlink_config: Config` field to `PatSpawnOptions` so spawn knows what to render. (Existing test code passes a static hand-written config_path JSON — must update; see §3.7.)
   - Error mapping: `PatConfigError::MissingRequiredField` → `std::io::Error` with `ErrorKind::InvalidInput`; other `PatConfigError` variants → corresponding `io::ErrorKind`. (PatProcess::spawn returns `std::io::Result<Self>` today; preserve that signature in v0.0.1.)
3. Tests (`src-tauri/tests/pat_config_test.rs`, new):
   - **6 tests** per §4 — at the upper-middle of the tight-scope range.
4. Update existing `src-tauri/tests/pat_process_test.rs` to construct `PatSpawnOptions` with a `tuxlink_config: Config` field instead of writing a hand-rolled `config.json` to disk.
5. No new external dependencies. `serde_json` is already a dep; `tempfile` already a dep.

### 2.2 Out of scope

- **Wizard changes.** AMD-13 already specifies the wizard writes tuxlink config + keyring. No wizard-side change needed.
- **`external/tuxlink-pat` fork changes.** Pat's cred-refactor (tuxlink-pat#2) is shipped; this spec doesn't touch Go code.
- **MBO address mapping.** `tuxlink.pat_mbo_address` is the operator's Winlink email address (`<callsign>@winlink.org`); Pat derives this from `MyCall` internally and has NO corresponding field in its `Config` struct (post-refactor `cfg/config.go` confirmed). The field exists in tuxlink config for UI display purposes only. Pat-config render does NOT map it.
- **AuxAddrs.** v0.0.1 is single-callsign scope per AMD-13. The rendered Pat config has `auxiliary_addresses: []`. Multi-callsign operators provision additional `(service="tuxlink-pat", account=AUXCALLSIGN)` keyring entries manually per AMD-11; their Pat AuxAddrs come from `~/.config/pat/config.json` if they hand-edit it, but tuxlink does not write them.
- **Radio transports (Hamlib, AX25, AGWPE, SerialTNC, Ardop, Pactor, VaraHF, VaraFM, GPSd, Prediction).** v0.0.1 supports CMS-Telnet / CMS-SSL only. Rendered Pat config leaves these as zero values (their `IsZero()` methods return true → JSON omits them per `omitzero` tags where applicable).
- **HTTPAddr.** `PatProcess::spawn` already passes `--addr <listen>` which overrides the config-file `http_addr`. Rendered config leaves `http_addr` empty; the CLI flag wins.
- **MOTD, ConnectAliases, Schedule, VersionReportingDisabled.** Empty/default. Not relevant to v0.0.1 client-side operation.
- **Offline-mode Pat config.** When `tuxlink_config.connect.connect_to_cms = false`, no Pat should be spawned (tuxlink runs in offline mode). `write_pat_config_atomic` returns `PatConfigError::OfflineModeNoConfigNeeded` if called with an offline config, treating this as a caller bug.
- **Schema-version negotiation for Pat config.** Pat's `Config::UnmarshalJSON` ignores unknown fields (Go's default). Tuxlink's renderer emits only the v0.0.1 minimum-viable subset; future fields are forward-compatible.
- **Migration / cleanup of pre-existing operator-managed Pat configs.** If `~/.config/pat/config.json` already exists from a prior standalone-Pat install, `write_pat_config_atomic` overwrites it. Acceptable for v0.0.1 (per cred-handling spec §5.1 "zero shipped users"); the upstream-PR-variant audience addressed separately.

### 2.3 Dependency map

This spec is upstream of:
- **`tuxlink-ln3`** (wizard cluster impl) — HARD prerequisite. Wizard impl plan assumes this spec's PatProcess amendment lands (or has landed). Confirmed by the wizard cluster spec §3 line 179.

No other bd issues block on this one.

---

## 3. Design

### 3.1 Pat Config schema (target)

Post-cred-refactor Pat `Config` struct ([tuxlink-pat origin/master `cfg/config.go`](https://github.com/cameronzucker/tuxlink-pat/blob/master/cfg/config.go)) — fields tuxlink-756 cares about:

| Pat field | JSON key | tuxlink source | v0.0.1 value |
|---|---|---|---|
| `MyCall` | `mycall` | `tuxlink.identity.callsign` | required when CMS path; copied verbatim |
| `AuxAddrs` | `auxiliary_addresses` | (none; AMD-13 single-callsign scope) | `[]` always |
| `Locator` | `locator` | `tuxlink.identity.grid` | copied at full 6-char precision when present; `""` otherwise |
| `AutoDownloadSizeLimit` | `auto_download_size_limit` | (default) | `-1` (no limit; Pat's safe default) |
| `ServiceCodes` | `service_codes` | (default) | `["PUBLIC"]` (Pat's standard default; most amateur traffic) |
| `HTTPAddr` | `http_addr` | (not rendered; CLI flag wins) | `""` |
| `MOTD` | `motd` | (none) | `[]` |
| `ConnectAliases` | `connect_aliases` | (none) | `{}` |
| `Listen` | `listen` | (none; v0.0.1 doesn't accept inbound) | `[]` |
| `HamlibRigs` | `hamlib_rigs` | (none) | `{}` |
| `AX25 / AX25Linux / AGWPE / SerialTNC / Ardop / Pactor / Telnet / VaraHF / VaraFM` | per-field | (none) | zero values |
| `GPSd` | `gpsd` | (none) | zero value |
| `Prediction` | `prediction` | (none) | zero value (omitzero) |
| `Schedule` | `schedule` | (none) | `{}` |
| `VersionReportingDisabled` | `version_reporting_disabled` | (none) | `false` (Pat default) |

The renderer emits a minimal-but-complete JSON document. Pat's `UnmarshalJSON` defaults `AutoDownloadSizeLimit` to `-1` when the key is absent (legacy-config tolerance), so explicit `-1` is belt-and-suspenders.

### 3.2 Render function

```rust
// src-tauri/src/pat_config.rs

use serde::Serialize;
use std::path::Path;
use thiserror::Error;

use crate::config::Config as TuxlinkConfig;

/// Pat config schema fields populated by this renderer. Kept as a sorted
/// const slice so future drift between this renderer and Pat's actual
/// expected fields is easy to inspect.
pub const PAT_CONFIG_SCHEMA_FIELDS: &[&str] = &[
    "auto_download_size_limit",
    "auxiliary_addresses",
    "http_addr",
    "locator",
    "mycall",
    "service_codes",
];

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PatConfigError {
    /// Required field missing from tuxlink config — caller passed a Config
    /// that doesn't have enough information to render a working Pat config.
    /// Most common: `connect_to_cms=true` but `identity.callsign` is None.
    #[error("Pat config render: required field missing: {0}")]
    MissingRequiredField(String),

    /// Caller passed an offline-mode tuxlink config; no Pat config should
    /// be written when tuxlink runs in offline mode (no Pat process spawned).
    /// This is a caller bug; the calling code should not invoke Pat config
    /// render when `connect.connect_to_cms = false`.
    #[error("Pat config render called with offline-mode tuxlink config")]
    OfflineModeNoConfigNeeded,

    /// serde_json::to_string failed during render — should never happen
    /// for our schema (no Float NaN, no map with non-string keys).
    /// Carries source for forensics.
    #[error("Pat config render: serde error: {0}")]
    RenderFailed(#[source] serde_json::Error),

    /// File I/O failed during atomic write (tempfile creation, persist,
    /// parent fsync, etc.). Source preserved.
    #[error("Pat config write: I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Render the Pat `config.json` content from tuxlink's config. Pure
/// function — no I/O. Returns the JSON as a `String`.
///
/// Returns `Err(MissingRequiredField)` if `connect.connect_to_cms = true`
/// but `identity.callsign` is `None`. Returns `Err(OfflineModeNoConfigNeeded)`
/// if `connect.connect_to_cms = false`.
pub fn render_pat_config(tuxlink_config: &TuxlinkConfig) -> Result<String, PatConfigError> {
    if !tuxlink_config.connect.connect_to_cms {
        return Err(PatConfigError::OfflineModeNoConfigNeeded);
    }
    let callsign = tuxlink_config
        .identity
        .callsign
        .as_deref()
        .ok_or_else(|| {
            PatConfigError::MissingRequiredField("identity.callsign".to_string())
        })?;

    let pat_config = PatConfigDto {
        mycall: callsign.to_string(),
        auxiliary_addresses: vec![],
        locator: tuxlink_config
            .identity
            .grid
            .as_deref()
            .unwrap_or("")
            .to_string(),
        auto_download_size_limit: -1,
        service_codes: vec!["PUBLIC".to_string()],
        http_addr: String::new(),
    };

    serde_json::to_string_pretty(&pat_config).map_err(PatConfigError::RenderFailed)
}

/// Render and atomically write Pat config to `dest`. Same-directory tempfile
/// + persist + parent-dir fsync pattern mirroring `crate::config::write_config_atomic`.
///
/// Creates `dest`'s parent directory if it does not exist (matches the
/// `XDG_CONFIG_HOME/pat/` first-run case).
pub fn write_pat_config_atomic(
    tuxlink_config: &TuxlinkConfig,
    dest: &Path,
) -> Result<(), PatConfigError> {
    let json = render_pat_config(tuxlink_config)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tempfile::NamedTempFile::new_in(
        dest.parent().unwrap_or_else(|| Path::new(".")),
    )?;
    std::fs::write(tmp.path(), json.as_bytes())?;
    // Persist (atomic rename). NamedTempFile::persist returns a File on
    // success; ignore the returned File (drops cleanly).
    tmp.persist(dest).map_err(|e| PatConfigError::Io(e.error))?;
    // Best-effort parent-dir fsync to flush the directory entry; matches
    // config::write_config_atomic discipline (per tuxlink-4mt v2 R4 P0-1).
    if let Some(parent) = dest.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

/// Wire-format DTO for Pat's Config struct. Field names match Pat's
/// `json:"..."` tags (snake_case). Only fields tuxlink-756 populates;
/// Pat's UnmarshalJSON tolerates missing fields per Go's default behavior.
#[derive(Debug, Serialize)]
struct PatConfigDto {
    mycall: String,
    auxiliary_addresses: Vec<String>, // always empty in v0.0.1
    locator: String,
    auto_download_size_limit: i64,
    service_codes: Vec<String>,
    http_addr: String,
}
```

### 3.3 Field mapping decisions (atomic; per `feedback_no_atomic_decisions_to_operator`)

These are converged in this spec; Codex round refines:

1. **`identity.callsign` is required.** Per AMD-1 + AMD-13, the CMS path requires a callsign. If `connect_to_cms=true` and `callsign=None`, that's an invariant violation — `Config::validate` would have rejected it at config-write time. The render function fails fast.

2. **`identity.grid` is optional, copied as-is when present.** Pat tolerates empty Locator (some operators omit it). Privacy precision-reduction per `Principle 7` is a separate runtime concern (broadcast-time, not config-file-time); the FULL grid lives in Pat's config so Pat's protocol layer uses it for CMS routing without further truncation.

3. **`auxiliary_addresses` is always empty `[]` in v0.0.1.** Multi-callsign operators set their AuxAddrs manually post-wizard if they want them. Tuxlink doesn't render them per AMD-13.

4. **`auto_download_size_limit: -1`.** Matches Pat's UnmarshalJSON default for omitted key + matches Express's "auto-download everything" behavior. Future setting: operator-tunable in v0.5+ Settings.

5. **`service_codes: ["PUBLIC"]`.** Pat's documented default. Most amateur traffic uses PUBLIC; EmComm-only operators set additional codes manually.

6. **`http_addr: ""`.** Empty intentionally — `PatProcess::spawn` passes `--addr 127.0.0.1:<port>` which overrides. Leaving empty avoids confusion if the config is inspected by hand.

7. **No `omitempty` on the DTO.** All fields explicitly emitted, even when default. Operators inspecting the rendered file see the schema clearly; future debugging is easier. Cost: slightly larger JSON file (~150 bytes typical).

8. **`serde_json::to_string_pretty`** for human-readable output. Operators may hand-inspect the file via `cat ~/.config/pat/config.json`; pretty-printing aids debug. Cost: ~10% larger; negligible.

### 3.4 `PatSpawnOptions` change

```rust
pub struct PatSpawnOptions {
    pub binary: PathBuf,
    pub config_path: PathBuf,         // now: DESTINATION for rendered config (was: existing file)
    pub mbox_dir: PathBuf,
    pub http_listen_port: u16,
    pub pid_file: PathBuf,
    pub log_sink: Option<std::sync::mpsc::Sender<String>>, // existing from tuxlink-z5f
    /// NEW (tuxlink-756): tuxlink's config, used to render Pat's config at
    /// spawn time. Required because the wizard writes tuxlink config +
    /// keyring entry but NOT Pat config (AMD-13). PatProcess::spawn calls
    /// `pat_config::write_pat_config_atomic(tuxlink_config, config_path)`
    /// before exec.
    pub tuxlink_config: crate::config::Config,
}
```

`PatProcess::spawn` modification — add before the `Command::new` line:

```rust
crate::pat_config::write_pat_config_atomic(&opts.tuxlink_config, &opts.config_path)
    .map_err(|e| match e {
        crate::pat_config::PatConfigError::Io(io_err) => io_err,
        crate::pat_config::PatConfigError::MissingRequiredField(field) => std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("PatProcess::spawn: tuxlink config missing required field: {field}"),
        ),
        crate::pat_config::PatConfigError::OfflineModeNoConfigNeeded => std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "PatProcess::spawn called with offline-mode tuxlink config; do not spawn Pat in offline mode",
        ),
        crate::pat_config::PatConfigError::RenderFailed(serde_err) => std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PatProcess::spawn: Pat config render failed: {serde_err}"),
        ),
    })?;
```

### 3.5 Existing tests (pat_process_test.rs) — required adaptation

Existing tests construct `PatSpawnOptions` and write a hand-rolled `pat-config.json` to disk before calling spawn. After this change:
- Drop the manual `std::fs::write(&opts.config_path, ...)` lines.
- Construct a valid `Config` via the public API (`Config { schema_version: 1, wizard_completed: true, connect: ConnectConfig { connect_to_cms: true, transport: CmsTransport::CmsSsl }, identity: IdentityConfig { callsign: Some("TEST1".to_string()), identifier: None, grid: Some("AA00aa".to_string()) }, privacy: ..., pat_mbo_address: None }`).
- Pass it as `tuxlink_config: my_config` in `PatSpawnOptions`.
- Behavior is unchanged: Pat sees the same effective config (mycall="TEST1", locator="AA00aa", no password — Pat reads from keyring, which the test ENV doesn't set, so any actual CMS session would fail; but the existing tests only test spawn + shutdown, not session, so this is fine).

### 3.6 Error model (PatConfigError variants)

| Variant | When | Recoverable? | Notes |
|---|---|---|---|
| `MissingRequiredField(field)` | callsign None in CMS path | No (caller bug) | `Config::validate` should have rejected this upstream; defense-in-depth |
| `OfflineModeNoConfigNeeded` | called with `connect_to_cms=false` | No (caller bug) | Caller should not spawn Pat in offline mode |
| `RenderFailed(serde_err)` | serde_json::to_string failed | No (impossible for our schema) | Surfaced for forensics; should never fire |
| `Io(std::io::Error)` | tempfile/persist/fsync failed | Maybe (disk full, EACCES, etc.) | Most-common runtime failure mode |

`#[non_exhaustive]` on the enum per the pattern established in `BackendError` (tuxlink-z5f v2 P1 #5).

### 3.7 Offline-mode discipline

When `connect.connect_to_cms = false`:
- The wizard's offline path (Task 11.5) runs.
- Tuxlink's main app should **not** call `PatProcess::spawn` — there's no CMS connectivity to provide.
- If something does call spawn with an offline config, this spec's defense-in-depth catches it: `render_pat_config` returns `OfflineModeNoConfigNeeded`; `PatProcess::spawn` propagates as `io::Error(InvalidInput)`.

The lifecycle decision ("when do we spawn Pat?") is OUT of scope for this spec — it lives in the future Tauri command surface that mounts/dismounts the WinlinkBackend. The render function just provides a defensive failure mode for an incorrect call.

---

## 4. Test plan (6 tests)

Tests live in `src-tauri/tests/pat_config_test.rs` (new file). No tokio runtime needed — pure synchronous I/O.

| # | Test name | What it verifies |
|---|---|---|
| 1 | `test_render_pat_config_emits_expected_fields_for_minimal_cms_config` | `render_pat_config(minimal_cms_config())` returns JSON containing all 6 keys from `PAT_CONFIG_SCHEMA_FIELDS`; `mycall`/`locator` match input; `auxiliary_addresses` is `[]`; `auto_download_size_limit` is `-1`; `service_codes` is `["PUBLIC"]`; `http_addr` is `""`. |
| 2 | `test_render_pat_config_with_empty_grid_emits_empty_locator` | When `identity.grid = None`, rendered JSON has `locator: ""`. |
| 3 | `test_render_pat_config_missing_callsign_returns_missing_required_field` | When `connect_to_cms = true` but `identity.callsign = None`, returns `Err(MissingRequiredField("identity.callsign"))`. |
| 4 | `test_render_pat_config_offline_mode_returns_offline_mode_error` | When `connect_to_cms = false`, returns `Err(OfflineModeNoConfigNeeded)`. |
| 5 | `test_write_pat_config_atomic_creates_parent_dir_and_writes_file` | Pass a destination path with non-existent parent. After `write_pat_config_atomic`, the parent dir exists, the file exists, and the file contents equal `render_pat_config(&same_config).unwrap()`. |
| 6 | `test_write_pat_config_atomic_overwrites_existing_file` | Pre-create the dest with bogus content. After `write_pat_config_atomic`, content equals the new render. (Validates the atomic-overwrite via tempfile + persist.) |

**Why 6 (not 24+):** per the bd-issue's tight-scope framing — the renderer has small surface area: one happy path + 2 input-error paths + 2 I/O paths = 5 minimum + 1 atomic-overwrite property test. Edge cases that DON'T need tests here:
- Schema-version mismatch — N/A; we render a fixed schema, no version negotiation.
- Concurrent writes — `tempfile::NamedTempFile::persist` is OS-atomic on the local filesystem (per `config::write_config_atomic`'s analogous design rationale in `tuxlink-4mt` spec §3.4).
- File-mode permissions — Pat config carries no secrets post-refactor; OS default (umask) is fine.
- Hostile inputs — operator-local trust boundary per `tuxlink-4mt` spec §2.2; same model.

---

## 5. Open questions (Codex-converge targets)

The Codex cross-provider round (1 round per bd-issue's no-carveout floor) should focus on:

1. **Field-mapping completeness.** Did I miss a Pat config field tuxlink needs to set for v0.0.1 functionality? Per §3.1, I render `mycall`, `locator`, `auxiliary_addresses`, `auto_download_size_limit`, `service_codes`, `http_addr` and leave everything else at Pat's zero defaults. Codex should cross-reference Pat 1.0.0's actual session-initialization code (`app/app.go`, `connect.go`) to verify nothing else is required.

2. **`service_codes: ["PUBLIC"]` correctness.** Is `PUBLIC` always the right default? Some EmComm circuits use different codes. Codex: check Pat's CLI default and verify omitting service_codes (relying on Pat's default) vs. explicit `["PUBLIC"]` produces identical behavior. If yes, prefer omission.

3. **HTTPAddr empty vs. CLI flag.** Pat 1.0.0's behavior: does `--addr` actually override `http_addr` cleanly, or does Pat fail if both are set / both are empty? I assume CLI-wins-over-config based on common patterns; Codex: verify.

4. **`auto_download_size_limit: -1` semantics.** Pat's `Config::UnmarshalJSON` defaults missing key to `-1` (no limit). Setting `-1` explicitly is redundant but defensive. Codex: any pitfall in explicit-vs-implicit here?

5. **Parent-dir fsync error swallowing.** §3.2's `write_pat_config_atomic` does `let _ = dir.sync_all()` — silently ignoring fsync failures. Same pattern as `config::write_config_atomic` (per tuxlink-4mt v2 R4 P0-1). Codex: any reason to surface fsync errors here? (Position: no; durability is best-effort for non-secret config; matches established pattern.)

6. **Race between Pat reading config and tuxlink renaming the temp.** `tempfile::NamedTempFile::persist` uses `rename(2)` which is atomic on the local FS. Once `PatProcess::spawn` invokes `Command::spawn`, Pat is loading the binary; by the time Pat opens its config file, the rename has long since completed. No race window. Codex: verify or surface a counter-example.

7. **`#[serde(rename_all = "snake_case")]` vs. explicit per-field renames.** I use explicit per-field renames (the DTO's field names already match Pat's JSON keys lowercase). Should I add `rename_all` for safety against future field additions? (Position: no; explicit is clearer; future additions get reviewed in their own PR.)

8. **`tempfile::NamedTempFile::persist` returns the original error if rename fails.** I use `.map_err(|e| PatConfigError::Io(e.error))`. Codex: verify this preserves enough error context for operators debugging EACCES / ENOSPC.

Findings from Codex R1 land in §6 as "v2 revision" + applied inline.

---

## 6. Revision log

| Version | Date | Author | Change summary |
|---|---|---|---|
| v1 | 2026-05-19 | badger-oak-dahlia | Initial spec — pre-adrev |

(v2 entry added after Codex R1.)

---

## 7. References

- **bd issue:** `tuxlink-756`
- **Upstream specs:**
  - `docs/superpowers/specs/2026-05-18-cred-handling-design.md` — the fork refactor that created this gap
  - `docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md` §3 lines 179-181 + §7.3 follow-up item 4 — surfaced the gap
- **Plan amendments:**
  - AMD-13 (plan line 2441) — wizard credential persistence to OS keyring; explicit "Pat config no longer written by wizard"
  - AMD-11 (plan line 285) — credential refactor removes `winlink_password_present` boolean; keyring is single source of truth
- **Pat fork code:** [tuxlink-pat origin/master `cfg/config.go`](https://github.com/cameronzucker/tuxlink-pat/blob/master/cfg/config.go) — post-refactor Config struct (no SecureLoginPassword, no AuxAddr.Password)
- **Memories:**
  - [`feedback_no_carveout_on_cross_provider_adrev`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_no_carveout_on_cross_provider_adrev.md) — ≥1 Codex round floor
  - [`feedback_discipline_triage_rule`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_discipline_triage_rule.md) — per-round scope tightening when design is settled by upstream
  - [`feedback_no_atomic_decisions_to_operator`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_no_atomic_decisions_to_operator.md) — atomic decisions (field mapping defaults, error variants) converge with Codex, not operator
  - [`feedback_ai_amateur_radio_reliability`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_ai_amateur_radio_reliability.md) — Codex training-data bias on amateur-radio reality; verify Pat-API claims against the fork's code
- **Existing tuxlink code:**
  - `src-tauri/src/config.rs` — tuxlink Config struct + `write_config_atomic` (the atomic-write pattern this spec mirrors)
  - `src-tauri/src/pat_process.rs` — current PatProcess (post-tuxlink-z5f log_sink refactor)
