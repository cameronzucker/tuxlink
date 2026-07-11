//! User-owned uninstall cleanup flow for Tuxlink data.
//!
//! Linux package removal intentionally keeps user data. This module provides an
//! explicit operator-run cleanup path (`tuxlink cleanup`) that can preview and
//! remove known Tuxlink paths from the current user's XDG directories.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const APP_ID: &str = "com.tuxlink.app";
const LEGACY_APP_NAME: &str = "tuxlink";
const KEYRING_SERVICE: &str = "tuxlink";
const LEGACY_KEYRING_SERVICE: &str = "tuxlink-pat";
const LISTENER_PASSWORD_ACCOUNT: &str = "p2p-listener:station-password";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    Keep,
    Transient,
    Full,
}

impl CleanupMode {
    fn label(self) -> &'static str {
        match self {
            CleanupMode::Keep => "keep user data",
            CleanupMode::Transient => "remove transient state/cache/logs only",
            CleanupMode::Full => "remove all Tuxlink operator data",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupTargetKind {
    Config,
    MailboxAndData,
    Transient,
    DesktopIntegration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CleanupTarget {
    pub path: PathBuf,
    pub kind: CleanupTargetKind,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyringTarget {
    pub service: String,
    pub account: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupPlan {
    pub mode: CleanupMode,
    pub targets: Vec<CleanupTarget>,
    pub keyring_targets: Vec<KeyringTarget>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RemovalOutcome {
    WouldRemove,
    Removed,
    Missing,
    Error(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct PathRemoval {
    pub path: PathBuf,
    pub outcome: RemovalOutcome,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyringRemoval {
    pub service: String,
    pub account: String,
    pub outcome: RemovalOutcome,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupReport {
    pub mode: CleanupMode,
    pub dry_run: bool,
    pub paths: Vec<PathRemoval>,
    pub keyring: Vec<KeyringRemoval>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupEnv {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    pub state_home: PathBuf,
    pub cache_home: PathBuf,
    pub custom_config_dir: Option<PathBuf>,
}

impl CleanupEnv {
    pub fn from_process() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                "HOME is not set; cannot resolve user-owned cleanup paths".to_string()
            })?;
        Ok(Self::from_parts(
            home,
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("XDG_DATA_HOME"),
            std::env::var_os("XDG_STATE_HOME"),
            std::env::var_os("XDG_CACHE_HOME"),
            std::env::var_os("TUXLINK_CONFIG_DIR"),
        ))
    }

    pub fn from_parts(
        home: PathBuf,
        xdg_config_home: Option<OsString>,
        xdg_data_home: Option<OsString>,
        xdg_state_home: Option<OsString>,
        xdg_cache_home: Option<OsString>,
        custom_config_dir: Option<OsString>,
    ) -> Self {
        let config_home = xdg_dir_or_default(xdg_config_home, &home, &[".config"]);
        let data_home = xdg_dir_or_default(xdg_data_home, &home, &[".local", "share"]);
        let state_home = xdg_dir_or_default(xdg_state_home, &home, &[".local", "state"]);
        let cache_home = xdg_dir_or_default(xdg_cache_home, &home, &[".cache"]);
        let custom_config_dir = custom_config_dir
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Self {
            home,
            config_home,
            data_home,
            state_home,
            cache_home,
            custom_config_dir,
        }
    }

    fn current_config_dir(&self) -> PathBuf {
        self.custom_config_dir
            .clone()
            .unwrap_or_else(|| self.config_home.join(LEGACY_APP_NAME))
    }

    fn legacy_config_dir(&self) -> PathBuf {
        self.config_home.join(LEGACY_APP_NAME)
    }

    fn tauri_config_dir(&self) -> PathBuf {
        self.config_home.join(APP_ID)
    }

    fn app_data_dir(&self) -> PathBuf {
        self.data_home.join(APP_ID)
    }

    fn legacy_data_dir(&self) -> PathBuf {
        self.data_home.join(LEGACY_APP_NAME)
    }

    fn state_dir(&self) -> PathBuf {
        self.state_home.join(LEGACY_APP_NAME)
    }
}

fn xdg_dir_or_default(value: Option<OsString>, home: &Path, suffix: &[&str]) -> PathBuf {
    if let Some(value) = value {
        if !value.is_empty() {
            let p = PathBuf::from(value);
            if p.is_absolute() {
                return p;
            }
        }
    }
    let mut p = home.to_path_buf();
    for part in suffix {
        p.push(part);
    }
    p
}

pub trait KeyringDeleter {
    /// Returns `Ok(true)` when a credential was removed, `Ok(false)` when it was absent.
    fn delete(&self, service: &str, account: &str) -> Result<bool, String>;
}

pub struct RealKeyringDeleter;

impl KeyringDeleter for RealKeyringDeleter {
    fn delete(&self, service: &str, account: &str) -> Result<bool, String> {
        let entry = keyring::Entry::new(service, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub fn build_plan(mode: CleanupMode, env: &CleanupEnv) -> CleanupPlan {
    let mut warnings = Vec::new();
    let mut targets = match mode {
        CleanupMode::Keep => Vec::new(),
        CleanupMode::Transient => transient_targets(env),
        CleanupMode::Full => {
            let mut t = full_targets(env, &mut warnings);
            t.extend(transient_targets(env));
            t
        }
    };
    dedupe_targets(&mut targets);

    let keyring_targets = if mode == CleanupMode::Full {
        keyring_targets(env, &mut warnings)
    } else {
        Vec::new()
    };

    CleanupPlan {
        mode,
        targets,
        keyring_targets,
        warnings,
    }
}

fn target<P: Into<PathBuf>>(
    path: P,
    kind: CleanupTargetKind,
    description: impl Into<String>,
) -> CleanupTarget {
    CleanupTarget {
        path: path.into(),
        kind,
        description: description.into(),
    }
}

fn transient_targets(env: &CleanupEnv) -> Vec<CleanupTarget> {
    let app_config = env.tauri_config_dir();
    let app_data = env.app_data_dir();
    let state = env.state_dir();
    vec![
        target(
            app_config.join(".window-state.json"),
            CleanupTargetKind::Transient,
            "saved window geometry",
        ),
        target(
            app_data.join("WebKitCache"),
            CleanupTargetKind::Transient,
            "WebKit disk cache",
        ),
        target(
            app_data.join("CacheStorage"),
            CleanupTargetKind::Transient,
            "webview cache storage",
        ),
        target(
            app_data.join("localstorage"),
            CleanupTargetKind::Transient,
            "webview local storage",
        ),
        target(
            app_data.join("hsts-storage.sqlite"),
            CleanupTargetKind::Transient,
            "webview HSTS cache",
        ),
        target(
            app_data.join("mediakeys"),
            CleanupTargetKind::Transient,
            "webview media-key cache",
        ),
        target(
            app_data.join("storage"),
            CleanupTargetKind::Transient,
            "webview storage cache",
        ),
        target(
            app_data.join("GPUCache"),
            CleanupTargetKind::Transient,
            "webview GPU cache",
        ),
        target(
            app_data.join("Service Worker"),
            CleanupTargetKind::Transient,
            "webview service-worker cache",
        ),
        target(
            app_data.join("tile-cache"),
            CleanupTargetKind::Transient,
            "map tile cache",
        ),
        target(
            state.join("logs"),
            CleanupTargetKind::Transient,
            "structured diagnostic logs",
        ),
        target(
            state.join("pat.pid"),
            CleanupTargetKind::Transient,
            "stale legacy Pat pid file",
        ),
    ]
}

fn full_targets(env: &CleanupEnv, warnings: &mut Vec<String>) -> Vec<CleanupTarget> {
    let mut targets = vec![
        target(
            env.current_config_dir(),
            CleanupTargetKind::Config,
            "current Tuxlink configuration",
        ),
        target(
            env.legacy_config_dir(),
            CleanupTargetKind::Config,
            "legacy Tuxlink configuration",
        ),
        target(
            env.tauri_config_dir(),
            CleanupTargetKind::Config,
            "Tauri app configuration and window state",
        ),
        target(
            env.app_data_dir(),
            CleanupTargetKind::MailboxAndData,
            "mailbox, contacts, stations, forms, webview data",
        ),
        target(
            env.legacy_data_dir(),
            CleanupTargetKind::MailboxAndData,
            "legacy Tuxlink data",
        ),
        target(
            env.state_dir(),
            CleanupTargetKind::Transient,
            "logs and process state",
        ),
        target(
            env.data_home.join("applications").join("tuxlink.desktop"),
            CleanupTargetKind::DesktopIntegration,
            "legacy user desktop entry",
        ),
        target(
            env.data_home
                .join("applications")
                .join(format!("{APP_ID}.desktop")),
            CleanupTargetKind::DesktopIntegration,
            "user desktop entry",
        ),
    ];
    targets.extend(desktop_icon_targets(env));
    targets.retain(|t| {
        if path_is_too_broad(&t.path, env) {
            warnings.push(format!(
                "Skipped unsafe cleanup target {}; refusing to delete a home/XDG root.",
                t.path.display()
            ));
            false
        } else {
            true
        }
    });
    targets
}

fn desktop_icon_targets(env: &CleanupEnv) -> Vec<CleanupTarget> {
    let mut out = Vec::new();
    for size in [
        "16x16",
        "24x24",
        "32x32",
        "48x48",
        "64x64",
        "128x128",
        "128x128@2x",
        "256x256",
        "512x512",
        "scalable",
    ] {
        for stem in [LEGACY_APP_NAME, APP_ID] {
            let ext = if size == "scalable" { "svg" } else { "png" };
            out.push(target(
                env.data_home
                    .join("icons")
                    .join("hicolor")
                    .join(size)
                    .join("apps")
                    .join(format!("{stem}.{ext}")),
                CleanupTargetKind::DesktopIntegration,
                "user icon cache entry",
            ));
        }
    }

    let hicolor = env.data_home.join("icons").join("hicolor");
    scan_matching_icons(&hicolor, &mut out);
    out
}

fn scan_matching_icons(root: &Path, out: &mut Vec<CleanupTarget>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            scan_matching_icons(&path, out);
        } else if ft.is_file() || ft.is_symlink() {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if matches!(
                name,
                "tuxlink.png" | "tuxlink.svg" | "com.tuxlink.app.png" | "com.tuxlink.app.svg"
            ) {
                out.push(target(
                    path,
                    CleanupTargetKind::DesktopIntegration,
                    "user icon cache entry",
                ));
            }
        }
    }
}

fn path_is_too_broad(path: &Path, env: &CleanupEnv) -> bool {
    path == env.home
        || path == env.config_home
        || path == env.data_home
        || path == env.state_home
        || path == env.cache_home
        || path == Path::new("/")
}

fn dedupe_targets(targets: &mut Vec<CleanupTarget>) {
    let mut seen = BTreeSet::new();
    targets.retain(|t| seen.insert(t.path.clone()));
}

fn keyring_targets(env: &CleanupEnv, warnings: &mut Vec<String>) -> Vec<KeyringTarget> {
    let mut identity_callsigns = discover_identity_callsigns(env);
    let mut peer_callsigns = discover_peer_callsigns(env);
    let mut out = Vec::new();

    for callsign in identity_callsigns.split_off(0) {
        out.push(KeyringTarget {
            service: KEYRING_SERVICE.into(),
            account: callsign.clone(),
            description: "Winlink CMS password for configured callsign".into(),
        });
        out.push(KeyringTarget {
            service: LEGACY_KEYRING_SERVICE.into(),
            account: callsign,
            description: "legacy Pat-era Winlink CMS password".into(),
        });
    }

    for callsign in peer_callsigns.split_off(0) {
        out.push(KeyringTarget {
            service: KEYRING_SERVICE.into(),
            account: format!("p2p-peer:{callsign}"),
            description: "P2P peer station password".into(),
        });
    }

    for (owner_id, endpoint_id) in discover_peer_endpoint_ids(env) {
        out.push(KeyringTarget {
            service: KEYRING_SERVICE.into(),
            account: format!("p2p-endpoint:{owner_id}:{endpoint_id}"),
            description: "P2P endpoint password".into(),
        });
    }

    out.push(KeyringTarget {
        service: KEYRING_SERVICE.into(),
        account: LISTENER_PASSWORD_ACCOUNT.into(),
        description: "P2P listener station password".into(),
    });

    dedupe_keyring_targets(&mut out);
    warnings.push(
        "Secret Service credentials cannot be enumerated service-wide by Tuxlink. \
         Full cleanup deletes known accounts discovered from Tuxlink config/listener files \
         plus the fixed listener password; inspect service 'tuxlink' and legacy service \
         'tuxlink-pat' manually for any credentials tied to callsigns no longer present on disk."
            .into(),
    );
    out
}

fn dedupe_keyring_targets(targets: &mut Vec<KeyringTarget>) {
    let mut seen = BTreeSet::new();
    targets.retain(|t| seen.insert((t.service.clone(), t.account.clone())));
}

fn discover_identity_callsigns(env: &CleanupEnv) -> Vec<String> {
    let mut out = BTreeSet::new();
    for path in identity_config_candidates(env) {
        if let Some(value) = read_json_value(&path) {
            collect_identity_callsigns(&value, &mut out);
        }
    }
    out.into_iter().collect()
}

fn identity_config_candidates(env: &CleanupEnv) -> Vec<PathBuf> {
    let mut paths = vec![
        env.current_config_dir().join("config.json"),
        env.legacy_config_dir().join("config.json"),
        env.tauri_config_dir().join("config.json"),
        env.tauri_config_dir().join("pat").join("config.json"),
    ];
    paths.sort();
    paths.dedup();
    paths
}

fn read_json_value(path: &Path) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn collect_identity_callsigns(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    if let Some(callsign) = value
        .get("identity")
        .and_then(|v| v.get("callsign"))
        .and_then(|v| v.as_str())
        .and_then(normalize_exact_callsign)
    {
        out.insert(callsign);
    }
    for key in ["mycall", "my_call", "callsign"] {
        if let Some(callsign) = value
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(normalize_exact_callsign)
        {
            out.insert(callsign);
        }
    }
}

fn discover_peer_callsigns(env: &CleanupEnv) -> Vec<String> {
    let mut out = BTreeSet::new();
    let config_dirs = [
        env.current_config_dir(),
        env.legacy_config_dir(),
        env.tauri_config_dir(),
    ];
    for dir in config_dirs {
        for transport in ["packet", "ardop", "vara", "telnet"] {
            let path = dir
                .join("listener")
                .join(transport)
                .join("allowed_stations.json");
            collect_callsigns_from_allowed_stations(&path, &mut out);
        }
    }
    for path in [
        env.app_data_dir().join("stations.json"),
        env.legacy_data_dir().join("stations.json"),
    ] {
        if let Some(value) = read_json_value(&path) {
            collect_keyed_callsigns(&value, &mut out);
        }
    }
    out.into_iter().collect()
}

/// Discover `(owner_id, endpoint_id)` pairs for `p2p-endpoint:*` keyring
/// accounts so a P2P endpoint secret is never orphaned by a Full cleanup
/// [R5-5]. Two sources:
///
/// - `contacts.json` (schema v2 contacts-superset, spec §AMENDMENT): the
///   LIVE store — endpoint secrets are keyed
///   `p2p-endpoint:<contact_id>:<endpoint_id>`.
/// - `peers.json`: a LEGACY artifact — dev builds of the pre-pivot peer
///   store may have written it, and sweeping a maybe-present stale file is
///   correct uninstall behavior.
///
/// Deliberately parses raw JSON rather than importing the contacts model —
/// this module has no dependency on other crate modules (same discipline as
/// `collect_keyed_callsigns` above), so an unrelated model refactor cannot
/// silently break cleanup enumeration.
fn discover_peer_endpoint_ids(env: &CleanupEnv) -> Vec<(String, String)> {
    let mut out = BTreeSet::new();
    for path in [
        env.app_data_dir().join("contacts.json"),
        env.legacy_data_dir().join("contacts.json"),
        env.app_data_dir().join("peers.json"),
        env.legacy_data_dir().join("peers.json"),
    ] {
        if let Some(value) = read_json_value(&path) {
            collect_peer_endpoint_ids(&value, &mut out);
        }
    }
    out.into_iter().collect()
}

fn collect_peer_endpoint_ids(value: &serde_json::Value, out: &mut BTreeSet<(String, String)>) {
    // "contacts" (live, schema v2) or "peers" (legacy artifact) — same
    // per-record shape either way: { id, endpoints: [{ id, .. }] }.
    for key in ["contacts", "peers"] {
        let Some(records) = value.get(key).and_then(|v| v.as_array()) else {
            continue;
        };
        for record in records {
            let Some(owner_id) = record.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(endpoints) = record.get("endpoints").and_then(|v| v.as_array()) else {
                continue;
            };
            for endpoint in endpoints {
                if let Some(endpoint_id) = endpoint.get("id").and_then(|v| v.as_str()) {
                    out.insert((owner_id.to_string(), endpoint_id.to_string()));
                }
            }
        }
    }
}

fn collect_callsigns_from_allowed_stations(path: &Path, out: &mut BTreeSet<String>) {
    let Some(value) = read_json_value(path) else {
        return;
    };
    let Some(callsigns) = value.get("callsigns").and_then(|v| v.as_array()) else {
        return;
    };
    for callsign in callsigns {
        if let Some(callsign) = callsign.as_str().and_then(normalize_exact_callsign) {
            out.insert(callsign);
        }
    }
}

fn collect_keyed_callsigns(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "callsign" | "callSign" | "stationCallsign") {
                    if let Some(callsign) = value.as_str().and_then(normalize_exact_callsign) {
                        out.insert(callsign);
                    }
                }
                collect_keyed_callsigns(value, out);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_keyed_callsigns(value, out);
            }
        }
        _ => {}
    }
}

fn normalize_exact_callsign(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains('*') {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if upper.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        Some(upper)
    } else {
        None
    }
}

pub fn execute_plan(
    plan: &CleanupPlan,
    dry_run: bool,
    keyring: &dyn KeyringDeleter,
) -> CleanupReport {
    let paths = plan
        .targets
        .iter()
        .map(|target| {
            let outcome = if dry_run {
                if target.path.exists() {
                    RemovalOutcome::WouldRemove
                } else {
                    RemovalOutcome::Missing
                }
            } else {
                remove_path(&target.path)
            };
            PathRemoval {
                path: target.path.clone(),
                outcome,
            }
        })
        .collect();

    let keyring = plan
        .keyring_targets
        .iter()
        .map(|target| {
            let outcome = if dry_run {
                RemovalOutcome::WouldRemove
            } else {
                match keyring.delete(&target.service, &target.account) {
                    Ok(true) => RemovalOutcome::Removed,
                    Ok(false) => RemovalOutcome::Missing,
                    Err(e) => RemovalOutcome::Error(e),
                }
            };
            KeyringRemoval {
                service: target.service.clone(),
                account: target.account.clone(),
                outcome,
            }
        })
        .collect();

    CleanupReport {
        mode: plan.mode,
        dry_run,
        paths,
        keyring,
        warnings: plan.warnings.clone(),
    }
}

pub fn preview_current_user_cleanup(mode: CleanupMode) -> Result<CleanupReport, String> {
    let env = CleanupEnv::from_process()?;
    let plan = build_plan(mode, &env);
    Ok(execute_plan(&plan, true, &NoopKeyringDeleter))
}

pub fn execute_current_user_cleanup(mode: CleanupMode) -> Result<CleanupReport, String> {
    let env = CleanupEnv::from_process()?;
    let plan = build_plan(mode, &env);
    Ok(execute_plan(&plan, false, &RealKeyringDeleter))
}

fn remove_path(path: &Path) -> RemovalOutcome {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return RemovalOutcome::Missing,
        Err(e) => return RemovalOutcome::Error(e.to_string()),
    };
    let result = if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Ok(()) => RemovalOutcome::Removed,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => RemovalOutcome::Missing,
        Err(e) => RemovalOutcome::Error(e.to_string()),
    }
}

pub fn handle_cli<I>(args: I) -> Option<i32>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    let subcommand = args.next()?;
    let subcommand = subcommand.to_string_lossy();
    if subcommand != "cleanup" && subcommand != "uninstall-cleanup" {
        return None;
    }
    Some(run_cleanup_cli(args.collect()))
}

#[derive(Debug, Default)]
struct CliOptions {
    mode: Option<CleanupMode>,
    dry_run: bool,
    yes: bool,
    json: bool,
    help: bool,
}

fn run_cleanup_cli(args: Vec<OsString>) -> i32 {
    let opts = match parse_cli_options(args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("{e}");
            print_cleanup_usage();
            return 2;
        }
    };
    if opts.help {
        print_cleanup_usage();
        return 0;
    }
    let env = match CleanupEnv::from_process() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let mode = match opts.mode {
        Some(mode) => mode,
        None if opts.dry_run => CleanupMode::Full,
        None => match prompt_for_mode() {
            Ok(mode) => mode,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        },
    };
    let plan = build_plan(mode, &env);
    if opts.json {
        print_json_plan_or_report(&plan, opts.dry_run);
    } else {
        print_plan(&plan, opts.dry_run);
    }

    if mode == CleanupMode::Keep {
        println!("Keeping Tuxlink user data.");
        return 0;
    }
    if opts.dry_run {
        return 0;
    }
    if !opts.yes && !confirm_destructive(mode) {
        eprintln!("Cleanup cancelled.");
        return 1;
    }

    let report = execute_plan(&plan, false, &RealKeyringDeleter);
    if opts.json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("failed to serialize cleanup report: {e}"),
        }
    } else {
        print_report(&report);
    }
    if report_has_errors(&report) {
        1
    } else {
        0
    }
}

