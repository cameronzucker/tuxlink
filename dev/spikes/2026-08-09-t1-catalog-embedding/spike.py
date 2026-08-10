#!/usr/bin/env python3
"""T1 encoder spike: embedding retrieval over the real Winlink catalog, on Pi CPU.

Phase-1 evidence for the five-classifier architecture (tuxlink-efk3k):
does a 22-33M-param sentence encoder, running on the Pi 5's CPU, retrieve
the right catalog item from a plain-language ask — and do cosine margins
separate "answer confidently" from "genuinely close, ask the operator"?

One model per process invocation so peak-RSS attribution is honest
(ru_maxrss is a process-lifetime high-water mark).

  spike.py run --model sentence-transformers/all-MiniLM-L6-v2
  spike.py report   # merge results/*.json into results/REPORT.md
"""

import argparse
import json
import os
import platform
import re
import resource
import statistics
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
CATALOG = Path(
    os.environ.get(
        "TUXLINK_CATALOG",
        HERE / "../../../src-tauri/resources/catalog/winlink-queries.txt",
    )
)
QUERIES = HERE / "queries.jsonl"
RESULTS = HERE / "results"
HOST_LABEL = os.environ.get("SPIKE_HOST_LABEL", platform.node() or "unknown")

# Item-text templates under test: how much of the pipe-record should be embedded.
TEMPLATES = {
    "desc": lambda s, i, d: d,
    "sec_desc": lambda s, i, d: f"{s.replace('_', ' ')}: {d}",
    "full": lambda s, i, d: f"{s.replace('_', ' ')} {i}: {d}",
}

# Asymmetric-retrieval prefixes some model families require.
MODEL_PREFIXES = {
    "intfloat/e5-small-v2": ("query: ", "passage: "),
    "BAAI/bge-small-en-v1.5": (
        "Represent this sentence for searching relevant passages: ",
        "",
    ),
}


def load_catalog():
    rows = []
    text = CATALOG.read_text(encoding="utf-8-sig")
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split("|")
        if len(parts) < 3:
            continue
        section, item_id, desc = parts[0], parts[1], parts[2]
        rows.append((section, item_id, desc))
    return rows


def load_queries():
    out = []
    for line in QUERIES.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            out.append(json.loads(line))
    return out


def rss_mib():
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024.0


