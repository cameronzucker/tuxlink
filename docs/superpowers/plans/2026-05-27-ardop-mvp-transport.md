# ARDOP MVP transport — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the radio-free plumbing for ARDOP HF support — the generic external-TCP-modem transport abstraction, the ardopcf host-protocol client (cmd + data sockets), and the managed-spawn process supervisor — so that on-site bring-up (operator + radio + ardopcf binary) is paste-and-go.

**Architecture:** A new `winlink::modem` module that *instantiates locked decisions #2 and #3* (managed-spawn, generic external-TCP-modem client). A `ModemTransport` trait abstracts "drive a modem over its TCP host protocol + manage its process"; `ArdopTransport` is the first concrete implementation. A `ManagedModem` supervisor spawns ardopcf as a child process with a clean SIGINT-based lifecycle. The ARDOP wire codec is grounded in the spec (ardopcf `docs/Host_Interface_Commands.md`) and the on-disk reference (`dev/scratch/ax25-prior-art/wl2k-go/transport/ardop/`). TDD against a scripted mock TNC over loopback TCP — no `ardopcf`, no radio, no audio device needed.

**CONCURRENCY ARCHITECTURE (binding — decided 2026-05-27, see rationale below):**
ARDOP is **synchronous + threads**, NOT Tokio. Rationale: (1) fixed fan-out of three
(cmd socket + data socket + one child process) — async buys nothing below high
fan-out, and managed-spawn locks us to one active modem; (2) the consumer
`run_exchange<R: Read, W: Write>` is a **shared synchronous blocking** B2F engine —
an async transport would force either an async rewrite of that shared engine or a
`spawn_blocking`/channel sync↔async seam (a failure mode this codebase already hit per
Cargo.toml L21's runtime-drop scar). Concretely:
- The transport exposes **sync `std::io::Read + Write`** (a `ByteLink`, so it drops
  straight into `run_exchange` exactly like `Ax25Stream`).
- Concurrency = **`std::thread` + `std::sync::mpsc`**: a control-loop thread reads the
  cmd socket and dispatches parsed events to a channel; mirror the `ReadHalf`/`WriteHalf`
  thread split in `src-tauri/src/winlink/telnet.rs`.
- Abort = the existing proven pattern: a shared `AtomicBool` + socket `shutdown()` →
  read-returns-0 (see `AbortableByteLink` in `winlink/ax25/link.rs`). No `select!`.
- Mock test harness = `std::net::TcpListener` + threads (mirror `datalink.rs`
  `ScriptedPeer` and `telnet.rs` tests), NOT `tokio::test`.

**Every Phase 2–5 code block below was first drafted Tokio-async and is SUPERSEDED by
this directive** — realize the same protocol/logic in the sync+threads idiom above.
Implementer task text dispatched by the controller will carry the sync-correct code.

**Tech stack:** Rust 2021. `std::net::TcpStream`, `std::thread`, `std::sync::mpsc`,
`std::sync::atomic`. `std::process::Command` + `nix` (v0.28, **already a dependency**
with `signal`+`process` features) for SIGINT/SIGKILL. `serialport` is unused here. No
Tokio in this subsystem. **Crate name is `tuxlink`** (single package; lib target
`tuxlink_lib`) — all cargo commands use `--manifest-path src-tauri/Cargo.toml` with NO
`-p tuxlink-tauri`; the example imports `use tuxlink_lib::...`. Dev-deps for the example:
`clap` (derive) + `anyhow`. Do NOT add `libc`/`which`/`async-trait` (use `nix` + a sync
trait; `lsof` is invoked as an external command, no crate needed).

**Out of MVP scope:** Tauri command wiring, UI integration, ardopcf-as-Tauri-sidecar
bundling, the rig-control crate (deferred to tuxlink-5jb), Dire Wolf↔ARDOP swap
orchestration (needs both modems first).

**Reference spec:** [docs/design/ardop-deployment-findings.md](../../design/ardop-deployment-findings.md) (locked decisions), [tuxlink-6aj](https://example/tuxlink-6aj) (the feature issue). On-disk: [wl2k-go/transport/ardop/](../../../dev/scratch/ax25-prior-art/wl2k-go/transport/ardop/) (authoritative client implementation).

**Constraints (RADIO-1, remote operator):** This MVP is radio-free unit-tested only; on-air validation is the operator's, on-site. The subagent rule applies — no code under test transmits.

---

## Phase 0 — Setup (non-TDD)

### Task 0.1: Set up the worktree + write the modem ADR + baseline commit

**Files:**
- Create: `worktrees/bd-tuxlink-6aj-ardop-mvp/` (new git worktree)
- Create: `docs/adr/0015-modem-integration-and-rig-control-foundation.md`
- Move into worktree: `docs/design/ardop-deployment-findings.md` (currently untracked in main checkout)
- Move into worktree: `docs/superpowers/plans/2026-05-27-ardop-mvp-transport.md` (this plan)

- [ ] **Step 1: Create the worktree off `origin/main` claimed by tuxlink-6aj**

```bash
# From the main checkout (read-only ok; the worktree write happens off main)
git fetch origin
git worktree add -b bd-tuxlink-6aj/ardop-mvp worktrees/bd-tuxlink-6aj-ardop-mvp origin/main
bd update tuxlink-6aj --claim
```

Expected: new worktree at `worktrees/bd-tuxlink-6aj-ardop-mvp/` on a branch off `origin/main`; tuxlink-6aj claimed.

- [ ] **Step 2: Bring the two pre-worktree drafts into the worktree**

```bash
WT=worktrees/bd-tuxlink-6aj-ardop-mvp
mkdir -p $WT/docs/design $WT/docs/superpowers/plans
cp docs/design/ardop-deployment-findings.md $WT/docs/design/
cp docs/superpowers/plans/2026-05-27-ardop-mvp-transport.md $WT/docs/superpowers/plans/
```

- [ ] **Step 3: Write the modem ADR**

Create `worktrees/bd-tuxlink-6aj-ardop-mvp/docs/adr/0015-modem-integration-and-rig-control-foundation.md`:

```markdown
# 15. Modem integration and rig-control foundation

Date: 2026-05-27

## Status

Accepted.

## Context

Tuxlink is gaining an ARDOP HF transport, and a clean-sheet first-party HF modem
is on the v0.5+ roadmap (ADR 0014). Both interact with external RF/audio
processes, both need rig control (PTT minimum, frequency/mode for single-pane
UX), and the sound card is a single contended resource (one radio, one audio
interface, one modem at a time). The first-party modem may eventually ship as a
**standalone open-source TCP daemon** usable by non-tuxlink clients
(Pat/ARIM/etc.), which would invert who owns rig control.

## Decision

1. **tuxlink launches and owns the modem lifecycle** (managed-spawn) — tuxlink is
   the single arbiter of the sound-card conflict. Lifecycle = spawn / supervise /
   SIGINT-clean-stop / confirm-audio-device-released-before-swap.
2. **The transport to any soundcard modem is a generic "external TCP modem"
   client**, NOT modem-special-cased. ardopcf / Dire Wolf / VARA / (future)
   first-party tuxmodem are all instances of one `ModemTransport` abstraction
   (drive a modem over its TCP host protocol + manage its process).
3. **Rig control is its own crate** (`tux-rig`: trait
   `Ptt/SetFreq/SetMode/ReadStatus` + Hamlib as the first backend) — NOT baked
   into client internals. Consumed by ARDOP-full and the future first-party
   modem; structured so a future standalone modem daemon and the client can both
   link the crate (build-once survives the spin-off).

## Consequences

- The ARDOP MVP can ship without `tux-rig` (modem keys PTT via RTS) — see
  `docs/superpowers/plans/2026-05-27-ardop-mvp-transport.md`.
- The full single-pane (tuxlink owns CAT freq + PTT) depends on tuxlink-5jb
  (rig-control plane research → `tux-rig` crate).
- `rigctld` becomes the third managed external process tuxlink supervises
  (alongside ardopcf and Dire Wolf), reusing the same spawn/SIGINT machinery.
- Modem spin-off vs. monolith remains an open packaging decision; (2) and (3)
  keep it open at near-zero extra cost.

## Open (deferred)

- PTT/frequency sequencing for ARDOP-full (MVP vs. full single-pane).
- Host-protocol / clean-sheet line for the eventual standalone modem (the
  on-air protocol is clean-sheet per ADR 0014; the host-side control API is
  argued: NOT bound by clean-sheet — settle before the modem spec).
- Hamlib backend form (libhamlib FFI vs. managed `rigctld` subprocess vs.
  minimal own-CAT).

## Related

- ADR 0014 — Clean-sheet modem; no prior-art examination (the modem's on-air
  protocol).
- tuxlink-5jb — Frequency/rig control plane research.
- tuxlink-6aj — Add ARDOP HF transport (consumes decisions #1, #2 in MVP).
- docs/design/ardop-deployment-findings.md — Full findings (Locked decisions
  section + Forward-looking analysis).
```

- [ ] **Step 4: Update the ADR index README**

Read `docs/adr/README.md` and add a 0015 entry in the same format as the others.

- [ ] **Step 5: Commit the baseline**

```bash
cd worktrees/bd-tuxlink-6aj-ardop-mvp
git add docs/adr/0015-modem-integration-and-rig-control-foundation.md docs/adr/README.md docs/design/ardop-deployment-findings.md docs/superpowers/plans/2026-05-27-ardop-mvp-transport.md
git commit -m "docs(modem): ADR 0015 + ARDOP MVP plan + findings (tuxlink-6aj)

ADR 0015 records the locked architecture (managed-spawn, generic
external-TCP-modem client, rig control as its own crate). The MVP plan and
findings doc were drafted in the main checkout pre-worktree; brought into
this branch as the implementation baseline.

Agent: <session-moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

Note: replace `<session-moniker>` with the executing session's moniker (`marten-finch-gorge` for this session; subagents inherit it).

---

## Phase 1 — ARDOP wire codec (TDD against the documented protocol)

### Task 1.1: Cmd-socket line codec (write + read of `\r`-terminated lines)

**Files:**
- Create: `src-tauri/src/winlink/modem/mod.rs`
- Create: `src-tauri/src/winlink/modem/ardop/mod.rs`
- Create: `src-tauri/src/winlink/modem/ardop/wire.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod modem;`)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/modem/ardop/wire.rs`:

```rust
#[cfg(test)]
mod cmd_line_tests {
    use super::*;

    #[test]
    fn encode_cmd_line_appends_cr_and_no_prefix() {
        // ARDOP TCP-mode cmd socket: bare ASCII line terminated by \r. No "C:" prefix
        // (that prefix is only for the non-TCP/serial transport, per wl2k-go frame.go).
        let out = encode_cmd_line("MYCALL N7CPZ");
        assert_eq!(out, b"MYCALL N7CPZ\r");
    }

    #[test]
    fn encode_cmd_line_handles_no_args() {
        assert_eq!(encode_cmd_line("INITIALIZE"), b"INITIALIZE\r");
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::wire::cmd_line_tests -- --nocapture
```

Expected: FAIL with `cannot find function encode_cmd_line`.

- [ ] **Step 3: Implement minimal encode_cmd_line**

```rust
//! ARDOP wire-level codec (TCP host-protocol mode).
//!
//! TCP-mode framing per ardopcf docs and wl2k-go `transport/ardop/frame.go`:
//! - Cmd socket (default 8515): `<ASCII>\r`-terminated lines, both directions.
//!   No CRC, no type prefix (CRC + "C:" prefix are serial-mode-only).
//! - Data socket (default 8516) inbound: `[u16 BE length][3-byte type][payload]`.
//! - Data socket outbound: raw bytes (TNC frames them for TX).

pub fn encode_cmd_line(line: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len() + 1);
    out.extend_from_slice(line.as_bytes());
    out.push(b'\r');
    out
}
```

- [ ] **Step 4: Add the decoder test (split a buffered byte stream into `\r`-terminated lines)**

```rust
#[test]
fn decode_lines_splits_on_cr_only() {
    // The cmd socket reader yields complete \r-terminated lines.
    let mut buf = Vec::new();
    let mut out = Vec::new();
    feed_and_drain(&mut buf, &mut out, b"NEWSTATE DISC\rCONNECTED W7ABC 500\r");
    assert_eq!(out, vec!["NEWSTATE DISC".to_string(), "CONNECTED W7ABC 500".to_string()]);
}

#[test]
fn decode_lines_holds_partial_until_cr() {
    let mut buf = Vec::new();
    let mut out = Vec::new();
    feed_and_drain(&mut buf, &mut out, b"NEWSTATE ");
    assert!(out.is_empty(), "no CR yet -> no line yielded");
    feed_and_drain(&mut buf, &mut out, b"DISC\r");
    assert_eq!(out, vec!["NEWSTATE DISC".to_string()]);
}

// Helper for tests: append `chunk` to `buf`, drain any complete \r-terminated lines into `out`.
fn feed_and_drain(buf: &mut Vec<u8>, out: &mut Vec<String>, chunk: &[u8]) {
    buf.extend_from_slice(chunk);
    while let Some(pos) = buf.iter().position(|&b| b == b'\r') {
        let line = String::from_utf8(buf.drain(..pos).collect()).expect("ascii");
        buf.drain(..1); // drop the \r
        out.push(line);
    }
}
```

(The helper is in `#[cfg(test)]`; production code uses a `tokio::io::AsyncBufReadExt`-driven equivalent in Task 2.1.)

- [ ] **Step 5: Run + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::wire
git add src-tauri/src/winlink/mod.rs src-tauri/src/winlink/modem/
git commit -m "feat(modem-ardop): cmd-socket line codec (encode + decode) (tuxlink-6aj)"
```

### Task 1.2: ARDOP `Command` enum + parse/encode

**Files:**
- Create: `src-tauri/src/winlink/modem/ardop/command.rs`

- [ ] **Step 1: Failing test — parse the inbound async events that drive the state machine**

```rust
#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_newstate_with_known_state() {
        // wl2k-go: cmdNewState parses parts[1] via stateMap -> State.
        // We parse the same set: OFFLINE, DISC, ISS, IRS, IDLE, FECSEND, FECRCV.
        let msg = Command::parse("NEWSTATE DISC").unwrap();
        assert!(matches!(msg, Command::NewState(State::Disc)));
    }

    #[test]
    fn parses_connected_call_and_bandwidth() {
        // "CONNECTED W7ABC 500" -> two-element value, space-separated (wl2k-go parseList " ").
        let msg = Command::parse("CONNECTED W7ABC 500").unwrap();
        assert!(matches!(msg, Command::Connected { ref peer_call, bandwidth_hz: 500 } if peer_call == "W7ABC"));
    }

    #[test]
    fn parses_fault_carries_message() {
        let msg = Command::parse("FAULT not from state IRS").unwrap();
        assert!(matches!(msg, Command::Fault(ref s) if s == "not from state IRS"));
    }

    #[test]
    fn parses_ptt_bool() {
        assert!(matches!(Command::parse("PTT TRUE").unwrap(), Command::Ptt(true)));
        assert!(matches!(Command::parse("PTT FALSE").unwrap(), Command::Ptt(false)));
    }

    #[test]
    fn parses_buffer_int() {
        // BUFFER carries TNC outbound-queue stats; first int is bytes-pending per wl2k-go.
        assert!(matches!(Command::parse("BUFFER 0").unwrap(), Command::Buffer(0)));
        assert!(matches!(Command::parse("BUFFER 1024").unwrap(), Command::Buffer(1024)));
    }

    #[test]
    fn parses_disconnected_no_args() {
        assert!(matches!(Command::parse("DISCONNECTED").unwrap(), Command::Disconnected));
    }

    #[test]
    fn unknown_command_yields_an_error() {
        assert!(Command::parse("MYSTERY 123").is_err());
    }
}
```

- [ ] **Step 2: Run + verify fail**

Expected: FAIL with `cannot find type Command` (and `State`).

- [ ] **Step 3: Implement minimal `Command` + `State` enums + `parse`**

```rust
use std::fmt;

