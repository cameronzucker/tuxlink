# shellcheck shell=bash
# lib/checkpoints.sh — the checkpoint pipeline definitions.
#
# Each checkpoint <id> has:
#   detect_<id>  — read-only, returns 0 iff already satisfied (no side effects)
#   do_<id>      — performs the step (defined in this file; may need internet)
# Ordering is authoritative and consumed by lib/pipeline.sh.
#
# Requires lib/env.sh (wv_prefix, wv_wineenv) to be sourced first.

# shellcheck disable=SC2034  # consumed by lib/pipeline.sh after sourcing
WV_CHECKPOINTS="deps prefix vara vb6 ocx verify autostart"

wv_label() {
  case "$1" in
    deps)      echo "System dependencies (WINE, winetricks)" ;;
    prefix)    echo "WINE prefix" ;;
    vara)      echo "VARA HF installation" ;;
    vb6)       echo "Visual Basic 6 runtime" ;;
    ocx)       echo "OCX controls (TCP + spectrum)" ;;
    verify)    echo "Launch + TCP verification" ;;
    autostart) echo "Auto-start on login" ;;
    *)         return 1 ;;
  esac
}

# ---- detectors (read-only) -------------------------------------------------

detect_deps() {
  command -v wine >/dev/null && command -v winetricks >/dev/null && command -v regsvr32 >/dev/null \
    && command -v ss >/dev/null
}
# A completed prefix has both syswow64 AND a registry file (system.reg is written
# once wineboot finishes); requiring both avoids skipping a half-built prefix.
detect_prefix() { [ -d "$(wv_prefix)/drive_c/windows/syswow64" ] && [ -f "$(wv_prefix)/system.reg" ]; }
detect_vara() { [ -e "$(wv_prefix)/drive_c/VARA HF/VARA.exe" ]; }
detect_vb6() { [ -e "$(wv_prefix)/drive_c/windows/syswow64/msvbvm60.dll" ]; }
# Require BOTH the critical TCP control (MSWINSCK) AND the last control the
# do_ocx loop registers (MSSTDFMT): if the last is present the loop completed,
# so a retry won't skip a partially-registered set.
detect_ocx() {
  local reg; reg="$(wv_prefix)/system.reg"
  grep -qi MSWINSCK "$reg" 2>/dev/null && grep -qi MSSTDFMT "$reg" 2>/dev/null
}
detect_autostart() { [ -e "$HOME/.config/systemd/user/wine-vara.service" ]; }

# ---- launch helpers --------------------------------------------------------

# Start VARA in the background; echo its PID. Assumes wv_wineenv already run.
wv_start_vara() {
  local exe; exe="$(wv_prefix)/drive_c/VARA HF/VARA.exe"
  wine "$exe" >/dev/null 2>&1 &
  echo $!
}

# Stop a VARA process started by wv_start_vara. Tolerant of an already-dead PID.
wv_stop_vara() { kill "$1" 2>/dev/null || true; }

# Poll for BOTH of VARA's TCP ports (8300 command, 8301 data) to LISTEN; then,
# if `nc` is available, confirm the VERSION handshake. Returns 0 on success,
# 1 after WV_VERIFY_TRIES attempts. Both ports are required because consumers
# use the data port (8301) and a command-only VARA would fail them later.
wv_wait_ports() {
  local tries="${WV_VERIFY_TRIES:-30}" i=0 listening
  while [ "$i" -lt "$tries" ]; do
    listening="$(ss -ltn 2>/dev/null)"
    if grep -q '127.0.0.1:8300' <<<"$listening" && grep -q '127.0.0.1:8301' <<<"$listening"; then
      if command -v nc >/dev/null; then
        printf 'VERSION\r' | nc -w2 127.0.0.1 8300 2>/dev/null | grep -qi VARA && return 0
      else
        return 0   # both ports up; no handshake tool available
      fi
    fi
    i=$((i + 1)); sleep "${WV_VERIFY_SLEEP:-1}"
  done
  return 1
}

