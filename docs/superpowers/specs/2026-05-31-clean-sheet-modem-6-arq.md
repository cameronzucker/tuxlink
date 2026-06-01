# Subsystem #6 — ARQ

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md).
> Incorporates overview §5.A.2 (ARQ is **mode-conditional**: ARQ applies above
> the FSK weak-signal floor mode; the floor mode itself operates ARQ-disabled
> with retransmit-the-whole-message semantics — FT8 pattern). ARQ is a
> per-mode attribute, not a project-wide universal.

## §1.A Mode-conditional ARQ (per overview §5.A.2)

ARQ is not a universal layer applied above the PHY. It's mode-conditional:

- **OFDM family modes:** ARQ applies. Frames carry sequence numbers; lost
  frames are retransmitted. Selective-repeat ARQ is preferred under HF
  burst-error patterns (per the AX.25 v2.2 + ARDOP references).
- **FSK weak-signal floor mode:** ARQ does NOT apply at the frame level.
  Short critical payloads are sent in a single FSK block. On reception
  failure, the SENDER retransmits the whole block (the receiver doesn't
  NACK; cycle-based retry is implicit). This is the FT8/JS8 pattern —
  no ARQ state machine at the lower-bound mode.

This means subsystem #6 implementation has TWO modes:

- **Full ARQ** for OFDM family (selective-repeat, sequence numbers,
  ACK/NACK or piggybacked ACK).
- **No ARQ** for FSK floor (the link-layer just sends the frame; if it
  doesn't decode, MAC-level retry resends).

## §1. Role

The ARQ subsystem provides **reliable delivery** over the lossy PHY+FEC
layer. When a frame is corrupted beyond FEC's correction capability and
the link is connection-oriented, ARQ requests retransmission until the
frame arrives intact (or the connection terminates).

ARQ is the difference between "this protocol is best-effort" (frames may
be lost) and "this protocol delivers" (frames are guaranteed to arrive
in order or the link breaks). For Winlink-style mail exchange, ARQ is
mandatory: you can't lose half a message.

## §2. What the subsystem is NOT

- **Not the FEC layer.** Forward error correction (#4) tries to fix
  errors without retransmission; ARQ retransmits when FEC can't.
- **Not the link layer framing.** MAC (#5) provides the frame structure
  with sequence numbers; ARQ uses those numbers.
- **Not the routing layer.** Once a frame is acknowledged on this link,
  ARQ's job is done; further routing through digipeaters or higher-level
  networks is out of scope here.

## §3. Forcing functions

1. **HF round-trip time.** HF has multi-second RTT (channel propagation
   + processing + medium-access contention). Stop-and-wait ARQ is wildly
   suboptimal at HF RTTs — windowed protocols are required.
2. **Channel-burst-error pattern.** HF errors are bursty. Go-back-N
   wastes throughput because a burst that kills frame N forces
   retransmission of N, N+1, ..., N+window. Selective-repeat is
   substantively better under burst errors.
3. **ACK / NACK signaling.** Explicit ACK frames cost bandwidth; implicit
   ACK (next data frame's sequence number) saves bandwidth but adds
   complexity. Tradeoff to settle.
4. **Window size.** Wide window = more in-flight frames = better
   throughput at high RTT. Wider window also costs more memory + more
   sequence-space requirements. AX.25 v2.0 capped at 7 (3-bit seq);
   v2.2 expanded to 127 (7-bit seq).
5. **Retransmission backoff.** When ARQ retransmits, how soon? Too soon
   = duplicate frame on a channel where the original is in flight; too
   late = throughput drop. Tradeoff with PHY's frame duration + measured
   RTT.
6. **Hybrid ARQ option.** Type II / Type III HARQ couples FEC with ARQ
   to send incremental redundancy on retransmission — saves bandwidth at
   complexity cost.
7. **No examination of VARA's ARQ** (ADR 0014).

## §4. Open design questions

| # | Question | Notes |
|---|---|---|
| §6.Q1 | Selective-repeat, go-back-N, or hybrid? | Selective-repeat preferred at HF; AX.25 v2.2 is the deployed reference. |
| §6.Q2 | Window size — fixed or negotiated? | Negotiation adds complexity; fixed is simpler if chosen well. |
| §6.Q3 | ACK style — explicit ACK frames, implicit (piggybacked on next data frame), or both? | Bandwidth/complexity tradeoff. |
| §6.Q4 | NACK supported? | Allows faster retransmit on detected loss; adds protocol complexity. |
| §6.Q5 | Retransmission backoff algorithm — fixed timer, exponential, RTT-tracking? | Standard tradeoffs. |
| §6.Q6 | HARQ — type I (none), type II (incremental), type III (rate-compatible)? | Couples with FEC #4 decision. |
| §6.Q7 | Connection-state machine — SABM/UA/DISC (AX.25 style), or other? | Affects MAC #5 frame types. |
| §6.Q8 | Maximum retransmission count before connection drop? | Operator-tunable or fixed. |

## §5. Citations from foundation doc

- §4.1: Lin/Costello (joint FEC + ARQ reference); Bertsekas/Gallager
  (ARQ throughput analysis).
- §6.2: ARDOP (worked example of selective-repeat HF ARQ).
- §6.3: AX.25 v2.2 Selective Reject — 1998-published spec, single
  complete implementation as of 2020. The 22-year adoption gap is a
  warning: implementing v2.2-style selective-repeat is not free.

## §6. Dependencies

- **Upstream:** subsystem #5 (MAC — provides frames with sequence
  numbers + connection-state primitives); subsystem #4 (FEC — defines
  what "frame received intact" means).
- **Downstream:** subsystem #7 (link adaptation — uses ARQ-level metrics
  like FER and retransmission count as channel-quality signals);
  subsystem #8 (host protocol — exposes connection state + throughput
  metrics to clients).

## §7. No-implementation-choice markers

No specific ARQ scheme, window size, ACK style, backoff algorithm, or
HARQ type designated.

## §8. Watched failure modes

- **Throughput-vs-RTT mispricing.** Stop-and-wait at 5-second HF RTT
  gives ~10% link utilization at best. The window has to be wide enough
  to keep frames in flight throughout the RTT.
- **Sequence wraparound under high throughput.** Wider sequence space
  is cheap if specified up-front; retrofitting wider sequences after
  v0.5+ ship is the AX.25 v2.0 → v2.2 problem repeating.
- **NACK storms.** Aggressive NACK on every detected loss can saturate
  the reverse channel; rate-limit NACKs or use implicit-NACK-via-ACK-
  inversion.
- **Connection-state-machine bugs.** ARQ's connection state machine is
  notoriously bug-prone (SABM/SABM-collide, DISC-during-FRMR, etc.).
  Adopt a battle-tested state machine (AX.25's or Q.921's) or
  exhaustively-test the new one.

Agent: mink-swallow-kite
