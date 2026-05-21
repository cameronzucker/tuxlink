//! Operator-only live-CMS smoke test (tuxlink-nk7, v0.0.1 plan Task 6).
//!
//! ⚠️ NEVER invoked by `cargo test`, NEVER run by CI, NEVER run by an AI agent
//! in a subagent shell. This binary TRANSMITS under the operator's amateur
//! radio callsign; each invocation requires explicit operator consent at
//! runtime via stdin. See docs/live-cms-testing-policy.md and
//! docs/pitfalls/implementation-pitfalls.md §0 (RADIO-1).
//!
//! It is a dedicated binary at `src-tauri/src/bin/live_cms_smoke.rs` — NOT a
//! file under `src-tauri/tests/` — precisely so `cargo test` cannot discover
//! or run it. `main` is never executed by the test harness.
//!
//! ## Credentials (AMD-12 — keyring, not env-var password)
//!
//! Per the 2026-05-18 cred-handling refactor, the Winlink password lives in the
//! OS keyring at `(service="tuxlink-pat", account=<NORMALIZED-CALLSIGN>)`, NOT
//! in an env var and NOT in Pat's config. The wizard (Task 9/10) writes that
//! entry. This binary VERIFIES the entry exists (and the keyring is reachable
//! and unlocked) BEFORE any transmission; if it is missing / locked /
//! unavailable, the binary prints an operator-actionable message and exits
//! non-zero with NO transmission — never a half-authenticated connect.
//!
//! Pat reads the same keyring entry itself at CMS-session time; this binary
//! does not pass the password anywhere — it only confirms it is present.
//!
//! ## Usage (operator runs this manually)
//!
//!   # One-time: run the tuxlink wizard so config.json + keyring entry exist.
//!   # Or set the keyring entry directly:
//!   #   secret-tool store --label='tuxlink-pat WL2K' service tuxlink-pat account <CALLSIGN>
//!   cargo run --bin live_cms_smoke
//!
//! Optional: `PAT_BINARY=/path/to/pat` overrides the default `pat`-on-PATH.

use std::io::{stdin, stdout, Write};
use std::path::PathBuf;
use std::process::exit;
use std::time::{Duration, Instant};

use tuxlink_lib::config::{self, CmsTransport, Config};
use tuxlink_lib::consent_gate::{check_consent, ConsentOutcome, TransmissionPlan};
use tuxlink_lib::pat_client::{MailboxFolder, PatClient};
use tuxlink_lib::pat_process::{PatProcess, PatSpawnOptions};

const KEYRING_SERVICE: &str = "tuxlink-pat";
const TARGET: &str = "SERVICE@winlink.org";

/// Keyring-precondition error categories, mapped from the `keyring` crate's
/// error variants per the cred-handling spec (§3.5 / §5.2). Each maps to an
/// operator-actionable message and a non-zero exit; NONE proceeds to transmit.
enum KeyringPrecondition {
    /// No entry for this callsign — operator never set credentials.
    Missing,
    /// Secret Service is reachable but locked (e.g. gnome-keyring locked).
    Locked,
    /// Secret Service unreachable / not installed / D-Bus down, or any other
    /// unclassified keyring failure (incl. multiple ambiguous entries).
    Unavailable(String),
}

