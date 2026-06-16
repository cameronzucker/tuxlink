#!/usr/bin/env python3
"""Record an interaction screencast of the tuxlink shell for the README.

Drives the real frontend (via the README harness shim) in Chromium and records
a WebM of a scripted flow: open a message -> toggle the APRS dock (with injected
heard traffic) -> cycle color schemes. Convert the WebM to a looping animated
WebP with ffmpeg afterwards (see harness README).

Prereq: VITE_TUXLINK_FIXTURE=1 pnpm exec vite --host 127.0.0.1 --port 1420 --strictPort
Usage:  python3 dev/readme-screenshot-harness/screencast.py <out_dir>
"""
import sys
from playwright.sync_api import sync_playwright

URL = "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell"
OUT_DIR = sys.argv[1] if len(sys.argv) > 1 else "/tmp/cast"
VW = {"width": 1280, "height": 800}

APRS_MSGS = [
    {"sender": "K4ARC", "addressee": "", "text": "Net opens 1800Z on the county VARA gateway — QNI welcome.", "msgid": "42"},
    {"sender": "WX4MTL", "addressee": "W4PHS", "text": "Bartlett HS staging confirmed, ETA 1545L with 6 cots.", "msgid": "17"},
    {"sender": "N4SAR", "addressee": "", "text": "Welfare: Hutchins family OK, sheltering in place.", "msgid": "88"},
]


def main():
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--force-color-profile=srgb"])

        # 1) Warm the vite module graph in a throwaway (unrecorded) context so the
        #    recorded run isn't dominated by a cold AppShell compile.
        warm = b.new_context(viewport=VW)
        wp = warm.new_page()
        wp.goto(URL, wait_until="networkidle", timeout=60000)
        wp.wait_for_timeout(2000)
        warm.close()

        # 2) Recorded context.
        ctx = b.new_context(viewport=VW, record_video_dir=OUT_DIR, record_video_size=VW, color_scheme="dark")
        page = ctx.new_page()
        page.goto(URL, wait_until="networkidle", timeout=60000)
        page.wait_for_selector("[data-testid=folder-sidebar]", timeout=20000)
        page.wait_for_timeout(900)

        # Beat 1 — open a message; the reading pane fills with the ICS-213.
        page.locator("[data-testid=message-row-M1]").first.click()
        page.wait_for_timeout(1900)

        # Beat 2 — toggle the APRS dock; inject heard traffic so it arrives live.
        page.locator("[data-testid=dash-aprs-control]").first.click()
        page.wait_for_timeout(500)
        page.evaluate("() => window.__harness.emit('aprs-listening:change', true)")
        for m in APRS_MSGS:
            page.evaluate("(m) => window.__harness.emit('aprs-message:new', m)", m)
            page.wait_for_timeout(700)
        page.wait_for_timeout(1600)

        # Beat 3 — cycle color schemes (whole UI re-skins).
        for scheme, hold in [("night-red", 1700), ("daylight", 1700), ("default", 1100)]:
            page.evaluate("(s) => window.__harness.scheme(s)", scheme)
            page.wait_for_timeout(hold)

        ctx.close()  # finalizes the video
        print("VIDEO", page.video.path())
        b.close()


if __name__ == "__main__":
    main()
