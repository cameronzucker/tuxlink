# CAT Rig Control + Single-Pane Connect — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Tuxlink WLE-parity CAT control — tune the rig's frequency + data mode over a managed `rigctld` subprocess before an ARDOP dial, with a manual frequency element and an expandable Rig control section in the modem pane.

**Architecture:** A new in-workspace `tux-rig` crate wraps a managed `rigctld` subprocess and speaks the Hamlib `rigctl` TCP protocol (`F`/`f`/`M`/`m`/`T`/`t`). The ARDOP connect flow tunes freq+mode through `tux-rig` in the pre-audio window between `transport.init()` and `connect_arq()`; on internal-codec radios it then stops rigctld to release the CAT serial before audio (close-serial). The live ARQ session keeps keying PTT via ardopcf's existing path; `tux-rig` owns freq/mode and manual/Tune-only PTT. The modem pane (`ArdopRadioPanel`) gains a frequency element and a Rig control expander reusing existing pane styling. Connect failure can optionally walk an ordered ranked list (auto-QSY), gated by an operator setting.

**Tech Stack:** Rust (Tauri backend, `std::net::TcpStream`, `std::process::Child` via the existing `ManagedModem`), TypeScript/React (Vitest + `@testing-library/react`), Hamlib `rigctld`.

**Design spec:** `docs/superpowers/specs/2026-06-26-rig-control-single-pane-design.md` (bd tuxlink-8fkkk).

## Global Constraints

- **Backend = managed `rigctld` subprocess** (operator-confirmed). FT-710 = hamlib model `1049`, data mode `PKTUSB`. No libhamlib FFI, no own-CAT.
- **Rust compiles in CI, not on the Pi** — push a draft PR early; do not run `cargo build`/`cargo test` locally on the Pi (it will not finish). Author tests; CI runs them.
- **Clippy `-D warnings`** is the CI gate. Avoid the known traps: `io::Error::other(x)` not `io::Error::new(ErrorKind::Other, x)`; `is_some_and`/`is_none_or` not `map_or(false, …)` (MSRV permitting — this repo already uses `is_some_and`, see `build_ardop_extra_args` neighbours); no needless clones.
- **No tuxlink-added safeguards** — no bounded-airtime caps, no TOT timers, no extra confirmation modals. Mirror WLE.
- **Serde:** the codebase has **no `rename_all` on structs** except where shown; `ArdopUiConfig` uses `#[serde(rename_all = "snake_case")]` only on the `PttMethod` enum. New config fields are snake_case Rust idents and MUST be mirrored in the TS `ArdopFullConfig` DTO in the **same change**.
- **RADIO-1:** no agent runs any code path that can key the radio. Author + commit; the licensee runs on-air. `tux-rig` lifecycle tests use a **fake rigctld** (a local TCP server / shell stub), never a real radio.
- **Worktree:** all work happens in `worktrees/bd-tuxlink-8fkkk-rig-control-single-pane` on branch `bd-tuxlink-8fkkk/rig-control-single-pane`. Commit from that cwd (the main-checkout race hook keys off cwd). Every commit carries `Agent: butte-crag-marten` + the `Co-Authored-By` trailer.

---

## File Structure

**New crate `src-tauri/tux-rig/`:**
- `Cargo.toml` — crate manifest; added as workspace member + path dep in `src-tauri/Cargo.toml`.
- `src/lib.rs` — public surface: `Rig` trait, `Mode`, `RigStatus`, `RigConfig`, `RigError`, re-exports.
- `src/mode.rs` — `Mode` enum + rigctl string mapping.
- `src/protocol.rs` — `rigctl` TCP wire encode/decode (pure, fake-server-testable).
- `src/client.rs` — `RigctldClient` (TcpStream impl of the protocol).
- `src/managed.rs` — `ManagedRig` (spawn rigctld + connect client + lifecycle + close-serial stop/respawn).

**Modified backend:**
- `src-tauri/Cargo.toml` — workspace member + dependency.
- `src-tauri/src/config.rs` — `ArdopUiConfig` new fields + defaults.
- `src-tauri/src/modem_commands.rs` — tune step in the connect flow; new `ardop_tune_rig` command; QSY loop; rig config translation.

**Modified frontend:**
- `src/radio/modes/ArdopRadioPanel.tsx` — `ArdopFullConfig` DTO mirror; frequency element; Rig control expander; Tune-only invoke; prefill carrying freq/mode.
- `src/radio/modes/ArdopRadioPanel.test.tsx` — component tests.
- `src/radio/modes/ArdopRadioPanel.css` — only if a new class is unavoidable; prefer existing classes.

---

# Phase 1 — `tux-rig` crate

## Task 1: Scaffold the `tux-rig` crate

**Files:**
- Create: `src-tauri/tux-rig/Cargo.toml`
- Create: `src-tauri/tux-rig/src/lib.rs`
- Modify: `src-tauri/Cargo.toml:7-9` (workspace members) + `[dependencies]`

**Interfaces:**
- Produces: crate `tux_rig` with public `RigError` enum.

- [ ] **Step 1: Create the crate manifest**

`src-tauri/tux-rig/Cargo.toml`:
```toml
[package]
name = "tux-rig"
version = "0.1.0"
edition = "2021"

[dependencies]
tracing = "0.1"

[dev-dependencies]
```

- [ ] **Step 2: Create the lib root with the error type**

`src-tauri/tux-rig/src/lib.rs`:
```rust
//! tux-rig — CAT rig control over a managed `rigctld` subprocess.
//!
//! Owns frequency/mode tuning and manual/Tune-only PTT. The live ARDOP ARQ
//! session keys PTT via ardopcf's own path, not this crate.

use std::fmt;

/// Errors from rig control.
#[derive(Debug)]
pub enum RigError {
    /// Underlying I/O (socket connect, read, write).
    Io(std::io::Error),
    /// rigctld returned a non-zero `RPRT` code.
    Rprt(i32),
    /// A response could not be parsed into the expected shape.
    Protocol(String),
    /// Spawning / supervising the rigctld subprocess failed.
    Spawn(String),
}

impl fmt::Display for RigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RigError::Io(e) => write!(f, "rig I/O error: {e}"),
            RigError::Rprt(code) => write!(f, "rigctld returned RPRT {code}"),
            RigError::Protocol(s) => write!(f, "rig protocol error: {s}"),
            RigError::Spawn(s) => write!(f, "rigctld spawn error: {s}"),
        }
    }
}

impl std::error::Error for RigError {}

impl From<std::io::Error> for RigError {
    fn from(e: std::io::Error) -> Self {
        RigError::Io(e)
    }
}
```

- [ ] **Step 3: Register the crate in the workspace**

