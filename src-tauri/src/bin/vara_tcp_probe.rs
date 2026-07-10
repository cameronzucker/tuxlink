//! Dev probe: validate the VARA TCP transport's wire codec + socket
//! pair against a live VARA modem instance. The probe opens both
//! sockets, sends a small init sequence (MYCALL + BW + LISTEN OFF),
//! then drains responses for a few seconds before exiting cleanly.
//!
//! NO CONNECT is issued — the probe does not initiate ARQ. The target
//! VARA instance must have no radio attached (or be in a known-safe
//! state) for the probe to be RADIO-1-free.
//!
//! Operator-runnable:
//!
//! ```bash
//! # default: connect to 127.0.0.1:8300/8301
//! cargo run --manifest-path src-tauri/Cargo.toml --bin vara_tcp_probe
//!
//! # against a remote instance:
//! TUXLINK_VARA_HOST=100.83.168.37 cargo run \
//!   --manifest-path src-tauri/Cargo.toml --bin vara_tcp_probe
//!
//! # override ports if the modem is on non-default sockets:
//! TUXLINK_VARA_HOST=100.83.168.37 TUXLINK_VARA_CMD_PORT=8400 \
//!   TUXLINK_VARA_DATA_PORT=8401 cargo run ...
//! ```
//!
//! `TUXLINK_VARA_MYCALL` overrides the callsign sent in the MYCALL
//! command; defaults to `N0CALL` (a known-invalid placeholder that
//! the modem can echo without registration).

use std::time::{Duration, Instant};

use tuxlink_lib::winlink::modem::vara::{
    Bandwidth, OutboundCommand, VaraConfig, VaraTransport,
};

fn main() {
    let host = std::env::var("TUXLINK_VARA_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let cmd_port: u16 = std::env::var("TUXLINK_VARA_CMD_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8300);
    let data_port: u16 = std::env::var("TUXLINK_VARA_DATA_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(cmd_port + 1);
    let mycall = std::env::var("TUXLINK_VARA_MYCALL").unwrap_or_else(|_| "N0CALL".into());
    let drain_secs: u64 = std::env::var("TUXLINK_VARA_DRAIN_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let cfg = VaraConfig {
        host: host.clone(),
        cmd_port,
        data_port,
        connect_timeout: Duration::from_secs(5),
        read_timeout: Some(Duration::from_millis(500)),
        data_read_timeout: Some(Duration::from_millis(500)),
    };

    println!(
        "[probe] connecting to VARA at {host}:{cmd_port} (cmd) + {host}:{data_port} (data) ..."
    );
    let mut transport = match VaraTransport::connect(cfg) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[probe] connect failed: {e}");
            std::process::exit(2);
        }
    };
    println!("[probe] sockets open.");

    // Drain anything VARA sends right after connect (READY, etc.).
    let banner_deadline = Instant::now() + Duration::from_secs(2);
    println!("[probe] draining startup banner for up to 2s ...");
    while Instant::now() < banner_deadline {
        match transport.recv() {
            Ok(Some(cmd)) => println!("  <- {cmd:?}"),
            Ok(None) => break, // timeout / EOF
            Err(e) => {
                eprintln!("[probe] banner read error: {e}");
                break;
            }
        }
    }

    // Init sequence: MYCALL → BW2300 → LISTEN OFF. Each setter is
    // echoed by VARA on success.
    println!("[probe] sending MYCALL {mycall} ...");
    if let Err(e) = transport.send(&OutboundCommand::MyCall(mycall.clone())) {
        eprintln!("[probe] send MYCALL failed: {e}");
        std::process::exit(3);
    }

    println!("[probe] sending BW2300 ...");
    if let Err(e) = transport.send(&OutboundCommand::Bw(Bandwidth::Bw2300)) {
        eprintln!("[probe] send BW2300 failed: {e}");
        std::process::exit(3);
    }

    println!("[probe] sending LISTEN OFF ...");
    if let Err(e) = transport.send(&OutboundCommand::Listen(false)) {
        eprintln!("[probe] send LISTEN OFF failed: {e}");
        std::process::exit(3);
    }

    // Drain responses for a few seconds so the operator can see
    // VARA's echo + any async events (PTT, BUFFER, IAMALIVE).
    let drain_deadline = Instant::now() + Duration::from_secs(drain_secs);
    println!("[probe] draining responses for {drain_secs}s ...");
    let mut event_count = 0usize;
    while Instant::now() < drain_deadline {
        match transport.recv() {
            Ok(Some(cmd)) => {
                event_count += 1;
                println!("  <- {cmd:?}");
            }
            Ok(None) => {
                // Timeout/EOF — keep polling until drain_deadline.
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("[probe] drain read error: {e}");
                break;
            }
        }
    }

    println!("[probe] {event_count} inbound event(s) observed in {drain_secs}s drain.");
    println!("[probe] closing sockets ...");
    if let Err(e) = transport.close() {
        eprintln!("[probe] close warning: {e}");
    }
    println!("[probe] done.");
}
