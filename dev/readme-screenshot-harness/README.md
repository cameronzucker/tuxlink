# README screenshot harness

This dev-only harness regenerates the README images from real frontend
components rendered in WebKitGTK. It uses canned, privacy-safe Tauri IPC
responses so screenshots do not expose an operator's real callsign, mailbox, or
station configuration.

> **Animated demo status (2026-06-17):** the screencast mechanism in
> [§ Animated screencast](#animated-screencast) works end-to-end and is kept for
> future use, but the animated demo is **intentionally not embedded in the
> README** while the product is pre-alpha — the APRS map open/load reads as buggy
> (because it currently is), which undersells the app. Regenerate and re-embed it
> in the README hero block once those surfaces are polished. The static feature
> screenshots stay.

```bash
# Bind IPv4 explicitly: `pnpm dev -- --host 127.0.0.1` mangles the flag and vite
# binds [::1] only, which the snapshot (127.0.0.1) cannot reach.
VITE_TUXLINK_FIXTURE=1 pnpm exec vite --host 127.0.0.1 --port 1420 --strictPort

# Hero: the mailbox shell with the VARA HF radio dock open + a message in the
# reading pane. snapshot.py has a hard 20s safety timeout, and a COLD vite
# compiles the whole AppShell graph on first load — run any one snapshot once to
# warm the module cache, then the real capture lands inside the window.
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell&dock=vara" \
    docs/readme/images/tuxlink-mailbox.png 1920 920 13000

WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=wizard" \
    docs/readme/images/tuxlink-first-run-wizard.png 1180 760 4000

WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=request" \
    docs/readme/images/tuxlink-request-center.png 1366 820 8000
```

Feature images are **cropped to the panel they advertise** so they read as
distinct features at thumbnail size, not near-identical full-window shots. The
6th `snapshot.py` arg is a CSS selector (cropped to that element's bounding box,
+ optional 7th `pad` arg) or `clip:x,y,w,h` for a multi-element region.

```bash
# ARDOP HF / Packet radio docks — cropped to the modem panel
for d in ardop packet; do
  WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    python3 dev/render-harness/snapshot.py \
      "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell&dock=$d" \
      "docs/readme/images/tuxlink-$d.png" 1920 920 13000 "[data-testid=radio-panel-root]" 10
done
# (tuxlink-ardop-hf.png keeps the -hf suffix: cp tuxlink-ardop.png tuxlink-ardop-hf.png)

# Simultaneous HF/VHF workspace — reading pane + APRS chat dock (injects heard traffic)
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell&dock=aprs" \
    docs/readme/images/tuxlink-workspace.png 1920 920 13000 "clip:520,52,1400,868"

# Color schemes — cropped to ribbon + folders + list + reading pane (excludes the dock)
for s in night-red daylight; do
  WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    python3 dev/render-harness/snapshot.py \
      "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell&dock=vara&scheme=$s" \
      "docs/readme/images/tuxlink-color-$s.png" 1920 920 13000 "clip:0,0,1180,920"
done
```

Supported views: `shell`, `wizard`, `request`.

The `shell` view accepts:

- `&dock=vara|ardop|packet` — opens that transport's radio modem dock (selects the
  connection, pins the panel via Ctrl+Shift+M, opens the top message so all panes
  have content).
- `&dock=aprs` — toggles the APRS tactical-chat dock open and injects a few heard
  messages over the `aprs-message:new` event so the channel shows live traffic.
- `&scheme=<id>` — applies a color scheme before render (e.g. `night-red`,
  `daylight`, `paper`, `high-contrast-light`, `grayscale`, `office-dark`,
  `repository-dark`). Composes with `dock`.

For scripted interaction (the animated demo), the harness also exposes
`window.__harness.emit(event, payload)` (inject a Tauri event, e.g. APRS chat
traffic) and `window.__harness.scheme(id)` (flip the color scheme on cue).

## Animated screencast

`screencast.py` drives Chromium (Playwright) through a scripted flow — read
Winlink messages, toggle the APRS dock (with injected chat traffic + heard
positions), open the heard-station map, and move between the two — and records a
WebM. Convert it to a looping animated WebP (what GitHub renders) with ffmpeg:

