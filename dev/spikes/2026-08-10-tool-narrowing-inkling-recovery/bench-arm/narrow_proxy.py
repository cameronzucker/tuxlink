#!/usr/bin/env python3
"""Narrowing rewrite proxy (classifier-driven tool narrowing, experimental arm).

Sibling of `tee_proxy.py` — same tee/ledger behavior, plus a REQUEST REWRITE
implementing the tuxlink-efk3k narrowing mechanism at the wire, so the bench
measures the proposed production shape with ZERO changes to the vendored
harness (ElmerSession rebuilds its provider per turn, so a Provider wrapper
cannot intercept; the reference-run precedent points the model config at a
local endpoint instead — this is that seam).

What it rewrites (POST bodies that carry a non-empty `tools` array — agent
turns; responder/vetting calls carry no tools and pass through untouched):

1. `tools` is FILTERED to the cell's frozen classifier shortlist + the
   deterministic pin-set (server_info / docs_search / the abort section).
   Filtering the request's own array keeps schemas byte-identical to
   production's.
2. A narrowing system message (same wording as spike v3/v4, for
   cross-instrument comparability) is PREPENDED: advisory shortlist +
   the full 92-name inventory, any of which may be called by name.

The agent-runner validates emitted calls against the FULL invoker surface
(unchanged), so an outside-array call-by-name executes — the lazy-schema
contract, live.

Cells are keyed by the EXACT first-user-message text (the corpus prompt,
stable across every turn of the agentic loop). A prompt with tools that has
no fixture entry is REFUSED (502, fail-closed) — a silent pass-through would
contaminate the arm with stock cells.

Fixture shape (see gen_narrow_fixture.py):
{
  "meta": {...provenance...},
  "pins": ["server_info", ...],
  "inventory_text": "- id: title\n...",       # all 92
  "cells": { "<sha256(prompt)>": {"cell": id, "prompt": text,
              "shortlist": [{"id","title","score"}, ...12] } }
}
"""
import argparse
import hashlib
import http.client
import json
import os
import ssl
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ARGS = None
FIXTURE = None
SCHEMAS = {}  # name -> {name, description, parameters}; loaded from --schemas
LOCK = threading.Lock()

NARROW_SYSTEM = (
    "Tool routing for this request: a classifier suggests these tools as "
    "most likely relevant (it can be wrong):\n{shortlist}\n\n"
    "Full tool inventory - ANY of these can be called by name even if its "
    "full definition is not in your tools list; call it and the definition "
    "will be provided:\n{inventory}"
)


def ledger_write(record):
    day = time.strftime("%Y%m%d", time.gmtime())
    path = os.path.join(ARGS.ledger, f"tee-{day}.jsonl")
    line = json.dumps(record, ensure_ascii=False)
    with LOCK:
        with open(path, "a") as f:
            f.write(line + "\n")


def first_user_text(messages):
    """First NON-EMPTY user message: the battery's transcript opens with a
    synthetic empty user turn before the corpus prompt (v25 ledger shape)."""
    for m in messages or []:
        if m.get("role") == "user":
            c = m.get("content")
            if isinstance(c, list):  # content-parts form
                c = "".join(p.get("text", "") for p in c if isinstance(p, dict))
            if isinstance(c, str) and c.strip():
                return c
    return None


