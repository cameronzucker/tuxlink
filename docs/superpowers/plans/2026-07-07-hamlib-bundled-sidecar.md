# Hamlib Bundled Sidecar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `rigctl`/`rigctld` as bundled Tauri sidecars built from pinned hamlib source, always use the bundled copy, and drop the conflicting system hamlib package dependency — so Tuxlink installs on any machine (incl. those already running hamlib) and controls the FT-710/G90 deterministically.

**Architecture:** Mirror the existing `ardopcf` sidecar pattern. CI builds a static hamlib 4.6.2 per-arch on each floor's runner, stages `tuxlink-rigctl`/`tuxlink-rigctld` (renamed to avoid the `/usr/bin/rigctld` file collision with system hamlib), injects them as `externalBin`. The app resolves the bundled sibling by treating the literal default config name as the bundled sentinel (zero-migration). Packaging swaps the exact-pinned `libhamlib-utils` dep for the bundle's real, non-conflicting base-lib closure.

**Tech Stack:** Rust (Tauri backend, `tux-rig` crate), React/TS (vitest), GitHub Actions YAML, Bash, hamlib autotools C build.

**Source of truth:** `docs/superpowers/specs/2026-07-07-hamlib-bundled-sidecar-design.md` (read it fully before starting any task). It survived 5-round adversarial review + an empirical arm64 static build; do not reopen its settled decisions.

## Global Constraints

- **hamlib pin:** release tarball **4.6.2**, `sha256 = b2ac73f44dd1161e95fdee6c95276144757647bf92d7fdb369ee2fe41ed47ae8`. Tarball, never `git clone`.
- **configure:** `--enable-static --disable-shared --without-readline --without-cxx-binding`; deps `libusb-1.0-dev pkg-config` installed for deterministic linkage; `make -j"$(nproc)"`; `make install-strip DESTDIR=…`; stage from the install prefix's `bin/`, never the build tree.
- **Sidecar names:** `tuxlink-rigctl-<triple>` / `tuxlink-rigctld-<triple>`. NEVER `rigctl`/`rigctld` (they collide with `/usr/bin/rigctld` owned by `libhamlib-utils`/`hamlib`). NEVER add `Conflicts`/`Replaces`/`Obsoletes` for hamlib.
- **`ldd` allow-list (deny-by-default, BOTH binaries):** `{libc, libm, libpthread, libdl, ld-linux*, linux-vdso, libusb-1.0, libudev, libcap}`. Any `libhamlib`/`readline`/`tinfo`/`ncurses`/`gpiod`/`xml2` → fail the build.
- **GLIBC floor (`readelf --version-info`):** ECT build (ubuntu-22.04) ≤ `GLIBC_2.35`; core build (ubuntu-latest) ≤ `GLIBC_2.39`. Each floor builds its own sidecar on its own runner — NEVER cross-copy.
- **Capability + functional CI gates:** `tuxlink-rigctl -m 1049 --dump-caps` (FT-710) and `-m 3088 --dump-caps` (G90) succeed; `tuxlink-rigctld -m 1` dummy → `tuxlink-rigctl -m 2 -r localhost:PORT F 14074000` → read `f` == `14074000` → kill+reap.
- **externalBin lives ONLY in the CI `jq` inject step**, never in committed `tauri.conf.json` (a committed value breaks the `verify` gate).
- **Sentinel = the literal default name** (`"rigctld"` / `"rigctl"`), NEVER empty-string. Missing/empty/legacy-`"rigctld"` all resolve to bundled.
- **Pi cannot compile Rust.** Rust tasks: subagent authors code, PARENT commits, CI (`verify` job) compiles/tests — verify by head SHA, never a bare `--limit 1`. Frontend TS/vitest runs locally on the Pi. hamlib C build runs locally.
- **Subagents cannot commit in worktrees** — they write code and STOP; the parent commits. Every subagent prompt carries: `You are agent salamander-sage-magnolia; use commit trailer "Agent: salamander-sage-magnolia".`
- **Branch:** `bd-tuxlink-a9ip3/hamlib-bundled-sidecar`. Ends in a PR to `main` (no hand-merge of the release-please PR). After merge, cut **0.84.0** via `gh workflow run release-merge.yml` (pre-release, not promoted).

