#!/usr/bin/env bash
# deb-install-smoke.sh — runs INSIDE a clean Debian container (no dev toolchain,
# fresh apt state) to validate Tuxlink's DOCUMENTED end-user install path.
#
# Three modes:
#   install (default) — SUPPORTED distro (Debian 13 Trixie+, Ubuntu 24.04+).
#     Mirrors docs/install.md: `apt-get update && apt-get install -y ./*.deb`,
#     then asserts apt resolved every Depends, the binary is present, and its
#     shared libraries ALL resolve (ldd has no "not found"). Additionally
#     asserts the hamlib sidecar bundling contract (tuxlink-hs2k): the
#     tuxlink .deb's own Depends field does NOT declare hamlib (system
#     hamlib may still land on the box via the `Recommends: wsjtx`
#     chain — tuxlink-b026z.2's jt9 decode oracle — which is legitimate
#     and asserted-for, not forbidden), the bundled tuxlink-rigctld binary
#     is present + fully linked, and a live dummy-backend rigctl
#     round-trip (set/get frequency) works end to end.
#   hamlib-present — the machine that actually breaks: a container that
#     ALREADY has libhamlib-utils installed (so /usr/bin/rigctld exists)
#     before Tuxlink is installed. Asserts the Tuxlink .deb still installs
#     cleanly — the direct guard against the historical /usr/bin/rigctld
#     collision now that the bundled binaries are named tuxlink-rigctl /
#     tuxlink-rigctld (tuxlink-hs2k).
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
# Usage: deb-install-smoke.sh <path-to-.deb> [install|hamlib-present|refuse]
set -euo pipefail

DEB="${1:?usage: deb-install-smoke.sh <path-to-.deb> [install|hamlib-present|refuse]}"
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

if [ "$MODE" = "hamlib-present" ]; then
  echo "::group::apt-get install libhamlib-utils (simulate a machine that already has system hamlib)"
  # The R2 collision case: /usr/bin/rigctld already exists (from libhamlib-utils)
  # BEFORE Tuxlink installs its own bundled tuxlink-rigctl/tuxlink-rigctld. The
  # binaries are renamed precisely so this never collides; installing here first
  # is the direct regression guard for that rename ever being undone.
  apt-get install -y libhamlib-utils
  echo "::endgroup::"
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

if [ "$MODE" = "install" ]; then
  # tuxlink-hs2k: the clean-container case is the ONE state that was never
  # broken (a machine that never had hamlib installed can't hit the R2
  # collision). Prove the bundled hamlib sidecar contract here instead: the
  # tuxlink .deb's own Depends field must not require system hamlib. That
  # contract is about tuxlink's OWN declared dependency, not about whether
  # libhamlib-utils ends up on the box by any means — since
  # `Recommends: wsjtx` (tuxlink-b026z.2, the jt9 decode oracle), apt's
  # default install-recommends legitimately pulls Debian's wsjtx package,
  # whose own dependency chain drags in libhamlib-utils for wsjtx's rig
  # control. That's an attributable, expected side effect, not a violation.
  # If libhamlib-utils shows up WITHOUT wsjtx present, something else is
  # pulling it in and the contract is actually broken. Either way, the
  # bundled tuxlink-rigctld assertions below prove the sidecar still works
  # correctly with system hamlib present on the box — the same guarantee
  # the hamlib-present mode pins explicitly.
  echo "::group::assert no system-hamlib Depends + bundled tuxlink-rigctld runs"
  deb_depends="$(dpkg-deb -f "$DEB" Depends || true)"
  if printf '%s' "$deb_depends" | grep -qiE 'hamlib'; then
    echo "FAIL: tuxlink .deb declares a hamlib Depends — the bundled-sidecar contract (tuxlink-hs2k) forbids it"
    echo "Depends: $deb_depends"
    exit 1
  fi
  if dpkg -s libhamlib-utils >/dev/null 2>&1 && ! dpkg -s wsjtx >/dev/null 2>&1; then
    echo "FAIL: system hamlib present but not attributable to the wsjtx recommends chain"
    exit 1
  fi
  test -x /usr/bin/tuxlink-rigctld
  test -x /usr/bin/tuxlink-rigctl
  missing_rigctld="$(ldd /usr/bin/tuxlink-rigctld 2>/dev/null | awk '/not found/ {print}')"
  if [ -n "$missing_rigctld" ]; then
    echo "FAIL: unresolved shared libraries in bundled tuxlink-rigctld:"
    echo "$missing_rigctld"
    exit 1
  fi
  /usr/bin/tuxlink-rigctld -m 1 -t 4590 & dp=$!
  sleep 1
  /usr/bin/tuxlink-rigctl -m 2 -r localhost:4590 F 14074000
  # `rigctl -m 2` prints a "rigctld: Hamlib ..." banner before the value; take the
  # frequency line (all-digits), not the whole multi-line capture.
  freq="$(/usr/bin/tuxlink-rigctl -m 2 -r localhost:4590 f | grep -m1 -E '^[0-9]+$' || true)"
  kill "$dp" 2>/dev/null || true
  if [ "$freq" != "14074000" ]; then
    echo "FAIL: dummy-backend rigctl round-trip returned '$freq', expected 14074000"
    exit 1
  fi
  echo "::endgroup::"
fi

echo "OK: ${DEB##*/} installs cleanly via apt and all shared libraries resolve."
