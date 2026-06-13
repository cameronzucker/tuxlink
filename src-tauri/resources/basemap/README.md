# Bundled vector basemap resources (tuxlink-ndi4)

This directory holds the **self-hosted vector OSM basemap** assets the app bundles
so the map renders fully offline with no cross-service tile dependency. The
binary assets are **produced out-of-band** by
[`scripts/build-basemap-bundle.sh`](../../../scripts/build-basemap-bundle.sh)
(it `pmtiles extract`s world z0–6 — ~45 MB — from the remote Protomaps planet over
HTTP Range in seconds; the ~120 GB planet is the remote source, never downloaded
whole, so this just doesn't belong in CI) and are **provenanced**, not mystery
blobs — see `provenance.json` after a build.

Expected contents (absent until the build script has run):

| Path | What | Served via |
|------|------|------------|
| `world-z0-6.pmtiles` | World overview, zoom 0–6 (~30–60 MB) | `tile://pmtiles/world` HTTP-206 Range (Rust `basemap` module) |
| `glyphs/<fontstack>/<range>.pbf` | Noto Sans Regular/Medium/Italic, Latin ranges 0–1023 | `'self'` origin, MapLibre `glyphs:` (NOT the pmtiles byte-range path — glyphs are `{fontstack}/{range}`-keyed) |
| `sprites/light.{json,png,@2x.png}` | POI icon sheet, light flavor | `'self'` origin, MapLibre `sprite:` |
| `sprites/dark.{json,png,@2x.png}` | POI icon sheet, dark flavor (distinct sheet — icons are raster PNG, not slot-color-derivable; plan A7) | `'self'` origin |
| `provenance.json` | Planet build id, pmtiles CLI version, bbox/zoom, basemaps-assets ref, sha256 | — |

## Why these specific assets

- **Fontstacks** `Noto Sans Regular` / `Medium` / `Italic` are exactly what the
  pinned `@protomaps/basemaps@5` **light** flavor references (verified via
  `text-font` on the generated style). Dark reuses the same glyph PBFs — glyph
  color lives in the style, not the glyph. Without bundled glyphs, labels 404 →
  an unlabeled map (plan A8).
- **Latin-only** glyph ranges are the documented EmComm default; CJK glyph sets
  balloon the bundle by hundreds of MB.
- **One pinned planet build** for the bundle AND every downloadable region pack
  (plan A10) — a divergent vector schema between the overview and a pack causes
  the z6 overview↔region compositing seam to blank. The 13-id vector schema
  (`boundaries…water`) is enforced at runtime by `src-tauri/src/basemap/validate.rs`.

## Wiring (after the assets exist)

Add to `src-tauri/tauri.conf.json` `bundle.resources`:

```json
"resources/basemap/**/*"
```

It is intentionally **omitted** until the build script has produced the assets:
`tauri build` errors on a resource path that resolves to nothing. With the assets
present, the app's `.setup()` registers `tile://pmtiles/world` at startup; absent,
that registration is a non-fatal skip (the source renders empty, no crash).

## Region packs (D1 — RESOLVED, operator 2026-06-13)

Downloadable region detail is **extracted on demand from the public Protomaps planet**
via the `go-pmtiles` sidecar over HTTP Range — tuxlink does not host packs. The current
planet build URL lives in `region-manifest.json` (bundled here as the offline default,
refreshed online via a Rust command). Full spec, including the fixed-box coverage model,
preset degree boxes, and schema-consistency-across-rotating-builds reasoning:
[`docs/design/2026-06-13-ndi4-d1-region-pack-distribution.md`](../../../docs/design/2026-06-13-ndi4-d1-region-pack-distribution.md).

- **D2 (phase 3):** baked-dark aesthetic re-approval against the meshmap mock — done
  (operator: "looks just like meshmap now").
