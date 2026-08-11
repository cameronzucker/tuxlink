#!/usr/bin/env python3
"""Battery v3 — the REAL-CONTEXT instrument (function-calling wire).

Production-faithful where v1/v2 were synthetic:
- NO invented system prompt. Production Elmer's base arm sends none
  (compose_system_prompt(None, false) = None, asserted by its own test);
  the only system message here is the one the NARROWING LAYER would add,
  and only in the narrowed condition — it IS the mechanism under test.
- REAL tool schemas (tool-schemas.json, dumped from the registry) in the
  request's `tools` array, OpenAI function format.
- Grading on emitted tool_calls, not a text grammar — ask-then-call and
  parameter questions are natural here, so capable models aren't fighting
  the frame.

Conditions:
  everything   : all 92 full schemas in `tools` — today's shipped shape;
                 also the real prefill-cost baseline.
  narrowed-net : top-12 full schemas in `tools` + one system message with
                 the advisory shortlist and the full 92-name inventory,
                 stating any listed tool may be CALLED BY NAME. On the
                 wrong-shortlist probes, recovery requires the model to
                 emit a tool_call for a function NOT in its tools array —
                 the lazy-schema mechanism's live-or-die question.

Cases: the same 28 (3 wrong-shortlist, 12 ordinary, 4 no-tool, 4 vague,
5 two-turn with stale turn-1 shortlists). Grading: tool buckets = first
emitted tool_call name in the correct set; no-tool = no tool_calls;
vague = no tool_calls (didn't guess; content recorded); two-turn = final
turn's tool_call vs target. Fabrication = a tool_call naming a function
in NEITHER the corpus nor the array.

Usage: python3 spike_v3.py --label <row> [--endpoint URL] [--model ID]
       [--api-key-env VAR]
"""

import argparse
import json
import os
import statistics
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SHORTLISTS = ROOT / "dev/spikes/2026-08-10-tool-surface-embedding/shortlists.jsonl"
CORPUS = ROOT / "src-tauri/resources/agents/tool-surface.jsonl"
SCHEMAS = HERE / "tool-schemas.json"

HIT_SAMPLE = [
    "armed-check", "inbox-list", "read-msg", "catalog-request", "cms-dial",
    "prop-check", "ardop-mail", "vara-drive", "ft8-band", "grid-set",
    "routine-run", "tune-radio",
]

MULTITURN = [
    {"id": "mt-status-printer", "shortlist_from": "ambig-status",
     "turn1": "what's the status",
     "ask": "Status of what - the radio link, the Winlink backend, or something else?",
     "turn2": "the label printer - is it ready to go?",
     "target": ["printer_list"], "recovery": True},
    {"id": "mt-connect-bt", "shortlist_from": "ambig-connect",
     "turn1": "connect",
     "ask": "Connect to what - a gateway over radio, or Winlink over the internet?",
     "turn2": "neither, I need to pair my bluetooth TNC first",
     "target": ["packet_list_bluetooth_devices"], "recovery": True},
    {"id": "mt-stop-privacy", "shortlist_from": "ambig-stop",
     "turn1": "stop everything",
     "ask": "Stop what - the current connection, the FT8 monitor, a routine?",
     "turn2": "actually what I want is to stop sharing my exact location publicly",
     "target": ["config_set_privacy"], "recovery": True},
    {"id": "mt-connect-cms", "shortlist_from": "ambig-connect",
     "turn1": "connect",
     "ask": "Connect to what - a gateway over radio, or Winlink over the internet?",
     "turn2": "winlink over the internet please",
     "target": ["cms_connect"], "recovery": False},
    {"id": "mt-config-grid", "shortlist_from": "ambig-config",
     "turn1": "change my settings",
     "ask": "Which settings would you like to change?",
     "turn2": "my grid square - we moved to DM43",
     "target": ["config_set_grid"], "recovery": False},
]

NARROW_SYSTEM = (
    "Tool routing for this request: a classifier suggests these tools as "
    "most likely relevant (it can be wrong):\n{shortlist}\n\n"
    "Full tool inventory - ANY of these can be called by name even if its "
    "full definition is not in your tools list; call it and the definition "
    "will be provided:\n{inventory}"
)


def load_jsonl(path):
    out = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            out.append(json.loads(line))
    return out


def chat(endpoint, model, key, messages, tools, max_tokens=2000, retries=2):
    body = json.dumps({"model": model, "max_tokens": max_tokens,
                       "messages": messages, "tools": tools}).encode()
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = f"Bearer {key}"
    for attempt in range(retries + 1):
        try:
            req = urllib.request.Request(endpoint, data=body, headers=headers)
            with urllib.request.urlopen(req, timeout=300) as r:
                d = json.loads(r.read())
                msg = d["choices"][0]["message"]
                return msg, d.get("usage") or {}
        except Exception as e:  # noqa: BLE001
            if attempt == retries:
                return {"__error__": str(e)}, {}
            time.sleep(4)


