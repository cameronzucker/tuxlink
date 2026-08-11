#!/usr/bin/env python3
"""Inkling recovery spike — does the safety net actually work? (operator
directive 2026-08-10: "Test it now"; hard requirement: Inkling can always
get past the classifier to the full toolset.)

Two presentation conditions against the SERVED Inkling, using REAL
classifier shortlists (dumped by eval_tools from the calibrated corpus):

  A narrowed-only  : top-12 shortlist, nothing else. Diagnostic — what
                     happens on a miss with no recovery path?
  B safety-net     : top-12 shortlist as advisory + the full 92-tool name
                     list (one-liners), any tool choosable by name. The
                     shipping candidate; the reachability requirement made
                     testable.

Query buckets from the labeled set (via shortlists.jsonl):
  MISS  (3): correct tool NOT in the top-12 — the recovery probes.
  HIT  (12): correct tool in top-12, spread across tiers — non-regression.
  NONE  (4): no tool should be chosen — does the net induce fabrication?

Reply grammar (proven style from the parseability spike): the model must
answer `TOOL: <name>` or `NONE`. Inkling wire shape: reasoning streams
first, grade message.content only, generous max_tokens.

Usage: python3 spike.py [--out results/]   (stdlib only; run from repo root
or the spike dir — paths resolve relative to this file)
"""

import argparse
import json
import re
import time
import urllib.request
from pathlib import Path

ENDPOINT = "https://inference.twin-bramble.ts.net/v1/chat/completions"
MODEL = "inkling-small-nvfp4"
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SHORTLISTS = ROOT / "dev/spikes/2026-08-10-tool-surface-embedding/shortlists.jsonl"
CORPUS = ROOT / "src-tauri/resources/agents/tool-surface.jsonl"

HIT_SAMPLE = [
    "armed-check", "inbox-list", "read-msg", "catalog-request", "cms-dial",
    "prop-check", "ardop-mail", "vara-drive", "ft8-band", "grid-set",
    "routine-run", "tune-radio",
]

SYSTEM = (
    "You are Tuxlink's assistant. Tuxlink is a Linux Winlink client for "
    "amateur radio. Your ONLY job here: choose the single best tool for the "
    "operator's request, or decline.\n"
    "Reply with EXACTLY one line, nothing else:\n"
    "TOOL: <tool_name>   (the one tool that handles the request)\n"
    "NONE                (no tool applies; the request is not a tool task)\n"
)

NET_PREAMBLE = (
    "A classifier suggests these tools as most likely relevant (it can be "
    "wrong):\n{shortlist}\n\n"
    "FULL TOOL LIST — you may choose ANY tool below by name, not just the "
    "suggestions:\n{full}\n"
)
NARROW_PREAMBLE = "Available tools:\n{shortlist}\n"


def load_jsonl(path):
    out = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            out.append(json.loads(line))
    return out


def chat(system, user, max_tokens=1600, retries=2):
    body = json.dumps({
        "model": MODEL,
        "max_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
    }).encode()
    for attempt in range(retries + 1):
        try:
            req = urllib.request.Request(
                ENDPOINT, data=body,
                headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=180) as r:
                msg = json.loads(r.read())["choices"][0]["message"]
                return msg.get("content")
        except Exception as e:  # noqa: BLE001 - spike: record, retry, move on
            if attempt == retries:
                return f"__ERROR__ {e}"
            time.sleep(3)


def parse_reply(content):
    if content is None:
        return ("null", None)
    m = re.search(r"TOOL:\s*([a-z0-9_]+)", content)
    if m:
        return ("tool", m.group(1))
    if re.search(r"\bNONE\b", content):
        return ("none", None)
    return ("unparsed", None)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(HERE / "results"))
    args = ap.parse_args()
    outdir = Path(args.out)
    outdir.mkdir(parents=True, exist_ok=True)

    rows = load_jsonl(SHORTLISTS)
    corpus = {e["id"]: e for e in load_jsonl(CORPUS)}
    full_list = "\n".join(
        f"- {e['id']}: {e['title']}" for e in sorted(corpus.values(), key=lambda e: e["id"])
    )

    def bucket(r):
        if r["kind"] == "none":
            return "none"
        if r["kind"] == "tool":
            in12 = r["rank"] is not None and r["rank"] <= 12
            if not in12:
                return "miss"
            if r["id"] in HIT_SAMPLE:
                return "hit"
        return None

    cases = [(bucket(r), r) for r in rows]
    cases = [(b, r) for b, r in cases if b]
    results = []
    tally = {}

    for cond in ("narrowed", "safety-net"):
        for b, r in cases:
            short12 = r["shortlist"][:12]
            shortlist_txt = "\n".join(f"- {c['id']}: {c['title']}" for c in short12)
            if cond == "narrowed":
                user = NARROW_PREAMBLE.format(shortlist=shortlist_txt)
            else:
                user = NET_PREAMBLE.format(shortlist=shortlist_txt, full=full_list)
            user += f"\nOperator request: {r['q']}"
            content = chat(SYSTEM, user)
            kind, picked = parse_reply(content)
            correct = set(r.get("correct") or [])
            if b == "none":
                ok = kind == "none"
            else:
                ok = kind == "tool" and picked in correct
            fabricated = kind == "tool" and picked not in corpus
            results.append({
                "cond": cond, "bucket": b, "id": r["id"], "q": r["q"],
                "correct": sorted(correct), "picked": picked, "kind": kind,
                "ok": ok, "fabricated": fabricated, "content": content,
            })
            key = (cond, b)
            n_ok, n = tally.get(key, (0, 0))
            tally[key] = (n_ok + (1 if ok else 0), n + 1)
            print(f"{cond:10s} {b:5s} {r['id']:18s} -> {kind}:{picked} "
                  f"{'OK' if ok else 'x'}{' FABRICATED' if fabricated else ''}")

    (outdir / "rows.jsonl").write_text(
        "\n".join(json.dumps(x) for x in results) + "\n")
    print("\n---- summary (ok/total) ----")
    for (cond, b), (n_ok, n) in sorted(tally.items()):
        print(f"{cond:10s} {b:5s}: {n_ok}/{n}")
    fab = sum(1 for x in results if x["fabricated"])
    print(f"fabricated tool names: {fab}/{len(results)}")


if __name__ == "__main__":
    main()
