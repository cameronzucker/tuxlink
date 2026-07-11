# Off-air space weather (WWV/WWVH)

Tuxlink decodes the NOAA Space Weather Prediction Center (SWPC) geophysical
alert **off the air**, from the WWV and WWVH time-signal voice broadcasts, using
the primary radio. This provides a solar-conditions input to the propagation
engine with no internet connection — a capability Winlink Express does not have.

## What it decodes

WWV broadcasts the SWPC alert by voice at **18 minutes past each hour**; WWVH at
**45 minutes past**. The bulletin refreshes every three hours and carries the
10.7 cm solar flux, the planetary A-index and K-index, and the observed and
forecast geomagnetic storm state. Tuxlink captures the announcement, transcribes
it with an offline speech-to-text model, and feeds the solar flux into the same
propagation forecast the internet path uses.

The result is stamped **"off-air WWV"** with its capture time, so its provenance
is always visible next to the conditions readout.

## One-time setup: the speech-to-text model

The decode runs a small offline Whisper model that ships separately from the
application (to keep the installer lean). Provision it once, while the machine
has internet:

```
bash scripts/fetch-stt-model.sh
```

The script downloads `ggml-base.en-q5_1.bin` (~57 MB) to
`~/.local/share/tuxlink/models/`, verifies its SHA-256, and is idempotent — it
exits immediately if the model is already present. After this, the decode is
fully off-air.

**Air-gapped installs.** Copy `ggml-base.en-q5_1.bin` onto the target machine by
hand — place it at `~/.local/share/tuxlink/models/ggml-base.en-q5_1.bin`, or set
`wwv_offair.model_path` in `config.json` to wherever the file lives. Verify the
copy with `sha256sum`.

## Using it

The **Refresh off-air** control sits beside the station finder's update actions.
Because the bulletin only airs at :18 and :45, the control arms a one-shot for
the nearest window rather than capturing immediately:

1. Select **Refresh off-air**. Tuxlink shows the next window it will use
   (for example, *Armed for WWV :18 UTC*) and continues to wait without blocking
   other work. Select **Cancel** to disarm.
2. At the window, Tuxlink tunes the radio to a WWV frequency, captures about
   70 seconds of audio, and returns the radio to its previous frequency and mode.
3. The transcript is parsed and the conditions readout updates with the off-air
   solar flux, A-index and K-index, stamped with the capture time.

Frequency selection follows the time of day (5 MHz overnight, 15 MHz midday).
If a capture cannot be copied — high noise, a weak signal — Tuxlink retries once
at the next window, then reports that it could not copy so the operator can try
again later.

Capture is intended to be occasional and pre-flight. Missing a three-hour cycle
is harmless; the last decoded value and its age remain visible.

## Requirements and scope

Off-air decode needs **CAT rig control** configured (see
[CAT and rigctld](12-cat-and-rigctld.md)) so Tuxlink can tune to WWV and restore
the radio afterward, plus a working receive-audio path. Where CAT is not
available, tune the radio to WWV manually before capturing.

This feature is **receive-only** — it tunes the VFO and records received audio,
and never keys the transmitter.

The off-air value is a single daily solar-flux reading, coarser than the smoothed
monthly forecast the internet path provides. That is the accepted trade for a
solar input that works with no connectivity: when the internet is down, a coarser
real reading keeps the propagation prediction running where it otherwise could
not.