fn parse_cli_options(args: Vec<OsString>) -> Result<CliOptions, String> {
    let mut opts = CliOptions::default();
    for arg in args {
        let arg = arg.to_string_lossy();
        match arg.as_ref() {
            "--help" | "-h" => opts.help = true,
            "--dry-run" => opts.dry_run = true,
            "--yes" | "-y" => opts.yes = true,
            "--json" => opts.json = true,
            "--keep" => set_mode(&mut opts, CleanupMode::Keep)?,
            "--transient" => set_mode(&mut opts, CleanupMode::Transient)?,
            "--all" => set_mode(&mut opts, CleanupMode::Full)?,
            other => return Err(format!("unknown cleanup option: {other}")),
        }
    }
    Ok(opts)
}

fn set_mode(opts: &mut CliOptions, mode: CleanupMode) -> Result<(), String> {
    if opts.mode.replace(mode).is_some() {
        return Err("choose only one cleanup mode".into());
    }
    Ok(())
}

fn print_cleanup_usage() {
    println!(
        "Usage: tuxlink cleanup [--keep | --transient | --all] [--dry-run] [--yes] [--json]\n\
\n\
Modes:\n\
  --keep       Keep all user data. This is the default package-uninstall behavior.\n\
  --transient  Remove cache, webview storage, logs, window state, and stale pid files.\n\
  --all        Remove Tuxlink config, mailbox/messages, contacts, stations, logs, cache,\n\
               user-local launcher leftovers, and known keyring entries.\n\
\n\
Without a mode, tuxlink cleanup prompts for one. --dry-run without a mode previews --all."
    );
}

