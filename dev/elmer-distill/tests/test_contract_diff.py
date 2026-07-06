"""Task 11 — model-free tool-return contract diff.

Shows, without any model in the loop, the structural gap between the simulator's
`{ok:true}` stub and the real testserver's world-projected DTO. This is the
fabrication-void map: every field the testserver populates that the sim leaves
absent is a place the agent must otherwise invent.
"""
import json
import os

from elmer_distill.contract_diff import (
    sim_return,
    testserver_return as _testserver_return,  # aliased: bare name would be pytest-collected
    diff_tool,
    build_table,
    main,
)

FX = os.path.join(os.path.dirname(__file__), "fixtures", "scenarios")


def _world(name="grounded-gateways-01.json"):
    return json.load(open(os.path.join(FX, name)))["world"]


def test_sim_return_is_ok_stub():
    assert sim_return("find_stations") == {"ok": True}


def test_testserver_return_populated_from_world():
    ret = _testserver_return("find_stations", _world())
    callsigns = [g["callsign"] for g in ret["gateways"]]
    assert "W7ABC" in callsigns
    assert ret["operator_grid"] == "CN85"


def test_diff_reports_void_fields():
    d = diff_tool("find_stations", _world())
    # the sim stub has none of these; the testserver populates them
    assert "gateways" in d["void_fields"]
    assert "operator_grid" in d["void_fields"]


def test_diff_position_and_rig():
    dp = diff_tool("position_status", _world())
    assert "grid" in dp["void_fields"]
    dr = diff_tool("rig_status", _world())
    assert "vfo_hz" in dr["void_fields"]


def test_build_table_covers_all_requested_tools():
    tools = ["find_stations", "position_status", "rig_status", "solar_conditions"]
    table = build_table(tools, _world())
    assert set(r["tool"] for r in table) == set(tools)
    for row in table:
        assert "void_fields" in row


def test_main_emits_json_table(capsys, tmp_path):
    out = tmp_path / "table.json"
    fixture = os.path.join(FX, "grounded-gateways-01.json")
    rc = main(["--fixture", fixture, "--tools", "find_stations,position_status",
               "--out", str(out)])
    assert rc == 0
    data = json.load(open(out))
    assert any(r["tool"] == "find_stations" for r in data)
