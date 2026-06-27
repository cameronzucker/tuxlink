# Playbook: ARDOP will not connect

An ordered diagnostic checklist for an ARDOP session that fails to establish.
Work top to bottom; the earlier items are the most common causes. Stop at the
first step that explains the failure.

## 1. Is the modem running?

ARDOP needs the `ardopcf` daemon running and reachable. Confirm `ardopcf` is
started and that Tuxlink's ARDOP command port (default 8515) matches the port
`ardopcf` is actually listening on. A command-port mismatch shows up as
Tuxlink reporting "Disconnected" while the `ardopcf` log shows activity.

## 2. Are the audio devices selected correctly?

ARDOP generates and decodes audio. Verify the capture (input) and playback
(output) devices in the ARDOP panel point at the correct sound card, normally
the DigiRig or equivalent interface, not the system default. If `ardopcf`
exits immediately on start, a wrong ALSA device name is the usual reason.

## 3. Does PTT key the transmitter?

The radio must actually transmit when ARDOP calls. Confirm the PTT method is
wired and working: a hardware line off the interface, a serial RTS/DTR line,
a CAT command, GPIO, or CM108. Watch the radio's TX indicator during a connect
attempt. If it never keys, the call never goes out and no gateway can answer.

## 4. Is the drive level right?

Audio drive that is too low cannot be decoded by the gateway; too high causes
splatter and the gateway rejects the signal as out of spec. Set TX audio so
the radio's ALC meter reads just below full scale, with radio-side DSP (noise
reduction, notch, slow AGC) disabled. Bad calibration is the single biggest
reason a session fails to establish on a band that is clearly open.

## 5. Is the gateway callsign and frequency correct?

Verify the target RMS gateway callsign is entered correctly and the radio is
tuned to that gateway's published dial frequency for the moment. A catalog
request gives the current gateway list with frequencies and supported
bandwidths. A stale or mistyped frequency means transmitting into empty
spectrum.

## 6. Do the band and time-of-day support the path?

HF propagation is time- and band-dependent. A gateway that is reachable at
midday on 20m may be unreachable at night. For short regional range, try 40m
NVIS after dark. If the band is dead between you and the gateway, no
calibration fixes it; change band or wait for the path to open.

## 7. Try a narrower bandwidth or an alternate gateway.

If sessions key the transmitter but fail to negotiate, the gateway may
support fewer bandwidths than offered, or the channel may be degraded. Step
down (for example from 1000 Hz to 500 Hz), or pick a different gateway from
the catalog that is closer or on a better-propagating band.

## 8. Read the session log for the failure point.

The session log shows exactly where the exchange stopped: never keyed, keyed
but no answer, answered but bandwidth negotiation failed, or connected but the
B2F handshake stalled. Each failure point maps to one of the steps above, so
the log tells you which step to revisit rather than retrying blindly.
