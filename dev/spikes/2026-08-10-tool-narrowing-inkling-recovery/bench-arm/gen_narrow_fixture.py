#!/usr/bin/env python3
"""Build the frozen narrowing fixture for the classifier-narrowed bench arm.

Trenchcoat rule (tuxlink classifier epic): the classifier NEVER runs inside
the bench. This script precomputes each corpus prompt's shortlist offline
(via tuxlink-classify's eval_tools dump) and freezes it into one fixture the
narrow proxy consumes deterministically.

Three subcommands, run in order on the bench host (r2):

  make-queries   corpus.json -> queries.jsonl for eval_tools (TUXLINK_QUERIES)
  make-fixture   shortlists.jsonl + tool-surface.jsonl -> narrow-fixture.json
  check-ledger   replay-join a prior run's tee ledger against the fixture:
                 every agent request's first user message MUST resolve to a
                 cell. A miss here would have been a fail-closed 502 mid-run.

Provenance (classifier rev, corpus sha, template version, k, pins) is stamped
into the fixture's meta block.
"""
import argparse
import glob
import hashlib
import json
import os
import sys

PINS_STATIC = ["server_info", "docs_search"]
K = 12


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def load_jsonl(path):
    out = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                out.append(json.loads(line))
    return out


def cmd_make_queries(args):
    corpus = json.load(open(args.corpus))
    n = 0
    with open(args.out, "w") as f:
        for p in corpus["prompts"]:
            f.write(json.dumps({
                "id": p["id"],
                "q": p["prompt"],
                "expect": {"kind": "tool", "tools": []},
                "behavior": "ask",
            }) + "\n")
            n += 1
    print(f"wrote {n} queries -> {args.out}")


def cmd_make_fixture(args):
    shortlists = load_jsonl(args.shortlists)
    surface = load_jsonl(args.surface)
    corpus = json.load(open(args.corpus))
    prompts = {p["id"]: p["prompt"] for p in corpus["prompts"]}

    pins = PINS_STATIC + sorted(
        r["id"] for r in surface if r.get("section") == "abort")
    inventory_text = "\n".join(
        f"- {r['id']}: {r['title']}" for r in sorted(surface, key=lambda r: r["id"]))

    cells = {}
    for row in shortlists:
        cid = row["id"]
        prompt = prompts.get(cid)
        if prompt is None:
            print(f"WARN: shortlist row {cid} not in corpus; skipped", file=sys.stderr)
            continue
        key = hashlib.sha256(prompt.encode()).hexdigest()
        if key in cells:
            # Text-identical cells (premise-variant pairs) collapse to one
            # entry: the classifier is deterministic on text, so the same
            # shortlist is correct for both. Record the alias for the ledger.
            cells[key].setdefault("aliases", []).append(cid)
            continue
        cells[key] = {
            "cell": cid,
            "prompt": prompt,
            "shortlist": [
                {"id": c["id"], "title": c["title"], "score": c["score"]}
                for c in row["shortlist"][:K]
            ],
        }
    covered = set()
    for c in cells.values():
        covered.add(c["cell"])
        covered.update(c.get("aliases", []))
    missing = sorted(set(prompts) - covered)
    if missing:
        print(f"ERROR: {len(missing)} corpus prompts lack shortlists: {missing[:8]}",
              file=sys.stderr)
        sys.exit(1)

    fixture = {
        "meta": {
            "generator": "gen_narrow_fixture.py",
            "k": K,
            "pins": pins,
            "corpus_sha256": sha256_file(args.corpus),
            "shortlists_sha256": sha256_file(args.shortlists),
            "surface_sha256": sha256_file(args.surface),
            "note": ("frozen classifier shortlists (bge-small-en-v1.5, "
                     "enriched-v1 template) + deterministic pin-set; the "
                     "classifier never runs inside the bench"),
        },
        "pins": pins,
        "inventory_text": inventory_text,
        "cells": cells,
    }
    with open(args.out, "w") as f:
        json.dump(fixture, f, indent=1)
    print(f"wrote fixture: {len(cells)} cells, pins={pins} -> {args.out}")


def cmd_check_ledger(args):
    fixture = json.load(open(args.fixture))
    cells = fixture["cells"]
    files = sorted(glob.glob(os.path.join(args.ledger, "tee-*.jsonl")))
    agent_reqs = hits = 0
    miss_prompts = {}
    for path in files:
        for row in load_jsonl(path):
            req = row.get("request")
            if not isinstance(req, dict) or not req.get("tools"):
                continue
            agent_reqs += 1
            prompt = None
            for m in req.get("messages") or []:
                if m.get("role") == "user":
                    c = m.get("content")
                    if isinstance(c, list):
                        c = "".join(p.get("text", "") for p in c if isinstance(p, dict))
                    if isinstance(c, str) and c.strip():
                        prompt = c
                        break
            key = hashlib.sha256((prompt or "").encode()).hexdigest()
            if key in cells:
                hits += 1
            else:
                miss_prompts[(prompt or "")[:100]] = \
                    miss_prompts.get((prompt or "")[:100], 0) + 1
    print(f"ledger agent requests: {agent_reqs}; fixture hits: {hits}; "
          f"misses: {agent_reqs - hits}")
    for p, n in sorted(miss_prompts.items(), key=lambda kv: -kv[1])[:10]:
        print(f"  MISS x{n}: {p!r}")
    sys.exit(0 if agent_reqs == hits else 1)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    q = sub.add_parser("make-queries")
    q.add_argument("--corpus", required=True)
    q.add_argument("--out", required=True)
    q.set_defaults(fn=cmd_make_queries)

    fx = sub.add_parser("make-fixture")
    fx.add_argument("--corpus", required=True)
    fx.add_argument("--shortlists", required=True)
    fx.add_argument("--surface", required=True)
    fx.add_argument("--out", required=True)
    fx.set_defaults(fn=cmd_make_fixture)

    ck = sub.add_parser("check-ledger")
    ck.add_argument("--fixture", required=True)
    ck.add_argument("--ledger", required=True)
    ck.set_defaults(fn=cmd_check_ledger)

    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
