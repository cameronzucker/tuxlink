#!/usr/bin/env python3
"""Deep, resumable scrape of the Winlink Programs Google Group.

Targets thousands of threads (~1-3 years of group history). Designed to
run unattended for hours. Aggressive checkpointing so partial completion
preserves all work-to-date if cookies expire mid-run.

Inputs:
  ./cookies.json — operator-exported Google session cookies (Cookie-Editor
    JSON format).

Outputs (all under this script's directory, gitignored):
  ./seen-urls.json    — set of thread URLs already scraped (resume key).
  ./corpus-deep.jsonl — one JSON record per thread, append-only.
  ./scrape-deep.log   — progress log (tail -f to monitor).
  ./thread-urls.txt   — Phase 1 output: all collected URLs.
  ./deep-list.png     — final listing screenshot after scroll-load.

Phases:
  1. Scroll-paginate the listing page to collect URLs (fast; ~10 min for
     thousands of threads).
  2. For each URL not in seen-urls.json: visit, extract subject + author
     + all post containers (OP + replies) + reply count + scraped-at
     timestamp; append to corpus-deep.jsonl; add URL to seen-urls.json.
     Settle 3–6 s between threads (jittered) for anti-detection courtesy.

Resumability:
  - Re-running the script picks up where it left off (skips URLs in
    seen-urls.json).
  - Phase 1 re-runs only if thread-urls.txt is missing.
  - corpus-deep.jsonl is append-only; never truncated.

Failure modes:
  - Cookies expired → detect login-redirect → abort with checkpoint
    preserved. Operator re-exports cookies, re-runs script, resumes.
  - Single-thread scrape error → logged, marked seen anyway, continue.
    (We don't infinite-loop on a permanently-broken thread.)
  - Network blip → Playwright's per-call timeout (15s) + per-thread
    try/except → skip + continue.
"""

import json
import os
import random
import sys
import time
import traceback
from pathlib import Path
from typing import Any

from playwright.sync_api import sync_playwright, Page, BrowserContext, TimeoutError as PWTimeout

ROOT = Path(__file__).parent
COOKIES_PATH = ROOT / "cookies.json"
SEEN_PATH = ROOT / "seen-urls.json"
CORPUS_PATH = ROOT / "corpus-deep.jsonl"
LOG_PATH = ROOT / "scrape-deep.log"
URLS_PATH = ROOT / "thread-urls.txt"
LIST_SCREENSHOT = ROOT / "deep-list.png"

GROUP_URL = "https://groups.google.com/g/winlink-programs-group"

# Phase 1 — listing pagination. Each click of [aria-label="Next page"]
# loads ~90 more threads. Google Groups uses classic pagination, NOT
# infinite scroll (per 2026-06-04 probe-scroll.py findings: scrolling
# does nothing on the listing page; the front page is hard-capped at
# ~96 anchors regardless of scroll mechanism).
MAX_PAGES = 200
PAGE_SETTLE_MS = 3000
PAGE_NO_PROGRESS_LIMIT = 2   # consecutive no-new-URL pages -> done

# Phase 2 — per-thread.
SETTLE_BASE_MS = 4000
SETTLE_JITTER_MS = 1500        # actual = base + uniform(0, jitter)
THREAD_NAV_TIMEOUT_MS = 20000
SAVE_EVERY_N = 1               # checkpoint every thread (cheap; bytes-tier)

# Politeness ceiling. If we've scraped this many threads in one run,
# break voluntarily — operator can re-launch later. Mostly a safety
# valve against a 24h runaway.
MAX_THREADS_PER_RUN = 5000

SAMESITE_MAP = {
    "no_restriction": "None",
    "lax": "Lax",
    "strict": "Strict",
    "none": "None",
    "unspecified": "Lax",
}


def log(msg: str) -> None:
    line = f"[{time.strftime('%H:%M:%S')}] {msg}"
    print(line, flush=True)
    with LOG_PATH.open("a") as f:
        f.write(line + "\n")


