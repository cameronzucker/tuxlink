# Cross-validation methodology + GNU Radio reference

Per forcing function §3.4 and spec §1.Q6, hf-channel-sim's output
statistics are cross-validated against an independent open
implementation. The chosen reference for v0.1 is **GNU Radio's
`gr-channels::fading_model` Watterson implementation** — the most
actively-maintained open Watterson model in the SDR community.

## Important: NO code is incorporated

GNU Radio is GPL. hf-channel-sim is AGPLv3-only with the explicit
goal of being a runtime dependency for AGPLv3 modems. Linking
against GR would compromise that. The cross-validation works by
**comparing statistical properties** of outputs, not by depending
on GR at runtime.

## Generation script (operator-run)

The following Python script runs in a GNU Radio environment with
`gr-channels` available. Save as `scripts/generate_gr_fixtures.py`
in a separate scratch location (not committed to the hf-channel-sim
repo — it's not part of the runtime).

```python
# scripts/generate_gr_fixtures.py
# Requires GNU Radio + gr-channels installed.
# Generates reference Watterson outputs for cross-validation.
# Output: f32 LE I/Q binary files.

import numpy as np
from gnuradio import gr, blocks, channels

def run_one(input_samples, delay_ms, doppler_hz, sample_rate, seed):
    # ... configure fading_model block with the given parameters ...
    # ... run flowgraph; capture output as numpy complex64 array ...
    # ... return the output ...
    raise NotImplementedError("complete per gr-channels API")

if __name__ == "__main__":
    n = 16384
    sr = 8000.0
    unit_in = np.ones(n, dtype=np.complex64)
    out = run_one(unit_in, delay_ms=1.0, doppler_hz=0.5, sample_rate=sr, seed=1)
    out.astype(np.complex64).tofile("tests/fixtures/gr_moderate_unit_input_seed1.f32")
```

(Skeleton only — the operator completes the GR flowgraph configuration
following `gr-channels` documentation when running this.)

## What we compare

Bit-identity is NOT expected between hf-channel-sim and GR — they
use different PRNGs and likely different FFT/filter implementations.
Statistical properties ARE compared, with these tolerances:

- **Mean power:** within ±0.5 dB.
- **Autocorrelation function at lags 1, 10, 100, 1000 samples:**
  within ±10% (magnitude).
- **Per-bin spectral magnitude (Doppler PSD shape):**
  within ±2 dB at bin 0; ±3 dB at the spread edges.

## Future cross-validation expansions

- Add ITS HF Channel Simulator as a SECOND reference once tooling is
  set up. Disagreement between two references against hf-channel-sim
  is a stronger signal than disagreement against one.
- Add per-condition fixtures (Good / Poor / Flutter) once the Moderate
  baseline is committed and green.

## Why this is post-Task-12 (filed as a separate bd issue)

The fixture-generation script requires GNU Radio installed and an
operator-supervised execution (a few minutes). It is not in the
hot path of the implementation plan; the cross-validation test is
`#[ignore]`-d until the fixtures land. The Rust harness exists and
is ready to assert as soon as the fixture file appears.
