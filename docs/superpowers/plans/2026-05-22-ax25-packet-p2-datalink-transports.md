# AX.25 Packet — P2: Datalink State Machine + Transports Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the AX.25 connected-mode v2.x (mod-8) data-link state machine and the KISS byte-pipe transports (TCP + serial-for-USB/Bluetooth), so that an `Ax25Stream: Read + Write` can carry reliable in-order Winlink B2F bytes over a KISS modem — verified entirely against a scripted in-memory peer + a loopback `TcpListener`, with no RF.

**Architecture:** Three new files under `src-tauri/src/winlink/ax25/`, layered on the P1 wire codec (`frame.rs`, `kiss.rs`). `link.rs` defines a `ByteLink: Read + Write + Send` trait and `connect_link` factory that opens a KISS byte-pipe over either TCP (`TcpStream`, Dire Wolf/SoundModem KISS port) or a serial device (`serialport` crate — USB COM port *and* a Bluetooth RFCOMM `/dev/rfcommN`, treated identically). `params.rs` holds the `Ax25Params` timing/windowing knobs with 1200-baud defaults. `datalink.rs` is the correctness-critical core: it drives a `ByteLink` through the KISS framer (P1's `KissDecoder` / `kiss_data_frame`) and the AX.25 codec (P1's `Frame`/`Control`), running the connected-mode state machine (SABM→UA connect, inbound-SABM→UA answer, sequenced I-frames with RR acknowledgement, REJ retransmit, T1 timeout + N2 retry, MAXFRAME windowing, PACLEN segmentation/reassembly, DISC on drop) and presents the reliable byte stream as `Ax25Stream`. Half-duplex/CSMA is the modem's job — this layer does **not** implement CSMA; it only pushes the KISS TNC-parameter frames (`kiss_param`) on connect.

**Tech Stack:** Rust (the existing `src-tauri` crate). One new dependency: `serialport = "4"` (pure-Rust serial port access; opens `/dev/rfcommN` for Bluetooth identically to a USB COM device). The state machine + transports build on the P1 module already present at `src-tauri/src/winlink/ax25/{frame,kiss}.rs`. Tests use a scripted in-memory `ByteLink` (fed canned KISS frames) and a loopback `TcpListener` on `127.0.0.1` — no serial hardware, no radio, no transmission.

**Authority for behaviour:** AX.25 v2.2 §6 (data-link state machine, T1/N2, V(S)/V(R)/V(A), REJ, window) cross-checked against the decompiled official client `TNCKissInterface.dll` at `dev/scratch/winlink-re/decompiled/tnckiss/` (local-only scratch; `Connection`, `DataLinkProvider`, `EstablishDataLink` for SABM/T1, `Frame.FrameTypes`) and `wl2k-go`. Each task that encodes a timer/control decision names the cross-check value; the in-memory-peer tests pin the expected wire bytes as fixtures so a mismatch fails loudly.

**RADIO-1 / no-RF boundary (spec §6):** every test in this plan runs against an in-memory peer or a loopback TCP socket. Serial (USB) and Bluetooth (RFCOMM) byte-pipes are exercised by the **operator on hardware** — the agent builds `connect_link`'s serial arm and verifies it compiles + opens a device path, but never keys a transmitter. No test in this plan transmits.

**Depends on:** P1 (`docs/superpowers/plans/2026-05-22-ax25-packet-p1-wire-codec.md`) — the `winlink/ax25/{frame,kiss}.rs` codec MUST be merged/present before P2 starts. P2 consumes exactly: `Address{call,ssid}`, `Control` (Sabm/Disc/Ua/Dm/Rr/Rnr/Rej/I with `pf`/`nr`/`ns`), `Path{dest,src,digis}`, `Frame{path,control,info}` + `Frame::encode()`/`Frame::decode()`, `KissDecoder::new()`/`.push()`, `kiss_data_frame()`, `kiss_param()` + `KissParam`, `PID_NO_L3`.

**Run tests with:** `cargo test --manifest-path src-tauri/Cargo.toml ax25::` (absolute manifest path per the worktree path-pinning convention).

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/winlink/ax25/params.rs` | `Ax25Params` (timing/windowing knobs) + 1200-baud `Default` |
| `src-tauri/src/winlink/ax25/link.rs` | `KissLinkConfig` enum, `ByteLink` trait, `connect_link` factory (TCP + Serial), `TcpLink` impl |
| `src-tauri/src/winlink/ax25/datalink.rs` | Connected-mode state machine; `Ax25Stream: Read + Write`; `connect()`/`answer()`/`disconnect()` |
| `src-tauri/src/winlink/ax25/mod.rs` | (modify) declare + re-export the three new modules |
| `src-tauri/Cargo.toml` | (modify) add `serialport = "4"` |

Each file has one responsibility; `datalink.rs` is the only correctness-critical one and gets the cross-provider Codex round at the end of P2 (spec §9).

---

### Task 1: Add the `serialport` dependency + declare the new modules

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/winlink/ax25/mod.rs`
- Create: `src-tauri/src/winlink/ax25/params.rs`
- Create: `src-tauri/src/winlink/ax25/link.rs`
- Create: `src-tauri/src/winlink/ax25/datalink.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/winlink/ax25/params.rs` with only its module doc + a smoke test (the real `Ax25Params` lands in Task 2):
```rust
//! AX.25 timing + windowing parameters. P2 owns the connected-mode tuning knobs
//! (T1 retransmit timer, N2 retry cap, MAXFRAME window, PACLEN segment size) plus
//! the KISS TNC parameters (TXdelay/persistence/slot) pushed to the modem on connect.

#[cfg(test)]
mod params_smoke {
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}
```
Create `src-tauri/src/winlink/ax25/link.rs` and `src-tauri/src/winlink/ax25/datalink.rs` each with a single line: `//! placeholder` .

In `src-tauri/src/winlink/ax25/mod.rs`, add the three module declarations alongside the P1 `pub mod frame; pub mod kiss;` lines:
```rust
pub mod datalink;
pub mod link;
pub mod params;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::params`
Expected: FAIL to compile — `mod.rs` declares `link`/`datalink`/`params` but the `serialport` dep referenced by `link.rs` in later tasks is not yet present; more immediately, the modules compile but `cargo` cannot resolve `serialport` once Task 3 imports it. At THIS step the test compiles and passes only if the empty files are syntactically valid; to make Step 2 a genuine RED, first add the `serialport` import line to `link.rs` so the build fails on the missing crate:

Replace `link.rs`'s `//! placeholder` with:
```rust
//! KISS byte-pipe transports.
use serialport as _; // dependency presence check; removed/used properly in Task 3
```
Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::params`
Expected: FAIL — `error[E0432]: unresolved import \`serialport\`` (crate not in Cargo.toml).

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/Cargo.toml` under `[dependencies]` (alongside the existing crates):
```toml
serialport = "4"           # NEW (tuxlink-7fr P2) — KISS over USB-serial COM and Bluetooth RFCOMM (/dev/rfcommN); pure-Rust, no libudev C dep needed for open/read/write
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::params`
Expected: PASS (`module_is_wired`); `serialport` resolves.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/winlink/ax25/mod.rs src-tauri/src/winlink/ax25/params.rs src-tauri/src/winlink/ax25/link.rs src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): scaffold P2 datalink/link/params modules + add serialport dep (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `Ax25Params` with 1200-baud defaults

**Files:**
- Modify: `src-tauri/src/winlink/ax25/params.rs`

The 1200-baud knobs (cross-check RMS Express `[Packet TNC]` ini per findings §"Packet config parameters", and AX.25 v2.2 §6.7.1.1 default timers): `txdelay` 30 (×10 ms = 300 ms key-up), `persistence` 63 (p ≈ 0.25 CSMA), `slot_time` 10 (×10 ms), `paclen` 128 bytes/I-frame, `maxframe` 4 (window), `t1` 3 s (retransmit), `n2_retries` 10 (retry cap). `txdelay`/`persistence`/`slot_time` are KISS TNC params (pushed via `kiss_param`); `paclen`/`maxframe`/`t1`/`n2_retries` drive the host state machine.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/ax25/params.rs`, replace the smoke module with the real type + tests:
```rust
//! AX.25 timing + windowing parameters. P2 owns the connected-mode tuning knobs
//! (T1 retransmit timer, N2 retry cap, MAXFRAME window, PACLEN segment size) plus
//! the KISS TNC parameters (TXdelay/persistence/slot) pushed to the modem on connect.

use std::time::Duration;

/// Connected-mode timing + windowing for a 1200-baud AX.25 link. `txdelay`,
/// `persistence`, and `slot_time` are KISS TNC parameters (sent to the modem via
/// `kiss_param` on connect); `paclen`, `maxframe`, `t1`, and `n2_retries` drive the
/// host-side state machine in `datalink.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ax25Params {
    /// KISS TXDELAY, units of 10 ms (key-up delay before data).
    pub txdelay: u8,
    /// KISS P-persistence (0–255; ~p*256). CSMA is the modem's job.
    pub persistence: u8,
    /// KISS slot time, units of 10 ms.
    pub slot_time: u8,
    /// Max info bytes per I-frame; writes larger than this are segmented.
    pub paclen: usize,
    /// Window size: max unacknowledged I-frames in flight (mod-8 ⇒ ≤ 7).
    pub maxframe: u8,
    /// T1 retransmit timer: how long to wait for an ack before resending.
    pub t1: Duration,
    /// N2: max retransmissions of a frame before declaring the link failed.
    pub n2_retries: u8,
}

