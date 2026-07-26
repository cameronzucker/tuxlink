#!/usr/bin/env python3
"""Ladder-2 adversarial reviewer. Nemotron-3-super-120b via OpenRouter, pinned
to Nebius fp4 (NVFP4). Reads {user prompt, built def, catalog}, emits an
adversarial QA critique to stdout. Reasoning on/off is the experimental factor.

usage: review.py <prompt_file> <def_file> <on|off> <catalog_file>
                 [final_text_file] [inventory_file]
env: OPENROUTER_API_KEY
     REVIEW_SKILL_FILE  when set, its contents replace the inline system prompt
                        (the rev_skill condition; catalog is still appended)

Two inputs added 2026-07-26 after the Ladder-2 analysis. Both close gaps the
reviewer could not see BY CONSTRUCTION, not gaps in its instructions:

  final_text  the builder's own account of what it built. 17 bundles contained a
              final_text contradicting the saved def (claiming a field was
              removed while the def still carried it). Detecting that requires
              comparing two artifacts; the reviewer was only ever given one, so
              it scored 0 on a failure mode the Sonnet judge caught 17 times.
  inventory   the routines actually present after the build. 22 bundles had a
              revise introduce an ORPHANED duplicate routine. The skill already
              cautions against INVENTING orphan claims when no inventory is
              supplied; supplying one lets it report real ones instead.

Both are optional and omitted-safe: with neither passed the prompt is byte-identical
to the pre-2026-07-26 reviewer, so rev_off/rev_on stay comparable across runs.
"""
import sys, json, os, urllib.request, urllib.error

prompt = open(sys.argv[1]).read()
def_path = sys.argv[2]
deftext = open(def_path).read() if (os.path.exists(def_path) and os.path.getsize(def_path) > 0) else "(the builder saved NO routine)"
reasoning_on = sys.argv[3] == "on"
catalog = open(sys.argv[4]).read()
final_text = ""
if len(sys.argv) > 5 and os.path.exists(sys.argv[5]):
    final_text = open(sys.argv[5], errors="ignore").read().strip()
inventory = ""
if len(sys.argv) > 6 and os.path.exists(sys.argv[6]):
    inventory = open(sys.argv[6], errors="ignore").read().strip()
key = os.environ["OPENROUTER_API_KEY"]
skill_file = os.environ.get("REVIEW_SKILL_FILE", "")

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
if skill_file and os.path.exists(skill_file):
    sys_prompt = open(skill_file, errors="ignore").read() + "\n\nACTION CATALOG:\n" + catalog
user = "USER REQUEST:\n" + prompt.strip() + "\n\nBUILDER'S ROUTINE (JSON def):\n" + deftext.strip()
if final_text:
    user += ("\n\nBUILDER'S OWN ACCOUNT OF WHAT IT BUILT (final_text). Compare this "
             "against the def above: a claim here that the def does not support is a "
             "must-fix honesty defect, not a style note.\n" + final_text)
if inventory:
    user += ("\n\nROUTINES PRESENT AFTER THE BUILD (inventory). More than one routine, "
             "or a routine unrelated to the request, means the builder ORPHANED a "
             "duplicate instead of replacing it.\n" + inventory)

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
