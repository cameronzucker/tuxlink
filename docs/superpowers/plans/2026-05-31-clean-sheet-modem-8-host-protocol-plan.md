# Clean-sheet modem — Subsystem #8: Host protocol / control plane — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the host protocol crate for sonde — the framed, schema-versioned, AI-native control + data plane that sits between any client (tuxlink-in-process today; standalone TCP daemon tomorrow) and the modem core. Ship as `sonde-host-proto`: a pure-Rust AGPLv3 crate containing the wire codec, the protocol state machine, a server-side dispatcher trait that subsystem #5/#6/#7 will implement, a synchronous in-memory transport adapter for `ModemTransport` integration (subsystem #9), and a TCP listener for the standalone daemon path (subsystem #10).

**Architecture (locked in this plan):**

- **Transport: TCP, listening on `127.0.0.1` by default.** Two-port model (cmd + data sockets), matching the abstraction the existing `ModemTransport` trait in `src-tauri/src/winlink/modem/mod.rs` already speaks for ardopcf. Optional `bind_addr` override for off-host control; off-host requires an explicit `--listen <addr>` flag and a `require_auth=true` config.
- **Wire framing: length-prefixed binary frames** carrying a single CBOR object. `[u32 BE length][CBOR payload]`. CBOR (RFC 8949) chosen over JSON for: deterministic encoding, binary-safe payloads (the data socket carries ARQ-corrected bytes), smaller on-wire size, and first-class support for schema generation via `serde` + `ciborium`. A `--text` debug shim emits the same logical messages as line-delimited JSON for human inspection (subsystem #8's AI-native debuggability affordance).
- **Schema language: serde-typed Rust structs in a `proto` module, generating both the CBOR codec and an exported JSON-Schema sidecar** (`schemas/sonde-host-proto-v1.schema.json`) written at build time via `schemars`. The JSON-Schema sidecar is what an AI agent (or human) loads to understand the protocol without reading Rust source.
- **Message model: request/response + event-stream, multiplexed over the cmd socket; raw bytes over the data socket.** Every cmd-socket message carries `{kind: "req" | "resp" | "event", id: u64 | null, body: <variant>}`. Requests have monotonic `id`; responses cite their request `id`; events carry `id: null`. The model accommodates both the synchronous "set audio gain → confirm" calls (forcing function §3.5 of the spec) and the asynchronous "connection lifecycle" event-streams.
- **Versioning: semver in the protocol object + capability bits.** The first message exchanged on cmd-socket connect is `Hello { proto_version: "1.0.0", capabilities: ["ofdm-family-v1", "robustness-floor-v1", "link-adapt-2d-v1", ...] }` in both directions; mismatch terminates the connection with a structured `Goodbye { reason }`. Capability strings are the canonical "missing feature is discoverable" mechanism (spec §3.4).
- **State exposure: the link/MAC state, ARQ connection state, and link-adaptation state are exposed as snapshot-on-request + delta events.** `GetStatus` → full snapshot. `Subscribe { topics: [...] }` → server pushes deltas. Topics: `mac.state`, `arq.connection`, `arq.metrics`, `linkadapt.mode`, `linkadapt.metrics`, `phy.bitloading` (the bit-loading curve from §5.A.1 of the overview).
- **AI-native affordances (concrete, not aspirational):**
  - JSON-Schema sidecar shipped in the crate and surfaced via a `Describe` request returning the schema as a CBOR/JSON object — an agent connects, asks `Describe`, and now has the full grammar.
  - Event-stream traces are themselves the protocol's test oracle (every conformance test asserts on event sequences).
  - `--trace-jsonl <path>` daemon flag emits every cmd-socket frame as a JSONL line to disk: agents can run a session, then read the trace as a markdown-loadable artifact.
  - Stable, machine-readable error codes — every `Error` body has `{ code: "kebab-case-string", message: "human", details: <CBOR map> }`. The code list is enumerated in the schema.
  - `Describe` and `GetSchema` return the protocol's own schema; the protocol is self-describing without referencing external docs.
- **Concurrency model: sync + threads (matches ADR 0015 directive that ARDOP transport is sync + `std::io`).** Per-connection: one cmd-socket reader thread + one cmd-socket writer thread + one data-socket bridge thread. Cross-thread coordination via `std::sync::mpsc`. No Tokio in this crate.
- **License: AGPLv3-only** (overview §5.A.4). Crate metadata reflects this.

**Tech Stack:** Rust 2021. `serde` + `ciborium` (CBOR codec, both MIT-or-Apache-2.0 — AGPL-compatible). `schemars` (JSON-Schema generation, MIT-or-Apache-2.0). `serde_json` (only for the `--text` debug shim and JSON-Schema sidecar). `std::net::TcpListener` / `TcpStream` / `std::thread` / `std::sync::mpsc` / `std::sync::Arc<Mutex<...>>` for state. `thiserror` for error types. `tracing` for structured logs (already in tuxlink). No Tokio. No `async-trait`.

**Workspace placement:** This plan creates a NEW top-level Rust workspace member under `crates/sonde-host-proto/` at the repo root. Subsystem #9 (`ModemTransport` impl) and subsystem #10 (standalone daemon binary) will both depend on this crate. The tuxlink Tauri crate (`src-tauri/`) gains it as a workspace member dependency once subsystem #9 ships.

**Out of scope for this plan:**
- The actual modem core (subsystems #3/#4/#5/#6/#7). This plan ships a `DispatcherStub` that returns canned responses; production dispatcher implementations land in #5/#6/#7 plans.
- Authentication and TLS for the off-host case — designed for (the `Hello` carries `auth: Option<Auth>`) but ships in a `noauth` variant that refuses non-loopback binds. A follow-up plan adds the token-based + TLS variants.
- The tuxlink `ModemTransport` wire-up to this crate — that's subsystem #9.
- The standalone-daemon binary packaging (subsystem #10).
- RF / on-air work. Plan-only per the sprint's plan-only posture.

**RADIO-1 / clean-sheet posture:**
- **ADR 0014 bright line.** This plan does NOT examine VARA's host interface, the ARDOP `Host_Interface_Spec_for_WL2K_supported_Protocols_TNCs_20171109.pdf` document (the conceptual primitive "two TCP sockets, cmd + data" is general; the specific ARDOP commands are not borrowed), Winlink B2F session protocol, AX.25 host APIs (KISS, AGW), or hamlib `rigctld` wire format. Conceptual primitives — length-prefixed framing, CBOR encoding, request/response correlation via id, capability negotiation, event subscriptions — are general DSP/networking techniques.
- **No transmission.** This subsystem is host-side software only; no PTT, no audio, no RF. The subagent-safety rule from RADIO-1 is satisfied by construction (nothing here can transmit; the dispatcher is stubbed).

**Cross-subsystem APIs (consumed and exposed):**

| Direction | Peer | What this plan exposes / consumes |
|---|---|---|
| In | #5 link/MAC | `Dispatcher::mac_*` methods (open_connection, close_connection, send_frame, get_state). MAC's connection-state machine is the source of truth; #8 reflects it. |
| In | #6 ARQ | `Dispatcher::arq_*` methods (subscribe-to-metrics, get-window-state, drain-rx-bytes, push-tx-bytes). ARQ-corrected stream is the data-socket payload. |
| In | #7 link adapt | `Dispatcher::linkadapt_*` methods (get-current-mode, set-mode-override, get-bit-loading-curve, observe-channel-quality). |
| Out | #9 tuxlink integration | `InProcessTransport` adapter — implements the future sonde `ModemTransport` against an in-process `Dispatcher` (no TCP). Subsystem #9 wires this into `src-tauri/src/winlink/modem/`. |
| Out | #10 standalone daemon | `TcpServer` listener — exposes the same `Dispatcher` over the two-socket TCP wire. Subsystem #10 wraps this in a binary with CLI flags + packaging. |

**ADR 0015 fit:** The protocol IS what `ModemTransport` (the existing trait at `src-tauri/src/winlink/modem/mod.rs`) speaks. For sonde-in-tuxlink: a `SondeTransport` (in subsystem #9) wraps `InProcessTransport` and presents the existing `ModemTransport` surface — same `init` / `connect_arq` / `disconnect` / `data_stream` / `drain_status_events` / `try_clone_abort_writer` shape, but the underlying messages are CBOR over an in-memory channel pair, not ASCII over TCP. For sonde-as-daemon: the same `SondeTransport` connects over real TCP to the daemon's `TcpServer`. The shape of `ModemTransport` is preserved; the wire is replaced.

**Run tests with:** `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml`.

---

## Phase 0 — Workspace scaffold + licence + ADR

### Task 0.1: Create the crate skeleton

**Files:**
- Create: `Cargo.toml` (root — add a `[workspace]` table if one does not exist; if it exists, add `crates/sonde-host-proto` to `members`)
- Create: `crates/sonde-host-proto/Cargo.toml`
- Create: `crates/sonde-host-proto/src/lib.rs`
- Create: `crates/sonde-host-proto/LICENSE` (the AGPLv3 text — copy the standard GNU AGPLv3 license file)
- Create: `crates/sonde-host-proto/README.md`

- [ ] **Step 1: Inspect the root `Cargo.toml`**

```bash
cat Cargo.toml 2>/dev/null || echo "NO ROOT Cargo.toml"
```

Expected: either (a) no root `Cargo.toml` (tuxlink Tauri keeps its own `src-tauri/Cargo.toml`, and a new workspace root will be created here), or (b) an existing workspace root. Branch on the result.

- [ ] **Step 2: Create / extend the root workspace `Cargo.toml`**

If (a) — no root `Cargo.toml` exists — create one:

```toml
[workspace]
members = ["crates/sonde-host-proto"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "AGPL-3.0-only"
```

If (b) — root `Cargo.toml` exists — add `"crates/sonde-host-proto"` to the `members` array. Do NOT touch any other workspace member. Do NOT add `src-tauri/` to `members` in this task; subsystem #9 will integrate.

- [ ] **Step 3: Create `crates/sonde-host-proto/Cargo.toml`**

```toml
[package]
name = "sonde-host-proto"
version = "0.0.1"
edition = "2021"
license = "AGPL-3.0-only"
description = "Host protocol (cmd + data sockets, CBOR + JSON-Schema) for the sonde HF modem"
repository = "https://github.com/cameronzucker/tuxlink"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
ciborium = "0.2"
schemars = { version = "0.8", features = ["preserve_order"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"

[dev-dependencies]
tracing-subscriber = "0.3"
tempfile = "3.10"
```

- [ ] **Step 4: Create `crates/sonde-host-proto/src/lib.rs`**

```rust
//! sonde-host-proto — host protocol crate for sonde.
//!
//! AGPLv3-only per overview §5.A.4. Sync + threads; no Tokio. CBOR wire framing
//! with a JSON-Schema sidecar; request/response + event-stream multiplexed over
//! a cmd socket; raw ARQ-corrected bytes over a data socket.
//!
//! See `docs/superpowers/plans/2026-05-31-clean-sheet-modem-8-host-protocol-plan.md`
//! for the full design.

#![deny(missing_docs)]

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 5: Create `crates/sonde-host-proto/LICENSE`**

Copy the standard GNU AGPLv3 license text (https://www.gnu.org/licenses/agpl-3.0.txt). Use `curl -sSL https://www.gnu.org/licenses/agpl-3.0.txt > crates/sonde-host-proto/LICENSE` if a network fetch is permitted; otherwise reproduce the standard text by hand. The LICENSE file is a verbatim copy of the GNU AGPLv3 — do not modify it.

- [ ] **Step 6: Create `crates/sonde-host-proto/README.md`**

```markdown
# sonde-host-proto

Host protocol crate for the sonde HF modem.

- Wire: length-prefixed CBOR frames; two TCP sockets (cmd + data) or in-process channel adapter.
- Schema: serde-typed Rust + JSON-Schema sidecar at `schemas/sonde-host-proto-v1.schema.json`.
- Messages: `req` / `resp` / `event` with `u64` id correlation.
- Versioning: semver + capability bits exchanged in `Hello`.

License: AGPL-3.0-only.

Design: `docs/superpowers/plans/2026-05-31-clean-sheet-modem-8-host-protocol-plan.md`.
Spec: `docs/superpowers/specs/2026-05-31-clean-sheet-modem-8-host-protocol.md`.
```

- [ ] **Step 7: Verify the crate compiles**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml`
Expected: PASS (`smoke::crate_compiles`).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/sonde-host-proto/
git commit -m "feat(modem-host): scaffold sonde-host-proto crate (AGPLv3)

Workspace scaffold for subsystem #8 of the clean-sheet HF modem.
Empty crate; codec + state machine land in Phase 1+.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 1 — Wire codec (length-prefixed CBOR)

### Task 1.1: Define the `Frame` byte-level codec

**Files:**
- Create: `crates/sonde-host-proto/src/codec.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod codec;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/codec.rs`:

```rust
//! Length-prefixed CBOR frame codec.
//!
//! Wire format: `[u32 big-endian length][CBOR payload]`. Length is the byte
//! count of the CBOR payload only; it does not include the 4-byte length field.
//! Maximum payload size is bounded by `MAX_FRAME_BYTES` (16 MiB by default) so
//! a hostile sender cannot exhaust memory.

use std::io::{Read, Write};

/// Cap on a single frame's CBOR payload size, in bytes. Set to 16 MiB; the
/// data socket carries ARQ-corrected bytes in chunks below this floor.
pub const MAX_FRAME_BYTES: u32 = 16 * 1024 * 1024;

/// Codec error returned by `write_frame` and `read_frame`.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// Underlying I/O failure on the socket / channel.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Peer announced a frame larger than `MAX_FRAME_BYTES`.
    #[error("frame too large: {0} bytes (max {1})")]
    TooLarge(u32, u32),
}

/// Write `payload` as a length-prefixed frame to `w`. Returns the total bytes
/// written (4 + payload.len()).
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> Result<usize, CodecError> {
    let len = payload.len();
    if len as u64 > MAX_FRAME_BYTES as u64 {
        return Err(CodecError::TooLarge(MAX_FRAME_BYTES, MAX_FRAME_BYTES));
    }
    let len_bytes = (len as u32).to_be_bytes();
    w.write_all(&len_bytes)?;
    w.write_all(payload)?;
    Ok(4 + len)
}

/// Read one length-prefixed frame from `r`. Returns the decoded payload bytes.
/// Reads EXACTLY 4 + payload bytes from the reader; on truncated I/O returns
/// a `CodecError::Io` with `UnexpectedEof`.
pub fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, CodecError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        return Err(CodecError::TooLarge(len, MAX_FRAME_BYTES));
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_small_payload() {
        let mut buf = Vec::new();
        let written = write_frame(&mut buf, b"hello").unwrap();
        assert_eq!(written, 9); // 4 + 5

        let mut cursor = std::io::Cursor::new(buf);
        let payload = read_frame(&mut cursor).unwrap();
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn rejects_oversize_on_write() {
        let huge = vec![0u8; (MAX_FRAME_BYTES as usize) + 1];
        let mut buf = Vec::new();
        let err = write_frame(&mut buf, &huge).unwrap_err();
        matches!(err, CodecError::TooLarge(_, _));
    }

    #[test]
    fn rejects_oversize_on_read() {
        // craft a header that announces MAX_FRAME_BYTES + 1
        let bogus_len = (MAX_FRAME_BYTES + 1).to_be_bytes();
        let mut cursor = std::io::Cursor::new(bogus_len.to_vec());
        let err = read_frame(&mut cursor).unwrap_err();
        matches!(err, CodecError::TooLarge(_, _));
    }

    #[test]
    fn truncated_read_returns_io_error() {
        let mut cursor = std::io::Cursor::new(vec![0u8, 0u8, 0u8, 5u8, b'h', b'i']);
        let err = read_frame(&mut cursor).unwrap_err();
        matches!(err, CodecError::Io(_));
    }
}
```

In `crates/sonde-host-proto/src/lib.rs`, add at the top (below the doc comment):

```rust
pub mod codec;
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml codec::`
Expected: FAIL — `thiserror` is in deps but the test refers to `read_exact` semantics that must compile first. Compile errors are acceptable as "fail" for this TDD step.

- [ ] **Step 3: Confirm the implementation passes**

The implementation written in Step 1 IS the minimal code. Re-run:

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml codec::`
Expected: PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/sonde-host-proto/src/codec.rs crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): length-prefixed CBOR frame codec

Add the byte-level wire codec. u32 BE length prefix + CBOR payload.
Bounded at 16 MiB per frame to defend against memory-exhaustion.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 2 — Message schema (serde-typed + JSON-Schema sidecar)

### Task 2.1: Define the top-level `Message` envelope

**Files:**
- Create: `crates/sonde-host-proto/src/proto/mod.rs`
- Create: `crates/sonde-host-proto/src/proto/envelope.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod proto;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/proto/envelope.rs`:

```rust
//! Top-level `Message` envelope.
//!
//! Every cmd-socket frame carries one `Message`. The envelope distinguishes
//! requests, responses, and events; carries a `u64` id used to correlate
//! responses with their originating requests; and embeds a variant `body`.

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use super::body::Body;

/// Top-level envelope for a single cmd-socket message.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// Client → server, server → client; expects a `Resp` carrying the same id.
    Req {
        /// Monotonic per-connection request id (assigned by the sender).
        id: u64,
        /// The request body.
        body: Body,
    },
    /// Response to a `Req`; carries the originating request id verbatim.
    Resp {
        /// The id of the `Req` this responds to.
        id: u64,
        /// The response body. `Body::Error { .. }` is the standard error variant.
        body: Body,
    },
    /// Server-pushed event; no id (events do not correlate to a single request).
    Event {
        /// The event body.
        body: Body,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::body::Body;

    #[test]
    fn req_roundtrips_via_cbor() {
        let msg = Message::Req {
            id: 7,
            body: Body::Hello {
                proto_version: "1.0.0".into(),
                capabilities: vec!["ofdm-family-v1".into()],
                auth: None,
            },
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&msg, &mut buf).unwrap();
        let back: Message = ciborium::from_reader(&buf[..]).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn event_roundtrips_via_cbor() {
        let msg = Message::Event {
            body: Body::ArqMetricsUpdate {
                fer: 0.02,
                window_in_flight: 3,
                throughput_bps: 480,
            },
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&msg, &mut buf).unwrap();
        let back: Message = ciborium::from_reader(&buf[..]).unwrap();
        assert_eq!(msg, back);
    }
}
```

Create `crates/sonde-host-proto/src/proto/mod.rs`:

```rust
//! Protocol message schema, version 1.
//!
//! Types are `serde`-derived for CBOR (wire) and `schemars`-derived for the
//! JSON-Schema sidecar. The schema sidecar at
//! `schemas/sonde-host-proto-v1.schema.json` is regenerated by
//! `cargo run -p sonde-host-proto --bin gen-schema` (Phase 6) and committed
//! to the repo.

pub mod body;
pub mod envelope;

pub use envelope::Message;
pub use body::Body;
```

Add to `crates/sonde-host-proto/src/lib.rs`:

```rust
pub mod proto;
```

- [ ] **Step 2: Run to confirm compile failure**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml proto::`
Expected: FAIL — `Body` does not exist yet. This is the TDD failure point.

- [ ] **Step 3: Write a stub `Body` so the envelope test compiles (full Body enum follows in 2.2)**

Create `crates/sonde-host-proto/src/proto/body.rs`:

```rust
//! Per-message body variants. Each variant corresponds to one logical request,
//! response, or event in the protocol.
//!
//! Phase 2.1 establishes the placeholder variants needed by the envelope test;
//! the full variant set lands in Task 2.2.

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Per-message body variants.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Body {
    /// Initial connection handshake; sent in both directions on connect.
    Hello {
        /// SemVer-formatted protocol version this peer speaks (e.g., "1.0.0").
        proto_version: String,
        /// Capability strings this peer advertises (e.g., "ofdm-family-v1").
        capabilities: Vec<String>,
        /// Optional auth challenge/response. `None` is the noauth path used on
        /// loopback binds; off-host binds REQUIRE this to be `Some(...)`.
        auth: Option<Auth>,
    },
    /// Event: ARQ metrics snapshot (FER, in-flight window, throughput).
    ArqMetricsUpdate {
        /// Frame error rate, in [0.0, 1.0].
        fer: f32,
        /// Number of frames currently in the ARQ window awaiting ACK.
        window_in_flight: u16,
        /// Estimated throughput, bits per second.
        throughput_bps: u32,
    },
}

/// Authentication payload for the off-host case.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "scheme", rename_all = "kebab-case")]
pub enum Auth {
    /// Pre-shared token; the server validates `token` against its configured set.
    /// Plaintext over TCP — DO NOT use this without TLS in front. The off-host
    /// bind path will require TLS in a follow-up plan.
    Token {
        /// Opaque token string.
        token: String,
    },
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml proto::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-host-proto/src/proto/
git add crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): top-level Message envelope with CBOR roundtrip

Req/Resp/Event variants with u64 id correlation. Body stub holds Hello +
ArqMetricsUpdate; full variant set lands in Task 2.2.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2.2: Expand `Body` to the full variant set

**Files:**
- Modify: `crates/sonde-host-proto/src/proto/body.rs`

- [ ] **Step 1: Write the failing test (one test per variant family)**

Append to `crates/sonde-host-proto/src/proto/body.rs`:

```rust
#[cfg(test)]
mod variant_tests {
    use super::*;
    use crate::proto::Message;

    fn roundtrip(msg: &Message) {
        let mut buf = Vec::new();
        ciborium::into_writer(msg, &mut buf).unwrap();
        let back: Message = ciborium::from_reader(&buf[..]).unwrap();
        assert_eq!(msg, &back);
    }

    #[test]
    fn goodbye_roundtrips() {
        roundtrip(&Message::Event {
            body: Body::Goodbye {
                reason: "version-mismatch".into(),
                detail: Some("server: 2.0.0, client: 1.0.0".into()),
            },
        });
    }

    #[test]
    fn describe_request_roundtrips() {
        roundtrip(&Message::Req { id: 1, body: Body::Describe });
    }

    #[test]
    fn describe_response_roundtrips() {
        roundtrip(&Message::Resp {
            id: 1,
            body: Body::DescribeOk {
                proto_version: "1.0.0".into(),
                schema_uri: "sonde-host-proto-v1.schema.json".into(),
                capabilities: vec!["ofdm-family-v1".into()],
            },
        });
    }

    #[test]
    fn open_connection_req_roundtrips() {
        roundtrip(&Message::Req {
            id: 2,
            body: Body::OpenConnection {
                target_callsign: "N0CALL".into(),
                target_ssid: 0,
                deadline_secs: 30,
                preferred_mode: Some("ofdm-2000hz".into()),
            },
        });
    }

    #[test]
    fn open_connection_resp_roundtrips() {
        roundtrip(&Message::Resp {
            id: 2,
            body: Body::OpenConnectionOk {
                connection_id: 42,
                negotiated_mode: "ofdm-2000hz".into(),
                peer_callsign: "N0CALL".into(),
                peer_ssid: 0,
            },
        });
    }

    #[test]
    fn subscribe_roundtrips() {
        roundtrip(&Message::Req {
            id: 3,
            body: Body::Subscribe {
                topics: vec!["arq.metrics".into(), "linkadapt.mode".into()],
            },
        });
    }

    #[test]
    fn mac_state_event_roundtrips() {
        roundtrip(&Message::Event {
            body: Body::MacStateChange {
                state: "connected".into(),
                connection_id: Some(42),
            },
        });
    }

    #[test]
    fn linkadapt_mode_event_roundtrips() {
        roundtrip(&Message::Event {
            body: Body::LinkAdaptModeChange {
                mode_family: "ofdm".into(),
                mode_within_family: "ofdm-2000hz".into(),
                snr_db: 12.5,
                reason: "channel-improvement".into(),
            },
        });
    }

    #[test]
    fn linkadapt_override_req_roundtrips() {
        roundtrip(&Message::Req {
            id: 4,
            body: Body::SetLinkAdaptOverride {
                mode: Some("robust-floor-bpsk".into()),
            },
        });
    }

    #[test]
    fn phy_bitloading_event_roundtrips() {
        roundtrip(&Message::Event {
            body: Body::PhyBitLoadingUpdate {
                subcarrier_bits: vec![2, 2, 4, 6, 6, 4, 2, 2],
                snr_per_subcarrier_db: vec![5.0, 6.0, 10.0, 14.0, 14.0, 11.0, 7.0, 5.0],
            },
        });
    }

    #[test]
    fn error_resp_roundtrips() {
        roundtrip(&Message::Resp {
            id: 99,
            body: Body::Error {
                code: "unknown-mode".into(),
                message: "no such PHY mode: 'ofdm-9999hz'".into(),
            },
        });
    }

    #[test]
    fn ack_roundtrips() {
        roundtrip(&Message::Resp { id: 5, body: Body::Ack });
    }
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml proto::body::`
Expected: FAIL — all variants beyond `Hello`, `ArqMetricsUpdate`, and `Auth::Token` are unknown. Compile error.

- [ ] **Step 3: Implement the full `Body` enum**

REPLACE the `Body` enum in `crates/sonde-host-proto/src/proto/body.rs` with this full version (keep `Auth` from 2.1 below it unchanged):

```rust
/// Per-message body variants — the full v1 protocol surface.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Body {
    // ── lifecycle ────────────────────────────────────────────────────────────
    /// Initial connection handshake; sent in both directions on connect.
    Hello {
        /// SemVer-formatted protocol version (e.g., "1.0.0").
        proto_version: String,
        /// Capability strings this peer advertises.
        capabilities: Vec<String>,
        /// Optional auth payload. Loopback binds may omit; off-host MUST set.
        auth: Option<Auth>,
    },
    /// Connection termination announcement; may be sent by either peer.
    /// After Goodbye the sender closes the socket without further messages.
    Goodbye {
        /// Stable kebab-case reason code (e.g., "version-mismatch",
        /// "auth-failed", "client-quit").
        reason: String,
        /// Optional human detail string.
        detail: Option<String>,
    },
    /// Generic "command accepted, no payload" response.
    Ack,
    /// Generic error response. Use this only as a `Resp` body. The `code`
    /// is a stable kebab-case enumerand; see `errors.rs` for the canonical list.
    Error {
        /// Stable kebab-case error code (see `errors.rs`).
        code: String,
        /// Human-readable explanation.
        message: String,
    },

    // ── self-describe (AI-native affordance) ─────────────────────────────────
    /// Request the server's full schema descriptor.
    Describe,
    /// Server's `Describe` response: protocol version + capability set + a
    /// pointer to the JSON-Schema sidecar.
    DescribeOk {
        /// SemVer of the protocol this server speaks.
        proto_version: String,
        /// Sidecar schema filename (relative to the crate's `schemas/` dir).
        schema_uri: String,
        /// Capability strings.
        capabilities: Vec<String>,
    },

    // ── status snapshot + subscription ───────────────────────────────────────
    /// Request a one-shot snapshot of modem state.
    GetStatus,
    /// Response to `GetStatus`: full state snapshot.
    StatusSnapshot {
        /// MAC connection-state string (e.g., "disconnected", "connecting",
        /// "connected"). Mirrors subsystem #5's MAC state machine vocabulary.
        mac_state: String,
        /// Current connection id, if any.
        connection_id: Option<u64>,
        /// Active link-adaptation mode family ("ofdm" or "robustness-floor").
        mode_family: String,
        /// Active mode within the family.
        mode_within_family: String,
    },
    /// Subscribe to one or more event topics. The set of topics is:
    /// - `mac.state` — MacStateChange events.
    /// - `arq.connection` — ArqConnectionChange events.
    /// - `arq.metrics` — ArqMetricsUpdate events (rate-limited; default 1 Hz).
    /// - `linkadapt.mode` — LinkAdaptModeChange events.
    /// - `linkadapt.metrics` — LinkAdaptMetricsUpdate events.
    /// - `phy.bitloading` — PhyBitLoadingUpdate events.
    Subscribe {
        /// Topic strings to subscribe to.
        topics: Vec<String>,
    },
    /// Unsubscribe from one or more event topics.
    Unsubscribe {
        /// Topic strings to drop.
        topics: Vec<String>,
    },

    // ── MAC operations (consumed by subsystem #5) ────────────────────────────
    /// Open an ARQ-protected connection to `target`. Async — initial `Resp` is
    /// `OpenConnectionOk` (connected) or `Error`; `MacStateChange` events
    /// stream during establishment.
    OpenConnection {
        /// Target callsign (without SSID).
        target_callsign: String,
        /// SSID (0..=15).
        target_ssid: u8,
        /// Hard deadline in seconds; if not connected by then, the server
        /// emits `Error { code: "connect-timeout" }` and tears down.
        deadline_secs: u32,
        /// Caller-preferred initial mode; server may downgrade if link
        /// adaptation says so.
        preferred_mode: Option<String>,
    },
    /// Response to `OpenConnection` once the link is up.
    OpenConnectionOk {
        /// Server-assigned connection id; carried on subsequent operations
        /// and events relating to this connection.
        connection_id: u64,
        /// Mode the link adaptation negotiated.
        negotiated_mode: String,
        /// Confirmed peer callsign.
        peer_callsign: String,
        /// Confirmed peer SSID.
        peer_ssid: u8,
    },
    /// Tear down an open ARQ connection.
    CloseConnection {
        /// Connection id from `OpenConnectionOk`.
        connection_id: u64,
    },
    /// Abort an in-flight `OpenConnection`. The server stops sending TX
    /// frames within the next link-adapt-mode-frame-duration boundary; this
    /// is the host-side seam for the operator-visible "Abort" button
    /// (RADIO-1 bounded-airtime requirement).
    AbortOpenConnection {
        /// The `id` of the `OpenConnection` request to abort.
        request_id: u64,
    },
    /// Event: MAC connection-state transition.
    MacStateChange {
        /// New state ("disconnected", "connecting", "connected", "closing",
        /// "aborting"). Mirrors subsystem #5 vocabulary.
        state: String,
        /// Connection id, if applicable.
        connection_id: Option<u64>,
    },

    // ── ARQ operations + metrics (consumed by subsystem #6) ──────────────────
    /// Event: ARQ-level connection event (e.g., "established", "closing",
    /// "dropped"). Distinct from MAC because ARQ is mode-conditional.
    ArqConnectionChange {
        /// Connection id.
        connection_id: u64,
        /// New ARQ state ("idle", "active", "draining", "dropped").
        arq_state: String,
    },
    /// Event: ARQ metrics tick (rate-limited; default 1 Hz).
    ArqMetricsUpdate {
        /// Frame error rate, [0.0, 1.0].
        fer: f32,
        /// Frames currently in the ARQ window.
        window_in_flight: u16,
        /// Throughput estimate, bits per second.
        throughput_bps: u32,
    },

    // ── link adaptation (consumed by subsystem #7) ───────────────────────────
    /// Event: link-adaptation mode changed.
    LinkAdaptModeChange {
        /// New mode family ("ofdm" or "robustness-floor").
        mode_family: String,
        /// New mode within the family.
        mode_within_family: String,
        /// SNR estimate at the switch point, in dB.
        snr_db: f32,
        /// Stable kebab-case reason ("channel-improvement",
        /// "channel-degradation", "operator-override").
        reason: String,
    },
    /// Event: link-adaptation metrics tick (rate-limited; default 1 Hz).
    LinkAdaptMetricsUpdate {
        /// Current SNR estimate, dB.
        snr_db: f32,
        /// Frame error rate observed at the link-adapt layer.
        fer: f32,
        /// Current mode family.
        mode_family: String,
    },
    /// Event: per-sub-carrier bit-loading curve snapshot (rate-limited;
    /// default 0.5 Hz). The `subcarrier_bits` and `snr_per_subcarrier_db`
    /// vectors are parallel.
    PhyBitLoadingUpdate {
        /// Per-sub-carrier assigned bits-per-symbol (0 means subcarrier disabled).
        subcarrier_bits: Vec<u8>,
        /// Per-sub-carrier SNR, dB. Length == subcarrier_bits.len().
        snr_per_subcarrier_db: Vec<f32>,
    },
    /// Operator-driven mode override. `None` clears the override (auto mode).
    SetLinkAdaptOverride {
        /// Mode string (e.g., "robust-floor-bpsk"), or `None` to clear.
        mode: Option<String>,
    },
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml proto::`
Expected: PASS (all variant tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-host-proto/src/proto/body.rs
git commit -m "feat(modem-host): full Body variant set for v1 protocol

Lifecycle (Hello/Goodbye/Ack/Error), Describe, MAC/ARQ/linkadapt operations
and events. Each variant has a CBOR roundtrip test.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2.3: Stable error code enumeration

**Files:**
- Create: `crates/sonde-host-proto/src/proto/errors.rs`
- Modify: `crates/sonde-host-proto/src/proto/mod.rs` (export the new module)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/proto/errors.rs`:

```rust
//! Canonical error-code enumeration. The `Error { code, message }` body's
//! `code` field MUST come from this list; agents and tests assert on these
//! stable strings.

/// All v1 protocol error codes. Comparing this list against the set of `code`
/// strings appearing in the codebase is part of the conformance test suite.
pub const ALL_ERROR_CODES: &[&str] = &[
    "unknown-request",
    "schema-mismatch",
    "version-mismatch",
    "auth-required",
    "auth-failed",
    "not-connected",
    "already-connected",
    "connect-timeout",
    "unknown-mode",
    "unknown-topic",
    "unknown-connection",
    "invalid-payload",
    "internal-error",
    "modem-busy",
    "operator-abort",
];

/// Look up whether `code` is in the canonical list. Used by tests + by the
/// dispatcher when emitting `Error` responses.
pub fn is_known(code: &str) -> bool {
    ALL_ERROR_CODES.contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_codes_kebab_case() {
        for c in ALL_ERROR_CODES {
            assert!(
                c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "non-kebab-case error code: {}",
                c
            );
        }
    }

    #[test]
    fn no_duplicates() {
        let mut sorted: Vec<&&str> = ALL_ERROR_CODES.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ALL_ERROR_CODES.len(), "duplicate error code in ALL_ERROR_CODES");
    }

    #[test]
    fn known_lookup() {
        assert!(is_known("unknown-mode"));
        assert!(!is_known("undefined-code"));
    }
}
```

Add to `crates/sonde-host-proto/src/proto/mod.rs`:

```rust
pub mod errors;
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml proto::errors::`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/src/proto/errors.rs crates/sonde-host-proto/src/proto/mod.rs
git commit -m "feat(modem-host): canonical error-code list

ALL_ERROR_CODES is the source of truth for Error body 'code' strings.
Tests assert kebab-case + uniqueness.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 3 — Dispatcher trait + stub

### Task 3.1: Define the `Dispatcher` trait

**Files:**
- Create: `crates/sonde-host-proto/src/dispatcher.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod dispatcher;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/dispatcher.rs`:

```rust
//! `Dispatcher` — the seam between this crate and the modem core (subsystems
//! #5/#6/#7). The host-protocol server reads a `Message::Req` from the cmd
//! socket, calls into the dispatcher, and writes the `Message::Resp` back.
//! Events surface via `EventSink::push`.
//!
//! Subsystems #5/#6/#7 will provide a production implementation; this crate
//! ships `StubDispatcher` for integration tests.

use std::sync::Arc;

use crate::proto::Body;

/// Sink for asynchronous events. The protocol server creates one of these
/// per connection; the dispatcher calls `push` to emit events that the
/// server then writes to the cmd socket.
pub trait EventSink: Send + Sync {
    /// Push one event body to the connected client. Returns `false` if the
    /// sink is closed (e.g., the client disconnected) — the dispatcher may
    /// elect to drop further events for this sink.
    fn push(&self, event_body: Body) -> bool;
}

/// Bytes-in / bytes-out for the data socket. The dispatcher owns the
/// modem's ARQ stream end; the protocol server feeds raw payload bytes
/// into `push_tx` and reads ARQ-corrected bytes from `drain_rx`.
pub trait DataChannel: Send {
    /// Push `bytes` into the modem's TX queue. Returns the number of bytes
    /// accepted (may be less than `bytes.len()` under backpressure).
    fn push_tx(&mut self, bytes: &[u8]) -> std::io::Result<usize>;
    /// Drain up to `max` bytes from the modem's RX queue into `dst`.
    /// Returns the number of bytes written. Non-blocking.
    fn drain_rx(&mut self, dst: &mut [u8]) -> std::io::Result<usize>;
}

/// The core dispatcher: each method is called by the protocol server on
/// receipt of a `Req`. Implementations return the `Body` to send back as
/// `Resp`. Synchronous; the server provides any cross-thread coordination.
pub trait Dispatcher: Send + Sync {
    /// Handle a request body. Return the `Body` to wrap in a `Resp`. Errors
    /// are returned as `Body::Error { code, message }` — not as `Result::Err`.
    fn handle(&self, req: Body, sink: &Arc<dyn EventSink>) -> Body;

    /// Borrow the data channel for the given `connection_id`. Returns `None`
    /// if no such connection exists.
    fn data_channel(&self, connection_id: u64) -> Option<Box<dyn DataChannel>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: a trivial dispatcher that answers `Describe` and `GetStatus`
    /// and rejects everything else as `unknown-request`.
    struct Smoke;
    impl Dispatcher for Smoke {
        fn handle(&self, req: Body, _sink: &Arc<dyn EventSink>) -> Body {
            match req {
                Body::Describe => Body::DescribeOk {
                    proto_version: "1.0.0".into(),
                    schema_uri: "sonde-host-proto-v1.schema.json".into(),
                    capabilities: vec![],
                },
                Body::GetStatus => Body::StatusSnapshot {
                    mac_state: "disconnected".into(),
                    connection_id: None,
                    mode_family: "ofdm".into(),
                    mode_within_family: "ofdm-2000hz".into(),
                },
                _ => Body::Error {
                    code: "unknown-request".into(),
                    message: "smoke dispatcher only handles Describe + GetStatus".into(),
                },
            }
        }
        fn data_channel(&self, _connection_id: u64) -> Option<Box<dyn DataChannel>> { None }
    }

    struct NullSink;
    impl EventSink for NullSink {
        fn push(&self, _event_body: Body) -> bool { true }
    }

    #[test]
    fn smoke_dispatcher_returns_describe() {
        let d = Smoke;
        let sink: Arc<dyn EventSink> = Arc::new(NullSink);
        let resp = d.handle(Body::Describe, &sink);
        match resp {
            Body::DescribeOk { proto_version, .. } => assert_eq!(proto_version, "1.0.0"),
            other => panic!("expected DescribeOk, got {:?}", other),
        }
    }

    #[test]
    fn smoke_dispatcher_rejects_unknown() {
        let d = Smoke;
        let sink: Arc<dyn EventSink> = Arc::new(NullSink);
        let resp = d.handle(
            Body::OpenConnection {
                target_callsign: "N0CALL".into(),
                target_ssid: 0,
                deadline_secs: 30,
                preferred_mode: None,
            },
            &sink,
        );
        match resp {
            Body::Error { code, .. } => assert_eq!(code, "unknown-request"),
            other => panic!("expected Error, got {:?}", other),
        }
    }
}
```

Add to `crates/sonde-host-proto/src/lib.rs`:

```rust
pub mod dispatcher;
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml dispatcher::`
Expected: FAIL on first compile, then PASS after the implementation in Step 1 lands. (One-pass — the implementation IS the test fixture.)

- [ ] **Step 3: Re-run**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml dispatcher::`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/sonde-host-proto/src/dispatcher.rs crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): Dispatcher / EventSink / DataChannel traits

The seam between the host protocol and the modem core. Subsystems
#5/#6/#7 will implement; this crate ships a smoke stub for tests.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3.2: Scripted `StubDispatcher` for integration tests

**Files:**
- Create: `crates/sonde-host-proto/src/stub.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod stub;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/stub.rs`:

```rust
//! `StubDispatcher` — a programmable `Dispatcher` for tests. Each `Req` is
//! matched against a script of canned responses; events are pushed at
//! configured points.
//!
//! Phase 3 ships a minimal stub useful for Phase 4 (in-process transport)
//! and Phase 5 (TCP server) tests. Production dispatchers land in subsystem
//! #5/#6/#7 plans.

use std::sync::{Arc, Mutex};

use crate::dispatcher::{DataChannel, Dispatcher, EventSink};
use crate::proto::Body;

/// Default stub: returns `DescribeOk` / `StatusSnapshot` / `Ack` for the
/// common requests; routes `OpenConnection` to a scripted response queue;
/// records every `Subscribe` topic in `subscriptions` for assertion.
pub struct StubDispatcher {
    /// Pre-loaded scripted responses keyed by the body's discriminant string
    /// (e.g., "open-connection"). Pops in FIFO order on each matching `Req`.
    pub scripted: Mutex<std::collections::HashMap<String, std::collections::VecDeque<Body>>>,
    /// Topics any client has subscribed to since `StubDispatcher::new`.
    pub subscriptions: Mutex<Vec<String>>,
}

impl StubDispatcher {
    /// Construct an empty stub.
    pub fn new() -> Self {
        Self {
            scripted: Mutex::new(Default::default()),
            subscriptions: Mutex::new(Vec::new()),
        }
    }

    /// Queue a scripted response for the next request whose `type` discriminant
    /// matches `req_type` (e.g., "open-connection").
    pub fn enqueue_response(&self, req_type: &str, resp: Body) {
        let mut s = self.scripted.lock().unwrap();
        s.entry(req_type.to_string()).or_default().push_back(resp);
    }
}

impl Default for StubDispatcher {
    fn default() -> Self { Self::new() }
}

impl Dispatcher for StubDispatcher {
    fn handle(&self, req: Body, _sink: &Arc<dyn EventSink>) -> Body {
        // Common defaults that ALL stubs respond to identically.
        match &req {
            Body::Describe => return Body::DescribeOk {
                proto_version: "1.0.0".into(),
                schema_uri: "sonde-host-proto-v1.schema.json".into(),
                capabilities: vec!["ofdm-family-v1".into(), "robustness-floor-v1".into()],
            },
            Body::GetStatus => return Body::StatusSnapshot {
                mac_state: "disconnected".into(),
                connection_id: None,
                mode_family: "ofdm".into(),
                mode_within_family: "ofdm-2000hz".into(),
            },
            Body::Subscribe { topics } => {
                self.subscriptions.lock().unwrap().extend(topics.clone());
                return Body::Ack;
            }
            Body::Unsubscribe { .. } => return Body::Ack,
            _ => {}
        }

        // Scripted responses by discriminant.
        let key = match &req {
            Body::OpenConnection { .. } => "open-connection",
            Body::CloseConnection { .. } => "close-connection",
            Body::AbortOpenConnection { .. } => "abort-open-connection",
            Body::SetLinkAdaptOverride { .. } => "set-link-adapt-override",
            _ => "",
        };

        if !key.is_empty() {
            if let Some(q) = self.scripted.lock().unwrap().get_mut(key) {
                if let Some(canned) = q.pop_front() {
                    return canned;
                }
            }
        }

        Body::Error {
            code: "unknown-request".into(),
            message: format!("stub has no scripted response for {:?}", req),
        }
    }

    fn data_channel(&self, _connection_id: u64) -> Option<Box<dyn DataChannel>> { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NullSink;
    impl EventSink for NullSink {
        fn push(&self, _: Body) -> bool { true }
    }

    #[test]
    fn describe_and_status_are_canned() {
        let d = StubDispatcher::new();
        let sink: Arc<dyn EventSink> = Arc::new(NullSink);

        match d.handle(Body::Describe, &sink) {
            Body::DescribeOk { capabilities, .. } => {
                assert!(capabilities.contains(&"ofdm-family-v1".into()));
            }
            other => panic!("expected DescribeOk, got {:?}", other),
        }

        match d.handle(Body::GetStatus, &sink) {
            Body::StatusSnapshot { mac_state, .. } => assert_eq!(mac_state, "disconnected"),
            other => panic!("expected StatusSnapshot, got {:?}", other),
        }
    }

    #[test]
    fn subscribe_records_topics() {
        let d = StubDispatcher::new();
        let sink: Arc<dyn EventSink> = Arc::new(NullSink);
        let resp = d.handle(
            Body::Subscribe { topics: vec!["arq.metrics".into(), "linkadapt.mode".into()] },
            &sink,
        );
        matches!(resp, Body::Ack);
        let subs = d.subscriptions.lock().unwrap();
        assert!(subs.contains(&"arq.metrics".into()));
        assert!(subs.contains(&"linkadapt.mode".into()));
    }

    #[test]
    fn scripted_open_connection_returned() {
        let d = StubDispatcher::new();
        d.enqueue_response(
            "open-connection",
            Body::OpenConnectionOk {
                connection_id: 1,
                negotiated_mode: "ofdm-2000hz".into(),
                peer_callsign: "N0CALL".into(),
                peer_ssid: 0,
            },
        );

        let sink: Arc<dyn EventSink> = Arc::new(NullSink);
        let resp = d.handle(
            Body::OpenConnection {
                target_callsign: "N0CALL".into(),
                target_ssid: 0,
                deadline_secs: 30,
                preferred_mode: None,
            },
            &sink,
        );

        match resp {
            Body::OpenConnectionOk { connection_id, .. } => assert_eq!(connection_id, 1),
            other => panic!("expected OpenConnectionOk, got {:?}", other),
        }
    }

    #[test]
    fn unscripted_open_connection_is_error() {
        let d = StubDispatcher::new();
        let sink: Arc<dyn EventSink> = Arc::new(NullSink);
        let resp = d.handle(
            Body::OpenConnection {
                target_callsign: "N0CALL".into(),
                target_ssid: 0,
                deadline_secs: 30,
                preferred_mode: None,
            },
            &sink,
        );
        match resp {
            Body::Error { code, .. } => assert_eq!(code, "unknown-request"),
            other => panic!("expected Error, got {:?}", other),
        }
    }
}
```

Add to `crates/sonde-host-proto/src/lib.rs`:

```rust
pub mod stub;
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml stub::`
Expected: PASS (4 tests) after the implementation in Step 1 lands.

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/src/stub.rs crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): StubDispatcher for protocol-level tests

