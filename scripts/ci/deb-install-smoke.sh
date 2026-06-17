#!/usr/bin/env bash
# deb-install-smoke.sh — runs INSIDE a clean Debian container (no dev toolchain,
# fresh apt state) to validate Tuxlink's DOCUMENTED end-user install path.
#
# Mirrors docs/install.md exactly:
#     sudo apt-get update
#     sudo apt-get install -y ./tuxlink_<ver>_<arch>.deb
# and asserts that
#   1. apt resolves EVERY Depends with no --fix-broken needed (WebKitGTK 4.1 and
#      libayatana-appindicator are auto-injected into the .deb by the Tauri
#      bundler; libsecret/libheif/libde265/libwebp come from tauri.conf.json), and
#   2. the installed binary's shared libraries ALL resolve on the stock image.
#
# This is class-prevention for GH #786 (tuxlink-w636): a clean-container PASS is
# proof the shipped .deb installs cleanly on a stock system (the #786 failure was
# a broken apt state on the tester's machine + `dpkg -i`, which does NOT resolve
# deps); a FAIL means a real dependency regression (wrong/missing package name,
# a dropped auto-inject) — caught in CI instead of by a tester.
#
# Usage: deb-install-smoke.sh <path-to-.deb>
set -euo pipefail

DEB="${1:?usage: deb-install-smoke.sh <path-to-.deb>}"
DEB="$(readlink -f "$DEB")"
test -f "$DEB"

export DEBIAN_FRONTEND=noninteractive

echo "::group::apt-get update (stock package index)"
apt-get update
echo "::endgroup::"

echo "::group::apt-get install ./${DEB##*/} (documented user path; resolves deps)"
# An absolute path makes apt treat the argument as a local .deb file (not a
# package name) and resolve its Depends from the configured repositories.
# Deliberately NOT `dpkg -i`: dpkg does not download/resolve deps — that misuse
# was the GH #786 wall. A non-zero exit here = unmet Depends = the bug we guard.
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
# ldd surfaces any NEEDED shared object the install failed to provide. On a clean
# image this is the real test of the dependency closure: a missing WebKitGTK or
# appindicator dep shows up here as "not found". (No --version launch: Tuxlink is
# a Tauri GUI app; there is no display in CI and a launch is not a dependency test.)
missing="$(ldd "$bin" 2>/dev/null | awk '/not found/ {print}')"
if [ -n "$missing" ]; then
  echo "FAIL: unresolved shared libraries after install:"
  echo "$missing"
  exit 1
fi
echo "::endgroup::"

echo "OK: ${DEB##*/} installs cleanly via apt and all shared libraries resolve."