/// Verify a keyring password entry exists for `callsign` and is readable.
/// Returns `Ok(())` to proceed; `Err(category)` to abort with no transmission.
///
/// This performs a READ (`get_password`); the mapping differs from the wizard's
/// write-path `map_keyring_error` in one key respect: `NoEntry` on a READ means
/// "no credential stored for this callsign" (the Missing category), whereas on
/// a write it would be unexpected. We deliberately keep this mapping local to
/// the read-precondition rather than reusing the wizard's write-path mapper.
fn verify_keyring_entry(callsign: &str) -> Result<(), KeyringPrecondition> {
    let entry = match keyring::Entry::new(KEYRING_SERVICE, callsign) {
        Ok(e) => e,
        // Constructing the entry handle failed — the backend itself is the
        // problem (Secret Service unreachable, etc.).
        Err(e) => return Err(KeyringPrecondition::Unavailable(format!("{e}"))),
    };
    match entry.get_password() {
        Ok(pw) if !pw.is_empty() => Ok(()),
        // An empty stored password is treated as a miss (per cred-handling spec
        // §3.6 TestGet_EmptyStoredTreatedAsMiss): there is no usable credential.
        Ok(_) => Err(KeyringPrecondition::Missing),
        Err(keyring::Error::NoEntry) => Err(KeyringPrecondition::Missing),
        Err(keyring::Error::NoStorageAccess(ref inner)) => {
            let msg = format!("{inner}").to_lowercase();
            if msg.contains("locked") {
                Err(KeyringPrecondition::Locked)
            } else {
                Err(KeyringPrecondition::Unavailable(format!("{inner}")))
            }
        }
        Err(other) => Err(KeyringPrecondition::Unavailable(format!("{other}"))),
    }
}

/// Print the operator-actionable remedy for a failed keyring precondition.
fn print_keyring_remedy(callsign: &str, cat: &KeyringPrecondition) {
    match cat {
        KeyringPrecondition::Missing => {
            eprintln!(
                "ERROR: no Winlink password found in the OS keyring for callsign {callsign}.\n\
                 No transmission occurred.\n\n\
                 Set credentials one of these ways, then re-run:\n  \
                 1. Run the tuxlink onboarding wizard (writes the keyring entry for you), or\n  \
                 2. Store it directly:\n     \
                 secret-tool store --label='tuxlink-pat WL2K' service {service} account {callsign}\n\n\
                 (The keyring account must be the UPPER-CASE, whitespace-trimmed callsign.)",
                service = KEYRING_SERVICE,
            );
        }
        KeyringPrecondition::Locked => {
            eprintln!(
                "ERROR: the OS keyring is LOCKED; cannot read the Winlink password for {callsign}.\n\
                 No transmission occurred.\n\n\
                 Unlock your keyring (log in to your desktop session / unlock the default\n\
                 collection in Seahorse or your keyring manager) and re-run."
            );
        }
        KeyringPrecondition::Unavailable(detail) => {
            eprintln!(
                "ERROR: the OS keyring (freedesktop Secret Service) is UNAVAILABLE; cannot read\n\
                 the Winlink password for {callsign}. No transmission occurred.\n\n\
                 Ensure a Secret Service provider is running (gnome-keyring-daemon / KWallet with\n\
                 the Secret Service interface) and the D-Bus session bus is reachable, then re-run.\n\n\
                 Detail: {detail}"
            );
        }
    }
}

/// `~/.config/pat/config.json` (honoring XDG_CONFIG_HOME) — where PatProcess
/// WRITES the rendered Pat config before exec.
fn pat_config_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("neither XDG_CONFIG_HOME nor HOME is set")?;
    Ok(base.join("pat").join("config.json"))
}

/// `~/.local/share/tuxlink/...` (honoring XDG_DATA_HOME).
fn data_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .ok_or("neither XDG_DATA_HOME nor HOME is set")?;
    Ok(base.join("tuxlink"))
}

/// `~/.local/state/tuxlink/...` (honoring XDG_STATE_HOME).
fn state_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
        .ok_or("neither XDG_STATE_HOME nor HOME is set")?;
    Ok(base.join("tuxlink"))
}