fn prompt_for_mode() -> Result<CleanupMode, String> {
    println!("Tuxlink uninstall cleanup");
    println!("1. Keep user data (normal package uninstall behavior)");
    println!("2. Remove transient/cache/log data only");
    println!("3. Remove all Tuxlink operator data");
    print!("Select 1, 2, or 3: ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    match line.trim() {
        "1" => Ok(CleanupMode::Keep),
        "2" => Ok(CleanupMode::Transient),
        "3" => Ok(CleanupMode::Full),
        _ => Err("invalid cleanup selection".into()),
    }
}

fn confirm_destructive(mode: CleanupMode) -> bool {
    match mode {
        CleanupMode::Keep => true,
        CleanupMode::Transient => {
            print!("Remove transient Tuxlink state for this user? Type yes to proceed: ");
            let _ = io::stdout().flush();
            let mut line = String::new();
            io::stdin().read_line(&mut line).is_ok() && line.trim() == "yes"
        }
        CleanupMode::Full => {
            print!("This removes Tuxlink messages, settings, contacts, stations, logs, cache, and known keyring entries for this user. Type DELETE to proceed: ");
            let _ = io::stdout().flush();
            let mut line = String::new();
            io::stdin().read_line(&mut line).is_ok() && line.trim() == "DELETE"
        }
    }
}

