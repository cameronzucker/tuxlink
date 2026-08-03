# laguna2 (INCOMPLETE): operator-terminated at 84/180 — Sparks reassigned to tuxlink-bench

Run: 2026-08-03 06:05 to 09:13 AZT, terminated by operator directive at 84
completed attempts (of 180) to free the dual-Spark cluster for the
tuxlink-bench harness build. Data preserved: 84/84 bundles judged, join
clean (0 stale, 0 missing); 17 mid-flight partial attempts pruned as
non-evidence. Marked OPERATOR-TERMINATED in the run log. Agent
chasm-wren-crag.

Model: `poolside/Laguna-S-2.1-NVFP4` at HF revision `f8fdfcdc` — the
**current** shipping build (the Aug-1 "1M context" promotion, +28GB over
the retired 256K build laguna1 ran; `bd show tuxlink-jwdsa` for the
silent-repush forensics). Served dual-solo on both Sparks at 131k + fp8 KV
(262k no longer fits the heavier build), temp 0.7, conc 16, generation
`40fd9b7e`.

## Partial topline (n=84, uneven cell coverage — read with care)

13 PASS / 22 PARTIAL / 49 FAIL = **15.5% strict / 28.6% lenient**, in the
same band as laguna1's 20.5% on the old build and generation. The churn
signature persists on the new build: 14 cancelled (17%) plus 8
invalid_action. Notable slow pace for conc 16 (~27 attempts/hr) — the
profile pairs the Aug main model with the July DFlash draft, and a
mismatched speculative draft tanks acceptance rates (pin drafts and mains
together — bench process rule). Cell coverage is uneven (C1 completed all
10, EU3 zero), so per-cell comparisons beyond directional reads are not
supported.

## Disposition

The operator's read at termination: the run "looks pretty rough" and
Laguna likely needs the advanced affordances of tuxlink-bench's simulated
environment (consent responder, absorption-aware compat, v2 cells) for a
fair measurement. A complete Laguna arm is v2-era work; this partial
stands as the v1 record of the current build's behavior. In the
comparison artifact with an operator-terminated caveat; excluded from any
completed-arm claims.

## Ledger

- Bundles: `r2-poe:~/6i8jz-run/battery-results/laguna2/` (+ `laguna2_joined.json`)
- Joined data: `dev/battery/comparison/data/laguna2_joined.json`
- Burn-in (complete, 16/16): `laguna2-burnin/`; quarantines
  `laguna2-burnin-gate1-xvfb-collision/` document the cross-run display
  collision that also hit opus1
- Arm issue: `bd show tuxlink-jwdsa` (revision forensics + serving
  corrections timeline)
