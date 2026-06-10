# voacapl CI bundling — reference

## Why `externalBin` and `itshfbc` are not in `tauri.conf.json`

Tauri 2 validates `externalBin` entries at `cargo build` time: it requires the
per-triple binary (`src-tauri/binaries/voacapl-<triple>`) to be present on disk
or the build fails with `resource path binaries/voacapl-... doesn't exist`. The
binary is gitignored (CI-generated), so committing `externalBin` breaks
`cargo build` on any clean checkout or CI runner that hasn't staged it yet.

The same applies to the `resources/itshfbc/**/*` glob: Tauri requires at least
one matching file; the `.gitkeep` satisfies that during staging, but the glob
match also trips the existence check in some Tauri versions when the full data
tree is absent.

**At the bundle-wiring step** (after staging the binary and itshfbc tree per the
instructions below), re-add both entries to `tauri.conf.json` — or inject them
via a `TAURI_CONFIG` environment variable patch — before running
`pnpm tauri build`:

```json
"externalBin": ["binaries/voacapl"],
"resources": ["resources/wle-forms/**/*", "resources/itshfbc/**/*", "resources/propagation/ssn-forecast.json"]
```

Do not commit this change; it must remain a CI-only step.

---

Before `pnpm tauri build` (or `pnpm tauri build --bundles deb,...`) can produce
a bundle, three artifacts must be staged in the worktree:

```
src-tauri/binaries/voacapl-<target-triple>   ← arch-specific binary
src-tauri/resources/itshfbc/                 ← itshfbc data tree (populated by makeitshfbc)
```

Both paths are gitignored (`src-tauri/.gitignore`) — they are never committed.
The `.gitkeep` files in each directory are tracked so `cargo build`'s resource
glob (`resources/itshfbc/**/*`) always matches at least one file.

---

## Local staging (Pi, dev machine)

These commands reproduce what CI does locally. Run them once before
`pnpm tauri build` or before running `propagation_live` if voacapl is not
already installed at the default path.

```bash
# 0. Prerequisites (one-time)
sudo apt-get install -y gfortran    # Fortran compiler; required to build voacapl

# 1. Clone and build voacapl from source
git clone https://github.com/jawatson/voacapl.git /tmp/voacapl-src
cd /tmp/voacapl-src
./configure --prefix="$HOME/.local"
make -j$(nproc)
make install
# Binary is now at ~/.local/bin/voacapl

# 2. Run makeitshfbc to populate the data tree
#    This creates ~/itshfbc/ with database/, coeffs/, antennas/, geo* subdirs
makeitshfbc
# Verify: ls ~/itshfbc/database/version.w32

# 3. Stage the binary into the worktree (Tauri externalBin convention)
#    Replace the triple with the output of `rustc -vV | grep host | awk '{print $2}'`
TRIPLE=$(rustc -vV | awk '/^host:/{print $2}')
mkdir -p src-tauri/binaries
cp ~/.local/bin/voacapl "src-tauri/binaries/voacapl-${TRIPLE}"

# 4. Stage the itshfbc data tree into resources
mkdir -p src-tauri/resources/itshfbc
rsync -a ~/itshfbc/ src-tauri/resources/itshfbc/
# (rsync preserves the .gitkeep if present; the large data files override it)
```

After these steps, `cargo build` and `pnpm tauri build` both work.

---

## amd64 CI snippet (GitHub Actions — ready to paste)

This snippet installs gfortran, builds voacapl, and stages both artifacts
before the Tauri bundle step. Insert it in `release.yml` between
"Install JS dependencies" and "Lint docs links":

```yaml
      - name: Build and stage voacapl
        run: |
          sudo apt-get install -y gfortran
          git clone --depth=1 https://github.com/jawatson/voacapl.git /tmp/voacapl-src
          cd /tmp/voacapl-src
          ./configure --prefix=/tmp/voacapl-install
          make -j$(nproc)
          make install
          makeitshfbc  # populates ~/itshfbc by default; see INSTALL for --prefix

          TRIPLE=$(rustc -vV | awk '/^host:/{print $2}')
          mkdir -p src-tauri/binaries
          cp /tmp/voacapl-install/bin/voacapl "src-tauri/binaries/voacapl-${TRIPLE}"

          mkdir -p src-tauri/resources/itshfbc
          rsync -a ~/itshfbc/ src-tauri/resources/itshfbc/
          ls -lh src-tauri/resources/itshfbc/database/version.w32
```

This has been manually verified on aarch64 (the dev Pi) but **has not been
tested in a GitHub Actions runner**. Treat it as a starting point; the exact
`makeitshfbc` prefix behavior may differ across versions.

---

## arm64 CI strategy — **OPERATOR DECISION — not yet wired**

The `release.yml` already uses a matrix with `ubuntu-latest` (amd64) and
`ubuntu-24.04-arm` (arm64). voacapl builds from Fortran source and produces an
arch-specific binary, so a separate build is needed per target triple.

Two options; neither is wired yet:

### Option A — Native arm64 runner (recommended path)

The `ubuntu-24.04-arm` runner in the existing matrix can build voacapl natively.
The amd64 snippet above should work unchanged on arm64 because the build is
from source.

**Risk:** `gfortran` availability on `ubuntu-24.04-arm` has not been confirmed.
Run `apt-cache show gfortran` in a one-off workflow to verify before committing.

**Tradeoff:** simplest approach; doubles the voacapl build time in CI (one per
matrix row). voacapl builds in well under a minute on the dev Pi, so this is
unlikely to be a bottleneck.

### Option B — Cross-compile from amd64

Build the arm64 binary on the amd64 runner using a cross-Fortran toolchain
(`gfortran-aarch64-linux-gnu`) and cross-configured autoconf.

**Risk:** voacapl's `configure` script may not support cross-compilation without
patching. Cross-Fortran toolchains on Ubuntu are less well-tested than native
builds.

**Tradeoff:** more complex; no clear benefit over Option A given arm64 runners
exist in the matrix already.

### Recommendation

Start with Option A (native arm64 runner). Confirm `gfortran` availability in
a one-off probe workflow, then paste the amd64 snippet into both matrix rows.
Validate by checking that `src-tauri/binaries/voacapl-aarch64-unknown-linux-gnu`
is present and executable after the step, before the Tauri bundle.

---

## .deb size growth

The itshfbc data tree is approximately 1.4 MB on disk. The binary is 1.3 MB.
Total addition to the `.deb` bundle is approximately **2.7 MB** before
compression. Actual `.deb` size growth depends on the Tauri bundle's compression
level and what the baseline deb currently contains. This is expected to be well
within acceptable package size for a hamradio desktop application.
