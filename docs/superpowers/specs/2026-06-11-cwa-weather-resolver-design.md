# Request Center weather resolver — resolve by NWS forecast office (CWA), all states (design)

**bd:** tuxlink-z1b7 · supersedes the resolution model of the location-hero spec
(`2026-06-10-request-center-location-hero-design.md`, tuxlink-96lu) · 2026-06-11 ·
agent yew-wren-fir

## Why

The location-hero shipped (tuxlink-96lu, PR #587 + predecessors) resolves the
operator's weather products from their Maidenhead grid. It works in Seattle and
fails in Phoenix. An operator at grid `DM33` (Phoenix) is offered the **Northern
Arizona** forecast (`AZ_ZON_NOFLA`, the Flagstaff office product), which does not
cover Phoenix, and **no alternatives**.

### Root cause

The Winlink catalog (`src-tauri/resources/catalog/winlink-queries.txt`) organizes
weather by **NWS forecast office** (County Warning Area / CWA), not by NWS public
zone and not by state. The product names carry the office:
`AZ_TAB_PHOE` "…Arizona Phoenix NWS", `AZ_ZON_NOFLA` "…Northern Arizona from
Flagstaff NWS", `TX_ZON_FOWOR` "…Fort Worth TX NWS", `FL_ZON_MEL` "…Melbourne NWS".

The catalog's granularity is wildly uneven across states:

| Shape | Example states | Products / state |
|---|---|---|
| Per-NWS-public-zone | WA (72), OR (61), ME (35), NJ (34) | dozens |
| Per-office (coarse) | AZ (7), TX (23), CA (19), FL (15), CO (5) | a handful |
| Statewide only | WV (1), and the 1–2-product tail | 1–2 |

Total: **56 states/territories, 523 weather products** (355 `_ZON_` zone
forecasts, 120 `_TAB_` tabular, plus a small tail).

The location-hero design keyed resolution on the operator's **exact NWS public
forecast zone** and bundled NWS public-zone geometry **only for the 8 states whose
catalog happens to be per-zone** (WA/OR/ME/NJ/VT/NH/DE/AK). For the other ~48
states the catalog has no per-public-zone product, so those states were placed in
the "unmapped-by-design" list and resolve to `null`. Arizona is one of them.

The spec and the mock (`docs/design/mockups/2026-06-10-request-center-location-hero.html`)
were both validated **exclusively on Seattle/Washington** — the one state shape the
design fits. That blind spot shipped across three iterations because nothing in the
process exercised a coarse-region state. **The implementation conforms to the spec;
the spec's data model is wrong for most of the country.**

### Decision (operator, 2026-06-11)

"**Pick region + show all.**" For every state, resolve the operator's actual catalog
region and auto-pick its product (Phoenix → `AZ_TAB_PHOE`), **and always** surface
"Browse all `<ST>` weather · N" so the operator is never stuck with one report.

The principled geographic key is the **NWS County Warning Area (CWA) polygon**
(~122 nationwide) because that is exactly how the catalog is subdivided. This
replaces the per-NWS-public-zone model uniformly; the 8 already-working states are a
special case of the same model (their CWAs simply have many sub-zone products).

## Definition of done

The "For your location" weather resolution is complete when:

1. For **any** operator grid inside a US state/territory present in the catalog, the
   section auto-resolves the **most-specific weather product whose geography
   contains the grid** as the primary card. "Most-specific" = office-level (CWA)
   product before statewide rollup. Phoenix (`DM33`) resolves to the Phoenix-office
   product, not Flagstaff.
2. When a resolved region has both a zone (`_ZON_`) and a tabular (`_TAB_`) variant,
   the **zone forecast is primary** and the tabular is offered as an alternative.
   When only one variant exists (e.g. Phoenix has only `AZ_TAB_PHOE`), that one is
   primary.
3. The section **always** offers "Browse all `<ST>` weather · N products" (N = the
   live count for the operator's state), navigating Browse pre-filtered to that
   state's `WX_US_<ST>` category. This affordance is present even when the primary
   card resolves, and is the sole location-weather affordance when no region polygon
   contains the grid (statewide fallback still lists the state's products).
4. Radar (tightest-region) and marine (coastal-only) resolution are unchanged from
   the shipped behavior.
5. Each card states what the CMS returns (text vs image), the resolved target
   (office/region name · filename), and the catalog filename.
6. The product→CWA mapping is **vetted for every catalog state** — every
   `WX_US_<ST>` product filename is either mapped to a real CWA geometry or recorded
   in an explicit `unmapped-by-design` list with a reason. No fuzzy auto-match ships
   unreviewed (carried from the prior spec's DoD #5; the failure this fixes is the
   *coverage* of that vetting, not its rigor).
7. Cross-state / multi-office products (the 8 known: `CA_ZON_SESWA`, `CA_ZON_NORT`,
   `CA_ZON_WESNE`, `NV_ZON_WESNE`, `LA_TAB_SW`, `LA_ZON_SW`, `TX_TAB_SE`,
   `TX_ZON_SE`) resolve correctly by geometry — a point in the shared region picks
   the shared product regardless of which `WX_US_<ST>` bucket it files under.
8. **Validated against multiple state shapes, not one.** The resolution test suite
   includes fixtures for at least: AZ Phoenix (`DM33`), AZ Flagstaff, a TX
   multi-office point, a CA cross-state point, a FL point, and the existing WA
   per-zone point — each asserting the expected primary filename. This is the
   guard that the WA-only blind spot cannot recur.
9. The section is reachable in a real build and **render-verified in WebKitGTK at
   1920×1080** with a coarse-region grid (`DM33`) before being claimed done (memory
   `reference-webkitgtk-render-harness` / `grim_realapp_validation_pandora`). The
   `src/request` test suite stays green; CI `verify` (clippy --all-targets + full
   vitest) passes.

## Data model

### Geographic key — CWA polygons

Bundle simplified **NWS County Warning Area** polygons (one per forecast office,
~122). Source: `api.weather.gov` (the build script already fetches NWS geometry;
extend it to the CWA/forecast-office boundaries). Simplify for display size,
matching the existing `nws-zones.geo.json` budget discipline (the shipped zone
bundle is 552 KB for 216 zones; ~122 whole-CWA polygons simplified should land in a
comparable budget — verify and prune).

`src/request/cwa.geo.json` — `FeatureCollection`, each feature:
`properties: { cwa: "PSR", name: "Phoenix", state: "AZ" }`, MultiPolygon geometry.

### Product → CWA mapping

`src/request/weather-product-map.json` — vetted, of shape:

```json
{
  "byCwa": {
    "PSR": { "state": "AZ", "primary": "AZ_TAB_PHOE", "tabular": "AZ_TAB_PHOE", "zone": null },
    "FGZ": { "state": "AZ", "primary": "AZ_ZON_NOFLA", "zone": "AZ_ZON_NOFLA", "tabular": "AZ_TAB_NORT" }
  },
  "statewide": { "WV": "WV_ZON_…" },
  "unmapped": { "AZ_…": "reason" }
}
```

- `primary` = `zone ?? tabular` (zone forecast preferred per DoD #2).
- `statewide` = per-state fallback product used when no CWA polygon contains the
  point (coastal water gaps, simplification holes, territories without CWA geometry).
- `unmapped` = products deliberately not surfaced as a location card (e.g. pure
  discussion/`_DIS_`, motorist/`_MOT_`, hazard rollups) with a one-line reason.

Vetting: the office name in every product description is matched to a CWA, by hand
review, recorded in `scripts/build-weather-map.md` (the audit trail, like the
existing `scripts/build-request-geo.md`). Cross-state products are mapped to the CWA
that issues them (e.g. `CA_ZON_SESWA` → Phoenix-region SW-AZ office or the issuing
office per the description) so a point in the shared area resolves there.

### Why not keep per-public-zone

The 8 per-zone states stay correct under the CWA model: their CWA contains the
operator and the *primary* product for that CWA is the office zone-forecast rollup;
the dozens of sub-zone products become the "Browse all `<ST>` · N" long tail (which
the mock already shows for WA: "Browse all WA local forecasts · 68 zones"). We do
not need two parallel resolvers. The shipped `nws-zones.geo.json` may be retained
only if the per-sub-zone primary is judged better UX for those 8 states during
adrev; default is to unify on CWA and treat sub-zones as the browse tail.

## Resolution algorithm (pure, `src/request/geo.ts`)

```
resolveWeather(grid):
  ll = gridToLatLon(grid)               # existing
  cwa = latLonToCwa(ll)                 # NEW: point-in-polygon over cwa.geo.json
  state = latLonToUsState(ll)           # existing, reliable
  primary = cwa ? byCwa[cwa].primary
                : statewide[state]      # fallback
  alternatives = all WX_US_<state> products  # for "Browse all <ST> · N"
  return { primary, primaryLabel: cwa?.name ?? state, alternatives, state }
```

`gridToRadarRegion` and `latLonToSeaArea` are unchanged.

## UI (`src/request/sections.ts` + `RequestCenter.tsx`)

- Primary weather card: label = office/region name (e.g. "Phoenix"), meta =
  `<CWA or region> · <filename>`, action `addCms(primary)`.
- If both zone + tabular exist for the CWA: a secondary card or inline toggle for
  the tabular variant.
- **Always** a "Browse all `<ST>` weather · N" card → `openBrowse('WX_US_<ST>')`
  with N computed from the loaded catalog. This subsumes the prior P3 follow-up
  (dynamic browse-all label).
- Radar + marine cards unchanged.
- The location section is now **never empty for a US grid** — at minimum it carries
  the statewide-fallback primary + browse-all + radar.

## Test plan

- **Resolution fixtures** (`geo.test.ts`): the DoD-#8 multi-shape table — each row
  `(grid|latlon) → expected primary filename`. Phoenix→`AZ_TAB_PHOE`,
  Flagstaff→`AZ_ZON_NOFLA`, a TX point→its office product, a CA cross-state
  point→`CA_ZON_SESWA`, a FL point, Seattle→its WA primary.
- **Completeness** (`catalogMap.test.ts`): every CWA in `cwa.geo.json` has a
  `byCwa` entry; every `byCwa.primary`/`zone`/`tabular` filename exists in the
  bundled catalog; every catalog `WX_US_<ST>` product is in exactly one of
  `byCwa` / `statewide` / `unmapped`.
- **Referential integrity**: no dangling filenames; no CWA without a state.
- **App-level** (`RequestCenter.app.test.tsx`): a coarse-region grid renders a
  primary weather card + browse-all (production mount path, per memory
  `test_production_mount_path_not_just_units`).

## Out of scope

- METAR (excluded, carried from prior spec).
- GRIB / Saildocs (separate card).
- Sub-CWA precision beyond what the catalog offers (the catalog has no
  finer-than-office product for coarse states; do not invent one).

## Adversarial review

Per project policy for hard-to-undo design (`feedback_discipline_triage_rule`,
`feedback_no_carveout_on_cross_provider_adrev`): at least one cross-provider Codex
round on this spec **before** build, specifically attacking (a) the CWA→product
mapping for non-WA state shapes, (b) cross-state product handling, (c)
statewide-fallback gaps, (d) the zone-vs-tabular primary choice. This is the round
that was missing when the WA-only model shipped.