fn print_json_plan_or_report(plan: &CleanupPlan, dry_run: bool) {
    let report = execute_plan(plan, true, &NoopKeyringDeleter);
    if dry_run {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("failed to serialize cleanup dry-run: {e}"),
        }
    } else {
        match serde_json::to_string_pretty(plan) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("failed to serialize cleanup plan: {e}"),
        }
    }
}

fn print_plan(plan: &CleanupPlan, dry_run: bool) {
    let verb = if dry_run { "Would run" } else { "Selected" };
    println!("{verb}: {}", plan.mode.label());
    if plan.targets.is_empty() && plan.keyring_targets.is_empty() {
        println!("No Tuxlink data will be removed.");
    } else {
        if !plan.targets.is_empty() {
            println!("\nPaths:");
            for target in &plan.targets {
                let exists = if target.path.exists() {
                    "exists"
                } else {
                    "missing"
                };
                println!(
                    "  - [{}] {} ({})",
                    exists,
                    target.path.display(),
                    target.description
                );
            }
        }
        if !plan.keyring_targets.is_empty() {
            println!("\nKeyring entries:");
            for target in &plan.keyring_targets {
                println!(
                    "  - service={} account={} ({})",
                    target.service, target.account, target.description
                );
            }
        }
    }
    print_warnings(&plan.warnings);
}

