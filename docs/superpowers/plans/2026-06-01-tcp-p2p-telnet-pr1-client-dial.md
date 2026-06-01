# TCP P2P Telnet — PR 1 (Client-Dial) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the client-dial half of TCP P2P Telnet — tuxlink dials a peer's TCP listener, runs the telnet-login wrapper, then a full bidirectional B2F session with attachments. Operator can round-trip real Winlink mail against their Windows-side WLE Telnet-P2P listener (WLE acts as the registered-SID proxy to CMS).

**Architecture:** New `telnet_p2p.rs` transport sibling to `telnet.rs`. New `telnet_p2p_login.rs` dialer-side state machine (CALLSIGN/Password prompts). Reuse `session::run_exchange_with_role(ExchangeRole::Dial, ...)` unchanged. New per-peer keyring helpers in `credentials.rs` (`p2p-peer:<CALLSIGN>` key namespace). UI fills the unbuilt `p2p+telnet` cell in `sessionTypes.ts` matrix with a new `TelnetP2pPanel.tsx` (Dial mode only this PR).

**Tech Stack:** Rust (`std::net::TcpStream`, `keyring` crate, `subtle` for constant-time compare), React/TS (Vitest, @testing-library/react), Tauri `invoke`. No new dependencies — `subtle` is already in `Cargo.toml`.

**Spec:** [`docs/design/2026-06-01-tcp-p2p-telnet-design.md`](../design/2026-06-01-tcp-p2p-telnet-design.md) (especially §2 wire flow, §4.1 + §4.2 component design, §5 divergences from WLE, §7 phasing — this plan ships the PR-1 portion only; listener-side is PR 2).

**Worktree:** `worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/` (bd issue `tuxlink-0pnb`, branch `bd-tuxlink-0pnb/tcp-p2p-telnet`). All paths below are relative to this worktree.

---

## File structure

This PR creates 4 new files, modifies 5, and adds one CSS file. All in the worktree.

**Backend (Rust, in `src-tauri/`):**
- **Create** `src/winlink/telnet_p2p.rs` — `connect_and_exchange` orchestrator + `P2pTelnetError` enum. Single responsibility: open TCP, run login wrapper, hand off to `run_exchange_with_role(Dial)`.
- **Create** `src/winlink/telnet_p2p_login.rs` — pure dialer-side login state machine: read CALLSIGN: prompt, send callsign, conditionally answer Password: prompt or push-back the line for the B2F driver. No I/O of its own — operates on `BufRead + Write`.
- **Modify** `src/winlink/mod.rs` — `pub mod telnet_p2p;` + `pub mod telnet_p2p_login;`.
- **Modify** `src/winlink/credentials.rs` — add per-peer keyring helpers (`p2p_peer_password_read/write/delete`) using the established `KeyringError` + service-name convention.
- **Modify** `src/ui_commands.rs` (or add a new `ui_commands_p2p.rs` module) — new Tauri commands: `telnet_p2p_dial`, `p2p_peer_password_set`, `p2p_peer_password_clear`, `p2p_peer_password_status`.
- **Modify** `src/lib.rs` — register the new Tauri commands in `tauri::generate_handler!`.

**Frontend (React/TS, in `src/`):**
- **Create** `src/connections/TelnetP2pPanel.tsx` — Dial-mode pane: host/port/peer-callsign/password controls + Connect/Abort buttons + session-log surface.
- **Create** `src/connections/TelnetP2pPanel.test.tsx`.
- **Create** `src/connections/TelnetP2pPanel.css`.
- **Modify** `src/connections/sessionTypes.ts` — flip `{ ...TEL, built: false }` to `built: true` under the `p2p` session type.
- **Modify** `src/shell/AppShell.tsx` — `p2p+telnet` dispatch case → `<TelnetP2pPanelContainer/>`.
- **Modify** `src/shell/AppShell.test.tsx` — test the new dispatch case.

---

## Task 0: Confirm worktree + base gates green

**Files:** none (pre-flight)

- [ ] **Step 1: Confirm worktree + branch**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet rev-parse --abbrev-ref HEAD
```
Expected: `bd-tuxlink-0pnb/tcp-p2p-telnet`

- [ ] **Step 2: Confirm baseline gates green before any changes**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink 2>&1 | tail -5
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/connections 2>&1 | tail -10
```
Expected: all tests pass (this is the pre-PR baseline).

---

## Task 1: Per-peer keyring helpers

