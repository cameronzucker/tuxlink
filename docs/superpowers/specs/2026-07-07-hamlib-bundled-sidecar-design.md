# Design — Bundle rigctl/rigctld as a sidecar; drop the system hamlib dependency

- **bd issue:** tuxlink-a9ip3
- **Date:** 2026-07-07
- **Agent:** salamander-sage-magnolia
- **Status:** approved (operator-converged) + hardened by 5-round adversarial review
  (4 Claude angles + Codex) and an **empirical arm64 static build** (hamlib 4.6.2)

## Problem

Tuxlink drives rig control by shelling out to hamlib's `rigctl`/`rigctld`
**binaries** — [`tux-rig/src/list.rs`](../../../src-tauri/tux-rig/src/list.rs) runs
`rigctl -l` to enumerate rig models, and
[`tux-rig/src/managed.rs`](../../../src-tauri/tux-rig/src/managed.rs) spawns
`rigctld -m <model> -r <serial> -s <baud> -t <port>`, talks the rigctld TCP
protocol to set freq/mode/PTT, then kills+reaps to release the CAT serial. It
does **not** link `libhamlib`; there is no hamlib FFI crate.

Despite that version-agnostic runtime coupling, the shipped packages declare a
**hard, exact-version system dependency** on hamlib:

- Core `.deb`: `bundle.linux.deb.depends` includes `libhamlib-utils`.
- `.rpm`: `bundle.linux.rpm.depends` includes `hamlib`.

`libhamlib-utils` internally pins its lib at an **exact** version
(`libhamlib4t64 (= 4.5.5-3.2build2)` on Ubuntu 24). On any machine that already
carries a *different* hamlib — a source build, a PPA, or a copy pulled by
WSJT-X/fldigi — that pin fails to resolve and the **entire Tuxlink install is
refused**.

**Verified failure (2026-07-07):** on Ubuntu 24 R2, which already had hamlib,
both `dpkg -i tuxlink_0.83.0_amd64.deb` and `apt install ./…` failed with
`libhamlib-utils … is not going to be installed`.

Related holes: the ECT low-floor build provisions hamlib nowhere (drops it from
`depends`, cannot `apt install` it on a frozen EOL platform); AppImage relies on
the host with no guarantee.

The pattern the ECT build already uses for this class of problem is to **bundle
the binary as a Tauri sidecar** — `ardopcf`, `voacapl`, `pmtiles` are built/staged
in CI and shipped inside the package because the platform can't be trusted to
provide them.

## Decision

Treat `rigctl`/`rigctld` like `ardopcf`: **build them from pinned hamlib source as
a per-arch Tauri sidecar, always use the bundled copy, and drop the conflicting
system hamlib package dependency.** Keep a small set of ubiquitous base-lib
dependencies (below) — the point is to drop the *conflicting exact-pinned hamlib*,
not to reach zero dependencies.

### Why "always bundled," not "prefer system"

A machine may carry an **ancient, never-configured hamlib** from a long-ago
third-party install. "Prefer system" would silently adopt that untested copy and
make Tuxlink's behavior depend on cruft we never validated — a failure that
*looks* like it should work. Determinism wins: every user runs the exact
`rigctld` we built and tested. This upgrades `list.rs`'s "accurate to the
operator's hamlib" intent (written when the system copy was the only option): a
recent bundled hamlib supports *more* rigs than most system copies.

## Empirical validation (arm64, hamlib 4.6.2, 2026-07-07)

A real static build on this Pi confirmed the load-bearing assumptions and set the
concrete build contract:

- `./configure --enable-static --disable-shared --without-readline
  --without-cxx-binding` → **`libhamlib` absent from `ldd`** (linked in); real
  **ELF** (not a libtool wrapper); readline/tinfo/ncurses/gpiod/xml2 all absent.
- `rigctl -m 1049 --dump-caps` (FT-710) and `-m 3088` (G90) both succeed; both
  appear in `rigctl -l`.
