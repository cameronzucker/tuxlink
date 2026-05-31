# Open-source foundations for the clean-sheet HF modem

> **Purpose.** This is the **citation library** for the v0.5+ clean-sheet HF modem
> design. Per ADR 0014, the modem is "designed clean-sheet from open, general
> engineering knowledge: modem theory (OFDM/QAM/PSK, FEC, ARQ), published
> academic and general amateur digital-mode literature, and first principles" and
> "design provenance should cite open sources; contributors must not introduce
> VARA-internal material into the design record." This document operationalizes
> that requirement by enumerating the open sources downstream design specs draw
> from.
>
> **What this is NOT.** It is not a synthesis. It does not pre-decide modem
> design choices. It does not reproduce protected content from copyrighted works
> — references cite the works by publication metadata for downstream consultation,
> not in-line reproduction.
>
> **What "clean-sheet" forbids.** Per ADR 0014 + the `feedback_clean_sheet_concepts_only`
> memory entry: this bibliography lists works that surface *conceptual primitives
> and failure modes*. It does NOT list works that would be adopted as specific
> protocol or waveform choices for tuxlink's modem. Research is for understanding
> the design space, not for cribbing a specific design.
>
> **Bright-line exclusions.** No reference to VARA's internals (decompilation,
> leaked source, third-party reverse-engineering write-ups, or black-box on-air
> characterization of VARA) appears in this bibliography or any of the works
> cited. If a reader notices such a reference creeping in via citation chain,
> remove it (the citation, not the source-work — the modem program just stops
> drawing from any specific work that requires that material).

## How to use this document

Each entry below carries:

- **Full citation** with publication metadata (author, title, venue, year, identifier).
- **Provenance**: open-access / public-domain / paywalled-but-available / etc. Tells you whether the source is freely linkable or requires institutional access.
- **Relevance**: which subsystem of the modem program (per the [program overview DRAFT](../superpowers/specs/2026-05-31-clean-sheet-modem-overview-DRAFT.md)) the source informs.
- **Key concepts / methods**: brief enumeration of what the source contributes — primitives, not choices.

Downstream subsystem design specs cite from here. When a new source enters scope,
add an entry; when a source turns out to be inapplicable, leave the entry but
mark `[deprecated-for-this-program]` rather than deleting (a documented disposition
is part of the design provenance record).

---

## §1. HF channel modeling — channel simulator subsystem

### 1.1 The Watterson model (canonical HF channel emulator)

