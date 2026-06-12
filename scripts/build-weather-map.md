# build-weather-map.ts — audit trail (tuxlink-z1b7)

Vetting record for the nationwide NWS-zone → Winlink-catalog weather mapping
(spec `docs/superpowers/specs/2026-06-11-cwa-weather-resolver-design.md`).

## Method

Each NWS public forecast zone is mapped to one catalog product by:

1. **Exact-name (preserved):** zones already in the committed 8-state map keep
   their exact per-zone product (Seattle `WAZ315` → `WA_ZON_SEA`). Zero regression.
2. **Office (CWA) → office-wide product**, in two passes:
   - **City match:** the office city (from `api.weather.gov/offices/<CWA>`, cached
     in `dev/scratch/cwa-offices.json`) appears in a product description
     (PSR "NWS Phoenix" → `AZ_TAB_PHOE`). Preference zone (`_ZON_`) > tabular
     (`_TAB_`) > statewide (`_FOR_`).
   - **Direction match:** when no product names the office city, the office's
     fractional position within the state is aligned to the directional product
     whose denoted position is nearest (Tucson office, SE of AZ → `AZ_ZON_SE`
     "Southeast Arizona"). Continuous nearest-target scoring (no threshold cliffs).
3. **Unmapped** otherwise (recorded with reason) → no primary card; the always-on
   "Browse all `<ST>` · N" carries those grids. Never a confidently-wrong product.

## Result (2026-06-11)

- 4024 zones · **3733 mapped (93%)** · 291 unmapped · 49 states covered.
- Unmapped concentration: cross-state offices that forecast a state's zones but
  file the product under a neighbor's `WX_US_<ST>` bucket (IL zones under St-Louis
  LSX / Paducah PAH; etc.), plus territories with no land forecast (PR/VI = 13).
  These correctly fall to browse-all — a known, safe coarsening, tracked for a
  coverage follow-up.

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
with GeometryCollection zones flattened to MultiPolygon. **63 MB → 2.7 MB.**
`pointInRing` is antimeridian-safe for AK/Pacific zones.

## Regenerate

```bash
pnpm tsx scripts/build-request-geo.ts --fetch-only   # zone lists + cwa (all states)
python3 dev/scratch/fetch-geom.py                     # per-zone geometry (bulk /zones returns null)
# (office names: dev/scratch/cwa-offices.json — fetched once from /offices/<CWA>)
pnpm tsx scripts/build-weather-map.ts --write         # emit map + geometry
```
