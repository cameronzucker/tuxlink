#!/usr/bin/env python3
"""Ladder-2 adversarial reviewer. Nemotron-3-super-120b via OpenRouter, pinned
to Nebius fp4 (NVFP4). Reads {user prompt, built def, catalog}, emits an
adversarial QA critique to stdout. Reasoning on/off is the experimental factor.

usage: review.py <prompt_file> <def_file> <on|off> <catalog_file>
env: OPENROUTER_API_KEY
"""
import sys, json, os, urllib.request, urllib.error

prompt = open(sys.argv[1]).read()
def_path = sys.argv[2]
deftext = open(def_path).read() if (os.path.exists(def_path) and os.path.getsize(def_path) > 0) else "(the builder saved NO routine)"
reasoning_on = sys.argv[3] == "on"
catalog = open(sys.argv[4]).read()
key = os.environ["OPENROUTER_API_KEY"]

sys_prompt = (
    "You are an adversarial QA reviewer for Tuxlink Routines: automations a builder assembles "
    "from a FIXED action catalog (the only actions/controls a routine may use). Below is the catalog, "
    "the user's request, and the routine the builder produced. Critique the routine ADVERSARIALLY "
    "against the user's request. Check every material requirement: is each implemented, or honestly "
    "flagged unsupported? Flag specifically: missing requirements; a vaguely-related action substituted "
    "for the real one; steps made unreachable by control flow (e.g. a compose placed after a success end); "
    "recurrence/schedule dropped to manual when the user asked for recurring; a missing send/receive or "
    "compose leg; and any action the builder called unavailable that IS in the catalog. Do NOT invent "
    "actions absent from the catalog. Be specific and terse; produce an actionable critique the builder can "
    "revise against. If the routine is correct and complete, say so plainly and stop.\n\n"
    "ACTION CATALOG:\n" + catalog
)
user = "USER REQUEST:\n" + prompt.strip() + "\n\nBUILDER'S ROUTINE (JSON def):\n" + deftext.strip()

body = {
    "model": "nvidia/nemotron-3-super-120b-a12b",
    "provider": {"order": ["Nebius"], "allow_fallbacks": False, "quantizations": ["fp4"]},
    "temperature": 0.2,
    "max_tokens": 16000,
    "reasoning": {"enabled": reasoning_on},
    "messages": [
        {"role": "system", "content": sys_prompt},
        {"role": "user", "content": user},
    ],
}
req = urllib.request.Request(
    "https://openrouter.ai/api/v1/chat/completions",
    data=json.dumps(body).encode(),
    headers={"Authorization": "Bearer " + key, "Content-Type": "application/json",
             "HTTP-Referer": "https://tuxlink.local", "X-Title": "tuxlink-ladder2-review"},
)
try:
    resp = json.load(urllib.request.urlopen(req, timeout=600))
except urllib.error.HTTPError as e:
    sys.stderr.write("HTTP %s: %s\n" % (e.code, e.read().decode()[:500]))
    sys.exit(2)
msg = resp["choices"][0]["message"]
prov = resp.get("provider", "?")
content = msg.get("content") or ""
reason = msg.get("reasoning") or ""
# stdout = the critique (consumed by the driver). stderr = provenance + reasoning trace.
sys.stdout.write(content)
sys.stderr.write("[provider=%s reasoning_on=%s reasoning_chars=%d]\n" % (prov, reasoning_on, len(reason)))
if reason:
    sys.stderr.write("[REASONING]\n" + reason + "\n")
