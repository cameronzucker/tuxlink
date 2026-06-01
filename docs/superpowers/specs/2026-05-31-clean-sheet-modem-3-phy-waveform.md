# Subsystem #3 — PHY / waveform

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md).
> Incorporates overview §5.A.1 (multi-mode ladder, bit-adaptive OFDM main
> family + robustness-modes-family floor with wide-band low-density-OFDM
> default + situational narrow-FSK), §5.A.2 (PHY-family routing
> implication), §5.A.6 (best-effort compute).

## §1. Role

The PHY / waveform subsystem is **what tuxmodem sounds like on the air.** It
takes link-layer frames (from subsystem #5) and renders them into audio-band
samples for the radio's input. On the receive side, it takes audio-band
samples from the radio's output and produces detected frames for the link
layer.

Per overview §5.A.1, the PHY is **two architecturally-distinct mode families**
stitched into one ladder:

- **Bit-adaptive OFDM family** (main throughput modes). Orthogonal sub-carriers
  spanning the radio's audio passband. Per-sub-carrier bit-loading: each
  sub-carrier individually carries 0..N bits per symbol based on observed
  per-sub-carrier SNR (DSL/xDSL pattern; ITU-T G.992/G.993 are the open
  references). High-SNR sub-carriers use denser constellations (16-QAM,
  64-QAM, possibly higher); low-SNR sub-carriers use sparse (QPSK, BPSK) or
  are turned off. Throughput emerges from the bit-loading curve × bandwidth
  × channel conditions × audio-passband response.

- **Robustness modes family** (bottom of the ladder). A small set of modes
  parameterized by what's limiting the link:
  - **Default — wide-band low-density-constellation OFDM.** When the
    limiting condition is per-Hz noise floor with full bandwidth
    available (typical tuxmodem case — own-frequency point-to-point or
    point-to-gateway, no crowding), use BPSK per sub-carrier across the
    full available passband with very strong FEC (rate-1/4 LDPC
    short-block or similar). Aggregate throughput scales with sub-carrier
    count; each sub-carrier sits independently above its Shannon
    threshold. Significantly outperforms FT8-class narrow-FSK at the same
    SNR floor by using wider bandwidth — at low per-sub-carrier SNR,
    higher-density constellations are below Shannon and cannot decode
    regardless of FEC, so going *wider* (not denser) is the only path to
    aggregate throughput. Design goal: beat ARDOP's narrowest-mode SNR
    floor at the noise-floor case.
  - **Situational — narrow-FSK** (FT8/JS8 conceptual primitive, 8-FSK).
    Reserved for the rare-for-tuxmodem case where the assigned frequency
    is genuinely bandwidth-constrained (crowded emcomm net, narrow
    available spectrum slice). Conceptual primitive borrowed from FT8/JS8
    weak-signal design (foundation doc §6.1) — primitive only, not
    specific protocol parameters per
    `feedback_clean_sheet_concepts_only`.

PHY composes: per-family modulation (OFDM with per-sub-carrier bit-loading,
or FSK with short-block FEC), synchronization (carrier frequency offset
estimation, symbol timing, frame sync detection — likely shared sync
infrastructure across families), and channel measurement (per-sub-carrier
SNR estimation for the OFDM family — fed back to subsystem #7 link
adaptation for bit-loading decisions). FEC (subsystem #4) varies by PHY
family.

## §2. What the subsystem is NOT

- **Not the rig control.** PTT, frequency, mode-set, audio gain are
  subsystem #9's concern (integration) via the `tux-rig` crate (ADR 0015).
- **Not the modem-runtime architecture.** Whether PHY is a Rust module
  inside a single process or its own daemon is part of subsystem #10
  (packaging).
- **Not the link-adaptation policy.** The PHY exposes per-mode primitives
  and per-sub-carrier SNR estimates; subsystem #7 owns the policy that
  drives bit-loading decisions and family/mode selection.

## §3. Forcing functions

1. **Per-radio audio-passband variability.** Not all target radios have
   flat-passband response across 300-2700 Hz; many roll off at the edges.
   Bit-adaptive OFDM naturally handles this — sub-carriers in the rolloff
   regions get zero or few bits — but the PHY must measure + react to it.
   FT-818 stock SSB filter is the canonical worst-case in the reference
   radio set (per the bench-rig spec).
2. **HF channel impairment envelope** per ITU-R F.520 (good/moderate/poor/
   flutter). PHY must demodulate usefully across the envelope; per-family
   responsibilities differ — OFDM family handles "good" through "moderate"
   with bit-loading degradation; robustness floor (default: wide-band
   low-density OFDM) handles "poor" / "flutter" with strong-FEC + many
   sub-carriers; situational narrow-FSK handles crowded-band cases.
3. **Sync robustness across families.** Sync (carrier offset, symbol
   timing, frame sync) likely shares infrastructure between families — but
   the robustness-family modes need more sync robustness than the OFDM
   family because
   it operates at the floor. Open: whether sync is per-family or shared
   primitives + family-specific tuning.
4. **Per-sub-carrier SNR estimation interface** (subsystem #1 §3.6 + this
   subsystem must coordinate). The channel simulator exposes per-sub-carrier
   SNR; the PHY consumes those measurements (both in simulation and on real
   RF). Interface stability matters.
5. **Compute target — best-effort** (overview §5.A.6). No pre-committed
   constraint on FFT size, equalization complexity, or constellation
   density. Profile + optimize where bottlenecks appear.
6. **No examination of VARA's PHY** (ADR 0014). The "obvious" temptation
   to "just look at what bandwidth modes VARA picks" or "see how VARA
   does bit-loading" is exactly the forbidden act. STOP if surfaces.

## §4. Open design questions (remaining at subsystem level)

| # | Question | Status / notes |
|---|---|---|
| §3.Q1 | Number of OFDM modes within the family? | Open. ARDOP uses 4 (200/500/1000/2000 Hz). tuxmodem may use fewer (3?) or more (5+). Settle informed by audio-passband measurements on the bench-rig radios + bit-loading characterization at each width. |
| §3.Q2 | Per-mode OFDM bandwidth choices? | Open. With bit-adaptive OFDM, exact mode bandwidths are less load-bearing than for fixed-constellation OFDM — the bit-loading adapts within the chosen bandwidth — but enumerated values still need pinning before subsystem #7 link-adaptation policy can step among them. |
| §3.Q3 | Sub-carrier count + spacing per OFDM mode? | Open. ADSL pattern uses 4.3125 kHz sub-carrier spacing with 256 sub-carriers; HF audio-band scales would be very different (tens to low hundreds of Hz sub-carrier spacing). |
| §3.Q4 | Robustness-modes-family specifics — default wide-band low-density OFDM parameters (sub-carrier count, FEC code rate) + situational narrow-FSK parameters (number of tones, symbol rate, block size) | Open. Default mode: BPSK per sub-carrier across ~2.3 kHz passband with rate-1/4 LDPC short-block is the starting point (yields ~575 bps net at -5 dB per-sub-carrier SNR — ~100x FT8 throughput at same SNR). Narrow-FSK situational mode borrows FT8 8-FSK 0.16 baud as conceptual reference point. Settle informed by channel-sim SNR-floor measurements. |
| §3.Q5 | Sync sequence design — preamble length, pilot insertion, frame-sync correlation? | Open. Affects acquisition reliability under noise. |
| §3.Q6 | Sample rate at the audio interface — 8/16/24/48 kHz? | Open. 48 kHz is the modern default; lower rates are computationally cheaper but constrain the sub-carrier grid. |
| §3.Q7 | Equalization strategy — rely on cyclic prefix (OFDM intrinsic), decision-feedback, MLSE, none? | Open. Major complexity-vs-performance tradeoff. |
| §3.Q8 | Pilot-aided vs. blind carrier/timing recovery? | Open. Pilot-aided is more robust; blind is bandwidth-efficient. |

## §5. Citations from foundation doc

- §2.1: Proakis, Sklar, Haykin (PHY fundamentals).
- §2.2: Shannon (capacity bounds — what the PHY can possibly achieve).
- §2.3: OFDM family — Cimini 1985, Wikipedia OFDM.
- §2.4: QAM family.
- §2.5: Meyr/Moeneclaey/Fechtel (synchronization — often the unsung difficulty).
- §6: ARDOP, FT8/FT4 — conceptual references for HF PHY design space.
- §5.1: Lyons, Smith dspguide.com (DSP foundations).

## §6. No-implementation-choice markers

This STUB does not designate:

- Specific modulation family
- Specific symbol rate
- Specific constellation
- Specific sync sequence
- Specific equalization algorithm
- Specific sample rate

## §7. Dependencies

- **Upstream:** subsystem #1 (channel simulator) — validates every PHY
  iteration; subsystem #4 (FEC) — provides the FEC layer below or folded-in.
- **Downstream:** subsystem #5 (MAC — provides frames to encode), subsystem #7
  (link adaptation — chooses PHY mode), subsystem #9 (integration — supplies
  audio I/O).
- **Co-iteration with:** subsystem #4 (FEC). Often the FEC layout informs PHY
  frame structure; often the PHY constraints inform FEC code rate selection.

## §8. Watched failure modes

- **VARA-shaped temptation.** The "what bandwidth + symbol rate + modulation
  did VARA pick" question is exactly the forbidden one. STOP if it surfaces.
- **Sim-only validation.** A PHY that works in the channel simulator can
  fail on real radios if it relies on flat audio response, perfect
  out-of-band rejection, etc. RF-rig (#2) + bench-rig characterization is
  the cross-check.
- **Over-targeting one channel condition.** PHY that wins on F.520 "good"
  but degrades faster than ARDOP on F.520 "poor" is the wrong tradeoff for
  emcomm. The link-adaptation policy (#7) needs a graceful degradation
  curve; the PHY has to provide it.

Agent: mink-swallow-kite
