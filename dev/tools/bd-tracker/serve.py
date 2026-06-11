#!/usr/bin/env python3
"""Loopback-only live viewer for the project's bd (beads) backlog.

Serves a single-page tracker UI that reads bd issues LIVE on every API hit
(no baked-in snapshot), so it always reflects current tracker state.

Usage:
    python3 dev/tools/bd-tracker/serve.py [--port 8765] [--host 127.0.0.1]

Then open http://127.0.0.1:8765/ in a browser. Bound to loopback only.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

HERE = Path(__file__).resolve().parent
# repo root = two levels up from dev/tools/bd-tracker
REPO_ROOT = HERE.parent.parent.parent
INDEX = HERE / "index.html"

# Statuses we treat as "open" (everything that is not closed/deferred).
DEFAULT_STATUSES = "open,in_progress,blocked"


def fetch_issues(statuses: str) -> tuple[int, bytes]:
    """Run bd and return (http_status, json_bytes)."""
    cmd = ["bd", "list", "--status", statuses, "--json", "--limit", "0"]
    try:
        out = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT),
            capture_output=True,
            timeout=30,
            check=False,
        )
    except FileNotFoundError:
        return 500, json.dumps({"error": "bd not found on PATH"}).encode()
    except subprocess.TimeoutExpired:
        return 504, json.dumps({"error": "bd timed out"}).encode()

    if out.returncode != 0:
        err = out.stderr.decode("utf-8", "replace")[:2000]
        return 500, json.dumps({"error": f"bd exited {out.returncode}", "stderr": err}).encode()

    raw = out.stdout.decode("utf-8", "replace").strip()
    # bd may emit a leading non-JSON banner line in some versions; trim to first '['.
    idx = raw.find("[")
    if idx > 0:
        raw = raw[idx:]
    try:
        data = json.loads(raw or "[]")
    except json.JSONDecodeError as exc:
        return 500, json.dumps({"error": f"bad JSON from bd: {exc}"}).encode()
    return 200, json.dumps(data).encode()


class Handler(BaseHTTPRequestHandler):
    def _send(self, code: int, body: bytes, ctype: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802 (stdlib naming)
        path = urlparse(self.path).path
        if path == "/" or path == "/index.html":
            try:
                body = INDEX.read_bytes()
            except OSError:
                self._send(500, b"index.html missing", "text/plain")
                return
            self._send(200, body, "text/html; charset=utf-8")
            return
        if path == "/api/issues":
            qs = urlparse(self.path).query
            statuses = DEFAULT_STATUSES
            for part in qs.split("&"):
                if part.startswith("status="):
                    statuses = part[len("status="):] or DEFAULT_STATUSES
            code, body = fetch_issues(statuses)
            self._send(code, body, "application/json")
            return
        self._send(404, b"not found", "text/plain")

    def log_message(self, fmt: str, *args) -> None:  # quieter logs
        sys.stderr.write("[bd-tracker] " + (fmt % args) + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--host", default="127.0.0.1", help="bind address (loopback only by default)")
    args = ap.parse_args()

    if args.host not in ("127.0.0.1", "localhost", "::1"):
        print(f"refusing non-loopback bind host {args.host!r}", file=sys.stderr)
        return 2

    httpd = ThreadingHTTPServer((args.host, args.port), Handler)
    url = f"http://{args.host}:{args.port}/"
    print(f"bd-tracker live at {url}  (repo: {REPO_ROOT})", flush=True)
    print("Ctrl+C to stop.", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nstopping", flush=True)
    finally:
        httpd.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
