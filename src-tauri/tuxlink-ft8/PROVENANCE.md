# tuxlink-ft8 — constants & tables provenance ledger

**Clean-room rule.** Every magic constant, table, or algorithm in this crate cites
an **allowed** source below. The rule has two tiers:

1. **FT8-protocol-specific expression** — the tables, constants, and framing that are
   particular to FT8 (LDPC matrices, Gray/Costas arrays, CRC parameters, message
   layout, the soft-symbol / demapper form, the min-sum schedule) — MUST come only
   from the **protocol sources** below, and NEVER from the GPL `wsjtr`/WSJT-X (see
   Forbidden). This is the actual clean-room boundary: it keeps GPL-derived *expression*
   out of the crate.
2. **Standard published algorithms & public-domain code** — general results that are
   textbook / public knowledge and not derived from any forbidden source — are permitted
   with citation to their originating literature. These are NOT FT8-specific and carry
   no GPL entanglement: e.g. the closed-form noncoherent M-FSK error probability
   (Proakis), normalized min-sum scaling (Chen/Fossorier), belief-propagation
   (S. Johnson), the SplitMix64 PRNG and Box–Muller transform (public domain). Citing
   them is honest provenance, not a clean-room violation.

**Protocol sources (tier 1):**

- QEX 2020 "The FT4 and FT8 Communication Protocols" (Franke/Somerville/Taylor)
- WB2FKO "Synchronization in FT8"
- `ft8_lib` (kgoba) — **MIT** (the tables/algorithms actually transcribed here)
- `RustFT8` (jl1nie) — **MIT** — an *available* permitted reference; **not** read or
  transcribed for the current code (the min-sum is derived from `ft8_lib` `bp_decode`
  + textbook min-sum, not from RustFT8).

**Forbidden.** `wsjtr` / WSJT-X are **GPL** and are used ONLY as a pre-built binary
test oracle (feed a WAV, read the stdout decode list). Never:

- read `wsjtr`/WSJT-X source,
- run `strings` / `nm` / `objdump` / a decompiler on the oracle binary,
- copy WSJT-X's `generator.dat` / `parity.dat` (the LDPC matrix comes from MIT
  `ft8_lib`, or is regenerated from the spec).

A CI grep-guard **should be added** (tracked follow-up) to fail the build if any
crate file *includes* GPL source (WSJT-X's `generator.dat` / `parity.dat`, or
`wsjtr` internals). It must match inclusion patterns, not the bare strings — the
strings appear here and in `ldpc.rs`/`lib.rs` only as **negative citations**
("NOT the source"), which a naive grep would false-positive on.

## Ledger

