#!/usr/bin/env bash
# Station Intelligence GPL-boundary guard (tuxlink-b026z.7).
# jt9/WSJT-X may be invoked as a subprocess ONLY, from ONE module.
# See docs/design/2026-07-10-station-intel-jt9-engine-delta.md §GPL boundary.
set -euo pipefail
fail=0
err() { echo "PROVENANCE VIOLATION: $1" >&2; fail=1; }

# 1. No GPL source files (WSJT-X Fortran / LDPC tables) in the tree.
if git ls-files | grep -E '\.(f90|f95)$|(^|/)(parity|generator)\.dat$' >&2; then
  err "GPL-source-shaped files tracked (see above)"
fi

# 2. No dependency edge on GPL Rust crates.
# (/dev/null sentinel: with an empty file list grep sees one unmatchable
# operand and exits 1 cleanly — never reads stdin, never hangs; do NOT use
# xargs -r, whose empty-input exit 0 would misread as a violation.)
if git ls-files '*Cargo.toml' | xargs grep -lnE '^\s*(wsjtr|ft8core)\s*=' /dev/null >&2; then
  err "Cargo dependency on wsjtr/ft8core"
fi

# 3. No FFI in any Rust file that mentions wsjt.
if git ls-files '*.rs' | xargs grep -liE 'wsjt' /dev/null | xargs grep -lnE '#\[link|extern\s+"C"' /dev/null >&2; then
  err "FFI in a wsjt-mentioning Rust file"
fi

# 4. No bundling of jt9/wsjtx in any artifact.
if jq -e '((.bundle.externalBin // []) + (.bundle.resources // [])) | map(select(test("jt9|wsjt"; "i"))) | length > 0' src-tauri/tauri.conf.json >/dev/null; then
  err "tauri.conf.json bundles jt9/wsjtx"
fi
if grep -nE 'binaries/(jt9|wsjt)' .github/workflows/release.yml >&2; then
  err "release.yml externalBin-injects jt9/wsjtx"
fi

# 5. Subprocess confinement: "jt9" in spawn position only inside the runner.
if git ls-files '*.rs' | grep -v 'tuxlink-jt9/src/runner.rs' \
    | xargs grep -lnE 'Command::new\([^)]*jt9' /dev/null >&2; then
  err "jt9 spawned outside tuxlink-jt9/src/runner.rs"
fi
# Strip comments BEFORE matching (grep -n would prefix line numbers, making
# the '^\s*//' filter dead — the guard would self-trip on the module doc,
# which legitimately says the flag is banned).
if grep -vE '^\s*//' src-tauri/tuxlink-jt9/src/runner.rs | grep -nE -- '-s\b|--shmem' >&2; then
  err "shmem flag in the jt9 arg builder (GPL boundary-crosser)"
fi

exit $fail
