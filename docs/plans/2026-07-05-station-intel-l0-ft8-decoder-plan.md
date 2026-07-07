# Plan: L0 — clean-room pure-Rust FT-8 decoder SPIKE (bd tuxlink-b026z.1)

Epic: tuxlink-b026z (Station Intelligence). Layer: L0 (decision gate).
Design doc: ~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260705-034957-passive-ft8-listener.md
Adversarial rounds folded in: Claude (dev/adversarial, self-adrev) + Codex
(dev/adversarial/2026-07-05-l0-ft8-decoder-approach-codex.md). Both cross-provider.
Agent: crag-moraine-juniper.

## Goal & fail-fast framing

L0 is a THROWAWAY SPIKE whose only job is to answer: *can a clean-room pure-Rust
decoder match WSJT-X on real 20m captures well enough to fund L1?* It must fail
CHEAP if the answer is no. The two adversarial rounds converge on this: build the
two thinnest de-risk slices FIRST (the LDPC/LLR core curve, then a real-WAV thin
slice) before investing in the full multi-candidate detector + multi-pass
subtraction. If either de-risk slice fails badly, STOP and reassess (fall back to
the wsjtr-dependency option from the design doc) rather than sink L-effort.

## ⭐ L0 SPIKE OUTCOME (2026-07-07): NO-GO → jt9/wsjtr fallback

The spike answered its question. **NO-GO.** M0–M2 shipped and the noiseless
synthetic gate passed, but M3's first test against REAL captures is decisive:

| Capture | jt9 (WSJT-X) reference | Our single-pass clean-room | Gate |
|---|---|---|---|
| 40m ordinary | 5/5 | **1/5 (20 %)** | ≥85 % |
| 20m quiet | 2/2 | **0/2 (0 %)** | ≥85 % |

Zero false decodes throughout. Root cause is **weak-signal coarse TIME
localization**, NOT the sync floor: a floor-free decode targeted at the exact
reference carriers still fails, because the coarse dB-contrast metric mislocates
the frame start `t0` beyond the ±40 ms fine-refine window on −14…−19 dB signals
(frequency lands within 1–3 Hz; `t0` is scattered/implausible). The missing lever
is a robust sub-sample time-sync stage (ft8_lib/WB2FKO `sync8d`-class) — bigger
than an M3 tuning task. The M2 GO only proved noiseless synthetic decode.

**Operator decision (2026-07-07, B+C):** take the design doc's wsjtr-dependency
fallback — Station Intelligence depends on the proven external decoder
(`jt9`/`wsjtr`). Keep this crate as a tested reference / learning artifact. The M3
deliverables merged anyway (hash-table population, within-slot dedup, and the
permanent `oracle` comparator harness — all green). Do NOT sink further L-effort
into clean-room weak-signal acquisition unless the jt9 dependency proves
problematic or the project matures with spare capacity. L1 (tuxlink-b026z.2) is
superseded by the fallback direction. M4 (crowded-band multi-pass subtraction) is
not attempted. Full evidence: handoff `dev/handoffs/2026-07-07-*-ft8-l0-nogo-*`.

## Where to work (pinned; do NOT drift)

- Worktree: /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-b026z.1-station-intel-ft8-l0
- Branch: bd-tuxlink-b026z.1/station-intel-ft8-l0 (off origin/main; the ant8s main
  checkout is ~2789 commits stale — never ground against it).
- Crate: NEW `src-tauri/tuxlink-ft8/` — pure-DSP LIBRARY, no Tauri deps. Add to
  `src-tauri/Cargo.toml` workspace `members`. License AGPL-3.0-or-later (match repo).
- Reference PDFs (read for spec, clean to cite): QEX FT4/FT8 paper + WB2FKO sync
  paper at /tmp/claude-1000/-home-administrator-Code-tuxlink/69a6cf97-7227-4796-b628-482b5286c7d4/scratchpad/ft8-refs/ .

## Hard constraints (copy into EVERY subagent prompt)

