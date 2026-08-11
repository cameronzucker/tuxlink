#!/usr/bin/env python3
"""ch3e9 step-1 spike: is an ENRICHED catalog index parseable/usable by Inkling?

Operator condition (ADR 0030 ruling 4): catalog index enrichment proceeds only
if it "passes muster with a spike against Inkling to ensure it's parseable."

Two formats under test, same enriched content:
  F1 jsonl - one JSON object per entry {id, section, title, intent, synonyms}
  F2 text  - compact line "ID | SECTION | title | intent | syn: a, b"

Two task families, both mechanically graded:
  PARSE  - structural probes over the index slice (counts, id lookup by title)
  SELECT - the T1 spike's 44 labeled plain-language queries; the model sees a
           pre-narrowed slice (the product flow: T1 classifier narrows, the
           agent picks) and must answer ITEM: <id> / ASK: <q> / NONE.

Inkling wire shape: reasoning streams first (message.reasoning), final text in
message.content (null until thinking completes) - generous max_tokens, grade
message.content only; null content = a recorded failure class, not a crash.

Usage: python3 spike.py [--limit N] [--out results/]
Stdlib only. Serving pre-flight is the caller's job (Step-0 done 2026-08-10).
"""

import argparse
import json
import re
import time
import urllib.request
from collections import defaultdict
from pathlib import Path

ENDPOINT = "https://inference.twin-bramble.ts.net/v1/chat/completions"
MODEL = "inkling-small-nvfp4"
ROOT = Path(__file__).resolve().parents[3]
CATALOG = ROOT / "src-tauri/resources/catalog/winlink-queries.txt"
QUERIES = ROOT / "dev/spikes/2026-08-09-t1-catalog-embedding/queries.jsonl"

# Deterministic per-section intent templates for the spike slice. The spike
# tests FORMAT parseability, not enrichment authorship; templates keep it
# reproducible. (Production enrichment quality is step-2 work.)
SECTION_INTENT = {
    "ARCTIC_ICE": "sea ice and iceberg hazard reports for northern waters",
    "AURORA": "aurora / northern lights visibility forecasts",
    "METAR": "current airport weather observations (METAR)",
    "PROPAGATION": "HF radio propagation and space weather summaries",
    "WX_US_AZ": "NWS text weather forecasts for Arizona zones and cities",
    "WX_US_NY": "NWS text weather forecasts for New York zones and cities",
    "SAT_KEPS": "satellite keplerian orbital elements for tracking",
    "MARINE_US_E": "marine coastal forecasts for the US east coast",
}
SECTION_SYNONYMS = {
    "ARCTIC_ICE": ["ice", "iceberg", "sea ice", "arctic"],
    "AURORA": ["aurora", "northern lights", "geomagnetic"],
    "METAR": ["metar", "airport weather", "aviation weather"],
    "PROPAGATION": ["propagation", "solar", "band conditions", "space weather"],
    "WX_US_AZ": ["weather", "forecast", "arizona"],
    "WX_US_NY": ["weather", "forecast", "new york"],
    "SAT_KEPS": ["keps", "satellite", "orbital elements", "tle"],
    "MARINE_US_E": ["marine", "coastal waters", "offshore"],
}


def load_catalog():
    entries = []
    for line in CATALOG.read_text(encoding="utf-8-sig").splitlines():
        parts = line.strip().split("|")
        if len(parts) >= 3:
            entries.append({"section": parts[0], "id": parts[1], "title": parts[2]})
    return entries


def load_queries():
    out = []
    for line in QUERIES.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            out.append(json.loads(line))
    return out


def build_slice(entries, queries, cap):
    """Sections referenced by the queries + the template sections, capped."""
    wanted = {q["expect"].get("section") for q in queries if q["expect"].get("section")}
    wanted |= set(SECTION_INTENT)
    sl = [e for e in entries if e["section"] in wanted]
    # Keep every query-labeled item; fill the rest round-robin per section.
    labeled = {i for q in queries for i in q["expect"].get("items", [])}
    keep = [e for e in sl if e["id"] in labeled]
    per_sec = defaultdict(list)
    for e in sl:
        if e["id"] not in labeled:
            per_sec[e["section"]].append(e)
    while len(keep) < cap and any(per_sec.values()):
        for sec in list(per_sec):
            if per_sec[sec] and len(keep) < cap:
                keep.append(per_sec[sec].pop(0))
    keep.sort(key=lambda e: (e["section"], e["id"]))
    return keep


def enrich(e):
    intent = SECTION_INTENT.get(e["section"], "on-demand Winlink catalog product")
    syn = SECTION_SYNONYMS.get(e["section"], [])
    return {**e, "intent": intent, "synonyms": syn}


def render(fmt, enriched):
    if fmt == "jsonl":
        return "\n".join(json.dumps(x, separators=(",", ":")) for x in enriched)
    return "\n".join(
        f"{x['id']} | {x['section']} | {x['title']} | {x['intent']} | syn: {', '.join(x['synonyms'])}"
        for x in enriched
    )


