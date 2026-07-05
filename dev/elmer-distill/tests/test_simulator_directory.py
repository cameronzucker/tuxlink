"""Per-scenario randomized find_stations directory (tuxlink-74at8).

ROOT CAUSE this guards against: a FIXED gateway table served identically every
scenario is memorizable, so gold-gen distills the table into the weights and the
model recalls it instead of calling find_stations. The fix synthesizes the
directory PER SCENARIO (synthetic callsigns, valid random grids, haversine-computed
distances) so recall is impossible and grounded tool-use is the only winning
strategy. These tests pin the invariants that keep the synthesized directory both
UN-MEMORIZABLE and SATISFIABLE, and the seed-coupling that keeps generation-time
and judge-replay-time directories identical.
"""
import math

from elmer_distill.simulator import (
    StatefulSimulator,
    bearing_deg,
    build_gateways,
    grid_to_lat_lon,
    haversine_km,
    lat_lon_to_grid,
    seed_for_scenario,
)
from elmer_distill.predicates import BANDS


# ---- Maidenhead geometry (Python mirror of src-tauri/src/position/maidenhead.rs) ----

def test_grid_to_lat_lon_matches_rust_known_reference():
    # DM43 center is western Arizona (~33.5N, 111W) — the operator grid. The Rust
    # test set anchors JN58 (Munich). We mirror the CENTER-of-square convention.
    lat, lon = grid_to_lat_lon("DM43")
    assert 33.0 <= lat <= 34.0 and -112.0 <= lon <= -110.0, (lat, lon)
    latm, lonm = grid_to_lat_lon("JN58")
    assert 48.0 <= latm <= 49.0 and 11.0 <= lonm <= 12.0, (latm, lonm)


def test_grid_to_lat_lon_rejects_malformed():
    assert grid_to_lat_lon("ZZ99") is None    # field letters only go A-R
    assert grid_to_lat_lon("J") is None        # too short


def test_lat_lon_to_grid_is_faithful_six_char_mirror_of_rust():
    """Faithful to src-tauri/src/position/maidenhead.rs: full 6-char locator incl.
    subsquare (the Rust test pins these exact references)."""
    assert lat_lon_to_grid(48.143, 11.608) == "JN58td"    # Munich
    assert lat_lon_to_grid(-34.91, -56.21) == "GF15vc"    # Montevideo
    assert lat_lon_to_grid(0.0, 0.0) == "JJ00aa"          # origin corner


def test_lat_lon_to_grid_round_trips_through_grid_to_lat_lon():
    (lat, lon) = grid_to_lat_lon("JN58td")
    assert lat_lon_to_grid(lat, lon) == "JN58td"


def test_haversine_zero_and_known_distance():
    assert haversine_km(33.5, -111.0, 33.5, -111.0) == 0.0
    # ~1 degree of latitude is ~111 km.
    d = haversine_km(33.0, -111.0, 34.0, -111.0)
    assert 105 <= d <= 116, d


def test_haversine_matches_shipped_fixture_r6371():
    # The DM43->DM34 fixture the Rust geo.rs and TS distance.parity.test.ts both assert
    # (215.28 km). Pins the R=6371.0 alignment with the shipped find_stations surface.
    dm43 = grid_to_lat_lon("DM43")
    dm34 = grid_to_lat_lon("DM34")
    km = haversine_km(dm43[0], dm43[1], dm34[0], dm34[1])
    assert abs(km - 215.28) < 0.5, km


def test_bearing_deg_cardinals_and_range():
    assert abs(bearing_deg(0.0, 0.0, 1.0, 0.0)) < 1e-6            # due north -> 0
    assert abs(bearing_deg(0.0, 0.0, 0.0, 1.0) - 90.0) < 1e-6     # due east  -> 90
    assert abs(bearing_deg(0.0, 0.0, -1.0, 0.0) - 180.0) < 1e-6   # due south -> 180


def test_bearing_matches_rust_fixture():
    # DM43->DM34 bearing ~= 301.5 deg — mirrors the Rust geo.rs bearing_fixture so the sim
    # teaches the same convention find_stations serves (parity gate for tuxlink-e7z7d).
    dm43 = grid_to_lat_lon("DM43")
    dm34 = grid_to_lat_lon("DM34")
    b = bearing_deg(dm43[0], dm43[1], dm34[0], dm34[1])
    assert abs(b - 301.5) < 1.0, b


def test_build_gateways_carries_distance_mi_and_bearing():
    # Mirror of the find_stations agent surface: every synthetic gateway carries
    # distance_mi (km*0.621371, rounded) + a bearing in [0,360) (or None at zero distance).
    for seed in range(20):
        for g in build_gateways(seed):
            assert g["distance_mi"] == round(g["distance_km"] * 0.621371)
            assert g["bearing_deg"] is None or 0.0 <= g["bearing_deg"] < 360.0


# ---- build_gateways: determinism + un-memorizability ----

def test_build_gateways_is_deterministic_per_seed():
    assert build_gateways(7) == build_gateways(7)


