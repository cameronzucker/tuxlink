#!/usr/bin/env python3
"""snap-click.py — snapshot.py's click-first sibling: load a harness URL in
real WebKitGTK, click one element (CSS selector), wait, then write a PNG.
Built for QA round-3 renders where the state under review is click-gated
(the BandSubsetPopover's open state, the WWV control's armed note).

Usage: snap-click.py <url> <click_selector> <out.png> [width] [height] [wait_ms] [post_click_ms]
"""
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gtk, WebKit2, GLib  # noqa: E402

url = sys.argv[1]
click_sel = sys.argv[2]
out = sys.argv[3]
width = int(sys.argv[4]) if len(sys.argv) > 4 else 1460
height = int(sys.argv[5]) if len(sys.argv) > 5 else 800
wait_ms = int(sys.argv[6]) if len(sys.argv) > 6 else 2500
post_click_ms = int(sys.argv[7]) if len(sys.argv) > 7 else 800

win = Gtk.OffscreenWindow()
win.set_default_size(width, height)
view = WebKit2.WebView()
view.set_size_request(width, height)
win.add(view)
win.show_all()

state = {"ok": False}


def _snapshot():
    def _cb(v, res, _d):
        try:
            surface = v.get_snapshot_finish(res)
            surface.write_to_png(out)
            state["ok"] = True
            print(f"wrote {out}")
        except Exception as e:  # noqa: BLE001
            print(f"snapshot failed: {e}")
        Gtk.main_quit()

    view.get_snapshot(
        WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, _cb, None
    )
    return False


def _click_then_snapshot():
    import json

    js = "var e=document.querySelector(%s); if(e) e.click(); !!e;" % json.dumps(click_sel)

    def _clicked(v, res, _d):
        try:
            val = v.run_javascript_finish(res).get_js_value().to_boolean()
            if not val:
                print(f"WARNING: selector matched nothing: {click_sel}")
        except Exception as e:  # noqa: BLE001
            print(f"click failed: {e}")
        GLib.timeout_add(post_click_ms, _snapshot)

    view.run_javascript(js, None, _clicked, None)
    return False


def _on_load(v, event):
    if event == WebKit2.LoadEvent.FINISHED:
        GLib.timeout_add(wait_ms, _click_then_snapshot)


view.connect("load-changed", _on_load)
view.load_uri(url)
GLib.timeout_add(40000, lambda: (Gtk.main_quit(), False)[1])
Gtk.main()
sys.exit(0 if state["ok"] else 1)
