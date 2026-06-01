# Subsystem #7 — Link adaptation

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md).
> Incorporates overview §5.A.1 (multi-mode ladder with two families — OFDM
> + FSK-floor), §5.A.2 (payload-size-aware MAC routing makes link
> adaptation a 2D policy).

## §1.A Two-dimensional adaptation policy (per overview §5.A.1 + §5.A.2)

Link adaptation in tuxmodem is **substantially more complex** than the
conventional "measure SNR, pick a mode" policy because of the dual-family
PHY architecture:

**Outer loop — mode-family selection:**

- Observe channel quality (SNR, FER, throughput, bit-loading curve shape).
- If channel quality is above the FSK-floor threshold → operate in the
  bit-adaptive OFDM family.
- If channel quality degrades past the OFDM family's usable envelope AND
  outgoing payload is short critical → drop into the FSK weak-signal floor.

**Inner loop — within-OFDM-family:**

- Per-sub-carrier bit-loading. Adapts continuously to per-sub-carrier SNR
  measurements (PHY exposes; link adaptation consumes). Higher per-sub-carrier
  SNR → more bits per symbol on that sub-carrier; lower → fewer or none.
- Mode-within-family selection (which discrete OFDM mode is in use — i.e.,
  which bandwidth + sub-carrier-count combination). Slower than bit-loading.

**Coordination with MAC payload-size routing:**

- MAC (subsystem #5) provides outgoing payload size.
- Link adaptation provides current channel-quality estimate + recommended
  mode.
- MAC combines the two to decide: route to OFDM-mode-N, or drop to FSK
  floor.

**Coordination with ARQ:**

- ARQ is mode-conditional (subsystem #6 §1.A). When link adaptation
  switches mode families (OFDM ↔ FSK floor), the ARQ state machine
  enables / disables accordingly.

## §1. Role

The link adaptation subsystem **dynamically selects the PHY mode** (and
potentially the FEC code rate) based on observed channel quality. Under
good channel conditions, run higher-rate / higher-density modulation;
under poor conditions, fall back to robust / low-rate modes.

This is the difference between a one-size-fits-all modem (works
acceptably across the channel envelope but never wins on any specific
condition) and an adaptive modem (matches actual conditions and gets
near-optimal throughput at each point on the SNR / BER curve).

## §2. What the subsystem is NOT

- **Not the PHY itself.** PHY (#3) provides the modes; link adaptation
  picks among them. The mode set is a PHY decision.
- **Not the FEC code-rate choice within a mode.** That's FEC (#4)'s
  layer. Link adaptation may co-step PHY + FEC together (typical) or
  step them independently (more flexible, more complex).
- **Not the ARQ retransmission policy.** ARQ (#6) handles per-frame
  retransmission; link adaptation handles per-link mode selection.
  They use overlapping signals (FER, throughput) but make different
  decisions.

## §3. Forcing functions

1. **Channel-quality metric.** The link-adaptation policy needs a
   signal: SNR estimate (from PHY), BER pre-FEC (from FEC decoder),
   frame error rate (from MAC+ARQ), or throughput (composite).
   Multiple signals can be fused; one of them is the primary.
2. **Mode-set granularity.** Few modes (2-4, ARDOP-style) = simpler
   policy + simpler operator UX; many modes = better throughput at
   intermediate conditions, more complex policy.
3. **Hysteresis vs. responsiveness.** Aggressive mode-switching at every
   noise tick = mode-flapping (spends more time switching than
   transmitting). Slow hysteresis = misses real channel changes.
   Tune empirically against ITU-R F.520 conditions.
4. **Negotiation overhead.** Mode switches need to be coordinated with
   the peer (you can't switch modes unilaterally — the peer has to know
   what to decode). Either explicit negotiation frames (clean,
   bandwidth-costly) or piggyback in MAC frame headers (efficient,
   couples #7 to #5).
5. **Operator override.** The operator may want to force a specific
   mode (e.g., during testing or under known-good conditions where
   the adapter is conservative). Manual mode selection must be
   supported.
6. **No examination of VARA's link adaptation** (ADR 0014).

## §4. Open design questions

| # | Question | Notes |
|---|---|---|
| §7.Q1 | Channel-quality metric — SNR, BER pre-FEC, FER, throughput, or fused? | Foundational choice. |
| §7.Q2 | Mode-step granularity? | Few modes vs. many modes. |
| §7.Q3 | Hysteresis policy — fixed thresholds, RL-style, channel-condition-aware? | Tradeoff complexity vs. responsiveness. |
| §7.Q4 | Negotiation — explicit frames, piggybacked headers, or both? | Couples to MAC #5. |
| §7.Q5 | Operator override — supported how? | UI + host protocol exposure. |
| §7.Q6 | Asymmetric mode selection — can the two peers use different modes in each direction? | Adds complexity; useful when uplink and downlink channel conditions differ (NVIS vs. ground-wave, frequency-selective fades). |
| §7.Q7 | Step rate — instantaneous, or with rate-limiting? | Throughput stability. |
| §7.Q8 | Recovery from a failed mode — graceful fallback or hard reset? | Connection-state-machine implication. |

## §5. Citations from foundation doc

- §6.2: ARDOP — mode-stepping pattern reference.
- §4.1: Lin/Costello + Bertsekas/Gallager — throughput-vs-channel-quality
  analysis substrate.
- §1.2: ITU-R F.520 — the channel conditions the policy must operate
  across.

## §6. Dependencies

- **Upstream:** subsystems #3 (PHY mode set), #4 (FEC code-rate options),
  #6 (ARQ-level metrics).
- **Downstream:** subsystem #8 (host protocol exposes link-adaptation
  state + operator override to clients); subsystem #9 (integration —
  UI surfaces).

## §7. No-implementation-choice markers

No specific policy, metric, mode set, or hysteresis algorithm
designated.

## §8. Watched failure modes

- **Mode-flapping.** Easy to write a policy that switches modes too
  aggressively. Test against constant-condition and slowly-varying-
  condition channel simulator runs to verify stable behavior.
- **Stale channel-quality state.** If channel quality is computed from
  rolling averages over too-long a window, the policy reacts slowly to
  real changes. Too-short a window and you flap.
- **Asymmetric-channel surprises.** If one direction's channel is much
  worse than the other (common in NVIS, when the two stations have
  different antenna systems), an algorithm that assumes symmetry will
  pick a too-optimistic mode for the bad direction.

Agent: mink-swallow-kite
