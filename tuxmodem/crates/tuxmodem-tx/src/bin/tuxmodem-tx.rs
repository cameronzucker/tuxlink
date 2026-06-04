//! tuxmodem-tx — payload → PHY → PTT + audio CLI.
//!
//! Phase 3 of the tuxmodem hardware bring-up: the plug-into-radio
//! milestone. Composes the [`tuxmodem_phy`] PHY encoder + audio output
//! with the [`tux_rig_rts`] PTT primitive into a single binary.
//!
//! ## Modes
//!
//! ```text
//! tuxmodem-tx --dry-run --payload <text|@FILE> --mode <NAME>
//!     Encode the payload + report the worst-case airtime budget,
//!     WITHOUT opening any audio device or asserting PTT. Validates
//!     the encode pipeline without RF risk.
//!
//! tuxmodem-tx --payload <text|@FILE> --mode <NAME> \
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

use tux_rig_rts::{LinuxTty, RtsPtt};
use tuxmodem_phy::audio_device::AudioOutput;

use tuxmodem_tx::{
    check_budget, encode_payload, resolve_payload, run_transmission, AirtimeBudget, Args,
    Mode, TxOutcome, DEFAULT_LEAD_IN, DEFAULT_MAX_AIRTIME,
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
        //      `tuxmodem-rx --decode-wav <PATH>` for a fully agent-
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

    let tty = LinuxTty::open(ptt_path).map_err(|e| format!("opening PTT device {ptt_path:?}: {e}"))?;
    let mut ptt = RtsPtt::new(tty).map_err(|e| format!("initializing PTT: {e}"))?;

    let abort = Arc::new(AtomicBool::new(false));
    install_signal_flag(libc::SIGINT, Arc::clone(&abort))?;
    install_signal_flag(libc::SIGTERM, Arc::clone(&abort))?;

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

/// Install a signal handler that sets the shared atomic. Matches the
/// pattern used by `tux-rig-rts`'s CLI: a single async-signal-safe
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
tuxmodem-tx — payload → PHY → PTT + audio composition

USAGE:
    tuxmodem-tx --dry-run --payload <TEXT|@FILE> --mode <NAME>
        Encode + report the airtime budget WITHOUT opening any device.

    tuxmodem-tx --write-wav <PATH> --payload <TEXT|@FILE> --mode <NAME>
        Encode + write the waveform to a 48 kHz f32 mono WAV file.
        No audio device, no PTT. Pairs with `tuxmodem-rx --decode-wav`.

    tuxmodem-tx --payload <TEXT|@FILE> --mode <NAME> \\
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
        --max-airtime <SECS>   override DEFAULT_MAX_AIRTIME (default 30; hard cap 60)
    -h, --help                 this usage

DISCOVERING AUDIO DEVICES:
    tuxmodem-audio-play --list

EXAMPLES:
    # Validate the encode pipeline with no RF risk:
    tuxmodem-tx --dry-run --payload \"TEST\" --mode wide-floor

    # Emit the waveform as a WAV for offline / agent loopback testing:
    tuxmodem-tx --write-wav /tmp/test.wav --payload \"TEST\" --mode wide-floor
    tuxmodem-rx --decode-wav /tmp/test.wav --expected \"TEST\"

    # Actual on-air TX (operator-only — agents must not run this):
    tuxmodem-tx --payload \"TEST\" --mode wide-floor \\
                --device 'USB Audio Device' --ptt-device /dev/digirig

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
