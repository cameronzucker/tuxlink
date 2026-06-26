# Tuxlink MCP — Core-API Extraction (Plan 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a transport-agnostic core service layer (`ui_core`) by lifting three representative command bodies (`mailbox_list`, `config_read`, plus a config-redaction primitive) out of their `#[tauri::command]` shims into plain functions callable by both the Tauri GUI adapter and (later) the MCP server adapter — with zero change to existing GUI behavior.

**Architecture:** Ports-and-adapters. A new `src-tauri/src/ui_core/` module holds Tauri-independent functions that take explicit dependencies (`&Arc<dyn WinlinkBackend>`, plain args) and return existing DTOs/`UiError`. Each `#[tauri::command]` becomes a thin adapter: resolve `State` → call the core fn. Mirrors the established `search/commands.rs` (thin) + `search/index.rs` (logic) split. This is Plan 1 of a sequence (see the Tuxlink MCP design doc); it is the unblocked foundation every later plan builds on. It is NOT network-exposable, so it is not gated by the AGPLv3 relicense.

**Tech Stack:** Rust, Tauri 2, `tokio`, `serde`. Tests: `#[tokio::test]` for async backend paths, `#[test]` for pure logic. No mocking library — construct real `NativeBackend` over a `tempdir`.

## Global Constraints

- **Rust edition / MSRV:** match the existing crate (do not bump). Do NOT use `Option::is_none_or` (too new for MSRV).
- **Clippy is `-D warnings`:** use `std::io::Error::other(x)` not `Error::new(ErrorKind::Other, x)`; use `x.is_some_and(|v| …)` not `x.map_or(false, …)`; no needless clones/borrows.
- **No behavior change to the GUI:** every re-pointed command must return byte-identical results to today. The Tauri command keeps its exact signature and `State` resolution; only the inner logic moves.
- **Dependency direction (this plan's interim state):** `ui_core` references DTOs/`UiError` from `crate::ui_commands` via `use`. Flipping that (moving DTOs into `ui_core/types.rs` with re-exports) is an explicit follow-up cleanup (Task 4), kept separate to keep each task's diff reviewable.
- **Commit discipline:** conventional commits; every commit ends with `Agent: <moniker>` and the `Co-Authored-By:` trailer. Per-task branch `bd-tuxlink-cvx84/core-api-extraction` (or a worktree if the main-checkout-race hook denies).
- **Execution environment:** the Pi cannot cold-compile cargo. Run-steps below say "Run: `cargo test …`" — execute these via the project's CI on a **draft PR** (push early), not a local Pi build. `grep` the diff for the clippy traps above before pushing.
- **bd:** this plan implements part of `tuxlink-cvx84`. Mark it `in_progress` when starting; do not close until the full MCP epic ships.

---

### Task 1: Create `ui_core` module + extract `list_mailbox`

**Files:**
- Create: `src-tauri/src/ui_core/mod.rs`
- Create: `src-tauri/src/ui_core/mailbox.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod ui_core;` near the other `mod` declarations)
- Modify: `src-tauri/src/ui_commands.rs:215-230` (re-point `mailbox_list` at the core fn)
- Test: `src-tauri/src/ui_core/mailbox.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::winlink_backend::{WinlinkBackend, MessageId}`, `crate::native_mailbox::FolderRef`, `crate::ui_commands::{MessageMetaDto, UiError, parse_folder_ref}`.
- Produces: `pub async fn ui_core::mailbox::list_mailbox(backend: &std::sync::Arc<dyn crate::winlink_backend::WinlinkBackend>, folder: &str) -> Result<Vec<crate::ui_commands::MessageMetaDto>, crate::ui_commands::UiError>` — later consumed by the `mailbox_list` Tauri command (this task) and the MCP `mailbox_list` tool (Plan 3).

Note: `parse_folder_ref` is currently a private helper in `ui_commands.rs`. Step 0 makes it `pub(crate)` so `ui_core` can call it.

- [ ] **Step 0: Make `parse_folder_ref` crate-visible**

In `src-tauri/src/ui_commands.rs`, change the `parse_folder_ref` declaration from `fn parse_folder_ref(` to:

```rust
pub(crate) fn parse_folder_ref(folder: &str) -> Result<crate::native_mailbox::FolderRef, UiError> {
```

(Keep its body unchanged. If it already returns `Result<FolderRef, UiError>`, only the visibility keyword changes.)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ui_core/mailbox.rs` with the core fn stub returning `unimplemented!()` and this test:

```rust
use std::sync::Arc;
use crate::ui_commands::{MessageMetaDto, UiError};
use crate::winlink_backend::WinlinkBackend;

