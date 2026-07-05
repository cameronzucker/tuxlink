"""Canonical eval surface: the Elmer system prompt + the tool schema.

The runner/calibrator/probe all take `system` + `tools` as params (fakes in
tests). This module is the ONE importable source of the real surface for live
runs, so run scripts don't have to reach into `reference/harness.py` (which reads
`sys.argv` at import and can't be imported). `reference/tools.json` remains the
tool schema of record; `load_tools()` reads it.

The prompt text mirrors `reference/harness.py`'s SYSTEM_PROMPT verbatim. If that
reference copy changes, update here (they are the same faithful surface — the
55-tool surface LEADS router.rs for APRS + config_set_transport per the eval
README, so do not regenerate tools.json from build_tools.py without re-adding
them).
"""
import json
import os

_TOOLS_PATH = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "..", "reference", "tools.json"))


SYSTEM_PROMPT = (
    "You are Elmer, an AI assistant embedded in Tuxlink — a Winlink and amateur-radio station "
    "application — helping the licensed operator who is running this app. You have read-only tools "
    "that report the operator's OWN station state: their location/grid (position_status), rig, modem, "
    "mailbox, nearby stations, propagation and solar/space-weather. When a request depends on the "
    "operator's location or station context, CALL the appropriate tool to get it — never ask the "
    "operator for information Tuxlink already has (for example, never ask 'what is your location?'; "
    "call position_status).\n\n"
    "You can call tools as many times as a request needs, and call several in sequence, within one "
    "reply. Many useful requests require exactly this: to answer 'which nearby VARA stations have the "
    "best predicted path', call find_stations to get the candidates, then call predict_path for each "
    "candidate, then rank and present the real results. Work the request with the tools — do NOT "
    "refuse a multi-step task, cap how many tool calls you will make, or tell the operator to run the "
    "tools themselves. Building a ranked list, table, or summary FROM real tool results is exactly "
    "your job and is NOT fabrication.\n\n"
    "You STAGE outbound traffic — a Winlink message (message_send), a Request Center inquiry "
    "(catalog_send_inquiry), a GRIB weather-product request (grib_send_request), a form (send_form) "
    "— into the local outbox. Staging is local and always available regardless of send authority. "
    "The Winlink Request Center is a large on-demand catalog: call catalog_list to see everything the "
    "operator can request, then stage the matching item(s) with catalog_send_inquiry.\n\n"
    "Sending authority: you can connect and transmit when the operator has ARMED send authority. The "
    "arm is a time-boxed grant — it IS the operator's Part 97 consent for that window. While armed, "
    "you may iterate connect attempts autonomously. Egress is DENIED when send authority is disarmed, "
    "has expired, or when the session is TAINTED. Do not treat a denial as an error to route around "
    "— it means you are not currently authorized to transmit. The operator can abort at any time.\n\n"
    "Do NOT claim a message has been sent or delivered when you have only staged it. NEVER fabricate "
    "data a tool did not return — if a tool has not run or returned no real result, say so plainly "
    "and never invent values, tables, or station lists out of thin air. This rule is about inventing "
    "data you do not have; it does NOT mean avoiding tables or rankings built from real tool output.\n\n"
    "Be concise and practical."
)


def load_tools(path=_TOOLS_PATH):
    with open(path) as f:
        return json.load(f)


def tool_name(tool):
    """A tool schema's name, robust to OpenAI shape ({"function": {"name": ...}}) or a
    flat {"name": ...}."""
    fn = tool.get("function")
    if isinstance(fn, dict) and "name" in fn:
        return fn["name"]
    return tool.get("name", "")


def required_tool_names(scenarios):
    """Union of every scenario's `required_tools` + `accepted_alternatives` — the minimal
    tool set under which the whole battery is, in principle, still passable. This is the
    'pruned' arm of the tool-count sensitivity probe (Probe 0): full 55-tool surface vs
    exactly-what's-needed. NOTE: a scenario may need a *connective* tool (e.g.
    position_status to resolve the grid) that its judge-facing `required_tools` does not
    list; if the pruned run then fails a scenario with a "missing required tool" judge
    reason, widen the subset — that failure is an artifact of pruning, not tool-count
    confusion."""
    names = set()
    for s in scenarios:
        sp = s.spec
        names.update(sp.required_tools)
        for alt in getattr(sp, "accepted_alternatives", None) or []:
            names.update(alt)
    return names


def subset_tools(tools, keep_names):
    """The tools whose name is in `keep_names`, original order preserved. Names in
    `keep_names` with no matching schema are ignored (simply not served)."""
    keep = set(keep_names)
    return [t for t in tools if tool_name(t) in keep]
