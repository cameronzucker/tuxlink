//! Tuxlink configuration types + validators + atomic-write surface.
//!
//! Spec: docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md
//! bd issue: tuxlink-4mt

use crate::winlink::ax25::KissLinkConfig;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bumped 2 → 3 (tuxlink-ulrz): `trash_auto_purge` was added under
/// `deny_unknown_fields` WITHOUT a version bump, so an older binary rejected the
/// whole config (and the write guard, which probes only `schema_version`, would
/// then clobber it). EVERY additive field MUST bump this — enforced by the
/// `config_schema_version_tracks_field_set` test below.
///
/// Bumped 4 → 5 (tuxlink-8fkkk): added the always-serialized top-level `rig`
/// section (radio-level CAT / rig-control, hoisted out of `[modem_ardop]` so
/// VARA reaches the same rig). A pre-v5 build under `deny_unknown_fields` would
/// reject a config carrying `"rig"`, so the field set change bumps the version
/// (the trash_auto_purge class). The bump also lets a v5 file be classified
/// `MigrateAdditive` by an intermediate build rather than `Unsupported`.
pub const CONFIG_SCHEMA_VERSION: u32 = 5;

/// What to do with an on-disk config of a given `schema_version` (Phase 2,
/// tuxlink-7iy2). A v1 file is a breaking migration candidate; a version ≥2 but
/// below current is additively loadable (new fields default via serde, file is
/// re-stamped on next write); the current version loads normally; anything ABOVE
/// current (a newer build wrote it) is unsupported — refused on write so a
/// downgrade never clobbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaAction {
    Current,
    MigrateFromV1,
    /// Older than current but additively loadable (≥2): read it (defaults fill the
    /// new fields), re-stamp to current on the next write. tuxlink-ulrz.
    MigrateAdditive,
    Unsupported { found: u32 },
}

/// Pure classifier (no I/O) so it is unit-testable without the filesystem.
pub fn detect_schema_action(found: u32) -> SchemaAction {
    match found {
        v if v == CONFIG_SCHEMA_VERSION => SchemaAction::Current,
        1 => SchemaAction::MigrateFromV1,
        v if v > 1 && v < CONFIG_SCHEMA_VERSION => SchemaAction::MigrateAdditive,
        other => SchemaAction::Unsupported { found: other },
    }
}

/// The exact v1 `identity` shape, parsed standalone so the migration can read a
/// v1 config without going through the v2 `Config` (whose schema_version guard
/// rejects 1). Phase 2 (tuxlink-7iy2).
#[derive(Debug, Clone, Deserialize)]
// `callsign` is read by IdentityMigration::plan/execute; `identifier`/`grid` are
// consumed by a later Phase-2 task, so the allow stays scoped to those fields.
#[allow(dead_code)]
pub struct LegacyConfigV1 {
    #[serde(default)]
    pub callsign: Option<String>,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub grid: Option<String>,
}

/// The pure decision of the v1->v2 identity migration (no I/O). Phase 2.
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

/// The v1->v2 identity migration. Phase 2 adds the pure `plan`; a later task adds
/// the I/O `execute` method on `MigrationPlan`.
pub struct IdentityMigration;

impl IdentityMigration {
    /// Pure: decide the migration from a legacy v1 config. An empty/whitespace
    /// callsign is treated as absent.
    pub fn plan(v1: &LegacyConfigV1) -> MigrationPlan {
        match v1.callsign.as_deref().filter(|c| !c.is_empty()) {
            Some(c) => MigrationPlan {
                full_callsign: Some(c.to_string()),
                per_full_subdir: Some(c.to_string()),
                move_inbox: true,
            },
            None => MigrationPlan {
                full_callsign: None,
                per_full_subdir: None,
                move_inbox: false,
            },
        }
    }
}

/// Outcome of running the v1->v2 identity migration. Phase 2 (tuxlink-7iy2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub sent_tagged: usize,
    pub outbox_tagged: usize,
    pub inbox_moved: bool,
    /// True when the migration found nothing to do (already migrated / no FULL).
    pub was_noop: bool,
}

impl MigrationPlan {
    /// Execute the migration (idempotent). Sentinel: if the IdentityStore at
    /// `store_path` already has >=1 FULL identity, this is a no-op.
    pub fn execute(
        &self,
        svc: &crate::identity::IdentityService,
        mbox_root: &std::path::Path,
        store_path: &std::path::Path,
        has_cms_account: bool,
        activation_secret: Option<&str>,
    ) -> Result<MigrationReport, crate::identity::IdentityError> {
        use crate::identity::{Address, Callsign, FullIdentity, IdentityStore};

        let mut store = IdentityStore::load(store_path)?;
        // Idempotency sentinel: a store with a FULL means migration already ran.
        if !store.full().is_empty() {
            return Ok(MigrationReport { sent_tagged: 0, outbox_tagged: 0, inbox_moved: false, was_noop: true });
        }
        // Offline-only v1 (no callsign): nothing to promote.
        let Some(call_str) = self.full_callsign.as_deref() else {
            return Ok(MigrationReport { sent_tagged: 0, outbox_tagged: 0, inbox_moved: false, was_noop: true });
        };

        let callsign = Callsign::parse(call_str)?;
        store.add_full(FullIdentity {
            callsign: callsign.clone(),
            label: None,
            has_cms_account,
            cms_registered: false,
        })?;
        store.set_last_selected(Address::Full(callsign.clone()));
        store.save()?;

        if let Some(secret) = activation_secret {
            svc.set_activation_secret(&callsign, secret)?;
        }

        // The v1->v2 migration deliberately does NOT relocate the inbox (tuxlink-ej7a).
        // The flat `<mbox>/inbox` is the only location the read path knows
        // (`Mailbox::folder_dir` is not per-FULL aware until Phase 4, tuxlink-2ns7).
        // Moving the inbox here — while the read path stayed flat — hid every inbox
        // message after the 0.52.1 upgrade (data displaced, not destroyed). Phase 4
        // owns the per-FULL inbox MOVE *together with* the per-FULL READ change, so
        // the two land atomically. `self.move_inbox` / `per_full_subdir` describe that
        // future intent; the v1->v2 step leaves the inbox in place.
        let inbox_moved = false;

        // Default-tag existing Sent + Outbox messages in place (shared store).
        let sent_tagged = tag_folder(mbox_root, crate::winlink_backend::MailboxFolder::Sent, call_str)?;
        let outbox_tagged = tag_folder(mbox_root, crate::winlink_backend::MailboxFolder::Outbox, call_str)?;

        Ok(MigrationReport { sent_tagged, outbox_tagged, inbox_moved, was_noop: false })
    }
}

/// Tag every `.b2f` in a shared folder with the FULL identity; returns the count.
fn tag_folder(
    mbox_root: &std::path::Path,
    folder: crate::winlink_backend::MailboxFolder,
    callsign: &str,
) -> Result<usize, crate::identity::IdentityError> {
    use crate::identity::IdentityError;
    use crate::winlink_backend::MessageId;
    let dir = mbox_root.join(match folder {
        crate::winlink_backend::MailboxFolder::Sent => "sent",
        crate::winlink_backend::MailboxFolder::Outbox => "outbox",
        _ => return Ok(0),
    });
    if !dir.exists() {
        return Ok(0);
    }
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).map_err(|e| IdentityError::Io(format!("read {}: {e}", dir.display())))? {
        let path = entry.map_err(|e| IdentityError::Io(format!("{e}")))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("b2f") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        crate::native_mailbox::tag_identity(mbox_root, folder, &MessageId::new(stem), callsign)
            .map_err(|e| IdentityError::Io(format!("tag identity: {e}")))?;
        n += 1;
    }
    Ok(n)
}

/// Top-level config struct. `deny_unknown_fields` is the AMD-11 drift defense:
/// any stale field (e.g. `winlink_password_present` from the pre-AMD-1 flat schema)
/// hard-fails at deserialize time rather than silently being dropped.
///
/// `#[serde(remote = "Self")]` (tuxlink-8fkkk): the derive generates an inherent
/// `Config::deserialize` associated fn rather than the trait impl, so the
/// hand-written `impl<'de> Deserialize<'de> for Config` below can call it and
/// then run a post-deserialize migration ([`Config::migrate_rig_from_legacy_ardop`])
/// — lifting a legacy `[modem_ardop]` CAT serial link into the new top-level
/// `[rig]` section. This is the documented serde idiom for post-deserialize
/// fix-ups; all field-level serde attributes (including the
/// `deserialize_schema_version` normalization) are preserved verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, remote = "Self")]
pub struct Config {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u32,
    pub wizard_completed: bool,
    pub connect: ConnectConfig,
    pub identity: IdentityConfig,
    pub privacy: PrivacyConfig,
    #[deprecated(
        note = "pat_mbo_address is unused after the Pat strip (ADR 0016); future writers \
                should not set it. Tracked for removal in a future major bump."
    )]
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default, skip_serializing)]
    pub pat_mbo_address: Option<String>,
    // winlink_password_present REMOVED per AMD-11; deny_unknown_fields catches drift.
    /// AX.25 packet transport settings (additive; defaults when absent). See
    /// `PacketConfig`. `#[serde(default)]` is the migration for old files.
    #[serde(default)]
    pub packet: PacketConfig,
    /// ARDOP HF modem settings (additive; absent until the operator configures ARDOP).
    /// `#[serde(default)]` migrates old config files that predate this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modem_ardop: Option<ArdopUiConfig>,
    /// VARA modem settings (additive; absent until the operator configures VARA).
    /// `#[serde(default)]` migrates old config files that predate this field.
    /// VARA is a third-party closed-source modem that runs as a separate
    /// process exposing two TCP sockets (cmd + data). Tuxlink connects as a
    /// client; tuxlink does NOT manage the VARA process lifecycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modem_vara: Option<VaraUiConfig>,
    /// Radio-level CAT / rig-control settings (tuxlink-8fkkk). Describes ONE
    /// physical transceiver and is consumed by BOTH the ARDOP and VARA paths:
    /// the hamlib model + rigctld endpoint for QSY / live-VFO, the CAT serial
    /// link (`cat_serial_path` / `cat_baud`), and the per-radio behavior flags
    /// (close-serial sequencing, live-VFO poll, QSY-on-fail). Hoisted out of
    /// `[modem_ardop]` so VARA reaches the same rig. `#[serde(default)]` migrates
    /// configs that predate this field (absent → `RigUiConfig::default()`); the
    /// legacy `[modem_ardop]` CAT serial link is lifted in by
    /// [`Config::migrate_rig_from_legacy_ardop`] during deserialize.
    #[serde(default)]
    pub rig: RigUiConfig,
    /// Telnet-P2P listener settings (additive; defaults when absent). The
    /// allowlist + station password live OUTSIDE this struct (the allowlist in
    /// `<config-dir>/listener/telnet/allowed_stations.json`, the password in
    /// the OS keyring); this struct carries only the bind + TTL knobs.
    ///
    /// bd: tuxlink-xehu
    #[serde(default)]
    pub telnet_listen: TelnetListenUiConfig,
    /// Network Post Office relay favorites (additive; empty when absent).
    /// `#[serde(default)]` migrates old config files that predate this field.
    /// bd: tuxlink-6c9y.
    #[serde(default)]
    pub network_po_favorites: Vec<RelayFavorite>,
    /// Prompt the operator to select which pending inbound messages to download
    /// on a CMS connect (WLE "Review Pending Messages" parity), instead of
    /// auto-downloading all. Default TRUE = review before download (the WLE emcomm
    /// default); operators opt out to auto-download-all via the dashboard ribbon's
    /// "On connect" control (tuxlink-pmp5). `#[serde(default = ...)]` migrates
    /// configs that predate this field (absent → true), satisfying
    /// `deny_unknown_fields` (the field is now KNOWN).
    #[serde(default = "default_review_inbound_before_download")]
    pub review_inbound_before_download: bool,
    /// LAN map-tile source (tuxlink-dyop Phase 8). `None` = no LAN source
    /// configured; the map serves the bundled offline base map (`StatusKind::Bundled`).
    /// Set by `configure_tile_source` only AFTER the source validates+activates
    /// (geodetic CRS + reachable host serving a real image). Carries NO auth field
    /// by design — `TileSource` has no credentials, so no secret is ever written to
    /// disk (keyring-later if auth is ever needed). `#[serde(default)]` migrates
    /// configs that predate this field (absent → `None`); the field is now KNOWN,
    /// so `deny_unknown_fields` is satisfied. `skip_serializing_if` keeps a no-source
    /// config byte-identical to its pre-dyop shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_tile_source: Option<crate::tiles::TileSource>,
    /// AREDN mesh master-node host for Post Office discovery (tuxlink-1w7t).
    /// `None` → discovery defaults to `localnode.local.mesh`. WLE stores a
    /// "Mesh Master Node" setting but IGNORES it (hardcodes localnode); tuxlink
    /// HONORS this value as the sysinfo base host (the P3a divergence/bugfix).
    /// `#[serde(default)]` migrates pre-1w7t configs (absent → `None`); the field
    /// is now KNOWN, satisfying `deny_unknown_fields`. `skip_serializing_if`
    /// keeps a no-override config byte-identical to its pre-1w7t shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aredn_master_node_host: Option<String>,
    /// APRS station identity settings (tuxlink-2f2n). The APRS identity is
    /// SEPARATE from the Winlink identity: an operator transmits APRS as
    /// `<callsign>-<source_ssid>` to `tocall` via `path`. Additive section —
    /// `#[serde(default)]` migrates configs that predate this field (absent →
    /// `AprsConfig::default()`); the field is now KNOWN, satisfying
    /// `deny_unknown_fields`.
    #[serde(default)]
    pub aprs: AprsConfig,
    /// Auto-purge expired Trash on a schedule (tuxlink-wl7n). When `true`
    /// (the default) the app sweeps the Trash folder at startup and every 6h,
    /// permanently removing messages whose `.trash` sidecar `deleted_at` is at
    /// least `trash_retention_days` old. `#[serde(default = ...)]` migrates
    /// configs that predate this field (absent → `true`); the field is KNOWN,
    /// satisfying `deny_unknown_fields`.
    #[serde(default = "default_trash_auto_purge")]
    pub trash_auto_purge: bool,
    /// Retention window (days) before an item in Trash is eligible for
    /// auto-purge (tuxlink-wl7n). Default 30. `#[serde(default = ...)]` migrates
    /// pre-field configs (absent → `30`).
    #[serde(default = "default_trash_retention_days")]
    pub trash_retention_days: u32,
    /// Close-to-tray behavior (tuxlink-5rvp / #882). When `true` (the default)
    /// closing the main window minimizes Tuxlink instead of quitting, so an
    /// active transfer or connection is not interrupted; the process stays
    /// alive in the system tray / window list. When `false` the operator has
    /// opted out and the close button quits the app. `#[serde(default = ...)]`
    /// migrates configs that predate this field (absent → `true`); the field
    /// is now KNOWN, satisfying `deny_unknown_fields`. Always serialized, so per
    /// the rule at `CONFIG_SCHEMA_VERSION` this field bumped the schema to v4
    /// (tuxlink-5rvp): a pre-v4 build must treat a v4 file as Unsupported on
    /// write rather than rejecting the unknown key.
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    /// Whether the one-time close-behavior prompt has been shown (tuxlink-5rvp
    /// / #882). `false` (the default) until the operator answers the first-close
    /// modal explaining the minimize-to-tray behavior; set `true` once answered,
    /// so the prompt never reappears. `#[serde(default)]` migrates pre-field
    /// configs (absent → `false`, the value `bool::default()` already provides,
    /// so no free fn is needed); the field is now KNOWN, satisfying
    /// `deny_unknown_fields`. Always serialized — part of the v4 bump above
    /// (tuxlink-5rvp).
    #[serde(default)]
    pub close_prompt_seen: bool,
    /// Elmer agent pane settings (tuxlink-13v2l, Task 8a).
    ///
    /// `#[serde(default)]` migrates configs that predate this field (absent →
    /// `ElmerConfig::default()`); the field is now KNOWN, satisfying
    /// `deny_unknown_fields`. `skip_serializing_if` keeps a default-config
    /// byte-identical to its pre-elmer shape.
    #[serde(default, skip_serializing_if = "ElmerConfig::is_default")]
    pub elmer: ElmerConfig,
}