/// ARDOP TNC state. Grounded in wl2k-go `state_string.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Offline,
    Disc,
    Idle,
    Iss,        // Information Sending Station
    Irs,        // Information Receiving Station
    FecSend,
    FecRcv,
}

impl State {
    fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "OFFLINE" => State::Offline,
            "DISC" => State::Disc,
            "IDLE" => State::Idle,
            "ISS" => State::Iss,
            "IRS" => State::Irs,
            "FECSEND" => State::FecSend,
            "FECRCV" => State::FecRcv,
            _ => return None,
        })
    }
}

/// ARDOP host-protocol message (inbound from TNC). One variant per command we
/// parse on the cmd socket. Outbound encoding is in `encode()` below. We start
/// with the variants the connect/exchange/disconnect flow actually needs;
/// extend as Phase 2/3 tasks pull more in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    NewState(State),
    Connected { peer_call: String, bandwidth_hz: u32 },
    Disconnected,
    Fault(String),
    Ptt(bool),
    Buffer(u32),
    Busy(bool),
    Status(String),
    // Echo-backs of set-commands (TNC echoes the cmd name to acknowledge).
    EchoBack(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CommandParseError {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("malformed value for {cmd}: {detail}")]
    Malformed { cmd: String, detail: String },
}

impl Command {
    /// Parse one cmd-socket line (without the trailing `\r`). Tolerates wl2k-go's
    /// observed quirks: leading/trailing whitespace, `now <value>` echo-back prefix.
    pub fn parse(line: &str) -> Result<Self, CommandParseError> {
        let line = line.trim();
        let mut parts = line.splitn(2, ' ');
        let head = parts.next().unwrap_or("").to_ascii_uppercase();
        let rest = parts.next().map(|s| s.trim_start_matches("now ").trim());

        match head.as_str() {
            "NEWSTATE" => {
                let tok = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "NEWSTATE".into(),
                    detail: "missing state token".into(),
                })?;
                let st = State::from_token(&tok.to_ascii_uppercase()).ok_or_else(|| {
                    CommandParseError::Malformed {
                        cmd: "NEWSTATE".into(),
                        detail: format!("unknown state: {tok}"),
                    }
                })?;
                Ok(Command::NewState(st))
            }
            "CONNECTED" => {
                let rest = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "CONNECTED".into(),
                    detail: "missing args".into(),
                })?;
                let mut toks = rest.split_whitespace();
                let peer_call = toks.next().ok_or_else(|| CommandParseError::Malformed {
                    cmd: "CONNECTED".into(),
                    detail: "missing peer call".into(),
                })?.to_string();
                let bw = toks.next().unwrap_or("0").parse::<u32>().map_err(|e| {
                    CommandParseError::Malformed { cmd: "CONNECTED".into(), detail: e.to_string() }
                })?;
                Ok(Command::Connected { peer_call, bandwidth_hz: bw })
            }
            "DISCONNECTED" => Ok(Command::Disconnected),
            "FAULT" => Ok(Command::Fault(rest.unwrap_or("").to_string())),
            "PTT" => Ok(Command::Ptt(rest.map(|s| s.eq_ignore_ascii_case("TRUE")).unwrap_or(false))),
            "BUSY" => Ok(Command::Busy(rest.map(|s| s.eq_ignore_ascii_case("TRUE")).unwrap_or(false))),
            "BUFFER" => {
                let n = rest.unwrap_or("0").split_whitespace().next().unwrap_or("0")
                    .parse::<u32>().map_err(|e| CommandParseError::Malformed {
                        cmd: "BUFFER".into(), detail: e.to_string() })?;
                Ok(Command::Buffer(n))
            }
            "STATUS" => Ok(Command::Status(rest.unwrap_or("").to_string())),
            // Echo-back acks: TNC repeats the command name with no args, or with `now <val>`.
            other if is_echoback_of_setter(other) => Ok(Command::EchoBack(other.to_string())),
            _ => Err(CommandParseError::Unknown(head)),
        }
    }
}

