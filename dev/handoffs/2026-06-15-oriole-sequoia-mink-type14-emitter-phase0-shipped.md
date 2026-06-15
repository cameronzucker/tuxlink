# Handoff — Find-a-Station antenna Phase 0: voacapl Type-14 emitter shipped (CI-green, Codex-reviewed)

**Agent:** oriole-sequoia-mink · **Date:** 2026-06-15
**Arc:** Built bd-tuxlink-j394 (Phase 0 of the antenna real-patterns epic) end-to-end:
the byte-exact VOACAP Type-14 `.voa` emitter, verified against voacapl two ways
(format round-trip + physics gate), Codex RF round, hardened, CI-green. PR #738 READY.

## SHIPPED (committed + pushed; PR #738 READY, awaiting operator merge)

**`src-tauri/src/propagation/type14.rs`** — `Type14Pattern { title, 30× FreqBlock { efficiency, gains[91] } } → to_voa() → Result<String, Type14Error>`.
Byte-exact fixed-format emitter: CRLF + trailing CRLF; 5-line header (Max Gain / Antenna
Type=14 / Frequency); 30 blocks × 10 lines; first line `%2d` index + `%6.2f` eff + 1 space +
10×`%7.3f` gains; continuations = 9-space indent + 10×`%7.3f`; **F7.3 with g77-style
leading-zero suppression** (`0.0`→`   .000`, `-0.072`→`  -.072`). Registered in `mod.rs`.
`.gitattributes` pins the golden fixture `-text` so CRLF survives CI checkout.

- Branch `bd-tuxlink-j394/type14-ingestion`, worktree `worktrees/bd-tuxlink-j394-type14-ingestion`.
- Commits: `d128cf61` (emitter + tests + golden) · `4928f520` (Codex value-validation hardening).
- **All 4 CI jobs PASS** (build-linux amd64/arm64 + verify amd64/arm64; verify runs the 15 Rust tests).

### Verification (the RF-critical part)
1. **Format:** a reference formatter (`dev/scratch/type14_ref.py`, gitignored) round-trips
   voacapl's own `sample.14` **byte-for-byte** (22625 bytes, `cmp` silent) → the fixed-format
   layout matches what voacapl's loader (`antcalc.for:184`, direct-access binary) expects.
2. **HARD GATE (physics) PASSED:** a high-angle pattern emitted through this format drives the
   215 km DM43→DM34 NVIS deck to **REL 1.00 / peak SNR 59 dB**; a zenith-null low-angle control →
   **REL 0.69 / 28 dB**. The **31 dB SNR delta equals the +6/−25 dBi gain difference** at the ~70°
   takeoff angle → voacapl reads my emitted gains into the *correct elevation bins*. (Run at
   REQ.SNR 24 dB; at the project-standard 73 dB both floor to ~0, which is why the keystone only
   saw the low→0 case. REL is thresholded; SNR is the continuous discriminator.)
3. **Golden contract:** `src-tauri/src/propagation/testdata/type14_hiang_golden.voa` is that
   voacapl-accepted high-angle pattern; a Rust test asserts `to_voa()` is byte-equal to it.

### Codex RF adversarial round — DONE
Agent `butte-isthmus-alder`. One real **P1/HIGH**: `to_voa()` validated block/gain count + title
but not gain/efficiency **values** — a real NEC deep null below −100 dBi (`-100.000` = 8 chars)
overflows the F7.3 field, widens the column, and silently shifts voacapl's gain table; NaN/inf fit
width 7 so need an `is_finite()` guard. **Fixed** (`4928f520`): 4 new `Type14Error` variants +
TDD tests for non-finite/overflow gains & efficiency and boundary-fit acceptance. Transcript:
`dev/adversarial/2026-06-15-type14-emitter-codex.md` (gitignored).

### Scope guard honored
Phase 0 did **NOT** switch the runtime default off the IONCAP path → no regression. The emitter
is foundation-only; nothing user-reachable yet (so the epic **wire-walk** correctly applies at
Phase 1's integration boundary, not now).

## State

- **My session branch:** `bd-tuxlink-j394/type14-ingestion` — HEAD `4928f520`, pushed, CI-green, PR #738 READY.
- **Worktree `worktrees/bd-tuxlink-j394-type14-ingestion`** (KEEP until #738 merges):
  - tracked: clean (all committed).
  - untracked/gitignored-stateful: `node_modules/` (reinstalled this session — was missing;
    pre-push `pnpm lint:docs`/tsx needs it), `dev/scratch/type14_ref.py` (gitignored reference
    formatter — the executable spec + voacapl round-trip harness, reusable for Phase 1),
    `dev/adversarial/2026-06-15-type14-emitter-codex.md` (gitignored Codex transcript).
- **voacapl dev scratch** (not in git): test patterns left at
  `~/itshfbc/antennas/default/{hiang,loang}.voa`; `~/itshfbc/run/voacapx.dat` restored to the
  keystone deck after the experiment.
- **Main checkout** `bd-tuxlink-xygm/recover-handoffs`: still **contended** (2 other live sessions
  on it at session end) → could not commit handoffs there.

## ⚠️ UNCOMMITTED handoffs on the main checkout (contention-blocked)

Two session-end handoffs are sitting **untracked** in `dev/handoffs/` on the main checkout and
must be committed on `bd-tuxlink-xygm/recover-handoffs` once it is no longer contended:
1. `2026-06-15-sage-gully-larch-region-download-shipped-hf-antenna-realpatterns-decided.md` (prior session).
2. `2026-06-15-oriole-sequoia-mink-type14-emitter-phase0-shipped.md` (this one).
The durable cross-session state (bd-j394 notes in Dolt + pushed PR #738) is intact regardless;
these markdowns are continuity convenience, not the source of truth.

## Pending operator actions

- **Merge PR #738** (Phase 0 emitter — CI-green, Codex-reviewed, ready).
- **Merge PR #716** (region download) and **PR #735** (antenna design doc) — still pending from prior session.
- Commit the two uncommitted handoffs above when the main checkout frees up.

## Next work (epic)

`tuxlink-j394` stays `in_progress` until #738 merges; on merge it closes and **unblocks
`tuxlink-bl01` (Phase 1/C — precomputed NEC pattern library)**. Phase 1 consumes this emitter:
build NEC over the recycled 10-preset catalog × a height grid (poor/dry-desert ground default,
verticals over a radial field, low no-tree heights), ship Type-14 files, retire the preset
dropdown, and **clamp deep NEC nulls to ≥ −99.999 dBi before emitting** (the emitter now errors
on overflow by design). Phase 1 is where the wire-walk gate fires.