impl Config {
    /// Lift the legacy `[modem_ardop]` CAT serial link into the top-level
    /// `[rig]` section (tuxlink-8fkkk). The CAT serial path + baud were RELEASED
    /// under `[modem_ardop]` before the rig config was hoisted to `Config.rig`,
    /// so a config written by an older build carries them there. This migration
    /// runs once at deserialize time (via the hand-written `Deserialize` impl):
    /// when `[rig]` carries no CAT serial path of its own AND a legacy
    /// `[modem_ardop]` carries one, the legacy values are copied into `rig`.
    ///
    /// The legacy fields are `#[serde(skip_serializing)]` on [`ArdopUiConfig`],
    /// so a subsequent save persists the CAT serial link ONLY under `[rig]` and
    /// it stops appearing under `[modem_ardop]` — the migration is one-way and
    /// self-healing. An explicit `[rig].cat_serial_path` always wins (no
    /// clobber of an operator-set value).
    fn migrate_rig_from_legacy_ardop(&mut self) {
        if self.rig.cat_serial_path.is_some() {
            return; // [rig] already owns the CAT serial link; do not clobber.
        }
        if let Some(legacy_path) = self
            .modem_ardop
            .as_ref()
            .and_then(|ardop| ardop.cat_serial_path.clone())
        {
            let legacy_baud = self
                .modem_ardop
                .as_ref()
                .map(|ardop| ardop.cat_baud)
                .unwrap_or_else(default_cat_baud);
            self.rig.cat_serial_path = Some(legacy_path);
            // Carry the legacy baud alongside the path so the rig keys at the
            // proven rate (default 38400 if the legacy config omitted it).
            self.rig.cat_baud = legacy_baud;
        }
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // `remote = "Self"` makes the derive emit this inherent associated fn;
        // we run the post-deserialize rig migration before handing the value back.
        let mut config = Config::deserialize(deserializer)?;
        config.migrate_rig_from_legacy_ardop();
        Ok(config)
    }
}

impl Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // `remote = "Self"` also emits an inherent `serialize` assoc fn (the
        // derive does NOT generate the trait impl when `remote` is set), so the
        // trait impl delegates to it. Serialization is otherwise unchanged.
        Config::serialize(self, serializer)
    }
}

/// A saved Network Post Office relay server entry.
///
/// Dedup key: `(host case-insensitive, port)`. The `callsign` and `label`
/// are display/routing metadata only — they do NOT affect the dedup check.
/// `deny_unknown_fields` is the AMD-11 drift defense for this sub-type too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayFavorite {
    /// Callsign of the relay station (display + B2F login). Non-empty.
    pub callsign: String,
    /// Operator-supplied human label (e.g. "Home mesh relay"). May be empty.
    pub label: String,
    /// Hostname or IP address. Non-empty. Dedup key (case-insensitive).
    pub host: String,
    /// TCP port. Dedup key (exact). Default relay port is 8772.
    pub port: u16,
}

/// Serde default for [`Config::review_inbound_before_download`]: `true` — review
/// before download, the WLE emcomm default (tuxlink-pmp5). A free fn because
/// serde's `default = "..."` takes a path and `bool`'s own `Default` is `false`.
fn default_review_inbound_before_download() -> bool {
    true
}

/// Serde default for [`Config::trash_auto_purge`]: `true` — auto-purge expired
/// Trash on a schedule by default (tuxlink-wl7n). A free fn because serde's
/// `default = "..."` takes a path and `bool`'s own `Default` is `false`.
fn default_trash_auto_purge() -> bool {
    true
}

/// Serde default for [`Config::trash_retention_days`]: `30` days (tuxlink-wl7n).
fn default_trash_retention_days() -> u32 {
    30
}

/// Serde default for [`Config::close_to_tray`]: `true` — closing the window
/// minimizes to tray (the current behavior) by default (tuxlink-5rvp / #882).
/// A free fn because serde's `default = "..."` takes a path and `bool`'s own
/// `Default` is `false`.
fn default_close_to_tray() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectConfig {
    /// Set by wizard Task 9. False = offline-only deployment.
    pub connect_to_cms: bool,
    /// Per the transport-visibility anti-pattern: always explicit, never auto-selected.
    pub transport: CmsTransport,
    /// CMS server host the operator dials (tuxlink-3o0). User-switchable in the
    /// inline SettingsPanel, replacing the former hardcoded `CMS_HOST` const +
    /// hidden `TUXLINK_CMS_HOST` env var (env stays a dev override on top of this).
    /// Default `cms-z.winlink.org` (the dev target that accepts the unregistered
    /// client; production `server.winlink.org` rejects it until tuxlink is
    /// registered). `#[serde(default)]` migrates pre-3o0 configs (no `host` key)
    /// transparently — `host` is now a KNOWN field, so `deny_unknown_fields` is
    /// satisfied.
    #[serde(default = "default_cms_host")]
    pub host: String,
}

/// The default CMS host (tuxlink-3o0). `cms-z.winlink.org` is the dev target that
/// accepts tuxlink's unregistered client SID; production `server.winlink.org`
/// rejects it until tuxlink is registered with Winlink. Mirrors the former
/// `winlink_backend::CMS_HOST` const value. `pub` so the wizard (first-run config
/// construction) and tests can reference the single canonical default.
pub fn default_cms_host() -> String {
    "cms-z.winlink.org".into()
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
    /// PHASE 2 TRANSITIONAL MIRROR (deleted in Phase 3, tuxlink-0063). The active
    /// FULL callsign — a projection of `IdentityStore::last_selected()`'s FULL,
    /// written here so the legacy `cfg.identity.<callsign>` readers compile
    /// unchanged until Phase 3 threads `SessionIdentity` through them. The
    /// IdentityStore (separate file) is the source of truth; this is a cache.
    #[serde(rename = "callsign", deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub active_full: Option<String>,
    /// Free-form station identifier for offline-mode operators (optional).
    /// Allowed on the offline path (`connect_to_cms = false`); not validated as required
    /// in v0.0.1. Same loose-validator rules as `callsign`.
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub identifier: Option<String>,
    /// Maidenhead grid, stored at full 6-char precision when known. Broadcast precision is
    /// governed by PrivacyConfig.position_precision (per Principle 7).
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PositionSource {
    /// Operator has manually entered a grid square; GPS is not used for position.
    Manual,
    /// Default. Position is derived from the GPS receiver.
    Gps,
}

fn default_position_source() -> PositionSource {
    PositionSource::Gps
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
    /// Active position source (tuxlink-686). Default `Gps` (GPS-on-by-default
    /// convention); a deliberate manual grid entry pins this to `Manual` at runtime.
    /// `#[serde(default)]` migrates pre-686 configs transparently (additive field).
    #[serde(default = "default_position_source")]
    pub position_source: PositionSource,
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

/// Serde-friendly mirror of P2's `winlink::ax25::Ax25Params` (which carries a
/// `Duration` that does not round-trip JSON cleanly). Persisted form stores the
/// T1 timer as milliseconds; `into_params()` converts to the runtime type.
/// Defaults are the 1200-baud values (match `Ax25Params::default`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ax25ParamsConfig {
    pub txdelay: u8,
    pub persistence: u8,
    pub slot_time: u8,
    pub paclen: u16,
    pub maxframe: u8,
    pub t1_ms: u64,
    pub n2_retries: u8,
}

impl Default for Ax25ParamsConfig {
    fn default() -> Self {
        // 1200-baud defaults; cross-checked against P2's Ax25Params::default.
        Ax25ParamsConfig {
            txdelay: 30,
            persistence: 63,
            slot_time: 10,
            paclen: 128,
            maxframe: 4,
            t1_ms: 3000,
            n2_retries: 10,
        }
    }
}

impl Ax25ParamsConfig {
    /// Convert to P2's runtime `Ax25Params` type. T1 is honored verbatim — tuxlink-2y4
    /// REVERTED the uhc RF floor (`MIN_RF_T1_MS`): it tripled worst-case airtime
    /// (3 s → 10 s per retransmit) and was the wrong lever. Runaway connect airtime is
    /// now bounded by the connect cap (`Ax25Params::connect_timeout` + a ≤2-SABM key
    /// limit in `datalink::connect`), not by inflating the retransmit timer.
    pub fn into_params(self) -> crate::winlink::ax25::Ax25Params {
        crate::winlink::ax25::Ax25Params {
            txdelay: self.txdelay,
            persistence: self.persistence,
            slot_time: self.slot_time,
            paclen: self.paclen as usize,
            maxframe: self.maxframe,
            t1: std::time::Duration::from_millis(self.t1_ms),
            n2_retries: self.n2_retries,
            // connect_timeout (the RADIO-1 airtime ceiling) is a fixed safety default,
            // not yet operator-tunable from the persisted [packet] section.
            ..Default::default()
        }
    }
}

/// The `[packet]` config section (spec §4.5): the AX.25 packet transport's
/// sticky, persisted settings. Global station SSID is sticky across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PacketConfig {
    /// Global, sticky station SSID (0–15). Operate as `<callsign>-<ssid>`.
    pub ssid: u8,
    /// The last KISS link the operator used (TCP host:port or serial device+baud).
    /// `None` until the operator configures one. Deserialized leniently (tuxlink-efo):
    /// an unrecognized variant degrades to `None` instead of bricking the whole read.
    #[serde(default, deserialize_with = "deserialize_lenient_link")]
    pub link: Option<KissLinkConfig>,
    /// AX.25 timing/windowing knobs (1200-baud defaults).
    pub params: Ax25ParamsConfig,
    /// Idle-listening default-on (spec §4.5): arm `answer()` when not dialing.
    pub listen_default: bool,
}

/// Deserialize `packet.link` leniently (tuxlink-efo): an unrecognized variant
/// (forward/sideways schema skew across concurrent dev builds — the original symptom
/// was a Bluetooth-aware build's config bricking a non-Bluetooth build) degrades to
/// `None` rather than erroring the whole config read. Reads the value as a generic
/// JSON value first (always succeeds for valid JSON), then tries to convert it to a
/// `KissLinkConfig`; any failure (unknown variant, missing/extra fields) yields
/// `None` so the rest of the config still loads.
fn deserialize_lenient_link<'de, D>(de: D) -> Result<Option<KissLinkConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(de)?;
    Ok(value.and_then(|v| serde_json::from_value::<KissLinkConfig>(v).ok()))
}

impl Default for PacketConfig {
    fn default() -> Self {
        PacketConfig {
            ssid: 0,
            link: None,
            params: Ax25ParamsConfig::default(),
            listen_default: true, // spec §4.5: listen is default-on
        }
    }
}

/// Reduce a grid stored at full precision to the form that may leave the
/// application on air (tuxlink-882). The grid is *stored* at full 6-char
/// precision; this is the privacy boundary: `FourCharGrid` (default) yields the
/// first 4 characters, `SixCharGrid` (opt-in) the first 6. Char-based truncation
/// is safe for ASCII Maidenhead locators. Any broadcast surface (the CMS handshake
/// locator today) MUST pass through this rather than the raw stored grid.
pub fn broadcast_grid(grid: &str, precision: PositionPrecision) -> String {
    let keep = match precision {
        PositionPrecision::FourCharGrid => 4,
        PositionPrecision::SixCharGrid => 6,
    };
    grid.chars().take(keep).collect()
}

fn deserialize_schema_version<'de, D>(d: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let v = u32::deserialize(d)?;
    match detect_schema_action(v) {
        // Current, or an additively-loadable older version (tuxlink-ulrz): accept and
        // NORMALIZE the in-memory marker to current so the next write re-stamps the
        // file forward. New fields fill via their serde defaults.
        SchemaAction::Current | SchemaAction::MigrateAdditive => Ok(CONFIG_SCHEMA_VERSION),
        // v1 is a breaking migration handled at startup (migrate_identity_if_v1)
        // BEFORE read_config; reaching here means it was not migrated.
        SchemaAction::MigrateFromV1 => Err(serde::de::Error::custom(
            "config schema_version 1 requires the startup v1→v2 migration",
        )),
        // Above current: a newer build wrote it. Refuse rather than mis-load a
        // forward-incompatible config (the downgrade case).
        SchemaAction::Unsupported { found } => Err(serde::de::Error::custom(format!(
            "unsupported config schema_version {} (this binary supports up to {})",
            found, CONFIG_SCHEMA_VERSION
        ))),
    }
}

fn deserialize_optional_nonempty_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // Maps JSON `null` → None; maps JSON `""` → None (treat empty-string as missing);
    // maps non-empty string → Some(s). Eliminates Some("") ambiguity per spec adrev R4 P1-1.
    let opt = <Option<String>>::deserialize(d)?;
    Ok(opt.filter(|s| !s.is_empty()))
}

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