- **Watterson, C.C., J.R. Juroshek, W.D. Bensema.** "Experimental Confirmation of an HF Channel Model." *IEEE Transactions on Communication Technology*, vol. COM-18, no. 6, December 1970, pp. 792–803.
- **Provenance:** Paywalled (IEEE Xplore). Publication metadata is openly citeable; the model itself has been independently re-described in many later works without copyright concern.
- **Relevance:** **Channel simulator subsystem (#1).** This is the standard academic HF ionospheric channel model — a two-tap, time-varying complex Gaussian channel with magnetoionic delay spread and Doppler spread parameters. Every HF modem PHY can be validated against a Watterson-channel implementation in software before the RF rig is involved. This is the DSP-first methodology's anchor.
- **Key concepts:** Tapped-delay-line channel model; magnetoionic Doppler spread; ITU-R-defined "good / moderate / poor" channel parameter sets; baseband-equivalent simulation; bit-error-rate (BER) vs. signal-to-noise ratio (SNR) characterization at standardized channel conditions.

### 1.2 ITU-R recommendations on HF data + channel simulators

- **ITU-R Recommendation F.520-2.** "Use of high-frequency radiotelegraph circuits for data transmission." International Telecommunication Union.
- **Provenance:** Open-access (ITU publishes recommendations as freely-readable PDFs on www.itu.int).
- **Relevance:** **Program overview (#0), channel simulator (#1), link adaptation (#7).** ITU-R F.520 defines the standardized HF channel parameter sets ("good," "moderate," "poor" — and "flutter") that the Watterson model takes as inputs. Without these standardized test conditions, no modem performance claim is comparable across designs.
- **Key concepts:** Channel parameter standardization; HF circuit characterization for data; test-condition vocabulary used by every subsequent HF modem evaluation paper.

- **ITU-R Recommendation F.1487.** "Testing of HF modems with bandwidths up to 12 kHz using ionospheric channel simulators." International Telecommunication Union, 2000.
- **Provenance:** Open-access (ITU).
- **Relevance:** **Channel simulator (#1), PHY (#3).** Specifies how to use an ionospheric channel simulator (Watterson-class) to evaluate HF modems up to 12 kHz bandwidth. Defines what "tested against ionospheric simulator" means in a way that's comparable across labs.
- **Key concepts:** Simulator-based testing methodology; test-signal generation; SNR-vs-channel-condition characterization; measurement uncertainty bounds.

### 1.3 HF propagation general references

- **Davies, K.** *Ionospheric Radio*. IEE/Peter Peregrinus, 1990.
- **Provenance:** Standard textbook; widely available in academic libraries.
- **Relevance:** **Channel simulator (#1), RF measurement rig (#2), link adaptation (#7).** Authoritative reference on ionospheric physics relevant to HF channel behavior — F-layer dynamics, multipath structure, NVIS geometry, MUF/LUF variation. Substrate for understanding *why* the Watterson model captures what it captures.
- **Key concepts:** Ionospheric layer structure; sky-wave vs. ground-wave; near-vertical incidence skywave (NVIS) geometry; diurnal and solar-cycle variation; absorption loss models.

- **Wikipedia: "High frequency."** Open-access overview at https://en.wikipedia.org/wiki/High_frequency.
- **Provenance:** Open-access (CC-BY-SA).
- **Relevance:** **Program overview (#0), RF measurement rig (#2).** Useful as a fast orientation for a contributor coming in cold; pointers into the standard references above.

---

## §2. General modem theory — PHY, FEC, ARQ subsystems

### 2.1 Foundational textbooks

- **Proakis, J.G., M. Salehi.** *Digital Communications*. 5th edition, McGraw-Hill, 2008. ISBN 978-0072957167.
- **Provenance:** Paywalled (commercial textbook).
- **Relevance:** **PHY (#3), FEC (#4), MAC/link (#5).** Authoritative academic reference for digital communications. Chapters on coherent and noncoherent detection, signal-space analysis, optimum receiver structures, channel capacity, equalization, and synchronization. Substrate for every PHY design decision.
- **Key concepts:** Signal-space representation; matched-filter detection; minimum-distance demodulation; Nyquist signaling; intersymbol interference; equalizer design; Maximum-likelihood sequence estimation.

- **Sklar, B.** *Digital Communications: Fundamentals and Applications*. 2nd edition, Prentice Hall, 2001. ISBN 978-0130847881.
- **Provenance:** Paywalled (commercial textbook).
- **Relevance:** **PHY (#3), FEC (#4).** Companion / lighter-touch reference to Proakis. Strong on link-budget analysis, modulation comparison, and FEC fundamentals presented for engineers rather than pure mathematicians.
- **Key concepts:** Link budget; modulation bandwidth-efficiency vs. power-efficiency tradeoffs; FEC fundamentals; turbo coding intro.

- **Haykin, S.** *Communication Systems*. 5th edition, Wiley, 2009. ISBN 978-0471697909.
- **Provenance:** Paywalled (commercial textbook).
- **Relevance:** **PHY (#3).** Strong on stochastic signal processing fundamentals, noise analysis, and adaptive equalization.
- **Key concepts:** Random-process theory; noise figures; spectral estimation; adaptive filtering theory.

### 2.2 Information theory (capacity bounds)

- **Shannon, C.E.** "A Mathematical Theory of Communication." *Bell System Technical Journal*, vol. 27, July & October 1948, pp. 379–423 & 623–656.
- **Provenance:** Open-access (Bell Labs has posted historical reprints; widely available in PDF form).
- **Relevance:** **Program overview (#0), PHY (#3), FEC (#4).** Foundational. Capacity bound for any communication channel; sets the absolute theoretical ceiling against which any modem's performance is measured.
- **Key concepts:** Channel capacity; mutual information; coding theorem; the BER-vs-SNR bound that no modem can exceed (and from which modem design is the practical engineering of how close to approach).

### 2.3 OFDM (often-relevant PHY family)

- **Wikipedia: "Orthogonal frequency-division multiplexing."** https://en.wikipedia.org/wiki/Orthogonal_frequency-division_multiplexing.
- **Provenance:** Open-access (CC-BY-SA). Fetched 2026-05-31; full content cached in research artifacts.
- **Relevance:** **PHY (#3).** Conceptual overview of OFDM as a PHY family: orthogonal sub-carriers, cyclic prefix, FFT-based modulation/demodulation, frequency-domain equalization.
- **Key concepts (NOT choices for tuxlink — primitives to understand):** Sub-carrier orthogonality; PAPR vs. linearity tradeoff (relevant given the FT-818 / G90 final-amp dynamic range); cyclic prefix vs. multipath delay-spread budget; pilot-based channel estimation.

- **Cimini, L.J.** "Analysis and Simulation of a Digital Mobile Channel Using Orthogonal Frequency Division Multiplexing." *IEEE Transactions on Communications*, vol. COM-33, no. 7, July 1985, pp. 665–675.
- **Provenance:** Paywalled (IEEE Xplore).
- **Relevance:** **PHY (#3).** Seminal OFDM paper applied to mobile communications; the analytical framework transfers cleanly to HF (HF and mobile share multipath + Doppler structure, differ in coherence-time parameters).

### 2.4 QAM / PSK / FSK (modulation families)

- **Wikipedia: "Quadrature amplitude modulation."** https://en.wikipedia.org/wiki/Quadrature_amplitude_modulation.
- **Provenance:** Open-access (CC-BY-SA). Fetched 2026-05-31.
- **Relevance:** **PHY (#3).** Conceptual overview of QAM as a constellation-based modulation family. Constellation density vs. SNR requirement tradeoff. Square-QAM, cross-QAM, non-rectangular constellations.
- **Key concepts:** Constellation design; demodulation under additive Gaussian noise; bit-to-symbol mapping; Gray coding; constellation shaping.

### 2.5 Synchronization (often the unsung difficulty)

- **Meyr, H., M. Moeneclaey, S. Fechtel.** *Digital Communication Receivers: Synchronization, Channel Estimation, and Signal Processing*. Wiley, 1997. ISBN 978-0471502753.
- **Provenance:** Paywalled (commercial textbook).
- **Relevance:** **PHY (#3).** Receiver-side synchronization — carrier recovery, timing recovery, frame sync — is often where prototype PHYs fall apart even when modulation/demodulation works in clean-channel simulation. Authoritative reference.
- **Key concepts:** Carrier-frequency offset estimation; symbol-timing recovery; frame-sync detection under noise; pilot-aided vs. blind synchronization.

---

## §3. FEC literature — FEC subsystem (#4)

### 3.1 Foundational coding

- **Reed, I.S., G. Solomon.** "Polynomial Codes Over Certain Finite Fields." *Journal of the Society for Industrial and Applied Mathematics*, vol. 8, no. 2, June 1960, pp. 300–304.
- **Provenance:** Paywalled (SIAM); widely re-described in textbooks.
- **Relevance:** **FEC (#4).** Defines Reed-Solomon codes. Used in essentially every burst-error-tolerant communication system since (CD-ROM, DVB, deep-space, packet radio). Strong block-error performance, well-understood decoding complexity.
- **Key concepts:** Block code over finite field; minimum distance; erasure decoding; concatenation with convolutional inner code; burst-error correction capability.

- **Viterbi, A.J.** "Error Bounds for Convolutional Codes and an Asymptotically Optimum Decoding Algorithm." *IEEE Transactions on Information Theory*, vol. IT-13, no. 2, April 1967, pp. 260–269.
- **Provenance:** Paywalled (IEEE Xplore).
- **Relevance:** **FEC (#4).** The Viterbi algorithm — optimal soft-decision decoder for convolutional codes. Used historically in deep-space + satellite + GSM voice channels. Constraint-length-vs-decoder-complexity tradeoff.
- **Key concepts:** Convolutional code; trellis structure; Viterbi MLSD decoding; soft-decision vs. hard-decision metric; constraint-length tradeoff.

- **Gallager, R.G.** "Low-Density Parity-Check Codes." Sc.D. thesis, MIT, 1963. (Republished as *Low-Density Parity-Check Codes*, MIT Press, 1963.)
- **Provenance:** Open-access (MIT thesis library: https://dspace.mit.edu/).
- **Relevance:** **FEC (#4).** LDPC origin paper. LDPCs were rediscovered in the 1990s and are now standard in Wi-Fi (802.11n+), DVB-S2, 5G. Near-capacity performance with iterative belief-propagation decoding.
- **Key concepts:** Sparse parity-check matrix; iterative belief-propagation decoding; Tanner graph representation; near-Shannon-capacity performance.

- **Berrou, C., A. Glavieux, P. Thitimajshima.** "Near Shannon Limit Error-Correcting Coding and Decoding: Turbo-Codes." *Proc. IEEE International Conference on Communications*, May 1993, pp. 1064–1070.
- **Provenance:** Paywalled (IEEE Xplore).
- **Relevance:** **FEC (#4).** Turbo codes — concatenated convolutional codes with iterative MAP decoding. Used in 3G/4G cellular. Comparable performance to LDPC at different complexity tradeoffs.

- **Arikan, E.** "Channel Polarization: A Method for Constructing Capacity-Achieving Codes for Symmetric Binary-Input Memoryless Channels." *IEEE Transactions on Information Theory*, vol. 55, no. 7, July 2009, pp. 3051–3073.
- **Provenance:** Paywalled (IEEE Xplore); preprint available via arXiv (https://arxiv.org/abs/0807.3917).
- **Relevance:** **FEC (#4).** Polar codes. Provably capacity-achieving on symmetric binary-input memoryless channels. Adopted in 5G control-channel coding. Different design philosophy than turbo/LDPC; potentially useful for short message lengths.

### 3.2 Coding theory overview

- **Costello, D.J., G.D. Forney.** "Channel Coding: The Road to Channel Capacity." *Proceedings of the IEEE*, vol. 95, no. 6, June 2007, pp. 1150–1177.
- **Provenance:** Paywalled (IEEE Xplore); open preprints occasionally circulate.
- **Relevance:** **FEC (#4).** Tutorial review of FEC history from Shannon to modern near-capacity codes. Useful for orienting which code family fits which constraint set (block-error vs. random-error performance, decoder-complexity budget, latency requirement, code-rate flexibility).

- **Wikipedia: "Forward error correction" / "Error correction code."** Open-access overviews at https://en.wikipedia.org/wiki/Forward_error_correction.
- **Provenance:** Open-access (CC-BY-SA). Fetched 2026-05-31.
- **Relevance:** **FEC (#4).** Fast orientation for a contributor.

- **Wikipedia: "Reed-Solomon error correction"**, **"Low-density parity-check code"**, **"Convolutional code"**, **"Polar code (coding theory)"**. All open-access (CC-BY-SA). Fetched 2026-05-31.
- **Relevance:** **FEC (#4).** Per-family overviews; pointers into the canonical papers.

---

## §4. ARQ literature — ARQ subsystem (#6)

### 4.1 ARQ schemes

- **Lin, S., D.J. Costello.** *Error Control Coding*. 2nd edition, Prentice Hall, 2004. ISBN 978-0130426727.
- **Provenance:** Paywalled (commercial textbook).
- **Relevance:** **FEC (#4), ARQ (#6).** Authoritative textbook covering both FEC and ARQ in one volume. Stop-and-wait, go-back-N, selective-repeat ARQ; hybrid ARQ (HARQ) combining FEC + retransmission.
- **Key concepts:** ARQ throughput-vs-latency tradeoffs; window sizing; selective-repeat vs. go-back-N performance under burst errors; Type-I / Type-II / Type-III HARQ.

- **Bertsekas, D.P., R.G. Gallager.** *Data Networks*. 2nd edition, Prentice Hall, 1992. ISBN 978-0132009164.
- **Provenance:** Paywalled (commercial textbook); chapters on ARQ widely available in academic settings.
- **Relevance:** **ARQ (#6), MAC (#5).** Authoritative on ARQ throughput analysis under various channel and traffic models.

- **Wikipedia: "Automatic repeat request."** https://en.wikipedia.org/wiki/Automatic_repeat_request.
- **Provenance:** Open-access (CC-BY-SA). Fetched 2026-05-31.
- **Relevance:** **ARQ (#6).** Quick orientation; pointers into the canonical references.

---

## §5. Software-defined radio + DSP-first methodology — channel simulator + PHY

### 5.1 DSP-first development practice

- **Lyons, R.G.** *Understanding Digital Signal Processing*. 3rd edition, Prentice Hall, 2010. ISBN 978-0137027415.
- **Provenance:** Paywalled (commercial textbook).
- **Relevance:** **All DSP-bearing subsystems (#1, #3, #4).** Practitioner-oriented DSP reference. Discrete-time signal processing, FFT, multi-rate (decimation/interpolation), digital filter design, real-time considerations.
- **Key concepts:** DFT/FFT efficiency; polyphase filter banks; multi-rate signal processing; fixed-point vs. floating-point implementation; numerical stability.

- **Smith, S.W.** *The Scientist and Engineer's Guide to Digital Signal Processing*. California Technical Publishing, 1997.
- **Provenance:** **Open-access** — author has made the full text freely available at http://www.dspguide.com/.
- **Relevance:** **All DSP-bearing subsystems (#1, #3, #4).** Free, accessible DSP reference. Good orientation for contributors without a formal DSP background.

### 5.2 SDR tooling (development substrate, not protocol adoption)

- **GNU Radio Project.** Open-source software-defined radio framework. https://www.gnuradio.org/.
- **Provenance:** Open-source (GPL).
- **Relevance:** **Channel simulator (#1), PHY (#3) — prototyping substrate only.** GNU Radio is widely used in academic and amateur SDR development as a DSP prototyping environment. Useful for early PHY exploration. *Not* a deployment runtime decision — that question is separately decided in the program overview.
- **Key concepts:** Block-flow signal processing; out-of-tree modules; UHD radio hardware abstraction; SoapySDR multi-vendor SDR support.

---

## §6. Amateur digital-mode protocol references (conceptual only — clean-sheet means concepts only)

> **Important.** Per the `feedback_clean_sheet_concepts_only` memory entry: this
> section enumerates amateur digital-mode protocols **for failure-mode and
> primitive-concept reference only**. It explicitly does NOT designate any of
> these as a design to clone, partially copy, or be bit-compatible with. The
> "obscurity of prior art is signal about engineering and execution quality;
> adopting specific choices inherits failure modes" rule applies. Read these
> for what they reveal about *the design space* and *what to watch out for*,
> not for what to put in tuxlink.

### 6.1 Open amateur HF data-mode references

- **WSJT-X / FT8 / FT4 / JS8 / JS8Call.** Joe Taylor K1JT et al. WSJT-X documentation and source code. Open-access at https://wsjt.sourceforge.io/ (WSJT-X) and https://files.js8call.com/ (JS8Call).
- **Provenance:** Open-source (GPL); design documentation publicly available.
- **Relevance:** **PHY (#3), FEC (#4), ARQ (#6) — conceptual reference.** FT8 / FT4 are weak-signal narrow-band modes; JS8 is a derived conversational mode. Useful for understanding LDPC-on-HF in practice (FT8 uses LDPC(174,91)), time-synchronized weak-signal protocol design, narrow-band coexistence. **Do not adopt specific parameters.**
- **Key concepts to absorb:** weak-signal demodulation under noise floor; tightly-time-synchronized protocol structure; narrow-band coexistence assumptions; LDPC short-block design.

- **Steinberg, S., et al.** "Work the World With WSJT-X, Part 1: Operating Capabilities" + "Part 2: Codes, Modes, and Cooperative Software Development." *QEX*, November/December 2017. (https://physics.princeton.edu/pulsar/k1jt/wsjtx-doc/wsjtx-main-2.6.1.html)
- **Provenance:** Open-access via the WSJT project's host.
- **Relevance:** PHY (#3) reference for understanding the WSJT family's protocol structure.

### 6.2 ARDOP

- **ARDOP (Amateur Radio Digital Open Protocol).** Open specification + multiple open implementations.
- **Reference implementation:** github.com/pflarue/ardop (ardopcf) — the implementation tuxlink already integrates via the ARDOP transport (ADR 0015 + bd-tuxlink-ytg).
- **Provenance:** Open-source (MIT or similar permissive license depending on implementation).
- **Relevance:** **PHY (#3), MAC (#5), ARQ (#6), Link adaptation (#7) — conceptual reference.** ARDOP is an open HF ARQ protocol that operates over a soundcard interface. Useful for understanding what a fully-open HF data protocol's design space looks like — OFDM PHY with FSK fallback for sync, multiple bandwidth modes (200/500/1000/2000 Hz), ARQ with selective-repeat-style retransmission.
- **Key concepts:** Bandwidth-selectable PHY modes; sync via FSK preamble; fragmented data + selective ACK; mode-stepping based on observed channel conditions.

### 6.3 AX.25 (packet radio layer 2)

- **TAPR (Tucson Amateur Packet Radio).** AX.25 Link Access Protocol for Amateur Packet Radio, version 2.2, July 1998.
- **Available at:** http://www.tapr.org/pub_ax25.html.
- **Provenance:** Open-access (TAPR publishes the spec freely).
- **Relevance:** **MAC (#5), ARQ (#6) — conceptual reference.** AX.25 is the established amateur packet link-layer protocol. Frame structure, link-establishment (SABM/UA), reliable delivery (I-frames with N(S)/N(R)), supervisory frames (RR/RNR/REJ), connectionless UI frames. Foundation for understanding link-layer design in an amateur context.
- **Key concepts:** HDLC-derived framing; layer-2 connection state machine; window-sized retransmission; layer-2 vs. layer-3 separation (AX.25 vs. NET/ROM / TheNET / etc.).
- **Wikipedia overview:** https://en.wikipedia.org/wiki/AX.25 — open-access (CC-BY-SA), fetched 2026-05-31.

### 6.4 Other HF data references worth knowing exist

- **PACTOR (closed proprietary, SCS GmbH).** **Explicitly NOT cited as design input** for the same reason VARA isn't. Listed here so a future contributor knows this exclusion is deliberate, not an oversight.
- **MIL-STD-188-110C.** US military HF data standard. Public specification (DOD-published, public availability varies). Conceptually relevant for understanding what military-grade HF data protocols look like (serial single-carrier 8-PSK, training-sequence-aided equalization, time-domain rather than OFDM). **Reference only for primitive concepts**, never specific parameters.
- **Stanag 4539 / Stanag 4285.** NATO HF data standards. Same posture as MIL-STD-188-110C.
- **OPENJC2.** Open HF data protocol (less common than the above). Reference exists; usage situational.

### 6.5 What this section is NOT going to cite

- VARA (Standard, Wide, FM). Per ADR 0014's bright line. The exclusion is explicit and documented; it is not an oversight.

---

## §7. Operator-confirmed reference radio inventory

For the bench rig (`docs/hardware/bench-rig-two-host-topology.md`):

- **Xiegu G90.** 20 W HF; modern CAT; operator-confirmed VARA HF Standard works on-air (per `project_g90_vara_standard_works_firsthand` memory). The known-good radio in the bench rig.
- **Yaesu FT-818(ND).** 5 W HF; menu-driven data setup; stock SSB filter (≤2300 Hz workable); EOL 2023. Constraints surface as forcing functions for the modem design (per the bench-rig spec).
- **Conscious non-target: Yaesu FT-817** (predecessor to 818). Operationally similar to 818 from a modem-design perspective; not separately characterized.

### Vendor / supplier references for the rig hardware

- **Masters Communications (W3KKC).** DRA-100-DIN6 and Motorola-16 adapter. https://www.masterscommunications.com/. Reserved for the VHF/UHF FM rig (CDM-1550LS+) per the existing `docs/hardware/modem-test-rig.md`.
- **DigiRig (LLC, Denis Grisak K0TX).** DigiRig Mobile / Lite — CM108B-class USB audio + HID PTT + serial CAT. https://digirig.net/. Two units in operator inventory.
- **RTL-SDR.** Inexpensive RTL2832U-based SDR (V3 / V4). https://www.rtl-sdr.com/. First-slice observer for the calibrated RF rig (per `project_rf_measurement_rig_design` memory).
- **C-Media (CM108B / CM119A datasheet anchors).** USB HID PTT report format is implementation-detail; Direwolf's `cm108.c` is authoritative (per `docs/hardware/modem-test-rig.md`). Direwolf source: https://github.com/wb2osz/direwolf.

### Operator's own observational data (in-scope per ADR 0014 §4)

- VARA HF Standard operates on G90 to real RMS gateways (confirmed firsthand).
- VARA HF Standard on FT-818 (5 W) makes few NVIS contacts due to power constraint, not protocol — same waveform, different RF reach.
- VARA FM works on FT-818 → on-air RMS Packet stations.

The above are the operator's *own first-person operational reports*. They're in-scope per ADR 0014 §3 ("Publicly advertised, operator-observable specifications that Cameron already knows from licensed operation of VARA — e.g., that VARA HF Standard occupies ≈2300 Hz of bandwidth, or that it is OFDM-based — are general background and do not require avoidance"). They are NOT design *input*; they establish the bandwidth ceiling and radio-class context.

---

## §8. Maintenance discipline

When adding a citation:

1. **Verify it's in-scope.** Per ADR 0014, no VARA internals from any source — including third-party RE write-ups. If unsure whether a source contains forbidden material, do not add it; consult Cameron.
2. **Cite by publication metadata, not by content reproduction.** Reproducing protected content from copyrighted works in this document creates a separate copyright concern. The citation library is a pointer set, not a content cache.
3. **Mark provenance.** "Open-access", "paywalled", "preprint-available-at-arXiv", "public-domain", or similar.
4. **Tag relevance.** Which subsystem of the program does this inform?
5. **Note key concepts, not specific choices.** This is per the clean-sheet-means-concepts rule.

When removing or deprecating a citation:

1. **Don't delete entries silently.** Mark `[deprecated-for-this-program]` with a one-line reason. The deprecation is itself part of the design provenance record.
2. **If removal is because the source turned out to contain forbidden material**, document that fact (e.g., "discovered to contain VARA-internal material at §X; removed per ADR 0014 bright line"). Future contributors should know what's already been ruled out.

---

Agent: mink-swallow-kite
