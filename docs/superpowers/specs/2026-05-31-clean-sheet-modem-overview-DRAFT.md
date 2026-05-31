# Clean-sheet HF modem program — overview (DRAFT)

> **Status: DRAFT — operator review pending.** This is a v0 draft written
> autonomously during the operator's 6-hour drive (2026-05-31, agent
> mink-swallow-kite). The brainstorming skill's HARD-GATE has NOT been cleared:
> the design has not been presented to and approved by the operator. This file
> exists as a markup target for that review, not as an approved spec. Until
> approval, no subsystem implementation work begins. After approval, the file
> is renamed (dropping `-DRAFT`) and committed as canonical.
>
> All quantitative targets in this draft are marked `[open: operator
> confirms]`. The decomposition, sequencing, and design-discipline framing are
> proposals; the open-question section at the end enumerates what the operator
> needs to settle.

## §0. Program scope

This is the **umbrella overview** for the v0.5+ clean-sheet HF modem program.
The program is too large for a single spec; this overview decomposes it into
sub-projects, names their relationships, and proposes their development order.
Each sub-project gets its own spec → plan → implementation cycle under this
umbrella.

**What the program is:**

A from-scratch HF data modem replacing VARA for tuxlink. Operationally usable
on amateur HF equipment representative of the operator's reference radios
(Xiegu G90 + Yaesu FT-818, per the bench-rig spec). Eventually packageable as
a standalone open-source TCP modem daemon (per ADR 0015) usable by non-tuxlink
clients (Pat / ARIM / etc.).

**What the program is NOT:**

- Not VARA-compatible. No bit-for-bit waveform interop. (ADR 0014; `project_v05_modem_design_posture` memory.)
- Not informed by examination of VARA's internals from any source — decompilation, leaked source, RE write-ups, black-box on-air. (ADR 0014, bright line.)
- Not constrained by a community-adoption migration story. Optimize for technical merit only; operator has community reach to drive adoption of a technically-superior alternative. (`project_v05_modem_design_posture` memory.)
- Not a bridge or interop layer. Full replacement.

**Success-criterion shape (quantitative targets [open: operator confirms]):**

- Total occupied bandwidth: ≤2300 Hz [open: confirm 2300 vs. wider]. **Rationale for the 2300 Hz proposal:** the FT-818's stock SSB IF filter (no Collins YF-122S upgrade) cleanly passes ≤2300 Hz; designs wider than this become "supported on better radios, degraded on FT-818." Operator's installed-base assumptions drive this target.
- Decode threshold under ITU-R F.520 "moderate" HF channel conditions: [open: SNR target in dB].
- Net throughput at moderate channel: [open: kbps target].
- ARQ-corrected end-to-end reliability under typical channel impairment: [open: target percentage at named conditions].
- Compatibility with Cameron's reference radios: works on G90 + FT-818 over the bench rig (`docs/hardware/bench-rig-two-host-topology.md`).
- Independent-creation defense preserved per ADR 0014 throughout the project lifecycle.

## §1. Subsystem decomposition

The program decomposes into ten sub-projects. Each is independently spec-able,
plan-able, and ship-able; cross-subsystem dependencies are explicit. Numbering
matches the bench-rig spec's references and the foundation doc's
`Relevance:` tags.