def convert_cookie(raw: dict[str, Any]) -> dict[str, Any] | None:
    name, value, domain = raw.get("name"), raw.get("value"), raw.get("domain")
    if not (name and value is not None and domain):
        return None
    out: dict[str, Any] = {
        "name": name, "value": value, "domain": domain,
        "path": raw.get("path", "/"),
        "httpOnly": bool(raw.get("httpOnly", False)),
        "secure": bool(raw.get("secure", False)),
    }
    if "expirationDate" in raw:
        out["expires"] = float(raw["expirationDate"])
    ss = raw.get("sameSite")
    if isinstance(ss, str) and ss.lower() in SAMESITE_MAP:
        out["sameSite"] = SAMESITE_MAP[ss.lower()]
    return out


def load_cookies() -> list[dict[str, Any]]:
    if not COOKIES_PATH.exists():
        sys.exit(f"FATAL: cookies file not found at {COOKIES_PATH}")
    raw = json.loads(COOKIES_PATH.read_text())
    converted = [c for c in (convert_cookie(r) for r in raw) if c is not None]
    log(f"loaded {len(converted)} cookies (skipped {len(raw) - len(converted)} invalid)")
    return converted


def load_seen() -> set[str]:
    if SEEN_PATH.exists():
        return set(json.loads(SEEN_PATH.read_text()))
    return set()


def save_seen(seen: set[str]) -> None:
    SEEN_PATH.write_text(json.dumps(sorted(seen)))


def detect_login_redirect(page: Page) -> str | None:
    url = page.url
    if "ServiceLogin" in url or "accounts.google.com" in url:
        return f"login redirect: {url}"
    return None


def collect_urls(page: Page) -> list[str]:
    """Phase 1: paginate the listing via Next-page clicks, collect all
    distinct thread URLs from page 1 → page N until pagination exhausts
    or MAX_PAGES is reached."""
    log(f"navigating to {GROUP_URL} (Phase 1: paginate + collect URLs)")
    page.goto(GROUP_URL, wait_until="domcontentloaded", timeout=30000)
    page.wait_for_timeout(PAGE_SETTLE_MS)

    redirect = detect_login_redirect(page)
    if redirect:
        page.screenshot(path=str(LIST_SCREENSHOT))
        sys.exit(f"FATAL: {redirect} (screenshot {LIST_SCREENSHOT})")

    seen_urls: list[str] = []
    seen_set: set[str] = set()
    no_progress = 0
    anchor_selector = 'a[href*="/g/winlink-programs-group/c/"]'
    next_selector = '[aria-label="Next page"]'

    for page_num in range(1, MAX_PAGES + 1):
        # Collect anchors visible right now.
        new_this_pass = 0
        try:
            for a in page.locator(anchor_selector).all():
                href = a.get_attribute("href") or ""
                if not href:
                    continue
                absolute = href if href.startswith("http") else f"https://groups.google.com{href.lstrip('.')}"
                absolute = absolute.split("?")[0].split("#")[0]
                if absolute not in seen_set:
                    seen_set.add(absolute)
                    seen_urls.append(absolute)
                    new_this_pass += 1
        except Exception as e:
            log(f"  page {page_num}: anchor collect error: {e}")

        log(f"  page {page_num}: +{new_this_pass} new (total {len(seen_urls)})")

        if new_this_pass == 0:
            no_progress += 1
            if no_progress >= PAGE_NO_PROGRESS_LIMIT:
                log(f"  page {page_num}: stopping — no new URLs for {no_progress} consecutive pages")
                break
        else:
            no_progress = 0

        # Click Next. Bail if button not present, not enabled, or click fails.
        try:
            next_btn = page.locator(next_selector).first
            if next_btn.count() == 0:
                log(f"  page {page_num}: no Next button — pagination exhausted")
                break
            if not next_btn.is_enabled():
                log(f"  page {page_num}: Next button disabled — pagination exhausted")
                break
            next_btn.click(timeout=5000)
            page.wait_for_timeout(PAGE_SETTLE_MS)
        except Exception as e:
            log(f"  page {page_num}: Next click failed: {e} — pagination exhausted")
            break

        # Checkpoint the URLs every 10 pages so a mid-run crash isn't fatal.
        if page_num % 10 == 0:
            URLS_PATH.write_text("\n".join(seen_urls))

    page.screenshot(path=str(LIST_SCREENSHOT))
    log(f"Phase 1 done: {len(seen_urls)} unique thread URLs collected from {page_num} listing pages")
    URLS_PATH.write_text("\n".join(seen_urls))
    return seen_urls


