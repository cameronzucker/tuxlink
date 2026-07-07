# Design — Bundle rigctl/rigctld as a sidecar; drop the system hamlib dependency

- **bd issue:** tuxlink-a9ip3
- **Date:** 2026-07-07
- **Agent:** salamander-sage-magnolia
- **Status:** approved (operator-converged); pending adversarial review

## Problem

Tuxlink drives rig control by shelling out to hamlib's `rigctl`/`rigctld`
**binaries** — [`tux-rig/src/list.rs`](../../../src-tauri/tux-rig/src/list.rs) runs
`rigctl -l` to enumerate rig models, and
[`tux-rig/src/managed.rs`](../../../src-tauri/tux-rig/src/managed.rs) spawns
`rigctld` (default binary name `"rigctld"`, resolved off `PATH`). It does **not**
link `libhamlib`; there is no hamlib FFI crate.

Despite that version-agnostic runtime coupling, the shipped packages declare a
**hard, exact-version system dependency** on hamlib:

- Core `.deb`: `bundle.linux.deb.depends` includes `libhamlib-utils`.
- `.rpm`: `bundle.linux.rpm.depends` includes `hamlib`.

`libhamlib-utils` internally pins its matching lib at an **exact** version
(`libhamlib4t64 (= 4.5.5-3.2build2)` on Ubuntu 24). On any machine that already
carries a *different* hamlib — a source build, a PPA, or a copy pulled in by
WSJT-X/fldigi — that exact pin fails to resolve and the **entire Tuxlink install
is refused**.

**Verified failure (2026-07-07):** on Ubuntu 24 R2, which already had hamlib,
both `dpkg -i tuxlink_0.83.0_amd64.deb` and `apt install ./…` failed with
`libhamlib-utils … is not going to be installed` because `libhamlib4t64 (= …)`
could not be satisfied against the pre-existing hamlib.

Two further holes surfaced while diagnosing:

1. **ECT low-floor build has no hamlib provisioning at all.** Its CI override
   (`ect-build.yml`) replaces `deb.depends` with `["libc6 (>= 2.35)",
   "libsecret-1-0"]`, dropping `libhamlib-utils` and not bundling it. Rig control
   on today's ECT `.deb` works only if the user already has hamlib — and its
   target (Ubuntu 22.10 EOL, glibc 2.36) is frozen, so `apt install
   libhamlib-utils` may not even resolve there.
2. **AppImage** declares no package deps and relies on the host; no hamlib
   guarantee.

The pattern the ECT build *already* uses for exactly this class of problem is to
**bundle the binary as a Tauri sidecar** — `ardopcf`, `voacapl`, and `pmtiles`
are compiled/staged in CI and shipped inside the package precisely because the
frozen platform cannot be trusted to provide them.

## Decision

Treat `rigctl`/`rigctld` the same way `ardopcf` is treated: **bundle them as a
per-arch Tauri sidecar, always use the bundled copy, and drop the system hamlib
dependency entirely.**

### Why "always bundled," not "prefer system"

An earlier "prefer the operator's system hamlib, fall back to bundled" idea was
rejected: a machine may carry an **ancient, never-configured hamlib** from a
long-ago third-party install. "Prefer system" would silently adopt that untested,
possibly-broken copy and make Tuxlink's behavior depend on cruft we never
validated — a failure that *looks* like it should work. Determinism wins: every
user runs the exact `rigctld` we built and tested.

This upgrades rather than betrays `list.rs`'s "accurate to the operator's hamlib"
intent — that comment predates any bundled copy, when the system was the only
option. A recent bundled hamlib supports *more* rigs than most ancient system
ones, so the model list improves.

### Escape hatch