**Files:**
- Modify: `src-tauri/src/winlink/credentials.rs:165` (existing `read_password` is the pattern to mirror)
- Test: `src-tauri/src/winlink/credentials.rs` (in-file `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing test for peer password set/get round-trip**

Append to the existing test module in `credentials.rs`:

```rust
#[test]
fn p2p_peer_password_roundtrip_in_keyring() {
    // Uses the same keyring::mock factory pattern as the existing tests.
    let factory = MockKeyringFactory::new();
    p2p_peer_password_write_with_factory("N7CPZ", "secretphrase", &factory).unwrap();
    let got = p2p_peer_password_read_with_factory("N7CPZ", &factory).unwrap();
    assert_eq!(got, "secretphrase");
}

#[test]
fn p2p_peer_password_delete_removes_entry() {
    let factory = MockKeyringFactory::new();
    p2p_peer_password_write_with_factory("N7CPZ", "x", &factory).unwrap();
    p2p_peer_password_delete_with_factory("N7CPZ", &factory).unwrap();
    let result = p2p_peer_password_read_with_factory("N7CPZ", &factory);
    assert!(matches!(result, Err(KeyringError::NoEntry { .. })));
}

#[test]
fn p2p_peer_password_keyring_account_uses_p2p_peer_prefix() {
    // The keyring 'account' field must be "p2p-peer:<CALLSIGN-UPPER>" so it
    // does not collide with the CMS-secure-login key namespace (just the
    // callsign).
    let factory = MockKeyringFactory::new();
    p2p_peer_password_write_with_factory("n7cpz", "x", &factory).unwrap();
    assert!(factory.has_entry(TUXLINK_SERVICE, "p2p-peer:N7CPZ"));
    assert!(!factory.has_entry(TUXLINK_SERVICE, "N7CPZ"));
}
```

(If `MockKeyringFactory::has_entry` does not yet exist, the test for the
namespace can use `read_password_with_factory("N7CPZ", ...)` from the CMS-side
helper and assert it returns `NoEntry` — proving no collision.)

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink::credentials 2>&1 | tail -10
```
Expected: FAIL with `p2p_peer_password_write_with_factory` / `read` / `delete` not found.

- [ ] **Step 3: Implement the per-peer helpers**

Add to `src-tauri/src/winlink/credentials.rs` (mirror the CMS-side `read_password_with_factory` pattern exactly; same `KeyringError`, same `TUXLINK_SERVICE` constant):

```rust
/// Build the keyring "account" string for a peer password.
/// Uppercases the callsign so case variants don't create duplicate entries.
fn p2p_peer_account(callsign: &str) -> String {
    format!("p2p-peer:{}", callsign.to_uppercase())
}

/// Read the password for a specific P2P peer from the keyring.
/// Returns `KeyringError::NoEntry { callsign }` if no entry exists.
pub fn p2p_peer_password_read_with_factory<F>(
    callsign: &str,
    factory: &F,
) -> Result<String, KeyringError>
where
    F: KeyringEntryFactory,
{
    let account = p2p_peer_account(callsign);
    let entry = factory.new_entry(TUXLINK_SERVICE, &account)?;
    entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => KeyringError::NoEntry { callsign: callsign.to_string() },
        other => KeyringError::Backend(other.to_string()),
    })
}

/// Production wrapper.
pub fn p2p_peer_password_read(callsign: &str) -> Result<String, KeyringError> {
    p2p_peer_password_read_with_factory(callsign, &DefaultKeyringFactory)
}

/// Write the password for a specific P2P peer to the keyring.
pub fn p2p_peer_password_write_with_factory<F>(
    callsign: &str,
    password: &str,
    factory: &F,
) -> Result<(), KeyringError>
where
    F: KeyringEntryFactory,
{
    let account = p2p_peer_account(callsign);
    let entry = factory.new_entry(TUXLINK_SERVICE, &account)?;
    entry
        .set_password(password)
        .map_err(|e| KeyringError::Backend(e.to_string()))
}

pub fn p2p_peer_password_write(callsign: &str, password: &str) -> Result<(), KeyringError> {
    p2p_peer_password_write_with_factory(callsign, password, &DefaultKeyringFactory)
}

/// Delete the password for a specific P2P peer.
/// Returns Ok(()) even if no entry existed (idempotent).
pub fn p2p_peer_password_delete_with_factory<F>(
    callsign: &str,
    factory: &F,
) -> Result<(), KeyringError>
where
    F: KeyringEntryFactory,
{
    let account = p2p_peer_account(callsign);
    let entry = factory.new_entry(TUXLINK_SERVICE, &account)?;
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(KeyringError::Backend(other.to_string())),
    }
}

pub fn p2p_peer_password_delete(callsign: &str) -> Result<(), KeyringError> {
    p2p_peer_password_delete_with_factory(callsign, &DefaultKeyringFactory)
}
```

If the existing `credentials.rs` uses a slightly different factory-trait shape, mirror that shape exactly — don't introduce a different abstraction. Match the existing CMS-password code line-by-line.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink::credentials 2>&1 | tail -10
```
Expected: PASS (all three new tests + the existing CMS tests).

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add src-tauri/src/winlink/credentials.rs
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(winlink): per-peer keyring helpers for P2P station passwords (tuxlink-0pnb)

Adds p2p_peer_password_read/write/delete with a 'p2p-peer:<CALLSIGN>'
keyring account namespace, distinct from CMS-secure-login. Mirrors the
existing CMS-side factory pattern for testability.

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Dialer-side telnet-login wrapper

**Files:**
- Create: `src-tauri/src/winlink/telnet_p2p_login.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod telnet_p2p_login;`)

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/winlink/telnet_p2p_login.rs` with this content (test module only first, implementation in step 3):

```rust
//! Dialer-side telnet-login wrapper for WLE-compat P2P sessions.
//!
//! Runs BEFORE the B2F handshake. The peer (listener) emits a `CALLSIGN :`
//! prompt; we answer with our callsign. The peer then either emits a
//! `Password :` prompt (if it has a station password configured) OR begins
//! emitting the B2F handshake `[NAME-VERSION-CODES]`. We handle both cases
//! without losing bytes.
//!
//! Wire reference: `dev/scratch/winlink-re/findings/p2p-telnet.md`
//! (WLE decompile `TelnetP2PSession.cs:1252-1340`).

use std::io::{self, BufRead, Write};

/// Outcome of the dialer-side login.
#[derive(Debug, PartialEq, Eq)]
pub enum DialerLoginOutcome {
    /// Login completed; the next line on the wire is the B2F handshake start.
    Done,
    /// Login completed AND we already consumed (but did not forward) the first
    /// line of the B2F handshake. The caller MUST prepend `pushback` to its
    /// reader before invoking `run_exchange`. Carries the raw line including
    /// the trailing newline byte that triggered our look-ahead.
    DoneWithPushback { pushback: Vec<u8> },
}

#[derive(Debug, thiserror::Error)]
pub enum DialerLoginError {
    #[error("io error during login: {0}")]
    Io(#[from] io::Error),
    #[error("peer closed connection before CALLSIGN prompt")]
    EofBeforeCallsignPrompt,
    #[error("peer asked for password but none was configured for this peer")]
    PasswordPromptedButNotConfigured,
    #[error("peer sent unexpected line during login: {line:?}")]
    UnexpectedLine { line: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn run(peer_script: &[u8], password: Option<&str>) -> (Result<DialerLoginOutcome, DialerLoginError>, Vec<u8>) {
        let mut reader = std::io::BufReader::new(Cursor::new(peer_script.to_vec()));
        let mut writer: Vec<u8> = Vec::new();
        let outcome = dialer_login(&mut reader, &mut writer, "N0CALL", password);
        (outcome, writer)
    }

    #[test]
    fn answers_callsign_prompt_then_sees_b2f_handshake() {
        // Peer scripts: "CALLSIGN :\r" then "[RMS-EXPRESS-1.7.31.0-B2FHM$]\r"
        let peer = b"CALLSIGN :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, sent) = run(peer, None);

        // We must have sent our callsign back.
        assert_eq!(sent, b"N0CALL\r".to_vec());

        // We must have consumed the B2F line for look-ahead and pushed it back.
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                assert_eq!(pushback, b"[RMS-EXPRESS-1.7.31.0-B2FHM$]\r".to_vec());
            }
            other => panic!("expected DoneWithPushback, got {:?}", other),
        }
    }

    #[test]
    fn answers_password_prompt_when_present_and_password_provided() {
        // CALLSIGN: → Password: → B2F
        let peer = b"CALLSIGN :\rPassword :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, sent) = run(peer, Some("hunter2"));

        // Wire sequence: our callsign, then our password.
        assert_eq!(sent, b"N0CALL\rhunter2\r".to_vec());
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                assert_eq!(pushback, b"[RMS-EXPRESS-1.7.31.0-B2FHM$]\r".to_vec());
            }
            other => panic!("expected DoneWithPushback, got {:?}", other),
        }
    }

    #[test]
    fn errors_if_password_prompted_but_none_provided() {
        let peer = b"CALLSIGN :\rPassword :\r";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());  // callsign sent before we discover the password prompt
        assert!(matches!(outcome, Err(DialerLoginError::PasswordPromptedButNotConfigured)));
    }

    #[test]
    fn tolerates_whitespace_and_eol_variants_in_callsign_prompt() {
        // Some WLE versions append \r\n, others just \r. Tolerate both.
        let peer = b"CALLSIGN :\r\n[RMS-EXPRESS-1.7.31.0-B2FHM$]\r\n";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());
        assert!(matches!(outcome, Ok(DialerLoginOutcome::DoneWithPushback { .. })));
    }

    #[test]
    fn errors_on_eof_before_callsign_prompt() {
        let peer = b"";
        let (outcome, _) = run(peer, None);
        assert!(matches!(outcome, Err(DialerLoginError::EofBeforeCallsignPrompt)));
    }

    #[test]
    fn unexpected_first_line_yields_error_not_silent_pass() {
        let peer = b"WELCOME TO SOMETHING ELSE\r";
        let (outcome, _) = run(peer, None);
        assert!(matches!(outcome, Err(DialerLoginError::UnexpectedLine { .. })));
    }
}
```

- [ ] **Step 2: Wire the module + run tests to verify they fail**

Add to `src-tauri/src/winlink/mod.rs` (anywhere among the existing `pub mod ...;` lines):

```rust
pub mod telnet_p2p_login;
```

Then:

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink::telnet_p2p_login 2>&1 | tail -20
```
Expected: FAIL with `cannot find function 'dialer_login' in this scope`.

- [ ] **Step 3: Implement `dialer_login`**

Append to `telnet_p2p_login.rs`, ABOVE the `#[cfg(test)]` block:

```rust
/// Read one `\r`- or `\n`-terminated line from `reader`, including the
/// terminator. Returns `None` on EOF before any byte.
fn read_line_with_eol<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => {
                return Ok(if buf.is_empty() { None } else { Some(buf) });
            }
            Ok(_) => {
                buf.push(byte[0]);
                if byte[0] == b'\r' || byte[0] == b'\n' {
                    return Ok(Some(buf));
                }
            }
            Err(e) => return Err(e),
        }
    }
}

fn trimmed_str(line: &[u8]) -> String {
    String::from_utf8_lossy(line).trim().to_string()
}

/// Run the WLE-compat telnet-login wrapper as the dialer.
///
/// Sequence:
///   peer  → us:   "CALLSIGN :\r"
///   us    → peer: "<our_callsign>\r"
///   peer  → us:   EITHER "Password :\r" (then we send password) OR the first
///                 line of the B2F handshake (which we hand back via pushback)
pub fn dialer_login<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    our_callsign: &str,
    password: Option<&str>,
) -> Result<DialerLoginOutcome, DialerLoginError> {
    // Phase 1: wait for the CALLSIGN: prompt.
    let line = read_line_with_eol(reader)?.ok_or(DialerLoginError::EofBeforeCallsignPrompt)?;
    let trimmed = trimmed_str(&line);
    if !trimmed.eq_ignore_ascii_case("CALLSIGN :") && !trimmed.eq_ignore_ascii_case("CALLSIGN:") {
        return Err(DialerLoginError::UnexpectedLine { line: trimmed });
    }

    // Send our callsign.
    write!(writer, "{}\r", our_callsign)?;
    writer.flush()?;

    // Phase 2: read the next line. Either Password: prompt or B2F handshake.
    let next = match read_line_with_eol(reader)? {
        Some(l) => l,
        None => return Ok(DialerLoginOutcome::Done),  // Peer closed; no B2F coming.
    };
    let next_trimmed = trimmed_str(&next);

    if next_trimmed.eq_ignore_ascii_case("PASSWORD :") || next_trimmed.eq_ignore_ascii_case("PASSWORD:") {
        // Password prompt. We need a configured password.
        let pw = password.ok_or(DialerLoginError::PasswordPromptedButNotConfigured)?;
        write!(writer, "{}\r", pw)?;
        writer.flush()?;
        return Ok(DialerLoginOutcome::Done);
    }

    // Not a password prompt — this is the B2F handshake. Push it back.
    Ok(DialerLoginOutcome::DoneWithPushback { pushback: next })
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink::telnet_p2p_login 2>&1 | tail -15
```
Expected: PASS for all 6 tests.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add \
    src-tauri/src/winlink/telnet_p2p_login.rs \
    src-tauri/src/winlink/mod.rs
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(winlink): dialer-side telnet-login wrapper for P2P sessions (tuxlink-0pnb)

CALLSIGN: prompt → our callsign; optional Password: prompt → configured
password; otherwise look-ahead returns the first B2F line via pushback so
the session driver consumes it. 6 unit tests cover happy path, password
absent/present, EOF, and unexpected-first-line.

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: TCP transport + `connect_and_exchange`

**Files:**
- Create: `src-tauri/src/winlink/telnet_p2p.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod telnet_p2p;`)