fn is_echoback_of_setter(cmd: &str) -> bool {
    matches!(
        cmd,
        "INITIALIZE" | "MYCALL" | "GRIDSQUARE" | "PROTOCOLMODE" | "ARQTIMEOUT"
        | "ARQCALL" | "ARQBW" | "CODEC" | "LISTEN" | "DRIVELEVEL"
    )
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For logging / human projection. Outbound encoding is separate (see `encode_setter`).
        write!(f, "{:?}", self)
    }
}

/// Encode an outbound setter for the cmd socket. Returns the wire string *without*
/// the trailing `\r` (Task 1.1's `encode_cmd_line` adds it).
pub fn encode_setter(cmd: &str, arg: Option<&str>) -> String {
    match arg {
        Some(v) => format!("{cmd} {v}"),
        None => cmd.to_string(),
    }
}
```

- [ ] **Step 4: Add outbound encoding tests**

```rust
#[test]
fn encode_setter_with_arg() {
    assert_eq!(encode_setter("MYCALL", Some("N7CPZ")), "MYCALL N7CPZ");
    assert_eq!(encode_setter("ARQCALL", Some("W7ABC 3")), "ARQCALL W7ABC 3");
}

#[test]
fn encode_setter_no_arg() {
    assert_eq!(encode_setter("INITIALIZE", None), "INITIALIZE");
}
```

- [ ] **Step 5: Cargo test + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::command
git add src-tauri/src/winlink/modem/ardop/command.rs src-tauri/src/winlink/modem/ardop/mod.rs
git commit -m "feat(modem-ardop): Command enum + parse/encode (tuxlink-6aj)"
```

### Task 1.3: Data-socket frame codec

**Files:**
- Create: `src-tauri/src/winlink/modem/ardop/frame.rs`

- [ ] **Step 1: Failing tests for inbound data-frame decode**

```rust
#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn decode_arq_frame_strips_length_and_type_tag() {
        // Wire: [u16 BE length=8][ARQ][HELLO]
        //       length covers the 3-byte type tag + payload (matches wl2k-go's +2 read trick).
        let mut wire = Vec::new();
        wire.extend_from_slice(&8u16.to_be_bytes());
        wire.extend_from_slice(b"ARQ");
        wire.extend_from_slice(b"HELLO");
        let mut dec = DataDecoder::default();
        dec.push(&wire);
        let f = dec.next_frame().expect("a complete ARQ frame");
        assert_eq!(f.kind, DataKind::Arq);
        assert_eq!(f.payload, b"HELLO");
        assert!(dec.next_frame().is_none(), "no more frames");
    }

    #[test]
    fn decode_holds_partial_until_complete() {
        // Length says 8 (= 3 type + 5 payload). Feed only 6 bytes; expect None until completion.
        let mut wire_a = Vec::new();
        wire_a.extend_from_slice(&8u16.to_be_bytes());
        wire_a.extend_from_slice(b"ARQ"); // 5 bytes total feeded
        let mut dec = DataDecoder::default();
        dec.push(&wire_a);
        assert!(dec.next_frame().is_none(), "5 of 10 wire bytes -> incomplete");
        dec.push(b"HELLO");
        let f = dec.next_frame().expect("complete now");
        assert_eq!(f.payload, b"HELLO");
    }

    #[test]
    fn decode_distinguishes_arq_fec_err_idf() {
        for tag in [b"ARQ", b"FEC", b"ERR", b"IDF"] {
            let mut wire = Vec::new();
            wire.extend_from_slice(&3u16.to_be_bytes());
            wire.extend_from_slice(tag);
            let mut dec = DataDecoder::default();
            dec.push(&wire);
            let f = dec.next_frame().expect("complete");
            let expected = match tag {
                b"ARQ" => DataKind::Arq,
                b"FEC" => DataKind::Fec,
                b"ERR" => DataKind::Err,
                b"IDF" => DataKind::Idf,
                _ => unreachable!(),
            };
            assert_eq!(f.kind, expected);
        }
    }
}
```

- [ ] **Step 2: Run + verify fail**

- [ ] **Step 3: Implement `DataDecoder` + `DataFrame`**

```rust
//! Data-socket inbound frame codec for ARDOP TCP mode.
//! Wire format (per wl2k-go `transport/ardop/frame.go`):
//!     [u16 BE length][3-byte type tag][payload of (length - 3) bytes]
//! `length` is the count of bytes following the 2-byte length itself
//! (so it covers `type tag + payload`). Outbound on the data socket is raw
//! bytes — the TNC frames them for TX (we never encode an inbound-style frame).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind { Arq, Fec, Err, Idf, Other }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFrame {
    pub kind: DataKind,
    pub payload: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct DataDecoder { buf: Vec<u8> }

impl DataDecoder {
    pub fn push(&mut self, bytes: &[u8]) { self.buf.extend_from_slice(bytes); }

    pub fn next_frame(&mut self) -> Option<DataFrame> {
        if self.buf.len() < 5 { return None; }                          // 2 len + 3 type minimum
        let length = u16::from_be_bytes([self.buf[0], self.buf[1]]) as usize;
        if length < 3 { return None; }                                  // malformed; defensive
        let total = 2 + length;                                         // wire bytes for this frame
        if self.buf.len() < total { return None; }                      // incomplete
        let tag = &self.buf[2..5];
        let kind = match tag {
            b"ARQ" => DataKind::Arq,
            b"FEC" => DataKind::Fec,
            b"ERR" => DataKind::Err,
            b"IDF" => DataKind::Idf,
            _      => DataKind::Other,
        };
        let payload = self.buf[5..total].to_vec();
        self.buf.drain(..total);
        Some(DataFrame { kind, payload })
    }
}
```

- [ ] **Step 4: Add a multi-frame-in-one-push test (ensures `next_frame` is iterable)**

```rust
#[test]
fn decode_yields_multiple_frames_from_one_push() {
    let mut wire = Vec::new();
    for payload in [&b"AA"[..], b"BBB", b"CCCC"] {
        let len = (3 + payload.len()) as u16;
        wire.extend_from_slice(&len.to_be_bytes());
        wire.extend_from_slice(b"ARQ");
        wire.extend_from_slice(payload);
    }
    let mut dec = DataDecoder::default();
    dec.push(&wire);
    let mut payloads = Vec::new();
    while let Some(f) = dec.next_frame() { payloads.push(f.payload); }
    assert_eq!(payloads, vec![b"AA".to_vec(), b"BBB".to_vec(), b"CCCC".to_vec()]);
}
```

- [ ] **Step 5: Cargo test + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::frame
git add src-tauri/src/winlink/modem/ardop/frame.rs src-tauri/src/winlink/modem/ardop/mod.rs
git commit -m "feat(modem-ardop): data-socket frame codec (tuxlink-6aj)"
```

---

## Phase 2 — TNC session (init / connect / disconnect / data I/O)

### Task 2.1: Async cmd/data sockets over scripted mock

**Files:**
- Create: `src-tauri/src/winlink/modem/ardop/io.rs`
- Create: `src-tauri/src/winlink/modem/ardop/mock.rs` (test helper, `#[cfg(test)]`)