#[tokio::main]
async fn main() {
    // 1. Load config (callsign / grid / transport). NO env-var password — the
    //    password lives in the OS keyring (AMD-12) and is verified below.
    let config: Config = match config::read_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: cannot read tuxlink config: {e}");
            eprintln!("Run the tuxlink onboarding wizard first. No transmission occurred.");
            exit(2);
        }
    };

    if !config.connect.connect_to_cms {
        eprintln!(
            "ERROR: config has connect_to_cms = false (offline-only deployment).\n\
             The live-CMS smoke test requires a CMS-enabled config with a callsign.\n\
             No transmission occurred."
        );
        exit(2);
    }

    // Callsign is required + present on the CMS path (config.validate enforces
    // this), but guard explicitly. Normalize to the keyring account convention
    // (TrimSpace + ToUpper) so the lookup matches what the wizard wrote.
    let callsign = match config.identity.callsign.as_deref() {
        Some(c) if !c.trim().is_empty() => c.trim().to_uppercase(),
        _ => {
            eprintln!("ERROR: config has no identity.callsign on the CMS path. No transmission occurred.");
            exit(2);
        }
    };

    let transport = match config.connect.transport {
        CmsTransport::CmsSsl => "CMS-SSL (telnet over TLS)",
        CmsTransport::Telnet => "telnet (plaintext)",
    };

    let start_instant = Instant::now();
    let start_utc = chrono::Utc::now();

    // 2. Keyring precondition (AMD-12): verify the password entry exists and is
    //    readable BEFORE any transmission. Abort with exit(2), no transmission,
    //    on missing / locked / unavailable.
    if let Err(cat) = verify_keyring_entry(&callsign) {
        print_keyring_remedy(&callsign, &cat);
        log_session(&start_utc, &callsign, 1, 0, "aborted-keyring-precondition", start_instant.elapsed());
        exit(2);
    }

    // 3. Build the scoped transmission plan from the config.
    let plan = TransmissionPlan {
        target: TARGET.into(),
        session_count: 1,
        // Honest worst-case airtime: /api/connect may hold the CMS session for
        // up to the 60s connect cap in run_smoke (tuxlink-22l).
        expected_duration_s: 60,
        content: format!(
            "tuxlink live_cms_smoke {} (v0.0.1 verification; transport={})",
            start_utc.to_rfc3339(),
            transport
        ),
        freq_mode_band: "telnet over IP; no RF".into(),
        callsign: callsign.clone(),
    };

    // 4. Consent gate. Proceed ONLY on exact "go".
    match check_consent(&plan, stdin().lock(), stdout().lock()) {
        ConsentOutcome::Granted => { /* proceed */ }
        ConsentOutcome::Aborted => {
            log_session(&start_utc, &callsign, plan.session_count, 0, "aborted-by-operator", start_instant.elapsed());
            exit(2);
        }
    }

    // 5. Run the live round-trip (spawn Pat, send, connect, poll, shutdown).
    let outcome = run_smoke(&config, &plan).await;
    let elapsed = start_instant.elapsed();

    match outcome {
        Ok(actual_sessions) => {
            println!("\nOK: received reply from {}", plan.target);
            log_session(&start_utc, &callsign, plan.session_count, actual_sessions, "success", elapsed);
        }
        Err(e) => {
            eprintln!("\nFAIL: {e}");
            log_session(&start_utc, &callsign, plan.session_count, 0, "failed", elapsed);
            exit(1);
        }
    }
}

