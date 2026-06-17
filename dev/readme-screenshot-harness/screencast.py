#!/usr/bin/env python3
"""Record an interaction screencast of the tuxlink shell for the README.

Highlights the adaptable emcomm workspace: read Winlink messages, then bring up
APRS tactical chat with the heard-station map alongside, and move between the two.
Drives the real frontend (via the README harness shim) in Chromium and records a
WebM; convert to a looping animated WebP afterwards (see the harness README).

Prereq:
  cp src-tauri/resources/basemap/world-z0-6.pmtiles public/basemap/   # 43 MB, for the map
  VITE_TUXLINK_FIXTURE=1 pnpm exec vite --host 127.0.0.1 --port 1420 --strictPort
Usage:
  python3 dev/readme-screenshot-harness/screencast.py <out_dir>
"""
import sys
from playwright.sync_api import sync_playwright

URL = "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell"
OUT_DIR = sys.argv[1] if len(sys.argv) > 1 else "/tmp/cast"
VW = {"width": 1280, "height": 800}

CHAT = [
    {"sender": "K4ARC", "addressee": "", "text": "Net opens 1800Z on the county VARA gateway — QNI welcome.", "msgid": "42"},
    {"sender": "WX4MTL", "addressee": "W4PHS", "text": "Bartlett HS staging confirmed, ETA 1545L with 6 cots.", "msgid": "17"},
    {"sender": "N4SAR", "addressee": "", "text": "Welfare: Hutchins family OK, sheltering in place.", "msgid": "88"},
]
# Heard positions around the operator's grid (EM75 — Memphis area).
POS = [
    {"sender": "WX4MTL", "lat": 35.20, "lon": -89.87, "symbolTable": "/", "symbolCode": "-", "comment": "EC Shelby Co"},
    {"sender": "K4ARC", "lat": 35.05, "lon": -90.10, "symbolTable": "/", "symbolCode": "r", "comment": "ARES net control"},
    {"sender": "N4SAR", "lat": 34.93, "lon": -89.84, "symbolTable": "/", "symbolCode": "j", "comment": "SAR team 2"},
    {"sender": "KK4OBN", "lat": 35.12, "lon": -90.05, "symbolTable": "/", "symbolCode": "k", "comment": "logistics"},
]


def main():
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--force-color-profile=srgb"])

        def click(page, tid):
            page.locator(f"[data-testid={tid}]").first.click(timeout=8000)

        def emit(page, event, payload):
            page.evaluate("([e,p]) => window.__harness.emit(e,p)", [event, payload])

        # Warm the vite module graph (unrecorded) so the cold AppShell compile +
        # first pmtiles range-fetches don't dominate the recorded run.
        warm = b.new_context(viewport=VW)
        wp = warm.new_page()
        wp.goto(URL, wait_until="networkidle", timeout=60000)
        wp.wait_for_timeout(2500)
        warm.close()

        # record_video_size MUST equal the viewport. A larger size (e.g. a
        # device_scale_factor=2 + 2x record_video_size attempt) does NOT capture
        # at higher resolution — Playwright renders the page in the top-left and
        # letterboxes the rest grey. Crispness comes from a high `q:v` on encode,
        # not from the capture size.
        ctx = b.new_context(
            viewport=VW,
            record_video_dir=OUT_DIR,
            record_video_size=VW,
            color_scheme="dark",
        )
        page = ctx.new_page()
        page.goto(URL, wait_until="networkidle", timeout=60000)
        page.wait_for_selector("[data-testid=folder-sidebar]", timeout=20000)
        page.wait_for_timeout(700)

        # --- Strategic: read Winlink messages ---
        click(page, "message-row-M1"); page.wait_for_timeout(1300)
        click(page, "message-row-M5"); page.wait_for_timeout(1200)   # an ICS-213RR
        click(page, "message-row-M6"); page.wait_for_timeout(1200)

        # --- Tactical: APRS chat + map ---
        click(page, "dash-aprs-control"); page.wait_for_timeout(400)
        emit(page, "aprs-listening:change", True)
        for m in CHAT:
            emit(page, "aprs-message:new", m); page.wait_for_timeout(550)
        for q in POS:
            emit(page, "aprs-position:new", q)
        page.wait_for_timeout(700)
        click(page, "aprs-map-toggle"); page.wait_for_timeout(3200)   # map expands with pins

        # --- Back to strategic, then tactical again (the adaptable workspace) ---
        click(page, "aprs-map-toggle"); page.wait_for_timeout(300)    # map off -> message returns
        click(page, "message-row-M2"); page.wait_for_timeout(1500)
        click(page, "aprs-map-toggle"); page.wait_for_timeout(2600)   # map back on

        ctx.close()
        print("VIDEO", page.video.path())
        b.close()


if __name__ == "__main__":
    main()
