#!/usr/bin/env python3
"""On-Pi map frame-timing driver — loads the perf-harness route in the same
WebKitGTK engine Tauri uses, lets the page run its deterministic pan/zoom script
while sampling requestAnimationFrame deltas, then reads the p50/p95 frame time
back out of the DOM. No compositor, no Tauri build.

This is the gate the front-end render-harness could never be: the render-harness
uses canned Tauri data + a trivial scene, so its fps number is not an app-level
prediction (docs/pitfalls/testing-pitfalls.md MAP-PERF-1). This harness mounts the
real MapLibreMap with a region-pack source + station pins + the Maidenhead grid at
real resolution under software GL, and measures REAL frame timing.

Run it with the software-GL env vars set (the Pi render profile):

    WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
      python3 dev/perf-harness/perf.py \
        "http://localhost:1420/dev/perf-harness/harness.html" \
        1366 768

Args: perf.py <url> [width] [height] [timeout_ms]
Exit code 0 on a measured result, 2 on timeout / no result.
"""
import json
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gtk, WebKit2, GLib  # noqa: E402

url = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:1420/dev/perf-harness/harness.html"
width = int(sys.argv[2]) if len(sys.argv) > 2 else 1366
height = int(sys.argv[3]) if len(sys.argv) > 3 else 768
# Generous default: warmup (2.5s) + run (12s) + tile-load slack.
timeout_ms = int(sys.argv[4]) if len(sys.argv) > 4 else 45000

# NOTE: an OffscreenWindow has no real swapchain, so the rAF cadence under it can
# differ from an on-screen window. A visible window measures the production render
# path more faithfully; the offscreen path is the conservative no-display fallback.
# Default to a visible window when a display is present.
win = Gtk.Window()
win.set_default_size(width, height)
view = WebKit2.WebView()
view.set_size_request(width, height)
win.add(view)
win.show_all()

state = {"done": False, "result": None}

# The DOM probe: read #perf-result's data-state + data-result. WebKit2GTK 4.1's
# run_javascript resolves a JS value we marshal back through the JSCValue API.
PROBE = (
    "(function(){var e=document.getElementById('perf-result');"
    "return e?JSON.stringify({state:e.getAttribute('data-state'),"
    "result:e.getAttribute('data-result')}):"
    "JSON.stringify({state:'missing',result:null});})()"
)


def _on_js(view_, res, _data):
    try:
        val = view_.run_javascript_finish(res)
        js = val.get_js_value()
        raw = js.to_string()
        probe = json.loads(raw)
    except Exception as e:  # noqa: BLE001
        print(f"PROBE ERROR: {e}", file=sys.stderr)
        return
    if probe.get("state") == "done" and probe.get("result"):
        state["done"] = True
        state["result"] = json.loads(probe["result"])
        Gtk.main_quit()


def _poll():
    if state["done"]:
        return False
    view.run_javascript(PROBE, None, _on_js, None)
    return True  # keep polling


def _on_load(v, event):
    if event == WebKit2.LoadEvent.FINISHED:
        # Poll the DOM result twice a second until the page reports 'done'.
        GLib.timeout_add(500, _poll)


view.connect("load-changed", _on_load)
view.load_uri(url)

# Hard safety timeout so the driver never hangs.
GLib.timeout_add(timeout_ms, lambda: (Gtk.main_quit(), False)[1])
Gtk.main()

if state["done"] and state["result"]:
    r = state["result"]
    print("=== MAP PERF (on-Pi, software GL) ===")
    print(f"  p50 frame time : {r.get('p50_ms')} ms")
    print(f"  p95 frame time : {r.get('p95_ms')} ms")
    print(f"  approx fps     : {r.get('approx_fps')}")
    print(f"  frames sampled : {r.get('frames')}")
    print(f"  region pack    : {r.get('pack')}")
    print(f"  run/warmup ms  : {r.get('run_ms')}/{r.get('warmup_ms')}")
    print(json.dumps(r))
    sys.exit(0)

print(
    "NO RESULT — the page never reported a measurement before the timeout.\n"
    "Likely causes: dev server not running on the URL host/port; the bundled\n"
    "world archive is absent so the map renders empty (still measurable, but\n"
    "check the console); or WebGL is unavailable in this WebKitGTK build.",
    file=sys.stderr,
)
sys.exit(2)