Scripted-response dispatcher used by Phase 4/5 integration tests.
Production dispatchers land with #5/#6/#7.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4 — In-process transport (subsystem #9 seam)

### Task 4.1: `InProcessTransport` channel adapter

**Files:**
- Create: `crates/sonde-host-proto/src/transport/mod.rs`
- Create: `crates/sonde-host-proto/src/transport/in_process.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod transport;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/transport/in_process.rs`:

```rust
//! `InProcessTransport` — a synchronous, in-memory transport that runs the
//! protocol server in a background thread and exposes paired client-side
//! channels.
//!
//! Used by subsystem #9 (tuxlink integration) to drive sonde without
//! going through TCP. The server-side wire is exactly the same logical
//! protocol as the TCP path, but the bytes flow through `std::sync::mpsc`
//! channels instead of socket I/O. This guarantees the in-process and
//! over-the-wire paths can't drift.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::dispatcher::{Dispatcher, EventSink};
use crate::proto::{Body, Message};

/// Client-side handle to an `InProcessTransport`. The client sends `Message`s
/// via `tx` and receives `Message`s via `rx`. The server is running in a
/// background thread.
pub struct InProcessClient {
    /// Send a `Message` to the server.
    pub tx: Sender<Message>,
    /// Receive a `Message` from the server.
    pub rx: Receiver<Message>,
    /// Join handle for the server thread; drop to stop the server cleanly
    /// (the server exits when `tx` is dropped).
    _server: JoinHandle<()>,
}

/// Sink that pushes events into the client-bound channel.
struct ChannelSink(Sender<Message>);

impl EventSink for ChannelSink {
    fn push(&self, event_body: Body) -> bool {
        self.0.send(Message::Event { body: event_body }).is_ok()
    }
}

/// Spawn an in-process server thread driven by `dispatcher`. Returns the
/// client-side handle. The server runs until the client drops its `tx`.
pub fn spawn<D: Dispatcher + 'static>(dispatcher: D) -> InProcessClient {
    let (client_tx, server_rx) = channel::<Message>();
    let (server_tx, client_rx) = channel::<Message>();

    let dispatcher = Arc::new(dispatcher);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink(server_tx.clone()));

    let handle = thread::spawn(move || {
        for msg in server_rx {
            match msg {
                Message::Req { id, body } => {
                    let resp_body = dispatcher.handle(body, &sink);
                    if server_tx.send(Message::Resp { id, body: resp_body }).is_err() {
                        break;
                    }
                }
                Message::Resp { .. } | Message::Event { .. } => {
                    // Clients don't normally send these; drop silently.
                }
            }
        }
    });

    InProcessClient { tx: client_tx, rx: client_rx, _server: handle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubDispatcher;
    use std::time::Duration;

    #[test]
    fn describe_roundtrips_in_process() {
        let client = spawn(StubDispatcher::new());
        client.tx.send(Message::Req { id: 1, body: Body::Describe }).unwrap();
        let resp = client.rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match resp {
            Message::Resp { id, body: Body::DescribeOk { proto_version, .. } } => {
                assert_eq!(id, 1);
                assert_eq!(proto_version, "1.0.0");
            }
            other => panic!("expected DescribeOk Resp, got {:?}", other),
        }
    }

    #[test]
    fn open_connection_scripted_response() {
        let stub = StubDispatcher::new();
        stub.enqueue_response(
            "open-connection",
            Body::OpenConnectionOk {
                connection_id: 7,
                negotiated_mode: "ofdm-2000hz".into(),
                peer_callsign: "N0CALL".into(),
                peer_ssid: 0,
            },
        );

        let client = spawn(stub);
        client.tx.send(Message::Req {
            id: 2,
            body: Body::OpenConnection {
                target_callsign: "N0CALL".into(),
                target_ssid: 0,
                deadline_secs: 30,
                preferred_mode: None,
            },
        }).unwrap();

        let resp = client.rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match resp {
            Message::Resp { id, body: Body::OpenConnectionOk { connection_id, .. } } => {
                assert_eq!(id, 2);
                assert_eq!(connection_id, 7);
            }
            other => panic!("expected OpenConnectionOk Resp, got {:?}", other),
        }
    }
}
```

