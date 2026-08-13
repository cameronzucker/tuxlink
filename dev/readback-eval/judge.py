#!/usr/bin/env python3
"""Readback-style judge driver (tuxlink-k2h9l).

Feeds each generated case (see tuxlink-routines/examples/readback_eval_gen.rs)
to reader models: the model plays the OPERATOR who made the request, sees the
app's readback of what was actually saved, and must say whether they match and
what differs. Per-style divergence-detection and false-alarm rates decide the
slice (b) wording (tuxlink-fb0hc) on evidence.

Usage:
  python3 judge.py cases.jsonl --model <openrouter-id> [--model <id2>] \
      --samples 3 --out verdicts.jsonl

The OpenRouter key comes from the OS keyring:
  secret-tool lookup service elmer-openrouter account teacher
"""

import argparse
import collections
import concurrent.futures
import json
import re
import subprocess
import sys
import urllib.request

API = "https://openrouter.ai/api/v1/chat/completions"

PROMPT = """You are the operator of a ham-radio application. You asked the app \
to save (or edit) an automation routine, in these words:

REQUEST: {request}

The app replied with this summary of what it ACTUALLY saved:

SUMMARY: {readback}

Question: does the saved routine match what you asked for? Read carefully — \
summaries can differ from requests in small, consequential ways (a callsign, \
a time, a schedule, whether it may transmit without asking).

Answer with STRICT JSON only, no prose around it:
{{"matches": true|false, "differences": ["each concrete mismatch, naming the value that differs"]}}
If everything matches, "differences" must be []."""


def key() -> str:
    return subprocess.run(
        ["secret-tool", "lookup", "service", "elmer-openrouter", "account", "teacher"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()


def call(model: str, prompt: str, api_key: str) -> dict:
    body = json.dumps({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.3,
        "max_tokens": 500,
    }).encode()
    req = urllib.request.Request(API, data=body, headers={
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    })
    with urllib.request.urlopen(req, timeout=120) as resp:
        payload = json.load(resp)
    text = payload["choices"][0]["message"]["content"]
    m = re.search(r"\{.*\}", text, re.DOTALL)
    if not m:
        return {"matches": None, "differences": [], "raw": text}
    try:
        parsed = json.loads(m.group(0))
        return {"matches": bool(parsed.get("matches")),
                "differences": [str(d) for d in parsed.get("differences", [])],
                "raw": text}
    except (json.JSONDecodeError, TypeError):
        return {"matches": None, "differences": [], "raw": text}


def judge_one(case, model, sample, api_key):
    verdict = call(model, PROMPT.format(request=case["request"], readback=case["readback"]), api_key)
    diffs_text = " ".join(verdict["differences"]).lower()
    detected_loose = verdict["matches"] is False
    detected_strict = detected_loose and any(
        k.lower() in diffs_text for k in case.get("detect_keys", [])
    )
    return {
        "case_id": case["case_id"], "style": case["style"],
        "mutation": case["mutation"], "clean": case["clean"],
        "model": model, "sample": sample,
        "matches": verdict["matches"], "differences": verdict["differences"],
        "detected_loose": detected_loose, "detected_strict": detected_strict,
    }


def summarize(verdicts):
    def rate(rows, pred):
        rows = list(rows)
        return (sum(1 for r in rows if pred(r)) / len(rows) * 100) if rows else float("nan")

    print("\n=== BOARD (per model, per style) ===")
    by_model = collections.defaultdict(list)
    for v in verdicts:
        if v["matches"] is not None:
            by_model[v["model"]].append(v)
    for model, rows in by_model.items():
        print(f"\n-- {model} --")
        print(f"{'style':<6} {'detect-loose%':>13} {'detect-strict%':>14} {'false-alarm%':>13} {'n-dirty':>8} {'n-clean':>8}")
        for style in ("A", "B", "C"):
            dirty = [r for r in rows if r["style"] == style and not r["clean"]]
            clean = [r for r in rows if r["style"] == style and r["clean"]]
            print(f"{style:<6} {rate(dirty, lambda r: r['detected_loose']):>13.1f} "
                  f"{rate(dirty, lambda r: r['detected_strict']):>14.1f} "
                  f"{rate(clean, lambda r: r['detected_loose']):>13.1f} "
                  f"{len(dirty):>8} {len(clean):>8}")
        print("\n   per mutation class (strict detection %):")
        classes = sorted({r["mutation"] for r in rows if not r["clean"]})
        header = "   " + f"{'class':<18}" + "".join(f"{s:>8}" for s in ("A", "B", "C"))
        print(header)
        for cls in classes:
            line = f"   {cls:<18}"
            for style in ("A", "B", "C"):
                cell = [r for r in rows if r["style"] == style and r["mutation"] == cls]
                line += f"{rate(cell, lambda r: r['detected_strict']):>8.1f}" if cell else f"{'—':>8}"
            print(line)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cases")
    ap.add_argument("--model", action="append", required=True)
    ap.add_argument("--samples", type=int, default=3)
    ap.add_argument("--out", default="verdicts.jsonl")
    ap.add_argument("--concurrency", type=int, default=8)
    args = ap.parse_args()

    api_key = key()
    cases = [json.loads(l) for l in open(args.cases) if l.strip()]
    jobs = [(c, m, s) for c in cases for m in args.model for s in range(args.samples)]
    print(f"{len(cases)} cases × {len(args.model)} models × {args.samples} samples = {len(jobs)} calls")

    verdicts = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = [pool.submit(judge_one, c, m, s, api_key) for (c, m, s) in jobs]
        for i, fut in enumerate(concurrent.futures.as_completed(futures), 1):
            try:
                verdicts.append(fut.result())
            except Exception as e:  # noqa: BLE001 — record and continue; rates need denominators
                print(f"call failed: {e}", file=sys.stderr)
            if i % 50 == 0:
                print(f"  {i}/{len(jobs)}")

    with open(args.out, "w") as f:
        for v in verdicts:
            f.write(json.dumps(v) + "\n")
    print(f"wrote {len(verdicts)} verdicts to {args.out}")
    summarize(verdicts)


if __name__ == "__main__":
    main()
