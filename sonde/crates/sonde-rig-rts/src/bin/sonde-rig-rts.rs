//! sonde-rig-rts — CLI for the serial-RTS PTT primitive.
//!
//! Operator-facing tool for bench-validating the RTS-PTT path
//! against real hardware — Digirig Mobile + Xiegu G90 in the
//! reference setup. Three subcommands:
//!
//! ```text
//! sonde-rig-rts toggle  --device <path>                — assert briefly then release
//! sonde-rig-rts assert  --device <path> --duration <s> — assert for at most N seconds
//! sonde-rig-rts release --device <path>                — release explicitly
//! ```
//!
//! ## Safety
//!
//! `assert --duration` is the bounded-airtime primitive, hard-capped
//! at 30 seconds. Longer asserts require the watchdog daemon (Phase
//! 1.5 of tuxlink-9ggl) running in a separate process so a SIGKILL
//! on this CLI cannot leave RTS stuck (which would key the radio
//! until power-cycled).
//!
//! SIGINT and SIGTERM trigger an explicit release before the
//! process exits. SIGKILL is uncatchable — Drop won't run, the
//! kernel-side serial driver MAY drop modem lines on fd close
//! depending on the driver, but we don't rely on that.
//!
//! RADIO-1: this CLI does NOT emit any audio. It only toggles the
//! RTS line. The operator is the licensee; this tool exists to
//! verify the PTT plumbing works before audio is wired in.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sonde_rig_rts::{LinuxTty, Ptt, PttState, RtsError, RtsPtt};

// Wrap the generic RtsPtt at the binary boundary with the real
// Linux tty backend.
type LinuxRtsPtt = RtsPtt<LinuxTty>;

const HARD_CAP_SECS: u64 = 30;

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
            "--help" | "-h" => parsed.help = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(parsed)
}

fn run(parsed: Parsed) -> Result<(), CliError> {
    let cmd = parsed.cmd.ok_or(CliError::MissingCommand)?;
    let device = parsed.device.ok_or(CliError::MissingDevice)?;

    let tty = LinuxTty::open(&device).map_err(CliError::Rts)?;
    let mut ptt = LinuxRtsPtt::new(tty).map_err(CliError::Rts)?;

    match cmd {
        Cmd::Release => {
            ptt.release().map_err(CliError::Rts)?;
            println!("released RTS on {device}");
        }
        Cmd::Toggle => {
            // Assert briefly so the radio's TX LED visibly flashes.
            ptt.assert().map_err(CliError::Rts)?;
            std::thread::sleep(Duration::from_millis(250));
            ptt.release().map_err(CliError::Rts)?;
            println!("toggled RTS on {device} (~250ms)");
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
            assert_with_signal_handling(&mut ptt, &device, secs)?;
        }
    }

    Ok(())
}

/// Assert RTS for `secs` seconds, with SIGINT/SIGTERM cleanly
/// releasing before exit. Drop is the backstop for panic-unwind
/// paths; this function adds the signal layer.
fn assert_with_signal_handling(
    ptt: &mut LinuxRtsPtt,
    device: &str,
    secs: u64,
) -> Result<(), CliError> {
    let signaled = Arc::new(AtomicBool::new(false));

    install_signal_flag(libc::SIGINT, Arc::clone(&signaled))?;
    install_signal_flag(libc::SIGTERM, Arc::clone(&signaled))?;

    ptt.assert().map_err(CliError::Rts)?;
    println!(
        "asserted RTS on {device} — auto-release in {secs}s or on SIGINT/SIGTERM",
    );

    let deadline = Instant::now() + Duration::from_secs(secs);
    let poll = Duration::from_millis(50);
    while Instant::now() < deadline {
        if signaled.load(Ordering::Acquire) {
            println!("signal received → releasing RTS early");
            break;
        }
        std::thread::sleep(poll);
    }

    ptt.release().map_err(CliError::Rts)?;
    println!("released RTS on {device}");
    debug_assert_eq!(ptt.state(), PttState::Released);
    Ok(())
}

/// Install a signal handler that sets the shared atomic. The handler
/// is async-signal-safe (a single relaxed atomic store). The flag is
/// process-global because signal handlers can't capture state.
fn install_signal_flag(sig: libc::c_int, flag: Arc<AtomicBool>) -> Result<(), CliError> {
    use std::sync::OnceLock;
    static FLAG_SLOT: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let _ = FLAG_SLOT.set(flag);

    extern "C" fn handler(_: libc::c_int) {
        if let Some(f) = FLAG_SLOT.get() {
            f.store(true, Ordering::Release);
        }
    }

    // SAFETY: `libc::signal` races against concurrent signal
    // delivery during process startup. We call from main() before
    // any second thread exists, so single-threaded reasoning suffices.
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
    Rts(RtsError),
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
            Self::Rts(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CliError {}

const USAGE: &str = "\
sonde-rig-rts — serial-RTS PTT control for Digirig / SignaLink-RTS / CP2102-class adapters

USAGE:
    sonde-rig-rts <COMMAND> --device <PATH> [OPTIONS]

COMMANDS:
    toggle              assert RTS briefly (~250 ms) then release — LED-flash sanity check
    assert              assert RTS for --duration seconds (auto-release at deadline)
    release             release RTS explicitly

OPTIONS:
    -d, --device <PATH>     path to the tty device (e.g. /dev/digirig or /dev/ttyUSB0)
        --duration <SECS>   for `assert`: how long to hold (1..=30)
    -h, --help              this usage

EXAMPLES:
    # Bench sanity check — G90 should briefly key for ~250 ms:
    sonde-rig-rts toggle --device /dev/digirig

    # Key the radio for up to 5 seconds (Ctrl+C aborts early + releases):
    sonde-rig-rts assert --device /dev/digirig --duration 5

SAFETY:
    --duration is hard-capped at 30 seconds. Beyond that, use the
    watchdog daemon (tuxlink-9ggl Phase 1.5) which owns the tty
    fd in a separate process so a SIGKILL on the modem cannot
    leave RTS asserted (which would key the radio until
    power-cycled).

    SIGINT/SIGTERM during an assert trigger an explicit release.
    SIGKILL is uncatchable — Drop won't run; rely on the watchdog
    daemon for SIGKILL-safe operation.

    On open, both RTS and DTR are explicitly cleared via TIOCMBIC
    BEFORE any state-changing op — this defuses the historical
    Linux failure mode where opening a tty asserts DTR (and
    sometimes RTS) by default.
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
        let p = parse_args(&s(&["toggle", "--device", "/dev/digirig"])).unwrap();
        assert_eq!(p.cmd, Some(Cmd::Toggle));
        assert_eq!(p.device.as_deref(), Some("/dev/digirig"));
    }

    #[test]
    fn parse_assert_with_duration() {
        let p = parse_args(&s(&[
            "assert",
            "--device",
            "/dev/digirig",
            "--duration",
            "5",
        ]))
        .unwrap();
        assert_eq!(p.cmd, Some(Cmd::Assert));
        assert_eq!(p.duration_secs, Some(5));
    }

    #[test]
    fn parse_release_minimal() {
        let p = parse_args(&s(&["release", "-d", "/dev/digirig"])).unwrap();
        assert_eq!(p.cmd, Some(Cmd::Release));
        assert_eq!(p.device.as_deref(), Some("/dev/digirig"));
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
}