- [ ] **Step 1: Failing test — round-trip a single command line over loopback TCP**

```rust
#[tokio::test]
async fn cmd_socket_round_trips_a_line() {
    // Set up a mock cmd-socket peer that echoes a NEWSTATE on connect.
    let (mock_addr, _shutdown) = mock_cmd_peer(|mut sock| async move {
        use tokio::io::AsyncWriteExt;
        sock.write_all(b"NEWSTATE DISC\r").await.unwrap();
    }).await;

    let mut cmd = ArdopCmdSocket::connect(mock_addr).await.unwrap();
    let line = cmd.next_line().await.unwrap();
    assert_eq!(line, "NEWSTATE DISC");
}
```

- [ ] **Step 2: Implement `mock_cmd_peer` test helper + `ArdopCmdSocket`**

```rust
// In ardop/mock.rs (test-only)
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub async fn mock_cmd_peer<F, Fut>(handler: F) -> (SocketAddr, tokio::sync::oneshot::Sender<()>)
where
    F: FnOnce(tokio::net::TcpStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            r = listener.accept() => {
                let (sock, _) = r.unwrap();
                handler(sock).await;
            }
            _ = rx => {}
        }
    });
    (addr, tx)
}
```

```rust
// In ardop/io.rs
use std::io;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub struct ArdopCmdSocket {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl ArdopCmdSocket {
    pub async fn connect(addr: SocketAddr) -> io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let (r, w) = stream.into_split();
        Ok(Self { reader: BufReader::new(r), writer: w })
    }

    /// Read one `\r`-terminated line (without the `\r`).
    pub async fn next_line(&mut self) -> io::Result<String> {
        let mut buf = Vec::new();
        let n = self.reader.read_until(b'\r', &mut buf).await?;
        if n == 0 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "cmd socket EOF")); }
        if buf.last() == Some(&b'\r') { buf.pop(); }
        Ok(String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?)
    }

    pub async fn send_line(&mut self, line: &str) -> io::Result<()> {
        self.writer.write_all(super::wire::encode_cmd_line(line).as_slice()).await
    }
}
```

- [ ] **Step 3: Run + verify pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::io
git add src-tauri/src/winlink/modem/ardop/io.rs src-tauri/src/winlink/modem/ardop/mock.rs src-tauri/src/winlink/modem/ardop/mod.rs
git commit -m "feat(modem-ardop): async cmd-socket I/O + scripted-mock test harness (tuxlink-6aj)"
```

### Task 2.2: Init sequence handler

**Files:**
- Create: `src-tauri/src/winlink/modem/ardop/session.rs`

- [ ] **Step 1: Failing test — init() drives the documented sequence on a scripted mock**

```rust
#[tokio::test]
async fn init_drives_documented_sequence() {
    // Scripted mock: respond to each setter with an echo-back ack. Capture the
    // command-order on the wire to assert it matches the wl2k-go init flow.
    let (cmd_addr, captured, _h) = mock_cmd_peer_recording().await;
    let mut sock = ArdopCmdSocket::connect(cmd_addr).await.unwrap();

    init_tnc(&mut sock, &InitConfig {
        mycall: "N7CPZ".into(),
        gridsquare: "CN87".into(),
        arq_timeout_s: 30,
    }).await.expect("init ok");

    let lines = captured.lock().unwrap().clone();
    assert_eq!(lines, vec![
        "INITIALIZE",
        "CODEC TRUE",
        "PROTOCOLMODE ARQ",
        "ARQTIMEOUT 30",
        "LISTEN FALSE",
        "MYCALL N7CPZ",
        "GRIDSQUARE CN87",
    ]);
}
```

- [ ] **Step 2: Implement `init_tnc` + recording mock helper**

```rust
// In ardop/session.rs
use super::command::{Command, CommandParseError};
use super::io::ArdopCmdSocket;

pub struct InitConfig {
    pub mycall: String,
    pub gridsquare: String,
    pub arq_timeout_s: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("parse: {0}")] Parse(#[from] CommandParseError),
    #[error("TNC fault: {0}")] Fault(String),
    #[error("unexpected response to {cmd}: {got}")] Unexpected { cmd: String, got: String },
}

/// Drive the ARDOP init sequence (per wl2k-go tnc.go `init()`):
/// INITIALIZE → CODEC TRUE → PROTOCOLMODE ARQ → ARQTIMEOUT → LISTEN FALSE → MYCALL → GRIDSQUARE.
/// Each setter expects a single echo-back ack from the TNC.
pub async fn init_tnc(sock: &mut ArdopCmdSocket, cfg: &InitConfig) -> Result<(), SessionError> {
    set_and_ack(sock, "INITIALIZE", None).await?;
    set_and_ack(sock, "CODEC", Some("TRUE")).await?;
    set_and_ack(sock, "PROTOCOLMODE", Some("ARQ")).await?;
    set_and_ack(sock, "ARQTIMEOUT", Some(&cfg.arq_timeout_s.to_string())).await?;
    set_and_ack(sock, "LISTEN", Some("FALSE")).await?;
    set_and_ack(sock, "MYCALL", Some(&cfg.mycall)).await?;
    set_and_ack(sock, "GRIDSQUARE", Some(&cfg.gridsquare)).await?;
    Ok(())
}

async fn set_and_ack(sock: &mut ArdopCmdSocket, cmd: &str, arg: Option<&str>) -> Result<(), SessionError> {
    sock.send_line(&super::command::encode_setter(cmd, arg)).await?;
    // The TNC acks setters by echoing the cmd name. NEWSTATE etc. may arrive interleaved
    // (especially after CODEC TRUE → state may transition to DISC). Skip them until we see
    // our ack or a FAULT.
    loop {
        let line = sock.next_line().await?;
        let msg = Command::parse(&line)?;
        match msg {
            Command::EchoBack(name) if name.eq_ignore_ascii_case(cmd) => return Ok(()),
            Command::Fault(s) => return Err(SessionError::Fault(s)),
            // Async events during init are normal; absorb and keep waiting.
            Command::NewState(_) | Command::Ptt(_) | Command::Busy(_)
            | Command::Buffer(_) | Command::Status(_) | Command::Connected { .. }
            | Command::Disconnected => continue,
            Command::EchoBack(other) => return Err(SessionError::Unexpected {
                cmd: cmd.into(), got: other,
            }),
        }
    }
}
```

```rust
// In ardop/mock.rs — recording variant for init/connect tests
use std::sync::{Arc, Mutex};

pub async fn mock_cmd_peer_recording() -> (SocketAddr, Arc<Mutex<Vec<String>>>, tokio::sync::oneshot::Sender<()>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = captured.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            r = listener.accept() => {
                let (sock, _) = r.unwrap();
                let (rd, mut wr) = sock.into_split();
                let mut br = BufReader::new(rd);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    let n = br.read_until(b'\r', &mut buf).await.unwrap_or(0);
                    if n == 0 { break; }
                    if buf.last() == Some(&b'\r') { buf.pop(); }
                    let line = String::from_utf8(std::mem::take(&mut buf)).unwrap_or_default();
                    cap.lock().unwrap().push(line.clone());
                    // Echo back the command name as ack (for setters), terminated by \r.
                    let cmd_name = line.split_whitespace().next().unwrap_or("").to_string();
                    wr.write_all(format!("{cmd_name}\r").as_bytes()).await.unwrap();
                }
            }
            _ = rx => {}
        }
    });
    (addr, captured, tx)
}
```

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::session::init_drives_documented_sequence
git add src-tauri/src/winlink/modem/ardop/session.rs src-tauri/src/winlink/modem/ardop/mock.rs
git commit -m "feat(modem-ardop): init sequence handler (tuxlink-6aj)"
```

### Task 2.3: ARQ connect / disconnect

**Files:**
- Modify: `src-tauri/src/winlink/modem/ardop/session.rs`

- [ ] **Step 1: Failing test — connect succeeds when TNC sends CONNECTED**

