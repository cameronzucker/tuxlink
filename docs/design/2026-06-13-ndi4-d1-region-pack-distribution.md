# Design — D1: region-pack distribution (extract-on-demand from the public Protomaps planet)

**Date:** 2026-06-13
**Agent:** gully-kingfisher-bayou
**bd issue:** tuxlink-ndi4
**Status:** LOCKED (operator, 2026-06-13). This doc resolves the three sub-decisions the
locked model left open and is the spec input for Phase 4.
**Parent design (source of truth for the renderer/format/coverage premises):**
[`2026-06-13-self-hosted-vector-osm-basemap-design.md`](2026-06-13-self-hosted-vector-osm-basemap-design.md).
**Plan:** [`docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md`](../superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md)
(D1 was the deferred decision at Phase 4; this doc supplies it).

---

## The locked model (operator, 2026-06-13 — not re-litigated here)

Region detail is obtained by **extract-on-demand from the public Protomaps planet** using
the `go-pmtiles` CLI over HTTP **Range** — tuxlink does not host per-region packs. A pack
download is a one-time online operation per area; the result is a permanent, offline-usable
`.pmtiles` archive on the operator's disk. tuxlink hosts only a small (~KB) **manifest** that
names the current planet build URL, because Protomaps planet builds rotate roughly monthly and
older builds 404 (`20240801` already returns 404; `20260608` is the current live build as of
2026-06-13).

Measured tiers (all z0–14, extracted from `build.protomaps.com/20260608` over Range):

| Tier | Coverage | Transfer | Role |
|---|---|---|---|
| Bundled world | z0–6, whole world | 43 MB | Ships in the `.deb`; offline-first, always present |
| Local | metro-scale box | ~17 MB / 8 s | Smallest on-demand pack |
| Regional | state-scale box | ~203 MB / 20 s | — |
| **Wide (default)** | multi-state box | **~1 GB / 40 s** | The default proactive offer |
| Continent | named continent, z0–14 | 15–35 GB | No zoom cap; most operators take only their own, once |

The bundled world overview is never re-fetched; only on-demand pack extraction touches the
network, and only the planet **build URL** it Range-reads from must be current.