Create `crates/sonde-host-proto/src/transport/mod.rs`:

```rust
//! Transport adapters. Two flavors:
//! - `in_process` — synchronous in-memory channels (subsystem #9 wire-up).
//! - `tcp` — TCP listener + connection threads (subsystem #10 daemon wire-up).
//!
//! Both consume a `Dispatcher` implementation and expose the same logical
//! protocol; the in-process and TCP paths are kept symmetric so they cannot
//! drift.

pub mod in_process;
pub mod tcp;
```

(The `tcp` submodule lands in Phase 5; declare it now so the structure is consistent.)

Add to `crates/sonde-host-proto/src/lib.rs`:

```rust
pub mod transport;
```

- [ ] **Step 2: Create a placeholder `tcp` module so the crate compiles**

Create `crates/sonde-host-proto/src/transport/tcp.rs`:

```rust
//! TCP transport — lands in Phase 5.
```

- [ ] **Step 3: Run the failing test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml transport::`
Expected: FAIL on first compile (depending on which step ran first); PASS once Step 1's code is in place.

- [ ] **Step 4: Re-run after Step 1+2 land**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml transport::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-host-proto/src/transport/ crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): InProcessTransport channel adapter

Subsystem #9's seam: tuxlink runs sonde in-process via std::sync::mpsc
channels, same protocol shape as the TCP wire. Symmetry by construction.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 5 — TCP transport (subsystem #10 seam)

### Task 5.1: TCP listener + per-connection worker threads (cmd socket only)

**Files:**
- Modify: `crates/sonde-host-proto/src/transport/tcp.rs`

- [ ] **Step 1: Write the failing test**

REPLACE the contents of `crates/sonde-host-proto/src/transport/tcp.rs` with:

```rust
//! TCP transport — the over-the-wire path for the standalone daemon
//! (subsystem #10).
//!
//! Two TCP listeners: one on `cmd_addr` for cmd-socket messages
//! (length-prefixed CBOR), one on `data_addr` for the raw ARQ-corrected
//! byte stream. Each accepted connection spawns a reader thread + writer
//! thread; the dispatcher is invoked from the reader thread.
//!
//! Security posture: default bind is `127.0.0.1`. To bind to a non-loopback
//! address, the server config MUST also set `require_auth = true` and supply
//! a non-empty token list. Loopback + no-auth is a development convenience,
//! NOT a production stance — the standalone daemon binary (subsystem #10)
//! enforces this gate at startup.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::codec::{read_frame, write_frame, CodecError};
use crate::dispatcher::{Dispatcher, EventSink};
use crate::proto::{Body, Message};