1. CLEAN-ROOM PROVENANCE. Implement ONLY from: QEX 2020 paper (Franke/Somerville/
   Taylor); WB2FKO "Synchronization in FT8"; MIT `ft8_lib` (kgoba); MIT `RustFT8`
   (jl1nie). The GPL `wsjtr`/WSJT-X is a PRE-BUILT BINARY TEST ORACLE ONLY:
   feed-WAV / read-stdout-decode-list is the ONLY permitted interaction. NEVER
   clone/read wsjtr or WSJT-X source; NEVER run `strings`/`nm`/`objdump`/a
   decompiler on the oracle binary (that reads GPL-derived expression); NEVER copy
   the LDPC matrix/generator from WSJT-X's `generator.dat`/`parity.dat` (those are
   GPL) — take the matrix from MIT `ft8_lib` or regenerate from the spec.
2. Every magic constant/table (LDPC parity matrix, generator, CRC-14 poly, Costas
   array, Gray map, LLR scale) carries a PROVENANCE COMMENT citing an allowed
   source (QEX/WB2FKO/ft8_lib/RustFT8) with file+line. No citation = provenance
   defect. A CI grep-guard fails the build if any file references wsjtr internals.
3. The dev Pi CANNOT cold-compile Rust. Do NOT expect local `cargo build`. Arm
   work with tests + clippy, PARENT commits+pushes, CI compiles both arches. Read
   CI by matching headSha + conclusion (bare `gh run watch`/--limit 1 latches stale).
4. Subagents CODE + STOP; the PARENT commits (subagents cannot commit in worktrees).
5. New Rust dep => regenerate Cargo.lock (`cargo metadata`) so `--locked` CI does
   not mask clippy/tests. Deps allowed: rustfft, realfft, hound (WAV I/O). Nothing
   from the GPL side.
6. RADIO-1 N/A — receive-only decoder, no transmit path.

## Spec facts (from the QEX paper — the deterministic KAT sources)

- Payload = exactly 77 bits. Message types by i3(3b)/n3: 0.0 free-text (f71),
  0.5 telemetry (t71), 1 standard (c28 r1 c28 r1 R1 g15), 3 RTTY-RU, 4 nonstd-call
  (h12 c58 h1 r2 c1), plus EU-VHF/DXpedition/Field-Day. Standard call = 28b;
  compound/special use 10/12/22-bit HASH, rendered `<CALL>` or `<...>`.
- CRC = 14-bit; QEX prints poly 0x6757 (includes the x^14 term); ft8_lib's crc.c
  uses 0x2757 (low-14-bit representation of the SAME poly). PIN the exact
  representation + bit order by KAT against a WSJT-X-generated message, NOT a
  synthetic round-trip. CRC covers the 77→82-bit (zero-padded) message → 91 bits.
- LDPC(174,91): 83 parity bits appended to the 91 msg+CRC bits → 174-bit codeword.
  Sparse 83×174 parity matrix; codeword valid iff all 83 syndromes = 0 (mod 2).
- 8-FSK: 174 bits → 58 channel symbols (3 bits each, values 0–7) via GRAY map
  (Table 3: 0=000,1=001,2=011,3=010,4=110,5=100,6=101,7=111). Full 79-symbol
  frame = {Costas, 29 sym, Costas, 29 sym, Costas}. Costas = 3,1,4,0,6,5,2.
- Modulation: T=0.160 s/symbol, tone spacing 6.25 Hz, canonical audio 12000 Hz
  (1920 samples/symbol), 79×0.16 = 12.64 s frame, 20 ms raised-cosine ramp.
- Decode (§6): noncoherent block detection (N=1,2,3), soft demapper
  Lj = K(max|Ci| over xj=1 − max|Ci| over xj=0), hybrid BP + OSD, and MULTI-PASS
  subtract-and-redecode (2–3 passes) — where most crowded-band decodes come from.
- Published FT8 thresholds (Table 6, AWGN): −20.8 dB (BP+OSD, no AP), −22.7 dB
  (max AP). The de-risk curve (M1) targets the no-AP −20.8 dB figure.

## Exit gate (refined per both adrev rounds — supersedes the design doc's one-liner)

On EACH of two captures (ordinary + crowded 20m) independently:
- Reference = WSJT-X decode list produced with **AP (a-priori) decoding DISABLED**
  (or AP-decoded messages labeled and EXCLUDED). AP decodes are structurally
  un-reproducible by a passive unaided decoder; including them rigs the gate.
