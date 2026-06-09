# Bundled world-map asset — provenance

`world-equirect-2048.png` is the offline base layer for the map subsystem
(`src/map/BaseMap.tsx`). It is a full-globe equirectangular (plate carrée)
raster covering exactly `[-180, 180] × [-90, 90]` with no crop, so it aligns to
Leaflet's `L.CRS.EPSG4326` `ImageOverlay` bounds without offset.

## Source

- **Product:** Natural Earth II with Shaded Relief and Water (`NE2_50M_SR_W`), 1:50m raster.
- **Publisher:** Natural Earth (https://www.naturalearthdata.com/downloads/50m-raster-data/50m-natural-earth-2/).
- **License:** Public domain. Natural Earth places all versions in the public domain; attribution is optional and is included here as good practice.
- **Download URL:** https://naciscdn.org/naturalearth/50m/raster/NE2_50M_SR_W.zip
- **Source archive sha256:** `7e0e07089b699a3cccad98dd1b2446390d8e3f8c5006359d477a329cebcafaa9`
- **Source raster:** `NE2_50M_SR_W.tif`, RGB, `10800 × 5400` px (exact 2:1 plate carrée).

## Transform

The source raster is downscaled to a fixed `2048 × 1024` target (exact 2:1
plate-carrée aspect), quantized to a 256-color palette to fit the bundle-size
budget, and losslessly recompressed:

```python
from PIL import Image
im = Image.open("NE2_50M_SR_W.tif").convert("RGB").resize((2048, 1024), Image.LANCZOS)
q = im.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
q.save("world-equirect-2048.png", format="PNG", optimize=True)
```

```python
import oxipng
oxipng.optimize("world-equirect-2048.png", level=6, strip=oxipng.StripChunks.safe())
```

The 2048 px width is the precision floor — do not downscale further. A
shaded-relief raster does not compress below ~1.2 MB at this resolution; the
256-color palette is the size/fidelity balance that stays under the 1.5 MB
bundle ceiling without dropping resolution.

## Output

- **File:** `world-equirect-2048.png`
- **Dimensions:** `2048 × 1024` px (palette/`P` mode, 256 colors)
- **Size:** 1,288,645 bytes (1258.4 KiB) — under the 1.5 MB bundle ceiling.
- **sha256:** `085fd2f71f44f60f9b4ecd96016f6a7242c4be6e1c26642c2b1b9e737b76272e`

The file exceeds Vite's default `assetsInlineLimit` (4096 bytes), so the
`import` in `BaseMap.tsx` resolves to a hashed file URL served from `'self'`,
matching the `img-src 'self'` content-security policy. It must not be shrunk
below 4 KiB.

Map data: Natural Earth (public domain, naturalearthdata.com).
