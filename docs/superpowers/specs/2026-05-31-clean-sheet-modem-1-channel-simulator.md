# Subsystem #1 — HF channel simulator

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md);
> incorporates the §5.A.1 (multi-mode PHY ladder, bit-adaptive OFDM main +
> robustness-modes-family floor), §5.A.4 (AGPLv3-only license), §5.A.5 (standalone
> public crate from day one), and §5.A.6 (best-effort compute) decisions
> from the 2026-05-31 brainstorm.
>
> **Scope:** what the HF channel simulator subsystem does, what it does NOT
> do, the forcing functions on its design, and the open questions that
> remain to be settled during implementation. Pre-decides architectural
> commitments inherited from the program overview §5.A.

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
6. **Per-sub-carrier SNR estimation interface** (overview §5.A.1 + §5.B
   imply this). Bit-adaptive OFDM requires per-sub-carrier channel-quality
   characterization for bit-loading decisions; the simulator must expose
   per-frequency-bin SNR statistics over time, not only aggregate BER.
   This is not a stretch goal — it's a load-bearing capability for
   subsystem #3's PHY family validation.
7. **Performance budget — best-effort** (per overview §5.A.6). No
   pre-committed real-time multiplier; profile under actual workloads,
   optimize where bottlenecks appear. Pi 5 is primary dev target; faster
   x86 desktops are best-effort acceleration paths for long CI sweeps.
8. **API shape.** Library-callable from tests (and from a hypothetical CLI
   for ad-hoc characterization). API stability is required since downstream
   subsystem tests depend on it.
9. **Standalone AGPLv3 public crate** (per overview §5.A.4 + §5.A.5).
   Implemented as a separate Rust crate, published independently on
   crates.io, licensed AGPLv3-only, with its own README + dated commits
   serving as the contemporaneous citation chain for the foundational papers
   (Watterson 1970, ITU-R F.520, F.1487).

## §4. Open design questions (remaining at subsystem level)

| # | Question | Status / notes |
|---|---|---|
| §1.Q1 | Independent Rust implementation, or wrap an existing open implementation? | **RESOLVED — independent Rust implementation.** Per overview §5.A.5 (standalone crate, independent provenance for the citation chain). Wrap precluded by overview's clean-sheet posture. |
| §1.Q2 | Sample-rate / sample-format API — fixed (e.g., 48 kHz f32 I/Q) or parameterized? | Open. Tradeoff: simpler API vs. flexibility for different PHY sample rates. Settle during implementation. |
| §1.Q3 | Audio-band vs. baseband I/Q input? | Likely audio-band (tuxmodem's PHY operates after the radio's SSB demod). Confirm during subsystem #3 development. |
| §1.Q4 | Channel-condition parameter representation — typed enum (Good/Moderate/Poor/Flutter) or numeric (multipath delay-spread + Doppler-spread)? | Open. Enum is safer + matches ITU-R F.520; numeric is more flexible. Probably both — typed enum as primary API + numeric escape hatch. |
| §1.Q5 | Multi-channel (frequency-selective) Watterson extension supported? | Open. F.1487 allows up to 12 kHz; pure Watterson is 2-tap. Probably 2-tap initially; multi-channel extension is "free" if added later. |
| §1.Q6 | Cross-validation reference — ITS, GNU Radio, both, or other? | Open. Listed in overview §5.C as a subsystem-level open question. Probably both for confidence; settle during implementation as part of the "done" gate. |
| §1.Q7 | Visualization / diagnostics — output BER vs. SNR curves, eye diagrams, scatter plots? | Open. Useful for development; tradeoff is scope creep. Recommended: bare-bones text-output BER tables first, visualizations added if needed. |
| §1.Q8 | Crate name? | Open. Working suggestions: `hf-channel-sim`, `watterson-rs`. Decide before crates.io publication. |

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
