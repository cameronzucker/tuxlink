"""Canonical eval surface loads + stays in sync with reference/harness.py.

The runner imports SYSTEM_PROMPT + tools from elmer_distill.surface (harness.py
reads sys.argv at import and can't be imported). This guards that (a) the surface
loads the full 55-tool schema, and (b) the prompt text has not drifted from the
reference copy — a silent divergence would eval the model against a different
system prompt than the reference harness documents.
"""
import ast
import os
import re

from elmer_distill.surface import SYSTEM_PROMPT, load_tools

HERE = os.path.dirname(__file__)
HARNESS = os.path.normpath(os.path.join(HERE, "..", "reference", "harness.py"))


def test_load_tools_is_full_surface():
    tools = load_tools()
    assert len(tools) == 55
    names = {t["function"]["name"] for t in tools}
    # APRS + transport tools the surface LEADS router.rs on
    assert "position_status" in names and "find_stations" in names


def test_prompt_matches_reference_harness():
    src = open(HARNESS).read()
    # pull the parenthesised SYSTEM_PROMPT = ( ... ) literal and eval just that
    m = re.search(r"SYSTEM_PROMPT\s*=\s*\((.*?)\)\s*\n\nPROMPT_TEXT", src, re.DOTALL)
    assert m, "could not locate SYSTEM_PROMPT literal in reference/harness.py"
    # ast.literal_eval (NOT eval) — safe: the match is an implicitly-concatenated
    # string literal, which parses to a single constant node.
    ref_prompt = ast.literal_eval("(" + m.group(1) + ")")
    assert SYSTEM_PROMPT == ref_prompt, "surface.py SYSTEM_PROMPT drifted from reference/harness.py"
