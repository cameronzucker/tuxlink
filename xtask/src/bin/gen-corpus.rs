//! Synthetic event-corpus generator. Produces ~1.5-2 MB of representative
//! JSONL events under the output directory by combining templated event
//! sequences with the real-string fixtures.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "dev/log-corpus-synthetic/")]
    output: PathBuf,
    #[arg(long, default_value = "dev/log-corpus-fixtures/")]
    fixtures: PathBuf,
    /// Approximate total bytes to produce.
    #[arg(long, default_value_t = 1_700_000)]
    target_bytes: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.output)?;

    let mut bytes_written = 0usize;
    let mut file_idx = 0;
    let base_ts = "2026-06-04T08:00:00Z".parse::<DateTime<Utc>>()?;
    let mut seq = 1u64;

    let fixtures = load_fixtures(&args.fixtures)?;

    while bytes_written < args.target_bytes {
        let path = args.output.join(format!("corpus-{file_idx:04}.jsonl"));
        let mut content = String::new();
        let chunk_target = (args.target_bytes - bytes_written).min(64 * 1024);

        while content.len() < chunk_target {
            let event = next_synthetic_event(seq, base_ts + Duration::milliseconds(seq as i64 * 137), &fixtures, seq);
            let line = serde_json::to_string(&event)?;
            content.push_str(&line);
            content.push('\n');
            seq += 1;
        }

        std::fs::write(&path, &content).with_context(|| format!("write {path:?}"))?;
        bytes_written += content.len();
        file_idx += 1;
    }

    println!("Generated {bytes_written} bytes across {file_idx} files at {:?}", args.output);
    Ok(())
}

#[derive(Default)]
struct Fixtures {
    keyring_errors: Vec<String>,
    audio_errors: Vec<String>,
    vara_errors: Vec<String>,
    ardop_errors: Vec<String>,
    bluez_errors: Vec<String>,
}

fn load_fixtures(dir: &std::path::Path) -> Result<Fixtures> {
    let mut f = Fixtures::default();
    let read = |name: &str| -> Result<Vec<String>> {
        let p = dir.join(name);
        if !p.exists() {
            return Ok(vec![]);
        }
        Ok(std::fs::read_to_string(&p)?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(String::from)
            .collect())
    };
    f.keyring_errors = read("keyring-errors.txt")?;
    f.audio_errors = read("audio-errors.txt")?;
    f.vara_errors = read("vara-errors.txt")?;
    f.ardop_errors = read("ardop-errors.txt")?;
    f.bluez_errors = read("bluez-errors.txt")?;
    Ok(f)
}

fn next_synthetic_event(seq: u64, ts: DateTime<Utc>, fixtures: &Fixtures, idx: u64) -> Value {
    // Cycle through event templates representing each cluster.
    let callsigns = ["K0ABC", "W7XYZ", "VE3ABC", "G0XYZ", "JA1ABC"];
    let gateways = ["K6XXX-10", "W7AAA-10", "VE3BBB-10", "K1CCC-10"];
    let transports = ["telnet", "vara", "ardop"];
    let attempt_ids = (0..50).map(|i| format!("att-x{i:04}")).collect::<Vec<_>>();

    let cluster_idx = (idx % 12) as usize;
    let callsign = callsigns[(idx as usize) % callsigns.len()];
    let gateway = gateways[(idx as usize) % gateways.len()];
    let transport = transports[(idx as usize) % transports.len()];
    let attempt_id = &attempt_ids[(idx as usize) % attempt_ids.len()];
    let ts_str = ts.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    let (target, level, msg, fields) = match cluster_idx {
        0 => (
            "tuxlink::winlink::session", "info", "dial start",
            json!({"transport": transport, "gateway": gateway, "callsign": callsign}),
        ),
        1 => (
            "tuxlink::winlink::session", "debug", "B2F handshake complete",
            json!({"attempt_id": attempt_id, "remote_sid": "WL2K-5.0-B2FWIHJM"}),
        ),
        2 => (
            "tuxlink::winlink::modem::vara", "debug", "VARA CONNECT command sent",
            json!({"target": gateway, "bandwidth_hz": 2300}),
        ),
        3 => (
            "tuxlink::winlink::ax25::frame", "debug", "I-frame received",
            json!({"ns": idx % 8, "nr": (idx + 1) % 8, "pf": false, "payload_bytes": 256}),
        ),
        4 => (
            "tuxlink::winlink::listener::decide", "info", "inbound session accepted",
            json!({"peer": "K7LED-7", "attempt_id": attempt_id}),
        ),
        5 => (
            "tuxlink::winlink::session", "warn", "dial failed: timeout",
            json!({"transport": transport, "gateway": gateway, "timeout_s": 110, "attempt_id": attempt_id}),
        ),
        6 => {
            let err = fixtures.keyring_errors.get((idx as usize) % fixtures.keyring_errors.len().max(1))
                .cloned().unwrap_or_default();
            (
                "tuxlink::logging::env_probes::keyring", "info", "keyring environment snapshot",
                json!({"backend": "secret_service", "error_seen": err, "dbus_reachable": true}),
            )
        }
        7 => {
            let err = fixtures.audio_errors.get((idx as usize) % fixtures.audio_errors.len().max(1))
                .cloned().unwrap_or_default();
            (
                "tuxlink::logging::env_probes::audio", "info", "audio environment snapshot",
                json!({"backend": "pipewire", "device_count": 2, "configured_match": true, "error_seen": err}),
            )
        }
        8 => {
            let err = fixtures.vara_errors.get((idx as usize) % fixtures.vara_errors.len().max(1))
                .cloned().unwrap_or_default();
            (
                "tuxlink::winlink::modem::vara", "error", "VARA process error",
                json!({"error": err, "attempt_id": attempt_id}),
            )
        }
        9 => {
            let err = fixtures.ardop_errors.get((idx as usize) % fixtures.ardop_errors.len().max(1))
                .cloned().unwrap_or_default();
            (
                "tuxlink::winlink::modem::ardop", "error", "ARDOP process error",
                json!({"error": err, "attempt_id": attempt_id}),
            )
        }
        10 => {
            let err = fixtures.bluez_errors.get((idx as usize) % fixtures.bluez_errors.len().max(1))
                .cloned().unwrap_or_default();
            (
                "tuxlink::winlink::ax25::rfcomm", "warn", "Bluetooth RFCOMM error",
                json!({"error": err}),
            )
        }
        _ => (
            "tuxlink::winlink::transfer", "info", "message sent",
            json!({"message_id": format!("m-{idx:06}"), "size_bytes": 1024 + idx % 4096, "to": callsign}),
        ),
    };

    json!({
        "v": 1,
        "ts": ts_str,
        "boot": "01927a8b-9c12-7000-a4d3-2f8e1b9c0001",
        "seq": seq,
        "level": level,
        "target": target,
        "module": target,
        "pid": 12345,
        "thread": {"id": 7, "name": "tokio-runtime-worker"},
        "attempt_id": attempt_id,
        "spans": [{"name": "dial_attempt", "id": "0x7f3a", "attempt_id": attempt_id}],
        "msg": msg,
        "fields": fields,
    })
}
