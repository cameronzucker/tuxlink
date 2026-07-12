#!/usr/bin/env python3
"""D3 computed-style gate — loads a harness URL in real WebKitGTK and reports
getComputedStyle for the interactive elements the WEBKIT-1 discipline watches
(appearance/border/border-radius ≠ native GTK; every dropdown = .tux-select).

Usage: style-probe.py <url> [click_selector] [wait_ms]
  click_selector: optional element to click before probing (e.g. the strip's
                  holding trigger, to open the BandSubsetPopover).
Prints one JSON line per probed element; exits 1 if any <select> lacks
.tux-select or any probed button still has fully-native styling.
"""
import json
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gtk, WebKit2, GLib  # noqa: E402

url = sys.argv[1]
click_sel = sys.argv[2] if len(sys.argv) > 2 else None
wait_ms = int(sys.argv[3]) if len(sys.argv) > 3 else 2500

PROBE_JS = """
(function () {
  var targets = [
    '.si-collapse',
    '[data-testid="ft8-strip-holding-trigger"]',
    '[data-testid="ft8-strip-start-cta"]',
    '[data-testid^="band-subset-chip-"]',
    '[data-testid="ft8-setup-cat-test"] button',
    '.ft8-setup__cta-row button',
    '.ft8-setup__device button',
    'button.station-finder__chipuse',
    'select',
  ];
  var out = [];
  targets.forEach(function (sel) {
    var els = Array.prototype.slice.call(document.querySelectorAll(sel), 0, 2);
    els.forEach(function (e) {
      var cs = getComputedStyle(e);
      out.push({
        sel: sel,
        tag: e.tagName.toLowerCase(),
        cls: (e.className || '').toString().slice(0, 80),
        appearance: cs.appearance || cs.webkitAppearance || '',
        borderRadius: cs.borderRadius,
        borderStyle: cs.borderStyle,
        borderColor: cs.borderColor,
        background: cs.backgroundColor,
        isTuxSelect: e.tagName === 'SELECT' ? e.className.indexOf('tux-select') >= 0 : null,
      });
    });
  });
  return JSON.stringify(out);
})()
"""

win = Gtk.OffscreenWindow()
win.set_default_size(1460, 760)
view = WebKit2.WebView()
view.set_size_request(1460, 760)
win.add(view)
win.show_all()

state = {"ok": False}


def _finish(raw):
    rows = json.loads(raw) if raw and raw != "null" else []
    failures = []
    for r in rows:
        print(json.dumps(r))
        # A native-GTK button: appearance auto AND square corners AND default
        # grey background. Any explicit project styling breaks the trifecta.
        if r["tag"] == "button" and r["appearance"] not in ("none",) \
           and r["borderRadius"] in ("0px",) :
            failures.append(f"native-looking button: {r['sel']}")
        if r["tag"] == "select" and r["isTuxSelect"] is False:
            failures.append(f"select without .tux-select: {r['sel']} ({r['cls']})")
    print(json.dumps({"probed": len(rows), "failures": failures}))
    state["ok"] = not failures
    Gtk.main_quit()


def _probe():
    def _cb(v, res, _d):
        try:
            val = v.run_javascript_finish(res).get_js_value().to_string()
            _finish(val)
        except Exception as e:  # noqa: BLE001
            print(json.dumps({"error": str(e)}))
            Gtk.main_quit()
    view.run_javascript(PROBE_JS, None, _cb, None)
    return False


def _maybe_click_then_probe():
    if click_sel:
        js = "var e=document.querySelector(%s); if(e) e.click(); !!e;" % json.dumps(click_sel)
        def _clicked(v, res, _d):
            try:
                v.run_javascript_finish(res)
            except Exception:  # noqa: BLE001
                pass
            GLib.timeout_add(600, _probe)
        view.run_javascript(js, None, _clicked, None)
    else:
        _probe()
    return False


def _on_load(v, event):
    if event == WebKit2.LoadEvent.FINISHED:
        GLib.timeout_add(wait_ms, _maybe_click_then_probe)


view.connect("load-changed", _on_load)
view.load_uri(url)
GLib.timeout_add(25000, lambda: (Gtk.main_quit(), False)[1])
Gtk.main()
sys.exit(0 if state["ok"] else 1)
