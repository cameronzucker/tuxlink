//! tux-rig-cm108 — CLI for the CM108 PTT primitive.
//!
//! Operator-facing tool for bench-validating the CM108 PTT path
//! against real hardware (Masters Communications DRA-100-DIN6 +
//! whatever radio the operator has wired). Three subcommands:
//!
//! ```text
//! tux-rig-cm108 toggle  --device <path>                — assert then immediately release
//! tux-rig-cm108 assert  --device <path> --duration <s> — assert for at most N seconds
//! tux-rig-cm108 release --device <path>                — release explicitly
//! ```
//!
//! ## Safety
//!
//! `assert --duration` is the bounded-airtime primitive. The CLI
//! hard-caps the duration at 30 seconds — exceeding that requires the
//! future watchdog daemon (separate process) per the bench-rig spec's
//! defense-in-depth list.
//!
//! Signal handling: SIGINT and SIGTERM trigger an explicit release
//! before the process exits. SIGKILL is uncatchable; the chip's
//! state-latching behavior means a SIGKILL during assert leaves PTT
//! stuck. Use the future watchdog process for SIGKILL-safe operation.
//!
//! RADIO-1: this CLI does NOT emit any audio. It only toggles the
//! PTT line. The operator is the licensee initiating any actual
//! transmission; this tool exists to verify the PTT plumbing works
//! before audio is wired in.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tux_rig_cm108::{Cm108Error, GpioPin, HidrawWriter, Ptt, PttState};

// Wrap the generic Cm108Ptt at the binary boundary with the real
// hidraw writer.
type Cm108 = tux_rig_cm108::writer::Cm108Ptt<HidrawWriter>;

const HARD_CAP_SECS: u64 = 30;
const DEFAULT_PIN: u8 = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    if parsed.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    match run(parsed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug)]
struct Parsed {
    cmd: Option<Cmd>,
    device: Option<String>,
    duration_secs: Option<u64>,
    pin: u8,
    help: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmd {
    Toggle,
    Assert,
    Release,
}

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut parsed = Parsed {
        cmd: None,
        device: None,
        duration_secs: None,
        pin: DEFAULT_PIN,
        help: false,
    };
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "toggle" => parsed.cmd = Some(Cmd::Toggle),
            "assert" => parsed.cmd = Some(Cmd::Assert),
            "release" => parsed.cmd = Some(Cmd::Release),
            "--device" | "-d" => {
                parsed.device = Some(
                    iter.next()
                        .ok_or_else(|| "--device requires a path".to_string())?
                        .clone(),
                );
            }
            "--duration" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--duration requires a value in seconds".to_string())?;
                parsed.duration_secs = Some(
                    v.parse::<u64>()
                        .map_err(|_| format!("--duration must be an integer: {v}"))?,
                );
            }
            "--pin" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--pin requires a value 1..=8".to_string())?;
                parsed.pin = v
                    .parse::<u8>()
                    .map_err(|_| format!("--pin must be 1..=8: {v}"))?;
            }
            "--help" | "-h" => parsed.help = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(parsed)
}

fn run(parsed: Parsed) -> Result<(), CliError> {
    let cmd = parsed.cmd.ok_or(CliError::MissingCommand)?;
    let device = parsed.device.ok_or(CliError::MissingDevice)?;
    let pin = GpioPin::new(parsed.pin).map_err(CliError::Cm108)?;

    let writer = HidrawWriter::open(&device).map_err(CliError::Cm108)?;
    let mut ptt = Cm108::new(writer, pin);

    match cmd {
        Cmd::Release => {
            ptt.release().map_err(CliError::Cm108)?;
            println!("released PTT on {device} (pin {})", pin.number());
        }
        Cmd::Toggle => {
            // Assert briefly so the LED visibly flashes; the Drop on
            // ptt's exit from this scope releases.
            ptt.assert().map_err(CliError::Cm108)?;
            std::thread::sleep(Duration::from_millis(250));
            ptt.release().map_err(CliError::Cm108)?;
            println!("toggled PTT on {device} (pin {}, ~250ms)", pin.number());
        }
        Cmd::Assert => {
            let secs = parsed.duration_secs.ok_or(CliError::MissingDuration)?;
            if secs == 0 {
                return Err(CliError::ZeroDuration);
            }
            if secs > HARD_CAP_SECS {
                return Err(CliError::DurationExceedsCap {
                    asked: secs,
                    cap: HARD_CAP_SECS,
                });
            }
            assert_with_signal_handling(&mut ptt, &device, pin, secs)?;
        }
    }

    Ok(())
}