def narrow_rewrite(body):
    """Rewrite an agent request in place. Returns (body, note) or raises
    LookupError on a fixture miss (the caller fail-closes)."""
    prompt = first_user_text(body.get("messages"))
    if prompt is None:
        raise LookupError("request has tools but no user message")
    key = hashlib.sha256(prompt.encode()).hexdigest()
    cell = FIXTURE["cells"].get(key)
    if cell is None:
        raise LookupError(f"no fixture entry for prompt {prompt[:120]!r}")

    keep = [c["id"] for c in cell["shortlist"]]
    keep += [p for p in FIXTURE.get("pins", []) if p not in keep]
    keep_set = set(keep)
    before = len(body.get("tools") or [])
    body["tools"] = [
        t for t in body.get("tools") or []
        if ((t.get("function") or {}).get("name")) in keep_set
    ]

    # SCHEMA FURNISHING (the lazy-schema contract's second half): production
    # semantics are "sticky for the session" — derived here STATELESSLY from
    # the transcript the request itself carries. Any tool the model has
    # already called by name (outside the shortlist+pins) gets its FULL
    # schema injected into this turn's array, so the first call may bounce
    # on guessed arguments but every subsequent call composes against the
    # real parameters. This is the fix for the argument-blind bounce class
    # (predict_path / routines_save) FINDINGS-BENCH-AB identified.
    furnished = []
    if SCHEMAS:
        for m in body.get("messages") or []:
            if m.get("role") != "assistant":
                continue
            for tc in m.get("tool_calls") or []:
                n = (tc.get("function") or {}).get("name")
                if (n and n not in keep_set and n not in furnished
                        and n in SCHEMAS):
                    furnished.append(n)
        for n in furnished:
            s = SCHEMAS[n]
            body["tools"].append({"type": "function", "function": {
                "name": s["name"], "description": s.get("description", ""),
                "parameters": s.get("parameters", {})}})

    shortlist_txt = "\n".join(
        f"- {c['id']}: {c['title']}" for c in cell["shortlist"])
    sys_text = NARROW_SYSTEM.format(
        shortlist=shortlist_txt, inventory=FIXTURE["inventory_text"])
    # The battery sends the Elmer system prompt as messages[0]; production
    # narrowing would EXTEND the system prompt (static prefix first), so the
    # narrowing block is appended to it. If no system message exists, prepend
    # one.
    msgs = body["messages"]
    if msgs and msgs[0].get("role") == "system" and isinstance(msgs[0].get("content"), str):
        msgs[0]["content"] = msgs[0]["content"] + "\n\n" + sys_text
        placement = "appended_to_system"
    else:
        body["messages"] = [{"role": "system", "content": sys_text}] + msgs
        placement = "prepended"

    return body, {
        "cell": cell["cell"],
        "tools_before": before,
        "tools_after": len(body["tools"]),
        "system_placement": placement,
        "furnished": furnished,
    }


class Tee(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_GET(self):
        self._relay("GET")

    def do_POST(self):
        self._relay("POST")

    def _relay(self, method):
        up = urllib.parse.urlparse(ARGS.upstream)
        body = b""
        if "Content-Length" in self.headers:
            body = self.rfile.read(int(self.headers["Content-Length"]))

        started = time.time()
        record = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(started)),
            "method": method,
            "path": self.path,
        }

        # ── the narrowing rewrite ────────────────────────────────────────
        parsed = _try_json(body)
        narrowed_note = None
        # Serving-name drift absorbed at the wire: the plan keeps v25's model
        # id, the serving registers the full name. Applies to EVERY request
        # with a model field (responder calls carry no tools but still 404
        # on a stale name).
        if (
            method == "POST"
            and isinstance(parsed, dict)
            and ARGS.model_map
            and parsed.get("model") in ARGS.model_map
        ):
            parsed["model"] = ARGS.model_map[parsed["model"]]
            body = json.dumps(parsed).encode()
        if (
            method == "POST"
            and isinstance(parsed, dict)
            and parsed.get("tools")
        ):
            try:
                parsed, narrowed_note = narrow_rewrite(parsed)
                body = json.dumps(parsed).encode()
            except LookupError as e:
                # Fail CLOSED: a silent stock pass-through would contaminate
                # the narrowed arm. The cell errors loudly instead.
                record["narrow_miss"] = str(e)
                record["request"] = parsed
                ledger_write(record)
                msg = json.dumps({"error": {
                    "message": f"narrow_proxy fail-closed: {e}",
                    "type": "narrow_fixture_miss"}}).encode()
                self.send_response(502)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(msg)))
                self.end_headers()
                self.wfile.write(msg)
                return
        record["request"] = parsed if parsed is not None else None
        if narrowed_note:
            record["narrow"] = narrowed_note

        ctx = ssl.create_default_context()
        if up.scheme == "https":
            conn = http.client.HTTPSConnection(up.hostname, up.port or 443, context=ctx, timeout=900)
        else:
            conn = http.client.HTTPConnection(up.hostname, up.port or 80, timeout=900)

        fwd = {k: v for k, v in self.headers.items()
               if k.lower() not in ("host", "connection", "content-length", "accept-encoding")}
        fwd["Host"] = up.hostname
        fwd["Accept-Encoding"] = "identity"
        if body:
            fwd["Content-Length"] = str(len(body))

        try:
            conn.request(method, self.path, body=body or None, headers=fwd)
            resp = conn.getresponse()
        except Exception as e:
            record["error"] = f"upstream: {e}"
            ledger_write(record)
            self.send_error(502, f"upstream: {e}")
            return

        ctype = resp.getheader("Content-Type", "")
        self.send_response(resp.status)
        for k, v in resp.getheaders():
            if k.lower() in ("connection", "transfer-encoding", "content-length"):
                continue
            self.send_header(k, v)

        if "text/event-stream" in ctype:
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            chunks = []
            try:
                while True:
                    piece = resp.read1(65536)
                    if not piece:
                        break
                    chunks.append(piece)
                    self.wfile.write(f"{len(piece):x}\r\n".encode() + piece + b"\r\n")
                    self.wfile.flush()
                self.wfile.write(b"0\r\n\r\n")
            except Exception as e:
                record["stream_error"] = str(e)
            record["response_sse"] = _join_sse(b"".join(chunks))
        else:
            data = resp.read()
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            try:
                self.wfile.write(data)
            except Exception as e:
                record["write_error"] = str(e)
            record["response"] = _try_json(data)

        record["elapsed_s"] = round(time.time() - started, 3)
        record["status"] = resp.status
        ledger_write(record)
        conn.close()


