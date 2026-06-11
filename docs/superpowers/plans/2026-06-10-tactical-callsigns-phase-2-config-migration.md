# Phase 2: Config + Migration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Each task is failing-test → run(FAIL) → minimal impl → run(PASS) → commit. Do NOT batch impl ahead of its test.

**Goal:** Replace tuxlink's single-callsign `IdentityConfig` with the persisted `IdentityStore` (identity list), bump the config schema version, and write a one-time, idempotent migration that turns the existing `identity.callsign` into the single FULL identity — moving the existing `native-mbox` inbox under that callsign's per-FULL root, default-tagging existing Sent/Outbox messages with that identity, and triggering a search-index rebuild. Replace `Config::validate`'s "CMS iff callsign" rule (false under tactical identities). Set every FULL identity's keyring activation secret at add-time, and ship the `identity_add_full` / `identity_add_tactical` / `identity_remove` / `identity_list` Tauri commands. The switch/active commands and the handle-threading of transmit paths land in **later phases (3, 6, 7)** and are explicitly OUT of scope here.

**Architecture:** Capability/handle model (master plan). Phase 1 (`tuxlink-d4wp`) created the `src-tauri/src/identity/` module with the canonical types (`Callsign`, `Address`, `FullIdentity`, `TacticalIdentity`, `IdentityStore`, `IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityError`). Phase 2 consumes those types verbatim — it does **not** redefine them. Phase 2 wires the persisted `IdentityStore` into the config/bootstrap lifecycle, performs the one-time legacy-config migration, and exposes the CRUD Tauri command surface. No transmit/listen path is touched (that is Phase 3's blast radius); the active `SessionIdentity` backend state is introduced in Phase 3.

**Tech stack:** Rust (Tauri backend), `serde`/`serde_json` (config persistence), the OS keyring (existing `winlink/credentials` surface, reached through `IdentityService::set_activation_secret` from Phase 1), the SQLite search index (`crate::search`), `tempfile` (atomic writes), `thiserror`.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md) — §"Config / migration", §"Mailbox model", §"CMS gating for tactical".
**Master plan (canonical interface contract — use type names verbatim):** [`docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md`](2026-06-10-tactical-callsigns-master-plan.md).
**bd issue:** tuxlink-7iy2 (depends on tuxlink-d4wp / Phase 1).

---

## Canonical names this phase consumes (from the master-plan contract — do NOT rename)

From the Phase 1 `identity` module:

```rust
crate::identity::Callsign            // Callsign::parse(&str) -> Result<Self, IdentityError>; .as_str()
crate::identity::Address             // Address::Full(Callsign) | Address::Tactical(String)
crate::identity::FullIdentity        // { callsign, label: Option<String>, has_cms_account, cms_registered }
crate::identity::TacticalIdentity    // { label, parent: Callsign, cms: TacticalCmsState }
crate::identity::TacticalCmsState    // Unknown | Registered{checked_unix} | NotRegistered{checked_unix}
crate::identity::IdentityStore       // load(&Path) / save() / full() / tactical() / full_by_callsign()
                                     // add_full() / add_tactical() / remove() / last_selected() / set_last_selected()
crate::identity::IdentityService     // authenticate() / set_activation_secret() / clear_activation_secret()
crate::identity::IdentityError       // InvalidCallsign | InvalidTactical | UnknownIdentity | ParentNotFound
                                     // | RemoveHasTacticals | NoSecretSet | CredentialMismatch | Keyring | Io
```

Phase-2-new types (this plan creates them):

- `crate::config::CONFIG_SCHEMA_VERSION` bumped `1` → `2`.
- `crate::config::IdentityMigration` — the pure migration planner/executor for Phase 2.
- `crate::config::ConfigValidationError` — the `CmsPathMissingCallsign` / `OfflinePathHasCallsign` variants are **removed**; a new posture (below) replaces them.
- `crate::ui_commands` (or a new `crate::identity::commands`) — `identity_list` / `identity_add_full` / `identity_add_tactical` / `identity_remove` Tauri commands + their DTOs.

**Keyring key (from the contract):** `tuxlink-identity-activation:<CALLSIGN>` — set via `IdentityService::set_activation_secret`. Phase 2 calls that method at add-time; it does NOT reimplement keyring access.

---

## File structure

```
src-tauri/src/
  config.rs                 # MODIFY: CONFIG_SCHEMA_VERSION 1->2; replace IdentityConfig usage;
                            #         replace Config::validate CMS-iff-callsign rule; add IdentityMigration.
  identity/
    mod.rs                  # (Phase 1) — Phase 2 may add `pub mod commands;` if commands live here.
    commands.rs             # NEW (or in ui_commands.rs): identity_list / identity_add_full /
                            #         identity_add_tactical / identity_remove + DTOs.
  bootstrap.rs              # MODIFY: load/migrate IdentityStore at startup; per-FULL mailbox root resolution.
  native_mailbox.rs         # MODIFY (minimal): expose a per-FULL mailbox-root helper for migration target;
                            #         NO folder-layout change (per-FULL namespacing detail = Phase 4).
  lib.rs                    # MODIFY: register the four new Tauri commands in generate_handler!.
docs/superpowers/plans/
  2026-06-10-tactical-callsigns-phase-2-config-migration.md   # THIS FILE
```

**Scope fence (read before writing any code):**

- Phase 2 makes the *identity list* the persisted source of truth and migrates the legacy single callsign into it. It does **not** thread `IdentityHandle`/`SessionIdentity` through transmit/connect/listen paths — those still read the active FULL callsign through their existing `cfg.identity.*` reads in Phase 2; Phase 3 (`tuxlink-0063`) converts them. To avoid breaking the ~10 current `cfg.identity.callsign` reads, Phase 2 **retains a single derived "active FULL callsign" mirror** on `Config` (see Task 2) so existing code compiles unchanged. The mirror is a *projection* of the store's `last_selected` FULL, not an independent field; Phase 3 deletes it as it migrates each reader.
- The per-FULL **inbox directory namespacing** in `native_mailbox.rs` (folder-layout change) is Phase 4's job. Phase 2's migration MOVES the existing flat `native-mbox/inbox` to the FULL's per-callsign root path that Phase 4 will read from, and records that root, but does not refactor `folder_dir`. The migration test asserts the inbox files land at the per-FULL path with contents intact.

