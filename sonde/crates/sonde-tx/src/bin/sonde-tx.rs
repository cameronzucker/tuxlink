//! sonde-tx — payload → PHY → PTT + audio CLI.
//!
//! Phase 3 of the sonde hardware bring-up: the plug-into-radio
//! milestone. Composes the [`sonde_phy`] PHY encoder + audio output
//! with the [`sonde_rig_rts`] PTT primitive into a single binary.
//!
//! ## Modes
//!
//! ```text
//! sonde-tx --dry-run --payload <text|@FILE> --mode <NAME>
//!     Encode the payload + report the worst-case airtime budget,
//!     WITHOUT opening any audio device or asserting PTT. Validates
//!     the encode pipeline without RF risk.
//!
//! sonde-tx --payload <text|@FILE> --mode <NAME> \
//!             --device <AUDIO> --ptt-device <TTY> \
//!             [--max-airtime <SECS>]
//!     Real on-air transmission. Asserts PTT, sleeps the lead-in,
//!     plays the encoded waveform, releases PTT.
//! ```
//!
//! ## Safety (RADIO-1)
//!
//! The full mode emits real RF when the operator's hardware is wired up.
//! This binary MUST NOT be run by automation under the operator's
//! callsign without the licensee's per-invocation consent. The agent
//! that builds this code does not run it against the real device.
//!
//! SIGINT and SIGTERM trigger an early release: the audio stream is
//! dropped (silencing the soundcard within a few callback ticks) and
//! the PTT line is released before the process exits.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sonde_rig_rts::{LinuxTty, RtsPtt};
use sonde_phy::audio_device::AudioOutput;

