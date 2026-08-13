#!/usr/bin/env python3
"""Readback-style judge driver (tuxlink-k2h9l) — PLAN-BASED transports only.

Feeds each generated case (see tuxlink-routines/examples/readback_eval_gen.rs)
to reader models: the model plays the OPERATOR who made the request, sees the
app's readback of what was actually saved, and must say whether they match and
what differs. Per-style divergence-detection and false-alarm rates decide the
slice (b) wording (tuxlink-fb0hc) on evidence.

Transports (operator ruling 2026-08-13: judges use PLAN-billed capacity, never
API-metered endpoints — one runaway session has blown whole budgets before,
and unpinned OpenRouter serving is unreliable; there is deliberately no
API-key path in this script):

  codex   — `codex exec --ephemeral` on the ChatGPT plan (gpt-5.6-luna).
  claude  — `claude -p --model sonnet` on the Claude plan.
  local   — an OpenAI-compatible LOCAL endpoint with no auth (e.g. the DGX
            Spark at https://inference.twin-bramble.ts.net); refuses
            non-private hosts.

Usage:
  python3 judge.py cases.jsonl \
      --judge codex:gpt-5.6-luna:medium --judge claude:sonnet \
      --samples 3 --out verdicts.jsonl
"""

import argparse
import collections
import concurrent.futures
import json
import subprocess
import sys
import time
import urllib.parse
import urllib.request

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

CORRECTION = (
    "\n\nCORRECTION: your prior reply did not contain the required JSON object. "
    'Reply with ONLY {"matches": true|false, "differences": [...]} and nothing else.'
)

QUOTA_MARKERS = ("usage limit", "at capacity", "rate limit")


def extract_verdict(text: str):
    """Pull the LAST {"matches": ...} object out of possibly-noisy stdout."""
    idx = max(text.rfind('{"matches"'), text.rfind('{ "matches"'))
    if idx < 0:
        return None
    depth = 0
    for j, ch in enumerate(text[idx:], start=idx):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                try:
                    parsed = json.loads(text[idx : j + 1])
                except json.JSONDecodeError:
                    return None
                return {
                    "matches": bool(parsed.get("matches")),
                    "differences": [str(d) for d in parsed.get("differences", [])],
                }
    return None


class Judge:
    """One reader: spec strings like codex:gpt-5.6-luna:medium /
    claude:sonnet / local:https://host/v1:model-name."""

    def __init__(self, spec: str):
        parts = spec.split(":", 1)
        self.kind = parts[0]
        if self.kind == "codex":
            rest = (parts[1] if len(parts) > 1 else "gpt-5.6-luna:medium").split(":")
            self.model = rest[0] or "gpt-5.6-luna"
            self.effort = rest[1] if len(rest) > 1 else "medium"
            self.label = f"codex/{self.model}@{self.effort}"
        elif self.kind == "claude":
            self.model = parts[1] if len(parts) > 1 else "sonnet"
            self.label = f"claude/{self.model}"
        elif self.kind == "local":
            rest = parts[1].rsplit(":", 1)
            self.base, self.model = rest[0], rest[1]
            host = urllib.parse.urlparse(self.base).hostname or ""
            if not (
                host in ("localhost", "127.0.0.1")
                or host.endswith(".ts.net")
                or host.startswith(("10.", "192.168."))
            ):
                raise ValueError(f"local transport refuses non-private host {host!r}")
            self.label = f"local/{self.model}"
        else:
            raise ValueError(f"unknown judge kind {self.kind!r}")

    def ask(self, prompt: str) -> str:
        if self.kind == "codex":
            cmd = [
                "codex", "exec", "--ephemeral", "--sandbox", "read-only",
                "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules",
                "--model", self.model,
                "--config", f'model_reasoning_effort="{self.effort}"', "-",
            ]
            r = subprocess.run(cmd, input=prompt, capture_output=True, text=True, timeout=300)
            out = (r.stdout or "") + "\n" + (r.stderr or "")
            if r.returncode != 0 and any(m in out.lower() for m in QUOTA_MARKERS):
                raise QuotaError(out[:200])
            return r.stdout or ""
        if self.kind == "claude":
            cmd = ["claude", "-p", "--model", self.model]
            r = subprocess.run(cmd, input=prompt, capture_output=True, text=True, timeout=300)
            out = (r.stdout or "") + "\n" + (r.stderr or "")
            if r.returncode != 0 and any(m in out.lower() for m in QUOTA_MARKERS):
                raise QuotaError(out[:200])
            return r.stdout or ""
        body = json.dumps({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.3,
            "max_tokens": 500,
        }).encode()
        req = urllib.request.Request(
            f"{self.base}/chat/completions", data=body,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=180) as resp:
            return json.load(resp)["choices"][0]["message"]["content"]


