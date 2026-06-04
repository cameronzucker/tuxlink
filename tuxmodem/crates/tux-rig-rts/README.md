# tux-rig-rts

Serial-RTS PTT primitive for amateur-radio digital-mode interfaces that
expose PTT as the RTS line of a USB-serial port. Phase 1 of the
tuxmodem hardware bring-up (tuxlink-mxyz / tuxlink-9ggl); eventually
rolls into the unified `tux-rig` crate per ADR 0015.

## What this controls

This is the PTT mechanism used by the **Digirig Mobile** and **Digirig
Lite** — the operator's reference HF bench rig with a Xiegu G90 — plus
any other CP2102/CP2104/FTDI-class USB-serial adapter wired the same
way. Asserting RTS on `/dev/ttyUSB*` keys the radio; clearing it
un-keys.

This is **NOT** the same mechanism as `tux-rig-cm108` (which uses the
CM108-family USB-HID GPIO mechanism for DRA-100 / SignaLink / AIOC
class adapters). Both are valid PTT backends; the future `tux-rig`
umbrella unifies them under a shared `Ptt` trait.

This is **NOT** CAT-PTT either. CAT-PTT sends a `TX` command over the
serial port to a radio that exposes CAT-PTT (G90 supports it; many
others do too). That's a separate per-radio backend filed under the
same umbrella.

## API sketch

```rust
use tux_rig_rts::{LinuxTty, Ptt, RtsPtt};

let tty = LinuxTty::open("/dev/digirig")?;
let mut ptt = RtsPtt::new(tty)?;

ptt.assert()?;
// ... TX something ...
ptt.release()?;
// Drop also releases as a safety net if you forget.
```

## CLI

The `tux-rig-rts` binary is the operator's bench-validation tool.

```sh
# Bench sanity check (G90 keys briefly, no audio needed):
tux-rig-rts toggle --device /dev/digirig

# Key the radio for up to 5 seconds (Ctrl+C aborts early and releases):
tux-rig-rts assert --device /dev/digirig --duration 5

# Explicit release (use when the line is stuck asserted from a
# previous misbehaving process):
tux-rig-rts release --device /dev/digirig
```

`--duration` is hard-capped at 30 seconds. Longer asserts require
the future watchdog daemon (Phase 1.5) which owns the tty fd from a
separate process — so a SIGKILL on the modem cannot leave RTS stuck.

## udev rule (one-time operator setup)

The Digirig's `/dev/ttyUSB*` device path is unstable across USB
re-enumeration. Pin a stable symlink keyed on the USB-serial chip's
VID:PID. Digirig Mobile uses a Silicon Labs CP2102N:

```sh
# /etc/udev/rules.d/99-digirig.rules
# Verify VID:PID with `lsusb` when Digirig is plugged.
KERNEL=="ttyUSB*", SUBSYSTEM=="tty", \
  ATTRS{idVendor}=="10c4", ATTRS{idProduct}=="ea60", \
  MODE="0660", GROUP="dialout", SYMLINK+="digirig"
```

Then `sudo udevadm control --reload` + replug. (The `dialout` group is
the standard Linux group for serial-port access; add your user to it
once if needed.)

## Safety

### Watched failure: spurious key on open

Opening `/dev/ttyUSB*` on Linux historically asserts DTR (and on some
configurations RTS too) as a vestige of modem-era serial semantics.
If the radio interprets either as PTT, **the radio keys at the moment
the process opens the device.**

`LinuxTty::open` defuses this:

1. Opens with `O_NOCTTY | O_NONBLOCK` so the tty doesn't become our
   controlling terminal.
2. Configures termios for raw mode + `CLOCAL` + **explicitly clears
   `CRTSCTS`** so the kernel doesn't manage RTS for hardware flow
   control (which would race our `TIOCMBIS`/`TIOCMBIC` calls).
3. Issues `TIOCMBIC` with `TIOCM_RTS | TIOCM_DTR` as the very first
   operation, BEFORE any caller-visible state change.

A regression test pins this: the `MockTtyWriter` observes
`OpenClearBoth` as the very first op every `RtsPtt::new` issues.

### PTT-stuck-on countermeasures

The kernel's serial driver may or may not drop modem lines when the
fd closes (depends on the driver). We don't rely on it:

1. **Drop-impl release** — `RtsPtt`'s `Drop` writes an explicit
   `TIOCMBIC` for `TIOCM_RTS` when state was `Asserted`. Covers
   graceful exits + panic-unwind paths.
2. **CLI SIGINT/SIGTERM handlers** — release before exit. Covers
   `kill <pid>` + `Ctrl+C`.
3. **Hard-capped CLI `--duration`** — 30 seconds max.

SIGKILL is uncatchable; the future watchdog daemon handles that case
by separating the tty fd into a process the modem cannot SIGKILL.

## Why direct ioctls, not the `serialport` crate

The popular `serialport` crate wraps termios for baud-rate-aware
communication. Our use case is the opposite — we never communicate.
Asking `serialport` to open without a baud is awkward, and pulling
in its dependencies for a single ioctl is unnecessary. Direct
`libc::ioctl` on the raw fd is ~30 lines and adds no new deps beyond
`libc` (which the workspace already pulls in).

## License

AGPL-3.0-only, matching the rest of the `tuxmodem/` workspace.