fn print_report(report: &CleanupReport) {
    println!("Cleanup complete: {}", report.mode.label());
    for item in &report.paths {
        println!(
            "  - {}: {}",
            outcome_label(&item.outcome),
            item.path.display()
        );
    }
    for item in &report.keyring {
        println!(
            "  - {}: keyring service={} account={}",
            outcome_label(&item.outcome),
            item.service,
            item.account
        );
    }
    print_warnings(&report.warnings);
}

fn print_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    println!("\nWarnings:");
    for warning in warnings {
        println!("  - {warning}");
    }
}

fn outcome_label(outcome: &RemovalOutcome) -> &'static str {
    match outcome {
        RemovalOutcome::Removed => "removed",
        RemovalOutcome::WouldRemove => "would remove",
        RemovalOutcome::Missing => "missing",
        RemovalOutcome::Error(_) => "error",
    }
}

fn report_has_errors(report: &CleanupReport) -> bool {
    report
        .paths
        .iter()
        .any(|p| matches!(p.outcome, RemovalOutcome::Error(_)))
        || report
            .keyring
            .iter()
            .any(|p| matches!(p.outcome, RemovalOutcome::Error(_)))
}

struct NoopKeyringDeleter;

impl KeyringDeleter for NoopKeyringDeleter {
    fn delete(&self, _service: &str, _account: &str) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use tempfile::tempdir;