/// Resolve the config file path. Precedence: `TUXLINK_CONFIG_DIR` (tuxlink-efo dev
/// override) > `XDG_CONFIG_HOME` > `~/.config`, ending in `.../tuxlink/config.json`
/// (or `<TUXLINK_CONFIG_DIR>/config.json`).
pub fn config_path() -> std::path::PathBuf {
    resolve_config_path(
        std::env::var_os("TUXLINK_CONFIG_DIR"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// The identity store (`identities.json`) lives next to `config.json` (Phase 1
/// store.rs design + the TUXLINK_CONFIG_DIR per-worktree isolation). Phase 2.
pub fn identity_store_path() -> std::path::PathBuf {
    config_path()
        .parent()
        .map(|p| p.join("identities.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("identities.json"))
}

/// Pure resolver behind [`config_path`] (testable without process-global env).
/// `TUXLINK_CONFIG_DIR` (tuxlink-efo) is a tuxlink-specific override so a per-worktree
/// dev build points at its OWN config dir — concurrent builds then stop contaminating
/// one shared `~/.config/tuxlink/config.json` (the dev cousin of the Vite :1420
/// collision). The dir holds `config.json` directly. Falls back to `XDG_CONFIG_HOME`,
/// then `~/.config`.
fn resolve_config_path(
    tuxlink_config_dir: Option<std::ffi::OsString>,
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> std::path::PathBuf {
    if let Some(dir) = tuxlink_config_dir {
        return std::path::PathBuf::from(dir).join("config.json");
    }
    let base = xdg_config_home
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = home.expect("HOME must be set");
            std::path::PathBuf::from(home).join(".config")
        });
    base.join("tuxlink").join("config.json")
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("CMS path requires an active FULL identity to be selected")]
    CmsPathNoActiveFull,
    #[error("invalid identity field `{field}`: {rule}")]
    InvalidIdentity { field: &'static str, rule: &'static str },
    #[error("packet.ssid {ssid} is out of the 0–15 AX.25 range")]
    PacketSsidOutOfRange { ssid: u8 },
}

impl Config {
    /// Cross-field semantic validation (can't be expressed via serde deserialize-with).
    /// Callers (wizard's `wizard_persist_cms`, `read_config`) invoke after deserialization.
    /// NOT auto-called by `write_config_atomic` — caller responsibility per spec §3.3.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.connect.connect_to_cms && self.identity.active_full.is_none() {
            return Err(ConfigValidationError::CmsPathNoActiveFull);
        }
        // The offline-forbids-callsign rule is intentionally removed (Phase 2,
        // tuxlink-7iy2): a P2P/RF-only deployment may select a FULL identity, and a
        // tactical operates with no own CMS account. The CMS<->callsign biconditional
        // was false under tactical identities.
        if let Some(ref c) = self.identity.active_full {
            if let Some(rule) = validate_identity_describe(c) {
                return Err(ConfigValidationError::InvalidIdentity { field: "callsign", rule });
            }
        }
        if let Some(ref i) = self.identity.identifier {
            if let Some(rule) = validate_identity_describe(i) {
                return Err(ConfigValidationError::InvalidIdentity { field: "identifier", rule });
            }
        }
        if self.packet.ssid > 15 {
            return Err(ConfigValidationError::PacketSsidOutOfRange { ssid: self.packet.ssid });
        }
        Ok(())
    }
}

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
    #[error("refuse to overwrite existing config at {path:?}: this binary cannot fully parse it \
             (a newer build likely added a field without bumping CONFIG_SCHEMA_VERSION, or the \
             file is corrupt). Preserving it rather than clobbering with defaults. Source: {source}")]
    ExistingConfigUnreadable { path: std::path::PathBuf, source: serde_json::Error },
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
/// serialization (flock) out of scope for v0.0.1.
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
                match detect_schema_action(probe.schema_version) {
                    // Future / unknown version (a newer build wrote it): refuse so a
                    // downgrade never clobbers it.
                    SchemaAction::Unsupported { found } => {
                        return Err(ConfigWriteError::SchemaVersionMismatch {
                            existing: found,
                            ours: CONFIG_SCHEMA_VERSION,
                        });
                    }
                    // Current or additively-loadable: this binary is EXPECTED to parse
                    // the existing file. Defense-in-depth (tuxlink-ulrz): the probe
                    // reads ONLY schema_version, so a config at a known version but with
                    // an UNKNOWN field (a field a newer build added without bumping the
                    // version — the exact bug that motivated this) passes the probe yet
                    // cannot be loaded. Refuse to clobber it with defaults; preserve it.
                    SchemaAction::Current | SchemaAction::MigrateAdditive => {
                        if let Err(source) = serde_json::from_slice::<Config>(&bytes) {
                            return Err(ConfigWriteError::ExistingConfigUnreadable {
                                path: path.clone(),
                                source,
                            });
                        }
                    }
                    // A v1 file is a legitimate BREAKING-migration target that does NOT
                    // parse as the current Config — the v1→v2 migration is precisely
                    // what is about to rewrite it. Do NOT apply the full-parse check.
                    SchemaAction::MigrateFromV1 => {}
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(ConfigWriteError::ProbeReadFailed { path: path.clone(), source: e });
        }
    }

    // Same-directory tempfile → atomic persist on local POSIX FS.
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(tmp.as_file(), config)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path).map_err(|e| ConfigWriteError::Io(e.error))?;

    // Parent-dir fsync per adrev R2 P0-3 + R4 P0-1: rename(2) is atomic but not DURABLE
    // until the parent directory's metadata flushes. tempfile::persist does not do this.
    let parent_dir = std::fs::File::open(parent)?;
    parent_dir.sync_all()?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct SchemaVersionProbe { schema_version: u32 }

/// How tuxlink keys the radio for ARDOP transmit (tuxlink-wu0k).
///
/// - [`Vox`](PttMethod::Vox) — no PTT line; the radio keys on detected audio
///   (or an external VOX). ardopcf is spawned with no PTT flag.
/// - [`SerialRts`](PttMethod::SerialRts) — ardopcf toggles RTS on the serial
///   port named by [`ArdopUiConfig::ptt_serial_path`] (`-p <path>`). The legacy
///   default before CAT PTT existed.
/// - [`CatCommand`](PttMethod::CatCommand) — the radio keys ONLY by a CAT
///   command (e.g. the Yaesu FT-710: `TX1;` / `TX0;`), and the serial port must
///   be CLOSED during audio to avoid USB-tree codec contention. tuxlink owns a
///   close-serial CAT-PTT bridge and points ardopcf at it over TCP
///   (`-c TCP:<port> -k <hex(key)> -u <hex(unkey)>`). Proven on air 2026-06-23.
///
/// Serialized lowercase (`"vox"` / `"serialRts"` is `"serial_rts"`) — see the
/// `rename_all = "snake_case"` attribute. Default is [`Vox`](PttMethod::Vox).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PttMethod {
    /// No PTT line — VOX / audio-detected keying.
    #[default]
    Vox,
    /// ardopcf RTS PTT on `ptt_serial_path` (`-p <path>`).
    SerialRts,
    /// CAT-command PTT via tuxlink's close-serial bridge.
    CatCommand,
}

/// Default CAT baud rate (the FT-710 Enhanced port speed proven 2026-06-23).
fn default_cat_baud() -> u32 {
    38400
}
/// Default rig data mode token (the value rigctld's `M` command sets). PKTUSB
/// is the HF Winlink default and the proven FT-710 data mode. Backs
/// [`RigUiConfig`] (tuxlink-31c63).
fn default_data_mode() -> String {
    "PKTUSB".to_string()
}
/// Default CAT key command (FT-710 `TX1;`).
fn default_cat_key_cmd() -> String {
    "TX1;".into()
}
/// Default CAT unkey command (FT-710 `TX0;`).
fn default_cat_unkey_cmd() -> String {
    "TX0;".into()
}
/// Default loopback TCP port the CAT-PTT bridge listens on (proven 2026-06-23).
fn default_cat_bridge_port() -> u16 {
    4532
}
/// Default rigctld host (loopback — rigctld almost always runs on the same
/// machine). Backs [`RigUiConfig`] (tuxlink-8fkkk).
fn default_rigctld_host() -> String {
    "127.0.0.1".into()
}
/// Default rigctld port. **4534, NOT hamlib's upstream 4532 (C1 fix,
/// tuxlink-8fkkk).** The close-serial CAT-PTT bridge ([`default_cat_bridge_port`])
/// binds 4532; if a tuxlink-spawned rigctld also defaulted to 4532 the two would
/// collide on the loopback bind. Defaulting rigctld to 4534 keeps the rig-control
/// endpoint clear of the bridge. (`RigUiConfig::default().rigctld_port !=
/// ArdopUiConfig::default().cat_bridge_port` is asserted by a unit test.)
fn default_rigctld_port() -> u16 {
    4534
}
/// Default rigctld binary name (on $PATH on most Linux distros that ship
/// hamlib). Backs [`RigUiConfig`] (tuxlink-8fkkk).
fn default_rigctld_binary() -> String {
    "rigctld".into()
}

/// Frontend-shaped ARDOP HF modem settings. Persisted as `[modem_ardop]` in config;
/// Task 3.3 (`modem_ardop_connect`) translates this into `ArdopConfig::extra_args` at
/// spawn time. `deny_unknown_fields` is intentionally absent here — the ARDOP config
/// is additive and forward-compat relaxation is acceptable for a new section.
///
/// `Deserialize` is hand-written (see below) ONLY to migrate pre-CAT-PTT
/// configs: a stored config that predates `ptt_method` derives the method from
/// the legacy `ptt_serial_path` field (`Some` → [`PttMethod::SerialRts`], `None`
/// → [`PttMethod::Vox`]) so old configs keep keying exactly as before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArdopUiConfig {
    /// Path or name of the ARDOP binary (e.g. `"ardopcf"`).
    pub binary: String,
    /// ALSA capture device (e.g. `"plughw:1,0"`).
    pub capture_device: String,
    /// ALSA playback device (e.g. `"plughw:1,0"`).
    pub playback_device: String,
    /// How tuxlink keys the radio (tuxlink-wu0k). Default [`PttMethod::Vox`].
    /// Migrated from `ptt_serial_path` for pre-CAT-PTT configs (see the
    /// hand-written `Deserialize`).
    #[serde(default)]
    pub ptt_method: PttMethod,
    /// Serial device for RTS PTT control. Consulted only when
    /// `ptt_method == SerialRts`. `None` = no serial RTS PTT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptt_serial_path: Option<String>,
    /// MIGRATION-ONLY (tuxlink-8fkkk). The CAT serial device once lived under
    /// `[modem_ardop]`; it is now radio-level on [`RigUiConfig`]
    /// (`Config.rig.cat_serial_path`). This field is retained ONLY to read a
    /// legacy `[modem_ardop].cat_serial_path` at deserialize time so
    /// [`Config::migrate_rig_from_legacy_ardop`] can lift it into `[rig]`.
    /// `skip_serializing` ensures a re-save never writes it back under
    /// `[modem_ardop]` — the CAT serial link persists ONLY under `[rig]`.
    /// Read the live value from `Config.rig`, NOT here.
    #[serde(default, skip_serializing)]
    pub cat_serial_path: Option<String>,
    /// MIGRATION-ONLY (tuxlink-8fkkk). Legacy `[modem_ardop].cat_baud`, lifted
    /// into `Config.rig.cat_baud` at deserialize time. `skip_serializing` keeps
    /// it out of a re-saved `[modem_ardop]`. Read the live value from
    /// `Config.rig`. Default 38400 (the FT-710 Enhanced port).
    #[serde(default = "default_cat_baud", skip_serializing)]
    pub cat_baud: u32,
    /// CAT key command (e.g. `TX1;`). Default `TX1;`.
    #[serde(default = "default_cat_key_cmd")]
    pub cat_key_cmd: String,
    /// CAT unkey command (e.g. `TX0;`). Default `TX0;`.
    #[serde(default = "default_cat_unkey_cmd")]
    pub cat_unkey_cmd: String,
    /// Loopback TCP port the CAT-PTT bridge listens on; ardopcf is pointed at it
    /// via `-c TCP:<port>`. Default 4532. Distinct from `cmd_port`/`data_port`.
    #[serde(default = "default_cat_bridge_port")]
    pub cat_bridge_port: u16,
    /// ARDOP command/control port (default 8515).
    pub cmd_port: u16,
    /// ARDOP ARQ bandwidth in Hz. One of {200, 500, 1000, 2000}. None means
    /// "let ardopcf use its default" (typically 2000 Hz, but the operator may
    /// have set a different default via the WebGUI or persistent config).
    /// The value, if set, is sent as `ARQBW <hz> FORCED` during init_tnc so
    /// the client-side preference overrides the server's preference for
    /// outbound calls (tuxlink-j0ij).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_hz: Option<u32>,
    /// ARDOP transmit drive level (0–100), sent as `DRIVELEVEL <n>` during
    /// init_tnc. `None` = leave at ardopcf's default. Too high a value clips
    /// the ARDOP multicarrier waveform and splatters across the band (unlike a
    /// single tone); ~40 is a clean digital level on a typical USB-soundcard
    /// chain (verified on-air 2026-06-25).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drive_level: Option<u8>,
    /// ConReq repeats packed into `ARQCALL <target> <n>` on an outbound connect.
    /// `None` = built-in default (15 ≈ ~50 s, bounded by the 120 s connect
    /// deadline). A real gateway may need to wake up and tune, and ARDOP is not
    /// tune-aware, so the call must sustain ConReqs; the prior fixed value of 3
    /// (~10 s) was too short to raise one (2026-06-25). Clamped to 2..=30 when read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_attempts: Option<u32>,
    /// ardopcf built-in WebGUI port. `None` (the default) means "derive from
    /// `cmd_port - 1`" per ardopcf's documented convention (default
    /// 8515 → 8514). An explicit `Some(port)` overrides the derivation so the
    /// operator can pin the WebGUI to a known port when ardopcf is built or
    /// configured to bind somewhere non-standard.
    ///
    /// Persisted shape decided by operator smoke 2026-05-31 round 3: the
    /// "Open WebGUI" button targets `http://localhost:<webgui_port>/` and the
    /// spawn passes `-G <webgui_port>` — both ends MUST read from the same
    /// source, so the derivation logic and the override flag are colocated on
    /// the config struct rather than recomputed at each call site.
    ///
    /// Use [`ArdopUiConfig::resolved_webgui_port`] to read the effective
    /// port; do not access this field directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgui_port: Option<u16>,
    /// How long the inbound listener stays armed, in MINUTES, before it
    /// self-expires. `0` (the default) means **no expiry** — the listener
    /// stays armed until the operator disarms it (WLE-parity; 2026-06-16
    /// operator decision). Any positive value arms for that many minutes; the
    /// arm path maps `0` to [`crate::winlink::listener::arms_record::NO_EXPIRY`]
    /// and a positive `n` to `Duration::from_secs(n * 60)`.
    ///
    /// Replaces a prior hardcoded 1-hour auto-expiry that was framed as
    /// "RADIO-1" — RADIO-1 governs agent behavior, not the app's UX, so the
    /// listener no longer self-closes by default (tuxlink-5g5d).
    #[serde(default)]
    pub listen_ttl_minutes: u32,
    // tuxlink-8fkkk: the rig-control fields (rig_hamlib_model, rigctld_host,
    // rigctld_port, rigctld_binary, close_serial_sequencing, live_vfo_poll,
    // qsy_on_fail) were hoisted to the radio-level [`RigUiConfig`]
    // (`Config.rig`) so VARA reaches the same rig as ARDOP. They were
    // UNRELEASED here (added in this PR's predecessor, never shipped under
    // `[modem_ardop]`), so they are removed cleanly with no migration. The CAT
    // serial link (cat_serial_path / cat_baud) WAS released here, so those two
    // remain above as migration-only `skip_serializing` fields.
}

