# Subsystem #1 — HF channel simulator (STUB)

> **Status: STUB.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview-DRAFT.md](2026-05-31-clean-sheet-modem-overview-DRAFT.md);
> not approved as canonical until the overview is. Renamed off `-STUB` after
> operator-approval of the overview + subsystem-level review of this doc.
>
> **Scope:** what the HF channel simulator subsystem does, what it does NOT do,
> the forcing functions on its design, and the open questions the operator
> needs to settle before implementation. Does NOT pre-decide algorithm choice,
> language, or API.

## §1. Role

The HF channel simulator is the **validation harness** for every later
modem subsystem (PHY, FEC, MAC, ARQ, link adaptation). It accepts baseband
I/Q (or audio-band) samples representing what the modem TX would emit, applies
a parameterized HF ionospheric channel model to those samples, and produces
channel-impaired output samples that the modem RX consumes.

The simulator is what makes the **DSP-first methodology** in the program
overview (§3) work. Every PHY candidate, every FEC choice, every ARQ scheme,
every link-adaptation policy gets evaluated against the simulator in
software-only loops before any RF rig is involved. BER, throughput, mode-
switching latency under standardized channel conditions become the comparable
performance metric across design iterations.

## §2. What the subsystem is NOT

- **Not the radio.** The simulator models the *channel* (the air between two
  radios), not the radios themselves. Real-radio characterization happens in
  subsystem #2 (RF measurement rig) and the bench rig.
- **Not the deployment runtime.** The simulator is a development + CI tool.
  It does not ship with tuxlink at runtime; it lives in tuxmodem's test
  infrastructure (or as its own crate per the Q7 question in the overview).
- **Not a perfect ionospheric model.** Watterson is the standard, and ITU-R
  F.520 "good/moderate/poor/flutter" parameter sets are the comparable test
  conditions — but real HF has phenomena Watterson doesn't capture (severe
  multipath, large Doppler spread, frequency-selective fading, non-Gaussian
  noise, QRM, aurora effects). Limitations are explicit and documented.

## §3. Forcing functions

1. **Watterson-class model.** Per the foundation doc §1.1, this is the
   canonical HF channel model: tapped-delay-line, magnetoionic Doppler spread,
   complex-Gaussian time-varying taps. Independent creation from the
   foundational papers (Watterson, Juroshek, Bensema 1970; ITU-R F.520;
   ITU-R F.1487) per ADR 0014 §1.
2. **ITU-R F.520 parameter sets.** Standardized "good," "moderate," "poor,"
   and "flutter" channel conditions. Every BER/throughput claim downstream
   must cite the F.520 channel condition under which it was measured.
3. **ITU-R F.1487 test methodology.** The standard for how an ionospheric
   channel simulator is used to evaluate HF modems up to 12 kHz. Compliance
   with this methodology is what makes performance claims comparable across
   labs.
4. **Cross-validation gate.** Per foundation doc §1.4 — the simulator's
   output statistics under standardized inputs are cross-validated against
   an independent open implementation (ITS or GNU Radio OOT module).
   Cross-validation is part of the subsystem's "done" definition; not an
   afterthought.
5. **Determinism and reproducibility.** Random number streams seeded
   explicitly; same seed + same input + same parameters → bit-identical
   output. Required for CI use as a regression-test substrate.
6. **Performance budget.** Must run faster than real-time on the development
   target (Pi 5 or x86 laptop, per overview Q4) so CI loops produce results
   in seconds, not hours. **Open question:** is real-time-equivalent sufficient
   or do we want, say, 10x real-time? [open]
7. **API shape.** Library-callable from tests (and from a hypothetical CLI
   for ad-hoc characterization). API stability is required since downstream
   subsystem tests depend on it.

## §4. Open design questions (subordinate to overview Q1-Q8)

| # | Question | Notes / what depends on it |
|---|---|---|
| §1.Q1 | Independent Rust implementation, or wrap an existing open implementation? | Independent implementation preserves clean-sheet posture; wrapping changes the citation chain. Decision interacts with overview Q7 (channel sim as own crate or in-tree). |
| §1.Q2 | Sample-rate / sample-format API — fixed (e.g., 48 kHz f32 I/Q) or parameterized? | Affects integration with downstream PHY tests. Tradeoff: simpler API vs. flexibility for different PHY sample rates. |
| §1.Q3 | Audio-band vs. baseband I/Q input? | Tuxmodem's PHY likely operates audio-band (after the radio's SSB demod); simulator's input format must match. |
| §1.Q4 | Channel-condition parameter representation — typed enum (Good/Moderate/Poor/Flutter) or numeric (multipath delay-spread + Doppler-spread)? | Enum is safer + matches ITU-R F.520; numeric is more flexible. Both are reasonable. |
| §1.Q5 | Multi-channel (frequency-selective) Watterson extension supported? | F.1487 allows up to 12 kHz; pure Watterson is 2-tap. tuxmodem likely doesn't need >2300 Hz so 2-tap is sufficient — but a multi-channel extension is "free" if added later. |
| §1.Q6 | Cross-validation reference — ITS, GNU Radio, both, or other? | Discussed in foundation doc §1.4. Settle here as part of subsystem #1's design. |
| §1.Q7 | Real-time-equivalent or accelerated? | Foreshadowed in §3.6 above. |
| §1.Q8 | Visualization / diagnostics — output BER vs. SNR curves, eye diagrams, scatter plots? | Useful for development; tradeoff is scope creep. |

## §5. Citations from foundation doc

- §1.1: Watterson, Juroshek, Bensema 1970 (foundational paper).
- §1.2: ITU-R F.520 (parameter sets), F.1487 (test methodology).
- §1.3: Davies, *Ionospheric Radio* (physics background).
- §1.4: Open implementations survey (ITS, GNU Radio, academic releases).
- §5.1: DSP fundamentals (Lyons, Smith dspguide.com).

## §6. No-implementation-choice markers

This STUB does not designate any of:

- Specific algorithm implementation
- Specific Rust crate dependencies
- Specific channel-condition default parameters
- Specific output sample format
- Specific testing format (e.g., golden BER tables vs. statistical assertions)

Those choices become part of the canonical subsystem #1 spec written **after**
operator approval of this STUB.

## §7. Dependencies

- **Upstream:** none. This is the most foundational subsystem.
- **Downstream consumers:** subsystems #3 (PHY), #4 (FEC), #6 (ARQ),
  #7 (link adaptation). Each of those subsystems' tests instantiates the
  channel simulator with parameterized channel conditions.

## §8. Watched failure modes

- **The "let me just check how VARA does it" temptation.** Per ADR 0014
  §2, this STOP rule applies to subsystem #1 too. Even though VARA's
  channel-handling approach is presumably not protected by copyright in
  the same way the waveform code is, examining VARA's behavior to
  inform channel-simulator parameters forfeits the independent-creation
  defense for the whole modem program. Don't.
- **Confusing the simulator's idealization for the real channel.** The
  simulator captures what Watterson + ITU-R F.520/F.1487 standardize.
  Real HF has phenomena outside that scope. A modem that demos perfectly
  in the simulator can still fail on-air; the RF measurement rig (#2) +
  the bench rig are the cross-checks.
- **Over-fitting the modem to the simulator's specific parameter sets.**
  Operator decisions in subsystem design (#3-#7) should weight performance
  across the F.520 envelope, not just optimize against one parameter set.

Agent: mink-swallow-kite
