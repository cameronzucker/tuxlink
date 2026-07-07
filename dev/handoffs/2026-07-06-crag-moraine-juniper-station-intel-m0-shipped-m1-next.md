# Handoff — 2026-07-06 (crag-moraine-juniper): Station Intelligence M0 shipped, M1 (go/no-go) next

Session designed the **Station Intelligence** feature (fold native passive FT-8 into
Find-a-Station) and built + **merged M0** — the deterministic foundation of a
clean-room, from-scratch, pure-Rust FT-8 decoder. **Next: M1 — the soft-demapper +
min-sum LDPC decoder + the AWGN-vs-SNR curve, whose result is the go/no-go for the
entire native-decoder bet.**

## ⭐ NEXT SESSION STARTS HERE: M1 (the decision gate)

A fresh M1 worktree is already set up (off `main`, which carries M0):
**`worktrees/bd-tuxlink-b026z.1-station-intel-ft8-m1`** (branch
`bd-tuxlink-b026z.1/station-intel-ft8-m1`, bd `tuxlink-b026z.1` claimed). `cd` there,
`pnpm install` (fresh worktree), open a draft PR early (Pi can't cold-compile).

**M1 = bd `tuxlink-b026z.1` continued.** In the `tuxlink-ft8` crate:
- **T1.1** soft-demapper (8-FSK tone powers → 3 bit-LLRs via max-log / log-sum-exp,
  scaled by an estimated noise variance — cite `ft8_lib`) + a min-sum belief-propagation
  LDPC decoder (normalized/offset tuning, iteration cap, LLR clipping, early-stop on
  `ldpc_syndrome == 0`). KAT: known clean LLR → exact codeword; sub-limit bit-errors recover.
- **T1.2 — THE GO/NO-GO:** an AWGN-vs-SNR harness. Encode ~50 known messages → 8-FSK
  tone-power model → calibrated AWGN over −15…−24 dB → LLR → min-sum → decode; print
  decode-probability vs SNR. **GATE: the ~50% point lands within ~1–2 dB of the
  published −20.8 dB (no-AP) FT8 threshold.** If ≥4 dB worse, the LLR-scaling/min-sum
  core is broken — **STOP, surface to the operator**, reconsider the `wsjtr`-dependency
  fallback BEFORE building any detector (M2+).

**Build M1 on the shipped M0 API** (all in `src-tauri/tuxlink-ft8/src/`):
`symbols::symbols_to_bits` (8-FSK gray → 174 bits) + `ldpc::ldpc_syndrome` /
`is_valid_codeword` + `crc::check_crc` + `message::unpack`.

## Canonical docs (both now on `main`)
- **Design doc (READ):** `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260705-034957-passive-ft8-listener.md` (Status: APPROVED — full feature: Station Intelligence, L0–L5, placement, two-tier naming).
- **L0 plan (READ):** `docs/plans/2026-07-05-station-intel-l0-ft8-decoder-plan.md` — subagent-ready, adrev-hardened. M1 = its "M1 — CORE de-risk" milestone.

## HARD clean-room discipline (into every M1 subagent prompt)
Implement ONLY from: QEX 2020 FT4/FT8 paper + WB2FKO "Synchronization in FT8" + MIT
`ft8_lib` (kgoba) + MIT `RustFT8` (jl1nie). The GPL `wsjtr`/WSJT-X is a **pre-built
binary test oracle ONLY** — never read its source, never `strings`/`objdump` its
binary, never copy `generator.dat`/`parity.dat`. Every constant carries a `//
provenance:` comment + a `PROVENANCE.md` row.
**Re-fetch the references** (they were in the prior session's `/tmp` scratch, now gone):
QEX PDF `https://wsjt.sourceforge.io/FT4_FT8_QEX.pdf`; WB2FKO `https://www.sportscliche.com/wb2fko/FT8sync.pdf`;
`git clone --depth 1 https://github.com/kgoba/ft8_lib` to a NON-repo scratch dir.

## bd state
- Epic **`tuxlink-b026z`** — Station Intelligence. Children: `.1` L0 (in_progress —
  M0 done+merged, M1–M4 pending), `.2`–`.6` = L1–L5, `.7` = clean-room CI grep-guard follow-up.
- Old epic `tuxlink-u3m0g` = PARKED (superseded PSKReporter/fusion framing).

## Branch / PR / worktree state
- **PR #1020 — MERGED** to `main` 2026-07-06 (merge commit `01595b66`). Its branch
  `bd-tuxlink-b026z.1/station-intel-ft8-l0` is now **dead** (ADR 0017).
- **OLD L0 worktree** `worktrees/bd-tuxlink-b026z.1-station-intel-ft8-l0` — everything
  merged; disposed this session via the ADR 0009 ritual (untracked = only the gitignored
  `dev/adversarial/` codex transcript + `node_modules`/`target`, nothing at-risk). If it
  still shows in `git worktree list`, run `git worktree prune`.
- **M1 worktree** `worktrees/bd-tuxlink-b026z.1-station-intel-ft8-m1` — this one; start here.

## CI notes (for M1)
- This Pi cannot cold-compile Rust; CI compiles. Draft PR early.
- `verify` job order: pnpm typecheck → **vitest** → build → clippy → cargo test. The
  frontend **vitest step is FLAKY** (unhandled async `fetch` "unknown scheme" rejections
  → `exit 101` despite all 308 files passing) and short-circuits before clippy/cargo-test.
  If verify fails on vitest, `gh run rerun <id> --failed`. `main` is green, so a rebase
  onto latest `main` may also clear it. M0 needed 3 CI cycles for exactly this reason plus
  two clippy `needless_range_loop`s and one mis-premised test — budget for that cadence.
- clippy is `--all-targets -D warnings` (stricter than local). MSRV **1.75**.

## Multi-session hook gotcha (bit us all session)
A second live session (`bd-tuxlink-cnz5o/sim-harness-poc`) put the main-checkout-race
hook in lease-required mode. Discipline that works: `cd` into your worktree as its OWN
Bash step, then run each git op **solo** (no `HEAD`/`status`/`gh --json headSha` siblings).
Compound `cd && git …` false-positives — the hook reads cwd before the `cd` runs.

## What shipped this session (M0, merged)
`tuxlink-ft8` crate: 77-bit message pack/unpack + callsign hash (`message.rs`), CRC-14
(`crc.rs`, poly `0x2757`), Gray map + Costas 174↔79 framing (`symbols.rs`), LDPC(174,91)
encode + syndrome (`ldpc.rs`). Byte-exact KATs cross-verified across modules + against an
independent Python transcription of MIT `ft8_lib`; all 41 KATs green on both arches. M0
review verdict **SOUND**. Clean-room provenance verified (only negative GPL citations).

## M3/M4 exit-gate oracle — CAPTURED this session (was the operator's task)
The operator put an **RTL-SDR Blog V3** on the Pi (Delta Loop) at session end, and I
captured the fixtures: `src-tauri/tuxlink-ft8/tests/fixtures/sdr/` — four real off-air
15 s FT-8 slots (40m crowded/ordinary, 20m busier/quiet) + their **AP-disabled** `jt9`
reference decode lists + a capture-recipe README. These are the ≥85%-message-match,
zero-false-decode oracle for M3/M4. (Receive-only capture; no transmit.)
- **Capture gotcha (in the README):** the V3 does HF via direct sampling on the
  **Q-branch** — `rtl_fm -M usb -E direct2 -f <dial> -s 1024000 -r 12000` (NOT `-E
  direct`, the empty I-branch). Reference = `jt9 -8 <wav>` with no `--my-call` (AP inert).
- **Only follow-up:** 20m was quiet at capture (≤4 decodes/slot), so the crowded-band
  arm (multi-pass subtraction stress, ~20–40 overlapping sigs) is lightly exercised.
  When 20m/40m are busy, re-grab a denser slot with the README recipe and add it.
- **RX-888 MkII was evaluated as a better capture source and DROPPED** — bandwidth-boxed
  on a Pi: at the Pi-safe 16 Msps its wideband HF input aliases (only RTL-SDR-class, 5 vs
  7 decodes); the cleaner 32 Msps OOMs the demod (~4 GB). Its 16-bit/all-HF edge needs
  more ingest than a Pi has. **Do NOT re-attempt the RX-888 on this Pi** for narrowband
  FT-8. The full pipeline does work (built streamer `rhgndf/rx888_stream` + FX3 firmware +
  `dev/scratch/rx888_demod.py` offline USB-demod + udev rule, all left on disk) — usable
  on a beefier host. Details in bd `tuxlink-b026z.1` notes. The RTL-SDR set is the oracle.