/// Spawn Pat, post the test message to the outbox, trigger a telnet connect,
/// poll the inbox for a reply from the target, then shut Pat down. Returns the
/// actual session count (1 on a received reply).
///
/// Pat's config (mycall + locator) is rendered from `config` by
/// `PatProcess::spawn` — this fn passes NO password (it lives in the keyring,
/// which Pat reads itself at CMS-session time, per AMD-12).
async fn run_smoke(config: &Config, plan: &TransmissionPlan) -> Result<u32, String> {
    let config_path = pat_config_path()?;
    let mbox_dir = data_dir()?.join("mbox");
    let pid_file = state_dir()?.join("pat.pid");

    // Resolve the Pat binary: PAT_BINARY override, else "pat" on PATH.
    let binary = std::env::var_os("PAT_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("pat"));

    // Stream Pat's own stderr to the console (prefixed `[pat]`) so a failed
    // connect surfaces Pat's reason (CMS unreachable, port blocked, auth, …)
    // instead of an opaque client-side error. The reader thread ends when Pat's
    // stderr closes (on shutdown). (tuxlink-22l: the prior `log_sink: None`
    // discarded all of this, making the round-trip a black box.)
    let (log_tx, log_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in log_rx {
            eprintln!("[pat] {line}");
        }
    });

    let opts = PatSpawnOptions {
        binary,
        config_path,
        mbox_dir,
        http_listen_port: 0, // ephemeral; PatProcess pre-binds + reports the port
        pid_file,
        log_sink: Some(log_tx),
        tuxlink_config: config.clone(),
        // Same 10s announce deadline the app/bootstrap uses (pat_process.rs
        // §adrev #7); live_cms_smoke spawns a real Pat the same way the app does.
        http_announce_timeout: Duration::from_secs(10),
    };
    let mut proc = PatProcess::spawn(opts).map_err(|e| format!("spawn pat: {e}"))?;
    let port = proc.http_port();
    let client = PatClient::new(format!("http://127.0.0.1:{port}"));

    // Post the test message to the outbox. `send` takes an RFC3339 date.
    let now_rfc3339 = chrono::Utc::now().to_rfc3339();
    if let Err(e) = client
        .send(&[plan.target.as_str()], "tuxlink setup test", &plan.content, &now_rfc3339)
        .await
    {
        let _ = proc.shutdown(Duration::from_secs(5));
        return Err(format!("send to outbox: {e}"));
    }

    // Trigger a telnet connect via Pat's HTTP API. Pat's `/api/connect` runs the
    // CMS session and MAY block for its full duration, so allow a generous 60s
    // (vs the old 30s, which cut a blocking connect off and reported a bare
    // "trigger connect" error). A request-level error here is NON-FATAL: the
    // session may still have run (Pat's streamed `[pat]` log shows what
    // happened), and the inbox poll below is the authoritative success signal.
    let connect_url = format!("http://127.0.0.1:{port}/api/connect?url=telnet");
    match reqwest::Client::builder().timeout(Duration::from_secs(60)).build() {
        Ok(http) => {
            if let Err(e) = http.post(&connect_url).send().await {
                eprintln!(
                    "[connect] request to {connect_url} did not complete cleanly: {e}\n\
                     [connect] continuing to poll the inbox — see the [pat] log above for the cause"
                );
            }
        }
        Err(e) => {
            let _ = proc.shutdown(Duration::from_secs(5));
            return Err(format!("build connect client: {e}"));
        }
    }

    // Poll the inbox up to 60s for a reply from the target.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match client.list(MailboxFolder::Inbox).await {
            Ok(msgs) => {
                if msgs.iter().any(|m| m.from.contains(TARGET)) {
                    let _ = proc.shutdown(Duration::from_secs(5));
                    return Ok(1);
                }
            }
            Err(e) => {
                // Transient list errors are tolerated until the deadline; a
                // persistent failure surfaces as the timeout error below.
                eprintln!("(transient) list inbox: {e}");
            }
        }
        if Instant::now() > deadline {
            let _ = proc.shutdown(Duration::from_secs(5));
            return Err(format!(
                "no reply from {TARGET} within 60s (see the [pat] log above for what Pat did)"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Append one audit line to `dev/live-cms-sessions.log` on EVERY exit path
/// (consent-abort, keyring-precondition abort, success, fail). UTC ISO-8601
/// timestamp, binary name, callsign, planned + actual session counts, outcome,
/// duration. Best-effort: a logging failure must not mask the run's outcome.
fn log_session(
    start_utc: &chrono::DateTime<chrono::Utc>,
    callsign: &str,
    planned_sessions: u32,
    actual_sessions: u32,
    outcome: &str,
    duration: Duration,
) {
    let line = format!(
        "{ts}  live_cms_smoke  callsign={callsign}  planned_sessions={planned}  actual_sessions={actual}  outcome={outcome}  duration_s={dur}  target={target}\n",
        ts = start_utc.to_rfc3339(),
        planned = planned_sessions,
        actual = actual_sessions,
        dur = duration.as_secs(),
        target = TARGET,
    );
    let path = PathBuf::from("dev").join("live-cms-sessions.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}