Discovery: a proactive offer at **location-set** (anchored on the operator grid, e.g. "Download
offline detail for your area? DM43/Phoenix ~1 GB") plus a discoverable **Tools → Offline maps**
manager (list / add / delete; presets Local / Regional / Wide + named continent picks). Zoom-into-
undownloaded is a gentle hint only, not a download trigger (the zoom-discovery model was rejected).

---

## Sub-decision 1 — coverage geometry: fixed box centered on the operator grid (not admin-region)

**Decision.** On-demand packs are **fixed longitude/latitude boxes centered on the operator's
grid-square centroid**, not true administrative regions (state/country polygons).

**Rationale.**

- The EmComm mental model is "detail for the area I operate in" — a radius around the station —
  not "the political boundary of my state." A box centered on the operator matches that intent and
  is independent of where the operator sits relative to an administrative border (a station near a
  state line wants both sides, which a state polygon would deny).
- It is mechanically the simplest correct extract: `go-pmtiles extract <planet> <out> --bbox=W,S,E,N
  --maxzoom=14`. No bundled admin-boundary GeoJSONs, no polygon-clip step, deterministic output.
- A box has a predictable, monotone size in degrees², so the manager can show a size estimate
  before download and the per-tier defaults are tunable from the manifest without an app release.

**The preset boxes** are expressed as half-widths around the operator centroid `(lon0, lat0)`:

| Preset | Δlon (half-width) | Δlat (half-width) | Typical transfer | Notes |
|---|---|---|---|---|
| Local | ±1.0° | ±0.75° | ~17 MB | A metropolitan area |
| Regional | ±3.0° | ±2.5° | ~200 MB | A state-sized area |
| **Wide (default)** | ±7.5° | ±6.0° | ~1 GB | Multi-state; the proactive-offer default |

The box is clamped to `[-180, 180] × [-85, 85]` (web-mercator latitude limit). These half-widths are
**starting values carried in the manifest** (`tiers[].half_deg`), so they are adjusted by editing the
manifest, not by shipping a new app build. Transferred size varies with latitude and land density,
so the manager shows the manifest's *typical* size as an estimate and records the **actual** bytes the
sidecar reports on completion.

**Continents** are also fixed boxes for v1 — a per-continent bbox table in the manifest
(`continents[]`). A continental box over-grabs ocean and adjacent land relative to a true continental
polygon; the operator accepted the 15–35 GB envelope and there is no zoom cap, so the over-grab is
within budget. A tighter polygon clip (`go-pmtiles extract --region <geojson>`) and by-name
state/country picks are a **documented deferred enhancement**, not v1 — they require bundling or
fetching admin/continent boundary polygons and a name→geometry index, which buys precision the box
model does not need for the EmComm use case.

## Sub-decision 2 — schema consistency across rotating builds: pin the schema, validate every pack

**Tension.** Plan A10 requires that the bundled z0–6 overview and every region pack share a vector
schema, or the Phase-4 R7 overview↔region compositing seam blanks. But the manifest's planet build
URL rotates monthly so that on-demand extraction keeps working — so a pack extracted next month comes
from a *different* build than the build the bundled overview was cut from.

**Decision.** Pin the **schema**, not the exact build. Every downloaded pack is validated against the
locked Protomaps **planetiler schema v4 / 13-`vector_layers` id set** (`boundaries, buildings, earth,
landcover, landuse, natural, physical_line, physical_point, places, pois, roads, transit, water` —
A10) by the already-built `src-tauri/src/basemap/validate.rs` at download time, before the atomic
install. Protomaps planet builds are schema-stable within a schema version, so any build the manifest
points at composites cleanly with the bundled overview as long as both are schema v4.

> **The bundled z0–6 overview is NOT run through `validate.rs`.** It is a trusted, provenanced,
> checked-in build-time artifact, registered directly via `PmtilesRegistry::register_path` at startup
> ([`lib.rs`](../../src-tauri/src/lib.rs)). This is deliberate: a z0–6 overview legitimately carries
> only **9** of the 13 layers — `natural`, `physical_line`, `physical_point`, and `transit` are
> high-zoom-only detail layers that do not populate at z0–6 (verified on the `20260608` extract). The
> 13-id superset check is the correct gate for a z0–14 **pack** (where all 13 are present) but would
> wrongly reject the overview. MapLibre renders a style layer whose source-layer is absent in a tile as
> simply empty (no error), so the overview renders correctly with the four high-zoom layers missing.
> **Phase-4 trap:** do not extend `validate.rs` over the bundled overview, and do not "fix" the 9-layer
> overview to 13.

**Failure mode is loud, not silent.** If Protomaps ever bumps the planetiler schema (v4→v5),
`validate.rs` **rejects** a newly-extracted pack with a clear error rather than installing a pack that
would blank the seam. That single event — rare, schema bumps are infrequent — is the only one
requiring an app release (re-cut the bundled overview from the new schema, bump the schema fixture).
Routine monthly build rotations need only a manifest edit.

## Sub-decision 3 — manifest hosting: in-repo JSON, bundled as the offline default, refreshed via a Rust command

**Decision.** The region manifest is a small JSON file **committed in the repository** at
`src-tauri/resources/basemap/region-manifest.json`. It is **bundled in the `.deb`** (it rides the
same `bundle.resources` glob as the world overview) so a fresh install has a known-good pinned build
offline on day one. When online, the app **refreshes** it from the canonical raw URL:

```
https://raw.githubusercontent.com/cameronzucker/tuxlink/main/src-tauri/resources/basemap/region-manifest.json
```

**Why this hosting choice.**

- **Zero infrastructure.** No tile server, no object store, no GitHub Pages configuration — the file
  is version-controlled alongside the code, and GitHub already serves `main` over a stable raw URL.
- **Auditable + reproducible.** The manifest's history is the repo's history; the build it pins is a
  reviewable diff, not a mutable server-side blob.
- **Offline-first.** The bundled copy is the fallback. The app works on first launch with no network;
  the refresh only updates the planet build URL and tunable tier boxes.

**The refresh is a Rust command, not a webview fetch.** A `reqwest` GET in a Tauri command fetches the
manifest, validates its shape, and caches it in app-data; the webview reads the cached manifest via
`invoke`. This keeps the webview CSP closed (current `connect-src 'self' http://127.0.0.1:* tile:` is
unchanged — no external origin is added) and lets Rust reject a malformed or oversized manifest before
it reaches the UI. This mirrors the #659 posture of doing network egress in Rust, never the webview.

**Manifest shape (illustrative):**

```json
{
  "schema": "tuxlink-basemap-manifest/1",
  "planet_build": "20260608",
  "planet_url": "https://build.protomaps.com/20260608.pmtiles",
  "pmtiles_schema": { "planetiler_version": 4, "vector_layers": ["boundaries", "buildings", "earth", "landcover", "landuse", "natural", "physical_line", "physical_point", "places", "pois", "roads", "transit", "water"] },
  "tiers": [
    { "id": "local",    "label": "Local",    "half_deg": [1.0, 0.75], "typical_bytes": 17000000 },
    { "id": "regional", "label": "Regional", "half_deg": [3.0, 2.5],  "typical_bytes": 203000000 },
    { "id": "wide",     "label": "Wide",     "half_deg": [7.5, 6.0],  "typical_bytes": 1000000000, "default": true }
  ],
  "continents": [
    { "id": "na", "label": "North America", "bbox": [-170, 5, -50, 84], "typical_bytes": 30000000000 }
  ]
}
```

`planet_build` is what the operator bumps when Protomaps rotates the live build. The `pmtiles_schema`
block is the cross-check the runtime `validate.rs` already enforces (A10); it is recorded here so a
manifest reviewer sees the schema the tiers were sized against.

---

## Mechanism (Phase 4 — reuses the Phase 1–3 plumbing; only three pieces are net-new)

```
 location-set  ──►  proactive offer (Wide default, anchored on operator grid)
 Tools→Offline maps manager ──►  preset pick (Local/Regional/Wide) or named continent
        │
        ▼  pre-flight free-space check (reject if insufficient)
 go-pmtiles SIDECAR (bundle.externalBin, tauri-plugin-shell — already a dep)
   pmtiles extract <manifest.planet_url> <tmp.pmtiles> --bbox=<box from operator grid> --maxzoom=14
        │  (Range-reads ONLY the requested tiles from the public planet; tens of MB–GB, not 120 GB)
        ▼
 Rust basemap::validate (ALREADY BUILT): PMTiles v3 magic + version 0x03 + 13-id schema + size budget
        │  reject → clear error, temp file deleted; no half-pack registered
        ▼  atomic install (R5 pattern: NamedTempFile → sync_all → persist → parent-dir fsync)
 PmtilesRegistry.register_path(<pack-id>, <installed path>)   (ALREADY BUILT)
        │
        ▼  served via the SAME tile://pmtiles/<pack-id> 206 seam (ALREADY BUILT)
 R7 dual-source compositing: overview source layers maxzoom 6 + region source layers minzoom 6
   (per-source zoom-range clamping, A11 — disjoint bands; never blank; full detail where downloaded)
```

**Reused, already on `main` (do not rebuild):** `basemap::validate` (PMTiles v3 + 13-id schema +
size budget), `PmtilesRegistry` / `PmtilesArchive` (`register_path`, lock-free `read_at`), the
`tile://pmtiles/<archive>` 206 handler in `lib.rs`, and the MapLibre style/source seam.

**Net-new in Phase 4:**

1. **go-pmtiles sidecar** — vendored as `bundle.externalBin` (`pmtiles-<target-triple>`, e.g.
   `pmtiles-aarch64-unknown-linux-gnu`), invoked through `tauri-plugin-shell`. The binary
   (`go-pmtiles` v1.30.3 arm64) is in `dev/scratch/ndi4-spikes/darkpreview/pmtiles`. The sidecar
   reaches `build.protomaps.com` itself (not the webview), so it is outside the CSP; its bounded
   work is `extract` of a fixed bbox at a fixed maxzoom.
2. **Region manifest** — fetch (Rust/reqwest) + cache + the bundled default (sub-decision 3).
3. **Pack-manager UI** — Tools → Offline maps: list installed packs (id, label, bbox, bytes,
   installed_at) with total disk used; add (preset/continent pick or the proactive offer); delete
   (explicit — packs are permanent resources, never auto-evicted); plus the location-set proactive
   offer entry point. Inline per [[feedback_inline_ui_no_window_clutter]] (no pop-up window).

**Pack data model (app-data `packs/manifest.json`, one entry per installed pack):**
`{ id, label, bbox, minzoom, maxzoom, schema, bytes, source_build, installed_at }`. Written **after**
the atomic rename + dir-fsync; a startup orphan-sweep deletes any `*.tmp`/partial left by an
interrupted download (mirrors `forms::import::sweep_stale_staging`).

## RADIO-1

None. Map rendering and HTTPS file download only; no transmission path is touched.

## Definition of done (Phase 4 — traced by the wire-walk gate before any "shipped" claim)

1. Tools → Offline maps lists installed packs with size + coverage and a running disk total.
2. A location-set proactive offer (Wide, anchored on the operator grid) downloads a pack end-to-end
   through the sidecar; pre-flight space check, temp+atomic install, and post-rename manifest write
   prevent any corrupt/half-registered pack.
3. A downloaded pack renders full z0–14 detail in its box, compositing with the bundled overview at
   the z6 boundary with no seam/blank (R7 / A11).
4. A schema-invalid or truncated download is rejected with a clear error; no pack is installed.
5. The operator can delete a pack, freeing disk; the manifest and registry both drop it.
6. The region manifest refreshes from the canonical raw URL via a Rust command (CSP unchanged) and
   falls back to the bundled default offline.

## Open items carried into the build (non-gating, settle during Phase 4 / Codex adrev)

- Exact per-continent bbox table values (sized once against measured continent extracts).
- Size-estimate display copy in the manager (manifest `typical_bytes` vs the sidecar's reported
  actual).
- Whether the proactive offer fires once per location-set or is rate-limited (UX, not architecture).
