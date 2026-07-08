//! tuxlink-mcp-testserver — tier-2 harness for MCP phase 3.1.
//!
//! Serves the REAL [`tuxlink_mcp_core`] router over a REAL Unix-domain socket,
//! wired to a REAL [`tuxlink_security::EgressGuard`], with NO Tauri and NO full
//! app build. This is the spike productionized: the agent runs `claude mcp add`
//! against this standalone binary on the Pi (no operator, no app) and calls
//! `server_info` to confirm the MCP layer + UDS + the guard behave end-to-end.
//!
//! ## Environment contract
//!
//! - `TUXLINK_MCP_SOCK` (REQUIRED): filesystem path the socket binds to — the
//!   path the shim / `claude mcp` connects to. Absent → clear error, non-zero
//!   exit.
//! - `TUXLINK_TEST_ARM` (OPTIONAL): integer seconds. When set and parseable,
//!   the guard is armed for that many seconds against the REAL clock, so
//!   `server_info` reports `armed=true` for the duration of the test.
//! - `TUXLINK_TEST_TAINT` (OPTIONAL): `1` or `true` (case-insensitive) taints
//!   the guard, so `server_info` reports `tainted=true`.
//! - `TUXLINK_TEST_NAME` (OPTIONAL): app name `server_info` reports. Defaults
//!   to `"tuxlink"` so a default tier-2 run reports the real app name, not this
//!   core/testserver crate identity.
//! - `TUXLINK_TEST_VERSION` (OPTIONAL): app version `server_info` reports.
//!   Defaults to `"testserver"`.
//!
//! With neither arm nor taint set, the guard is fresh (`armed=false`,
//! `tainted=false`).
//
//! Phase 3.2: the McpState is now wired with canned mock ports (see `mocks`)
//! so the tier-2 round-trip can exercise the tier-1 read tools + taint behavior
//! against a real UDS without the Tauri monolith.

use std::path::PathBuf;
use std::sync::Arc;

use tuxlink_mcp_core::ports::{
    ConfigPort, DevicePort, LogPort, MailboxPort, PredictionPort, SearchPort, StationPort,
    StatusPort,
};
use tuxlink_mcp_core::{McpState, TuxlinkMcp};
use tuxlink_security::EgressGuard;

mod fixture;
mod mocks;
mod scenario_ports;