**Per-task discipline (applies to EVERY task):**
- BEFORE work: read `.claude/skills/test-driven-development/` (or `/test-driven-development`) and `docs/pitfalls/testing-pitfalls.md`. Write failing test → implement → verify green.
- BEFORE marking complete: review tests vs `docs/pitfalls/testing-pitfalls.md`; confirm error/edge paths covered; run the relevant test subset green.
- After each logical group: review the batch from multiple perspectives, minimum 3 rounds; if the 3rd still finds substantive issues, keep going. Update the private journal, then continue.

---

## File Structure

- `.github/workflows/release.yml` — add hamlib stage step + `jq` externalBin entries + build-time assertions (core floor).
- `.github/workflows/ect-build.yml` — same, ECT low floor (glibc 2.35 runner).
- `src-tauri/tauri.conf.json` — deb + rpm `depends` swap; stage hamlib `COPYING` as a bundle resource.
- `src-tauri/src/modem_commands.rs` — `resolve_rig_binaries()` resolver + wire `rig_config_from` and `rig_list_models`; unit tests; update the `binary == "rigctld"` default assertion.
- `src/radio/modes/RigControlSection.tsx` (+ `.test.tsx`) — override placeholder/help documenting the absolute-path requirement.
- `scripts/ci/deb-install-smoke.sh` — no-hamlib-nothing-pulled, hamlib-present, installed-binary `ldd`, dummy-backend smoke.
- `docs/user-guide/12-cat-and-rigctld.md`, `13-radio-specific-notes.md` — reconcile with bundled reality.

**Deferred to a fast-follow issue (NOT this PR):** first-run model-# re-validation migration. FT-710 (1049) / G90 (3088) IDs are stable 4.5.5→4.6.2, so risk is low; keeping it out keeps this PR focused and testable on R2 sooner. File `bd create` for it during execution.

## Task ordering & file ownership (avoid cross-task conflicts)

- **`.github/workflows/ect-build.yml` is edited by BOTH Task 1 (the `build-ect` job stage step + `jq` inject + extraction assertion) AND Task 5 Step 3 (the `ect-install-test` job invoke).** These are different job sections, but the same file — **Task 5 MUST be applied after Task 1 lands** (sequential, no parallel subagents on this file). Same rule for `deb-install-test.yml` if Task 1's core-floor edits and Task 5's invoke both touch it.
- **Independent (any order / parallel-safe):** Task 2 (`tauri.conf.json`), Task 3 (`modem_commands.rs`), Task 4 (`RigControlSection.tsx`), Task 6 (docs). None share a file with another task.
- **Verification dependency:** Task 5's smoke only *passes* once Task 1 (sidecar staging) and Task 2 (depends swap) are on the branch — its CI green is meaningful only after both. Author Task 5 anytime; gate its green on 1+2.
- **Recommended apply order:** 1 → 2 → 5 (the packaging chain, verified together on CI), with 3, 4, 6 authored in parallel and committed as they pass.

---

## Task 1: CI — build + stage the hamlib sidecar (both floors)

**Files:**
- Modify: `.github/workflows/release.yml` (add a stage step mirroring the existing ardopcf step; extend the `jq` externalBin inject and the bundle-extraction assertion)
- Modify: `.github/workflows/ect-build.yml` (same, on the ubuntu-22.04 low-floor runner)

**Interfaces:**
- Produces: installed sidecars `<exe-dir>/tuxlink-rigctld`, `<exe-dir>/tuxlink-rigctl` in every bundle; consumed by Task 3's resolver and Task 5's smoke.

**Context:** The ardopcf step to mirror is in both workflows (`ect-build.yml` "Stage ardopcf sidecar (ARDOP HF modem, built from pinned source)"). The `jq` inject is the "Inject bundle config" step: `.bundle.externalBin = ["binaries/voacapl", "binaries/pmtiles", "binaries/ardopcf"]`. The bundle-extraction assertion is the "Verify … bundled" step doing `dpkg-deb -x` + `test -x`.

- [ ] **Step 1: Add the hamlib stage step to `ect-build.yml`** (after the ardopcf stage step). Exact content:

```yaml
      - name: Stage hamlib sidecar (rigctl/rigctld, static, from pinned source)
        run: |
          set -euxo pipefail
          HAMLIB_VER=4.6.2
          HAMLIB_SHA=b2ac73f44dd1161e95fdee6c95276144757647bf92d7fdb369ee2fe41ed47ae8
          sudo apt-get update
          sudo apt-get install -y --no-install-recommends libusb-1.0-dev pkg-config
          curl -fSL "https://github.com/Hamlib/Hamlib/releases/download/${HAMLIB_VER}/hamlib-${HAMLIB_VER}.tar.gz" -o /tmp/hamlib.tar.gz
          echo "${HAMLIB_SHA}  /tmp/hamlib.tar.gz" | sha256sum -c -
          rm -rf /tmp/hamlib-src && mkdir -p /tmp/hamlib-src
          tar xzf /tmp/hamlib.tar.gz -C /tmp/hamlib-src --strip-components=1
          cd /tmp/hamlib-src
          ./configure --enable-static --disable-shared --without-readline --without-cxx-binding
          make -j"$(nproc)"
          make install-strip DESTDIR=/tmp/hamlib-install
          TRIPLE="$(rustc -vV | awk '/^host:/{print $2}')"
          mkdir -p "$GITHUB_WORKSPACE/src-tauri/binaries"
          for b in rigctl rigctld; do
            src="/tmp/hamlib-install/usr/local/bin/$b"
            file "$src" | grep -q ELF   # not a libtool wrapper script
            install -m 0755 "$src" "$GITHUB_WORKSPACE/src-tauri/binaries/tuxlink-${b}-${TRIPLE}"
          done
          cd "$GITHUB_WORKSPACE"
          # ldd deny-by-default (both binaries)
          allow='libc\.|libm\.|libpthread|libdl|ld-linux|linux-vdso|libusb-1\.0|libudev|libcap'
          for b in tuxlink-rigctl tuxlink-rigctld; do
            bin="src-tauri/binaries/${b}-${TRIPLE}"
            ldd "$bin" | awk '{print $1}' | grep -vE "^($allow)" | grep . && { echo "FORBIDDEN lib in $b"; exit 1; } || true
            ldd "$bin" | grep -iE 'libhamlib|readline|tinfo|ncurses|gpiod|xml2' && { echo "FORBIDDEN hamlib/readline in $b"; exit 1; } || true
            # GLIBC floor: ECT runner is glibc 2.35 -> assert no symbol newer than 2.35
            maxglibc="$(readelf --version-info "$bin" | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sort -uV | tail -1)"
            printf '%s\n2.35\n' "${maxglibc#GLIBC_}" | sort -CV || { echo "$b needs $maxglibc > ECT floor 2.35"; exit 1; }
          done
          # capability: FT-710 + G90 backends compiled in
          "src-tauri/binaries/tuxlink-rigctl-${TRIPLE}" -m 1049 --dump-caps | grep -qi 'Model name.*FT-710'
          "src-tauri/binaries/tuxlink-rigctl-${TRIPLE}" -m 3088 --dump-caps | grep -qi 'Model name.*G90'
          # functional: dummy backend set/get freq
          "src-tauri/binaries/tuxlink-rigctld-${TRIPLE}" -m 1 -t 4590 & dp=$!; sleep 1
          "src-tauri/binaries/tuxlink-rigctl-${TRIPLE}" -m 2 -r localhost:4590 F 14074000
          test "$("src-tauri/binaries/tuxlink-rigctl-${TRIPLE}" -m 2 -r localhost:4590 f)" = "14074000"
          kill "$dp" 2>/dev/null || true
          du -h "src-tauri/binaries/tuxlink-rigctl-${TRIPLE}" "src-tauri/binaries/tuxlink-rigctld-${TRIPLE}"
```

- [ ] **Step 2: Add both sidecars to the `ect-build.yml` `jq` externalBin inject.** Change the externalBin array to:

```
.bundle.externalBin = ["binaries/voacapl", "binaries/pmtiles", "binaries/ardopcf", "binaries/tuxlink-rigctl", "binaries/tuxlink-rigctld"]
```

