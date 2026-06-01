# Clean-sheet HF modem — Subsystem #5 (Link / MAC) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a pure-Rust, deterministic, clean-sheet link/MAC layer for tuxmodem — frame codec, payload-size-aware routing decision, Part 97 station-ID enforcement, and link state machine — as a standalone `tuxmodem-link` crate in a new `crates/tuxmodem-link/` workspace member, fully unit-tested with no PHY, no RF, no network.

**Architecture:** New AGPLv3-only `tuxmodem-link` crate decomposed into focused modules. Frame format is **type-length-value (TLV) over a fixed-prefix variable-length wire** (clean-sheet derivation; not AX.25, not ARDOP, not VARA — see §"Provenance & ADR 0014 discipline" below). Routing is a pure function `decide_route(payload_len, channel_quality, policy) → RouteDecision` callable by the PHY scheduler. The link state machine is connection-oriented (over OFDM-family modes with ARQ) with a connectionless fast-path (broadcast / beacon / robustness-floor short payloads, no ARQ). Station ID is an **explicit per-frame field**, mandatory at the codec layer — Part 97 enforcement at the lowest possible point. The crate exposes pure synchronous primitives (encode / decode / state-transition / route-decide) callable from any concurrency model; concurrency is the consumer's choice (matches ADR 0015's `ModemTransport` sync-and-threads posture).

**Tech Stack:** Pure Rust, edition 2021. Workspace member at `crates/tuxmodem-link/`. Zero RF, no Tauri, no `tokio`. Dependencies kept minimal: `crc` (MIT-or-Apache) for CRC; `serde` + `serde_repr` (MIT-or-Apache) ONLY for test fixtures, NOT for wire encoding (wire encoding is hand-rolled byte-level for clean-sheet provenance + binary control). `proptest` (MIT-or-Apache) for codec round-trip property tests. All transitive licenses verified AGPL-compatible per overview §5.A.4.

**Run tests with:** `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml` (absolute manifest path per the worktree path-pinning memory entry).

