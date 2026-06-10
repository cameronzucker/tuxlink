# voacapl I/O contract — captured by running it (U1 grounding spike, 2026-06-10)

> Agent `isthmus-condor-kingfisher`. Real capture against `voacapl` built from
> source on this arm64 Pi; nothing here is inferred from prose or a single
> example. Resolves the spec §12 open item "exact `voacapl` input-deck shape +
> output parse" and feeds the U1 plan (`tuxlink-ipjt`, umbrella `tuxlink-axq0`).
> Spec: [`docs/design/2026-06-10-find-a-station-propagation-map-design.md`](../design/2026-06-10-find-a-station-propagation-map-design.md) §5.

## What was run

- Cloned `github.com/jawatson/voacapl`, configured `--prefix=$HOME/.local` (no
  sudo for install), `make` (clean, exit 0 on **aarch64** — proves arm64
  field-feasibility), `make install`, then `makeitshfbc` to materialise
  `~/itshfbc/` (coefficient data + run dir).
- **Only system change requiring sudo:** `apt install gfortran` (operator-approved).
- Ran a real **DM43 → DM34** point-to-point circuit (operator-ref grid → N0DAJ,
  Wickenburg AZ) with N0DAJ's real VARA HF dials. Engine ran clean; output
  round-trips (echoed circuit + 215.2 km distance + 301.65° azimuth all correct).

Tracked artifacts (committed in this branch):
- `src-tauri/tests/fixtures/voacap/dm43-dm34-input-deck.dat` — the real input deck that ran.
- `src-tauri/tests/fixtures/voacap/dm43-dm34-voacapx.out` — the real 684-line output (24 hourly blocks).

Reference prototypes (gitignored scratch on the author's disk, not in the repo;
re-derivable from this contract): `gen_voacap_deck.py` (parametric deck generator),
`parse_voacapx_out.py` (output parser). The tracked fixtures above are the ground
truth the U1 Rust tests assert against.

## Invocation

```sh
voacapl ~/itshfbc            # reads ~/itshfbc/run/voacapx.dat, writes ~/itshfbc/run/voacapx.out
```

The sole argument is the **itshfbc root**. Input/output filenames are fixed
(`run/voacapx.dat` → `run/voacapx.out`). For a per-station sidecar U1 must either
(a) point each run at a per-run itshfbc-style dir, or (b) serialise runs through
one run dir. **Open item for U1:** concurrency model + scratch-dir isolation
(the binary writes `voacapx.out` in the run dir — parallel runs collide).

## Input-deck card formats (authoritative — from `src/voacapw/voacap.for` WRITE formats)

Fixed-column "card" format. Keyword left-justified in cols 1-10, then fixed-width
numeric fields. **These are FORTRAN format strings, copied from source — not guessed.**

| Card | FORTRAN format | Notes |
|---|---|---|
| `COEFFS` | `'COEFFS    ',a4` | `CCIR` |
| `TIME` | `'TIME      ',4i5` | start, stop, step, ? → `1 24 1 1` = all 24 UTC hours |
| `MONTH` | `'MONTH     ',i5,10f5.2` | year (i5), month as f5.2 (`6.00`) |
| `SUNSPOT` | `'SUNSPOT   ',12f5.0` | smoothed SSN; **f5.0 keeps the trailing dot** (`100.`) |
| `LABEL` | `'LABEL     ',2a20` | two 20-char labels (tx, rx) |
| `CIRCUIT` | `'CIRCUIT   ',f5.2,a1,3(f9.2,a1),2x,a1,1x,i5` | **see below — the load-bearing one** |
| `SYSTEM` | `'SYSTEM    ',f5.0,f5.0,f5.2,f5.0,3f5.2` | noise / req-rel / req-snr / multipath tol |
| `FPROB` | `'FPROB     ',4f5.2` | F-layer probabilities |
| `ANTENNA` | `'ANTENNA   ',4i5,f10.3,1h[,a21,1h],f5.1,f10.4` | `.voa` pattern path in `[...]`; **last f10.4 = power kW** |
| `FREQUENCY` | `'FREQUENCY ',11f5.2` | exactly 11 MHz slots; unused = `0.00` |
| `METHOD` | `'METHOD    ',2i5` | `30 0` = METHOD 30 (complete system perf, point-to-point) |

### CIRCUIT card — the one a hand-guess gets wrong

```
'CIRCUIT   ',f5.2,a1,3(f9.2,a1),2x,a1,1x,i5
 └ 10 chars ┘└TXlat┘└┘└──TXlon──┘ └──RXlat──┘ └──RXlon──┘  └P┘ └ i5 ┘
```

- **Asymmetry:** TX-lat is `F5.2` (5 wide), but TX-lon / RX-lat / RX-lon are each
  `F9.2` (9 wide). Easy to get wrong if you assume all four are identical width.
