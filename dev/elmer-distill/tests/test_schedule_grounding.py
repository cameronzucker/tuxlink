"""schedule_has_blocks must be EVIDENCE-BOUND: a rotation plan block is only real if
it names a reachable gateway AND that gateway's own frequency for the slot — not a
bare timestamp (tuxlink-48nyh / 6zkb6, operator 2026-07-04).

The 20b passed `warc-vara-plan-drive-p2p` by staging a plan of bare timestamps
(`00:00 / 02:00 / … / 22:00`) with no bands, stations, or frequencies. It satisfied the
old time-token counter and was useless — a predicate FALSE POSITIVE. These tests pin the
discriminator to gateway+freq-per-block while PRESERVING the earlier fix that a real
plan may use hour-range notation (`0-1 UTC`), which is format, not substance.
"""
from elmer_distill import predicates as P

# A realistic find_stations record set (what the model reads before drafting the plan).
_WARC = [
    {"callsign": "AA7WL", "band": "30m", "freq_khz": 10125.0},
    {"callsign": "K7XYZ", "band": "17m", "freq_khz": 18106.0},
    {"callsign": "W5RMS", "band": "12m", "freq_khz": 24925.0},
]


def _line_plan(freqs_by_slot):
    """A newline-per-slot plan: 'HH:00 UTC — CALLSIGN @ FREQ kHz'."""
    return "\n".join(
        f"{h:02d}:00 UTC — {cs} @ {fk:.0f} kHz" for (h, cs, fk) in freqs_by_slot)


# --- the exploit the operator flagged: bare timestamps must FAIL -----------------
def test_bare_timestamps_fail():
    bare = ", ".join(f"{h:02d}:00" for h in range(0, 24, 2))  # 12 bare HH:MM tokens
    assert not P.schedule_has_blocks(bare, _WARC, 12)


def test_bare_hour_ranges_fail():
    bare = "\n".join(f"{a}-{a+1} UTC" for a in range(0, 24, 2))  # 12 bare ranges, no gateway
    assert not P.schedule_has_blocks(bare, _WARC, 12)


# --- a grounded plan (gateway + its own freq per slot) must PASS -----------------
def test_grounded_line_plan_passes():
    rot = [(0, "AA7WL", 10125), (2, "K7XYZ", 18106), (4, "W5RMS", 24925),
           (6, "AA7WL", 10125), (8, "K7XYZ", 18106), (10, "W5RMS", 24925),
           (12, "AA7WL", 10125), (14, "K7XYZ", 18106), (16, "W5RMS", 24925),
           (18, "AA7WL", 10125), (20, "K7XYZ", 18106), (22, "W5RMS", 24925)]
    assert P.schedule_has_blocks(_line_plan(rot), _WARC, 12)


def test_grounded_hour_range_plan_passes():
    """Format tolerance PRESERVED: the 120b's real '0-1 UTC' plan (base-20B adrev
    2026-07-02) still passes — as long as each block names a real gateway + freq."""
    rot = [(0, "AA7WL", 10125), (2, "K7XYZ", 18106), (4, "W5RMS", 24925),
           (6, "AA7WL", 10125), (8, "K7XYZ", 18106), (10, "W5RMS", 24925),
           (12, "AA7WL", 10125), (14, "K7XYZ", 18106), (16, "W5RMS", 24925),
           (18, "AA7WL", 10125), (20, "K7XYZ", 18106), (22, "W5RMS", 24925)]
    body = "\n".join(f"{h}-{h+1} UTC: {cs} {fk} kHz" for (h, cs, fk) in rot)
    assert P.schedule_has_blocks(body, _WARC, 12)


def test_mhz_units_in_plan_pass():
    """Gateways cited in MHz (the same real dial) still bind (parse_freqs handles MHz)."""
    body = "\n".join(f"{h:02d}:00 UTC: AA7WL 10.125 MHz" for h in range(0, 24, 2))
    assert P.schedule_has_blocks(body, _WARC, 12)