# ---- actions (side effects; some require internet) -------------------------

# Installs WINE + winetricks, plus iproute2 (ss) and a netcat (nc) used by the
# verify checkpoint. Requires internet. Debian/apt only: on a non-apt host
# (Fedora, openSUSE, Arch) fail loudly with manual instructions rather than
# blindly invoking apt-get, which does not exist there.
do_deps() {
  if command -v apt-get >/dev/null; then
    pkexec apt-get install -y wine winetricks iproute2 netcat-openbsd
  else
    echo "no apt-get on this system. Install these with your package manager, then re-run: wine winetricks iproute2 (ss) and a netcat (nc)" >&2
    return 1
  fi
}

do_prefix() {
  wv_wineenv
  # Decline wine-mono (.NET) and wine-gecko: VARA is VB6, not .NET, and the
  # prompts would block a non-interactive init. Disabling the DLLs suppresses them.
  WINEDLLOVERRIDES="mscoree,mshtml=" wine wineboot --init
}

do_vara() {
  if [ -z "${WV_INSTALLER:-}" ] || [ ! -e "$WV_INSTALLER" ]; then
    echo "no installer supplied (--installer <path>)" >&2; return 1
  fi
  wv_wineenv
  wine "$WV_INSTALLER" /VERYSILENT "/DIR=C:\\VARA HF"
}

do_vb6() { wv_wineenv; winetricks -q vb6run; }

do_ocx() {
  wv_wineenv
  local ocx sys rc=0; ocx="$(wv_prefix)/drive_c/VARA HF/OCX"; sys="$(wv_prefix)/drive_c/windows/syswow64"
  cp "$ocx"/*.OCX "$ocx"/*.DLL "$sys"/ 2>/dev/null || true
  # Register each control; a failure of ANY must fail the checkpoint (do not let
  # the loop's last success mask an earlier failure — errexit is suppressed here).
  local c
  for c in MSCOMCTL.OCX COMDLG32.OCX MSCOMM32.OCX MSWINSCK.OCX MSCHRT20.OCX MSSTDFMT.DLL; do
    regsvr32 /s "C:\\windows\\syswow64\\$c" || rc=1
  done
  return "$rc"
}

do_verify() {
  wv_wineenv
  local pid rc=0
  pid="$(wv_start_vara)"
  wv_wait_ports || rc=1
  wv_stop_vara "$pid"
  return "$rc"
}

do_autostart() {
  local unit_dir="$HOME/.config/systemd/user" exe
  exe="$(wv_prefix)/drive_c/VARA HF/VARA.exe"
  mkdir -p "$unit_dir" || { echo "cannot create $unit_dir" >&2; return 1; }
  # Quote Environment= values: an unquoted path with spaces would be split by
  # systemd into multiple (broken) assignments.
  cat >"$unit_dir/wine-vara.service" <<UNIT || { echo "cannot write unit" >&2; return 1; }
[Unit]
Description=VARA HF under WINE (wine-vara-setup)
After=graphical-session.target

[Service]
Environment="WINEPREFIX=$(wv_prefix)"
Environment="WINEDEBUG=-all"
ExecStart=/usr/bin/wine "$exe"
Restart=on-failure

[Install]
WantedBy=default.target
UNIT
  # Enabling needs a user systemd/dbus session; over a bare SSH it may be absent.
  # The unit is installed regardless (it starts at next graphical login); warn
  # rather than hard-fail so headless provisioning still succeeds.
  if ! { systemctl --user daemon-reload && systemctl --user enable --now wine-vara.service; } 2>/dev/null; then
    # Without a successful `enable` the wants/ symlink is not created, so the
    # unit will NOT auto-start. Be honest and give the manual command.
    echo "unit written but could not be enabled (no active user systemd session?). Enable it later with: systemctl --user enable --now wine-vara.service" >&2
  fi
  return 0
}
