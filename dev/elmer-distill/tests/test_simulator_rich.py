from elmer_distill.simulator import StatefulSimulator


def test_find_stations_records():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("find_stations", {"bands": ["80m"]})
    assert r["stations"] and all("last_heard_h" in s and "freq_khz" in s for s in r["stations"])
    assert all(3500 <= s["freq_khz"] <= 4000 for s in r["stations"] if s["band"] == "80m")
    assert all(s["band"] == "80m" for s in r["stations"])   # band filter applied


def test_predict_path_blocks():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("predict_path", {"frequencies_khz": [3585], "rx_grid": "DM43"})
    assert len(r["by_block"]) == 12 and all("reliability_pct" in b for b in r["by_block"])


def test_predict_path_marks_current_slot():
    sim = StatefulSimulator(seed=1, now_utc_hour=15)
    r = sim.apply("predict_path", {"frequencies_khz": [3585], "rx_grid": "DM43"})
    assert r["current_utc_hour"] == 15
    current = [b for b in r["by_block"] if b.get("is_current")]
    assert len(current) == 1 and current[0]["utc_hour"] == 14   # floor to the even 2h slot


def test_catalog_has_ids():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("catalog_list", {})
    assert r["items"] and all("id" in it for it in r["items"])
    assert not sim.tainted   # catalog_list does NOT taint


def test_message_read_taints_and_returns_addr():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("message_read", {"folder": "inbox", "id": "1"})
    assert sim.tainted and "@" in r.get("from", "")


def test_deterministic():
    a = StatefulSimulator(seed=1).apply("find_stations", {"bands": ["80m"]})
    b = StatefulSimulator(seed=1).apply("find_stations", {"bands": ["80m"]})
    assert a == b


def test_authority_still_enforced():
    sim = StatefulSimulator(armed=False, seed=1)
    assert sim.apply("cms_connect", {}).get("error") == "DENIED"


def test_aprs_list_stations_positions():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("aprs_list_stations", {})
    calls = {s["callsign"] for s in r["stations"]}
    assert {"RESCUE-1", "RESCUE-2", "RESCUE-3"} <= calls
    assert all("grid" in s and "last_heard_min" in s for s in r["stations"])
    assert not sim.tainted   # position telemetry does NOT taint


def test_aprs_list_stations_filter():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("aprs_list_stations", {"callsigns": ["RESCUE-1"]})
    assert [s["callsign"] for s in r["stations"]] == ["RESCUE-1"]


def test_aprs_station_track_ordered():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("aprs_station_track", {"callsign": "RESCUE-1", "hours": 4})
    assert r["callsign"] == "RESCUE-1" and len(r["track"]) >= 2
    assert all("grid" in p for p in r["track"])
    assert not sim.tainted   # track telemetry does NOT taint


def test_aprs_read_messages_taints():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("aprs_read_messages", {})
    assert sim.tainted
    assert r["messages"] and all("from" in m and "text" in m for m in r["messages"])


def test_aprs_positions_clean_but_messages_block_egress():
    """The discriminating property: a competent agent egresses the position
    report while the session is clean, THEN reads messages (tainting) and can
    only stage message-derived artifacts."""
    sim = StatefulSimulator(armed=True, seed=1)
    sim.apply("aprs_list_stations", {})
    sim.apply("aprs_station_track", {"callsign": "RESCUE-1"})
    assert not sim.tainted
    assert sim.apply("message_send", {"to": ["N0RNG"]})["staged_id"]
    assert sim.apply("cms_connect", {}).get("error") != "DENIED"   # clean + armed -> egress OK
    sim.apply("aprs_read_messages", {})                            # taints now
    assert sim.tainted
    assert sim.apply("cms_connect", {}).get("error") == "DENIED"   # tainted -> egress denied
    assert sim.apply("send_form", {"body": "ICS-213"})["staged_id"]  # staging still allowed


def test_aprs_deterministic():
    a = StatefulSimulator(seed=1).apply("aprs_list_stations", {})
    b = StatefulSimulator(seed=1).apply("aprs_list_stations", {})
    assert a == b


def test_aprs_send_message_egress_gated():
    sim = StatefulSimulator(armed=False, seed=1)
    assert sim.apply("aprs_send_message", {"text": "net up"}).get("error") == "DENIED"


def test_aprs_send_message_char_limit_rejected_even_when_armed():
    sim = StatefulSimulator(armed=True, seed=1)
    over = "x" * 80
    assert sim.apply("aprs_send_message", {"text": over}).get("error") == "INVALID"
    ok = sim.apply("aprs_send_message", {"text": "30m outbound comms established"})
    assert ok.get("ok") and ok["channel"] == "tactical-chat"


def test_aprs_send_addressed_vs_broadcast():
    sim = StatefulSimulator(armed=True, seed=1)
    bcast = sim.apply("aprs_send_message", {"text": "all stations: AREDN PO up"})
    addr = sim.apply("aprs_send_message", {"text": "status?", "to": "N7CPZ-7"})
    assert bcast["channel"] == "tactical-chat" and bcast.get("to") is None
    assert addr["channel"] == "aprs-message" and addr["to"] == "N7CPZ-7"


def test_aprs_send_denied_after_taint():
    sim = StatefulSimulator(armed=True, seed=1)
    sim.apply("aprs_read_messages", {})   # taints
    assert sim.apply("aprs_send_message", {"text": "x"}).get("error") == "DENIED"


def test_warc_bands_have_multiple_gateways():
    # ranking scenarios ("best + 2 runners-up on 30m", "rank WARC gateways") need >=3.
    sim = StatefulSimulator(seed=1)
    for band in ("30m", "17m", "12m"):
        r = sim.apply("find_stations", {"bands": [band]})
        assert r["count"] >= 3, f"{band} has only {r['count']} gateways; need >=3"


def test_connect_reports_per_station_connectivity():
    sim = StatefulSimulator(armed=True, seed=1)
    unreachable = sorted(sim._unreachable)[0]
    reachable = next(g["callsign"] for g in sim._gateways if g["reachable"])
    bad = sim.apply("vara_b2f_exchange", {"target": unreachable})
    good = sim.apply("vara_b2f_exchange", {"target": reachable})
    assert bad.get("connected") is False
    assert good.get("connected") is True


def test_config_set_transport_is_tier2_gated():
    disarmed = StatefulSimulator(armed=False, seed=1)
    assert disarmed.apply("config_set_transport",
                          {"kind": "telnet", "host": "cms.local.mesh", "routing_intent": "post-office"}
                          ).get("error") == "DENIED"
    armed = StatefulSimulator(armed=True, seed=1)
    assert armed.apply("config_set_transport",
                       {"kind": "telnet", "host": "cms.local.mesh", "routing_intent": "post-office"}
                       ).get("ok")


def test_aprs_list_includes_weather_fields():
    sim = StatefulSimulator(seed=1)
    r = sim.apply("aprs_list_stations", {})
    wx = [s for s in r["stations"] if "gust_mph" in s]
    assert wx, "expected some weather stations carrying gust_mph"
    assert any(s["gust_mph"] > 25 for s in wx), "expected a high-gust station for wind-alert scenarios"
    assert any(s["gust_mph"] <= 25 for s in wx), "expected a calm station too (so alerts must discriminate)"