- Match = MULTISET on normalized message identity per message-type match key
  (standard: callsign-pair+report+grid; free-text/telemetry: exact payload string;
  hashed `<...>`: matched as its own class or excluded — decide + document, do not
  silently fail bit-perfect decodes that only lack a hash-table entry).
- PASS = recover ≥85% of the reference multiset, with ZERO false decodes. "Zero
  false" is enforced by GUARDS, not CRC alone: LDPC syndrome=0 AND CRC-14 pass AND
  a sync-metric floor on the winning candidate. Measure the false rate across BOTH
  captures; CRC-14 has a ~1/16384 per-converged-candidate false-accept, so the
  guard stack (not CRC) is what delivers zero.

## Milestones & tasks (subagent-ready; strict TDD)

Every task preamble: "Read .claude/skills/test-driven-development + docs/pitfalls/
testing-pitfalls.md. TDD: failing test → implement → green. You are agent
crag-moraine-juniper. Obey the CLEAN-ROOM PROVENANCE constraint above." Every task
completion check: "Review tests vs testing-pitfalls.md; confirm error/edge paths
covered; run the crate's tests (note: full build is CI-only on this Pi) and report."

### M0 — Foundations (deterministic KATs, no audio). Sequential within, one subagent.
- **T0.1 Crate scaffold.** Create `src-tauri/tuxlink-ft8` lib crate; add to workspace
  members; add deps rustfft/realfft/hound; regenerate Cargo.lock; a trivial passing
  test so CI has something to compile. Do NOT add any GPL dep. (Parent then opens a
  DRAFT PR so CI starts.)
- **T0.2 Message pack/unpack.** Implement 77-bit pack/unpack for types 1 (standard),
  0.0 (free-text), 0.5 (telemetry), 3, 4 (nonstd/hash). KAT vectors: transcribe
  example messages↔bit-fields from QEX Table 1/2 + ft8_lib `unpack.c`/`pack.c`
  (MIT). Test each type round-trips AND matches the cited bit layout. Include the
  10/12/22-bit callsign HASH function (cite ft8_lib) + a slot-scoped hash table.
- **T0.3 CRC-14.** Implement; PIN poly representation + bit order by KAT against a
  message whose 77+14 bits are extracted from a WSJT-X-GENERATED WAV (not synthetic).
  Provenance-comment the poly.
- **T0.4 Gray + Costas symbol mapping.** 174 bits ↔ 58 gray-coded symbols; assemble/
  disassemble the 79-symbol frame with Costas at 0/36/72. KATs from Table 3 + the
  Costas array. (Codex flagged this as a distinct error-prone layer — do NOT fold
  it into T0.2 or the demod.)
- **T0.5 LDPC(174,91) encode + syndrome.** Transcribe the sparse parity matrix +
  generator from MIT `ft8_lib` (provenance-comment; NOT from WSJT-X). Encode a msg→
  174-bit codeword; assert all 83 syndromes = 0; KAT a known codeword.

### M1 — CORE de-risk: LLR + min-sum vs published SNR curve (Claude's #5). One subagent.
- **T1.1 Soft demapper + min-sum LDPC decoder.** From 8 per-symbol tone powers →
  3 bit-LLRs via max-log/log-sum-exp, scaled by an estimated noise variance
  (per-symbol max-subtract + noise-normalize; cite ft8_lib). Min-sum BP with
  normalized/offset tuning, iteration cap, LLR clipping, early-stop on syndrome=0.
  KAT: a known clean LLR vector decodes to the exact expected codeword; injected
  bit-errors below the correction limit recover.
- **T1.2 AWGN-vs-SNR harness (THE go/no-go).** Encode ~50 known messages → 8-FSK
  tone-power model → add calibrated AWGN over SNR −15…−24 dB → LLR → min-sum →
  decode. Plot/print decode-probability vs SNR. **GATE: the 50%-decode point lands
  within ~1–2 dB of the published −20.8 dB (no-AP) FT8 threshold.** If it is ≥4 dB
  worse, the LLR-scaling/min-sum core is broken — STOP, fix, or escalate the
  wsjtr-fallback decision BEFORE building any detector. Record the curve in the PR.