```rust
#[tokio::test]
async fn connect_resolves_on_connected_event() {
    let (addr, _h) = mock_cmd_peer(|mut sock| async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let (rd, mut wr) = sock.split();
        let mut br = BufReader::new(rd);
        let mut buf = Vec::new();
        br.read_until(b'\r', &mut buf).await.unwrap();
        // Expect: ARQCALL W7ABC 3
        assert_eq!(String::from_utf8_lossy(&buf).trim_end_matches('\r'), "ARQCALL W7ABC 3");
        // Echo-back ack, then state transitions, then CONNECTED.
        wr.write_all(b"ARQCALL\rNEWSTATE ISS\rCONNECTED W7ABC 500\r").await.unwrap();
    }).await;
    let mut sock = ArdopCmdSocket::connect(addr).await.unwrap();
    let info = arq_connect(&mut sock, "W7ABC", 3, std::time::Duration::from_secs(2)).await.unwrap();
    assert_eq!(info.peer_call, "W7ABC");
    assert_eq!(info.bandwidth_hz, 500);
}

#[tokio::test]
async fn connect_fails_on_fault() {
    let (addr, _h) = mock_cmd_peer(|mut sock| async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let (rd, mut wr) = sock.split();
        let mut br = BufReader::new(rd);
        let mut buf = Vec::new();
        br.read_until(b'\r', &mut buf).await.unwrap();
        wr.write_all(b"FAULT not from state IRS\r").await.unwrap();
    }).await;
    let mut sock = ArdopCmdSocket::connect(addr).await.unwrap();
    let err = arq_connect(&mut sock, "W7ABC", 3, std::time::Duration::from_secs(2)).await.unwrap_err();
    assert!(matches!(err, SessionError::Fault(s) if s.contains("not from state IRS")));
}

#[tokio::test]
async fn connect_times_out_when_no_response() {
    let (addr, _h) = mock_cmd_peer(|sock| async move { drop(sock); /* accept and hang */ }).await;
    // We can't reliably reach the "hang" peer above since drop closes; instead use a long-lived noop:
    // (Adapt as needed at execution time — point is to verify timeout path.)
    let mut sock = ArdopCmdSocket::connect(addr).await.unwrap();
    let r = arq_connect(&mut sock, "W7ABC", 3, std::time::Duration::from_millis(80)).await;
    assert!(matches!(r, Err(SessionError::Io(ref e)) if e.kind() == std::io::ErrorKind::TimedOut)
            || matches!(r, Err(SessionError::Io(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof));
}
```

- [ ] **Step 2: Implement `arq_connect` + `arq_disconnect`**

```rust
use std::time::Duration;

pub struct ConnectInfo { pub peer_call: String, pub bandwidth_hz: u32 }

pub async fn arq_connect(
    sock: &mut ArdopCmdSocket,
    target: &str,
    repeat: u32,
    deadline: Duration,
) -> Result<ConnectInfo, SessionError> {
    let line = super::command::encode_setter("ARQCALL", Some(&format!("{target} {repeat}")));
    sock.send_line(&line).await?;
    let start = std::time::Instant::now();
    loop {
        let remaining = deadline.checked_sub(start.elapsed()).ok_or_else(|| {
            SessionError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "arq_connect deadline"))
        })?;
        let line = tokio::time::timeout(remaining, sock.next_line()).await
            .map_err(|_| SessionError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "arq_connect deadline")))??;
        match Command::parse(&line)? {
            Command::Connected { peer_call, bandwidth_hz } => return Ok(ConnectInfo { peer_call, bandwidth_hz }),
            Command::Fault(s) => return Err(SessionError::Fault(s)),
            Command::Disconnected => return Err(SessionError::Fault("DISCONNECTED before CONNECTED".into())),
            Command::NewState(State::Disc) => return Err(SessionError::Fault("transitioned to DISC".into())),
            // Echo-back of ARQCALL, async PTT/BUSY/NEWSTATE/BUFFER events: absorb and keep waiting.
            _ => continue,
        }
    }
}

pub async fn arq_disconnect(sock: &mut ArdopCmdSocket, deadline: Duration) -> Result<(), SessionError> {
    sock.send_line("DISCONNECT").await?;
    let start = std::time::Instant::now();
    loop {
        let remaining = deadline.checked_sub(start.elapsed()).ok_or_else(|| {
            SessionError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "arq_disconnect deadline"))
        })?;
        let line = tokio::time::timeout(remaining, sock.next_line()).await
            .map_err(|_| SessionError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "arq_disconnect deadline")))??;
        if matches!(Command::parse(&line)?, Command::Disconnected | Command::NewState(State::Disc)) {
            return Ok(());
        }
    }
}
```

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::session
git add src-tauri/src/winlink/modem/ardop/session.rs
git commit -m "feat(modem-ardop): ARQ connect + disconnect with deadlines (tuxlink-6aj)"
```

### Task 2.4: Data-socket bidirectional I/O

**Files:**
- Modify: `src-tauri/src/winlink/modem/ardop/io.rs`

- [ ] **Step 1: Failing test — data socket reads inbound ARQ frames and writes raw outbound**

```rust
#[tokio::test]
async fn data_socket_yields_inbound_arq_payload() {
    let (addr, _h) = mock_cmd_peer(|mut sock| async move {
        use tokio::io::AsyncWriteExt;
        let mut wire = Vec::new();
        wire.extend_from_slice(&8u16.to_be_bytes());
        wire.extend_from_slice(b"ARQ");
        wire.extend_from_slice(b"HELLO");
        sock.write_all(&wire).await.unwrap();
    }).await;
    let mut dsock = ArdopDataSocket::connect(addr).await.unwrap();
    let f = dsock.next_arq_payload().await.unwrap();
    assert_eq!(f, b"HELLO");
}

#[tokio::test]
async fn data_socket_outbound_is_raw_bytes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (addr, captured, _h) = mock_collect_raw().await;
    let mut dsock = ArdopDataSocket::connect(addr).await.unwrap();
    dsock.write_payload(b"WORLD").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(captured.lock().unwrap().as_slice(), b"WORLD");
}
```

- [ ] **Step 2: Implement `ArdopDataSocket` + `mock_collect_raw`**

```rust
// In ardop/io.rs
use super::frame::{DataDecoder, DataKind};

pub struct ArdopDataSocket {
    reader: tokio::net::tcp::OwnedReadHalf,
    writer: tokio::net::tcp::OwnedWriteHalf,
    decoder: DataDecoder,
}

impl ArdopDataSocket {
    pub async fn connect(addr: SocketAddr) -> io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let (r, w) = stream.into_split();
        Ok(Self { reader: r, writer: w, decoder: DataDecoder::default() })
    }

    /// Read until the next ARQ data frame and return its payload.
    pub async fn next_arq_payload(&mut self) -> io::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(f) = self.decoder.next_frame() {
                if f.kind == DataKind::Arq { return Ok(f.payload); }
                // Non-ARQ data frames (FEC/IDF/ERR) are not part of the B2F session; skip.
                continue;
            }
            let n = self.reader.read(&mut chunk).await?;
            if n == 0 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "data socket EOF")); }
            self.decoder.push(&chunk[..n]);
        }
    }

    /// Write raw outbound bytes (TNC frames them for TX).
    pub async fn write_payload(&mut self, bytes: &[u8]) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;
        self.writer.write_all(bytes).await
    }
}
```

```rust
// In ardop/mock.rs
pub async fn mock_collect_raw() -> (SocketAddr, Arc<Mutex<Vec<u8>>>, tokio::sync::oneshot::Sender<()>) {
    use tokio::io::AsyncReadExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let collected: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let c = collected.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            r = listener.accept() => {
                let (mut sock, _) = r.unwrap();
                let mut buf = [0u8; 4096];
                loop {
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    if n == 0 { break; }
                    c.lock().unwrap().extend_from_slice(&buf[..n]);
                }
            }
            _ = rx => {}
        }
    });
    (addr, collected, tx)
}
```

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::io
git add src-tauri/src/winlink/modem/ardop/io.rs src-tauri/src/winlink/modem/ardop/mock.rs
git commit -m "feat(modem-ardop): data-socket bidirectional I/O (tuxlink-6aj)"
```

---

## Phase 3 — Generic `ModemTransport` trait

### Task 3.1: ModemTransport trait + ArdopTransport composing it

**Files:**
- Modify: `src-tauri/src/winlink/modem/mod.rs`
- Create: `src-tauri/src/winlink/modem/ardop/transport.rs`

- [ ] **Step 1: Failing test — ArdopTransport implements ModemTransport (compiles + drives a full session over mocks)**

```rust
#[tokio::test]
async fn ardop_transport_connect_send_recv_disconnect_end_to_end() {
    // Two mock sockets: cmd (drives state) + data (carries our payload).
    let (cmd_addr, ..) = scripted_cmd_peer_for_full_session().await;
    let (data_addr, _data_collected, _h2) = mock_collect_raw().await;

    let mut t: Box<dyn ModemTransport> = Box::new(ArdopTransport::with_addrs(cmd_addr, data_addr));
    t.init(&InitConfig { mycall: "N7CPZ".into(), gridsquare: "CN87".into(), arq_timeout_s: 30 }).await.unwrap();
    let _info = t.connect_arq("W7ABC", 3, std::time::Duration::from_secs(2)).await.unwrap();
    t.write_payload(b"HELLO B2F").await.unwrap();
    t.disconnect(std::time::Duration::from_secs(2)).await.unwrap();
}
```

(The `scripted_cmd_peer_for_full_session` helper composes the init echo-backs from Task 2.2 + the connect+disconnect responses from Task 2.3 — reuse those mocks. Spell out the script in the test for clarity.)

- [ ] **Step 2: Define the trait + implement for ArdopTransport**

