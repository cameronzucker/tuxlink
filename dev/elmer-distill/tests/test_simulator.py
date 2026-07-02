from elmer_distill.simulator import StatefulSimulator


def test_egress_denied_when_disarmed():
    sim = StatefulSimulator(armed=False)
    r = sim.apply("cms_connect", {})
    assert r.get("error") == "DENIED"


def test_egress_denied_when_tainted_even_if_armed():
    sim = StatefulSimulator(armed=True)
    sim.apply("session_log_snapshot", {})     # taints
    assert sim.tainted is True
    r = sim.apply("cms_connect", {})
    assert r.get("error") == "DENIED"


def test_egress_ok_when_armed_and_clean():
    sim = StatefulSimulator(armed=True)
    r = sim.apply("cms_connect", {})
    assert "error" not in r


def test_tier2_write_gated_like_egress():
    sim = StatefulSimulator(armed=False)
    assert sim.apply("config_set_ardop", {"drive": 50}).get("error") == "DENIED"
    sim.arm()
    assert "error" not in sim.apply("config_set_ardop", {"drive": 50})


def test_staging_always_ok_and_recorded():
    sim = StatefulSimulator(armed=False)
    r = sim.apply("message_send", {"to": "a@b.com", "subject": "x", "body": "hello"})
    assert r["staged_id"] and len(sim.staged) == 1


def test_stop_never_gated():
    sim = StatefulSimulator(armed=False)
    sim.apply("session_log_snapshot", {})     # taint + disarmed
    assert "error" not in sim.apply("cms_abort", {})


def test_benign_reads_do_not_taint():
    sim = StatefulSimulator(armed=True)
    for t in ("catalog_list", "docs_search", "position_status", "user_folders_list"):
        sim.apply(t, {})
    assert sim.tainted is False
