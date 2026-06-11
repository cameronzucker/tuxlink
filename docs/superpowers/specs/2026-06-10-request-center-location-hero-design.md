# Request Center — location-aware "For your location" hero (design)

**bd:** tuxlink-96lu · follow-up to the Request Center re-skin (PR #559/#564) and the
centered-dialog change (PR #576, tuxlink-4ls0) · 2026-06-10

## Why

The Request Center's "For your location" section resolves two products from the
operator's Maidenhead grid: a whole-state forecast and a coarse marine sea-area.
A whole-state forecast is the wrong default — an operator wants the forecast for
where the station sits, not a statewide summary. The bundled Winlink catalog
(`src-tauri/resources/catalog/winlink-queries.txt`) already carries far more
granular location products: per-NWS-zone text forecasts (68 for Washington
alone), region-scoped radar snapshots, and sea-area marine forecasts. The
"For your location" section surfaces none of that granularity.

This design replaces the State + Marine pair with the complete set of location
products that **resolve and apply** for the operator's grid, each auto-resolved
and labelled with what the CMS returns.

## Definition of done

The "For your location" section is complete when, for any operator grid that
falls within a US state present in the catalog:

1. The section auto-resolves and presents the operator's **exact NWS public
   forecast zone** text forecast as the primary card.
2. The section auto-resolves and presents the **tightest-scoped radar region**
   that contains the grid.
3. The section auto-resolves and presents the **sea-area marine forecast** when,
   and only when, the grid is coastal.
4. Each card states what the CMS returns (text vs image), the resolved
   target (zone name / radar region / sea area), and the catalog filename.
5. The grid-to-zone mapping is **vetted** for every catalog state: every
   `WX_US_<ST>` zone-forecast filename is either mapped to a real NWS zone
   geometry or recorded in an explicit unmapped-by-design list. No fuzzy
   auto-match ships unreviewed.
6. The section is reachable in a real build of the app (not a component in
   isolation), and the existing `src/request` test suite stays green.

Anything outside this set (state forecast, the operator's other in-state zones,
marine-point products, satellite, weather fax) is reachable through Browse, not
the hero.

> **Shipped-coverage reality (added 2026-06-10 during implementation).** DoD #1's
> "exact NWS public forecast zone" primary card resolves **only where the bundled
> Winlink catalog carries per-public-zone forecasts** — verified to be **8 states:
> WA, OR, ME, NJ, VT, NH, DE, AK**. For the other 41 catalog states, the finest
> product the catalog offers is the multi-zone NWS-office-area ZFP (e.g. "Zone
> Forecast for California from Eureka"), which spans many public zones; mapping one
> to a single zone would violate DoD #1, so the zone card correctly **omits** there
> (the regional/office ZFPs stay in Browse, recorded unmapped-by-design). This is an
> **upstream catalog limitation, not unfinished work** — DoD #5's completeness gate
> proves every catalog zone-forecast filename is accounted for. Radar (all 161
> regions, nationwide) and marine (all coasts) resolve regardless, so every US
> operator's hero shows at least radar; the section degrades card-by-card and is
> never empty within the US. Broadening zone coverage requires the catalog to gain
> per-zone products (upstream), not more mapping work here.

## Scope

**In:** `src/request/geo.ts` (zone + radar resolvers), `src/request/sections.ts`
(the location section build), `src/request/catalogMap.ts` (filename resolution),
`src/request/RequestCenter.tsx` + `.css` (the hero markup), a bundled NWS
public-zone geometry asset, a bundled NWS-zone→Winlink-filename mapping, a
radar-region table, and a committed generation script that produces the bundled
data from authoritative sources.

**Out (this design):**
- **METAR.** The catalog's 55 METAR items are non-US (zero `K`-prefix ICAO
  airports); a US operator has no airport METAR to resolve. Excluded.
- **Marine-point products** — buoy observations (`WX_BUOY`), NAVTEX
  (`WX_NAVTEX`), offshore (`WX_OFFSHORE`) — and **satellite** (`SAT_PIX`) and
  **weather fax** (`WX_FAX`). Coverage is sparse and the field use case for a
  hero card is unestablished; these remain in Browse pending operator demand.
- **Non-US grids.** A grid that resolves to no US state presents no location
  hero (the national chips still render). Future scope.
- **Travel mode.** The hero resolves the home grid; choosing a different
  location is future scope.

## The product set (data-grounded)

Every card auto-resolves from the grid and is gated on genuine applicability,
not merely "a nearest exists":

| Card | Resolves to | CMS returns | Applies when |
|---|---|---|---|
| **Zone forecast** (primary) | the exact NWS public forecast zone covering the grid | text, ~4 KB | grid is within a catalog state |
| **Regional radar** | the tightest `WX_US_RAD` region containing the grid | image, ~15 KB | grid is within US radar coverage |
| **Marine forecast** | the grid's sea area (`WX_EASTPAC` / `WX_ATLANTIC` / `WX_CAR_GULF` / `WX_US_COAST`) | text, 5–30 KB | grid is coastal (sea-area resolver returns non-null) |

The set adapts to location: an inland grid (Denver) presents Zone + Radar; a
coastal grid (Seattle: `CN87uo`) presents Zone + Radar + Marine. The cards are
ordered Zone (primary, visually dominant), then Radar, then Marine.

## Geo-resolution architecture

### Zone resolution (the central mechanism)

`grid → lat/lon → NWS public forecast zone → Winlink catalog filename`.

- **Geometry.** Bundle the NWS public forecast zone polygons as
  `src/request/nws-zones.geo.json` (a simplified GeoJSON, mirroring the existing
  `us-states.geo.json`). Source: the NWS public-zone dataset
  (`api.weather.gov/zones?type=public&area=<ST>` returns id + name + geometry; the
  GIS shapefile is the bulk source). The dataset is public domain.
- **Resolution.** `gridToNwsZone(lat, lon): { id, name } | null` runs the same
  point-in-polygon technique as `latLonToUsState`, returning the NWS zone id
  (e.g. `WAZ558`) and name (e.g. `Seattle and Vicinity`).
- **Filename mapping.** A bundled table `nws-zone-to-catalog.json` maps each NWS
  zone id to the Winlink catalog filename. The Winlink zone-forecast descriptions
  are the NWS zone names (verified: catalog `WA_ZON_BLUF` "Foothills of the Blue
  Mountains of Washington" = NWS `WAZ029`). Most rows match by normalised name;
  a tail of catalog descriptions are abbreviated to a length limit
  (`"F-hills & Valleys of cent King County Cascades"`) and are hand-resolved.
  The mapping is built by a committed generation script and reviewed; the fuzzy
  auto-match never ships unreviewed (Definition of Done #5).

### Radar resolution

`grid → radar region`. The `WX_US_RAD` items are region-scoped
(`US.RAD.PSND` "Puget Sound & SJDF", `US.RAD.NWWA` "W Washington & NW Oregon",
`US.RAD.PNW` "Pacific Northwest"). A curated `radar-regions.json` table maps each
`WX_US_RAD` filename to a bounding polygon/box. `gridToRadarRegion(lat, lon)`
returns the smallest-area region containing the grid (so Seattle resolves to
Puget Sound, not the broader Pacific Northwest). The table is built from the
authoritative region extents and committed with the generation script.

### Marine resolution

Unchanged. The existing `latLonToSeaArea` resolver supplies the sea-area marine
card; the marine card renders only when it returns non-null.

## sections.ts

`buildSections` replaces the current Weather (`kind:'location'`) section's two
cards with the resolved set:

- `loc-zone-forecast` (addCms, the resolved zone filename) — primary.
- `loc-radar` (addCms, the resolved `WX_US_RAD` filename).
- `loc-marine` (openBrowse, the sea-area category) — only when coastal.

The cards omit individually when their resolver returns null (no zone match → no
zone card; inland → no marine card). The location section is omitted only when it
has zero cards (a non-US grid). The national sections are unchanged.

## UI (within the centered dialog, PR #576)

The hero presents the zone forecast as a primary, amber-edged card (icon tile +
zone name + "Your NWS public forecast zone" + zone id and filename in mono + Add
control), then radar and marine as a supporting grid. Each card states what the
CMS returns. The Browse reveal scales its label to the in-state zone count
("Browse all WA local forecasts · 68 zones"). The mock is
`docs/design/mockups/2026-06-10-request-center-location-hero.html`.

## Edge cases

- **Grid resolves to no NWS zone** (ocean, non-US, or a gap in the bundled
  geometry): the zone card omits. If the grid still resolves to a US state with
  catalog zones, the Browse reveal routes the operator to that state's zone list.
- **State present in the catalog but a specific zone is unmapped** (the abbreviated
  tail not yet hand-resolved): the unmapped zone is reachable through Browse; the
  hero omits the zone card rather than guess. The mapping-completeness test
  (below) fails the build until every catalog zone filename is mapped or listed
  unmapped-by-design.
- **No radar region contains the grid:** the radar card omits.
- **Grid is coastal but the sea area is not in the catalog:** the marine card
  omits (the resolver already returns one of the catalog categories).

## Testing

- `gridToNwsZone` resolves representative grids per state to the correct zone
  (sampled against the NWS dataset).
- `gridToRadarRegion` resolves representative grids to the tightest region
  (Seattle → `US.RAD.PSND`).
- **Mapping completeness:** a test asserts every `WX_US_<ST>` zone-forecast
  filename in the bundled catalog is present in `nws-zone-to-catalog.json` or in
  the explicit unmapped-by-design list. This enforces Definition of Done #5.
- `buildSections` produces the adaptive set: coastal grid → zone + radar +
  marine; inland grid → zone + radar; non-US grid → no location section.
- No card carries fabricated data: the buoy/satellite/fax families are absent
  from the hero; every resolved filename exists in the bundled catalog (guarded
  by the existing real-file catalog test pattern).

## Data provenance

The bundled assets (`nws-zones.geo.json`, `nws-zone-to-catalog.json`,
`radar-regions.json`) are produced by a committed generation script under
`scripts/`, reading the public-domain NWS public-zone dataset and the bundled
catalog. The script records the source dataset version. Regenerating is a
documented step so the data stays current as the catalog evolves.

## Out of scope / fast-follows

Marine-point products (buoy / NAVTEX / offshore), satellite, and weather fax as
hero cards (pending an operator-validated use case); METAR (no US data); non-US
grids; travel/alternate-location selection.
