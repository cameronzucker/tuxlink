#!/usr/bin/env python3
"""Headless WebKitGTK snapshot — renders a URL in the same engine Tauri uses and
writes a PNG. No compositor, no Tauri build.

Usage: snapshot.py <url> <out.png> [width] [height] [wait_ms]
"""
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gtk, WebKit2, GLib  # noqa: E402

url = sys.argv[1]
out = sys.argv[2]
width = int(sys.argv[3]) if len(sys.argv) > 3 else 1366
height = int(sys.argv[4]) if len(sys.argv) > 4 else 800
wait_ms = int(sys.argv[5]) if len(sys.argv) > 5 else 1500

win = Gtk.OffscreenWindow()
win.set_default_size(width, height)
view = WebKit2.WebView()
view.set_size_request(width, height)
# Make the webview background opaque dark so transparent gaps are visible as the app bg.
win.add(view)
win.show_all()

_done = {"v": False}


def _save():
    def _cb(v, res, _data):
        try:
            surface = v.get_snapshot_finish(res)
            surface.write_to_png(out)
            print(f"WROTE {out} ({surface.get_width()}x{surface.get_height()})")
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
