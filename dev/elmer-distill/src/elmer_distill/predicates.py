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
# MHz-with-unit, e.g. "3.750 MHz" / "14.105 MHz" — the same real dial a model may
# cite in MHz instead of kHz. Requiring the explicit unit avoids treating arbitrary
# decimals as frequencies (base-20B adrev 2026-07-02, must-fix 3).
_FREQ_MHZ_RE = re.compile(r"\b(\d{1,2}(?:\.\d{1,4})?)\s*MHz\b", re.I)


def parse_freqs_khz(text):
    """Extract plausible HF frequencies (kHz) from free text — bare kHz values and
    MHz-with-unit citations (converted to kHz)."""
    out = []
    for x in _FREQ_RE.findall(text):
        v = float(x)
        if 3000 <= v <= 30000:
            out.append(v)
    for x in _FREQ_MHZ_RE.findall(text):
        mhz = float(x)
        if 1.8 <= mhz <= 30:
            out.append(mhz * 1000)
    return out


# Split a staged body into per-station clauses. Real reports separate stations by
# comma / semicolon / newline / bullet; binding a callsign to its OWN grid/freq/gust
# *within one clause* defeats "list all callsigns then all grids" substring games
# and swapped-position fabrications (Codex adrev 2026-07-02, findings 3 & 4).
_CLAUSE_RE = re.compile(r"[,;\n|]|(?: - )|(?: / )")
_NUM_RE = re.compile(r"\d+")


def _clauses(text):
    return [c for c in _CLAUSE_RE.split(text) if c.strip()]


def _cites_callsign(cs_upper, text_upper):
    """True iff `cs_upper` appears as a whole token, not embedded in a larger
    alphanumeric run — 'AA7WL' matches '| AA7WL |' but NOT 'NOTAA7WLX' (Codex adrev
    2026-07-04, substring-callsign false-positive). Boundary = anything but [A-Z0-9],
    so hyphenated tactical calls ('RESCUE-1', 'WX-CAVE') still match as whole tokens."""
    return re.search(r"(?<![A-Z0-9])" + re.escape(cs_upper) + r"(?![A-Z0-9])",
                     text_upper) is not None


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
            if _cites_callsign(cs, cu) and any(abs(f - r["freq_khz"]) <= 1 and freq_in_band(f, band) for f in freqs):
                valid.add((cs, r["freq_khz"]))
    return len(valid) >= minimum


_BLOCK_RE = re.compile(r"\b(?:[01]?\d|2[0-3]):[0-5]\d\b")
# Hour-range blocks like "0-1", "00-01", "22-23" (plain or unicode dash) — the
# common "2-hour increment" schedule format a model uses instead of HH:MM
# (base-20B adrev 2026-07-02, confirmed defect: warc-vara staged a real 12-block
# plan in "0-1 UTC" form and was false-failed).
_HOUR = r"(?:[01]?\d|2[0-3])"
_HOUR_END = r"(?:[01]?\d|2[0-4])"   # a range END may be 24 (midnight / end-of-day): "22-24"
_DASH = r"[-‐‑‒–—]"
_HOURRANGE_RE = re.compile(r"\b(" + _HOUR + r")" + _DASH + r"(" + _HOUR_END + r")\b")

# Schedule ENTRIES are separated by newline / semicolon / bullet ONLY — NOT by comma
# or pipe, so an entry keeps its own time RANGE ("00:00 - 02:00 UTC, AA7WL 10.125"),
# its trailing "..., GATEWAY freq" clause (comma), and MARKDOWN-TABLE cell dividers
# ("| 00:00 | AA7WL | 10.125 |") intact within the one row (Codex adrev 2026-07-04,
# false-negatives on comma-in-block + table formats). The tradeoff is that a whole
# plan crammed onto ONE comma-joined line, or a gateway split onto a CONTINUATION line
# below its time, is under-credited — both are rare and discouraged by the scaffold's
# per-block co-location instruction (baseline_g0._predicate_line).
_ENTRY_RE = re.compile(r"[;\n•·▪‣]+")


def _time_blocks(entry):
    """The set of normalized time blocks in one entry — HH:MM points and hour ranges
    ('0-1' -> '00-01', plain/unicode dash; range end may be 24)."""
    blocks = set(_BLOCK_RE.findall(entry))
    for a, b in _HOURRANGE_RE.findall(entry):
        blocks.add(f"{int(a):02d}-{int(b):02d}")
    return blocks


def schedule_has_blocks(text, records, n):
    """True iff the staged text lays out >= n DISTINCT time blocks, EACH grounded in a
    real gateway: a `records` callsign co-located with THAT gateway's own frequency
    (within 1 kHz) in the same entry. A bare list of timestamps with no gateway/freq
    (the 20b's warc-vara "plan") fails — a grounded rotation names WHAT to call and
    WHERE for each slot, not just WHEN (tuxlink-48nyh, operator 2026-07-04).

    Time FORMAT stays tolerant (HH:MM points or hour ranges, plain/unicode dash) so the
    120b's real "0-1 UTC" plan still passes; the discriminator is gateway+freq presence,
    not clock notation (base-20B adrev 2026-07-02 format fix preserved).

    Anti-gaming: a grounded entry is credited by the frozenset of its OWN time blocks, so
    dumping all 12 timestamps beside a single gateway (one entry) or copy-pasting one
    slot twelve times (one distinct signature) counts once, not n times.
    """
    if not records:
        return False
    seen = set()
    grounded = 0
    for entry in _ENTRY_RE.split(text):
        blocks = _time_blocks(entry)
        if not blocks:
            continue
        eu = entry.upper()
        freqs = parse_freqs_khz(entry)
        if not freqs:
            continue
        if not any(_cites_callsign(str(r["callsign"]).upper(), eu)
                   and any(abs(f - r["freq_khz"]) <= 1 for f in freqs)
                   for r in records):
            continue
        sig = frozenset(blocks)
        if sig in seen:
            continue
        seen.add(sig)
        grounded += 1
    return grounded >= n


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
        if any(_cites_callsign(cs.upper(), c.upper()) and grid in c.upper() for c in clauses):
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
            if _cites_callsign(cs, c.upper()) and any(int(n) > threshold for n in _NUM_RE.findall(c)):
                hits += 1
                break
    return hits >= minimum