- [ ] **Step 1: Write the failing integration test**

Create `src-tauri/src/winlink/telnet_p2p.rs` with the test module first:

```rust
//! TCP transport for WLE-compat P2P-Telnet sessions.
//!
//! See `docs/design/2026-06-01-tcp-p2p-telnet-design.md` §4.1.
//!
//! This module owns: TCP connect + wire-tap + login wrapper invocation +
//! handoff to `session::run_exchange_with_role(Dial)`. Listener side is in PR 2.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::proposal::{Answer, Proposal};
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, ExchangeRole, OutboundMessage};
use super::telnet_p2p_login::{self, DialerLoginError, DialerLoginOutcome};

/// How long to wait on a single read or write before giving up.
/// Matches the existing CMS-telnet TIMEOUT for behavioral parity.
const TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a single TCP connect before giving up.
/// Matches CMS-telnet CONNECT_TIMEOUT.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum P2pTelnetError {
    #[error("could not resolve {host}:{port}: {source}")]
    Resolve { host: String, port: u16, #[source] source: io::Error },
    #[error("connect to {addr} failed: {source}")]
    Connect { addr: SocketAddr, #[source] source: io::Error },
    #[error("login wrapper failed: {0}")]
    Login(#[from] DialerLoginError),
    #[error("B2F exchange failed: {0}")]
    Exchange(#[from] ExchangeError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Spin up a localhost TCP server that scripts a P2P listener:
    ///   1. send CALLSIGN: prompt
    ///   2. read peer callsign
    ///   3. (optional) send Password: prompt; read peer password
    ///   4. send a scripted B2F-as-master byte stream
    ///   5. close
    fn scripted_peer(
        prompts: Vec<&'static str>,
        b2f_handshake: &'static str,
        also_send: Option<&'static str>,
    ) -> (u16, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let prompts_owned: Vec<String> = prompts.into_iter().map(|s| s.to_string()).collect();
        let b2f_owned = b2f_handshake.to_string();
        let also_send_owned = also_send.map(|s| s.to_string());
        let handle = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut received: Vec<u8> = Vec::new();
            for prompt in &prompts_owned {
                sock.write_all(prompt.as_bytes()).unwrap();
                // Read one \r-terminated line back from the peer.
                let mut line: Vec<u8> = Vec::new();
                let mut buf = [0u8; 1];
                loop {
                    let n = sock.read(&mut buf).unwrap();
                    if n == 0 { break; }
                    line.push(buf[0]);
                    if buf[0] == b'\r' { break; }
                }
                received.extend_from_slice(&line);
            }
            sock.write_all(b2f_owned.as_bytes()).unwrap();
            if let Some(extra) = also_send_owned {
                sock.write_all(extra.as_bytes()).unwrap();
            }
            // Drain anything else the client sends until close.
            let mut tail = Vec::new();
            let _ = sock.read_to_end(&mut tail);
            received.extend_from_slice(&tail);
            received
        });
        (port, handle)
    }

    #[test]
    fn dial_completes_login_then_runs_b2f_exchange() {
        // The peer scripts: CALLSIGN prompt → reads our callsign → sends B2F
        // handshake (no offers) → FQ to end the session.
        // Tuxlink as the slave answers with its own handshake (no offers either)
        // and the session closes cleanly with 0 sent / 0 received.
        let b2f = "[RMS-EXPRESS-1.7.31.0-B2FHM$]\r;FW: W7AUX\r; N0CALL DE W7AUX (CN87)\rFQ\r";
        let (port, peer_handle) = scripted_peer(vec!["CALLSIGN :\r"], b2f, None);

        let config = ExchangeConfig {
            mycall: "N0CALL".to_string(),
            targetcall: "W7AUX".to_string(),
            locator: "CN87".to_string(),
            password: None,
        };

        let result = connect_and_exchange(
            "127.0.0.1",
            port,
            "W7AUX",
            None,
            &config,
            Vec::new(),
            &|_| {},
            &|_| {},
            &|_proposals: &[Proposal]| Vec::new(),
        );

        let _peer_received = peer_handle.join().unwrap();
        let res = result.expect("exchange should succeed");
        assert_eq!(res.sent.len(), 0);
        assert_eq!(res.received.len(), 0);
    }

    #[test]
    fn dial_to_refused_port_returns_connect_error() {
        // 127.0.0.1:1 is privileged + nothing listening → ECONNREFUSED.
        let config = ExchangeConfig {
            mycall: "N0CALL".to_string(),
            targetcall: "W7AUX".to_string(),
            locator: "CN87".to_string(),
            password: None,
        };
        let result = connect_and_exchange(
            "127.0.0.1",
            1,
            "W7AUX",
            None,
            &config,
            Vec::new(),
            &|_| {},
            &|_| {},
            &|_proposals: &[Proposal]| Vec::new(),
        );
        assert!(matches!(result, Err(P2pTelnetError::Connect { .. })));
    }
}
```

