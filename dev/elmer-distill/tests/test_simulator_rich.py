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