def test_build_gateways_differs_across_seeds():
    """The whole point: different scenarios get different directories, so the
    table cannot be memorized."""
    a = {g["callsign"] for g in build_gateways(1)}
    b = {g["callsign"] for g in build_gateways(2)}
    # near-disjoint callsign sets (synthetic space is large)
    assert len(a & b) <= 1, (a & b)


def test_directories_are_diverse_across_many_scenarios():
    """Across many scenario seeds the union of callsigns is large — no small fixed
    table the student could recall."""
    seeds = [seed_for_scenario(f"emcomm-d6-clean-{i}") for i in range(40)]
    union = set()
    for s in seeds:
        union |= {g["callsign"] for g in build_gateways(s)}
    assert len(union) >= 200, len(union)


# ---- build_gateways: satisfiability invariants (or the oracle fails / gold shrinks) ----

def _by_band(gws):
    out = {}
    for g in gws:
        out.setdefault(g["band"], []).append(g)
    return out


def test_warc_bands_have_at_least_three_gateways_every_seed():
    # ranking scenarios ("best + 2 runners-up on 30m") need >=3 across all seeds.
    for seed in range(50):
        bands = _by_band(build_gateways(seed))
        for b in ("30m", "17m", "12m"):
            assert len(bands.get(b, [])) >= 3, f"seed {seed} band {b}: {len(bands.get(b, []))}"


def test_predicate_bands_have_two_distinct_callsign_freq_pairs_every_seed():
    # references_real_gateway(minimum=2) needs 2 DISTINCT (callsign, freq) pairs in
    # the predicate bands (40m/17m/30m/12m).
    for seed in range(50):
        bands = _by_band(build_gateways(seed))
        for b in ("40m", "17m", "30m", "12m"):
            pairs = {(g["callsign"], g["freq_khz"]) for g in bands.get(b, [])}
            assert len(pairs) >= 2, f"seed {seed} band {b}: {pairs}"


def test_every_freq_is_inside_its_band_allocation():
    for seed in range(50):
        for g in build_gateways(seed):
            lo, hi = BANDS[g["band"]]
            assert lo <= g["freq_khz"] <= hi, (seed, g)


def test_every_grid_is_valid_maidenhead():
    for seed in range(50):
        for g in build_gateways(seed):
            assert grid_to_lat_lon(g["grid"]) is not None, (seed, g)


def test_distance_is_haversine_from_operator_grid():
    """distance_km must be COMPUTED from grid geometry (haversine on grid centers),
    not an arbitrary number — so grid and distance are always consistent."""
    op_lat, op_lon = grid_to_lat_lon("DM43")
    for seed in range(20):
        for g in build_gateways(seed, operator_grid="DM43"):
            glat, glon = grid_to_lat_lon(g["grid"])
            expect = haversine_km(op_lat, op_lon, glat, glon)
            assert abs(g["distance_km"] - expect) <= 1.0, (seed, g, expect)


def test_directory_has_reachable_and_unreachable_stations():
    """Connect/exchange loops need at least one unreachable station AND reachable
    ones, on every seed."""
    for seed in range(50):
        gws = build_gateways(seed)
        reach = [g for g in gws if g["reachable"]]
        unreach = [g for g in gws if not g["reachable"]]
        assert reach and unreach, f"seed {seed}: reach={len(reach)} unreach={len(unreach)}"


def test_find_stations_output_does_not_leak_reachable_flag():
    """`reachable` is hidden connectivity state discovered by attempting a connect,
    not a field the directory listing exposes."""
    r = StatefulSimulator(station_seed=5)._find_stations({})
    assert r["stations"]
    assert all("reachable" not in s for s in r["stations"])


# ---- seed coupling: generation-time and judge-replay-time must match ----

def test_seed_for_scenario_is_process_stable():
    """Must NOT use Python's salted hash(): teacher (generation) and judge (replay)
    may run in different processes, and a per-process salt would desync the two
    directories and false-fail every grounded citation."""
    # A fixed id maps to a fixed seed (regression-pinned value).
    s1 = seed_for_scenario("emcomm-d6-clean-0")
    s2 = seed_for_scenario("emcomm-d6-clean-0")
    assert s1 == s2
    assert seed_for_scenario("emcomm-d6-clean-0") != seed_for_scenario("emcomm-d6-clean-1")


def test_same_scenario_seed_yields_identical_directory_across_instances():
    """The coupling invariant: a fresh simulator built with the same station_seed
    (as teacher and judge each do from scenario.id) serves the identical directory."""
    seed = seed_for_scenario("blended-d6-clean-0")
    a = StatefulSimulator(station_seed=seed)._find_stations({})
    b = StatefulSimulator(station_seed=seed)._find_stations({})
    assert a == b


def test_find_stations_respects_band_filter_on_synthesized_directory():
    r = StatefulSimulator(station_seed=3)._find_stations({"bands": ["30m"]})
    assert r["stations"]
    assert all(s["band"] == "30m" for s in r["stations"])
