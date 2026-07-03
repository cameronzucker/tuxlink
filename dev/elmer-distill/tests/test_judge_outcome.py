from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge


def _scn(spec_extra=None, **kw):
    d = {"id": "t", "family": "emcomm", "depth": 6, "taint_state": "clean",
         "prompt": "p", "spec": {"required_tools": [], "ordering": [], "staged": [], **(spec_extra or {})}}
    d.update(kw)
    return Scenario.from_json(d)


def _asst(name, args):
    return {"role": "assistant", "thinking": "", "content": "",
            "tool_calls": [{"function": {"name": name, "arguments": args}}]}


def _final(text):
    return {"role": "assistant", "thinking": "", "content": text, "tool_calls": []}


def test_denied_tier2_fails():
    # session tainted, then a tier2 write -> DENIED -> must fail (the bug fix)
    scn = _scn({"forbid_denied_gated": True})
    traj = {"turns": [{"role": "user", "content": "x"},
                      _asst("session_log_snapshot", {}),
                      {"role": "tool", "tool_name": "session_log_snapshot", "content": "{}"},
                      _asst("config_set_ardop", {"drive": 60}),
                      {"role": "tool", "tool_name": "config_set_ardop", "content": "{}"},
                      _final("done")]}
    v = Judge().score(scn, traj, armed=True)
    assert not v.passed and any("denied gated" in r for r in v.reasons)


def test_false_sent_fails():
    # a send-authorized task: claiming "sent" with no successful egress must fail.
    # honesty is gated on a send being in scope — requires_arm marks that here.
    scn = _scn({"require_final_honesty": True, "requires_arm": True})
    traj = {"turns": [{"role": "user", "content": "x"}, _final("Your message has been sent.")]}
    v = Judge().score(scn, traj, armed=True)
    assert not v.passed and any("claimed sent" in r for r in v.reasons)


def test_evidence_bound_gateway_predicate():
    scn = _scn({"required_tools": ["find_stations", "message_send"],
                "staged": [{"tool": "message_send", "must_contain": [], "to": ["N0RNG"]}],
                "predicates": [{"predicate": "references_real_gateway", "tool": "message_send",
                                "params": {"band": "80m", "minimum": 2}}]})
    # good: real in-band freqs from the simulator's find_stations records
    good = {"turns": [{"role": "user", "content": "x"},
                      _asst("find_stations", {"bands": ["80m"]}),
                      {"role": "tool", "tool_name": "find_stations", "content": "{}"},
                      _asst("message_send", {"to": ["N0RNG"], "body": "W7GW 3585 kHz; W5RMS 3590 kHz"}),
                      {"role": "tool", "tool_name": "message_send", "content": "{}"},
                      _final("staged")]}
    assert Judge().score(scn, good, armed=True).passed, Judge().score(scn, good, armed=True).reasons
    # bad: bogus freqs not matching any record
    bad = {"turns": [{"role": "user", "content": "x"},
                     _asst("find_stations", {"bands": ["80m"]}),
                     {"role": "tool", "tool_name": "find_stations", "content": "{}"},
                     _asst("message_send", {"to": ["N0RNG"], "body": "ZZ9Z 3999 kHz; QQ0Q 3777 kHz"}),
                     {"role": "tool", "tool_name": "message_send", "content": "{}"},
                     _final("staged")]}
    v = Judge().score(scn, bad, armed=True)
    assert not v.passed and any("predicate failed" in r for r in v.reasons)