def chat(system, user, max_tokens=1600):
    body = json.dumps(
        {
            "model": MODEL,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "max_tokens": max_tokens,
            "temperature": 0.1,
        }
    ).encode()
    req = urllib.request.Request(
        ENDPOINT, data=body, headers={"Content-Type": "application/json"}
    )
    t0 = time.perf_counter()
    with urllib.request.urlopen(req, timeout=300) as r:
        out = json.loads(r.read())
    msg = out["choices"][0]["message"]
    return {
        "content": msg.get("content"),
        "finish": out["choices"][0].get("finish_reason"),
        "wall_s": round(time.perf_counter() - t0, 1),
        "completion_tokens": out.get("usage", {}).get("completion_tokens"),
    }


def parse_probes(enriched):
    by_sec = defaultdict(list)
    for x in enriched:
        by_sec[x["section"]].append(x)
    probes = []
    secs = sorted(by_sec)[:3]
    for sec in secs:
        probes.append(
            (f"How many entries are in section {sec}? Answer with just the number.",
             lambda t, n=len(by_sec[sec]): str(n) in re.findall(r"\d+", t or ""))
        )
    for sec in secs[:2]:
        x = by_sec[sec][0]
        probes.append(
            (f"Which entry id has the title \"{x['title']}\"? Answer with just the id.",
             lambda t, i=x["id"]: i in (t or ""))
        )
    x = enriched[len(enriched) // 2]
    probes.append(
        (f"What section is id {x['id']} in? Answer with just the section name.",
         lambda t, s=x["section"]: s in (t or ""))
    )
    return probes


SELECT_SYS = (
    "You are matching an operator's plain-language request against the Winlink "
    "catalog index below. Reply with EXACTLY ONE line in one of these forms:\n"
    "ITEM: <id>            - when one index entry clearly answers the request\n"
    "ASK: <one question>   - when several entries could match and you need the operator to choose\n"
    "NONE                  - when nothing in the index matches the request\n"
    "Never invent an id that is not in the index.\n\nINDEX:\n"
)


def grade_select(q, content):
    t = (content or "").strip()
    first = t.splitlines()[0] if t else ""
    kind = q["expect"]["kind"]
    behavior = q.get("behavior", "answer")
    if content is None:
        return False, "null-content"
    if kind == "none":
        return first.upper().startswith("NONE"), f"want NONE got {first[:60]}"
    if behavior == "ask" or kind == "ambig":
        return first.upper().startswith("ASK"), f"want ASK got {first[:60]}"
    m = re.match(r"ITEM:\s*(\S+)", first, re.IGNORECASE)
    if not m:
        return False, f"want ITEM got {first[:60]}"
    got = m.group(1)
    if kind == "item":
        return got in q["expect"]["items"], f"want one of {q['expect']['items']} got {got}"
    if kind == "section":
        return got, f"section-check {q['expect']['section']} got {got}"  # resolved by caller
    return False, "unknown kind"


def run(limit, outdir):
    entries = load_catalog()
    queries = load_queries()[: limit or None]
    slice_ = build_slice(entries, queries, cap=120)
    enriched = [enrich(e) for e in slice_]
    by_id = {x["id"]: x for x in enriched}
    outdir.mkdir(parents=True, exist_ok=True)
    summary = {"slice": len(enriched), "queries": len(queries), "formats": {}}
    rows = []

    for fmt in ("jsonl", "text"):
        index = render(fmt, enriched)
        sys_parse = (
            "Answer questions about the catalog index below, precisely and "
            "with no extra words.\n\nINDEX:\n" + index
        )
        p_ok = 0
        probes = parse_probes(enriched)
        for prompt, check in probes:
            r = chat(sys_parse, prompt, max_tokens=900)
            ok = bool(check(r["content"]))
            p_ok += ok
            rows.append({"fmt": fmt, "task": "parse", "prompt": prompt, "ok": ok, **r})

        s_ok = 0
        for q in queries:
            r = chat(SELECT_SYS + index, q["q"])
            ok, note = grade_select(q, r["content"])
            # Section-kind ITEM picks come back as the picked id (a str) for
            # THIS caller to resolve; ask/none verdicts are already bool. The
            # first cut ran `True in by_id` and clobbered correct ASKs.
            if q["expect"]["kind"] == "section" and isinstance(ok, str):
                ok = ok in by_id and by_id[ok]["section"] == q["expect"]["section"]
            s_ok += bool(ok)
            rows.append(
                {"fmt": fmt, "task": "select", "qid": q["id"], "ok": bool(ok), "note": note, **r}
            )
        summary["formats"][fmt] = {
            "parse": f"{p_ok}/{len(probes)}",
            "select": f"{s_ok}/{len(queries)}",
        }
        (outdir / "rows.jsonl").write_text("\n".join(json.dumps(x) for x in rows))
        (outdir / "summary.json").write_text(json.dumps(summary, indent=1))
        print(json.dumps({fmt: summary["formats"][fmt]}))
    print(json.dumps(summary))


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--out", default=str(Path(__file__).parent / "results"))
    a = ap.parse_args()
    run(a.limit, Path(a.out))
