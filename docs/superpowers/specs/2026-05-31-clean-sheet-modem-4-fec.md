# Subsystem #4 — Forward error correction

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md).
> Incorporates overview §5.A.1 (per-family FEC strategies — OFDM family
> uses per-sub-carrier or per-mode-wide FEC; robustness floor uses short-block
> strong FEC), §5.A.2 (FEC interacts with payload-size routing — short
> payloads to the robustness floor get one FEC strategy; long payloads in the
> OFDM family get another), §5.A.4 (no GPL-only runtime dependencies),
> §5.A.6 (best-effort decoder complexity).

## §1. Role

The FEC subsystem adds **structured redundancy** to PHY-emitted symbols so
that channel-induced errors can be detected and corrected at the receiver
without retransmission. Composes with ARQ (#6): FEC takes the first cut at
error correction; ARQ catches what FEC can't.

FEC may be folded into PHY (#3) as a single layer (typical for
narrow-band weak-signal protocols like FT8, where FEC bits and modulation
bits are intertwined in the LDPC payload + Costas array sync sequence), or
kept as a discrete layer with a clear inner/outer separation (typical for
wider-band ARQ protocols).

## §2. What the subsystem is NOT

- **Not the code-family choice (yet).** Reed-Solomon, convolutional+Viterbi,
  LDPC, turbo, polar — all are open per the clean-sheet posture.
- **Not interleaving.** Interleaving (bit/symbol/block) is part of the FEC
  architecture in practice. Whether interleaving is "in" subsystem #4 or
  "in" PHY (#3) is an architectural choice (see Q4 below).
- **Not ARQ.** ARQ (#6) is retransmission; FEC is forward correction.
  HARQ (Hybrid ARQ) couples the two; the seam between #4 and #6 is one
  of the open architectural questions.

## §3. Forcing functions

1. **Performance against ITU-R F.520 channel conditions** (consistent with
   PHY's same constraint). FEC's job is to extend the useful SNR range of
   the PHY. Net SNR-floor improvement under "moderate" condition is the
   first-order metric.
2. **Decoder complexity budget** (overview Q4 deployment target). LDPC
   iterative decoders, turbo decoders, and Viterbi decoders have well-
   characterized complexity scaling. Real-time decode on a Pi 5 is
   feasible for all of these at modest constraint lengths/block sizes;
   high-performance versions can saturate the host.
3. **Code-rate flexibility.** Different link-adaptation modes (#7) likely
   want different code rates. Either FEC supports a range of code rates
   natively (LDPC's rate-compatible designs) or the system designates
   multiple FEC modes selectable per link-adaptation step.
4. **Block size vs. latency.** Larger block sizes win more coding gain
   but increase latency before a complete block can be decoded. For
   interactive operator-driven uses (Winlink mail exchange, for
   example), block latency of a few seconds is acceptable. For real-time
   keyboard chat (JS8-style), tighter latency matters. **Open question:**
   what's sonde's latency budget? [open]
5. **HF burst-error pattern.** HF errors are often bursty (deep fades
   lasting tens to hundreds of ms). FEC families differ in burst-error
   resilience — Reed-Solomon over GF(256) handles bursts naturally;
   LDPC with interleaving handles bursts after de-interleaving; bare
   convolutional codes are weak against bursts without interleaving.
6. **No examination of VARA's FEC** (ADR 0014).

## §4. Open design questions

| # | Question | Notes |
|---|---|---|
| §4.Q1 | Code family — Reed-Solomon, convolutional+Viterbi, LDPC, polar, turbo, hybrid? | The big architectural choice. Each family has well-understood tradeoffs at HF data rates. |
| §4.Q2 | Code rate(s) — fixed (single rate) or family (multiple rates per link-adapt step)? | LDPC and polar both support rate-compatible designs natively. |
| §4.Q3 | Block size? | Latency vs. coding gain. |
| §4.Q4 | Interleaving — inside the FEC layer, inside PHY, or both? | Affects burst-error performance. |
| §4.Q5 | FEC as discrete layer vs. folded into PHY? | Architectural choice; LDPC-with-explicit-sync is the FT8 pattern, while ARDOP keeps FEC + PHY more discretely separable. |
| §4.Q6 | Soft-decision input from PHY? | Required for LDPC and convolutional+Viterbi. PHY must produce soft bits (log-likelihood ratios) for those code families. |
| §4.Q7 | HARQ — type I (FEC redundancy stays fixed across retransmissions), type II (additional parity per retransmission), type III (incremental redundancy)? | Couples #4 with #6; architectural choice. |

## §5. Citations from foundation doc

- §3.1: Reed-Solomon 1960, Viterbi 1967, Gallager LDPC 1963, Berrou turbo
  1993, Arikan polar 2009.
- §3.2: Costello/Forney tutorial; Wikipedia FEC/RS/LDPC/conv/polar entries.
- §4.1: Lin/Costello *Error Control Coding* (joint FEC + ARQ reference).
- §6.1: K1JT FT4/FT8 paper (worked example of LDPC + sync in narrow-band HF).
- §6.2: ARDOP open spec (worked example of HF ARQ + FEC layered separately).

## §6. Dependencies

- **Upstream:** subsystem #1 (validation harness).
- **Downstream:** subsystem #3 (PHY — provides modulated symbols; FEC may be
  folded in), subsystem #6 (ARQ — coordinates with FEC if HARQ).
- **Co-iteration with:** #3 and #6.

## §7. No-implementation-choice markers

This STUB does not designate any specific code family, code rate, block
size, decoder algorithm, or HARQ type.

## §8. Watched failure modes

- **Optimistic decode-rate projection.** Stated performance for a code
  family is typically against AWGN; HF performance is worse due to
  multipath, fading, and Doppler. The channel simulator (#1) is the gate
  on realistic performance claims.
- **Decoder-complexity miscalculation.** Soft-decision LDPC at moderate
  iteration counts is fast on modern CPUs; max-iteration LDPC with
  edge-case messages can stall. Profile under representative inputs.
- **HARQ scope creep.** Type III HARQ with rate-compatible incremental
  redundancy is powerful but architecturally heavy. Start with type I or
  no HARQ; upgrade if needed.

Agent: mink-swallow-kite
