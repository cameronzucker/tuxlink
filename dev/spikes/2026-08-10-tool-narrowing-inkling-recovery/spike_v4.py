#!/usr/bin/env python3
"""Battery v4 — the Inkling-focused overnight instrument.

Operator pivot (2026-08-11): the small-model panel is abandoned; Inkling is
the only backend that matters and the classifier must BENEFIT it. v4 extends
v3 (the real-context function-calling instrument) along four axes:

1. FULL CASE SET: all 44 in-top12 labeled tool rows (v3 sampled 12), the
   3 shortlist-miss rows, 4 none, 4 ambig, and 13 two-turn cases (v3's 5
   plus 8 new recovery/control pivots built on the same stale shortlists).
2. THIRD CONDITION — narrowed-pinned: the top-12 array plus a small
   deterministic ALWAYS-INCLUDE pin-set (server_info, docs_search, the
   abort section) — the FINDINGS-v3 hypothesis under direct test. Same
   narrowing system message as narrowed-net; only the tools array grows.
3. REASONING CAPTURE: the serving stack exposes a `reasoning` field on the
   chat message; v4 records it (plus untruncated-enough content and full
   tool_call names/arguments) so failures can be read, not guessed at.
4. REPETITIONS + RESUME: production temperature 0.2, N reps (default 3),
   rows appended + flushed per case and skipped on restart, so a crashed
   run resumes instead of restarting. Per-call wall-clock latency is
   recorded — the everything-vs-narrowed latency delta on the real serving
   is the wall-clock worth-it datum.

Grading is v3's, unchanged, for comparability: tool buckets = first emitted
tool_call name in the correct set; none/ambig = no tool_calls; two-turn =
final-turn call vs target; fabricated = a name in neither corpus nor array.
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

# v3's five two-turn cases, verbatim, + 8 new pivots on the same stale
# turn-1 shortlists. recovery=True means the target is NOT in that stale
# top-12, so the narrowed conditions must leave the array to succeed.
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
    # --- v4 additions ---
    {"id": "mt2-status-inbox", "shortlist_from": "ambig-status",
     "turn1": "what's the status",
     "ask": "Status of what - the radio link, the Winlink backend, or something else?",
     "turn2": "my weather catalog request - did anything come back? check the inbox",
     "target": ["mailbox_list"], "recovery": True},
    {"id": "mt2-status-wwv", "shortlist_from": "ambig-status",
     "turn1": "what's the status",
     "ask": "Status of what - the radio link, the Winlink backend, or something else?",
     "turn2": "the off-air WWV space weather capture - is one available yet?",
     "target": ["wwv_offair_available"], "recovery": True},
    {"id": "mt2-status-varainstall", "shortlist_from": "ambig-status",
     "turn1": "what's the status",
     "ask": "Status of what - the radio link, the Winlink backend, or something else?",
     "turn2": "the VARA install you kicked off earlier - how far along is it?",
     "target": ["vara_install_status"], "recovery": False},
    {"id": "mt2-connect-rigtune", "shortlist_from": "ambig-connect",
     "turn1": "connect",
     "ask": "Connect to what - a gateway over radio, or Winlink over the internet?",
     "turn2": "hold on, first get the radio onto 7.101 MHz USB",
     "target": ["rig_tune"], "recovery": True},
    {"id": "mt2-connect-p2ppass", "shortlist_from": "ambig-connect",
     "turn1": "connect",
     "ask": "Connect to what - a gateway over radio, or Winlink over the internet?",
     "turn2": "to my buddy KJ7ABC peer-to-peer, but first check whether my peer password is even set up",
     "target": ["p2p_peer_password_status"], "recovery": True},
    {"id": "mt2-config-gpssource", "shortlist_from": "ambig-config",
     "turn1": "change my settings",
     "ask": "Which settings would you like to change?",
     "turn2": "switch my position source over to the USB GPS puck",
     "target": ["position_set_source"], "recovery": True},
    {"id": "mt2-config-privacy", "shortlist_from": "ambig-config",
     "turn1": "change my settings",
     "ask": "Which settings would you like to change?",
     "turn2": "turn off public sharing of my exact location",
     "target": ["config_set_privacy"], "recovery": False},
    {"id": "mt2-stop-cmsabort", "shortlist_from": "ambig-stop",
     "turn1": "stop everything",
     "ask": "Stop what - the current connection, the FT8 monitor, a routine?",
     "turn2": "the winlink session - kill it",
     "target": ["cms_abort"], "recovery": False},
]

NARROW_SYSTEM = (
    "Tool routing for this request: a classifier suggests these tools as "
    "most likely relevant (it can be wrong):\n{shortlist}\n\n"
    "Full tool inventory - ANY of these can be called by name even if its "
    "full definition is not in your tools list; call it and the definition "
    "will be provided:\n{inventory}"
)

PIN_IDS_STATIC = ["server_info", "docs_search"]  # + abort section, resolved at load


def load_jsonl(path):
    out = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            out.append(json.loads(line))
    return out


def chat(endpoint, model, key, messages, tools, max_tokens=4000, retries=2,
         temperature=None):
    payload = {"model": model, "max_tokens": max_tokens,
               "messages": messages, "tools": tools}
    if temperature is not None:
        payload["temperature"] = temperature
    body = json.dumps(payload).encode()
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = f"Bearer {key}"
    for attempt in range(retries + 1):
        t0 = time.monotonic()
        try:
            req = urllib.request.Request(endpoint, data=body, headers=headers)
            with urllib.request.urlopen(req, timeout=420) as r:
                d = json.loads(r.read())
                msg = d["choices"][0]["message"]
                return msg, d.get("usage") or {}, time.monotonic() - t0
        except Exception as e:  # noqa: BLE001
            if attempt == retries:
                return {"__error__": str(e)}, {}, time.monotonic() - t0
            time.sleep(6)


def first_call(msg):
    for tc in msg.get("tool_calls") or []:
        fn = tc.get("function") or {}
        if fn.get("name"):
            return fn["name"]
    return None


def call_digest(msg):
    out = []
    for tc in msg.get("tool_calls") or []:
        fn = tc.get("function") or {}
        out.append({"name": fn.get("name"),
                    "arguments": (fn.get("arguments") or "")[:600]})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", default="https://inference.twin-bramble.ts.net/v1/chat/completions")
    ap.add_argument("--model", default="thinkingmachines/Inkling-Small-NVFP4")
    ap.add_argument("--api-key-env", default="")
    ap.add_argument("--label", required=True)
    ap.add_argument("--temperature", type=float, default=0.2)
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--conditions", default="everything,narrowed-net,narrowed-pinned")
    ap.add_argument("--max-cases", type=int, default=0, help="smoke-test cap")
    args = ap.parse_args()
    key = os.environ.get(args.api_key_env) if args.api_key_env else None
    conditions = [c.strip() for c in args.conditions.split(",") if c.strip()]
    outdir = HERE / "results-v4" / args.label
    outdir.mkdir(parents=True, exist_ok=True)
    rowfile = outdir / "rows.jsonl"

    rows = {r["id"]: r for r in load_jsonl(SHORTLISTS)}
    corpus = {e["id"]: e for e in load_jsonl(CORPUS)}
    schemas = {s["name"]: s for s in json.loads(SCHEMAS.read_text())}
    inventory = "\n".join(
        f"- {e['id']}: {e['title']}" for e in sorted(corpus.values(), key=lambda e: e["id"]))
    pins = PIN_IDS_STATIC + sorted(
        e["id"] for e in corpus.values() if e.get("section") == "abort")

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
            cases.append(("hit" if in12 else "miss", r, None))
    for mt in MULTITURN:
        cases.append(("multiturn", rows[mt["shortlist_from"]], mt))
    if args.max_cases:
        cases = cases[: args.max_cases]

    done = set()
    if rowfile.exists():
        for x in load_jsonl(rowfile):
            done.add((x["rep"], x["cond"], x["id"]))
        print(f"resume: {len(done)} rows already present", flush=True)

    out = open(rowfile, "a")
    n_new = 0
    for rep in range(1, args.reps + 1):
        for cond in conditions:
            for b, r, mt in cases:
                cid = mt["id"] if mt else r["id"]
                if (rep, cond, cid) in done:
                    continue
                short12 = [c["id"] for c in r["shortlist"][:12]]
                if cond == "everything":
                    array_ids = sorted(schemas)
                    sys_msgs = []
                else:
                    array_ids = [n for n in short12 if n in schemas]
                    if cond == "narrowed-pinned":
                        array_ids += [p for p in pins
                                      if p in schemas and p not in array_ids]
                    shortlist_txt = "\n".join(
                        f"- {c['id']}: {c['title']}" for c in r["shortlist"][:12])
                    sys_msgs = [{"role": "system", "content": NARROW_SYSTEM.format(
                        shortlist=shortlist_txt, inventory=inventory)}]
                tools = [fndef(n) for n in array_ids]
                if b == "multiturn":
                    messages = sys_msgs + [
                        {"role": "user", "content": mt["turn1"]},
                        {"role": "assistant", "content": mt["ask"]},
                        {"role": "user", "content": mt["turn2"]},
                    ]
                    correct = set(mt["target"])
                else:
                    messages = sys_msgs + [{"role": "user", "content": r["q"]}]
                    correct = set(r.get("correct") or [])
                msg, usage, dt = chat(args.endpoint, args.model, key, messages,
                                      tools, temperature=args.temperature)
                err = msg.get("__error__")
                picked = None if err else first_call(msg)
                content = err if err else (msg.get("content") or "")
                reasoning = "" if err else (msg.get("reasoning") or
                                            msg.get("reasoning_content") or "")
                if b in ("none", "ambig"):
                    ok = picked is None and not err
                else:
                    ok = picked in correct
                outside_array = bool(picked) and cond != "everything" and \
                    picked not in array_ids
                fabricated = bool(picked) and picked not in corpus
                row = {
                    "rep": rep, "cond": cond, "bucket": b, "id": cid,
                    "correct": sorted(correct), "picked": picked, "ok": ok,
                    "error": bool(err),
                    "outside_array": outside_array, "fabricated": fabricated,
                    "recovery": bool(mt and mt.get("recovery")),
                    "prompt_tokens": usage.get("prompt_tokens"),
                    "completion_tokens": usage.get("completion_tokens"),
                    "latency_s": round(dt, 2),
                    "tool_calls": call_digest(msg) if not err else [],
                    "content": (content or "")[:4000],
                    "reasoning": (reasoning or "")[:6000],
                }
                out.write(json.dumps(row) + "\n")
                out.flush()
                n_new += 1
                print(f"r{rep} {cond:15s} {b:9s} {cid:22s} -> {picked} "
                      f"{'OK' if ok else 'x'}{' ERR' if err else ''}"
                      f"{' OUTSIDE-ARRAY' if outside_array else ''}"
                      f"{' FABRICATED' if fabricated else ''} {dt:5.1f}s",
                      flush=True)
            print(f"== rep {rep} condition {cond} complete ==", flush=True)
    out.close()

    # ---- summary over the FULL row file (all reps) ----
    allrows = load_jsonl(rowfile)
    tally = {}
    for x in allrows:
        k = (x["cond"], x["bucket"])
        n_ok, n = tally.get(k, (0, 0))
        tally[k] = (n_ok + (1 if x["ok"] else 0), n + 1)
    print(f"\n---- {args.label} v4 summary (ok/total, all reps) ----")
    for (cond, b), (n_ok, n) in sorted(tally.items()):
        print(f"{cond:15s} {b:9s}: {n_ok}/{n}")
    for cond in conditions:
        oa = [x for x in allrows if x["cond"] == cond and x["outside_array"]]
        oa_ok = [x for x in oa if x["ok"]]
        print(f"{cond:15s} outside-array: {len(oa)} (correct {len(oa_ok)})")
    fab = sum(1 for x in allrows if x["fabricated"])
    errs = sum(1 for x in allrows if x.get("error"))
    print(f"fabricated: {fab}/{len(allrows)}  errors: {errs}")
    for cond in conditions:
        toks = [x["prompt_tokens"] for x in allrows
                if x["cond"] == cond and x["prompt_tokens"]]
        lats = [x["latency_s"] for x in allrows
                if x["cond"] == cond and x["latency_s"] and not x.get("error")]
        if toks:
            print(f"{cond:15s} median prompt tokens: {int(statistics.median(toks))}"
                  f"  median latency: {statistics.median(lats):.1f}s")
    rsn = sum(1 for x in allrows if x.get("reasoning"))
    print(f"rows with non-empty reasoning: {rsn}/{len(allrows)}")


if __name__ == "__main__":
    main()