- Dummy-backend smoke: `rigctld -m 1` → `rigctl F 14074000` → read-back
  `14074000`. The real spawn→connect→set→get→reap path works.
- **Residual dynamic deps: `libusb-1.0`, `libudev`, `libcap`** (ubiquitous, stable,
  non-conflicting base libs) plus `libc`/`libm`. These become explicit `Depends`.
- Unstripped binary is **24 MB** → must `strip` (→ ~3–5 MB).
- The Pi build references up to `GLIBC_2.38` (Pi is glibc 2.41) — proof that each
  floor's sidecar **must** be built on its matching runner and the GLIBC symbol
  floor asserted in CI. Validation binary is throwaway; real sidecars come from
  the matched CI runners.

## Changes

### 1. CI — build + stage the hamlib sidecar (both build floors)

Mirror the `ardopcf` staging step in **both** `release.yml` (core, `ubuntu-latest`
= glibc 2.39) and `ect-build.yml` (low floor, `ubuntu-22.04` = glibc 2.35), amd64
+ arm64 matrix each:

1. **Fetch pinned source:** download the **hamlib 4.6.2 release tarball** and
   verify `sha256 = b2ac73f44dd1161e95fdee6c95276144757647bf92d7fdb369ee2fe41ed47ae8`.
   Tarball (not `git clone`) — it ships generated `configure`, so no autotools /
   no voacapl-style timestamp dance. Pin recorded in the workflow = source
   provenance record.
2. **Deterministic deps:** `apt-get install -y libusb-1.0-dev pkg-config` so
   libusb linkage is deterministic (not dependent on undocumented runner
   contents). USB support is retained for USB-CAT rigs (IC-705 etc.); FT-710/G90
   use serial and don't need it, but it's harmless and ubiquitous.
3. **Configure + build + install:**
   `./configure --enable-static --disable-shared --without-readline
   --without-cxx-binding`, `make -j"$(nproc)"`, `make install-strip
   DESTDIR=…`. Stage `rigctl`/`rigctld` from the **install prefix `$prefix/bin/`**
   (never the build tree — libtool wrappers) into `src-tauri/binaries/`.
4. **Sidecar names (COLLISION FIX — ship-blocker):** stage as
   **`tuxlink-rigctl-<triple>`** and **`tuxlink-rigctld-<triple>`**. Tauri strips
   the triple and installs sidecars next to the main binary at `/usr/bin/…`;
   naming them `rigctl`/`rigctld` would land on `/usr/bin/rigctl` / `/usr/bin/rigctld`,
   **which `libhamlib-utils` (deb) and `hamlib` (rpm) already own → dpkg/rpm
   refuse to overwrite → install fails on every machine with hamlib**, worse than
   today and bidirectionally (later `apt install wsjtx` would fail too). The
   `tuxlink-` prefix is owned by no other package. **Do NOT use
   `Conflicts`/`Replaces`/`Obsoletes`** — that force-removes hamlib and
   cascade-breaks WSJT-X/fldigi/direwolf-PTT.
