#!/usr/bin/env python3
"""Three-way bench comparison over the judged contract stores:
stock (full catalog) vs narrowed (classifier shortlist) vs furnished
(shortlist + schema furnishing on by-name calls).

Deterministic pairing: restrict to cells present in ALL judged stores, tally
judge buckets per tier and per cell, focus the elmer/elmer-ultra tiers (the
diagnostic-regression read), list narrowed-vs-furnished disagreements, and
measure per-arm by-name traffic + invalid_args bounces split by first-use
(schema-blind in both arms) vs later-use (schema furnished only in the
furnished arm — the treatment seam).

Run from the Pi (reads stores + bundles over ssh):
    python3 analyze_bench_ab.py [--host r2-poe]
"""
import argparse
import collections
import json
import re
import statistics
import subprocess

STOCK = "~/bench-overnight/inkling-v25-full"
NARROW = "~/bench-overnight/inkling-v25-narrowed"
FURNISH = "~/bench-overnight/inkling-v25-furnish"
STORE = "judgments.contract-v25-silent-cannot-false-succeed.gpt-5.6-luna-high.jsonl"
CORPUS = "~/bench-dev-mtr/dev/ladder-v2/shakeout-corpus.json"
FIXTURE = "~/bench-overnight/narrow-fixture.json"

ARMS = [("stock", STOCK), ("narrowed", NARROW), ("furnished", FURNISH)]
BUCKET_ORDER = ["delivered", "delivered_with_defects", "honest_shortfall", "unreliable"]


def sh(host, cmd):
    return subprocess.run(["ssh", "-o", "BatchMode=yes", host, cmd],
                          capture_output=True, text=True, timeout=180).stdout


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


def load_fixture(host):
    fixture = json.loads(sh(host, f"cat {FIXTURE}"))
    short_by_cell = {v["cell"]: {c["id"] for c in v["shortlist"]}
                     for v in fixture["cells"].values()}
    for v in fixture["cells"].values():
        for alias in v.get("aliases", []):
            short_by_cell[alias] = {c["id"] for c in v["shortlist"]}
    return short_by_cell, set(fixture.get("pins", []))


