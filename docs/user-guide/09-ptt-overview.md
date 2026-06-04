# PTT methods overview

Push-to-talk — keying the transmitter for the duration of an outbound
transmission — is the single most failure-prone link in any data-mode chain.
Tuxlink supports four PTT methods. Picking the right one depends on the
radio, the interface, and what other software shares the chain.

## The four methods

| Method | How it keys | Pros | Cons |
|---|---|---|---|
| **VOX** | Modem audio triggers the radio's voice-activated transmit | No control line needed | Slow to key (50–100 ms), drops on quiet audio frames, drifts as audio level changes |
| **COM serial PTT** | Modem asserts RTS or DTR on a serial line; radio's PTT input listens | Hard-wired, deterministic | Needs a wire from the PC to the radio, sometimes plus a transistor inverter |
| **CAT command PTT** | Modem sends a "transmit on/off" command over the radio's CAT serial link | Same wire as frequency / mode control | Adds CAT latency to every key; can conflict with rigctld |
| **Hardware PTT line** | An interface box (DigiRig, SignaLink with its TX LED tap, dedicated USB-CAT cable) provides a dedicated PTT line | Cleanest, fastest, no software conflict | Requires an interface box or a soundcard with PTT support |

Each method has a context where it's the right answer:

- **Use VOX for** quick demos, training-station setups where you don't care
  about marginal performance, or radios that have no PTT input.
- **Use COM serial PTT for** soundcard-only setups where the soundcard
  itself has no PTT support.
- **Use CAT command PTT for** rigs where you're already running CAT for
  frequency / mode control and you don't run another piece of software
  (like rigctld) on the same port.
- **Use a hardware PTT line for** everything else. This is the default for
  serious Winlink work.

## VOX

VOX (Voice Operated Transmit) keys the radio when audio exceeds a
threshold. It is the simplest method — no wires, no software, no
configuration beyond enabling VOX on the radio and tuning its delay /
threshold.

Two problems make VOX poor for data modes:

1. **Slow key time.** The radio waits to confirm audio is present (typically
   50–100 ms) before switching to transmit. Tuxlink's modem expects the
   transmit chain to be ready when audio starts; the missed leading edge
   of the first frame is a session-killer for ARDOP and VARA.
2. **Drops on quiet audio.** Some packet frames have low instantaneous
   power; VOX may drop transmit between frames. The receiver sees a glitch
   and the protocol retries or fails.

VOX is acceptable for FM packet at 1200 baud (the protocol tolerates the
leading-edge loss) and unacceptable for HF data modes.

## COM serial PTT (RTS / DTR)

A serial port's RTS or DTR line is software-assertable. The radio's PTT
input is a contact closure to ground (most rigs) or a positive voltage
(some).

The conventional wiring: a USB-serial adapter on the PC, a 3.5 mm or 6.3
mm connector on the radio's PTT input, a wire between them. Some radios
expect a transistor in line because the serial line voltage doesn't quite
match the radio's PTT expectation. The DigiRig and similar interfaces
build this transistor in.

Tuxlink's modem configuration accepts the serial device path
(`/dev/ttyUSB0`, `/dev/ttyACM0`) plus the choice of RTS or DTR.

## CAT command PTT

Modern radios expose a CAT (Computer Aided Tuning) interface — usually a
serial port over USB — for setting frequency, mode, and a host of other
parameters. Most CAT command sets include a "transmit on / transmit off"
command. Tuxlink can drive PTT this way by sending the appropriate command
through the CAT port.

CAT-PTT works when the same serial port is otherwise idle. It does not
work alongside rigctld (Hamlib's rig daemon) because two processes cannot
share an exclusive serial port. The split (rigctld for frequency, hardware
PTT for transmit) is the usual resolution — see
[CAT and rigctld](12-cat-and-rigctld.md).

## Hardware PTT line

A purpose-built interface — DigiRig, SignaLink with its TX output tapped,
the BB6PRO, or a soundcard cable that includes a PTT pin — provides a
dedicated PTT line that the OS sees as a serial port (typically via FTDI
or CH340 chips). The OS asserts RTS or DTR; the interface translates that
into the radio's expected PTT signalling.

This is the default for serious data-mode work because it has the lowest
latency, the cleanest software story (it looks like RTS-PTT to the modem),
and decouples PTT from CAT.

Tuxlink ships with explicit canonical support for the DigiRig (see
[DigiRig setup](10-digirig.md)). Other interfaces work the same way at the
software layer — pick the serial device path and the PTT line, and the
modem keys correctly.

## Recommended chain

For a new Winlink station using tuxlink, the chain that "just works":

1. A radio with a CAT serial interface (most modern HF + portable rigs).
2. A DigiRig (or equivalent) providing audio in + audio out + hardware PTT.
3. CAT to the radio's CAT port (frequency / mode control via rigctld).
4. Hardware PTT via the DigiRig's PTT line (modem keys via RTS).
5. Audio in / audio out via the DigiRig's USB sound card.

This combination keeps PTT, CAT, and audio independent — when one fails,
the others still report state usefully in the session log.

## Where next

- [DigiRig setup](10-digirig.md) — the canonical interface, end-to-end wiring + config.
- [SignaLink and other soundcards](11-signalink-and-others.md) — alternatives.
- [CAT and rigctld](12-cat-and-rigctld.md) — when CAT control is necessary.
- [Radio-specific notes](13-radio-specific-notes.md) — per-rig settings.
