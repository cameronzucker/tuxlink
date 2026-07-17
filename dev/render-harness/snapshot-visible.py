#!/usr/bin/env python3
"""Headless WebKitGTK snapshot of the VISIBLE viewport (not the full document).

Usage: snapshot-visible.py <url> <out.png> [width] [height] [wait_ms]

Why a VISIBLE-region sibling to snapshot.py (tuxlink-dmwte task 11): the popped
dockable-surface shell (`.pop-surface-host`) is a `height: 100vh` flex column
whose body is `overflow: auto` and whose bottom status strip is a separate flex
row. Under `SnapshotRegion.FULL_DOCUMENT` + `WEBKIT_DISABLE_COMPOSITING_MODE`,
the software path drops that scroll layer and writes a uniformly-blank frame —
the DOM and layout are correct (verified via getBoundingClientRect + innerText),
only the full-document capture is empty. `SnapshotRegion.VISIBLE` captures the
realized viewport and renders the shell correctly.

Every pop-out window is sized to exactly one viewport (spec §3: Routines
960×680, Tac Map 1100×750, APRS Chat 440×640; all floor 420×360), so VISIBLE ==
the whole window — nothing is lost by not using FULL_DOCUMENT. Use snapshot.py
for taller-than-viewport documents; use this for the pop-window shell and any
other viewport-height fixture (the vacated-slot / docked-header dock columns).
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
wait_ms = int(sys.argv[5]) if len(sys.argv) > 5 else 2500

win = Gtk.OffscreenWindow()
win.set_default_size(width, height)
view = WebKit2.WebView()
view.set_size_request(width, height)
win.add(view)
win.show_all()

_done = {"v": False}


def _save():
    def _cb(v, res, _d):
        try:
            surface = v.get_snapshot_finish(res)
            surface.write_to_png(out)
            print(f"WROTE {out} ({surface.get_width()}x{surface.get_height()})")
            _done["v"] = True
        except Exception as e:  # noqa: BLE001
            print(f"SNAPSHOT ERROR: {e}", file=sys.stderr)
        Gtk.main_quit()

    view.get_snapshot(
        WebKit2.SnapshotRegion.VISIBLE,
        WebKit2.SnapshotOptions.NONE,
        None,
        _cb,
        None,
    )
    return False


def _on_load(v, event):
    if event == WebKit2.LoadEvent.FINISHED:
        GLib.timeout_add(wait_ms, _save)


view.connect("load-changed", _on_load)
view.load_uri(url)

GLib.timeout_add(20000, lambda: (Gtk.main_quit(), False)[1])
Gtk.main()
sys.exit(0 if _done["v"] else 2)
