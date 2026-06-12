# build-weather-map.ts — audit trail (tuxlink-z1b7)

Vetting record for the nationwide NWS-zone → Winlink-catalog weather mapping
(spec `docs/superpowers/specs/2026-06-11-cwa-weather-resolver-design.md`).

## Method

Each NWS public forecast zone is mapped to one catalog product by:

1. **Exact-name (preserved):** zones already in the committed 8-state map keep
   their exact per-zone product (Seattle `WAZ315` → `WA_ZON_SEA`). Zero regression.
2. **Global office-city → product** (any state bucket): the office city (from
   `api.weather.gov/offices/<CWA>`, cached in `dev/scratch/cwa-offices.json`)
   appears in a product description (PSR "NWS Phoenix" → `AZ_TAB_PHOE`). Searched
   across ALL buckets so cross-state offices resolve (St-Louis LSX serving IL
   zones → the MO St-Louis product). High-confidence, so it beats direction.
   Preference zone (`_ZON_`) > tabular (`_TAB_`) > statewide (`_FOR_`).
3. **Per-state direction → product:** when no office-city match, the office's
   fractional position within its state is aligned to the directional product
   whose denoted position is nearest (Tucson, SE of AZ → `AZ_ZON_SE`). Continuous
   nearest-target scoring, no threshold cliffs.
4. **Statewide fallback:** a whole-state product (no sub-region — `VA_ZON_VA`
   "…for Virginia", `HI_ZON_HIISL` "…for Hawaii") covers any remaining grid in a
   state that has one. Accurate, beats no primary card.
5. **Unmapped** otherwise (recorded with reason) → no primary card; the always-on
   "All `<State>` forecasts · N" browse-all carries those grids. Never a
   confidently-wrong product.

## Result (2026-06-11)

- 4024 zones · **3954 mapped (98.3%)** · 70 unmapped · 50 states covered.
- Residual 70: territories with no land forecast (PR = 13, correct — radar +
  browse-all only), Minnesota (56 — offices whose products neither name the city
  nor are directional, and MN has no clean statewide product), 1 Delaware zone.
  These get the always-on browse-all (their state's full product set) — the
  correct behavior, not a coverage defect.

## End-to-end verification (via the real `gridToNwsZone` path)

| Grid | Zone | Primary |
|---|---|---|
| `DM33` (Phoenix, 4-char) | AZZ538 Tonopah Desert (PSR) | `AZ_TAB_PHOE` ✓ |
| `DM33xk` (Phoenix, 6-char) | AZZ543 Central Phoenix | `AZ_TAB_PHOE` ✓ |
| Flagstaff | AZZ012 (FGZ) | `AZ_ZON_NOFLA` ✓ |
| Tucson | AZZ504 (TWC) | `AZ_ZON_SE` ✓ |
| `CN87uo` Seattle | WAZ315 City of Seattle | `WA_ZON_SEA` ✓ (preserved) |
| Detroit | MIZ070 Macomb | `MI_ZON_SE` ✓ |
| Reno | NVZ004 | `NV_ZON_WESNE` ✓ (cross-state) |

## Geometry

`nws-zones.geo.json` carries the **mapped** zones only (unmapped grids get their
state from `us-states.geo.json` for browse-all). Douglas–Peucker at tolerance
0.02° (~2 km; ample given operator grids are 4–6 char, ≥5 km) + 4-decimal rounding,
with GeometryCollection zones flattened to MultiPolygon. **63 MB → 2.2 MB** (gzip ~530 KB).
`pointInRing` is antimeridian-safe for AK/Pacific zones.

## Regenerate

```bash
pnpm tsx scripts/build-request-geo.ts --fetch-only   # zone lists + cwa (all states)
python3 dev/scratch/fetch-geom.py                     # per-zone geometry (bulk /zones returns null)
# (office names: dev/scratch/cwa-offices.json — fetched once from /offices/<CWA>)
pnpm tsx scripts/build-weather-map.ts --write         # emit map + geometry
```
