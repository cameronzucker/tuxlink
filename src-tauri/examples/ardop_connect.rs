//! On-site ARDOP bring-up CLI.
//!
//! Spawns ardopcf, drives the full init → connect_arq → data read → disconnect
//! → shutdown cycle against a real (or lab) ARDOP TNC on the local machine.
//!
//! # RADIO-1 WARNING
//!
//! **This example transmits when `--target` is reachable.**
//! Run only with explicit, per-invocation operator consent under a licensed
//! amateur callsign. Do NOT run from agent shells, CI pipelines, or any
//! automated context. See `docs/live-cms-testing-policy.md`.
//!
//! # Usage
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example ardop_connect -- \
//!   --binary ardopcf \
//!   --mycall N7CPZ \
//!   --gridsquare CN87 \
//!   --capture plughw:1,0 \
//!   --playback plughw:1,0 \
//!   [--ptt /dev/ttyUSB0] \
//!   [--cmd-port 8515] \
//!   --target W7RMS-10 \
//!   [--repeat 3] \
//!   [--audio-device-path /dev/snd/pcmC1D0c]
//! ```

use std::time::Duration;

use tuxlink_lib::winlink::modem::ardop::transport::ArdopTransport;
use tuxlink_lib::winlink::modem::ardop::ArdopConfig;
use tuxlink_lib::winlink::modem::{InitConfig, ModemTransport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Argument parsing (std::env::args; no clap/anyhow) ────────────────
    let mut args = std::env::args().skip(1); // skip argv[0]

    let mut binary = String::from("ardopcf");
    let mut mycall = String::new();
    let mut gridsquare = String::new();
    let mut capture = String::new();
    let mut playback = String::new();
    let mut ptt: Option<String> = None;
    let mut cmd_port: u16 = 8515;
    let mut target = String::new();
    let mut repeat: u32 = 3;
    let mut audio_device_path: Option<std::path::PathBuf> = None;

    while let Some(flag) = args.next() {
        let mut value = || -> Result<String, Box<dyn std::error::Error>> {
            args.next()
                .ok_or_else(|| format!("flag {flag} requires a value").into())
        };
        match flag.as_str() {
            "--binary" => binary = value()?,
            "--mycall" => mycall = value()?,
            "--gridsquare" => gridsquare = value()?,
            "--capture" => capture = value()?,
            "--playback" => playback = value()?,
            "--ptt" => ptt = Some(value()?),
            "--cmd-port" => {
                cmd_port = value()?
                    .parse()
                    .map_err(|e| format!("--cmd-port: {e}"))?;
            }
            "--target" => target = value()?,
            "--repeat" => {
                repeat = value()?
                    .parse()
                    .map_err(|e| format!("--repeat: {e}"))?;
            }
            "--audio-device-path" => {
                audio_device_path = Some(std::path::PathBuf::from(value()?));
            }
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(1);
            }
        }
    }

    // Validate required args.
    for (name, val) in [
        ("--mycall", &mycall),
        ("--gridsquare", &gridsquare),
        ("--capture", &capture),
        ("--playback", &playback),
        ("--target", &target),
    ] {
        if val.is_empty() {
            return Err(format!("missing required argument: {name}").into());
        }
    }

    let data_port = cmd_port
        .checked_add(1)
        .ok_or("cmd_port overflows at u16::MAX")?;

    // ── Build ArdopConfig ─────────────────────────────────────────────────
    //
    // ardopcf positional calling convention: ardopcf <cmd_port> <capture> <playback>
    // Optional PTT flag: ardopcf -p <ptt_device> <cmd_port> <capture> <playback>
    let mut extra_args: Vec<String> = Vec::new();
    if let Some(ref ptt_dev) = ptt {
        extra_args.push("-p".into());
        extra_args.push(ptt_dev.clone());
    }
    extra_args.push(cmd_port.to_string());
    extra_args.push(capture.clone());
    extra_args.push(playback.clone());

    let cfg = ArdopConfig {
        binary: std::path::PathBuf::from(&binary),
        extra_args,
        cmd_port,
        data_port,
        audio_device_path,
    };

    println!("ardop_connect: spawning {binary} on ports {cmd_port}/{data_port}");
    println!("  mycall={mycall}  gridsquare={gridsquare}  target={target}  repeat={repeat}");

    // ── with_managed_modem + init ─────────────────────────────────────────
    let mut transport = ArdopTransport::with_managed_modem(cfg)?;
    println!("ardop_connect: TNC ports bound; running init sequence...");

    let init_cfg = InitConfig {
        mycall: mycall.clone(),
        gridsquare: gridsquare.clone(),
        arq_timeout_s: 30,
        arq_bandwidth_hz: None,
    };
    transport.init(&init_cfg)?;
    println!("ardop_connect: init OK");

    // ── connect_arq ───────────────────────────────────────────────────────
    println!("ardop_connect: dialling {target} (repeat={repeat}, timeout=45s)...");
    let connect_deadline = Duration::from_secs(45);
    let info = transport.connect_arq(&target, repeat, connect_deadline)?;
    println!(
        "ardop_connect: CONNECTED peer={} bandwidth_hz={}",
        info.peer_call, info.bandwidth_hz
    );

    // ── Bounded read from data_stream (~15s) ─────────────────────────────
    //
    // data_stream() returns &mut dyn ReadWrite (io::Read + io::Write). The
    // underlying DataSocket wraps a TcpStream; tuxlink sets a read timeout
    // during the B2F exchange. Here we do a single best-effort read: the
    // ARQ link timeout (set to 30s in InitConfig.arq_timeout_s) will cause
    // the TNC to drop the link if the peer sends nothing, so the read will
    // eventually return an EOF or error — naturally bounding the call. This
    // is a bring-up CLI tool; the operator is present and can Ctrl-C.
    {
        let stream = transport.data_stream()?;
        let mut buf = vec![0u8; 4096];
        match stream.read(&mut buf) {
            Ok(0) => println!("ardop_connect: data stream EOF"),
            Ok(n) => println!("ardop_connect: read {n} bytes from data stream"),
            Err(e) => println!("ardop_connect: data read error (normal on idle link): {e}"),
        }
    }

    // ── disconnect + shutdown ─────────────────────────────────────────────
    println!("ardop_connect: disconnecting...");
    // disconnect() is best-effort here; shutdown() will also attempt disconnect.
    let _ = transport.disconnect(Duration::from_secs(10));

    println!("ardop_connect: shutting down TNC process...");
    transport.shutdown()?;
    println!("ardop_connect: done");

    Ok(())
}
