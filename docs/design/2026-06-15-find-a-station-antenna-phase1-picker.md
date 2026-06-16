# Find-a-Station antenna Phase 1 (C): precomputed pattern library + antenna/height picker

**Date:** 2026-06-15 · **Decision owner:** Cameron (operator-of-record, RF) · **bd:** tuxlink-bl01
**Status:** approved (brainstorm 2026-06-15); ready to plan
**Parent epic:** [`2026-06-15-find-a-station-antenna-real-patterns.md`](2026-06-15-find-a-station-antenna-real-patterns.md) — Phase 1/C
**Depends on:** Phase 0 (tuxlink-j394, PR #738 merged) — the VOACAP Type-14 emitter and ingestion point.

## Problem (recap)

The antenna control offers 10 product-name presets that the backend collapses onto 3 IONCAP
type-codes, so different selections within a bucket produce byte-identical predictions. Phase 0
replaced the IONCAP path with real Type-14 elevation-gain ingestion. Phase 1 supplies the real
patterns and the UI to choose among them, and retires the preset dropdown.

## Scope

Build-time NEC patterns over a curated antenna catalog × a height grid, shipped as Type-14 files
and selected by an antenna + height picker with a live elevation-pattern preview. One ground
(poor/dry desert) for Phase 1. This is the epic's first user-reachable flow; the wire-walk gate
fires here.

Out of scope: embedded live NEC from operator geometry (Phase 2/B), pattern-file import (Phase 3/A),
a ground × pattern matrix (documented Phase 1 limitation, below).

## Curated antenna catalog (8 entries)

Two classes, because feedpoint/apex height moves the elevation lobe only for horizontal antennas;
ground-mounted verticals are modeled at a fixed geometry.

| Value (serde) | Label | Class | Height axis |
|---|---|---|---|
| `efhw-sloper` | End-fed half-wave (EFHW) / sloper | horizontal wire | grid |
| `nvis-wire-dipole` | Low NVIS wire dipole / OCFD | horizontal wire | grid |
| `resonant-portable-dipole` | Portable dipole (linked / inverted-V) | horizontal wire | grid |
| `beam-yagi` | Beam / Yagi (directional) | horizontal | grid |
| `portable-vertical-whip` | Portable vertical whip | vertical | ground-mounted (fixed) |
| `base-vertical-radials` | Base vertical + radials | vertical | ground-mounted (fixed) |
| `mobile-hf-whip` | Mobile HF whip (screwdriver / Hamstick) | short vertical | ground-mounted (fixed) |
| `unknown` | Unknown / generic | neutral | none (neutral pattern) |

**Removed:** `random-wire-unun` and `magnetic-loop`. Neither has a defensible distinct NEC model —
a random wire's pattern is entirely geometry-dependent, and a magnetic loop has no canonical NEC
archetype. With a live preview drawing a lobe for every entry, modeling them would render a visible
fiction. Operator decision 2026-06-15: remove both.

**Migration:** the `AntennaPreset` enum drops the two removed variants. `propagation_prefs_read`
maps any persisted `random-wire-unun` or `magnetic-loop` value to `unknown` so existing installs do
not crash or present an empty picker. The Rust and TypeScript enums stay in sync.

### Height grid (horizontal antennas)

`2.5 m · 4 m · 6 m · 9 m` apex. No-tree desert-realistic supports per the parent epic's modeling
environment (memory `project_rf_deployment_environment`); this range naturally surfaces the
high-angle/NVIS behavior the EmComm use case needs rather than idealized half-wave-high lobes.
Verticals carry no height axis (ground-mounted at a fixed representative geometry).

## Picker interaction (approved: slider + live preview)

In the existing inline `station-finder__antenna` control strip — no pop-up window
(`feedback_inline_ui_no_window_clutter`):

- **Antenna** — `<select>` with the 8 curated entries.
- **Height** — a snapping slider with labeled ticks at the four grid stops. It cannot land between
  patterns that exist; the value reads back as a grid stop.
- **Conditional vertical state** — when a vertical or `unknown` is selected, the slider is replaced
  by an honest "Ground-mounted — height fixed" (verticals) / neutral note (`unknown`).
- **Live elevation preview** — a small polar plot (0° horizon → 90° zenith, the antenna-pattern
  convention) that redraws the real NEC lobe for the selected antenna + height and marks the peak
  takeoff angle. Updates on antenna or height change. `unknown` draws a flat/neutral lobe labeled
  "not modeled."
- **Ground default** flips `average` → `poor-soil` (`GROUND_TYPE_OPTIONS` order unchanged). Ground,
  Noise, Req SNR, TX power fields are otherwise unchanged.

## Architecture & data flow

```
build time:  NEC (nec2c/necpp)  ──►  Type14Pattern  ──►  to_voa()  ──►  bundled .voa assets
                                          │                (Phase 0 emitter; null-clamped)
                                          ▼
runtime:  pick antenna+height ─► select bundled .voa ─┬─► scratch antennas/default/  ─► voacapl
                                                       │      (Phase 0 ingestion point)
                                                       └─► elevation slice (91 pts @ op freq) ─► preview
```

- **Bundled assets.** The build emits one Type-14 `.voa` per `{horizontal antenna × grid height}` +
  one per vertical + one neutral `unknown` = 20 files (4 × 4 + 3 + 1). Sized well under the 1–2 MB epic budget.
  Patterns ship as bundled resources, not generated per-run.
- **Ingestion (unchanged).** The selected pattern feeds voacapl through the Phase 0 path: written
  into the scratch `antennas/default/` and referenced from the deck ANTENNA card.
- **Preview interface (new).** The preview needs the gain-vs-elevation curve in the frontend. The
  backend exposes a **compact elevation slice** (the 91-point elevation column at the operating /
  predicted frequency) for the selected antenna + height — a read-only projection of the same
  Type-14 data that feeds voacapl, so there is one source of truth and no second pattern. Surface as
  a dedicated command (e.g. `antenna_pattern_preview`) or a field on the predict response; the plan
  step picks one.

## NEC build — critical constraints

- **Null clamp (REQUIRED).** Deep NEC nulls below the F7.3 field minimum overflow the Type-14 column
  and the Phase 0 emitter now **errors by design** on field overflow. The build MUST clamp every
  gain to **≥ −99.999 dBi** before calling `to_voa()`, or the library build fails.
- **Ground.** All patterns modeled at the default poor/dry-desert ground (low ε, low σ).
- **Verticals** modeled ground-mounted over poor soil **with a representative radial field** (the
  audience deploys radials / Faraday-cloth mats) — a documented assumption, neither bare earth
  (unfairly pessimistic) nor a perfect ground plane (the opposite lie).
- **Yagi** modeled at boresight; Type-14 is elevation-only, so azimuth gain is not represented.

## Documented modeling assumptions (ship in-app + in this doc)

Per `feedback_ai_amateur_radio_reliability`, pattern honesty is bounded by stated assumptions:
radial count, the poor-soil ground, the discrete height grid, and yagi-at-boresight. These ship
documented, not implied as exact.

**Single-ground limitation (Phase 1).** Patterns are precomputed only at poor/dry-desert ground. The
ground selector still feeds voacapl's path-level ground card, but the antenna *pattern* axis is fixed
at poor soil. For the target audience this is the representative case — coastal US chaparral-biome
soil south of NorCal is effectively desert-like, so poor soil covers nearly the whole audience. A
salt-water operator's vertical would have a stronger low-angle lobe than the preview shows; that is
the documented edge. Operator decision 2026-06-15: accept the limitation for Phase 1; no ground ×
pattern matrix.

## Testing & RF discipline

Per the parent epic's "must" list and `no_carveout_on_cross_provider_adrev`:

- **TDD.** Deck tests assert (a) a *distinct* `.voa` per antenna + height, and (b) height
  sensitivity — a low horizontal wire's high-angle lobe rises as apex height drops (e.g. type-23 at
  2.5 m vs 9 m short-path reliability). Migration test: a persisted removed preset reads back as
  `unknown`. Preview-slice test: the frontend elevation curve equals the emitted Type-14 column.
