# Request Center weather resolver — per-public-zone, all states (design)

**bd:** tuxlink-z1b7 · supersedes the resolution model of the location-hero spec
(`2026-06-10-request-center-location-hero-design.md`, tuxlink-96lu) · 2026-06-11 ·
agent yew-wren-fir

> **Revision history:** v1 of this spec proposed keying resolution on NWS County
> Warning Area (CWA) polygons. A self-adrev (2 independent reviewers, catalog-grounded;
> `dev/adversarial/2026-06-11-cwa-weather-resolver-selfadrev.md`) proved CWA is the
> wrong key — the catalog is not 1:1 with CWAs (the Phoenix CWA alone carries 4
> distinct products), no CWA-geometry endpoint exists, and collapsing to CWA regresses
> the 8 states that work today. **This v2 keys on the NWS public forecast zone (the
> shipped resolver) and extends its coverage to all states.** CWA is used only as a
> build-time grouping aid, never as the resolution key.

## Why

The location hero (tuxlink-96lu, shipped) resolves weather from the operator's
Maidenhead grid. It works in Seattle and fails in Phoenix: an operator at grid
`DM33` (Phoenix) is offered the **Northern Arizona** product (`AZ_ZON_NOFLA`,
Flagstaff office), which does not cover Phoenix, and **no alternatives**.

### Root cause

The Winlink catalog (`src-tauri/resources/catalog/winlink-queries.txt`, 523 weather
products across 56 states/territories) carries, for each state, a set of products
keyed to NWS forecast regions. The shipped resolver maps the operator's **exact NWS
public forecast zone → catalog filename** via a vetted table (`nws-zone-to-catalog.json`),
but that table and the bundled zone geometry (`nws-zones.geo.json`) **cover only 8
states** (WA/OR/ME/NJ/VT/NH/DE/AK) — the states whose catalog products are per-public-zone.
The other ~48 states were placed in an "unmapped-by-design" list and resolve to
`null`. Arizona is one of them. The spec and mock were validated **only on
Seattle/WA**, so the coverage gap shipped across three iterations. **The
implementation conforms to the spec; the spec's coverage is the defect.**

### Decision (operator, 2026-06-11)

"**Pick region + show all.**" Auto-resolve the operator's actual local product
(Phoenix → `AZ_TAB_PHOE`) **and always** offer "Browse all `<ST>` weather · N".

This is achieved by **extending the existing per-public-zone resolver to every state**
— additive, zero regression to the 8 working states. Each NWS public zone maps to
exactly one catalog product. This dissolves every ambiguity the CWA model could not:
within the Phoenix office (PSR) the Phoenix-metro zones map to `AZ_TAB_PHOE` while the
Yuma/SW zones map to `AZ_ZON_SW`; a zone's product is independent of which
`WX_US_<ST>` bucket the filename files under, so cross-state products resolve by
geometry.

## Definition of done

1. For **any** operator grid inside an NWS public zone, the section auto-resolves the
   **catalog product mapped to that zone** as the primary card. Phoenix (`DM33XK`)
   resolves to `AZ_TAB_PHOE`; Flagstaff resolves to `AZ_ZON_NOFLA`; Seattle still
   resolves to `WA_ZON_SEA` ("City of Seattle") — **no regression to the 8 states
   that work today**.
