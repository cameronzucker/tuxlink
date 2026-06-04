//! tux-rig-watchdog — SIGKILL-safe PTT release daemon (Phase 1.5, tuxlink-23ps).
//!
//! Holds PTT for up to `--max-seconds` seconds, releasing on whichever
//! of these fires first:
//! - SIGINT / SIGTERM received
//! - stdin closes (EOF detected — the OS closes the pipe automatically
//!   when the parent process dies, INCLUDING under SIGKILL)
//! - max-seconds elapses
//!
//! Designed to be spawned as a child of `tuxmodem-tx` or similar long-
//! transmission orchestrators. The parent pipes whatever it wants
//! through stdin (or just keeps the pipe open); when the parent dies
//! by any means, the OS closes our stdin, we detect EOF, and we
//! release PTT before exit.
//!
//! ## Usage
//!
//! ```text
//! tux-rig-watchdog --device /dev/digirig --max-seconds 30
//! ```
//!
//! Asserts PTT immediately. Releases automatically when any termination
//! condition fires. Logs the outcome to stderr.
//!
//! ## Safety
//!
//! Same RADIO-1 posture as the existing `tux-rig-rts` binary: the
//! operator is the licensee; agent codes + commits + operator runs
//! against the real device.

use std::io::Read;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tux_rig_rts::{
    run_watchdog, LinuxTty, RtsPtt, WatchdogOutcome, HARD_CAP_DURATION,
};

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
    device: Option<String>,
    max_seconds: u64,
    help: bool,
}

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut device: Option<String> = None;
    let mut max_seconds: u64 = 30;
    let mut help = false;
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--device" | "-d" => {
                device = Some(
                    iter.next()
                        .ok_or_else(|| "--device requires a path".to_string())?
                        .clone(),
                );
            }
            "--max-seconds" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--max-seconds requires a value".to_string())?;
                max_seconds = v
                    .parse::<u64>()
                    .map_err(|_| format!("--max-seconds must be an integer: {v}"))?;
                if max_seconds == 0 {
                    return Err("--max-seconds must be > 0".to_string());
                }
                if max_seconds > HARD_CAP_DURATION.as_secs() {
                    return Err(format!(
                        "--max-seconds {} exceeds hard cap of {} seconds",
                        max_seconds,
                        HARD_CAP_DURATION.as_secs()
                    ));
                }
            }
            "--help" | "-h" => help = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Parsed {
        device,
        max_seconds,
        help,
    })
}

fn run(parsed: Parsed) -> Result<(), String> {
    let device = parsed
        .device
        .ok_or_else(|| "missing --device <path>".to_string())?;

    let tty = LinuxTty::open(&device)
        .map_err(|e| format!("opening PTT device {device:?}: {e}"))?;
    let mut ptt = RtsPtt::new(tty).map_err(|e| format!("initializing PTT: {e}"))?;

    // Shared flags. Each watcher thread / handler sets its own.
    let signal = Arc::new(AtomicBool::new(false));
    let stdin_eof = Arc::new(AtomicBool::new(false));

    install_signal_flag(libc::SIGINT, Arc::clone(&signal))?;
    install_signal_flag(libc::SIGTERM, Arc::clone(&signal))?;

    // Stdin watcher thread — blocks on a read; sets the flag when read
    // returns 0 bytes (EOF) or errors. We don't care about the bytes
    // themselves; the existence of a closed pipe is the signal.
    let eof_setter = Arc::clone(&stdin_eof);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut lock = stdin.lock();
        let mut buf = [0u8; 256];
        loop {
            match lock.read(&mut buf) {
                Ok(0) => {
                    // EOF.
                    eof_setter.store(true, Ordering::Release);
                    return;
                }
                Ok(_) => {
                    // Got some bytes; discard. Loop to detect EOF later.
                }
                Err(_) => {
                    // I/O error → treat as EOF (parent's pipe broke).
                    eof_setter.store(true, Ordering::Release);
                    return;
                }
            }
        }
    });

    eprintln!(
        "tux-rig-watchdog: asserting PTT on {} — auto-release in {} s OR on \
         SIGINT/SIGTERM OR on stdin EOF",
        device, parsed.max_seconds
    );

    let max_duration = Duration::from_secs(parsed.max_seconds);
    let outcome = run_watchdog(
        &mut ptt,
        max_duration,
        &stdin_eof,
        &signal,
        Duration::from_millis(20),
    )
    .map_err(|e| format!("watchdog: {e}"))?;

    eprintln!(
        "tux-rig-watchdog: released PTT — reason: {}",
        outcome_label(outcome)
    );
    Ok(())
}