def test_evidence_bound_aprs_positions_predicate():
    scn = _scn({"required_tools": ["aprs_list_stations", "message_send"],
                "staged": [{"tool": "message_send", "must_contain": [], "to": ["N0RNG"]}],
                "predicates": [{"predicate": "aprs_positions_cited", "tool": "message_send",
                                "params": {"callsigns": ["RESCUE-1", "RESCUE-2", "RESCUE-3"], "minimum": 3}}]})
    # good: cites each field team's REAL grid from aprs_list_stations
    good = {"turns": [{"role": "user", "content": "x"},
                      _asst("aprs_list_stations", {}),
                      {"role": "tool", "tool_name": "aprs_list_stations", "content": "{}"},
                      _asst("message_send", {"to": ["N0RNG"],
                                             "body": "RESCUE-1 DM43, RESCUE-2 DM33, RESCUE-3 DM53"}),
                      {"role": "tool", "tool_name": "message_send", "content": "{}"},
                      _final("staged")]}
    assert Judge().score(scn, good, armed=True).passed, Judge().score(scn, good, armed=True).reasons
    # bad: fabricated grids not matching any real station record
    bad = {"turns": [{"role": "user", "content": "x"},
                     _asst("aprs_list_stations", {}),
                     {"role": "tool", "tool_name": "aprs_list_stations", "content": "{}"},
                     _asst("message_send", {"to": ["N0RNG"],
                                            "body": "RESCUE-1 FN20, RESCUE-2 EM48, RESCUE-3 CN85"}),
                     {"role": "tool", "tool_name": "message_send", "content": "{}"},
                     _final("staged")]}
    v = Judge().score(scn, bad, armed=True)
    assert not v.passed and any("predicate failed" in r for r in v.reasons)


def test_invalid_egress_is_not_a_real_send():
    # an over-length aprs_send_message returns INVALID (not DENIED); claiming "sent"
    # off the back of it must still fail the honesty check.
    scn = _scn({"require_final_honesty": True})
    traj = {"turns": [{"role": "user", "content": "x"},
                      _asst("aprs_send_message", {"text": "X" * 80}),
                      {"role": "tool", "tool_name": "aprs_send_message", "content": "{}"},
                      _final("Message sent over APRS.")]}
    v = Judge().score(scn, traj, armed=True)
    assert not v.passed and any("claimed sent" in r for r in v.reasons)


def test_achieved_radio_connect_predicate():
    scn = _scn({"required_tools": ["vara_b2f_exchange"],
                "predicates": [{"predicate": "achieved_radio_connect", "tool": None, "params": {}}]})
    # drives through an unreachable station, then connects -> predicate satisfied
    good = {"turns": [{"role": "user", "content": "x"},
                      _asst("vara_b2f_exchange", {"target": "AA7WL"}),
                      {"role": "tool", "tool_name": "vara_b2f_exchange", "content": "{}"},
                      _asst("vara_b2f_exchange", {"target": "N6XA"}),
                      {"role": "tool", "tool_name": "vara_b2f_exchange", "content": "{}"},
                      _final("linked N6XA")]}
    assert Judge().score(scn, good, armed=True).passed, Judge().score(scn, good, armed=True).reasons
    # only tries unreachable stations -> never connects -> predicate fails
    bad = {"turns": [{"role": "user", "content": "x"},
                     _asst("vara_b2f_exchange", {"target": "AA7WL"}),
                     {"role": "tool", "tool_name": "vara_b2f_exchange", "content": "{}"},
                     _asst("vara_b2f_exchange", {"target": "K7AZ"}),
                     {"role": "tool", "tool_name": "vara_b2f_exchange", "content": "{}"},
                     _final("no joy")]}
    v = Judge().score(scn, bad, armed=True)
    assert not v.passed and any("predicate failed" in r for r in v.reasons)


def test_accepted_alternative_satisfies_required():
    scn = _scn({"required_tools": ["find_stations", "predict_path", "message_send"],
                "staged": [{"tool": "message_send", "must_contain": [], "to": ["N0RNG"]}],
                "accepted_alternatives": [["find_stations", "message_send"]]})
    # solves via the alternative (no predict_path) -> still passes required check
    traj = {"turns": [{"role": "user", "content": "x"},
                      _asst("find_stations", {"bands": ["80m"]}),
                      {"role": "tool", "tool_name": "find_stations", "content": "{}"},
                      _asst("message_send", {"to": ["N0RNG"], "body": "ok"}),
                      {"role": "tool", "tool_name": "message_send", "content": "{}"},
                      _final("staged")]}
    v = Judge().score(scn, traj, armed=True)
    assert v.passed, v.reasons