Modify `src-tauri/Cargo.toml` members (line 8) to add `"tux-rig"`:
```toml
[workspace]
members = [".", "tuxlink-security", "tuxlink-mcp-core", "tuxlink-mcp", "tuxlink-mcp-testserver", "tux-rig"]
default-members = ["."]
```
And add to the root crate `[dependencies]`:
```toml
tux-rig = { path = "tux-rig" }
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tux-rig/Cargo.toml src-tauri/tux-rig/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat(tux-rig): scaffold crate + RigError

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `Mode` enum + rigctl mapping

**Files:**
- Create: `src-tauri/tux-rig/src/mode.rs`
- Modify: `src-tauri/tux-rig/src/lib.rs` (add `mod mode; pub use mode::Mode;`)

**Interfaces:**
- Produces: `enum Mode { PktUsb, Usb, Lsb, Pktlsb, DataU, DataL }`; `Mode::rigctl_str(&self) -> &'static str`; `Mode::from_rigctl(&str) -> Option<Mode>`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/tux-rig/src/mode.rs`:
```rust
//! Radio data/voice modes and their Hamlib `rigctl` string forms.

/// A subset of Hamlib modes relevant to HF Winlink. `rigctl_str` is the exact
/// token rigctld expects after `M`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    PktUsb,
    Usb,
    Lsb,
    PktLsb,
    DataU,
    DataL,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ft710_data_mode_is_pktusb() {
        assert_eq!(Mode::PktUsb.rigctl_str(), "PKTUSB");
    }

    #[test]
    fn round_trips_through_rigctl_str() {
        for m in [Mode::PktUsb, Mode::Usb, Mode::Lsb, Mode::PktLsb, Mode::DataU, Mode::DataL] {
            assert_eq!(Mode::from_rigctl(m.rigctl_str()), Some(m));
        }
    }

    #[test]
    fn unknown_mode_is_none() {
        assert_eq!(Mode::from_rigctl("FM"), None);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run (in CI / when available): `cargo test -p tux-rig mode::`
Expected: FAIL — `rigctl_str` / `from_rigctl` not defined.

- [ ] **Step 3: Implement the mapping**

Add to `src-tauri/tux-rig/src/mode.rs` above the `#[cfg(test)]` block:
```rust
impl Mode {
    /// The exact token rigctld's `M` command expects.
    pub fn rigctl_str(&self) -> &'static str {
        match self {
            Mode::PktUsb => "PKTUSB",
            Mode::Usb => "USB",
            Mode::Lsb => "LSB",
            Mode::PktLsb => "PKTLSB",
            Mode::DataU => "USB-D",
            Mode::DataL => "LSB-D",
        }
    }

    /// Parse a rigctld mode token back into a `Mode`.
    pub fn from_rigctl(s: &str) -> Option<Mode> {
        match s.trim() {
            "PKTUSB" => Some(Mode::PktUsb),
            "USB" => Some(Mode::Usb),
            "LSB" => Some(Mode::Lsb),
            "PKTLSB" => Some(Mode::PktLsb),
            "USB-D" => Some(Mode::DataU),
            "LSB-D" => Some(Mode::DataL),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: Wire the module + run the test**

Add to `src-tauri/tux-rig/src/lib.rs`:
```rust
mod mode;
pub use mode::Mode;
```
Run: `cargo test -p tux-rig mode::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tux-rig/src/mode.rs src-tauri/tux-rig/src/lib.rs
git commit -m "feat(tux-rig): Mode enum + rigctl string mapping

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `rigctl` protocol encode/decode (pure)

**Files:**
- Create: `src-tauri/tux-rig/src/protocol.rs`
- Modify: `src-tauri/tux-rig/src/lib.rs` (`mod protocol;`)

**Interfaces:**
- Consumes: `Mode` (Task 2), `RigError` (Task 1).
- Produces:
  - `fn cmd_set_freq(hz: u64) -> String`
  - `fn cmd_set_mode(mode: Mode) -> String`
  - `fn cmd_set_ptt(on: bool) -> String`
  - `const CMD_GET_FREQ: &str`, `const CMD_GET_MODE: &str`, `const CMD_GET_PTT: &str`
  - `fn parse_rprt(line: &str) -> Result<(), RigError>`
  - `fn parse_freq(line: &str) -> Result<u64, RigError>`

- [ ] **Step 1: Write the failing tests**

`src-tauri/tux-rig/src/protocol.rs`:
```rust
//! rigctl TCP wire forms. Pure string in / string out so the protocol is
//! testable without a socket. rigctld terminates each command response; on
//! success a *set* returns `RPRT 0`, a *get* returns the value line(s).

use crate::{Mode, RigError};

pub const CMD_GET_FREQ: &str = "f\n";
pub const CMD_GET_MODE: &str = "m\n";
pub const CMD_GET_PTT: &str = "t\n";

/// `F <Hz>` — set VFO frequency in Hz.
pub fn cmd_set_freq(hz: u64) -> String {
    format!("F {hz}\n")
}

/// `M <mode> 0` — set mode; passband `0` = rig default.
pub fn cmd_set_mode(mode: Mode) -> String {
    format!("M {} 0\n", mode.rigctl_str())
}

/// `T 1` / `T 0` — set PTT.
pub fn cmd_set_ptt(on: bool) -> String {
    format!("T {}\n", if on { 1 } else { 0 })
}

/// Parse a `RPRT <code>` reply. `RPRT 0` = ok; anything else = `RigError::Rprt`.
pub fn parse_rprt(line: &str) -> Result<(), RigError> {
    let t = line.trim();
    let code = t
        .strip_prefix("RPRT ")
        .ok_or_else(|| RigError::Protocol(format!("expected RPRT reply, got {t:?}")))?;
    let n: i32 = code
        .trim()
        .parse()
        .map_err(|_| RigError::Protocol(format!("bad RPRT code {code:?}")))?;
    if n == 0 {
        Ok(())
    } else {
        Err(RigError::Rprt(n))
    }
}

/// Parse the single value line returned by `f` (frequency in Hz).
pub fn parse_freq(line: &str) -> Result<u64, RigError> {
    line.trim()
        .parse()
        .map_err(|_| RigError::Protocol(format!("bad frequency line {line:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_freq_is_F_space_hz_newline() {
        assert_eq!(cmd_set_freq(7_102_000), "F 7102000\n");
    }

    #[test]
    fn set_mode_pktusb() {
        assert_eq!(cmd_set_mode(Mode::PktUsb), "M PKTUSB 0\n");
    }

    #[test]
    fn set_ptt_on_off() {
        assert_eq!(cmd_set_ptt(true), "T 1\n");
        assert_eq!(cmd_set_ptt(false), "T 0\n");
    }

    #[test]
    fn rprt_zero_is_ok() {
        assert!(parse_rprt("RPRT 0\n").is_ok());
    }

    #[test]
    fn rprt_nonzero_is_err_with_code() {
        match parse_rprt("RPRT -1\n") {
            Err(RigError::Rprt(-1)) => {}
            other => panic!("expected Rprt(-1), got {other:?}"),
        }
    }

    #[test]
    fn rprt_garbage_is_protocol_err() {
        assert!(matches!(parse_rprt("hello"), Err(RigError::Protocol(_))));
    }

    #[test]
    fn parse_freq_reads_hz() {
        assert_eq!(parse_freq("7102000\n").unwrap(), 7_102_000);
    }
}
```

