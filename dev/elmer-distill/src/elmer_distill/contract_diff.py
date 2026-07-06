"""Model-free tool-return contract diff (cnz5o Task 11).

Produces the fabrication-void map WITHOUT a model: for each covered read tool it
compares the simulator's `{ok:true}` stub against the DTO the real testserver
projects from the fixture `world`. Every field the testserver populates that the
sim leaves absent is a "void field" — a place the agent must otherwise invent to
answer the prompt.

The testserver return is projected from the SAME fixture `world` the real Rust
testserver reads (`ScenarioStatus`/`ScenarioStation`/... over `Arc<World>`), so
this diff mirrors the real harness without running it. `--testserver-cmd` may
optionally capture the live DTO from the Rust harness on R2 instead.
"""
import argparse
import json
import subprocess
import sys

# Which world datum each covered tool projects from, and the DTO field set the
# testserver return carries (used to enumerate void fields when the datum exists).
# Field lists mirror the real DTOs in tuxlink-mcp-core/src/ports.rs.
_GATEWAY_FIELDS = ["mode", "channel", "callsign", "grid", "frequencies_khz",
                   "antenna", "distance_km", "distance_mi", "bearing_deg"]


def sim_return(tool):
    """The simulator's stubbed return for a read tool — a content-free ack."""
    return {"ok": True}


def _find_stations(world):
    stations = world.get("stations") or {}
    return {
        "gateways": stations.get("gateways", []),
        "fetched_at_ms": stations.get("fetched_at_ms"),
        "operator_grid": stations.get("operator_grid"),
    }


def _position_status(world):
    pos = world.get("position") or {}
    return {
        "has_fix": pos.get("has_fix"),
        "grid": pos.get("grid"),
        "source": pos.get("source"),
    }


def _rig_status(world):
    rig = world.get("rig")
    if not rig:
        # void world: unconfigured, all live fields None (matches ScenarioStatus).
        return {"vfo_hz": None, "mode": None, "ptt": None, "configured": False}
    return {
        "vfo_hz": rig.get("vfo_hz"),
        "mode": rig.get("mode"),
        "ptt": rig.get("ptt"),
        "configured": rig.get("configured", False),
    }


def _modem_status(world):
    modem = world.get("modem") or {}
    return {
        "kind": modem.get("kind"),
        "connected": modem.get("connected"),
        "state": modem.get("state"),
    }


def _solar_conditions(world):
    solar = world.get("solar")
    if not solar:
        return {}
    return {
        "sfi": solar.get("sfi"),
        "a_index": solar.get("a_index"),
        "k_index": solar.get("k_index"),
        "ssn": solar.get("ssn"),
        "updated_at_ms": solar.get("updated_at_ms"),
        "source": solar.get("source"),
    }


_PROJECTORS = {
    "find_stations": _find_stations,
    "position_status": _position_status,
    "rig_status": _rig_status,
    "modem_status": _modem_status,
    "solar_conditions": _solar_conditions,
}


def testserver_return(tool, world, testserver_cmd=None):
    """The DTO the testserver returns for `tool`, projected from `world`.

    When `testserver_cmd` is given, the live DTO is captured from the Rust harness
    (R2) instead of projected: the command is invoked with the tool name appended
    and its stdout parsed as JSON.
    """
    if testserver_cmd:
        proc = subprocess.run(
            list(testserver_cmd) + [tool],
            capture_output=True, text=True, check=True,
        )
        return json.loads(proc.stdout)
    proj = _PROJECTORS.get(tool)
    if proj is None:
        return {}
    return proj(world)


def _void_fields(ts_return):
    """Fields present (non-None, non-empty) in the testserver return but absent
    from the sim `{ok:true}` stub — i.e. everything the agent would otherwise
    invent."""
    fields = []
    for k, v in ts_return.items():
        if v is None:
            continue
        if isinstance(v, (list, dict, str)) and len(v) == 0:
            continue
        fields.append(k)
    return fields


def diff_tool(tool, world, testserver_cmd=None):
    """Diff one tool: the sim stub vs the world-projected testserver return."""
    sim = sim_return(tool)
    ts = testserver_return(tool, world, testserver_cmd=testserver_cmd)
    return {
        "tool": tool,
        "sim_return": sim,
        "testserver_return": ts,
        "void_fields": _void_fields(ts),
    }


def build_table(tools, world, testserver_cmd=None):
    """Build the contract-diff table over all requested tools."""
    return [diff_tool(t, world, testserver_cmd=testserver_cmd) for t in tools]


def main(argv=None):
    ap = argparse.ArgumentParser(description="Model-free tool-return contract diff.")
    ap.add_argument("--fixture", required=True, help="scenario fixture JSON path")
    ap.add_argument("--tools", required=True,
                    help="comma-separated tool names to diff")
    ap.add_argument("--out", help="write the JSON table here (default: stdout)")
    ap.add_argument("--testserver-cmd",
                    help="optional live capture command (space-separated); the "
                         "tool name is appended per tool")
    args = ap.parse_args(argv)

    with open(args.fixture) as f:
        world = json.load(f).get("world", {})
    tools = [t.strip() for t in args.tools.split(",") if t.strip()]
    ts_cmd = args.testserver_cmd.split() if args.testserver_cmd else None
    table = build_table(tools, world, testserver_cmd=ts_cmd)
    payload = json.dumps(table, indent=2)
    if args.out:
        with open(args.out, "w") as f:
            f.write(payload)
    else:
        print(payload)
    return 0


if __name__ == "__main__":
    sys.exit(main())