- Each coordinate is immediately followed by a 1-char **hemisphere letter**
  (`N`/`S`, `E`/`W`). **Longitude sign lives in the letter**, magnitude positive:
  111°W → `111.00` + `W`, never `-111.00`. The U1 deck-builder must split signed
  lat/lon into (magnitude, hemisphere).
- `2x` then path direction `a1` (`S`hort / `L`ong great-circle), `1x`, `i5` (0).
- Verified: generated card length == 55, and the echoed card in `voacapx.out`
  row 15 matches byte-for-byte.

## Output format (`voacapx.out`, METHOD 30)

- Header block (program name, version `16.1207W`, then a verbatim **echo of every
  input card** — useful for provenance/debugging).
- A circuit summary block per page:
  - Row with `AZIMUTHS` + `N. MI.` + `KM` headers, then the data row:
    `33.50 N  111.00 W - 34.50 N  113.00 W    301.65  120.54     116.2    215.2`
    → **TX→RX azimuth 301.65°** (antenna bearing for the right-rail compass),
    reverse azimuth 120.54°, distance 116.2 N.Mi. / **215.2 km**. U1 reads bearing
    + distance straight from here — no separate Maidenhead great-circle math needed.
- **24 per-hour blocks.** Each opens with a FREQ header row, then ~21 labelled
  parameter rows. Layout:

```
   1.0  7.8  3.6  7.1  7.1 10.1 14.1 14.1  0.0 ...  FREQ   <- hour, MUF, then freqs
        1F2  1F2  1F2  1F2  1F2  1F2  1F2   -   ...  MODE
       ...                                           ... (TANGLE, DELAY, V HITE, MUFday, LOSS, DBU)
        -98  -85  -91  -91 -111 -157 -157   -   ...  S DBW  <- signal power dBW
         59   65   66   66   49    9    9   -   ...  SNR    <- dB
       0.04 0.21 0.18 0.18 0.03 0.00 0.00   -   ...  REL    <- circuit reliability 0..1 (HEADLINE METRIC)
       ...                                           ... (MPROB, S PRB, SIG LW/UP, SNR LW/UP, TGAIN, RGAIN, SNRxx)
```

### Parse contract

1. Find a `FREQ` header row (right-edge label `FREQ`, a 6-char field at col 67,
   0-based — verified against the captured output; the data region never reaches
   col 67). First token =
   UTC hour, second = MUF (MHz), rest = active frequencies (this block's column key).
2. Read following rows; the **right-edge label** (6-char field at col 67) names the parameter
   (`REL`, `SNR`, `S DBW`, `MUFday`, `MODE`, …).
3. Each value is a fixed ~5-char field in the data region (cols 6-60), one per
   frequency; `-` marks an unused frequency slot.
4. **REL** is the headline reachability metric the spec ranks on. The 24 hourly
   REL values per frequency feed the right-rail "24-h reliability sparkline".

### Real captured REL (DM43→DM34, June, SSN 100, 100 W) — physics sanity check

40 m (7.1 MHz) peaks **at night** (NVIS short-path), 20 m is dead at 215 km —
correct physics, confirming the deck + parse are real:

```
UTC  MUF   REL( 3.59 7.10 7.11 10.15 14.10 14.12 )
 1.0   7.8  0.04 0.21 0.18 0.18 0.03 0.00
11.0   5.3  0.08 0.31 0.05 0.05 0.00 0.00
20.0   7.8  0.00 0.00 0.01 0.01 0.01 0.00     <- daytime, short NVIS path weak
```

(Absolute REL is low because REQ.SNR=73 dB + 100 W into modest antennas is a
demanding bar; the *shape over time/band* is what matters and is correct.
Default power / REQ.SNR / antenna model are U1 config decisions — see below.)

## Open items carried into the U1 plan

1. **Concurrency / scratch isolation** — fixed `run/voacapx.out` filename; parallel
   per-station runs collide. Need per-run dir or serialised queue (ties to spec §5
   ranking mode (a) point-to-point-per-station vs (b) area-coverage).
2. **Default power / antenna / REQ.SNR** — spec §5 says 100 W + simple dipole,
   operator-configurable. The shipped antenna `.voa` files (`const17.voa`,
   `swwhip.voa`) are placeholders; U1 picks defensible amateur defaults. Do NOT
   hand-tune REL — the engine is the source of truth (amateur-radio-reliability
   caution).
3. **`.voa` antenna pattern bundling** — runs reference `default/*.voa` under
   `~/itshfbc/antennas/`; U1 must bundle these per-arch alongside the binary +
   coeffs.
4. **`itshfbc` data bundling in CI** — `makeitshfbc` materialises a ~MB data tree
   from the install prefix; U1 bundles binary + coeffs + antennas per-arch
   (arm64 + amd64).
5. **SSN cache/forecast format** — SSN is the only time-varying input; bundle a
   forecast table, cache under `app_data_dir()`, never a per-session download.
6. **Month/hour granularity** — one run covers all 24 UTC hours for one month;
   the map's "time now" selects an hour column; "band now" selects a frequency.
```
