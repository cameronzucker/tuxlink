# Sourcing note: `dev/scratch/request-geo/` and future geo assets

The location-aware Request Center hero resolves an operator's Maidenhead grid
to weather products. This build script fetches + processes NWS public forecast
zone data so that tuxlink can map a grid square to the correct catalog entries.

## Source dataset

**NWS Zones API — public forecast zones with geometry**

```
https://api.weather.gov/zones?type=public&area=<ST>&include_geometry=true
```

One request per US state, returning a GeoJSON `FeatureCollection` of public
forecast zones. Each feature carries an `id` (e.g. `WAZ558`), a `name`, an
`effectiveDate`, and a full polygon geometry.

The state set is derived at runtime from `src-tauri/resources/catalog/winlink-queries.txt`:
only `WX_US_<XX>` categories (exactly two uppercase letters) that contain at
least one description matching `/zone forecast/i` are included.

## Invocations

```bash
# Task 1 — fetch + cache raw NWS responses
pnpm tsx scripts/build-request-geo.ts --fetch-only

# Force re-fetch even if cache exists
pnpm tsx scripts/build-request-geo.ts --fetch-only --force

# Full pipeline (adds simplification + zone→catalog map — later tasks)
pnpm tsx scripts/build-request-geo.ts
```

## Raw cache

Raw NWS responses are written to `dev/scratch/request-geo/raw/<ST>.json`
(one file per state, e.g. `WA.json`). This directory is covered by the
`dev/scratch/` entry in `.gitignore` and is never committed.

The fetch is idempotent: a state whose raw cache file already exists is skipped
on subsequent runs unless `--force` is passed. This avoids hammering the NWS
API during iterative development.

## Emitted assets (full pipeline — later tasks)

The full pipeline (no flags) will produce:

| File | Description |
|---|---|
| `dev/scratch/request-geo/zones-simplified.json` | Zone geometries simplified for point-in-polygon queries (Task 2) |
| `dev/scratch/request-geo/zone-catalog-map.json` | `zoneId → [{category, filename}]` mapping (Task 3) |
| `dev/scratch/request-geo/zone-catalog-unmapped.txt` | Zone IDs present in NWS data with no catalog match (Task 3) |
| `dev/scratch/request-geo/radar-table.json` | Radar station → zone coverage table (future task) |

These are scratch outputs; only the production-ready derived assets (zone→catalog
map, bundled geometry) will be committed to the repo after operator review.

## Re-running to refresh data

Run the script without `--force` to update only states whose cache is absent.
Run with `--force` to re-fetch all states from the NWS API. The script logs the
maximum `effectiveDate` seen across all zone features — this serves as the
dataset version.

## Attribution

NWS data is produced by the US National Weather Service (NOAA) and is in the
public domain. The `User-Agent` header identifies tuxlink per NWS API policy:
`tuxlink-dev (https://tuxlink.org)`.