use sonde_tx::{
    check_budget, encode_payload, resolve_payload, run_transmission, AbortablePlay,
    AirtimeBudget, Args, Mode, TxOutcome, DEFAULT_LEAD_IN, DEFAULT_MAX_AIRTIME,
};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match Args::parse(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if let Err(e) = args.validate() {
        eprintln!("error: {e}");
        eprintln!();
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), String> {
    // ---- common: resolve payload, parse mode, encode ----
    let payload = resolve_payload(args.payload.as_deref().unwrap())
        .map_err(|e| e.to_string())?;
    let mode = Mode::parse(args.mode.as_deref().unwrap()).map_err(|e| e.to_string())?;
    let buffer = encode_payload(mode, &payload, args.frame_mode).map_err(|e| e.to_string())?;

    let budget = AirtimeBudget::from_buffer_defaults(&buffer);
    let max = args.max_airtime.unwrap_or(DEFAULT_MAX_AIRTIME);
    let effective = check_budget(&budget, max).map_err(|e| e.to_string())?;

    println!(
        "encoded {} byte(s) payload under mode {} (frame-mode: {})",
        payload.len(),
        mode.short_name(),
        args.frame_mode.short_name(),
    );
    println!(
        "  buffer duration:      {:.3} s  ({} samples)",
        buffer.duration_seconds(),
        buffer.samples().len()
    );
    println!("  lead-in:              {} ms", budget.lead_in.as_millis());
    println!("  tail-drain:           {} ms", budget.tail_drain.as_millis());
    println!("  setup slack:          {} ms", budget.setup_slack.as_millis());
    println!(
        "  total airtime budget: {:.3} s  (effective cap: {} s)",
        budget.total().as_secs_f32(),
        effective.as_secs(),
    );

    if args.dry_run {
        println!("--dry-run: no audio device opened, no PTT asserted");
        return Ok(());
    }

    if let Some(path) = args.write_wav.as_deref() {
        // ---- write-wav mode: emit the encoded waveform to a 48 kHz
        //      f32 mono WAV. No audio device, no PTT. Pairs with
        //      `sonde-rx --decode-wav <PATH>` for a fully agent-
        //      runnable loopback test.
        buffer
            .write_wav(path)
            .map_err(|e| format!("writing WAV to {}: {e}", path.display()))?;
        println!(
            "--write-wav: wrote {} samples ({:.3} s) to {}",
            buffer.samples().len(),
            buffer.duration_seconds(),
            path.display()
        );
        return Ok(());
    }

    // ---- full mode: open device + ptt, install signal handlers, transmit ----
    let device = args.device.as_deref().unwrap();
    let ptt_path = args.ptt_device.as_deref().unwrap();

    let mut audio = AudioOutput::open(Some(device))
        .map_err(|e| format!("opening audio device {device:?}: {e}"))?;

    let abort = Arc::new(AtomicBool::new(false));
    install_signal_flag(libc::SIGINT, Arc::clone(&abort))?;
    install_signal_flag(libc::SIGTERM, Arc::clone(&abort))?;

    if args.watchdog {
        // ---- watchdog mode (Phase 1.5 slice 2): spawn sonde-rig-watchdog
        //      to hold PTT. The watchdog asserts on startup, releases on
        //      stdin EOF — which fires automatically when this process
        //      exits, including under SIGKILL.
        return run_via_watchdog(&args, ptt_path, &buffer, &mut audio, &abort, effective);
    }

    let tty = LinuxTty::open(ptt_path).map_err(|e| format!("opening PTT device {ptt_path:?}: {e}"))?;
    let mut ptt = RtsPtt::new(tty).map_err(|e| format!("initializing PTT: {e}"))?;

    println!(
        "asserting PTT on {ptt_path}; waiting {} ms lead-in; playing {:.3} s; releasing",
        DEFAULT_LEAD_IN.as_millis(),
        buffer.duration_seconds(),
    );

    let outcome = run_transmission(
        &mut ptt,
        &mut audio,
        &buffer,
        DEFAULT_LEAD_IN,
        &abort,
    )
    .map_err(|e| e.to_string())?;

    match outcome {
        TxOutcome::Completed => println!("transmission completed cleanly"),
        TxOutcome::AbortedEarly => println!("aborted early (signal received) — PTT released"),
    }
    Ok(())
}

/// Watchdog-mode orchestration. Spawns `sonde-rig-watchdog` as a child
/// process with stdin piped, opens audio, sleeps the lead-in, plays the
/// buffer, then drops the stdin pipe so the watchdog detects EOF and
/// releases PTT. Finally waits for the watchdog to exit.
///
/// SIGKILL safety: if this process is SIGKILL'd mid-play, the kernel
/// closes the pipe to the watchdog automatically → watchdog sees EOF →
/// releases PTT. That's the whole point — `RtsPtt::Drop` is bypassed
/// under SIGKILL, but the OS still closes the pipe.
fn run_via_watchdog(
    args: &Args,
    ptt_path: &str,
    buffer: &sonde_phy::audio_io::AudioBuffer,
    audio: &mut AudioOutput,
    abort: &AtomicBool,
    effective_cap: Duration,
) -> Result<(), String> {
    use std::process::{Command, Stdio};

    let watchdog_bin = args
        .watchdog_bin
        .as_deref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sonde-rig-watchdog".to_string());

    let mut child = Command::new(&watchdog_bin)
        .arg("--device")
        .arg(ptt_path)
        .arg("--max-seconds")
        .arg(effective_cap.as_secs().to_string())
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawning watchdog {watchdog_bin:?}: {e}"))?;

    // Keep the stdin pipe handle alive in scope; dropping it later closes
    // the pipe and signals EOF to the watchdog.
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "watchdog stdin pipe disappeared after spawn".to_string())?;

    println!(
        "spawned watchdog ({watchdog_bin}) on {ptt_path}; \
         waiting {} ms for watchdog to assert + lead-in; playing {:.3} s",
        DEFAULT_LEAD_IN.as_millis(),
        buffer.duration_seconds(),
    );

    // Give the watchdog a moment to do OpenClearBoth + AssertRts, then
    // the radio's TX chain time to fully key. The existing DEFAULT_LEAD_IN
    // (~100 ms) covers both; the watchdog asserts within a few ms of
    // spawn, so the dominant component is still the radio's TX-key delay.
    std::thread::sleep(DEFAULT_LEAD_IN);

    let play_result = AbortablePlay::play_blocking_with_abort(audio, buffer, abort);

    // Drop the stdin pipe NOW so the watchdog sees EOF and releases PTT
    // before we wait for it to exit.
    drop(stdin);

    let status = child
        .wait()
        .map_err(|e| format!("waiting for watchdog to exit: {e}"))?;

    match play_result {
        Ok(sonde_phy::audio_device::PlayOutcome::Completed) => {
            println!(
                "transmission completed cleanly (watchdog exit: {})",
                status_label(&status)
            );
            Ok(())
        }
        Ok(sonde_phy::audio_device::PlayOutcome::Aborted) => {
            println!(
                "aborted early (signal received) — PTT released by watchdog (exit: {})",
                status_label(&status)
            );
            Ok(())
        }
        Err(e) => Err(format!("playback error: {e} (watchdog exit: {})", status_label(&status))),
    }
}

fn status_label(s: &std::process::ExitStatus) -> String {
    match s.code() {
        Some(c) => format!("code {c}"),
        None => "signaled".to_string(),
    }
}