    fn test_env(root: &Path) -> CleanupEnv {
        CleanupEnv::from_parts(
            root.join("home"),
            Some(OsString::from(root.join("config"))),
            Some(OsString::from(root.join("data"))),
            Some(OsString::from(root.join("state"))),
            Some(OsString::from(root.join("cache"))),
            None,
        )
    }

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn keep_plan_has_no_targets_and_does_not_delete_data() {
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        let config = env.legacy_config_dir().join("config.json");
        write(&config, "{}");

        let plan = build_plan(CleanupMode::Keep, &env);
        let report = execute_plan(&plan, false, &NoopKeyringDeleter);

        assert!(plan.targets.is_empty());
        assert!(report.paths.is_empty());
        assert!(config.exists(), "keep mode must not remove user data");
    }

    #[test]
    fn transient_cleanup_removes_cache_logs_and_window_state_but_keeps_mailbox() {
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        let window_state = env.tauri_config_dir().join(".window-state.json");
        let cache = env.app_data_dir().join("WebKitCache").join("index");
        let mailbox = env
            .app_data_dir()
            .join("native-mbox")
            .join("inbox")
            .join("m.b2f");
        let logs = env.state_dir().join("logs").join("tuxlink.jsonl");
        write(&window_state, "{}");
        write(&cache, "cache");
        write(&mailbox, "message");
        write(&logs, "{}");

        let plan = build_plan(CleanupMode::Transient, &env);
        let report = execute_plan(&plan, false, &NoopKeyringDeleter);

        assert!(!window_state.exists());
        assert!(!env.app_data_dir().join("WebKitCache").exists());
        assert!(!env.state_dir().join("logs").exists());
        assert!(
            mailbox.exists(),
            "transient cleanup must preserve mailbox data"
        );
        assert!(!report_has_errors(&report));
    }