- **Null-clamp test.** A synthetic pattern with a sub-−100 dBi null builds successfully after
  clamping (and would error without it).
- **At least one Codex adversarial RF round** on the implementation diff.
- **Wire-walk** at the integration boundary — the operator picks an antenna + height, the preview
  redraws, and the forecast changes — before any "shipped" claim. This is the epic's first
  user-reachable flow.

## Open / deferred

- Ground × pattern matrix — deferred (documented Phase 1 limitation above).
- Ionosphere still rests on voacapl's shared IGY-era median maps (epic-level limitation, unchanged).
- Per-antenna default height (which grid stop is preselected) — pick a sensible default per antenna
  in the plan (e.g. 6 m for wires); not a blocking decision.

## References

- Parent epic + decision: `docs/design/2026-06-15-find-a-station-antenna-real-patterns.md`.
- Phase 0 emitter: `src-tauri/src/propagation/type14.rs`; handoff
  `dev/handoffs/2026-06-15-oriole-sequoia-mink-type14-emitter-phase0-shipped.md`.
- Current UI: `src/catalog/AntennaControl.tsx`, `src/catalog/propagationPrefs.ts`.
- Recalibration note (per-preset mapping table, confidence flags):
  `docs/design/2026-06-14-find-a-station-prediction-recalibration.md`.