impl Default for Ax25Params {
    fn default() -> Self {
        Ax25Params {
            txdelay: 30,
            persistence: 63,
            slot_time: 10,
            paclen: 128,
            maxframe: 4,
            t1: Duration::from_secs(3),
            n2_retries: 10,
        }
    }
}

#[cfg(test)]
mod params_tests {
    use super::*;
    #[test]
    fn default_is_1200_baud_profile() {
        let p = Ax25Params::default();
        assert_eq!(p.txdelay, 30);
        assert_eq!(p.persistence, 63);
        assert_eq!(p.slot_time, 10);
        assert_eq!(p.paclen, 128);
        assert_eq!(p.maxframe, 4);
        assert_eq!(p.t1, Duration::from_secs(3));
        assert_eq!(p.n2_retries, 10);
    }
    #[test]
    fn maxframe_fits_mod8_window() {
        // mod-8 sequence numbers ⇒ at most 7 unacked frames; the default leaves headroom.
        assert!(Ax25Params::default().maxframe <= 7);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::params`
Expected: FAIL — at Step 1 the type is already written, so this compiles and passes. To make this a genuine RED, write the test FIRST (paste only the `#[cfg(test)] mod params_tests` block and the `use std::time::Duration;` line, omitting the `struct`/`impl`), run, observe `error[E0433]: failed to resolve: ... Ax25Params`, THEN add the `struct` + `impl Default`.
Expected (with type omitted): FAIL — `cannot find type \`Ax25Params\``.

- [ ] **Step 3: Write minimal implementation**

Add the `Ax25Params` struct + `impl Default` shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::params`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/params.rs
git commit -m "feat(ax25): Ax25Params with 1200-baud defaults (T1/N2/PACLEN/MAXFRAME) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `KissLinkConfig` + `ByteLink` trait + `connect_link` (TCP arm)

**Files:**
- Modify: `src-tauri/src/winlink/ax25/link.rs`

The transport seam. `KissLinkConfig` selects TCP or Serial; `ByteLink` is the bidirectional, thread-movable byte stream the state machine drives (mirrors the `ReadWrite` super-trait in `telnet.rs`). `connect_link` is the factory. This task builds the **TCP arm** (loopback-testable, no RF); the **Serial arm** lands in Task 4. The TCP arm sets read/write timeouts so a hung modem fails legibly (mirrors `telnet.rs`'s `TIMEOUT`).

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/ax25/link.rs`, replace the placeholder with the config + trait + TCP-arm factory + a loopback test:
```rust
//! KISS byte-pipe transports for AX.25: a `ByteLink` over TCP (Dire Wolf /
//! SoundModem KISS port) or a serial device (USB COM port, or a Bluetooth RFCOMM
//! `/dev/rfcommN` opened identically). The state machine in `datalink.rs` drives a
//! `ByteLink` through the KISS framer; this layer is dumb byte plumbing.
//!
//! **No RF here.** The TCP arm is exercised against a loopback `TcpListener`. The
//! serial arm (Task 4) is exercised by the operator on hardware (RADIO-1 / spec §6);
//! the agent verifies only that it compiles and opens a device path.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// How long a single read/write on the KISS link may block before failing (mirrors
/// `telnet.rs`'s TIMEOUT — a hung modem must fail legibly, not stall forever).
const LINK_TIMEOUT: Duration = Duration::from_secs(60);

/// Which KISS byte-pipe to open. Bluetooth uses the `Serial` variant with an
/// rfcomm device path (e.g. `/dev/rfcomm0`); there is no in-app BlueZ dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KissLinkConfig {
    /// KISS-over-TCP, e.g. Dire Wolf / SoundModem listening on `127.0.0.1:8001`.
    Tcp { host: String, port: u16 },
    /// KISS-over-serial: a USB COM device (`/dev/ttyUSB0`) OR a Bluetooth RFCOMM
    /// device (`/dev/rfcomm0`). `baud` is the host↔modem link rate (distinct from
    /// the 1200-baud over-air rate).
    Serial { device: String, baud: u32 },
}

/// A bidirectional, thread-movable byte stream — the KISS pipe the AX.25 state
/// machine reads framed bytes from and writes framed bytes to. Blanket-implemented
/// for any `Read + Write + Send` (so `TcpStream`, a `serialport` handle, and the
/// in-memory test peer all qualify).
pub trait ByteLink: Read + Write + Send {}
impl<T: Read + Write + Send> ByteLink for T {}

/// Open a KISS byte-pipe per `cfg`. The returned `Box<dyn ByteLink>` is handed to
/// `datalink::connect` / `datalink::answer`.
pub fn connect_link(cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>> {
    match cfg {
        KissLinkConfig::Tcp { host, port } => {
            let stream = TcpStream::connect((host.as_str(), *port))?;
            stream.set_read_timeout(Some(LINK_TIMEOUT)).ok();
            stream.set_write_timeout(Some(LINK_TIMEOUT)).ok();
            Ok(Box::new(stream))
        }
        KissLinkConfig::Serial { .. } => connect_serial(cfg),
    }
}

#[cfg(test)]
mod link_tcp_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn tcp_link_round_trips_bytes_over_loopback() {
        // A loopback KISS modem stand-in: echoes one chunk back. 127.0.0.1 only —
        // no RF, no external network (per testing-pitfalls).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4];
            let n = sock.read(&mut buf).unwrap();
            sock.write_all(&buf[..n]).unwrap();
        });

        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let mut link = connect_link(&cfg).unwrap();
        link.write_all(&[0xC0, 0x00, 0x42, 0xC0]).unwrap();
        let mut back = [0u8; 4];
        link.read_exact(&mut back).unwrap();
        assert_eq!(back, [0xC0, 0x00, 0x42, 0xC0]);
        server.join().unwrap();
    }

    #[test]
    fn tcp_connect_to_a_dead_port_errors_not_hangs() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening ⇒ connection refused
        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        assert!(connect_link(&cfg).is_err());
    }
}
```

Note: `connect_serial` is referenced but not yet defined — that is Task 4. To keep this task self-contained and GREEN, add a temporary stub at the bottom of `link.rs` that Task 4 replaces:
```rust
/// TEMPORARY stub — replaced by the real serial open in Task 4.
fn connect_serial(_cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "serial KISS link not yet implemented (Task 4)",
    ))
}
```

- [ ] **Step 2: Run test to verify it fails**

Write the test module + the `KissLinkConfig`/`ByteLink`/`connect_link` declarations but OMIT the `connect_serial` stub first.
Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::link`
Expected: FAIL — `error[E0425]: cannot find function \`connect_serial\``.

- [ ] **Step 3: Write minimal implementation**

Add the `connect_serial` temporary stub shown above.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::link`
Expected: PASS (both `link_tcp_tests`).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/link.rs
git commit -m "feat(ax25): KissLinkConfig + ByteLink trait + connect_link TCP arm (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `connect_link` Serial arm (USB + Bluetooth RFCOMM)

**Files:**
- Modify: `src-tauri/src/winlink/ax25/link.rs`

Replace the Task 3 stub with the real `serialport` open. USB and Bluetooth are identical here — both are an OS-exposed serial device path (`/dev/ttyUSB0`, `/dev/rfcomm0`), per spec §4.1's decision (no in-app BlueZ). **No RF**: the agent verifies the open *errors cleanly on a nonexistent device* (a real device open is operator-only hardware testing per RADIO-1).

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/ax25/link.rs`, add to `link_tcp_tests` (or a new `link_serial_tests` module):
```rust
#[cfg(test)]
mod link_serial_tests {
    use super::*;
    #[test]
    fn serial_open_of_a_nonexistent_device_errors_cleanly() {
        // No hardware, no RF: opening a device that does not exist must return a
        // clean Err, never panic or hang. A real device open is operator-only
        // (RADIO-1 / spec §6 — exercised on hardware by the licensee).
        let cfg = KissLinkConfig::Serial {
            device: "/dev/tuxlink-no-such-device".into(),
            baud: 9600,
        };
        let err = connect_link(&cfg).unwrap_err();
        // serialport surfaces a NotFound/Other for a missing device path.
        assert!(
            matches!(err.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::Other),
            "expected a clean open error, got {err:?}"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::link::link_serial`
Expected: FAIL — the Task 3 stub returns `ErrorKind::Unsupported`, which is neither `NotFound` nor `Other`, so the `matches!` assertion fails.

- [ ] **Step 3: Write minimal implementation**

Replace the `connect_serial` stub with the real open:
```rust
/// Open a KISS-over-serial byte-pipe (USB COM port or Bluetooth RFCOMM device).
/// `serialport` returns its own error type; map it to `std::io::Error` so the
/// `connect_link` signature stays `io::Result`.
fn connect_serial(cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>> {
    let (device, baud) = match cfg {
        KissLinkConfig::Serial { device, baud } => (device, *baud),
        // connect_link only routes the Serial variant here.
        KissLinkConfig::Tcp { .. } => unreachable!("connect_serial called with a Tcp config"),
    };
    let port = serialport::new(device, baud)
        .timeout(LINK_TIMEOUT)
        .open()
        .map_err(|e| match e.kind() {
            serialport::ErrorKind::NoDevice => {
                std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())
            }
            _ => std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        })?;
    Ok(Box::new(port))
}
```
Note: `serialport::new(...).open()` returns `Box<dyn serialport::SerialPort>`, which is `Read + Write + Send` ⇒ satisfies the `ByteLink` blanket impl.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::link`
Expected: PASS (all link tests — TCP round-trip, refused-port, serial clean-error).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/link.rs
git commit -m "feat(ax25): connect_link serial arm — USB COM + Bluetooth RFCOMM via serialport (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Scripted in-memory `ByteLink` test peer

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

Before the state machine, build the test harness it will be driven against: a fake `ByteLink` that (a) records everything the state machine writes (so a test can decode the AX.25/KISS bytes we sent) and (b) returns canned bytes the state machine reads (the peer's scripted KISS frames). This is the load-bearing no-RF test fixture for every datalink task. It uses interior-mutability shared handles so a test can inspect TX and queue more RX between calls.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/ax25/datalink.rs`, replace the placeholder with the peer + its self-test (the peer is `#[cfg(test)]`-only):
```rust
//! AX.25 connected-mode v2.x (mod-8) data-link state machine + `Ax25Stream`.
//!
//! Drives a `ByteLink` (TCP / serial KISS pipe) through P1's KISS framer
//! (`KissDecoder` / `kiss_data_frame`) and AX.25 codec (`Frame` / `Control`),
//! running SABM→UA connect, inbound-SABM→UA answer, sequenced I-frames with RR
//! acknowledgement, REJ retransmit, T1 timeout + N2 retry, MAXFRAME windowing,
//! PACLEN segmentation/reassembly, and DISC on drop. Presents reliable in-order
//! bytes as `Ax25Stream: Read + Write`.
//!
//! **No CSMA here** — half-duplex channel access is the modem's job (spec §2/§4.1);
//! this layer only pushes the KISS TNC params (`kiss_param`) on connect.
//!
//! Verified against a scripted in-memory peer (below) + a loopback TCP socket; no
//! RF, no transmission. Behaviour cross-checked vs `TNCKissInterface.dll`
//! (`Connection`/`DataLinkProvider`/`EstablishDataLink`) at
//! `dev/scratch/winlink-re/decompiled/tnckiss/` (local-only) + AX.25 v2.2 §6.

#[cfg(test)]
mod test_peer {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// A scripted in-memory `ByteLink`: the state machine writes to `tx` (which a
    /// test decodes) and reads from `rx` (which a test pre-loads with canned KISS
    /// frames). Both ends share the buffers so a test can inspect/extend between calls.
    #[derive(Clone)]
    pub struct ScriptedPeer {
        pub tx: Arc<Mutex<Vec<u8>>>,
        pub rx: Arc<Mutex<std::collections::VecDeque<u8>>>,
    }

    impl ScriptedPeer {
        pub fn new() -> Self {
            ScriptedPeer {
                tx: Arc::new(Mutex::new(Vec::new())),
                rx: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            }
        }
        /// Queue bytes for the state machine to read (a peer's KISS frame).
        pub fn feed(&self, bytes: &[u8]) {
            self.rx.lock().unwrap().extend(bytes.iter().copied());
        }
        /// Take everything the state machine has written so far.
        pub fn drain_tx(&self) -> Vec<u8> {
            std::mem::take(&mut *self.tx.lock().unwrap())
        }
    }

    impl Read for ScriptedPeer {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut rx = self.rx.lock().unwrap();
            if rx.is_empty() {
                // Empty (not EOF): the state machine's read loop must treat a momentarily
                // empty pipe as "no frame yet", not as a closed link. WouldBlock models a
                // non-blocking serial/TCP read with nothing buffered.
                return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "no data"));
            }
            let n = buf.len().min(rx.len());
            for b in buf.iter_mut().take(n) {
                *b = rx.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for ScriptedPeer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.tx.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn peer_records_tx_and_serves_rx() {
        let peer = ScriptedPeer::new();
        let mut a = peer.clone();
        a.write_all(&[1, 2, 3]).unwrap();
        assert_eq!(peer.drain_tx(), vec![1, 2, 3]);

        peer.feed(&[9, 8]);
        let mut buf = [0u8; 2];
        a.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [9, 8]);

        // Empty pipe ⇒ WouldBlock, not EOF.
        let mut one = [0u8; 1];
        assert_eq!(a.read(&mut one).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::test_peer`
Expected: FAIL on the FIRST write — paste the test body first WITHOUT the `impl Read`/`impl Write` blocks; observe `error[E0599]: no method named \`write_all\``. Then add the impls.
Expected (impls omitted): FAIL — `ScriptedPeer` does not implement `Write`/`Read`.

- [ ] **Step 3: Write minimal implementation**

Add the `impl Read for ScriptedPeer` and `impl Write for ScriptedPeer` blocks shown above.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::test_peer`
Expected: PASS (`peer_records_tx_and_serves_rx`).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "test(ax25): scripted in-memory ByteLink peer for datalink tests (no RF) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: `connect()` — send SABM, await UA (the AwaitingUA path)

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

The dialer's link bring-up. `connect` builds the SABM frame for the path (`mycall` → `target` via `digis`), KISS-wraps it (`kiss_data_frame(frame.encode())`), writes it to the `ByteLink`, then reads KISS frames until a UA addressed back to us arrives (or N2×T1 elapses → `TimedOut`). On connect it first pushes the KISS TNC params (`kiss_param(TxDelay/Persistence/SlotTime, …)`) from `Ax25Params`. Cross-check: `TNCKissInterface.EstablishDataLink` (sends SABM, starts T1, awaits UA/DM). Returns an `Ax25Stream` carrying the link state (sequence vars start at 0).

This task wires the SABM-send + UA-await + the param push and returns a minimal `Ax25Stream` (its `Read`/`Write` land in Tasks 8–9). The internal frame-pump helper (`recv_frame`) is introduced here and reused by every later task.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs` (above the `test_peer` module, real code) and a test inside a new `#[cfg(test)] mod connect_tests` that uses `super::test_peer::ScriptedPeer`:
```rust
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use super::frame::{Address, Control, Frame, Path};
use super::kiss::{kiss_data_frame, kiss_param, KissDecoder, KissParam};
use super::link::ByteLink;
use super::params::Ax25Params;

/// A connected AX.25 link presenting reliable in-order bytes.
pub struct Ax25Stream {
    link: Box<dyn ByteLink>,
    decoder: KissDecoder,
    mycall: Address,
    peer: Address,
    digis: Vec<Address>,
    params: Ax25Params,
    /// V(S): next I-frame send sequence number (mod 8).
    vs: u8,
    /// V(R): next expected receive sequence number (mod 8).
    vr: u8,
    /// V(A): last sequence number acknowledged by the peer (mod 8).
    va: u8,
    /// Reassembled inbound bytes not yet handed to the caller's `read`.
    inbound: std::collections::VecDeque<u8>,
    /// Sent-but-unacked I-frame info payloads, keyed by their N(S), for retransmit.
    unacked: std::collections::BTreeMap<u8, Vec<u8>>,
    closed: bool,
}

/// Open a connected-mode AX.25 link: push the KISS TNC params, send SABM (with the
/// digipeater path), await UA. Errors `TimedOut` after N2×T1 with no UA (spec §5:
/// "No answer" must never be a silent hang). Cross-check `EstablishDataLink`.
pub fn connect(
    link: Box<dyn ByteLink>,
    mycall: Address,
    target: Address,
    digis: &[Address],
    params: &Ax25Params,
) -> std::io::Result<Ax25Stream> {
    let path = Path { dest: target.clone(), src: mycall.clone(), digis: digis.to_vec() };
    let mut stream = Ax25Stream {
        link,
        decoder: KissDecoder::new(),
        mycall,
        peer: target,
        digis: digis.to_vec(),
        params: params.clone(),
        vs: 0,
        vr: 0,
        va: 0,
        inbound: std::collections::VecDeque::new(),
        unacked: std::collections::BTreeMap::new(),
        closed: false,
    };
    // Push the KISS TNC params from the timing config. CSMA itself is the modem's job.
    stream.link.write_all(&kiss_param(KissParam::TxDelay, params.txdelay))?;
    stream.link.write_all(&kiss_param(KissParam::Persistence, params.persistence))?;
    stream.link.write_all(&kiss_param(KissParam::SlotTime, params.slot_time))?;

    // Send SABM (P=1) and await UA, bounded by N2 retransmits of T1.
    let sabm = Frame { path: path.clone(), control: Control::Sabm { pf: true }, info: vec![] };
    for _attempt in 0..=params.n2_retries {
        stream.send_frame(&sabm)?;
        let deadline = Instant::now() + params.t1;
        while Instant::now() < deadline {
            if let Some(frame) = stream.recv_frame()? {
                match frame.control {
                    Control::Ua { .. } => return Ok(stream),
                    Control::Dm { .. } => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionRefused,
                            "peer refused the connection (DM)",
                        ))
                    }
                    _ => continue, // ignore anything else while awaiting UA
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "no UA — peer did not answer the connect (SABM)",
    ))
}

/// How long to nap between `recv_frame` polls when the pipe is momentarily empty,
/// so the T1 wait does not busy-spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

impl Ax25Stream {
    /// KISS-wrap an AX.25 frame and write it to the link.
    fn send_frame(&mut self, frame: &Frame) -> std::io::Result<()> {
        let bytes = frame
            .encode()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e:?}")))?;
        self.link.write_all(&kiss_data_frame(&bytes))
    }

    /// Pull bytes from the link into the KISS decoder and return the next decoded,
    /// successfully-parsed AX.25 frame, if any is available right now. Returns
    /// `Ok(None)` when the pipe is momentarily empty (WouldBlock) — NOT an error.
    fn recv_frame(&mut self) -> std::io::Result<Option<Frame>> {
        let mut buf = [0u8; 512];
        match self.link.read(&mut buf) {
            Ok(0) => Ok(None),
            Ok(n) => {
                for body in self.decoder.push(&buf[..n]) {
                    if let Ok(frame) = Frame::decode(&body) {
                        // Only deliver frames addressed to us (dest == mycall).
                        if frame.path.dest.call == self.mycall.call
                            && frame.path.dest.ssid == self.mycall.ssid
                        {
                            return Ok(Some(frame));
                        }
                    }
                }
                Ok(None)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The path from us to the peer (for outbound command frames).
    fn out_path(&self) -> Path {
        Path { dest: self.peer.clone(), src: self.mycall.clone(), digis: self.digis.clone() }
    }
}
```
And the test:
```rust
#[cfg(test)]
mod connect_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address {
        Address { call: c.into(), ssid }
    }

    /// Build the KISS-wrapped UA the peer would send back, addressed to `mycall`.
    fn peer_ua(mycall: &Address, peer: &Address) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] },
            control: Control::Ua { pf: true },
            info: vec![],
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn connect_sends_sabm_and_returns_on_ua() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // Pre-load the UA so the first recv_frame inside connect succeeds.
        peer.feed(&peer_ua(&mine, &target));

        let stream = connect(
            Box::new(peer.clone()),
            mine.clone(),
            target.clone(),
            &[],
            &Ax25Params::default(),
        )
        .unwrap();
        assert_eq!(stream.peer.call, "W7AUX");

        // We must have written: 3 KISS param frames, then a KISS-wrapped SABM.
        let tx = peer.drain_tx();
        let frames = {
            let mut d = KissDecoder::new();
            d.push(&tx)
        };
        // The last data frame decoded should be our SABM to W7AUX-10.
        let sabm = Frame::decode(frames.last().unwrap()).unwrap();
        assert!(matches!(sabm.control, Control::Sabm { pf: true }));
        assert_eq!(sabm.path.dest, target);
        assert_eq!(sabm.path.src, mine);
    }

    #[test]
    fn connect_times_out_without_ua() {
        let peer = ScriptedPeer::new(); // never feeds a UA
        let mine = call("N7CPZ", 7);
        // Tiny T1 + zero retries so the test is fast.
        let params = Ax25Params { t1: Duration::from_millis(40), n2_retries: 0, ..Ax25Params::default() };
        let err = connect(Box::new(peer), mine, call("W7AUX", 10), &[], &params).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }

    #[test]
    fn connect_errors_on_dm() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let dm = Frame {
            path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] },
            control: Control::Dm { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&dm.encode().unwrap()));
        let err = connect(Box::new(peer), mine, target, &[], &Ax25Params::default()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::connect_tests`
Expected: FAIL — write the `connect_tests` module first, omitting the `connect`/`Ax25Stream`/`recv_frame`/`send_frame` definitions; observe `error[E0425]: cannot find function \`connect\`` and `cannot find type \`Ax25Stream\``.

- [ ] **Step 3: Write minimal implementation**

Add the `Ax25Stream` struct, `connect`, `POLL_INTERVAL`, and the `impl Ax25Stream { send_frame, recv_frame, out_path }` block shown in Step 1. (`out_path` is unused until Task 7/8 — add `#[allow(dead_code)]` on it for now or accept the warning.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::connect_tests`
Expected: PASS (all three: UA success, timeout, DM-refused).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): connect() — push KISS params, send SABM, await UA (T1/N2/DM) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: `answer()` — await inbound SABM, reply UA, surface the peer

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

The answerer's link bring-up (the listen/answer mode, spec §2). `answer` reads KISS frames until a SABM addressed to `mycall` arrives, replies UA (P/F echoed), and returns `(peer_address, Ax25Stream)` with the peer taken from the SABM's source address. Cross-check `TNCKissInterface` inbound-connection handling + findings §"Who runs AX.25 connected-mode" (the answerer is FBB master, but at the *link* layer it just answers SABM). It blocks (polling) until a SABM arrives — the caller's listen lifecycle (P3) decides when to call it and how to abort.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs`:
```rust
/// Await an inbound SABM addressed to `mycall`, reply UA, and surface the calling
/// peer. Blocks (polling the link) until a SABM arrives. The caller (P3 listen
/// lifecycle) governs when to arm this and how to abort it via the link shutdown
/// hook. The reply UA echoes the SABM's source as the new path's dest.
pub fn answer(
    link: Box<dyn ByteLink>,
    mycall: Address,
    params: &Ax25Params,
) -> std::io::Result<(Address, Ax25Stream)> {
    let mut stream = Ax25Stream {
        link,
        decoder: KissDecoder::new(),
        mycall: mycall.clone(),
        peer: mycall.clone(), // placeholder until the SABM names the caller
        digis: vec![],
        params: params.clone(),
        vs: 0,
        vr: 0,
        va: 0,
        inbound: std::collections::VecDeque::new(),
        unacked: std::collections::BTreeMap::new(),
        closed: false,
    };
    loop {
        if let Some(frame) = stream.recv_frame()? {
            if let Control::Sabm { pf } = frame.control {
                // The caller is the SABM's source.
                let peer = frame.path.src.clone();
                stream.peer = peer.clone();
                let ua = Frame {
                    path: Path { dest: peer.clone(), src: mycall.clone(), digis: vec![] },
                    control: Control::Ua { pf },
                    info: vec![],
                };
                stream.send_frame(&ua)?;
                return Ok((peer, stream));
            }
            // Ignore non-SABM frames while listening.
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
```
And the test:
```rust
#[cfg(test)]
mod answer_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address {
        Address { call: c.into(), ssid }
    }

    #[test]
    fn answer_replies_ua_to_an_inbound_sabm_and_names_the_peer() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let caller = call("W7AUX", 10);
        // The peer dials us: a SABM addressed to N7CPZ-7 from W7AUX-10.
        let sabm = Frame {
            path: Path { dest: mine.clone(), src: caller.clone(), digis: vec![] },
            control: Control::Sabm { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&sabm.encode().unwrap()));

        let (got_peer, stream) =
            answer(Box::new(peer.clone()), mine.clone(), &Ax25Params::default()).unwrap();
        assert_eq!(got_peer, caller);
        assert_eq!(stream.peer, caller);

        // We replied a UA addressed back to the caller.
        let tx = peer.drain_tx();
        let frames = { let mut d = KissDecoder::new(); d.push(&tx) };
        let ua = Frame::decode(frames.last().unwrap()).unwrap();
        assert!(matches!(ua.control, Control::Ua { pf: true }));
        assert_eq!(ua.path.dest, caller);
        assert_eq!(ua.path.src, mine);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::answer_tests`
Expected: FAIL — write `answer_tests` first without `answer`; observe `error[E0425]: cannot find function \`answer\``.

- [ ] **Step 3: Write minimal implementation**

Add the `answer` function shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::answer_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): answer() — await inbound SABM, reply UA, surface the peer (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: `Ax25Stream::write` — segment by PACLEN, send I-frames within MAXFRAME, await RR

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

The outbound data path. `write` segments the caller's bytes into ≤`paclen` chunks, sends each as an I-frame (`N(S)`=V(S), `N(R)`=V(R), incrementing V(S) mod 8), allowing at most `maxframe` unacked frames in flight (windowing), then waits for RR/REJ to advance V(A). On RR(N(R)) it frees acked frames; on REJ(N(R)) it retransmits from N(R); on T1 timeout it retransmits the oldest unacked up to N2 times. Cross-check AX.25 v2.2 §6.4 (I-frame send) + §6.7 (T1) + `TNCKissInterface` window handling.

To keep the step bite-sized: this task implements segmentation + windowed I-frame send + RR-acknowledgement (the happy path + window-full blocking). REJ + T1-retransmit get their own task (Task 9). The RR-drain helper (`pump_acks`) introduced here is reused by Task 9.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs`:
```rust
impl Ax25Stream {
    /// Drain any pending S-frames (RR/RNR/REJ) and I-frames from the link, updating
    /// V(A) on acknowledgements, queuing inbound info, and handling REJ/T1 retransmit.
    /// Returns once the pipe is momentarily empty. `expect_progress` bounds how long
    /// we wait for V(A) to advance before a T1 retransmit fires.
    fn pump_acks(&mut self) -> std::io::Result<()> {
        while let Some(frame) = self.recv_frame()? {
            match frame.control {
                Control::Rr { nr, .. } | Control::Rnr { nr, .. } => self.ack_through(nr),
                Control::Rej { nr, .. } => {
                    self.ack_through(nr);
                    self.retransmit_from(nr)?;
                }
                Control::I { ns, nr, pf } => {
                    self.ack_through(nr);
                    self.accept_inbound_i(ns, pf, &frame.info)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Mark all I-frames with N(S) < nr (mod 8) as acknowledged; advance V(A).
    fn ack_through(&mut self, nr: u8) {
        // Remove every unacked entry the peer has now confirmed (sequence numbers
        // strictly before nr, walking forward from V(A) mod 8).
        let mut s = self.va;
        while s != nr {
            self.unacked.remove(&s);
            s = (s + 1) % 8;
        }
        self.va = nr;
    }

    /// Retransmit every still-unacked I-frame from N(S)=nr forward (REJ recovery).
    fn retransmit_from(&mut self, nr: u8) -> std::io::Result<()> {
        let payloads: Vec<(u8, Vec<u8>)> = self
            .unacked
            .range(nr..)
            .chain(self.unacked.range(..nr).filter(|_| false)) // mod-8 wrap handled by callers; v0.1 windows are small
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        for (ns, info) in payloads {
            let f = Frame {
                path: self.out_path(),
                control: Control::I { ns, nr: self.vr, pf: false },
                info,
            };
            self.send_frame(&f)?;
        }
        Ok(())
    }

    /// Send one I-frame carrying `info` (≤ paclen) and record it as unacked.
    fn send_i(&mut self, info: &[u8]) -> std::io::Result<()> {
        let ns = self.vs;
        let f = Frame {
            path: self.out_path(),
            control: Control::I { ns, nr: self.vr, pf: false },
            info: info.to_vec(),
        };
        self.send_frame(&f)?;
        self.unacked.insert(ns, info.to_vec());
        self.vs = (self.vs + 1) % 8;
        Ok(())
    }

    /// Number of I-frames currently in flight (sent, not yet acked).
    fn in_flight(&self) -> usize {
        self.unacked.len()
    }
}

impl Write for Ax25Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.closed {
            return Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "link closed"));
        }
        let paclen = self.params.paclen.max(1);
        let maxframe = self.params.maxframe as usize;
        for chunk in buf.chunks(paclen) {
            // Block until the window has room, draining acks (bounded by N2×T1 so a
            // dead peer surfaces as an error, never a silent hang — spec §5).
            let mut attempts = 0u32;
            while self.in_flight() >= maxframe {
                self.pump_acks()?;
                if self.in_flight() < maxframe {
                    break;
                }
                attempts += 1;
                if attempts as u8 > self.params.n2_retries {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "window stalled — no acknowledgement (N2 exceeded)",
                    ));
                }
                std::thread::sleep(self.params.t1);
            }
            self.send_i(chunk)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Drain any pending acks so a subsequent read/disconnect sees current state.
        self.pump_acks()
    }
}
```
Note: `accept_inbound_i` is referenced by `pump_acks` but lands in Task 10. To keep THIS task GREEN, add a temporary minimal version now and replace it in Task 10:
```rust
impl Ax25Stream {
    /// TEMPORARY (full reassembly + RR-reply in Task 10): queue an in-order I-frame's
    /// info and advance V(R).
    fn accept_inbound_i(&mut self, ns: u8, _pf: bool, info: &[u8]) -> std::io::Result<()> {
        if ns == self.vr {
            self.inbound.extend(info.iter().copied());
            self.vr = (self.vr + 1) % 8;
        }
        Ok(())
    }
}
```
And the test:
```rust
#[cfg(test)]
mod write_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    /// A connected stream with a fresh peer, bypassing the connect handshake.
    fn connected(peer: &ScriptedPeer, params: Ax25Params) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let p = Path { dest: target.clone(), src: mine.clone(), digis: vec![] };
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine,
            peer: target,
            digis: vec![],
            params,
            vs: 0, vr: 0, va: 0,
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    /// Build the KISS-wrapped RR the peer sends to acknowledge through `nr`.
    fn peer_rr(mycall: &Address, peer: &Address, nr: u8) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] },
            control: Control::Rr { nr, pf: false },
            info: vec![],
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn write_under_paclen_sends_one_i_frame() {
        let peer = ScriptedPeer::new();
        let mut s = connected(&peer, Ax25Params::default());
        let n = s.write(b"HELLO").unwrap();
        assert_eq!(n, 5);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert_eq!(frames.len(), 1);
        let f = Frame::decode(&frames[0]).unwrap();
        assert!(matches!(f.control, Control::I { ns: 0, nr: 0, pf: false }));
        assert_eq!(f.info, b"HELLO");
        assert_eq!(s.vs, 1);
    }

    #[test]
    fn write_over_paclen_is_segmented() {
        let peer = ScriptedPeer::new();
        // Pre-feed enough RRs so the window never stalls.
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        for nr in 1..=4u8 {
            peer.feed(&peer_rr(&mine, &target, nr));
        }
        let mut s = connected(&peer, Ax25Params { paclen: 4, maxframe: 4, ..Ax25Params::default() });
        let n = s.write(b"ABCDEFG").unwrap(); // 7 bytes / paclen 4 ⇒ 2 segments (4 + 3)
        assert_eq!(n, 7);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let infos: Vec<Vec<u8>> = frames.iter().map(|b| Frame::decode(b).unwrap().info).collect();
        assert_eq!(infos, vec![b"ABCD".to_vec(), b"EFG".to_vec()]);
    }

    #[test]
    fn write_blocks_until_the_window_drains_then_completes() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // maxframe 2, paclen 1 ⇒ "XYZ" needs 3 frames but only 2 fit; an RR through 2
        // must free the window so the 3rd sends.
        peer.feed(&peer_rr(&mine, &target, 2));
        peer.feed(&peer_rr(&mine, &target, 3));
        let params = Ax25Params { paclen: 1, maxframe: 2, t1: Duration::from_millis(20), ..Ax25Params::default() };
        let mut s = connected(&peer, params);
        let n = s.write(b"XYZ").unwrap();
        assert_eq!(n, 3);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert_eq!(frames.len(), 3, "all three segments must eventually be sent");
        assert_eq!(s.va, 3, "V(A) advanced as RRs arrived");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::write_tests`
Expected: FAIL — write `write_tests` first without the `impl Write` / helper blocks; observe `the trait bound \`Ax25Stream: std::io::Write\` is not satisfied` / `no method named \`write\``.

