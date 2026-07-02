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


def references_real_gateway(staged_args_json, records, band, minimum):
    """True iff >= `minimum` distinct in-band freqs in the staged text match a
    real `find_stations` record's freq_khz (within 1 kHz) on that band.
    """
    freqs = parse_freqs_khz(staged_args_json)
    valid = {
        f for f in freqs
        if freq_in_band(f, band)
        and any(abs(f - r["freq_khz"]) <= 1 and r.get("band") == band for r in records)
    }
    return len(valid) >= minimum


_BLOCK_RE = re.compile(r"\b(?:[01]?\d|2[0-3]):[0-5]\d\b")


def schedule_has_blocks(text, n):
    """True iff the text contains >= n distinct HH:MM time blocks."""
    return len(set(_BLOCK_RE.findall(text))) >= n
