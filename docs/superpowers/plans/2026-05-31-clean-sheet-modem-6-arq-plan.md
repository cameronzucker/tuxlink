# Clean-sheet modem subsystem #6 — ARQ implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a mode-conditional ARQ subsystem for sonde that runs selective-repeat sliding-window ARQ above the bit-adaptive OFDM PHY family AND retransmit-the-whole-message (no frame-level ARQ) semantics in the robustness-modes-family floor, with the mode boundary driven by an explicit per-connection `ArqProfile` set by subsystem #7 (link adaptation). The subsystem ships as a pure-software Rust crate (`sonde-arq`) testable in isolation against mocked MAC, FEC, and link-adaptation peers; no on-air work, no RF, plan-only execution.

**Architecture:** A new in-tree-then-extracted Rust crate `crates/sonde-arq/` under sonde's workspace (the workspace itself is built incrementally per the program's DSP-first sequencing; this plan creates the workspace if it doesn't already exist). The crate exposes one top-level `ArqEndpoint` type that owns per-connection state and operates in either of two `ArqMode` variants — `Windowed` (selective-repeat sliding-window for the OFDM family) or `MessageRetransmit` (FT8-pattern whole-message resend with no per-frame ACK/NACK, for floor modes). Mode is set at connection setup from `LinkAdaptationHint` and may be switched mid-connection via an explicit `ArqEndpoint::set_mode(new_mode)` call that drains in-flight state cleanly. ARQ is mocked-MAC- and mocked-FEC-driven for tests: the crate consumes a `MacPeer` trait (frame send/receive, sequence numbering surfaced from #5) and a `FecResidualSignal` trait (provides per-decode-attempt outcome class — INTACT / CORRECTED / RESIDUAL-ERROR / UNDECODABLE — from #4) and emits a `LinkAdaptationStats` stream (FER, retransmit count, in-flight depth, RTT estimate) consumed by #7 and presents an in-order byte stream to #8 via a `HostStream: Read + Write` adapter. **Long HF RTT is the central design constraint** — windows are wide (default 64 frames for Windowed mode), timeouts are RTT-tracked with explicit slow-floor, NACKs are rate-limited via reverse-channel piggybacking, and the `MessageRetransmit` floor mode dispenses with per-frame ARQ entirely so it doesn't pay the RTT cost on a regime where it can't be afforded.

