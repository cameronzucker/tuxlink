"""Discrimination guards for the operator-authored greenfield gate scenarios
(tuxlink-6zkb6). Each predicate-bearing scenario must separate a competent
trajectory from its targeted failure mode; if a judge/simulator change softens
any of these, it breaks here.
"""
import json
import os

from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge

_DIR = os.path.join(os.path.dirname(__file__), "..", "gate", "candidates")


def _load(scenario_id):
    return Scenario.from_json(json.load(open(os.path.join(_DIR, f"{scenario_id}.json"))))


def _a(name, args):
    return {"role": "assistant", "thinking": "", "content": "",
            "tool_calls": [{"function": {"name": name, "arguments": args}}]}


def _final(text):
    return {"role": "assistant", "thinking": "", "content": text, "tool_calls": []}


def test_thirtym_reach_discriminates():
    s = _load("thirtym-reach-cms-aprs-allhands")
    good = {"turns": [{"role": "user", "content": "x"},
                      _a("position_status", {}), _a("find_stations", {"bands": ["30m"]}),
                      _a("predict_path", {"frequencies_khz": [10125]}),
                      _a("message_send", {"to": ["recipient@domain.com"],
                                          "body": "Top 30m: AA7WL 10125, W5RMS 10140, N6XA 10132"}),
                      _a("cms_connect", {}),
                      _a("aprs_send_message", {"text": "30m outbound comms established"}),
                      _final("done")]}
    bad = {"turns": [{"role": "user", "content": "x"},
                     _a("position_status", {}), _a("find_stations", {"bands": ["30m"]}),
                     _a("message_send", {"to": ["recipient@domain.com"],
                                         "body": "Top 30m: ZZ9Z 9999, QQ0Q 9998, XX1X 9997"}),
                     _a("cms_connect", {}), _a("aprs_send_message", {"text": "established"}),
                     _final("done")]}
    assert Judge().score(s, good, armed=True).passed, Judge().score(s, good, armed=True).reasons
    assert not Judge().score(s, bad, armed=True).passed   # fabricated freqs + skipped predict_path


def test_wx_gust_broadcast_discriminates():
    s = _load("aprs-wx-gust-broadcast")
    good = {"turns": [{"role": "user", "content": "x"},
                      _a("aprs_list_stations", {}),
                      _a("message_send", {"body": "Wind alert: WX-CAVE 41mph, KE7QRP-9 32mph gusting"}),
                      _a("aprs_send_message", {"text": "WIND WX-CAVE 41 KE7QRP-9 32 gusts >25"}),
                      _final("done")]}
    bad = {"turns": [{"role": "user", "content": "x"},
                     _a("aprs_list_stations", {}),
                     _a("message_send", {"body": "Wind alert: WX-MESA gusting"}),   # WX-MESA is calm
                     _a("aprs_send_message", {"text": "calm"}), _final("done")]}
    assert Judge().score(s, good, armed=True).passed, Judge().score(s, good, armed=True).reasons
    assert not Judge().score(s, bad, armed=True).passed   # cited a calm station as a hazard


def test_warc_capstone_discriminates():
    s = _load("warc-vara-plan-drive-p2p")
    # a GROUNDED 12-block plan: each slot names a real WARC gateway + its own freq
    # (from find_stations), across 30m/17m/12m. Bare band labels are no longer a plan.
    plan = ("00:00 AA7WL 10125; 02:00 K7AZ 10118; 04:00 KB0RFC 18105; 06:00 W7GW 18110; "
            "08:00 KM7N 24915; 10:00 W5RMS 24920; 12:00 N6XA 10132; 14:00 W5RMS 10140; "
            "16:00 N6XA 18130; 18:00 K7AZ 24905; 20:00 AA7WL 10125; 22:00 KM7N 24915")
    good = {"turns": [{"role": "user", "content": "x"},
                      _a("position_status", {}), _a("find_stations", {"bands": ["30m", "17m", "12m"]}),
                      _a("predict_path", {"frequencies_khz": [10125]}),
                      _a("message_send", {"body": plan}),
                      _a("vara_b2f_exchange", {"target": "AA7WL"}),   # unreachable
                      _a("vara_b2f_exchange", {"target": "N6XA"}),    # connects
                      _a("aprs_send_message", {"text": "HF plan established+confirmed, P2P sent"}),
                      _final("done")]}
    bad = {"turns": [{"role": "user", "content": "x"},
                     _a("position_status", {}), _a("find_stations", {"bands": ["30m"]}),
                     _a("message_send", {"body": "Plan: call around on 30m sometime"}),   # no 12 blocks
                     _a("vara_b2f_exchange", {"target": "AA7WL"}),
                     _a("vara_b2f_exchange", {"target": "K7AZ"}),     # never connects
                     _a("aprs_send_message", {"text": "done"}), _final("done")]}
    assert Judge().score(s, good, armed=True).passed, Judge().score(s, good, armed=True).reasons
    v = Judge().score(s, bad, armed=True)
    assert not v.passed   # missing blocks + never connected + skipped predict_path
