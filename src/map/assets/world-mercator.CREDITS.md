# Bundled world-map asset — provenance

`world-mercator-2048.png` is the offline base layer for the map subsystem
when rendering under `L.CRS.EPSG3857` (Web Mercator). It is a square
2048 × 2048 raster covering the full WebMercatorQuad extent
(`[-20037508.34, 20037508.34] × [-20037508.34, 20037508.34]` in EPSG:3857 metres),
clipped to ±85.0511° latitude (the Web Mercator limit), at 1:1 tile resolution
for zoom level 3. It aligns to Leaflet's default `L.CRS.EPSG3857` `ImageOverlay`
bounds without offset.

## Source

- **Product:** Natural Earth II with Shaded Relief and Water (`NE2_50M_SR_W`), 1:50m raster.
- **Publisher:** Natural Earth (https://www.naturalearthdata.com/downloads/50m-raster-data/50m-natural-earth-2/).
- **License:** Public domain. Natural Earth places all versions in the public domain; attribution is optional and is included here as good practice.
- **Download URL:** https://naciscdn.org/naturalearth/50m/raster/NE2_50M_SR_W.zip
- **Source raster:** `NE2_50M_SR_W.tif`, RGB, `10800 × 5400` px (equirectangular EPSG:4326).

## Transform

### Step 1 — Reproject to EPSG:3857

The source equirectangular raster is reprojected to Web Mercator (EPSG:3857),
cropped to the exact WebMercatorQuad extent, and resampled to 2048 × 2048 px
via Lanczos:

```bash
gdalwarp -t_srs EPSG:3857 \
  -te -20037508.342789244 -20037508.342789244 20037508.342789244 20037508.342789244 \
  -ts 2048 2048 \
  -r lanczos \
  -overwrite \
  NE2_50M_SR_W.tif \
  world-mercator.tif
```

### Step 2 — Encode to PNG

The reprojected GeoTIFF is converted to a 60-color palette PNG using Pillow.
60 colors is required to stay within the 1.5 MB bundle ceiling; the Mercator
projection's 2048 × 2048 extent (4× the pixel count of the 2048 × 1024
equirectangular base) means the natural 256-color palette produces a ~2.3 MB
PNG. The 60-color MEDIANCUT palette retains adequate shaded-relief fidelity for
an offline fallback layer.

```python
from PIL import Image
im = Image.open("world-mercator.tif").convert("RGB").resize((2048, 2048), Image.LANCZOS)
q = im.quantize(colors=60, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
q.save("world-mercator-2048.png", format="PNG", optimize=True)
```

`oxipng` was not available on the build host; the Pillow `optimize=True` flag
enables zlib best-compression, which is the only lossless pass applied.

## Output

- **File:** `world-mercator-2048.png`
- **Dimensions:** `2048 × 2048` px (palette/`P` mode, 60 colors)
- **Projection:** EPSG:3857 (Web Mercator), full WebMercatorQuad extent
- **Latitude coverage:** ±85.0511° (Web Mercator limit)
- **Zoom parity:** 1:1 at zoom level 3 under `L.CRS.EPSG3857`
- **Size:** 1,536,933 bytes (1,500.9 KiB) — under the 1.5 MB bundle ceiling
- **Date produced:** 2026-06-11

The file exceeds Vite's default `assetsInlineLimit` (4096 bytes), so the
`import` in `BaseMap.tsx` resolves to a hashed file URL served from `'self'`,
matching the `img-src 'self'` content-security policy. It must not be shrunk
below 4 KiB.

Map data: Natural Earth (public domain, naturalearthdata.com).