    #[test]
    fn full_cleanup_removes_current_legacy_tauri_data_and_desktop_leftovers_idempotently() {
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        let config = env.legacy_config_dir().join("config.json");
        let tauri_config = env.tauri_config_dir().join("pat").join("config.json");
        let mailbox = env
            .app_data_dir()
            .join("native-mbox")
            .join("inbox")
            .join("m.b2f");
        let legacy_data = env.legacy_data_dir().join("old.json");
        let desktop = env.data_home.join("applications").join("tuxlink.desktop");
        let icon = env
            .data_home
            .join("icons")
            .join("hicolor")
            .join("512x512")
            .join("apps")
            .join("com.tuxlink.app.png");
        write(&config, r#"{"identity":{"callsign":"W4PHS"}}"#);
        write(&tauri_config, r#"{"mycall":"N7CPZ"}"#);
        write(&mailbox, "message");
        write(&legacy_data, "{}");
        write(&desktop, "[Desktop Entry]");
        write(&icon, "png");

        let plan = build_plan(CleanupMode::Full, &env);
        let report1 = execute_plan(&plan, false, &NoopKeyringDeleter);
        let report2 = execute_plan(&plan, false, &NoopKeyringDeleter);

        assert!(!env.legacy_config_dir().exists());
        assert!(!env.tauri_config_dir().exists());
        assert!(!env.app_data_dir().exists());
        assert!(!env.legacy_data_dir().exists());
        assert!(!desktop.exists());
        assert!(!icon.exists());
        assert!(!report_has_errors(&report1));
        assert!(
            !report_has_errors(&report2),
            "second cleanup must be idempotent"
        );
    }

    #[derive(Default)]
    struct MockKeyring {
        existing: RefCell<BTreeSet<(String, String)>>,
        deleted: RefCell<Vec<(String, String)>>,
    }

    impl MockKeyring {
        fn with(service: &str, account: &str) -> Self {
            let this = Self::default();
            this.existing
                .borrow_mut()
                .insert((service.to_string(), account.to_string()));
            this
        }
    }

    impl KeyringDeleter for MockKeyring {
        fn delete(&self, service: &str, account: &str) -> Result<bool, String> {
            let key = (service.to_string(), account.to_string());
            self.deleted.borrow_mut().push(key.clone());
            Ok(self.existing.borrow_mut().remove(&key))
        }
    }

    #[test]
    fn full_cleanup_deletes_known_keyring_accounts_and_warns_about_non_enumerable_entries() {
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        write(
            &env.legacy_config_dir().join("config.json"),
            r#"{"identity":{"callsign":"w4phs"}}"#,
        );
        write(
            &env.legacy_config_dir()
                .join("listener")
                .join("telnet")
                .join("allowed_stations.json"),
            r#"{"allow_all":false,"callsigns":["n7cpz","BAD*"],"ips":[]}"#,
        );

        let plan = build_plan(CleanupMode::Full, &env);
        let mock = MockKeyring::with(KEYRING_SERVICE, "W4PHS");
        let report = execute_plan(&plan, false, &mock);
        let deleted = mock.deleted.borrow();

        assert!(deleted.contains(&(KEYRING_SERVICE.into(), "W4PHS".into())));
        assert!(deleted.contains(&(LEGACY_KEYRING_SERVICE.into(), "W4PHS".into())));
        assert!(deleted.contains(&(KEYRING_SERVICE.into(), "p2p-peer:N7CPZ".into())));
        assert!(!deleted.contains(&(KEYRING_SERVICE.into(), "p2p-peer:BAD*".into())));
        assert!(deleted.contains(&(KEYRING_SERVICE.into(), LISTENER_PASSWORD_ACCOUNT.into())));
        assert_eq!(
            report
                .keyring
                .iter()
                .filter(|r| matches!(r.outcome, RemovalOutcome::Removed))
                .count(),
            1
        );
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("cannot be enumerated service-wide")));
    }