2. The primary product for each zone is recorded in the **vetted mapping table**
   (table-driven), not derived by parsing `_ZON_`/`_TAB_` from filenames at runtime.
   Vetting preference when a region offers more than one product: zone-text (`_ZON_`)
   > tabular (`_TAB_`) > statewide text (`_FOR_`). Where only one exists (Phoenix has
   only the tabular `AZ_TAB_PHOE`), that one is primary, and the card states it is a
   tabular/numeric forecast (DoD #5).
3. The section **always** offers "Browse all `<ST>` weather · N products" (N = live
   count for the operator's state), navigating Browse pre-filtered to that category.
   The browse list is **unioned by geometry**, not only by state bucket: products
   whose region contains the grid but file under a neighbor state (mirror-pair
   cross-state products) are included and de-duplicated. This affordance is present
   even when the primary resolves, and is the sole weather-text affordance when the
   grid's zone has no mapped land product.
4. Radar (tightest-region) and marine (coastal-only) resolution are unchanged.
5. Each card states what the CMS returns (text vs image), the resolved target
   (zone/region name · filename), and whether a text product is zone-text or tabular.
6. The zone→product mapping is **vetted for every catalog state** — every NWS public
   zone in the bundled geometry maps to a real catalog filename or is recorded in an
   explicit `unmapped` list with a reason; every `WX_US_<ST>` land product is reachable
   from at least one zone OR from the state's browse-all. No fuzzy auto-match ships
   unreviewed. The build script proposes mappings (by CWA + region-name heuristics);
   a human/agent vets, with the audit trail in `scripts/build-weather-map.md`.
7. **Territories / states with no land forecast** (`PR`, `GUAM`, `SAMOA` — all
   products are marine/discussion) are on an explicit `no-land-forecast` list: their
   zones map to no weather-text card; the section is carried by radar + browse-all.
   No marine product is ever mislabeled as "your forecast".
8. **States with no statewide product** (e.g. `WV` = only `WV_TAB_PANH`, the
   Panhandle) never fabricate a statewide fallback. If the operator's zone has no
   mapped product, the primary card is omitted and the section relies on browse-all;
   a sub-region product is shown as primary only when geometry confirms it contains
   the grid.
9. **Validated against the structural edges, not one state.** The resolution test
   table (geo.test.ts) includes, each asserting the expected primary (or explicit
   "no primary card"):
   - `DM33XK` Phoenix → `AZ_TAB_PHOE` (tabular-only branch)
   - Flagstaff AZ → `AZ_ZON_NOFLA`
   - Seattle (`CN87uo`) → `WA_ZON_SEA` (regression guard for the 8 states)
   - Reno NV area → the `WESNE` shared product (cross-state, regardless of NV/CA bucket)
   - WV interior point → **no** confident whole-state forecast (P1-3 guard)
   - San Juan PR → **no** land-forecast card; radar + browse only (territory guard)
   - Anchorage AK → correct AK office, not a neighbor (antimeridian guard)
   - a just-offshore coastal point → resolves to the containing state, not null
   - Detroit MI → `MI_ZON_SE`, proving Great-Lakes polygon gaps don't null the result
10. Grid-precision handling: the operator's stored grid may be 4-char (`DM33`), whose
    decoded **square center** can sit ~85 km from the true location and straddle a zone
    boundary. Resolution is over the decoded center against the (finer) zone polygons;
    when the center falls in no zone (offshore / simplification gap), the primary card
    is omitted and browse-all carries the section — **never a confidently-wrong neighbor
    product**. The card meta names the grid it resolved from.
11. **Antimeridian safety:** point-in-polygon (`pointInRing`) gains ±180° handling (or
    the bundle normalizes longitudes to 0–360 for AK/Pacific) so Aleutian/Pacific zones
    resolve correctly. Covered by the Anchorage fixture.
12. Reachable in a real build and **render-verified in WebKitGTK at 1920×1080** with a
    coarse-region grid (`DM33`/`DM33XK`) before claimed done (memory
    `reference-webkitgtk-render-harness` / `grim_realapp_validation_pandora`). `src/request`
    suite green; CI `verify` (clippy --all-targets + full vitest) passes.

## Data model

### Geometry — `src/request/nws-zones.geo.json` (extended)

Extend the shipped bundle (216 zones / 8 states, 552 KB) to **all NWS public forecast
zones nationwide** (~4080). Source + technique unchanged: `build-request-geo.ts`
already fetches per-zone geometry from `api.weather.gov/zones/forecast/<ID>` and
Douglas–Peucker-simplifies. No new polygon-union/dissolve dependency is introduced
(that was the CWA model's cost). Measure the resulting bundle; prune precision to keep
it within a sane budget (the 8-state bundle is 552 KB; nationwide many-small-zones will
be larger — simplify harder, drop interior vertices, and verify size before claiming
done). Each feature: `properties: { id, name, state, cwa }` (cwa retained only as a
vetting/grouping aid).

### Mapping — `src/request/nws-zone-to-catalog.json` (extended)

`{ "map": { "<ZONE_ID>": "<CATALOG_FILENAME>" }, "unmapped": { "<ZONE_ID>": "reason" },
"noLandForecast": ["PR","GUAM","SAMOA", …], "crossState": ["CA_ZON_SESWA", …] }`

- Each zone → exactly one product (or `unmapped`).
- The build script proposes a mapping per zone using (a) the zone's `cwa` + (b)
  region-name match against product descriptions; the proposal is **vetted** (most of
  the ~700 land products name a region, not an office, so this is geographic judgment,
  not pure string match — budget for it). Audit trail in `scripts/build-weather-map.md`.
- `crossState` enumerates **all** shared-region products (the 8 named in v1 are <50%
  of them — also the 4-state AL/MS/FL panhandle set, the AR/LA/OK/TX "Four State" set,
  tri-state NYC, Southern New England, CA/OR, FL/GA, etc.); these resolve by geometry
  and are de-duplicated in browse-all.

## Resolution algorithm (pure, `src/request/geo.ts`)

```
resolveWeather(grid):
  ll   = gridToLatLon(grid)                  # existing
  zone = gridToNwsZone(ll.lat, ll.lon)       # existing; now nationwide geometry
  primary = zone ? map[zone.id] : null       # null => omit primary card (no fabrication)
  state   = zone?.state ?? latLonToUsState(ll)
  browseAll = products whose category == WX_US_<state>
              UNION products whose region geometry contains ll   # cross-state
  return { primary, primaryLabel: zone?.name ?? state, browseAll, state }
```

`gridToRadarRegion` and `latLonToSeaArea` unchanged.

## UI (`src/request/sections.ts` + `RequestCenter.tsx`)

- Primary weather card when `primary` resolves: label = zone/region name, meta =
  `<zone/region> · <filename>` + text-vs-tabular note, action `addCms(primary)`.
- **Always** a "Browse all `<ST>` weather · N" card → `openBrowse('WX_US_<ST>')`,
  N from the loaded catalog. Subsumes the prior P3 dynamic-label follow-up.
- Radar + marine unchanged. The section is never *useless* for a US grid (radar +
  browse-all at minimum); it may legitimately omit the primary weather-text card for a
  no-land-forecast territory or an unmapped zone — that is correct, not empty.

## Test plan

- **Resolution fixtures** (`geo.test.ts`): the DoD-#9 structural-edge table.
- **Completeness** (`catalogMap.test.ts`): every bundled zone is in `map` or
  `unmapped`; every mapped filename exists in the catalog; every land `WX_US_<ST>`
  product is reachable from a zone or the state browse-all; `_DIS_`/`_MOT_`/`_HAZ_`
  classified non-primary.
- **Referential integrity**: no dangling filenames; no zone without a state.
- **App-level** (`RequestCenter.app.test.tsx`): a coarse-region grid renders primary
  (or correctly omits it) + browse-all on the production mount path (memory
  `test_production_mount_path_not_just_units`).

## Out of scope

METAR; GRIB/Saildocs (separate card); sub-zone precision finer than the catalog offers.

## Adversarial review

v1→v2 incorporates a same-model self-adrev (operator-authorized in place of Codex for
the alpha timeline; cross-provider Codex quota resets Jun 13 1:49 PM). Dispositions
folded above; raw findings in `dev/adversarial/2026-06-11-cwa-weather-resolver-selfadrev.md`.
A cross-provider Codex round on **this v2** remains advisable on the implementation
when quota returns.