/// Assert PTT for `secs` seconds, with SIGINT/SIGTERM cleanly
/// releasing before exit. Drop is the backstop for panic-unwind
/// paths; this function adds the signal layer that Drop alone
/// cannot cover.
fn assert_with_signal_handling(
    ptt: &mut Cm108,
    device: &str,
    pin: GpioPin,
    secs: u64,
) -> Result<(), CliError> {
    let signaled = Arc::new(AtomicBool::new(false));

    // Install SIGINT + SIGTERM handlers. Each handler ONLY sets the
    // atomic flag — the main thread polls it and does the release in
    // the normal control flow. (Calling write(2) on a hidraw fd from
    // a real signal handler is technically allowed since write() is
    // async-signal-safe, but routing through a flag avoids the
    // Rust-borrowing concerns + makes Drop a sufficient backstop.)
    install_signal_flag(libc::SIGINT, Arc::clone(&signaled))?;
    install_signal_flag(libc::SIGTERM, Arc::clone(&signaled))?;

    ptt.assert().map_err(CliError::Cm108)?;
    println!(
        "asserted PTT on {device} (pin {}) — auto-release in {secs}s or on SIGINT/SIGTERM",
        pin.number()
    );

    let deadline = Instant::now() + Duration::from_secs(secs);
    let poll = Duration::from_millis(50);
    while Instant::now() < deadline {
        if signaled.load(Ordering::Acquire) {
            println!("signal received → releasing PTT early");
            break;
        }
        std::thread::sleep(poll);
    }

    ptt.release().map_err(CliError::Cm108)?;
    println!("released PTT on {device}");
    // Verify post-release state for diagnostic clarity.
    debug_assert_eq!(ptt.state(), PttState::Released);
    Ok(())
}

/// Install a signal handler that just sets `flag` to true. The
/// handler is installed via `signal(2)`'s minimal interface
/// (`libc::signal`) — async-signal-safe and adequate for a CLI that
/// only needs "stop the sleep loop on signal".
fn install_signal_flag(sig: libc::c_int, flag: Arc<AtomicBool>) -> Result<(), CliError> {
    use std::sync::OnceLock;
    static FLAG_SLOT: OnceLock<Arc<AtomicBool>> = OnceLock::new();

    // The signal handler is a plain `extern "C" fn` with no captures,
    // so we route the flag through a process-global slot. The slot
    // is initialized on the first install_signal_flag call; both
    // SIGINT and SIGTERM share the same atomic, which is exactly
    // what we want (either signal means "release now").
    let _ = FLAG_SLOT.set(flag);

    extern "C" fn handler(_: libc::c_int) {
        if let Some(f) = FLAG_SLOT.get() {
            f.store(true, Ordering::Release);
        }
    }

    // SAFETY: libc::signal is unsafe because handler-registration
    // races with concurrent signal delivery during process startup.
    // We call it from main() before any second thread exists.
    //
    // The double cast (`*const ()` first, then `sighandler_t`) is
    // required by the Rust 2026 `function_casts_as_integer` lint
    // (function items aren't pointer-sized in the abstract machine;
    // an explicit `*const ()` hop makes the integer conversion legal).
    let prev = unsafe { libc::signal(sig, handler as *const () as libc::sighandler_t) };
    if prev == libc::SIG_ERR {
        return Err(CliError::SignalInstall {
            signal: sig,
            errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
        });
    }
    Ok(())
}

