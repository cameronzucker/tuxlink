# WebKitGTK render harness — no-compile visual smoke

A headless render of front-end components in the **exact WebKitGTK engine Tauri
uses**, with **no Rust build and no menu-driving**. Built after PR #559 shipped a
Request Center re-skin that passed every automated gate but was visually broken
in the real WebKitGTK webview (icons mis-centered, an invisible close control,
a large content↔basket dead band, and a 6-char-grid geo collapse) — none of
which Chromium/jsdom can surface (see memory `chromium-not-webkitgtk-proxy`).

## Why it exists

The grim WebKitGTK smoke is the check that catches fit/render defects, but a full
`tauri dev` build is a ~20-minute Rust compile this device can't spare per
feature. This harness renders the front end alone: Vite serves the components, a
Tauri-IPC shim feeds canned data, and `libwebkit2gtk-4.1` (via Python GObject
introspection) snapshots the result to PNG. Seconds, not minutes.

## Usage

```bash
# 1. Run the dev server from the worktree under test
pnpm dev    # serves :1420

# 2. Snapshot a harness route in real WebKitGTK (software GL; offscreen)
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://localhost:1420/dev/render-harness/harness.html?grid=CN87uo&view=home" \
    /tmp/out.png 1366 800 2500
# then open /tmp/out.png
```

`harness.tsx` mounts `<RequestCenter>` with a shimmed `window.__TAURI_INTERNALS__`
(canned `config_read` / `catalog_list`). Query params: `grid` (e.g. `CN87`,
`CN87uo`, empty), `view` (`home|browse|grib`). `snapshot.py` args: `url out.png
[width] [height] [wait_ms]`.

## Scope / caveats

- Dev-only. Not shipped, not a CI gate. It renders the front end with **mocked**
  Tauri data — it does not exercise the Rust backend. For backend-dependent
  behavior, the real app is still required.
- It is a strong *visual* check (layout, fit, icon/render, token application) in
  the production webview engine — the gap that the automated suite cannot cover.
- PNGs are git-ignored (`*.png`); commit only the harness scripts.