---

## Tasks

### Task 1 — Bump `CONFIG_SCHEMA_VERSION` to 2 and gate old-version reads through migration (not hard-fail)

Currently `deserialize_schema_version` hard-errors on any `schema_version != CONFIG_SCHEMA_VERSION` ([config.rs:318-330](../../../src-tauri/src/config.rs)), and `write_config_atomic` refuses to overwrite a mismatched on-disk version ([config.rs:511-529](../../../src-tauri/src/config.rs)). Phase 2 must accept a v1 file *for migration* while still rejecting unknown future versions.

**Files:**
- `src-tauri/src/config.rs` — `CONFIG_SCHEMA_VERSION` const (line 9); `deserialize_schema_version` (lines 318-330); `SchemaVersionProbe` (line 545); `write_config_atomic` schema guard (lines 511-529).

- [ ] **Failing test** — add to `config.rs` `mod tests`: a v1-shaped config JSON deserializes through a NEW `read_config_or_migrate`-shaped seam as a *migration candidate*, not an error. Minimal first step: test the lower-level probe.
  ```rust
  #[test]
  fn schema_version_1_is_recognized_as_migratable_not_rejected() {
      // A v1 file must be detectable as "needs migration", distinct from an
      // unknown future version which is still rejected.
      assert_eq!(super::detect_schema_action(1), super::SchemaAction::MigrateFromV1);
      assert_eq!(super::detect_schema_action(CONFIG_SCHEMA_VERSION), super::SchemaAction::Current);
      assert_eq!(super::detect_schema_action(999), super::SchemaAction::Unsupported { found: 999 });
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml schema_version_1_is_recognized_as_migratable_not_rejected` — fails: `detect_schema_action` / `SchemaAction` do not exist.
- [ ] **Minimal impl** in `config.rs`:
  - Change `pub const CONFIG_SCHEMA_VERSION: u32 = 1;` → `= 2;` (line 9).
  - Add the classifier (kept pure so it is unit-testable without I/O):
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SchemaAction {
        Current,
        MigrateFromV1,
        Unsupported { found: u32 },
    }
    pub fn detect_schema_action(found: u32) -> SchemaAction {
        match found {
            v if v == CONFIG_SCHEMA_VERSION => SchemaAction::Current,
            1 => SchemaAction::MigrateFromV1,
            other => SchemaAction::Unsupported { found: other },
        }
    }
    ```
  - Leave `deserialize_schema_version` as-is for now (it gates the *current* `Config` struct, which after this task represents v2). The v1 file is parsed by a dedicated `LegacyConfigV1` shape in Task 3, NOT by `Config`, so the strict version check on `Config` is correct.
- [ ] **Run (expect PASS):** `cargo test --manifest-path src-tauri/Cargo.toml schema_version_1_is_recognized_as_migratable_not_rejected`.
- [ ] **Run regression:** `cargo test --manifest-path src-tauri/Cargo.toml --lib config::tests` — the existing migration tests hard-code `CONFIG_SCHEMA_VERSION` via `format!("…{ver}…", ver = CONFIG_SCHEMA_VERSION)`, so they auto-track the bump and must stay green (they assert v2-shaped fields). If any existing test embeds a literal `"schema_version": 1`, update it to `CONFIG_SCHEMA_VERSION`.
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/config.rs
  git commit -m "feat(config)!: bump schema_version to 2 + add SchemaAction classifier

  Phase 2 (tuxlink-7iy2) of multiple/tactical callsigns. v1 configs are now
  migration candidates (SchemaAction::MigrateFromV1) rather than hard-rejected;
  unknown future versions stay Unsupported. BREAKING CHANGE: config schema_version
  is now 2; v1 files are migrated on first read.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 2 — Replace `IdentityConfig` on `Config` with an `IdentityStore` reference + a derived active-FULL mirror

`Config` currently embeds `IdentityConfig { callsign, identifier, grid }` ([config.rs:21, 142-160](../../../src-tauri/src/config.rs)). The persisted identity *list* lives in its own `IdentityStore` file (Phase 1's `IdentityStore::load(path)` / `save()`), NOT inline in `config.json` — keeping secrets-free identity state in its own file mirrors how the allowlist + keyring already live outside `config.json`. `Config` keeps only the cross-cutting non-identity-list bits (`grid`, `identifier` for offline display) plus a **derived** active-FULL callsign mirror so the ~10 existing `cfg.identity.callsign` readers compile unchanged until Phase 3 migrates them.

**Files:**
- `src-tauri/src/config.rs` — `Config.identity` field (line 21); `IdentityConfig` struct (lines 142-160); the `cms_config()`/test fixtures referencing `IdentityConfig { callsign, identifier, grid }`.
- `src-tauri/src/bootstrap.rs` — test fixture `IdentityConfig { callsign: Some("W4PHS"…), … }` (lines 307-311) + `cfg.identity.callsign = None` (line 394).

- [ ] **Failing test** — in `config.rs` `mod tests`: a v2 `IdentityConfig` keeps `identifier`/`grid` and an `active_full: Option<String>` derived mirror, and round-trips.
  ```rust
  #[test]
  fn identity_config_v2_carries_active_full_mirror_and_round_trips() {
      let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
      cfg.identity.active_full = Some("W1ABC".into());
      cfg.identity.grid = Some("CN87ux".into());
      let s = serde_json::to_string(&cfg).unwrap();
      let back: Config = serde_json::from_str(&s).unwrap();
      assert_eq!(back.identity.active_full.as_deref(), Some("W1ABC"));
      assert_eq!(back.identity.grid.as_deref(), Some("CN87ux"));
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml identity_config_v2_carries_active_full_mirror_and_round_trips`.
- [ ] **Minimal impl** in `config.rs`: rename the legacy `callsign` field to `active_full` and document it as a Phase-3-deleted projection of `IdentityStore::last_selected`’s FULL.
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct IdentityConfig {
      /// PHASE 2 TRANSITIONAL MIRROR (deleted in Phase 3). The active FULL
      /// callsign — a projection of `IdentityStore::last_selected()` written here
      /// so the ~10 legacy `cfg.identity.<callsign>` readers compile unchanged
      /// until Phase 3 (tuxlink-0063) threads `SessionIdentity` through them.
      /// The IdentityStore (separate file) is the source of truth; this is a cache.
      #[serde(rename = "callsign", deserialize_with = "deserialize_optional_nonempty_string", default)]
      pub active_full: Option<String>,
      #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
      pub identifier: Option<String>,
      #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
      pub grid: Option<String>,
  }
  ```
  Keep the serde wire name `"callsign"` (via `rename`) so v2 JSON is byte-compatible with the field every existing reader already serializes; only the Rust field identifier changes. Update the ~10 `cfg.identity.callsign` reads to `cfg.identity.active_full` (mechanical rename — `rg "identity\.callsign"` across `src-tauri/src` and fix each; they are read-only reads on transmit/connect paths per the spec's "~10 sites" inventory).
- [ ] **Run (expect PASS):** `cargo test --manifest-path src-tauri/Cargo.toml identity_config_v2_carries_active_full_mirror_and_round_trips`.
- [ ] **Run regression:** `cargo test --manifest-path src-tauri/Cargo.toml --lib` (or at minimum `config::tests bootstrap::tests winlink_backend::tests`) — fixtures using `IdentityConfig { callsign: … }` must be updated to `active_full: …`; the bootstrap fixture at lines 307-311 and the `cfg.identity.callsign = None` at line 394 become `active_full`.
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/config.rs src-tauri/src/bootstrap.rs src-tauri/src/winlink_backend.rs src-tauri/src/modem_commands.rs src-tauri/src/ui_commands.rs
  git commit -m "refactor(config): rename IdentityConfig.callsign -> active_full transitional mirror

  Phase 2 (tuxlink-7iy2). The persisted IdentityStore (separate file) becomes the
  identity source of truth; Config.identity.active_full is a serde-wire-compatible
  projection of last_selected's FULL so legacy callsign readers compile unchanged
  until Phase 3 threads SessionIdentity through them.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 3 — Replace the `Config::validate` "CMS iff callsign" rule

The spec mandates: the `connect_to_cms ⇔ identity.callsign.is_some()` biconditional is **false under tactical identities** — a tactical operates without its own CMS account, and a FULL identity may be P2P/RF-only with no CMS account. Current `Config::validate` enforces both directions ([config.rs:413-434](../../../src-tauri/src/config.rs)) with `CmsPathMissingCallsign` (line 414-416) + `OfflinePathHasCallsign` (line 417-419).

**Files:**
- `src-tauri/src/config.rs` — `ConfigValidationError` enum (lines 397-407); `Config::validate` (lines 413-434).

**New validation posture (replaces the biconditional):**
- A CMS-mode config (`connect_to_cms = true`) requires that an active FULL identity is selectable — i.e. `identity.active_full.is_some()` — because a CMS connect needs *a* licensed principal. (This keeps the "you can't CMS-connect as nobody" guard, which is real, while dropping the false offline direction.)
- The offline direction (`!connect_to_cms ⇒ callsign must be None`) is **removed**: an offline/P2P deployment may legitimately have a FULL identity selected (it just isn't using CMS). This is the rule the spec calls out as false under tactical.
- The `active_full`/`identifier` field-shape validation (`validate_identity_describe`) and the packet-SSID range check are retained unchanged.

- [ ] **Failing test** — in `config.rs` `mod tests`:
  ```rust
  #[test]
  fn offline_config_with_active_full_is_now_valid() {
      // The replaced rule: a P2P/RF-only deployment may select a FULL identity
      // without connecting to CMS. Under the old biconditional this was
      // OfflinePathHasCallsign; under tactical identities it is legal.
      let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
      cfg.connect.connect_to_cms = false;
      cfg.identity.active_full = Some("W1ABC".into());
      assert!(cfg.validate().is_ok(), "offline + a selected FULL identity is valid (tactical posture)");
  }

  #[test]
  fn cms_config_without_any_active_full_is_rejected() {
      let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
      cfg.connect.connect_to_cms = true;
      cfg.identity.active_full = None;
      assert!(matches!(cfg.validate().unwrap_err(), ConfigValidationError::CmsPathNoActiveFull));
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml offline_config_with_active_full_is_now_valid cms_config_without_any_active_full_is_rejected`.
- [ ] **Minimal impl** in `config.rs`:
  - In `ConfigValidationError`: remove `CmsPathMissingCallsign` + `OfflinePathHasCallsign`; add
    ```rust
    #[error("CMS path requires an active FULL identity to be selected")]
    CmsPathNoActiveFull,
    ```
  - In `Config::validate`: replace lines 414-419 with
    ```rust
    if self.connect.connect_to_cms && self.identity.active_full.is_none() {
        return Err(ConfigValidationError::CmsPathNoActiveFull);
    }
    // The offline-forbids-callsign rule is intentionally removed (Phase 2,
    // tuxlink-7iy2): a P2P/RF-only deployment may select a FULL identity, and a
    // tactical operates with no own CMS account. The CMS<->callsign biconditional
    // was false under tactical identities.
    ```
  - Keep the `active_full`/`identifier` field-shape checks (lines 420-429), updating `self.identity.callsign` → `self.identity.active_full`.
- [ ] **Run (expect PASS):** the two tests above, plus `cargo test --manifest-path src-tauri/Cargo.toml --lib bootstrap::tests` (the `offline_mode_is_not_connected` test at bootstrap.rs:387-397 sets `callsign = None` "to keep the fixture coherent" — it stays green because bootstrap reads only the two gating flags, but update the field name).
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/config.rs
  git commit -m "feat(config)!: replace Config::validate CMS-iff-callsign rule

  Phase 2 (tuxlink-7iy2). The connect_to_cms<->callsign biconditional is false
  under tactical identities (a tactical has no own CMS account; a FULL identity
  may be P2P/RF-only). Drops OfflinePathHasCallsign entirely; CmsPathMissingCallsign
  becomes CmsPathNoActiveFull (CMS still needs *a* licensed principal selected).
  BREAKING CHANGE: offline configs with a selected callsign now validate.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 4 — The migration planner: pure decision from a legacy v1 config (no I/O)

The migration's *decision* (what becomes the FULL identity, what the per-FULL mailbox root is, whether there is anything to migrate) must be a pure function so it is unit-testable without touching the filesystem or keyring. The legacy v1 config carries `identity.callsign` (the single station call); that becomes the one FULL identity. If v1 had no callsign (offline-only deployment with only `identifier`), there is no FULL identity to create — the migration produces an empty store and skips the mailbox move.

**Files:**
- `src-tauri/src/config.rs` — new `LegacyConfigV1` deserialize shape + `IdentityMigration::plan`.
- Reference: legacy `IdentityConfig` shape (the pre-Task-2 `{ callsign, identifier, grid }`), `native_mailbox` root `<app_data>/native-mbox` ([bootstrap.rs:145-146](../../../src-tauri/src/bootstrap.rs)).

- [ ] **Failing test** — in `config.rs` `mod tests`:
  ```rust
  #[test]
  fn migration_plan_promotes_legacy_callsign_to_single_full_identity() {
      let v1 = LegacyConfigV1 {
          callsign: Some("W1ABC".into()),
          identifier: None,
          grid: Some("CN87ux".into()),
      };
      let plan = IdentityMigration::plan(&v1);
      assert_eq!(plan.full_callsign.as_deref(), Some("W1ABC"));
      assert!(plan.move_inbox, "an existing callsign means the flat inbox migrates under it");
      // Per-FULL inbox root is namespaced by callsign (Phase 4 reads this path).
      assert_eq!(plan.per_full_subdir.as_deref(), Some("W1ABC"));
  }

  #[test]
  fn migration_plan_offline_only_config_creates_no_full_identity() {
      let v1 = LegacyConfigV1 { callsign: None, identifier: Some("FIELD-1".into()), grid: None };
      let plan = IdentityMigration::plan(&v1);
      assert!(plan.full_callsign.is_none());
      assert!(!plan.move_inbox, "no callsign => nothing to move; the flat store stays where it is");
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml migration_plan_promotes_legacy_callsign_to_single_full_identity migration_plan_offline_only_config_creates_no_full_identity`.
- [ ] **Minimal impl** in `config.rs`:
  ```rust
  /// The exact v1 `identity` shape, parsed standalone so the migration can read a
  /// v1 file without going through the v2 `Config` (whose schema_version guard
  /// rejects 1).
  #[derive(Debug, Clone, Deserialize)]
  pub struct LegacyConfigV1 {
      #[serde(default)] pub callsign: Option<String>,
      #[serde(default)] pub identifier: Option<String>,
      #[serde(default)] pub grid: Option<String>,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct MigrationPlan {
      /// The legacy callsign promoted to the single FULL identity (None for an
      /// offline-only v1 with no callsign).
      pub full_callsign: Option<String>,
      /// Subdir name under the mailbox root for this FULL's per-callsign inbox
      /// (Phase 4 reads from here). `Some(callsign)` iff `full_callsign` is Some.
      pub per_full_subdir: Option<String>,
      /// Whether the flat `native-mbox/inbox` must be moved under the per-FULL root.
      pub move_inbox: bool,
  }

  pub struct IdentityMigration;
  impl IdentityMigration {
      pub fn plan(v1: &LegacyConfigV1) -> MigrationPlan {
          match v1.callsign.as_deref().filter(|c| !c.is_empty()) {
              Some(c) => MigrationPlan {
                  full_callsign: Some(c.to_string()),
                  per_full_subdir: Some(c.to_string()),
                  move_inbox: true,
              },
              None => MigrationPlan { full_callsign: None, per_full_subdir: None, move_inbox: false },
          }
      }
  }
  ```
- [ ] **Run (expect PASS):** the two tests above.
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/config.rs
  git commit -m "feat(config): add pure IdentityMigration::plan v1->v2 decision

  Phase 2 (tuxlink-7iy2). Pure planner: the legacy single callsign becomes the one
  FULL identity (per_full_subdir = callsign, move_inbox = true); an offline-only v1
  with no callsign produces an empty store and no inbox move. No I/O — keyring +
  filesystem effects land in the executor (next task).

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 5 — The migration executor: build the IdentityStore, move the inbox, tag Sent/Outbox, rebuild index (THE PHASE-2 CORE TEST)

This is the fiddly, must-commit-the-test-first task (master-plan session guidance). The executor takes the plan, an `IdentityService` (Phase 1, for `set_activation_secret`), a mailbox root, an `IdentityStore` path, and a search service handle, and performs the one-time migration **idempotently** (running it twice is a no-op the second time). It:

1. Creates the single `FullIdentity` (`has_cms_account` derived from the v1 `connect_to_cms`; `cms_registered = false` until verified) and `add_full`s it to a fresh `IdentityStore`, then `set_last_selected(Address::Full(callsign))` and `save()`.
2. Sets the activation secret via `IdentityService::set_activation_secret(callsign, secret)` — the migration receives the secret from the caller (the bootstrap-time migration uses the already-stored CMS login secret for a CMS identity, or prompts for a local passphrase for an offline FULL identity; the executor itself just persists whatever it is handed). At *migration* time, if no secret is available (offline FULL with none set), the FULL identity is created but flagged needs-activation — migration does NOT block on a missing secret; the operator sets it on next launch via the activation flow (Phase 6).
3. Moves `native-mbox/inbox/*` → `native-mbox/<CALLSIGN>/inbox/*` (the per-FULL inbox path Phase 4 reads), preserving every `.b2f` + `.read` file. Idempotent: if the destination already exists and the source is gone, it is a no-op.
4. Default-tags every existing Sent + Outbox message with the FULL identity (the identity tag is a sidecar `.identity` marker file next to each `.b2f`, keyed to the message id — Sent/Outbox remain a *shared* store per the spec, so they are NOT moved, only tagged in place).
5. Triggers `SearchService::rebuild_index(mailbox_root)` ([search/commands.rs:134](../../../src-tauri/src/search/commands.rs)) so the relocated inbox + the identity tags are re-indexed.

**Files:**
- `src-tauri/src/config.rs` — `IdentityMigration::execute`.
- `src-tauri/src/native_mailbox.rs` — a small `per_full_inbox_dir(root, callsign)` helper + a `tag_identity(folder, id, callsign)` writer (sidecar `.identity` file); folder_dir at line 260; store layout at lines 68-70.
- `src-tauri/src/search/commands.rs` — `SearchService::rebuild_index` (line 134), called after the move.

- [ ] **Failing test (THE migration test)** — in `config.rs` `mod tests`, using a `tempfile::TempDir` for the mailbox root and a fake/in-memory `IdentityService` (Phase 1 provides a test constructor; if it does not, this task adds `IdentityService::with_memory_keyring()` as a test-only helper):
  ```rust
  #[test]
  fn migrate_single_callsign_config_promotes_one_full_and_keeps_inbox_intact() {
      use crate::native_mailbox::Mailbox;
      use crate::winlink_backend::MailboxFolder;

      let mbox_root = tempfile::TempDir::new().unwrap();
      let store_path = mbox_root.path().join("identities.json");

      // Seed a legacy flat mailbox: one inbox message + one sent message.
      let mbox = Mailbox::new(mbox_root.path());
      let inbox_id = mbox.store(MailboxFolder::Inbox, &sample_raw_message("INBOX-1")).unwrap();
      let sent_id  = mbox.store(MailboxFolder::Sent,  &sample_raw_message("SENT-1")).unwrap();

      let v1 = LegacyConfigV1 { callsign: Some("W1ABC".into()), identifier: None, grid: Some("CN87".into()) };
      let svc = crate::identity::IdentityService::with_memory_keyring(store_path.clone());

      let report = IdentityMigration::plan(&v1)
          .execute(&svc, mbox_root.path(), &store_path, /*has_cms_account=*/true,
                   /*activation_secret=*/Some("cms-pw"))
          .expect("migration must succeed");

      // (a) exactly one FULL identity, last_selected = it.
      let store = crate::identity::IdentityStore::load(&store_path).unwrap();
      assert_eq!(store.full().len(), 1);
      assert_eq!(store.full()[0].callsign.as_str(), "W1ABC");
      assert!(matches!(store.last_selected(), Some(crate::identity::Address::Full(c)) if c.as_str() == "W1ABC"));

      // (b) the inbox moved under the per-FULL root, contents intact.
      let per_full = Mailbox::new(mbox_root.path().join("W1ABC"));
      let metas = per_full.list(MailboxFolder::Inbox).unwrap();
      assert_eq!(metas.len(), 1, "the one inbox message survived the move");
      assert_eq!(metas[0].id, inbox_id);
      assert!(!mbox_root.path().join("inbox").join(format!("{}.b2f", inbox_id.0)).exists(),
              "the flat inbox no longer holds the migrated message");

      // (c) the sent message is tagged with the FULL identity, in place (shared store).
      assert!(mbox_root.path().join("sent").join(format!("{}.identity", sent_id.0)).exists(),
              "existing Sent messages get a default identity tag");
      assert_eq!(report.sent_tagged + report.outbox_tagged, 1);

      // (d) the activation secret was set in the keyring.
      assert!(svc.has_activation_secret(&crate::identity::Callsign::parse("W1ABC").unwrap()));

      // (e) idempotent: a second run is a clean no-op.
      let again = IdentityMigration::plan(&v1)
          .execute(&svc, mbox_root.path(), &store_path, true, Some("cms-pw")).unwrap();
      assert!(again.was_noop, "re-running the migration must be a no-op");
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml migrate_single_callsign_config_promotes_one_full_and_keeps_inbox_intact` — fails: `execute`, `with_memory_keyring`, `has_activation_secret`, `per_full_inbox_dir`, `.identity` tagging, `MigrationReport` do not exist. **Commit the failing test now** (master-plan guidance: "commit the migration test first") so the test is captured before the impl churn:
  ```bash
  git add src-tauri/src/config.rs
  git commit -m "test(config): add v1->v2 single-callsign migration test (red)

  Phase 2 (tuxlink-7iy2). Asserts a single-callsign config + an existing flat
  mailbox migrates to one FULL identity with the inbox moved under the per-FULL
  root intact, Sent/Outbox tagged in place, activation secret set, and idempotency.
  Impl follows; committed red per master-plan guidance.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```
- [ ] **Minimal impl** — across three files:
  - `native_mailbox.rs`: add
    ```rust
    /// The per-FULL mailbox root for `callsign` (Phase 4 namespacing target).
    /// `<root>/<CALLSIGN>/` — the inbox for that FULL lives at `<root>/<CALLSIGN>/inbox`.
    pub fn per_full_root(root: &std::path::Path, callsign: &str) -> std::path::PathBuf {
        root.join(callsign)
    }
    /// Write the default identity tag sidecar for a message in a shared folder
    /// (Sent/Outbox stay shared per spec; the tag records which identity owns it).
    pub fn tag_identity(root: &std::path::Path, folder: MailboxFolder, id: &MessageId, callsign: &str)
        -> std::io::Result<()> {
        let dir = match folder { MailboxFolder::Sent => "sent", MailboxFolder::Outbox => "outbox", _ => return Ok(()) };
        std::fs::write(root.join(dir).join(format!("{}.identity", id.0)), callsign.as_bytes())
    }
    ```
  - `config.rs`: add `MigrationReport { sent_tagged, outbox_tagged, inbox_moved, was_noop }` and `IdentityMigration::execute`. The executor: (1) early-returns `was_noop=true` if the IdentityStore at `store_path` already has ≥1 FULL identity (the migration-already-ran sentinel); (2) builds + adds the `FullIdentity`, sets last_selected, saves; (3) calls `IdentityService::set_activation_secret` when a secret is provided; (4) `std::fs::rename`s `root/inbox` → `root/<CALLSIGN>/inbox` (create parent first; if `root/inbox` is absent, skip — fresh install); (5) walks `root/sent` + `root/outbox`, writing a `.identity` sidecar per `.b2f`; (6) calls `search_service.rebuild_index(root.to_path_buf())` (passed in or invoked by the bootstrap caller — keep the *index rebuild* in the bootstrap caller if threading the `SearchService` into `config.rs` creates a layering cycle; the test asserts the move + tags, and a separate bootstrap test asserts rebuild is invoked).
  - `identity` module (test support): add `IdentityService::with_memory_keyring(store_path)` + `has_activation_secret(&Callsign)` if Phase 1 did not expose them. If Phase 1 already provides an injectable keyring backend, use it and drop the `with_memory_keyring` step.
- [ ] **Run (expect PASS):** `cargo test --manifest-path src-tauri/Cargo.toml migrate_single_callsign_config_promotes_one_full_and_keeps_inbox_intact`.
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/config.rs src-tauri/src/native_mailbox.rs src-tauri/src/identity/mod.rs
  git commit -m "feat(config): implement v1->v2 identity migration executor

  Phase 2 (tuxlink-7iy2). Promotes the legacy callsign to one FULL identity, moves
  the flat inbox under the per-FULL root intact, default-tags existing Sent/Outbox
  in place (shared store), sets the activation secret, and is idempotent. Inbox
  files survive the move (test-verified).

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 6 — Wire migration + IdentityStore load into startup (bootstrap), and trigger the index rebuild

`install_native` resolves the mailbox dir to `<app_data>/native-mbox` ([bootstrap.rs:145-146](../../../src-tauri/src/bootstrap.rs)) and has the `SearchService` handle in scope ([bootstrap.rs:201-203](../../../src-tauri/src/bootstrap.rs)). Startup must, before installing the backend: detect a v1 on-disk config, run the migration, and rebuild the index. Phase 2 keeps this minimal and well-seam'd: a pure `migrate_on_startup_decision` (testable) plus the I/O glue.

**Files:**
- `src-tauri/src/bootstrap.rs` — `bootstrap_decision` (lines 58-82) and `install_native` (lines 144-211); the `SchemaAction` classifier from Task 1; the migration executor from Task 5.

- [ ] **Failing test** — in `bootstrap.rs` `mod tests`: a pure decision that, given a `SchemaAction`, says whether to run migration before spawn.
  ```rust
  #[test]
  fn startup_runs_migration_for_v1_then_spawns() {
      assert_eq!(super::migration_step(SchemaAction::MigrateFromV1), MigrationStep::MigrateThenContinue);
      assert_eq!(super::migration_step(SchemaAction::Current), MigrationStep::ContinueNoMigration);
      assert_eq!(super::migration_step(SchemaAction::Unsupported { found: 9 }), MigrationStep::AbortUnsupported);
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml startup_runs_migration_for_v1_then_spawns`.
- [ ] **Minimal impl** in `bootstrap.rs`: add the pure `migration_step(SchemaAction) -> MigrationStep` classifier and an `MigrationStep` enum. In `install_native`, after resolving `mbox_dir` (line 146) and before constructing the backend (line 205): probe the on-disk `config.json` `schema_version` via `SchemaVersionProbe`; if `SchemaAction::MigrateFromV1`, parse the v1 `identity` block into `LegacyConfigV1`, run `IdentityMigration::plan(&v1).execute(...)` against `mbox_dir` + the IdentityStore path (`<app_data>/identities.json`), then call `search_service.rebuild_index(mbox_dir.clone())` ([search/commands.rs:134](../../../src-tauri/src/search/commands.rs)) and rewrite `config.json` at v2. A migration failure is **non-fatal** (consistent with bootstrap's "all paths non-fatal" posture, [bootstrap.rs:88-90](../../../src-tauri/src/bootstrap.rs)): log + a session-log line, fall through to install with the un-migrated store rather than refuse to launch.
- [ ] **Run (expect PASS):** `cargo test --manifest-path src-tauri/Cargo.toml startup_runs_migration_for_v1_then_spawns` plus `cargo test --manifest-path src-tauri/Cargo.toml --lib bootstrap::tests`.
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/bootstrap.rs
  git commit -m "feat(bootstrap): run v1->v2 identity migration + index rebuild at startup

  Phase 2 (tuxlink-7iy2). Pure migration_step classifier gates a one-time migration
  before native-backend install; rebuilds the search index over the relocated inbox
  + identity tags. Non-fatal on failure (bootstrap posture): logs and installs the
  un-migrated store rather than refusing to launch.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 7 — `identity_list` / `identity_add_full` / `identity_add_tactical` / `identity_remove` Tauri commands

Phase 2's command surface (the master-plan "Phase 2 partial" set). The switch/active commands (`identity_switch`, `identity_active`) are explicitly Phase 6/7 and NOT added here. `identity_add_full` sets the activation secret at add-time via `IdentityService::set_activation_secret` (the CMS password for a CMS identity, a local passphrase otherwise). DTOs carry NO secrets.

**Files:**
- `src-tauri/src/identity/commands.rs` — NEW (add `pub mod commands;` to `identity/mod.rs`); or append to `ui_commands.rs` mirroring `packet_config_get`/`packet_config_set`.
- `src-tauri/src/lib.rs` — `generate_handler!` list (lines 452-521); register the four commands next to the packet commands (lines 517-521) following the existing comment-tagged style.

- [ ] **Failing test** — in `identity/commands.rs` `mod tests`: the command logic against a memory-keyring `IdentityService` + temp `IdentityStore`, exercising the *inner* functions (the `#[tauri::command]` wrappers delegate to plain fns so they are unit-testable without a Tauri runtime, the established pattern).
  ```rust
  #[test]
  fn add_full_persists_identity_and_sets_activation_secret() {
      let store_path = tempfile::TempDir::new().unwrap().path().join("identities.json");
      let svc = crate::identity::IdentityService::with_memory_keyring(store_path.clone());

      add_full_inner(&svc, "W1ABC", Some("Personal".into()), /*has_cms_account=*/false, "local-pass")
          .expect("add_full");

      let store = crate::identity::IdentityStore::load(&store_path).unwrap();
      assert_eq!(store.full().len(), 1);
      assert_eq!(store.full()[0].callsign.as_str(), "W1ABC");
      assert!(svc.has_activation_secret(&crate::identity::Callsign::parse("W1ABC").unwrap()));

      // The DTO list carries needs_auth/cms flags but NO secret.
      let dto = list_inner(&svc).unwrap();
      assert_eq!(dto.full.len(), 1);
      let serialized = serde_json::to_string(&dto).unwrap();
      assert!(!serialized.contains("local-pass"), "DTO must never carry the secret");
  }

  #[test]
  fn add_tactical_under_unknown_parent_is_rejected() {
      let store_path = tempfile::TempDir::new().unwrap().path().join("identities.json");
      let svc = crate::identity::IdentityService::with_memory_keyring(store_path);
      let err = add_tactical_inner(&svc, "EOC-3", "W9NONE").unwrap_err();
      assert!(matches!(err, crate::identity::IdentityError::ParentNotFound));
  }

  #[test]
  fn remove_full_with_tacticals_is_rejected() {
      let store_path = tempfile::TempDir::new().unwrap().path().join("identities.json");
      let svc = crate::identity::IdentityService::with_memory_keyring(store_path);
      add_full_inner(&svc, "W1ABC", None, false, "p").unwrap();
      add_tactical_inner(&svc, "EOC-3", "W1ABC").unwrap();
      let err = remove_inner(&svc, &crate::identity::Address::Full(
          crate::identity::Callsign::parse("W1ABC").unwrap())).unwrap_err();
      assert!(matches!(err, crate::identity::IdentityError::RemoveHasTacticals));
  }
  ```
- [ ] **Run (expect FAIL):** `cargo test --manifest-path src-tauri/Cargo.toml identity::commands`.
- [ ] **Minimal impl** in `identity/commands.rs`:
  - DTOs: `IdentityListDto { full: Vec<FullIdentityDto>, tactical: Vec<TacticalIdentityDto> }`, `FullIdentityDto { callsign, label, has_cms_account, cms_registered, needs_auth: bool }`, `TacticalIdentityDto { label, parent, cms_badge: &'static str }` — all `Serialize`, NO secret fields. `needs_auth` = `true` (Phase 2: every FULL needs re-auth on launch per the spec; Phase 6 refines from the in-memory session).
  - `add_full_inner(svc, callsign, label, has_cms_account, activation_secret)`: parse `Callsign`, `add_full(FullIdentity { callsign, label, has_cms_account, cms_registered: false })`, then `svc.set_activation_secret(&callsign, activation_secret)`; save store.
  - `add_tactical_inner(svc, label, parent)`: parse parent `Callsign`, `add_tactical(TacticalIdentity { label, parent, cms: TacticalCmsState::Unknown })` (propagates `ParentNotFound`).
  - `remove_inner(svc, &Address)`: `store.remove(addr)` (propagates `RemoveHasTacticals`); on removing a FULL, `svc.clear_activation_secret(&callsign)`.
  - `list_inner(svc)`: read the store, map to the DTO.
  - The four `#[tauri::command]` wrappers (`identity_list`, `identity_add_full`, `identity_add_tactical`, `identity_remove`) take `State<IdentityService>` (managed state registered in `lib.rs` `.setup()` — add that registration) and delegate to the `_inner` fns, mapping `IdentityError` to a serializable command error like the other commands.
- [ ] **Run (expect PASS):** `cargo test --manifest-path src-tauri/Cargo.toml identity::commands`.
- [ ] **Register** in `lib.rs` `generate_handler!` (after line 521, packet commands):
  ```rust
  crate::identity::commands::identity_list,        // tuxlink-7iy2 (Phase 2 identity CRUD)
  crate::identity::commands::identity_add_full,    // tuxlink-7iy2
  crate::identity::commands::identity_add_tactical,// tuxlink-7iy2
  crate::identity::commands::identity_remove,      // tuxlink-7iy2
  ```
  and register `IdentityService` as managed state in `.setup()` (mirror the `SearchService` registration). Confirm with a build: `cargo build --manifest-path src-tauri/Cargo.toml`.
- [ ] **Commit:**
  ```bash
  git add src-tauri/src/identity/commands.rs src-tauri/src/identity/mod.rs src-tauri/src/lib.rs
  git commit -m "feat(identity): identity_list/add_full/add_tactical/remove Tauri commands

  Phase 2 (tuxlink-7iy2). The CRUD command surface; add_full sets the keyring
  activation secret at add-time (CMS password or local passphrase). DTOs carry
  needs_auth + cms badge flags and never a secret. switch/active land in Phase 6/7.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 8 — Gate: clippy clean + full lib test sweep

**Files:** all of the above.

- [ ] **Run clippy (re-run until exit 0 — it hides later-target lints):**
  ```bash
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
  ```
  Fix every warning (idiom lints on the new code, `#[allow(deprecated)]` where a fixture still sets `pat_mbo_address`, etc.). Re-run until clean.
- [ ] **Run the lib test sweep** (the CI `verify` gate runs the full suite, not a scoped subset — per the `scoped_vitest_misses_contract_tests` memory, a far-away contract test can fail while scoped tests pass):
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml --lib
  ```
  Confirm green: `config::tests`, `bootstrap::tests`, `identity::*`, `native_mailbox::tests`, `search::commands::rebuild_tests`, `winlink_backend::tests`.
- [ ] **Reap any stray test processes** (shared-Pi hygiene): `pgrep -f 'tuxlink|cargo' ` should not show orphans from this session; kill only your own PIDs.
- [ ] **Commit** any clippy/test fixups:
  ```bash
  git add -A src-tauri/src
  git commit -m "chore(identity): clippy clean + green lib sweep for Phase 2

  Phase 2 (tuxlink-7iy2) gate: cargo clippy --all-targets -D warnings clean and the
  full --lib test sweep green.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Self-review (Phase 2 spec coverage)

Mapping each Phase-2 spec/master-plan requirement to the task that delivers it:

- **Replace single-callsign `IdentityConfig` with the persisted `IdentityStore` (identity list)** — Task 2 (the active-FULL mirror keeps legacy readers compiling; the `IdentityStore` is the source of truth) + Tasks 5/7 (it is populated by migration + the add commands). The list itself is Phase 1's type, consumed verbatim.
- **Bump the config `schema_version`** — Task 1 (`CONFIG_SCHEMA_VERSION` 1 → 2) with a `SchemaAction` classifier so v1 is a migration candidate, not a hard reject; unknown future versions still rejected.
- **One-time migration: existing `identity.callsign` → single FULL identity** — Tasks 4 (pure plan) + 5 (executor builds the one `FullIdentity`, `set_last_selected`); the migration test asserts exactly one FULL with `last_selected` pointing at it.
- **Migration moves the existing mailbox inbox under the FULL callsign's per-FULL root** — Task 5 step 3 + the migration test's assertion (b): inbox files move to `<root>/<CALLSIGN>/inbox` with `inbox_id` intact and the flat path emptied. The folder-layout refactor of `native_mailbox::folder_dir` is deferred to Phase 4; Phase 2 only relocates + records the path.
- **Migration default-tags existing Sent/Outbox messages with that identity** — Task 5 step 4 (a `.identity` sidecar per message, in place — Sent/Outbox stay the shared store per the spec) + test assertion (c) + `MigrationReport.sent_tagged/outbox_tagged`.
- **Migration triggers a search-index rebuild** — Task 5 step 5 + Task 6 (`SearchService::rebuild_index` invoked at startup after the move), reusing the existing rebuild API ([search/commands.rs:134](../../../src-tauri/src/search/commands.rs)).
- **Replace `Config::validate`'s "CMS iff callsign" rule (false under tactical)** — Task 3: removes the biconditional (`OfflinePathHasCallsign` deleted; offline + a selected FULL is now valid), retains a real "CMS needs *a* selected FULL principal" guard (`CmsPathNoActiveFull`), with both directions tested.
- **At add-time every FULL identity gets a keyring activation secret (CMS password for CMS identities; local passphrase otherwise) via `IdentityService::set_activation_secret`** — Task 7 `add_full_inner` (sets the secret at add-time) + Task 5 (migration sets it for the promoted legacy identity); the keyring key format `tuxlink-identity-activation:<CALLSIGN>` is Phase 1's, used through the contract method, not reimplemented.
- **Add `identity_add_full` / `identity_add_tactical` / `identity_remove` / `identity_list` Tauri commands; switch/active land later** — Task 7 (the four commands + DTOs that carry needs_auth/cms flags and NO secrets) + registration in `lib.rs`. `identity_switch`/`identity_active` are explicitly excluded (Phase 6/7).

**Out-of-scope confirmations (so the next agent does not over-build):**
- No `IdentityHandle`/`SessionIdentity` threading through transmit/connect/listen — Phase 3.
- No `folder_dir` namespacing refactor in `native_mailbox.rs` — Phase 4 (Phase 2 only moves the inbox + adds the `per_full_root`/`tag_identity` helpers).
- No CMS-registration verification network code for tactical — Phase 5.
- No re-auth-on-launch enforcement or listener identity capture — Phase 6.
- No ribbon switcher / inline unlock / mailbox identity filter UI — Phase 7.

**Type-name fidelity:** every identity type is the master-plan canonical name (`IdentityStore`, `FullIdentity`, `TacticalIdentity`, `TacticalCmsState`, `Address`, `Callsign`, `IdentityService`, `IdentityError` + its exact variants, `set_activation_secret`/`clear_activation_secret`/`authenticate`). Phase-2-introduced names (`SchemaAction`, `IdentityMigration`, `MigrationPlan`, `MigrationReport`, `LegacyConfigV1`, `CmsPathNoActiveFull`) are local to this phase and documented above.

**Definition of done:** all eight tasks complete; the migration test + every new test green; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` clean; the full `cargo test --manifest-path src-tauri/Cargo.toml --lib` sweep green; CI green on both arches; PR merged; bd issue tuxlink-7iy2 closed.