fn outcome_label(o: WatchdogOutcome) -> &'static str {
    match o {
        WatchdogOutcome::MaxDurationElapsed => "max-duration elapsed",
        WatchdogOutcome::StdinEof => "stdin EOF (parent died or closed pipe)",
        WatchdogOutcome::Signaled => "SIGINT/SIGTERM received",
    }
}

/// Install a signal handler that sets the shared atomic. Mirrors the
/// pattern used by `tux-rig-rts`'s CLI: a single async-signal-safe
/// `AtomicBool::store` from the handler, with the [`Arc`] stashed in
/// a process-static [`OnceLock`] (signal handlers can't capture).
fn install_signal_flag(sig: libc::c_int, flag: Arc<AtomicBool>) -> Result<(), String> {
    use std::sync::OnceLock;
    static FLAG_SLOT: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let _ = FLAG_SLOT.set(flag);

    extern "C" fn handler(_: libc::c_int) {
        if let Some(f) = FLAG_SLOT.get() {
            f.store(true, Ordering::Release);
        }
    }

    // SAFETY: libc::signal races against concurrent signal delivery
    // during process startup. We call from main() before the stdin
    // watcher thread spawns, so single-threaded reasoning suffices.
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
tux-rig-watchdog — SIGKILL-safe PTT release daemon (Phase 1.5, tuxlink-23ps)

USAGE:
    tux-rig-watchdog --device <PATH> [--max-seconds <N>]

OPTIONS:
    -d, --device <PATH>          path to the tty device (e.g. /dev/digirig)
        --max-seconds <SECS>     auto-release after N seconds (default 30; hard cap 60)
    -h, --help                   this usage

BEHAVIOR:
    Asserts PTT immediately on startup. Releases PTT and exits when
    ANY of these conditions fire:

    1. --max-seconds elapses
    2. SIGINT or SIGTERM is received
    3. stdin EOF is detected (the OS closes the pipe automatically
       when the parent process dies, INCLUDING under SIGKILL)

    Condition 3 is the whole point. tux-rig-rts's Drop impl releases
    PTT on SIGINT/SIGTERM/panic-unwind, but SIGKILL skips Drop —
    leaving PTT asserted until power-cycle. Spawning this binary as a
    child of tuxmodem-tx (or similar) and piping anything to its stdin
    means: parent dies → OS closes pipe → we see EOF → release PTT.

EXAMPLE (operator spawns directly):
    # In one terminal: hold PTT for 10 seconds; Ctrl+C aborts early.
    tux-rig-watchdog --device /dev/digirig --max-seconds 10

EXAMPLE (parent spawns + pipes; future tuxmodem-tx integration):
    parent_process | tux-rig-watchdog --device /dev/digirig --max-seconds 30
    # When parent_process exits, the OS closes our stdin → EOF → release.

SAFETY (RADIO-1):
    This binary toggles the RTS line — keying the radio in the
    Digirig + SignaLink-RTS class. The operator is the licensee. Per
    project policy, agents may write + test this code but the
    operator runs it against the real radio.

    --max-seconds is hard-capped at 60. The hard cap is in the
    library (tux_rig_rts::HARD_CAP_DURATION); changing it requires a
    code change.
";
