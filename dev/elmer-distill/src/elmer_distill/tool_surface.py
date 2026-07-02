"""Classification of the 50-tool Elmer MCP surface.

Sets are verified against reference/tools.json (see the epic spec §4/§4a). The
taint set is EXACT: only untrusted-content reads taint the session. Egress and
Tier-2 writes require armed send-authority AND an un-tainted session.
"""
import json
import os

_DEFAULT = os.path.join(os.path.dirname(__file__), "..", "..", "reference", "tools.json")

# Untrusted-content reads that taint the session (verified against tools.json).
TAINT_TOOLS = {"mailbox_list", "message_read", "session_log_snapshot", "tauri_search_run"}

# Egress — keys the transmitter / network round-trip; needs armed + un-tainted.
EGRESS_TOOLS = {"cms_connect", "verify_cms_connection", "rig_tune", "ardop_connect",
                "ardop_b2f_exchange", "vara_b2f_exchange", "packet_connect"}

# Tier-2 config writes — also need armed + un-tainted.
TIER2_WRITE_TOOLS = {"config_set_ardop", "config_set_vara", "packet_config_set",
                     "config_set_grid", "position_set_source", "config_set_privacy",
                     "packet_set_listen", "mailbox_move"}

# Staging into the local outbox — ALWAYS available (no authority required).
STAGING_TOOLS = {"message_send", "send_form", "catalog_send_inquiry", "grib_send_request"}

# Stopping — NEVER gated.
STOP_TOOLS = {"cms_abort", "modem_ardop_disconnect", "vara_stop_session"}


def load_tool_names(path=None):
    """Return the set of tool names defined in tools.json."""
    with open(path or _DEFAULT) as f:
        return {t["function"]["name"] for t in json.load(f)}


def classify(tool):
    """Map a tool name to one of:
    taint_read | egress | tier2_write | staging | stop | read
    """
    if tool in TAINT_TOOLS:
        return "taint_read"
    if tool in EGRESS_TOOLS:
        return "egress"
    if tool in TIER2_WRITE_TOOLS:
        return "tier2_write"
    if tool in STAGING_TOOLS:
        return "staging"
    if tool in STOP_TOOLS:
        return "stop"
    return "read"
