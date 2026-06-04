//! tuxmodem-rx — capture + demod + BER CLI (Phase 4, tuxlink-xvrb).
//!
//! See the crate-level docs for the workflow. Safety: pure capture
//! (no PTT, no audio output, no transmission). RADIO-1 does NOT gate
//! this binary the way it gates `tuxmodem-tx`.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tuxmodem_phy::audio_device::{list_input_devices, AudioInput, RecordOutcome};

use tuxmodem_rx::{
    compute_ber, decode_one_symbol_with_offset, read_wav, record_to_wav, resolve_expected,
    Args, FrameMode, Mode,
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
    if args.list_devices {
        return run_list_devices();
    }
    if let Some(path) = args.decode_wav.as_deref() {
        let mode = parse_mode(&args)?;
        return run_decode_wav(path, mode, args.frame_mode, args.expected.as_deref());
    }
    if let Some(path) = args.record_wav.as_deref() {
        // validate() already enforced these are present
        let device = args.device.as_deref().unwrap();
        let duration = Duration::from_secs(args.duration_secs.unwrap() as u64);
        return run_record_wav(path, device, duration);
    }
    unreachable!("validate() rejects no-operation");
}

fn parse_mode(args: &Args) -> Result<Mode, String> {
    let raw = args.mode.as_deref().unwrap_or("wide-floor");
    Mode::parse(raw).map_err(|e| e.to_string())
}

fn run_list_devices() -> Result<(), String> {
    let devices = list_input_devices().map_err(|e| e.to_string())?;
    if devices.is_empty() {
        println!("(no input devices)");
        return Ok(());
    }
    println!(
        "{:<48} {:>4}  {:>15}  48kHz-f32?",
        "device", "ch", "rate range (Hz)"
    );
    println!("{}", "-".repeat(80));
    for d in devices {
        println!(
            "{:<48} {:>4}  {:>7} - {:>5}  {}",
            truncate(&d.name, 48),
            d.default_channels,
            d.min_sample_rate_hz,
            d.max_sample_rate_hz,
            if d.supports_48k_f32 { "yes" } else { "no" },
        );
    }
    Ok(())
}

fn run_decode_wav(
    path: &std::path::Path,
    mode: Mode,
    frame_mode: FrameMode,
    expected: Option<&str>,
) -> Result<(), String> {
    let buffer = read_wav(path).map_err(|e| e.to_string())?;
    println!(
        "loaded {} samples ({:.3} s) from {}",
        buffer.samples().len(),
        buffer.duration_seconds(),
        path.display()
    );
    let needed = mode.symbol_size_samples();
    println!(
        "decoding under mode {} (one symbol = {} samples, frame-mode: {})",
        mode.short_name(),
        needed,
        frame_mode.short_name(),
    );
    let (start, decoded) =
        decode_one_symbol_with_offset(mode, buffer.samples(), frame_mode)
            .map_err(|e| e.to_string())?;
    if let Some(s) = start {
        println!("  preamble detected at sample {s}");
    }
    println!("decoded {} byte(s):", decoded.len());
    println!("  hex:  {}", hex(&decoded));
    println!("  text: {}", lossy_utf8(&decoded));

    if let Some(exp_arg) = expected {
        let exp = resolve_expected(exp_arg).map_err(|e| e.to_string())?;
        let report = compute_ber(&exp, &decoded);
        println!(
            "BER vs --expected ({}): {}/{} bit errors  →  {:.4e}",
            exp_arg,
            report.bit_errors,
            report.bits_compared,
            report.ber()
        );
        if report.len_mismatch() {
            println!(
                "  length mismatch: expected {} byte(s), decoded {} byte(s)",
                report.expected_len, report.decoded_len
            );
        }
        if report.is_clean() {
            println!("  result: CLEAN MATCH");
        } else {
            println!("  result: MISMATCH");
            return Err(format!(
                "decode did not match --expected ({} bit errors, lengths {}/{})",
                report.bit_errors, report.expected_len, report.decoded_len
            ));
        }
    }
    Ok(())
}