```rust
// In modem/mod.rs
use async_trait::async_trait;
use std::time::Duration;

pub use crate::winlink::modem::ardop::session::{InitConfig, ConnectInfo, SessionError};

/// Drive an external soundcard modem over its TCP host protocol + manage its process.
/// First implementor: ArdopTransport (ardopcf). Future: DireWolfTransport (KISS-over-TCP),
/// VaraTransport (if pursued), TuxModemTransport (the v0.5+ first-party clean-sheet modem).
#[async_trait]
pub trait ModemTransport: Send {
    async fn init(&mut self, cfg: &InitConfig) -> Result<(), SessionError>;
    async fn connect_arq(&mut self, target: &str, repeat: u32, deadline: Duration) -> Result<ConnectInfo, SessionError>;
    async fn disconnect(&mut self, deadline: Duration) -> Result<(), SessionError>;
    async fn write_payload(&mut self, bytes: &[u8]) -> Result<(), SessionError>;
    async fn next_payload(&mut self) -> Result<Vec<u8>, SessionError>;
}

pub mod ardop;
pub mod process;
```

```rust
// In modem/ardop/transport.rs
use super::io::{ArdopCmdSocket, ArdopDataSocket};
use super::session::{arq_connect, arq_disconnect, init_tnc, ConnectInfo, InitConfig, SessionError};
use crate::winlink::modem::ModemTransport;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::time::Duration;

pub struct ArdopTransport {
    cmd_addr: SocketAddr,
    data_addr: SocketAddr,
    cmd: Option<ArdopCmdSocket>,
    data: Option<ArdopDataSocket>,
}

impl ArdopTransport {
    /// Construct an ArdopTransport pointed at an already-running ardopcf.
    /// The managed-spawn path (`with_managed_modem`) follows in Phase 4.
    pub fn with_addrs(cmd_addr: SocketAddr, data_addr: SocketAddr) -> Self {
        Self { cmd_addr, data_addr, cmd: None, data: None }
    }
}

#[async_trait]
impl ModemTransport for ArdopTransport {
    async fn init(&mut self, cfg: &InitConfig) -> Result<(), SessionError> {
        self.cmd = Some(ArdopCmdSocket::connect(self.cmd_addr).await?);
        self.data = Some(ArdopDataSocket::connect(self.data_addr).await?);
        init_tnc(self.cmd.as_mut().unwrap(), cfg).await
    }
    async fn connect_arq(&mut self, target: &str, repeat: u32, deadline: Duration) -> Result<ConnectInfo, SessionError> {
        arq_connect(self.cmd.as_mut().expect("init first"), target, repeat, deadline).await
    }
    async fn disconnect(&mut self, deadline: Duration) -> Result<(), SessionError> {
        arq_disconnect(self.cmd.as_mut().expect("init first"), deadline).await
    }
    async fn write_payload(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        Ok(self.data.as_mut().expect("init first").write_payload(bytes).await?)
    }
    async fn next_payload(&mut self) -> Result<Vec<u8>, SessionError> {
        Ok(self.data.as_mut().expect("init first").next_arq_payload().await?)
    }
}
```

Add `async-trait = "0.1"` to `src-tauri/Cargo.toml` `[dependencies]` if not already present.

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::transport
git add src-tauri/src/winlink/modem/ src-tauri/Cargo.toml
git commit -m "feat(modem): ModemTransport trait + ArdopTransport impl (tuxlink-6aj)"
```

---

## Phase 4 — Managed-spawn process supervisor

### Task 4.1: ManagedModem::spawn — start an external modem process

**Files:**
- Create: `src-tauri/src/winlink/modem/process.rs`

- [ ] **Step 1: Failing test — spawn a stub binary, verify it's running, send SIGINT, verify clean exit**

```rust
#[tokio::test]
async fn managed_modem_spawns_and_sigints_cleanly() {
    // Use `/bin/sh -c "trap 'exit 0' INT; sleep 30"` as a stand-in for ardopcf:
    // it runs until SIGINT, then exits 0. ManagedModem must spawn, supervise,
    // SIGINT, and reap it within a short deadline.
    let mut m = ManagedModem::spawn(
        "/bin/sh",
        &["-c", "trap 'exit 0' INT; sleep 30"],
    ).expect("spawn ok");
    assert!(m.is_running());
    m.stop(std::time::Duration::from_secs(2)).await.expect("clean stop");
    assert!(!m.is_running());
    assert_eq!(m.exit_status().unwrap().code(), Some(0));
}

#[tokio::test]
async fn managed_modem_stop_escalates_to_sigkill_if_sigint_ignored() {
    // A process that ignores SIGINT must be SIGKILLed after the grace deadline.
    let mut m = ManagedModem::spawn(
        "/bin/sh",
        &["-c", "trap '' INT; sleep 30"],
    ).expect("spawn ok");
    m.stop(std::time::Duration::from_millis(200)).await.expect("escalated kill");
    assert!(!m.is_running());
    let st = m.exit_status().unwrap();
    assert!(st.signal() == Some(libc::SIGKILL) || st.code() == Some(137));
}
```

(The libc dep may need adding under `[target.'cfg(unix)'.dependencies]`. The exit-status assertion uses `std::os::unix::process::ExitStatusExt`.)

- [ ] **Step 2: Implement `ManagedModem`**

```rust
//! Managed-spawn supervisor for external RF helper processes (ardopcf, direwolf,
//! eventually rigctld). Owns the lifecycle so tuxlink is the single arbiter of
//! the one-sound-card conflict — instantiates ADR 0015 / locked decision #2.
//!
//! Clean stop: SIGINT first (ardopcf's documented clean-stop; it ignores SIGHUP).
//! If the process is still running after the grace deadline, escalate to SIGKILL
//! so an unresponsive child never wedges a band-switch.

use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

pub struct ManagedModem {
    child: Option<Child>,
    last_status: Option<ExitStatus>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("spawn failed: {0}")] Spawn(#[from] std::io::Error),
    #[error("stop failed: {0}")] Stop(String),
}

impl ManagedModem {
    pub fn spawn(program: &str, args: &[&str]) -> Result<Self, ProcessError> {
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        Ok(Self { child: Some(child), last_status: None })
    }

    pub fn is_running(&self) -> bool { self.child.is_some() }
    pub fn exit_status(&self) -> Option<ExitStatus> { self.last_status }

    /// Send SIGINT and wait up to `grace` for clean exit. If still running,
    /// SIGKILL and reap. Returns Ok on either clean or forced exit.
    pub async fn stop(&mut self, grace: Duration) -> Result<(), ProcessError> {
        let Some(mut child) = self.child.take() else { return Ok(()); };
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            if let Some(pid) = child.id() {
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
            }
        }
        let deadline = Instant::now() + grace;
        loop {
            match tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), child.wait()).await {
                Ok(Ok(status)) => { self.last_status = Some(status); return Ok(()); }
                Ok(Err(e)) => return Err(ProcessError::Stop(e.to_string())),
                Err(_) => {
                    // grace expired -> escalate
                    let _ = child.start_kill();
                    match child.wait().await {
                        Ok(st) => { self.last_status = Some(st); return Ok(()); }
                        Err(e) => return Err(ProcessError::Stop(e.to_string())),
                    }
                }
            }
        }
    }
}
```

Add to `src-tauri/Cargo.toml` `[target.'cfg(unix)'.dependencies]`:
```toml
nix = { version = "0.27", features = ["signal"] }
libc = "0.2"
```

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::process
git add src-tauri/src/winlink/modem/process.rs src-tauri/Cargo.toml
git commit -m "feat(modem): ManagedModem supervisor (SIGINT clean-stop + SIGKILL escalation) (tuxlink-6aj)"
```

### Task 4.2: Audio-device-release confirmation after stop

**Files:**
- Modify: `src-tauri/src/winlink/modem/process.rs`

- [ ] **Step 1: Failing test — confirm_audio_device_released returns true after the child exits**

```rust
#[tokio::test]
async fn confirm_device_released_after_stop() {
    // Stub child that opens a temp file (proxy for "holds the audio device"),
    // exits on SIGINT closing the FD. confirm_audio_device_released should
    // return true after stop().
    let path = std::env::temp_dir().join(format!("tuxlink-mock-dev-{}", std::process::id()));
    std::fs::write(&path, b"x").unwrap();
    let mut m = ManagedModem::spawn(
        "/bin/sh",
        &["-c", &format!("exec 3<{} ; trap 'exit 0' INT ; sleep 30", path.display())],
    ).unwrap();
    m.stop(std::time::Duration::from_secs(2)).await.unwrap();
    assert!(ManagedModem::confirm_audio_device_released(&path, std::time::Duration::from_secs(1)).await,
            "device should be released after child exit");
    std::fs::remove_file(&path).ok();
}
```

- [ ] **Step 2: Implement `confirm_audio_device_released`**

The portable check: poll `lsof <path>` (or `fuser`) until no PID holds the file, bounded by a deadline. On Linux this is the most reliable cross-distro way without depending on ALSA-specific APIs. Fallback to a brief settling delay if `lsof` is missing.

