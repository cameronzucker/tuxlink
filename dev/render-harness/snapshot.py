#!/usr/bin/env python3
"""Headless WebKitGTK snapshot — renders a URL in the same engine Tauri uses and
writes a PNG. No compositor, no Tauri build.

Usage: snapshot.py <url> <out.png> [width] [height] [wait_ms] [selector] [pad]

When [selector] (a CSS selector) is given, the PNG is cropped to that element's
bounding box (plus optional [pad] px, default 0) so a feature shot frames just
the panel it advertises instead of the whole 1920px window. Omit it for a
full-document capture (the establishing/hero shots).
"""
import json
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
import cairo  # noqa: E402
from gi.repository import Gtk, WebKit2, GLib  # noqa: E402

url = sys.argv[1]
out = sys.argv[2]
width = int(sys.argv[3]) if len(sys.argv) > 3 else 1366
height = int(sys.argv[4]) if len(sys.argv) > 4 else 800
wait_ms = int(sys.argv[5]) if len(sys.argv) > 5 else 1500
selector = sys.argv[6] if len(sys.argv) > 6 else None
pad = int(sys.argv[7]) if len(sys.argv) > 7 else 0

# [selector] may instead be an explicit pixel region "clip:x,y,w,h" — used to
# frame a multi-element region (e.g. reading pane + dock) that no single
# element wraps.
clip_rect = None
if selector and selector.startswith("clip:"):
    _p = [int(float(n)) for n in selector[len("clip:"):].split(",")]
    clip_rect = {"x": _p[0], "y": _p[1], "w": _p[2], "h": _p[3]}
    selector = None

win = Gtk.OffscreenWindow()
win.set_default_size(width, height)
view = WebKit2.WebView()
view.set_size_request(width, height)
# Make the webview background opaque dark so transparent gaps are visible as the app bg.
win.add(view)
win.show_all()

_done = {"v": False}


def _crop(surface, rect):
    """Crop a cairo ImageSurface to rect={x,y,w,h} (clamped to the surface)."""
    sw, sh = surface.get_width(), surface.get_height()
    x = max(0, int(rect["x"]) - pad)
    y = max(0, int(rect["y"]) - pad)
    w = min(sw - x, int(rect["w"]) + 2 * pad)
    h = min(sh - y, int(rect["h"]) + 2 * pad)
    if w <= 0 or h <= 0:
        return surface
    cropped = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
    ctx = cairo.Context(cropped)
    ctx.set_source_surface(surface, -x, -y)
    ctx.paint()
    return cropped


def _write(surface):
    surface.write_to_png(out)
    print(f"WROTE {out} ({surface.get_width()}x{surface.get_height()})")
    _done["v"] = True
    Gtk.main_quit()


def _save():
    def _cb(v, res, _data):
        try:
            surface = v.get_snapshot_finish(res)
            if clip_rect:
                _write(_crop(surface, clip_rect))
                return
            if not selector:
                _write(surface)
                return
            # Crop to the selector's bounding box (viewport == document for the
            # fixed-height shell, DPR 1, so client rect maps to snapshot px).
            js = (
                "(function(){var e=document.querySelector(%s);"
                "if(!e)return 'null';var r=e.getBoundingClientRect();"
                "return JSON.stringify({x:r.x,y:r.y,w:r.width,h:r.height});})()"
                % json.dumps(selector)
            )

            def _js_cb(view2, res2, _d):
                try:
                    val = view2.run_javascript_finish(res2).get_js_value().to_string()
                    rect = json.loads(val) if val and val != "null" else None
                    _write(_crop(surface, rect) if rect else surface)
                    if rect is None:
                        print(f"SELECTOR NOT FOUND: {selector} (wrote full)", file=sys.stderr)
                except Exception as e:  # noqa: BLE001
                    print(f"CROP ERROR: {e}", file=sys.stderr)
                    _write(surface)

            v.run_javascript(js, None, _js_cb, None)
        except Exception as e:  # noqa: BLE001
            print(f"SNAPSHOT ERROR: {e}", file=sys.stderr)
            _done["v"] = True
            Gtk.main_quit()

    view.get_snapshot(
        WebKit2.SnapshotRegion.FULL_DOCUMENT,
        WebKit2.SnapshotOptions.NONE,
        None,
        _cb,
        None,
    )
    return False


def _on_load(v, event):
    if event == WebKit2.LoadEvent.FINISHED:
        # Let React mount + the (async) shimmed invokes resolve + re-render.
        GLib.timeout_add(wait_ms, _save)


view.connect("load-changed", _on_load)
view.load_uri(url)

# Safety timeout so we never hang.
GLib.timeout_add(20000, lambda: (Gtk.main_quit(), False)[1])
Gtk.main()
sys.exit(0 if _done["v"] else 2)
