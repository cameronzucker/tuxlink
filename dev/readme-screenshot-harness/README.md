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

Supported views: `shell`, `wizard`, `request`. The `shell` view accepts
`&dock=vara` to open the VARA HF radio modem dock (selects the connection, pins
the panel via Ctrl+Shift+M, and opens the top message so all panes have content).
