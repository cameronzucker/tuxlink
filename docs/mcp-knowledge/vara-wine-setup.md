# Playbook: installing VARA HF under WINE

How to help an operator provision VARA HF on Linux, and how to drive the
provisioning tools. Read this before calling `vara_install_start`.

## What this is

VARA HF is a proprietary Windows HF data modem. It has no native Linux build, so
Tuxlink runs it under WINE (a Windows compatibility layer). Tuxlink bundles a
setup engine that automates the fragile one-time install: the WINE prefix, the
Visual Basic 6 runtime VARA needs, the OCX control registration, and a launch
check.

This is a **prep-time install**, not a runtime concern. Once installed, VARA
runs as an ordinary external process on `127.0.0.1:8300/8301`; Tuxlink speaks to
it over TCP and does not manage it at runtime.

## Two hard constraints

- **x86_64 Linux only.** VARA is a 32/64-bit x86 Windows binary. It cannot run
  on ARM (a Raspberry Pi, an Apple-silicon VM). Call `vara_engine_available`
  first: `false` means this build/host cannot provision VARA, and there is
  nothing to guide the operator toward here.
- **Non-transmit.** Provisioning installs software (`apt`, `winetricks`, `wine`)
  and opens VARA's local TCP ports during the verify step. It never keys a
  radio. It therefore does **not** require armed send authority and does not go
  through the transmit consent gate.

## Prerequisites the operator must have

1. **WINE** installed system-wide (on Debian/Ubuntu: `sudo apt install wine`).
   The setup engine's `deps` checkpoint checks for it.
2. **The VARA HF installer `.exe`**, downloaded by the operator themselves.
   Tuxlink cannot bundle it — it is proprietary. The current build is published
   at rosmodem.wordpress.com (the author's site) and linked from winlink.org.
   The operator downloads it to their own machine (for example
   `~/Downloads/VARA HF v4.x.x setup.exe`) and gives you that path.

The free (unregistered) tier of VARA HF runs at reduced speed but installs and
connects fine; registration is a licence key the operator enters inside VARA
later, unrelated to this install.

## The install pipeline

`vara_install_start` runs seven checkpoints in order. `vara_install_status`
reports each one's state so you can tell the operator where a run stopped:

1. `deps` — WINE and required system packages are present.
2. `prefix` — a dedicated WINE prefix is created.
3. `vara` — the operator's installer `.exe` runs under WINE.
4. `vb6` — the Visual Basic 6 runtime VARA depends on is installed.
5. `ocx` — VARA's OCX control is registered (`regsvr32`).
6. `verify` — VARA launches and its TCP ports answer.
7. `autostart` — VARA is wired to start with the session.

## How to drive it

1. Call `vara_engine_available`. If `false`, stop — this host cannot run VARA;
   suggest ARDOP (open, no WINE) instead.
2. Call `vara_install_status`. If `ready` is `true`, VARA is already provisioned;
   there is nothing to install. Otherwise the `checkpoints` show how far a
   previous attempt got.
3. Confirm the operator has installed WINE and downloaded the VARA `.exe`. Ask
   for the full path to that file. Do not guess it.
4. Call `vara_install_start` with `installer_path` set to that path. The setup
   engine runs a privileged step through **pkexec**, which pops a password
   dialog on the operator's screen — the operator, not you, enters their OS
   password. The install runs for several minutes.
5. Read the returned summary. `ok: true` means VARA is provisioned (the summary
   also reports the WINE `prefix` and the VARA version). On failure, re-run
   `vara_install_status` and read the failing checkpoint's `detail` to explain
   what went wrong (a missing dependency, a denied pkexec prompt, a bad installer
   path) and what to fix.

## When it fails

- **`deps` failed** — WINE is not installed. Guide the operator to install it.
- **pkexec denied / cancelled** — the operator dismissed the password prompt.
  Re-run and have them approve it.
- **installer not found** — the `installer_path` is wrong or the file moved.
  Ask for the correct path and re-run.
- **`verify` failed** — VARA installed but does not launch cleanly under this
  WINE prefix. Report the `detail` verbatim; this usually needs the operator to
  inspect the VARA window manually.