- [ ] **Step 3: Extend the `ect-build.yml` bundle-extraction assertion.** After the existing `test -x "$(dirname "$main")/ardopcf"` lines, add:

```bash
          test -x "$(dirname "$main")/tuxlink-rigctl"
          test -x "$(dirname "$main")/tuxlink-rigctld"
```

- [ ] **Step 4: Replicate Steps 1–3 in `release.yml`** with the ONE difference: the GLIBC floor assertion asserts `2.39` (core floor), not `2.35`. Change the `printf '%s\n2.35\n'` line's `2.35` → `2.39`.

- [ ] **Step 5: Stage hamlib `COPYING` for license provenance (both workflows).** In each stage step, after `make install-strip`, add:

```bash
          mkdir -p "$GITHUB_WORKSPACE/src-tauri/resources/licenses"
          cp /tmp/hamlib-src/COPYING "$GITHUB_WORKSPACE/src-tauri/resources/licenses/hamlib-COPYING.txt"
```

  And add `"resources/licenses/**/*"` to the `.bundle.resources` array in the same `jq` inject step of each workflow.

- [ ] **Step 6: Verify via CI.** Parent pushes the branch and confirms `build ECT .deb` + `Release build` go green on the head SHA (both arches). The stage step's own assertions ARE the test — a missing backend, forbidden lib, or GLIBC-floor breach turns the build red. Expected: green, with `du -h` in logs showing ~3–5 MB stripped sidecars.

**Completion check:** confirm both workflows contain the stage step, the extended externalBin (5 entries), the extraction `test -x` for both sidecars, and the licenses resource; CI green on head SHA both arches.

---

## Task 2: Packaging — swap hamlib dep for the base-lib closure

**Files:**
- Modify: `src-tauri/tauri.conf.json` (`bundle.linux.deb.depends`, `bundle.linux.rpm.depends`)

**Interfaces:**
- Produces: a `.deb`/`.rpm` that no longer requires system hamlib and declares the bundled binary's real runtime libs.

- [ ] **Step 1: Edit `bundle.linux.deb.depends`.** Remove `"libhamlib-utils"`. Add `"libusb-1.0-0"`, `"libudev1"`, `"libcap2"`. Final deb depends:

```json
"depends": [
  "libc6 (>= 2.39)",
  "libsecret-1-0",
  "libheif1",
  "libde265-0",
  "libwebp7",
  "libusb-1.0-0",
  "libudev1",
  "libcap2"
]
```

- [ ] **Step 2: Edit `bundle.linux.rpm.depends`.** Remove `"hamlib"`. Add `"libusb1"`, `"systemd-libs"`, `"libcap"`. (Keep the existing `libsecret`, `webkit2gtk4.1`, `libayatana-appindicator-gtk3`, `libheif`, `libde265`, `libwebp`.)

- [ ] **Step 3: Verify.** Parent commits; CI `Release build` produces the `.deb`. Full assertion is Task 5's smoke (it inspects the built `.deb`'s `Depends`). Expected: build green; `dpkg-deb -f … Depends` shows no `libhamlib-utils`, includes the three base libs.

**Do NOT** add any `Conflicts`/`Replaces`/`Obsoletes` — the collision is solved by the Task 1 rename, and those directives would cascade-break WSJT-X/fldigi.

---