#[derive(Debug)]
enum CliError {
    MissingCommand,
    MissingDevice,
    MissingDuration,
    ZeroDuration,
    DurationExceedsCap { asked: u64, cap: u64 },
    SignalInstall { signal: libc::c_int, errno: i32 },
    Cm108(Cm108Error),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCommand => write!(f, "missing subcommand (toggle | assert | release)"),
            Self::MissingDevice => write!(f, "missing --device <path>"),
            Self::MissingDuration => write!(f, "assert requires --duration <seconds>"),
            Self::ZeroDuration => write!(f, "--duration 0 makes no sense; use `release` instead"),
            Self::DurationExceedsCap { asked, cap } => write!(
                f,
                "--duration {asked} exceeds the {cap}-second hard cap. For longer asserts use the watchdog daemon (tuxlink-9ggl Phase 1.5).",
            ),
            Self::SignalInstall { signal, errno } => {
                write!(f, "signal({signal}) install failed: errno={errno}")
            }
            Self::Cm108(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CliError {}

const USAGE: &str = "\
tux-rig-cm108 — CM108-family USB-HID PTT control

USAGE:
    tux-rig-cm108 <COMMAND> --device <PATH> [OPTIONS]

COMMANDS:
    toggle              assert PTT briefly (~250 ms) then release — LED-flash sanity check
    assert              assert PTT for --duration seconds (auto-release at deadline)
    release             release PTT explicitly

OPTIONS:
    -d, --device <PATH>     path to the hidraw device (e.g. /dev/dra100-ptt or /dev/hidraw3)
        --duration <SECS>   for `assert`: how long to hold (1..=30)
        --pin <N>           CM108 GPIO pin number 1..=8 (default: 3 — DRA-100-DIN6 convention)
    -h, --help              this usage

EXAMPLES:
    # Bench sanity check — DRA-100 RED LED should flash for ~250 ms:
    tux-rig-cm108 toggle --device /dev/dra100-ptt

    # Key the radio for up to 5 seconds (Ctrl+C aborts early + releases):
    tux-rig-cm108 assert --device /dev/dra100-ptt --duration 5

SAFETY:
    --duration is hard-capped at 30 seconds. Beyond that, use the
    watchdog daemon (tuxlink-9ggl Phase 1.5) which owns the hidraw
    fd in a separate process so a SIGKILL on the modem cannot
    leave PTT stuck.

    SIGINT/SIGTERM during an assert trigger an explicit release.
    SIGKILL is uncatchable — Drop won't run, the chip latches the
    asserted state, and the radio keeps transmitting until power-
    cycled or the watchdog daemon force-releases.
";

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_toggle_with_device() {
        let p = parse_args(&s(&["toggle", "--device", "/dev/hidraw0"])).unwrap();
        assert_eq!(p.cmd, Some(Cmd::Toggle));
        assert_eq!(p.device.as_deref(), Some("/dev/hidraw0"));
        assert_eq!(p.pin, 3);
    }

    #[test]
    fn parse_assert_with_duration_and_custom_pin() {
        let p = parse_args(&s(&[
            "assert",
            "--device",
            "/dev/dra100-ptt",
            "--duration",
            "5",
            "--pin",
            "1",
        ]))
        .unwrap();
        assert_eq!(p.cmd, Some(Cmd::Assert));
        assert_eq!(p.duration_secs, Some(5));
        assert_eq!(p.pin, 1);
    }

    #[test]
    fn parse_release_minimal() {
        let p = parse_args(&s(&["release", "-d", "/dev/dra100-ptt"])).unwrap();
        assert_eq!(p.cmd, Some(Cmd::Release));
        assert_eq!(p.device.as_deref(), Some("/dev/dra100-ptt"));
    }

    #[test]
    fn parse_help_flag() {
        let p = parse_args(&s(&["--help"])).unwrap();
        assert!(p.help);
    }

    #[test]
    fn parse_rejects_unknown_arg() {
        let err = parse_args(&s(&["toggle", "--gibberish"])).unwrap_err();
        assert!(err.contains("unknown argument"));
    }

    #[test]
    fn parse_rejects_duration_without_value() {
        let err = parse_args(&s(&["assert", "--device", "/dev/x", "--duration"])).unwrap_err();
        assert!(err.contains("--duration"));
    }

    #[test]
    fn parse_rejects_non_numeric_duration() {
        let err = parse_args(&s(&["assert", "-d", "/dev/x", "--duration", "forever"]))
            .unwrap_err();
        assert!(err.contains("--duration"));
    }

    #[test]
    fn parse_rejects_pin_out_of_byte_range() {
        let err = parse_args(&s(&["toggle", "-d", "/dev/x", "--pin", "999"])).unwrap_err();
        assert!(err.contains("--pin"));
    }
}