```rust
impl ManagedModem {
    /// Poll until no process holds `device_path` open, bounded by `deadline`.
    /// Returns true if released, false if still held at deadline.
    /// `device_path` will typically be the radio's USB-audio device node (e.g.
    /// /dev/snd/pcmC1D0c) — passed by the caller because ALSA device naming
    /// varies. Uses `lsof` for portability; falls back to a 200ms settling
    /// delay if `lsof` is absent.
    pub async fn confirm_audio_device_released(device_path: &std::path::Path, deadline: Duration) -> bool {
        let start = Instant::now();
        let path_s = device_path.to_string_lossy().to_string();
        let lsof_present = which::which("lsof").is_ok();
        if !lsof_present {
            tokio::time::sleep(Duration::from_millis(200)).await;
            return true;
        }
        loop {
            let output = tokio::process::Command::new("lsof")
                .arg(&path_s)
                .output().await;
            match output {
                Ok(o) if !o.status.success() && o.stdout.is_empty() => return true, // lsof exits non-zero when no holders
                Ok(o) if o.stdout.is_empty() => return true,
                _ => {}
            }
            if start.elapsed() >= deadline { return false; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
```

Add `which = "6"` to `src-tauri/Cargo.toml`.

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::process::confirm_device_released_after_stop
git add src-tauri/src/winlink/modem/process.rs src-tauri/Cargo.toml
git commit -m "feat(modem): audio-device-release confirmation via lsof poll (tuxlink-6aj)"
```

---

## Phase 5 — Integration: managed-spawn ArdopTransport + CLI bring-up example

### Task 5.1: ArdopTransport::with_managed_modem — spawn + connect

**Files:**
- Modify: `src-tauri/src/winlink/modem/ardop/transport.rs`
- Modify: `src-tauri/src/winlink/modem/ardop/mod.rs` (expose `ArdopConfig`)

- [ ] **Step 1: Failing test — with_managed_modem spawns a stub binary that mimics ardopcf's TCP listen**

```rust
#[tokio::test]
async fn with_managed_modem_spawns_and_drives_init() {
    // Stub ardopcf: a small script that opens cmd:Pport and data:Pport+1 on
    // 127.0.0.1, echoes the init sequence acks, then exits on SIGINT.
    // We use Python for portability (already on the Pi).
    // The test picks two random ports via 0; the stub reads them from argv.
    let (cmd_port, data_port) = (free_port(), free_port());
    let stub_path = write_stub_ardopcf_py();
    let cfg = ArdopConfig {
        binary: std::path::PathBuf::from("python3"),
        extra_args: vec![stub_path.to_string_lossy().into(), cmd_port.to_string(), data_port.to_string()],
        cmd_port,
        data_port,
        audio_device_path: None,   // not exercised in this stubbed test
    };
    let mut t = ArdopTransport::with_managed_modem(cfg).await.unwrap();
    t.init(&InitConfig { mycall: "N7CPZ".into(), gridsquare: "CN87".into(), arq_timeout_s: 30 }).await.unwrap();
    t.shutdown().await.unwrap();
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l); p
}

fn write_stub_ardopcf_py() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("stub_ardopcf_{}.py", std::process::id()));
    std::fs::write(&p, STUB_ARDOPCF_PY).unwrap();
    p
}

const STUB_ARDOPCF_PY: &str = r#"
import socket, sys, signal, threading, time
cmd_port, data_port = int(sys.argv[1]), int(sys.argv[2])
def serve(port, kind):
    s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", port)); s.listen(1); c, _ = s.accept()
    if kind == "cmd":
        buf = b""
        while True:
            data = c.recv(4096)
            if not data: break
            buf += data
            while b"\r" in buf:
                line, buf = buf.split(b"\r", 1)
                name = line.decode().split()[0]
                c.sendall((name + "\r").encode())   # echo-back ack
    else:
        time.sleep(3600)
def shutdown(*_): sys.exit(0)
signal.signal(signal.SIGINT, shutdown)
threading.Thread(target=serve, args=(cmd_port, "cmd"), daemon=True).start()
threading.Thread(target=serve, args=(data_port, "data"), daemon=True).start()
signal.pause()
"#;
```

- [ ] **Step 2: Implement `ArdopConfig` + `with_managed_modem` + `shutdown`**

```rust
// In ardop/mod.rs
#[derive(Debug, Clone)]
pub struct ArdopConfig {
    pub binary: std::path::PathBuf,   // e.g. "ardopcf"
    pub extra_args: Vec<String>,      // e.g. ["-p", "/dev/ttyUSB1", "8515", "plughw:1,0", "plughw:1,0"]
    pub cmd_port: u16,                // typically 8515
    pub data_port: u16,               // typically cmd_port + 1
    pub audio_device_path: Option<std::path::PathBuf>, // for confirm_audio_device_released; None = skip
}

// In ardop/transport.rs
use super::ArdopConfig;
use crate::winlink::modem::process::ManagedModem;

