# Vendored: protomaps-leaflet

- **Version:** 5.1.0
- **Source:** https://www.npmjs.com/package/protomaps-leaflet (`npm pack protomaps-leaflet@5`, 2026-06-20)
- **License:** BSD-3-Clause (upstream `LICENSE` preserved verbatim in this
  directory; the upstream npm metadata labels it "BSD-3-Clause"). NOTE: the
  Task 1 plan template guessed "MIT" — the actual upstream license is
  BSD-3-Clause, recorded accurately here.
- **Why vendored:** upstream is in maintenance mode and this stack is on
  tuxlink's offline/EmComm-critical path (ADR-0011 fork-and-own). Pinning the
  artifact in-repo removes the registry dependency at build time and lets us
  patch the PMTiles source wiring if upstream cannot read our `tile://`
  custom-protocol seam.
- **Dist shape:**
  - Entry: `index.js` — the upstream ESM build (`dist/esm/index.js`), **minified**
    (single-line, with an adjacent `index.js.map` source map).
  - Types: `index.d.ts` — upstream's shipped declarations (`dist/esm/index.d.ts`),
    readable/unminified.
  - Source map: `index.js.map` copied alongside for debuggability.
- **Confirmed exported API (from `index.d.ts` + the bundle's `export { … }`):**
  - `leafletLayer(options?: LeafletLayerOptions)` — named export (runtime token
    `Pn as leafletLayer`). This is the seam entry point.
  - `LeafletLayerOptions extends L.GridLayerOptions` with these fields used by
    tuxlink's seam: `url?: PMTiles | string`, `sources?: Record<string, SourceOptions>`,
    `flavor?: string`, `lang?: string`, `attribution?: string`, `bounds?`,
    plus all `L.GridLayerOptions` (`minZoom`, `maxZoom`, `pane`, `zIndex`, …).
  - NOTE: `SourceOptions` is `{ levelDiff?; maxDataZoom?; url?; sources? }` — it
    does **not** carry a top-level `maxzoom`, so the seam uses the plain `{ url }`
    form and relies on the PMTiles archive header's own maxzoom for overzoom
    (matching the MapLibre path), per `basemapLeaflet.ts`.
- **Runtime/peer dependencies pulled in by this bundle** (bare imports inside
  `index.js`, now added as direct deps of tuxlink so the vendored bundle
  resolves under pnpm's strict node_modules): `@mapbox/point-geometry`,
  `@mapbox/vector-tile`, `pbf`, `potpack`, `rbush`, `color2k`,
  `@protomaps/basemaps` (already present), `pmtiles` (already present at `^4`;
  upstream pinned `^3` but the v4 API surface the bundle uses — `new PMTiles(url)`
  + Range fetch — is compatible and is the same version the MapLibre path uses).
  `leaflet` is a peer (`import L from "leaflet"`), added as a direct dep.

## Upstream LICENSE (BSD-3-Clause)

```
Copyright 2021-2024 Protomaps LLC

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