| # | Sub-project | One-line role |
|---|---|---|
| 0 | **Program overview** (this doc) | Goals, success criteria, design discipline, sequencing, decomposition itself. |
| 1 | **Channel simulator** | Software HF ionospheric channel emulator (Watterson-class). DSP-first validation harness — every PHY candidate is tested against it offline before RF. |
| 2 | **RF measurement rig** | Calibrated hardware. SDR + directional coupler + step attenuator topology, per `project_rf_measurement_rig_design`. Validates real radios + characterizes the real channel. |
| 3 | **PHY / waveform** | Modulation, sub-carrier structure (if OFDM-family), sync, frame detection. The "what does it sound like on air" layer. |
| 4 | **FEC** | Forward error correction. Could fold into #3 or stay separate depending on architecture choice. |
| 5 | **Link / MAC** | Frame format, framing, headers, addressing, station identification. |
| 6 | **ARQ** | Retransmission strategy, selective-repeat vs. go-back-N vs. hybrid, ACK design, window sizing. |
| 7 | **Link adaptation** | Mode-stepping based on observed channel quality (SNR, BER, throughput). |
| 8 | **Host protocol / control plane** | API between client and modem. ADR 0015's "open question" — must be settled before subsystems #5/#6 freeze. |
| 9 | **Integration in tuxlink** | `ModemTransport` plugin per ADR 0015. Supervised process lifecycle, sound-card-contention enforcement, audio-device handoff. |
| 10 | **Standalone daemon packaging** | Spin-off as open-source TCP modem usable by Pat / ARIM / etc. Per ADR 0015's preserved optionality. |

## §2. Per-subsystem descriptions

### §2.0 Program overview (this doc) — the umbrella

Establishes the program's scope, goals, success criteria, design discipline,
and sequencing. Cited by every downstream subsystem spec as the canonical
"why." Lives at `docs/superpowers/specs/<date>-clean-sheet-modem-overview.md`
once approved.

### §2.1 Channel simulator — the validation harness

A software implementation of the ITU-R standardized HF ionospheric channel
model (Watterson-class — Watterson, Juroshek, Bensema 1970; ITU-R F.520 +
F.1487 standardized channel parameters). Takes baseband I/Q + channel-condition
parameter set ("good" / "moderate" / "poor" / "flutter") and produces channel-
impaired baseband I/Q.

**Why this is sub-project #1:** standard DSP-first development methodology.
Every PHY candidate (#3), every FEC choice (#4), every link adaptation strategy
(#7) is validated against the channel simulator in software-only loops before
any RF rig is needed. BER-vs-SNR curves under standardized channel conditions
become the comparable performance metric across design iterations.

**What it produces:** a Rust crate (or similar) callable from tests; runs in
CI; produces BER/throughput characterization reports for any candidate PHY.

**Inputs from upstream:** none (foundational).
**Consumers:** #3, #4, #6, #7 — all DSP subsystems.

### §2.2 RF measurement rig

Calibrated hardware for characterizing real radios and the real channel.
Topology and component plan are in `project_rf_measurement_rig_design`
memory + the bench-rig spec. RTL-SDR V4 first-slice, RX-888 MkII upgrade path,
directional coupler + step attenuator chain.

**Why this is parallel-track:** doesn't gate the modem design (software
channel sim does that), but it's the ground-truth verifier for any claim about
how the modem actually performs on real radios. Built alongside #1 once
hardware acquisition lands.

**What it produces:** calibrated measurement capability for TX characterization
of the operator's radios (per ADR 0014 §4, explicitly in-scope), and an
independent RF capture path for cross-validation of the bench rig.

**Inputs:** hardware acquisition.
**Consumers:** #3, #9 (integration testing).

### §2.3 PHY / waveform

The physical-layer modem: what the modem sounds like on air. Modulation choice
(constellation density vs. SNR requirement tradeoff), sub-carrier structure
(if OFDM-family — primitive concept per OFDM foundations §2.3 of foundation
doc), synchronization (carrier offset, symbol timing, frame sync), framing
detection.

**Forcing functions:**
- Total occupied bandwidth ≤2300 Hz [open: confirm].
- Operable on G90 + FT-818 (per bench rig). FT-818 specifically constrains
  audio passband shape, dynamic range, AGC interaction.
- Performance against Watterson-channel-simulated impairment under ITU-R F.520
  "moderate" conditions [open: SNR / BER target].

**What it produces:** a PHY implementation, BER/SNR characterization reports
from the channel simulator, RF-validated performance reports from the bench
rig.

**Inputs:** #1 (validation), #4 (FEC layer below it), #8 (host-protocol API
for upper-layer interface).
**Consumers:** #5, #9.