def traffic(host, run, label, short_by_cell, pins):
    """By-name reachability + invalid_args bounce profile for one arm.

    first-use = the first call to a given outside-shortlist tool within one
    attempt (no schema in either arm — furnishing triggers on the NEXT
    request); later-use = subsequent calls (schema present only in the
    furnished arm). The later-use bounce-rate delta is the treatment effect.
    """
    # grep '' prefixes every line with its file path — cell parsed from the
    # path, JSON from the remainder. No quoting gymnastics.
    raw = sh(host, f"grep '' {run}/base/*/attempt-*/tool_calls.jsonl")
    total = outside = pin_calls = invalid_total = 0
    outside_names = collections.Counter()
    invalid_by_tool = collections.Counter()
    seen_in_attempt = set()
    use = {"first": [0, 0], "later": [0, 0]}  # class -> [calls, invalid]
    for line in raw.splitlines():
        path, _, payload = line.partition(":")
        try:
            r = json.loads(payload)
        except Exception:
            continue
        parts = path.split("/")
        cell = parts[-3] if len(parts) >= 3 else None
        attempt = parts[-2] if len(parts) >= 2 else None
        name = r.get("tool")
        if not cell or not name or cell not in short_by_cell:
            continue
        total += 1
        bounced = r.get("status") == "invalid_args"
        if bounced:
            invalid_total += 1
            invalid_by_tool[name] += 1
        if name in pins and name not in short_by_cell[cell]:
            pin_calls += 1
        elif name not in short_by_cell[cell]:
            outside += 1
            outside_names[name] += 1
            key = (cell, attempt, name)
            cls = "later" if key in seen_in_attempt else "first"
            seen_in_attempt.add(key)
            use[cls][0] += 1
            use[cls][1] += bounced
    print(f"\n== {label}-arm tool traffic ==\n"
          f"  calls: {total} | to pinned-only tools: {pin_calls} | "
          f"outside shortlist+pins (by-name lazy calls): {outside} | "
          f"invalid_args (all calls): {invalid_total}")
    for n, k in outside_names.most_common(12):
        print(f"    {n}: {k}  (invalid_args: {invalid_by_tool.get(n, 0)})")
    for cls in ("first", "later"):
        calls, inv = use[cls]
        rate = f"{inv/calls:5.1%}" if calls else "  n/a"
        print(f"  by-name {cls}-use: {calls} calls, {inv} invalid_args ({rate})")
    return total, invalid_total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="r2-poe")
    args = ap.parse_args()

    tier = {p["id"]: p["tier"]
            for p in json.loads(sh(args.host, f"cat {CORPUS}"))["prompts"]}
    stores = {label: load_store(args.host, run) for label, run in ARMS}
    cells = [({c for c, _ in s}) for s in stores.values()]
    both = sorted(set.intersection(*cells))
    print("  ".join(f"{label}: {len(s)} rows / {len({c for c, _ in s})} cells"
                    for label, s in stores.items()))
    print(f"paired cells (in all {len(ARMS)} stores): {len(both)}\n")

    def tally(store, cellset):
        per_tier = collections.defaultdict(collections.Counter)
        for (c, a), b in store.items():
            if c in cellset:
                per_tier[tier.get(c, "?")][b] += 1
        return per_tier

    tallies = {label: tally(s, set(both)) for label, s in stores.items()}
    print("== paired per-tier bucket tables (stock -> narrowed -> furnished) ==")
    for t in ("task-rabbit", "assistant", "collaborator", "elmer", "elmer-ultra"):
        if not any(t in tl for tl in tallies.values()):
            continue
        rows = {label: tl.get(t, {}) for label, tl in tallies.items()}
        tots = {label: sum(r.values()) for label, r in rows.items()}
        print(f"{t:12s} n=" + "->".join(str(tots[l]) for l, _ in ARMS) + "  "
              + "  ".join(f"{b}:" + "->".join(str(rows[l].get(b, 0)) for l, _ in ARMS)
                          for b in BUCKET_ORDER))
        if all(tots.values()):
            print(f"{'':12s} delivered-rate "
                  + " -> ".join(f"{rows[l].get('delivered', 0)/tots[l]:5.1%}"
                                for l, _ in ARMS))

    print("\n== THE READ: elmer + elmer-ultra cells, per-arm buckets ==")
    for t in ("elmer", "elmer-ultra"):
        for c in both:
            if tier.get(c) != t:
                continue
            per_arm = []
            for label, s in stores.items():
                attempts = sorted(a for (cc, a) in s if cc == c)
                per_arm.append(f"{label}={sorted(s[(c, a)] for a in attempts)}")
            print(f"  {c:28s} [{t:11s}] " + "  ".join(per_arm))

    print("\n== per-cell disagreements (common attempts, narrowed vs furnished) ==")
    narrow, furnish = stores["narrowed"], stores["furnished"]
    diffs = 0
    for c in both:
        attempts = sorted({a for (cc, a) in narrow if cc == c}
                          & {a for (cc, a) in furnish if cc == c})
        nb = sorted(narrow[(c, a)] for a in attempts)
        fb = sorted(furnish[(c, a)] for a in attempts)
        if nb != fb:
            diffs += 1
            print(f"  {c:28s} [{tier.get(c, '?'):12s}] narrowed={nb} furnished={fb}")
    print(f"  ({diffs} of {len(both)} paired cells differ)")

    short_by_cell, pins = load_fixture(args.host)
    for label, run in ARMS[1:]:
        traffic(args.host, run, label, short_by_cell, pins)

    # Wall-clock per unit from the runner logs, paired on cells all arms
    # completed. Caveats carried in FINDINGS prose: narrowed's early units ran
    # concurrent with the v4 battery; the furnished run spans the serving
    # crash + resume. Indicative, not a controlled timing experiment.
    pat = re.compile(r"base-(.+)-\d+ → \w+ \((\d+)s\)")
    def durations(log):
        out = collections.defaultdict(list)
        for m in pat.finditer(sh(args.host, f"cat {log} 2>/dev/null")):
            out[m.group(1)].append(int(m.group(2)))
        return out
    logs = {"stock": f"{STOCK}.log",
            "narrowed": "~/bench-overnight/inkling-v25-narrowed.log",
            "furnished": "~/bench-overnight/inkling-v25-furnish.log"}
    durs = {label: durations(log) for label, log in logs.items()}
    paired = sorted(set.intersection(*(set(d) for d in durs.values())))
    if paired:
        print(f"\n== wall-clock per unit, paired cells ({len(paired)}) ==")
        for label in logs:
            xs = [x for c in paired for x in durs[label][c]]
            print(f"  {label:9s} median {statistics.median(xs):5.1f}s  "
                  f"mean {statistics.mean(xs):5.1f}s  (n={len(xs)})")


if __name__ == "__main__":
    main()