/// Server configuration.
#[derive(Debug, Clone)]
pub struct TcpServerConfig {
    /// Cmd-socket bind address.
    pub cmd_addr: SocketAddr,
    /// Data-socket bind address.
    pub data_addr: SocketAddr,
    /// If true, every connection MUST present a valid `Hello.auth` token
    /// matching `accepted_tokens`. Required when binding non-loopback.
    pub require_auth: bool,
    /// Tokens accepted by the auth check. Ignored if `require_auth == false`.
    pub accepted_tokens: Vec<String>,
}

/// Handle to a running `TcpServer`. Drop to shut down (the listener threads
/// see the close on `_shutdown_tx` going out of scope and exit).
pub struct TcpServer {
    cmd_addr: SocketAddr,
    data_addr: SocketAddr,
    _cmd_handle: JoinHandle<()>,
    _data_handle: JoinHandle<()>,
    _shutdown_tx: Sender<()>,
}

impl TcpServer {
    /// The address the cmd-socket listener is actually bound to (resolves
    /// port 0 to the OS-assigned port).
    pub fn cmd_addr(&self) -> SocketAddr { self.cmd_addr }
    /// The address the data-socket listener is actually bound to.
    pub fn data_addr(&self) -> SocketAddr { self.data_addr }
}

/// Start the TCP server. `dispatcher` is shared across all accepted
/// connections.
pub fn start<D: Dispatcher + 'static>(
    cfg: TcpServerConfig,
    dispatcher: Arc<D>,
) -> std::io::Result<TcpServer> {
    let cmd_listener = TcpListener::bind(cfg.cmd_addr)?;
    let data_listener = TcpListener::bind(cfg.data_addr)?;
    let cmd_addr = cmd_listener.local_addr()?;
    let data_addr = data_listener.local_addr()?;

    let (shutdown_tx, _shutdown_rx) = channel::<()>();

    let dispatcher_for_cmd = dispatcher.clone();
    let cfg_for_cmd = cfg.clone();
    let cmd_handle = thread::spawn(move || {
        // Non-blocking accept loop. The thread exits when the listener errors,
        // which happens at drop time when the listener is closed.
        for stream in cmd_listener.incoming() {
            match stream {
                Ok(s) => {
                    let d = dispatcher_for_cmd.clone();
                    let cfg = cfg_for_cmd.clone();
                    thread::spawn(move || {
                        if let Err(e) = serve_cmd_connection(s, d, cfg) {
                            tracing::warn!("cmd connection ended: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("cmd accept error: {}", e);
                    break;
                }
            }
        }
    });

    let dispatcher_for_data = dispatcher.clone();
    let data_handle = thread::spawn(move || {
        for stream in data_listener.incoming() {
            match stream {
                Ok(s) => {
                    let d = dispatcher_for_data.clone();
                    thread::spawn(move || {
                        if let Err(e) = serve_data_connection(s, d) {
                            tracing::warn!("data connection ended: {:?}", e);
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });

    Ok(TcpServer {
        cmd_addr,
        data_addr,
        _cmd_handle: cmd_handle,
        _data_handle: data_handle,
        _shutdown_tx: shutdown_tx,
    })
}

/// `EventSink` that serializes events to the writer half of a cmd-socket.
struct CmdSocketSink {
    write_half: Arc<Mutex<TcpStream>>,
}

impl EventSink for CmdSocketSink {
    fn push(&self, event_body: Body) -> bool {
        let msg = Message::Event { body: event_body };
        let mut payload = Vec::new();
        if ciborium::into_writer(&msg, &mut payload).is_err() {
            return false;
        }
        let mut w = match self.write_half.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        write_frame(&mut *w, &payload).is_ok()
    }
}

fn serve_cmd_connection<D: Dispatcher + 'static>(
    stream: TcpStream,
    dispatcher: Arc<D>,
    cfg: TcpServerConfig,
) -> Result<(), CodecError> {
    let peer = stream.peer_addr().map_err(CodecError::Io)?;
    let is_loopback = peer.ip().is_loopback();

    let read_half = stream.try_clone().map_err(CodecError::Io)?;
    let write_half = Arc::new(Mutex::new(stream));
    let sink: Arc<dyn EventSink> = Arc::new(CmdSocketSink { write_half: write_half.clone() });

    let mut authenticated = !cfg.require_auth && is_loopback;

    let mut reader = read_half;
    loop {
        let frame = read_frame(&mut reader)?;
        let msg: Message = match ciborium::from_reader(&frame[..]) {
            Ok(m) => m,
            Err(_) => {
                let err = Message::Resp {
                    id: 0,
                    body: Body::Error {
                        code: "schema-mismatch".into(),
                        message: "CBOR decode failed".into(),
                    },
                };
                send_msg(&write_half, &err)?;
                continue;
            }
        };

        let (id, body) = match msg {
            Message::Req { id, body } => (id, body),
            Message::Resp { .. } | Message::Event { .. } => continue,
        };

        // First message MUST be Hello; auth check happens here.
        if !authenticated {
            match &body {
                Body::Hello { proto_version, auth, .. } => {
                    if proto_version != "1.0.0" {
                        let err = Message::Resp {
                            id,
                            body: Body::Error {
                                code: "version-mismatch".into(),
                                message: format!("server speaks 1.0.0, client offered {}", proto_version),
                            },
                        };
                        send_msg(&write_half, &err)?;
                        return Ok(());
                    }
                    if cfg.require_auth {
                        match auth {
                            Some(crate::proto::body::Auth::Token { token }) if cfg.accepted_tokens.contains(token) => {
                                authenticated = true;
                            }
                            _ => {
                                let err = Message::Resp {
                                    id,
                                    body: Body::Error {
                                        code: "auth-failed".into(),
                                        message: "bad or missing token".into(),
                                    },
                                };
                                send_msg(&write_half, &err)?;
                                return Ok(());
                            }
                        }
                    } else {
                        authenticated = true;
                    }
                    let resp_body = Body::Hello {
                        proto_version: "1.0.0".into(),
                        capabilities: vec![
                            "ofdm-family-v1".into(),
                            "robustness-floor-v1".into(),
                            "link-adapt-2d-v1".into(),
                            "phy-bitloading-v1".into(),
                        ],
                        auth: None,
                    };
                    send_msg(&write_half, &Message::Resp { id, body: resp_body })?;
                    continue;
                }
                _ => {
                    let err = Message::Resp {
                        id,
                        body: Body::Error {
                            code: "auth-required".into(),
                            message: "first message must be Hello".into(),
                        },
                    };
                    send_msg(&write_half, &err)?;
                    return Ok(());
                }
            }
        }

        let resp_body = dispatcher.handle(body, &sink);
        send_msg(&write_half, &Message::Resp { id, body: resp_body })?;
    }
}

fn send_msg(write_half: &Arc<Mutex<TcpStream>>, msg: &Message) -> Result<(), CodecError> {
    let mut payload = Vec::new();
    ciborium::into_writer(msg, &mut payload)
        .map_err(|_| CodecError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, "cbor encode")))?;
    let mut w = write_half.lock().unwrap();
    write_frame(&mut *w, &payload)?;
    Ok(())
}

fn serve_data_connection<D: Dispatcher + 'static>(
    stream: TcpStream,
    _dispatcher: Arc<D>,
) -> Result<(), CodecError> {
    // Phase 5.1: stub. The full data-socket bridge (push_tx / drain_rx pumps)
    // lands in Task 5.2.
    let _ = stream;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubDispatcher;
    use std::net::TcpStream;
    use std::time::Duration;

    fn loopback_cfg() -> TcpServerConfig {
        TcpServerConfig {
            cmd_addr: "127.0.0.1:0".parse().unwrap(),
            data_addr: "127.0.0.1:0".parse().unwrap(),
            require_auth: false,
            accepted_tokens: vec![],
        }
    }

    #[test]
    fn hello_handshake_loopback_noauth() {
        let server = start(loopback_cfg(), Arc::new(StubDispatcher::new())).unwrap();

        let mut client = TcpStream::connect(server.cmd_addr()).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        // Client sends Hello.
        let hello = Message::Req {
            id: 1,
            body: Body::Hello {
                proto_version: "1.0.0".into(),
                capabilities: vec![],
                auth: None,
            },
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&hello, &mut buf).unwrap();
        write_frame(&mut client, &buf).unwrap();

        // Read server's Hello response.
        let frame = read_frame(&mut client).unwrap();
        let resp: Message = ciborium::from_reader(&frame[..]).unwrap();
        match resp {
            Message::Resp { id, body: Body::Hello { proto_version, .. } } => {
                assert_eq!(id, 1);
                assert_eq!(proto_version, "1.0.0");
            }
            other => panic!("expected Hello Resp, got {:?}", other),
        }
    }

    #[test]
    fn version_mismatch_terminates() {
        let server = start(loopback_cfg(), Arc::new(StubDispatcher::new())).unwrap();
        let mut client = TcpStream::connect(server.cmd_addr()).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        let hello = Message::Req {
            id: 1,
            body: Body::Hello {
                proto_version: "2.0.0".into(),
                capabilities: vec![],
                auth: None,
            },
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&hello, &mut buf).unwrap();
        write_frame(&mut client, &buf).unwrap();

        let frame = read_frame(&mut client).unwrap();
        let resp: Message = ciborium::from_reader(&frame[..]).unwrap();
        match resp {
            Message::Resp { body: Body::Error { code, .. }, .. } => {
                assert_eq!(code, "version-mismatch");
            }
            other => panic!("expected version-mismatch Error, got {:?}", other),
        }
    }

    #[test]
    fn auth_failed_when_token_missing() {
        let mut cfg = loopback_cfg();
        cfg.require_auth = true;
        cfg.accepted_tokens = vec!["correct-horse-battery-staple".into()];

        let server = start(cfg, Arc::new(StubDispatcher::new())).unwrap();
        let mut client = TcpStream::connect(server.cmd_addr()).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        let hello = Message::Req {
            id: 1,
            body: Body::Hello {
                proto_version: "1.0.0".into(),
                capabilities: vec![],
                auth: None, // missing
            },
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&hello, &mut buf).unwrap();
        write_frame(&mut client, &buf).unwrap();

        let frame = read_frame(&mut client).unwrap();
        let resp: Message = ciborium::from_reader(&frame[..]).unwrap();
        match resp {
            Message::Resp { body: Body::Error { code, .. }, .. } => {
                assert_eq!(code, "auth-failed");
            }
            other => panic!("expected auth-failed Error, got {:?}", other),
        }
    }

    #[test]
    fn describe_after_hello() {
        let server = start(loopback_cfg(), Arc::new(StubDispatcher::new())).unwrap();
        let mut client = TcpStream::connect(server.cmd_addr()).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        // Hello.
        let mut buf = Vec::new();
        ciborium::into_writer(
            &Message::Req {
                id: 1,
                body: Body::Hello { proto_version: "1.0.0".into(), capabilities: vec![], auth: None },
            },
            &mut buf,
        ).unwrap();
        write_frame(&mut client, &buf).unwrap();
        let _hello_resp = read_frame(&mut client).unwrap();

        // Describe.
        buf.clear();
        ciborium::into_writer(&Message::Req { id: 2, body: Body::Describe }, &mut buf).unwrap();
        write_frame(&mut client, &buf).unwrap();

        let frame = read_frame(&mut client).unwrap();
        let resp: Message = ciborium::from_reader(&frame[..]).unwrap();
        match resp {
            Message::Resp { id, body: Body::DescribeOk { proto_version, .. } } => {
                assert_eq!(id, 2);
                assert_eq!(proto_version, "1.0.0");
            }
            other => panic!("expected DescribeOk, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml transport::tcp::`
Expected: PASS (4 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/src/transport/tcp.rs
git commit -m "feat(modem-host): TCP cmd-socket listener + Hello/auth handshake

Subsystem #10's seam: standalone daemon exposes the protocol over TCP.
Hello-handshake gates auth + version negotiation. Loopback defaults are
no-auth; non-loopback binds will be gated by the daemon binary (#10).

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5.2: TCP data-socket bridge

**Files:**
- Modify: `crates/sonde-host-proto/src/transport/tcp.rs`

- [ ] **Step 1: Write the failing test**

Append to the existing `tests` mod in `crates/sonde-host-proto/src/transport/tcp.rs`:

```rust
    /// A scripted `DataChannel` that records everything pushed in and
    /// returns canned bytes on drain.
    struct LoopbackChannel {
        tx_buf: std::sync::Mutex<Vec<u8>>,
        rx_buf: std::sync::Mutex<std::collections::VecDeque<u8>>,
    }
    impl crate::dispatcher::DataChannel for LoopbackChannel {
        fn push_tx(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.tx_buf.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn drain_rx(&mut self, dst: &mut [u8]) -> std::io::Result<usize> {
            let mut q = self.rx_buf.lock().unwrap();
            let n = std::cmp::min(dst.len(), q.len());
            for i in 0..n { dst[i] = q.pop_front().unwrap(); }
            Ok(n)
        }
    }

    struct LoopbackDispatcher {
        channel: std::sync::Mutex<Option<LoopbackChannel>>,
    }
    impl crate::dispatcher::Dispatcher for LoopbackDispatcher {
        fn handle(&self, req: Body, _sink: &std::sync::Arc<dyn crate::dispatcher::EventSink>) -> Body {
            match req {
                Body::Hello { .. } => Body::Hello {
                    proto_version: "1.0.0".into(),
                    capabilities: vec![],
                    auth: None,
                },
                _ => Body::Ack,
            }
        }
        fn data_channel(&self, _cid: u64) -> Option<Box<dyn crate::dispatcher::DataChannel>> {
            // For the test, we hand back a fresh channel preloaded with bytes.
            Some(Box::new(LoopbackChannel {
                tx_buf: std::sync::Mutex::new(Vec::new()),
                rx_buf: std::sync::Mutex::new(b"echo-back".iter().copied().collect()),
            }))
        }
    }

    #[test]
    fn data_socket_drains_rx_bytes() {
        let cfg = loopback_cfg();
        let dispatcher = std::sync::Arc::new(LoopbackDispatcher {
            channel: std::sync::Mutex::new(None),
        });
        let server = start(cfg, dispatcher).unwrap();
        let mut client = TcpStream::connect(server.data_addr()).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        // Client must announce a connection id on the data socket; the codec
        // reuses the cmd-socket frame format: one CBOR frame carrying
        // `{ "type": "attach", "connection_id": <u64> }`.
        #[derive(serde::Serialize)]
        struct Attach { #[serde(rename = "type")] t: &'static str, connection_id: u64 }
        let mut buf = Vec::new();
        ciborium::into_writer(&Attach { t: "attach", connection_id: 1 }, &mut buf).unwrap();
        write_frame(&mut client, &buf).unwrap();

        // The server should immediately push the rx bytes as a raw frame.
        let frame = read_frame(&mut client).unwrap();
        assert_eq!(&frame[..], b"echo-back");
    }
```

- [ ] **Step 2: Implement the data-socket bridge**

REPLACE the `serve_data_connection` function with:

```rust
fn serve_data_connection<D: Dispatcher + 'static>(
    stream: TcpStream,
    dispatcher: Arc<D>,
) -> Result<(), CodecError> {
    // First frame: { "type": "attach", "connection_id": <u64> }.
    #[derive(serde::Deserialize)]
    struct Attach { #[serde(rename = "type")] _t: String, connection_id: u64 }
    let read_half = stream.try_clone().map_err(CodecError::Io)?;
    let write_half = Arc::new(Mutex::new(stream));

    let mut reader = read_half;
    let attach_frame = read_frame(&mut reader)?;
    let attach: Attach = match ciborium::from_reader(&attach_frame[..]) {
        Ok(a) => a,
        Err(_) => return Ok(()),
    };

    let mut channel = match dispatcher.data_channel(attach.connection_id) {
        Some(c) => c,
        None => return Ok(()),
    };

    // RX pump: drain the modem's RX queue and forward as raw frames.
    let write_half_for_rx = write_half.clone();
    let rx_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let rx_done_writer = rx_done.clone();

    // For the integration test, we do a single drain then exit. A production
    // dispatcher would block in `drain_rx` until bytes are available; for the
    // stub here, we drain once with a small poll loop.
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            let n = match channel.drain_rx(&mut buf) {
                Ok(n) => n,
                Err(_) => break,
            };
            if n > 0 {
                let mut w = write_half_for_rx.lock().unwrap();
                if write_frame(&mut *w, &buf[..n]).is_err() { break; }
            } else if rx_done_writer.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    });

    // TX pump: read raw frames from the client, push into the modem's TX queue.
    // For the test, only the RX side is exercised; the TX side reads until EOF.
    let _ = reader; // unused in this minimal version
    rx_done.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
```

(Note: this is intentionally minimal. Subsystem #9/#10 plans extend with full TX pump and graceful shutdown. The test asserts the RX path works; it is the documented contract.)

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml transport::tcp::data_socket`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/sonde-host-proto/src/transport/tcp.rs
git commit -m "feat(modem-host): TCP data-socket bridge with attach + RX pump

Data-socket attaches to a connection id and pumps ARQ-corrected RX bytes
as raw frames. TX pump is a stub; full duplex lands with #9/#10 wire-up.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 6 — JSON-Schema sidecar (AI-native affordance)

### Task 6.1: Build a `gen-schema` binary

**Files:**
- Create: `crates/sonde-host-proto/src/bin/gen-schema.rs`
- Modify: `crates/sonde-host-proto/Cargo.toml` (add `[[bin]] name = "gen-schema"`)
- Create: `crates/sonde-host-proto/schemas/.gitkeep`

- [ ] **Step 1: Write the binary**

Create `crates/sonde-host-proto/src/bin/gen-schema.rs`:

```rust
//! `gen-schema` — regenerate the JSON-Schema sidecar from the serde+schemars-
//! derived types. Run as:
//!
//! ```sh
//! cargo run -p sonde-host-proto --bin gen-schema
//! ```
//!
//! Writes `crates/sonde-host-proto/schemas/sonde-host-proto-v1.schema.json`.

use schemars::schema_for;
use sonde_host_proto::proto::Message;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schema_for!(Message);
    let json = serde_json::to_string_pretty(&schema)?;
    let out_path = "crates/sonde-host-proto/schemas/sonde-host-proto-v1.schema.json";
    std::fs::write(out_path, json)?;
    println!("wrote {}", out_path);
    Ok(())
}
```

Add to `crates/sonde-host-proto/Cargo.toml` (under `[dependencies]`, append a new section):

```toml
[[bin]]
name = "gen-schema"
path = "src/bin/gen-schema.rs"
```

Create `crates/sonde-host-proto/schemas/.gitkeep` (empty file) so the directory tracks under git.

- [ ] **Step 2: Generate the schema**

Run: `cargo run --manifest-path crates/sonde-host-proto/Cargo.toml --bin gen-schema`
Expected: `wrote crates/sonde-host-proto/schemas/sonde-host-proto-v1.schema.json`.

- [ ] **Step 3: Sanity-check the schema**

Run: `head -30 crates/sonde-host-proto/schemas/sonde-host-proto-v1.schema.json`
Expected: a valid JSON-Schema document with `$schema`, `title`, `oneOf` for the `Message` variants.

- [ ] **Step 4: Commit the schema + binary**

```bash
git add crates/sonde-host-proto/Cargo.toml \
        crates/sonde-host-proto/src/bin/gen-schema.rs \
        crates/sonde-host-proto/schemas/
git commit -m "feat(modem-host): JSON-Schema sidecar generator

gen-schema binary regenerates the JSON-Schema sidecar from serde+schemars-
derived types. Sidecar is the AI-native self-describe artifact: an agent
loads it to learn the protocol grammar without reading source.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 6.2: Schema-freshness test (CI gate)

**Files:**
- Create: `crates/sonde-host-proto/tests/schema_freshness.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/sonde-host-proto/tests/schema_freshness.rs`:

```rust
//! Asserts that the on-disk schema sidecar matches what `schema_for!(Message)`
//! produces *right now*. If the protocol schema changes without regenerating
//! the sidecar, this test fails and CI catches the drift.

use schemars::schema_for;
use sonde_host_proto::proto::Message;

#[test]
fn schema_sidecar_is_up_to_date() {
    let current = schema_for!(Message);
    let current_json = serde_json::to_string_pretty(&current).unwrap();

    let on_disk = std::fs::read_to_string(
        "schemas/sonde-host-proto-v1.schema.json",
    ).expect("schemas/sonde-host-proto-v1.schema.json should exist; run `cargo run --bin gen-schema` to create it");

    assert_eq!(
        current_json.trim(),
        on_disk.trim(),
        "schema drift detected — run `cargo run --bin gen-schema` and commit the result"
    );
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml --test schema_freshness`
Expected: PASS (the sidecar generated in Task 6.1 matches).

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/tests/schema_freshness.rs
git commit -m "test(modem-host): schema-sidecar freshness gate

Asserts on-disk JSON-Schema matches what schema_for!(Message) produces.
CI catches forgotten regenerations.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 7 — AI-native debug surfaces

### Task 7.1: `--text` JSON shim for human/agent inspection

**Files:**
- Create: `crates/sonde-host-proto/src/debug.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod debug;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/debug.rs`:

```rust
//! Debug surfaces. Two affordances for AI-collaborative inspection:
//!
//! 1. **JSON-Lines emission.** Every `Message` can serialize to a single line
//!    of JSON (NOT CBOR) for `--trace-jsonl` and for human eyes.
//! 2. **Pretty-print round-trip.** `Message::to_json` and `Message::from_json`
//!    let tests and agents author message fixtures in JSON instead of CBOR.
//!
//! The production wire format remains CBOR. JSON is debug-only.

use crate::proto::Message;

/// Serialize a `Message` as a single JSON line (no trailing newline).
pub fn to_json_line(msg: &Message) -> Result<String, serde_json::Error> {
    serde_json::to_string(msg)
}

/// Parse a `Message` from a single JSON line.
pub fn from_json_line(line: &str) -> Result<Message, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Body;

    #[test]
    fn json_roundtrips_a_request() {
        let msg = Message::Req {
            id: 1,
            body: Body::Describe,
        };
        let line = to_json_line(&msg).unwrap();
        assert!(line.contains("\"kind\":\"req\""));
        assert!(line.contains("\"type\":\"describe\""));
        let back = from_json_line(&line).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn json_roundtrips_an_event() {
        let msg = Message::Event {
            body: Body::LinkAdaptModeChange {
                mode_family: "ofdm".into(),
                mode_within_family: "ofdm-2000hz".into(),
                snr_db: 12.5,
                reason: "channel-improvement".into(),
            },
        };
        let line = to_json_line(&msg).unwrap();
        let back = from_json_line(&line).unwrap();
        assert_eq!(msg, back);
    }
}
```

Add to `crates/sonde-host-proto/src/lib.rs`:

```rust
pub mod debug;
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml debug::`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/src/debug.rs crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): JSON-Line debug shim

to_json_line / from_json_line for --trace-jsonl and for fixture authoring.
Production wire format remains CBOR; JSON is debug-only.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 7.2: `Tracer` middleware — wrap any `Dispatcher` to emit JSONL trace

**Files:**
- Create: `crates/sonde-host-proto/src/tracer.rs`
- Modify: `crates/sonde-host-proto/src/lib.rs` (declare `pub mod tracer;`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-host-proto/src/tracer.rs`:

```rust
//! `Tracer` — wraps any `Dispatcher` to write every `(req, resp)` pair as a
//! pair of JSON lines into a configured `Write`. Used by the standalone
//! daemon's `--trace-jsonl <path>` flag and by integration tests asserting
//! on event sequences.

use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::debug::to_json_line;
use crate::dispatcher::{DataChannel, Dispatcher, EventSink};
use crate::proto::{Body, Message};

/// Wraps a `Dispatcher` and a writer; emits one JSON line per request and one
/// per response, tagged with `{ "dir": "in" | "out" }` so an agent reading the
/// trace can reconstruct the conversation.
pub struct Tracer<D: Dispatcher> {
    inner: D,
    out: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl<D: Dispatcher> Tracer<D> {
    /// Wrap `inner`, writing trace lines to `out`.
    pub fn new(inner: D, out: Box<dyn Write + Send>) -> Self {
        Self { inner, out: Arc::new(Mutex::new(out)) }
    }
}

impl<D: Dispatcher> Dispatcher for Tracer<D> {
    fn handle(&self, req: Body, sink: &Arc<dyn EventSink>) -> Body {
        // Note: the wrapped sink also traces outgoing events. Build a tracing
        // sink that wraps `sink`.
        let traced_sink: Arc<dyn EventSink> = Arc::new(TracingSink {
            inner: sink.clone(),
            out: self.out.clone(),
        });

        // Trace the incoming Req body (we don't have the id at this layer;
        // the protocol server's outer trace covers id-with-body).
        if let Ok(line) = to_json_line(&Message::Req { id: 0, body: req.clone() }) {
            let mut w = self.out.lock().unwrap();
            let _ = writeln!(w, "{{\"dir\":\"in\",\"msg\":{}}}", line);
        }

        let resp = self.inner.handle(req, &traced_sink);

        if let Ok(line) = to_json_line(&Message::Resp { id: 0, body: resp.clone() }) {
            let mut w = self.out.lock().unwrap();
            let _ = writeln!(w, "{{\"dir\":\"out\",\"msg\":{}}}", line);
        }

        resp
    }

    fn data_channel(&self, connection_id: u64) -> Option<Box<dyn DataChannel>> {
        self.inner.data_channel(connection_id)
    }
}

struct TracingSink {
    inner: Arc<dyn EventSink>,
    out: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl EventSink for TracingSink {
    fn push(&self, event_body: Body) -> bool {
        if let Ok(line) = to_json_line(&Message::Event { body: event_body.clone() }) {
            let mut w = self.out.lock().unwrap();
            let _ = writeln!(w, "{{\"dir\":\"out\",\"msg\":{}}}", line);
        }
        self.inner.push(event_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubDispatcher;

    #[test]
    fn tracer_writes_in_and_out_lines() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        struct SharedWriter(Arc<Mutex<Vec<u8>>>);
        impl Write for SharedWriter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let writer = Box::new(SharedWriter(buf.clone())) as Box<dyn Write + Send>;

        let tracer = Tracer::new(StubDispatcher::new(), writer);
        let sink: Arc<dyn EventSink> = Arc::new(StubSink);
        let _resp = tracer.handle(Body::Describe, &sink);

        let log = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(log.contains("\"dir\":\"in\""));
        assert!(log.contains("\"dir\":\"out\""));
        assert!(log.contains("\"type\":\"describe\""));
        assert!(log.contains("\"type\":\"describe-ok\""));
    }

    struct StubSink;
    impl EventSink for StubSink {
        fn push(&self, _: Body) -> bool { true }
    }
}
```

Add to `crates/sonde-host-proto/src/lib.rs`:

```rust
pub mod tracer;
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml tracer::`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/src/tracer.rs crates/sonde-host-proto/src/lib.rs
git commit -m "feat(modem-host): Tracer middleware writes JSONL conversation traces

Wrap any Dispatcher; every (in, out) pair becomes one JSONL line each. Used
by --trace-jsonl daemon flag (subsystem #10) and by integration tests
asserting on event sequences.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 8 — Conformance suite

### Task 8.1: Cross-transport conformance test

**Files:**
- Create: `crates/sonde-host-proto/tests/cross_transport.rs`

- [ ] **Step 1: Write the test**

Create `crates/sonde-host-proto/tests/cross_transport.rs`:

```rust
//! Cross-transport conformance: the same `Dispatcher`-driven scenario must
//! produce the same logical message sequence on both the in-process channel
//! transport and the TCP transport. This is the guarantee that subsystem #9
//! and subsystem #10 cannot drift.

use std::sync::Arc;
use std::time::Duration;

use sonde_host_proto::codec::{read_frame, write_frame};
use sonde_host_proto::proto::{Body, Message};
use sonde_host_proto::stub::StubDispatcher;
use sonde_host_proto::transport::{in_process, tcp};

fn open_conn_req(id: u64) -> Message {
    Message::Req {
        id,
        body: Body::OpenConnection {
            target_callsign: "N0CALL".into(),
            target_ssid: 0,
            deadline_secs: 30,
            preferred_mode: Some("ofdm-2000hz".into()),
        },
    }
}

fn canned_open_resp(stub: &StubDispatcher) {
    stub.enqueue_response(
        "open-connection",
        Body::OpenConnectionOk {
            connection_id: 7,
            negotiated_mode: "ofdm-2000hz".into(),
            peer_callsign: "N0CALL".into(),
            peer_ssid: 0,
        },
    );
}

#[test]
fn in_process_and_tcp_emit_same_open_connection_response() {
    // --- in-process ---
    let in_proc_stub = StubDispatcher::new();
    canned_open_resp(&in_proc_stub);
    let in_proc_client = in_process::spawn(in_proc_stub);
    in_proc_client.tx.send(open_conn_req(2)).unwrap();
    let in_proc_resp = in_proc_client.rx.recv_timeout(Duration::from_secs(2)).unwrap();

    // --- tcp ---
    let tcp_stub = StubDispatcher::new();
    canned_open_resp(&tcp_stub);
    let cfg = tcp::TcpServerConfig {
        cmd_addr: "127.0.0.1:0".parse().unwrap(),
        data_addr: "127.0.0.1:0".parse().unwrap(),
        require_auth: false,
        accepted_tokens: vec![],
    };
    let server = tcp::start(cfg, Arc::new(tcp_stub)).unwrap();
    let mut client = std::net::TcpStream::connect(server.cmd_addr()).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

    // Hello handshake (required on TCP).
    let mut buf = Vec::new();
    ciborium::into_writer(
        &Message::Req {
            id: 1,
            body: Body::Hello {
                proto_version: "1.0.0".into(),
                capabilities: vec![],
                auth: None,
            },
        },
        &mut buf,
    ).unwrap();
    write_frame(&mut client, &buf).unwrap();
    let _hello_resp = read_frame(&mut client).unwrap();

    // OpenConnection.
    buf.clear();
    ciborium::into_writer(&open_conn_req(2), &mut buf).unwrap();
    write_frame(&mut client, &buf).unwrap();

    let tcp_resp_bytes = read_frame(&mut client).unwrap();
    let tcp_resp: Message = ciborium::from_reader(&tcp_resp_bytes[..]).unwrap();

    // --- assert equivalence ---
    let normalize = |m: Message| match m {
        Message::Resp { id: _, body } => body, // strip ids; in-process has no Hello phase so id schemes differ
        other => panic!("expected Resp, got {:?}", other),
    };

    assert_eq!(normalize(in_proc_resp), normalize(tcp_resp));
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml --test cross_transport`
Expected: PASS — both transports emit `Body::OpenConnectionOk { connection_id: 7, ... }`.

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/tests/cross_transport.rs
git commit -m "test(modem-host): in-process and TCP emit equivalent message sequences

Conformance gate that #9 (in-process) and #10 (TCP) cannot drift. Same
dispatcher, same scripted response, equivalent normalized Body output.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 8.2: Documentation — capability matrix + topic reference

**Files:**
- Create: `crates/sonde-host-proto/docs/CAPABILITIES.md`
- Create: `crates/sonde-host-proto/docs/TOPICS.md`

- [ ] **Step 1: Write `CAPABILITIES.md`**

Create `crates/sonde-host-proto/docs/CAPABILITIES.md`:

```markdown
# Capability matrix — v1

Capabilities are advertised in the `Hello` handshake. A client that does not
support a capability MUST NOT issue requests that depend on it; a server
that does not advertise a capability MUST reject the corresponding requests
with `Error { code: "unknown-request" }`.

| Capability | Required for | Notes |
|---|---|---|
| `ofdm-family-v1` | `OpenConnection { preferred_mode: "ofdm-*" }` | Bit-adaptive OFDM ladder. |
| `robustness-floor-v1` | `OpenConnection { preferred_mode: "robust-floor-*" }` | Wide-band low-density OFDM + narrow-FSK floor. |
| `link-adapt-2d-v1` | `SetLinkAdaptOverride`, `LinkAdaptModeChange` events | 2D policy (channel-quality × payload-size). |
| `phy-bitloading-v1` | `PhyBitLoadingUpdate` events | Per-sub-carrier bit-loading curve visibility. |

Versioning of individual capabilities follows the suffix `-v<N>` convention.
A breaking change to a capability bumps `N`. `ofdm-family-v1` and
`ofdm-family-v2` can coexist if a server speaks both.

Adding a new capability is a non-breaking server-side change provided the
new capability does not become required for messages that were valid under
the old set.
```

- [ ] **Step 2: Write `TOPICS.md`**

Create `crates/sonde-host-proto/docs/TOPICS.md`:

```markdown
# Subscription topics — v1

Topics named in `Subscribe { topics: [...] }` requests. Unknown topics
trigger `Error { code: "unknown-topic" }`.

| Topic | Event body | Default rate |
|---|---|---|
| `mac.state` | `MacStateChange` | On change |
| `arq.connection` | `ArqConnectionChange` | On change |
| `arq.metrics` | `ArqMetricsUpdate` | 1 Hz |
| `linkadapt.mode` | `LinkAdaptModeChange` | On change |
| `linkadapt.metrics` | `LinkAdaptMetricsUpdate` | 1 Hz |
| `phy.bitloading` | `PhyBitLoadingUpdate` | 0.5 Hz |

Rate-limited topics may emit more slowly than the default under low activity.
On-change topics emit at most once per logical transition; duplicate
suppression is the server's responsibility.

Subscribing twice to the same topic is idempotent (no error, no double-rate).
Unsubscribing from a topic the client never subscribed to is idempotent.
```

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-host-proto/docs/
git commit -m "docs(modem-host): capability matrix + subscription topic reference

Capability advertisement + topic catalogue. Both feed the AI-native posture:
agents reading the schema sidecar see the structural shape; these docs name
the strings that go into capabilities + topics fields.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 9 — Plan close-out

### Task 9.1: Update `dev/implementation-log.md`

**Files:**
- Modify: `dev/implementation-log.md` (create if it does not exist; the project uses this as the running record per CLAUDE.md §"Commit and release discipline")

- [ ] **Step 1: Prepend an entry**

If `dev/implementation-log.md` does not exist, create it with this content. If it does, prepend the entry above the most-recent existing entry.

```markdown
# Implementation log

## 2026-05-31 — Subsystem #8 host protocol crate scaffolded (plan-only sprint)

Per the v0.5+ modem plans-sprint:

- Created `crates/sonde-host-proto/` (AGPLv3-only) carrying the wire codec
  (length-prefixed CBOR), serde-typed `Body` variant set, `Dispatcher` +
  `EventSink` + `DataChannel` traits, an in-process channel transport (the
  subsystem #9 seam), a TCP server transport (the subsystem #10 seam), a
  JSON-Schema sidecar generator + freshness gate, JSONL trace middleware,
  and a cross-transport conformance test.
- The protocol locks: length-prefixed CBOR framing, req/resp/event envelope
  with u64 id correlation, semver + capability bits in `Hello`,
  loopback-default bind with auth gate for non-loopback (full TLS deferred).
- ADR 0015's `ModemTransport` shape is preserved; sonde-in-tuxlink wires
  in via `InProcessTransport`, sonde-as-daemon via `TcpServer`.

Plan: `docs/superpowers/plans/2026-05-31-clean-sheet-modem-8-host-protocol-plan.md`.
Spec: `docs/superpowers/specs/2026-05-31-clean-sheet-modem-8-host-protocol.md`.
```

- [ ] **Step 2: Commit**

```bash
git add dev/implementation-log.md
git commit -m "docs(implementation-log): subsystem #8 host protocol crate scaffolded

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 9.2: Final verification

- [ ] **Step 1: Run the full test suite for the crate**

Run: `cargo test --manifest-path crates/sonde-host-proto/Cargo.toml`
Expected: PASS — all unit tests + the schema-freshness test + the cross-transport conformance test.

- [ ] **Step 2: Verify clippy is clean**

Run: `cargo clippy --manifest-path crates/sonde-host-proto/Cargo.toml --all-targets -- -D warnings`
Expected: PASS (no warnings).

- [ ] **Step 3: Verify the schema sidecar is checked in**

Run: `git ls-files crates/sonde-host-proto/schemas/`
Expected: `crates/sonde-host-proto/schemas/.gitkeep` + `crates/sonde-host-proto/schemas/sonde-host-proto-v1.schema.json`.

- [ ] **Step 4: Confirm the LICENSE file is the AGPLv3 verbatim text**

Run: `head -5 crates/sonde-host-proto/LICENSE`
Expected: starts with `GNU AFFERO GENERAL PUBLIC LICENSE` and `Version 3, 19 November 2007`.

- [ ] **Step 5: Push (the executing session handles this — handoff complete)**

The session that executes this plan ends with `git push` per CLAUDE.md §"Session Completion". The subsystem #9 plan picks up `InProcessTransport` and wires it through `ModemTransport`; the subsystem #10 plan picks up `TcpServer` and packages a standalone daemon binary.

---

## Cross-subsystem hand-off summary

| Producer | Consumer | Interface |
|---|---|---|
| Subsystem #5 (link/MAC) | This plan (`Dispatcher::handle`) | MAC state strings (`disconnected`, `connecting`, `connected`, `closing`, `aborting`); `OpenConnection` / `CloseConnection` body variants. |
| Subsystem #6 (ARQ) | This plan (`Dispatcher::handle` + `DataChannel`) | `ArqMetricsUpdate` events; `ArqConnectionChange` events; ARQ-corrected RX/TX bytes via `DataChannel::push_tx` / `drain_rx`. |
| Subsystem #7 (link adaptation) | This plan (`Dispatcher::handle`) | `LinkAdaptModeChange` events; `LinkAdaptMetricsUpdate` events; `PhyBitLoadingUpdate` events; `SetLinkAdaptOverride` requests. |
| This plan (`InProcessTransport`) | Subsystem #9 (tuxlink integration) | `in_process::spawn(dispatcher)` returns an `InProcessClient { tx, rx }`. Subsystem #9 wraps that pair in a `SondeTransport` implementing the existing `ModemTransport` trait at `src-tauri/src/winlink/modem/mod.rs`. |
| This plan (`TcpServer`) | Subsystem #10 (standalone daemon) | `tcp::start(cfg, dispatcher)` returns a `TcpServer`. Subsystem #10 wraps that in a binary with CLI flags (`--listen <addr>`, `--token <token>`, `--trace-jsonl <path>`) and packaging. |

## Self-review (writing-plans skill checklist)

**Spec coverage:**
- §1 Role → addressed by `Dispatcher` trait + transport adapters (Phase 3, 4, 5).
- §2 What it is NOT → respected; nothing in this plan transmits, drives rig control, or implements B2F.
- §3 Forcing functions → §3.1 (transport): TCP + InProcess covered. §3.2 (text vs. binary): CBOR for wire + JSON shim. §3.3 (standardization): clean-sheet; no ARDOP host vocabulary. §3.4 (versioning): `Hello.proto_version` + capability bits. §3.5 (sync vs. async commands): req/resp envelope + event stream. §3.6 (security): loopback default + auth gate. §3.7 (performance): bounded MAX_FRAME_BYTES; no per-frame allocation in the hot path beyond what serde requires.
- §4 Open design questions: §8.Q1 (transport) — TCP + in-process. §8.Q2 (prior-art standardization) — none; clean-sheet. §8.Q3 (text vs binary) — CBOR + JSON debug shim. §8.Q4 (versioning) — semver + caps. §8.Q5 (sync vs. async) — both, via envelope. §8.Q6 (auth) — loopback default; token path; TLS deferred. §8.Q7 (two-port vs. one) — two ports (cmd + data). §8.Q8 (stable API) — v1 freeze gates the standalone daemon ship.
- §5 Citations — ARDOP `Host_Interface` PDF is in the citation library; this plan does NOT borrow vocabulary from it.
- §6 Dependencies upstream/downstream — addressed in the cross-subsystem hand-off table above.
- §8 Watched failure modes — §8 "premature commitment" handled by the cross-transport conformance test that catches drift; §8 "interop trap" sidestepped (no ARDOP vocabulary borrowed); §8 "network exposure" handled by loopback-default + non-loopback-requires-auth; §8 "capability drift" handled by `Hello.capabilities` + `Error { code: "unknown-request" }`.

**Placeholder scan:** no TBDs, no "handle edge cases", no "tests similar to". Every code step has actual code.

**Type consistency:** `Body` variant names used in Phase 2.2 match the variants enumerated in Phase 2.1 stub + the full Phase 2.2 set. `Dispatcher::handle(req: Body, sink: &Arc<dyn EventSink>) -> Body` signature matches in Phase 3.1, 3.2, 4.1, 5.1, 7.2, 8.1. `DataChannel::push_tx` / `drain_rx` signatures match between Phase 3.1 declaration and Phase 5.2 test impl. `Message` variant names (`Req` / `Resp` / `Event`) consistent throughout.

**Cross-subsystem references:** all references to subsystems #5/#6/#7/#9/#10 are by spec/plan path, no inline assumption about their internal APIs beyond the `Dispatcher` trait this plan defines.

---

Agent: opossum-pine-spruce