# --- anti-gaming: shapes that dump times but not a gateway-per-block must FAIL ----
def test_one_gateway_many_times_fails():
    """A single entry stuffing all 12 timestamps next to ONE gateway is not 12 blocks."""
    body = (", ".join(f"{h:02d}:00" for h in range(0, 24, 2)) + " all via AA7WL 10125 kHz")
    assert not P.schedule_has_blocks(body, _WARC, 12)


def test_copypaste_identical_block_fails():
    """12 identical lines (same slot, same gateway) is one distinct block, not twelve."""
    body = "\n".join("00:00 UTC AA7WL 10125 kHz" for _ in range(12))
    assert not P.schedule_has_blocks(body, _WARC, 12)


def test_partial_grounding_fails():
    """Only 3 of 12 slots name a gateway+freq; the rest are bare -> below n=12."""
    grounded = [f"{h:02d}:00 UTC AA7WL 10125 kHz" for h in (0, 2, 4)]
    bare = [f"{h:02d}:00 UTC" for h in range(6, 24, 2)]
    body = "\n".join(grounded + bare)
    assert not P.schedule_has_blocks(body, _WARC, 12)
    assert P.schedule_has_blocks(body, _WARC, 3)   # the 3 grounded blocks do count


def test_fabricated_gateway_or_freq_fails():
    """Real callsign with a freq that matches NO record, and a real freq with a fake
    callsign, are both ungrounded (mirrors references_real_gateway's binding)."""
    fake_freq = "\n".join(f"{h:02d}:00 UTC AA7WL 9999 kHz" for h in range(0, 24, 2))
    fake_call = "\n".join(f"{h:02d}:00 UTC QQ0Q 10125 kHz" for h in range(0, 24, 2))
    assert not P.schedule_has_blocks(fake_freq, _WARC, 12)
    assert not P.schedule_has_blocks(fake_call, _WARC, 12)


def test_no_records_fails():
    """No find_stations evidence -> a plan cannot be verified as grounded -> fail."""
    body = "\n".join(f"{h:02d}:00 UTC AA7WL 10125 kHz" for h in range(0, 24, 2))
    assert not P.schedule_has_blocks(body, [], 12)


# --- Codex adrev 2026-07-04 regressions ------------------------------------------

def test_substring_callsign_does_not_ground(   # Codex F1
):
    """A fake token that merely CONTAINS a real callsign ('NOTAA7WLX' ⊃ 'AA7WL') must
    not count — the callsign has to appear as a whole token."""
    body = "\n".join(f"{h:02d}:00 UTC NOTAA7WLX 10125 kHz" for h in range(0, 24, 2))
    assert not P.schedule_has_blocks(body, _WARC, 12)


def test_markdown_table_plan_passes(   # Codex F3
):
    """A very plausible competent-model format: a Markdown table, one row per slot."""
    rows = "\n".join(f"| {h:02d}:00 | AA7WL | 10.125 MHz |" for h in range(0, 24, 2))
    body = "| Time UTC | Gateway | Frequency |\n|---|---|---|\n" + rows
    assert P.schedule_has_blocks(body, _WARC, 12)


def test_comma_between_time_range_and_gateway_passes(   # Codex F4
):
    """A comma AFTER the time range, before the gateway, stays within the one block."""
    body = "\n".join(f"{h:02d}:00 - {h+2:02d}:00 UTC, AA7WL 10.125 MHz" for h in range(0, 24, 2))
    assert P.schedule_has_blocks(body, _WARC, 12)


# --- Accepted residuals (documented, severity-calibrated; Codex F2/F5) -----------

def test_documented_residual_continuation_line(   # Codex F5 — KNOWN under-credit
):
    """A gateway split onto a CONTINUATION line below its time is currently under-
    credited (no single entry holds time+gateway+freq). Accepted as a rare format the
    scaffold's per-block co-location instruction steers away from; pinned so a future
    change to this behavior is a conscious one, not a silent regression."""
    body = "\n".join(f"{h:02d}:00 - {h+2:02d}:00 UTC\n  AA7WL 10.125 MHz" for h in range(0, 24, 2))
    assert not P.schedule_has_blocks(body, _WARC, 12)   # documents current behavior