fn run_record_wav(
    path: &std::path::Path,
    device: &str,
    duration: Duration,
) -> Result<(), String> {
    let mut input = AudioInput::open(Some(device))
        .map_err(|e| format!("opening input device {device:?}: {e}"))?;
    let channels = input.channels();
    let resolved = input
        .device_name()
        .unwrap_or_else(|_| device.to_string());

    let abort = Arc::new(AtomicBool::new(false));
    install_signal_flag(libc::SIGINT, Arc::clone(&abort))?;
    install_signal_flag(libc::SIGTERM, Arc::clone(&abort))?;

    println!(
        "recording {} s from '{}' ({} channel(s)) → {}",
        duration.as_secs(),
        resolved,
        channels,
        path.display(),
    );
    println!("(press Ctrl-C to stop early)");

    let (outcome, buffer) =
        record_to_wav(&mut input, duration, &abort, path).map_err(|e| e.to_string())?;
    match outcome {
        RecordOutcome::Completed => println!(
            "recorded {} samples ({:.3} s) → {}",
            buffer.samples().len(),
            buffer.duration_seconds(),
            path.display()
        ),
        RecordOutcome::Aborted => println!(
            "aborted early; wrote {} samples ({:.3} s) → {}",
            buffer.samples().len(),
            buffer.duration_seconds(),
            path.display()
        ),
    }
    Ok(())
}

fn install_signal_flag(sig: libc::c_int, flag: Arc<AtomicBool>) -> Result<(), String> {
    use std::sync::OnceLock;
    static FLAG_SLOT: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let _ = FLAG_SLOT.set(flag);

    extern "C" fn handler(_: libc::c_int) {
        if let Some(f) = FLAG_SLOT.get() {
            f.store(true, Ordering::Release);
        }
    }

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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn lossy_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

const USAGE: &str = "\
tuxmodem-rx — capture + demod + BER (Phase 4, tuxlink-xvrb)

USAGE:
    tuxmodem-rx --list-devices
        Enumerate CPAL input devices and their 48 kHz f32 support.

    tuxmodem-rx --decode-wav <PATH> [--mode <NAME>] [--expected <ARG>]
        Read a WAV (48 kHz f32 mono); decode the first OFDM-symbol
        worth of samples; print bytes (+ BER vs --expected if given).

    tuxmodem-rx --record-wav <PATH> --device <NAME> --duration <SECS>
        Capture N seconds of audio to a WAV file. Ctrl-C aborts early
        (writes the partial buffer).

OPTIONS:
        --list-devices            enumerate input devices
        --decode-wav <PATH>       WAV to demodulate
        --record-wav <PATH>       WAV to capture into
        --mode <NAME>             PHY mode (default: wide-floor)
        --frame-mode <NAME>       frame format: raw / sync / multi-sync (default: raw)
                                  - raw:        first symbol_size samples, no preamble
                                  - sync:       find preamble, decode single symbol (≤9 bytes)
                                  - multi-sync: find preamble, decode N symbols via
                                                length-prefix header (up to u16::MAX bytes)
        --expected <ARG>          expected payload (text or @file) for BER
    -d, --device <NAME>           CPAL input device name (for --record-wav)
        --duration <SECS>         capture duration in seconds (for --record-wav)
    -h, --help                    this usage

EXAMPLES:
    # Discover available input devices:
    tuxmodem-rx --list-devices

    # Decode a captured symbol; report BER against a known expected payload:
    tuxmodem-rx --decode-wav captured.wav --expected \"TEST\"

    # Decode a multi-symbol payload with preamble alignment:
    tuxmodem-rx --decode-wav off-air.wav --expected \"LONG-MESSAGE\" \\
                --frame-mode multi-sync

    # Capture 10 s of audio from the Digirig's input:
    tuxmodem-rx --record-wav off-air.wav --device 'USB Audio Device' --duration 10

SAFETY:
    Pure capture — no PTT, no audio output, no transmission. RADIO-1
    does not gate this binary; agents may run any mode end-to-end.

    The decode path takes the FIRST symbol-sized chunk of the WAV
    (currently 2560 samples for the wide-floor mode). Without frame
    sync (PHY Phase 12+) longer files are NOT scanned — pad/trim to
    the symbol of interest before decoding.
";