- [ ] **Step 3: Write minimal implementation**

Add the `impl Ax25Stream { pump_acks, ack_through, retransmit_from, send_i, in_flight }` block, the temporary `accept_inbound_i`, and `impl Write for Ax25Stream` shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::write_tests`
Expected: PASS (single I-frame, segmentation, window-drain).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): Ax25Stream::write — PACLEN segmentation + MAXFRAME window + RR ack (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: T1 timeout retransmit + REJ retransmit, capped at N2

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

Reliability under loss. Two recovery paths: (a) **REJ** — the peer rejects at N(R); retransmit every unacked frame from N(R) (already wired in `retransmit_from` via `pump_acks` in Task 8 — this task tests it). (b) **T1 timeout** — no ack arrives within T1; retransmit the oldest unacked frame, up to N2 times, then fail the link (`TimedOut`). Add an explicit `await_ack(min_va)` helper that the write window-drain and disconnect use, with the N2 cap. Cross-check AX.25 v2.2 §6.7.1 (T1) + §4.3.2.3 (REJ) + `TNCKissInterface` retry handling.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs`:
```rust
impl Ax25Stream {
    /// Wait for V(A) to reach at least `target` (mod-8, measured as count of frames
    /// acked from the wait's start), retransmitting the oldest unacked frame each T1
    /// up to N2 times. Returns `TimedOut` if N2 is exhausted without progress (spec §5).
    fn await_ack(&mut self, target_in_flight_drained_to: usize) -> std::io::Result<()> {
        let mut retries = 0u8;
        loop {
            self.pump_acks()?;
            if self.in_flight() <= target_in_flight_drained_to {
                return Ok(());
            }
            if retries >= self.params.n2_retries {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "no acknowledgement after N2 retransmits (T1 timeout)",
                ));
            }
            // T1 expired with frames still unacked: retransmit the oldest, bump retry.
            if let Some((&ns, info)) = self.unacked.iter().next() {
                let info = info.clone();
                let f = Frame {
                    path: self.out_path(),
                    control: Control::I { ns, nr: self.vr, pf: true }, // P=1 polls for an RR
                    info,
                };
                self.send_frame(&f)?;
            }
            retries += 1;
            std::thread::sleep(self.params.t1);
        }
    }
}
```
And the tests:
```rust
#[cfg(test)]
mod recovery_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn connected(peer: &ScriptedPeer, params: Ax25Params) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine, peer: target, digis: vec![], params,
            vs: 0, vr: 0, va: 0,
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    fn peer_s(mycall: &Address, peer: &Address, control: Control) -> Vec<u8> {
        let f = Frame { path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] }, control, info: vec![] };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn rej_retransmits_from_the_rejected_sequence() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 4, ..Ax25Params::default() });
        // Send 3 frames (N(S) 0,1,2) — no acks fed, so they stay unacked.
        s.send_i(b"A").unwrap();
        s.send_i(b"B").unwrap();
        s.send_i(b"C").unwrap();
        let _ = peer.drain_tx(); // discard the originals
        // Peer rejects at N(R)=1 ⇒ retransmit frames 1 and 2 (B, C), not 0.
        peer.feed(&peer_s(&mine, &target, Control::Rej { nr: 1, pf: false }));
        s.pump_acks().unwrap();
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let resent: Vec<Vec<u8>> = frames.iter().map(|b| Frame::decode(b).unwrap().info).collect();
        assert_eq!(resent, vec![b"B".to_vec(), b"C".to_vec()]);
        assert_eq!(s.va, 1, "REJ N(R)=1 acknowledged frame 0");
    }

    #[test]
    fn t1_timeout_retransmits_then_fails_after_n2() {
        let peer = ScriptedPeer::new(); // never acks
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 1, t1: Duration::from_millis(10), n2_retries: 2, ..Ax25Params::default() });
        s.send_i(b"Z").unwrap();
        let _ = peer.drain_tx();
        // await_ack must retransmit up to N2 times, then TimedOut.
        let err = s.await_ack(0).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        // n2_retries=2 ⇒ exactly 2 retransmissions of frame Z.
        assert_eq!(frames.len(), 2, "expected N2=2 retransmits, got {}", frames.len());
        assert!(frames.iter().all(|b| Frame::decode(b).unwrap().info == b"Z"));
    }

    #[test]
    fn t1_retransmit_stops_once_acked() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 1, t1: Duration::from_millis(10), n2_retries: 5, ..Ax25Params::default() });
        s.send_i(b"Q").unwrap();
        let _ = peer.drain_tx();
        // Ack arrives before N2 is hit ⇒ await_ack returns Ok.
        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 1, pf: false }));
        s.await_ack(0).unwrap();
        assert_eq!(s.va, 1);
        assert_eq!(s.in_flight(), 0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::recovery_tests`