class QuotaError(RuntimeError):
    pass


def judge_one(case, judge: Judge, sample: int):
    prompt = PROMPT.format(request=case["request"], readback=case["readback"])
    verdict = None
    for attempt in range(4):
        try:
            text = judge.ask(prompt if attempt < 2 else prompt + CORRECTION)
        except QuotaError:
            # Plan quota: defer and retry, never skip and never downgrade
            # (the standing Codex-quota rule).
            time.sleep(90)
            continue
        verdict = extract_verdict(text)
        if verdict is not None:
            break
    if verdict is None:
        return {
            "case_id": case["case_id"], "style": case["style"],
            "mutation": case["mutation"], "clean": case["clean"],
            "model": judge.label, "sample": sample,
            "matches": None, "differences": [],
            "detected_loose": False, "detected_strict": False,
        }
    diffs_text = " ".join(verdict["differences"]).lower()
    detected_loose = verdict["matches"] is False
    detected_strict = detected_loose and any(
        k.lower() in diffs_text for k in case.get("detect_keys", [])
    )
    return {
        "case_id": case["case_id"], "style": case["style"],
        "mutation": case["mutation"], "clean": case["clean"],
        "model": judge.label, "sample": sample,
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
        print("   " + f"{'class':<18}" + "".join(f"{s:>8}" for s in ("A", "B", "C")))
        for cls in classes:
            line = f"   {cls:<18}"
            for style in ("A", "B", "C"):
                cell = [r for r in rows if r["style"] == style and r["mutation"] == cls]
                line += f"{rate(cell, lambda r: r['detected_strict']):>8.1f}" if cell else f"{'—':>8}"
            print(line)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cases")
    ap.add_argument("--judge", action="append", required=True,
                    help="codex[:model[:effort]] | claude[:model] | local:<base-url>:<model>")
    ap.add_argument("--samples", type=int, default=3)
    ap.add_argument("--out", default="verdicts.jsonl")
    ap.add_argument("--concurrency", type=int, default=5)
    args = ap.parse_args()

    judges = [Judge(spec) for spec in args.judge]
    cases = [json.loads(l) for l in open(args.cases) if l.strip()]
    jobs = [(c, j, s) for c in cases for j in judges for s in range(args.samples)]
    print(f"{len(cases)} cases × {len(judges)} judges × {args.samples} samples = {len(jobs)} calls")
    print("judges:", ", ".join(j.label for j in judges))

    verdicts = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = [pool.submit(judge_one, c, j, s) for (c, j, s) in jobs]
        for i, fut in enumerate(concurrent.futures.as_completed(futures), 1):
            try:
                verdicts.append(fut.result())
            except Exception as e:  # noqa: BLE001 — record and continue; rates need denominators
                print(f"call failed: {e}", file=sys.stderr)
            if i % 50 == 0:
                print(f"  {i}/{len(jobs)}", flush=True)

    with open(args.out, "w") as f:
        for v in verdicts:
            f.write(json.dumps(v) + "\n")
    unparsed = sum(1 for v in verdicts if v["matches"] is None)
    print(f"wrote {len(verdicts)} verdicts to {args.out} ({unparsed} unparseable)")
    summarize(verdicts)


if __name__ == "__main__":
    main()
