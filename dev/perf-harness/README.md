# Map perf harness — on-Pi frame-timing smoke

A real frame-timing measurement of the production map under software GL on the
Pi, during a scripted pan/zoom, with a region pack + station pins + the Maidenhead
grid mounted at real window resolution. It is the gate the front-end
render-harness (`dev/render-harness/`) could never be.

## Why it exists

The map perf RCA (tuxlink-vnk7) found the ~45 fps "forecast" had been measured on
the front-end-only render-harness: canned Tauri data, a trivial scene, no real
tile decode, no markers, no pack compositing. That number never predicted real
app perf. MapLibre GL JS runs through software GL (llvmpipe) on the Pi, and the
production scene — vector tiles decoded and rasterized on the CPU, ~50 pin circle
features, a recomputing Maidenhead lattice, and a composited region pack — is a
different fill-rate problem entirely.

This harness mounts the **real** `MapLibreMap` with a production-representative
scene and measures real `requestAnimationFrame` frame timing while a deterministic
pan/zoom script drives the camera. The page samples the rAF deltas (the Python
driver only loads the route and reads the result back out of the DOM, because
full WebKitGTK frame sampling via GObject introspection is impractical).

## What it mounts

- `MapLibreMap` with the bundled world overview.
- A region-pack source — a real installed pack if the backend reports one,
  otherwise a shimmed pack id (`perf-region-pack`) so the pack-compositing code
  path runs. Override with `?pack=<id>` or `?pack=` (empty → overview only).
- ~50 station pins as the exact circle layer `StationFinderMap` ships
  (data-driven radius/colour/stroke per reachability tier).
- `MaidenheadGridLayer` (recomputing lattice on pan/zoom).

## Run command

Start the dev server from the worktree under test, then run the driver with the
software-GL env vars (the Pi render profile):

```bash
# 1. Dev server (serves :1420)
pnpm dev

# 2. Measure (software GL; visible window so the rAF cadence matches production)
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/perf-harness/perf.py \
    "http://localhost:1420/dev/perf-harness/harness.html" \
    1366 768
```

The driver prints p50/p95 frame time (ms), approximate fps, and the frame count.

Tunable URL query params: `runMs` (scripted-motion window, default 12000),
`warmupMs` (discarded settle window, default 2500), `pack` (region-pack id).
`perf.py` args: `url [width] [height] [timeout_ms]`.

## What it measures

The p50 and p95 `requestAnimationFrame` inter-frame interval over the scripted
pan/zoom run, after a warmup window that discards the initial style/tile settle.
p50 is the typical frame; p95 is the tail that determines whether interaction
feels janky. Approximate fps is `1000 / p50_ms`.

## Pass threshold

**Starting bar: p95 frame time ≤ ~33 ms (~30 fps).** This is a starting
threshold to calibrate against measured numbers on this hardware, not a
final-tuned target. 30 fps sustained at the tail is the floor for pan/zoom that
reads as smooth; the RCA's headline question is whether the real scene clears it
under software GL, which the mocked render-harness's ~45 fps could not answer.
Record measured p50/p95 in the RCA / handoff and tighten or relax the bar from
real data.

## Scope / caveats

- Dev-only. Not shipped. **NOT a CI gate** — CI has no Pi compute and no software
  GL profile; this is operator-run on the target hardware.
- Pure rendering. No radio, no transmit path, nothing to transmit.
- The bundled world archive is an out-of-band build artifact. If it is absent in
  the worktree, `tile://pmtiles/world` 404s and the map renders empty; the
  harness still produces a frame-timing number, but a representative measurement
  needs the bundle present (and, for the pack path, a real installed pack rather
  than the shim). A measurement run on an empty map under-states real cost.
- Output artifacts (`*.png`, `*.json.out`) are git-ignored; commit only the
  harness scripts.