### M2 — ACQUISITION de-risk: real-WAV thin slice (Codex's #1/#15). One subagent, after M1.
- **T2.1 Channelize + baseband.** WAV (hound) → resample/mix/decimate to canonical
  12000 Hz baseband with proper windowing (Codex #3: NOT a naive 32-pt FFT at native
  rate — specify decimation so bins resolve 6.25 Hz). Provenance-cite the approach
  to WB2FKO/ft8_lib.
- **T2.2 Costas sync (coarse) + fine refine.** Coarse 2-D search: freq step ≤3.125 Hz,
  time step ≤40–80 ms over DT ∈ [−2.5, +5] s; sync metric = summed Costas-tone power
  minus off-tone, noise-normalized, scored across ALL THREE Costas blocks (WB2FKO).
  RANKED candidate list (top ~200–300), NOT a fixed threshold. Then per-candidate
  fine time/freq refinement (parabolic/phase interpolation) — a distinct testable unit.
- **T2.3 Thin end-to-end KAT.** WSJT-X-GENERATED single-signal WAVs (one per message
  type; generate via WSJT-X's own encoder or ft8_lib's gen) → channelize → sync →
  demod → LLR → LDPC → assert the exact known message. This is the "real audio
  arrives before step 6" fix both rounds demanded.

### M3 — Full single-pass pipeline + the oracle comparator. One subagent, after M2.
- **T3.1 Full slot decode.** Iterate the ranked candidate list, decode each, dedup
  within-slot exactly as WSJT-X does (multiset on normalized message identity).
  Apply the zero-false GUARD stack (syndrome=0 + CRC + sync floor).
- **T3.2 Oracle comparator.** `fixtures/`-driven harness: our decodes vs a reference
  decode list; implement the refined match rules (AP-disabled reference, per-type
  match keys, multiset, hash-class). Emit parity % + false count. Ships as the
  PERMANENT regression harness (passive, reproducible, zero TX).
- **T3.3 Ordinary-capture parity.** Run against ft8_lib's bundled sample WAVs (known
  decodes) as the initial oracle NOW; leave a documented `fixtures/` slot for the
  operator's real ordinary 20m SDR capture + AP-disabled WSJT-X log as final input.

### M4 — Crowded-band + multi-pass subtraction (the ≥85% crowded arm). One subagent, after M3.
- **T4.1 Decode→reconstruct→subtract→redecode** (2–3 passes; QEX §6 channel-gain
  subtraction). Both rounds: single-pass will fail the crowded capture; this is the
  highest-effort, most-likely-under-scoped piece. Budget it explicitly here.
- **T4.2 Crowded-capture parity.** Against a deliberately crowded 20m capture (operator
  fixture) + AP-disabled WSJT-X log. This closes the L0 exit gate.

## Fixtures strategy (unblock now; operator supplies the final gate input)
- NOW: ft8_lib bundled sample WAVs + WSJT-X-generated per-type WAVs (deterministic
  known answers) drive M0–M3.
- FINAL GATE (operator assignment): one ordinary + one crowded 20m SDR capture, each
  with a WSJT-X decode log produced **with AP disabled**. Documented `fixtures/README`
  states exactly how to regenerate the reference (WSJT-X flags used).

## Provenance ledger + CI guard (cross-cutting; T0.1 seeds, every task maintains)
- `tuxlink-ft8/PROVENANCE.md`: table of every constant/table → allowed source +
  file:line. `wsjtr` listed as binary-oracle-only.
- CI check: grep the crate for forbidden tokens (wsjtr source paths, generator.dat/
  parity.dat from WSJT-X) → fail build if present.

## Review loop (per BRF): after M0, after M1, after each of M2–M4, ≥3 review rounds
from multiple perspectives (correctness, provenance, test-quality vs testing-pitfalls,
weak-signal completeness). If substantive findings on round 3, keep going.

## Execution recommendation
Sequential subagents M0→M1→M2→M3→M4 (each layer depends on the prior; parallelism
would conflict on the same crate). Parent commits + pushes after each subagent + the
review loop; draft PR opened after T0.1 so CI compiles continuously. HARD STOPS:
after M1 (SNR curve) and after M2 (real-WAV slice) — either failing badly triggers a
go/no-go conversation with the operator before proceeding.