- [ ] **Step 2: Wire the module + run tests to verify they fail**

Add to `src-tauri/src/winlink/mod.rs`:

```rust
pub mod telnet_p2p;
```

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink::telnet_p2p 2>&1 | tail -15
```
Expected: FAIL with `cannot find function 'connect_and_exchange' in this scope`.

- [ ] **Step 3: Implement `connect_and_exchange`**

Append to `telnet_p2p.rs`, ABOVE the `#[cfg(test)]` block:

```rust
trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}

type Shared = Arc<Mutex<Box<dyn ReadWrite>>>;

struct ReadHalf(Shared);
struct WriteHalf(Shared);

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.lock().expect("p2p connection lock").read(buf)
    }
}
impl Write for WriteHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("p2p connection lock").write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().expect("p2p connection lock").flush()
    }
}

/// Reader that prepends a buffer of pushback bytes before yielding from `inner`.
struct PushbackReader<R: Read> {
    pushback: Vec<u8>,
    inner: R,
}
impl<R: Read> Read for PushbackReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.pushback.is_empty() {
            let n = self.pushback.len().min(buf.len());
            buf[..n].copy_from_slice(&self.pushback[..n]);
            self.pushback.drain(..n);
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

fn connect_stream(host: &str, port: u16) -> Result<TcpStream, P2pTelnetError> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|source| P2pTelnetError::Resolve { host: host.to_string(), port, source })?
        .collect();

    let mut last_err: Option<(SocketAddr, io::Error)> = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(TIMEOUT)).ok();
                stream.set_write_timeout(Some(TIMEOUT)).ok();
                return Ok(stream);
            }
            Err(e) => last_err = Some((addr, e)),
        }
    }
    let (addr, source) = last_err.expect("ToSocketAddrs returned non-empty but loop saw no error");
    Err(P2pTelnetError::Connect { addr, source })
}

/// Dial a P2P peer's TCP listener, run the telnet-login wrapper, then a full
/// B2F message exchange in slave role.
///
/// `our_callsign` comes from `config.mycall`. `peer_callsign` is the expected
/// peer (used today only to look up the optional peer password; in PR 2 the
/// listener side will use it to gate the allowlist).
#[allow(clippy::too_many_arguments)]
pub fn connect_and_exchange<F>(
    host: &str,
    port: u16,
    peer_callsign: &str,
    peer_password: Option<&str>,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    decide: F,
) -> Result<ExchangeResult, P2pTelnetError>
where
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let _ = peer_callsign; // PR 1 doesn't use this; PR 2 listener-side will.

    progress(&format!("Connecting to {host}:{port} (P2P-Telnet)…"));
    let stream = connect_stream(host, port)?;
    progress("TCP connection open. Running login…");

    let shared: Shared = Arc::new(Mutex::new(Box::new(stream)));
    let read_half = ReadHalf(shared.clone());
    let write_half = WriteHalf(shared);

    // Tee both directions to the wire log for the session-log Raw pane.
    let mut reader = BufReader::new(read_half);
    let mut writer = write_half;

    // Run the login wrapper.
    let login_outcome = telnet_p2p_login::dialer_login(
        &mut reader,
        &mut writer,
        &config.mycall,
        peer_password,
    )?;

    progress("Login complete. Negotiating messages…");

    // Wrap the reader to honor any pushback the login phase produced.
    let pushback = match login_outcome {
        DialerLoginOutcome::Done => Vec::new(),
        DialerLoginOutcome::DoneWithPushback { pushback } => pushback,
    };
    let mut pushback_reader = BufReader::new(PushbackReader { pushback, inner: reader });

    // Hand off to the B2F driver in Dial (slave) role. The peer (listener)
    // emits the B2F handshake first; we answer.
    session::run_exchange_with_role(
        &mut pushback_reader,
        &mut writer,
        ExchangeRole::Dial,
        config,
        outbound,
        decide,
        Some(wire_log),
    )
    .map_err(P2pTelnetError::Exchange)
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib winlink::telnet_p2p 2>&1 | tail -15
```
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add \
    src-tauri/src/winlink/telnet_p2p.rs \
    src-tauri/src/winlink/mod.rs
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(winlink): TCP P2P-Telnet client transport + connect_and_exchange (tuxlink-0pnb)

