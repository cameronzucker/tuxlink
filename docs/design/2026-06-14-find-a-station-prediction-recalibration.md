# Find-a-Station prediction recalibration + antenna height (bd tuxlink-13d8)

Date: 2026-06-14 · Agent: sandbar-crag-cardinal · Branch: `bd-tuxlink-13d8/fas-recal` (off origin/main)

Operator report (2026-06-14): predictions are over-optimistic ("low NVIS + 1 W returned
excellent numbers for all stations within 500 miles"), time-of-day effect looks weak, the
antenna picker offers type but no height, and several fields/header are too wide. Operator
constraint: cannot validate many permutations on air, so the fix must be grounded in
authoritative prior art, not on-air trial-and-error.

This note is the durable spec. Research transcripts (3 parallel agents) and the validation
runs are summarized here; raw runs were `/usr/bin/voacapl` against `/home/administrator/itshfbc`.

## Root cause (investigated; evidence-backed)

NOT a code bug in the hour-selection path — `StationFinderPanel.tsx:44/65` correctly captures
`new Date().getUTCHours()` once and indexes `relByHour[utcHour]` consistently into the map,
rail, and channel grouping. The backend (`parse.rs`) preserves all 24 UTC hours per channel.
Time-of-day plumbing is correct.

The over-optimism is **model calibration + an antenna-model defect**:

1. **REQ.SNR = 22.0 dB-Hz was CW-grade.** VOACAP REQ.SNR is referenced to a 1 Hz bandwidth:
   `REQ.SNR[dB-Hz] = SNR[dB] + 10·log₁₀(bandwidth_Hz)`. 22 ≈ VARA's *absolute decode floor*
   (link establishes then drops), not a *reliable connect*. → near-100% reachability for short
   paths even at 1 W.
2. **Antenna presets collapse to isotropic.** `antenna.rs::voa_file()` maps 5 of 10 presets
   (EfhwSloper, NvisWireDipole, RandomWireUnun, ResonantPortableDipole, MagneticLoop) all to
   `ccir.000` = Type 0 ISOTROPE (flat 0 dBi, no elevation pattern). NVIS physics (high-angle
   gain, height-above-ground) is entirely unmodeled.
3. **No antenna-height input** anywhere — yet height (in wavelengths) is the dominant NVIS gain
   variable.
4. **SSN static** (`ssn-forecast.json` has only `2026-06: 100.0`; `ssn_for` falls back to it for
   all months). Minor; real but no seasonal/cycle variation.

### Validation evidence (direct voacapl, DM43→DM34, 134 mi, June, 100 W)

Local noon at DM43 ≈ UT 19-20; midnight ≈ UT 7. 80m / 40m REL by UT hour:

| scenario | 80m midday (UT19-20) | 80m night (UT11) | 40m night (UT18-21) |
|---|---|---|---|
| A: isotropic + req_snr 22 (**current**) | 0.96 | 1.00 | 0.54-0.81 |
| B: isotropic + req_snr 36 | 0.71 | 0.93 | 0.01-0.10 |
| C: NVIS dipole 0.15λ + req_snr 36 (**proposed**) | 0.83 | 0.98 | 0.02-0.17 |

Scenario A hides everything (flat 0.96-1.00). Scenario C reproduces the correct physics:
80m better at night (0.83 midday → 0.98 night, D-layer absorption), 40m closes at night (MUF
drops below 7 MHz). req_snr is the dominant over-optimism lever; the low dipole adds correct
high-angle gain for short paths (and would *lower* DX — also correct).

## Fix A — REQ.SNR default 22 → 38 (DONE this branch)

`prefs.rs DEFAULT_REQ_SNR_DB = 38.0` + frontend `DEFAULT_PROPAGATION_PREFS.reqSnrDb = 38`.
38 = VOACAP author's SSB anchor; mildly conservative vs VARA-HF reliable-connect (~35-37),
the safe direction for an availability predictor. Operators who saved a custom value keep it.

### Per-mode REQ.SNR table (dB-Hz) — for the follow-up mode-derivation

Converted via `SNR_dB + 10·log₁₀(BW_Hz)`. Confidence per research agent (citations in the
agent transcripts; primary sources: voacap.com "10 mistakes", VOACAP blog, PA3FWM technote).

| Mode | in-channel SNR | BW (Hz) | REQ.SNR (dB-Hz) | conf |
|---|---|---|---|---|
| VARA HF (reliable connect) | ~+1..+3 @ 2.4 kHz | 2400 | **35-37** | 5 |
| VARA HF 500 (narrow) | floor | 500 | ~5-13 (floor) | 4 |
| ARDOP HF (robust) | ~-3..0 @ 500 | 500 | **24-27** | 4 |
| Packet 300 bd (HF) | ~+5..+8 @ ~500 | 500 | ~32-35 | 2 (LOW) |
| Packet 1200 (VHF FM) | n/a — line-of-sight, VOACAP doesn't model | — | — | — |
| SSB voice (ref) | +5..+10 @ 2.5 kHz | 2500 | 38-44 | 9 |
| CW (ref) | -15 @ 2.5 kHz | 2500 | 19 | 9 |
| FT8 (ref) | -21 @ 2.5 kHz | 2500 | 13 | 8 |

**Follow-up:** thread the station's offered HF modes → pick the *most sensitive* (lowest
REQ.SNR) mode the operator has, so reachability = "can I connect with my best mode." Pass as
`reqSnrDb` override on `predictPath`; fall back to the pref (38) when mode unknown.

