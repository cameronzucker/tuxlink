#!/usr/bin/env bash
#
# tuxmodem-loopback-smoke.sh — agent-runnable end-to-end loopback validation
# for the tuxmodem program (tuxlink-l5rf, follow-up to tuxlink-9ggl umbrella).
#
# For each of the three frame modes (raw / sync / multi-sync), encodes a
# payload via tuxmodem-tx --write-wav, decodes via tuxmodem-rx --decode-wav
# --expected, and verifies CLEAN MATCH. No radio, no PTT, no Part 97 risk
# — purely WAV-mediated I/O on the local filesystem.
#
# Exit code 0 = all cases passed; 1 = one or more failures.
#
# Usage:
#   bash scripts/tuxmodem-loopback-smoke.sh
#
# Per [RADIO-1] this script does NOT exercise the on-air TX/RX path. For
# that, see the docs in tuxmodem/crates/tuxmodem-tx (PR #366 onward).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TX_MANIFEST="${REPO_ROOT}/tuxmodem/crates/tuxmodem-tx/Cargo.toml"
RX_MANIFEST="${REPO_ROOT}/tuxmodem/crates/tuxmodem-rx/Cargo.toml"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

PASS=0
FAIL=0
TOTAL=0

# Helpers ────────────────────────────────────────────────────────────

run_case() {
  local label="$1"
  local frame_mode="$2"
  local payload="$3"
  local wav_path="${WORK_DIR}/${label}.wav"

  TOTAL=$((TOTAL + 1))
  echo
  echo "[case ${TOTAL}] ${label}: --frame-mode ${frame_mode}, payload ${#payload} byte(s)"

  if ! cargo run --quiet --manifest-path "${TX_MANIFEST}" --bin tuxmodem-tx -- \
        --write-wav "${wav_path}" \
        --payload "${payload}" \
        --mode wide-floor \
        --frame-mode "${frame_mode}" >/dev/null; then
    echo "  FAIL: tuxmodem-tx exited non-zero"
    FAIL=$((FAIL + 1))
    return
  fi

  local rx_output
  if ! rx_output="$(cargo run --quiet --manifest-path "${RX_MANIFEST}" --bin tuxmodem-rx -- \
        --decode-wav "${wav_path}" \
        --expected "${payload}" \
        --frame-mode "${frame_mode}" 2>&1)"; then
    echo "  FAIL: tuxmodem-rx exited non-zero"
    echo "  ${rx_output}"
    FAIL=$((FAIL + 1))
    return
  fi

  if grep -q "CLEAN MATCH" <<< "${rx_output}"; then
    echo "  PASS"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: no CLEAN MATCH in tuxmodem-rx output"
    echo "${rx_output}" | sed 's/^/    /'
    FAIL=$((FAIL + 1))
  fi
}

# Random-ish payload generator (deterministic for reproducibility)
gen_payload() {
  local n="$1"
  python3 -c "
import sys
n = int(sys.argv[1])
state = 0xDEADBEEF
out = []
for _ in range(n):
    state = (state * 1103515245 + 12345) & 0xFFFFFFFF
    out.append((state >> 16) & 0xFF)
sys.stdout.buffer.write(bytes(out))
" "${n}" | base64 -w 0
}

# Cases ──────────────────────────────────────────────────────────────

# Note: payloads must avoid embedded NUL bytes for the raw mode case (the
# bare receive() trim heuristic strips trailing zeros). Use short ASCII
# strings for raw + sync; use base64-encoded random bytes for multi-sync.

echo "tuxmodem loopback smoke (tuxlink-l5rf)"
echo "======================================"

run_case "raw-5b" "raw" "TEST!"
run_case "sync-7b" "sync" "PREAMB!"

LONG_PAYLOAD="$(gen_payload 200)"   # ~268 chars base64-encoded
run_case "multi-sync-200b" "multi-sync" "${LONG_PAYLOAD}"

# Summary ────────────────────────────────────────────────────────────

echo
echo "======================================"
echo "Result: ${PASS}/${TOTAL} passed, ${FAIL} failed"
if [[ ${FAIL} -eq 0 ]]; then
  exit 0
else
  exit 1
fi