def evaluate(model_name: str):
    from sentence_transformers import SentenceTransformer

    catalog = load_catalog()
    queries = load_queries()
    q_prefix, p_prefix = MODEL_PREFIXES.get(model_name, ("", ""))

    rss_before = rss_mib()
    t0 = time.perf_counter()
    model = SentenceTransformer(model_name, device="cpu")
    load_s = time.perf_counter() - t0
    rss_loaded = rss_mib()

    out = {
        "model": model_name,
        "host": HOST_LABEL,
        "n_items": len(catalog),
        "n_queries": len(queries),
        "load_s": round(load_s, 2),
        "rss_before_mib": round(rss_before, 1),
        "rss_loaded_mib": round(rss_loaded, 1),
        "templates": {},
    }

    for tname, render in TEMPLATES.items():
        item_texts = [p_prefix + render(s, i, d) for s, i, d in catalog]
        t0 = time.perf_counter()
        item_emb = model.encode(
            item_texts, batch_size=64, normalize_embeddings=True,
            show_progress_bar=False,
        )
        precompute_s = time.perf_counter() - t0

        # Warmup, then timed single-query encodes (the interactive path).
        model.encode([q_prefix + "warmup query"], normalize_embeddings=True)
        per_query_ms, records = [], []
        for q in queries:
            t0 = time.perf_counter()
            q_emb = model.encode(
                [q_prefix + q["q"]], normalize_embeddings=True,
                show_progress_bar=False,
            )[0]
            per_query_ms.append((time.perf_counter() - t0) * 1000)

            sims = item_emb @ q_emb
            order = sims.argsort()[::-1]
            top = [
                (catalog[j][0], catalog[j][1], float(sims[j]))
                for j in order[:5]
            ]
            margin = float(sims[order[0]] - sims[order[1]])

            expect = q["expect"]
            hit1 = hit5 = sec1 = False
            if expect["kind"] == "item":
                ids = set(expect["items"])
                hit1 = top[0][1] in ids
                hit5 = any(t[1] in ids for t in top)
                sec1 = top[0][0] == expect.get("section", top[0][0])
            elif expect["kind"] == "section":
                hit1 = top[0][0] == expect["section"]
                hit5 = any(t[0] == expect["section"] for t in top)
                sec1 = hit1
            records.append({
                "id": q["id"],
                "behavior": q["behavior"],
                "kind": expect["kind"],
                "hit1": hit1,
                "hit5": hit5,
                "sec1": sec1,
                "top1_sim": top[0][2],
                "margin": margin,
                "top5": [f"{s}/{i}:{sim:.3f}" for s, i, sim in top],
            })

        scored = [r for r in records if r["kind"] in ("item", "section")]
        answer_m = [r["margin"] for r in records if r["behavior"] == "answer"]
        ask_m = [r["margin"] for r in records if r["behavior"] == "ask"]
        none_s = [r["top1_sim"] for r in records if r["kind"] == "none"]
        match_s = [r["top1_sim"] for r in scored]
        out["templates"][tname] = {
            "precompute_s": round(precompute_s, 2),
            "query_ms_median": round(statistics.median(per_query_ms), 1),
            "query_ms_p95": round(
                sorted(per_query_ms)[int(0.95 * len(per_query_ms))], 1
            ),
            "top1": round(sum(r["hit1"] for r in scored) / len(scored), 3),
            "top5": round(sum(r["hit5"] for r in scored) / len(scored), 3),
            "sec1": round(sum(r["sec1"] for r in scored) / len(scored), 3),
            "margin_answer_median": round(statistics.median(answer_m), 4)
            if answer_m else None,
            "margin_ask_median": round(statistics.median(ask_m), 4)
            if ask_m else None,
            "top1sim_nomatch_max": round(max(none_s), 4) if none_s else None,
            "top1sim_match_min": round(min(match_s), 4) if match_s else None,
            "records": records,
        }
        out["rss_peak_mib"] = round(rss_mib(), 1)

    RESULTS.mkdir(exist_ok=True)
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", model_name).strip("-").lower()
    (RESULTS / f"{slug}.json").write_text(json.dumps(out, indent=1))
    print(f"wrote results/{slug}.json  rss_peak={out['rss_peak_mib']}MiB")


def report():
    rows = []
    for f in sorted(RESULTS.rglob("*.json")):
        rows.append(json.loads(f.read_text()))
    lines = [
        "# T1 catalog-embedding spike — results",
        "",
        f"Catalog: {rows[0]['n_items']} items; "
        f"queries: {rows[0]['n_queries']} labeled "
        "(hand-authored against the catalog itself; no bench corpus vendored).",
        "CPU-only on every host; host column identifies the machine.",
        "",
        "| host | model | template | top1 | top5 | sec1 | q ms (med/p95) | "
        "precompute s | RSS peak MiB | margin ans/ask | nomatch-vs-match sim |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        for tname, t in r["templates"].items():
            lines.append(
                f"| {r.get('host', 'pi5')} | {r['model']} | {tname} | "
                f"{t['top1']} | {t['top5']} | "
                f"{t['sec1']} | {t['query_ms_median']}/{t['query_ms_p95']} | "
                f"{t['precompute_s']} | {r['rss_peak_mib']} | "
                f"{t['margin_answer_median']}/{t['margin_ask_median']} | "
                f"max {t['top1sim_nomatch_max']} vs min {t['top1sim_match_min']} |"
            )
    (RESULTS / "REPORT.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    runp = sub.add_parser("run")
    runp.add_argument("--model", required=True)
    sub.add_parser("report")
    args = ap.parse_args()
    if args.cmd == "run":
        evaluate(args.model)
    else:
        report()
