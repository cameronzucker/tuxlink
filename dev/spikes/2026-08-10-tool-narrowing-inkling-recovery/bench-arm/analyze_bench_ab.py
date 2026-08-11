#!/usr/bin/env python3
"""Stock-vs-narrowed bench A/B over the judged contract stores.

Deterministic pairing: restrict to cells present in BOTH stores (the narrowed
run's gate already excluded debt cells), tally judge buckets per tier and per
cell, list every cell whose bucket distribution differs, and measure the
narrowed arm's outside-shortlist reachability from its tool_calls records.

Run from the Pi (reads both stores + bundles over ssh):
    python3 analyze_bench_ab.py [--host r2-poe] [--partial]
"""
import argparse
import collections
import json
import re
import statistics
import subprocess

STOCK = "~/bench-overnight/inkling-v25-full"
NARROW = "~/bench-overnight/inkling-v25-narrowed"
STORE = "judgments.contract-v25-silent-cannot-false-succeed.gpt-5.6-luna-high.jsonl"
CORPUS = "~/bench-dev-mtr/dev/ladder-v2/shakeout-corpus.json"
FIXTURE = "~/bench-overnight/narrow-fixture.json"

BUCKET_ORDER = ["delivered", "delivered_with_defects", "honest_shortfall", "unreliable"]


def sh(host, cmd):
    return subprocess.run(["ssh", "-o", "BatchMode=yes", host, cmd],
                          capture_output=True, text=True, timeout=120).stdout


def load_store(host, run):
    rows = []
    for line in sh(host, f"cat {run}/{STORE} 2>/dev/null").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    out = {}
    for r in rows:
        parts = r["id"].split("/")
        cell = parts[1] if len(parts) > 2 else r["id"]
        attempt = parts[-1]
        out[(cell, attempt)] = r["bucket"]
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="r2-poe")
    args = ap.parse_args()

    tier = {p["id"]: p["tier"]
            for p in json.loads(sh(args.host, f"cat {CORPUS}"))["prompts"]}
    stock = load_store(args.host, STOCK)
    narrow = load_store(args.host, NARROW)
    cells_n = {c for c, _ in narrow}
    cells_s = {c for c, _ in stock}
    both = sorted(cells_n & cells_s)
    print(f"stock judged: {len(stock)} rows / {len(cells_s)} cells; "
          f"narrowed judged: {len(narrow)} rows / {len(cells_n)} cells; "
          f"paired cells: {len(both)}\n")

    def tally(store, cells):
        per_tier = collections.defaultdict(collections.Counter)
        for (c, a), b in store.items():
            if c in cells:
                per_tier[tier.get(c, "?")][b] += 1
        return per_tier

    ts, tn = tally(stock, set(both)), tally(narrow, set(both))
    print("== paired per-tier bucket tables (stock -> narrowed) ==")
    for t in ("task-rabbit", "assistant", "collaborator", "elmer", "elmer-ultra"):
        if t not in ts and t not in tn:
            continue
        s, n = ts.get(t, {}), tn.get(t, {})
        stot, ntot = sum(s.values()), sum(n.values())
        print(f"{t:12s} n={stot}->{ntot}  " + "  ".join(
            f"{b}:{s.get(b,0)}->{n.get(b,0)}" for b in BUCKET_ORDER))
        if stot and ntot:
            print(f"{'':12s} delivered-rate {s.get('delivered',0)/stot:5.1%} -> "
                  f"{n.get('delivered',0)/ntot:5.1%}")

    print("\n== per-cell disagreements (attempt buckets, stock vs narrowed) ==")
    diffs = 0
    for c in both:
        sb = sorted(b for (cc, a), b in stock.items() if cc == c)
        nb = sorted(b for (cc, a), b in narrow.items() if cc == c)
        if sb != nb:
            diffs += 1
            print(f"  {c:28s} [{tier.get(c,'?'):12s}] stock={sb} narrowed={nb}")
    print(f"  ({diffs} of {len(both)} paired cells differ)")

    # Outside-shortlist reachability in the narrowed arm's real agentic flow.
    fixture = json.loads(sh(args.host, f"cat {FIXTURE}"))
    short_by_cell = {v["cell"]: {c["id"] for c in v["shortlist"]}
                     for v in fixture["cells"].values()}
    for v in fixture["cells"].values():
        for alias in v.get("aliases", []):
            short_by_cell[alias] = {c["id"] for c in v["shortlist"]}
    pins = set(fixture.get("pins", []))
    # grep '' prefixes every line with its file path — cell parsed from the
    # path, JSON from the remainder. No quoting gymnastics.
    raw = sh(args.host, f"grep '' {NARROW}/base/*/attempt-*/tool_calls.jsonl")
    total = outside = pin_calls = 0
    outside_names = collections.Counter()
    for line in raw.splitlines():
        path, _, payload = line.partition(":")
        try:
            r = json.loads(payload)
        except Exception:
            continue
        parts = path.split("/")
        cell = parts[-3] if len(parts) >= 3 else None
        name = r.get("tool")
        if not cell or not name or cell not in short_by_cell:
            continue
        total += 1
        if name in pins and name not in short_by_cell[cell]:
            pin_calls += 1
        elif name not in short_by_cell[cell]:
            outside += 1
            outside_names[name] += 1
    print(f"\n== narrowed-arm tool traffic ==\n"
          f"  calls: {total} | to pinned-only tools: {pin_calls} | "
          f"outside shortlist+pins (by-name lazy calls): {outside}")
    for n, k in outside_names.most_common(12):
        print(f"    {n}: {k}")

    # Wall-clock worth-it, real-agentic-loop version: per-unit durations from
    # the two runner logs, paired on the cells both arms completed.
    pat = re.compile(r"base-(.+)-\d+ → \w+ \((\d+)s\)")
    def durations(log):
        out = collections.defaultdict(list)
        for m in pat.finditer(sh(args.host, f"cat {log} 2>/dev/null")):
            out[m.group(1)].append(int(m.group(2)))
        return out
    ds = durations(f"{STOCK}.log")
    dn = durations("~/bench-overnight/inkling-v25-narrowed.log")
    paired = sorted(set(ds) & set(dn))
    if paired:
        s_all = [x for c in paired for x in ds[c]]
        n_all = [x for c in paired for x in dn[c]]
        print(f"\n== wall-clock per unit, paired cells ({len(paired)}) ==\n"
              f"  stock   median {statistics.median(s_all):5.1f}s  mean {statistics.mean(s_all):5.1f}s\n"
              f"  narrowed median {statistics.median(n_all):5.1f}s  mean {statistics.mean(n_all):5.1f}s")
        per_tier = collections.defaultdict(lambda: ([], []))
        for c in paired:
            per_tier[tier.get(c, "?")][0].extend(ds[c])
            per_tier[tier.get(c, "?")][1].extend(dn[c])
        for t, (a, b) in sorted(per_tier.items()):
            print(f"  {t:12s} stock {statistics.median(a):5.1f}s -> "
                  f"narrowed {statistics.median(b):5.1f}s  (n={len(a)}/{len(b)})")


if __name__ == "__main__":
    main()
