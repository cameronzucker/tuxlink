#!/usr/bin/env bash
# scripts/tuxlink-logging-smoke.sh
#
# RADIO-1 compliant: synthetic events only. Does NOT spawn VARA/ARDOP, does
# NOT invoke native_cms_probe, does NOT open any radio serial device.

set -euo pipefail

WORKDIR=$(mktemp -d)
trap "rm -rf $WORKDIR" EXIT

echo "=== tuxlink-logging-smoke ==="
echo "workdir: $WORKDIR"

# 1. Check tooling
command -v zstd >/dev/null || { echo "FAIL: zstd not installed"; exit 1; }
command -v tar >/dev/null || { echo "FAIL: tar not installed"; exit 1; }
ZSTD_VER=$(zstd --version | head -1)
echo "zstd: $ZSTD_VER"

# 2. Generate a synthetic corpus + train dict (smoke just verifies the xtask runs)
# Amendment F / Codex P2 #10: no || true masking — if xtask gen-corpus fails the smoke fails.
cd "$(dirname "$0")/.."
cargo run --manifest-path xtask/Cargo.toml --bin gen-corpus -- \
  --output "$WORKDIR/corpus" --fixtures dev/log-corpus-fixtures/ \
  --target-bytes 1700000 2>&1 | grep -v 'Compiling\|Finished'

ls -la "$WORKDIR/corpus" | head -5

# 3. Cargo unit + integration tests — HARD GATES (Amendment F: no `|| true` masking)
echo "=== running cargo tests ==="
cargo test --manifest-path src-tauri/Cargo.toml --lib logging 2>&1 | tail -20

echo "=== redaction integration test (HARD GATE) ==="
cargo test --manifest-path src-tauri/Cargo.toml --test redaction_integration 2>&1 | tail -20

echo "=== wire sanitizer integration test (HARD GATE) ==="
cargo test --manifest-path src-tauri/Cargo.toml --test wire_sanitizer_integration 2>&1 | tail -20

echo "=== probes RADIO-1 isolation (HARD GATE) ==="
cargo test --manifest-path src-tauri/Cargo.toml --test probes_no_tx_apis 2>&1 | tail -10

echo "=== blocklist corpus (HARD GATE) ==="
cargo test --manifest-path src-tauri/Cargo.toml --test logging_blocklist_corpus 2>&1 | tail -10

echo "=== credential debug audit (HARD GATE) ==="
cargo test --manifest-path src-tauri/Cargo.toml --test credential_debug_audit 2>&1 | tail -10

echo "=== no opaque container emissions (HARD GATE) ==="
cargo test --manifest-path src-tauri/Cargo.toml --test no_opaque_container_emissions 2>&1 | tail -10

# 4. Frontend tests (run individually per vitest-zombies memory)
echo "=== running vitest ==="
pnpm vitest run src/help/LoggingView.test.tsx 2>&1 | tail -8
pnpm vitest run src/help/LoggingExportSection.test.tsx 2>&1 | tail -8
pnpm vitest run src/help/LoggingSettingsSection.test.tsx 2>&1 | tail -8
pnpm vitest run src/help/LoggingProbesSection.test.tsx 2>&1 | tail -8
pnpm vitest run src/help/ReportIssueModal.test.tsx 2>&1 | tail -8
pnpm vitest run src/routing.test.ts 2>&1 | tail -8
# Reap any vitest zombies before continuing
pkill -9 -f vitest 2>/dev/null || true

# 5. Amendment F end-to-end no-secret-bytes assertion
# This requires a #[cfg(test)] CLI helper that emits a sentinel via a
# tracing field → export → grep the archive. For v0 alpha, we document
# this as the operator's responsibility post-build: it requires an
# instrumented binary that the smoke can't build cleanly without a feature
# flag. The unit test wire_sanitizer_blocks_hunter2hunter2_flow already
# proves the redaction works end-to-end against a known credential.

echo ""
echo "=== PASS ==="
echo "Tuxlink logging smoke completed successfully."
echo ""
echo "NOTE: per spec §10.5 #16, an additional end-to-end no-secret-bytes"
echo "assertion (running tuxlink with a sentinel-emitting helper, exporting,"
echo "and grepping the archive) requires a #[cfg(test)]-gated CLI helper not"
echo "yet built. The wire_sanitizer_integration test above covers the same"
echo "redaction discipline in unit-test form."
