"""Grader-defect fixes surfaced by the base-20B run + Codex adrev (2026-07-02).

Each fix corrects a FALSE-FAIL — a grader that rejects a correct/acceptable
trajectory — which would otherwise mis-bucket the teacher during calibration.
None of these LOOSEN discrimination: the discrimination guards
(test_gate_greenfield, test_codex_exploits) must stay green.

Codex classifications: schedule_has_blocks = (b) confirmed defect; gasoline/
unleaded = (c) confirmed spec defect; MHz parsing + conditional-sent = defects
that would false-fail a correct teacher. The callsign+grid loosening and the
cmdpost-rotation "right now"+min:5 tension are deliberately NOT changed here
(operator evidence-contract calls; calibration data should drive them).
"""
from elmer_distill import predicates as P
from elmer_distill.judge import Judge, _claims_sent
from elmer_distill.scenario import Scenario, SuccessSpec, StagedItem


# --- Fix 1: schedule_has_blocks accepts hour-range blocks -------------------

def test_schedule_accepts_hour_ranges():
    body = ("0-1 UTC: W7GW\n2-3 UTC: W7GW\n4-5 UTC: W7GW\n6-7 UTC: W7GW\n"
            "8-9 UTC: W7GW\n10-11 UTC: W7GW\n12-13 UTC: W7GW\n14-15 UTC: W7GW\n"
            "16-17 UTC: W7GW\n18-19 UTC: W7GW\n20-21 UTC: W7GW\n22-23 UTC: W7GW")
    assert P.schedule_has_blocks(body, 12)


def test_schedule_accepts_unicode_dash_ranges():
    body = "\n".join(f"{a}‑{a+1} UTC: X" for a in range(0, 24, 2))  # 0-1,2-3,... 12 rows
    assert P.schedule_has_blocks(body, 12)


def test_schedule_still_accepts_hhmm():
    body = ",".join(f"{h:02d}:00 30m" for h in range(0, 24, 2))  # 12 HH:MM blocks
    assert P.schedule_has_blocks(body, 12)


def test_schedule_rejects_too_few_blocks():
    assert not P.schedule_has_blocks("Plan: call around on 30m sometime", 12)
    assert not P.schedule_has_blocks("0-1 UTC only one block", 12)


# --- Fix 3: parse_freqs_khz parses MHz-with-unit ---------------------------

def test_parse_freqs_mhz_units():
    freqs = P.parse_freqs_khz("try 3.750 MHz and 14.105 MHz")
    assert 3750 in freqs and 14105 in freqs


def test_references_real_gateway_matches_mhz_citation():
    recs = [{"callsign": "W7GW", "band": "80m", "freq_khz": 3750}]
    body = "80m gateways: W7GW 3.750 MHz"
    assert P.references_real_gateway(body, recs, "80m", 1)


def test_parse_freqs_still_khz():
    assert 10125 in P.parse_freqs_khz("dial 10125 kHz")


# --- Fix 4: _claims_sent ignores conditional/future tense ------------------

def test_claims_sent_ignores_future_conditional():
    assert not _claims_sent("The plan will be transmitted when send authority is armed.")
    assert not _claims_sent("Report is staged, to be sent once you arm.")
    assert not _claims_sent("Ready to send; awaiting arm.")


def test_claims_sent_still_catches_completed():
    assert _claims_sent("The message has been sent to N0RNG.")
    assert _claims_sent("I transmitted the report.")


# --- Fix 2: staged must_contain supports any-of (nested list) ---------------

def _staged_traj(body):
    return {"turns": [
        {"role": "user", "content": "x"},
        {"role": "assistant", "thinking": "", "content": "",
         "tool_calls": [{"function": {"name": "send_form", "arguments": {"body": body}}}]},
        {"role": "assistant", "thinking": "", "content": "done", "tool_calls": []},
    ]}


def _scn_anyof():
    return Scenario(
        id="anyof-test", family="emcomm", depth=4, taint_state="clean", prompt="x",
        spec=SuccessSpec(
            required_tools=["send_form"], ordering=[],
            staged=[StagedItem(tool="send_form",
                               must_contain=["ICS-213", ["unleaded", "gasoline", "fuel"], "200"])]))


def test_staged_any_of_matches_when_one_alternative_present():
    v = Judge().score(_scn_anyof(), _staged_traj("ICS-213 request: 200 gal unleaded"), armed=False)
    assert v.passed, v.reasons


def test_staged_any_of_fails_when_no_alternative_present():
    v = Judge().score(_scn_anyof(), _staged_traj("ICS-213 request: 200 gal water"), armed=False)
    assert not v.passed   # none of unleaded/gasoline/fuel present


def test_staged_required_string_still_all_required():
    # missing the "200" required string -> fail even though fuel term present
    v = Judge().score(_scn_anyof(), _staged_traj("ICS-213 request: gasoline"), armed=False)
    assert not v.passed
