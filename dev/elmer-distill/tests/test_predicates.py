from elmer_distill.predicates import (freq_in_band, distance_band, parse_freqs_khz,
                                      references_real_gateway, schedule_has_blocks, BANDS)


def test_freq_in_band():
    assert freq_in_band(3585, "80m") and not freq_in_band(7100, "80m")
    assert freq_in_band(10125, "30m") and freq_in_band(18100, "17m")


def test_distance_band():
    assert distance_band(800, 500, 1500) and not distance_band(200, 500, 1500)


def test_parse_freqs():
    assert parse_freqs_khz("W7GW 3585 kHz, K7AZ 3592") == [3585.0, 3592.0]
    assert parse_freqs_khz("no freqs here") == []


def test_references_real_gateway_binds_to_records():
    records = [{"callsign": "W7GW", "band": "80m", "freq_khz": 3585.0},
               {"callsign": "K7AZ", "band": "80m", "freq_khz": 3592.0}]
    good = '{"body":"W7GW 3585 kHz; K7AZ 3592 kHz"}'
    bad = '{"body":"KZ9Z 3999 kHz; QQ0Q 3888 kHz"}'   # freqs not in any record
    assert references_real_gateway(good, records, "80m", 2)
    assert not references_real_gateway(bad, records, "80m", 2)


def test_schedule_blocks():
    assert schedule_has_blocks("00:00 80m; 02:00 40m; 04:00 20m", 3)
    assert not schedule_has_blocks("sometime later", 3)


def test_bands_cover_warc():
    for b in ("30m", "17m", "12m", "80m", "40m", "20m"):
        assert b in BANDS