5. **externalBin injection (CI-only):** add `binaries/tuxlink-rigctl` and
   `binaries/tuxlink-rigctld` to the **`jq` inject step** in each workflow — never
   to the committed `tauri.conf.json` (a committed `externalBin` breaks the
   `verify` gate, which validates sidecar existence at cargo-build time on runners
   that haven't staged them).
6. **CI assertions (build time, both floors, both arches):**
   - `file` → both staged binaries are **ELF**, not shell scripts.
   - `ldd` on **both** `tuxlink-rigctl` and `tuxlink-rigctld`, **deny-by-default**:
     fail unless every entry is in the allow-set `{libc, libm, libpthread, libdl,
     ld-linux*, linux-vdso, libusb-1.0, libudev, libcap}`. Any of
     `libhamlib`/`readline`/`tinfo`/`ncurses`/`gpiod`/`xml2` → red build.
   - `readelf --version-info` → assert max referenced `GLIBC_*` symbol ≤ the
     floor: **ECT build ≤ `GLIBC_2.35`**, core build ≤ `GLIBC_2.39`.
   - **Capability:** `tuxlink-rigctl -m 1049 --dump-caps` and `-m 3088 --dump-caps`
     succeed (FT-710 + G90 backends present — closes version-floor, backend-pruning,
     and PTT parity at once). Fixture list also asserts IC-7300 (3073), FT-991
     for regression breadth.
   - **Functional:** `tuxlink-rigctld -m 1 -t <port>` (dummy) → `tuxlink-rigctl -m 2
     -r localhost:<port> F 14074000` → read `f` == `14074000` → kill+reap.
   - Extend the bundle-extraction assertion to `test -x …/tuxlink-rigctl` and
     `…/tuxlink-rigctld` alongside `ardopcf`/`voacapl`/`pmtiles` (a missed stage
     step → red build, since CI has no rig hardware to catch it at runtime).
7. **Cache:** `actions/cache` the staged
   `src-tauri/binaries/tuxlink-rig{ctl,ctld}-<triple>` keyed on
   `hamlib-4.6.2-<arch>-<configure-flags-hash>`; steady-state = cache restore, not
   a ~250-backend recompile across 4 legs.
8. **License/provenance:** ship hamlib's `COPYING` (GPL-2.0-or-later utils) in a
   `THIRD-PARTY-LICENSES` bundle resource; the pinned source ref in the workflow is
   the corresponding-source pointer. Extend the same mechanism to `ardopcf`/
   `voacapl` which have the same latent obligation.

### 2. Packaging — swap the hamlib dependency for base libs

In `src-tauri/tauri.conf.json`:

- `bundle.linux.deb.depends`: **remove** `libhamlib-utils`; **add** `libusb-1.0-0`,
  `libudev1`, `libcap2` (the bundled binary's real runtime closure — stable,
  non-conflicting).
- `bundle.linux.rpm.depends`: **remove** `hamlib`; **add** `libusb1`,
  `systemd-libs` (libudev), `libcap`.
- `direwolf` remains a `recommends` (untouched).
- No `externalBin` in committed config (see §1.5).

### 3. App — default to the bundled binary, resolve its path

- **Resolution lives in `modem_commands.rs`** (the Tauri layer), **not** the
  `tux-rig` leaf crate (which stays a dumb `String`→`Command::new` consumer).
  Mirror `resolve_ardop_binary` (`modem_commands.rs:101`): given the configured
  value, if it equals the **literal default name** resolve
  `current_exe().parent().join("tuxlink-rigctld")` iff it `.exists()`, else fall
  back to the configured value (bare name → `$PATH`, absolute path → verbatim).
- **Sentinel = the literal default name, NOT empty-string.** Existing configs
  persist `rigctld_binary: "rigctld"`; treat that literal (and `"rigctl"`) as the
  bundled opt-in. Missing/empty/legacy-`"rigctld"` all → bundled. Only a
  non-default value (absolute path or other command) bypasses the bundle. This
  gives **zero-migration** upgrade; an empty-string sentinel would mis-read every
  existing config as an override and re-break 100% of upgraders.
- **Two binaries, one field:** `rig_list_models` (`modem_commands.rs:171`)
  currently hardcodes `"rigctl"` and reads no config — add `resolve_rigctl_binary`
  and wire it. For an explicit `rigctld_binary` **override**, derive the `rigctl`
  sibling from the **same directory** as the resolved `rigctld` (so picker models
  and control daemon never version-skew). Document that reaching *system* hamlib
  requires an absolute path / non-default name; reflect in the field placeholder.
- **`rig_config_from` (`modem_commands.rs:1916`)** is the single seam feeding both
  ARDOP (`tune_rig_for_connect`) and VARA (`vara/commands.rs`) — resolve there.
- **Errors:** keep a distinct, human-readable message for "bundled sidecar not
  found at resolved path" vs. "rigctld ran but the rig didn't answer" — do not
  collapse both into an opaque `RigError::Spawn`. No "install hamlib" banner
  (the binary always ships).
- **Type seam:** resolver returns `PathBuf`; `RigConfig.binary` is `String` —
  `.to_string_lossy().into_owned()` at the call site (as `ArdopConfig` does).

### 4. Docs — reconcile stale rig-control docs

`docs/user-guide/12-cat-and-rigctld.md` and `13-radio-specific-notes.md` state
"Tuxlink has no built-in rig-control client / rigctld deferred," but the code
drives CAT+PTT through the VARA/ARDOP connect path today. Update both to describe
the bundled `rigctld`, the zero-config default, and the `rigctld_binary` override.

## Testing

- **Rust (CI — Pi cannot compile Rust):**
  - Resolver unit test mirroring
    `resolve_ardop_binary_honors_explicit_paths_and_defaults_to_name`
    (`modem_commands.rs:2082`): literal default → bundled path when sibling
    exists, else bare-name fallback; absolute override → verbatim; `rigctl`
    sibling derived from `rigctld` dir. Update the `modem_commands.rs:4321`
    default assertion to exercise the fallback explicitly.
- **Frontend (vitest — runs on the Pi):**
  - `RigControlSection.test.tsx`: default config maps to bundled; explicit
    override round-trips; placeholder documents the absolute-path requirement.
- **Packaging / integration (CI, both arches):**
  - `deb-install-smoke.sh` gains cases: **(a)** clean/no-hamlib container →
    install succeeds, `! dpkg -s libhamlib-utils` (nothing pulled), and the
    installed `/usr/bin/tuxlink-rigctld -m 1` dummy smoke passes; **(b)**
    **hamlib pre-installed** (`apt-get install -y libhamlib-utils` first) →
    Tuxlink install still succeeds (the direct guard for the R2 collision);
    **(c)** upgrade-from-a-0.83.0-like state.
  - `ldd` the *installed* `tuxlink-rigctld` (not just the main binary) — fail on
    `not found`.
  - rpm: `rpm -qlp` shows `tuxlink-rigctld`, never `/usr/bin/rigctld`.
  - Use output-grep guards (`… | grep -q Hamlib`), not exit codes, for
    `--version`/`-h` (hamlib exit conventions vary).

## Rollout

- Ships via PR off `bd-tuxlink-a9ip3/hamlib-bundled-sidecar`.
- A `fix` (broken install) + a sidecar `feat`; release-please cuts **0.84.0** after
  merge; cut via `gh workflow run release-merge.yml` (pre-release, not promoted).
  Operator installs the amd64 `.deb` on R2 (Ubuntu 24) to test end-to-end.
- **Model-# migration:** IDs for FT-710 (1049) / G90 (3088) are stable across
  4.5.5→4.6.2, so real risk is low; on first run after the swap, validate the
  stored `rig_hamlib_model` against `tuxlink-rigctl -l` and surface a "re-select
  your rig" prompt only if the number no longer maps to the same mfg/model string.

## Out of scope

- Rig-control UX beyond the binary-resolution default and the migration prompt.
- Bundling other hamlib tools (`rotctl`, etc.).
- Windows/macOS packaging (Linux-only project).
- libusb-only rigs on a hypothetical no-libusb build — libusb is retained, so
  moot; noted only as the boundary the `ldd` allow-list encodes.

## Resolved risks (were open in v1)

- **Static feasibility** — empirically proven (above); the shared-`.so` fallback
  is **deleted** (unworkable under Tauri's externalBin layout, and unneeded).
- **`ldd`-proves-portability-not-capability** — closed by the `--dump-caps` +
  dummy-backend + `readelf` GLIBC-floor assertions.
- **File collision / upgrade re-break** — closed by the `tuxlink-` sidecar rename
  and the hamlib-present smoke case.
- **Sentinel migration** — closed by the literal-default-name sentinel.
