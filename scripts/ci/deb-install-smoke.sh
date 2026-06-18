#!/usr/bin/env bash
# deb-install-smoke.sh — runs INSIDE a clean Debian container (no dev toolchain,
# fresh apt state) to validate Tuxlink's DOCUMENTED end-user install path.
#
# Two modes:
#   install (default) — SUPPORTED distro (Debian 13 Trixie+, Ubuntu 24.04+).
#     Mirrors docs/install.md: `apt-get update && apt-get install -y ./*.deb`,
#     then asserts apt resolved every Depends, the binary is present, and its
#     shared libraries ALL resolve (ldd has no "not found").
#   refuse — UNSUPPORTED distro (e.g. Debian 12 Bookworm: glibc 2.36 < the
#     binary's 2.39 floor). Asserts apt REFUSES cleanly because of the
#     `libc6 (>= 2.39)` Depends — i.e. the user gets a clear unmet-dependency
#     error, NOT a "successful" install of a binary that then dies at ld.so
#     with `GLIBC_2.39 not found`. That silent-broken-install was the GH #786
#     class of pain; this asserts we fail loud and early instead.
#
# Class-prevention for GH #786 (tuxlink-w636). Deliberately never `dpkg -i`
# (does not resolve deps).
#
# Usage: deb-install-smoke.sh <path-to-.deb> [install|refuse]
set -euo pipefail

DEB="${1:?usage: deb-install-smoke.sh <path-to-.deb> [install|refuse]}"
DEB="$(readlink -f "$DEB")"
test -f "$DEB"
MODE="${2:-install}"

export DEBIAN_FRONTEND=noninteractive

echo "::group::apt-get update (stock package index)"
apt-get update
echo "::endgroup::"

if [ "$MODE" = "refuse" ]; then
  echo "::group::expect apt to REFUSE ./${DEB##*/} (unsupported distro)"
  set +e
  out="$(apt-get install -y "$DEB" 2>&1)"; rc=$?
  set -e
  printf '%s\n' "$out"
  echo "::endgroup::"

  if [ "$rc" -eq 0 ]; then
    echo "FAIL: apt INSTALLED the package on an unsupported distro — expected a clean refusal."
    exit 1
  fi
  if dpkg -s tuxlink 2>/dev/null | grep -qE '^Status: install ok installed$'; then
    echo "FAIL: tuxlink ended up configured despite the expected refusal."
    exit 1
  fi
  if ! printf '%s' "$out" | grep -qiE 'libc6|unmet dependencies|not be installed|Depends'; then
    echo "FAIL: apt failed, but not with the expected unmet-dependency refusal."
    exit 1
  fi
  echo "OK: ${DEB##*/} is correctly REFUSED by apt on this unsupported distro (libc6 floor enforced)."
  exit 0
fi

echo "::group::apt-get install ./${DEB##*/} (documented user path; resolves deps)"
# Absolute path => apt treats it as a local .deb and resolves its Depends from
# the configured repositories. A non-zero exit = unmet Depends = the bug we guard.
apt-get install -y "$DEB"
echo "::endgroup::"

echo "::group::assert package installed + binary present"
dpkg -s tuxlink | grep -qE '^Status: install ok installed$'
bin="$(dpkg -L tuxlink | grep -E '/bin/tuxlink$' | head -1)"
test -n "$bin"
test -x "$bin"
echo "binary: $bin"
echo "::endgroup::"

echo "::group::assert all runtime shared libraries resolve"
# A missing WebKitGTK/appindicator dep (or a glibc/libheif floor the package
# manager didn't enforce) shows up here as "not found". (No --version launch:
# Tuxlink is a Tauri GUI app; there is no display in CI and a launch is not a
# dependency test.)
missing="$(ldd "$bin" 2>/dev/null | awk '/not found/ {print}')"
if [ -n "$missing" ]; then
  echo "FAIL: unresolved shared libraries after install:"
  echo "$missing"
  exit 1
fi
echo "::endgroup::"

echo "OK: ${DEB##*/} installs cleanly via apt and all shared libraries resolve."