### §2.4 FEC

The forward error correction layer. Choice between block codes (Reed-Solomon),
convolutional codes (Viterbi-decoded), modern codes (LDPC, polar, turbo).
Code-rate flexibility. Soft-decision vs. hard-decision decoder. Decoder
complexity budget.

**Forcing functions:**
- Decoder complexity must run in real time on the deployment target hardware
  (Pi 5 or similar) [open: define deployment-target compute budget].
- Code structure compatible with the PHY's frame layout.
- Performance margin against Watterson-channel error patterns.

**What it produces:** an FEC implementation (encoder + decoder), BER-vs-SNR
characterization with and without the FEC, runtime performance benchmarks.

**Inputs:** #1.
**Consumers:** #3 (folded into the PHY waveform), #6 (FEC interacts with ARQ
in hybrid-ARQ designs).

### §2.5 Link / MAC

Frame format, framing, headers, addressing. Station identification. Frame
sequence numbering. Connection-state machine (if connection-oriented).

**Forcing functions:**
- Station ID per Part 97 — every transmission must identify the licensed
  station. (Operator-level requirement; spec must reflect.)
- Frame structure aligned with PHY frame detection.
- Header overhead vs. payload tradeoff [open: target overhead percentage at
  typical message size].

**What it produces:** frame layout spec, addressing scheme, link state machine.

**Inputs:** #3 (PHY frame structure constrains this).
**Consumers:** #6, #7, #8.

### §2.6 ARQ

Reliable-delivery mechanism above the lossy PHY+FEC layer. Selective-repeat
vs. go-back-N vs. hybrid. ACK / NACK scheme. Window sizing. Retransmission
backoff. Hybrid ARQ (combining FEC redundancy + retransmission) — Type-I,
Type-II, or Type-III if applicable.

**Forcing functions:**
- Round-trip-time budget for HF (multi-second RTT typical) → large windows
  required for throughput.
- Channel-burst-error pattern (channel sim characterization) → selective-
  repeat preferred over go-back-N at high BER.
- Memory budget for held-but-unACKed frames.

**What it produces:** ARQ implementation, throughput-vs-channel characterization.

**Inputs:** #1, #4, #5.
**Consumers:** #7, #8, #9.

### §2.7 Link adaptation

Dynamic mode-stepping based on observed channel quality. The modem starts
optimistically (highest-density modulation, lowest FEC overhead) and steps
down to robust modes as SNR / BER degrades. Could be operator-commanded or
fully automatic.

**Forcing functions:**
- Number of discrete modes (typically 2–4 in HF data systems).
- Mode-switching latency.
- Hysteresis (avoid rapid mode-flapping under marginal channel).

**What it produces:** link-adaptation policy implementation, channel-quality
metric definitions, mode-switch threshold parameters.

**Inputs:** #1, #3, #4, #6.
**Consumers:** #9.

### §2.8 Host protocol / control plane — ADR 0015's open question

The API between the modem (process or library) and the client (tuxlink or
any other consumer of the standalone modem daemon). ADR 0015 explicitly
defers this:

> "Open (deferred): Host-protocol / clean-sheet line for the eventual standalone
> modem (the on-air protocol is clean-sheet per ADR 0014; the host-side control
> API is argued: NOT bound by clean-sheet — settle before the modem spec)."

**The argument from ADR 0015** is that the on-air protocol is bound by ADR 0014's
clean-sheet rule, but the host-side control API is not — because the host API
is a software interface design choice rather than a transmitted waveform that
faces the IP-defense question.

**Key open questions for §2.8 design (decision needed before #5/#6 freeze):**

- TCP / Unix-domain-socket / stdio / shared-memory / D-Bus / etc.?
- Command protocol style — textual (KISS-like, KENWOOD-AT-style, or new) vs.
  binary?
- Standardize against any prior art (`hostmode`, KISS, `direwolf` interface,
  ardopcf protocol) for cross-implementation portability? This is the
  ADR-0015-flagged question: yes/no, and if yes, which?
- Versioning + capability negotiation discipline.

**What it produces:** host-protocol specification, reference parser/serializer,
integration test fixtures.

**Inputs:** #5 (frame structure constrains naming), #6 (ARQ state must be
addressable from the API).
**Consumers:** #9, #10.

