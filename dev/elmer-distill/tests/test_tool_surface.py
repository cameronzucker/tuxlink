from elmer_distill.tool_surface import (
    load_tool_names, TAINT_TOOLS, EGRESS_TOOLS, STAGING_TOOLS, STOP_TOOLS, classify)


def test_tool_count():
    names = load_tool_names()
    assert len(names) == 55
    assert "position_status" in names and "cms_connect" in names
    assert "config_set_transport" in names


def test_aprs_tools_present():
    names = load_tool_names()
    assert {"aprs_list_stations", "aprs_station_track", "aprs_read_messages",
            "aprs_send_message"} <= names


def test_taint_set_is_exact():
    assert TAINT_TOOLS == {"mailbox_list", "message_read", "session_log_snapshot",
                           "tauri_search_run", "aprs_read_messages"}
    for benign in ("catalog_list", "docs_search", "user_folders_list"):
        assert benign not in TAINT_TOOLS


def test_classify():
    assert classify("cms_connect") == "egress"
    assert classify("config_set_ardop") == "tier2_write"
    assert classify("config_set_transport") == "tier2_write"
    assert classify("message_send") == "staging"
    assert classify("cms_abort") == "stop"
    assert classify("position_status") == "read"
    assert classify("session_log_snapshot") == "taint_read"


def test_classify_aprs():
    # position/track are structured telemetry -> plain reads; message read taints.
    assert classify("aprs_list_stations") == "read"
    assert classify("aprs_station_track") == "read"
    assert classify("aprs_read_messages") == "taint_read"
    # sending over APRS/tactical chat keys the transmitter -> egress.
    assert classify("aprs_send_message") == "egress"