- [ ] **Step 2: Run to verify it fails, then passes**

Add `mod protocol;` to `src-tauri/tux-rig/src/lib.rs`.
Run: `cargo test -p tux-rig protocol::`
Expected: PASS (7 tests). (Implementation is written alongside the tests above; if authored test-first, the `cmd_*`/`parse_*` bodies are Step 3.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tux-rig/src/protocol.rs src-tauri/tux-rig/src/lib.rs
git commit -m "feat(tux-rig): rigctl protocol encode/decode

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `RigctldClient` over TCP + the `Rig` trait

**Files:**
- Create: `src-tauri/tux-rig/src/client.rs`
- Modify: `src-tauri/tux-rig/src/lib.rs` (add `Rig` trait, `RigStatus`, `mod client;`)

**Interfaces:**
- Consumes: `protocol::*`, `Mode`, `RigError`.
- Produces:
  - `trait Rig { fn set_freq(&mut self, hz: u64) -> Result<(), RigError>; fn set_mode(&mut self, mode: Mode) -> Result<(), RigError>; fn ptt(&mut self, on: bool) -> Result<(), RigError>; fn read_status(&mut self) -> Result<RigStatus, RigError>; }`
  - `struct RigStatus { freq_hz: u64, mode: Option<Mode>, ptt: bool }`
  - `struct RigctldClient` with `connect(host: &str, port: u16) -> Result<Self, RigError>` and `impl Rig`.

- [ ] **Step 1: Define the trait + status in `lib.rs`**

Add to `src-tauri/tux-rig/src/lib.rs`:
```rust
mod client;
pub use client::RigctldClient;

/// A snapshot of rig state from `read_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RigStatus {
    pub freq_hz: u64,
    pub mode: Option<Mode>,
    pub ptt: bool,
}

/// Frequency/mode/PTT control of a radio.
pub trait Rig {
    fn set_freq(&mut self, hz: u64) -> Result<(), RigError>;
    fn set_mode(&mut self, mode: Mode) -> Result<(), RigError>;
    fn ptt(&mut self, on: bool) -> Result<(), RigError>;
    fn read_status(&mut self) -> Result<RigStatus, RigError>;
}
```

- [ ] **Step 2: Write the failing test against a fake rigctld**

`src-tauri/tux-rig/src/client.rs`:
```rust
//! `rigctld` TCP client. One request line → one reply (set: `RPRT n`;
//! get: value line(s)). The client opens a short-lived line exchange per call.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::protocol::{self, CMD_GET_FREQ, CMD_GET_MODE, CMD_GET_PTT};
use crate::{Mode, Rig, RigError, RigStatus};

/// A connected rigctld control client.
pub struct RigctldClient {
    stream: TcpStream,
}

impl RigctldClient {
    /// Connect to a running rigctld at `host:port`.
    pub fn connect(host: &str, port: u16) -> Result<Self, RigError> {
        let stream = TcpStream::connect((host, port))?;
        Ok(Self { stream })
    }

    /// Send one command and return the first reply line, trimmed.
    fn exchange(&mut self, cmd: &str) -> Result<String, RigError> {
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.flush()?;
        let mut reader = BufReader::new(&self.stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        Ok(line.trim_end().to_string())
    }
}

impl Rig for RigctldClient {
    fn set_freq(&mut self, hz: u64) -> Result<(), RigError> {
        let reply = self.exchange(&protocol::cmd_set_freq(hz))?;
        protocol::parse_rprt(&reply)
    }

    fn set_mode(&mut self, mode: Mode) -> Result<(), RigError> {
        let reply = self.exchange(&protocol::cmd_set_mode(mode))?;
        protocol::parse_rprt(&reply)
    }

    fn ptt(&mut self, on: bool) -> Result<(), RigError> {
        let reply = self.exchange(&protocol::cmd_set_ptt(on))?;
        protocol::parse_rprt(&reply)
    }

    fn read_status(&mut self) -> Result<RigStatus, RigError> {
        let freq = protocol::parse_freq(&self.exchange(CMD_GET_FREQ)?)?;
        let mode = Mode::from_rigctl(&self.exchange(CMD_GET_MODE)?);
        let ptt = self.exchange(CMD_GET_PTT)?.trim() == "1";
        Ok(RigStatus { freq_hz: freq, mode, ptt })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    /// Spawn a one-shot fake rigctld that answers `set` with `RPRT 0` and the
    /// three getters with fixed values. Returns the bound port.
    fn fake_rigctld() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 {
                let cmd = line.trim_end();
                let reply = match cmd.chars().next() {
                    Some('F') | Some('M') | Some('T') => "RPRT 0\n".to_string(),
                    Some('f') => "7102000\n".to_string(),
                    Some('m') => "PKTUSB\n3000\n".to_string(),
                    Some('t') => "0\n".to_string(),
                    _ => "RPRT -1\n".to_string(),
                };
                writer.write_all(reply.as_bytes()).unwrap();
                writer.flush().unwrap();
                line.clear();
            }
        });
        port
    }

    #[test]
    fn set_freq_succeeds_against_fake() {
        let port = fake_rigctld();
        let mut c = RigctldClient::connect("127.0.0.1", port).unwrap();
        c.set_freq(7_102_000).unwrap();
    }

    #[test]
    fn set_mode_succeeds_against_fake() {
        let port = fake_rigctld();
        let mut c = RigctldClient::connect("127.0.0.1", port).unwrap();
        c.set_mode(Mode::PktUsb).unwrap();
    }

    #[test]
    fn read_status_parses_freq_and_mode() {
        let port = fake_rigctld();
        let mut c = RigctldClient::connect("127.0.0.1", port).unwrap();
        let s = c.read_status().unwrap();
        assert_eq!(s.freq_hz, 7_102_000);
        assert_eq!(s.mode, Some(Mode::PktUsb));
        assert!(!s.ptt);
    }
}
```

> **Note for the implementer:** `read_status` uses a fresh `BufReader` per `exchange`, which drops buffered bytes between calls. For the get-mode reply rigctld emits two lines (`PKTUSB\n<passband>\n`); only the first is consumed, which is correct here. If a future change pipelines multiple gets, hold one persistent `BufReader` on the struct instead. Keep the per-call form for now (YAGNI).

- [ ] **Step 2b: Run the tests**

Run: `cargo test -p tux-rig client::`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tux-rig/src/client.rs src-tauri/tux-rig/src/lib.rs
git commit -m "feat(tux-rig): RigctldClient + Rig trait over rigctl TCP

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `ManagedRig` — spawn rigctld + close-serial lifecycle

**Files:**
- Create: `src-tauri/tux-rig/src/managed.rs`
- Modify: `src-tauri/tux-rig/src/lib.rs` (`mod managed; pub use managed::{ManagedRig, RigConfig};`)

**Interfaces:**
- Consumes: `RigctldClient`, `Rig`, `Mode`, `RigError`.
- Produces:
  - `struct RigConfig { binary: String, model: u32, serial_path: String, baud: u32, host: String, port: u16 }`
  - `impl RigConfig { fn rigctld_args(&self) -> Vec<String> }`
  - `struct ManagedRig` with `spawn(cfg: RigConfig) -> Result<Self, RigError>`, `tune(&mut self, hz: u64, mode: Mode) -> Result<(), RigError>`, `release_serial(&mut self)` (stop the subprocess — close-serial), `status(&mut self) -> Result<RigStatus, RigError>`, `Drop`.

- [ ] **Step 1: Write the failing test for `rigctld_args`**

`src-tauri/tux-rig/src/managed.rs`:
```rust
//! Managed rigctld subprocess + tune/close-serial lifecycle.
//!
//! On internal-codec radios, `release_serial` STOPS rigctld after tuning so the
//! CAT serial is free before audio streams (see project_ft710_internal_codec_tx_reset).
//! On the DRA-100 path the caller simply never calls `release_serial`.

