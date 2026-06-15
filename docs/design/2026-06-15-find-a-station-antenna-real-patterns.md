# Find-a-Station antenna: real elevation patterns, not presets (bd-bl01 epic)

**Date:** 2026-06-15 · **Decision owner:** Cameron (operator-of-record, RF) · **Status:** approved; Phase 0 ready to build

## Problem

The antenna picker offers 10 named-product presets, but the backend (`antenna.rs::ioncap()`)
maps them onto just 3 parametric IONCAP type-codes (22 vertical / 23 dipole / 24 yagi). The
5 "horizontal" presets are byte-identical, the 3 verticals are byte-identical → within a bucket,
different selections produce identical predictions. PR #707 partially fixed the gross
all-isotrope collapse (added 3 archetypes + operator height/ground), but the abstraction itself
is wrong: **a product name cannot determine an elevation pattern** — height, ground, and
deployment config dominate it. Any name→pattern mapping is therefore a fiction (proxy, guess,
or omission). And an antenna-blind "band-open" estimate is meaningless, because a band's
usability for a *link* depends on the antenna's takeoff angle.

## Decision (2026-06-15)

**Keep voacapl. Stop emitting IONCAP type-codes. Feed voacapl real elevation-gain patterns.**

The engine is not the lever. Deep research (run `wf_d521fe59-160`, 102 agents, 24/25 claims
verified) established: voacapl and the ITU-R P.533 reference (ITURHFProp) *both* ingest the
antenna as a real modeled/measured gain table (VOACAP Type-14 = 91-point elevation × 30 freq;
Type-13 = full azimuth × elevation × freq), not parametric presets — so switching engines does
not address the antenna problem. voacapl is also the license-clean choice (effectively GPLv3;
ITURHFProp carries a bespoke ITU "free from copyright assertions / as-is" grant with no GPL
statement — one redistribution claim was outright refuted). ICEPAC/REC533 share the same
IGY-era maps with no advantage; pythonprop/Proppy are only front-ends.

voacapl already supports the Type-13/14 path. We were using its *weak* parametric path. The fix
is to switch the ingestion to real patterns and source those patterns honestly.

## Pattern sources — priority order C > B > A, all feeding the one ingestion point

- **C — precomputed NEC pattern library (default).** Build-time NEC runs over the recycled
  catalog × a height grid, shipped as Type-14 files (~1–2 MB). UI = pick antenna + height →
  real precomputed pattern. Retires the preset dropdown.
- **B — embedded NEC (ground truth).** Bundle `nec2c`/`necpp` (~0.2 MB); operator describes
  real geometry → live NEC run → Type-14 → voacapl. For "I built something weird / want my exact
  setup."
- **A — file import (escape hatch).** Import an `.n13`/`.n14`/EZNEC export straight into the
  ingestion point. Small (just a file read).

No source fabricates gain-vs-angle numbers: C/B compute from NEC, A is the operator's own model.

## Architecture

**Foundation:** `deck.rs`/`engine.rs`/`commands.rs` write a real **Type-14** pattern into the
scratch `antennas/default/` and reference it from the ANTENNA card — reusing #707's
`tx_antenna_voa_content` plumbing (already threads generated content into the scratch). This
*replaces* `operator_voa_content`'s IONCAP-param emission; the change is mostly *what content we
write* (a real 91-point gain table), not new plumbing. The gateway (RX) side keeps its honest
coarse `B`/`D`/`V` self-reported code (the ceiling of the Winlink data), mapped to a
representative real pattern.

## Modeling environment (Tech-Prepper-grounded; operator-confirmed 2026-06-15)

See memory `project_rf_deployment_environment` + `dev/scratch/ham-knowledge-store` Tech Prepper
transcripts (the EmComm Tools creator; audience proxy).

- **Catalog:** recycle the 10 existing hamexandria presets (operator-vetted) — see
  `propagationPrefs.ts::ANTENNA_PRESET_OPTIONS`.
- **Ground default = poor/dry desert** (low ε, low σ), operator-selectable. (Was generic
  "average" ε=13/σ=0.005, which flatters verticals and is wrong for the audience.)
- **Verticals:** modeled ground-mounted over poor soil **with a representative radial field**
  (the audience always deploys radials / Faraday-cloth mats) — documented assumption; not bare
  earth (unfairly pessimistic) and not a perfect ground plane (the opposite lie).
- **Height grid:** realistic no-tree values — wires as slopers/inverted-V at ~2.5–9 m apex;
  verticals ground-mounted. Naturally surfaces the high-angle/NVIS regional behavior the EmComm
  use case needs, vs. idealized half-wave-high patterns.

## Sizing (measured 2026-06-15)

C ≈ 1–2 MB precomputed patterns; B ≈ 0.2 MB (`nec2c` installed-size 221 KB); A ≈ 0. Negligible
next to the ~97 MB existing map payload (go-pmtiles sidecar 52 MB + world basemap ~45 MB) that
actually drove the 18→90 MB .deb growth. Binary size does not differentiate B vs C.

## Sequencing (bd decomposition)

- **Phase 0 — foundation:** Type-14 ingestion + one hand-made test pattern proving the path
  end-to-end. Prerequisite for C/B/A.
- **Phase 1 — C:** NEC pattern library + UI (pick antenna + height). **bd-bl01** (reframed).
- **Phase 2 — B:** embedded NEC from geometry.
- **Phase 3 — A:** pattern-file import.

C/B/A are independent consumers of Phase 0.

## Discipline (RF-critical)

Per the recalibration note's "must" list + project memories (`ai_amateur_radio_reliability`,
`no_carveout_on_cross_provider_adrev`): TDD (deck tests assert a *distinct* pattern per
antenna+height and height-sensitivity — e.g. a low wire's high-angle lobe rises as height
drops), at least one Codex adversarial RF round, and a wire-walk before any "shipped" claim.

## Open questions / caveats

- This fixes the **antenna** side only. The **ionosphere** still rests on voacapl's shared
  IGY-era median maps — unchanged, and a known limitation for short NVIS paths.
- NVIS / short-regional accuracy of the ITS family is not independently validated in the
  literature surfaced by the research (open question, not a blocker).
- C's honesty is bounded by its documented NEC modeling assumptions — they are real NEC runs,
  not guesses, but the assumptions (radial count, ground, height) must ship documented.

## References

- Deep-research run `wf_d521fe59-160` (engine landscape + antenna ingestion, cited).
- `docs/design/2026-06-14-find-a-station-prediction-recalibration.md` (Fix C; this supersedes its
  antenna-model portion).
- VOACAP/HamCAP Type-13/14: https://www.voacap.com/hamcap-type1314.html
- ITU-R-HF (P.533) reference: https://github.com/ITU-R-Study-Group-3/ITU-R-HF