impl ArdopUiConfig {
    /// Resolve the effective WebGUI port: explicit `webgui_port` if set,
    /// otherwise `cmd_port - 1` (ardopcf's documented convention).
    ///
    /// Returns `None` when `cmd_port < 2` AND no explicit override is set —
    /// that case can't derive a valid bindable TCP port and the WebGUI is
    /// disabled in the spawn (no `-G` flag emitted). Both `build_ardop_extra_args`
    /// and the frontend `onOpenWebGuiClick` consult this single helper so they
    /// agree on the port regardless of operator overrides.
    pub fn resolved_webgui_port(&self) -> Option<u16> {
        if let Some(p) = self.webgui_port {
            return Some(p);
        }
        if self.cmd_port >= 2 {
            Some(self.cmd_port - 1)
        } else {
            None
        }
    }
}

impl Default for ArdopUiConfig {
    fn default() -> Self {
        Self {
            binary: "ardopcf".into(),
            capture_device: String::new(),
            playback_device: String::new(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: default_cat_baud(),
            cat_key_cmd: default_cat_key_cmd(),
            cat_unkey_cmd: default_cat_unkey_cmd(),
            cat_bridge_port: default_cat_bridge_port(),
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            // None → derive from cmd_port - 1 (8514 with the default cmd_port).
            // Operator can pin explicitly via the radio panel when ardopcf's
            // build/config has the WebGUI somewhere non-standard.
            webgui_port: None,
            // 0 = no self-expiry (WLE-parity default; 2026-06-16 operator
            // decision). Operator opts into a finite duration in minutes.
            listen_ttl_minutes: 0,
        }
    }
}

impl<'de> Deserialize<'de> for ArdopUiConfig {
    /// Hand-written so a pre-CAT-PTT config (no `ptt_method` key) MIGRATES
    /// instead of defaulting blindly: an old config that recorded a
    /// `ptt_serial_path` was using ardopcf's RTS PTT, so it deserializes to
    /// [`PttMethod::SerialRts`]; an old config with no PTT path was VOX, so it
    /// deserializes to [`PttMethod::Vox`]. A config that already carries
    /// `ptt_method` is honored verbatim. All other fields fill via their serde
    /// defaults exactly as the derived impl would.
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Shadow struct with the SAME serde shape as ArdopUiConfig, except
        // `ptt_method` is optional so absence is detectable for migration.
        #[derive(Deserialize)]
        struct Shadow {
            binary: String,
            capture_device: String,
            playback_device: String,
            #[serde(default)]
            ptt_method: Option<PttMethod>,
            #[serde(default)]
            ptt_serial_path: Option<String>,
            #[serde(default)]
            cat_serial_path: Option<String>,
            #[serde(default = "default_cat_baud")]
            cat_baud: u32,
            #[serde(default = "default_cat_key_cmd")]
            cat_key_cmd: String,
            #[serde(default = "default_cat_unkey_cmd")]
            cat_unkey_cmd: String,
            #[serde(default = "default_cat_bridge_port")]
            cat_bridge_port: u16,
            cmd_port: u16,
            #[serde(default)]
            bandwidth_hz: Option<u32>,
            #[serde(default)]
            drive_level: Option<u8>,
            #[serde(default)]
            connect_attempts: Option<u32>,
            #[serde(default)]
            webgui_port: Option<u16>,
            #[serde(default)]
            listen_ttl_minutes: u32,
            // tuxlink-8fkkk: rig fields hoisted to RigUiConfig; cat_serial_path
            // / cat_baud remain (migration-only — read here to lift into [rig]).
        }

        let s = Shadow::deserialize(de)?;
        let ptt_method = s.ptt_method.unwrap_or_else(|| {
            // Back-compat migration: derive from the legacy field.
            if s.ptt_serial_path.is_some() {
                PttMethod::SerialRts
            } else {
                PttMethod::Vox
            }
        });

        Ok(ArdopUiConfig {
            binary: s.binary,
            capture_device: s.capture_device,
            playback_device: s.playback_device,
            ptt_method,
            ptt_serial_path: s.ptt_serial_path,
            cat_serial_path: s.cat_serial_path,
            cat_baud: s.cat_baud,
            cat_key_cmd: s.cat_key_cmd,
            cat_unkey_cmd: s.cat_unkey_cmd,
            cat_bridge_port: s.cat_bridge_port,
            cmd_port: s.cmd_port,
            bandwidth_hz: s.bandwidth_hz,
            drive_level: s.drive_level,
            connect_attempts: s.connect_attempts,
            webgui_port: s.webgui_port,
            listen_ttl_minutes: s.listen_ttl_minutes,
        })
    }
}

/// Frontend-shaped VARA modem settings. Persisted as `[modem_vara]` in config.
/// Phase 2 (bd-tuxlink-dfmf) — minimal TCP-transport config; full session-state
/// integration (B2F over VARA, RADIO-1 connect-to-peer) arrives in Phase 3.
///
/// VARA differs from ARDOP in two ways tuxlink models:
///   1. VARA is a separate third-party process tuxlink does NOT spawn — only
///      `host` + `cmd_port` + `data_port` are needed (no `binary`, no audio
///      device hints; VARA handles its own audio).
///   2. VARA exposes 3 variants — HF Standard (2300 Hz), HF Tactical (2750
///      Hz), and VARA FM (~6800 Hz). The variant is selected operator-side
///      via `bandwidth_hz` and which VARA instance the operator pointed
///      tuxlink at (different binaries listen on different ports).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaraUiConfig {
    /// VARA cmd-socket host. Default `127.0.0.1` (local-machine VARA).
    pub host: String,
    /// VARA command socket port. Default `8300`.
    pub cmd_port: u16,
    /// VARA data socket port. Default `8301` (conventionally `cmd_port + 1`).
    pub data_port: u16,
    /// VARA bandwidth in Hz. Common values: 500 (narrow HF), 2300 (HF
    /// Standard), 2750 (HF Tactical), ~6800 (VARA FM). `None` = leave VARA
    /// at whatever bandwidth it was last configured for (don't send `BW` at
    /// session start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_hz: Option<u32>,
}

impl Default for VaraUiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            cmd_port: 8300,
            data_port: 8301,
            bandwidth_hz: None,
        }
    }
}

// ============================================================================
// Radio-level CAT / rig-control config (tuxlink-8fkkk)
// ============================================================================

/// Radio-level CAT / rig-control settings, persisted under `[rig]` in config.
///
/// Describes ONE physical transceiver and is consumed by BOTH the ARDOP and
/// VARA connect/tune paths (a station has one radio; the modem mode is
/// orthogonal). Hoisted out of `[modem_ardop]` (tuxlink-8fkkk) so VARA reaches
/// the same rig as ARDOP.
///
/// Fields:
/// - `rig_hamlib_model`: hamlib model ID for rigctld-based QSY / VFO control.
///   `None` = no rigctld integration (close-serial CAT-PTT still works without
///   it). e.g. `1049` for Icom IC-7300, `1037` for Yaesu FT-817.
/// - `rigctld_host` / `rigctld_port`: where rigctld listens. Default
///   `127.0.0.1:4534` — **4534 NOT 4532** to avoid colliding with the
///   close-serial CAT-PTT bridge (which binds 4532). See [`default_rigctld_port`].
/// - `rigctld_binary`: binary name/path tuxlink spawns. Default `"rigctld"`.
/// - `close_serial_sequencing`: when `true`, close the CAT serial before audio
///   and re-open after TX (radios that share one serial between CAT + audio PTT).
/// - `live_vfo_poll`: when `true`, poll the VFO from rigctld so the panel's
///   frequency readout stays current.
/// - `qsy_on_fail`: when `true`, walk the ranked candidate frequencies on a
///   connect failure (QSY to the next gateway/freq). Default `false`.
/// - `cat_serial_path` / `cat_baud`: the CAT serial link (device + baud). The
///   ARDOP CAT-PTT bridge and the rigctld QSY path both use this serial port.
///
/// `Deserialize` is derived with `#[serde(default = ...)]` per field — matching
/// [`VaraUiConfig`]'s derived style — so a config that predates `[rig]` (or
/// omits any field) fills from the defaults. `deny_unknown_fields` is
/// intentionally absent (consistent with the other additive UI-config sections).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RigUiConfig {
    /// Hamlib rig model ID for rigctld-based QSY / VFO control. `None` = no
    /// rigctld integration. `skip_serializing_if` keeps a no-model config tidy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig_hamlib_model: Option<u32>,
    /// Host where rigctld is listening. Default `127.0.0.1`.
    #[serde(default = "default_rigctld_host")]
    pub rigctld_host: String,
    /// TCP port rigctld is listening on. Default `4534` (NOT 4532 — avoids the
    /// CAT-PTT bridge bind collision; see [`default_rigctld_port`]).
    #[serde(default = "default_rigctld_port")]
    pub rigctld_port: u16,
    /// rigctld binary name or path. Default `"rigctld"` (expected on $PATH).
    #[serde(default = "default_rigctld_binary")]
    pub rigctld_binary: String,
    /// Close the CAT serial before audio and re-open after TX (internal-codec
    /// radios that share one serial between CAT and audio PTT). Default `false`.
    #[serde(default)]
    pub close_serial_sequencing: bool,
    /// Poll the VFO frequency from rigctld in real time so the panel readout
    /// stays current. Default `false`.
    #[serde(default)]
    pub live_vfo_poll: bool,
    /// Walk the ranked candidate frequencies on a connect failure (QSY to the
    /// next gateway/freq). Default `false`.
    #[serde(default)]
    pub qsy_on_fail: bool,
    /// CAT serial device for QSY/VFO control and the ARDOP CAT-PTT bridge.
    /// `None` until the operator picks a port. `skip_serializing_if` keeps an
    /// unset config tidy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cat_serial_path: Option<String>,
    /// CAT serial baud. Default 38400 (the FT-710 Enhanced port speed proven
    /// 2026-06-23).
    #[serde(default = "default_cat_baud")]
    pub cat_baud: u32,
    /// Rig data mode token (e.g. "PKTUSB", "USB-D") rigctld sets on tune. Default
    /// "PKTUSB". Parsed via `tux_rig::Mode::from_rigctl`; an unrecognised token
    /// falls back to the ardop default at tune time. (tuxlink-31c63)
    #[serde(default = "default_data_mode")]
    pub data_mode: String,
    /// Logical keys of profile-managed fields the operator has hand-edited, so a
    /// later radio change does NOT overwrite them with the new radio's profile
    /// value. Keys: "ptt_method", "data_mode", "cat_baud", "close_serial".
    /// Additive; empty by default. (tuxlink-31c63)
    #[serde(default)]
    pub rig_field_overrides: Vec<String>,
}

impl Default for RigUiConfig {
    fn default() -> Self {
        Self {
            rig_hamlib_model: None,
            rigctld_host: default_rigctld_host(),
            rigctld_port: default_rigctld_port(),
            rigctld_binary: default_rigctld_binary(),
            close_serial_sequencing: false,
            live_vfo_poll: false,
            qsy_on_fail: false,
            cat_serial_path: None,
            cat_baud: default_cat_baud(),
            data_mode: default_data_mode(),
            rig_field_overrides: Vec::new(),
        }
    }
}

// ============================================================================
// Telnet-P2P listener config (tuxlink-xehu)
// ============================================================================

/// Telnet-P2P listener settings. The allowlist + station password live OUTSIDE
/// this struct (allowlist in `<config-dir>/listener/telnet/allowed_stations.json`,
/// password in the OS keyring) so this struct carries only the bind + TTL knobs.
///
/// ## Defaults (DIVERGE from WLE)
///
/// | Knob       | tuxlink default | WLE default     | Why                                |
/// |------------|-----------------|-----------------|------------------------------------|
/// | `port`     | 8774            | 8774            | parity (telnet-p2p.md §1)          |
/// | `bind_addr`| `"127.0.0.1"`   | `"Default"` ≈ 0.0.0.0 | telnet-p2p.md §9.3 — operator opts into LAN |
/// | `ttl_secs` | 3600 (1 hour)   | infinite        | RADIO-1 framing — arming = consent for armed window |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TelnetListenUiConfig {
    /// TCP port the listener binds. Default 8774 per
    /// `dev/scratch/winlink-re/findings/telnet-p2p.md §1` (NOT 8772 — that's
    /// the RMS-Relay hub port).
    pub port: u16,
    /// Bind address. Default `127.0.0.1` (loopback) — DIVERGES from WLE's
    /// "all interfaces" default per telnet-p2p.md §9.3. Operator opts into
    /// LAN/all by setting this to `"0.0.0.0"` or a specific NIC address.
    pub bind_addr: String,
    /// Arm-window TTL in seconds. Default 3600 (1 hour). Operator can set
    /// shorter for narrower consent windows.
    pub ttl_secs: u64,
}

impl Default for TelnetListenUiConfig {
    fn default() -> Self {
        Self {
            port: 8774,
            bind_addr: "127.0.0.1".into(),
            ttl_secs: 3600,
        }
    }
}

// ============================================================================
// APRS station identity config (tuxlink-2f2n)
// ============================================================================

