# README screenshot harness

This dev-only harness regenerates the README images from real frontend
components rendered in WebKitGTK. It uses canned, privacy-safe Tauri IPC
responses so screenshots do not expose an operator's real callsign, mailbox, or
station configuration.

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

`screencast.py` drives Chromium (Playwright) through a scripted flow — open a
message, toggle the APRS dock with injected traffic, cycle color schemes — and
records a WebM. Convert it to a looping animated WebP (what GitHub renders) with
ffmpeg:

```bash
VITE_TUXLINK_FIXTURE=1 pnpm exec vite --host 127.0.0.1 --port 1420 --strictPort  # leave running
python3 dev/readme-screenshot-harness/screencast.py /tmp/cast                     # -> /tmp/cast/<hash>.webm

ffmpeg -ss 0.4 -i /tmp/cast/*.webm \
  -vf "fps=10,scale=760:-1:flags=lanczos" \
  -loop 0 -an -vcodec libwebp -lossless 0 -q:v 35 -compression_level 6 -preset picture \
  docs/readme/images/tuxlink-demo.webp
```

Keep it small: GitHub renders animated WebP/GIF/APNG inline (no `<video>` for
committed files; animated SVG is stripped by the camo image proxy). 10 fps /
760 px / q35 lands ~2 MB for a ~13 s loop.
