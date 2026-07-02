#!/usr/bin/env python3
"""Agentic eval harness against a llama.cpp llama-server OpenAI endpoint (/v1/chat/completions).
Same faithful Elmer surface + mocks as harness.py; adapted request/response shape for OpenAI tools.
Usage: harness_oai.py LABEL PROMPT_IDS OUTDIR [PORT]
"""
import json, sys, time, urllib.request, os, re

LABEL   = sys.argv[1]
PROMPTS = [int(x) for x in sys.argv[2].split(",")]
OUTDIR  = sys.argv[3] if len(sys.argv) > 3 else "/root/evalout"
PORT    = sys.argv[4] if len(sys.argv) > 4 else "8080"
HERE    = os.path.dirname(os.path.abspath(__file__))
os.makedirs(OUTDIR, exist_ok=True)
TOOLS = json.load(open(os.path.join(HERE, "tools.json")))
URL = f"http://localhost:{PORT}/v1/chat/completions"

# reuse the exact system prompt + prompts + mocks from the ollama harness
import importlib.util
spec = importlib.util.spec_from_file_location("h", os.path.join(HERE, "harness.py"))
# harness.py runs main() only under __main__, so importing is safe for its defs
h = importlib.util.module_from_spec(spec)
sys.argv_backup = sys.argv
sys.argv = ["harness.py", "x", "1"]   # satisfy harness.py module-level arg reads
spec.loader.exec_module(h)
sys.argv = sys.argv_backup
SYSTEM_PROMPT = h.SYSTEM_PROMPT
PROMPT_TEXT = h.PROMPT_TEXT
run_tool = h.run_tool
nonascii_ratio = h.nonascii_ratio

def chat(messages):
    body = {"model": "gpt-oss", "messages": messages, "tools": TOOLS,
            "tool_choice": "auto", "temperature": 0.0, "max_tokens": 4096, "stream": False}
    req = urllib.request.Request(URL, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(req, timeout=3600).read())

def run(pid, log):
    messages = [{"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": PROMPT_TEXT[pid]}]
    tool_calls_total = 0; tools_used = []; final = ""; eval_tokens = 0
    t0 = time.time(); reached = False; err = None; MAX_TURNS = 20
    log.write(f"\n{'='*70}\nLABEL={LABEL} PROMPT {pid} (llama.cpp OpenAI)\n{'='*70}\nUSER: {PROMPT_TEXT[pid]}\n")
    for turn in range(MAX_TURNS):
        try:
            d = chat(messages)
        except Exception as e:
            err = str(e); log.write(f"\n[TURN {turn}] REQUEST ERROR: {err}\n"); break
        eval_tokens += (d.get("usage") or {}).get("completion_tokens") or 0
        msg = (d.get("choices") or [{}])[0].get("message", {}) or {}
        content = msg.get("content") or ""
        reasoning = msg.get("reasoning_content") or ""
        tcs = msg.get("tool_calls") or []
        log.write(f"\n[TURN {turn}] reason_len={len(reasoning)} content_len={len(content)} tool_calls={len(tcs)}\n")
        if reasoning: log.write(f"  REASON: {reasoning[:1500]}\n")
        if content:   log.write(f"  CONTENT: {content[:3000]}\n")
        # append assistant turn (with tool_calls verbatim so tool msgs can reference ids)
        am = {"role": "assistant", "content": content}
        if tcs: am["tool_calls"] = tcs
        messages.append(am)
        if tcs:
            for tc in tcs:
                fn = tc.get("function", {})
                name = fn.get("name", "?")
                raw = fn.get("arguments", "") or "{}"
                try: args = json.loads(raw) if isinstance(raw, str) else raw
                except Exception: args = {}
                tool_calls_total += 1; tools_used.append(name)
                result = run_tool(name, args)
                log.write(f"  -> CALL {name}({json.dumps(args)[:300]})\n     RESULT {json.dumps(result)[:400]}\n")
                messages.append({"role": "tool", "tool_call_id": tc.get("id", ""),
                                 "content": json.dumps(result)})
        else:
            final = content; reached = True; break
    dt = time.time() - t0
    summary = {"model": LABEL, "device": "3090+ncpumoe", "prompt": pid, "turns": turn + 1,
               "tool_calls": tool_calls_total, "distinct_tools": sorted(set(tools_used)),
               "tool_sequence": tools_used, "reached_final": reached, "wall_s": round(dt, 1),
               "eval_tokens": eval_tokens, "final_len": len(final),
               "nonascii_ratio": round(nonascii_ratio(final), 3),
               "garbage": nonascii_ratio(final) > 0.20 and len(final) > 40,
               "error": err, "final_snippet": final[:800]}
    log.write(f"\nSUMMARY: {json.dumps(summary)}\n")
    return summary

def main():
    tag = re.sub(r'[^A-Za-z0-9._-]', '_', LABEL)
    for pid in PROMPTS:
        with open(os.path.join(OUTDIR, f"{tag}__p{pid}.log"), "w") as log:
            s = run(pid, log)
        with open(os.path.join(OUTDIR, "results.jsonl"), "a") as rf:
            rf.write(json.dumps(s) + "\n")
        print(json.dumps({k: s[k] for k in ("model","prompt","turns","tool_calls","distinct_tools","reached_final","wall_s","final_len","error")}))
    print("done", tag)

if __name__ == "__main__":
    main()