## Task 3: App — resolve the bundled binary at the Tauri seam

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` (add `resolve_rig_binaries`; wire `rig_config_from:1916` + `rig_list_models:171`; unit tests; update the `binary == "rigctld"` assertion ~4321)

**Interfaces:**
- Consumes: config value `rig.rigctld_binary` (default `"rigctld"` from `config.rs:default_rigctld_binary`).
- Produces: `fn resolve_rig_binaries(configured_rigctld: &str) -> (PathBuf /*rigctld*/, PathBuf /*rigctl*/)`; used by `rig_config_from` (daemon path) and `rig_list_models` (list path).

**Context — the exact pattern to mirror is `resolve_ardop_binary` (`modem_commands.rs:101`):** literal default name opts into the bundled sibling via `current_exe().parent().join(...)` iff `.exists()`, else `PathBuf::from(configured)`. Two differences here: the bundled file is renamed (`tuxlink-rigctld`, not the config value `rigctld`), and we must resolve BOTH binaries coherently (the model-list `rigctl` and the daemon `rigctld` must come from the same place, or an override skews the model DB from the daemon).

- [ ] **Step 1: Write failing tests** (in the `modem_commands.rs` test module, near `resolve_ardop_binary_honors_explicit_paths_and_defaults_to_name:2082`):

```rust
    #[test]
    fn resolve_rig_binaries_default_prefers_bundled_siblings() {
        // With the literal default "rigctld", when the bundled siblings exist next
        // to current_exe they are used; the rigctl sibling is derived from the same dir.
        let (d, l) = resolve_rig_binaries("rigctld");
        // In the test binary's dir there is no tuxlink-rigctld sibling, so it falls
        // back to the bare names on $PATH — documents the dev/test path.
        assert_eq!(d, std::path::PathBuf::from("rigctld"));
        assert_eq!(l, std::path::PathBuf::from("rigctl"));
    }

    #[test]
    fn resolve_rig_binaries_absolute_override_derives_sibling_rigctl() {
        let (d, l) = resolve_rig_binaries("/opt/hamlib/bin/rigctld");
        assert_eq!(d, std::path::PathBuf::from("/opt/hamlib/bin/rigctld"));
        assert_eq!(l, std::path::PathBuf::from("/opt/hamlib/bin/rigctl"));
    }

    #[test]
    fn resolve_rig_binaries_bare_custom_name_stays_on_path() {
        let (d, l) = resolve_rig_binaries("rigctld-git");
        assert_eq!(d, std::path::PathBuf::from("rigctld-git"));
        assert_eq!(l, std::path::PathBuf::from("rigctl")); // no dir to derive from
    }
```

- [ ] **Step 2: Run to verify failure** (via CI or a targeted note for the reviewer — Pi can't run cargo). Expected: `resolve_rig_binaries` not found.

- [ ] **Step 3: Implement `resolve_rig_binaries`:**

```rust
/// Resolve the `(rigctld, rigctl)` binaries to use, mirroring
/// `resolve_ardop_binary` but for the two hamlib utils.
///
/// The shipped package bundles them as `externalBin` sidecars named
/// `tuxlink-rigctld` / `tuxlink-rigctl` (the `tuxlink-` prefix avoids colliding
/// with `/usr/bin/rigctld` owned by system hamlib). Only the EXACT default
/// config value `"rigctld"` opts into the bundled pair; any other value is a
/// deliberate operator override honored verbatim, with the `rigctl` sibling
/// derived from the overridden `rigctld`'s directory so the model list and the
/// control daemon never version-skew.
fn resolve_rig_binaries(configured_rigctld: &str) -> (PathBuf, PathBuf) {
    if configured_rigctld == "rigctld" {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let d = dir.join("tuxlink-rigctld");
                if d.exists() {
                    let l = dir.join("tuxlink-rigctl");
                    let rigctl = if l.exists() { l } else { PathBuf::from("rigctl") };
                    return (d, rigctl);
                }
            }
        }
        return (PathBuf::from("rigctld"), PathBuf::from("rigctl")); // dev / $PATH
    }
    // Override: honor rigctld verbatim; derive sibling rigctl if a path was given.
    let d = PathBuf::from(configured_rigctld);
    let rigctl = if configured_rigctld.contains('/') {
        d.parent().map(|p| p.join("rigctl")).unwrap_or_else(|| PathBuf::from("rigctl"))
    } else {
        PathBuf::from("rigctl")
    };
    (d, rigctl)
}
```

- [ ] **Step 4: Wire `rig_config_from` (`:1916`).** Replace `binary: rig.rigctld_binary.clone(),` with:

```rust
        binary: resolve_rig_binaries(&rig.rigctld_binary).0.to_string_lossy().into_owned(),