### §2.9 Integration in tuxlink

The `ModemTransport` plugin per ADR 0015: managed spawn of the modem process,
supervised lifecycle (SIGINT-clean-stop, confirm-audio-device-released-before-
swap), generic abstraction so the same code path drives tuxmodem and existing
external modems (ardopcf, Dire Wolf, future others).

**Forcing functions:**
- ADR 0015's lifecycle ownership.
- Existing `ModemTransport` trait (already defined for ardopcf integration).
- Consent-gate alignment with RADIO-1.

**What it produces:** new `ModemTransport` implementation for tuxmodem,
integration tests against the ARDOP-pattern test suite.

**Inputs:** #8.
**Consumers:** end-users.

### §2.10 Standalone daemon packaging

Spin off tuxmodem as a standalone open-source TCP modem usable by clients
other than tuxlink (Pat / ARIM / etc.). Inverts who owns rig control (the
client owns rig control, the daemon owns the modem). Per ADR 0015's preserved
optionality.

**Forcing functions:**
- API stability commitment (versioning, deprecation window).
- License choice (likely permissive: MIT, Apache 2.0, or similar — to maximize
  adoption — but operator-decision).
- Packaging discipline (Debian + RPM at minimum, ideally Homebrew + Windows
  installer eventually).

**What it produces:** standalone daemon release, public repository, packaging.

**Inputs:** #8, #9.
**Consumers:** external software.

## §3. Sequencing rationale — DSP-first

**Recommended development order:** 0 → 1 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10,
with #2 (RF rig) developed in parallel from when there's a candidate PHY to
validate.

