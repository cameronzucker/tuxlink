#!/usr/bin/env python3
"""Tool-narrowing battery v2 — multi-model, vague intent, multi-turn.

Extends the v1 recovery spike after the operator's devil's-advocate round:
v1 prompts were single-turn and simple. v2 adds:

  AMBIG (4)     : the vague single-turn asks ("connect", "what's the
                  status") — expected behavior is ASK, under an extended
                  reply grammar TOOL:/ASK:/NONE.
  MULTITURN (5) : scripted two-turn conversations. The shortlist shown is
                  the one computed from the VAGUE turn-1 text (worst case:
                  a stale shortlist that was never re-run), and the turn-2
                  clarification steers to a specific tool — three cases
                  land OUTSIDE that stale top-12 (cross-turn recovery),
                  two land inside (control).
  usage capture : prompt/completion tokens per call from the API's usage
                  field — the cost table comes from the same runs.
  model rows    : --endpoint/--model/--api-key-env/--label make the same
                  battery run against any OpenAI-compatible server
                  (Inkling on the Spark, GPT-5.6-Luna via OpenRouter, the
                  small and 30B rows served locally on the Spark).

Buckets: MISS(3) + HIT(12) + NONE(4) + AMBIG(4) + MULTITURN(5) = 28 cases
x 2 conditions (narrowed / safety-net) = 56 calls per model.

Grading: TOOL: must name a correct tool; AMBIG expects ASK; NONE expects
NONE; MULTITURN grades the final turn's TOOL: against the target.
Fabrication (a tool name not in the corpus) is flagged everywhere.
"""

import argparse
import json
import os
import re
import statistics
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SHORTLISTS = ROOT / "dev/spikes/2026-08-10-tool-surface-embedding/shortlists.jsonl"
CORPUS = ROOT / "src-tauri/resources/agents/tool-surface.jsonl"

HIT_SAMPLE = [
    "armed-check", "inbox-list", "read-msg", "catalog-request", "cms-dial",
    "prop-check", "ardop-mail", "vara-drive", "ft8-band", "grid-set",
    "routine-run", "tune-radio",
]

# Two-turn cases. shortlist_from names the ambig query whose top-12 (the
# stale turn-1 shortlist) is shown; the assistant's clarifying question is
# scripted; turn2 steers to `target`.
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