const SOCK_ENV: &str = "TUXLINK_MCP_SOCK";
const ARM_ENV: &str = "TUXLINK_TEST_ARM";
const TAINT_ENV: &str = "TUXLINK_TEST_TAINT";
const NAME_ENV: &str = "TUXLINK_TEST_NAME";
const VERSION_ENV: &str = "TUXLINK_TEST_VERSION";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the socket path (required). The shim / `claude mcp` connects here.
    let sock_path: PathBuf = match std::env::var_os(SOCK_ENV) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            eprintln!(
                "error: {SOCK_ENV} is required (the UDS path to bind). \
                 e.g. {SOCK_ENV}=/run/user/$(id -u)/tuxlink-test.sock {}",
                env!("CARGO_PKG_NAME")
            );
            std::process::exit(2);
        }
    };

    // Build the REAL guard with the REAL clock, so an arm of e.g. 3600 is
    // genuinely live for the duration of the tier-2 test.
    let guard = EgressGuard::new();

    // Optional: pre-arm the guard so `server_info` reports armed=true.
    if let Some(raw) = std::env::var_os(ARM_ENV) {
        let raw = raw.to_string_lossy();
        match raw.trim().parse::<u64>() {
            Ok(secs) => {
                let deadline = guard.arm(secs);
                eprintln!("{ARM_ENV}={secs}: armed for {secs}s (deadline unix={deadline})");
            }
            Err(_) => {
                eprintln!(
                    "error: {ARM_ENV} must be an integer number of seconds, got {raw:?}"
                );
                std::process::exit(2);
            }
        }
    }

    // Optional: taint the guard so `server_info` reports tainted=true.
    if let Some(raw) = std::env::var_os(TAINT_ENV) {
        let raw = raw.to_string_lossy();
        let v = raw.trim();
        if v.eq_ignore_ascii_case("1") || v.eq_ignore_ascii_case("true") {
            guard.taint(tuxlink_security::TaintReason::MessageRead);
            eprintln!("{TAINT_ENV}={v}: session tainted");
        }
    }

    // Identity the `server_info` tool reports. Overridable for test
    // flexibility; defaults to the real app name so a default tier-2 run
    // reports name="tuxlink", NOT this core/testserver crate identity.
    let name = std::env::var(NAME_ENV).unwrap_or_else(|_| "tuxlink".to_string());
    let version = std::env::var(VERSION_ENV).unwrap_or_else(|_| "testserver".to_string());

    let guard = Arc::new(guard);
    // Egress/abort probe flags. The egress mock shares the SAME guard built from
    // the environment above, so TUXLINK_TEST_ARM/TAINT drive the real gate
    // end-to-end: a gated egress is denied unless armed + un-tainted.
    let egress_op_ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let aborted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let staged = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Attachment base dir for the write mock's dest validation. A real,
    // canonicalizable directory so `validate_attachment_dest`'s symlink-escape
    // check has something to canonicalize against.
    let attach_base = std::env::temp_dir().join(format!("tuxlink-mcp-attach-{}", std::process::id()));
    std::fs::create_dir_all(&attach_base)?;

    // TUXLINK_TEST_SCENARIO (OPTIONAL): when set, load the scenario world and
    // serve the REAL DTOs it seeds through the scenario read ports instead of the
    // recognizable mock stubs. Absent/empty ⇒ keep the mock ports (current
    // behavior). A bad path fails LOUDLY (non-zero exit) rather than degrading.
    let scenario = match fixture::load_scenario_from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    if let Some(world) = &scenario {
        let gw = world
            .stations
            .as_ref()
            .map(|s| s.gateways.len())
            .unwrap_or(0);
        eprintln!("scenario loaded ({gw} gateways)");
    }

    // Read ports: branch to the scenario impls (real seeded DTOs) when a scenario
    // is loaded, else the mock stubs. The egress/abort/write/compose ports —
    // which hold the REAL EgressGuard — are UNCHANGED in both branches so the
    // gate still runs for real; the harness measures the read-tier fabrication,
    // not the guard.
    #[allow(clippy::type_complexity)]
    let (status, mailbox, search, config, devices, logs, stations, prediction): (
        Arc<dyn StatusPort>,
        Arc<dyn MailboxPort>,
        Arc<dyn SearchPort>,
        Arc<dyn ConfigPort>,
        Arc<dyn DevicePort>,
        Arc<dyn LogPort>,
        Arc<dyn StationPort>,
        Arc<dyn PredictionPort>,
    ) = match &scenario {
        Some(world) => (
            Arc::new(scenario_ports::ScenarioStatus(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioMailbox(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioSearch(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioConfig(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioDevice(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioLog(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioStation(Arc::clone(world))),
            Arc::new(scenario_ports::ScenarioPrediction(Arc::clone(world))),
        ),
        None => (
            Arc::new(mocks::MockStatus),
            Arc::new(mocks::MockMailbox),
            Arc::new(mocks::MockSearch),
            Arc::new(mocks::MockConfig),
            Arc::new(mocks::MockDevice),
            Arc::new(mocks::MockLog),
            Arc::new(mocks::MockStation),
            Arc::new(mocks::MockPrediction),
        ),
    };

    let state = McpState {
        guard: Arc::clone(&guard),
        name: name.clone(),
        version: version.clone(),
        status,
        mailbox,
        search,
        config,
        devices,
        logs,
        egress: Arc::new(mocks::MockEgress::new(
            Arc::clone(&guard),
            Arc::clone(&egress_op_ran),
        )),
        abort: Arc::new(mocks::MockAbort::new(Arc::clone(&aborted))),
        write: Arc::new(mocks::MockWrite::new(
            Arc::clone(&guard),
            Arc::clone(&egress_op_ran),
            attach_base,
        )),
        compose: Arc::new(mocks::MockCompose::new(Arc::clone(&staged))),
        stations,
        prediction,
        // Provisioning is non-transmit + ungated; the mock is used in both the
        // scenario and no-scenario branches (like egress/abort/write/compose).
        provision: Arc::new(mocks::MockProvision),
    };
    let router = TuxlinkMcp::new(Arc::new(state));

    eprintln!(
        "{name} v{version} listening on {} (armed={}, tainted={})",
        sock_path.display(),
        guard.armed_remaining() > 0,
        guard.is_tainted(),
    );

    // Race the serve loop against Ctrl-C so a clean SIGINT unlinks the socket
    // (the transport's drop guard removes the socket file on return).
    tokio::select! {
        res = tuxlink_mcp_core::transport_uds::serve(router, &sock_path) => {
            // serve() only returns on an accept/serve error; surface it.
            res?;
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("received Ctrl-C; shutting down and unlinking {}", sock_path.display());
        }
    }

    Ok(())
}