**Why DSP-first (#1 before #3):** standard SDR-development practice. The
channel simulator is the validation harness; building it first means every
later PHY iteration, FEC choice, and adaptation policy is measurable against
a standardized, reproducible, software-only test. Without the simulator, PHY
iteration depends on RF-rig availability, which throttles iteration speed by
orders of magnitude.

**Why #4 (FEC) before #5 (MAC):** the FEC's frame structure constrains what
the MAC layer's framing can look like. If FEC is folded into the PHY (likely),
then #4 and #3 ship together.

**Why #6 (ARQ) before #7 (link adaptation):** link adaptation needs channel-
quality observations the ARQ layer naturally surfaces (frame error rate,
retransmission count). Building #7 against #6 means link adaptation policy
sees real protocol-level signals, not just PHY-level demod metrics.

**Why #8 (host protocol) is timed where it is:** must be settled before #5 /
#6 freeze, because frames + ARQ state must be addressable from the API. But
the protocol *choice* (TCP / Unix socket / etc.) doesn't gate #1 / #3 / #4 —
those subsystems run in pure software loops without needing an external API.

**Why #2 (RF rig) is parallel:** the rig characterizes real radios, not the
modem itself; it's needed for ground-truth validation of #3 once a candidate
PHY exists, but it doesn't gate the early subsystem builds. Hardware
acquisition and rig assembly happen on a separate timeline.

**Why #9 / #10 are last:** integration and packaging happen after the modem
is technically working. Premature integration churns the integration layer
against an unstable substrate.

## §4. Design discipline — operationalizing ADR 0014

ADR 0014 establishes the clean-sheet posture. This overview operationalizes it
in five rules every subsystem spec must follow:

1. **Source provenance discipline.** Every design choice cites at least one
   open-source reference from `docs/research/modem-foundations.md`. The
   citation isn't bureaucracy — it's the contemporaneous record that the
   choice came from open sources, which is what the independent-creation
   defense rests on.

2. **No VARA in the citation chain.** If a subsystem reference depends on a
   citation that depends on VARA-internal material, the chain is broken at the
   point of dependency. The chain stops; the alternative path is found.

3. **Operator-confirmed observables are background only.** Per ADR 0014 §3,
   what the operator already knows from licensed operation of VARA (≈2300 Hz
   bandwidth, OFDM-based) is background — informs the design space framing
   but is NOT cited as design input. Specific parameters from operator's
   observations are NOT inherited into tuxmodem.

4. **Own-equipment characterization is in-scope.** Per ADR 0014 §4, the
   RF measurement rig characterizes the operator's own radios + the HF
   channel; this is explicitly permitted and informs the modem design.
   Pointing measurement at any VARA emission is the forbidden activity.

5. **The "I'll just check how VARA does it" temptation has a STOP rule.**
   Per ADR 0014 §2 + `project_v05_modem_design_posture` memory: that single
   act forfeits the independent-creation defense. If a contributor — human or
   AI — feels the urge, the discipline is STOP. Subsystem specs must include
   the watched-failure-mode entry "this section's framing might tempt
   investigation of prior art; stop instead."

## §5. Open questions (operator must settle)

| # | Question | Why it matters | Default-if-no-answer (proposed) |
|---|---|---|---|
| Q1 | Total occupied bandwidth target — confirm 2300 Hz vs. wider? | Caps PHY design space. FT-818 stock filter forces ≤2300 Hz for installed-base compatibility; wider designs require Collins YF-122S filter or limit deployment to better-equipped radios. | 2300 Hz |
| Q2 | Decode threshold under ITU-R F.520 "moderate" channel conditions — target SNR in dB for usable decode? | Defines the channel-condition envelope the modem must operate over. Affects PHY constellation density + FEC overhead. | [open] |
| Q3 | Net throughput target — kbps at moderate channel, kbps at good channel? | Caps PHY data rate. Tradeoff against #Q2. | [open] |
| Q4 | Deployment-target compute budget — Pi 5 / x86 / ARM / etc.? | Caps FEC decoder complexity + PHY DSP complexity. | Pi 5 + x86 reasonable laptop |
| Q5 | Host-protocol-API form — TCP vs. Unix socket vs. stdio vs. shared memory? Standardize against any prior art? | Caps deployment + spin-off design. ADR 0015's deferred question. | TCP, novel protocol, versioned |
| Q6 | License for the standalone modem daemon spin-off — MIT / Apache 2.0 / GPLv3 / dual-license? | Caps adoption. Permissive license maximizes adoption; copyleft constrains downstream incorporation. | MIT or Apache 2.0 |
| Q7 | Should the channel simulator be its own crate (public) or live inside tuxmodem? | Affects whether external researchers can reuse it; affects independent-creation defense (a public Watterson simulator is a public artifact). | Standalone open-source crate |
| Q8 | Bench-rig second host — which machine becomes Host B? | Caps the timeline for the bench-rig becoming operational. | Operator picks — laptop / Pi / mini-PC |

## §6. References

### Internal

- ADR 0014 — Clean-sheet modem; no prior-art examination.
- ADR 0015 — Modem integration and rig-control foundation.
- `docs/hardware/modem-test-rig.md` — VHF/UHF FM modem hardware chain (CDM-1550LS+).
- `docs/hardware/bench-rig-two-host-topology.md` — HF bench rig (G90 + FT-818, two hosts), session 2026-05-31.
- `docs/research/modem-foundations.md` — Citation library for the program, session 2026-05-31.
- Memory: `project_v05_modem_design_posture`, `project_rf_measurement_rig_design`, `project_g90_vara_standard_works_firsthand`, `feedback_clean_sheet_concepts_only`, `feedback_ai_amateur_radio_reliability`.

### External

See `docs/research/modem-foundations.md` for the full annotated bibliography.
This overview cites specific sources by name where directly relevant; the
foundation doc is the canonical pointer set.

---

Agent: mink-swallow-kite
