# Subsystem #3 — PHY / waveform (STUB)

> **Status: STUB.** Subordinate to the program overview DRAFT. Operator-review
> pending. No implementation choices pre-decided.

## §1. Role

The PHY / waveform subsystem is **what tuxmodem sounds like on the air.** It
takes link-layer frames (from subsystem #5) and renders them into audio-band
samples for the radio's input. On the receive side, it takes audio-band
samples from the radio's output and produces detected frames for the link
layer.

PHY composes: modulation (constellation choice, sub-carrier structure if any),
synchronization (carrier frequency offset estimation, symbol timing, frame
sync detection), and channel adaptation (in concert with subsystem #7).
FEC (subsystem #4) may be folded into the PHY layer or kept separate
depending on architectural choice.

## §2. What the subsystem is NOT

- **Not the modulation family choice (yet).** OFDM, single-carrier QAM,
  multi-carrier MFSK, hybrid — all are open per the clean-sheet posture.
  The choice is one of the operator's overview Q1-Q8 decisions and the
  subsystem-canonical-spec's first move.
- **Not the rig control.** PTT, frequency, mode-set, audio gain are
  subsystem #9's concern (integration) via the `tux-rig` crate (ADR 0015).
- **Not the modem-runtime architecture.** Whether PHY is a Rust module
  inside a single process or its own daemon is part of subsystem #10
  (packaging).

## §3. Forcing functions

1. **Bandwidth ceiling ≤2300 Hz** (overview Q1 default, subject to operator
   confirmation). FT-818 stock SSB IF filter is the binding constraint —
   designs that don't fit force an aftermarket-filter requirement or restrict
   the deployable installed base.
2. **HF channel impairment envelope** per ITU-R F.520 (good/moderate/poor/
   flutter). PHY must demodulate usefully under at least the "moderate"
   condition; "poor" and "flutter" are stretch goals informing link
   adaptation (#7).
3. **Real-radio audio path constraints.** Not all radios in the target
   installed base have flat-passband response across 300-2700 Hz; many
   roll off at the edges. PHY signal energy near band edges is fragile;
   middle-of-band is robust. Bench-rig characterization of the G90 + FT-818
   surfaces specific audio-passband behaviors.
4. **Sync robustness vs. data rate.** Every PHY trades off how much of the
   transmission is sync overhead vs. payload. Low-SNR conditions need more
   sync; high-SNR can lean on shorter preambles. Open question whether
   tuxmodem supports adaptive preamble length [open].
5. **Compute budget** (overview Q4). Demod complexity, particularly any
   coherent equalizer, has to run real-time on the deployment target.
6. **No examination of VARA's PHY** (ADR 0014). The "obvious" temptation
   to "just look at what bandwidth modes VARA picks" is exactly the
   forbidden act.

## §4. Open design questions

| # | Question | Notes |
|---|---|---|
| §3.Q1 | Modulation family — OFDM-class, single-carrier QAM, MFSK, hybrid? | The big PHY architectural choice. Each has well-understood tradeoffs (PAPR, equalization complexity, sync robustness). |
| §3.Q2 | Total occupied bandwidth — operator confirms overview Q1 (proposal: 2300 Hz). | Caps everything else. |
| §3.Q3 | Number of operator-selectable bandwidth modes? | ARDOP uses 4 (200/500/1000/2000 Hz). tuxmodem may use fewer (3?) or more (5+ with finer steps). |
| §3.Q4 | Sync sequence design — preamble length, pilot insertion, frame-sync correlation? | Affects acquisition reliability under noise. |
| §3.Q5 | Sample rate at the audio interface — 8/16/24/48 kHz? | Affects bandwidth representability + computational cost. |
| §3.Q6 | Equalization strategy — none (rely on cyclic prefix if OFDM), decision-feedback, MLSE? | Major complexity-vs-performance tradeoff. |
| §3.Q7 | Pilot-aided vs. blind carrier/timing recovery? | Sync convention. |
| §3.Q8 | Frame size — fixed or variable? | Interacts with MAC #5 framing. |

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