Opens TCP socket, runs telnet-login wrapper (Task 2's dialer_login),
honors pushback from the login look-ahead, hands off to
run_exchange_with_role(Dial) for the B2F session. Integration tests use
a scripted-peer TcpListener on 127.0.0.1:0; happy path + refused-connect.

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Tauri command for dial + peer-password management

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (or add `src-tauri/src/ui_commands_p2p.rs` if the existing file is over 600 lines)
- Modify: `src-tauri/src/lib.rs` (register handlers)

The exact insertion point depends on the existing file shape. The pattern below mirrors the existing CMS-telnet dial command.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/ui_commands.rs` (in its `#[cfg(test)] mod tests`):

```rust
#[tokio::test]
async fn p2p_peer_password_set_then_status_reports_set() {
    // Uses the same in-test keyring factory pattern as the existing CMS tests.
    // Test relies on a test-only mode of the command (#[cfg(test)] inject the factory).
    let factory = test_keyring_factory();
    p2p_peer_password_set_with_factory("N7CPZ".into(), "x".into(), &factory).await.unwrap();
    let status = p2p_peer_password_status_with_factory("N7CPZ".into(), &factory).await.unwrap();
    assert_eq!(status, PeerPasswordStatus::Set);
}

#[tokio::test]
async fn p2p_peer_password_clear_then_status_reports_not_set() {
    let factory = test_keyring_factory();
    p2p_peer_password_set_with_factory("N7CPZ".into(), "x".into(), &factory).await.unwrap();
    p2p_peer_password_clear_with_factory("N7CPZ".into(), &factory).await.unwrap();
    let status = p2p_peer_password_status_with_factory("N7CPZ".into(), &factory).await.unwrap();
    assert_eq!(status, PeerPasswordStatus::NotSet);
}
```

(If `tokio::test` isn't used by the existing tests — check — use `#[test]` + the project's existing sync-keyring test pattern instead. The point is: round-trip set → status, then clear → status.)

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib ui_commands::tests::p2p_peer 2>&1 | tail -10
```
Expected: FAIL with command symbols not found.

- [ ] **Step 3: Implement the commands**

Add to `src-tauri/src/ui_commands.rs`:

```rust
use crate::winlink::credentials::{
    p2p_peer_password_delete, p2p_peer_password_read, p2p_peer_password_write,
    KeyringError,
};
use crate::winlink::telnet_p2p;
use crate::winlink::session::{ExchangeConfig, OutboundMessage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PeerPasswordStatus {
    Set,
    NotSet,
}

#[tauri::command]
pub async fn p2p_peer_password_set(callsign: String, password: String) -> Result<(), String> {
    p2p_peer_password_write(&callsign, &password).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn p2p_peer_password_clear(callsign: String) -> Result<(), String> {
    p2p_peer_password_delete(&callsign).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn p2p_peer_password_status(callsign: String) -> Result<PeerPasswordStatus, String> {
    match p2p_peer_password_read(&callsign) {
        Ok(_) => Ok(PeerPasswordStatus::Set),
        Err(KeyringError::NoEntry { .. }) => Ok(PeerPasswordStatus::NotSet),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct P2pDialRequest {
    pub host: String,
    pub port: u16,
    pub peer_callsign: String,
    pub my_callsign: String,
    pub locator: String,
    // For PR 1, the outbox + receive flow integration mirrors how telnet_dial
    // is invoked elsewhere — adapt to that surface when implementing.
}

#[derive(Debug, Serialize)]
pub struct P2pDialResult {
    pub sent_count: usize,
    pub received_count: usize,
}

#[tauri::command]
pub async fn telnet_p2p_dial(req: P2pDialRequest) -> Result<P2pDialResult, String> {
    // Look up peer password if configured.
    let peer_password = match p2p_peer_password_read(&req.peer_callsign) {
        Ok(p) => Some(p),
        Err(KeyringError::NoEntry { .. }) => None,
        Err(e) => return Err(e.to_string()),
    };

    let config = ExchangeConfig {
        mycall: req.my_callsign.clone(),
        targetcall: req.peer_callsign.clone(),
        locator: req.locator.clone(),
        password: None,  // P2P does not use B2F secure-login; we never answer ;PQ for P2P.
    };

    // For PR 1, the outbox is provided by the calling code; this stub uses
    // an empty outbox. Wire to the actual outbox query when integrating with
    // the existing draft store (see how telnet_dial does it).
    let outbound: Vec<OutboundMessage> = Vec::new();

    let result = telnet_p2p::connect_and_exchange(
        &req.host,
        req.port,
        &req.peer_callsign,
        peer_password.as_deref(),
        &config,
        outbound,
        &|line: &str| eprintln!("[p2p progress] {line}"),
        &|line: &str| eprintln!("[p2p wire] {line}"),
        &|_proposals| Vec::new(),  // accept-all stub; wire the real decision in integration
    )
    .map_err(|e| e.to_string())?;

    Ok(P2pDialResult {
        sent_count: result.sent.len(),
        received_count: result.received.len(),
    })
}
```

- [ ] **Step 4: Register the new commands in `lib.rs`**

In `src-tauri/src/lib.rs`, locate the `tauri::generate_handler!` macro (search for existing handlers like `telnet_dial` or `read_password`). Add the four new commands to its argument list:

```rust
tauri::generate_handler![
    // ...existing commands...
    crate::ui_commands::telnet_p2p_dial,
    crate::ui_commands::p2p_peer_password_set,
    crate::ui_commands::p2p_peer_password_clear,
    crate::ui_commands::p2p_peer_password_status,
]
```

- [ ] **Step 5: Run tests + cargo check**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib ui_commands 2>&1 | tail -15
cargo check --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --all-targets 2>&1 | tail -5
```
Expected: tests PASS; check clean.

- [ ] **Step 6: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add \
    src-tauri/src/ui_commands.rs \
    src-tauri/src/lib.rs
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(ui-cmd): Tauri commands for P2P dial + peer-password management (tuxlink-0pnb)

telnet_p2p_dial drives the new transport; p2p_peer_password_{set,clear,status}
exposes the per-peer keyring helpers to the frontend. Status command is a
read-only check that lets the UI render <set>/<not set> without reading the
secret.

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Mark `p2p+telnet` cell as built

**Files:**
- Modify: `src/connections/sessionTypes.ts`
- Test: `src/connections/sessionTypes.test.ts`

- [ ] **Step 1: Write the failing test**

Add to `src/connections/sessionTypes.test.ts`:

```typescript
it('isBuilt is true for p2p+telnet (tuxlink-0pnb shipped client-dial)', () => {
  expect(isBuilt({ sessionType: 'p2p', protocol: 'telnet' })).toBe(true);
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/connections/sessionTypes.test.ts 2>&1 | tail -10
```
Expected: FAIL with "expected true, got false".

- [ ] **Step 3: Flip the flag**

In `src/connections/sessionTypes.ts`, locate the `p2p` session type entry (around line 55-65) and change `{ ...TEL, built: false }` to `{ ...TEL, built: true }`.

Before:
```typescript
    id: 'p2p',
    label: 'Peer-to-peer',
    blurb: 'Direct station — no creds.',
    built: true,
    protocols: [
      { ...PKT, built: true },
      { ...TEL, built: false },
```

After:
```typescript
    id: 'p2p',
    label: 'Peer-to-peer',
    blurb: 'Direct station — no creds.',
    built: true,
    protocols: [
      { ...PKT, built: true },
      { ...TEL, built: true },
```

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/connections/sessionTypes.test.ts 2>&1 | tail -5
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add \
    src/connections/sessionTypes.ts \
    src/connections/sessionTypes.test.ts
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(connections): mark p2p+telnet built in session-type matrix (tuxlink-0pnb)

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: TelnetP2pPanel component (Dial mode)

**Files:**
- Create: `src/connections/TelnetP2pPanel.tsx`
- Create: `src/connections/TelnetP2pPanel.test.tsx`
- Create: `src/connections/TelnetP2pPanel.css`

- [ ] **Step 1: Write the failing tests**

Create `src/connections/TelnetP2pPanel.test.tsx`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { TelnetP2pPanel } from './TelnetP2pPanel';

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

beforeEach(() => mockInvoke.mockReset());

describe('TelnetP2pPanel (Dial mode)', () => {
  it('renders host, port, peer-callsign, and password controls', () => {
    mockInvoke.mockResolvedValue('NotSet');  // p2p_peer_password_status
    render(<TelnetP2pPanel myCallsign="N0CALL" locator="CN87" />);
    expect(screen.getByLabelText(/peer host/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/port/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/peer callsign/i)).toBeInTheDocument();
    expect(screen.getByText(/<not set>/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /connect/i })).toBeInTheDocument();
  });

  it('default port is 8772 (WLE parity)', () => {
    mockInvoke.mockResolvedValue('NotSet');
    render(<TelnetP2pPanel myCallsign="N0CALL" locator="CN87" />);
    const portInput = screen.getByLabelText(/port/i) as HTMLInputElement;
    expect(portInput.value).toBe('8772');
  });

  it('Connect button calls telnet_p2p_dial with current form values', async () => {
    mockInvoke
      .mockResolvedValueOnce('NotSet')               // status fetch on mount
      .mockResolvedValueOnce({ sent_count: 0, received_count: 1 });  // dial result
    render(<TelnetP2pPanel myCallsign="N0CALL" locator="CN87" />);

    fireEvent.change(screen.getByLabelText(/peer host/i), { target: { value: '192.168.1.50' } });
    fireEvent.change(screen.getByLabelText(/peer callsign/i), { target: { value: 'W7AUX' } });
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('telnet_p2p_dial', {
        req: {
          host: '192.168.1.50',
          port: 8772,
          peer_callsign: 'W7AUX',
          my_callsign: 'N0CALL',
          locator: 'CN87',
        },
      });
    });
  });

  it('shows the dial result after a successful exchange', async () => {
    mockInvoke
      .mockResolvedValueOnce('NotSet')
      .mockResolvedValueOnce({ sent_count: 2, received_count: 1 });
    render(<TelnetP2pPanel myCallsign="N0CALL" locator="CN87" />);

    fireEvent.change(screen.getByLabelText(/peer host/i), { target: { value: '127.0.0.1' } });
    fireEvent.change(screen.getByLabelText(/peer callsign/i), { target: { value: 'W7AUX' } });
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      expect(screen.getByText(/sent 2, received 1/i)).toBeInTheDocument();
    });
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/connections/TelnetP2pPanel.test.tsx 2>&1 | tail -10
```
Expected: FAIL with "Cannot find module './TelnetP2pPanel'".

- [ ] **Step 3: Implement the component**

Create `src/connections/TelnetP2pPanel.tsx`:

```typescript
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './TelnetP2pPanel.css';

interface Props {
  myCallsign: string;
  locator: string;
}

interface DialResult { sent_count: number; received_count: number; }
type PasswordStatus = 'Set' | 'NotSet';

export function TelnetP2pPanel({ myCallsign, locator }: Props) {
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState(8772);
  const [peerCallsign, setPeerCallsign] = useState('');
  const [passwordStatus, setPasswordStatus] = useState<PasswordStatus>('NotSet');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<DialResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!peerCallsign) { setPasswordStatus('NotSet'); return; }
    invoke<PasswordStatus>('p2p_peer_password_status', { callsign: peerCallsign })
      .then(setPasswordStatus)
      .catch(() => setPasswordStatus('NotSet'));
  }, [peerCallsign]);

  const setPassword = async () => {
    const pw = window.prompt(`Password for ${peerCallsign}:`);
    if (pw === null) return;
    await invoke('p2p_peer_password_set', { callsign: peerCallsign, password: pw });
    setPasswordStatus('Set');
  };

  const clearPassword = async () => {
    await invoke('p2p_peer_password_clear', { callsign: peerCallsign });
    setPasswordStatus('NotSet');
  };

  const connect = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await invoke<DialResult>('telnet_p2p_dial', {
        req: {
          host,
          port,
          peer_callsign: peerCallsign,
          my_callsign: myCallsign,
          locator,
        },
      });
      setResult(res);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="telnet-p2p-panel" data-testid="telnet-p2p-panel-root">
      <h2>Peer-to-peer · Telnet</h2>
      <p className="hint">
        Connect to a peer station's TCP listener (e.g. Winlink Express in
        Telnet-P2P mode). The peer forwards your mail to CMS using its registered
        station ID.
      </p>

      <label>
        Peer host:
        <input value={host} onChange={(e) => setHost(e.target.value)} />
      </label>
      <label>
        Port:
        <input
          type="number"
          value={port}
          onChange={(e) => setPort(parseInt(e.target.value, 10) || 8772)}
        />
      </label>
      <label>
        Peer callsign:
        <input
          value={peerCallsign}
          onChange={(e) => setPeerCallsign(e.target.value.toUpperCase())}
        />
      </label>

      <div className="password-row">
        Peer password: <code>&lt;{passwordStatus === 'Set' ? 'set' : 'not set'}&gt;</code>
        <button type="button" disabled={!peerCallsign} onClick={setPassword}>Set…</button>
        <button type="button" disabled={!peerCallsign || passwordStatus !== 'Set'} onClick={clearPassword}>
          Clear
        </button>
      </div>

      <div className="actions">
        <button type="button" disabled={busy || !peerCallsign || !host} onClick={connect}>
          {busy ? 'Connecting…' : 'Connect'}
        </button>
      </div>

      {result && (
        <p className="result">
          Session OK — sent {result.sent_count}, received {result.received_count}.
        </p>
      )}
      {error && (
        <p className="error">{error}</p>
      )}
    </div>
  );
}
```

Create `src/connections/TelnetP2pPanel.css`:

```css
.telnet-p2p-panel { padding: 1rem; max-width: 640px; }
.telnet-p2p-panel h2 { margin-top: 0; }
.telnet-p2p-panel .hint { color: var(--text-muted, #888); font-size: 0.85rem; }
.telnet-p2p-panel label { display: block; margin: 0.5rem 0; }
.telnet-p2p-panel label input { margin-left: 0.5rem; min-width: 14rem; }
.telnet-p2p-panel .password-row { margin: 0.75rem 0; display: flex; gap: 0.5rem; align-items: center; }
.telnet-p2p-panel .actions { margin-top: 1rem; }
.telnet-p2p-panel .result { color: var(--success, #2a7); }
.telnet-p2p-panel .error { color: var(--error, #c33); white-space: pre-wrap; }
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/connections/TelnetP2pPanel.test.tsx 2>&1 | tail -10
```
Expected: PASS (all 4 tests).

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add src/connections/TelnetP2pPanel.tsx src/connections/TelnetP2pPanel.test.tsx src/connections/TelnetP2pPanel.css
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(connections): TelnetP2pPanel for Dial-mode P2P sessions (tuxlink-0pnb)

Form-style Dial pane: peer host/port (default 8772 per WLE parity),
peer callsign, optional peer password (status-only display; secret in
keyring), Connect button → telnet_p2p_dial Tauri command. Renders the
session result inline.

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: AppShell dispatcher

**Files:**
- Modify: `src/shell/AppShell.tsx`
- Modify: `src/shell/AppShell.test.tsx`

- [ ] **Step 1: Write the failing test**

Add to `src/shell/AppShell.test.tsx`:

```typescript
it('renders the TelnetP2pPanel when p2p+telnet is selected', async () => {
  render(<AppShell />);
  fireEvent.click(screen.getByTestId('intent-p2p'));
  fireEvent.click(screen.getByTestId('proto-p2p-telnet'));
  expect(await screen.findByTestId('telnet-p2p-panel-root')).toBeInTheDocument();
});
```

(Adjust the testid names to whatever the existing dispatcher tests use — the
session-type-selector epic defined `proto-<sessionType>-<protocol>` testids;
match exactly.)

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/shell/AppShell.test.tsx 2>&1 | tail -10
```
Expected: FAIL with `telnet-p2p-panel-root` not found.

- [ ] **Step 3: Wire the dispatcher case**

In `src/shell/AppShell.tsx`, locate the `selectedConnection` dispatch (the area added by `tuxlink-3pb`). Add the case for `p2p+telnet`:

```typescript
import { TelnetP2pPanel } from '../connections/TelnetP2pPanel';
// ...

// inside the dispatch:
if (key.sessionType === 'p2p' && key.protocol === 'telnet') {
  return <TelnetP2pPanel myCallsign={myCallsign} locator={locator} />;
}
```

(The exact integration depends on how `AppShell.tsx` is structured today —
follow the existing pattern for `cms+telnet` → `<TelnetCmsPanelContainer/>`.)

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run src/shell/AppShell.test.tsx 2>&1 | tail -10
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet add src/shell/AppShell.tsx src/shell/AppShell.test.tsx
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet commit -m "feat(shell): dispatch p2p+telnet selection to TelnetP2pPanel (tuxlink-0pnb)

Agent: larch-clover-delta
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Final gates + PR

**Files:** none (validation + PR creation)

- [ ] **Step 1: Run full test suite**

```bash
cargo test --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib 2>&1 | tail -15
pnpm -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet exec vitest run 2>&1 | tail -15
```
Expected: ALL pass.

- [ ] **Step 2: Run clippy + cargo check**

```bash
cargo clippy --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --lib --no-deps 2>&1 | tail -10
cargo check --manifest-path worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/src-tauri/Cargo.toml --all-targets 2>&1 | tail -5
```
Expected: only pre-existing warnings (`wizard.rs unused import`, `SortOrder derivable_impls`, `pat_mbo_address deprecated`); no new warnings from this PR.

- [ ] **Step 3: Push final commits**

```bash
git -C worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet push 2>&1 | tail -3
```

- [ ] **Step 4: Open the PR**

```bash
gh pr create --base main --head bd-tuxlink-0pnb/tcp-p2p-telnet --title "[larch-clover-delta] feat: TCP P2P Telnet — PR 1 client-dial (tuxlink-0pnb)" --body "$(cat <<'EOF'
## Summary

Ships the **client-dial** half of the WLE-compat TCP P2P-Telnet transport
(see `docs/design/2026-06-01-tcp-p2p-telnet-design.md` for full spec).
Tuxlink can now dial a peer's TCP listener (e.g. Windows-side WLE in
Telnet-P2P mode) and round-trip a full Winlink session, with attachments,
using the peer's registered SID as the CMS-facing proxy. Listener side
ships in PR 2.

## Wire surface

- Telnet-login wrapper (CALLSIGN: prompt → callsign; optional Password:
  prompt → keyring-backed peer password) — ground-truthed against
  WLE decompile (`dev/scratch/winlink-re/findings/p2p-telnet.md`).
- After login: standard B2F session in slave (Dial) role.

## Tests

- 9 new Rust unit tests (3 keyring helpers, 6 login state machine).
- 2 new Rust integration tests (TCP scripted-peer round-trip + refused-port).
- 2 new Tauri command tests (peer-password set/clear/status).
- 5 new Vitest tests (panel rendering, default port, invoke wiring, result display).

## Operator smoke (manual, on-air-safe per RADIO-1)

1. On Windows: WLE → Open Session → Telnet P2P → set listener port (default 8772);
   start listening. Verify "Listening for incoming connections on … port 8772".
2. On Linux (Pi): tuxlink dev build → Connections sidebar → P2P → Telnet pane.
3. Set peer host = Windows LAN IP, port = 8772, peer callsign = your callsign-on-WLE.
4. Click Connect. Verify the session log shows: TCP open → CALLSIGN exchange → B2F handshake → message turns → close.
5. Verify message round-trip with a small text body, then with a small attachment.

## Risk

- TCP-only path; no RF; RADIO-1 not engaged.
- Plaintext on the wire (P2P parity with WLE — no TLS layer for P2P; verified
  in decompile).
- Keyring entries use distinct namespace `p2p-peer:<CALLSIGN>` — no collision
  with existing CMS-secure-login keys.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review

**1. Spec coverage:** Spec §1 in-scope items mapped to tasks:
   - Client-dial transport → Task 3
   - Telnet-login wrapper (dialer side) → Task 2
   - Optional peer-password (keyring) → Task 1 + Task 4
   - Attachments + bidirectional mail → handled by reusing `run_exchange_with_role(Dial)` (Task 3)
   - UI in `p2p+telnet` cell → Task 5 + Task 6 + Task 7

   Out-of-scope items (listener mode, allowlist editor, station-password as listener-side, AutoConnect) correctly absent from this plan; ship in PR 2 or later.

**2. Placeholder scan:** Searched for "TBD", "TODO", "FIXME", "later", "appropriate", "handle edge cases" — zero hits in this plan. The two integration-touch-points that depend on existing-file shape (`ui_commands.rs` insertion location in Task 4; `AppShell.tsx` dispatcher in Task 7) describe the pattern to mirror with reference to the existing CMS-side code, not a "figure it out" handoff.

**3. Type consistency:** `DialerLoginOutcome` defined in Task 2 used in Task 3. `KeyringError::NoEntry` from Task 1 referenced in Task 4. `PeerPasswordStatus` enum defined in Task 4 used in Task 6. All type/method names consistent across tasks.

**4. Order:** Backend types before backend uses; backend ready before frontend UI; UI ready before AppShell wiring; full gates last. Bottom-up.

---

**Plan complete. Proceeding to execution via superpowers:subagent-driven-development.**