pub async fn list_mailbox(
    _backend: &Arc<dyn WinlinkBackend>,
    _folder: &str,
) -> Result<Vec<MessageMetaDto>, UiError> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink_backend::{MessageId, NativeBackend};
    use crate::native_mailbox::{Mailbox, MailboxFolder};
    use crate::test_helpers::native_test_config;
    use tempfile::tempdir;

    // Seeds one inbox message and lists it through the extracted core fn.
    #[tokio::test]
    async fn list_mailbox_returns_seeded_inbox_message() {
        let dir = tempdir().unwrap();
        // Seed a raw message directly into the mailbox the backend will read.
        let seed = Mailbox::new(dir.path());
        let raw = crate::winlink_backend::compose_message(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000,
        ).to_bytes();
        let _id: MessageId = seed.store(MailboxFolder::Inbox, &raw).unwrap();

        let backend: Arc<dyn WinlinkBackend> =
            Arc::new(NativeBackend::new(native_test_config(), dir.path()));

        let metas = list_mailbox(&backend, "inbox").await.unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].subject, "Hi");
    }
}
```

Create `src-tauri/src/ui_core/mod.rs`:

```rust
pub mod mailbox;
```

Add to `src-tauri/src/lib.rs` (next to the other `mod` lines, e.g. after `mod ui_commands;`):

```rust
mod ui_core;
```

> EXECUTOR NOTE: `compose_message`, `native_test_config`, and `Mailbox`/`MailboxFolder` are used verbatim by the existing test at `winlink_backend.rs:2104` and `test_helpers.rs`. Confirm their exact visibility/path on the first CI compile; if `compose_message` is test-private to `winlink_backend`, substitute the public seeding path that the existing `winlink_backend` tests use (same `tempdir` + `Mailbox::store`).

- [ ] **Step 2: Run test to verify it fails**

Run (via draft-PR CI): `cargo test -p <crate> ui_core::mailbox::tests::list_mailbox_returns_seeded_inbox_message`
Expected: FAIL — `unimplemented!()` panic ("not implemented").

- [ ] **Step 3: Write minimal implementation**

Replace the `list_mailbox` stub body with the logic lifted verbatim from `mailbox_list` (`ui_commands.rs:215`):

```rust
pub async fn list_mailbox(
    backend: &Arc<dyn WinlinkBackend>,
    folder: &str,
) -> Result<Vec<MessageMetaDto>, UiError> {
    use crate::native_mailbox::FolderRef;
    let parsed = crate::ui_commands::parse_folder_ref(folder)?;
    let metas = match parsed {
        FolderRef::System(f) => backend.list_messages(f).await?,
        FolderRef::User(slug) => backend.list_user_messages(&slug).await?,
    };
    Ok(metas.into_iter().map(MessageMetaDto::from).collect())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p <crate> ui_core::mailbox::tests::list_mailbox_returns_seeded_inbox_message`
Expected: PASS.

- [ ] **Step 5: Re-point the Tauri command at the core fn**

Replace the body of `mailbox_list` in `ui_commands.rs:215` with a thin adapter (signature unchanged):

```rust
#[tauri::command]
pub async fn mailbox_list(
    folder: String,
    state: State<'_, BackendState>,
) -> Result<Vec<MessageMetaDto>, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    crate::ui_core::mailbox::list_mailbox(&backend, &folder).await
}
```

- [ ] **Step 6: Run the full backend test suite to confirm no behavior change**

Run: `cargo test -p <crate>`
Expected: PASS (existing mailbox tests unaffected; new core test green).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ui_core/mod.rs src-tauri/src/ui_core/mailbox.rs src-tauri/src/lib.rs src-tauri/src/ui_commands.rs
git commit -m "refactor(ui_core): extract list_mailbox into transport-agnostic core

mailbox_list is now a thin Tauri adapter over ui_core::mailbox::list_mailbox,
the first command in the core-API extraction (Plan 1, tuxlink-cvx84). No GUI
behavior change.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Extract `read_config_view`

**Files:**
- Create: `src-tauri/src/ui_core/config.rs`
- Modify: `src-tauri/src/ui_core/mod.rs` (add `pub mod config;`)
- Modify: `src-tauri/src/ui_commands.rs:1226-1232` (re-point `config_read`)
- Test: `src-tauri/src/ui_core/config.rs` (inline tests)

**Interfaces:**
- Consumes: `crate::config`, `crate::ui_commands::{ConfigViewDto, UiError}`.
- Produces: `pub fn ui_core::config::read_config_view() -> Result<crate::ui_commands::ConfigViewDto, crate::ui_commands::UiError>` — consumed by the `config_read` command (this task) and the MCP `config_read` tool (Plan 3, which will wrap it in Task 3's redaction).

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ui_core/config.rs`:

```rust
use crate::ui_commands::{ConfigViewDto, UiError};

pub fn read_config_view() -> Result<ConfigViewDto, UiError> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // read_config_view returns a DTO whose fields mirror the persisted config.
    // Uses the real on-disk read path; in a clean test env config::read_config
    // returns defaults, so we assert the call succeeds and yields a DTO.
    #[test]
    fn read_config_view_returns_a_dto() {
        let view = read_config_view().expect("config read should succeed");
        // ConfigViewDto is constructed from the persisted Config via From; the
        // host field is always present (defaulted if unset).
        let _ = view.host;
    }
}
```

Add to `src-tauri/src/ui_core/mod.rs`:

```rust
pub mod config;
```

> EXECUTOR NOTE: `config::read_config()` reads from the OS config path. If the test environment has no config file, confirm it returns defaults (not an error). If it errors on missing-file, the existing `config_read` command would already fail the same way — match that behavior; if needed, point the test at a `tempdir` config via the same mechanism `config.rs` tests use.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p <crate> ui_core::config::tests::read_config_view_returns_a_dto`
Expected: FAIL — `unimplemented!()` panic.

- [ ] **Step 3: Write minimal implementation**

Replace the stub with the logic lifted verbatim from `config_read` (`ui_commands.rs:1226`):

```rust
pub fn read_config_view() -> Result<ConfigViewDto, UiError> {
    let cfg = crate::config::read_config().map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;
    Ok(ConfigViewDto::from(&cfg))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p <crate> ui_core::config::tests::read_config_view_returns_a_dto`
Expected: PASS.

- [ ] **Step 5: Re-point the Tauri command**

Replace `config_read` in `ui_commands.rs:1226` with:

```rust
#[tauri::command]
pub async fn config_read() -> Result<ConfigViewDto, UiError> {
    crate::ui_core::config::read_config_view()
}
```

- [ ] **Step 6: Run the suite**

Run: `cargo test -p <crate>`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ui_core/config.rs src-tauri/src/ui_core/mod.rs src-tauri/src/ui_commands.rs
git commit -m "refactor(ui_core): extract read_config_view into core

config_read is now a thin adapter over ui_core::config::read_config_view
(Plan 1, tuxlink-cvx84). No behavior change.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Config-redaction primitive (`redact_config_view`) — the first MCP-sink redaction

**Files:**
- Modify: `src-tauri/src/ui_core/config.rs` (add `redact_config_view` + tests)

**Interfaces:**
- Consumes: `crate::config::broadcast_grid`, `crate::config::PositionPrecision`, `crate::ui_commands::ConfigViewDto`.
- Produces: `pub fn ui_core::config::redact_config_view(view: ConfigViewDto) -> ConfigViewDto` — reduces `grid` to 4-char (`FourCharGrid`) regardless of the operator's broadcast precision. Consumed by the MCP `config_read` tool (Plan 3) to enforce precise-location redaction at the read sink (per the design doc's redaction-classes requirement; the `config_read` DTO's real leak is full-precision location, NOT a password — `ConfigViewDto` carries no credential).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/ui_core/config.rs`:

```rust
    use crate::config::{broadcast_grid, PositionPrecision};
    use crate::test_helpers::native_test_config;

    // A 6-char stored grid is reduced to 4-char at the MCP sink even when the
    // operator's own broadcast precision is SixCharGrid.
    #[test]
    fn redact_config_view_forces_grid_to_four_char() {
        let mut cfg = native_test_config();
        cfg.identity.grid = Some("CN87ux".to_string());
        cfg.privacy.position_precision = PositionPrecision::SixCharGrid;
        let view = ConfigViewDto::from(&cfg);
        assert_eq!(view.grid.as_deref(), Some("CN87ux")); // unredacted before

        let redacted = redact_config_view(view);
        assert_eq!(redacted.grid.as_deref(), Some("CN87")); // 4-char after
    }

    // No grid stored → stays None (no panic).
    #[test]
    fn redact_config_view_handles_absent_grid() {
        let mut cfg = native_test_config();
        cfg.identity.grid = None;
        let view = ConfigViewDto::from(&cfg);
        let redacted = redact_config_view(view);
        assert_eq!(redacted.grid, None);
    }
```

Add the stub above the `tests` module:

```rust
pub fn redact_config_view(view: ConfigViewDto) -> ConfigViewDto {
    unimplemented!()
}
```

> EXECUTOR NOTE: confirm `crate::test_helpers::native_test_config` is reachable from this module under `cfg(test)` (the Explore inventory reports it is the standard fixture). If `test_helpers` is `#[cfg(test)]`-gated to a different module, add `#[cfg(test)] pub(crate) fn` visibility or construct the `Config` inline with `..Default::default()`-style nested literals.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p <crate> ui_core::config::tests::redact_config_view`
Expected: FAIL — `unimplemented!()` panic on both.

- [ ] **Step 3: Write minimal implementation**

Replace the stub:

```rust
pub fn redact_config_view(mut view: ConfigViewDto) -> ConfigViewDto {
    // Precise location is the real leak in config_read (no credential field
    // exists in ConfigViewDto). Force 4-char Maidenhead at the MCP sink,
    // independent of the operator's on-air broadcast precision.
    view.grid = view
        .grid
        .map(|g| crate::config::broadcast_grid(&g, crate::config::PositionPrecision::FourCharGrid));
    view
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p <crate> ui_core::config::tests::redact_config_view`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ui_core/config.rs
git commit -m "feat(ui_core): add redact_config_view MCP-sink redaction primitive

Reduces config grid to 4-char Maidenhead for the MCP read sink (the real
config_read leak is precise location, not a credential). First piece of the
redaction-classes requirement (Plan 1, tuxlink-cvx84).

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4 (cleanup, optional in Plan 1): Flip the type dependency direction

**Files:**
- Create: `src-tauri/src/ui_core/types.rs`
- Modify: `src-tauri/src/ui_commands.rs` (move `MessageMetaDto`, `ConfigViewDto`, `UiError` definitions out; add `pub use crate::ui_core::types::{MessageMetaDto, ConfigViewDto, UiError};` for back-compat)
- Modify: `src-tauri/src/ui_core/{mailbox,config}.rs` (import from `super::types` instead of `crate::ui_commands`)

**Rationale:** the correct ports-and-adapters direction is `commands → core`, not `core → commands`. This task moves the shared DTO/error types into the core and re-exports them so every existing `crate::ui_commands::UiError` reference keeps compiling.

> This task is a pure-move refactor with a high reference count. Right-sized as its own reviewable diff, and OPTIONAL for Plan 1's deliverable (the seam works without it). Defer to a dedicated cleanup PR if Plan 1 is time-boxed. Steps omitted here intentionally — schedule as a focused move-and-reexport task with `cargo build` + full-suite green as the gate, since it changes no behavior.

---

## Self-Review

**1. Spec coverage (against the design doc's Plan-1 scope — "Core-API extraction vertical slice"):**
- Transport-agnostic core layer exists → Task 1 creates `ui_core`. ✓
- A pure read extracted → `list_mailbox` (Task 1). ✓
- A config read extracted → `read_config_view` (Task 2). ✓
- The redaction-at-sink primitive proven → `redact_config_view` (Task 3), implementing the design's "config_read leaks precise location → reduce grid" finding. ✓
- Adapter pattern (command → core) established → re-point steps in Tasks 1 & 2. ✓
- Correct dependency direction → Task 4 (optional cleanup). ✓ (flagged as deferrable)
- Gap (intentional): `message_read` (the taint source) is NOT in this plan — its extraction follows the identical Task-1 pattern but its taint semantics belong to Plan 2 (egress + taint security core), so it is deferred to avoid implying taint behavior that Plan 2 owns.

**2. Placeholder scan:** no "TBD"/"handle errors"/"similar to Task N". Every code step shows full code. Two EXECUTOR NOTEs flag CI-verifiable import/visibility details I cannot confirm without compiling (cold-cargo) — these are honest verification checkpoints, not placeholders.

**3. Type consistency:** `list_mailbox` returns `Vec<MessageMetaDto>` (matches `mailbox_list`); `read_config_view`/`redact_config_view` use `ConfigViewDto`; all errors are `UiError`. `broadcast_grid(&str, PositionPrecision) -> String` matches `config.rs:252`. `state.current() -> Option<Arc<dyn WinlinkBackend>>` matches `app_backend.rs`. Consistent.