def _try_json(b):
    if not b:
        return None
    try:
        return json.loads(b)
    except Exception:
        return {"_raw": b.decode("utf-8", "replace")[:4000]}


def _join_sse(raw):
    content, reasoning, n = [], [], 0
    for line in raw.decode("utf-8", "replace").splitlines():
        if not line.startswith("data:"):
            continue
        payload = line[5:].strip()
        if payload == "[DONE]":
            continue
        n += 1
        try:
            d = json.loads(payload)
            delta = (d.get("choices") or [{}])[0].get("delta") or {}
            if delta.get("content"):
                content.append(delta["content"])
            rc = delta.get("reasoning_content") or delta.get("reasoning")
            if rc:
                reasoning.append(rc)
        except Exception:
            pass
    return {
        "chunks": n,
        "content": "".join(content),
        "reasoning": "".join(reasoning) or None,
    }


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", default="127.0.0.2:8892")
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--fixture", required=True)
    ap.add_argument("--model-map", action="append", default=[],
                    help="old=new served-model-name mapping (repeatable)")
    ap.add_argument("--schemas", default=None,
                    help="tool-schemas.json (full registry dump); enables "
                         "furnish-on-by-name-call")
    ARGS = ap.parse_args()
    ARGS.model_map = dict(m.split("=", 1) for m in ARGS.model_map)
    if ARGS.schemas:
        with open(os.path.expanduser(ARGS.schemas)) as f:
            SCHEMAS = {s["name"]: s for s in json.load(f)}
    ARGS.ledger = os.path.expanduser(ARGS.ledger)
    os.makedirs(ARGS.ledger, exist_ok=True)
    with open(os.path.expanduser(ARGS.fixture)) as f:
        FIXTURE = json.load(f)
    assert FIXTURE.get("cells") and FIXTURE.get("inventory_text"), "fixture incomplete"
    host, port = ARGS.listen.rsplit(":", 1)
    print(f"narrow_proxy: {ARGS.listen} -> {ARGS.upstream}, "
          f"{len(FIXTURE['cells'])} cells, pins {FIXTURE.get('pins')}, "
          f"ledger {ARGS.ledger}")
    ThreadingHTTPServer((host, int(port)), Tee).serve_forever()
