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
// TODO(3.2+): extend McpState with mock backend/mailbox/log handles as later
// phases add tools beyond the inert `server_info`.

use std::path::PathBuf;
use std::sync::Arc;

use tuxlink_mcp_core::{McpState, TuxlinkMcp};
use tuxlink_security::EgressGuard;

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
            guard.taint();
            eprintln!("{TAINT_ENV}={v}: session tainted");
        }
    }

    // Identity the `server_info` tool reports. Overridable for test
    // flexibility; defaults to the real app name so a default tier-2 run
    // reports name="tuxlink", NOT this core/testserver crate identity.
    let name = std::env::var(NAME_ENV).unwrap_or_else(|_| "tuxlink".to_string());
    let version = std::env::var(VERSION_ENV).unwrap_or_else(|_| "testserver".to_string());

    let guard = Arc::new(guard);
    let state = McpState {
        guard: Arc::clone(&guard),
        name: name.clone(),
        version: version.clone(),
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