SYSTEM = (
    "You are Tuxlink's assistant. Tuxlink is a Linux Winlink client for "
    "amateur radio. Your ONLY job here: choose the single best tool for the "
    "operator's request, ask ONE clarifying question if the request is too "
    "vague to pick, or decline.\n"
    "Reply with EXACTLY one line, nothing else:\n"
    "TOOL: <tool_name>     (the one tool that handles the request)\n"
    "ASK: <question>       (the request is too vague to choose)\n"
    "NONE                  (no tool applies; not a tool task)\n"
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


def chat(endpoint, model, key, messages, max_tokens=1600, retries=2):
    body = json.dumps({"model": model, "max_tokens": max_tokens,
                       "messages": messages}).encode()
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = f"Bearer {key}"
    for attempt in range(retries + 1):
        try:
            req = urllib.request.Request(endpoint, data=body, headers=headers)
            with urllib.request.urlopen(req, timeout=240) as r:
                d = json.loads(r.read())
                msg = d["choices"][0]["message"]
                usage = d.get("usage") or {}
                return msg.get("content"), usage
        except Exception as e:  # noqa: BLE001
            if attempt == retries:
                return f"__ERROR__ {e}", {}
            time.sleep(4)


def parse_reply(content):
    if content is None:
        return ("null", None)
    if isinstance(content, str) and content.startswith("__ERROR__"):
        return ("error", None)
    m = re.search(r"TOOL:\s*`?([a-z0-9_]+)`?", content)
    if m:
        return ("tool", m.group(1))
    if re.search(r"ASK:", content):
        return ("ask", None)
    if re.search(r"\bNONE\b", content):
        return ("none", None)
    return ("unparsed", None)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", default="https://inference.twin-bramble.ts.net/v1/chat/completions")
    ap.add_argument("--model", default="inkling-small-nvfp4")
    ap.add_argument("--api-key-env", default="")
    ap.add_argument("--label", required=True)
    ap.add_argument("--max-tokens", type=int, default=1600)
    args = ap.parse_args()
    key = os.environ.get(args.api_key_env) if args.api_key_env else None
    outdir = HERE / "results-v2" / args.label
    outdir.mkdir(parents=True, exist_ok=True)

    rows = {r["id"]: r for r in load_jsonl(SHORTLISTS)}
    corpus = {e["id"]: e for e in load_jsonl(CORPUS)}
    full_list = "\n".join(
        f"- {e['id']}: {e['title']}" for e in sorted(corpus.values(), key=lambda e: e["id"]))

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
    for cond in ("narrowed", "safety-net"):
        for b, r, mt in cases:
            short12 = r["shortlist"][:12]
            shortlist_txt = "\n".join(f"- {c['id']}: {c['title']}" for c in short12)
            preamble = (NARROW_PREAMBLE.format(shortlist=shortlist_txt)
                        if cond == "narrowed" else
                        NET_PREAMBLE.format(shortlist=shortlist_txt, full=full_list))
            if b == "multiturn":
                messages = [
                    {"role": "system", "content": SYSTEM},
                    {"role": "user", "content": preamble + f"\nOperator request: {mt['turn1']}"},
                    {"role": "assistant", "content": f"ASK: {mt['ask']}"},
                    {"role": "user", "content": mt["turn2"]},
                ]
                correct = set(mt["target"])
                cid = mt["id"]
            else:
                messages = [
                    {"role": "system", "content": SYSTEM},
                    {"role": "user", "content": preamble + f"\nOperator request: {r['q']}"},
                ]
                correct = set(r.get("correct") or [])
                cid = r["id"]
            content, usage = chat(args.endpoint, args.model, key, messages,
                                  max_tokens=args.max_tokens)
            kind, picked = parse_reply(content)
            if b == "none":
                ok = kind == "none"
            elif b == "ambig":
                ok = kind == "ask"
            else:
                ok = kind == "tool" and picked in correct
            fabricated = kind == "tool" and picked not in corpus
            results.append({
                "cond": cond, "bucket": b, "id": cid,
                "correct": sorted(correct), "picked": picked, "kind": kind,
                "ok": ok, "fabricated": fabricated,
                "recovery": bool(mt and mt.get("recovery")),
                "prompt_tokens": usage.get("prompt_tokens"),
                "completion_tokens": usage.get("completion_tokens"),
                "content": content,
            })
            k = (cond, b)
            n_ok, n = tally.get(k, (0, 0))
            tally[k] = (n_ok + (1 if ok else 0), n + 1)
            print(f"{cond:10s} {b:9s} {cid:20s} -> {kind}:{picked} "
                  f"{'OK' if ok else 'x'}{' FABRICATED' if fabricated else ''}",
                  flush=True)

    (outdir / "rows.jsonl").write_text("\n".join(json.dumps(x) for x in results) + "\n")
    print(f"\n---- {args.label} summary (ok/total) ----")
    for (cond, b), (n_ok, n) in sorted(tally.items()):
        print(f"{cond:10s} {b:9s}: {n_ok}/{n}")
    mt_rec = [x for x in results if x["bucket"] == "multiturn" and x["recovery"]]
    for cond in ("narrowed", "safety-net"):
        sub = [x for x in mt_rec if x["cond"] == cond]
        print(f"{cond:10s} multiturn-RECOVERY: {sum(x['ok'] for x in sub)}/{len(sub)}")
    fab = sum(1 for x in results if x["fabricated"])
    print(f"fabricated tool names: {fab}/{len(results)}")
    for cond in ("narrowed", "safety-net"):
        toks = [x["prompt_tokens"] for x in results
                if x["cond"] == cond and x["prompt_tokens"]]
        if toks:
            print(f"{cond:10s} median prompt tokens: {int(statistics.median(toks))}")


if __name__ == "__main__":
    main()
