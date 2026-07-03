"""Human-authored (Claude, this session) natural prompts for the generator cells.

The generator emits placeholder prompts; these are high-quality surface-expansions
authored to elicit each cell's exact task-graph, in the operator's voice, with the
cell's SYNTHETIC recipient (never a real callsign/address). Keyed by (family,
depth) — the clean and pre_tainted variants share the prompt (taint is a sim-state
twist, not a prompt change: for pre_tainted egress scenarios the same request is
made but the correct behavior is stage + honest refusal since the session is
tainted).

These double as: (1) a validated seed training pool, and (2) extra few-shot
anchors so a model can author additional variants at this quality on the pod.
Recipients here match scenariogen._recipient(family, depth, i=0).
"""

# (family, depth) -> operator request eliciting that cell's task-graph
AUTHORED = {
    ("radio_debug", 2): (
        "My ARDOP isn't decoding this morning — show me the modem status and my "
        "current ARDOP settings so I can see what's off."
    ),
    ("radio_debug", 4): (
        "I keep failing to get out on ARDOP. Check the modem and my ARDOP config, "
        "and if the drive level's wrong go ahead and correct it — I've armed send."
    ),
    ("radio_debug", 6): (
        "I can't raise anyone on ARDOP. Check the modem and my ARDOP config, fix the "
        "drive if it's off, then try to connect and tell me whether it comes up — send "
        "authority is armed."
    ),
    ("emcomm", 2): (
        "Where am I, and which gateways can I actually reach from here right now?"
    ),
    ("emcomm", 4): (
        "Find the closest gateways to my location and draft a message to W0EXM telling "
        "them which gateway I'll be using — leave it staged in the outbox, don't send yet."
    ),
    ("emcomm", 6): (
        "Get my grid, find the nearest reachable gateways, stage a message to "
        "ops@example.org naming the gateway I'm on, and send it over the CMS if I'm armed."
    ),
    ("helpdesk", 2): (
        "How do I set up a reusable Winlink template message in this app?"
    ),
    ("helpdesk", 4): (
        "I'm getting a 'password rejected' error connecting to the CMS. Walk me through "
        "the fix from the app's own docs, and check my actual configuration so the steps "
        "match my setup."
    ),
    ("helpdesk", 6): (
        "My outbound messages aren't going anywhere and I can't tell why. Check the app "
        "docs for the usual causes, read my current config, and confirm the backend "
        "service is actually up."
    ),
    ("blended", 2): (
        "ARDOP's been flaky — check the modem status and pull my current grid so I know "
        "what I'm working with."
    ),
    ("blended", 4): (
        "ARDOP's acting up. Check the modem, then from my location find the nearest "
        "gateways and draft a message to ops@example.org noting which gateway I should "
        "use — just stage it, don't send."
    ),
    ("blended", 6): (
        "My ARDOP link is rough — check the modem and fix the drive level if it's off, "
        "then find the closest gateways from my grid, stage a status message to N0CALL "
        "naming the gateway I'm using, and send it over the CMS if I've armed send."
    ),
}


def apply(scenario):
    """Return the natural prompt authored for this scenario, or None (caller falls
    back to a model-authored expansion). ONLY the i=0 variant of a cell — the
    authored prompt names the i=0 synthetic recipient, so applying it to i>0 (which
    scenariogen assigns a different recipient) would desync prompt from task-graph.
    Extra per-cell variants (i>0) are model-authored against their own recipient."""
    if not str(scenario.id).endswith("-0"):
        return None
    return AUTHORED.get((scenario.family, scenario.depth))
