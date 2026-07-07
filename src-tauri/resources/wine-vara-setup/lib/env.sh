# shellcheck shell=bash
# lib/env.sh — architecture preflight, prefix + WINE environment resolution.
# Sourced by bin/wine-vara-setup and by tests. No side effects at source time.

# Echo the machine architecture. Wraps `uname -m` so tests can shim it.
wv_arch() { uname -m; }

# Return 0 iff this host can run VARA (native x86_64). Otherwise explain and fail.
# VARA under box64/ARM emulation is transmit-blocked in practice, so it is refused.
wv_preflight() {
  local arch
  arch="$(wv_arch)"
  if [ "$arch" != "x86_64" ]; then
    printf 'wine-vara-setup: unsupported architecture %q.\n' "$arch" >&2
    printf 'VARA HF requires x86_64 (native WINE). ARM/box64 is transmit-blocked and not supported.\n' >&2
    return 1
  fi
  return 0
}

# Echo the resolved WINE prefix path (override: WINE_VARA_PREFIX).
wv_prefix() { printf '%s\n' "${WINE_VARA_PREFIX:-$HOME/.local/share/wine-vara/prefix}"; }

# Export the WINE environment for the resolved prefix. Idempotent.
wv_wineenv() {
  WINEPREFIX="$(wv_prefix)"
  export WINEPREFIX
  export WINEDEBUG="${WINEDEBUG:--all}"
  # A wow64 (win64) prefix is correct: it exposes syswow64 where VARA's 32-bit
  # VB6 runtime and OCX controls are registered. A win32 prefix has no syswow64.
  export WINEARCH="win64"
}