```bash
# The map renders from the bundled basemap. Glyphs+sprites are already served
# from public/basemap/; copy the 43 MB vector archive there too (NOT committed),
# and the harness redirects its tile:// fetches to it. Remove it after.
cp src-tauri/resources/basemap/world-z0-6.pmtiles public/basemap/

VITE_TUXLINK_FIXTURE=1 pnpm exec vite --host 127.0.0.1 --port 1420 --strictPort  # leave running
python3 dev/readme-screenshot-harness/screencast.py /tmp/cast                     # -> /tmp/cast/<hash>.webm (full-frame, 1280x800)

# Downscale the full-frame capture to the README display width (860) and trim the
# cold front load (~9 s) to a ~15 s window. Crispness comes from `q:v` (0-100,
# HIGHER = crisper) — NOT from a bigger capture. (device_scale_factor=2 does not
# raise the video resolution; it just letterboxes the frame grey — don't.)
ffmpeg -ss 8.8 -t 15 -i /tmp/cast/*.webm \
  -vf "fps=10,scale=860:-1:flags=lanczos" \
  -loop 0 -an -vcodec libwebp -q:v 74 -compression_level 6 -preset picture \
  docs/readme/images/tuxlink-demo.webp

rm public/basemap/world-z0-6.pmtiles
```

Keep it small: GitHub renders animated WebP/GIF/APNG inline (no `<video>` for
committed files; animated SVG is stripped by the camo image proxy). Crispness
comes from a high `q:v` (74); a full-frame ~15 s loop at 860 px / 10 fps lands
~3.4 MB. Drop `q:v` toward ~60 or fps to ~8 if you need it smaller.

## How the pieces fit

- **`harness.tsx`** mounts the real `AppShell` / `Wizard` / `RequestCenter` with a
  `window.__TAURI_INTERNALS__` shim that answers `invoke()` from canned, privacy-safe
  fixtures, so the UI renders without a Rust/Tauri build. Query params (`view`,
  `dock`, `scheme`) select the scene; `window.__harness.emit/.scheme` let a driver
  inject events (APRS traffic) and flip the palette mid-capture.
- **`snapshot.py`** renders a URL in headless WebKitGTK (the same engine Tauri uses)
  to a PNG, optionally cropped to a CSS selector or `clip:` region — the still
  screenshots.
- **`screencast.py`** drives the same scenes in Chromium (Playwright) and records a
  WebM — the animation. (Chromium, because Playwright can script clicks + record;
  WebKitGTK can't be driven the same way.)
- **`ffmpeg`** turns the WebM into a looping animated WebP.

## Gotchas & learnings (hard-won)

- **GitHub README animation = WebP / GIF / APNG via `<img>` only.** A committed
  `.mp4` + `<video>` is flaky (video works when *uploaded* as a GitHub attachment,
  not from the repo); animated SVG is rasterized/stripped by GitHub's camo image
  proxy and shows frozen. WebP is the crisp, small, reliable choice.
- **`device_scale_factor=2` does NOT give a higher-resolution screencast.** Playwright
  records at `record_video_size`; a 2× size just renders the page in the top-left and
  letterboxes the rest grey. Keep `record_video_size == viewport`. Crispness comes
  from the encode (`q:v`, 0–100, higher = crisper), not the capture size.
- **The APRS map needs the PMTiles vector archive.** Glyphs + sprites already serve
  from `public/basemap/`; the basemap data is served via the Rust `tile://` seam,
  which 404s in the harness. Copy `world-z0-6.pmtiles` into `public/basemap/` (range
  requests work via vite) and the harness rewrites `tile://pmtiles/world` to it.
- **The map open/load is currently slow** (~2 s of empty land-color before tiles
  paint) — the main reason the demo isn't README-ready. A future cut should pre-warm
  the map so it's drawn before it slides in.
- **The recorded context reloads cold** (~5–9 s) even after a warm pass — separate
  Playwright contexts don't share cache. Trim that front load in ffmpeg.
- **Verify the whole frame, not just one detail.** A demo that looked crisp shipped
  with a grey letterbox filling half the frame because the check fixated on text
  sharpness. Sample frames across the loop and check the corners.