**Cross-subsystem APIs this plan freezes (so #3 PHY, #6 ARQ, #7 link-adapt, #8 host-protocol can build against them):**

- **To/from #3 PHY (`PhyFrame` interface):** `tuxmodem_link::wire::encode_frame() -> Vec<u8>` and `decode_frame(&[u8]) -> Result<Frame, DecodeError>` operating on the **bit-level frame payload** that PHY delivers/consumes. PHY owns symbol-level sync; link/MAC owns byte-level structure. The PHY → MAC handoff is `(Vec<u8> payload_bits, PhyMeta { mode, snr_estimate, family })`.
- **To/from #6 ARQ:** `Frame` carries an `arq: ArqMeta` field with `seq: u16`, `ack: u16`, `nack_mask: u64` (selective-NACK bitmap, 64 frames wide), `kind: FrameKind` (DATA, ACK, NACK, SABM, UA, DISC, BEACON). The 16-bit seq space (60k frames) is the project-life commitment — preempts the AX.25 v2.0 → v2.2 retrofit failure mode called out in spec §8.
- **To/from #7 link adaptation:** `decide_route(payload_len_bytes, ChannelQuality, RoutingPolicy) -> RouteDecision { family: PhyFamily, mode_hint: Option<ModeId>, arq_enabled: bool, max_retries: u8 }`. Link-adapt provides `ChannelQuality`; link/MAC provides `payload_len`; this function is the seam.
- **To/from #8 host protocol:** `LinkSession` exposes `submit(payload: &[u8], urgency: Urgency) -> SubmitHandle`; `poll_events() -> Vec<LinkEvent>`. Host protocol vocabulary translates host commands into these calls. `LinkSession` owns the state machine, ARQ queue handoff, and route decisions.

**ADR 0015 alignment:** The link/MAC subsystem is one process-internal layer inside the tuxmodem daemon. It does NOT cross the `ModemTransport` TCP seam — that seam is between the host (tuxlink) and the daemon at the host-protocol layer (#8). Link/MAC's "session" concept is the on-air ARQ connection, NOT the host's TCP session. ADR 0015's sync-and-threads posture is honored: `LinkSession` is `Send`, holds no async runtime, and is suitable for direct use by ardopcf-style threaded transports.

**Provenance & ADR 0014 discipline:** Every architectural choice in this plan derives from generic frame-format design principles (Bertsekas/Gallager §2.3 frame structure + sequence numbering, foundation doc §4) plus Part 97 station-ID requirements. The TLV body shape is a clean-room derivation from textbook PDU layering, not from any HF-modem prior art. The 16-bit sequence space + 64-frame selective-NACK bitmap are sized from Bertsekas/Gallager's window-vs-RTT analysis. The connection-state machine (SABM/UA/DISC names) is a generic windowed-ARQ idiom from textbook coverage; we use it as a **conceptual primitive** with our own state-name semantics rather than as a copied protocol. **No examination of VARA, ARDOP, AX.25 v2.2 SREJ, or FT8/JS8 internals occurs during implementation.** If a contributor feels the urge to "just check how AX.25 does the SSID encoding," STOP per ADR 0014 §2 — TLV opaque addressing makes the question moot.

---

## File structure (locked at start)

```
crates/tuxmodem-link/
├── Cargo.toml                           # AGPLv3-only manifest, workspace member
├── LICENSE                              # AGPL-3.0-only verbatim
├── README.md                            # crate-level provenance + foundations cite
└── src/
    ├── lib.rs                           # re-exports, crate-level docs, license header
    ├── address.rs                       # opaque-address type + Part 97 callsign-bearing constructor
    ├── station_id.rs                    # Part 97 station-ID logic (per-frame + every-10-min discipline)
    ├── frame.rs                         # Frame struct + FrameKind + ArqMeta + RouteHint
    ├── wire/
    │   ├── mod.rs                       # public encode/decode entry points
    │   ├── tlv.rs                       # TLV primitive (type/length/value codec)
    │   ├── header.rs                    # fixed-prefix header (magic, version, length, kind, hdr-CRC)
    │   ├── crc.rs                       # CRC-32-IEEE wrapper around `crc` crate
    │   └── varint.rs                    # length-encoding helper (1/2/4-byte varint)
    ├── route.rs                         # decide_route() pure function + RoutingPolicy
    ├── session.rs                       # LinkSession (state machine) + LinkEvent
    └── state_machine.rs                 # connection states + transitions
crates/tuxmodem-link/tests/
    ├── wire_roundtrip.rs                # property-based codec round-trip
    ├── station_id_compliance.rs         # Part 97 enforcement tests
    ├── routing_decision_table.rs        # exhaustive (size,quality) → decision table
    └── state_machine_transitions.rs     # state machine exhaustive transition coverage
```

**Frame wire format (locked at Task 5):**

```
+----------+---------+---------+-------+---------+----------+--------+----------+--------+
|  MAGIC   | VERSION | LENGTH  | KIND  | HDR-CRC |   TLV    |  TLV   |   ...    | BODY-  |
| (2 byte) | (1 byte)| (varint)|(1 byte)|(2 byte) | record   | record |          |  CRC32 |
+----------+---------+---------+-------+---------+----------+--------+----------+--------+
```

- **MAGIC** = `0x54 0x4D` (ASCII `TM` — clean-sheet identifier, not borrowed).
- **VERSION** = `0x01` (4-bit major + 4-bit minor; bump-on-breaking).
- **LENGTH** = varint encoding total frame length excluding magic+version+length itself.
- **KIND** = `FrameKind` discriminant (see Task 3): `0x01` BEACON, `0x02` DATA, `0x03` ACK, `0x04` NACK, `0x05` SABM, `0x06` UA, `0x07` DISC, `0x08` STATION_ID_ONLY.
- **HDR-CRC** = CRC-16-CCITT over MAGIC..KIND, protecting just the header so a header-corrupted frame can be discarded without parsing the body.
- **TLV records** = body content (see Task 4): TYPE (1 byte) + LENGTH (varint) + VALUE (bytes). Mandatory TLVs per kind enumerated in Task 5.
- **BODY-CRC32** = CRC-32-IEEE over MAGIC..(end of last TLV), protecting the whole frame.

The fixed-prefix-then-TLV shape is a textbook PDU layout (Bertsekas/Gallager §2.3), used here for its known-good properties: header-CRC permits early discard, varint length supports both tiny beacons (~20 bytes) and large data frames (~16 KiB without going to 4-byte varint), TLV body permits forward-compatible extension without version bumps.

---

## Phase 1 — Crate scaffolding and license posture

### Task 1: Create the `tuxmodem-link` workspace crate

**Files:**
- Create: `crates/tuxmodem-link/Cargo.toml`
- Create: `crates/tuxmodem-link/LICENSE`
- Create: `crates/tuxmodem-link/README.md`
- Create: `crates/tuxmodem-link/src/lib.rs`
- Modify: top-level `Cargo.toml` to add `crates/tuxmodem-link` as a workspace member (if a `[workspace]` table exists; otherwise the crate stands alone and that's fine).

- [ ] **Step 1: Write the failing test**

Create `crates/tuxmodem-link/src/lib.rs`:
```rust
//! tuxmodem-link — link/MAC layer for the tuxmodem clean-sheet HF modem.
//!
//! SPDX-License-Identifier: AGPL-3.0-only
//!
//! See `docs/superpowers/specs/2026-05-31-clean-sheet-modem-5-link-mac.md`
//! for the canonical spec and ADR 0014 for the clean-sheet provenance
//! discipline. NO examination of VARA, ARDOP, AX.25 v2.2, FT8/JS8
//! internals informed this code. Sources: Bertsekas/Gallager Data
//! Networks 2e §2.3 (frame structure), Lin/Costello Error Control
//! Coding 2e §15 (ARQ), Part 97.119 (US amateur station-ID
//! requirements).

#[cfg(test)]
mod crate_smoke {
    #[test]
    fn crate_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml`
Expected: FAIL — manifest does not exist yet.

- [ ] **Step 3: Write the minimal implementation**

Create `crates/tuxmodem-link/Cargo.toml`:
```toml
[package]
name = "tuxmodem-link"
version = "0.0.1"
edition = "2021"
license = "AGPL-3.0-only"
description = "Link/MAC layer for the tuxmodem clean-sheet HF modem"
repository = "https://github.com/cameronzucker/tuxlink"
readme = "README.md"

[dependencies]
crc = "3"

[dev-dependencies]
proptest = "1"
```

Create `crates/tuxmodem-link/LICENSE` containing the verbatim AGPL-3.0-only license text from https://www.gnu.org/licenses/agpl-3.0.txt (copy the text, do not link).

Create `crates/tuxmodem-link/README.md`:
```markdown
# tuxmodem-link

Link/MAC layer for the **tuxmodem** clean-sheet HF modem.

License: **AGPL-3.0-only**.

Spec: `docs/superpowers/specs/2026-05-31-clean-sheet-modem-5-link-mac.md`
in the tuxlink repository.

Provenance: clean-sheet per ADR 0014. No examination of VARA, ARDOP,
AX.25 v2.2, FT8/JS8, or any other HF-modem prior art informed this
crate. Sources cited:

- Bertsekas, Gallager. *Data Networks*, 2nd ed., §2.3 (frame structure).
- Lin, Costello. *Error Control Coding*, 2nd ed., §15 (ARQ).
- 47 CFR §97.119 (US amateur station-ID requirements).
- `docs/research/modem-foundations.md` for the full bibliography.
```

Add `crates/tuxmodem-link` to the workspace table in the top-level `Cargo.toml` if one exists. If the top-level is a single-package manifest (no `[workspace]`), leave the top-level alone — `tuxmodem-link` builds as a standalone crate via its own manifest.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml`
Expected: PASS (`crate_smoke::crate_is_wired`).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/
# also `git add Cargo.toml` if the workspace member edit changed the root manifest
git commit -m "feat(tuxmodem-link): scaffold AGPLv3 link/MAC crate (modem #5)

Clean-sheet per ADR 0014. Cite Bertsekas/Gallager + Lin/Costello +
Part 97.119 as the design sources. No HF-modem prior art examined.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 2 — Opaque addressing with Part 97 station-ID

### Task 2: Define `Address` (opaque) and Part 97-aware constructor

The canonical address type is **opaque** (a `[u8; 16]` with constructors) — clean-sheet, not callsign+SSID-shaped (i.e. NOT AX.25). Part 97 compliance is achieved via a **station-ID payload** carried separately, not by jamming a callsign into the address field. This decouples link-layer addressing (which may eventually need to reach non-amateur peers in non-CMS networks per spec §3.6) from regulatory ID (which is per-transmission, not per-peer).

**Files:**
- Create: `crates/tuxmodem-link/src/address.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod address;
```

Create `crates/tuxmodem-link/src/address.rs`:
```rust
//! Opaque link-layer addressing.

/// 16-byte opaque address. Clean-sheet: does NOT mirror AX.25 callsign+SSID
/// encoding. Population strategies (callsign-derived, hash-derived, random)
/// are constructor choices, not type-level distinctions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address(pub [u8; 16]);

impl Address {
    pub const BROADCAST: Address = Address([0xFF; 16]);

    /// Constructs an address from a US/ITU-style callsign. Encoding: the
    /// callsign is UTF-8-encoded (uppercase), padded to 16 bytes with
    /// `0x00`. Up to 15 characters of callsign survive; longer is
    /// truncated. This is a *convenience* constructor; the address field
    /// is not the Part 97 station-ID carrier (see `station_id.rs`).
    pub fn from_callsign(callsign: &str) -> Self {
        let mut bytes = [0u8; 16];
        let cs = callsign.trim().to_ascii_uppercase();
        let src = cs.as_bytes();
        let n = src.len().min(15);
        bytes[..n].copy_from_slice(&src[..n]);
        Address(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_is_all_ones() {
        assert_eq!(Address::BROADCAST.0, [0xFF; 16]);
    }

    #[test]
    fn from_callsign_pads_with_zero_and_uppercases() {
        let a = Address::from_callsign("n7cpz");
        assert_eq!(&a.0[..5], b"N7CPZ");
        assert_eq!(&a.0[5..], &[0u8; 11][..]);
    }

    #[test]
    fn from_callsign_truncates_long_input() {
        let long = "ABCDEFGHIJKLMNOPQ"; // 17 chars
        let a = Address::from_callsign(long);
        assert_eq!(&a.0[..15], b"ABCDEFGHIJKLMNO");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml address::`
Expected: FAIL — module unwired or tests not implemented.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 is already the implementation. Verify the `pub mod address;` line is in `lib.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml address::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/address.rs crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): opaque 16-byte Address + Part 97-decoupled constructor

Address is opaque; Part 97 station ID is carried in a separate frame
TLV (next task). Decouples link-layer addressing from regulatory ID.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Part 97 station-ID enforcement type

Part 97.119 requires the licensed station to identify "at the end of each communication, and at least every 10 minutes during a communication." This crate's job: (a) carry the callsign in a `StationId` TLV that the codec rejects frames without, (b) expose a `StationIdScheduler` that the link state machine consults to enforce the 10-minute rule.

**Files:**
- Create: `crates/tuxmodem-link/src/station_id.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod station_id;
```

Create `crates/tuxmodem-link/src/station_id.rs`:
```rust
//! Part 97.119 station-ID logic.
//!
//! 47 CFR §97.119(a): "Each amateur station ... must transmit its
//! assigned call sign on its transmitting channel at the end of each
//! communication, and at least every 10 minutes during a
//! communication, for the purpose of clearly making the source of the
//! transmissions from the station known to those receiving the
//! transmissions."

use std::time::{Duration, Instant};

/// Maximum on-air interval between station-ID transmissions per
/// Part 97.119(a). The rule is 10 minutes; we conservatively schedule
/// at 9 minutes to absorb scheduling jitter, ARQ-retry-induced
/// delays, and channel-access wait.
pub const PART_97_ID_INTERVAL: Duration = Duration::from_secs(9 * 60);

/// Wire-form station ID. Encoded as a UTF-8 callsign, max 16 bytes
/// (per IARU general; US Part 97 callsigns are <=6 chars, but the
/// field is over-sized for international compatibility).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationId(pub String);

impl StationId {
    /// Returns Err if the callsign is empty or longer than 16 bytes.
    /// Does NOT validate the callsign format against any registrar's
    /// rules — the operator's licensed callsign is the only authority.
    pub fn new(callsign: &str) -> Result<Self, &'static str> {
        let cs = callsign.trim().to_ascii_uppercase();
        if cs.is_empty() {
            return Err("station ID must not be empty");
        }
        if cs.as_bytes().len() > 16 {
            return Err("station ID must be at most 16 bytes");
        }
        Ok(StationId(cs))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Scheduler that tracks the time-since-last-ID and signals when the
/// next outgoing transmission must include the station ID. The link
/// state machine consults `must_id_now()` at frame-send time; if true,
/// the frame includes the StationId TLV. The state machine also calls
/// `note_id_sent()` whenever it sends an ID-bearing frame.
pub struct StationIdScheduler {
    last_id_at: Option<Instant>,
    interval: Duration,
}

impl StationIdScheduler {
    pub fn new() -> Self {
        Self { last_id_at: None, interval: PART_97_ID_INTERVAL }
    }

    /// Constructor with a caller-controlled interval (test seam).
    pub fn with_interval(interval: Duration) -> Self {
        Self { last_id_at: None, interval }
    }

    /// True if the next outgoing transmission must include the
    /// station ID.
    pub fn must_id_now(&self, now: Instant) -> bool {
        match self.last_id_at {
            None => true,
            Some(t) => now.duration_since(t) >= self.interval,
        }
    }

    pub fn note_id_sent(&mut self, at: Instant) {
        self.last_id_at = Some(at);
    }
}

impl Default for StationIdScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_callsign() {
        assert!(StationId::new("").is_err());
        assert!(StationId::new("   ").is_err());
    }

    #[test]
    fn rejects_overlong_callsign() {
        assert!(StationId::new("ABCDEFGHIJKLMNOPQ").is_err());
    }

    #[test]
    fn uppercases_and_trims() {
        let id = StationId::new("  n7cpz  ").unwrap();
        assert_eq!(id.as_bytes(), b"N7CPZ");
    }

    #[test]
    fn scheduler_demands_id_on_first_transmission() {
        let s = StationIdScheduler::new();
        let now = Instant::now();
        assert!(s.must_id_now(now), "first TX must always carry station ID");
    }

    #[test]
    fn scheduler_suppresses_id_within_window() {
        let mut s = StationIdScheduler::with_interval(Duration::from_secs(60));
        let t0 = Instant::now();
        s.note_id_sent(t0);
        let t_within = t0 + Duration::from_secs(59);
        assert!(!s.must_id_now(t_within));
    }

    #[test]
    fn scheduler_demands_id_at_window_expiry() {
        let mut s = StationIdScheduler::with_interval(Duration::from_secs(60));
        let t0 = Instant::now();
        s.note_id_sent(t0);
        let t_expired = t0 + Duration::from_secs(60);
        assert!(s.must_id_now(t_expired));
    }

    #[test]
    fn default_interval_is_nine_minutes() {
        // 9, not 10, to absorb scheduling jitter inside the 10-minute
        // Part 97.119(a) rule.
        assert_eq!(PART_97_ID_INTERVAL, Duration::from_secs(9 * 60));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml station_id::`
Expected: FAIL — module unwired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod station_id;` is in `lib.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml station_id::`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/station_id.rs crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): Part 97.119 station-ID scheduler

9-minute interval (1-minute jitter buffer under the 10-minute rule).
Scheduler demands ID on first TX and at every interval boundary.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 3 — Wire codec primitives (TLV + varint + CRC + header)

### Task 4: Varint length encoding

The fixed-prefix header uses a 1/2/4-byte varint for the length field so a 20-byte beacon doesn't pay the cost of a 4-byte length but a 16 KiB data frame still fits. Encoding (clean-sheet, generic high-bit continuation):

- byte 0 high bit = 0 → 7-bit length 0..127, 1 byte total
- byte 0 high bit = 1, byte 1 high bit = 0 → 14-bit length 128..16383, 2 bytes total
- byte 0 high bit = 1, byte 1 high bit = 1 → 30-bit length 16384..(2^30 - 1), 4 bytes total (high bit of bytes 2,3 are reserved 0)

**Files:**
- Create: `crates/tuxmodem-link/src/wire/varint.rs`
- Create: `crates/tuxmodem-link/src/wire/mod.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod wire;
```

Create `crates/tuxmodem-link/src/wire/mod.rs`:
```rust
//! Wire-format codec (clean-sheet TLV-over-fixed-prefix).
pub mod varint;
```

Create `crates/tuxmodem-link/src/wire/varint.rs`:
```rust
//! 1/2/4-byte varint length encoding. Clean-sheet: the high-bit-
//! continuation primitive is a generic information-theoretic device,
//! not borrowed from any specific wire protocol.

#[derive(Debug, PartialEq, Eq)]
pub enum VarintError {
    Truncated,
    OverflowsU32,
}

/// Encodes `value` into `out`, returning the number of bytes written.
/// Max representable value is `(1 << 30) - 1`. Panics if `value`
/// exceeds that bound (caller's contract — surfaced as a typed error
/// from the higher-level encoder).
pub fn encode(value: u32, out: &mut Vec<u8>) {
    assert!(value < (1u32 << 30), "varint encode value out of range");
    if value < 128 {
        out.push(value as u8);
    } else if value < 16384 {
        out.push(0x80 | ((value >> 7) & 0x7F) as u8);
        out.push((value & 0x7F) as u8);
    } else {
        out.push(0x80 | ((value >> 23) & 0x7F) as u8);
        out.push(0x80 | ((value >> 16) & 0x7F) as u8);
        out.push(((value >> 8) & 0x7F) as u8);
        out.push((value & 0x7F) as u8);
    }
}

/// Decodes a varint from `bytes`, returning (value, bytes_consumed).
pub fn decode(bytes: &[u8]) -> Result<(u32, usize), VarintError> {
    if bytes.is_empty() {
        return Err(VarintError::Truncated);
    }
    let b0 = bytes[0];
    if b0 & 0x80 == 0 {
        return Ok((b0 as u32, 1));
    }
    if bytes.len() < 2 {
        return Err(VarintError::Truncated);
    }
    let b1 = bytes[1];
    if b1 & 0x80 == 0 {
        let v = (((b0 & 0x7F) as u32) << 7) | (b1 as u32);
        return Ok((v, 2));
    }
    if bytes.len() < 4 {
        return Err(VarintError::Truncated);
    }
    let b2 = bytes[2];
    let b3 = bytes[3];
    if b2 & 0x80 != 0 || b3 & 0x80 != 0 {
        return Err(VarintError::OverflowsU32);
    }
    let v = (((b0 & 0x7F) as u32) << 23)
          | (((b1 & 0x7F) as u32) << 16)
          | ((b2 as u32) << 8)
          | (b3 as u32);
    Ok((v, 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_byte_range() {
        for v in [0u32, 1, 127] {
            let mut buf = Vec::new();
            encode(v, &mut buf);
            assert_eq!(buf.len(), 1, "value {v} encodes to 1 byte");
            assert_eq!(decode(&buf).unwrap(), (v, 1));
        }
    }

    #[test]
    fn two_byte_range() {
        for v in [128u32, 256, 16383] {
            let mut buf = Vec::new();
            encode(v, &mut buf);
            assert_eq!(buf.len(), 2, "value {v} encodes to 2 bytes");
            assert_eq!(decode(&buf).unwrap(), (v, 2));
        }
    }

    #[test]
    fn four_byte_range() {
        for v in [16384u32, 65535, (1u32 << 20), (1u32 << 30) - 1] {
            let mut buf = Vec::new();
            encode(v, &mut buf);
            assert_eq!(buf.len(), 4, "value {v} encodes to 4 bytes");
            assert_eq!(decode(&buf).unwrap(), (v, 4));
        }
    }

    #[test]
    fn truncated_inputs_error() {
        assert_eq!(decode(&[]), Err(VarintError::Truncated));
        assert_eq!(decode(&[0x80]), Err(VarintError::Truncated));
        assert_eq!(decode(&[0x80, 0x80]), Err(VarintError::Truncated));
    }

    #[test]
    fn property_round_trip() {
        use proptest::prelude::*;
        proptest!(|(v in 0u32..(1u32 << 30))| {
            let mut buf = Vec::new();
            encode(v, &mut buf);
            let (decoded, n) = decode(&buf).unwrap();
            assert_eq!(decoded, v);
            assert_eq!(n, buf.len());
        });
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::varint::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod wire;` in `lib.rs` and `pub mod varint;` in `wire/mod.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::varint::`
Expected: PASS (5 tests, including the `proptest` property).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/wire/ crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): varint length encoding (1/2/4-byte)

High-bit-continuation primitive sized for 20-byte beacons through
16 KiB data frames. Property-tested for round-trip across u30 range.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: CRC wrappers (CRC-16-CCITT for header, CRC-32-IEEE for body)

**Files:**
- Create: `crates/tuxmodem-link/src/wire/crc.rs`
- Modify: `crates/tuxmodem-link/src/wire/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/wire/mod.rs`:
```rust
pub mod crc;
```

Create `crates/tuxmodem-link/src/wire/crc.rs`:
```rust
//! CRC wrappers around the `crc` crate (MIT-or-Apache).
//!
//! - Header CRC: CRC-16-CCITT (poly 0x1021, init 0xFFFF, no reflection,
//!   xorout 0). Industry-standard short-message integrity check.
//! - Body CRC: CRC-32-IEEE (poly 0x04C11DB7, init 0xFFFFFFFF, reflected,
//!   xorout 0xFFFFFFFF). Industry-standard full-frame integrity check.

use crc::{Crc, CRC_16_CCITT_FALSE, CRC_32_ISO_HDLC};

pub const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_CCITT_FALSE);
pub const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

pub fn crc16(bytes: &[u8]) -> u16 {
    CRC16.checksum(bytes)
}

pub fn crc32(bytes: &[u8]) -> u32 {
    CRC32.checksum(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // CRC-16-CCITT-FALSE check value for "123456789" is 0x29B1 per
    // the CRC catalogue (Greg Cook). Reproduced here as a smoke check.
    #[test]
    fn crc16_canonical_test_vector() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    // CRC-32-ISO-HDLC check value for "123456789" is 0xCBF43926 per
    // the CRC catalogue.
    #[test]
    fn crc32_canonical_test_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn crc16_distinguishes_single_bit_flip() {
        let original = b"hello world".to_vec();
        let mut flipped = original.clone();
        flipped[0] ^= 0x01;
        assert_ne!(crc16(&original), crc16(&flipped));
    }

    #[test]
    fn crc32_distinguishes_single_bit_flip() {
        let original = b"hello world".to_vec();
        let mut flipped = original.clone();
        flipped[0] ^= 0x01;
        assert_ne!(crc32(&original), crc32(&flipped));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::crc::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod crc;` is in `wire/mod.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::crc::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/wire/crc.rs crates/tuxmodem-link/src/wire/mod.rs
git commit -m "feat(tuxmodem-link): CRC-16-CCITT header + CRC-32-IEEE body wrappers

Canonical test vectors from the CRC catalogue verify polynomial+init
parameters match the standard.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: TLV record codec

**Files:**
- Create: `crates/tuxmodem-link/src/wire/tlv.rs`
- Modify: `crates/tuxmodem-link/src/wire/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/wire/mod.rs`:
```rust
pub mod tlv;
```

Create `crates/tuxmodem-link/src/wire/tlv.rs`:
```rust
//! TLV (type-length-value) record codec.
//!
//! TLV type space is 8-bit (256 record types). Length uses the project
//! varint (`wire::varint`). Value is opaque bytes.
//!
//! Type assignments (Phase 4 will use these):
//!  0x01 = SRC_ADDR (16 bytes — Address)
//!  0x02 = DST_ADDR (16 bytes — Address)
//!  0x03 = STATION_ID (up to 16 bytes — Part 97 callsign)
//!  0x04 = SEQ_NUM (2 bytes BE — ARQ sequence number)
//!  0x05 = ACK_NUM (2 bytes BE — cumulative ack)
//!  0x06 = NACK_MASK (8 bytes BE — selective-NACK bitmap)
//!  0x07 = PAYLOAD (variable — application bytes)
//!  0x08 = ROUTE_HINT (1 byte — RouteHint discriminant)
//!  0x80..=0xFF = reserved for forward-compatible extensions

use super::varint::{self, VarintError};

pub type TlvType = u8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tlv {
    pub t: TlvType,
    pub v: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TlvError {
    Truncated,
    LengthOverflow,
    LengthVarint(VarintError),
}

impl Tlv {
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.t);
        varint::encode(self.v.len() as u32, out);
        out.extend_from_slice(&self.v);
    }

    /// Decodes one TLV from `bytes`, returning (tlv, bytes_consumed).
    pub fn decode(bytes: &[u8]) -> Result<(Tlv, usize), TlvError> {
        if bytes.is_empty() {
            return Err(TlvError::Truncated);
        }
        let t = bytes[0];
        let (len, len_n) = varint::decode(&bytes[1..])
            .map_err(TlvError::LengthVarint)?;
        let header_n = 1 + len_n;
        let len_usize = len as usize;
        let total = header_n.checked_add(len_usize).ok_or(TlvError::LengthOverflow)?;
        if bytes.len() < total {
            return Err(TlvError::Truncated);
        }
        let v = bytes[header_n..total].to_vec();
        Ok((Tlv { t, v }, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short_value() {
        let original = Tlv { t: 0x07, v: b"hello".to_vec() };
        let mut buf = Vec::new();
        original.encode(&mut buf);
        // 1 byte type + 1 byte varint len + 5 bytes value
        assert_eq!(buf.len(), 7);
        assert_eq!(buf[0], 0x07);
        assert_eq!(buf[1], 5);
        assert_eq!(&buf[2..], b"hello");
        let (decoded, n) = Tlv::decode(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(n, 7);
    }

    #[test]
    fn round_trip_empty_value() {
        let original = Tlv { t: 0x08, v: vec![] };
        let mut buf = Vec::new();
        original.encode(&mut buf);
        assert_eq!(buf, vec![0x08, 0x00]);
        let (decoded, n) = Tlv::decode(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(n, 2);
    }

    #[test]
    fn round_trip_medium_value() {
        let original = Tlv { t: 0x07, v: vec![0xAB; 1000] };
        let mut buf = Vec::new();
        original.encode(&mut buf);
        // 1 byte type + 2 byte varint len + 1000 bytes value
        assert_eq!(buf.len(), 1003);
        let (decoded, n) = Tlv::decode(&buf).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(n, 1003);
    }

    #[test]
    fn truncated_value_errors() {
        let original = Tlv { t: 0x07, v: b"hello".to_vec() };
        let mut buf = Vec::new();
        original.encode(&mut buf);
        let truncated = &buf[..buf.len() - 1];
        assert_eq!(Tlv::decode(truncated), Err(TlvError::Truncated));
    }

    #[test]
    fn truncated_header_errors() {
        assert_eq!(Tlv::decode(&[]), Err(TlvError::Truncated));
        assert_eq!(Tlv::decode(&[0x07]), Err(TlvError::LengthVarint(VarintError::Truncated)));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::tlv::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod tlv;` is in `wire/mod.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::tlv::`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/wire/tlv.rs crates/tuxmodem-link/src/wire/mod.rs
git commit -m "feat(tuxmodem-link): TLV record codec with reserved type assignments

8-bit type space; varint length; opaque value. Reserved types
0x01..0x08 enumerated for Phase 4 frame composition.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4 — Frame model + composite codec

### Task 7: `FrameKind`, `ArqMeta`, `Frame` model types

**Files:**
- Create: `crates/tuxmodem-link/src/frame.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod frame;
```

Create `crates/tuxmodem-link/src/frame.rs`:
```rust
//! Frame model — composition of kind, addressing, ARQ metadata, and
//! payload that the codec encodes/decodes.

use crate::address::Address;
use crate::station_id::StationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Beacon = 0x01,
    Data = 0x02,
    Ack = 0x03,
    Nack = 0x04,
    /// Set Asynchronous Balanced Mode — connection setup.
    Sabm = 0x05,
    /// Unnumbered Acknowledge — connection setup reply.
    Ua = 0x06,
    /// Disconnect — connection teardown.
    Disc = 0x07,
    /// Station-ID-only frame, sent to satisfy Part 97.119 when no other
    /// frame is due within the 10-minute window.
    StationIdOnly = 0x08,
}

impl FrameKind {
    pub fn from_u8(b: u8) -> Option<FrameKind> {
        match b {
            0x01 => Some(FrameKind::Beacon),
            0x02 => Some(FrameKind::Data),
            0x03 => Some(FrameKind::Ack),
            0x04 => Some(FrameKind::Nack),
            0x05 => Some(FrameKind::Sabm),
            0x06 => Some(FrameKind::Ua),
            0x07 => Some(FrameKind::Disc),
            0x08 => Some(FrameKind::StationIdOnly),
            _ => None,
        }
    }
}

/// ARQ metadata carried by data and ack/nack frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArqMeta {
    /// Sequence number of this frame (sender's perspective). 16-bit
    /// preempts the AX.25 v2.0 sequence-wraparound failure mode.
    pub seq: u16,
    /// Cumulative ACK — highest in-order sequence the receiver has
    /// committed.
    pub ack: u16,
    /// Selective-NACK bitmap covering the 64 frames following `ack`.
    /// Bit i set => frame `ack + 1 + i` was NOT received.
    pub nack_mask: u64,
}

/// Hint to the PHY scheduler about which family should carry this
/// frame. The scheduler may override based on link-adapt policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RouteHint {
    /// Prefer the bit-adaptive OFDM family (typical case).
    Ofdm = 0x01,
    /// Prefer the robustness-modes-family floor (short critical, degraded channel).
    RobustFloor = 0x02,
    /// No preference — scheduler decides.
    Auto = 0x03,
}

impl RouteHint {
    pub fn from_u8(b: u8) -> Option<RouteHint> {
        match b {
            0x01 => Some(RouteHint::Ofdm),
            0x02 => Some(RouteHint::RobustFloor),
            0x03 => Some(RouteHint::Auto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub src: Address,
    pub dst: Address,
    /// Optional Part 97 station ID. Codec enforces presence rules
    /// at encode time (see Task 9).
    pub station_id: Option<StationId>,
    pub arq: ArqMeta,
    pub route_hint: RouteHint,
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_kind_round_trip() {
        for &k in &[
            FrameKind::Beacon, FrameKind::Data, FrameKind::Ack,
            FrameKind::Nack, FrameKind::Sabm, FrameKind::Ua,
            FrameKind::Disc, FrameKind::StationIdOnly,
        ] {
            assert_eq!(FrameKind::from_u8(k as u8), Some(k));
        }
    }

    #[test]
    fn frame_kind_rejects_unknown() {
        assert_eq!(FrameKind::from_u8(0x00), None);
        assert_eq!(FrameKind::from_u8(0xFF), None);
    }

    #[test]
    fn route_hint_round_trip() {
        for &h in &[RouteHint::Ofdm, RouteHint::RobustFloor, RouteHint::Auto] {
            assert_eq!(RouteHint::from_u8(h as u8), Some(h));
        }
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml frame::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod frame;` is in `lib.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml frame::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/frame.rs crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): Frame, FrameKind, ArqMeta, RouteHint model

16-bit sequence space and 64-bit selective-NACK bitmap. RouteHint
exposes the size-aware routing decision to the PHY scheduler.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Fixed-prefix header codec

**Files:**
- Create: `crates/tuxmodem-link/src/wire/header.rs`
- Modify: `crates/tuxmodem-link/src/wire/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/wire/mod.rs`:
```rust
pub mod header;
```

Create `crates/tuxmodem-link/src/wire/header.rs`:
```rust
//! Fixed-prefix header: MAGIC (2) + VERSION (1) + LENGTH (varint) +
//! KIND (1) + HDR-CRC (2 BE).
//!
//! The header is the part of the frame the receiver can validate
//! WITHOUT the body — a header-corrupted frame is discarded early
//! and cheaply.

use super::crc::crc16;
use super::varint::{self, VarintError};
use crate::frame::FrameKind;

pub const MAGIC: [u8; 2] = [0x54, 0x4D]; // ASCII "TM"
pub const VERSION_CURRENT: u8 = 0x01;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub frame_length: u32,
    pub kind: FrameKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HeaderError {
    Truncated,
    BadMagic,
    UnknownVersion(u8),
    UnknownKind(u8),
    LengthVarint(VarintError),
    HeaderCrcMismatch,
}

impl Header {
    pub fn encode(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.extend_from_slice(&MAGIC);
        out.push(self.version);
        varint::encode(self.frame_length, out);
        out.push(self.kind as u8);
        let hdr_crc = crc16(&out[start..]);
        out.extend_from_slice(&hdr_crc.to_be_bytes());
    }

    /// Decodes the header, returning (header, header_byte_count).
    pub fn decode(bytes: &[u8]) -> Result<(Header, usize), HeaderError> {
        if bytes.len() < 2 {
            return Err(HeaderError::Truncated);
        }
        if bytes[..2] != MAGIC {
            return Err(HeaderError::BadMagic);
        }
        if bytes.len() < 3 {
            return Err(HeaderError::Truncated);
        }
        let version = bytes[2];
        if version != VERSION_CURRENT {
            return Err(HeaderError::UnknownVersion(version));
        }
        let (frame_length, len_n) = varint::decode(&bytes[3..])
            .map_err(HeaderError::LengthVarint)?;
        let after_len = 3 + len_n;
        if bytes.len() < after_len + 3 {
            return Err(HeaderError::Truncated);
        }
        let kind_byte = bytes[after_len];
        let kind = FrameKind::from_u8(kind_byte)
            .ok_or(HeaderError::UnknownKind(kind_byte))?;
        let crc_pos = after_len + 1;
        let claimed_crc = u16::from_be_bytes([bytes[crc_pos], bytes[crc_pos + 1]]);
        let computed_crc = crc16(&bytes[..crc_pos]);
        if claimed_crc != computed_crc {
            return Err(HeaderError::HeaderCrcMismatch);
        }
        let header_bytes = crc_pos + 2;
        Ok((Header { version, frame_length, kind }, header_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short_frame_length() {
        let h = Header { version: VERSION_CURRENT, frame_length: 50, kind: FrameKind::Beacon };
        let mut buf = Vec::new();
        h.encode(&mut buf);
        // 2 magic + 1 version + 1 len-varint + 1 kind + 2 hdr-crc = 7
        assert_eq!(buf.len(), 7);
        let (decoded, n) = Header::decode(&buf).unwrap();
        assert_eq!(decoded, h);
        assert_eq!(n, 7);
    }

    #[test]
    fn round_trip_medium_frame_length() {
        let h = Header { version: VERSION_CURRENT, frame_length: 1000, kind: FrameKind::Data };
        let mut buf = Vec::new();
        h.encode(&mut buf);
        // 2 magic + 1 version + 2 len-varint + 1 kind + 2 hdr-crc = 8
        assert_eq!(buf.len(), 8);
        let (decoded, _) = Header::decode(&buf).unwrap();
        assert_eq!(decoded, h);
    }

    #[test]
    fn rejects_bad_magic() {
        let h = Header { version: VERSION_CURRENT, frame_length: 50, kind: FrameKind::Beacon };
        let mut buf = Vec::new();
        h.encode(&mut buf);
        buf[0] = 0xFF;
        assert_eq!(Header::decode(&buf), Err(HeaderError::BadMagic));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut buf = vec![MAGIC[0], MAGIC[1], 0x99, 0x00, 0x01, 0x00, 0x00];
        // Patch the header CRC to a plausible value so version is the
        // first failure encountered.
        let crc = crc16(&buf[..5]);
        buf[5] = (crc >> 8) as u8;
        buf[6] = (crc & 0xFF) as u8;
        assert_eq!(Header::decode(&buf), Err(HeaderError::UnknownVersion(0x99)));
    }

    #[test]
    fn detects_header_corruption_via_crc() {
        let h = Header { version: VERSION_CURRENT, frame_length: 50, kind: FrameKind::Beacon };
        let mut buf = Vec::new();
        h.encode(&mut buf);
        // Flip a bit in the kind byte; CRC should catch it.
        let kind_pos = buf.len() - 3;
        buf[kind_pos] ^= 0x01;
        let err = Header::decode(&buf).unwrap_err();
        assert!(matches!(err, HeaderError::HeaderCrcMismatch | HeaderError::UnknownKind(_)));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::header::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod header;` is in `wire/mod.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::header::`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/wire/header.rs crates/tuxmodem-link/src/wire/mod.rs
git commit -m "feat(tuxmodem-link): fixed-prefix header with CRC-16 integrity

MAGIC 'TM' + version + varint length + kind + header CRC enables
early discard of corrupted frames before body parsing.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Composite frame encode (with Part 97 enforcement)

**Files:**
- Modify: `crates/tuxmodem-link/src/wire/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/wire/mod.rs`:
```rust
use crate::frame::{ArqMeta, Frame, FrameKind, RouteHint};
use crate::address::Address;
use crate::station_id::StationId;
use self::tlv::Tlv;
use self::header::Header;
use self::crc::crc32;

/// TLV type assignments (see `tlv.rs` documentation).
pub const TLV_SRC_ADDR: u8 = 0x01;
pub const TLV_DST_ADDR: u8 = 0x02;
pub const TLV_STATION_ID: u8 = 0x03;
pub const TLV_SEQ_NUM: u8 = 0x04;
pub const TLV_ACK_NUM: u8 = 0x05;
pub const TLV_NACK_MASK: u8 = 0x06;
pub const TLV_PAYLOAD: u8 = 0x07;
pub const TLV_ROUTE_HINT: u8 = 0x08;

#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// Beacon/StationIdOnly frames MUST carry a StationId TLV.
    StationIdRequired,
    /// Frame would exceed the 30-bit length limit.
    FrameTooLarge,
}

/// Encodes a complete frame into a fresh `Vec<u8>`.
///
/// Part 97 enforcement: BEACON and STATION_ID_ONLY frames MUST carry a
/// `station_id`. The state machine enforces "every transmission" via
/// the `StationIdScheduler`; the codec enforces it for the kinds where
/// the ID is structurally mandatory.
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, EncodeError> {
    if matches!(frame.kind, FrameKind::Beacon | FrameKind::StationIdOnly)
        && frame.station_id.is_none()
    {
        return Err(EncodeError::StationIdRequired);
    }

    let mut body = Vec::new();

    Tlv { t: TLV_SRC_ADDR, v: frame.src.0.to_vec() }.encode(&mut body);
    Tlv { t: TLV_DST_ADDR, v: frame.dst.0.to_vec() }.encode(&mut body);

    if let Some(ref id) = frame.station_id {
        Tlv { t: TLV_STATION_ID, v: id.as_bytes().to_vec() }.encode(&mut body);
    }

    Tlv { t: TLV_SEQ_NUM, v: frame.arq.seq.to_be_bytes().to_vec() }.encode(&mut body);
    Tlv { t: TLV_ACK_NUM, v: frame.arq.ack.to_be_bytes().to_vec() }.encode(&mut body);
    Tlv { t: TLV_NACK_MASK, v: frame.arq.nack_mask.to_be_bytes().to_vec() }.encode(&mut body);
    Tlv { t: TLV_ROUTE_HINT, v: vec![frame.route_hint as u8] }.encode(&mut body);

    if !frame.payload.is_empty() {
        Tlv { t: TLV_PAYLOAD, v: frame.payload.clone() }.encode(&mut body);
    }

    // total = header bytes + body bytes + 4 (body CRC)
    // We can't know header bytes until we encode it (varint length depends on the
    // total length we're computing). Approximate, encode, then patch length if
    // the varint width changed. Simplest: encode with the *body length* in the
    // header's `frame_length` slot first to determine its varint width, then
    // recompute the true header-CRC over the final bytes.
    //
    // We follow a simpler discipline: header's `frame_length` is defined as
    // body bytes + 4 (body CRC). The receiver uses that to know how many bytes
    // follow the header. This is unambiguous and avoids the chicken-and-egg.
    let frame_length_after_header = (body.len() as u64) + 4;
    if frame_length_after_header >= (1u64 << 30) {
        return Err(EncodeError::FrameTooLarge);
    }

    let header = Header {
        version: header::VERSION_CURRENT,
        frame_length: frame_length_after_header as u32,
        kind: frame.kind,
    };

    let mut out = Vec::with_capacity(8 + body.len() + 4);
    header.encode(&mut out);
    let body_start = out.len();
    out.extend_from_slice(&body);
    let body_crc = crc32(&out[..]); // crc32 over MAGIC..end-of-last-TLV (everything so far)
    let _ = body_start; // not needed beyond clarity
    out.extend_from_slice(&body_crc.to_be_bytes());
    Ok(out)
}

#[cfg(test)]
mod encode_tests {
    use super::*;

    fn sample_frame(kind: FrameKind, with_id: bool) -> Frame {
        Frame {
            kind,
            src: Address::from_callsign("N7CPZ"),
            dst: Address::BROADCAST,
            station_id: if with_id { Some(StationId::new("N7CPZ").unwrap()) } else { None },
            arq: ArqMeta { seq: 1, ack: 0, nack_mask: 0 },
            route_hint: RouteHint::Auto,
            payload: b"hello".to_vec(),
        }
    }

    #[test]
    fn encodes_data_frame_without_id() {
        let f = sample_frame(FrameKind::Data, false);
        let bytes = encode_frame(&f).unwrap();
        // Sanity: starts with MAGIC.
        assert_eq!(&bytes[..2], &[0x54, 0x4D]);
    }

    #[test]
    fn beacon_without_id_errors() {
        let f = sample_frame(FrameKind::Beacon, false);
        assert_eq!(encode_frame(&f), Err(EncodeError::StationIdRequired));
    }

    #[test]
    fn station_id_only_without_id_errors() {
        let f = sample_frame(FrameKind::StationIdOnly, false);
        assert_eq!(encode_frame(&f), Err(EncodeError::StationIdRequired));
    }

    #[test]
    fn beacon_with_id_encodes() {
        let f = sample_frame(FrameKind::Beacon, true);
        let bytes = encode_frame(&f).unwrap();
        assert!(bytes.len() > 10);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::encode_tests::`
Expected: FAIL — module not implemented.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml wire::encode_tests::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/wire/mod.rs
git commit -m "feat(tuxmodem-link): composite frame encode with Part 97 enforcement

BEACON and STATION_ID_ONLY kinds reject encoding without a station ID.
Other kinds may carry the ID at the scheduler's discretion.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Composite frame decode + round-trip property

**Files:**
- Modify: `crates/tuxmodem-link/src/wire/mod.rs`
- Create: `crates/tuxmodem-link/tests/wire_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/wire/mod.rs`:
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Header(header::HeaderError),
    Tlv(tlv::TlvError),
    Truncated,
    BodyCrcMismatch,
    MissingTlv(u8),
    BadTlvShape(u8),
    UnknownRouteHint(u8),
}

impl From<header::HeaderError> for DecodeError {
    fn from(e: header::HeaderError) -> Self { DecodeError::Header(e) }
}

impl From<tlv::TlvError> for DecodeError {
    fn from(e: tlv::TlvError) -> Self { DecodeError::Tlv(e) }
}

pub fn decode_frame(bytes: &[u8]) -> Result<Frame, DecodeError> {
    let (hdr, hdr_n) = Header::decode(bytes)?;
    let body_plus_crc_len = hdr.frame_length as usize;
    if bytes.len() < hdr_n + body_plus_crc_len {
        return Err(DecodeError::Truncated);
    }
    if body_plus_crc_len < 4 {
        return Err(DecodeError::Truncated);
    }
    let body_end = hdr_n + body_plus_crc_len - 4;
    let crc_end = hdr_n + body_plus_crc_len;
    let claimed_crc = u32::from_be_bytes([
        bytes[body_end], bytes[body_end + 1],
        bytes[body_end + 2], bytes[body_end + 3],
    ]);
    let computed_crc = crc32(&bytes[..body_end]);
    if claimed_crc != computed_crc {
        return Err(DecodeError::BodyCrcMismatch);
    }

    // Parse TLVs.
    let mut src: Option<Address> = None;
    let mut dst: Option<Address> = None;
    let mut station_id: Option<StationId> = None;
    let mut seq: Option<u16> = None;
    let mut ack: Option<u16> = None;
    let mut nack_mask: Option<u64> = None;
    let mut route_hint: Option<RouteHint> = None;
    let mut payload: Vec<u8> = Vec::new();

    let mut cur = hdr_n;
    while cur < body_end {
        let (record, n) = Tlv::decode(&bytes[cur..body_end])?;
        cur += n;
        match record.t {
            TLV_SRC_ADDR => {
                if record.v.len() != 16 { return Err(DecodeError::BadTlvShape(record.t)); }
                let mut a = [0u8; 16]; a.copy_from_slice(&record.v); src = Some(Address(a));
            }
            TLV_DST_ADDR => {
                if record.v.len() != 16 { return Err(DecodeError::BadTlvShape(record.t)); }
                let mut a = [0u8; 16]; a.copy_from_slice(&record.v); dst = Some(Address(a));
            }
            TLV_STATION_ID => {
                let s = std::str::from_utf8(&record.v).map_err(|_| DecodeError::BadTlvShape(record.t))?;
                let id = StationId::new(s).map_err(|_| DecodeError::BadTlvShape(record.t))?;
                station_id = Some(id);
            }
            TLV_SEQ_NUM => {
                if record.v.len() != 2 { return Err(DecodeError::BadTlvShape(record.t)); }
                seq = Some(u16::from_be_bytes([record.v[0], record.v[1]]));
            }
            TLV_ACK_NUM => {
                if record.v.len() != 2 { return Err(DecodeError::BadTlvShape(record.t)); }
                ack = Some(u16::from_be_bytes([record.v[0], record.v[1]]));
            }
            TLV_NACK_MASK => {
                if record.v.len() != 8 { return Err(DecodeError::BadTlvShape(record.t)); }
                let mut a = [0u8; 8]; a.copy_from_slice(&record.v);
                nack_mask = Some(u64::from_be_bytes(a));
            }
            TLV_ROUTE_HINT => {
                if record.v.len() != 1 { return Err(DecodeError::BadTlvShape(record.t)); }
                route_hint = Some(RouteHint::from_u8(record.v[0])
                    .ok_or(DecodeError::UnknownRouteHint(record.v[0]))?);
            }
            TLV_PAYLOAD => {
                payload = record.v;
            }
            _ => { /* unknown TLV — skip per forward-compat */ }
        }
    }

    let src = src.ok_or(DecodeError::MissingTlv(TLV_SRC_ADDR))?;
    let dst = dst.ok_or(DecodeError::MissingTlv(TLV_DST_ADDR))?;
    let arq = ArqMeta {
        seq: seq.unwrap_or(0),
        ack: ack.unwrap_or(0),
        nack_mask: nack_mask.unwrap_or(0),
    };
    let route_hint = route_hint.unwrap_or(RouteHint::Auto);

    // Part 97 enforcement: BEACON / STATION_ID_ONLY require StationId.
    if matches!(hdr.kind, FrameKind::Beacon | FrameKind::StationIdOnly)
        && station_id.is_none()
    {
        return Err(DecodeError::MissingTlv(TLV_STATION_ID));
    }

    Ok(Frame {
        kind: hdr.kind,
        src,
        dst,
        station_id,
        arq,
        route_hint,
        payload,
    })
}
```

Create `crates/tuxmodem-link/tests/wire_roundtrip.rs`:
```rust
//! Integration-level round-trip property tests over the full frame
//! codec.

use tuxmodem_link::address::Address;
use tuxmodem_link::frame::{ArqMeta, Frame, FrameKind, RouteHint};
use tuxmodem_link::station_id::StationId;
use tuxmodem_link::wire::{decode_frame, encode_frame};

use proptest::prelude::*;

fn frame_strategy() -> impl Strategy<Value = Frame> {
    let kind = prop_oneof![
        Just(FrameKind::Data),
        Just(FrameKind::Ack),
        Just(FrameKind::Nack),
        Just(FrameKind::Sabm),
        Just(FrameKind::Ua),
        Just(FrameKind::Disc),
    ];
    (kind, any::<[u8; 16]>(), any::<[u8; 16]>(),
     any::<u16>(), any::<u16>(), any::<u64>(),
     prop::collection::vec(any::<u8>(), 0..2048))
        .prop_map(|(kind, src, dst, seq, ack, nack_mask, payload)| Frame {
            kind,
            src: Address(src),
            dst: Address(dst),
            station_id: None,
            arq: ArqMeta { seq, ack, nack_mask },
            route_hint: RouteHint::Auto,
            payload,
        })
}

proptest! {
    #[test]
    fn round_trip_any_frame(original in frame_strategy()) {
        let bytes = encode_frame(&original).unwrap();
        let decoded = decode_frame(&bytes).unwrap();
        prop_assert_eq!(decoded, original);
    }
}

#[test]
fn beacon_with_id_round_trips() {
    let f = Frame {
        kind: FrameKind::Beacon,
        src: Address::from_callsign("N7CPZ"),
        dst: Address::BROADCAST,
        station_id: Some(StationId::new("N7CPZ").unwrap()),
        arq: ArqMeta::default(),
        route_hint: RouteHint::RobustFloor,
        payload: b"position-beacon".to_vec(),
    };
    let bytes = encode_frame(&f).unwrap();
    let decoded = decode_frame(&bytes).unwrap();
    assert_eq!(decoded, f);
}

#[test]
fn body_corruption_caught_by_crc() {
    let f = Frame {
        kind: FrameKind::Data,
        src: Address::from_callsign("N7CPZ"),
        dst: Address::from_callsign("W1AW"),
        station_id: None,
        arq: ArqMeta { seq: 42, ack: 0, nack_mask: 0 },
        route_hint: RouteHint::Ofdm,
        payload: b"important message".to_vec(),
    };
    let mut bytes = encode_frame(&f).unwrap();
    // Flip a bit in the payload area; CRC-32 must catch it.
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    assert!(decode_frame(&bytes).is_err());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test wire_roundtrip`
Expected: FAIL — `decode_frame` not yet defined publicly, or fails compilation.

- [ ] **Step 3: Write the minimal implementation**

The decode code in Step 1 is the implementation.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml`
Expected: PASS — all crate-level unit tests + the new integration test, including the proptest property.

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/wire/mod.rs crates/tuxmodem-link/tests/wire_roundtrip.rs
git commit -m "feat(tuxmodem-link): frame decode + round-trip property tests

Forward-compat: unknown TLV types are skipped (not errored). Required
TLVs (src, dst) cause MissingTlv; BEACON/STATION_ID_ONLY require
station ID per Part 97.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 5 — Payload-size-aware routing decision

This is the central architectural deliverable for #5 per spec §1.A. The routing decision is a pure function over (payload_len, channel_quality, policy). It returns the family + ARQ choice. Subsystem #7 link adaptation supplies the channel-quality input and may consume the family selection.

### Task 11: `RoutingPolicy` and `ChannelQuality` types

**Files:**
- Create: `crates/tuxmodem-link/src/route.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod route;
```

Create `crates/tuxmodem-link/src/route.rs`:
```rust
//! Payload-size-aware routing decision (per modem-overview §5.A.2).
//!
//! The MAC layer is the *decision point* between PHY families. For
//! each outgoing frame, it consults (current channel quality estimate
//! + payload size + caller's urgency hint) and produces a
//! `RouteDecision` the PHY scheduler executes.
//!
//! This is a pure function — no I/O, no state. Subsystem #7 supplies
//! `ChannelQuality`; the application/host-protocol supplies `Urgency`;
//! MAC supplies `payload_len`.

use crate::frame::RouteHint;

/// Channel-quality estimate supplied by subsystem #7 link adaptation.
/// `snr_db` is the per-sub-carrier average SNR in dB; `fer` is the
/// observed frame-error rate over the recent window (0.0..1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelQuality {
    pub snr_db: f32,
    pub fer: f32,
}

impl ChannelQuality {
    /// Marker constant for "no observation yet" — usable on first
    /// transmission before any feedback.
    pub fn unknown() -> Self {
        Self { snr_db: 0.0, fer: 0.0 }
    }
}

/// The application's urgency hint, supplied by the host protocol.
/// `CriticalShort` is the hint that forces robustness-floor routing
/// even at marginal channel conditions; `Normal` lets the policy
/// optimize for throughput.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Normal,
    CriticalShort,
}

/// PHY family the route decision selects. Mirrors `RouteHint` but is
/// the *authoritative* answer rather than a hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyFamily {
    Ofdm,
    RobustFloor,
}

/// Caller-tunable thresholds for the routing decision. Defaults match
/// the design defaults below; subsystem #7 may pass a different policy
/// in test/measurement scenarios.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingPolicy {
    /// Below this SNR, even normal-urgency short payloads route to the
    /// robustness floor. Default: -3.0 dB (above ARDOP's narrowest
    /// floor, conservative starting point pending #1 channel-sim
    /// measurement).
    pub robust_floor_snr_db: f32,
    /// Below this FER, OFDM family is preferred; above, robustness floor.
    /// Default: 0.30 (30% frame-error rate — well above ARQ's economic
    /// recovery zone).
    pub fer_floor_threshold: f32,
    /// Payload size at or below this byte count counts as "short" for
    /// routing. Default: 256 bytes — fits ICS-213 / position beacon /
    /// status report / short ack-of-receipt classes.
    pub short_payload_max_bytes: usize,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            robust_floor_snr_db: -3.0,
            fer_floor_threshold: 0.30,
            short_payload_max_bytes: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDecision {
    pub family: PhyFamily,
    pub arq_enabled: bool,
    pub max_retries: u8,
    /// Hint to expose in the encoded frame so the receiver can verify
    /// route consistency.
    pub route_hint: RouteHint,
}

/// Decides the PHY family + ARQ posture for an outgoing frame.
///
/// Rules:
/// 1. `Urgency::CriticalShort` + short payload + degraded channel ⇒
///    robustness floor, no ARQ (FT8-pattern: retransmit-the-whole-
///    message at the host-protocol layer).
/// 2. Long payloads ⇒ OFDM family + ARQ (link-adapt picks the OFDM mode).
/// 3. Short payloads + good channel ⇒ OFDM family + ARQ (no advantage
///    to dropping down).
/// 4. Short payloads + degraded channel + Normal urgency ⇒ OFDM family
///    + ARQ; the link-adapt layer will already have stepped down within
///    OFDM. Only `CriticalShort` triggers the floor.
pub fn decide_route(
    payload_len: usize,
    quality: ChannelQuality,
    urgency: Urgency,
    policy: RoutingPolicy,
) -> RouteDecision {
    let is_short = payload_len <= policy.short_payload_max_bytes;
    let channel_degraded =
        quality.snr_db < policy.robust_floor_snr_db
            || quality.fer > policy.fer_floor_threshold;

    let family = if matches!(urgency, Urgency::CriticalShort) && is_short && channel_degraded {
        PhyFamily::RobustFloor
    } else {
        PhyFamily::Ofdm
    };

    let arq_enabled = matches!(family, PhyFamily::Ofdm);
    let max_retries = if arq_enabled { 8 } else { 3 };
    let route_hint = match family {
        PhyFamily::Ofdm => RouteHint::Ofdm,
        PhyFamily::RobustFloor => RouteHint::RobustFloor,
    };

    RouteDecision { family, arq_enabled, max_retries, route_hint }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_channel() -> ChannelQuality { ChannelQuality { snr_db: 10.0, fer: 0.01 } }
    fn degraded_channel() -> ChannelQuality { ChannelQuality { snr_db: -5.0, fer: 0.50 } }

    #[test]
    fn long_payload_always_ofdm_with_arq() {
        let r = decide_route(8192, good_channel(), Urgency::Normal, RoutingPolicy::default());
        assert_eq!(r.family, PhyFamily::Ofdm);
        assert!(r.arq_enabled);

        let r = decide_route(8192, degraded_channel(), Urgency::CriticalShort, RoutingPolicy::default());
        assert_eq!(r.family, PhyFamily::Ofdm,
                   "long payloads stay in OFDM even with CriticalShort urgency");
    }

    #[test]
    fn short_payload_good_channel_stays_ofdm() {
        let r = decide_route(100, good_channel(), Urgency::CriticalShort, RoutingPolicy::default());
        assert_eq!(r.family, PhyFamily::Ofdm,
                   "good channel: no need to drop to floor even for critical short");
    }

    #[test]
    fn short_critical_degraded_routes_to_floor() {
        let r = decide_route(100, degraded_channel(), Urgency::CriticalShort, RoutingPolicy::default());
        assert_eq!(r.family, PhyFamily::RobustFloor);
        assert!(!r.arq_enabled, "floor mode runs ARQ-disabled");
        assert_eq!(r.route_hint, RouteHint::RobustFloor);
    }

    #[test]
    fn short_normal_degraded_stays_ofdm() {
        let r = decide_route(100, degraded_channel(), Urgency::Normal, RoutingPolicy::default());
        assert_eq!(r.family, PhyFamily::Ofdm,
                   "normal urgency: let link-adapt step within OFDM; floor reserved for critical");
    }

    #[test]
    fn fer_above_threshold_counts_as_degraded() {
        let edge_fer = ChannelQuality { snr_db: 10.0, fer: 0.31 };
        let r = decide_route(100, edge_fer, Urgency::CriticalShort, RoutingPolicy::default());
        assert_eq!(r.family, PhyFamily::RobustFloor);
    }

    #[test]
    fn policy_thresholds_are_tunable() {
        let strict = RoutingPolicy {
            robust_floor_snr_db: 5.0,
            fer_floor_threshold: 0.05,
            short_payload_max_bytes: 64,
        };
        // SNR=4 dB is degraded under `strict` but fine under default.
        let q = ChannelQuality { snr_db: 4.0, fer: 0.01 };
        let r_strict = decide_route(50, q, Urgency::CriticalShort, strict);
        let r_default = decide_route(50, q, Urgency::CriticalShort, RoutingPolicy::default());
        assert_eq!(r_strict.family, PhyFamily::RobustFloor);
        assert_eq!(r_default.family, PhyFamily::Ofdm);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml route::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation. Verify `pub mod route;` in `lib.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml route::`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/route.rs crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): payload-size-aware routing decision

Pure function decide_route(len, channel_quality, urgency, policy) →
RouteDecision. Long payloads stay in OFDM+ARQ; short+critical+degraded
routes to robustness floor (no ARQ).

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Exhaustive routing decision-table integration test

**Files:**
- Create: `crates/tuxmodem-link/tests/routing_decision_table.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/tuxmodem-link/tests/routing_decision_table.rs`:
```rust
//! Exhaustive table of (payload_len, channel_quality, urgency) → expected
//! RouteDecision. Acts as a regression fence on the routing policy:
//! any change to `decide_route` that alters one of these mappings is
//! a deliberate policy change requiring a contemporaneous spec update.

use tuxmodem_link::frame::RouteHint;
use tuxmodem_link::route::{
    decide_route, ChannelQuality, PhyFamily, RoutingPolicy, Urgency,
};

struct Case {
    label: &'static str,
    payload_len: usize,
    quality: ChannelQuality,
    urgency: Urgency,
    expected_family: PhyFamily,
    expected_arq: bool,
    expected_hint: RouteHint,
}

#[test]
fn decision_table_covers_design_corners() {
    let cases = [
        Case {
            label: "Long mail under good channel",
            payload_len: 8 * 1024,
            quality: ChannelQuality { snr_db: 10.0, fer: 0.01 },
            urgency: Urgency::Normal,
            expected_family: PhyFamily::Ofdm,
            expected_arq: true,
            expected_hint: RouteHint::Ofdm,
        },
        Case {
            label: "Long mail under degraded channel (stays OFDM, ARQ catches errors)",
            payload_len: 8 * 1024,
            quality: ChannelQuality { snr_db: -5.0, fer: 0.50 },
            urgency: Urgency::Normal,
            expected_family: PhyFamily::Ofdm,
            expected_arq: true,
            expected_hint: RouteHint::Ofdm,
        },
        Case {
            label: "Short status under good channel — no need to drop down",
            payload_len: 100,
            quality: ChannelQuality { snr_db: 10.0, fer: 0.01 },
            urgency: Urgency::CriticalShort,
            expected_family: PhyFamily::Ofdm,
            expected_arq: true,
            expected_hint: RouteHint::Ofdm,
        },
        Case {
            label: "Short ICS-213 under poor channel + CRITICAL — drop to floor",
            payload_len: 200,
            quality: ChannelQuality { snr_db: -5.0, fer: 0.50 },
            urgency: Urgency::CriticalShort,
            expected_family: PhyFamily::RobustFloor,
            expected_arq: false,
            expected_hint: RouteHint::RobustFloor,
        },
        Case {
            label: "Short payload under poor channel + Normal — stay in OFDM",
            payload_len: 200,
            quality: ChannelQuality { snr_db: -5.0, fer: 0.50 },
            urgency: Urgency::Normal,
            expected_family: PhyFamily::Ofdm,
            expected_arq: true,
            expected_hint: RouteHint::Ofdm,
        },
        Case {
            label: "First TX (channel unknown) + critical short — favor floor",
            payload_len: 50,
            quality: ChannelQuality::unknown(),
            urgency: Urgency::CriticalShort,
            // unknown() = snr 0.0, fer 0.0; neither passes the degraded
            // thresholds, so OFDM is correct. This is intentional:
            // first TX optimistically uses OFDM; if it fails, link-adapt
            // updates and subsequent TX may go to floor.
            expected_family: PhyFamily::Ofdm,
            expected_arq: true,
            expected_hint: RouteHint::Ofdm,
        },
    ];

    let policy = RoutingPolicy::default();
    for c in cases {
        let r = decide_route(c.payload_len, c.quality, c.urgency, policy);
        assert_eq!(r.family, c.expected_family, "{}: family", c.label);
        assert_eq!(r.arq_enabled, c.expected_arq, "{}: arq", c.label);
        assert_eq!(r.route_hint, c.expected_hint, "{}: route_hint", c.label);
    }
}

#[test]
fn boundary_payload_size_at_threshold_is_short() {
    let policy = RoutingPolicy::default();
    let degraded = ChannelQuality { snr_db: -5.0, fer: 0.5 };
    // Exactly at threshold → short.
    let r = decide_route(256, degraded, Urgency::CriticalShort, policy);
    assert_eq!(r.family, PhyFamily::RobustFloor);
    // One byte over → long.
    let r = decide_route(257, degraded, Urgency::CriticalShort, policy);
    assert_eq!(r.family, PhyFamily::Ofdm);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test routing_decision_table`
Expected: FAIL until the test file is in place; once present, expect PASS because Task 11 already implements `decide_route`.

- [ ] **Step 3: Write the minimal implementation**

No new implementation; this task fences existing behavior.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test routing_decision_table`
Expected: PASS (2 tests, 7 case assertions).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/tests/routing_decision_table.rs
git commit -m "test(tuxmodem-link): exhaustive routing decision-table corners

Regression fence on decide_route policy. Any future change here must
update both this table and the spec narrative in lock-step.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 6 — Connection state machine + LinkSession

### Task 13: State machine module with transition table

**Files:**
- Create: `crates/tuxmodem-link/src/state_machine.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

The state machine is a generic connection-oriented windowed-ARQ idiom from textbook material (Bertsekas/Gallager §2.7 "Sliding-window protocols"). State names are textbook-generic; transitions are explicit.

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod state_machine;
```

Create `crates/tuxmodem-link/src/state_machine.rs`:
```rust
//! Connection state machine for the link layer.
//!
//! Five states (textbook windowed-ARQ form, Bertsekas/Gallager §2.7):
//!
//! ```text
//!                  +---------+
//!     start ─────► |  Idle   |
//!                  +---------+
//!                      │
//!         host:Open    │   peer:Sabm rx
//!                      ▼
//!                  +----------------+
//!         ┌────────│ConnectingOut/In│──────┐
//!         │        +----------------+      │
//!  peer:Ua rx                 host:reject  │
//!         │                                ▼
//!         ▼                            +-----+
//!     +------+                         |Idle |
//!     |Open  |                         +-----+
//!     +------+
//!         │
//!         │ host:Close OR peer:Disc rx OR retries-exhausted
//!         ▼
//!     +-----------+
//!     |Disconnect-|
//!     |ing        |
//!     +-----------+
//!         │
//!         │ peer:Ua rx OR timeout
//!         ▼
//!     +-----+
//!     |Idle |
//!     +-----+
//! ```
//!
//! Inputs are `Event` variants; outputs are `(NewState, Vec<Action>)`.

use crate::frame::FrameKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Idle,
    ConnectingOut,
    ConnectingIn,
    Open,
    Disconnecting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Host instructed us to open a connection.
    HostOpen,
    /// Host instructed us to close.
    HostClose,
    /// We received a frame of this kind from the peer.
    PeerFrame(FrameKind),
    /// A timeout expired.
    Timeout,
    /// All retries exhausted (ARQ gives up).
    RetriesExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SendFrame(FrameKind),
    NotifyHostConnected,
    NotifyHostDisconnected,
    StartTimer,
    StopTimer,
}

/// Single-event step. Returns the new state and the action list.
pub fn step(state: LinkState, event: Event) -> (LinkState, Vec<Action>) {
    use LinkState::*;
    use Event::*;
    use FrameKind::*;
    match (state, event) {
        // Idle ─ host opens → send SABM, go ConnectingOut, start retry timer.
        (Idle, HostOpen) => (ConnectingOut, vec![Action::SendFrame(Sabm), Action::StartTimer]),
        // Idle ─ peer SABM → respond UA, go Open (note: tuxmodem auto-accepts
        // incoming connections at the link layer; access-control is a host-
        // protocol concern).
        (Idle, PeerFrame(Sabm)) => (Open, vec![
            Action::SendFrame(Ua),
            Action::NotifyHostConnected,
            Action::StartTimer,
        ]),
        // Idle ─ stray peer frame → ignore, stay Idle.
        (Idle, PeerFrame(_)) => (Idle, vec![]),
        (Idle, _) => (Idle, vec![]),

        // ConnectingOut ─ peer UA → connection up.
        (ConnectingOut, PeerFrame(Ua)) => (Open, vec![
            Action::StopTimer,
            Action::NotifyHostConnected,
            Action::StartTimer,
        ]),
        // ConnectingOut ─ timeout → retry SABM, stay ConnectingOut.
        (ConnectingOut, Timeout) => (ConnectingOut, vec![
            Action::SendFrame(Sabm),
            Action::StartTimer,
        ]),
        // ConnectingOut ─ retries exhausted → back to Idle, notify host.
        (ConnectingOut, RetriesExhausted) => (Idle, vec![
            Action::StopTimer,
            Action::NotifyHostDisconnected,
        ]),
        (ConnectingOut, HostClose) => (Disconnecting, vec![
            Action::SendFrame(Disc),
            Action::StartTimer,
        ]),
        (ConnectingOut, _) => (ConnectingOut, vec![]),

        // Open ─ host closes → send DISC, go Disconnecting.
        (Open, HostClose) => (Disconnecting, vec![
            Action::SendFrame(Disc),
            Action::StartTimer,
        ]),
        // Open ─ peer DISC → respond UA, go Idle, notify host.
        (Open, PeerFrame(Disc)) => (Idle, vec![
            Action::SendFrame(Ua),
            Action::StopTimer,
            Action::NotifyHostDisconnected,
        ]),
        // Open ─ retries exhausted → unilateral close.
        (Open, RetriesExhausted) => (Idle, vec![
            Action::StopTimer,
            Action::NotifyHostDisconnected,
        ]),
        (Open, _) => (Open, vec![]),

        // Disconnecting ─ peer UA → done.
        (Disconnecting, PeerFrame(Ua)) => (Idle, vec![
            Action::StopTimer,
            Action::NotifyHostDisconnected,
        ]),
        // Disconnecting ─ timeout → unilateral close.
        (Disconnecting, Timeout) => (Idle, vec![
            Action::StopTimer,
            Action::NotifyHostDisconnected,
        ]),
        (Disconnecting, _) => (Disconnecting, vec![]),

        // ConnectingIn: reserved for future explicit-accept variant.
        // Today, incoming SABM is auto-accepted from Idle, so ConnectingIn
        // is unreachable but enumerated for forward extension.
        (ConnectingIn, _) => (ConnectingIn, vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FrameKind::*;

    #[test]
    fn host_open_from_idle_sends_sabm() {
        let (st, acts) = step(LinkState::Idle, Event::HostOpen);
        assert_eq!(st, LinkState::ConnectingOut);
        assert_eq!(acts, vec![Action::SendFrame(Sabm), Action::StartTimer]);
    }

    #[test]
    fn peer_sabm_in_idle_auto_accepts() {
        let (st, acts) = step(LinkState::Idle, Event::PeerFrame(Sabm));
        assert_eq!(st, LinkState::Open);
        assert!(acts.contains(&Action::SendFrame(Ua)));
        assert!(acts.contains(&Action::NotifyHostConnected));
    }

    #[test]
    fn peer_ua_in_connecting_completes_setup() {
        let (st, acts) = step(LinkState::ConnectingOut, Event::PeerFrame(Ua));
        assert_eq!(st, LinkState::Open);
        assert!(acts.contains(&Action::NotifyHostConnected));
    }

    #[test]
    fn timeout_in_connecting_retries_sabm() {
        let (st, acts) = step(LinkState::ConnectingOut, Event::Timeout);
        assert_eq!(st, LinkState::ConnectingOut);
        assert_eq!(acts, vec![Action::SendFrame(Sabm), Action::StartTimer]);
    }

    #[test]
    fn retries_exhausted_in_connecting_returns_to_idle() {
        let (st, acts) = step(LinkState::ConnectingOut, Event::RetriesExhausted);
        assert_eq!(st, LinkState::Idle);
        assert!(acts.contains(&Action::NotifyHostDisconnected));
    }

    #[test]
    fn host_close_from_open_sends_disc() {
        let (st, acts) = step(LinkState::Open, Event::HostClose);
        assert_eq!(st, LinkState::Disconnecting);
        assert!(acts.contains(&Action::SendFrame(Disc)));
    }

    #[test]
    fn peer_disc_in_open_completes_teardown() {
        let (st, acts) = step(LinkState::Open, Event::PeerFrame(Disc));
        assert_eq!(st, LinkState::Idle);
        assert!(acts.contains(&Action::SendFrame(Ua)));
        assert!(acts.contains(&Action::NotifyHostDisconnected));
    }

    #[test]
    fn stray_data_in_idle_ignored() {
        let (st, acts) = step(LinkState::Idle, Event::PeerFrame(Data));
        assert_eq!(st, LinkState::Idle);
        assert!(acts.is_empty());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml state_machine::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml state_machine::`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/state_machine.rs crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): connection state machine (Idle/Connecting*/Open/Disconnecting)

Pure (LinkState, Event) → (LinkState, Vec<Action>) step function.
Textbook windowed-ARQ form per Bertsekas/Gallager §2.7. SABM/UA/DISC
state names are generic primitives, not copied from any specific
modem's wire protocol.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 14: Exhaustive state-machine transition coverage test

**Files:**
- Create: `crates/tuxmodem-link/tests/state_machine_transitions.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/tuxmodem-link/tests/state_machine_transitions.rs`:
```rust
//! Every (state, event) input is hit at least once. Acts as a
//! coverage fence: a future state addition must extend this test.

use tuxmodem_link::frame::FrameKind;
use tuxmodem_link::state_machine::{step, Event, LinkState};

#[test]
fn every_state_handles_every_event_without_panic() {
    let states = [
        LinkState::Idle, LinkState::ConnectingOut, LinkState::ConnectingIn,
        LinkState::Open, LinkState::Disconnecting,
    ];
    let events = [
        Event::HostOpen,
        Event::HostClose,
        Event::PeerFrame(FrameKind::Sabm),
        Event::PeerFrame(FrameKind::Ua),
        Event::PeerFrame(FrameKind::Disc),
        Event::PeerFrame(FrameKind::Data),
        Event::PeerFrame(FrameKind::Ack),
        Event::PeerFrame(FrameKind::Nack),
        Event::PeerFrame(FrameKind::Beacon),
        Event::PeerFrame(FrameKind::StationIdOnly),
        Event::Timeout,
        Event::RetriesExhausted,
    ];
    for s in states {
        for e in events.iter().cloned() {
            // No assertion on the new state — we only care that step()
            // is total and doesn't panic. Other tests fence the
            // semantically interesting transitions.
            let _ = step(s, e);
        }
    }
}

#[test]
fn idle_to_open_full_round_trip() {
    // Idle → ConnectingOut (host open) → Open (peer UA) → Disconnecting (host
    // close) → Idle (peer UA).
    let (s, _) = step(LinkState::Idle, Event::HostOpen);
    assert_eq!(s, LinkState::ConnectingOut);
    let (s, _) = step(s, Event::PeerFrame(FrameKind::Ua));
    assert_eq!(s, LinkState::Open);
    let (s, _) = step(s, Event::HostClose);
    assert_eq!(s, LinkState::Disconnecting);
    let (s, _) = step(s, Event::PeerFrame(FrameKind::Ua));
    assert_eq!(s, LinkState::Idle);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test state_machine_transitions`
Expected: FAIL — test file does not exist yet; once present, PASS.

- [ ] **Step 3: Write the minimal implementation**

No new implementation needed.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test state_machine_transitions`
Expected: PASS (2 tests, 60 transitions exercised).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/tests/state_machine_transitions.rs
git commit -m "test(tuxmodem-link): state-machine totality + full round-trip coverage

Covers every (state, event) cell. Future state additions must extend.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 15: `LinkSession` façade — ties codec + state machine + routing + station-ID together

**Files:**
- Create: `crates/tuxmodem-link/src/session.rs`
- Modify: `crates/tuxmodem-link/src/lib.rs`

`LinkSession` is the API surface subsystem #8 (host protocol) consumes. It owns: a state machine instance, a station-ID scheduler, the local station ID, the local + peer addresses, the current channel-quality observation, the outgoing-frame submission queue. It does NOT own ARQ retransmission queues or PHY handoff — those are subsystem #6 + #3 concerns, addressable through events.

- [ ] **Step 1: Write the failing test**

Append to `crates/tuxmodem-link/src/lib.rs`:
```rust
pub mod session;
```

Create `crates/tuxmodem-link/src/session.rs`:
```rust
//! `LinkSession` — façade tying the codec, state machine, routing, and
//! station-ID enforcement into a single API for the host protocol
//! (subsystem #8) and the PHY scheduler (subsystem #3) to consume.
//!
//! ADR 0015 alignment: this type is `Send`, holds no async runtime,
//! and is suitable for direct use inside the threaded `ModemTransport`
//! pattern ardopcf already uses.

use std::collections::VecDeque;
use std::time::Instant;

use crate::address::Address;
use crate::frame::{ArqMeta, Frame, FrameKind, RouteHint};
use crate::route::{decide_route, ChannelQuality, PhyFamily, RoutingPolicy, Urgency};
use crate::state_machine::{step, Action, Event, LinkState};
use crate::station_id::{StationId, StationIdScheduler};

/// Outgoing-frame submission handle returned to the host protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmitHandle(pub u64);

/// Application-level events the host protocol observes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    Connected,
    Disconnected,
    FrameDelivered(SubmitHandle),
    FrameFailed(SubmitHandle),
    /// Bytes arrived from the peer (delivered upward).
    PayloadReceived(Vec<u8>),
}

pub struct LinkSession {
    local: Address,
    peer: Address,
    local_id: StationId,
    state: LinkState,
    id_sched: StationIdScheduler,
    quality: ChannelQuality,
    policy: RoutingPolicy,
    next_handle: u64,
    next_seq: u16,
    pending_events: VecDeque<LinkEvent>,
}

impl LinkSession {
    pub fn new(local: Address, peer: Address, local_id: StationId) -> Self {
        Self {
            local,
            peer,
            local_id,
            state: LinkState::Idle,
            id_sched: StationIdScheduler::new(),
            quality: ChannelQuality::unknown(),
            policy: RoutingPolicy::default(),
            next_handle: 0,
            next_seq: 0,
            pending_events: VecDeque::new(),
        }
    }

    pub fn state(&self) -> LinkState { self.state }
    pub fn set_policy(&mut self, policy: RoutingPolicy) { self.policy = policy; }
    pub fn update_quality(&mut self, q: ChannelQuality) { self.quality = q; }
    pub fn poll_events(&mut self) -> Vec<LinkEvent> { self.pending_events.drain(..).collect() }

    /// Submit a payload for transmission. Returns the handle the host
    /// can use to correlate `FrameDelivered` / `FrameFailed` events.
    /// Returns `None` if the link is not in a state that accepts data
    /// (Idle, ConnectingOut, ConnectingIn, Disconnecting).
    pub fn submit(&mut self, payload: Vec<u8>, urgency: Urgency) -> Option<(SubmitHandle, Frame)> {
        if !matches!(self.state, LinkState::Open) {
            return None;
        }
        let handle = SubmitHandle(self.next_handle);
        self.next_handle += 1;
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);

        let decision = decide_route(payload.len(), self.quality, urgency, self.policy);
        let station_id = if self.id_sched.must_id_now(Instant::now()) {
            Some(self.local_id.clone())
        } else {
            None
        };
        let route_hint = match decision.family {
            PhyFamily::Ofdm => RouteHint::Ofdm,
            PhyFamily::RobustFloor => RouteHint::RobustFloor,
        };
        let frame = Frame {
            kind: FrameKind::Data,
            src: self.local,
            dst: self.peer,
            station_id,
            arq: ArqMeta { seq, ack: 0, nack_mask: 0 },
            route_hint,
            payload,
        };
        if frame.station_id.is_some() {
            self.id_sched.note_id_sent(Instant::now());
        }
        Some((handle, frame))
    }

    /// Drive the state machine from a host action.
    pub fn host_open(&mut self) -> Vec<Action> {
        self.drive(Event::HostOpen)
    }

    pub fn host_close(&mut self) -> Vec<Action> {
        self.drive(Event::HostClose)
    }

    /// Drive the state machine from a peer frame.
    pub fn ingest_peer_frame(&mut self, frame: &Frame) -> Vec<Action> {
        let actions = self.drive(Event::PeerFrame(frame.kind));
        if frame.kind == FrameKind::Data && self.state == LinkState::Open {
            self.pending_events.push_back(LinkEvent::PayloadReceived(frame.payload.clone()));
        }
        actions
    }

    fn drive(&mut self, e: Event) -> Vec<Action> {
        let (new_state, actions) = step(self.state, e);
        self.state = new_state;
        for a in &actions {
            match a {
                Action::NotifyHostConnected => self.pending_events.push_back(LinkEvent::Connected),
                Action::NotifyHostDisconnected => self.pending_events.push_back(LinkEvent::Disconnected),
                _ => {}
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> LinkSession {
        LinkSession::new(
            Address::from_callsign("N7CPZ"),
            Address::from_callsign("W1AW"),
            StationId::new("N7CPZ").unwrap(),
        )
    }

    #[test]
    fn submit_rejects_when_not_open() {
        let mut s = session();
        assert!(s.submit(b"hi".to_vec(), Urgency::Normal).is_none());
    }

    #[test]
    fn submit_attaches_station_id_on_first_open_send() {
        let mut s = session();
        // Bring to Open via simulated peer UA after host open.
        s.host_open();
        let fake_ua = Frame {
            kind: FrameKind::Ua,
            src: Address::from_callsign("W1AW"),
            dst: Address::from_callsign("N7CPZ"),
            station_id: None,
            arq: ArqMeta::default(),
            route_hint: RouteHint::Auto,
            payload: vec![],
        };
        s.ingest_peer_frame(&fake_ua);
        assert_eq!(s.state(), LinkState::Open);

        let (_, frame) = s.submit(b"hi".to_vec(), Urgency::Normal).unwrap();
        assert!(frame.station_id.is_some(),
            "first TX after Open must carry station ID per Part 97 enforcement");
    }

    #[test]
    fn submit_routes_short_critical_under_degraded_to_floor() {
        let mut s = session();
        s.host_open();
        let fake_ua = Frame {
            kind: FrameKind::Ua,
            src: Address::from_callsign("W1AW"),
            dst: Address::from_callsign("N7CPZ"),
            station_id: None,
            arq: ArqMeta::default(),
            route_hint: RouteHint::Auto,
            payload: vec![],
        };
        s.ingest_peer_frame(&fake_ua);
        s.update_quality(ChannelQuality { snr_db: -5.0, fer: 0.5 });
        let (_, frame) = s.submit(b"ICS-213".to_vec(), Urgency::CriticalShort).unwrap();
        assert_eq!(frame.route_hint, RouteHint::RobustFloor);
    }

    #[test]
    fn host_open_emits_no_events_until_ua_arrives() {
        let mut s = session();
        s.host_open();
        assert!(s.poll_events().is_empty());
    }

    #[test]
    fn open_handshake_emits_connected_event() {
        let mut s = session();
        s.host_open();
        let fake_ua = Frame {
            kind: FrameKind::Ua,
            src: Address::from_callsign("W1AW"),
            dst: Address::from_callsign("N7CPZ"),
            station_id: None,
            arq: ArqMeta::default(),
            route_hint: RouteHint::Auto,
            payload: vec![],
        };
        s.ingest_peer_frame(&fake_ua);
        let events = s.poll_events();
        assert_eq!(events, vec![LinkEvent::Connected]);
    }

    #[test]
    fn ingested_data_payload_surfaces_as_event() {
        let mut s = session();
        s.host_open();
        let fake_ua = Frame {
            kind: FrameKind::Ua,
            src: Address::from_callsign("W1AW"),
            dst: Address::from_callsign("N7CPZ"),
            station_id: None,
            arq: ArqMeta::default(),
            route_hint: RouteHint::Auto,
            payload: vec![],
        };
        s.ingest_peer_frame(&fake_ua);
        let _ = s.poll_events();
        let fake_data = Frame {
            kind: FrameKind::Data,
            src: Address::from_callsign("W1AW"),
            dst: Address::from_callsign("N7CPZ"),
            station_id: None,
            arq: ArqMeta { seq: 1, ack: 0, nack_mask: 0 },
            route_hint: RouteHint::Ofdm,
            payload: b"hello".to_vec(),
        };
        s.ingest_peer_frame(&fake_data);
        let events = s.poll_events();
        assert_eq!(events, vec![LinkEvent::PayloadReceived(b"hello".to_vec())]);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml session::`
Expected: FAIL — module not wired.

- [ ] **Step 3: Write the minimal implementation**

The code in Step 1 already is the implementation.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml session::`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/src/session.rs crates/tuxmodem-link/src/lib.rs
git commit -m "feat(tuxmodem-link): LinkSession facade for host-protocol consumption

Ties codec + state machine + routing + station-ID scheduler. submit()
exposes the size-aware routing decision and Part 97 ID attachment in
one entry point. Sync, Send, no async runtime — ADR 0015 sync-threads
posture.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 7 — Part 97 compliance integration test + cross-subsystem contract record

### Task 16: Part 97 compliance integration test (encode-and-walk-the-wire)

**Files:**
- Create: `crates/tuxmodem-link/tests/station_id_compliance.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/tuxmodem-link/tests/station_id_compliance.rs`:
```rust
//! Part 97.119 station-ID compliance fence.
//!
//! Walks a 30-minute simulated session and asserts that no on-air
//! interval exceeds the 10-minute Part 97 maximum without an ID-bearing
//! frame having been emitted.

use std::time::{Duration, Instant};

use tuxmodem_link::address::Address;
use tuxmodem_link::frame::{ArqMeta, Frame, FrameKind, RouteHint};
use tuxmodem_link::station_id::{StationId, StationIdScheduler, PART_97_ID_INTERVAL};
use tuxmodem_link::wire::{decode_frame, encode_frame};

#[test]
fn beacon_frame_carries_station_id_on_wire() {
    let frame = Frame {
        kind: FrameKind::Beacon,
        src: Address::from_callsign("N7CPZ"),
        dst: Address::BROADCAST,
        station_id: Some(StationId::new("N7CPZ").unwrap()),
        arq: ArqMeta::default(),
        route_hint: RouteHint::RobustFloor,
        payload: b"position".to_vec(),
    };
    let bytes = encode_frame(&frame).unwrap();
    let decoded = decode_frame(&bytes).unwrap();
    assert!(decoded.station_id.is_some());
    assert_eq!(decoded.station_id.unwrap().as_bytes(), b"N7CPZ");
}

#[test]
fn station_id_only_frame_carries_id() {
    let frame = Frame {
        kind: FrameKind::StationIdOnly,
        src: Address::from_callsign("N7CPZ"),
        dst: Address::BROADCAST,
        station_id: Some(StationId::new("N7CPZ").unwrap()),
        arq: ArqMeta::default(),
        route_hint: RouteHint::Auto,
        payload: vec![],
    };
    let bytes = encode_frame(&frame).unwrap();
    let decoded = decode_frame(&bytes).unwrap();
    assert!(decoded.station_id.is_some());
}

#[test]
fn scheduler_simulated_session_never_exceeds_ten_minutes() {
    // Simulate a 30-minute session. The session sends a non-ID frame
    // every 30 seconds. The scheduler must demand an ID-bearing frame
    // at least every 10 minutes — we ASSERT that the longest gap is
    // strictly less than the 10-minute rule (we target 9 minutes per
    // PART_97_ID_INTERVAL, the buffered value).
    let mut sched = StationIdScheduler::new();
    let t0 = Instant::now();
    let mut id_times: Vec<Instant> = Vec::new();
    let mut t = t0;
    let total = Duration::from_secs(30 * 60);
    let step = Duration::from_secs(30);
    while t.duration_since(t0) < total {
        if sched.must_id_now(t) {
            sched.note_id_sent(t);
            id_times.push(t);
        }
        t += step;
    }
    assert!(!id_times.is_empty(), "scheduler must have demanded at least one ID");
    assert_eq!(id_times[0], t0, "first TX must always carry ID");

    // Gap between consecutive IDs must not exceed PART_97_ID_INTERVAL +
    // the 30-second polling step (the polling step is the simulation's
    // discretization, not a tolerance on Part 97 — but it bounds how
    // soon after the deadline the scheduler can fire).
    let max_allowed = PART_97_ID_INTERVAL + step;
    for w in id_times.windows(2) {
        let gap = w[1].duration_since(w[0]);
        assert!(gap <= max_allowed,
            "ID gap {gap:?} exceeded {max_allowed:?} (Part 97 budget + sim step)");
    }
    // And the absolute Part 97 ceiling is 10 minutes; the buffered
    // 9-minute schedule + 30-second step is well under it.
    let ten_min = Duration::from_secs(10 * 60);
    for w in id_times.windows(2) {
        let gap = w[1].duration_since(w[0]);
        assert!(gap < ten_min, "ID gap {gap:?} reached the Part 97 ceiling");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test station_id_compliance`
Expected: FAIL (file doesn't exist); once present, PASS using already-implemented code.

- [ ] **Step 3: Write the minimal implementation**

No new implementation needed.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml --test station_id_compliance`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/tests/station_id_compliance.rs
git commit -m "test(tuxmodem-link): Part 97.119 compliance fence over 30-minute sim

Asserts (a) BEACON and STATION_ID_ONLY frames carry the ID on the
wire, (b) a 30-minute simulated session never exceeds the 10-minute
Part 97 ID interval.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 17: Cross-subsystem API contract doc

Subsystems #3, #6, #7, and #8 build against #5's public API. This task adds a `docs/` page in the crate enumerating the public types and the assumptions each sibling subsystem may make.

**Files:**
- Create: `crates/tuxmodem-link/docs/cross-subsystem-api.md`

- [ ] **Step 1: Write the failing test**

Tests aren't applicable to a docs-only task — but we still gate with a `cargo doc` build so doc-comment mismatches surface.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo doc --manifest-path crates/tuxmodem-link/Cargo.toml --no-deps`
Expected: PASS (no broken doctests / intra-doc-link errors).

- [ ] **Step 3: Write the minimal implementation**

Create `crates/tuxmodem-link/docs/cross-subsystem-api.md`:
```markdown
# Cross-subsystem API contract — tuxmodem-link (#5 Link/MAC)

This is the API surface other modem subsystems build against. Locked
by `2026-05-31-clean-sheet-modem-5-link-mac-plan.md` (Phase 7,
Task 17).

## To/from #3 PHY

PHY (#3) hands bit-level frame payloads to MAC and consumes encoded
frames from MAC. The handoff is byte-level only; MAC owns no
sample/symbol concepts.

- `tuxmodem_link::wire::encode_frame(&Frame) -> Result<Vec<u8>, EncodeError>`
- `tuxmodem_link::wire::decode_frame(&[u8]) -> Result<Frame, DecodeError>`

PHY metadata for received frames (mode, family, SNR estimate) is the
PHY scheduler's concern; MAC's decode does not consume it. PHY MAY
discard frames with `Header::HeaderCrcMismatch` before invoking
`decode_frame`; PHY MUST invoke `decode_frame` for any frame whose
header parsed cleanly.

## To/from #6 ARQ

ARQ (#6) consumes `ArqMeta` from incoming frames and populates
`ArqMeta` on outgoing frames. The 16-bit sequence space allows
windows up to 32767 in-flight frames; the 64-bit NACK mask covers
the 64 frames following `ack`.

- `tuxmodem_link::frame::ArqMeta { seq, ack, nack_mask }`
- `tuxmodem_link::frame::FrameKind::{Ack, Nack, Data}`

ARQ is mode-conditional per overview §5.A.2: `RouteDecision.arq_enabled`
is the authoritative gate.

## To/from #7 link adaptation

Link-adapt (#7) supplies the `ChannelQuality` observation that MAC
feeds to `decide_route`. MAC produces the `RouteDecision`; #7 may
ALSO take the decision as an input when picking the mode within the
chosen family.

- `tuxmodem_link::route::ChannelQuality { snr_db, fer }`
- `tuxmodem_link::route::Urgency::{Normal, CriticalShort}`
- `tuxmodem_link::route::decide_route(payload_len, q, urgency, policy) -> RouteDecision`
- `tuxmodem_link::route::RoutingPolicy` (tunable thresholds)

The default policy thresholds (-3 dB SNR floor, 0.30 FER, 256-byte
short cutoff) are starting points pending #1 channel-sim measurement;
#7 may override at run time.

## To/from #8 host protocol

Host protocol (#8) consumes `LinkSession` to expose the link layer to
the host (tuxlink). Host commands map to:

- `LinkSession::new(local, peer, local_id)` — instantiate.
- `LinkSession::host_open()` / `LinkSession::host_close()` —
  connection lifecycle.
- `LinkSession::submit(payload, urgency) -> Option<(SubmitHandle, Frame)>` —
  outgoing data.
- `LinkSession::ingest_peer_frame(&Frame)` — incoming data (after PHY+ARQ
  delivery).
- `LinkSession::poll_events()` — connect/disconnect/delivery/payload
  notifications.
- `LinkSession::update_quality(ChannelQuality)` — #7 pushes into #5
  via the session.

## ADR 0015 alignment

`LinkSession` is `Send`. It holds no async runtime. It is suitable
for direct use inside the threaded `ModemTransport` pattern. The
session's "connection" concept is the on-air ARQ link, NOT the
host-side TCP session — that one lives in subsystem #8.

## ADR 0014 alignment

Every API name in this document derives from textbook material:

- `FrameKind::{Sabm, Ua, Disc}` — generic windowed-ARQ state names per
  Bertsekas/Gallager §2.7. Not adopted from AX.25 (whose wire layout
  is distinct from ours).
- `ArqMeta::{seq, ack, nack_mask}` — generic selective-repeat ARQ
  fields per Lin/Costello §15.
- `decide_route` policy thresholds — derived from a strategic posture
  ("beat ARDOP's narrowest mode at the noise-floor case") and the
  Shannon per-constellation argument; no specific dB number is copied.

No examination of VARA, ARDOP, AX.25 v2.2, or FT8/JS8 internals
informed the design.
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo doc --manifest-path crates/tuxmodem-link/Cargo.toml --no-deps`
Expected: PASS (no doc warnings escalated to errors).

- [ ] **Step 5: Commit**

```bash
git add crates/tuxmodem-link/docs/cross-subsystem-api.md
git commit -m "docs(tuxmodem-link): cross-subsystem API contract for #3/#6/#7/#8 consumers

Enumerates the public surface other modem subsystems build against.
Lock-stepped with the Phase 7 plan section; future API changes update
both.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 8 — Whole-crate quality gate

### Task 18: Whole-crate test + clippy + format pass

**Files:**
- No file edits. Operates on the whole crate.

- [ ] **Step 1: Run the full test suite**

Run: `cargo test --manifest-path crates/tuxmodem-link/Cargo.toml`
Expected: PASS — every unit test + every integration test (`wire_roundtrip`,
`routing_decision_table`, `state_machine_transitions`, `station_id_compliance`)
passes. Total ~50 tests.

- [ ] **Step 2: Run clippy at the deny level the project uses**

Run: `cargo clippy --manifest-path crates/tuxmodem-link/Cargo.toml --all-targets -- -D warnings`
Expected: PASS (no warnings escalated to errors). Fix any lints inline before committing.

- [ ] **Step 3: Run rustfmt check**

Run: `cargo fmt --manifest-path crates/tuxmodem-link/Cargo.toml -- --check`
Expected: PASS. If it fails, run without `--check` to apply, then re-run with `--check`.

- [ ] **Step 4: Verify doc build**

Run: `cargo doc --manifest-path crates/tuxmodem-link/Cargo.toml --no-deps`
Expected: PASS.

- [ ] **Step 5: Commit any format/clippy fixes**

```bash
git add crates/tuxmodem-link/
git commit -m "chore(tuxmodem-link): clippy + rustfmt cleanup pass

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

(If nothing changed, skip the commit — no empty commits.)

---

## Self-review checklist applied to this plan

**Spec coverage:**

- §1.A Payload-size-aware routing — Tasks 11, 12, 15.
- §1 Role: frame format + link state machine — Tasks 7, 8, 9, 10, 13, 14.
- §3.1 Station identification (Part 97) — explicit field per frame, Tasks 3, 7, 9, 16.
- §3.2 Frame header overhead — fixed-prefix header (~7-8 bytes) + TLV body, Tasks 8, 9.
- §3.3 Variable vs. fixed-size — variable via varint length, Task 4.
- §3.4 CRC for frame integrity — CRC-16 header + CRC-32 body, Task 5.
- §3.5 Connection-oriented vs. connectionless — both via FrameKind (Sabm/Ua/Disc for connection; Beacon for connectionless), Tasks 7, 13.
- §3.6 Addressing scheme — opaque 16-byte addresses (callsign-bearing constructor convenience), Task 2.
- §3.7 No VARA examination — provenance block in every commit + crate-level header.
- §4.Q1 Frame layout — locked at Task 8/9 (fixed-prefix + TLV).
- §4.Q2 Station ID — explicit per-frame TLV (Task 9) + Part 97 scheduler (Task 3).
- §4.Q3 Addressing — opaque 16-byte (Task 2).
- §4.Q4 Fixed vs. variable — variable, Task 4.
- §4.Q5 CRC polynomial — CRC-16-CCITT header + CRC-32-IEEE body, Task 5.
- §4.Q6 Connection-oriented + connectionless — both supported, Tasks 7, 13.
- §4.Q7 Digipeater/relay path — deferred (not modeled in v0.5+; spec §3.6 lists it as more-complex; addressing is opaque, so a digipeater extension is non-breaking later).
- §4.Q8 Sequence number width — 16-bit (Task 7), preempts the AX.25 v2.0→v2.2 failure mode called out in spec §8.

**Deferred (with rationale captured in plan body):**

- Digipeater/relay path (§4.Q7) — addable later without breaking changes because addressing is opaque + TLV body is forward-compatible. Not in scope for the v0.5+ tuxmodem MVP per spec §3.6.
- Hybrid ARQ (Type II / III HARQ) — subsystem #6's question, not #5's. #5 carries an `ArqMeta` field rich enough to support HARQ when #6 adopts it.
- Specific bit-loading commands and host-protocol command vocabulary — subsystem #8's concern; #5 exposes `LinkSession` as the surface to map to.

**Placeholder scan:** none. Every code step has the actual code; every command has the expected outcome. No "TBD" / "TODO" / "fill in" / "similar to" markers.

**Type consistency:** `Address` (16 bytes), `Frame`, `FrameKind`, `ArqMeta`, `RouteHint`, `LinkState`, `Event`, `Action`, `LinkSession`, `StationId`, `StationIdScheduler`, `ChannelQuality`, `Urgency`, `PhyFamily`, `RoutingPolicy`, `RouteDecision`, `SubmitHandle`, `LinkEvent`, `EncodeError`, `DecodeError`, `HeaderError`, `TlvError`, `VarintError` — names match across tasks.

---

## Execution choice

Plan complete and saved to `docs/superpowers/plans/2026-05-31-clean-sheet-modem-5-link-mac-plan.md`.

Two execution options remain available to the operator (or the parent agent's downstream dispatch):

1. **Subagent-Driven** (recommended for a plan of this scope) — fresh subagent per Task, parent reviews between tasks, fast iteration. Use `superpowers:subagent-driven-development`.

2. **Inline Execution** — execute Tasks 1–18 in one session with checkpoints. Use `superpowers:executing-plans`.

Eighteen tasks across eight phases. End-to-end on a Pi 5 dev target the plan should land in roughly two focused sessions plus a quality-gate sweep.