def first_call(msg):
    for tc in msg.get("tool_calls") or []:
        fn = tc.get("function") or {}
        if fn.get("name"):
            return fn["name"]
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", default="https://inference.twin-bramble.ts.net/v1/chat/completions")
    ap.add_argument("--model", default="inkling-small-nvfp4")
    ap.add_argument("--api-key-env", default="")
    ap.add_argument("--label", required=True)
    args = ap.parse_args()
    key = os.environ.get(args.api_key_env) if args.api_key_env else None
    outdir = HERE / "results-v3" / args.label
    outdir.mkdir(parents=True, exist_ok=True)

    rows = {r["id"]: r for r in load_jsonl(SHORTLISTS)}
    corpus = {e["id"]: e for e in load_jsonl(CORPUS)}
    schemas = {s["name"]: s for s in json.loads(SCHEMAS.read_text())}
    inventory = "\n".join(
        f"- {e['id']}: {e['title']}" for e in sorted(corpus.values(), key=lambda e: e["id"]))

    def fndef(name):
        s = schemas[name]
        return {"type": "function", "function": {
            "name": s["name"], "description": s["description"],
            "parameters": s["parameters"]}}

    all_tools = [fndef(n) for n in sorted(schemas)]

    cases = []
    for r in rows.values():
        if r["kind"] == "none":
            cases.append(("none", r, None))
        elif r["kind"] == "ambig":
            cases.append(("ambig", r, None))
        elif r["kind"] == "tool":
            in12 = r["rank"] is not None and r["rank"] <= 12
            if not in12:
                cases.append(("miss", r, None))
            elif r["id"] in HIT_SAMPLE:
                cases.append(("hit", r, None))
    for mt in MULTITURN:
        cases.append(("multiturn", rows[mt["shortlist_from"]], mt))

    results, tally = [], {}
    for cond in ("everything", "narrowed-net"):
        for b, r, mt in cases:
            short12 = [c["id"] for c in r["shortlist"][:12]]
            if cond == "everything":
                tools = all_tools
                sys_msgs = []
            else:
                tools = [fndef(n) for n in short12 if n in schemas]
                shortlist_txt = "\n".join(
                    f"- {c['id']}: {c['title']}" for c in r["shortlist"][:12])
                sys_msgs = [{"role": "system", "content": NARROW_SYSTEM.format(
                    shortlist=shortlist_txt, inventory=inventory)}]
            if b == "multiturn":
                messages = sys_msgs + [
                    {"role": "user", "content": mt["turn1"]},
                    {"role": "assistant", "content": mt["ask"]},
                    {"role": "user", "content": mt["turn2"]},
                ]
                correct = set(mt["target"])
                cid = mt["id"]
            else:
                messages = sys_msgs + [{"role": "user", "content": r["q"]}]
                correct = set(r.get("correct") or [])
                cid = r["id"]
            msg, usage = chat(args.endpoint, args.model, key, messages, tools)
            picked = None if "__error__" in msg else first_call(msg)
            content = msg.get("content") if "__error__" not in msg else msg["__error__"]
            if b in ("none", "ambig"):
                ok = picked is None and "__error__" not in msg
            else:
                ok = picked in correct
            # Recovery bookkeeping: was the picked tool absent from the array?
            outside_array = bool(picked) and cond == "narrowed-net" and picked not in short12
            fabricated = bool(picked) and picked not in corpus
            results.append({
                "cond": cond, "bucket": b, "id": cid,
                "correct": sorted(correct), "picked": picked, "ok": ok,
                "outside_array": outside_array, "fabricated": fabricated,
                "recovery": bool(mt and mt.get("recovery")),
                "prompt_tokens": usage.get("prompt_tokens"),
                "completion_tokens": usage.get("completion_tokens"),
                "content": (content or "")[:300] if isinstance(content, str) else content,
            })
            k = (cond, b)
            n_ok, n = tally.get(k, (0, 0))
            tally[k] = (n_ok + (1 if ok else 0), n + 1)
            print(f"{cond:12s} {b:9s} {cid:20s} -> {picked} "
                  f"{'OK' if ok else 'x'}"
                  f"{' OUTSIDE-ARRAY' if outside_array else ''}"
                  f"{' FABRICATED' if fabricated else ''}", flush=True)

    (outdir / "rows.jsonl").write_text("\n".join(json.dumps(x) for x in results) + "\n")
    print(f"\n---- {args.label} v3 summary (ok/total) ----")
    for (cond, b), (n_ok, n) in sorted(tally.items()):
        print(f"{cond:12s} {b:9s}: {n_ok}/{n}")
    oa = [x for x in results if x["outside_array"]]
    oa_ok = [x for x in oa if x["ok"]]
    print(f"outside-array calls emitted: {len(oa)} (correct: {len(oa_ok)}) "
          f"<- the lazy-schema live-or-die datum")
    fab = sum(1 for x in results if x["fabricated"])
    print(f"fabricated: {fab}/{len(results)}")
    for cond in ("everything", "narrowed-net"):
        toks = [x["prompt_tokens"] for x in results
                if x["cond"] == cond and x["prompt_tokens"]]
        if toks:
            print(f"{cond:12s} median prompt tokens: {int(statistics.median(toks))}")


if __name__ == "__main__":
    main()