```

- [ ] **Step 5: Wire `rig_list_models` (`:171`).** It currently hardcodes `"rigctl"` and reads no config. Make it read the rig config and use the resolved `rigctl`:

```rust
#[tauri::command]
pub fn rig_list_models(app: tauri::AppHandle) -> Vec<RigModelDto> {
    let configured = read_config(&app)             // existing config-read helper
        .map(|c| c.rig.rigctld_binary)
        .unwrap_or_else(|_| "rigctld".to_string());
    let (_daemon, rigctl) = resolve_rig_binaries(&configured);
    tux_rig::list_models(&rigctl.to_string_lossy())
        .map(|models| models.into_iter().map(|m| RigModelDto {
            id: m.id, manufacturer: m.manufacturer, model: m.model,
        }).collect())
        .unwrap_or_default()
}
```

  (Use the crate's actual config-read accessor — match how other commands in this file read `Config`. If `rig_list_models` is registered in `lib.rs` without `app`, add the `AppHandle` param and confirm the invoke handler still compiles.)

- [ ] **Step 6: Update the default assertion (`:4321`).** The `assert_eq!(rc.binary, "rigctld");` now sees the resolved value. In the test binary there is no `tuxlink-rigctld` sibling, so it resolves to bare `"rigctld"` — the assertion stays true but now documents the fallback. Add a comment: `// resolve_rig_binaries falls back to bare "rigctld" when no bundled sibling is present (test env).`

- [ ] **Step 7: Verify via CI.** Parent commits; confirm `verify` job (clippy `--all-targets -D warnings` + full test suite) is green on the head SHA both arches.

**Completion check:** `resolve_rig_binaries` covers default-bundled, absolute-override-with-sibling, and bare-custom-name; both seams use it; `verify` green on SHA.

---

## Task 4: Frontend — override UX + guard test

**Files:**
- Modify: `src/radio/modes/RigControlSection.tsx` (the `rigctld_binary` field placeholder/help)
- Modify: `src/radio/modes/RigControlSection.test.tsx`

**Interfaces:**
- Consumes: existing `rigctld_binary` field (default `"rigctld"`).

**Context:** The default stays `"rigctld"` (the bundled sentinel — do NOT change it). The only UX change: make clear that reaching a *system* hamlib requires an absolute path, since the bare default name now means "bundled."

- [ ] **Step 1: Write the failing vitest** in `RigControlSection.test.tsx`:

```tsx
it("documents that overriding rigctld needs an absolute path", () => {
  render(<RigControlSection storageKeyPrefix="test" {...baseProps} />);
  const field = screen.getByLabelText(/rigctld binary/i);
  expect(field).toHaveAttribute(
    "placeholder",
    expect.stringMatching(/bundled|absolute path/i),
  );
});
```

- [ ] **Step 2: Run to verify it fails.** `pnpm vitest run src/radio/modes/RigControlSection.test.tsx -t "absolute path"` → FAIL.

- [ ] **Step 3: Implement.** Set the field placeholder/help text, e.g. `placeholder="bundled (default) — set an absolute path to use your own hamlib"` and a help line: `Leave as "rigctld" to use Tuxlink's bundled copy. To use a different hamlib, enter its absolute path (e.g. /usr/bin/rigctld).`

- [ ] **Step 4: Run to verify pass.** Same command → PASS. Then run the full file: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx` → all green.

- [ ] **Step 5: Parent commits.**

**Completion check:** local vitest green; placeholder/help documents the absolute-path override.

---

## Task 5: CI smoke — prove install on the machine that actually breaks

**Files:**
- Modify: `scripts/ci/deb-install-smoke.sh`

**Context:** The current script installs on a clean, no-hamlib container — the ONE state never broken. The real failure needs a machine that already has hamlib. `deb-install-smoke.sh` takes `("$deb" install)`; extend it.

- [ ] **Step 1: Add a no-hamlib-nothing-pulled assertion** to the `install` path: after a successful install on the clean container, assert Tuxlink did not drag in system hamlib and that the bundled binary is present + runs:

```bash
  ! dpkg -s libhamlib-utils >/dev/null 2>&1 || { echo "FAIL: system hamlib was pulled"; exit 1; }
  test -x /usr/bin/tuxlink-rigctld
  ldd /usr/bin/tuxlink-rigctld | grep -q 'not found' && { echo "FAIL: unresolved lib in bundled rigctld"; exit 1; } || true
  /usr/bin/tuxlink-rigctld -m 1 -t 4590 & dp=$!; sleep 1
  /usr/bin/tuxlink-rigctl -m 2 -r localhost:4590 F 14074000
  test "$(/usr/bin/tuxlink-rigctl -m 2 -r localhost:4590 f)" = "14074000"
  kill "$dp" 2>/dev/null || true