/// Install a signal handler that sets the shared atomic. Matches the
/// pattern used by `sonde-rig-rts`'s CLI: a single async-signal-safe
/// `AtomicBool::store` from the handler, with the [`Arc`] stashed in
/// a process-static [`OnceLock`] (signal handlers can't capture).
fn install_signal_flag(sig: libc::c_int, flag: Arc<AtomicBool>) -> Result<(), String> {
    use std::sync::OnceLock;
    // Two slots — one each for SIGINT and SIGTERM. The handler can't
    // distinguish but BOTH flags reference the SAME caller-owned
    // AtomicBool, so a single store suffices in either path.
    static FLAG_SLOT: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let _ = FLAG_SLOT.set(flag);

    extern "C" fn handler(_: libc::c_int) {
        if let Some(f) = FLAG_SLOT.get() {
            f.store(true, Ordering::Release);
        }
    }

    // SAFETY: libc::signal races against concurrent signal delivery
    // during process startup. We call from main() before any second
    // thread exists, so single-threaded reasoning suffices.
    #[allow(unsafe_code)]
    let prev = unsafe { libc::signal(sig, handler as *const () as libc::sighandler_t) };
    if prev == libc::SIG_ERR {
        let errno = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(0);
        return Err(format!("signal({sig}) install failed: errno={errno}"));
    }
    Ok(())
}

const USAGE: &str = "\
sonde-tx — payload → PHY → PTT + audio composition

USAGE:
    sonde-tx --dry-run --payload <TEXT|@FILE> --mode <NAME>
        Encode + report the airtime budget WITHOUT opening any device.

    sonde-tx --write-wav <PATH> --payload <TEXT|@FILE> --mode <NAME>
        Encode + write the waveform to a 48 kHz f32 mono WAV file.
        No audio device, no PTT. Pairs with `sonde-rx --decode-wav`.

    sonde-tx --payload <TEXT|@FILE> --mode <NAME> \\
                --device <AUDIO> --ptt-device <TTY> \\
                [--max-airtime <SECS>]
        Real on-air transmission.

OPTIONS:
        --mode <NAME>          PHY mode (currently: `wide-floor` / `floor-wblo`)
        --payload <ARG>        payload text, or `@<path>` to read from a file
        --dry-run              encode + report only; no audio, no PTT
        --write-wav <PATH>     encode + write WAV; no audio device, no PTT
    -d, --device <NAME>        CPAL audio device name (required for full TX)
    -p, --ptt-device <PATH>    serial TTY path for RTS PTT (required for full TX)
        --frame-mode <NAME>    frame format: raw / sync / multi-sync (default: raw)
                               - raw:        bare OFDM symbol, no preamble
                               - sync:       preamble + single symbol (≤9 bytes)
                               - multi-sync: preamble + N symbols, length-prefix
                                             header, up to u16::MAX bytes
        --max-airtime <SECS>   override DEFAULT_MAX_AIRTIME (default 30; hard cap 60)
        --watchdog             spawn sonde-rig-watchdog as a child process to hold PTT
                               (Phase 1.5 SIGKILL-safe TX — recommended for production)
        --watchdog-bin <PATH>  explicit path to the sonde-rig-watchdog binary
                               (default: looked up on PATH as `sonde-rig-watchdog`)
    -h, --help                 this usage

DISCOVERING AUDIO DEVICES:
    sonde-audio-play --list

EXAMPLES:
    # Validate the encode pipeline with no RF risk:
    sonde-tx --dry-run --payload \"TEST\" --mode wide-floor

    # Emit the waveform as a WAV for offline / agent loopback testing:
    sonde-tx --write-wav /tmp/test.wav --payload \"TEST\" --mode wide-floor
    sonde-rx --decode-wav /tmp/test.wav --expected \"TEST\"

    # Long-payload loopback via multi-symbol framing (no length cap):
    sonde-tx --write-wav /tmp/long.wav --payload @./large-file.bin \\
                --mode wide-floor --frame-mode multi-sync
    sonde-rx --decode-wav /tmp/long.wav --expected @./large-file.bin \\
                --frame-mode multi-sync

    # Actual on-air TX (operator-only — agents must not run this):
    sonde-tx --payload \"TEST\" --mode wide-floor \\
                --device 'USB Audio Device' --ptt-device /dev/digirig

    # On-air TX with SIGKILL-safe PTT via watchdog daemon (recommended):
    sonde-tx --payload \"TEST\" --mode wide-floor \\
                --device 'USB Audio Device' --ptt-device /dev/digirig \\
                --watchdog \\
                --watchdog-bin /path/to/sonde-rig-watchdog

SAFETY (RADIO-1):
    The full mode emits real RF when the radio is wired to the chosen
    audio device. Per project policy this binary MUST NOT be run by
    automation under the operator's callsign without per-invocation
    licensee consent. Agents may write + test this code but must not
    run it against the real radio.

    SIGINT and SIGTERM during transmission trigger early-release:
    the audio stream is dropped (silencing the soundcard within a
    few callback ticks) and the PTT line is released before exit.

    Total airtime is budget-gated BEFORE PTT assert; configurations
    that would exceed the budget are rejected without keying the
    radio. The hard cap is 60 s regardless of --max-airtime.
";