def scrape_thread(page: Page, url: str) -> dict[str, Any]:
    """Phase 2: visit a thread, pull subject + author + all posts + reply count."""
    page.goto(url, wait_until="domcontentloaded", timeout=THREAD_NAV_TIMEOUT_MS)
    page.wait_for_timeout(1500)

    subject = ""
    try:
        subject = page.locator("h1").first.inner_text(timeout=3000).strip()
    except Exception:
        try:
            subject = page.title().strip()
        except Exception:
            pass

    # OP author = first h3.
    author = ""
    try:
        author = page.locator("h3").first.inner_text(timeout=2000).strip()
    except Exception:
        pass

    # All posts — region elements; first is OP, rest are replies.
    posts: list[dict[str, str]] = []
    try:
        regions = page.locator('div[role="region"]').all()
        for idx, region in enumerate(regions[:50]):  # safety cap
            try:
                body = region.inner_text(timeout=2500)[:6000]  # cap per-post
                if body.strip():
                    posts.append({"idx": idx, "body": body})
            except Exception:
                pass
    except Exception as e:
        posts.append({"idx": 0, "body": f"[scrape error: {e}]"})

    return {
        "url": url,
        "subject": subject,
        "op_author": author,
        "post_count": len(posts),
        "posts": posts,
        "scraped_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }


def main() -> int:
    LOG_PATH.write_text("")  # truncate at run start
    cookies = load_cookies()
    seen = load_seen()
    log(f"resume state: {len(seen)} URLs already scraped")

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx: BrowserContext = browser.new_context(
            user_agent=("Mozilla/5.0 (X11; Linux aarch64) "
                        "AppleWebKit/537.36 (KHTML, like Gecko) "
                        "Chrome/130.0.0.0 Safari/537.36"),
            viewport={"width": 1280, "height": 1600},
        )
        ctx.add_cookies(cookies)
        page = ctx.new_page()

        # Phase 1 — only if thread-urls.txt missing (resume-friendly).
        if URLS_PATH.exists():
            urls = [l.strip() for l in URLS_PATH.read_text().splitlines() if l.strip()]
            log(f"Phase 1 SKIP — {len(urls)} URLs already in {URLS_PATH.name}")
        else:
            urls = collect_urls(page)

        # Phase 2 — iterate URLs.
        to_scrape = [u for u in urls if u not in seen]
        log(f"Phase 2 starting — {len(to_scrape)} URLs to scrape "
            f"({len(seen)} already done)")

        scraped_this_run = 0
        try:
            with CORPUS_PATH.open("a") as corpus_fp:
                for i, url in enumerate(to_scrape):
                    if scraped_this_run >= MAX_THREADS_PER_RUN:
                        log(f"hit MAX_THREADS_PER_RUN={MAX_THREADS_PER_RUN}; pausing")
                        break

                    try:
                        record = scrape_thread(page, url)
                    except PWTimeout as e:
                        log(f"  [{i+1}/{len(to_scrape)}] TIMEOUT {url}: {e}")
                        record = {"url": url, "error": "timeout",
                                  "scraped_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                    except Exception as e:
                        log(f"  [{i+1}/{len(to_scrape)}] ERROR {url}: {e}")
                        record = {"url": url, "error": str(e),
                                  "scraped_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}

                    # Detect mid-run login redirect (cookies expired).
                    if detect_login_redirect(page):
                        log("FATAL mid-run: cookies expired (login redirect detected)")
                        save_seen(seen)
                        return 2

                    corpus_fp.write(json.dumps(record) + "\n")
                    corpus_fp.flush()
                    seen.add(url)
                    scraped_this_run += 1

                    if i % 25 == 0:
                        log(f"  [{i+1}/{len(to_scrape)}] {record.get('subject', '')[:80]}")
                    if i % SAVE_EVERY_N == 0:
                        save_seen(seen)

                    # Polite jitter.
                    jitter = random.randint(0, SETTLE_JITTER_MS)
                    page.wait_for_timeout(SETTLE_BASE_MS + jitter)
        finally:
            save_seen(seen)
            log(f"run complete — scraped {scraped_this_run} this run, "
                f"{len(seen)} total in corpus")
            browser.close()

    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except Exception as e:
        log(f"UNCAUGHT: {e}\n{traceback.format_exc()}")
        sys.exit(3)