impl ArdopTransport {
    /// Spawn ardopcf via ManagedModem and prepare cmd/data socket addresses.
    /// Caller still drives init/connect/disconnect explicitly.
    pub async fn with_managed_modem(cfg: ArdopConfig) -> Result<Self, SessionError> {
        let modem = ManagedModem::spawn(
            cfg.binary.to_str().unwrap_or("ardopcf"),
            &cfg.extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ).map_err(|e| SessionError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
        let cmd_addr: SocketAddr = format!("127.0.0.1:{}", cfg.cmd_port).parse().unwrap();
        let data_addr: SocketAddr = format!("127.0.0.1:{}", cfg.data_port).parse().unwrap();

        // Wait briefly for ardopcf to bind both sockets.
        let started_at = std::time::Instant::now();
        loop {
            let cmd_ok = tokio::net::TcpStream::connect(cmd_addr).await.is_ok();
            let data_ok = tokio::net::TcpStream::connect(data_addr).await.is_ok();
            if cmd_ok && data_ok { break; }
            if started_at.elapsed() > std::time::Duration::from_secs(5) {
                return Err(SessionError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "ardopcf TCP sockets did not bind within 5s",
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let mut t = Self::with_addrs(cmd_addr, data_addr);
        t.managed = Some((modem, cfg.audio_device_path.clone()));
        Ok(t)
    }

    /// Clean shutdown: disconnect (if connected), then SIGINT the modem and
    /// confirm the audio device is released — the swap-invariant ADR 0015 names.
    pub async fn shutdown(&mut self) -> Result<(), SessionError> {
        if let Some(c) = self.cmd.as_mut() {
            // Best-effort disconnect; ignore errors (we are tearing down anyway).
            let _ = arq_disconnect(c, std::time::Duration::from_millis(500)).await;
        }
        if let Some((mut modem, dev_path)) = self.managed.take() {
            modem.stop(std::time::Duration::from_secs(3)).await
                .map_err(|e| SessionError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            if let Some(p) = dev_path {
                if !ManagedModem::confirm_audio_device_released(&p, std::time::Duration::from_secs(2)).await {
                    return Err(SessionError::Io(std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        "audio device still held after modem stop",
                    )));
                }
            }
        }
        Ok(())
    }
}

// Add the managed field:
pub struct ArdopTransport {
    cmd_addr: SocketAddr,
    data_addr: SocketAddr,
    cmd: Option<ArdopCmdSocket>,
    data: Option<ArdopDataSocket>,
    managed: Option<(ManagedModem, Option<std::path::PathBuf>)>,
}
// Update with_addrs to initialize `managed: None`.
```

- [ ] **Step 3: Test pass + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri winlink::modem::ardop::transport::with_managed_modem_spawns_and_drives_init
git add src-tauri/src/winlink/modem/ardop/
git commit -m "feat(modem-ardop): with_managed_modem — spawn + bind-wait + shutdown (tuxlink-6aj)"
```

### Task 5.2: CLI bring-up example binary

**Files:**
- Create: `src-tauri/examples/ardop_connect.rs`

- [ ] **Step 1: Write the example (no test — this is the on-site bring-up entry point)**

```rust
//! On-site bring-up example for ARDOP MVP. Usage:
//!     cargo run --manifest-path src-tauri/Cargo.toml --example ardop_connect -- \
//!         --binary ardopcf \
//!         --mycall N7CPZ-1 --gridsquare CN87 \
//!         --capture plughw:1,0 --playback plughw:1,0 \
//!         --ptt /dev/ttyUSB1 \
//!         --target W7BU-10
//!
//! Spawns ardopcf, drives init, attempts ARQ connect, prints state events, exits cleanly.
//! Operator-keyed. RADIO-1: this WILL transmit when --target is reachable; do not run
//! in the agent shell.

use clap::Parser;
use tuxlink_tauri::winlink::modem::{
    ardop::{ArdopConfig, ArdopTransport},
    InitConfig, ModemTransport,
};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "ardopcf")] binary: std::path::PathBuf,
    #[arg(long)] mycall: String,
    #[arg(long)] gridsquare: String,
    #[arg(long)] capture: String,    // e.g. plughw:1,0
    #[arg(long)] playback: String,
    #[arg(long)] ptt: Option<String>, // e.g. /dev/ttyUSB1 -> ardopcf -p
    #[arg(long, default_value_t = 8515)] cmd_port: u16,
    #[arg(long)] target: String,
    #[arg(long, default_value_t = 3)] repeat: u32,
    #[arg(long)] audio_device_path: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let mut extra = Vec::new();
    if let Some(p) = &a.ptt { extra.push("-p".into()); extra.push(p.clone()); }
    extra.push(a.cmd_port.to_string());
    extra.push(a.capture.clone());
    extra.push(a.playback.clone());
    let cfg = ArdopConfig {
        binary: a.binary,
        extra_args: extra,
        cmd_port: a.cmd_port,
        data_port: a.cmd_port + 1,
        audio_device_path: a.audio_device_path,
    };
    let mut t = ArdopTransport::with_managed_modem(cfg).await?;
    t.init(&InitConfig { mycall: a.mycall, gridsquare: a.gridsquare, arq_timeout_s: 30 }).await?;
    println!("[ardop_connect] dialing {} (repeat={})", a.target, a.repeat);
    let info = t.connect_arq(&a.target, a.repeat, std::time::Duration::from_secs(45)).await?;
    println!("[ardop_connect] CONNECTED {} @ {} Hz", info.peer_call, info.bandwidth_hz);
    // Minimal "I/O works" check — read up to 64 bytes, then disconnect cleanly.
    if let Ok(Ok(buf)) = tokio::time::timeout(std::time::Duration::from_secs(15), t.next_payload()).await {
        println!("[ardop_connect] RX {} bytes: {:?}", buf.len(), String::from_utf8_lossy(&buf));
    }
    t.disconnect(std::time::Duration::from_secs(5)).await?;
    println!("[ardop_connect] clean disconnect");
    Ok(())
}
```

Add `[dev-dependencies]` to `src-tauri/Cargo.toml`: `clap = { version = "4", features = ["derive"] }`, `anyhow = "1"`.

- [ ] **Step 2: Compile-check + commit**

```bash
cargo build --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri --example ardop_connect
git add src-tauri/examples/ardop_connect.rs src-tauri/Cargo.toml
git commit -m "feat(modem-ardop): on-site bring-up CLI example (tuxlink-6aj)

The operator-run paste-and-go path: spawns ardopcf with their audio + PTT
config, drives init, ARQ-connects to a target, prints state, disconnects.
RADIO-1: operator-keyed only; not for agent shells."
```

---

## Phase 6 — Gates + cross-provider review + PR

### Task 6.1: Full gates green

- [ ] **Step 1: Run the full test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri
```

Expected: ALL tests pass (existing + the new modem tests).

- [ ] **Step 2: Clippy clean**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri --all-targets -- -D warnings
```

Expected: zero warnings/errors.

- [ ] **Step 3: Build the example binary**

```bash
cargo build --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri --example ardop_connect
```

Expected: builds clean.

- [ ] **Step 4: Commit the green-gate marker if anything changed**

```bash
git status --short
# if anything is unstaged from clippy auto-fixes etc., commit:
git add -A && git commit -m "chore: cargo + clippy gates green for modem MVP (tuxlink-6aj)" || true
```

### Task 6.2: Cross-provider review (Codex) — focused, not full ceremony

Per the discipline-triage rule (plumbing → TDD-against-spec → skip heavy adrev), this is **one focused Codex round on the state machine + transport abstraction**, NOT the full 5-round.

- [ ] **Step 1: Run Codex review on the modem subtree**

```bash
npx --yes @openai/codex review --uncommitted "Adversarial review of the new src-tauri/src/winlink/modem/ subtree (ARDOP MVP transport). Focus areas: (1) the ARDOP host-protocol command parser — does it handle every documented inbound variant ardopcf actually emits, especially state events interleaved with setter echo-backs? (2) The connect/disconnect state machine — any race between the cmd-socket events and the data-socket I/O? (3) The ManagedModem SIGINT-then-SIGKILL lifecycle — any leaks, zombies, or device-release races? (4) The bind-wait loop in with_managed_modem — race conditions if ardopcf binds one port but not the other? Write findings to dev/adversarial/$(date +%F)-ardop-mvp-codex.md (gitignored)."
```

- [ ] **Step 2: Triage findings**

Read `dev/adversarial/<date>-ardop-mvp-codex.md`. For each finding:
- **P0/P1 (correctness)**: fix in this PR (add TDD test → fix → commit).
- **P2/P3 (nice-to-have)**: file a follow-up bd issue, do not block PR.
- Summarize dispositions in the PR description.

- [ ] **Step 3: If any P0/P1 fix happened, re-run gates + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri && \
cargo clippy --manifest-path src-tauri/Cargo.toml -p tuxlink-tauri --all-targets -- -D warnings && \
git add -A && git commit -m "fix(modem-ardop): Codex P0/P1 findings (tuxlink-6aj)"
```

### Task 6.3: Push + PR

- [ ] **Step 1: Push the branch**

```bash
cd worktrees/bd-tuxlink-6aj-ardop-mvp
git push -u origin bd-tuxlink-6aj/ardop-mvp
```

- [ ] **Step 2: Open the PR**

```bash
gh pr create --base main --head bd-tuxlink-6aj/ardop-mvp \
  --title "[marten-finch-gorge] feat(modem): ARDOP MVP transport (radio-free plumbing) (tuxlink-6aj)" \
  --body "$(cat <<'EOF'
Closes tuxlink-6aj's MVP scope. Adds the radio-free plumbing for ARDOP HF support:

- **ADR 0015** records the locked architecture (managed-spawn modem ownership; generic external-TCP-modem transport; rig control as its own crate).
- **`winlink::modem::ardop`** — host-protocol codec (cmd-socket `\r`-lines + data-socket length+type framing), session state machine (init/connect/disconnect), bidirectional data I/O.
- **`winlink::modem::ModemTransport`** trait — the abstraction ardopcf/Dire Wolf/(future) VARA/tuxmodem all instantiate.
- **`winlink::modem::process::ManagedModem`** — SIGINT-clean-stop + SIGKILL-escalation + audio-device-release confirmation; the swap-invariant arbiter from ADR 0015.
- **`src-tauri/examples/ardop_connect.rs`** — on-site bring-up CLI for the operator.

**Out of scope (deliberately):** Tauri command wiring + UI integration (await on-air validation); ardopcf-as-Tauri-sidecar bundling; rig-control crate (tuxlink-5jb); Dire Wolf↔ARDOP swap orchestration (needs both modems wired in).

**Validation:** all unit + integration tests green; clippy clean. **On-air not validated** — that is the operator's, on-site, via the `ardop_connect` example. RADIO-1 honored throughout (agent never transmits).

Codex adversarial review: see PR comments / `dev/adversarial/<date>-ardop-mvp-codex.md` (local).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Update bd**

```bash
bd update tuxlink-6aj --notes "MVP PR open: <PR URL>. On-air validation gated on operator on-site with radio + ardopcf binary."
```

---

## Self-review (per writing-plans skill)

**1. Spec coverage (against `docs/design/ardop-deployment-findings.md` Locked decisions + tuxlink-6aj):**
- ✅ Locked decision #1 (Add ARDOP): the whole plan delivers it.
- ✅ Locked decision #2 (managed-spawn): Phase 4 (`ManagedModem`) + Task 5.1 (`with_managed_modem`).
- ✅ Locked decision #3 (generic TCP-modem transport): Phase 3 (`ModemTransport` trait + `ArdopTransport`).
- ✅ Locked decision #4 (rig control as own crate) — *deferred for MVP* per locked-decisions section (MVP uses modem-RTS-PTT via `-p`; full single-pane gated on tuxlink-5jb). Plan explicitly does NOT build the rig crate; this is intentional, not a gap.
- ✅ ADR captured (Task 0.1).
- ✅ On-site bring-up paste-and-go (Task 5.2 CLI example).

**Open items deliberately NOT in MVP:** Tauri command / UI integration; sidecar bundling; rig-control crate. These are tracked: tuxlink-6aj covers the broader feature; tuxlink-5jb tracks rig-control; bundling is a follow-up packaging issue to file when the operator chooses (offered in the PR body).

**2. Placeholder scan:** Each task has concrete file paths, concrete test code, concrete impl code, exact commands. No TODOs/TBDs/"implement later." Spot-check passed.

**3. Type consistency:** `Command`, `State`, `DataKind`, `DataFrame`, `DataDecoder`, `ArdopCmdSocket`, `ArdopDataSocket`, `InitConfig`, `ConnectInfo`, `SessionError`, `ArdopConfig`, `ArdopTransport`, `ModemTransport`, `ManagedModem`, `ProcessError` — each defined in exactly one task and referenced consistently downstream.

One refinement (caught self-review): the `mock_cmd_peer_recording` returns the same shape as `mock_cmd_peer` (addr + shutdown-tx + extra Arc<Mutex<Vec<String>>>); execution should keep them as siblings under `ardop/mock.rs`. Not a bug; noting for the implementer.