## Fix B — man-made noise environment (operator-selectable)

Correction to the original framing: −145 dBW is **residential** (VOACAP's own default), NOT
rural-quiet. It's a fine default; the gap is that it isn't operator-selectable. SYSTEM card
field 2 carries a positive dBW@3MHz magnitude (INTEGER — whole numbers only).

Current deck (`deck.rs`) hardwires `145.`. Decode of our SYSTEM line
`SYSTEM       1. 145. 0.10  90. 22.0 3.00 0.10` (from voacapl `decred.for`):
`<dead power> <man-made-noise dBW> <min-angle> <req-rel %> <REQ.SNR dB-Hz> <multipath-power> <multipath-delay>`.

| UI label | field 2 value | renders |
|---|---|---|
| City / Industrial | `140.` | −140.0 dBW |
| Residential (suburban) — **default** | `145.` | −145.0 dBW |
| Rural | `150.` | −150.0 dBW |
| Quiet rural / Suburban-quiet | `155.` | −155.0 dBW |
| Remote | `164.` | −164.0 dBW |

voacapl exposes only this single man-made-noise scalar (atmospheric + galactic are computed
internally from the coeff maps; the per-component toggles in `genois.for` are not deck-wired).

**Plumbing:** add `noise_dbw: f64` (or a `NoiseEnvironment` enum) to `PropagationPrefs` (default
145), thread through `PredictionInputs` → `deck.rs` SYSTEM card; extend `propagation_prefs_write`
signature + `AntennaControl.tsx` + `propagationPrefs.ts`. Note: changing the command signature
is a frontend-contract change — update the `invoke('propagation_prefs_write', …)` call together.

## Fix C — antenna type + height (.voa generation) — the big one

Do NOT precompute patterns in Rust. voacapl computes height-dependent, ground-dependent
elevation patterns internally for parametric IONCAP antenna types. The `.voa` file carries the
type code + geometry + height; emit a tiny `.voa` per prediction with the operator's height
plugged in, and point the ANTENNA card at it.

### .voa format + key type codes (verified against `/home/administrator/Code/voacapl` source)

```
<title ≤70 chars>
 N     N parameters
  0.00  [ 1] Max Gain dBi..:
  23    [ 2] Antenna Type..:   <- type code drives everything
   13   [ 3] Dielectric....:   <- ground ε_r
0.00500 [ 4] Conductivity..:   <- ground σ (S/m)
 7.100  [ 5] Operating Freq:
  -0.50 [ 6] Antenna Length:   <- NEGATIVE = wavelengths, POSITIVE = meters
  -0.15 [ 7] Antenna Height:   <- per-parameter sign convention (iongain.for)
   0.0  [ 8] Gain ab dipole:
```

Type codes: **0** isotrope; **11** tabulated 91-value elevation table (0-90°, dB rel. to max);
**22** IONCAP Vertical Monopole; **23** IONCAP Horizontal Dipole (param 7 = height); **24**
IONCAP Horizontal Yagi (param 8 = gain over dipole); **27** Sloping Vee.

**Sign convention (load-bearing):** negative param = wavelengths (frequency-independent);
positive = meters. Recommendation: **height in POSITIVE meters** (operator enters meters;
voacapl recomputes the per-band height-in-wavelengths pattern within the run — physically
correct for a fixed-height antenna across 80-20m). **Length** `-0.50` (half-wave, resonant —
assumes ATU-matched amateur wire on each band).

### Per-preset mapping

| preset | type | length | notes |
|---|---|---|---|
| NvisWireDipole | 23 | -0.50 | low height → high-angle NVIS lobe emerges |
| ResonantPortableDipole | 23 | -0.50 | |
| EfhwSloper | 23 (or 27) | -0.50 | EFHW≈horizontal dipole; sloper→type 27 if modeling slope (LOW-MED conf) |
| RandomWireUnun | 23 low | -0.50 | no native type; horizontal-dipole proxy (LOW conf) |
| BaseVerticalRadials / PortableVerticalWhip / MobileHfWhip | 22 | -0.25 | ground-mounted; "height" = element length, NOT feedpoint height — UI height input N/A for verticals |
| BeamYagi | 24 | -0.50 | param 8 = gain over dipole ~5 dB; height = boom height |
| MagneticLoop | 23 low (or hand type-11) | -0.50 | no native type; proxy (LOW conf — flag) |
| Unknown | 23 | -0.50 | generic horizontal at default height |

Ground constants (`[3]` ε_r, `[4]` σ): Average ε_r=13 σ=0.005; Sea 80/5; Poor 3/0.001. Expose
a ground-type dropdown (default Average). Note shipped `ccir.*` use ε_r=4 σ=0.01 (older ITS
"average" convention) — pick one and be consistent.

### Engine change required for C

`deck.rs` references `[default/<file>]` which voacapl resolves to `<root>/antennas/default/<file>`;
`engine.rs` symlinks `antennas` **read-only**, so generated files can't be written there. Change
`make_scratch_itshfbc` to create a **writable** `antennas/default/` (real dir + per-file symlinks
to the bundled stock files, so `ccir.000` etc. still resolve), and have the run path write the
generated `tx.voa`/`rx.voa` into it (short names fit the 13-char card slot). Thread the generated
`.voa` content for tx/rx through `PredictionInputs` (e.g. `tx_antenna_voa_content: Option<String>`)
→ `run_voacapl`. This is RF-critical engine code; **must be CI-compiled + the antenna-specific
deck tests extended**, and ideally a `propagation_live`-style integration run confirming the NVIS
reliability shifts with height (write a type-23 .voa at 0.15λ vs 0.6λ, assert short-path REL rises
at low height) before merge.

## Visual polish (items 1-3) — separate PR; UI/UX → brainstorm/wire-walk first, grim-verify (NOT Chromium)

Reference mock: `dev/scratch/2026-06-10-find-a-station-map-mockD-propagation.html`.

1. **Band success bars colored by %** — currently always orange. The mock colors the `.prop .pbar
   .fill` by reachability tier (`relToTier`: ≥0.70 good/green, ≥0.40 fair/yellow, ≥0.15
   marginal/orange, else skip). Apply `tierColorVar(relToTier(rel))` to the bar fill in
   `StationRail.tsx` (the path-forecast bars).
2. **Dot/symbol disambiguation** — modes (VARA/ARDOP/Packet) and reachability tiers
   (good/fair/marginal/skip) reuse overlapping colored dots; even the mock is ambiguous
   (green=VARA=good, orange=ARDOP=marginal). Give modes a distinct SYMBOL/shape set and
   reachability a distinct color ramp so they can't be confused. **Needs a brainstorm** — design
   the two encodings.
3. **Field compaction** — many 0.63.0 fields are full-width on their own lines; header too tall.
   Pack onto shared rows, shrink the header, give the map more vertical space (mock's tight
   `pane__head` + one-line `bandbar`).

## bd / status

- tuxlink-13d8 (this) — owns the worktree. Fix A landed; B, C specced here.
- Supersedes the open RF-model portion of **tuxlink-s0r1 #3** (which shipped req_snr 22 +
  isotropic presets — the values this corrects).
- File a separate bd for the visual polish (items 1-3) before that PR.
