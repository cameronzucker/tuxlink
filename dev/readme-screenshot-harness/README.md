# README screenshot harness

This dev-only harness regenerates the README images from real frontend
components rendered in WebKitGTK. It uses canned, privacy-safe Tauri IPC
responses so screenshots do not expose an operator's real callsign, mailbox, or
station configuration.

```bash
VITE_TUXLINK_FIXTURE=1 pnpm dev -- --host 127.0.0.1

WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell" \
    docs/readme/images/tuxlink-mailbox.png 1920 1080 15000

WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=wizard" \
    docs/readme/images/tuxlink-first-run-wizard.png 1180 760 4000

WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=request" \
    docs/readme/images/tuxlink-request-center.png 1366 820 8000
```

Supported views are `shell`, `wizard`, and `request`.