| Constant / table | Value / status | Allowed source |
|---|---|---|
| Costas array | `3,1,4,0,6,5,2` | QEX 2020 §4; `ft8_lib` `constants.c` (MIT) |
| Frame geometry | 79 = {7,29,7,29,7} | QEX 2020 §4 |
| Info symbols | 58 (× 3 bits = 174) | QEX 2020 §4 |
| Payload / codeword / msg+CRC | 77 / 174 / 91 | QEX 2020 §2–3 |
| Tone spacing / symbol time | 6.25 Hz / 0.160 s | QEX 2020 §4, Table 4 |
| CRC-14 polynomial | `0x2757` (low-14, leading `x^14` dropped) = `0x6757 & 0x3FFF`; MSB-first, 14-bit register; computed over 77 payload bits **zero-extended to 82** | `ft8_lib` `constants.h` `FT8_CRC_POLYNOMIAL=0x2757` + `crc.c` `ftx_compute_crc`/`ftx_add_crc` (MIT); QEX 2020 §3 (`0x6757` incl. `x^14`) |
| CRC-14 input length | 82 bits (77 payload + 5 zero) | QEX 2020 §3; `ft8_lib` `ftx_add_crc` (`96-14`) (MIT) |
| Gray map (bits→tone) | `kFT8_Gray_map = {0,1,3,2,5,6,4,7}`; `gray_decode` is its inverse (QEX Table 3 tone→bits) | `ft8_lib` `constants.c` `kFT8_Gray_map` + `encode.c` (MIT); QEX 2020 Table 3 |
| Costas block offsets | sync groups at symbol indices `0, 36, 72` (7 symbols each); info fills `7..36` & `43..72` | `ft8_lib` `constants.h` `FT8_SYNC_OFFSET=36`, `FT8_NUM_SYNC=3` + `encode.c` (MIT); QEX 2020 §4 |
| LDPC(174,91) generator matrix | `kFTX_LDPC_generator[83][12]` — 83 rows × 91 bits (12 bytes, MSB-first, low 5 bits of byte 11 unused); parity bit `i` = GF(2) dot-product of row `i` with the 91-bit msg+CRC | `ft8_lib` `constants.c` `kFTX_LDPC_generator` + `encode.c` `encode174` (MIT), **NOT** WSJT-X `generator.dat`; QEX 2020 §3 |
| LDPC(174,91) parity-check incidence | `kFTX_LDPC_Nm[83][7]` (per-check incident codeword bits, 1-origin, `0` sentinel on 6-bit checks) + `kFTX_LDPC_Num_rows[83]`; syndrome = XOR of incident bits per check | `ft8_lib` `constants.c` `kFTX_LDPC_Nm`/`kFTX_LDPC_Num_rows` + `ldpc.c` `ldpc_check` (MIT), **NOT** WSJT-X `parity.dat`; QEX 2020 §3 |
| LDPC(174,91) variable→check incidence | `kFTX_LDPC_Mn[174][3]` (per-variable incident checks, 1-origin; transpose of `Nm`, every variable in exactly 3 checks); `mn_nm_graph_consistency` KAT asserts `Mn`/`Nm` describe the same Tanner graph | `ft8_lib` `constants.c` `kFTX_LDPC_Mn` (MIT), **NOT** WSJT-X `parity.dat`; QEX 2020 §3 |
| LDPC codeword ordering | systematic-first: bits `0..91` = msg+CRC verbatim, `91..174` = 83 parity bits (parity-check order); 22-byte MSB-first pack, checksum starts at bit 91 | `ft8_lib` `encode.c` `encode174` byte layout + `constants.h` `FTX_LDPC_{N,K,M,N_BYTES,K_BYTES}` (MIT) |
| Soft-demap LLR convention | `llr[i] = log(P(bit_i=1)/P(bit_i=0))`; positive ⟹ bit 1; hard decision `llr[i] > 0`. Pinned to avoid `ft8_lib`'s stale/wrong `log(P0/P1)` comment; the max-log formula is used with **no** sign flip | pinned crate convention (T1.1 brief); matches `ft8_lib` `ldpc_decode` `plain[i]=(l>0)?1:0` (MIT) |
| Soft-demapper max-log bit metric | per symbol: `s2[j] = p[GRAY_MAP[j]]`; `l0 = max4(s2[4..7]) − max4(s2[0..3])`, `l1 = max4(s2[2,3,6,7]) − max4(s2[0,1,4,5])`, `l2 = max4(s2[1,3,5,7]) − max4(s2[0,2,4,6])`; `l0` = MSB matching `symbols_to_bits` | `ft8_lib` `ft8/decode.c` `ft8_extract_symbol` `logl[0..3]` (MIT); QEX 2020 §6 soft-symbol metric |
| LLR variance normalization | scale all 174 LLRs by `sqrt(NORM_COEFF / variance)`, `NORM_COEFF = 24.0` (experimentally-tuned; enables the −20.8 dB threshold); guard `variance ≤ 0` (return unscaled) | `ft8_lib` `ft8/decode.c` `ftx_normalize_logl` literal `24.0f` (MIT) |
| Min-sum decoder algorithm | normalized min-sum BP over the Tanner graph; per-edge `tov`/`toc`; var→check `q = ch + Σ_{m'≠m} tov`; check→var `tov = −α·(Π_{n'≠n} sign(−q))·min_{n'≠n}\|q\|`; posterior `ch + Σ tov`; early-stop on syndrome 0, best-so-far retained; clip messages to ±`CLIP`. The `−`/`sign(−q)` structure mirrors `bp_decode`'s `−2·atanh(Π tanh(−Tnm/2))`; the `sub_limit_error_recovery` KAT empirically pins it (textbook `+α·Πsign(q)·min` fails to correct a single flip on this code) | `ft8_lib` `ft8/ldpc.c` `bp_decode` schedule + check-node sign structure (MIT); S. Johnson "Iterative Error Correction"; textbook min-sum (**not** transcribed from `RustFT8`) |
| Min-sum normalization factor α | `ALPHA = 0.75` (scales each check→variable message; compensates min-sum optimism vs exact sum-product; tunable by T1.2) | normalized min-sum (Chen/Fossorier "Reduced-Complexity Decoding of LDPC Codes"); standard value for this code class |
| Min-sum message clip bound | `CLIP = 20.0` (finite clamp on channel LLRs + messages, prevents `±inf` runaway) | BP numerical-stability guard (Johnson, "Iterative Error Correction") |
| Callsign hash (10/12/22-bit) | multiplier `47055833459` (`0xAF5A2E6F3`); n12=n22>>10, n10=n22>>12 | `ft8_lib` `message.c` `save_callsign` (MIT); QEX Table 2 `h22/h12/h10` |
| Payload byte length | 10 bytes (77 bits, top 3 unused) | `ft8_lib` `message.h` `FTX_PAYLOAD_LENGTH_BYTES` (MIT) |
| Special-token limits | `MAX22=4194304`, `NTOKENS=2063592`, `MAXGRID4=32400` | `ft8_lib` `message.c` (MIT) |
| Char tables | FULL(42) `" 0-9A-Z+-./?"`, ALNUM_SPACE_SLASH(38), ALNUM_SPACE(37), LETTERS_SPACE(27), ALNUM(36), NUMERIC(10) | `ft8_lib` `text.h` table comments (MIT); QEX Table 2 |
| Basecall mixed-radix | `37·36·10·27·27·27` | `ft8_lib` `message.c` `pack_basecall` (MIT); QEX Table 2 `c28` |
| Special c28 tokens | `DE=0, QRZ=1, CQ=2` | `ft8_lib` `message.c` `pack28`/`unpack28` (MIT) |
| Grid/report sentinels | grid=g15; blank=`MAXGRID4+1`; RRR/RR73/73=`+2/+3/+4`; report=`MAXGRID4+35+dd` | `ft8_lib` `message.c` `packgrid`/`unpackgrid` (MIT); QEX Table 2 `g15/R1/r2` |
| Free-text / telemetry pack | base-42 over 13 chars (f71) / 71-bit hex (t71), left-shift-by-1 into 10-byte payload | `ft8_lib` `message.c` `ftx_message_encode_free`/`_telemetry` (MIT); QEX Table 1 rows `0.0`/`0.5` |
| Std message bit layout | `c28 r1 c28 r1 R1 g15`, `i3` at bits 74..76, `n3` at 71..73 | `ft8_lib` `message.c` `ftx_message_encode_std`/`ftx_message_get_i3`/`_get_n3` (MIT); QEX Table 1 |
| SNR-in-2500-Hz conversion (T1.2) | `SNR_2500_dB = 10·log10(γ) − 26.02 dB`, `γ = Es/N0`; offset `= 10·log10(T·2500) = 10·log10(0.16·2500) = 10·log10(400) ≈ 26.0206 dB`; inverse `γ = 10^((SNR_2500_dB+26.02)/10)`. (`awgn.rs` `gamma_to_snr2500_db`/`snr2500_db_to_gamma`; test-only) | QEX 2020 §4 (`Ps = Es/T`, `T = 0.16 s`) + Table-5 text ("SNR in 2500 Hz bandwidth at P(decode)=0.5") |
| Noncoherent 8-FSK AWGN tone model (T1.2) | per symbol: complex noise `n_k = x+iy`, `x,y ~ N(0,0.5)` (⟹ `E[\|n_k\|²]=1`, unit noise power/bin); true tone adds signal phasor `a·(cosθ+i·sinθ)`, `a=sqrt(γ)`, θ uniform (noncoherent); observation = 8 tone **magnitudes** `\|value_k\|` (amplitudes) fed to `soft_demap`. (`awgn.rs` `symbol_magnitudes`/`frame_magnitudes`; test-only) | QEX 2020 §4 (modulation), §6 (soft symbol on `\|Ci\|`), §8 (AWGN channel); standard noncoherent orthogonal M-FSK AWGN model (Proakis, *Digital Communications*) |
| Noncoherent orthogonal M-FSK symbol-error probability (T1.2 calibration) | `Pe(γ) = Σ_{n=1}^{M-1} (−1)^{n+1}·C(M−1,n)/(n+1)·exp(−(n/(n+1))·γ)`, `M=8`; exact alternating sum (not a bound), used only to calibrate the model's SNR axis against the uncoded `argmax\|value_k\|` detector; `Pe(γ→0)→(M-1)/M=7/8`. (`awgn.rs` `noncoherent_mfsk_pe`; test-only) | J.G. Proakis, *Digital Communications* — noncoherent orthogonal M-ary FSK symbol-error probability (standard comms result) |
| Harness PRNG (T1.2, no new dep) | SplitMix64: state `+= 0x9E3779B97F4A7C15`, mix with `0xBF58476D1CE4E5B9`/`0x94D049BB133111EB` and shifts 30/27/31; uniform `f64` via top-53-bits/2^53; two `N(0,1)` per Box–Muller (`u1∈(0,1]`); scale by `sqrt(0.5)` for the `N(0,0.5)` noise components. Seed fixed per test; stream varied by (SNR,codeword,trial) indices. (`awgn.rs` `Rng`; test-only) | SplitMix64, public domain (Steele/Lea/Vigna, "Fast Splittable Pseudorandom Number Generators", OOPSLA 2014); Box–Muller transform (Box & Muller 1958, public domain) |
| Spectrogram geometry (T2.1) | symbol window `SYMBOL_SAMPLES = 1920` (= `0.160 s × 12000`); quarter-symbol hop `HOP_SAMPLES = 480` (`TIME_OSR = 4`); zero-pad to `FFT_LEN = 3840` (`FREQ_OSR = 2`) ⇒ `BIN_HZ = 3.125` (= 6.25/2); 15 s slot ⇒ 372 windows, 1921 one-sided bins. (`channelize.rs`) | WB2FKO "Synchronization in FT8" (`sync8.f90`: 40 ms quarter-symbol hop, 160 ms windows zero-padded to 320 ms, 372 spectra); MIT `ft8_lib` `decode.c` `time_osr`/`freq_osr` waterfall model; QEX 2020 §4/Table 4 (symbol time, tone spacing) |
| Hann analysis window (T2.1) | `w[k] = 0.5 − 0.5·cos(2πk/n)` applied to each symbol window before the FFT | standard public-domain DSP (Harris, "On the Use of Windows for Harmonic Analysis with the DFT", Proc. IEEE 1978) |
| Single-bin DFT / Goertzel tone power (T2.1/T2.2) | `\|Σ x[n]·e^{−jωn}\|²`, `ω = 2πf/fs`, via rotating-phasor accumulation over an arbitrary `(start, len, freq)` window; used for sub-bin fine-refine + per-symbol tone extraction. (`channelize.rs` `tone_power`) | Goertzel algorithm — standard public-domain DSP (Goertzel, Amer. Math. Monthly 1958) |
| Costas coarse sync metric (T2.2) | mean dB neighbour-contrast: for each of the 3 Costas blocks (symbol offsets 0/36/72), each Costas tone `sm` vs its ±1-tone freq neighbours (`±FREQ_OSR` bins) and ±1-symbol time neighbours (`±TIME_OSR` steps), `10·log10(P+ε)` differences averaged; scale-invariant, empty regions score ~0 dB. (`sync.rs` `costas_metric`) | MIT `ft8_lib` `decode.c` `ft8_sync_score` (neighbour-difference score, `p8[sm]−p8[sm±1]`/`±block_stride`) (MIT); WB2FKO `Sabc`/`Sbc` per-block Costas summation |
| Coarse search bounds (T2.2) | fc ∈ [100, 2600] Hz (tone-0) at 3.125 Hz (1 bin); `t0` ∈ [−62, +125] quarter-symbol steps (= DT [−2.5, +5] s); dedup within 4 Hz (weaker discarded); keep top 300 | WB2FKO passband example (200–2500 Hz), `−2 ≤ ∆t ≤ +3 s`, 4 Hz dedup, ≤200 candidates (widened low/high for the 800/2400 Hz fixtures); MIT `ft8_lib` `ftx_find_candidates` partial-frame `time_offset` loop |
| Fine refinement (T2.2) | maximize Costas cross-energy over time ±40 ms (2 ms steps) and frequency ±one tone (±6.25 Hz, 0.25 Hz steps). Frequency span widened from WB2FKO's ±2.5 Hz to one tone because the dB-contrast coarse metric localizes fc only to ±one tone on GFSK, while the cross-energy objective is sharply unimodal at the true carrier. (`sync.rs` `fine_refine`) | WB2FKO fine sync (`ft8b`/`sync8d`: ±40 ms time, sub-Hz `∆f`); span adaptation documented in `fine_refine` |
| Info-symbol extraction skip-schedule (T2.2) | info symbol `i` at frame position `i + (i<29 ? 7 : 14)`; 8 tone powers at `fc + tone·6.25 Hz` (raw FSK tone, not Gray-decoded) → `[[f32;8];58]` for `soft_demap`. (`sync.rs` `extract_info_powers`) | MIT `ft8_lib` `decode.c` `ft8_extract_likelihood` (`sym_idx = k + (k<29?7:14)`) + `ft8_extract_symbol` (8 tone bins) (MIT); crate `symbols::assemble_frame` layout |
| Sync-metric floor (T2.3 false-decode guard) | `SYNC_FLOOR = 10.0` dB. Measured separation: the 5 noiseless fixtures score 20.8–21.8 dB; silence ≈ 0 dB and deterministic white noise ≤ 5.8 dB (max over the whole 2-D search). Guards a downstream `converged && CRC` gate against admitting an empty slot. (`sync.rs`) | empirical, pinned by the `noise_stays_below_floor` / `sync_metric_signal_vs_noise` KATs; complements `decode.rs` all-zero guard |
| Non-standard-callsign unpack (i3=4) | 12-bit hash `h12` from payload[0..2]; 58-bit base-38 call `c58` from payload[1..9] over the 38-char `space 0-9 A-Z /` table (right-aligned, trimmed); `iflip`/`nrpt`/`icq` flags select ordering, `CQ` token, and `RRR`/`RR73`/`73` suffix. (`message.rs` `unpack_nonstd`/`unpack58`) | `ft8_lib` `message.c` `ftx_message_decode_nonstd` + `unpack58` (MIT); QEX 2020 Table 1 row `4` (`h12 c58 …`) |

**Deferred to T0.2-follow-up** (marked `TODO(T0.2-follow-up)` in `message.rs`,
not half-implemented): EU VHF (`i3=2`), RTTY RU (`i3=3`), nonstandard-call type-4
**packing** (`pack58` — the transmit path; **unpacking** `unpack58`/`unpack_nonstd`
is now implemented for M2's T2.3 acquisition KAT, receive-only), DXpedition
(`n3=1`), Field Day (`n3=3/4`), `CQ nnn`/`CQ a[bcd]` modifiers, and the
`3DA0`/`3X` prefix work-arounds.

Update this table in the same commit that introduces each constant.