/// APRS station identity settings, persisted under `aprs` in config.json.
///
/// The APRS identity is SEPARATE from the Winlink identity: an operator
/// transmits APRS as `<base-callsign>-<source_ssid>` (the base callsign comes
/// from the selected FULL identity; `source_ssid` is the APRS-specific SSID,
/// independent of the packet/Winlink SSID) addressed to `tocall` via `path`.
///
/// - `source_ssid`: APRS SSID (0–15). Default 0.
/// - `tocall`: the destination/"to" call identifying the sending software.
///   Default `APZTUX` (an experimental `APZ…` tocall reserved for tuxlink).
/// - `path`: the digipeater alias list. Default `WIDE1-1,WIDE2-1` (the common
///   2-hop wide path); parsed via [`crate::winlink::aprs::identity::parse_path`]
///   (0..=2 hops, AX.25 limit).
///
/// `deny_unknown_fields` is intentionally absent (consistent with the other
/// additive UI-config sections): the section is forward-compat relaxed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AprsConfig {
    pub source_ssid: u8,
    pub tocall: String,
    pub path: String,
}

impl Default for AprsConfig {
    fn default() -> Self {
        Self { source_ssid: 0, tocall: "APZTUX".into(), path: "WIDE1-1,WIDE2-1".into() }
    }
}

// ---------------------------------------------------------------------------
// ElmerConfig — Elmer agent pane settings (tuxlink-13v2l, Task 8a)
// ---------------------------------------------------------------------------

/// Operator-configurable settings for the Elmer agent pane.
///
/// All fields default to the loopback-ollama posture; no config file entry is
/// required for a local-model setup. The endpoint is OPERATOR-ONLY — it is
/// never written by the agent or reachable from a tool result (AC-7, SSRF-2).
///
/// Added under `#[serde(default, skip_serializing_if = "ElmerConfig::is_default")]`
/// on `Config` so pre-elmer config files load cleanly and remain byte-identical
/// to their pre-elmer shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElmerConfig {
    /// The chat-completions endpoint URL. Must resolve to a loopback address
    /// (`127.0.0.0/8` / `::1` / `localhost`) — see `LoopbackEndpoint::parse`.
    ///
    /// Default: local Ollama (`http://127.0.0.1:11434/v1/chat/completions`).
    pub agent_endpoint: String,
    /// The model identifier string passed to the endpoint.
    ///
    /// Default: `"llama3"` (the most common locally-hosted Ollama model name).
    pub agent_model: String,
    /// Per-turn wall-clock timeout for one Elmer agent turn, in SECONDS
    /// (tuxlink-1wi5w). Replaces the former hardcoded 15-minute ceiling in
    /// `elmer/session.rs`. Large local models (e.g. `gpt-oss-120b` on a modest
    /// backend) routinely need minutes per turn, so the operator can raise or
    /// lower this from the Elmer Model form; `elmer_config_set` clamps the
    /// requested value to `[30, 3600]` seconds (0.5–60 min) before persisting,
    /// and the live-applied per-turn build (`ElmerSession::send`) reads the
    /// clamped value off the model-config snapshot.
    ///
    /// Default: `900` (15 minutes — the prior hardcoded value).
    /// `#[serde(default = "default_agent_turn_timeout_secs")]` migrates configs
    /// that predate this field (absent → `900`). `ElmerConfig` carries no
    /// `deny_unknown_fields`, so this additive field is backward- AND
    /// forward-compatible without a `CONFIG_SCHEMA_VERSION` bump.
    #[serde(default = "default_agent_turn_timeout_secs")]
    pub agent_turn_timeout_secs: u32,
    /// Whether the operator has completed the Elmer model-access onboarding flow
    /// (tuxlink-wpqwy). `false` on first run (absent from disk → serde default).
    /// Set to `true` by `config_set_inner` on any successful save. The DTO's
    /// migration expression `onboarded || !is_default()` means an existing user
    /// whose config content already differs from factory defaults is treated as
    /// onboarded even before their first explicit save with this field present.
    ///
    /// `#[serde(default)]` deserializes absent-from-disk entries as `false` so
    /// pre-onboarding configs load cleanly. `ElmerConfig` has no
    /// `deny_unknown_fields`, so this additive field is backward- AND
    /// forward-compatible.
    #[serde(default)]
    pub onboarded: bool,
}

/// Serde default for [`ElmerConfig::agent_turn_timeout_secs`]: `900` seconds
/// (15 minutes — the prior hardcoded per-turn ceiling). A free fn because
/// serde's `default = "..."` takes a path and `u32`'s own `Default` is `0`.
fn default_agent_turn_timeout_secs() -> u32 {
    900
}

impl Default for ElmerConfig {
    fn default() -> Self {
        Self {
            agent_endpoint: "http://127.0.0.1:11434/v1/chat/completions".into(),
            agent_model: "llama3".into(),
            agent_turn_timeout_secs: default_agent_turn_timeout_secs(),
            onboarded: false,
        }
    }
}

