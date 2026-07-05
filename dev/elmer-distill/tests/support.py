"""Test helpers for the per-scenario randomized gateway directory (tuxlink-74at8).

The judge replays each scenario through a directory synthesized from its id
(`build_gateways(seed_for_scenario(scenario.id))`). A test that hand-cites gateways
must draw them from that SAME directory, or a perfectly-grounded citation would
false-fail. These helpers derive the ground truth so tests stay coupled to the
directory the judge actually sees.
"""
from elmer_distill.simulator import build_gateways, seed_for_scenario


def directory_for(scenario_id):
    """The gateway directory the judge will replay for this scenario id."""
    return build_gateways(seed_for_scenario(scenario_id))


def in_band(scenario_id, band):
    return [g for g in directory_for(scenario_id) if g["band"] == band]


def gateway_clause(scenario_id, band, n):
    """A '<CALL> <freq> kHz; ...' citation of the first n in-band gateways — enough to
    satisfy references_real_gateway(band, minimum=n)."""
    gws = in_band(scenario_id, band)
    assert len(gws) >= n, f"{scenario_id} {band}: only {len(gws)} gateways, need {n}"
    return "; ".join(f"{g['callsign']} {int(g['freq_khz'])} kHz" for g in gws[:n])


def schedule_plan(scenario_id, bands, n):
    """n distinct-time blocks, each naming a real gateway + its own freq (cycling
    through the given bands' records) — satisfies schedule_has_blocks(n)."""
    recs = [g for b in bands for g in in_band(scenario_id, b)]
    assert recs, f"{scenario_id}: no records in {bands}"
    assert n <= 12, "schedule_plan uses even-hour blocks (max 12 distinct)"
    return "; ".join(f"{2 * i:02d}:00 {recs[i % len(recs)]['callsign']} "
                     f"{int(recs[i % len(recs)]['freq_khz'])} kHz" for i in range(n))


def reachable_calls(scenario_id):
    return [g["callsign"] for g in directory_for(scenario_id) if g["reachable"]]


def unreachable_calls(scenario_id):
    return [g["callsign"] for g in directory_for(scenario_id) if not g["reachable"]]