use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{Mode, RigError, RigStatus, RigctldClient, Rig};

/// How rigctld is invoked + where its control socket lives.
#[derive(Debug, Clone)]
pub struct RigConfig {
    pub binary: String,
    pub model: u32,
    pub serial_path: String,
    pub baud: u32,
    pub host: String,
    pub port: u16,
}

impl RigConfig {
    /// Argv (after the binary) for `rigctld -m <model> -r <serial> -s <baud> -t <port>`.
    pub fn rigctld_args(&self) -> Vec<String> {
        vec![
            "-m".into(), self.model.to_string(),
            "-r".into(), self.serial_path.clone(),
            "-s".into(), self.baud.to_string(),
            "-t".into(), self.port.to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ft710_args() {
        let cfg = RigConfig {
            binary: "rigctld".into(),
            model: 1049,
            serial_path: "/dev/ttyUSB0".into(),
            baud: 38400,
            host: "127.0.0.1".into(),
            port: 4532,
        };
        assert_eq!(
            cfg.rigctld_args(),
            vec!["-m", "1049", "-r", "/dev/ttyUSB0", "-s", "38400", "-t", "4532"],
        );
    }
}
```

- [ ] **Step 2: Run to verify the args test fails, then passes**

Add `mod managed; pub use managed::{ManagedRig, RigConfig};` to `lib.rs` (ManagedRig added in Step 3).
Run: `cargo test -p tux-rig managed::tests::ft710_args`
Expected: PASS.

- [ ] **Step 3: Implement `ManagedRig`**

Add to `src-tauri/tux-rig/src/managed.rs` (above the test module):
```rust
/// A spawned rigctld plus a connected client. Stops the subprocess on drop.
pub struct ManagedRig {
    child: Option<Child>,
    client: RigctldClient,
    cfg: RigConfig,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_POLL: Duration = Duration::from_millis(100);
const STOP_GRACE: Duration = Duration::from_millis(500);

impl ManagedRig {
    /// Spawn rigctld and connect a control client once its socket accepts.
    pub fn spawn(cfg: RigConfig) -> Result<Self, RigError> {
        let child = Command::new(&cfg.binary)
            .args(cfg.rigctld_args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RigError::Spawn(format!("failed to spawn {}: {e}", cfg.binary)))?;

        let start = Instant::now();
        let client = loop {
            match RigctldClient::connect(&cfg.host, cfg.port) {
                Ok(c) => break c,
                Err(_) if start.elapsed() < CONNECT_TIMEOUT => {
                    thread::sleep(CONNECT_POLL);
                }
                Err(e) => return Err(e),
            }
        };

        Ok(Self { child: Some(child), client, cfg })
    }

    /// Set frequency (Hz) then mode. Order matters: freq before mode mirrors WLE.
    pub fn tune(&mut self, hz: u64, mode: Mode) -> Result<(), RigError> {
        self.client.set_freq(hz)?;
        self.client.set_mode(mode)?;
        Ok(())
    }

    /// Read the current rig state.
    pub fn status(&mut self) -> Result<RigStatus, RigError> {
        self.client.read_status()
    }

    /// Close-serial: stop rigctld so the CAT serial is released before audio.
    /// Idempotent. After this, `tune`/`status` will fail until `spawn` is called
    /// again (the caller re-spawns on the next connect).
    pub fn release_serial(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ManagedRig {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort SIGKILL + reap within grace; rigctld has no clean-stop
            // protocol and holds only the serial, so kill is safe.
            let _ = child.kill();
            let deadline = Instant::now() + STOP_GRACE;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = child.wait();
        }
    }
}
```

- [ ] **Step 4: Add a lifecycle test against a fake rigctld binary**

Append to the `#[cfg(test)] mod tests` in `managed.rs`:
```rust
    use std::io::Write;

    /// Write a tiny shell script that behaves like rigctld: bind the `-t <port>`
    /// TCP port and answer set/get. Returns its path (in a unique temp dir).
    fn fake_rigctld_script(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("fake-rigctld.sh");
        let script = r#"#!/usr/bin/env bash
# crude rigctld: parse -t PORT, listen, answer one client with fixed replies.
port=4532
while [ $# -gt 0 ]; do case "$1" in -t) port="$2"; shift 2;; *) shift;; esac; done
# Use ncat/nc if present; emit RPRT 0 for sets, values for gets.
exec 1>/dev/null 2>&1
# Loop forever serving; the test only needs one connection.
while true; do
  { while read -r line; do
      case "$line" in
        F*|M*|T*) printf 'RPRT 0\n';;
        f*) printf '7102000\n';;
        m*) printf 'PKTUSB\n3000\n';;
        t*) printf '0\n';;
        *) printf 'RPRT -1\n';;
      esac
    done; } | nc -l 127.0.0.1 "$port" 2>/dev/null || sleep 0.2
done
"#;
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    // Gated: requires `nc` (netcat). Skips cleanly if absent so CI without nc
    // still passes the pure tests. Run explicitly with `--ignored` where nc exists.
    #[test]
    #[ignore = "requires netcat; run where available"]
    fn spawn_tune_and_release() {
        let tmp = std::env::temp_dir().join(format!("tuxrig-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let script = fake_rigctld_script(&tmp);
        // pick an ephemeral-ish high port
        let port = 45_321;
        let cfg = RigConfig {
            binary: script.to_string_lossy().into_owned(),
            model: 1049,
            serial_path: "/dev/null".into(),
            baud: 38400,
            host: "127.0.0.1".into(),
            port,
        };
        let mut rig = ManagedRig::spawn(cfg).expect("spawn fake rigctld");
        rig.tune(7_102_000, Mode::PktUsb).expect("tune");
        rig.release_serial();
        // After release, a tune attempt errors (socket gone).
        assert!(rig.tune(7_102_000, Mode::PktUsb).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
```

> The pure `rigctld_args` test is the CI gate; the `#[ignore]` lifecycle test documents+exercises the spawn path where `nc` exists (the operator's Pi). The `protocol`/`client` tests already cover the wire behavior against an in-process fake without external tools.

- [ ] **Step 5: Run + commit**

Run: `cargo test -p tux-rig` (expected: all non-ignored pass).
```bash
git add src-tauri/tux-rig/src/managed.rs src-tauri/tux-rig/src/lib.rs
git commit -m "feat(tux-rig): ManagedRig spawn/tune/release-serial lifecycle

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

# Phase 2 — Config

## Task 6: `ArdopUiConfig` rig fields + TS DTO mirror

**Files:**
- Modify: `src-tauri/src/config.rs:859-1030` (`ArdopUiConfig` + `Default`)
- Modify: `src/radio/modes/ArdopRadioPanel.tsx:262-293` (`ArdopFullConfig`)

**Interfaces:**
- Produces (Rust): new `ArdopUiConfig` fields — `rig_hamlib_model: Option<u32>`, `rigctld_host: String`, `rigctld_port: u16`, `rigctld_binary: String`, `close_serial_sequencing: bool`, `live_vfo_poll: bool`, `qsy_on_fail: bool`. (CAT serial reuses the existing `cat_serial_path` + `cat_baud`.)
- Produces (TS): matching snake_case fields on `ArdopFullConfig`.

- [ ] **Step 1: Write the failing Rust test (defaults)**

Add to the `#[cfg(test)] mod tests` near `ArdopUiConfig` in `config.rs`:
```rust
#[test]
fn ardop_ui_config_rig_defaults() {
    let c = ArdopUiConfig::default();
    assert_eq!(c.rig_hamlib_model, None);
    assert_eq!(c.rigctld_host, "127.0.0.1");
    assert_eq!(c.rigctld_port, 4532);
    assert_eq!(c.rigctld_binary, "rigctld");
    assert!(!c.close_serial_sequencing);
    assert!(!c.live_vfo_poll);
    assert!(!c.qsy_on_fail);
}

#[test]
fn ardop_ui_config_rig_fields_round_trip_json() {
    let mut c = ArdopUiConfig::default();
    c.rig_hamlib_model = Some(1049);
    c.close_serial_sequencing = true;
    let json = serde_json::to_string(&c).unwrap();
    let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.rig_hamlib_model, Some(1049));
    assert!(back.close_serial_sequencing);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p tuxlink ardop_ui_config_rig`
Expected: FAIL — fields don't exist.

- [ ] **Step 3: Add the fields + defaults**

In `ArdopUiConfig` (after `listen_ttl_minutes`):
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig_hamlib_model: Option<u32>,
    #[serde(default = "default_rigctld_host")]
    pub rigctld_host: String,
    #[serde(default = "default_rigctld_port")]
    pub rigctld_port: u16,
    #[serde(default = "default_rigctld_binary")]
    pub rigctld_binary: String,
    #[serde(default)]
    pub close_serial_sequencing: bool,
    #[serde(default)]
    pub live_vfo_poll: bool,
    #[serde(default)]
    pub qsy_on_fail: bool,
```
Add default fns near the existing `default_cat_baud` etc.:
```rust
fn default_rigctld_host() -> String { "127.0.0.1".into() }
fn default_rigctld_port() -> u16 { 4532 }
fn default_rigctld_binary() -> String { "rigctld".into() }
```
Add the new fields to `impl Default for ArdopUiConfig`:
```rust
            rig_hamlib_model: None,
            rigctld_host: default_rigctld_host(),
            rigctld_port: default_rigctld_port(),
            rigctld_binary: default_rigctld_binary(),
            close_serial_sequencing: false,
            live_vfo_poll: false,
            qsy_on_fail: false,
```

> Note: `ArdopUiConfig` derives `Serialize` only (not `Deserialize`) in the struct attr shown — verify the existing derive. If `Deserialize` is absent, the round-trip test needs it; add `Deserialize` to the derive list (the struct already round-trips through config read, so it is present in practice — confirm and keep consistent).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p tuxlink ardop_ui_config_rig`
Expected: PASS (2 tests).

- [ ] **Step 5: Mirror the TS DTO**

In `src/radio/modes/ArdopRadioPanel.tsx`, extend `interface ArdopFullConfig`:
```typescript
  rig_hamlib_model: number | null;
  rigctld_host: string;
  rigctld_port: number;
  rigctld_binary: string;
  close_serial_sequencing: boolean;
  live_vfo_poll: boolean;
  qsy_on_fail: boolean;
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/config.rs src/radio/modes/ArdopRadioPanel.tsx
git commit -m "feat(config): rigctld + close-serial + qsy fields on ArdopUiConfig (+TS DTO)

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

# Phase 3 — Connect-flow tune step

## Task 7: `rig_config_from` translation + `ardop_tune_rig` command (Tune-only)

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` (new helper + new `#[tauri::command]`; register in the invoke handler / `lib.rs` generate_handler list)

**Interfaces:**
- Consumes: `ArdopUiConfig` (Task 6), `tux_rig::{ManagedRig, RigConfig, Mode}`.
- Produces:
  - `fn rig_config_from(ardop_ui: &ArdopUiConfig) -> Option<tux_rig::RigConfig>` — `None` when `rig_hamlib_model`/`cat_serial_path` absent (rig control not configured).
  - `fn ardop_data_mode() -> tux_rig::Mode` (PKTUSB for HF Winlink).
  - `#[tauri::command] fn ardop_tune_rig(freq_hz: u64) -> Result<(), String>` — spawn rigctld, tune, then drop (release). Tune-only path.

- [ ] **Step 1: Write the failing test for `rig_config_from`**

Add to the `#[cfg(test)] mod tests` in `modem_commands.rs`:
```rust
#[test]
fn rig_config_present_when_model_and_serial_set() {
    let mut ui = ArdopUiConfig::default();
    ui.rig_hamlib_model = Some(1049);
    ui.cat_serial_path = Some("/dev/ttyUSB0".into());
    let rc = rig_config_from(&ui).expect("rig config");
    assert_eq!(rc.model, 1049);
    assert_eq!(rc.serial_path, "/dev/ttyUSB0");
    assert_eq!(rc.port, 4532);
    assert_eq!(rc.binary, "rigctld");
}

#[test]
fn rig_config_absent_when_unconfigured() {
    let ui = ArdopUiConfig::default();
    assert!(rig_config_from(&ui).is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tuxlink rig_config`
Expected: FAIL — `rig_config_from` undefined.

- [ ] **Step 3: Implement the helper + Tune-only command**

Add to `modem_commands.rs`:
```rust
/// Build a `tux_rig::RigConfig` from the ARDOP UI config, or `None` if rig
/// control is not configured (no hamlib model or no CAT serial).
pub(crate) fn rig_config_from(ardop_ui: &ArdopUiConfig) -> Option<tux_rig::RigConfig> {
    let model = ardop_ui.rig_hamlib_model?;
    let serial_path = ardop_ui
        .cat_serial_path
        .clone()
        .filter(|p| !p.trim().is_empty())?;
    Some(tux_rig::RigConfig {
        binary: ardop_ui.rigctld_binary.clone(),
        model,
        serial_path,
        baud: ardop_ui.cat_baud,
        host: ardop_ui.rigctld_host.clone(),
        port: ardop_ui.rigctld_port,
    })
}

/// HF Winlink data mode (FT-710 = PKTUSB).
pub(crate) fn ardop_data_mode() -> tux_rig::Mode {
    tux_rig::Mode::PktUsb
}

/// Tune-only: set the rig to `freq_hz` + the HF data mode over CAT, then release
/// the serial (drop). Does NOT dial. Used by the "Tune…" affordance.
#[tauri::command]
pub fn ardop_tune_rig(freq_hz: u64) -> Result<(), String> {
    let cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    let ardop_ui = cfg.modem_ardop.unwrap_or_default();
    let rc = rig_config_from(&ardop_ui)
        .ok_or_else(|| "rig control not configured — set the rig model + CAT serial".to_string())?;
    let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
    rig.tune(freq_hz, ardop_data_mode()).map_err(|e| e.to_string())?;
    // Drop releases the serial (close-serial-safe for internal-codec radios).
    Ok(())
}
```
Register `ardop_tune_rig` in the Tauri `generate_handler!` list (search `modem_ardop_connect,` in `src-tauri/src/lib.rs` and add `ardop_tune_rig,` next to it).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p tuxlink rig_config`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/modem_commands.rs src-tauri/src/lib.rs
git commit -m "feat(rig): rig_config_from + ardop_tune_rig Tune-only command

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Tune step in the connect flow (pre-audio)

**Files:**
- Modify: `src-tauri/src/modem_commands.rs:342-388` (`modem_ardop_connect_post_consume_with_factory`)

**Interfaces:**
- Consumes: `rig_config_from`, `ardop_data_mode`, the per-connect `freq_hz`, `ManagedRig`.
- Produces: a `tune_rig_for_connect(ardop_ui, freq_hz) -> Result<Option<ManagedRig>, String>` helper that tunes pre-audio and returns the live `ManagedRig` (DRA-100 path keeps it; close-serial path releases it).

> **Design:** Tune happens after `transport.init()` succeeds and before `connect_arq()`. On `close_serial_sequencing`, drop/`release_serial` the rig BEFORE `connect_arq` (which starts audio). Otherwise keep the `ManagedRig` alive (DRA-100 holds CAT freely) and store it on the session so it stops on disconnect.

- [ ] **Step 1: Write the failing test for the tune helper's release logic**

Add to `modem_commands.rs` tests (pure decision function, no real rig):
```rust
#[test]
fn close_serial_releases_rig_before_audio() {
    // close_serial_sequencing = true → helper must NOT retain the rig handle.
    let mut ui = ArdopUiConfig::default();
    ui.close_serial_sequencing = true;
    assert!(should_release_after_tune(&ui));
}

#[test]
fn dra100_path_retains_rig() {
    let ui = ArdopUiConfig::default(); // close_serial_sequencing = false
    assert!(!should_release_after_tune(&ui));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tuxlink _after_tune`
Expected: FAIL — `should_release_after_tune` undefined.

- [ ] **Step 3: Implement the helper + insert the tune step**

Add the decision fn:
```rust
/// Whether to stop rigctld (release the CAT serial) immediately after tuning,
/// before audio. True on internal-codec radios (close-serial sequencing).
pub(crate) fn should_release_after_tune(ardop_ui: &ArdopUiConfig) -> bool {
    ardop_ui.close_serial_sequencing
}
```
In `modem_ardop_connect_post_consume_with_factory`, between the `transport.init()` success (line ~344) and the `transport.connect_arq()` call (line ~388), insert (the `freq_hz: Option<u64>` parameter is threaded in from the connect command — see Task 8 Step 4):
```rust
    // --- Pre-audio CAT tune (tux-rig) ---
    // Only when rig control is configured AND a target frequency is known.
    let mut live_rig: Option<tux_rig::ManagedRig> = None;
    if let (Some(rc), Some(hz)) = (rig_config_from(ardop_ui), freq_hz) {
        let mut rig = tux_rig::ManagedRig::spawn(rc)
            .map_err(|e| format!("rigctld spawn failed: {e}"))?;
        rig.tune(hz, ardop_data_mode())
            .map_err(|e| format!("CAT tune failed: {e}"))?;
        if should_release_after_tune(ardop_ui) {
            rig.release_serial(); // close-serial: free the serial before audio
        } else {
            live_rig = Some(rig); // DRA-100: keep CAT up for the session
        }
    }
    // `live_rig` is moved into the session below so it stops on disconnect.
```
Store `live_rig` on the `ModemSession` (add an `Option<tux_rig::ManagedRig>` slot to the session struct, dropped on disconnect/teardown alongside the transport). If a session slot is out of scope here, bind `live_rig` for the connection lifetime by handing it to `session.install_transport` via a companion field; the minimal form is a `session.set_rig(live_rig)` setter mirroring `install_transport`.

- [ ] **Step 4: Thread `freq_hz` through the connect command**

`modem_ardop_connect` (line ~1123) gains an optional `freq_hz: Option<u64>` arg from the frontend; pass it down through `modem_ardop_connect_post_consume_with_factory`. The frontend sends the frequency element's value (Task 10). When `None` (no freq known), the tune step is skipped — dialing proceeds without retuning (back-compat with today).

- [ ] **Step 5: Run + commit**

Run: `cargo test -p tuxlink` (the pure decision tests pass; the tune step itself is integration-tested via the connect-flow factory tests already present — extend one to assert tune-then-dial ordering with a fake rig if a `Rig` seam is injected; otherwise the `tux-rig` unit tests + the decision-fn tests are the gate).
```bash
git add src-tauri/src/modem_commands.rs
git commit -m "feat(rig): pre-audio CAT tune step in ARDOP connect flow

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

# Phase 4 — QSY-on-fail (operator-selectable)

## Task 9: Ordered-list QSY loop, gated by `qsy_on_fail`

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` (connect command accepts an ordered list; loop on failure)

**Interfaces:**
- Consumes: `qsy_on_fail` config, an ordered `Vec<DialCandidate>` from the frontend.
- Produces: `struct DialCandidate { target: String, freq_hz: Option<u64> }`; a connect path that, when `qsy_on_fail`, walks candidates tune→dial until one connects or the list is exhausted.

- [ ] **Step 1: Write the failing test for candidate-walking order**

```rust
#[test]
fn qsy_walks_candidates_until_first_success() {
    // A pure planner: given outcomes [fail, fail, ok], the planner reports it
    // attempted indices [0,1,2] and stopped at 2.
    let candidates = vec![
        DialCandidate { target: "W7DG".into(), freq_hz: Some(7_102_000) },
        DialCandidate { target: "KE7XYZ".into(), freq_hz: Some(10_145_500) },
        DialCandidate { target: "N6ARA".into(), freq_hz: Some(14_109_000) },
    ];
    let mut attempted = Vec::new();
    let outcome = walk_candidates(&candidates, true, |idx, _c| {
        attempted.push(idx);
        idx == 2 // succeed on the third
    });
    assert_eq!(attempted, vec![0, 1, 2]);
    assert_eq!(outcome, Some(2));
}

#[test]
fn no_qsy_attempts_only_first() {
    let candidates = vec![
        DialCandidate { target: "W7DG".into(), freq_hz: Some(7_102_000) },
        DialCandidate { target: "KE7XYZ".into(), freq_hz: Some(10_145_500) },
    ];
    let mut attempted = Vec::new();
    let outcome = walk_candidates(&candidates, false, |idx, _c| {
        attempted.push(idx);
        false // first fails
    });
    assert_eq!(attempted, vec![0]); // qsy off → no walk
    assert_eq!(outcome, None);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tuxlink candidates`
Expected: FAIL — `walk_candidates`/`DialCandidate` undefined.

- [ ] **Step 3: Implement the pure planner**

```rust
/// One dial target + its tune frequency.
#[derive(Debug, Clone)]
pub struct DialCandidate {
    pub target: String,
    pub freq_hz: Option<u64>,
}

/// Walk candidates in order, calling `attempt(idx, candidate)` which returns
/// `true` on a successful connect. Stops at the first success and returns its
/// index. When `qsy_on_fail` is false, only the first candidate is attempted.
/// Returns `None` if no attempt succeeded.
pub(crate) fn walk_candidates<F>(
    candidates: &[DialCandidate],
    qsy_on_fail: bool,
    mut attempt: F,
) -> Option<usize>
where
    F: FnMut(usize, &DialCandidate) -> bool,
{
    for (idx, c) in candidates.iter().enumerate() {
        if attempt(idx, c) {
            return Some(idx);
        }
        if !qsy_on_fail {
            break;
        }
    }
    None
}
```

- [ ] **Step 4: Wire the planner into the connect command**

The connect command receives `candidates: Vec<DialCandidate>` (frontend sends the selected dial as a 1-element list, or the ordered ranked list when the operator enabled auto-QSY). Each `attempt` closure runs the existing tune (Task 8) + `connect_arq`, recording the per-candidate outcome via the favorites attempt log (`favorite_record_attempt`). Honor the in-flight abort side channel between attempts (check the abort flag; stop the walk on abort).

- [ ] **Step 5: Run + commit**

Run: `cargo test -p tuxlink candidates`
Expected: PASS (2 tests).
```bash
git add src-tauri/src/modem_commands.rs
git commit -m "feat(rig): operator-gated QSY-on-fail candidate walk

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

# Phase 5 — Modem-pane UI

## Task 10: Frequency & mode element in the connect area

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx` (connect section render + state + `modem_ardop_connect` invoke)
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx`

**Interfaces:**
- Consumes: `ArdopFullConfig` (Task 6), `handlePrefill`/`FavoriteDial` (carry `freq`).
- Produces: a frequency string state `freqMhz`, parsed to Hz for the connect invoke; a **Tune…** button calling `ardop_tune_rig`.

- [ ] **Step 1: Write the failing component test**

Add to `ArdopRadioPanel.test.tsx`:
```typescript
it('shows the frequency element and sends freq_hz on Connect', async () => {
  const invoke = await mockInvoke();
  renderPanel(<ArdopRadioPanel onClose={() => {}} />);
  // type a target + frequency
  fireEvent.change(screen.getByTestId('ardop-target'), { target: { value: 'W7DG' } });
  fireEvent.change(screen.getByTestId('ardop-freq'), { target: { value: '7.102' } });
  fireEvent.click(screen.getByTestId('ardop-connect'));
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith('modem_ardop_connect', expect.objectContaining({
      target: 'W7DG',
      freqHz: 7102000,
    }));
  });
});
```
> Match the existing test harness's invoke-mock accessor (`mockInvoke`/`defaultInvokeImpl` pattern shown in the file). Use the file's existing `renderPanel` helper.

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "frequency element"`
Expected: FAIL — no `ardop-freq` element / no `freqHz` arg.

- [ ] **Step 3: Implement the frequency element**

Add state + a parse helper near the other `useState` in `ArdopRadioPanel`:
```typescript
const [freqMhz, setFreqMhz] = useState('');
// Parse "7.102" (MHz) → 7102000 Hz; null when blank/invalid.
const freqHz = useMemo(() => {
  const t = freqMhz.trim();
  if (!t) return null;
  const mhz = Number(t);
  if (!Number.isFinite(mhz) || mhz <= 0) return null;
  return Math.round(mhz * 1_000_000);
}, [freqMhz]);
```
In the connect section JSX (after the Target input), using existing classes:
```tsx
<div className="radio-panel-input-row">
  <label htmlFor="ardop-freq">Frequency (MHz)</label>
  <input
    id="ardop-freq"
    data-testid="ardop-freq"
    className="radio-panel-input radio-panel-mono"
    value={freqMhz}
    onChange={(e) => setFreqMhz(e.target.value)}
    placeholder="7.102"
    inputMode="decimal"
  />
  <button
    type="button"
    className="radio-panel-btn radio-panel-btn-sm"
    data-testid="ardop-tune"
    disabled={freqHz === null}
    onClick={() => { if (freqHz !== null) void invoke('ardop_tune_rig', { freqHz }); }}
  >
    Tune…
  </button>
</div>
```
Update the Connect invoke (line ~732) to include the frequency:
```typescript
await invoke('modem_ardop_connect', {
  target: target.trim(),
  freqHz, // null when unknown → backend skips retune
});
```
Add `data-testid="ardop-target"` to the target input and `data-testid="ardop-connect"` to the Connect button if not already present.

- [ ] **Step 4: Run to verify pass + commit**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx`
Expected: PASS.
```bash
git add src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/ArdopRadioPanel.test.tsx
git commit -m "feat(ui): ARDOP frequency element + Tune-only button

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Prefill carries frequency from Find a Station

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx:387-420` (`handlePrefill`)
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx`

**Interfaces:**
- Consumes: `FavoriteDial` (has optional `freq` string).
- Produces: `handlePrefill` sets `freqMhz` from `dial.freq` when present.

- [ ] **Step 1: Write the failing test**

```typescript
it('prefill from Find a Station fills target and frequency', async () => {
  renderPanel(<ArdopRadioPanel onClose={() => {}} />);
  act(() => {
    emitGatewayPrefill('ardop-hf', { mode: 'ardop-hf', gateway: 'W7DG', freq: '7.102 MHz' });
  });
  await waitFor(() => {
    expect(screen.getByTestId('ardop-target')).toHaveValue('W7DG');
    expect(screen.getByTestId('ardop-freq')).toHaveValue('7.102');
  });
});
```
> `emitGatewayPrefill` mirrors how the file's existing prefill tests dispatch the event (reuse the existing test helper / event-dispatch pattern in the file; if none, dispatch the same CustomEvent `listenGatewayPrefill` subscribes to).

- [ ] **Step 2–3: Implement the freq extraction in `handlePrefill`**

```typescript
const handlePrefill = useCallback((dial: FavoriteDial) => {
  setTarget(dial.gateway);
  pendingDialRef.current = dial;
  writeLastTarget('ardop-hf', dial.gateway);
  // Pull a numeric MHz out of the dial's freq metadata ("7.102 MHz" → "7.102").
  if (dial.freq) {
    const m = dial.freq.match(/[\d.]+/);
    if (m) setFreqMhz(m[0]);
  }
}, []);
```

- [ ] **Step 4: Run + commit**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "prefill"`
Expected: PASS.
```bash
git add src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/ArdopRadioPanel.test.tsx
git commit -m "feat(ui): prefill carries frequency from Find a Station handoff

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Rig control expander (config UI)

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx` (new expander in the Radio config area)
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx`

**Interfaces:**
- Consumes: `ArdopFullConfig` rig fields (Task 6) + the existing `expander` markup + `persistArdop` save.
- Produces: a "Rig control" expander with rig model, CAT port (reuses `cat_serial_path`), CAT backend (rigctld, display), close-serial toggle, live-VFO toggle (disabled when close-serial on), qsy-on-fail toggle.

- [ ] **Step 1: Write the failing test (mutual exclusion)**

```typescript
it('disables live-VFO poll when close-serial sequencing is on', async () => {
  renderPanel(<ArdopRadioPanel onClose={() => {}} />);
  fireEvent.click(screen.getByTestId('rig-control-expander-summary'));
  fireEvent.click(screen.getByTestId('rig-close-serial')); // turn on
  await waitFor(() => {
    expect(screen.getByTestId('rig-live-vfo')).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "live-VFO"`
Expected: FAIL — no rig-control expander.

- [ ] **Step 3: Implement the expander**

Mirror the file's existing `expander` / `expander-summary` markup (the same component the Audio/Advanced sections use). Add the Rig control expander with: a rig-model `<select>` (FT-710 → value 1049), a CAT-port input bound to `cat_serial_path`, a read-only "Managed rigctld" backend line, and three toggles bound to `close_serial_sequencing`, `live_vfo_poll` (rendered `disabled` when `close_serial_sequencing` is true and forced false), and `qsy_on_fail`. Each change calls the existing `persistArdop`/`config_set_ardop` save path. Use `data-testid` values `rig-control-expander-summary`, `rig-close-serial`, `rig-live-vfo`, `rig-qsy-on-fail`, `rig-model`, `rig-cat-port`.

- [ ] **Step 4: Run + commit**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx`
Expected: PASS (all panel tests).
```bash
git add src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/ArdopRadioPanel.test.tsx
git commit -m "feat(ui): Rig control expander (model/port/close-serial/live-vfo/qsy)

Agent: butte-crag-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

# Phase 6 — Integration gate

## Task 13: Draft PR + Codex cross-provider adrev

- [ ] **Step 1:** Push the branch; open a **draft** PR (`gh pr create --draft --base main`) so GitHub CI compiles the Rust (it will not compile on the Pi). Verify CI: `cargo test` (incl. `-p tux-rig`), clippy `-D warnings`, `pnpm vitest`, `pnpm lint`.
- [ ] **Step 2:** Run the **mandatory cross-provider Codex adrev** on the diff (`docs/adversarial/2026-06-26-rig-control-codex.md`, gitignored). Attack angles: rigctld lifecycle leaks (zombie subprocess on connect failure / abort), close-serial ordering (serial released BEFORE audio on internal-codec), QSY abort-honoring, freq parse (MHz↔Hz, locale decimal), serde DTO drift (Rust↔TS field parity). If Codex is quota-limited, defer the round (do not substitute Claude).
- [ ] **Step 3:** Disposition findings; fix or file follow-ups. Mark PR ready. Merge per ADR 0010 (no squash) after green + adrev.
- [ ] **Step 4:** On-air validation is **operator-run** (RADIO-1): hand the licensee a runnable connect against a real gateway with the FT-710 on the DRA-100 path. Agent does not run it.

---

## Self-Review

**Spec coverage:**
- Backend = rigctld → Tasks 1–5, 7. ✓
- Frequency element + Tune-only → Task 10. ✓
- Rig control expander (PTT keying already exists; CAT model/port/close-serial/live-VFO) → Task 12. ✓
- Connect button plain label → no rename needed (existing button kept; Task 10 only adds `data-testid`). ✓
- QSY-on-fail operator-selectable → Tasks 6 (flag), 9 (loop), 12 (toggle). ✓
- Close-serial sequencing → Tasks 5 (`release_serial`), 8 (`should_release_after_tune`). ✓
- Live-VFO ⊻ close-serial → Tasks 6 (forced-false note), 12 (disabled). ✓
- Find a Station handoff carries freq → Task 11. ✓
- Reuse not rebuild (ranking/finder untouched) → no task modifies Find a Station. ✓
- tux-rig crate per ADR 0015 → Phase 1. ✓
- Config + TS DTO mirror → Task 6. ✓
- Codex adrev → Task 13. ✓

**Placeholder scan:** No "TBD"/"handle errors"/"similar to". Steps that delegate to existing patterns (expander markup, prefill-event dispatch) point at the exact in-file precedent rather than inventing a parallel one — intentional, not a placeholder.

**Type consistency:** `Mode::rigctl_str`/`from_rigctl` (Task 2) used consistently in `protocol.rs` (Task 3) and `client.rs` (Task 4). `RigConfig` fields (`binary/model/serial_path/baud/host/port`, Task 5) match `rig_config_from` (Task 7). `ArdopUiConfig` new fields (Task 6) match the TS `ArdopFullConfig` mirror and the Task 12 toggles (`close_serial_sequencing`, `live_vfo_poll`, `qsy_on_fail`, `rig_hamlib_model`, `rigctld_*`). `freqHz` arg name consistent between Task 10 invoke and Task 8 backend param (`freq_hz` Rust ↔ `freqHz` Tauri camelCase bridge).

**Known integration seams flagged for the implementer (not placeholders — judgment points):**
1. Task 8's `live_rig` storage needs a session slot (`Option<ManagedRig>` on `ModemSession`, dropped on disconnect). If the session struct can't take a `tux-rig` dep cleanly, store a boxed `Drop` handle instead.
2. Task 8/9 interaction: the tune step lives inside the per-candidate `attempt` closure (Task 9), so the two compose — Task 8 builds the single-dial path first; Task 9 generalizes it to the list. Implement 8 then 9.