The rig config already carries a `rigctld_binary` field
([`RigControlSection.tsx:37`](../../../src/radio/modes/RigControlSection.tsx#L37)).
Default it to the resolved bundled sidecar path. A power user who genuinely wants
their own hamlib sets this field explicitly — opt-in, never silent.

## Changes

### 1. CI — build + stage the hamlib sidecar (both build floors)

Mirror the `ardopcf` staging step in **both** `release.yml` (core) and
`ect-build.yml` (low floor):

- Fetch a **pinned** hamlib source release, `./configure` it, `make`, and stage
  `rigctl` + `rigctld` into `src-tauri/binaries/rigctl-<triple>` and
  `rigctld-<triple>`.
- Add `binaries/rigctl` and `binaries/rigctld` to the `bundle.externalBin` list
  (core's inject step and ect's inject step both already set `externalBin`).

**Linking decision (default: static hamlib).** `rigctld`/`rigctl` dynamically
link `libhamlib.so`, so this is more involved than `ardopcf`. Default approach:
build hamlib with `--enable-static --disable-shared` and link the utils against
the static lib, producing self-contained single-file binaries (the `ardopcf`
shape). Disable optional backends that drag exotic runtime deps; CAT-over-serial
(the common path: FT-710, G90) needs no `libusb`. **CI MUST assert the bundled
binary's runtime deps are low-floor** via `ldd rigctld` — only ubiquitous libs
(`libc`, `libm`, `libpthread`, and at most `libusb-1.0`/`libudev` which exist on
every target incl. Bookworm/ECT) may appear. If static linking proves
impractical, the fallback is shipping `libhamlib.so.N` beside the binaries with
an `$ORIGIN` rpath — decided during implementation, gated on the `ldd` assertion.

### 2. Packaging — remove the system hamlib dependency

In `src-tauri/tauri.conf.json`:

- `bundle.linux.deb.depends`: **remove** `libhamlib-utils`.
- `bundle.linux.rpm.depends`: **remove** `hamlib`.
- Do **not** add either to `recommends`. hamlib is no longer a system dependency
  in any form; it ships inside the package.
- `direwolf` remains a `recommends` (unrelated; untouched).

### 3. App — default to the bundled binary, resolve its path

- **Sidecar path resolution:** mirror how the app already resolves the `ardopcf`
  sidecar path (see `src-tauri/src/modem_commands.rs` / `lib.rs` externalBin
  resolution). Provide the resolved absolute path of the bundled `rigctld`
  (and `rigctl` for `list.rs`) to `tux-rig`.
- **`managed.rs`:** default `RigConfig.binary` to the resolved bundled `rigctld`
  path instead of the literal `"rigctld"`. When `rigctld_binary` config is set
  non-empty, honor it verbatim (the override).
- **`list.rs`:** invoke the bundled `rigctl` for `rigctl -l` model enumeration,
  same override semantics.
- **`RigControlSection.tsx`:** default `rigctld_binary` to empty/sentinel meaning
  "use bundled"; the backend substitutes the resolved path. Keep the field
  editable as the override.
- **No "install hamlib" banner.** Since `rigctld` always ships, there is no
  "hamlib absent" state to warn about. A spawn failure of the *bundled* binary
  keeps its existing `RigError::Spawn` surfacing (a real bug signal, not a
  missing-package signal).

## Testing

- **Rust (via CI — Pi cannot compile Rust):**
  - Unit test that `managed.rs` uses the provided bundled path by default and the
    `rigctld_binary` override when set.
  - Unit test `list.rs` model parsing is unchanged (already covered; keep green).
- **Frontend (vitest — runs locally on the Pi):**
  - `RigControlSection.test.tsx`: default config yields the bundled-binary
    sentinel, not `"rigctld"`; an explicit override round-trips.
- **Packaging / integration (CI):**
  - `ldd` assertion on the bundled `rigctld` (low-floor deps only) in both
    `release.yml` and `ect-build.yml`.
  - Extend `ect-install-test` / `deb-install-smoke.sh`: install the `.deb` on a
    **clean container that has no hamlib**, and assert (a) install succeeds and
    (b) the bundled `rigctld` runs (`rigctld --version` or `-h`). This is the
    direct automated guard against the R2 failure and the ECT hole.
  - Verify the bundle extraction test lists `rigctl`/`rigctld` alongside
    `ardopcf`/`voacapl`/`pmtiles` (extend the existing `dpkg-deb -x` assertion).

## Rollout

- Ships via PR off `bd-tuxlink-a9ip3/hamlib-bundled-sidecar`.
- The banner/`feat` framing is gone; this is a `fix` (broken install) plus the
  sidecar `feat`. release-please cuts the next version (**0.84.0** given the
  sidecar feature) after merge; cut via `gh workflow run release-merge.yml`
  (pre-release, not promoted). Operator installs the amd64 `.deb` on R2 to test
  end-to-end.

## Out of scope

- Changing rig-control UX beyond the binary-resolution default.
- Bundling any other hamlib tool (`rotctl`, etc.) — only `rigctl`/`rigctld`.
- Windows/macOS packaging (Linux-only project).

## Key risk (for adversarial review to attack first)

Static-vs-shared bundling of `libhamlib`: whether a portable, low-floor
`rigctld` can be produced that spawns and controls a serial CAT rig identically
to the system copy, across amd64 + arm64 and the modern + Bookworm/ECT floors,
without dragging a runtime dependency the low floor lacks.