    #[test]
    fn full_cleanup_enumerates_peer_endpoint_keyring_accounts() {
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        write(
            &env.app_data_dir().join("peers.json"),
            r#"{"schema_version":1,"peers":[
                {"id":"p1","canonical_base":"W6ABC","endpoints":[
                    {"id":"e1","host":"1.2.3.4","port":8772,"last_seen":"2026-07-10T00:00:00Z"},
                    {"id":"e2","host":"1.2.3.5","port":8772,"last_seen":"2026-07-10T00:00:00Z"}
                ]},
                {"id":"p2","canonical_base":"N7CPZ","endpoints":[]}
            ]}"#,
        );

        let plan = build_plan(CleanupMode::Full, &env);
        assert!(
            plan.keyring_targets
                .iter()
                .any(|t| t.account == "p2p-endpoint:p1:e1"
                    && t.description == "P2P endpoint password")
        );

        let mock = MockKeyring::with(KEYRING_SERVICE, "p2p-endpoint:p1:e1");
        let report = execute_plan(&plan, false, &mock);
        let deleted = mock.deleted.borrow();

        assert!(deleted.contains(&(KEYRING_SERVICE.into(), "p2p-endpoint:p1:e1".into())));
        assert!(deleted.contains(&(KEYRING_SERVICE.into(), "p2p-endpoint:p1:e2".into())));
        // A peer with no endpoints contributes no endpoint keyring target.
        assert!(!deleted
            .iter()
            .any(|(_, account)| account.starts_with("p2p-endpoint:p2:")));
        assert_eq!(
            report
                .keyring
                .iter()
                .filter(|r| matches!(r.outcome, RemovalOutcome::Removed))
                .count(),
            1
        );
    }

    #[test]
    fn full_cleanup_enumerates_contact_endpoint_keyring_accounts() {
        // Post-pivot (spec §AMENDMENT): endpoint secrets are keyed by
        // CONTACT id under contacts.json (schema v2); peers.json above stays
        // enumerated as a legacy dev-build artifact.
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        write(
            &env.app_data_dir().join("contacts.json"),
            r#"{"schema_version":2,"contacts":[
                {"id":"c1","name":"","callsign":"W6ABC","endpoints":[
                    {"id":"e1","host":"1.2.3.4","port":8772,"last_seen":"2026-07-11T00:00:00Z"}
                ]},
                {"id":"c2","name":"","callsign":"N7CPZ","endpoints":[]}
            ],"groups":[]}"#,
        );

        let plan = build_plan(CleanupMode::Full, &env);
        assert!(
            plan.keyring_targets
                .iter()
                .any(|t| t.account == "p2p-endpoint:c1:e1"
                    && t.description == "P2P endpoint password")
        );
        // A contact with no endpoints contributes no endpoint keyring target.
        assert!(!plan
            .keyring_targets
            .iter()
            .any(|t| t.account.starts_with("p2p-endpoint:c2:")));
    }

    #[test]
    fn dry_run_reports_existing_targets_without_removing_them() {
        let tmp = tempdir().unwrap();
        let env = test_env(tmp.path());
        let logs = env.state_dir().join("logs").join("tuxlink.jsonl");
        write(&logs, "{}");

        let plan = build_plan(CleanupMode::Transient, &env);
        let report = execute_plan(&plan, true, &NoopKeyringDeleter);

        assert!(logs.exists());
        assert!(report
            .paths
            .iter()
            .any(|p| p.path == env.state_dir().join("logs")
                && matches!(p.outcome, RemovalOutcome::WouldRemove)));
    }

    #[test]
    fn cleanup_mode_deserializes_snake_case_for_tauri_commands() {
        assert_eq!(
            serde_json::from_str::<CleanupMode>("\"keep\"").unwrap(),
            CleanupMode::Keep
        );
        assert_eq!(
            serde_json::from_str::<CleanupMode>("\"transient\"").unwrap(),
            CleanupMode::Transient
        );
        assert_eq!(
            serde_json::from_str::<CleanupMode>("\"full\"").unwrap(),
            CleanupMode::Full
        );
    }
}