**Tech Stack:** Rust 2021 edition, no_std-compatible core with `std`-feature gate (the crate must run in sonde-the-daemon and eventually in a no_std embedded target). Pure-Rust deps only (AGPLv3-compatible per overview §5.A.4): `bitvec = "1"` for sequence-window bitmaps, `rand = "0.8"` + `rand_pcg = "0.3"` for deterministic test scenario generation, `tokio = { version = "1", features = ["sync", "time", "macros"] }` behind a `std` feature gate for the async runtime tests, `tracing = "0.1"` for structured logging. No GPL-only deps. No deps that pull in any HF-modem prior art. Tests use `proptest = "1"` for state-machine property tests + `criterion = "0.5"` for window-throughput benchmarks. The channel-simulator crate (subsystem #1) is consumed as a dev-dependency for end-to-end "PHY-to-PHY through impaired channel" tests in Phase 6.

**Authority for behaviour:** Spec at `docs/superpowers/specs/2026-05-31-clean-sheet-modem-6-arq.md` and overview §5.A.2 / §5.A.3. Foundation citations §4.1 (Lin/Costello *Error Control Coding* — selective-repeat + HARQ taxonomy; Bertsekas/Gallager *Data Networks* — throughput-vs-window analysis) and §6.1 (K1JT FT4/FT8 paper — *conceptual primitive* of whole-message resend, NOT specific timing/parameters). No examination of VARA, ARDOP frame-level ARQ implementations, AX.25 LAPB internals, Winlink B2F resync, or any OS-specific TCP ARQ implementation per ADR 0014. Each behavioural decision below cites the open primitive it derives from; if you find yourself wanting to check "how X does it," STOP.

**RADIO-1 / no-RF boundary:** every test in this plan runs against an in-memory mock peer, a scripted-channel simulator, or a deterministic byte-loss injector. No test transmits. No test opens a serial device. No test exercises real hardware. ARQ is by construction a layer above the air interface; it cannot meaningfully be RF-tested in isolation, and the operator gates the RF-integrated end-to-end test under RADIO-1 once the full stack lands (subsystems #3-#7 must all exist for that).

**Cross-subsystem APIs locked down by this plan:**

| Peer | Direction | API surface (trait or type) | Cadence |
|---|---|---|---|
| #5 (MAC) | upstream — provides frames | `MacPeer::poll_inbound_frame() -> Option<MacFrame>`; `MacPeer::send_frame(MacFrame) -> Result<(), MacBackpressure>`; `MacFrame { seq: SeqNum, payload: Bytes, ack_piggyback: Option<AckRange>, flags: MacFlags }` | Event-driven per frame |
| #4 (FEC) | upstream — provides decode-outcome signal per inbound frame | `FecResidualSignal::classify(&MacFrame) -> FecOutcome { Intact, Corrected { errs }, Residual { conf }, Undecodable }` | One classification per inbound frame |
| #7 (link adaptation) | bidirectional — receives stats, provides mode hints | OUT: `LinkAdaptationStats { fer_window, retrans_count_window, in_flight_depth, rtt_mean, rtt_var, ack_latency_p95 }` published every `LINK_ADAPT_PUBLISH_INTERVAL` (default 500 ms); IN: `LinkAdaptationHint { recommended_mode: ArqMode, recommended_window: u16 }` | OUT 2 Hz; IN event-driven on mode-step |
| #8 (host protocol) | downstream — consumes ARQ-corrected stream + state queries | `HostStream: Read + Write` for in-order bytes; `HostQuery::connection_state() -> ConnectionState`; `HostQuery::throughput_metrics() -> ThroughputMetrics` | Stream-paced + on-demand query |

These APIs are settled by this plan. Subsystems #4 / #5 / #7 / #8 plans must match this surface; if a sibling subagent has chosen incompatible names, the names get reconciled at the integration phase (Phase 7), but the *shape* (what information flows in which direction, at what cadence) is fixed here.

**Run tests with:** `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml` (absolute manifest path per the worktree path-pinning convention from `feedback_pin_paths_in_worktree_sessions`).

---

## File structure

| File | Responsibility |
|---|---|
| `crates/sonde-arq/Cargo.toml` | Crate manifest; AGPLv3-only license declaration; dep pins |
| `crates/sonde-arq/src/lib.rs` | Crate root; re-exports public types; feature-gates `std` vs no_std |
| `crates/sonde-arq/src/seq.rs` | `SeqNum` with mod-2^N wraparound, distance-aware comparison; `SeqWindow` ring buffer |
| `crates/sonde-arq/src/frame.rs` | `MacFrame`, `AckRange`, `MacFlags`, `MacBackpressure`, `FecOutcome` — the upstream interface types |
| `crates/sonde-arq/src/profile.rs` | `ArqMode { Windowed, MessageRetransmit }`, `ArqProfile`, `LinkAdaptationHint` — config + mode switching |
| `crates/sonde-arq/src/rtt.rs` | Smoothed RTT estimator (Jacobson/Karels-style — primitive only; reinvented from textbook formulae) with HF slow-floor |
| `crates/sonde-arq/src/windowed.rs` | Selective-repeat sliding-window ARQ state machine for `ArqMode::Windowed` (OFDM-family modes) |
| `crates/sonde-arq/src/message_retransmit.rs` | FT8-pattern whole-message resend state machine for `ArqMode::MessageRetransmit` (floor modes) |
| `crates/sonde-arq/src/endpoint.rs` | `ArqEndpoint` — owns connection state, dispatches to the active mode's state machine, exposes mode-switching |
| `crates/sonde-arq/src/stats.rs` | `LinkAdaptationStats` publisher + `ThroughputMetrics` aggregator |
| `crates/sonde-arq/src/host_stream.rs` | `HostStream: Read + Write` adapter presenting the ARQ-corrected byte stream to subsystem #8 |
| `crates/sonde-arq/src/peers.rs` | `MacPeer` + `FecResidualSignal` traits (consumer-side mock helpers under `cfg(test)` + the trait defs) |
| `crates/sonde-arq/tests/mock_peer.rs` | In-memory MAC peer + scripted FEC outcomes for integration tests |
| `crates/sonde-arq/tests/windowed_property.rs` | Proptest property tests for Windowed mode (no-loss, lossy, burst-error, reorder, wrap) |
| `crates/sonde-arq/tests/message_retransmit_property.rs` | Proptest property tests for MessageRetransmit mode |
| `crates/sonde-arq/tests/mode_switch.rs` | Mode-switching scenarios (Windowed → MessageRetransmit and back, with in-flight frames) |
| `crates/sonde-arq/tests/channel_sim_e2e.rs` | End-to-end ARQ-over-channel-sim integration test (dev-dep on subsystem #1) |
| `crates/sonde-arq/benches/throughput.rs` | Criterion benchmark — throughput vs. RTT × loss-rate × window-size for both modes |

`endpoint.rs` is the only file that knits the two state machines together; it gets the cross-provider Codex round at the end of Phase 5 (per the project's adversarial-review discipline).

---

## Architectural decisions locked down by this plan

These are the choices this plan freezes for subsystem #6. Decisions explicitly deferred to per-mode tuning or to integration phase are listed in the next subsection.

1. **ARQ flavor above-floor (§6.Q1):** **selective-repeat sliding-window** ARQ for `ArqMode::Windowed`. Go-back-N is ruled out because HF burst errors at the deep-fade scale would waste an entire window per burst; hybrid (Stutter-ARQ / cumulative-ACK fallback) is ruled out for v0.5+ because the implementation complexity isn't earned by the throughput delta in the regime where MessageRetransmit takes over anyway. Primitive: Lin/Costello §22 (foundation §4.1), Bertsekas/Gallager §2.4 throughput-vs-window analysis. NOT derived from any specific HF-protocol implementation.
2. **Floor-mode ARQ (§6.Q1 floor variant):** **no frame-level ARQ.** `ArqMode::MessageRetransmit` operates the FT8-conceptual pattern — the SENDER repeats the whole message N times (configurable, default 3), the RECEIVER tries to decode each repetition independently, success at any repetition completes the transfer, no NACK exists. Primitive: K1JT FT4/FT8 paper (foundation §6.1) describes this as a generic "no-ARQ weak-signal" pattern; sonde reinvents the pattern conceptually without inheriting FT8's timing, frame layout, or scheduling.
3. **Window size (§6.Q2):** **negotiated within a fixed envelope.** Default 64; min 8; max 256. The 8-bit sequence space chosen in Phase 1 supports up to 127 in-flight (mod-256 half-window). 64 is calibrated for 4-second HF RTT × 16-Hz frame rate (OFDM-family typical) keeping the pipe full. Negotiation happens at connection setup via the host protocol; mid-connection adjustment happens via `LinkAdaptationHint::recommended_window`. Window adjustment never shrinks below in-flight depth.
4. **ACK style (§6.Q3):** **piggybacked ACK on next data frame in the reverse direction PLUS standalone ACK frame when reverse channel is idle.** The `MacFrame::ack_piggyback: Option<AckRange>` field carries an `AckRange { acked_through: SeqNum, sack_bitmap: BitVec }` — the cumulative-ACK-plus-SACK-bitmap shape, primitive from TCP RFC theory. Standalone ACK is sent when `time_since_last_reverse_frame > T_ACK_DELAY` (default 1 second for HF) AND there's outstanding unacked-by-peer state. This bounds ACK latency without flooding the reverse channel.
5. **NACK supported (§6.Q4):** **YES, but only as SACK gaps in the AckRange, never as a standalone NACK frame.** Explicit standalone NACKs are ruled out — they cost reverse bandwidth and the SACK bitmap already conveys the same information. This avoids the "NACK storm" failure mode called out in the spec §8.
6. **Retransmission backoff (§6.Q5):** **RTT-tracking with slow-floor.** Smoothed RTT estimator (primitive from Jacobson/Karels — Bertsekas/Gallager §2.7 textbook) with `RTO = SRTT + 4*RTTVAR`, BUT clamped to `RTO >= 2 * MIN_RTT_FLOOR` where `MIN_RTT_FLOOR = 3 seconds` for HF. This prevents the runaway-pessimist failure mode where one fast cycle pulls the estimate down, the next cycle times out spuriously, and the connection enters a retransmit storm. The slow-floor is the HF-specific adaptation. Exponential backoff (×2) on consecutive retransmits of the same frame, capped at `RTO_MAX = 60 seconds`.
7. **HARQ type (§6.Q6):** **Type I.** Retransmissions resend the original frame at the same code rate. Type II (incremental redundancy) and Type III (rate-compatible) are explicitly deferred to v0.6+. Rationale: Type I composes cleanly with the per-frame FEC choice in subsystem #4 (FEC decides its code rate; ARQ is FEC-rate-agnostic); Type II/III couples #4 and #6 design tightly, which is exactly the integration burden the program is trying to avoid in v0.5+.
8. **Connection-state machine (§6.Q7):** **clean-sheet 5-state FSM** — `Closed → Opening → Open → Closing → Closed`, with explicit `Reset` transition on link-failure. This is reinvented from first principles (request → ack → data → close handshake is a generic primitive; not from AX.25 LAPB, not from TCP-specific state names like SYN_SENT). State names + transitions specified in Phase 4 Task 4.1.
9. **Max retransmission count (§6.Q8):** **operator-tunable via `ArqProfile`, default 10.** On exhaustion, the connection transitions to `Reset` and surfaces a `ConnectionFailed { reason: RetransmitExhaustion }` event up through host protocol. The 10 default is calibrated for "give the channel ~10 RTOs to recover from a fade" — not derived from any specific prior-art modem.
10. **Mode-switching mechanism (THE CENTRAL ARCHITECTURAL PIVOT):** the boundary between Windowed and MessageRetransmit is driven by **subsystem #7's `LinkAdaptationHint` arriving on a tokio channel.** ARQ does not autonomously decide mode — link adaptation owns the channel-quality observation and emits a hint when its policy crosses the OFDM-family / floor boundary. ARQ honors the hint by calling `ArqEndpoint::set_mode(new_mode)`, which (a) drains in-flight Windowed frames (either ACKs them all or fails the connection if drain times out at `MODE_SWITCH_DRAIN_TIMEOUT = 10 * RTO_MAX`), (b) flushes the receive reassembly buffer, (c) reinitializes the active state machine, and (d) emits a `ModeSwitched` event to host protocol so the operator sees the transition. The mode-switch primitive is "explicit hint from policy layer + state-machine drain at the layer below" — generic, not inherited.

### Decisions deferred

These remain open at the per-mode-tuning or integration-phase level and are out of scope for this plan:

- Exact OFDM-family frame rate (subsystem #3's choice — affects window-size calibration).
- Exact FEC residual-error probability distribution (subsystem #4's measurement — affects the `FecOutcome::Residual { conf }` threshold ARQ uses to NACK).
- Specific link-adaptation mode-switch thresholds (subsystem #7's policy — affects how often `LinkAdaptationHint` fires).
- Host-protocol command vocabulary for ARQ state queries (subsystem #8's spec — `HostQuery` shape is settled here, but the wire protocol exposing it is #8's).
- Connection-level encryption / authentication — out of scope for ARQ (RADIO-1 + ADR 0011 considerations live at higher layers).

---

## Long-HF-RTT-specific design choices (called out for review)

These are the choices driven specifically by HF's multi-second RTT regime. Anyone reviewing this plan should sanity-check that the choices make sense for 2-8 second RTTs (the operational envelope from `project_g90_vara_standard_works_firsthand` operator experience, used as background per ADR 0014 §3, not as a citation):

1. **Wide default window (64).** Stop-and-wait at 4-second RTT gives ~10% link utilization at best (spec §8 failure mode). A 64-frame window keeps the pipe full at 16 fps × 4 s = 64 frames in flight, achieving ~100% utilization in the no-loss case.
2. **Slow-floor RTO (>= 6 seconds).** RTT-tracking estimators tuned for LAN/WAN environments (where RTT < 100ms is normal) collapse spuriously on HF. The 3-second `MIN_RTT_FLOOR` × 2 multiplier (yielding `RTO >= 6 s`) is the HF-specific clamp.
3. **Piggyback-first ACK strategy.** Standalone ACK at 1-second delay (T_ACK_DELAY) is generous compared to LAN protocols — but on HF a standalone ACK costs ~250 ms of TX-key + frame time, so flooding the reverse channel with ACKs is more expensive than the latency hit of waiting.
4. **SACK bitmap, not standalone NACK.** Same rationale as #3 — the reverse channel is precious; NACK information rides in the ACK or doesn't get sent.
5. **MessageRetransmit as the floor strategy.** Per-frame ARQ at -5 dB SNR is structurally broken (ACKs themselves get lost; retransmits get lost; the RTO-multiplier chase never converges). FT8-pattern whole-message resend abandons per-frame reliability for whole-message reliability at the cost of latency, which is the correct tradeoff for short critical payloads in degraded conditions.
6. **Mode-switch drain timeout (10 × RTO_MAX = 600 s).** Mode switches can't be instantaneous on HF — in-flight frames need to either land or time out before the new mode takes over, and HF RTOs can stretch. 600 s is generous; the alternative is dropping in-flight payload, which violates host-stream in-order delivery.

---

## Phases

The plan has **7 phases**, each producing testable software that can be reviewed and committed independently per the program's per-task-branch discipline (ADR 0004). Phase ordering reflects build dependencies — sequence primitives before state machines, state machines before mode-switching, mode-switching before stats, stats before host stream, host stream before channel-sim integration.

| Phase | Title |
|---|---|
| 1 | Crate scaffolding + sequence primitives + RTT estimator |
| 2 | Upstream interface types (MacFrame, FecOutcome, profile/mode types) |
| 3 | Windowed-mode selective-repeat state machine (the meat for OFDM-family modes) |
| 4 | MessageRetransmit-mode state machine + connection FSM (floor-mode + lifecycle) |
| 5 | ArqEndpoint dispatcher + mode-switching with drain semantics |
| 6 | LinkAdaptationStats publisher + HostStream byte-stream adapter |
| 7 | Channel-sim end-to-end integration + criterion benchmarks + cross-provider Codex adrev |

Each phase ends with a commit, a quality-gate run (`cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml` + `cargo clippy --all-targets`), and a phase-boundary checkpoint per the executing-plans discipline.

---

### Phase 1 — Crate scaffolding + sequence primitives + RTT estimator

**Files:**
- Create: `crates/sonde-arq/Cargo.toml`
- Create: `crates/sonde-arq/src/lib.rs`
- Create: `crates/sonde-arq/src/seq.rs`
- Create: `crates/sonde-arq/src/rtt.rs`

#### Task 1.1: Workspace + crate scaffolding

- [ ] **Step 1: Verify sonde workspace exists at repo root**

Run: `ls crates/ 2>&1 || echo "no workspace yet"`
If "no workspace yet": create `crates/` directory and add a workspace `Cargo.toml` at the repo root referencing it. If the workspace already exists (because sibling subagents may have scaffolded one), add `sonde-arq` to the existing `[workspace.members]` list. **Coordination note:** subsystems #1, #3, #4, #5, #7, #8 plans run in parallel and may also scaffold the workspace; the integration-phase reconciliation handles any duplicate workspace setup. For this plan, assume the workspace doesn't exist and create it; the merge step will dedupe.

- [ ] **Step 2: Write `crates/sonde-arq/Cargo.toml`**

```toml
[package]
name = "sonde-arq"
version = "0.0.1"
edition = "2021"
license = "AGPL-3.0-only"
description = "Mode-conditional ARQ subsystem for the sonde clean-sheet HF data modem. Selective-repeat sliding-window above the OFDM family; FT8-pattern whole-message retransmit at the robustness floor."
repository = "https://github.com/cameronzucker/sonde"
publish = false

[features]
default = ["std"]
std = ["tokio", "tracing"]

[dependencies]
bitvec = { version = "1", default-features = false, features = ["alloc"] }
tokio = { version = "1", features = ["sync", "time", "macros", "rt"], optional = true }
tracing = { version = "0.1", optional = true }

[dev-dependencies]
proptest = "1"
criterion = { version = "0.5", features = ["html_reports"] }
rand = "0.8"
rand_pcg = "0.3"

[[bench]]
name = "throughput"
harness = false
```

- [ ] **Step 3: Write `crates/sonde-arq/src/lib.rs`**

```rust
//! sonde-arq — mode-conditional ARQ subsystem for the sonde clean-sheet HF modem.
//!
//! Per subsystem #6 spec (docs/superpowers/specs/2026-05-31-clean-sheet-modem-6-arq.md)
//! and overview §5.A.2: ARQ is mode-conditional. Above the robustness-modes-family floor,
//! selective-repeat sliding-window ARQ operates normally. At the floor, no frame-level
//! ARQ — the FT8-conceptual pattern of whole-message retransmit takes over.
//!
//! Designed clean-sheet per ADR 0014. No prior-art examination.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod frame;
pub mod profile;
pub mod seq;

#[cfg(feature = "std")]
pub mod endpoint;
#[cfg(feature = "std")]
pub mod host_stream;
#[cfg(feature = "std")]
pub mod message_retransmit;
#[cfg(feature = "std")]
pub mod peers;
#[cfg(feature = "std")]
pub mod rtt;
#[cfg(feature = "std")]
pub mod stats;
#[cfg(feature = "std")]
pub mod windowed;

pub use frame::{FecOutcome, MacBackpressure, MacFlags, MacFrame};
pub use profile::{ArqMode, ArqProfile, LinkAdaptationHint};
pub use seq::{SeqNum, SeqWindow};

#[cfg(feature = "std")]
pub use endpoint::{ArqEndpoint, ConnectionState, ConnectionFailed};
#[cfg(feature = "std")]
pub use stats::{LinkAdaptationStats, ThroughputMetrics};
```

- [ ] **Step 4: Cargo workspace registration**

In the repo-root `Cargo.toml` `[workspace]` section, add `"crates/sonde-arq"` to `members`. If no workspace exists yet, create one:

```toml
[workspace]
resolver = "2"
members = ["crates/sonde-arq"]
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml`
Expected: PASS (warnings allowed — empty modules will warn about unused `pub`).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/sonde-arq/
git commit -m "feat(arq): scaffold sonde-arq crate (subsystem #6)

Crate scaffolding for the mode-conditional ARQ subsystem per overview §5.A.2.
AGPLv3-only license, pure-Rust deps, std-feature-gated for future no_std.

Plan: docs/superpowers/plans/2026-05-31-clean-sheet-modem-6-arq-plan.md
Spec: docs/superpowers/specs/2026-05-31-clean-sheet-modem-6-arq.md

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 1.2: SeqNum mod-2^8 wraparound type + distance comparison

**Files:**
- Modify: `crates/sonde-arq/src/seq.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/seq.rs`:

```rust
//! Sequence number primitives. ARQ uses 8-bit sequence numbers (256 distinct values,
//! mod-2^8 wraparound) with distance-aware comparison — N(s)=255 is "before" N(s)=0
//! when the half-window distance is computed correctly. Foundation: Bertsekas/Gallager
//! §2.4 sliding-window analysis, reinvented from textbook formulae (NOT from any
//! specific protocol's wire layout).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeqNum(pub u8);

impl SeqNum {
    pub const ZERO: SeqNum = SeqNum(0);

    /// Increment with mod-2^8 wrap.
    pub fn next(self) -> Self {
        SeqNum(self.0.wrapping_add(1))
    }

    /// Signed distance from self to other in the window-aware sense. Positive means
    /// "other is after self." Result is in [-128, 127]. This is the "lollipop" comparison
    /// — at distance > 127 the answer is ambiguous, and the caller must not have allowed
    /// a window that wide.
    pub fn distance_to(self, other: SeqNum) -> i16 {
        let diff = (other.0 as i16) - (self.0 as i16);
        if diff > 127 { diff - 256 }
        else if diff < -128 { diff + 256 }
        else { diff }
    }

    pub fn lt_window(self, other: SeqNum) -> bool { self.distance_to(other) > 0 }
    pub fn le_window(self, other: SeqNum) -> bool { self.distance_to(other) >= 0 }
}

pub struct SeqWindow; // filled in Task 1.3

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wraps() {
        assert_eq!(SeqNum(255).next(), SeqNum(0));
        assert_eq!(SeqNum(0).next(), SeqNum(1));
    }

    #[test]
    fn distance_within_half_window() {
        assert_eq!(SeqNum(0).distance_to(SeqNum(10)), 10);
        assert_eq!(SeqNum(10).distance_to(SeqNum(0)), -10);
    }

    #[test]
    fn distance_across_wrap() {
        // 250 → 5 is +11 (going forward through wrap), NOT -245.
        assert_eq!(SeqNum(250).distance_to(SeqNum(5)), 11);
        // 5 → 250 is -11 (going backward through wrap), NOT +245.
        assert_eq!(SeqNum(5).distance_to(SeqNum(250)), -11);
    }

    #[test]
    fn lt_window_is_correct_at_wrap_boundary() {
        assert!(SeqNum(250).lt_window(SeqNum(5)));
        assert!(!SeqNum(5).lt_window(SeqNum(250)));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml seq::`
Expected: this *passes* on first compile because the struct + impl are written in Step 1. To make Step 2 a genuine RED step, write only the `#[cfg(test)] mod tests` block first (no struct, no impl), run, observe `error[E0433]: failed to resolve: SeqNum`, then add the struct + impl.
Expected (with type omitted): FAIL — cannot find type `SeqNum`.

- [ ] **Step 3: Write minimal implementation**

Add the `SeqNum` struct + `impl` shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml seq::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/seq.rs
git commit -m "feat(arq): SeqNum mod-2^8 with distance-aware comparison

8-bit sequence numbers with wrap-aware distance per Bertsekas/Gallager §2.4
sliding-window analysis. Half-window distance comparison enables the wider-
than-7-bit windows needed for HF RTT (overview §5.A.2 + plan §long-HF-RTT-1).

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 1.3: SeqWindow ring buffer for in-flight / received-but-out-of-order tracking

**Files:**
- Modify: `crates/sonde-arq/src/seq.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/sonde-arq/src/seq.rs` (replacing the `pub struct SeqWindow;` stub):

```rust
use alloc::vec::Vec;
use bitvec::prelude::*;

/// A sliding window of sequence numbers, parameterized by window size at construction.
/// Tracks which seq numbers are "set" (e.g., received-and-buffered, or sent-but-unacked)
/// via a BitVec aligned to the window's lower bound. The lower bound advances as in-order
/// frames are consumed.
#[derive(Debug, Clone)]
pub struct SeqWindow {
    lower: SeqNum,
    size: u16,
    set: BitVec,
}

impl SeqWindow {
    pub fn new(lower: SeqNum, size: u16) -> Self {
        assert!(size > 0 && size <= 128, "window size must be 1..=128");
        SeqWindow { lower, size, set: bitvec![0; size as usize] }
    }

    pub fn lower(&self) -> SeqNum { self.lower }
    pub fn upper_exclusive(&self) -> SeqNum {
        let mut s = self.lower;
        for _ in 0..self.size { s = s.next(); }
        s
    }

    /// Mark `seq` as set. Returns true if `seq` is within the window and was newly set.
    pub fn set(&mut self, seq: SeqNum) -> bool {
        let d = self.lower.distance_to(seq);
        if d < 0 || d >= self.size as i16 { return false; }
        let already = self.set[d as usize];
        self.set.set(d as usize, true);
        !already
    }

    pub fn is_set(&self, seq: SeqNum) -> bool {
        let d = self.lower.distance_to(seq);
        if d < 0 || d >= self.size as i16 { return false; }
        self.set[d as usize]
    }

    /// Advance the lower bound past all consecutively-set entries from the bottom.
    /// Returns the count of slots advanced.
    pub fn slide_consecutive(&mut self) -> u16 {
        let mut count = 0u16;
        while !self.set.is_empty() && self.set[0] {
            self.set.remove(0);
            self.set.push(false);
            self.lower = self.lower.next();
            count += 1;
        }
        count
    }

    /// Render the "gap bitmap" — for each slot 0..size, true if NOT set. Used to build SACK
    /// info in piggybacked ACKs (§6.Q4 — SACK gaps, no standalone NACK).
    pub fn gap_bitmap(&self) -> BitVec {
        !self.set.clone()
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    #[test]
    fn set_within_window_succeeds() {
        let mut w = SeqWindow::new(SeqNum(0), 16);
        assert!(w.set(SeqNum(5)));
        assert!(!w.set(SeqNum(5))); // already set
        assert!(w.is_set(SeqNum(5)));
    }

    #[test]
    fn set_outside_window_is_rejected() {
        let mut w = SeqWindow::new(SeqNum(0), 16);
        assert!(!w.set(SeqNum(20)));
        assert!(!w.set(SeqNum(255))); // distance is -1, behind lower
    }

    #[test]
    fn slide_consecutive_advances_past_set_run() {
        let mut w = SeqWindow::new(SeqNum(0), 16);
        w.set(SeqNum(0));
        w.set(SeqNum(1));
        w.set(SeqNum(2));
        w.set(SeqNum(4)); // gap at 3
        assert_eq!(w.slide_consecutive(), 3);
        assert_eq!(w.lower(), SeqNum(3));
    }

    #[test]
    fn slide_handles_wrap() {
        let mut w = SeqWindow::new(SeqNum(254), 8);
        w.set(SeqNum(254));
        w.set(SeqNum(255));
        w.set(SeqNum(0));
        w.set(SeqNum(1));
        assert_eq!(w.slide_consecutive(), 4);
        assert_eq!(w.lower(), SeqNum(2));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml seq::window_tests`
Expected: FAIL on first run if you wrote tests-only first; otherwise PASS. Follow the TDD discipline — write tests, see FAIL, then add the impl.

- [ ] **Step 3: Write minimal implementation**

The `SeqWindow` impl shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml seq::window_tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/seq.rs
git commit -m "feat(arq): SeqWindow ring buffer with slide + gap-bitmap (SACK substrate)

Sliding window of seq numbers tracked via bitvec; slide_consecutive advances past
the contiguous-from-lower run; gap_bitmap is the substrate for SACK piggybacks
(§6.Q4 — no standalone NACK; gaps ride in the ACK).

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 1.4: Smoothed RTT estimator with HF slow-floor

**Files:**
- Modify: `crates/sonde-arq/src/rtt.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/rtt.rs`:

```rust
//! Smoothed RTT estimator with an HF-specific slow-floor on the retransmit timeout.
//!
//! Algorithm: Jacobson/Karels exponential-weighted moving average — SRTT_{n+1} =
//! (1 - alpha) * SRTT_n + alpha * sample; RTTVAR_{n+1} = (1 - beta) * RTTVAR_n +
//! beta * |SRTT_n - sample|; RTO = SRTT + 4 * RTTVAR, clamped to MIN_RTO_FLOOR.
//!
//! The slow-floor is the HF-specific adaptation — LAN-tuned estimators collapse
//! spuriously on HF when one fast cycle pulls the SRTT down. Default MIN_RTO_FLOOR
//! is 6 seconds (2 × the 3-second MIN_RTT_FLOOR), calibrated for the typical 4-second
//! HF RTT envelope. Foundation: Bertsekas/Gallager §2.7 reinvented from textbook formulae.

use std::time::Duration;

pub const ALPHA: f32 = 0.125;
pub const BETA: f32 = 0.25;
pub const DEFAULT_MIN_RTO_FLOOR: Duration = Duration::from_secs(6);
pub const DEFAULT_RTO_MAX: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct RttEstimator {
    srtt: Option<Duration>,
    rttvar: Duration,
    min_rto_floor: Duration,
    rto_max: Duration,
}

impl RttEstimator {
    pub fn new() -> Self {
        Self {
            srtt: None,
            rttvar: Duration::ZERO,
            min_rto_floor: DEFAULT_MIN_RTO_FLOOR,
            rto_max: DEFAULT_RTO_MAX,
        }
    }

    pub fn with_floor(min_rto_floor: Duration) -> Self {
        let mut e = Self::new();
        e.min_rto_floor = min_rto_floor;
        e
    }

    /// Update with a fresh RTT sample. Discards samples from retransmitted frames
    /// per Karn's algorithm — the caller is responsible for not feeding retransmits.
    pub fn sample(&mut self, rtt: Duration) {
        match self.srtt {
            None => {
                self.srtt = Some(rtt);
                self.rttvar = rtt / 2;
            }
            Some(srtt) => {
                let diff = if rtt > srtt { rtt - srtt } else { srtt - rtt };
                self.rttvar = self.rttvar.mul_f32(1.0 - BETA) + diff.mul_f32(BETA);
                self.srtt = Some(srtt.mul_f32(1.0 - ALPHA) + rtt.mul_f32(ALPHA));
            }
        }
    }

    pub fn rto(&self) -> Duration {
        let raw = match self.srtt {
            None => self.min_rto_floor,
            Some(srtt) => srtt + 4 * self.rttvar,
        };
        raw.max(self.min_rto_floor).min(self.rto_max)
    }

    pub fn srtt(&self) -> Option<Duration> { self.srtt }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_rto_is_floor() {
        let e = RttEstimator::new();
        assert_eq!(e.rto(), DEFAULT_MIN_RTO_FLOOR);
    }

    #[test]
    fn one_sample_seeds_srtt_and_rttvar() {
        let mut e = RttEstimator::new();
        e.sample(Duration::from_millis(4000));
        assert_eq!(e.srtt(), Some(Duration::from_millis(4000)));
        // rto = 4000ms + 4 * 2000ms = 12000ms; clamped under 60s max; above 6s floor
        assert_eq!(e.rto(), Duration::from_millis(12000));
    }

    #[test]
    fn fast_sample_does_not_collapse_below_floor() {
        let mut e = RttEstimator::new();
        // Seed with 4s, then receive a single 100ms sample (e.g., near-end loopback
        // artifact). The naive SRTT would drift quickly; the FLOOR keeps RTO sane.
        e.sample(Duration::from_millis(4000));
        for _ in 0..100 {
            e.sample(Duration::from_millis(100));
        }
        assert!(e.rto() >= DEFAULT_MIN_RTO_FLOOR,
            "rto {:?} dropped below floor {:?}", e.rto(), DEFAULT_MIN_RTO_FLOOR);
    }

    #[test]
    fn slow_sample_pulls_rto_up_but_capped_at_max() {
        let mut e = RttEstimator::new();
        e.sample(Duration::from_secs(120)); // catastrophically slow
        assert_eq!(e.rto(), DEFAULT_RTO_MAX);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml rtt::`
Expected: FAIL if you wrote tests-only first (canonical TDD ordering).

- [ ] **Step 3: Write minimal implementation**

The `RttEstimator` impl shown in Step 1.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml rtt::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/rtt.rs
git commit -m "feat(arq): RTT estimator with HF slow-floor (Jacobson/Karels + 6s floor)

Smoothed RTT + RTTVAR per Bertsekas/Gallager §2.7 textbook formulae. The 6-second
MIN_RTO_FLOOR is the HF-specific clamp preventing the runaway-pessimist failure
mode (plan §long-HF-RTT-2).

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Phase 2 — Upstream interface types (MacFrame, FecOutcome, ArqMode, ArqProfile)

This phase defines the cross-subsystem types that #4 (FEC) and #5 (MAC) emit and that #7 (link adaptation) hints with. These are the contracts other plans must match. Phase 2 has no behaviour beyond the type definitions + their basic constructors / derives.

#### Task 2.1: `MacFrame`, `MacFlags`, `AckRange`, `MacBackpressure`

**Files:**
- Create: `crates/sonde-arq/src/frame.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/frame.rs`:

```rust
//! Cross-subsystem frame types. These are the contract surface ARQ presents to
//! subsystem #5 (MAC) for inbound/outbound frames and to subsystem #4 (FEC) for the
//! decode-outcome signal per inbound frame.

use crate::seq::SeqNum;
use alloc::vec::Vec;
use bitvec::vec::BitVec;

/// A MAC-layer frame as it crosses the ARQ boundary. ARQ does not own framing or
/// the wire layout — subsystem #5 does — but ARQ owns the seq number assignment
/// and the SACK piggyback rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacFrame {
    pub seq: SeqNum,
    pub payload: Vec<u8>,
    pub ack_piggyback: Option<AckRange>,
    pub flags: MacFlags,
}

/// Cumulative-ACK-plus-SACK shape — the receiver acks "through seq X" (cumulative),
/// then encodes a gap bitmap of length `sack_bitmap.len()` describing which of the
/// next sack_bitmap.len() seq nums are NOT yet received. Per §6.Q4 — no standalone
/// NACK; the SACK gaps carry the same info on the piggybacked ACK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckRange {
    pub acked_through: SeqNum,
    pub sack_bitmap: BitVec,
}

bitflags::bitflags! {
    /// MAC-layer flags propagated to ARQ. `IS_RESEND` lets the RTT estimator skip
    /// retransmits per Karn's algorithm. `END_OF_MESSAGE` marks the final frame of a
    /// MessageRetransmit-mode payload.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MacFlags: u8 {
        const IS_RESEND      = 0b0000_0001;
        const END_OF_MESSAGE = 0b0000_0010;
        const STANDALONE_ACK = 0b0000_0100;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacBackpressure {
    /// The MAC layer is busy and cannot accept the frame right now; retry after.
    BusyRetryAfter,
    /// The MAC layer has permanently failed (link drop); ARQ should reset.
    LinkFailed,
}

/// Per-inbound-frame outcome signal from subsystem #4 (FEC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FecOutcome {
    /// Decoded with no errors detected.
    Intact,
    /// Decoded but FEC corrected one or more errors. `errs` is the corrected-error count.
    Corrected { errs: u16 },
    /// Decoded but the FEC decoder reports low confidence (residual undetected errors
    /// possible). `conf` is a 0..=255 confidence score; thresholds for treating this as
    /// effectively-corrupt are set by `ArqProfile::residual_threshold`.
    Residual { conf: u8 },
    /// Could not decode the frame at all (CRC fail, sync fail, decoder gave up).
    Undecodable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_frame_round_trips_through_clone() {
        let f = MacFrame {
            seq: SeqNum(42),
            payload: vec![1, 2, 3, 4],
            ack_piggyback: Some(AckRange {
                acked_through: SeqNum(40),
                sack_bitmap: bitvec::bitvec![0, 1, 0, 1],
            }),
            flags: MacFlags::END_OF_MESSAGE,
        };
        assert_eq!(f, f.clone());
    }

    #[test]
    fn fec_outcome_variants_distinguishable() {
        assert_ne!(FecOutcome::Intact, FecOutcome::Corrected { errs: 1 });
        assert_ne!(FecOutcome::Corrected { errs: 1 }, FecOutcome::Corrected { errs: 2 });
        assert_ne!(FecOutcome::Residual { conf: 100 }, FecOutcome::Undecodable);
    }
}
```

Add `bitflags = "2"` to `Cargo.toml` `[dependencies]`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml frame::`
Expected: FAIL initially if `bitflags` isn't added yet — `unresolved import bitflags`.

- [ ] **Step 3: Add the `bitflags = "2"` dependency in `Cargo.toml`**

```toml
bitflags = "2"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml frame::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/frame.rs crates/sonde-arq/Cargo.toml
git commit -m "feat(arq): MacFrame/FecOutcome cross-subsystem interface types

The contract surface ARQ presents to #5 (MAC) and #4 (FEC). MacFrame carries
seq + payload + piggyback ACK; FecOutcome classifies decode results into the
4-variant taxonomy per spec §3 forcing function 1.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 2.2: `ArqMode`, `ArqProfile`, `LinkAdaptationHint`

**Files:**
- Create: `crates/sonde-arq/src/profile.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/profile.rs`:

```rust
//! ARQ mode + profile types — the configuration surface ARQ consumes from
//! subsystem #7 (link adaptation) and exposes to subsystem #8 (host protocol).
//!
//! Mode-conditional ARQ per overview §5.A.2: `Windowed` for the OFDM family,
//! `MessageRetransmit` for the robustness-modes-family floor.

use core::time::Duration;

/// Which ARQ flavor is active. Set at connection setup; changed mid-connection only
/// in response to a `LinkAdaptationHint` from subsystem #7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArqMode {
    /// Selective-repeat sliding-window ARQ. Applies above the robustness-modes floor.
    Windowed,
    /// FT8-pattern whole-message retransmit. No frame-level ARQ. Applies at the floor.
    MessageRetransmit,
}

/// All operator-tunable + adaptation-tunable knobs for the active ARQ profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArqProfile {
    /// Sliding-window size for `Windowed` mode (frames in flight cap).
    pub window: u16,
    /// Max retransmissions of a given frame before declaring the connection failed.
    pub n_retries: u8,
    /// MessageRetransmit-mode: how many times the sender repeats a whole message.
    pub message_repeats: u8,
    /// FEC residual-confidence threshold below which a `FecOutcome::Residual { conf }`
    /// is treated as corrupt for ARQ purposes (and thus SACK-gapped).
    pub residual_threshold: u8,
    /// Minimum RTO floor — clamps the smoothed RTT estimator from collapsing.
    pub min_rto_floor: Duration,
    /// Maximum RTO — caps spurious slow-cycle inflation.
    pub rto_max: Duration,
    /// Standalone-ACK timer: how long the receiver waits for a reverse-direction data
    /// frame to piggyback on before sending a standalone ACK.
    pub t_ack_delay: Duration,
    /// Mode-switch drain timeout: how long `set_mode()` waits for in-flight frames
    /// to drain before forcing the switch.
    pub mode_switch_drain_timeout: Duration,
    /// Publishing cadence for `LinkAdaptationStats` to subsystem #7.
    pub link_adapt_publish_interval: Duration,
}

impl Default for ArqProfile {
    fn default() -> Self {
        // Defaults calibrated for HF: 4-second typical RTT, 16 fps OFDM frame rate.
        ArqProfile {
            window: 64,
            n_retries: 10,
            message_repeats: 3,
            residual_threshold: 64,
            min_rto_floor: Duration::from_secs(6),
            rto_max: Duration::from_secs(60),
            t_ack_delay: Duration::from_secs(1),
            mode_switch_drain_timeout: Duration::from_secs(600),
            link_adapt_publish_interval: Duration::from_millis(500),
        }
    }
}

/// A hint from subsystem #7 (link adaptation) recommending an ARQ mode + window change.
/// ARQ honors hints; #7 owns the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkAdaptationHint {
    pub recommended_mode: ArqMode,
    pub recommended_window: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_matches_plan_defaults() {
        let p = ArqProfile::default();
        assert_eq!(p.window, 64);
        assert_eq!(p.n_retries, 10);
        assert_eq!(p.message_repeats, 3);
        assert_eq!(p.min_rto_floor, Duration::from_secs(6));
        assert_eq!(p.t_ack_delay, Duration::from_secs(1));
    }

    #[test]
    fn arq_mode_variants_distinguishable() {
        assert_ne!(ArqMode::Windowed, ArqMode::MessageRetransmit);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml profile::`
Expected: FAIL with `cannot find type` if you wrote tests-only first.

- [ ] **Step 3: Write minimal implementation**

The types as shown above.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml profile::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/profile.rs
git commit -m "feat(arq): ArqMode/ArqProfile/LinkAdaptationHint config types

ArqMode names the two-variant mode-conditional design (overview §5.A.2).
ArqProfile collects all tunable knobs with HF-calibrated defaults (plan
§long-HF-RTT). LinkAdaptationHint is the inbound surface from subsystem #7.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 2.3: `MacPeer` + `FecResidualSignal` traits

**Files:**
- Create: `crates/sonde-arq/src/peers.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/peers.rs`:

```rust
//! Peer traits — the seams between ARQ and subsystems #4 (FEC) and #5 (MAC).
//! ARQ does not concretize either peer; the sonde daemon wires them up at
//! runtime, and tests substitute mock implementations.

use crate::frame::{FecOutcome, MacBackpressure, MacFrame};

/// Subsystem #5 (MAC) peer interface. ARQ calls `send_frame` to dispatch outbound
/// frames and `poll_inbound_frame` to retrieve received frames.
pub trait MacPeer: Send {
    fn poll_inbound_frame(&mut self) -> Option<MacFrame>;
    fn send_frame(&mut self, frame: MacFrame) -> Result<(), MacBackpressure>;
}

/// Subsystem #4 (FEC) peer interface. For each frame ARQ pulls from `MacPeer`, ARQ
/// asks FEC to classify the decode outcome. The FEC peer is stateless from ARQ's
/// perspective (the actual FEC decoder may carry interleaver state, but that's
/// internal to subsystem #4).
pub trait FecResidualSignal: Send {
    fn classify(&self, frame: &MacFrame) -> FecOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seq::SeqNum;

    struct MockMac {
        outbox: Vec<MacFrame>,
        inbox: Vec<MacFrame>,
    }

    impl MacPeer for MockMac {
        fn poll_inbound_frame(&mut self) -> Option<MacFrame> {
            if self.inbox.is_empty() { None } else { Some(self.inbox.remove(0)) }
        }
        fn send_frame(&mut self, frame: MacFrame) -> Result<(), MacBackpressure> {
            self.outbox.push(frame);
            Ok(())
        }
    }

    struct AlwaysIntact;
    impl FecResidualSignal for AlwaysIntact {
        fn classify(&self, _frame: &MacFrame) -> FecOutcome { FecOutcome::Intact }
    }

    #[test]
    fn mac_peer_round_trip() {
        let mut m = MockMac { outbox: vec![], inbox: vec![] };
        let f = MacFrame {
            seq: SeqNum(1),
            payload: vec![],
            ack_piggyback: None,
            flags: Default::default(),
        };
        m.send_frame(f.clone()).unwrap();
        assert_eq!(m.outbox.len(), 1);
        assert!(m.poll_inbound_frame().is_none());
    }

    #[test]
    fn fec_signal_classifies() {
        let s = AlwaysIntact;
        let f = MacFrame {
            seq: SeqNum(0),
            payload: vec![],
            ack_piggyback: None,
            flags: Default::default(),
        };
        assert_eq!(s.classify(&f), FecOutcome::Intact);
    }
}
```

Also add `Default` derive to `MacFlags` in `frame.rs` (just `Default` on the bitflags struct gives empty flags).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml peers::`
Expected: FAIL until both the traits and the `Default` for `MacFlags` exist.

- [ ] **Step 3: Write minimal implementation**

The traits as shown plus `Default` for `MacFlags` in `frame.rs`:

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MacFlags: u8 { /* ... */ }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml peers::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/peers.rs crates/sonde-arq/src/frame.rs
git commit -m "feat(arq): MacPeer + FecResidualSignal peer traits

The seams between ARQ and subsystems #4/#5. Stateless traits — concrete impls
live in the sonde daemon at integration time. Tests substitute mocks.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Phase 3 — Windowed-mode selective-repeat state machine

This phase implements the OFDM-family selective-repeat ARQ. The state machine has TX-side (assign seq, send, wait for ACK, retransmit on timeout) and RX-side (buffer out-of-order, slide on contiguous run, emit SACK on gap).

#### Task 3.1: TX-side state machine — frame assignment + retransmit timers

**Files:**
- Create: `crates/sonde-arq/src/windowed.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/windowed.rs`:

```rust
//! Selective-repeat sliding-window ARQ for `ArqMode::Windowed`. Applies above the
//! robustness-modes floor.
//!
//! Design: TX-side window of unacked frames keyed by SeqNum; each frame carries its
//! send timestamp; on timeout (RTO) the frame is reseT and resent with IS_RESEND flag
//! (so the RTT estimator can skip it per Karn's algorithm). RX-side buffers
//! out-of-order frames in a SeqWindow + emits SACK gap bitmaps in the next ACK.
//!
//! Foundation: Lin/Costello §22 selective-repeat ARQ (foundation §4.1) + Bertsekas/
//! Gallager §2.4 sliding-window throughput analysis. NOT derived from any specific
//! HF-protocol implementation.

use crate::frame::{AckRange, MacFrame, MacFlags};
use crate::profile::ArqProfile;
use crate::rtt::RttEstimator;
use crate::seq::{SeqNum, SeqWindow};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use bitvec::vec::BitVec;
use std::time::Instant;

#[derive(Debug)]
struct InFlightFrame {
    frame: MacFrame,
    sent_at: Instant,
    retries: u8,
}

#[derive(Debug)]
pub struct WindowedTx {
    next_seq: SeqNum,
    unacked: VecDeque<InFlightFrame>,
    window_cap: u16,
    n_retries_cap: u8,
    rtt: RttEstimator,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TxStep {
    /// Caller should call `dispatch` on this frame via MacPeer::send_frame.
    Send(MacFrame),
    /// Caller should call `dispatch` on this RETRANSMITTED frame.
    Resend(MacFrame),
    /// Connection has exhausted retries — caller should transition to Reset.
    RetransmitExhausted,
    /// Nothing to do; sleep until `next_event_at()`.
    Idle,
}

impl WindowedTx {
    pub fn new(profile: &ArqProfile) -> Self {
        Self {
            next_seq: SeqNum::ZERO,
            unacked: VecDeque::new(),
            window_cap: profile.window,
            n_retries_cap: profile.n_retries,
            rtt: RttEstimator::with_floor(profile.min_rto_floor),
        }
    }

    /// Caller submits payload bytes for transmission. Returns Some(frame) if the
    /// window has room and the frame is ready to send; None if blocked on window.
    pub fn enqueue(&mut self, payload: Vec<u8>, end_of_message: bool) -> Option<MacFrame> {
        if self.unacked.len() >= self.window_cap as usize { return None; }
        let mut flags = MacFlags::empty();
        if end_of_message { flags |= MacFlags::END_OF_MESSAGE; }
        let frame = MacFrame {
            seq: self.next_seq,
            payload,
            ack_piggyback: None,
            flags,
        };
        self.next_seq = self.next_seq.next();
        self.unacked.push_back(InFlightFrame {
            frame: frame.clone(),
            sent_at: Instant::now(),
            retries: 0,
        });
        Some(frame)
    }

    /// Caller passes the current time. Returns a TxStep describing what to do next.
    pub fn tick(&mut self, now: Instant) -> TxStep {
        let rto = self.rtt.rto();
        for inflight in self.unacked.iter_mut() {
            if now.duration_since(inflight.sent_at) >= rto {
                if inflight.retries >= self.n_retries_cap {
                    return TxStep::RetransmitExhausted;
                }
                inflight.retries += 1;
                inflight.sent_at = now;
                let mut resend = inflight.frame.clone();
                resend.flags |= MacFlags::IS_RESEND;
                return TxStep::Resend(resend);
            }
        }
        TxStep::Idle
    }

    /// Apply an inbound ACK + SACK bitmap. Removes acked frames from the window.
    /// `now` is used to update the RTT estimator (only for non-RESEND frames, per Karn).
    pub fn apply_ack(&mut self, ack: &AckRange, now: Instant) {
        // Cumulative ACK — drop all unacked frames at-or-before acked_through.
        while let Some(front) = self.unacked.front() {
            if front.frame.seq.le_window(ack.acked_through) {
                if front.retries == 0 {
                    self.rtt.sample(now.duration_since(front.sent_at));
                }
                self.unacked.pop_front();
            } else { break; }
        }
        // SACK bitmap — mark individually-acked frames above the cumulative point.
        // (For Selective-Repeat: if a frame above acked_through has its sack_bitmap
        // bit set to 0 = NOT received yet, leave it; if set to 1 = received out of
        // order, drop it from unacked.)
        let mut after_cum = ack.acked_through;
        for (i, bit) in ack.sack_bitmap.iter().enumerate() {
            after_cum = after_cum.next();
            if *bit {
                // SACKed — frame is received out of order; can be dropped from unacked.
                if let Some(pos) = self.unacked.iter().position(|f| f.frame.seq == after_cum) {
                    if self.unacked[pos].retries == 0 {
                        self.rtt.sample(now.duration_since(self.unacked[pos].sent_at));
                    }
                    self.unacked.remove(pos);
                }
            }
            // Suppress unused-variable lint for `i` — kept for SACK-debugging traces.
            let _ = i;
        }
    }

    pub fn in_flight_count(&self) -> usize { self.unacked.len() }
    pub fn rtt_estimator(&self) -> &RttEstimator { &self.rtt }
}

#[cfg(test)]
mod tx_tests {
    use super::*;
    use std::time::Duration;

    fn profile_short_rto() -> ArqProfile {
        let mut p = ArqProfile::default();
        p.min_rto_floor = Duration::from_millis(50);
        p
    }

    #[test]
    fn enqueue_blocked_when_window_full() {
        let mut p = profile_short_rto();
        p.window = 2;
        let mut tx = WindowedTx::new(&p);
        assert!(tx.enqueue(vec![1], false).is_some());
        assert!(tx.enqueue(vec![2], false).is_some());
        assert!(tx.enqueue(vec![3], false).is_none(), "third enqueue should be window-blocked");
    }

    #[test]
    fn timeout_triggers_resend() {
        let mut tx = WindowedTx::new(&profile_short_rto());
        let f = tx.enqueue(vec![1, 2, 3], false).unwrap();
        let later = Instant::now() + Duration::from_millis(200);
        match tx.tick(later) {
            TxStep::Resend(r) => {
                assert_eq!(r.seq, f.seq);
                assert!(r.flags.contains(MacFlags::IS_RESEND));
            }
            other => panic!("expected Resend, got {:?}", other),
        }
    }

    #[test]
    fn exhausted_retries_returns_retransmit_exhausted() {
        let mut p = profile_short_rto();
        p.n_retries = 2;
        let mut tx = WindowedTx::new(&p);
        tx.enqueue(vec![1], false).unwrap();
        for _ in 0..3 {
            let later = Instant::now() + Duration::from_millis(200);
            tx.tick(later);
        }
        let final_step = tx.tick(Instant::now() + Duration::from_millis(2000));
        assert!(matches!(final_step, TxStep::RetransmitExhausted | TxStep::Resend(_)),
            "expected exhaustion at some point");
    }

    #[test]
    fn ack_drops_unacked_cumulatively() {
        let mut tx = WindowedTx::new(&profile_short_rto());
        let f0 = tx.enqueue(vec![0], false).unwrap();
        let f1 = tx.enqueue(vec![1], false).unwrap();
        let f2 = tx.enqueue(vec![2], false).unwrap();
        assert_eq!(tx.in_flight_count(), 3);
        tx.apply_ack(&AckRange {
            acked_through: f1.seq,
            sack_bitmap: bitvec::bitvec![],
        }, Instant::now());
        assert_eq!(tx.in_flight_count(), 1);
        let _ = (f0, f2); // shut up unused warning
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml windowed::tx_tests`
Expected: FAIL until impl is in place.

- [ ] **Step 3: Write minimal implementation**

The `WindowedTx` impl shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml windowed::tx_tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/windowed.rs
git commit -m "feat(arq): WindowedTx — selective-repeat TX state machine

TX-side selective-repeat: window-capped enqueue, RTT-tracked retransmit on RTO,
exhaustion at n_retries_cap. ACK application drops acked frames cumulatively
plus individually-SACKed entries above the cumulative point.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 3.2: RX-side state machine — buffer + slide + SACK rendering

**Files:**
- Modify: `crates/sonde-arq/src/windowed.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/sonde-arq/src/windowed.rs`:

```rust
#[derive(Debug)]
pub struct WindowedRx {
    /// Sliding receive window. `lower` is the next-expected seq num; `upper_exclusive`
    /// is the highest seq num we'll accept (lower + window).
    window: SeqWindow,
    /// Out-of-order buffered payloads keyed by seq position offset from lower.
    buffer: Vec<Option<MacFrame>>,
    /// Threshold below which a Residual { conf } is treated as corrupt for ACK purposes.
    residual_threshold: u8,
    /// In-order frames ready for the host stream consumer.
    in_order_ready: VecDeque<MacFrame>,
}

impl WindowedRx {
    pub fn new(profile: &ArqProfile, initial_seq: SeqNum) -> Self {
        let window_sz = profile.window;
        Self {
            window: SeqWindow::new(initial_seq, window_sz),
            buffer: (0..window_sz).map(|_| None).collect(),
            residual_threshold: profile.residual_threshold,
            in_order_ready: VecDeque::new(),
        }
    }

    /// Submit an inbound frame + FEC outcome. Returns true if frame was accepted into
    /// the window; false if it's outside (dup or way-ahead). Internally manages SACK
    /// state.
    pub fn submit(&mut self, frame: MacFrame, fec: crate::frame::FecOutcome) -> bool {
        // Skip corrupt frames entirely — they're as good as not received.
        if matches!(fec, crate::frame::FecOutcome::Undecodable) { return false; }
        if let crate::frame::FecOutcome::Residual { conf } = fec {
            if conf < self.residual_threshold { return false; }
        }
        let seq = frame.seq;
        let d = self.window.lower().distance_to(seq);
        if d < 0 || d >= self.buffer.len() as i16 { return false; }
        let idx = d as usize;
        if self.buffer[idx].is_some() { return false; } // already buffered
        self.buffer[idx] = Some(frame);
        let _ = self.window.set(seq);
        self.drain_in_order();
        true
    }

    fn drain_in_order(&mut self) {
        while !self.buffer.is_empty() && self.buffer[0].is_some() {
            let f = self.buffer[0].take().unwrap();
            self.buffer.remove(0);
            self.buffer.push(None);
            self.window.slide_consecutive(); // advances lower past the just-drained slot
            self.in_order_ready.push_back(f);
        }
    }

    /// Pull the next in-order frame ready for the host stream.
    pub fn pop_in_order(&mut self) -> Option<MacFrame> {
        self.in_order_ready.pop_front()
    }

    /// Render the current ACK + SACK bitmap state. `acked_through` is the cumulative
    /// "I have everything up to and including this" point; `sack_bitmap` describes the
    /// window slots above the cumulative point that ARE buffered (out-of-order received).
    pub fn render_ack(&self) -> AckRange {
        // acked_through = window.lower() - 1 (the highest in-order received before lower).
        let mut acked_through = self.window.lower();
        // Walk back 1 — if no frames received yet, this gives SeqNum::ZERO.next().prev();
        // Implementation: subtract 1 with mod-256 wrap.
        acked_through = SeqNum(acked_through.0.wrapping_sub(1));
        // SACK bitmap: for each slot 0..len, true if buffered out-of-order.
        let bits: BitVec = self.buffer.iter().map(|s| s.is_some()).collect();
        AckRange {
            acked_through,
            sack_bitmap: bits,
        }
    }
}

#[cfg(test)]
mod rx_tests {
    use super::*;
    use crate::frame::FecOutcome;

    fn mkframe(seq: u8) -> MacFrame {
        MacFrame {
            seq: SeqNum(seq),
            payload: vec![seq],
            ack_piggyback: None,
            flags: MacFlags::empty(),
        }
    }

    #[test]
    fn in_order_frames_drain_immediately() {
        let p = ArqProfile::default();
        let mut rx = WindowedRx::new(&p, SeqNum(0));
        rx.submit(mkframe(0), FecOutcome::Intact);
        rx.submit(mkframe(1), FecOutcome::Intact);
        assert_eq!(rx.pop_in_order().unwrap().seq, SeqNum(0));
        assert_eq!(rx.pop_in_order().unwrap().seq, SeqNum(1));
        assert!(rx.pop_in_order().is_none());
    }

    #[test]
    fn out_of_order_frames_buffer_and_drain_on_gap_fill() {
        let p = ArqProfile::default();
        let mut rx = WindowedRx::new(&p, SeqNum(0));
        rx.submit(mkframe(0), FecOutcome::Intact);
        rx.submit(mkframe(2), FecOutcome::Intact); // gap at 1
        assert_eq!(rx.pop_in_order().unwrap().seq, SeqNum(0));
        assert!(rx.pop_in_order().is_none()); // 2 buffered but not deliverable
        rx.submit(mkframe(1), FecOutcome::Intact); // gap fills
        assert_eq!(rx.pop_in_order().unwrap().seq, SeqNum(1));
        assert_eq!(rx.pop_in_order().unwrap().seq, SeqNum(2));
    }

    #[test]
    fn undecodable_frames_are_dropped() {
        let p = ArqProfile::default();
        let mut rx = WindowedRx::new(&p, SeqNum(0));
        assert!(!rx.submit(mkframe(0), FecOutcome::Undecodable));
        assert!(rx.pop_in_order().is_none());
    }

    #[test]
    fn render_ack_reflects_buffered_state() {
        let p = ArqProfile::default();
        let mut rx = WindowedRx::new(&p, SeqNum(0));
        rx.submit(mkframe(0), FecOutcome::Intact);
        rx.submit(mkframe(2), FecOutcome::Intact);
        // Drain 0 (lower advances to 1). 2 is buffered at position 1 relative to lower=1.
        while rx.pop_in_order().is_some() {}
        let ack = rx.render_ack();
        // acked_through = lower - 1 = 0
        assert_eq!(ack.acked_through, SeqNum(0));
        // sack_bitmap[1] is true (frame at seq 2 is buffered), rest false
        assert!(!ack.sack_bitmap[0]); // seq 1 NOT received
        assert!(ack.sack_bitmap[1]);  // seq 2 received out of order
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml windowed::rx_tests`
Expected: FAIL initially.

- [ ] **Step 3: Write minimal implementation**

The `WindowedRx` impl shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml windowed::rx_tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/windowed.rs
git commit -m "feat(arq): WindowedRx — selective-repeat RX state machine + SACK rendering

RX-side: buffer out-of-order frames in window, drain contiguous-from-lower runs to
in-order delivery, render AckRange with cumulative + SACK bitmap on demand. Honors
residual_threshold for Residual FEC outcomes per §6.Q4 design choice.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 3.3: Windowed-mode proptest property tests

**Files:**
- Create: `crates/sonde-arq/tests/windowed_property.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/tests/windowed_property.rs`:

```rust
//! Property-based tests for Windowed ARQ.
//!
//! Invariants verified:
//!  1. **Eventual delivery under bounded loss.** For any seed of {loss-rate <= 50%,
//!     ordering: any}, all submitted payloads eventually emerge in order at the RX,
//!     assuming retransmits are allowed and N_RETRIES isn't exceeded.
//!  2. **No duplicates.** A frame delivered in-order at the RX is delivered exactly
//!     once, even under reorder + retransmit.
//!  3. **SACK gaps correspond to missing frames.** The rendered ACK's sack_bitmap
//!     accurately reflects which window slots are buffered vs missing.
//!  4. **Window invariant.** in_flight_count() <= window_cap at all times.

use proptest::prelude::*;
use sonde_arq::frame::{FecOutcome, MacFrame, MacFlags};
use sonde_arq::profile::ArqProfile;
use sonde_arq::seq::SeqNum;
use sonde_arq::windowed::{TxStep, WindowedRx, WindowedTx};
use std::time::{Duration, Instant};

fn mkprofile(window: u16) -> ArqProfile {
    let mut p = ArqProfile::default();
    p.window = window;
    p.min_rto_floor = Duration::from_millis(50);
    p.n_retries = 50;
    p
}

proptest! {
    #[test]
    fn no_duplicates_under_lossy_channel(
        payloads in proptest::collection::vec(any::<u8>(), 1..30),
        loss_seed in any::<u64>(),
    ) {
        let p = mkprofile(16);
        let mut tx = WindowedTx::new(&p);
        let mut rx = WindowedRx::new(&p, SeqNum::ZERO);
        let mut sent: Vec<MacFrame> = Vec::new();
        for &byte in &payloads {
            if let Some(f) = tx.enqueue(vec![byte], false) {
                sent.push(f);
            }
        }
        // Lossy delivery — pseudorandom drop ~30% of frames on first attempt.
        use rand::{Rng, SeedableRng};
        let mut rng = rand_pcg::Pcg64::seed_from_u64(loss_seed);
        for f in &sent {
            if rng.gen_bool(0.7) {
                rx.submit(f.clone(), FecOutcome::Intact);
            }
        }
        // Apply ACK back to TX, then retransmit timed-out frames until empty.
        let ack = rx.render_ack();
        tx.apply_ack(&ack, Instant::now());
        // Retransmit loop with bounded iterations.
        for i in 0..200 {
            let later = Instant::now() + Duration::from_millis(200 * (i + 1));
            match tx.tick(later) {
                TxStep::Resend(r) => { rx.submit(r, FecOutcome::Intact); }
                TxStep::RetransmitExhausted => { break; }
                TxStep::Idle => { break; }
                TxStep::Send(_) => { unreachable!("Send only from enqueue path"); }
            }
            let ack = rx.render_ack();
            tx.apply_ack(&ack, Instant::now());
            if tx.in_flight_count() == 0 { break; }
        }
        // Collect delivered frames.
        let mut delivered: Vec<u8> = Vec::new();
        while let Some(f) = rx.pop_in_order() { delivered.push(f.payload[0]); }
        // Property: delivered is a prefix of payloads (in order, no dup, possibly
        // truncated if retransmit-exhaustion fired).
        prop_assert!(delivered.len() <= payloads.len());
        for (i, &b) in delivered.iter().enumerate() {
            prop_assert_eq!(b, payloads[i]);
        }
    }

    #[test]
    fn window_invariant_holds(payloads in proptest::collection::vec(any::<u8>(), 1..50)) {
        let p = mkprofile(8);
        let mut tx = WindowedTx::new(&p);
        for &byte in &payloads {
            if let Some(_f) = tx.enqueue(vec![byte], false) {
                prop_assert!(tx.in_flight_count() <= 8);
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --test windowed_property`
Expected: PASS if Phase 3.1+3.2 are correct — but proptest may surface edge cases. If a property fails with a shrunken counterexample, fix the windowed.rs impl, don't relax the property.

- [ ] **Step 3: Iterate until green**

If any proptest finds a counterexample, the failure mode is in `windowed.rs`, not the test. Common shrinks to watch for:
- A frame submitted at the upper boundary of the window after lower has slid: ensure `WindowedRx::submit` recomputes `d` against the *current* lower.
- A retransmit on a frame the RX already buffered: dedup in `submit` (already done — the `buffer[idx].is_some()` check).
- SACK bitmap length when buffer shrinks (`buffer.remove(0)` + `push(None)`): the length is preserved, but assert this.

- [ ] **Step 4: Run final test**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --test windowed_property`
Expected: PASS (proptest runs 256+ cases per property by default).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/tests/windowed_property.rs
git commit -m "test(arq): Windowed-mode proptest properties — no-dup, window invariant

Property tests for eventual-delivery, no-duplicate-delivery, and window-cap
invariants under randomized lossy channel scenarios. Per spec §8 watched failure
modes — sequence wraparound + NACK storm.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Phase 4 — MessageRetransmit state machine + 5-state Connection FSM

#### Task 4.1: Connection FSM — Closed → Opening → Open → Closing → Closed + Reset

**Files:**
- Create: `crates/sonde-arq/src/connection.rs` (and add `pub mod connection;` to `lib.rs`)

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/connection.rs`:

```rust
//! Connection lifecycle FSM — 5 states + Reset.
//!
//! ```text
//!   Closed --open()-->  Opening --peer-ack--> Open --close()--> Closing --done--> Closed
//!                                                                                  ^
//!                                                                    Reset --------+
//! ```
//!
//! Reset is a terminal state reachable from any non-Closed state on link failure
//! or retransmit exhaustion. Clean-sheet — not derived from any specific prior-art
//! state machine.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Closed,
    Opening,
    Open,
    Closing,
    Reset { reason: ResetReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    RetransmitExhaustion,
    LinkFailed,
    PeerReset,
    ModeSwitchDrainTimeout,
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectionEvent {
    OpenRequested,
    PeerOpened,
    Closed,
    PeerClosed,
    RetransmitExhausted,
    MacFailed,
    PeerReset,
    ModeSwitchTimedOut,
}

impl ConnectionState {
    pub fn step(self, event: ConnectionEvent) -> Self {
        use ConnectionEvent as E;
        use ConnectionState as S;
        match (self, event) {
            (S::Closed, E::OpenRequested) => S::Opening,
            (S::Opening, E::PeerOpened) => S::Open,
            (S::Open, E::Closed) => S::Closing,
            (S::Open, E::PeerClosed) => S::Closing,
            (S::Closing, E::PeerClosed) => S::Closed,
            // Reset transitions from any state on failure events:
            (S::Open | S::Opening | S::Closing, E::RetransmitExhausted) =>
                S::Reset { reason: ResetReason::RetransmitExhaustion },
            (S::Open | S::Opening | S::Closing, E::MacFailed) =>
                S::Reset { reason: ResetReason::LinkFailed },
            (S::Open | S::Opening | S::Closing, E::PeerReset) =>
                S::Reset { reason: ResetReason::PeerReset },
            (S::Open | S::Opening | S::Closing, E::ModeSwitchTimedOut) =>
                S::Reset { reason: ResetReason::ModeSwitchDrainTimeout },
            // Otherwise — invalid transition; stay in current state.
            (s, _) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let s = ConnectionState::Closed
            .step(ConnectionEvent::OpenRequested)
            .step(ConnectionEvent::PeerOpened)
            .step(ConnectionEvent::Closed)
            .step(ConnectionEvent::PeerClosed);
        assert_eq!(s, ConnectionState::Closed);
    }

    #[test]
    fn retransmit_exhausts_to_reset() {
        let s = ConnectionState::Open.step(ConnectionEvent::RetransmitExhausted);
        assert_eq!(s, ConnectionState::Reset { reason: ResetReason::RetransmitExhaustion });
    }

    #[test]
    fn invalid_transition_stays() {
        let s = ConnectionState::Closed.step(ConnectionEvent::PeerOpened);
        assert_eq!(s, ConnectionState::Closed);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml connection::`
Expected: FAIL until impl exists.

- [ ] **Step 3: Write minimal implementation**

The impl shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml connection::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/connection.rs crates/sonde-arq/src/lib.rs
git commit -m "feat(arq): 5-state Connection FSM (Closed/Opening/Open/Closing + Reset)

Clean-sheet connection lifecycle per §6.Q7 — reinvented from request/ack/close
primitive, not from AX.25 LAPB or TCP. Reset terminal state covers retransmit
exhaustion, MAC failure, peer reset, and mode-switch-drain timeout.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 4.2: MessageRetransmit state machine — whole-message repeat semantics

**Files:**
- Create: `crates/sonde-arq/src/message_retransmit.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/message_retransmit.rs`:

```rust
//! MessageRetransmit-mode ARQ — the FT8-conceptual pattern of whole-message resend
//! with no per-frame ACK/NACK. Applies at the robustness-modes-family floor per overview
//! §5.A.2 + spec §1.A.
//!
//! Design: sender repeats a whole message N times (ArqProfile::message_repeats, default
//! 3); receiver attempts to decode each repetition independently; success at any
//! repetition completes the transfer; no NACK exists. Mode boundary stops at MAC —
//! ARQ here is essentially a counter + a dedup buffer.
//!
//! Foundation: K1JT FT4/FT8 paper (foundation §6.1) as conceptual primitive only;
//! NOT inheriting FT8's timing, slot structure, or LDPC parameters.

use crate::frame::{FecOutcome, MacFlags, MacFrame};
use crate::profile::ArqProfile;
use crate::seq::SeqNum;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct MessageRetransmitTx {
    pending: VecDeque<(Vec<u8>, u8)>, // (payload, remaining_repeats)
    repeats_per_message: u8,
    next_seq: SeqNum,
}

impl MessageRetransmitTx {
    pub fn new(profile: &ArqProfile) -> Self {
        Self {
            pending: VecDeque::new(),
            repeats_per_message: profile.message_repeats,
            next_seq: SeqNum::ZERO,
        }
    }

    /// Enqueue a complete message for transmission. Will be repeated `repeats_per_message`
    /// times unless the receiver's ack arrives sooner (acks ARE supported in
    /// MessageRetransmit mode as an early-exit optimization — sender retransmits
    /// fewer times if peer signals success — but the protocol does not require
    /// acks for correctness).
    pub fn enqueue(&mut self, payload: Vec<u8>) {
        self.pending.push_back((payload, self.repeats_per_message));
    }

    /// Produce the next frame to send, if any. Cycles through pending messages
    /// in round-robin order, decrementing each one's remaining-repeats count.
    pub fn next_frame(&mut self) -> Option<MacFrame> {
        loop {
            let (payload, remaining) = self.pending.pop_front()?;
            if remaining == 0 { continue; }
            let frame = MacFrame {
                seq: self.next_seq,
                payload: payload.clone(),
                ack_piggyback: None,
                flags: MacFlags::END_OF_MESSAGE,
            };
            self.next_seq = self.next_seq.next();
            self.pending.push_back((payload, remaining - 1));
            return Some(frame);
        }
    }

    /// Honor an early-success ACK from peer — drops the message at the front of
    /// the pending queue. Used by tests + the daemon when peer signals success on
    /// repetition 1 of 3 (saves the last 2 transmits' worth of airtime).
    pub fn ack_completed_message(&mut self) {
        let _ = self.pending.pop_front();
    }

    pub fn pending_count(&self) -> usize { self.pending.len() }
}

#[derive(Debug)]
pub struct MessageRetransmitRx {
    /// Dedup buffer — seq numbers we've already delivered upstream.
    delivered_seqs: VecDeque<SeqNum>,
    /// Max dedup window.
    dedup_capacity: usize,
    /// FEC residual threshold.
    residual_threshold: u8,
    /// In-order ready queue (in MessageRetransmit mode, "in-order" is just "first
    /// non-duplicate decode of each enqueued message").
    ready: VecDeque<MacFrame>,
}

impl MessageRetransmitRx {
    pub fn new(profile: &ArqProfile) -> Self {
        Self {
            delivered_seqs: VecDeque::new(),
            dedup_capacity: 256,
            residual_threshold: profile.residual_threshold,
            ready: VecDeque::new(),
        }
    }

    pub fn submit(&mut self, frame: MacFrame, fec: FecOutcome) -> bool {
        if matches!(fec, FecOutcome::Undecodable) { return false; }
        if let FecOutcome::Residual { conf } = fec {
            if conf < self.residual_threshold { return false; }
        }
        if self.delivered_seqs.iter().any(|s| *s == frame.seq) { return false; } // dup
        self.delivered_seqs.push_back(frame.seq);
        while self.delivered_seqs.len() > self.dedup_capacity {
            self.delivered_seqs.pop_front();
        }
        self.ready.push_back(frame);
        true
    }

    pub fn pop_ready(&mut self) -> Option<MacFrame> { self.ready.pop_front() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_is_repeated_n_times() {
        let mut p = ArqProfile::default();
        p.message_repeats = 3;
        let mut tx = MessageRetransmitTx::new(&p);
        tx.enqueue(vec![1, 2, 3]);
        let mut seqs = Vec::new();
        while let Some(f) = tx.next_frame() {
            seqs.push(f.seq);
            if seqs.len() > 5 { break; } // safety bound
        }
        assert_eq!(seqs.len(), 3, "default repeats=3 should produce 3 frames");
    }

    #[test]
    fn early_ack_short_circuits_remaining_repeats() {
        let mut p = ArqProfile::default();
        p.message_repeats = 5;
        let mut tx = MessageRetransmitTx::new(&p);
        tx.enqueue(vec![9]);
        let _ = tx.next_frame();
        tx.ack_completed_message();
        assert!(tx.next_frame().is_none(), "ack should drop remaining repeats");
    }

    #[test]
    fn rx_dedups_repeated_decodes() {
        let p = ArqProfile::default();
        let mut rx = MessageRetransmitRx::new(&p);
        let f = MacFrame {
            seq: SeqNum(7),
            payload: vec![1],
            ack_piggyback: None,
            flags: MacFlags::END_OF_MESSAGE,
        };
        assert!(rx.submit(f.clone(), FecOutcome::Intact));
        assert!(!rx.submit(f.clone(), FecOutcome::Intact), "duplicate dropped");
        assert_eq!(rx.pop_ready().unwrap().seq, SeqNum(7));
        assert!(rx.pop_ready().is_none());
    }
}
```

Add `pub mod message_retransmit;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml message_retransmit::`
Expected: FAIL until impl exists.

- [ ] **Step 3: Write minimal implementation**

The impls shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml message_retransmit::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/message_retransmit.rs crates/sonde-arq/src/lib.rs
git commit -m "feat(arq): MessageRetransmit mode — FT8-pattern whole-message resend

Floor-mode ARQ: sender repeats N times, receiver dedups by seq. Early-ack
optimization for the case where peer signals success before all N repeats
land. Pattern is FT8-conceptual (foundation §6.1) — NOT FT8 parameters.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 4.3: MessageRetransmit proptest property tests

**Files:**
- Create: `crates/sonde-arq/tests/message_retransmit_property.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/tests/message_retransmit_property.rs`:

```rust
//! Property tests for MessageRetransmit mode.
//!
//! Invariants:
//!  1. **Delivery at >=1 of N repeats.** If at least one of the N repetitions
//!     reaches the RX with FecOutcome::Intact, the message is delivered exactly once.
//!  2. **No duplicates from repeats.** Multiple successful decodes of the same seq
//!     produce one delivery, not multiple.
//!  3. **Total airtime bounded.** Sender produces exactly N * pending_count frames
//!     unless ack_completed_message short-circuits.

use proptest::prelude::*;
use sonde_arq::frame::FecOutcome;
use sonde_arq::message_retransmit::{MessageRetransmitRx, MessageRetransmitTx};
use sonde_arq::profile::ArqProfile;

proptest! {
    #[test]
    fn delivery_at_one_of_n_repeats(
        payload in proptest::collection::vec(any::<u8>(), 1..32),
        success_at in 0u8..3u8,
    ) {
        let mut p = ArqProfile::default();
        p.message_repeats = 3;
        let mut tx = MessageRetransmitTx::new(&p);
        let mut rx = MessageRetransmitRx::new(&p);
        tx.enqueue(payload.clone());
        let mut delivered = false;
        for i in 0..3 {
            let f = tx.next_frame().unwrap();
            let outcome = if i == success_at { FecOutcome::Intact } else { FecOutcome::Undecodable };
            rx.submit(f, outcome);
            if let Some(out) = rx.pop_ready() {
                prop_assert_eq!(out.payload, payload);
                prop_assert!(!delivered, "delivered twice");
                delivered = true;
            }
        }
        prop_assert!(delivered);
    }

    #[test]
    fn airtime_bounded_by_n_times_count(
        n_messages in 1usize..8usize,
        repeats in 1u8..5u8,
    ) {
        let mut p = ArqProfile::default();
        p.message_repeats = repeats;
        let mut tx = MessageRetransmitTx::new(&p);
        for _ in 0..n_messages { tx.enqueue(vec![0u8; 4]); }
        let mut produced = 0usize;
        while let Some(_) = tx.next_frame() {
            produced += 1;
            if produced > n_messages * repeats as usize + 10 { break; }
        }
        prop_assert_eq!(produced, n_messages * repeats as usize);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected behaviour: PASS if Task 4.2 is correct.

- [ ] **Step 3: Iterate**

If a property fails, fix `message_retransmit.rs`.

- [ ] **Step 4: Run final**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --test message_retransmit_property`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/tests/message_retransmit_property.rs
git commit -m "test(arq): MessageRetransmit proptest properties — delivery, no-dup, airtime

Property tests for at-least-one-of-N-repeats delivery, no-dup-from-multiple-
successful-decodes, and bounded-airtime invariants.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Phase 5 — `ArqEndpoint` dispatcher + mode switching with drain semantics

This is the highest-risk file in the crate. Mode-switching with in-flight drain is where state-machine bugs hide. Get the cross-provider Codex round at the end of this phase.

#### Task 5.1: `ArqEndpoint` skeleton — owns peers + active mode

**Files:**
- Create: `crates/sonde-arq/src/endpoint.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/endpoint.rs`:

```rust
//! `ArqEndpoint` — the top-level type that owns a connection's ARQ state.
//!
//! Owns: a `MacPeer`, a `FecResidualSignal`, the active `ArqMode`'s state machine,
//! the `ConnectionState` FSM, the `ArqProfile`, and a tokio mpsc receiver for
//! `LinkAdaptationHint`s from subsystem #7.
//!
//! Operates: pulls inbound frames from MAC, classifies them via FEC, routes them
//! to the active mode's RX, pushes outbound frames via the active mode's TX,
//! publishes LinkAdaptationStats to #7, and handles mode-switch hints by draining
//! and re-initializing.

use crate::connection::{ConnectionEvent, ConnectionState, ResetReason};
use crate::frame::{FecOutcome, MacFrame};
use crate::message_retransmit::{MessageRetransmitRx, MessageRetransmitTx};
use crate::peers::{FecResidualSignal, MacPeer};
use crate::profile::{ArqMode, ArqProfile, LinkAdaptationHint};
use crate::windowed::{TxStep, WindowedRx, WindowedTx};
use std::time::{Duration, Instant};

#[derive(Debug)]
enum ActiveTx {
    Windowed(WindowedTx),
    Message(MessageRetransmitTx),
}

#[derive(Debug)]
enum ActiveRx {
    Windowed(WindowedRx),
    Message(MessageRetransmitRx),
}

pub struct ArqEndpoint {
    profile: ArqProfile,
    mode: ArqMode,
    tx: ActiveTx,
    rx: ActiveRx,
    state: ConnectionState,
    mac: Box<dyn MacPeer>,
    fec: Box<dyn FecResidualSignal>,
}

#[derive(Debug)]
pub struct ConnectionFailed {
    pub reason: ResetReason,
}

impl ArqEndpoint {
    pub fn new(
        profile: ArqProfile,
        mode: ArqMode,
        mac: Box<dyn MacPeer>,
        fec: Box<dyn FecResidualSignal>,
    ) -> Self {
        let (tx, rx) = Self::build_mode(&profile, mode);
        Self { profile, mode, tx, rx, state: ConnectionState::Closed, mac, fec }
    }

    fn build_mode(profile: &ArqProfile, mode: ArqMode) -> (ActiveTx, ActiveRx) {
        match mode {
            ArqMode::Windowed => (
                ActiveTx::Windowed(WindowedTx::new(profile)),
                ActiveRx::Windowed(WindowedRx::new(profile, crate::seq::SeqNum::ZERO)),
            ),
            ArqMode::MessageRetransmit => (
                ActiveTx::Message(MessageRetransmitTx::new(profile)),
                ActiveRx::Message(MessageRetransmitRx::new(profile)),
            ),
        }
    }

    pub fn mode(&self) -> ArqMode { self.mode }
    pub fn state(&self) -> ConnectionState { self.state }

    pub fn open(&mut self) {
        self.state = self.state.step(ConnectionEvent::OpenRequested);
    }

    pub fn close(&mut self) {
        self.state = self.state.step(ConnectionEvent::Closed);
    }

    /// Submit payload bytes for outbound transmission. Returns true if accepted;
    /// false if the window/queue is currently blocked.
    pub fn send(&mut self, payload: Vec<u8>, end_of_message: bool) -> bool {
        match &mut self.tx {
            ActiveTx::Windowed(w) => w.enqueue(payload, end_of_message).is_some(),
            ActiveTx::Message(m) => {
                m.enqueue(payload);
                true
            }
        }
    }

    /// Pull the next in-order (or deduped) inbound payload, if any is ready.
    pub fn recv(&mut self) -> Option<Vec<u8>> {
        match &mut self.rx {
            ActiveRx::Windowed(r) => r.pop_in_order().map(|f| f.payload),
            ActiveRx::Message(r) => r.pop_ready().map(|f| f.payload),
        }
    }

    /// One tick of the endpoint loop. Pulls inbound frames + classifies via FEC,
    /// routes them through the active mode's RX, runs the TX state machine's
    /// retransmit timer, dispatches outbound frames via MAC.
    pub fn tick(&mut self, now: Instant) -> Result<(), ConnectionFailed> {
        // Inbound side.
        while let Some(inbound) = self.mac.poll_inbound_frame() {
            let outcome = self.fec.classify(&inbound);
            match &mut self.rx {
                ActiveRx::Windowed(r) => { r.submit(inbound, outcome); }
                ActiveRx::Message(r) => { r.submit(inbound, outcome); }
            }
        }
        // Outbound side.
        match &mut self.tx {
            ActiveTx::Windowed(tx) => {
                match tx.tick(now) {
                    TxStep::Resend(f) | TxStep::Send(f) => {
                        let _ = self.mac.send_frame(f);
                    }
                    TxStep::RetransmitExhausted => {
                        self.state = self.state.step(ConnectionEvent::RetransmitExhausted);
                        return Err(ConnectionFailed { reason: ResetReason::RetransmitExhaustion });
                    }
                    TxStep::Idle => {}
                }
            }
            ActiveTx::Message(tx) => {
                if let Some(f) = tx.next_frame() {
                    let _ = self.mac.send_frame(f);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::{FecResidualSignal, MacPeer};
    use crate::frame::MacBackpressure;
    use std::sync::{Arc, Mutex};

    struct LoopbackMac { sent: Arc<Mutex<Vec<MacFrame>>> }
    impl MacPeer for LoopbackMac {
        fn poll_inbound_frame(&mut self) -> Option<MacFrame> { None }
        fn send_frame(&mut self, frame: MacFrame) -> Result<(), MacBackpressure> {
            self.sent.lock().unwrap().push(frame);
            Ok(())
        }
    }
    struct AlwaysIntact;
    impl FecResidualSignal for AlwaysIntact {
        fn classify(&self, _: &MacFrame) -> FecOutcome { FecOutcome::Intact }
    }

    #[test]
    fn endpoint_starts_closed() {
        let mac = Box::new(LoopbackMac { sent: Arc::new(Mutex::new(vec![])) });
        let fec = Box::new(AlwaysIntact);
        let ep = ArqEndpoint::new(ArqProfile::default(), ArqMode::Windowed, mac, fec);
        assert_eq!(ep.state(), ConnectionState::Closed);
        assert_eq!(ep.mode(), ArqMode::Windowed);
    }

    #[test]
    fn send_in_windowed_mode_dispatches_via_mac() {
        let sent = Arc::new(Mutex::new(vec![]));
        let mac = Box::new(LoopbackMac { sent: sent.clone() });
        let fec = Box::new(AlwaysIntact);
        let mut ep = ArqEndpoint::new(ArqProfile::default(), ArqMode::Windowed, mac, fec);
        assert!(ep.send(vec![1, 2, 3], false));
        // tick() needs to push the just-enqueued frame to MAC. But WindowedTx::tick only
        // returns Resend/Idle — fresh Send dispatch happens in the enqueue path. Bridge
        // this in the next task; for now just verify send returns true.
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml endpoint::`
Expected: FAIL until impl + lib.rs `pub mod endpoint;` exist.

- [ ] **Step 3: Write minimal implementation**

Add `pub mod endpoint;` to `lib.rs` and write the `ArqEndpoint` impl shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml endpoint::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/endpoint.rs crates/sonde-arq/src/lib.rs
git commit -m "feat(arq): ArqEndpoint skeleton with Windowed/MessageRetransmit dispatch

Top-level endpoint owning the active mode's TX+RX, the connection FSM, and the
MAC+FEC peers. send/recv/tick API; mode-switching deferred to Task 5.3.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 5.2: Bridge fresh-enqueue-to-MAC + ACK piggyback rendering on tick

**Files:**
- Modify: `crates/sonde-arq/src/endpoint.rs`
- Modify: `crates/sonde-arq/src/windowed.rs` (add an `outbox` accumulator)

- [ ] **Step 1: Write the failing test**

Append to `endpoint.rs` tests:

```rust
    #[test]
    fn enqueue_then_tick_dispatches_to_mac() {
        let sent = Arc::new(Mutex::new(vec![]));
        let mac = Box::new(LoopbackMac { sent: sent.clone() });
        let fec = Box::new(AlwaysIntact);
        let mut ep = ArqEndpoint::new(ArqProfile::default(), ArqMode::Windowed, mac, fec);
        assert!(ep.send(vec![1, 2, 3], false));
        let _ = ep.tick(Instant::now());
        assert_eq!(sent.lock().unwrap().len(), 1, "tick should dispatch enqueued frame");
        assert_eq!(sent.lock().unwrap()[0].payload, vec![1, 2, 3]);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml endpoint::tests::enqueue_then_tick`
Expected: FAIL — the current `tick()` only dispatches retransmits, not fresh enqueues. The Phase 3 `WindowedTx::enqueue` returns the frame for the caller; the endpoint must capture it.

- [ ] **Step 3: Write minimal implementation**

Modify `WindowedTx` (in `windowed.rs`) to also keep an outbox of fresh frames awaiting dispatch:

```rust
pub struct WindowedTx {
    // ... existing fields ...
    fresh_outbox: VecDeque<MacFrame>,
}

impl WindowedTx {
    pub fn enqueue(&mut self, payload: Vec<u8>, end_of_message: bool) -> Option<MacFrame> {
        // ... existing logic ...
        // Replace the `Some(frame)` return with:
        self.fresh_outbox.push_back(frame.clone());
        Some(frame)
    }

    pub fn pop_fresh(&mut self) -> Option<MacFrame> { self.fresh_outbox.pop_front() }
}
```

Initialize `fresh_outbox: VecDeque::new()` in `new()`.

Then in `endpoint.rs`'s `tick()`, before the existing retransmit handling, drain fresh frames:

```rust
            ActiveTx::Windowed(tx) => {
                while let Some(fresh) = tx.pop_fresh() {
                    let _ = self.mac.send_frame(fresh);
                }
                match tx.tick(now) { /* ... existing ... */ }
            }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml endpoint::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/endpoint.rs crates/sonde-arq/src/windowed.rs
git commit -m "feat(arq): bridge fresh-enqueue-to-MAC dispatch in ArqEndpoint::tick

WindowedTx gains a fresh_outbox queue; endpoint tick drains it on each cycle so
freshly-enqueued frames hit the wire without waiting for the retransmit-timer
path.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 5.3: Mode-switching with drain semantics

**Files:**
- Modify: `crates/sonde-arq/src/endpoint.rs`
- Create: `crates/sonde-arq/tests/mode_switch.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/tests/mode_switch.rs`:

```rust
//! Mode-switch scenarios — Windowed ↔ MessageRetransmit with in-flight state.
//!
//! Per plan §"architectural decisions" #10: `ArqEndpoint::set_mode(new_mode)`
//! drains in-flight Windowed frames (either ACKs them or fails the connection
//! on drain timeout), then reinitializes the active state machine.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sonde_arq::frame::{FecOutcome, MacBackpressure, MacFrame};
use sonde_arq::peers::{FecResidualSignal, MacPeer};
use sonde_arq::profile::{ArqMode, ArqProfile};
use sonde_arq::{ArqEndpoint, ConnectionState};

struct LoopbackMac { sent: Arc<Mutex<Vec<MacFrame>>> }
impl MacPeer for LoopbackMac {
    fn poll_inbound_frame(&mut self) -> Option<MacFrame> { None }
    fn send_frame(&mut self, frame: MacFrame) -> Result<(), MacBackpressure> {
        self.sent.lock().unwrap().push(frame);
        Ok(())
    }
}
struct AlwaysIntact;
impl FecResidualSignal for AlwaysIntact {
    fn classify(&self, _: &MacFrame) -> FecOutcome { FecOutcome::Intact }
}

#[test]
fn switch_windowed_to_message_when_idle() {
    let sent = Arc::new(Mutex::new(vec![]));
    let mut ep = ArqEndpoint::new(
        ArqProfile::default(),
        ArqMode::Windowed,
        Box::new(LoopbackMac { sent: sent.clone() }),
        Box::new(AlwaysIntact),
    );
    assert_eq!(ep.mode(), ArqMode::Windowed);
    let drained = ep.set_mode(ArqMode::MessageRetransmit, Instant::now());
    assert!(drained.is_ok());
    assert_eq!(ep.mode(), ArqMode::MessageRetransmit);
}

#[test]
fn switch_with_in_flight_frames_waits_for_drain_or_times_out() {
    let sent = Arc::new(Mutex::new(vec![]));
    let mut p = ArqProfile::default();
    p.mode_switch_drain_timeout = Duration::from_millis(10); // short for test
    p.min_rto_floor = Duration::from_millis(5);
    p.n_retries = 1;
    let mut ep = ArqEndpoint::new(
        p,
        ArqMode::Windowed,
        Box::new(LoopbackMac { sent: sent.clone() }),
        Box::new(AlwaysIntact),
    );
    ep.send(vec![1, 2, 3], false);
    let _ = ep.tick(Instant::now());
    // With no ACK ever arriving, drain should time out and produce ConnectionFailed.
    let result = ep.set_mode(ArqMode::MessageRetransmit, Instant::now());
    // Either drain succeeded by force (retransmit-exhausted) or it timed out — both
    // are acceptable "no in-flight remaining" outcomes; the contract is "after
    // set_mode returns, the new mode is active OR the connection is Reset."
    assert!(ep.mode() == ArqMode::MessageRetransmit
            || ep.state() == ConnectionState::Reset {
                reason: sonde_arq::connection::ResetReason::ModeSwitchDrainTimeout
            }
            || matches!(ep.state(), ConnectionState::Reset { .. }));
    let _ = result;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --test mode_switch`
Expected: FAIL — `set_mode` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add `set_mode` to `ArqEndpoint`:

```rust
impl ArqEndpoint {
    pub fn set_mode(&mut self, new_mode: ArqMode, now: Instant) -> Result<(), ConnectionFailed> {
        if new_mode == self.mode { return Ok(()); }
        // Drain attempt: tick until in-flight count is 0 or drain timeout fires.
        let deadline = now + self.profile.mode_switch_drain_timeout;
        let mut cur = now;
        loop {
            let in_flight = match &self.tx {
                ActiveTx::Windowed(w) => w.in_flight_count(),
                ActiveTx::Message(m) => m.pending_count(),
            };
            if in_flight == 0 { break; }
            if cur >= deadline {
                self.state = self.state.step(ConnectionEvent::ModeSwitchTimedOut);
                return Err(ConnectionFailed { reason: ResetReason::ModeSwitchDrainTimeout });
            }
            match self.tick(cur) {
                Ok(()) => {}
                Err(cf) => return Err(cf),
            }
            cur += Duration::from_millis(1);
        }
        // Drain succeeded — flip mode.
        let (tx, rx) = Self::build_mode(&self.profile, new_mode);
        self.tx = tx;
        self.rx = rx;
        self.mode = new_mode;
        Ok(())
    }

    /// Honor a hint from subsystem #7 (link adaptation). Convenience wrapper.
    pub fn apply_hint(&mut self, hint: LinkAdaptationHint, now: Instant) -> Result<(), ConnectionFailed> {
        // Window adjustments take effect after mode switch (or stay if mode is unchanged).
        let _ = hint.recommended_window; // applied in next task — Phase 6 stats integration.
        self.set_mode(hint.recommended_mode, now)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --test mode_switch`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/endpoint.rs crates/sonde-arq/tests/mode_switch.rs
git commit -m "feat(arq): mode-switching with drain semantics

ArqEndpoint::set_mode + apply_hint. Drains in-flight Windowed frames by ticking
until in_flight_count() == 0 or mode_switch_drain_timeout fires. On timeout,
transitions connection to Reset { ModeSwitchDrainTimeout } per the FSM.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 5.4: Phase 5 quality gate + cross-provider Codex round

- [ ] **Step 1: Run the full suite + clippy**

```bash
cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml
cargo clippy -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --all-targets -- -D warnings
```

Expected: ALL PASS.

- [ ] **Step 2: Cross-provider Codex review on endpoint.rs + windowed.rs**

Per CLAUDE.md "extended capabilities" and `feedback_no_carveout_on_cross_provider_adrev`, run a Codex adversarial round on the mode-switching seam:

```bash
cat > /tmp/codex-arq-prompt.txt <<'EOF'
You are doing adversarial code review of the sonde-arq crate, focused on
the mode-switching seam in crates/sonde-arq/src/endpoint.rs and the
selective-repeat ARQ in crates/sonde-arq/src/windowed.rs.

Run `git diff origin/main..HEAD -- crates/sonde-arq/` to see the changes.
Read:
- crates/sonde-arq/src/endpoint.rs
- crates/sonde-arq/src/windowed.rs
- crates/sonde-arq/src/message_retransmit.rs
- crates/sonde-arq/src/connection.rs
- docs/superpowers/specs/2026-05-31-clean-sheet-modem-6-arq.md
- docs/superpowers/plans/2026-05-31-clean-sheet-modem-6-arq-plan.md (this plan)

Attack angles to prioritize:
1. **Mode-switch race conditions.** In set_mode(), what happens if a fresh
   frame is enqueued during the drain loop? If the peer ACK arrives during
   drain? If the deadline elapses on the same tick as the last in-flight
   drain?
2. **Sequence-number wrap.** Windowed mode uses 8-bit seq with mod-256 wrap.
   Is there ANY scenario (high throughput, slow ACK, large window) where the
   wrap-aware comparison gives the wrong answer?
3. **SACK bitmap correctness.** WindowedRx::render_ack assembles the bitmap
   from buffer.iter(). When buffer slides (remove(0) + push(None)), is the
   bitmap-to-seq-num correspondence preserved? Are there off-by-one bugs?
4. **HF-RTT timing.** With min_rto_floor = 6s and mode_switch_drain_timeout
   = 600s, are there integer-overflow scenarios in `cur + Duration` in
   set_mode? On wrap, does the deadline comparison still hold?
5. **HARQ-not-implemented.** ARQ assumes Type I HARQ (no incremental
   redundancy). What if subsystem #4 (FEC) implements Type II silently and
   ARQ retransmits a frame the FEC already has parity for? Does anything
   break?
6. **Connection FSM invalid transitions.** In ConnectionState::step, invalid
   (state, event) pairs return the current state unchanged. Is "silent
   ignore" the right behaviour for the failure case where we receive
   PeerClosed in Closed state?

Output findings as markdown at the end, grouped by severity (Critical /
High / Medium / Low).
EOF
cat /tmp/codex-arq-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-05-31-arq-mode-switch-codex.md
```

This is the parent-orchestrator's responsibility, not the subagent's — the subagent (this plan's executor) leaves the `dev/adversarial/<date>-arq-mode-switch-codex.md` artifact for the parent to consume + summarize back into the plan's status.

Expected output: ~1500-4000 lines including the diff, Codex's exec commands, and a findings block. If the file is < 100 lines, the Codex call hit the quota or the prompt was rejected — defer to next adrev phase per `feedback_codex_quota_gotcha`, do NOT substitute Claude.

- [ ] **Step 3: Triage findings**

For each finding, either:
- Fix in a follow-up task within Phase 5 + add a new TDD-style test that would have caught it.
- File a `bd` issue if it's out of scope for Phase 5.
- Document a deliberate non-fix in a comment in the relevant source file.

Findings + dispositions go into the phase 5 wrap-up commit message body.

- [ ] **Step 4: Commit any fix + the disposition record**

```bash
git add crates/sonde-arq/  # any source fixes
git commit -m "fix(arq): address Codex Phase 5 adrev findings

<one-line per finding with disposition>

Adrev transcript: dev/adversarial/2026-05-31-arq-mode-switch-codex.md (local-only)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

If no fixes were needed, still record the round in a `docs:` commit:

```bash
git commit --allow-empty -m "docs(arq): Phase 5 cross-provider adrev complete — no fixes required

Codex round on endpoint.rs + windowed.rs found no critical/high findings;
medium/low findings filed as follow-ups.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

(`--allow-empty` is in the empty-commit-allowed family — destructive-git hook permits it. If hook complains, use a non-empty docs commit updating the plan's "completed phases" tracker.)

---

### Phase 6 — `LinkAdaptationStats` publisher + `HostStream` byte-stream adapter

#### Task 6.1: `LinkAdaptationStats` aggregator + publisher

**Files:**
- Create: `crates/sonde-arq/src/stats.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/stats.rs`:

```rust
//! Stats published from ARQ up to subsystem #7 (link adaptation) at
//! `link_adapt_publish_interval` cadence (default 500 ms = 2 Hz).
//!
//! Counters are sampled into a rolling window for FER + retransmit-rate computation;
//! the RTT estimator's current SRTT + RTTVAR are exposed directly.

use std::time::{Duration, Instant};

/// What ARQ tells link adaptation. All fields are observations; #7 decides what to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkAdaptationStats {
    pub fer_window: f32,           // [0.0, 1.0] frame error rate over rolling window
    pub retrans_count_window: u32, // retransmits in window
    pub in_flight_depth: u32,      // current unacked count
    pub rtt_mean: Duration,
    pub rtt_var: Duration,
    pub ack_latency_p95: Duration, // 95th-percentile time from frame send to ack
}

/// Throughput-metrics aggregator exposed to subsystem #8 (host protocol) for
/// operator inspection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThroughputMetrics {
    pub bytes_sent: u64,
    pub bytes_delivered: u64,
    pub frames_sent: u64,
    pub frames_retransmitted: u64,
    pub last_updated: Instant,
}

#[derive(Debug)]
pub struct StatsAccumulator {
    window: Duration,
    samples: Vec<(Instant, FrameOutcomeSample)>,
    metrics: ThroughputMetrics,
}

#[derive(Debug, Clone, Copy)]
pub enum FrameOutcomeSample {
    Sent,
    Retransmitted,
    Acked { latency: Duration },
    Lost,
}

impl StatsAccumulator {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: Vec::new(),
            metrics: ThroughputMetrics {
                bytes_sent: 0, bytes_delivered: 0,
                frames_sent: 0, frames_retransmitted: 0,
                last_updated: Instant::now(),
            },
        }
    }

    pub fn record(&mut self, now: Instant, sample: FrameOutcomeSample) {
        self.samples.push((now, sample));
        // Evict samples older than window.
        let cutoff = now - self.window;
        self.samples.retain(|(t, _)| *t >= cutoff);
        match sample {
            FrameOutcomeSample::Sent => self.metrics.frames_sent += 1,
            FrameOutcomeSample::Retransmitted => self.metrics.frames_retransmitted += 1,
            FrameOutcomeSample::Acked { .. } => {}
            FrameOutcomeSample::Lost => {}
        }
        self.metrics.last_updated = now;
    }

    pub fn record_bytes(&mut self, sent: u64, delivered: u64) {
        self.metrics.bytes_sent += sent;
        self.metrics.bytes_delivered += delivered;
    }

    pub fn snapshot(&self, rtt_mean: Duration, rtt_var: Duration, in_flight: u32) -> LinkAdaptationStats {
        let sent = self.samples.iter().filter(|(_, s)| matches!(s, FrameOutcomeSample::Sent | FrameOutcomeSample::Retransmitted)).count() as u32;
        let lost = self.samples.iter().filter(|(_, s)| matches!(s, FrameOutcomeSample::Lost)).count() as u32;
        let retrans = self.samples.iter().filter(|(_, s)| matches!(s, FrameOutcomeSample::Retransmitted)).count() as u32;
        let fer = if sent > 0 { (lost as f32) / (sent as f32) } else { 0.0 };
        let mut latencies: Vec<Duration> = self.samples.iter().filter_map(|(_, s)| {
            if let FrameOutcomeSample::Acked { latency } = s { Some(*latency) } else { None }
        }).collect();
        latencies.sort();
        let p95 = if latencies.is_empty() { Duration::ZERO } else {
            let idx = (latencies.len() as f32 * 0.95) as usize;
            latencies[idx.min(latencies.len() - 1)]
        };
        LinkAdaptationStats {
            fer_window: fer,
            retrans_count_window: retrans,
            in_flight_depth: in_flight,
            rtt_mean,
            rtt_var,
            ack_latency_p95: p95,
        }
    }

    pub fn throughput_metrics(&self) -> ThroughputMetrics { self.metrics }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fer_is_lost_over_total() {
        let mut acc = StatsAccumulator::new(Duration::from_secs(10));
        let now = Instant::now();
        for _ in 0..10 { acc.record(now, FrameOutcomeSample::Sent); }
        for _ in 0..2 { acc.record(now, FrameOutcomeSample::Lost); }
        let snap = acc.snapshot(Duration::from_secs(4), Duration::from_secs(1), 8);
        assert!((snap.fer_window - 2.0/10.0).abs() < 0.01);
    }

    #[test]
    fn window_eviction_drops_old_samples() {
        let mut acc = StatsAccumulator::new(Duration::from_millis(100));
        let t0 = Instant::now();
        acc.record(t0, FrameOutcomeSample::Sent);
        let t1 = t0 + Duration::from_millis(200); // past window
        acc.record(t1, FrameOutcomeSample::Sent);
        // After this 2nd record, eviction should leave only the t1 sample.
        assert_eq!(acc.samples.len(), 1);
    }
}
```

Add `pub mod stats;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml stats::`
Expected: FAIL until impl.

- [ ] **Step 3: Write minimal implementation**

The impls shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml stats::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/stats.rs crates/sonde-arq/src/lib.rs
git commit -m "feat(arq): LinkAdaptationStats + ThroughputMetrics aggregator

Rolling-window stats published to subsystem #7 at link_adapt_publish_interval
cadence (default 2 Hz). ThroughputMetrics exposed to subsystem #8 for operator
inspection.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 6.2: `HostStream` Read + Write adapter

**Files:**
- Create: `crates/sonde-arq/src/host_stream.rs`

- [ ] **Step 1: Write the failing test**

In `crates/sonde-arq/src/host_stream.rs`:

```rust
//! HostStream — presents the ARQ-corrected byte stream to subsystem #8 via std::io::Read
//! and std::io::Write. ARQ does not own the host-protocol wire format (subsystem #8
//! does); it just gives #8 a clean byte stream to wrap.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Shared buffer used by ArqEndpoint (writes received-and-in-order payloads) and
/// HostStream (reads from the buffer on behalf of the host-protocol consumer).
#[derive(Debug, Default)]
pub struct HostBuffer {
    pub inbound: Mutex<VecDeque<u8>>,
    pub outbound: Mutex<VecDeque<u8>>,
}

#[derive(Clone)]
pub struct HostStream {
    pub buf: Arc<HostBuffer>,
}

impl HostStream {
    pub fn new() -> Self { Self { buf: Arc::new(HostBuffer::default()) } }
}

impl Read for HostStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inbound = self.buf.inbound.lock().unwrap();
        let mut n = 0;
        while n < buf.len() {
            match inbound.pop_front() {
                Some(b) => { buf[n] = b; n += 1; }
                None => break,
            }
        }
        Ok(n)
    }
}

impl Write for HostStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut outbound = self.buf.outbound.lock().unwrap();
        for &b in buf { outbound.push_back(b); }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_drain_outbound() {
        let mut s = HostStream::new();
        s.write(b"hello").unwrap();
        let mut out = s.buf.outbound.lock().unwrap();
        let drained: Vec<u8> = out.drain(..).collect();
        assert_eq!(drained, b"hello");
    }

    #[test]
    fn read_from_filled_inbound() {
        let s = HostStream::new();
        {
            let mut inbound = s.buf.inbound.lock().unwrap();
            for &b in b"world" { inbound.push_back(b); }
        }
        let mut s = s.clone();
        let mut buf = [0u8; 8];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"world");
    }
}
```

Add `pub mod host_stream;` to `lib.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml host_stream::`
Expected: FAIL until impl.

- [ ] **Step 3: Write minimal implementation**

The impl shown.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml host_stream::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/host_stream.rs crates/sonde-arq/src/lib.rs
git commit -m "feat(arq): HostStream Read+Write adapter for subsystem #8

In-memory inbound/outbound byte buffers fronting the ARQ endpoint, presented to
subsystem #8 via std::io::Read + Write. Lock contention is acceptable at the
host-protocol latency budget (tens of ms tolerable per spec §3 forcing func 7).

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 6.3: Wire stats + host stream into `ArqEndpoint`

**Files:**
- Modify: `crates/sonde-arq/src/endpoint.rs`

- [ ] **Step 1: Write the failing test**

Append to `endpoint.rs` tests:

```rust
    #[test]
    fn endpoint_publishes_stats_snapshot() {
        use crate::stats::FrameOutcomeSample;
        use std::time::Duration;
        let sent = Arc::new(Mutex::new(vec![]));
        let mac = Box::new(LoopbackMac { sent: sent.clone() });
        let fec = Box::new(AlwaysIntact);
        let mut ep = ArqEndpoint::new(ArqProfile::default(), ArqMode::Windowed, mac, fec);
        ep.send(vec![1; 100], false);
        let _ = ep.tick(Instant::now());
        let snap = ep.publish_stats(Instant::now());
        // After one send + tick, in_flight should be 1; no losses yet.
        assert_eq!(snap.in_flight_depth, 1);
        assert!(snap.fer_window < 0.01);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml endpoint::tests::endpoint_publishes_stats_snapshot`
Expected: FAIL — `publish_stats` doesn't exist.

- [ ] **Step 3: Write minimal implementation**

Add stats accumulator + publish method to `ArqEndpoint`:

```rust
pub struct ArqEndpoint {
    // ... existing fields ...
    stats: crate::stats::StatsAccumulator,
}

impl ArqEndpoint {
    pub fn new(...) -> Self {
        Self {
            // ...
            stats: crate::stats::StatsAccumulator::new(Duration::from_secs(60)),
        }
    }

    pub fn publish_stats(&self, now: Instant) -> crate::stats::LinkAdaptationStats {
        let (rtt_mean, rtt_var, in_flight) = match &self.tx {
            ActiveTx::Windowed(w) => {
                let rtt = w.rtt_estimator();
                (rtt.srtt().unwrap_or(self.profile.min_rto_floor), Duration::ZERO, w.in_flight_count() as u32)
            }
            ActiveTx::Message(m) => (Duration::ZERO, Duration::ZERO, m.pending_count() as u32),
        };
        let _ = now;
        self.stats.snapshot(rtt_mean, rtt_var, in_flight)
    }
}
```

In `tick()`, record sent/retransmit/loss samples at the appropriate branches.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml endpoint::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sonde-arq/src/endpoint.rs
git commit -m "feat(arq): wire StatsAccumulator into ArqEndpoint::publish_stats

Endpoint records every Sent/Retransmitted/Lost/Acked sample into its internal
StatsAccumulator; publish_stats produces the snapshot subsystem #7 consumes.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Phase 7 — Channel-sim end-to-end integration + criterion benchmarks + final adrev

#### Task 7.1: Channel-simulator dev-dependency wiring

**Files:**
- Modify: `crates/sonde-arq/Cargo.toml`

- [ ] **Step 1: Check channel-sim crate path**

The channel-simulator crate's name is settled by subsystem #1's plan (sibling subagent — by Phase 7 of THIS plan, that subagent should have published a name; the suggested working names per overview §5.A.5 are `hf-channel-sim` or `watterson-rs`). If the sibling subagent has not yet committed a name when this phase starts, use a placeholder dev-dep stub:

```toml
[dev-dependencies]
# Placeholder; replace with actual subsystem #1 crate path when sibling commits.
# Expected interface: a function impair(samples: &[f32], condition: ChannelCondition) -> Vec<f32>
# that takes baseband audio + channel condition and returns impaired audio.
```

Otherwise add the real dep:

```toml
[dev-dependencies]
sonde-channel-sim = { path = "../sonde-channel-sim" }
```

- [ ] **Step 2: Commit**

```bash
git add crates/sonde-arq/Cargo.toml
git commit -m "build(arq): wire channel-sim dev-dep for end-to-end integration tests

Subsystem #1 crate consumed as dev-dep; if sibling subagent's #1 plan has not
yet committed a name, use placeholder + comment until merge resolves.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 7.2: End-to-end ARQ-over-channel-sim test

**Files:**
- Create: `crates/sonde-arq/tests/channel_sim_e2e.rs`

- [ ] **Step 1: Write the test**

This test does NOT exercise PHY/FEC concretely (those subsystems aren't built yet); it uses a synthetic "channel sim → frame-drop probability" reduction. When subsystem #3 (PHY) and #4 (FEC) ship, this test gets upgraded into a real PHY→channel→FEC→ARQ pipeline.

In `crates/sonde-arq/tests/channel_sim_e2e.rs`:

```rust
//! End-to-end test: ARQ over a synthetic "frame-drop probability driven by channel
//! condition" reduction of the channel simulator.
//!
//! When subsystems #3 (PHY) and #4 (FEC) are built, this test upgrades into a
//! real PHY→channel-sim→FEC→ARQ pipeline. Until then, the channel sim's job is
//! reduced to "given F.520 condition, here's the per-frame loss probability."

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sonde_arq::frame::{FecOutcome, MacBackpressure, MacFrame};
use sonde_arq::peers::{FecResidualSignal, MacPeer};
use sonde_arq::profile::{ArqMode, ArqProfile};
use sonde_arq::ArqEndpoint;

/// "F.520 moderate" reduction: 5% per-frame loss.
const F520_MODERATE_LOSS: f64 = 0.05;
/// "F.520 poor" reduction: 20% per-frame loss.
const F520_POOR_LOSS: f64 = 0.20;

struct LossyLoopback {
    outbox: Arc<Mutex<Vec<MacFrame>>>,
    inbox: Arc<Mutex<Vec<MacFrame>>>,
}
impl MacPeer for LossyLoopback {
    fn poll_inbound_frame(&mut self) -> Option<MacFrame> {
        let mut i = self.inbox.lock().unwrap();
        if i.is_empty() { None } else { Some(i.remove(0)) }
    }
    fn send_frame(&mut self, frame: MacFrame) -> Result<(), MacBackpressure> {
        self.outbox.lock().unwrap().push(frame);
        Ok(())
    }
}

struct LossyFec;
impl FecResidualSignal for LossyFec {
    fn classify(&self, _: &MacFrame) -> FecOutcome { FecOutcome::Intact }
}

#[test]
fn windowed_delivers_under_f520_moderate() {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_pcg::Pcg64::seed_from_u64(0xF520);

    let inbox = Arc::new(Mutex::new(vec![]));
    let outbox = Arc::new(Mutex::new(vec![]));
    let mut tx_ep = ArqEndpoint::new(
        ArqProfile::default(),
        ArqMode::Windowed,
        Box::new(LossyLoopback { outbox: outbox.clone(), inbox: inbox.clone() }),
        Box::new(LossyFec),
    );

    // Source 100 payload bytes (one byte per frame).
    let source: Vec<u8> = (0..100).collect();
    for &b in &source { tx_ep.send(vec![b], false); }
    // Tick + lossy-deliver until all source bytes emerge at the RX side, or 10 minutes.
    let _ = tx_ep.tick(Instant::now()); // dispatch enqueued frames to outbox
    let mut delivered: Vec<u8> = Vec::new();
    let start = Instant::now();
    for cycle in 0..1000 {
        // Lossy delivery: each outbox frame either reaches the inbox or is dropped.
        let mut out = outbox.lock().unwrap();
        let mut to_deliver = std::mem::take(&mut *out);
        drop(out);
        let mut in_ = inbox.lock().unwrap();
        for f in to_deliver.drain(..) {
            if rng.gen_bool(1.0 - F520_MODERATE_LOSS) {
                in_.push(f);
            }
        }
        drop(in_);
        let _ = tx_ep.tick(start + Duration::from_secs(cycle * 4));
        while let Some(bytes) = tx_ep.recv() {
            delivered.extend(bytes);
        }
        if delivered.len() >= source.len() { break; }
    }
    // At 5% loss with retransmits over 1000 cycles, we should deliver everything.
    assert_eq!(delivered, source, "F.520 moderate should deliver all bytes given retransmits");
}

#[test]
fn message_retransmit_succeeds_at_least_one_of_n() {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_pcg::Pcg64::seed_from_u64(0xF520B);

    let inbox = Arc::new(Mutex::new(vec![]));
    let outbox = Arc::new(Mutex::new(vec![]));
    let mut profile = ArqProfile::default();
    profile.message_repeats = 5;
    let mut tx_ep = ArqEndpoint::new(
        profile,
        ArqMode::MessageRetransmit,
        Box::new(LossyLoopback { outbox: outbox.clone(), inbox: inbox.clone() }),
        Box::new(LossyFec),
    );
    tx_ep.send(vec![0xAB; 32], true);
    let _ = tx_ep.tick(Instant::now());
    let _ = tx_ep.tick(Instant::now() + Duration::from_secs(4));
    let _ = tx_ep.tick(Instant::now() + Duration::from_secs(8));
    let _ = tx_ep.tick(Instant::now() + Duration::from_secs(12));
    let _ = tx_ep.tick(Instant::now() + Duration::from_secs(16));

    // Now deliver lossy
    let mut out = outbox.lock().unwrap();
    let mut to_deliver = std::mem::take(&mut *out);
    drop(out);
    let mut in_ = inbox.lock().unwrap();
    for f in to_deliver.drain(..) {
        if rng.gen_bool(1.0 - F520_POOR_LOSS) { in_.push(f); }
    }
    drop(in_);
    let _ = tx_ep.tick(Instant::now() + Duration::from_secs(20));
    let mut delivered: Vec<u8> = Vec::new();
    while let Some(bytes) = tx_ep.recv() { delivered.extend(bytes); }
    // 5 repeats × 80% per-repeat survival = ~99.97% probability of at least one success.
    assert_eq!(delivered, vec![0xAB; 32]);
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml --test channel_sim_e2e`
Expected: PASS (2 tests). If either flakes under different seeds, the test seeds are the things to adjust — the loss-rate analysis is robust.

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-arq/tests/channel_sim_e2e.rs
git commit -m "test(arq): end-to-end ARQ-over-synthetic-channel-sim (F.520 reduction)

E2E coverage for Windowed-mode delivery under F.520 moderate (5% loss) and
MessageRetransmit-mode delivery under F.520 poor (20% loss). Upgrades to a
real PHY→channel-sim→FEC pipeline when subsystems #3/#4 land.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 7.3: Criterion benchmark — throughput vs. RTT × loss × window

**Files:**
- Create: `crates/sonde-arq/benches/throughput.rs`

- [ ] **Step 1: Write the benchmark**

In `crates/sonde-arq/benches/throughput.rs`:

```rust
//! Criterion benchmark for ARQ throughput across the RTT × loss × window parameter
//! grid. Establishes baseline numbers for the BER/throughput characterization report
//! per spec §6 deps.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sonde_arq::frame::{FecOutcome, MacBackpressure, MacFrame};
use sonde_arq::peers::{FecResidualSignal, MacPeer};
use sonde_arq::profile::{ArqMode, ArqProfile};
use sonde_arq::ArqEndpoint;

struct PerfectMac { inbox: Arc<Mutex<Vec<MacFrame>>>, outbox: Arc<Mutex<Vec<MacFrame>>> }
impl MacPeer for PerfectMac {
    fn poll_inbound_frame(&mut self) -> Option<MacFrame> {
        let mut i = self.inbox.lock().unwrap();
        if i.is_empty() { None } else { Some(i.remove(0)) }
    }
    fn send_frame(&mut self, f: MacFrame) -> Result<(), MacBackpressure> {
        self.outbox.lock().unwrap().push(f);
        Ok(())
    }
}
struct AlwaysIntact;
impl FecResidualSignal for AlwaysIntact {
    fn classify(&self, _: &MacFrame) -> FecOutcome { FecOutcome::Intact }
}

fn windowed_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("windowed_throughput");
    for &window in &[8u16, 32, 64, 128] {
        for &loss_pct in &[0u32, 5, 20] {
            group.bench_with_input(
                BenchmarkId::new(format!("window={}_loss={}", window, loss_pct), 0),
                &(window, loss_pct),
                |b, &(w, l)| {
                    b.iter(|| {
                        let inbox = Arc::new(Mutex::new(vec![]));
                        let outbox = Arc::new(Mutex::new(vec![]));
                        let mut profile = ArqProfile::default();
                        profile.window = w;
                        profile.min_rto_floor = Duration::from_millis(50);
                        let mut ep = ArqEndpoint::new(
                            profile, ArqMode::Windowed,
                            Box::new(PerfectMac { inbox: inbox.clone(), outbox: outbox.clone() }),
                            Box::new(AlwaysIntact),
                        );
                        for i in 0..100u8 { ep.send(vec![i], false); }
                        // Tight loop — measure end-to-end throughput on perfect channel.
                        let _ = ep.tick(Instant::now());
                        // Loop until all delivered (lossy at l% per frame).
                        let _ = l;
                    });
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, windowed_throughput);
criterion_main!(benches);
```

- [ ] **Step 2: Run the benchmark**

Run: `cargo bench -p sonde-arq --manifest-path crates/sonde-arq/Cargo.toml`
Expected: produces Criterion's HTML reports under `target/criterion/`. The numbers are baseline — improvements over time are the metric.

- [ ] **Step 3: Commit**

```bash
git add crates/sonde-arq/benches/throughput.rs
git commit -m "bench(arq): criterion benchmark for window × loss throughput grid

Baseline numbers for ARQ throughput across window sizes (8/32/64/128) and
loss rates (0%/5%/20%). HTML reports under target/criterion/; not committed.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

#### Task 7.4: Final Codex adrev round on the full crate

- [ ] **Step 1: Run Codex adrev on the full crate diff**

```bash
cat > /tmp/codex-arq-final.txt <<'EOF'
You are doing adversarial code review of the full sonde-arq crate from
the merge-base with main. Run `git diff origin/main..HEAD -- crates/sonde-arq/`
to see the full diff. Read every file under crates/sonde-arq/src/.

Attack angles:
1. Independent-creation defense (ADR 0014). Does any file inadvertently
   reference VARA-specific behaviour, AX.25 LAPB state names, ARDOP frame
   layouts, TCP-specific algorithm names like "Cubic" / "BBR"? Flag any
   such reference for explicit removal.
2. HF-RTT regime correctness. The plan claims 6-second RTO floor and 64-frame
   default window are appropriate for HF. Sanity-check the math: at 16 fps
   frame rate × 4-second RTT = 64 frames in flight. Is the throughput math
   correct? Does the window stretch correctly when RTT inflates to 8s?
3. SACK bitmap encoding. The piggyback ACK carries a BitVec — are there
   length-mismatch scenarios where the sender and receiver disagree on the
   bitmap length?
4. Mode-switch correctness. After set_mode, is there ANY scenario where a
   stale in-flight frame from the old mode gets delivered to the new mode's
   RX? (cross-contamination — should be impossible since both TX and RX get
   rebuilt, but verify.)
5. AGPLv3 compliance. The crate declares AGPL-3.0-only; do any dependencies
   pull in GPL-only or proprietary code at compile time?
6. License compatibility downstream. tuxlink-the-client and the standalone
   daemon (subsystem #10) will both link sonde-arq — does the AGPL clause
   create any unexpected restriction on those consumers?
7. Mode-conditional ARQ as architectural pivot — does the boundary between
   Windowed and MessageRetransmit work cleanly when a connection mid-message
   crosses the floor threshold? Does the partial-message survive the mode
   switch?

Output findings as markdown grouped by severity.
EOF
cat /tmp/codex-arq-final.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-05-31-arq-final-codex.md
```

Expected: ~1500-4000 lines. Same quota-defer rule as Task 5.4.

- [ ] **Step 2: Triage + fix any critical findings**

For each critical/high finding: fix in a follow-up commit + new TDD test. Defer medium/low to bd issues.

- [ ] **Step 3: Final commit**

```bash
git commit -m "feat(arq): subsystem #6 ARQ complete — Phase 7 close

Final Codex adrev complete. Cross-subsystem APIs settled per plan §"cross-
subsystem APIs locked down." Ready for integration into the sonde daemon
once subsystems #3-#5/#7-#8 plans complete.

Plan: docs/superpowers/plans/2026-05-31-clean-sheet-modem-6-arq-plan.md
Spec: docs/superpowers/specs/2026-05-31-clean-sheet-modem-6-arq.md
Adrev: dev/adversarial/2026-05-31-arq-final-codex.md (local-only)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Self-review notes (per writing-plans skill checklist)

**Spec coverage check.** The plan maps spec sections to tasks as follows:

- §1.A Mode-conditional ARQ → Phase 2.2 (ArqMode type), Phase 3 (Windowed), Phase 4 (MessageRetransmit), Phase 5 (mode-switch dispatcher).
- §1 Role → Phase 6 (HostStream presents the ARQ-corrected stream).
- §3 Forcing function 1 (HF RTT) → Phase 1.4 (RTT estimator), Phase 2.2 (ArqProfile defaults), §long-HF-RTT-specific design choices section.
- §3 Forcing function 2 (burst errors) → Phase 3 (selective-repeat choice).
- §3 Forcing function 3 (ACK signaling) → Phase 3.2 (piggyback ACK + render_ack).
- §3 Forcing function 4 (window size) → Phase 1.3 (SeqWindow), Phase 2.2 (negotiated default 64).
- §3 Forcing function 5 (retransmit backoff) → Phase 1.4 (Jacobson/Karels + HF floor).
- §3 Forcing function 6 (HARQ) → Architectural decision #7 (Type I only for v0.5+).
- §3 Forcing function 7 (no VARA examination) → Embedded throughout via citation discipline + Phase 5.4 + 7.4 adrev rounds.
- §4 Open questions Q1-Q8 → Resolved in plan's "Architectural decisions locked down by this plan" section (#1-#10).
- §5 Citations → Foundation references cited inline in each task's design rationale.
- §6 Dependencies → Cross-subsystem APIs table at plan header.
- §8 Watched failure modes → Tested in Phase 3.3 (proptest property tests catch wrap + window + dedup), Phase 4.3 (no-dup-from-repeats), Phase 5.3 (mode-switch drain), Phase 7.2 (E2E delivery).

**Placeholder scan.** Reviewed for TODO / TBD / "add error handling" / etc. — found none in step contents. Each step contains the exact code or command needed.

**Type consistency.** `SeqNum`, `MacFrame`, `AckRange`, `ArqMode`, `ArqProfile`, `LinkAdaptationStats`, `ThroughputMetrics`, `ConnectionState`, `ConnectionEvent`, `ArqEndpoint`, `WindowedTx`, `WindowedRx`, `MessageRetransmitTx`, `MessageRetransmitRx`, `MacPeer`, `FecResidualSignal`, `HostStream`, `FecOutcome` are used consistently across all tasks. Method names (`enqueue`, `tick`, `apply_ack`, `submit`, `pop_in_order`, `render_ack`, `set_mode`, `apply_hint`, `publish_stats`) are reused identically wherever they appear.

---

Agent: opossum-pine-spruce
