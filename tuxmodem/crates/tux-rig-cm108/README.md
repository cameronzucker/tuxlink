# tux-rig-cm108

CM108-family USB-HID PTT primitive for amateur-radio digital-mode adapters.
Phase 1 of the tuxmodem hardware bring-up (tuxlink-u1js / tuxlink-9ggl);
eventually rolls into the unified `tux-rig` crate per ADR 0015.

## What this controls

C-Media CM108-family USB audio codec chips (CM108, CM108AH, CM108B,
CM109, CM119, CM119A, CM119B, plus the SSS1621/1623 work-alikes and
the AIOC emulator) expose a HID interface alongside their USB audio
interfaces. The HID interface exposes 8 GPIO pins addressable via a
5-byte feature report — the same mechanism Direwolf, Hamlib's `cm108`
rig type, and fldigi's "C-Media GPIO PTT" all use.

On the Masters Communications DRA-100-DIN6 adapter (this project's
reference bench rig), the chip's GPIO3 drives the radio's PTT line
through a 2N2222 buffer.

## API sketch

```rust
use tux_rig_cm108::{Cm108Ptt, GpioPin, HidrawWriter, Ptt};

let writer = HidrawWriter::open("/dev/dra100-ptt")?;
let pin = GpioPin::new(3)?; // DRA-100-DIN6 PTT pin
let mut ptt = Cm108Ptt::new(writer, pin);

ptt.assert()?;
// ... TX something ...
ptt.release()?;
// Drop also releases as a safety net if you forget.
```

## CLI

The `tux-rig-cm108` binary is the operator's bench-validation tool.

```bash
# Bench sanity check (LED flash, no radio activity needed):
tux-rig-cm108 toggle --device /dev/dra100-ptt

# Key the radio for up to 5 seconds (Ctrl+C aborts early and releases):
tux-rig-cm108 assert --device /dev/dra100-ptt --duration 5

# Explicit release (use when the chip is stuck in asserted state from a
# previous misbehaving process):
tux-rig-cm108 release --device /dev/dra100-ptt
```

`--duration` is hard-capped at 30 seconds. Longer asserts require the
future watchdog daemon (Phase 1.5) which owns the hidraw fd from a
separate process — so a SIGKILL on the modem cannot leave PTT stuck.

## udev rule (one-time operator setup)

The hidraw device path is unstable across USB re-enumeration. Pin a
stable symlink keyed on the CM119A's USB VID:PID. From the bench-rig
spec at `docs/hardware/modem-test-rig.md`:

```sh
# /etc/udev/rules.d/99-dra100-ptt.rules
# Verify VID:PID with `lsusb` when DRA-100 is plugged.
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", \
  ATTRS{idVendor}=="0d8c", ATTRS{idProduct}=="<verify>", \
  MODE="0660", GROUP="plugdev", SYMLINK+="dra100-ptt"
```

Then `sudo udevadm control --reload` + replug.

## Safety

PTT-stuck-on is the worst failure mode. The chip latches its last
commanded GPIO state if the controlling process dies — so a modem
crash with PTT asserted leaves the radio transmitting until manually
intervened.

This crate's mitigations:

1. **Drop-impl release** — `Cm108Ptt`'s `Drop` writes an explicit
   release report when state was `Asserted`. Covers graceful exits +
   panic-unwind paths.
2. **CLI SIGINT/SIGTERM handlers** — release before exit. Covers
   `kill <pid>` + `Ctrl+C`.
3. **Hard-capped CLI `--duration`** — 30 seconds max in this binary.

SIGKILL is uncatchable; the future watchdog daemon handles that case
by separating the hidraw fd into a process the modem cannot SIGKILL.

## Report format reference

5-byte HID feature report sent via plain `write(2)` to
`/dev/hidraw*`. Layout per Direwolf's `cm108.c` (GPL-3,
AGPL-3-compatible):

```
byte 0:  0x00      (reserved)
byte 1:  0x00      (reserved)
byte 2:  iodata    (GPIO output state — bit N-1 = pin-N high)
byte 3:  iomask    (GPIO direction mask — bit N-1 = pin-N is output)
byte 4:  0x00      (reserved)
```

The crate does **not** hand-recite these bytes from documentation
memory. The byte layout is in code, in `src/report.rs`, with tests
that pin the exact bit pattern against Direwolf's known-working values.
Subtle CM108/CM119/CM119A revision differences make hand-recital
risky.

## License

AGPL-3.0-only, matching the rest of the `tuxmodem/` workspace.
