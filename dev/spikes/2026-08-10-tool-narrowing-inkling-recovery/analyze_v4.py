#!/usr/bin/env python3
"""v4 battery analysis: paired condition effects, recovery, latency, reasoning.

Deterministic. Reads results-v4/<label>/rows.jsonl (any completeness), prints:
1. ok/total per (condition, bucket) with per-rep spread — condition effects
   read against run-to-run variance, the FINDINGS-v3 discipline.
2. PAIRED per-case view: for each case, per-condition ok-counts across reps;
   lists every case where conditions disagree (the cases worth reading).
3. Recovery table: the stale-shortlist two-turn cases + shortlist-miss cases,
   per condition — the narrowing mechanism's live-or-die rows.
4. Outside-array + pin-set absorption: how many outside-array calls the pins
   converted to in-array (narrowed-net vs narrowed-pinned).
5. Latency + prompt-size medians per condition (the wall-clock worth-it case).
6. Reasoning digest: for failures, dump reasoning/content excerpts to
   failures-<label>.md for the qualitative read.

Usage: python3 analyze_v4.py --label inkling-v4
"""
import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(label):
    rows = []
    for line in (HERE / "results-v4" / label / "rows.jsonl").read_text().splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", default="inkling-v4")
    args = ap.parse_args()
    rows = load(args.label)
    conds = sorted({r["cond"] for r in rows},
                   key=lambda c: ("everything", "narrowed-net", "narrowed-pinned").index(c)
                   if c in ("everything", "narrowed-net", "narrowed-pinned") else 9)
    reps = sorted({r["rep"] for r in rows})
    print(f"{len(rows)} rows, conditions {conds}, reps {reps}\n")

    # 1. condition × bucket with per-rep spread
    print("== ok/total per condition × bucket (per-rep in brackets) ==")
    buckets = ["hit", "miss", "multiturn", "none", "ambig"]
    for b in buckets:
        line = f"{b:9s}"
        for c in conds:
            sub = [r for r in rows if r["cond"] == c and r["bucket"] == b]
            if not sub:
                continue
            per_rep = []
            for rep in reps:
                rr = [r for r in sub if r["rep"] == rep]
                if rr:
                    per_rep.append(f"{sum(r['ok'] for r in rr)}/{len(rr)}")
            line += f" | {c}: {sum(r['ok'] for r in sub)}/{len(sub)} [{' '.join(per_rep)}]"
        print(line)

    # 2. paired per-case disagreements
    print("\n== cases where conditions disagree (ok-count across reps) ==")
    by_case = defaultdict(lambda: defaultdict(list))
    for r in rows:
        by_case[(r["bucket"], r["id"])][r["cond"]].append(r["ok"])
    n_disagree = 0
    for (b, cid), per_cond in sorted(by_case.items()):
        counts = {c: f"{sum(v)}/{len(v)}" for c, v in per_cond.items()}
        fracs = {c: sum(v) / len(v) for c, v in per_cond.items()}
        if len({round(f, 2) for f in fracs.values()}) > 1:
            n_disagree += 1
            print(f"  {b:9s} {cid:24s} " +
                  "  ".join(f"{c}={counts[c]}" for c in conds if c in counts))
    print(f"  ({n_disagree} disagreeing cases of {len(by_case)})")

    # 3. recovery rows
    print("\n== stale-shortlist recovery (two-turn, target OUTSIDE turn-1 top-12) ==")
    for (b, cid), per_cond in sorted(by_case.items()):
        sample = next(r for r in rows if r["id"] == cid and r["bucket"] == b)
        if b == "multiturn" and sample.get("recovery"):
            counts = {c: f"{sum(v)}/{len(v)}" for c, v in per_cond.items()}
            print(f"  {cid:24s} " +
                  "  ".join(f"{c}={counts[c]}" for c in conds if c in counts))
    print("== shortlist-miss single-turns (correct tool not in top-12) ==")
    for (b, cid), per_cond in sorted(by_case.items()):
        if b == "miss":
            counts = {c: f"{sum(v)}/{len(v)}" for c, v in per_cond.items()}
            print(f"  {cid:24s} " +
                  "  ".join(f"{c}={counts[c]}" for c in conds if c in counts))

    # 4. outside-array + pin absorption
    print("\n== outside-array calls (emitted / correct) ==")
    for c in conds:
        oa = [r for r in rows if r["cond"] == c and r["outside_array"]]
        print(f"  {c:15s}: {len(oa)} emitted, {sum(r['ok'] for r in oa)} correct")
    pin_ids = {"server_info", "docs_search", "cms_abort",
               "modem_ardop_disconnect", "vara_stop_session"}
    net_rows = [r for r in rows if r["cond"] == "narrowed-net"
                and r["picked"] in pin_ids and r["outside_array"]]
    print(f"  pin-absorbable outside-array calls under narrowed-net: {len(net_rows)}"
          f" (these become in-array under narrowed-pinned)")

    # 5. latency / prompt medians
    print("\n== medians per condition (non-error rows) ==")
    for c in conds:
        sub = [r for r in rows if r["cond"] == c and not r.get("error")]
        lat = [r["latency_s"] for r in sub if r.get("latency_s")]
        tok = [r["prompt_tokens"] for r in sub if r.get("prompt_tokens")]
        gen = [r["completion_tokens"] for r in sub if r.get("completion_tokens")]
        print(f"  {c:15s}: latency {statistics.median(lat):5.1f}s | prompt "
              f"{int(statistics.median(tok)):6d} tok | completion "
              f"{int(statistics.median(gen)):4d} tok")
    errs = sum(1 for r in rows if r.get("error"))
    fab = sum(1 for r in rows if r.get("fabricated"))
    rsn = sum(1 for r in rows if r.get("reasoning"))
    print(f"\nerrors: {errs}  fabricated: {fab}  rows-with-reasoning: {rsn}/{len(rows)}")

    # 6. failure reasoning digest
    out = HERE / "results-v4" / args.label / f"failures-{args.label}.md"
    with open(out, "w") as f:
        f.write(f"# v4 failure digest — {args.label}\n\n"
                "Every non-ok row, grouped by case, with picked tool, reasoning\n"
                "and content excerpts. The qualitative instrument for 'what does\n"
                "Inkling need to work in harmony with the classifier'.\n")
        for (b, cid), _ in sorted(by_case.items()):
            fails = [r for r in rows if r["id"] == cid and r["bucket"] == b
                     and not r["ok"]]
            if not fails:
                continue
            f.write(f"\n## {b} / {cid}\n")
            for r in fails:
                f.write(f"\n### rep {r['rep']} · {r['cond']} · picked="
                        f"{r['picked']} (correct {r['correct']})"
                        f"{' OUTSIDE-ARRAY' if r['outside_array'] else ''}\n")
                if r.get("reasoning"):
                    f.write("\nREASONING:\n```\n" + r["reasoning"][:2500] + "\n```\n")
                if r.get("content"):
                    f.write("\nCONTENT:\n```\n" + str(r["content"])[:1200] + "\n```\n")
                calls = r.get("tool_calls") or []
                if calls:
                    f.write("\nCALLS: " + json.dumps(calls)[:600] + "\n")
    print(f"failure digest -> {out}")


if __name__ == "__main__":
    main()
