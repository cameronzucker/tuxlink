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