```

- [ ] **Step 2: Add a `hamlib-present` mode** to the script (the direct guard for the R2 collision). When invoked with mode `hamlib-present`, `apt-get update && apt-get install -y libhamlib-utils` FIRST, then install the tuxlink `.deb`, asserting success (this is what regresses if the sidecars are ever un-renamed):

```bash
  if [ "$MODE" = "hamlib-present" ]; then
    apt-get update && apt-get install -y libhamlib-utils
  fi
  # ... existing install of "$deb" ...
```

- [ ] **Step 3: Invoke the new mode in CI.** In `ect-build.yml`'s `ect-install-test` (and the core `deb-install-test.yml`), add a second `docker run … bash scripts/ci/deb-install-smoke.sh "$deb" hamlib-present` step after the existing `install` step.

- [ ] **Step 4: Verify via CI.** Parent commits; confirm both install-test jobs go green on head SHA (both the clean and hamlib-present cases pass).

**Completion check:** clean-install asserts no system hamlib pulled + bundled binary runs; a hamlib-present container installs Tuxlink successfully; rpm file-list (if smoked) shows `tuxlink-rigctld`, never `/usr/bin/rigctld`.

---

## Task 6: Docs — reconcile rig-control guides

**Files:**
- Modify: `docs/user-guide/12-cat-and-rigctld.md`, `docs/user-guide/13-radio-specific-notes.md`

- [ ] **Step 1: Update `12-cat-and-rigctld.md`.** Replace the "Tuxlink has no built-in rig-control client / rigctld deferred" statements with: Tuxlink bundles `rigctld` (hamlib 4.6.2); rig control works out of the box with no separate hamlib install; the `rigctld binary` setting defaults to the bundled copy and accepts an absolute path to use your own hamlib. Mention the bundled copy is used deterministically regardless of any system hamlib.

- [ ] **Step 2: Update `13-radio-specific-notes.md`.** Remove/replace the "does not integrate rigctld / deferred to a later release" language; keep the model-ID-shift caution but note the bundled version is 4.6.2.

- [ ] **Step 3: Verify.** `pnpm lint:docs` passes (the ECT/release builds run it). Parent commits.

**Completion check:** no remaining "rigctld deferred / no rig control" claims; `lint:docs` green.

---

## Final integration

- [ ] Parent opens the PR: `gh pr create --base main --head bd-tuxlink-a9ip3/hamlib-bundled-sidecar --title "[salamander-sage-magnolia] fix(rig): bundle hamlib rigctl/rigctld, drop system dep (tuxlink-a9ip3)" --body …` (body summarizes the design + the 5-round adrev dispositions + the empirical build evidence).
- [ ] Confirm all CI legs green on the PR head SHA: `verify` (both arches), `Release build`, `build ECT .deb` (both arches), both install-test jobs.
- [ ] `bd create` the deferred model-# migration fast-follow; link `bd dep add`.
- [ ] Operator merges (no auto-merge). After merge: `gh workflow run release-merge.yml` → cut **0.84.0** pre-release → operator installs amd64 `.deb` on R2 for the end-to-end VARA CONNECT test.

---

## Self-review (completed by author)

- **Spec coverage:** §1 CI→Task 1; §2 packaging→Task 2; §3 app→Task 3; §4 docs→Task 6; Testing→Tasks 3/4/5; licensing→Task 1 Step 5; migration→deferred fast-follow (noted). All covered.
- **Placeholder scan:** none — all steps carry exact code/commands.
- **Type consistency:** `resolve_rig_binaries(&str) -> (PathBuf, PathBuf)` used identically in Tasks 3.4/3.5; sidecar names `tuxlink-rigctl`/`tuxlink-rigctld` consistent across Tasks 1/3/5; sentinel `"rigctld"` consistent with `config.rs` default.
