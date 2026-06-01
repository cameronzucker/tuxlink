#!/usr/bin/env bash
# Mode 7: port 1420 ownership / Vite strictPort collision.
#
# Scenario: multiple worktrees of tuxlink + multiple agents → multiple
# `tauri dev` instances trying to bind Vite's :1420. Vite's strictPort
# means only ONE can run at a time machine-wide. Without verification,
# converge-build would launch tauri dev, the strictPort check would
# fail, and the user would see a confusing error.
#
# Handler: converge-build.sh's `verify_port_free` — after the blanket
# pkill, checks port 1420 via `ss -lntp` / `lsof`; exits 7 if still
# held. (Codex P1 #4 fix in PR #203.)
# Future handler (PR #206 dev-server lease): inspect the lease file +
# show the owner's PID/cwd/branch before deciding whether to kill.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

converge="${REPO_ROOT}/scripts/converge-build.sh"

# Verify verify_port_free function exists.
if ! grep -q '^verify_port_free()' "${converge}"; then
  fixture_fail "verify_port_free() missing"
  report_and_exit
fi
fixture_pass "verify_port_free() defined"

# Verify it tries ss FIRST, then falls back to lsof.
if grep -A 20 '^verify_port_free()' "${converge}" | grep -q 'command -v ss'; then
  fixture_pass "verify_port_free tries ss"
else
  fixture_fail "verify_port_free does NOT try ss"
fi
if grep -A 30 '^verify_port_free()' "${converge}" | grep -q 'command -v lsof'; then
  fixture_pass "verify_port_free has lsof fallback"
else
  fixture_fail "verify_port_free missing lsof fallback"
fi

# Verify it exits 7 when port still occupied.
if grep -A 30 '^verify_port_free()' "${converge}" | grep -q 'exit 7'; then
  fixture_pass "verify_port_free exits 7 on port-still-busy"
else
  fixture_fail "verify_port_free does NOT exit 7 on busy port"
fi

# Verify the audit log records the busy state.
if grep -A 30 '^verify_port_free()' "${converge}" | grep -q 'audit "port_1420_busy"'; then
  fixture_pass "verify_port_free audits port-busy state for forensics"
else
  fixture_fail "verify_port_free does NOT audit port-busy"
fi

# Verify the kill_stale_dev_processes function exists too (verify_port_free
# is meaningless without something to kill first).
if grep -q '^kill_stale_dev_processes()' "${converge}"; then
  fixture_pass "kill_stale_dev_processes() defined"
else
  fixture_fail "kill_stale_dev_processes() missing"
fi

# Verify the kill pattern matches the three process classes.
if grep -A 30 '^kill_stale_dev_processes()' "${converge}" | \
    grep -q 'tauri dev|target/debug/tuxlink|node\.\*vite'; then
  fixture_pass "kill_stale_dev_processes matches tauri/tuxlink/vite"
else
  fixture_fail "kill_stale_dev_processes pattern missing"
fi

report_and_exit
