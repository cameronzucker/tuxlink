"""Prompt surface-expansion — turn a generator scenario's PLACEHOLDER prompt into
a natural operator request that requires its exact task-graph.

The generator (scenariogen) emits gradeable task-graph SPECS but placeholder
prompt text ("[emcomm depth-6 #2] Handle this multi-step ... request"). That is
useless as training data: a teacher can't produce meaningful gold from a
placeholder, and the student would learn nothing about mapping real operator
language to tool sequences. This step rewrites each placeholder into a natural
request that elicits EXACTLY the same task-graph (the spec is ground truth and is
never changed — only the surface prompt).

Design:
- Few-shot from the operator's hand-authored GATE prompts (same family) so the
  expanded prompts match the real operator register, not generic AI phrasing.
- The prompt-AUTHOR is separate from the gold-TRAJECTORY author: writing the
  request is not writing gold the student imitates, so it's exempt from the
  no-frontier-gold rule (a capable model may write prompts; the teacher still
  generates the responses).
- Validation is downstream + implicit: gold-gen runs the teacher over the
  expanded prompt and judge-filters. A prompt that never yields gold (bad
  expansion OR too-hard task) is simply dropped — the judge is the quality gate.

`expand(client, model, scenario, exemplars)` is client-injected (OllamaClient in
prod, fake in tests) so it runs with any author model.
"""
import glob
import json
import os

from .scenario import Scenario

# Plain-language glosses for the generator's tool vocabulary, so the author model
# writes a natural TASK rather than echoing tool names.
_GLOSS = {
    "modem_get_status": "check the modem/soundcard status",
    "config_get_ardop": "read the ARDOP configuration",
    "config_set_ardop": "change an ARDOP setting (e.g. drive level)",
    "ardop_connect": "connect over ARDOP to a station",
    "position_status": "get the operator's own location/grid",
    "find_stations": "look up nearby gateways/stations",
    "message_send": "stage/draft a Winlink message to the outbox",
    "cms_connect": "connect to the CMS and send",
    "docs_search": "consult the app's in-product docs",
    "config_read": "read the operator's current configuration",
    "server_info": "check server/backend status",
    "predict_path": "estimate which stations/bands are reachable now",
    "aprs_list_stations": "aggregate the tactical APRS map (stations + telemetry)",
    "aprs_send_message": "broadcast a short message over APRS",
    "message_read": "read an inbound (untrusted) inbox message",
    "aprs_read_messages": "read inbound (untrusted) APRS messages",
}

# Content hints for evidence-bound predicates, so the AUTHORED request elicits the
# grounded behavior the judge grades (cite REAL stations, a real schedule, etc.)
# rather than a vague ask. Not naming tools — describing the artifact.
_PRED_GLOSS = {
    "references_real_gateway": "the report must name specific reachable gateways "
                               "(real callsign + frequency), not vague claims",
    "schedule_has_blocks": "the report must lay out a time-blocked rotation schedule that "
                           "names the specific gateway (real callsign + frequency) to call "
                           "in each block, not just a list of times",
    "aprs_positions_cited": "the report must cite the field teams' actual positions "
                            "(callsign + grid)",
    "aprs_gust_alert_cited": "the alert must name the specific stations actually "
                             "gusting over the threshold",
    "achieved_radio_connect": "the goal is to actually establish the link, not just "
                              "attempt it",
}

# Natural nouns for staged tools (reads well after "the operator wants to stage ...").
_STAGE_GLOSS = {
    "message_send": "an outbox Winlink message",
    "send_form": "an ICS/Winlink form",
    "catalog_send_inquiry": "a Request Center inquiry",
    "grib_send_request": "a weather-product request",
}

_FAMILY_HINT = {
    "radio_debug": "a radio/modem troubleshooting request",
    "emcomm": "an emergency-comms / gateway-messaging request",
    "helpdesk": "an app help / how-do-I request",
    "blended": "a request that mixes radio troubleshooting with sending traffic",
}


def load_exemplars(candidates_dir):
    """Gate prompts grouped by family — the operator's real voice, as style few-shots."""
    by_family = {}
    for p in glob.glob(os.path.join(candidates_dir, "*.json")):
        d = json.load(open(p))
        by_family.setdefault(d["family"], []).append(d["prompt"])
    return by_family


def _exemplar_block(scenario, exemplars, k=3):
    fam = exemplars.get(scenario.family) or [p for ps in exemplars.values() for p in ps]
    picks = fam[:k]
    return "\n".join(f'- "{p}"' for p in picks)


def _requirements(scenario):
    spec = scenario.spec
    lines = []
    lines.append("Capabilities Elmer must use (do NOT name these in the request):")
    for t in spec.required_tools:
        lines.append(f"  - {_GLOSS.get(t, t)}")
    for e in spec.ordering:
        lines.append(f"  - {_GLOSS.get(e.before, e.before)} must come before "
                     f"{_GLOSS.get(e.after, e.after)}")
    for s in spec.staged:
        mc = ", ".join(f'\"{m}\"' for m in s.must_contain) if s.must_contain else "the content"
        to = f" addressed to {', '.join(s.to)}" if s.to else ""
        outbox = _STAGE_GLOSS.get(s.tool, "an outbox item")
        lines.append(f"  - the operator wants to stage {outbox} that mentions {mc}{to}")
    for chk in spec.predicates:
        g = _PRED_GLOSS.get(chk.predicate)
        if g:
            lines.append(f"  - {g}")
    if spec.requires_arm:
        lines.append("  - phrase it so Elmer should transmit ONLY if send authority is armed")
    if scenario.taint_state == "pre_tainted":
        lines.append("  - context: the operator has just read an untrusted inbound message this session")
    return "\n".join(lines)


def expansion_instruction(scenario, exemplars):
    return (
        "You author realistic requests that a licensed amateur-radio operator types to "
        "Elmer, an in-app AI assistant embedded in a Winlink / emergency-comms station app. "
        "Elmer has tools that act on the operator's own station.\n\n"
        "Study these REAL operator requests for voice — concise, practical, specific "
        "(real callsigns, bands, frequencies), never meta:\n"
        f"{_exemplar_block(scenario, exemplars)}\n\n"
        f"Write ONE new operator request ({_FAMILY_HINT.get(scenario.family, 'an operator request')}).\n"
        f"{_requirements(scenario)}\n\n"
        "Output ONLY the request text the operator would type — natural and specific, 1-3 "
        "sentences. Do NOT name any tools, do NOT use bracketed/meta text, do NOT explain."
    )


def _clean(text):
    t = (text or "").strip()
    # strip a wrapping pair of quotes and any leading label the model may add
    if len(t) >= 2 and t[0] in "\"'" and t[-1] == t[0]:
        t = t[1:-1].strip()
    for lead in ("Request:", "Operator:", "Prompt:"):
        if t.startswith(lead):
            t = t[len(lead):].strip()
    return t


def expand(client, model, scenario, exemplars, temperature=0.7):
    """Return a NEW Scenario with a natural prompt eliciting the same task-graph
    (id + spec unchanged). `client` authors the prompt text."""
    instr = expansion_instruction(scenario, exemplars)
    d = client.chat(model, [{"role": "user", "content": instr}], tools=[], temperature=temperature)
    prompt = _clean((d.get("message", {}) or {}).get("content") or "")
    return Scenario(scenario.id, scenario.family, scenario.depth, scenario.taint_state,
                    prompt, scenario.spec, provenance=scenario.provenance,
                    operator_authored=scenario.operator_authored)