impl ElmerConfig {
    /// Returns `true` when this config is byte-for-byte equivalent to the
    /// default — used by `#[serde(skip_serializing_if)]` to keep the config
    /// file clean for operators who have not customized the Elmer endpoint.
    ///
    /// `onboarded` IS included in this check (tuxlink-wpqwy persistence fix).
    /// When a user saves with default content (e.g. the local Ollama defaults)
    /// and the picker sets `onboarded = true`, this method must return `false`
    /// so that `skip_serializing_if` does NOT skip the `elmer` section.  If
    /// `onboarded` were excluded, `is_default()` would return `true` for that
    /// case and the whole section would be omitted, losing the `onboarded`
    /// flag on disk — causing the picker to re-appear on the next launch.
    ///
    /// Migration read expression (`onboarded || !is_default()`) still works:
    /// - Never-touched (flag false, content default): `is_default()` = true
    ///   (both match their defaults); read yields `false || false = false` →
    ///   picker shows correctly.
    /// - Old customized pre-field (content differs, flag absent → false):
    ///   content mismatch makes `is_default()` = false; read yields
    ///   `false || true = true` → no picker (migration path).
    /// - Saved default-content (flag true, content default): flag differs from
    ///   the default `false`, so `is_default()` = false; section persists;
    ///   read yields `true || … = true` → no picker (this bug fixed).
    /// - Saved customized (flag true, content differs): `is_default()` = false;
    ///   persists; read true → no picker.
    pub fn is_default(&self) -> bool {
        let d = ElmerConfig::default();
        self.agent_endpoint == d.agent_endpoint
            && self.agent_model == d.agent_model
            && self.agent_turn_timeout_secs == d.agent_turn_timeout_secs
            && self.onboarded == d.onboarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_1_is_recognized_as_migratable_not_rejected() {
        assert_eq!(super::detect_schema_action(1), super::SchemaAction::MigrateFromV1);
        assert_eq!(super::detect_schema_action(CONFIG_SCHEMA_VERSION), super::SchemaAction::Current);
        assert_eq!(super::detect_schema_action(999), super::SchemaAction::Unsupported { found: 999 });
    }

    // tuxlink-ulrz: a version ≥2 but below current is additively loadable (not
    // unsupported), so the write guard treats it as a legitimate overwrite target
    // and read_config can self-heal it forward.
    #[test]
    fn older_additive_versions_are_migratable_not_unsupported() {
        // current is 5 (tuxlink-8fkkk); 2 is additively loadable.
        assert_eq!(super::detect_schema_action(2), super::SchemaAction::MigrateAdditive);
        // strictly-above-current (a newer build's config) is unsupported → refused on write.
        assert_eq!(
            super::detect_schema_action(CONFIG_SCHEMA_VERSION + 1),
            super::SchemaAction::Unsupported { found: CONFIG_SCHEMA_VERSION + 1 }
        );
    }

    // A valid minimal config body at `schema_version`, with an optional trailing
    // `extra` JSON fragment (e.g. `, "some_future_field": true`). Mirrors the
    // working fixture in position_source_defaults_to_gps_when_absent_from_config.
    fn config_json(schema_version: u32, extra: &str) -> String {
        format!(
            r#"{{
                "schema_version": {schema_version},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }}{extra}
            }}"#
        )
    }

    // tuxlink-ulrz: a v2 config (e.g. one written before trash_auto_purge bumped the
    // version to 3) deserializes — new fields default, schema_version normalizes to
    // current so the next write re-stamps it. This is the forward-compat that the
    // unconditional `v != CURRENT` reject used to break.
    #[test]
    fn v2_config_loads_and_normalizes_to_current() {
        let cfg: Config = serde_json::from_str(&config_json(2, ""))
            .expect("a v2 config must load under the current binary (additive forward-compat)");
        assert_eq!(cfg.schema_version, CONFIG_SCHEMA_VERSION, "marker normalizes to current");
        assert!(cfg.trash_auto_purge, "the field added in v3 defaults to true when absent");
    }

    // tuxlink-ulrz: a config from a NEWER build (schema_version above current) is
    // refused on READ — the downgrade case. The write guard separately refuses to
    // overwrite it.
    #[test]
    fn future_schema_version_is_rejected_on_read() {
        assert!(serde_json::from_str::<Config>(&config_json(CONFIG_SCHEMA_VERSION + 1, "")).is_err());
    }

    // tuxlink-ulrz REGRESSION: the exact failure mode. A config at the CURRENT
    // schema_version but carrying an unknown field (a field a newer build added
    // without bumping the version) must NOT be silently loadable, and the bare
    // schema_version probe must still 'pass' — which is precisely why the write
    // guard now does a full parse before overwriting.
    #[test]
    fn unknown_field_at_current_version_fails_full_parse_but_passes_probe() {
        let json = config_json(CONFIG_SCHEMA_VERSION, r#", "some_future_field": true"#);
        assert!(
            serde_json::from_str::<Config>(&json).is_err(),
            "unknown field must fail deserialize (deny_unknown_fields)"
        );
        let probe: SchemaVersionProbe =
            serde_json::from_str(&json).expect("probe reads only schema_version");
        assert_eq!(detect_schema_action(probe.schema_version), SchemaAction::Current);
    }

    // tuxlink-ulrz DISCIPLINE: lock the ALWAYS-serialized top-level Config field set
    // to CONFIG_SCHEMA_VERSION. Adding/removing such a field (the trash_auto_purge
    // class — a non-Option additive field, the kind that broke compat) changes this
    // set and fails the test, forcing the author to bump CONFIG_SCHEMA_VERSION and
    // update this golden list. (Option fields with skip_serializing_if=None are
    // absent here when None and are forward-compat by construction.)
    #[test]
    fn config_schema_version_tracks_field_set() {
        let cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, ""))
            .expect("minimal config deserializes");
        let value = serde_json::to_value(&cfg).expect("config serializes to JSON");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("config serializes to a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut expected = vec![
            "schema_version", "wizard_completed", "connect", "identity", "privacy",
            "packet", "rig", "telnet_listen", "network_po_favorites",
            "review_inbound_before_download", "aprs",
            "trash_auto_purge", "trash_retention_days",
            "close_to_tray", "close_prompt_seen",
        ];
        expected.sort_unstable();
        assert_eq!(
            keys, expected,
            "Config's always-serialized field set changed without bumping \
             CONFIG_SCHEMA_VERSION (now {}). Bump it (+ add a migration if non-additive) \
             and update this golden set (tuxlink-ulrz).",
            CONFIG_SCHEMA_VERSION
        );
    }

    #[test]
    fn migration_plan_promotes_legacy_callsign_to_single_full_identity() {
        let v1 = LegacyConfigV1 {
            callsign: Some("W1ABC".into()),
            identifier: None,
            grid: Some("CN87ux".into()),
        };
        let plan = IdentityMigration::plan(&v1);
        assert_eq!(plan.full_callsign.as_deref(), Some("W1ABC"));
        // move_inbox records PHASE 4 intent (the per-FULL inbox move lands with the
        // per-FULL read change). The v1->v2 executor itself does NOT move the inbox
        // (tuxlink-ej7a); see `MigrationPlan::execute`.
        assert!(plan.move_inbox, "an existing callsign flags the inbox for Phase 4 per-FULL relocation");
        assert_eq!(plan.per_full_subdir.as_deref(), Some("W1ABC"));
    }

    #[test]
    fn migration_plan_offline_only_config_creates_no_full_identity() {
        let v1 = LegacyConfigV1 { callsign: None, identifier: Some("FIELD-1".into()), grid: None };
        let plan = IdentityMigration::plan(&v1);
        assert!(plan.full_callsign.is_none());
        assert!(!plan.move_inbox, "no callsign => nothing to move; the flat store stays where it is");
    }

    // tuxlink-686: position_source defaults to Gps when the field is absent from an
    // existing (schema_version 1) config. This is the additive-migration test: old
    // config files that predate the field must load without error and resolve Gps.
    #[test]
    fn position_source_defaults_to_gps_when_absent_from_config() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config without position_source should deserialize");
        assert_eq!(
            config.privacy.position_source,
            PositionSource::Gps,
            "missing position_source must default to Gps"
        );
    }

    // tuxlink-3o0: the additive-migration test for `connect.host`. An OLD
    // ConnectConfig JSON (only `connect_to_cms` + `transport`, NO `host` key —
    // the pre-3o0 shape) must deserialize with `host` defaulting to
    // cms-z.winlink.org. `host` is now a KNOWN field, so the struct's
    // `deny_unknown_fields` is satisfied; `#[serde(default = "default_cms_host")]`
    // supplies the value when the key is absent.
    #[test]
    fn connect_host_defaults_to_cms_z_when_absent_from_config() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": true, "transport": "CmsSsl" }},
                "identity": {{ "callsign": "W1TEST", "identifier": null, "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config without connect.host should deserialize");
        assert_eq!(
            config.connect.host, "cms-z.winlink.org",
            "missing connect.host must default to cms-z.winlink.org"
        );
    }

    // tuxlink-dyop Phase 8: an OLD config JSON that predates `map_tile_source`
    // (the pre-dyop shape — no `map_tile_source` key) must deserialize with the
    // field defaulting to `None`. The field is now KNOWN, so `deny_unknown_fields`
    // is satisfied; `#[serde(default)]` supplies `None` when the key is absent.
    #[test]
    fn map_tile_source_defaults_to_none_when_absent_from_config() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config without map_tile_source should deserialize");
        assert!(
            config.map_tile_source.is_none(),
            "missing map_tile_source must default to None"
        );
    }

    // tuxlink-dyop Phase 8: a configured `map_tile_source` round-trips through
    // serialize → deserialize (proves on-disk persistence of an activated source).
    #[test]
    fn map_tile_source_round_trips_when_set() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }},
                "map_tile_source": {{
                    "url": "http://192.168.1.5:8080/tiles/",
                    "scheme": "Xyz",
                    "minZoom": 0,
                    "maxZoom": 16,
                    "cacheBudgetMb": 384,
                    "attribution": null,
                    "label": "shack"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config with map_tile_source should deserialize");
        let src = config.map_tile_source.as_ref().expect("source present");
        assert_eq!(src.url, "http://192.168.1.5:8080/tiles/");
        assert_eq!(src.label, "shack");
        // Round-trip back out and parse again — the serialized form re-parses
        // (camelCase field names match the TileSource serde contract).
        let reser = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&reser).unwrap();
        assert_eq!(back.map_tile_source.unwrap().url, "http://192.168.1.5:8080/tiles/");
    }

    // tuxlink-3o0: a configured host round-trips (proves persistence, not just
    // the default).
    #[test]
    fn connect_host_round_trips_when_set() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": true, "transport": "Telnet", "host": "server.winlink.org" }},
                "identity": {{ "callsign": "W1TEST", "identifier": null, "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.connect.host, "server.winlink.org");
        let reserialized = serde_json::to_string(&config).unwrap();
        let reloaded: Config = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(reloaded.connect.host, "server.winlink.org");
    }

    // tuxlink-bsiy: the opt-in `review_inbound_before_download` preference
    // round-trips through serde when set to true (proves persistence, not just
    // the default).
    #[test]
    fn review_inbound_before_download_round_trips_when_true() {
        let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
        cfg.review_inbound_before_download = true;
        let serialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&serialized).unwrap();
        assert!(
            reloaded.review_inbound_before_download,
            "review_inbound_before_download=true must survive a serialize→deserialize round-trip"
        );
    }

    // tuxlink-pmp5: review-before-download is now the DEFAULT (the WLE emcomm
    // default). An OLD config JSON with NO `review_inbound_before_download` key
    // (every config that predates this field) must deserialize with the field
    // defaulting to TRUE. The field is KNOWN to the struct, so
    // `deny_unknown_fields` stays satisfied; the serde default fn supplies true
    // when the key is absent.
    #[test]
    fn review_inbound_before_download_defaults_true_when_absent_from_config() {
        let json = sample_config_json_without_packet();
        assert!(
            !json.contains("review_inbound_before_download"),
            "fixture must omit the key for this migration test to be meaningful"
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("config without review_inbound_before_download should deserialize");
        assert!(
            cfg.review_inbound_before_download,
            "missing review_inbound_before_download must default to true (review before download)"
        );
    }

    // tuxlink-wl7n: an OLD config JSON with NO trash-auto-purge keys (every
    // config that predates this field) must deserialize with `trash_auto_purge`
    // defaulting to true and `trash_retention_days` to 30. The fields are KNOWN
    // to the struct, so `deny_unknown_fields` stays satisfied; the serde default
    // fns supply the values when the keys are absent.
    #[test]
    fn trash_auto_purge_defaults_when_absent_from_config() {
        let json = sample_config_json_without_packet();
        assert!(
            !json.contains("trash_auto_purge") && !json.contains("trash_retention_days"),
            "fixture must omit the trash keys for this migration test to be meaningful"
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("config without trash-auto-purge keys should deserialize");
        assert!(
            cfg.trash_auto_purge,
            "missing trash_auto_purge must default to true"
        );
        assert_eq!(
            cfg.trash_retention_days, 30,
            "missing trash_retention_days must default to 30"
        );
    }

    // tuxlink-5rvp / #882: an OLD config JSON with NO close-behavior keys (every
    // config that predates the close-to-tray prompt) must deserialize with
    // `close_to_tray` defaulting to true (current minimize-to-tray behavior) and
    // `close_prompt_seen` defaulting to false (so the one-time prompt still
    // shows). The fields are KNOWN to the struct, so `deny_unknown_fields` stays
    // satisfied; the serde defaults supply the values when the keys are absent.
    #[test]
    fn close_behavior_defaults_when_absent_from_config() {
        let json = sample_config_json_without_packet();
        assert!(
            !json.contains("close_to_tray") && !json.contains("close_prompt_seen"),
            "fixture must omit the close-behavior keys for this migration test to be meaningful"
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("config without close-behavior keys should deserialize");
        assert!(
            cfg.close_to_tray,
            "missing close_to_tray must default to true (minimize-to-tray)"
        );
        assert!(
            !cfg.close_prompt_seen,
            "missing close_prompt_seen must default to false (prompt not yet shown)"
        );
    }

    // tuxlink-5rvp / #882: the close-behavior fields must round-trip through
    // serde unchanged (explicit values survive a serialize → deserialize cycle).
    #[test]
    fn close_behavior_round_trips() {
        let json = sample_config_json_without_packet();
        let mut cfg: Config = serde_json::from_str(&json).expect("base config deserializes");
        cfg.close_to_tray = false;
        cfg.close_prompt_seen = true;
        let serialized = serde_json::to_string(&cfg).expect("config serializes");
        let round: Config =
            serde_json::from_str(&serialized).expect("serialized config deserializes");
        assert!(!round.close_to_tray, "close_to_tray must survive the round-trip");
        assert!(round.close_prompt_seen, "close_prompt_seen must survive the round-trip");
    }

    // tuxlink-efo: a packet.link variant THIS build doesn't know (forward/sideways
    // schema skew across concurrent dev builds — the original symptom was a
    // Bluetooth-aware build's config bricking a non-Bluetooth build) must NOT brick
    // app-open. read_config degrades the unparseable link to None; the rest of the
    // config is preserved.
    #[test]
    fn unknown_packet_link_variant_degrades_to_none_not_brick() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }},
                "packet": {{ "ssid": 7, "link": {{ "Telepathy": {{ "mac": "00:11:22" }} }} }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("an unknown packet.link variant must degrade to None, not error the whole read");
        assert_eq!(config.packet.link, None, "the unknown link variant degrades to None");
        assert_eq!(config.packet.ssid, 7, "the rest of the packet section is preserved");
        assert_eq!(
            config.identity.identifier.as_deref(),
            Some("W1TEST"),
            "identity (and the rest of the config) is preserved through the degradation"
        );
    }

    // tuxlink-efo regression guard: a KNOWN link variant still parses to Some — the
    // lenient degradation must not swallow valid links.
    #[test]
    fn known_packet_link_variant_still_parses_to_some() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }},
                "packet": {{ "ssid": 7, "link": {{ "Bluetooth": {{ "mac": "38:D2:00:01:55:5C" }} }} }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.packet.link,
            Some(KissLinkConfig::Bluetooth { mac: "38:D2:00:01:55:5C".into() }),
            "a known link variant must round-trip to Some, not degrade"
        );
    }

    // P5 back-compat: a packet section with NO `link` key at all (an older config,
    // or one an operator never configured a link in) loads cleanly as `link: None` —
    // adding the ManagedDireWolf variant must not change this. Pairs with the
    // unknown-variant degradation test above (which covers a link the build can't read).
    #[test]
    fn packet_section_without_link_loads_as_none() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }},
                "packet": {{ "ssid": 7 }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("a packet section with no link key must load, not error");
        assert_eq!(config.packet.link, None, "absent link key loads as None");
        assert_eq!(config.packet.ssid, 7, "the rest of the packet section is preserved");
    }

    // P5: a `packet.link` carrying a ManagedDireWolf written by THIS build round-trips
    // through serialize → deserialize equal (the known-variant counterpart to the
    // lenient-degradation test for unknown variants). Exercised at the PacketConfig
    // level so it does not depend on a full-Config constructor.
    #[test]
    fn managed_direwolf_link_round_trips_through_packet_config() {
        use crate::winlink::ax25::devices::{PttChoice, StableAudioId, StableIdKind};
        let pc = PacketConfig {
            ssid: 7,
            link: Some(KissLinkConfig::ManagedDireWolf {
                audio_device: StableAudioId {
                    kind: StableIdKind::ByIdSymlink,
                    value: "usb-C-Media_DigiRig_Audio-00".into(),
                },
                ptt: PttChoice::SerialRts { tty: "/dev/ttyUSB0".into() },
            }),
            params: Ax25ParamsConfig::default(),
            listen_default: true,
        };
        let json = serde_json::to_string(&pc).expect("serialize packet config with managed link");
        let back: PacketConfig =
            serde_json::from_str(&json).expect("deserialize packet config with managed link");
        assert_eq!(back.link, pc.link, "managed link round-trips equal through the lenient deser");
    }

    // tuxlink-efo: a tuxlink-specific config-dir override so a per-worktree dev build
    // points at its OWN config and concurrent builds stop contaminating one shared
    // ~/.config/tuxlink/config.json. Takes precedence over XDG_CONFIG_HOME; the dir
    // holds config.json directly. Tested via the pure resolver (no process-global env).
    #[test]
    fn resolve_config_path_prefers_tuxlink_config_dir() {
        assert_eq!(
            resolve_config_path(Some("/tmp/wt-a".into()), Some("/xdg".into()), Some("/home/u".into())),
            std::path::PathBuf::from("/tmp/wt-a/config.json")
        );
    }

    #[test]
    fn resolve_config_path_falls_back_to_xdg_then_home() {
        assert_eq!(
            resolve_config_path(None, Some("/xdg".into()), Some("/home/u".into())),
            std::path::PathBuf::from("/xdg/tuxlink/config.json")
        );
        assert_eq!(
            resolve_config_path(None, None, Some("/home/u".into())),
            std::path::PathBuf::from("/home/u/.config/tuxlink/config.json")
        );
    }

    // tuxlink-882: the privacy boundary. The grid is stored full; what may go on
    // air is reduced to the configured precision — 4 chars by default, 6 on opt-in.
    #[test]
    fn broadcast_grid_default_four_char_reduces_six_char_stored_grid() {
        assert_eq!(broadcast_grid("CN87ux", PositionPrecision::FourCharGrid), "CN87");
    }

    #[test]
    fn broadcast_grid_six_char_optin_keeps_full_precision() {
        assert_eq!(broadcast_grid("CN87ux", PositionPrecision::SixCharGrid), "CN87ux");
    }

    #[test]
    fn broadcast_grid_is_a_noop_when_stored_grid_already_short() {
        // A 4-char stored grid stays 4-char under either setting (nothing to reveal).
        assert_eq!(broadcast_grid("CN87", PositionPrecision::FourCharGrid), "CN87");
        assert_eq!(broadcast_grid("CN87", PositionPrecision::SixCharGrid), "CN87");
    }

    #[test]
    fn broadcast_grid_handles_empty() {
        assert_eq!(broadcast_grid("", PositionPrecision::FourCharGrid), "");
    }

    fn sample_config_json_without_packet() -> String {
        // A v1-shaped config with NO `packet` key — proves the field defaults.
        serde_json::json!({
            "schema_version": CONFIG_SCHEMA_VERSION,
            "wizard_completed": true,
            "connect": { "connect_to_cms": false, "transport": "Telnet" },
            "identity": { "callsign": null, "identifier": "FIELD-1", "grid": "CN87" },
            "privacy": { "gps_state": "Off", "position_precision": "FourCharGrid" },
            "pat_mbo_address": null
        })
        .to_string()
    }

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

    #[test]
    fn config_defaults_packet_section_when_absent() {
        let json = sample_config_json_without_packet();
        let cfg: Config = serde_json::from_str(&json).unwrap();
        let packet = cfg.packet;
        assert_eq!(packet.ssid, 0, "SSID defaults to 0");
        assert!(packet.listen_default, "listen is default-on (spec §4.5)");
        assert!(packet.link.is_none(), "no last KISS link until the operator sets one");
    }

    #[test]
    fn packet_config_round_trips_with_sticky_ssid_and_link() {
        // Persist an SSID + a TCP KISS link + tuned params, reload, assert sticky.
        let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
        cfg.packet = PacketConfig {
            ssid: 7,
            link: Some(KissLinkConfig::Tcp {
                host: "127.0.0.1".into(),
                port: 8001,
            }),
            params: Ax25ParamsConfig { paclen: 128, maxframe: 4, ..Default::default() },
            listen_default: false,
        };
        let serialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reloaded.packet.ssid, 7);
        assert!(!reloaded.packet.listen_default);
        assert_eq!(reloaded.packet.params.paclen, 128);
        match reloaded.packet.link {
            Some(KissLinkConfig::Tcp { host, port }) => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 8001);
            }
            other => panic!("expected a TCP KISS link, got {other:?}"),
        }
    }

    #[test]
    fn packet_ssid_above_15_is_rejected() {
        let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
        cfg.packet.ssid = 16;
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ConfigValidationError::PacketSsidOutOfRange { ssid: 16 }),
            "expected PacketSsidOutOfRange, got {err:?}"
        );
    }

    // --- tuxlink-2y4: AX.25 connect T1 is honored verbatim (uhc floor reverted) ---
    // The uhc RF floor (MIN_RF_T1_MS = 10 s) tripled worst-case airtime and was the
    // wrong lever for the runaway-keying incident; 2y4 reverted it. Runaway airtime is
    // bounded by datalink::connect's ≤2-SABM key limit + connect_timeout cap, NOT by
    // inflating the retransmit timer. into_params now passes T1 through unchanged.

    #[test]
    fn into_params_honors_a_short_t1_verbatim_no_floor() {
        // The historical 3 s auto-default is passed through as-is — NOT floored to 10 s
        // (tuxlink-2y4 reverted the uhc floor).
        let cfg = Ax25ParamsConfig { t1_ms: 3000, ..Ax25ParamsConfig::default() };
        assert_eq!(
            cfg.into_params().t1,
            std::time::Duration::from_millis(3000),
            "T1 must be honored verbatim — the uhc RF floor was reverted (2y4)"
        );
    }

    #[test]
    fn into_params_honors_a_long_configured_t1_verbatim() {
        // A longer configured T1 is the operator's choice — passed through verbatim.
        let cfg = Ax25ParamsConfig { t1_ms: 15_000, ..Ax25ParamsConfig::default() };
        assert_eq!(
            cfg.into_params().t1,
            std::time::Duration::from_millis(15_000),
            "a configured T1 must be honored verbatim"
        );
    }

    #[test]
    fn into_params_sets_the_radio1_connect_timeout_ceiling() {
        // tuxlink-2y4: every runtime params carries the RADIO-1 connect airtime ceiling.
        let cfg = Ax25ParamsConfig::default();
        assert_eq!(
            cfg.into_params().connect_timeout,
            std::time::Duration::from_secs(25),
            "into_params must carry the connect_timeout safety ceiling"
        );
    }

    // --- tuxlink-4ek: ArdopUiConfig persistence tests ---

    #[test]
    fn ardop_ui_config_round_trips_through_json() {
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: Some("/dev/ttyUSB0".into()),
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.binary, "ardopcf");
        assert_eq!(back.cmd_port, 8515);
        assert_eq!(back.ptt_serial_path.as_deref(), Some("/dev/ttyUSB0"));
    }

    // --- tuxlink-wu0k: PttMethod + CAT-PTT field persistence + back-compat ---

    #[test]
    fn ardop_ui_config_round_trips_cat_command_ptt() {
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:CARD=Device,DEV=0".into(),
            playback_device: "plughw:CARD=Device,DEV=0".into(),
            ptt_method: PttMethod::CatCommand,
            ptt_serial_path: None,
            cat_serial_path: Some("/dev/ttyUSB0".into()),
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        // PttMethod serializes snake_case.
        assert!(
            json.contains("\"ptt_method\":\"cat_command\""),
            "ptt_method must serialize as \"cat_command\"; got: {json}"
        );
        // tuxlink-8fkkk: the CAT serial link (cat_serial_path / cat_baud) is now
        // radio-level (Config.rig / RigUiConfig) and only MIGRATION-ONLY on
        // ArdopUiConfig (#[serde(skip_serializing)]), so it must NOT serialize
        // under [modem_ardop]. The serial link's persistence + legacy lift are
        // covered by the RigUiConfig round-trip + migrate_rig_from_legacy_ardop
        // tests. The PTT-method-specific fields (method, key/unkey cmds, bridge
        // port) still round-trip through ArdopUiConfig.
        assert!(
            !json.contains("cat_serial_path"),
            "cat_serial_path must not serialize under [modem_ardop] (it is radio-level now); got: {json}"
        );
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ptt_method, PttMethod::CatCommand);
        assert_eq!(back.cat_key_cmd, "TX1;");
        assert_eq!(back.cat_unkey_cmd, "TX0;");
        assert_eq!(back.cat_bridge_port, 4532);
    }

    #[test]
    fn ardop_ui_config_migrates_legacy_serial_rts_when_ptt_method_absent() {
        // A pre-CAT-PTT config recorded a ptt_serial_path and NO ptt_method:
        // it was using ardopcf RTS PTT, so it must migrate to SerialRts.
        let legacy = r#"{
            "binary": "ardopcf",
            "capture_device": "plughw:1,0",
            "playback_device": "plughw:1,0",
            "ptt_serial_path": "/dev/ttyUSB0",
            "cmd_port": 8515
        }"#;
        let back: ArdopUiConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(
            back.ptt_method,
            PttMethod::SerialRts,
            "legacy config with a ptt_serial_path must migrate to SerialRts"
        );
        assert_eq!(back.ptt_serial_path.as_deref(), Some("/dev/ttyUSB0"));
        // CAT fields fill from their defaults.
        assert_eq!(back.cat_baud, 38400);
        assert_eq!(back.cat_key_cmd, "TX1;");
        assert_eq!(back.cat_unkey_cmd, "TX0;");
        assert_eq!(back.cat_bridge_port, 4532);
    }

    #[test]
    fn ardop_ui_config_migrates_legacy_vox_when_no_ptt_fields() {
        // A pre-CAT-PTT config with neither ptt_method nor ptt_serial_path was
        // VOX. It must migrate to Vox, not SerialRts.
        let legacy = r#"{
            "binary": "ardopcf",
            "capture_device": "plughw:1,0",
            "playback_device": "plughw:1,0",
            "cmd_port": 8515
        }"#;
        let back: ArdopUiConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(
            back.ptt_method,
            PttMethod::Vox,
            "legacy config with no PTT fields must migrate to Vox"
        );
        assert_eq!(back.ptt_serial_path, None);
    }

    #[test]
    fn ardop_ui_config_explicit_ptt_method_overrides_legacy_derivation() {
        // If ptt_method IS present, it wins even when a ptt_serial_path also
        // exists (e.g. operator switched to CAT but the old serial path lingers).
        let cfg = r#"{
            "binary": "ardopcf",
            "capture_device": "plughw:1,0",
            "playback_device": "plughw:1,0",
            "ptt_method": "cat_command",
            "ptt_serial_path": "/dev/ttyUSB0",
            "cat_serial_path": "/dev/ttyUSB0",
            "cmd_port": 8515
        }"#;
        let back: ArdopUiConfig = serde_json::from_str(cfg).unwrap();
        assert_eq!(back.ptt_method, PttMethod::CatCommand);
    }

    #[test]
    fn ptt_method_defaults_to_vox() {
        assert_eq!(PttMethod::default(), PttMethod::Vox);
    }

    // --- tuxlink-j0ij: ArdopUiConfig.bandwidth_hz persistence tests ---

    #[test]
    fn ardop_ui_config_round_trips_with_bandwidth_hz_some() {
        // bandwidth_hz: Some(500) → serializes to {... "bandwidth_hz": 500 ...},
        // deserializes back to Some(500). Round-trip preserves the operator's
        // ARQ-bandwidth preference across config writes (tuxlink-j0ij).
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: Some(500),
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            json.contains("\"bandwidth_hz\":500"),
            "serialized config must contain bandwidth_hz: 500 — got: {json}"
        );
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.bandwidth_hz,
            Some(500),
            "bandwidth_hz must round-trip Some(500) verbatim"
        );
    }

    #[test]
    fn ardop_ui_config_round_trips_with_bandwidth_hz_none() {
        // bandwidth_hz: None → serializes WITHOUT a "bandwidth_hz" key
        // (skip_serializing_if = "Option::is_none"), deserializes back to None
        // (the Default::default for Option). Migration path for pre-j0ij configs.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("bandwidth_hz"),
            "None bandwidth_hz must be omitted from serialized JSON — got: {json}"
        );

        // Also verify a config WITHOUT the field deserializes (None default).
        let no_bw_json = r#"{"binary":"ardopcf","capture_device":"","playback_device":"","cmd_port":8515}"#;
        let back: ArdopUiConfig = serde_json::from_str(no_bw_json).unwrap();
        assert_eq!(
            back.bandwidth_hz, None,
            "pre-j0ij config (no bandwidth_hz key) must deserialize as None"
        );
    }

    // --- Operator smoke 2026-05-31 round 3: webgui_port override + resolution ---

    #[test]
    fn resolved_webgui_port_falls_back_to_cmd_port_minus_one() {
        // Default: webgui_port = None → derive from cmd_port - 1. With the
        // default cmd_port=8515, this yields 8514 — the value the frontend
        // expects and the value the `-G` spawn flag sets. Single helper rules
        // out the drift class that caused round-3's "connection refused."
        let cfg = ArdopUiConfig::default();
        assert_eq!(cfg.resolved_webgui_port(), Some(8514));
    }

    #[test]
    fn resolved_webgui_port_uses_explicit_override_when_set() {
        // Operator pin: explicit webgui_port wins over the derivation. Lets
        // an operator point both ends (spawn + button) at a non-conventional
        // ardopcf build/configuration.
        let cfg = ArdopUiConfig {
            cmd_port: 8515,
            webgui_port: Some(9999),
            ..Default::default()
        };
        assert_eq!(cfg.resolved_webgui_port(), Some(9999));
    }

    #[test]
    fn resolved_webgui_port_returns_none_when_unresolvable() {
        // cmd_port < 2 AND no explicit override → no valid port can be
        // derived; the spawn omits `-G` and the frontend surfaces an error.
        for low in [0u16, 1u16] {
            let cfg = ArdopUiConfig {
                cmd_port: low,
                webgui_port: None,
                ..Default::default()
            };
            assert_eq!(cfg.resolved_webgui_port(), None,
                "cmd_port={low}: must be unresolvable without override");
        }
        // ... but an explicit override still wins even when cmd_port is too low:
        let cfg = ArdopUiConfig {
            cmd_port: 0,
            webgui_port: Some(8514),
            ..Default::default()
        };
        assert_eq!(cfg.resolved_webgui_port(), Some(8514));
    }

    #[test]
    fn ardop_ui_config_round_trips_with_webgui_port_override() {
        // Operator-pinned webgui_port must round-trip cleanly. Mirrors the
        // bandwidth_hz pattern (skip_serializing_if when None).
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: Some(9080),
            listen_ttl_minutes: 0,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            json.contains("\"webgui_port\":9080"),
            "serialized config must contain webgui_port: 9080 — got: {json}"
        );
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.webgui_port, Some(9080));
    }

    #[test]
    fn ardop_ui_config_omits_webgui_port_when_none() {
        let cfg = ArdopUiConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("webgui_port"),
            "None webgui_port must be omitted from serialized JSON — got: {json}"
        );
        // Pre-existing configs that lack the key must deserialize as None
        // (additive migration; consistent with bandwidth_hz).
        let no_wg_json = r#"{"binary":"ardopcf","capture_device":"","playback_device":"","cmd_port":8515}"#;
        let back: ArdopUiConfig = serde_json::from_str(no_wg_json).unwrap();
        assert_eq!(back.webgui_port, None);
    }

    // --- tuxlink-8fkkk: RigUiConfig (radio-level rig control, hoisted from
    // [modem_ardop] so VARA reaches the same rig) ---

    #[test]
    fn rig_ui_config_defaults() {
        let c = RigUiConfig::default();
        assert_eq!(c.rig_hamlib_model, None);
        assert_eq!(c.rigctld_host, "127.0.0.1");
        // C1: 4534, NOT 4532 — avoids colliding with the CAT-PTT bridge bind.
        assert_eq!(c.rigctld_port, 4534);
        assert_eq!(c.rigctld_binary, "rigctld");
        assert!(!c.close_serial_sequencing);
        assert!(!c.live_vfo_poll);
        assert!(!c.qsy_on_fail);
        assert_eq!(c.cat_serial_path, None);
        assert_eq!(c.cat_baud, 38400);
    }

    /// C1 (tuxlink-8fkkk): the rigctld port default MUST differ from the CAT-PTT
    /// bridge port so a tuxlink-spawned rigctld and the bridge do not collide on
    /// the loopback bind.
    #[test]
    fn rig_ui_config_rigctld_port_differs_from_cat_bridge_port() {
        assert_ne!(
            RigUiConfig::default().rigctld_port,
            ArdopUiConfig::default().cat_bridge_port,
            "rigctld_port (4534) must differ from cat_bridge_port (4532) — C1 bind-collision fix"
        );
    }

    #[test]
    fn rig_ui_config_round_trips_json() {
        let c = RigUiConfig {
            rig_hamlib_model: Some(1049),
            close_serial_sequencing: true,
            live_vfo_poll: true,
            qsy_on_fail: true,
            cat_serial_path: Some("/dev/ttyUSB0".into()),
            cat_baud: 19200,
            ..Default::default()
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: RigUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c, "RigUiConfig must round-trip JSON verbatim");
    }

    #[test]
    fn rig_ui_config_data_mode_and_overrides_round_trip() {
        let cfg = RigUiConfig {
            data_mode: "USB-D".to_string(),
            rig_field_overrides: vec!["cat_baud".to_string(), "ptt_method".to_string()],
            ..RigUiConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: RigUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data_mode, "USB-D");
        assert_eq!(back.rig_field_overrides, vec!["cat_baud", "ptt_method"]);
    }

    #[test]
    fn rig_ui_config_defaults_new_fields_when_absent() {
        // A config JSON that predates data_mode / rig_field_overrides fills both
        // from their #[serde(default)]s.
        let back: RigUiConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(back.data_mode, "PKTUSB");
        assert!(back.rig_field_overrides.is_empty());
    }

    /// Migration (tuxlink-8fkkk): a legacy config whose CAT serial link lived
    /// under `[modem_ardop]` and which has no `[rig]` section must lift the
    /// serial path + baud into `Config.rig` at deserialize time.
    #[test]
    fn legacy_modem_ardop_cat_serial_migrates_to_rig() {
        let json = config_json(
            CONFIG_SCHEMA_VERSION,
            r#", "modem_ardop": {
                "binary": "ardopcf",
                "capture_device": "plughw:1,0",
                "playback_device": "plughw:1,0",
                "cmd_port": 8515,
                "cat_serial_path": "/dev/ttyUSB0",
                "cat_baud": 38400
            }"#,
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("legacy config with modem_ardop CAT serial must deserialize");
        assert_eq!(
            cfg.rig.cat_serial_path.as_deref(),
            Some("/dev/ttyUSB0"),
            "legacy [modem_ardop].cat_serial_path must lift into [rig]"
        );
        assert_eq!(cfg.rig.cat_baud, 38400, "legacy cat_baud must lift into [rig]");
    }

    /// Migration must NOT clobber an explicit `[rig]`: when the config already
    /// carries a `[rig].cat_serial_path`, the legacy `[modem_ardop]` value is
    /// ignored.
    #[test]
    fn explicit_rig_section_is_not_overwritten_by_legacy_ardop() {
        let json = config_json(
            CONFIG_SCHEMA_VERSION,
            r#", "rig": { "cat_serial_path": "/dev/ttyACM1", "cat_baud": 57600 },
                "modem_ardop": {
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "cat_serial_path": "/dev/ttyUSB0",
                    "cat_baud": 38400
                }"#,
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("config with explicit [rig] must deserialize");
        assert_eq!(
            cfg.rig.cat_serial_path.as_deref(),
            Some("/dev/ttyACM1"),
            "explicit [rig].cat_serial_path must win over legacy [modem_ardop]"
        );
        assert_eq!(cfg.rig.cat_baud, 57600, "explicit [rig].cat_baud must be preserved");
    }

    /// A re-save persists the CAT serial link ONLY under `[rig]`: because the
    /// `[modem_ardop]` CAT fields are `skip_serializing`, the migrated value
    /// stops appearing under `[modem_ardop]` after the next serialize.
    #[test]
    fn migrated_cat_serial_is_resaved_under_rig_only() {
        let json = config_json(
            CONFIG_SCHEMA_VERSION,
            r#", "modem_ardop": {
                "binary": "ardopcf",
                "capture_device": "plughw:1,0",
                "playback_device": "plughw:1,0",
                "cmd_port": 8515,
                "cat_serial_path": "/dev/ttyUSB0",
                "cat_baud": 38400
            }"#,
        );
        let cfg: Config = serde_json::from_str(&json).expect("legacy config deserializes");
        let reserialized = serde_json::to_string(&cfg).unwrap();
        // The CAT serial link now lives under [rig]; modem_ardop no longer
        // carries cat_serial_path on a re-save (skip_serializing).
        let reloaded: Config = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(reloaded.rig.cat_serial_path.as_deref(), Some("/dev/ttyUSB0"));
        let modem_ardop_str = serde_json::to_string(reloaded.modem_ardop.as_ref().unwrap()).unwrap();
        assert!(
            !modem_ardop_str.contains("cat_serial_path"),
            "re-saved [modem_ardop] must NOT carry cat_serial_path (skip_serializing): {modem_ardop_str}"
        );
    }

    #[test]
    fn config_with_modem_ardop_some_then_none_round_trips() {
        // Build a minimal Config JSON with modem_ardop set, verify it round-trips.
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_ardop": {{
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "bandwidth_hz": 500
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("Config with modem_ardop must deserialize");
        assert!(cfg.modem_ardop.is_some(), "modem_ardop should be Some");
        let ardop = cfg.modem_ardop.as_ref().unwrap();
        assert_eq!(ardop.binary, "ardopcf");
        assert_eq!(ardop.cmd_port, 8515);
        assert!(ardop.ptt_serial_path.is_none(), "absent ptt_serial_path defaults to None");
        assert_eq!(
            ardop.bandwidth_hz,
            Some(500),
            "bandwidth_hz must deserialize when present (tuxlink-j0ij)"
        );

        // Round-trip: serialize and reload.
        let reserialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&reserialized).unwrap();
        assert!(reloaded.modem_ardop.is_some());
        assert_eq!(
            reloaded.modem_ardop.as_ref().unwrap().bandwidth_hz,
            Some(500),
            "bandwidth_hz must survive a serialize→deserialize round-trip"
        );

        // Verify modem_ardop is absent from a config that never had it (migration path).
        let json_no_ardop = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg_no_ardop: Config = serde_json::from_str(&json_no_ardop)
            .expect("old config without modem_ardop must deserialize (migration)");
        assert!(cfg_no_ardop.modem_ardop.is_none(), "modem_ardop must default to None when absent");
    }

    // --- tuxlink-dfmf: VaraUiConfig persistence + migration tests ---

    #[test]
    fn vara_ui_config_defaults_to_localhost_8300_8301() {
        let cfg = VaraUiConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.cmd_port, 8300);
        assert_eq!(cfg.data_port, 8301);
        assert_eq!(cfg.bandwidth_hz, None);
    }

    #[test]
    fn vara_ui_config_round_trips_through_serde() {
        let cfg = VaraUiConfig {
            host: "192.168.1.50".into(),
            cmd_port: 8400,
            data_port: 8401,
            bandwidth_hz: Some(2750),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: VaraUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn vara_ui_config_omits_bandwidth_when_none() {
        let cfg = VaraUiConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("bandwidth_hz"),
            "None bandwidth must NOT appear in serialized output (skip_serializing_if), got: {json}"
        );
    }

    #[test]
    fn config_modem_vara_round_trips_when_some() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_vara": {{
                    "host": "192.168.1.50",
                    "cmd_port": 8400,
                    "data_port": 8401,
                    "bandwidth_hz": 2750
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json).expect("Config with modem_vara must deserialize");
        assert!(cfg.modem_vara.is_some());
        let vara = cfg.modem_vara.as_ref().unwrap();
        assert_eq!(vara.host, "192.168.1.50");
        assert_eq!(vara.cmd_port, 8400);
        assert_eq!(vara.bandwidth_hz, Some(2750));
    }

    #[test]
    fn config_modem_vara_absent_migrates_to_none() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("old config without modem_vara must deserialize (migration)");
        assert!(cfg.modem_vara.is_none(), "modem_vara must default to None when absent");
    }

    // --- tuxlink-6c9y: RelayFavorite persistence + migration tests ---

    // Migration: a Config JSON WITHOUT `network_po_favorites` deserializes
    // to an empty Vec (proves `#[serde(default)]`). Mirrors
    // `config_modem_vara_absent_migrates_to_none` above.
    #[test]
    fn config_network_po_favorites_absent_migrates_to_empty_vec() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("old config without network_po_favorites must deserialize (migration)");
        assert!(
            cfg.network_po_favorites.is_empty(),
            "network_po_favorites must default to empty Vec when absent"
        );
    }

    // Round-trip: a Config with one RelayFavorite serializes and deserializes
    // back equal. Mirrors `vara_ui_config_round_trips_through_serde` above.
    #[test]
    fn relay_favorite_round_trips_through_serde() {
        let fav = RelayFavorite {
            callsign: "W7AUX".into(),
            label: "Home mesh relay".into(),
            host: "192.168.1.100".into(),
            port: 8772,
        };
        let json = serde_json::to_string(&fav).unwrap();
        let back: RelayFavorite = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fav);
    }

    // A full Config round-trip carrying a non-empty network_po_favorites Vec.
    #[test]
    fn config_with_network_po_favorites_round_trips() {
        let fav = RelayFavorite {
            callsign: "W7AUX".into(),
            label: "Test relay".into(),
            host: "relay.local".into(),
            port: 8772,
        };
        let json_in = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "network_po_favorites": [{{
                    "callsign": "W7AUX",
                    "label": "Test relay",
                    "host": "relay.local",
                    "port": 8772
                }}]
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json_in)
            .expect("Config with network_po_favorites must deserialize");
        assert_eq!(cfg.network_po_favorites.len(), 1);
        assert_eq!(cfg.network_po_favorites[0], fav);

        // Round-trip through serialization.
        let serialized = serde_json::to_string(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cfg2.network_po_favorites, cfg.network_po_favorites);
    }

    // tuxlink-7iy2 Phase 2 + tuxlink-ej7a P0 fix: the v1->v2 identity-migration
    // EXECUTOR. A single-callsign v1 config + an existing flat mailbox migrates to
    // one FULL identity, the inbox STAYS FLAT and visible to the production read
    // path (the per-FULL inbox move is Phase 4's job, landed together with the
    // per-FULL read change — see tuxlink-ej7a), existing Sent messages are tagged
    // with the FULL identity in place, the activation secret is provisioned in the
    // keyring, and a second run is a clean no-op.
    #[test]
    fn migrate_single_callsign_config_promotes_one_full_and_keeps_inbox_intact() {
        fn sample_raw_message(subject: &str) -> Vec<u8> {
            crate::winlink::compose::compose_message("N7CPZ", &["W1AW"], &[], subject, "body", 1_716_200_000).to_bytes()
        }
        fn mid_of(raw: &[u8]) -> crate::winlink_backend::MessageId {
            crate::winlink_backend::MessageId(
                crate::winlink::message::Message::from_bytes(raw)
                    .unwrap()
                    .header("Mid")
                    .unwrap()
                    .to_string(),
            )
        }

        let mbox_root = tempfile::TempDir::new().unwrap();
        let store_path = mbox_root.path().join("identities.json");

        // Seed a TRUE legacy flat mailbox by writing the raw b2f directly at the
        // flat <root>/inbox + <root>/sent paths a pre-v2 install actually had.
        // (Post-Phase-4, Mailbox::new().store() namespaces under mailbox/<ns>/, so
        // it can no longer seed the flat layout this v1->v2 migration operates on.)
        let inbox_raw = sample_raw_message("INBOX-1");
        let sent_raw = sample_raw_message("SENT-1");
        let inbox_id = mid_of(&inbox_raw);
        let sent_id = mid_of(&sent_raw);
        std::fs::create_dir_all(mbox_root.path().join("inbox")).unwrap();
        std::fs::write(mbox_root.path().join("inbox").join(format!("{}.b2f", inbox_id.0)), &inbox_raw).unwrap();
        std::fs::create_dir_all(mbox_root.path().join("sent")).unwrap();
        std::fs::write(mbox_root.path().join("sent").join(format!("{}.b2f", sent_id.0)), &sent_raw).unwrap();

        let v1 = LegacyConfigV1 { callsign: Some("W1ABC".into()), identifier: None, grid: Some("CN87".into()) };
        let svc = crate::identity::IdentityService::with_memory_keyring();

        let report = IdentityMigration::plan(&v1)
            .execute(&svc, mbox_root.path(), &store_path, /*has_cms_account=*/true, /*activation_secret=*/Some("cms-pw"))
            .expect("migration must succeed");

        // (a) exactly one FULL identity, last_selected = it.
        let store = crate::identity::IdentityStore::load(&store_path).unwrap();
        assert_eq!(store.full().len(), 1);
        assert_eq!(store.full()[0].callsign.as_str(), "W1ABC");
        assert!(matches!(store.last_selected(), Some(crate::identity::Address::Full(c)) if c.as_str() == "W1ABC"));

        // (b) The v1->v2 config migration does NOT relocate the inbox (tuxlink-ej7a):
        // it leaves the flat <root>/inbox in place. Phase 4's migrate_legacy_layout
        // is the SEPARATE step that moves flat -> mailbox/<FULL>/inbox (covered by
        // native_mailbox::migrate_legacy_flat_layout_to_per_full); this test pins the
        // intermediate state — the message stays at the flat path, untouched.
        assert!(mbox_root.path().join("inbox").join(format!("{}.b2f", inbox_id.0)).exists(),
                "the v1->v2 migration leaves the inbox message at the flat path");
        assert!(!mbox_root.path().join("W1ABC").exists(),
                "the v1->v2 migration must NOT create a per-FULL inbox dir (that is Phase 4)");
        assert!(!mbox_root.path().join("mailbox").exists(),
                "the v1->v2 migration must NOT create the Phase-4 mailbox/ tree");
        assert!(!report.inbox_moved, "migration report records no inbox relocation");

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

    // --- tuxlink-2f2n: AprsConfig defaults + persistence tests ---

    #[test]
    fn aprs_config_defaults() {
        let c = AprsConfig::default();
        assert_eq!(c.source_ssid, 0);
        assert_eq!(c.tocall, "APZTUX");
        assert_eq!(c.path, "WIDE1-1,WIDE2-1");
    }

    #[test]
    fn aprs_config_round_trips_through_json() {
        let c = AprsConfig { source_ssid: 7, tocall: "APZTUX".into(), path: "WIDE2-1".into() };
        let s = serde_json::to_string(&c).unwrap();
        let back: AprsConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }

    // --- tuxlink-wpqwy: ElmerConfig.onboarded sentinel ---

    /// `ElmerConfig::default()` must have `onboarded == false`.
    /// Guards the serde `#[serde(default)]` path: absent-from-disk entries must
    /// deserialize to `false`.
    #[test]
    fn elmer_config_default_onboarded_is_false() {
        assert!(!ElmerConfig::default().onboarded);
    }

    /// An `ElmerConfig` JSON blob that omits the `onboarded` field (pre-wpqwy
    /// on-disk format) must deserialize to `onboarded == false`.
    #[test]
    fn elmer_config_missing_onboarded_deserializes_to_false() {
        let json = r#"{
            "agent_endpoint": "http://127.0.0.1:11434/v1/chat/completions",
            "agent_model": "llama3",
            "agent_turn_timeout_secs": 900
        }"#;
        let cfg: ElmerConfig = serde_json::from_str(json).expect("deserialize");
        assert!(!cfg.onboarded, "absent `onboarded` must default to false");
    }

    /// `ElmerConfig::is_default()` must return `false` when `onboarded` is `true`
    /// even if all content fields are at their factory defaults.  This is the
    /// tuxlink-wpqwy persistence fix: when a user saves with default content (e.g.
    /// loopback Ollama), `config_set_inner` sets `onboarded = true`; `is_default()`
    /// must return `false` so that `skip_serializing_if` does NOT omit the `elmer`
    /// section, ensuring `onboarded: true` reaches disk and survives the next launch.
    #[test]
    fn elmer_config_is_default_reacts_to_onboarded_flag() {
        let cfg = ElmerConfig {
            onboarded: true,
            ..ElmerConfig::default()
        };
        assert!(
            !cfg.is_default(),
            "is_default() must return false when onboarded=true (even with default \
             content fields) so that skip_serializing_if persists the elmer section"
        );
    }

    /// A completely-default `ElmerConfig` (content AND `onboarded == false`) IS
    /// `is_default()`.  Guards the never-touched first-run path: the `elmer`
    /// section must be omitted from the config file until the operator saves.
    #[test]
    fn elmer_config_fully_default_is_default() {
        let cfg = ElmerConfig::default();
        assert!(
            cfg.is_default(),
            "ElmerConfig with all factory defaults (including onboarded=false) must \
             be is_default() so the elmer section is omitted from a fresh config file"
        );
    }

    /// Persistence fix round-trip: serializing a `Config` whose `elmer` section has
    /// default content but `onboarded = true` must INCLUDE the `elmer` key (not skip
    /// it).  The outer `Config` field uses
    /// `#[serde(default, skip_serializing_if = "ElmerConfig::is_default")]`; with
    /// `onboarded` now counted by `is_default()`, the section is not omitted and the
    /// flag survives a serialize→deserialize cycle.
    #[test]
    fn elmer_config_onboarded_true_with_default_content_persists() {
        // A user who saves with loopback Ollama defaults: content stays default but
        // onboarded is flipped to true.
        let cfg = ElmerConfig {
            onboarded: true,
            ..ElmerConfig::default()
        };
        // is_default() must be false so skip_serializing_if does not drop the section.
        assert!(!cfg.is_default(), "onboarded=true must make is_default() false");

        // Round-trip the containing Config struct to confirm the `elmer` key appears.
        // We only need to wrap in a minimal JSON object; we can serialize ElmerConfig
        // itself here since the skip_serializing_if attr is on the parent Config field.
        // The important property is already captured by !is_default() above — that is
        // what skip_serializing_if evaluates.  Additionally verify that a fresh
        // deserialization of the serialized ElmerConfig recovers onboarded=true.
        let json = serde_json::to_string(&cfg).expect("serialize ElmerConfig");
        let back: ElmerConfig = serde_json::from_str(&json).expect("deserialize ElmerConfig");
        assert!(back.onboarded, "onboarded=true must survive a serde round-trip");
    }

    /// A config with a non-default endpoint must NOT be `is_default()` regardless
    /// of the `onboarded` flag value.
    #[test]
    fn elmer_config_is_default_false_when_content_differs() {
        let cfg = ElmerConfig {
            agent_endpoint: "https://api.openai.com/v1/chat/completions".into(),
            ..ElmerConfig::default()
        };
        assert!(!cfg.is_default(), "non-default endpoint must make is_default() return false");
    }
}
