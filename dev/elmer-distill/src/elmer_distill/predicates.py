"""Evidence-bound domain predicates for the Elmer eval.

Codex adrev B/F: correctness checks must bind to real tool outputs, not free-text
substring matches (which are gameable and false-fail). A claimed gateway/frequency
must actually appear in a `find_stations` record; a schedule must have real time
blocks; frequencies must fall in the real band allocation.
"""
import re

# Real amateur allocations (kHz).
BANDS = {
    "80m": (3500, 4000), "40m": (7000, 7300), "20m": (14000, 14350),
    "30m": (10100, 10150), "17m": (18068, 18168), "12m": (24890, 24990),
}


def freq_in_band(khz, band):
    lo, hi = BANDS[band]
    return lo <= float(khz) <= hi


def distance_band(km, lo, hi):
    return lo <= float(km) <= hi


_FREQ_RE = re.compile(r"\b(\d{4,5}(?:\.\d+)?)\b")


def parse_freqs_khz(text):
    """Extract plausible HF frequencies (kHz) from free text."""
    out = []
    for x in _FREQ_RE.findall(text):
        v = float(x)
        if 3000 <= v <= 30000:
            out.append(v)
    return out


# Split a staged body into per-station clauses. Real reports separate stations by
# comma / semicolon / newline / bullet; binding a callsign to its OWN grid/freq/gust
# *within one clause* defeats "list all callsigns then all grids" substring games
# and swapped-position fabrications (Codex adrev 2026-07-02, findings 3 & 4).
_CLAUSE_RE = re.compile(r"[,;\n|]|(?: - )|(?: / )")
_NUM_RE = re.compile(r"\d+")


def _clauses(text):
    return [c for c in _CLAUSE_RE.split(text) if c.strip()]


def references_real_gateway(staged_args_json, records, band, minimum):
    """True iff >= `minimum` DISTINCT real in-band gateways are cited with their OWN
    frequency in the same clause — i.e. the record's callsign AND a freq matching that
    record's freq_khz (within 1 kHz) co-occur. A real freq next to a bogus callsign
    does not count.
    """
    valid = set()
    for c in _clauses(staged_args_json):
        cu = c.upper()
        freqs = parse_freqs_khz(c)
        for r in records:
            if r.get("band") != band:
                continue
            cs = str(r["callsign"]).upper()
            if cs in cu and any(abs(f - r["freq_khz"]) <= 1 and freq_in_band(f, band) for f in freqs):
                valid.add((cs, r["freq_khz"]))
    return len(valid) >= minimum


_BLOCK_RE = re.compile(r"\b(?:[01]?\d|2[0-3]):[0-5]\d\b")


def schedule_has_blocks(text, n):
    """True iff the text contains >= n distinct HH:MM time blocks."""
    return len(set(_BLOCK_RE.findall(text))) >= n


def aprs_positions_cited(staged_args_json, records, callsigns, minimum=None):
    """True iff >= `minimum` of the named callsigns are cited with their OWN real grid
    in the same clause. Requiring callsign + its own grid together defeats swapped
    positions and 'all callsigns then all grids' substring games. `minimum` defaults
    to all named callsigns.
    """
    by_call = {r["callsign"].upper(): r for r in records}
    clauses = _clauses(staged_args_json)
    hits = 0
    for cs in callsigns:
        rec = by_call.get(cs.upper())
        if not rec:
            continue
        grid = str(rec["grid"]).upper()
        if any(cs.upper() in c.upper() and grid in c.upper() for c in clauses):
            hits += 1
    need = len(callsigns) if minimum is None else minimum
    return hits >= need


def aprs_gust_alert_cited(staged_args_json, records, threshold, minimum=1):
    """True iff >= `minimum` stations whose REAL gust_mph exceeds `threshold` are cited
    with a numeric value above `threshold` in the same clause. Requiring a real gusting
    station AND an over-threshold number together defeats name-only 'stations seen'
    lists and citing calm stations as hazards.
    """
    clauses = _clauses(staged_args_json)
    hits = 0
    for r in records:
        g = r.get("gust_mph")
        if g is None or float(g) <= threshold:
            continue
        cs = str(r["callsign"]).upper()
        for c in clauses:
            if cs in c.upper() and any(int(n) > threshold for n in _NUM_RE.findall(c)):
                hits += 1
                break
    return hits >= minimum