Expected: FAIL — write `recovery_tests` first without `await_ack`; observe `error[E0599]: no method named \`await_ack\``. (The `rej_retransmits_from_the_rejected_sequence` test exercises `pump_acks`/`retransmit_from` from Task 8 and would already compile, but the module won't compile until `await_ack` exists.)

- [ ] **Step 3: Write minimal implementation**

Add the `await_ack` method shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::recovery_tests`
Expected: PASS (REJ retransmit, T1+N2 failure, T1 stop-on-ack).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): T1 timeout retransmit (capped at N2) + REJ recovery (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: `Ax25Stream::read` — deliver inbound I-frames in order, send RR

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

The inbound data path. `read` drains the link via `pump_acks` (which now must do the full inbound job: in-order I-frame ⇒ queue info + advance V(R) + reply RR(V(R)); out-of-order ⇒ REJ(V(R))), then copies queued bytes into the caller's buffer. Replace the Task 8 temporary `accept_inbound_i` with the real one (RR reply + REJ-on-gap + reassembly across PACLEN segments). Cross-check AX.25 v2.2 §6.4.2 (I-frame receive, V(R) advance, RR/REJ) + `TNCKissInterface` receive handling.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/ax25/datalink.rs`, replace the temporary `accept_inbound_i` (from Task 8) with the full version:
```rust
impl Ax25Stream {
    /// Process an inbound I-frame. In order (N(S)==V(R)): queue its info, advance
    /// V(R), reply RR(V(R)) acknowledging it. Out of order (gap): drop it and reply
    /// REJ(V(R)) to request retransmission from the expected sequence. Reassembly
    /// across PACLEN segments is implicit — every accepted info chunk is appended to
    /// the inbound byte queue, which `read` drains in order.
    fn accept_inbound_i(&mut self, ns: u8, _pf: bool, info: &[u8]) -> std::io::Result<()> {
        if ns == self.vr {
            self.inbound.extend(info.iter().copied());
            self.vr = (self.vr + 1) % 8;
            let rr = Frame {
                path: self.out_path(),
                control: Control::Rr { nr: self.vr, pf: false },
                info: vec![],
            };
            self.send_frame(&rr)?;
        } else {
            // Sequence gap: reject, asking the peer to resend from V(R).
            let rej = Frame {
                path: self.out_path(),
                control: Control::Rej { nr: self.vr, pf: false },
                info: vec![],
            };
            self.send_frame(&rej)?;
        }
        Ok(())
    }
}

impl Read for Ax25Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Drain the link first so any freshly-arrived I-frames are queued + acked.
        self.pump_acks()?;
        // If nothing queued and the peer hasn't sent anything, poll once more so a
        // caller in a read loop makes progress without busy-spinning.
        if self.inbound.is_empty() && !self.closed {
            std::thread::sleep(POLL_INTERVAL);
            self.pump_acks()?;
        }
        let n = buf.len().min(self.inbound.len());
        for b in buf.iter_mut().take(n) {
            *b = self.inbound.pop_front().unwrap();
        }
        Ok(n)
    }
}
```
And the test:
```rust
#[cfg(test)]
mod read_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn connected(peer: &ScriptedPeer) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine, peer: target, digis: vec![],
            params: Ax25Params::default(),
            vs: 0, vr: 0, va: 0,
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    /// An inbound I-frame from the peer to us, with N(S)=ns carrying `info`.
    fn peer_i(mycall: &Address, peer: &Address, ns: u8, info: &[u8]) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] },
            control: Control::I { ns, nr: 0, pf: false },
            info: info.to_vec(),
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn read_delivers_in_order_i_frames_and_replies_rr() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // Two in-order I-frames reassemble into one byte stream.
        peer.feed(&peer_i(&mine, &target, 0, b"FOO"));
        peer.feed(&peer_i(&mine, &target, 1, b"BAR"));
        let mut s = connected(&peer);
        let mut got = Vec::new();
        let mut buf = [0u8; 16];
        // Two reads drain both queued frames.
        let n1 = s.read(&mut buf).unwrap();
        got.extend_from_slice(&buf[..n1]);
        let n2 = s.read(&mut buf).unwrap();
        got.extend_from_slice(&buf[..n2]);
        assert_eq!(got, b"FOOBAR");
        assert_eq!(s.vr, 2, "V(R) advanced past both frames");
        // We replied RR for each.
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let rrs: Vec<u8> = frames
            .iter()
            .filter_map(|b| match Frame::decode(b).unwrap().control {
                Control::Rr { nr, .. } => Some(nr),
                _ => None,
            })
            .collect();
        assert_eq!(rrs, vec![1, 2], "RR(1) then RR(2)");
    }

    #[test]
    fn out_of_order_i_frame_triggers_rej() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // We expect N(S)=0 but the peer sends N(S)=1 ⇒ gap ⇒ REJ(0), no delivery.
        peer.feed(&peer_i(&mine, &target, 1, b"OOPS"));
        let mut s = connected(&peer);
        let mut buf = [0u8; 16];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(n, 0, "out-of-order frame is not delivered");
        assert_eq!(s.vr, 0, "V(R) unchanged on a gap");
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert!(
            frames.iter().any(|b| matches!(Frame::decode(b).unwrap().control, Control::Rej { nr: 0, .. })),
            "expected a REJ(0)"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::read_tests`
Expected: FAIL — write `read_tests` first without `impl Read`; observe `the trait bound \`Ax25Stream: std::io::Read\` is not satisfied`. (Also: until `accept_inbound_i` is upgraded, `read_delivers_in_order_i_frames_and_replies_rr` fails because the temporary version sends no RR.)

- [ ] **Step 3: Write minimal implementation**

Replace the temporary `accept_inbound_i` with the full version and add `impl Read for Ax25Stream`, both shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink`
Expected: PASS (read_tests + all earlier datalink tests still green — re-run the whole `datalink` module to confirm `accept_inbound_i`'s upgrade didn't regress write/recovery tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): Ax25Stream::read — in-order I-frame delivery + RR/REJ reply (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: `disconnect()` + `Drop` — send DISC, await UA

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

Graceful teardown (spec §4.3: `Drop`/`disconnect()` sends DISC). `disconnect` flushes any pending acks, sends DISC (P=1), and awaits UA (bounded by one T1 — teardown is best-effort; we do not block the caller forever if the peer has already gone). It is idempotent (a second call is a no-op). `Drop` calls `disconnect`, ignoring the result, so a dropped `Ax25Stream` always tries to release the link. Cross-check AX.25 v2.2 §6.3.4 (DISC/UA) + `TNCKissInterface` disconnect.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs`:
```rust
impl Ax25Stream {
    /// Tear down the link: flush pending acks, send DISC (P=1), await UA (best-effort,
    /// bounded by one T1). Idempotent — a second call after the link is closed is a no-op.
    pub fn disconnect(&mut self) -> std::io::Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let _ = self.pump_acks(); // best-effort drain; teardown proceeds regardless
        let disc = Frame {
            path: self.out_path(),
            control: Control::Disc { pf: true },
            info: vec![],
        };
        self.send_frame(&disc)?;
        // Await UA for one T1; a peer that has already vanished must not hang teardown.
        let deadline = Instant::now() + self.params.t1;
        while Instant::now() < deadline {
            if let Some(frame) = self.recv_frame()? {
                if matches!(frame.control, Control::Ua { .. }) {
                    return Ok(());
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        Ok(()) // best-effort: DISC sent even if no UA came back
    }
}

impl Drop for Ax25Stream {
    fn drop(&mut self) {
        // A dropped stream always tries to release the link; ignore teardown errors.
        let _ = self.disconnect();
    }
}
```
And the test:
```rust
#[cfg(test)]
mod disconnect_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn connected(peer: &ScriptedPeer) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine, peer: target, digis: vec![],
            params: Ax25Params { t1: Duration::from_millis(20), ..Ax25Params::default() },
            vs: 0, vr: 0, va: 0,
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    #[test]
    fn disconnect_sends_disc_and_returns_on_ua() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let ua = Frame {
            path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] },
            control: Control::Ua { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&ua.encode().unwrap()));
        let mut s = connected(&peer);
        s.disconnect().unwrap();
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert!(
            frames.iter().any(|b| matches!(Frame::decode(b).unwrap().control, Control::Disc { pf: true })),
            "expected a DISC frame"
        );
    }

    #[test]
    fn disconnect_is_best_effort_when_peer_is_gone() {
        let peer = ScriptedPeer::new(); // never replies UA
        let mut s = connected(&peer);
        // Must not hang — bounded by one (tiny) T1.
        let start = Instant::now();
        s.disconnect().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "teardown must be bounded");
    }

    #[test]
    fn disconnect_is_idempotent() {
        let peer = ScriptedPeer::new();
        let mut s = connected(&peer);
        s.disconnect().unwrap();
        let _ = peer.drain_tx();
        s.disconnect().unwrap(); // second call: no-op, sends nothing
        assert!(peer.drain_tx().is_empty(), "a closed link sends no further DISC");
    }

    #[test]
    fn drop_sends_disc() {
        let peer = ScriptedPeer::new();
        {
            let _s = connected(&peer);
            // dropped at end of scope ⇒ Drop calls disconnect
        }
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert!(
            frames.iter().any(|b| matches!(Frame::decode(b).unwrap().control, Control::Disc { .. })),
            "Drop must send DISC"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::disconnect_tests`
Expected: FAIL — write `disconnect_tests` first without `disconnect`/`Drop`; observe `error[E0599]: no method named \`disconnect\``.

- [ ] **Step 3: Write minimal implementation**

Add the `pub fn disconnect` method and `impl Drop for Ax25Stream` shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::disconnect_tests`
Expected: PASS (DISC+UA, best-effort, idempotent, Drop-sends-DISC).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "feat(ax25): disconnect() + Drop — send DISC, await UA (best-effort, bounded) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: End-to-end connect→write→read→disconnect over the scripted peer

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

A single integration test that walks the whole lifecycle against the scripted peer: SABM→UA connect, send an I-frame and get its RR, receive an inbound I-frame (delivered + RR'd), then disconnect (DISC→UA). This is the "no path first exercised live at release" guard (spec §6) for the *direct* (0-digi) case plus a *digipeated* (1-relay) connect variant, proving both paths the spec calls out.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs`:
```rust
#[cfg(test)]
mod lifecycle_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn wrap(f: &Frame) -> Vec<u8> { kiss_data_frame(&f.encode().unwrap()) }

    #[test]
    fn full_session_direct_path() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let back = |dest: &Address, src: &Address, c: Control, info: Vec<u8>| {
            wrap(&Frame { path: Path { dest: dest.clone(), src: src.clone(), digis: vec![] }, control: c, info })
        };
        // Pre-script the peer's whole side: UA (connect), RR(1) (ack our I-frame),
        // one inbound I-frame, then UA (disconnect).
        peer.feed(&back(&mine, &target, Control::Ua { pf: true }, vec![]));
        peer.feed(&back(&mine, &target, Control::Rr { nr: 1, pf: false }, vec![]));
        peer.feed(&back(&mine, &target, Control::I { ns: 0, nr: 1, pf: false }, b"HI".to_vec()));
        peer.feed(&back(&mine, &target, Control::Ua { pf: true }, vec![]));

        let mut s = connect(Box::new(peer.clone()), mine.clone(), target.clone(), &[], &Ax25Params::default()).unwrap();
        assert_eq!(s.write(b"PING").unwrap(), 4);
        let mut buf = [0u8; 16];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"HI");
        s.disconnect().unwrap();

        // Our side, decoded: SABM, then an I-frame "PING", an RR for the inbound, a DISC.
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let controls: Vec<Control> = frames.iter().map(|b| Frame::decode(b).unwrap().control).collect();
        assert!(controls.iter().any(|c| matches!(c, Control::Sabm { .. })));
        assert!(controls.iter().any(|c| matches!(c, Control::I { ns: 0, .. })));
        assert!(controls.iter().any(|c| matches!(c, Control::Disc { .. })));
    }

    #[test]
    fn connect_via_one_digipeater_carries_the_relay() {
        // Spec §6: the digipeated (≥1-relay) path must be exercised before release.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let digi = call("W7RPT", 1);
        // The UA comes back addressed to us (the modem strips the path on the reply
        // surface for our purposes); recv_frame only checks dest == mycall.
        let ua = Frame { path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] }, control: Control::Ua { pf: true }, info: vec![] };
        peer.feed(&wrap(&ua));
        let s = connect(Box::new(peer.clone()), mine.clone(), target.clone(), std::slice::from_ref(&digi), &Ax25Params::default()).unwrap();
        assert_eq!(s.digis, vec![digi.clone()]);
        // Our SABM must carry the digi in its path.
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let sabm = frames.iter().map(|b| Frame::decode(b).unwrap()).find(|f| matches!(f.control, Control::Sabm { .. })).unwrap();
        assert_eq!(sabm.path.digis, vec![digi]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::lifecycle_tests`
Expected: FAIL the first time only if a behaviour gap exists — but since all primitives now exist, this is a pure integration check. To honour TDD, run it BEFORE confirming the helpers compose; if `connect_via_one_digipeater_carries_the_relay` fails because the digi isn't threaded into `out_path`/the SABM path, that is a genuine RED to fix.
Expected: PASS for `full_session_direct_path`; `connect_via_one_digipeater` may RED if `connect`'s `path` build (Task 6) dropped the digis — verify `connect` sets `digis: digis.to_vec()` and `out_path` includes them.

- [ ] **Step 3: Write minimal implementation**

If `connect_via_one_digipeater_carries_the_relay` failed: confirm `connect` (Task 6) stores `digis: digis.to_vec()` in the struct AND builds the SABM from a `Path` that includes `digis` (it builds `path` with `digis: digis.to_vec()` and sends `Control::Sabm` over that `path` — verify the SABM uses `path`, not `out_path()` which is also correct since `out_path` clones `self.digis`). No code change needed if Task 6 was implemented as written; this task only adds the integration test.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::lifecycle_tests`
Expected: PASS (both — direct + digipeated).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "test(ax25): end-to-end lifecycle (connect/write/read/disconnect) + digipeated path (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 13: TcpLink integration over a loopback `TcpListener`

**Files:**
- Modify: `src-tauri/src/winlink/ax25/datalink.rs`

Prove the state machine drives a real `TcpStream` (via `connect_link`), not just the in-memory peer — the seam P3's orchestration relies on. A loopback server on `127.0.0.1` plays a minimal KISS modem: it reads the client's SABM and replies a KISS-wrapped UA, then echoes nothing further. **No RF, no external network** (per testing-pitfalls). This closes the gap between the unit-tested state machine and the real `ByteLink` impl.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/ax25/datalink.rs`:
```rust
#[cfg(test)]
mod tcp_integration_tests {
    use super::*;
    use super::link::{connect_link, KissLinkConfig};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    #[test]
    fn connect_over_loopback_tcp_completes_sabm_ua() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);

        // The loopback "modem": read until it has a full KISS frame containing a SABM,
        // then reply a KISS-wrapped UA addressed back to N7CPZ-7. 127.0.0.1 only.
        let mine_s = mine.clone();
        let target_s = target.clone();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut decoder = KissDecoder::new();
            let mut buf = [0u8; 256];
            loop {
                let n = sock.read(&mut buf).unwrap();
                if n == 0 { return; }
                for body in decoder.push(&buf[..n]) {
                    if let Ok(f) = Frame::decode(&body) {
                        if matches!(f.control, Control::Sabm { .. }) {
                            let ua = Frame {
                                path: Path { dest: mine_s.clone(), src: target_s.clone(), digis: vec![] },
                                control: Control::Ua { pf: true },
                                info: vec![],
                            };
                            sock.write_all(&kiss_data_frame(&ua.encode().unwrap())).unwrap();
                            return;
                        }
                    }
                }
            }
        });

        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let link = connect_link(&cfg).unwrap();
        // Non-blocking-ish: the loopback server replies promptly; a generous T1 covers scheduling.
        let params = Ax25Params { t1: Duration::from_secs(2), n2_retries: 1, ..Ax25Params::default() };
        let stream = connect(link, mine, target.clone(), &[], &params).unwrap();
        assert_eq!(stream.peer, target);
        server.join().unwrap();
    }
}
```
Note: a real `TcpStream` with a 60 s read timeout (set by `connect_link`) blocks on `read` until data or timeout. The state machine's `recv_frame` treats `WouldBlock` as `Ok(None)`, but a blocking socket returns data (not `WouldBlock`) and the loopback server replies within milliseconds, so `connect`'s first `recv_frame` after the SABM read returns the UA. The generous T1 (2 s) is ample for loopback scheduling.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::tcp_integration`
Expected: FAIL — write the test first; it references `super::link::{connect_link, KissLinkConfig}`. If those imports/items are correct it should compile; observe any RED from a real timing/blocking-read mismatch (e.g. if `connect`'s read loop spins on `WouldBlock` that a blocking `TcpStream` never returns). This is a genuine integration RED to confirm the seam.

- [ ] **Step 3: Write minimal implementation**

No new product code if Tasks 3/6 were implemented as written. If the test REDs on a blocking-read interaction (the `connect` loop sleeps then re-reads, but a blocking `TcpStream::read` already waited the full T1), set the loopback `TcpStream` read timeout shorter for the test path via the existing `connect_link` `LINK_TIMEOUT` — or, the minimal fix: in `connect_link`'s TCP arm, the read timeout is already `LINK_TIMEOUT` (60 s); the test's loopback server replies before the first read returns, so `recv_frame` gets the UA on the first `read`. No change expected. If a real deadlock surfaces, the fix is to make the loopback server reply *before* the client's `connect` issues its first post-SABM read (it does — the server replies immediately after decoding the SABM).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::datalink::tcp_integration`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "test(ax25): TcpLink integration — connect over loopback TcpListener (no RF) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 14: Cross-provider Codex adversarial review of the datalink state machine

**Files:**
- (no source change unless findings require) — produces `dev/adversarial/2026-05-22-ax25-datalink-codex.md` (gitignored, local-only)

The state machine is correctness-critical and hard-to-undo (spec §9 → full robustness pipeline). After the unit + integration tests pass, run **at least one Codex round** on the datalink commits as an independent gate (project policy: cross-provider adrev is the unique value; do NOT substitute a Claude agent). This is not a TDD task — it is the verification gate that precedes declaring P2 done.

- [ ] **Step 1: Run the Codex review against the P2 datalink work**

```bash
npx --yes @openai/codex review --base main \
  "Adversarially review the AX.25 connected-mode state machine in src-tauri/src/winlink/ax25/datalink.rs. Attack angles: (1) mod-8 sequence-number wraparound in ack_through/retransmit_from/await_ack — is V(A)→N(R) walk correct across the 7→0 boundary and does it ever loop forever or under/over-ack? (2) T1/N2 retransmit: can a lost RR cause duplicate delivery or a premature link-failure? (3) REJ recovery: does retransmit_from resend the right frames given mod-8 wrap? (4) inbound REJ-on-gap: can an out-of-order burst wedge V(R)? (5) window accounting: can in_flight() under/overcount so write() deadlocks or floods past MAXFRAME? (6) disconnect/Drop: any panic or double-DISC. Cross-check control-byte and timer behaviour against AX.25 v2.2 section 6 and the decompiled TNCKissInterface (Connection/DataLinkProvider/EstablishDataLink) at dev/scratch/winlink-re/decompiled/tnckiss/. Report concrete defects with the offending line, not style nits." \
  2>&1 | tee dev/adversarial/2026-05-22-ax25-datalink-codex.md
```
Expected: a findings transcript at `dev/adversarial/2026-05-22-ax25-datalink-codex.md` (read process stdout via the `tee`, not just the file — Codex's sandbox may swallow a file-write; the `tee` is the reliable capture per CLAUDE.md). If Codex returns a usage-limit message, that is a capacity-defer (NOT a skip): wait for reset or defer the round; do not substitute Claude.

- [ ] **Step 2: Triage findings + fix true positives via the normal TDD loop**

For each genuine defect Codex reports: write a failing test reproducing it (in the relevant `#[cfg(test)] mod` of `datalink.rs`), then the minimal fix, then re-run `cargo test --manifest-path src-tauri/Cargo.toml ax25::`. Record each finding + disposition (fixed / won't-fix-because) in the PR body or handoff — the raw transcript stays local-only (gitignored).

- [ ] **Step 3: Re-run the full ax25 suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::`
Expected: PASS (all P1 + P2 tests).

- [ ] **Step 4: Commit any fixes**
```bash
git add src-tauri/src/winlink/ax25/datalink.rs
git commit -m "fix(ax25): address Codex adversarial-review findings on the datalink state machine (tuxlink-7fr)

<one line per finding + disposition>

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```
(If Codex found nothing actionable, skip the commit and note "Codex round clean" in the handoff.)

---

## Self-review (completed by author)

**1. Spec coverage (P2 slice of §4.1/§4.3/§5/§6):**
- **§4.1 transports** — `KissLinkConfig::{Tcp, Serial}` + `connect_link` (Task 3 TCP arm, Task 4 serial arm); Bluetooth uses the `Serial` variant with an rfcomm device path (no in-app BlueZ), exactly as the spec decides. The `register_socket`-style abort hook is NOT re-implemented here: the spec mirrors telnet's abort, but at the *transport* layer P2 exposes `Box<dyn ByteLink>`; the abort wiring (cloning the underlying `TcpStream`/closing the serial port) is P3's orchestration concern (it owns the lifecycle + the abort handle, mirroring `winlink_backend.rs`'s `register_socket`/`.shutdown()`). Noted as a P3 hand-off point, not a gap.
- **§4.3 AX.25 engine** — connected-mode v2.x mod-8 state machine: `connect` SABM→UA (Task 6), `answer` SABM→UA (Task 7), I-frame send + RR ack (Task 8), PACLEN segmentation + MAXFRAME window (Task 8), REJ + T1/N2 retransmit (Task 9), in-order inbound delivery + RR/REJ (Task 10), `disconnect()`/`Drop` DISC (Task 11). `Ax25Params` 1200 defaults (Task 2). KISS TNC params pushed on connect via `kiss_param` (Task 6). 0–2 digipeater path carried through `connect`/`out_path` (Task 12 digipeated test).
- **§5 error handling** — connect "no UA" bounded by N2×T1 → `TimedOut`, never a silent hang (Task 6); DM → `ConnectionRefused` (Task 6); write window-stall → `TimedOut` (Task 8); T1/N2 exhaustion → `TimedOut` (Task 9); disconnect best-effort/bounded (Task 11). Link-failure mid-exchange propagates as the `io::Error` from `recv_frame`/`send_frame` (the `Read`/`Write` impls return it to the caller, i.e. `run_exchange`, which maps it to `ExchangeError::ConnectionClosed` — verified against `session.rs`'s `write_bytes`/`read_line` error mapping).
- **§6 testing / RADIO-1 boundary** — every test runs against the scripted in-memory peer (Task 5) or a loopback `TcpListener` (Tasks 3, 13). Serial/Bluetooth are operator-on-hardware (Task 4 only checks a clean open-error on a nonexistent device — no real device, no RF). Both the direct (0-relay) AND digipeated (1-relay) connect paths are exercised (Task 12), per the spec's "neither path first exercised live at release." The Codex round (Task 14) is the §9 robustness gate.
- **CSMA** — deliberately NOT implemented; half-duplex channel access is the modem's job (stated in `datalink.rs` module doc + the `Ax25Params` persistence/slot doc). P2 only pushes the KISS params.

**2. Placeholder scan:** No "TBD"/"handle edge cases"/"similar to Task N" placeholders. Two *intentional, named* temporaries exist with explicit replacement tasks: `connect_serial` stub (Task 3 → real in Task 4) and `accept_inbound_i` minimal (Task 8 → full in Task 10); each is shown in full real Rust and its replacement is a concrete later task, not a deferred TODO. Every code step contains runnable Rust + an exact `cargo test ax25::...` command + expected result.

**3. Type consistency against the shared contracts (P1→P4 compose check):**
- Consumed-from-P1 (verified against the P1 plan's defined names): `Address{call,ssid}`, `Control::{Sabm,Disc,Ua,Dm,Rr,Rnr,Rej,I}` with `pf`/`nr`/`ns`, `Path{dest,src,digis}`, `Frame{path,control,info}` + `Frame::encode()->Result<Vec<u8>,_>` / `Frame::decode(&[u8])->Result<Frame,_>`, `KissDecoder::new()`/`.push(&[u8])->Vec<Vec<u8>>`, `kiss_data_frame(&[u8])->Vec<u8>`, `kiss_param(KissParam,u8)->Vec<u8>` + `KissParam::{TxDelay,Persistence,SlotTime}`, `PID_NO_L3`. All used exactly as P1 defines them. (Note: `Frame::encode` returns `Result`, so `send_frame` maps its error — consistent with P1's signature.)
- Defined-by-P2 (the EXACT names P3 consumes): `pub enum KissLinkConfig { Tcp { host: String, port: u16 }, Serial { device: String, baud: u32 } }`; `pub trait ByteLink: std::io::Read + std::io::Write + Send {}` + `pub fn connect_link(cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>>`; `pub struct Ax25Params { txdelay: u8, persistence: u8, slot_time: u8, paclen: usize, maxframe: u8, t1: Duration, n2_retries: u8 }` + `Default`; `pub struct Ax25Stream` impl `Read + Write` + `pub fn disconnect(&mut self)`; `pub fn connect(link: Box<dyn ByteLink>, mycall: Address, target: Address, digis: &[Address], params: &Ax25Params) -> std::io::Result<Ax25Stream>`; `pub fn answer(link: Box<dyn ByteLink>, mycall: Address, params: &Ax25Params) -> std::io::Result<(Address, Ax25Stream)>`. These match the assignment's required signatures byte-for-byte (one allowed refinement: `disconnect` returns `std::io::Result<()>` rather than the assignment's bare `disconnect(&mut self)` — a strict superset that lets the caller observe teardown errors; `Drop` ignores the result so the "sends DISC on drop" contract holds either way).

**4. Verification-during-execution points (cross-check against TNCKissInterface, spec §9 + the assignment):** Tasks 6/7/9/10/11 each name the AX.25 v2.2 section AND the `TNCKissInterface` symbol (`EstablishDataLink`, `Connection`/`DataLinkProvider`, the inbound-SABM handler, the DISC/UA path) to cross-check; the in-memory-peer tests pin the expected control bytes (SABM `0x2F|P`, UA `0x63|P`, RR with `nr<<5`, DM `0x0F|P`, DISC `0x43|P`, I with `ns<<1`/`nr<<5`) as fixtures via P1's `Frame`/`Control`, so a bit-level mismatch fails loudly. The decompiled tree is local-only scratch (gitignored, on the original author's disk per the findings doc); cited by path as the authority exactly as P1 does. Task 14's Codex round is the independent cross-provider gate on the wraparound/timer logic that fixtures alone cannot fully cover.

## Follow-on plans (not in P2)
- **P3 — Winlink-over-packet integration:** `ExchangeRole {Dial, Answer}` in `session.rs`, `TransportConfig::Packet { link: KissLinkConfig, ssid, role }`, the idle-listen lifecycle (arm `answer()` → on inbound CONNECTED run the master-role exchange → re-arm), the abort hook (clone the underlying socket / close the serial port, mirroring `winlink_backend.rs`'s `register_socket`/`.shutdown()`), config `[packet]` + global sticky SSID. **Consumes P2's exact public API above.** Coordinate `config.rs`/`winlink_backend.rs` merges with `tuxlink-686`.
- **P4 — UI:** Connections-section Packet entry, reading-pane connection panel, SSID control, ribbon/status transport+listen, session-log packet lines.
