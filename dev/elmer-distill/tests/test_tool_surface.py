from elmer_distill.tool_surface import (
    load_tool_names, TAINT_TOOLS, EGRESS_TOOLS, STAGING_TOOLS, STOP_TOOLS, classify)


def test_all_50_tools_load():
    names = load_tool_names()
    assert len(names) == 50
    assert "position_status" in names and "cms_connect" in names


def test_taint_set_is_exact():
    assert TAINT_TOOLS == {"mailbox_list", "message_read", "session_log_snapshot", "tauri_search_run"}
    for benign in ("catalog_list", "docs_search", "user_folders_list"):
        assert benign not in TAINT_TOOLS


def test_classify():
    assert classify("cms_connect") == "egress"
    assert classify("config_set_ardop") == "tier2_write"
    assert classify("message_send") == "staging"
    assert classify("cms_abort") == "stop"
    assert classify("position_status") == "read"
    assert classify("session_log_snapshot") == "taint_read"
