# macOS App Bundle (`.app`/`.dmg`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This plan is being executed **inline, autonomously** in the authoring session (operator away).

**Goal:** Produce a macOS **`.app`** bundle (the primary deliverable) — and, **best-effort**, a `.dmg` — from `pnpm tauri build --ci --no-sign` on this Apple Silicon M5 / macOS Tahoe 26.5.1 host, with a minimal proper `bundle.macOS` config. The bundle is **arm64-only** (host build; a universal binary needs `--target universal-apple-darwin` + the x86_64 target, out of scope) and **unsigned** (`--no-sign`; notarization needs a paid Apple Developer account, out of scope). Verified **entirely headlessly** (no window launch, no Screen Recording). This closes the last unexercised item in the macOS build assessment (§V.7).

**Scope correction (review round 2):** the `.dmg` is explicitly **not** required for success — Tauri's dmg step shells to `hdiutil`+AppleScript/Finder, which can hang under remote-control/headless, so it is attempted under a hard `timeout` watchdog and its failure is a documented non-blocker. The `.app` is the success-defining artifact.

**Architecture:** Tauri 2.11.3 already emits a `.app`/`.dmg` on macOS by default (`bundle.targets: "all"`, `icon.icns` present). The work is: (1) recon the default build to learn its real behavior; (2) add a small, **macOS-target-scoped** `bundle.macOS` block (minimum system version; deliberately no entitlements for a network-client v1); (3) build + headlessly verify the artifact; (4) document and commit. All changes confined to `src-tauri/tauri.conf.json`'s `bundle.macOS` and the assessment doc — the Linux `bundle.linux` block and all Rust/TS code are untouched, so Linux CI is unaffected.

**Tech Stack:** Tauri 2.11.3 CLI, `cargo` 1.96 (aarch64-apple-darwin), pnpm 11.9, macOS `codesign`/`spctl`/`plutil` for headless bundle verification.

## Living Document Contract

This plan is a living document. Every executing agent MUST update it as
execution progresses, not only at completion.

- **On phase claim:** the executor MUST flip the banner to 🚧 IN PROGRESS
  with a claim timestamp (ISO 8601 UTC) and the active branch name. The
  banner MUST NOT include an expected-completion estimate — agents cannot
  reliably estimate their own wall-clock, and a fabricated duration
  becomes a stale anchor that misleads future readers. Followers
  encountering a 🚧 banner determine liveness by observable signals (PR
  existence, recent branch commits), not by arithmetic on expected times.
  See Step 5's stale-claim reclaim protocol.
- **On phase ship:** the executor MUST update that phase's **Execution
  Status** banner with the shipped commit SHA(s) and date. If a PR is
  open, the PR number and URL MUST appear in the top-of-plan Execution
  Status table.
- **On phase defer:** the executor MUST update the banner with ⏸ status
  AND a prose description of the unblock condition + a link to the
  likely-unblocker artifact (plan page, task, or PR whose own Execution
  Status banner will signal completion). Prose + link is durable across
  paraphrases and scope edits; exact-string coordination between agents
  is not.
- **On PR merge:** the executor MUST record the merge SHA in the banner
  + the top-of-plan Execution Status table.
- **On deviation from the written plan** (scope edits, structural
  refactors, dropped tasks, reordered phases): the executor MUST
  inline-document the deviation in the affected task AND summarize it
  in the top-of-plan Execution Status as a "Deviations" subsection.
  Deviation state MUST NOT live only in PR notes or status reports.
- **On discovery** (pre-existing drift surfaced during execution, new
  bugs found, architectural issues noted): the executor MUST add a
  "Discoveries" subsection at the top of the plan with pointers to the
  files/lines affected. Follow-up dispatches read this subsection to
  avoid duplicate discovery work.

The plan SHOULD reflect reality at the end of every session that touches
it. Anything worth putting in a status report to the user is worth
putting in the plan.

Rationale: `/writing-plans-enhanced` Step 5. Writing at ship time is
cheap; reconstruction by downstream readers is expensive, compounds
across dispatches, and fails silently when state is split across PR
notes and commit messages.

---

## Execution Status

**Overall:** ✅ Complete (2026-06-28) — `.app` built + headlessly verified; `.dmg` failed headlessly (documented non-blocker). Reviewed (R1 self · R2 Codex cross-model, 12 findings · R3 self, 2 · R4 clean), executed inline/autonomous on branch `feat/macos-build-assessment`. Results in assessment §V.10.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Recon (gps-fix compile + build) | ✅ Done | — | gps-fix compiles on macOS (2m11s, bin produced) |
| 2 — Minimal `bundle.macOS` config | ✅ Done | — | `minimumSystemVersion: 11.0`; purely additive (linux untouched) |
| 3 — Build + headless verification | ✅ Done | — | `.app` ✅ (arm64, resources OK, ad-hoc/unsigned); `.dmg` ❌ (bundle_dmg.sh exit 1) |
| 4 — Document §V + commit | ✅ Done | — | §V.10 added |

### Deviations
- **Phase order:** executed Phase 2 (add `bundle.macOS`) *before* Phase 1's full recon build, so a single `.app` build served as both recon and final — avoided two multi-minute release builds. The guarded risk (build failure) was still caught fail-fast: Phase 1 Step 1 compiled the gps-fix `beforeBundleCommand` standalone first, and the optional `minimumSystemVersion` key cannot break the build.

### Discoveries
- **`.dmg` fails headlessly:** `bundle_dmg.sh` exits 1 (not a timeout) — Tauri's dmg-layout step (hdiutil + Finder/AppleScript) is fragile in headless/remote-control contexts. `.app` is unaffected. Deferred to operator/GUI session. (§V.10)
- **Bundle identifier `com.tuxlink.app` ends in `.app`:** Tauri warns this conflicts with the macOS `.app` extension. Build still succeeds. Cross-cutting rename (tauri.conf + polkit action) → out of scope, flagged. (§V.10)
- **gps-fix `beforeBundleCommand`** compiles on macOS but bundles nothing into the `.app` (Linux deb/rpm payload) — harmless but wasteful; future cleanup could skip it on macOS. (§V.10)

---

## Prerequisites (already satisfied this session)

- `pnpm build` works (esbuild build-script approved via `pnpm approve-builds --all`; the WeatherGlyph case fix is committed). `pnpm tauri build` runs `beforeBuildCommand: "pnpm build"`, so **if this gate were not cleared the whole bundle build would fail there** — on a fresh checkout, run `pnpm install --frozen-lockfile && pnpm approve-builds --all` first.
- The macOS keyring/`apple-native` + statvfs fixes are committed, so the Rust release build links on macOS.
- Build artifacts land under `src-tauri/target/release/bundle/` (gitignored). A release build of ~760 crates (incl. `aws-lc-sys`) not yet compiled in release will take several minutes — run the build in the background and watch the log.
- **Disk (review round 2):** a release `target/` + `.app` copy + `.dmg` temp image are multi-GB. Verified 713 GB free at plan time (`df -h .`), so this is a non-issue here; on a constrained host run `df -h . /tmp` first and require >5 GB free.
- **Signing/interactivity (review round 2):** all autonomous bundle builds use `--ci` (non-interactive; `[env: CI=]`) **and** `--no-sign` (skip code signing) so ambient keychain/Developer-ID state can never trigger a GUI prompt. No `APPLE_*`/`SIGN*`/`CODESIGN*` env is set (verified); the produced bundle is therefore **unsigned**.
- **Watchdog (review round 2):** `timeout` and `gtimeout` are present (Homebrew coreutils) and are used to bound the dmg step.

## Note on "TDD" for a packaging task

There is no unit-testable code here; the analog of TDD is **verification-command-first**: each phase states the exact headless shell check and its expected output *before* the change, and the phase is not "done" until that check passes. **A piped build command masks its real exit code (`… | tee` returns tee's status), so success is judged by artifact existence + structural checks, never by a piped exit code.** No window is ever launched (operator is remote without Screen Recording). On-air/RF is irrelevant (RADIO-1 untouched).

---

## Phase 1 — Recon the default bundle build

**Execution Status:** ✅ DONE (2026-06-28) — gps-fix compiles on macOS (release, 2m11s, bin produced); the recon build was folded into Phase 3 (see Deviations).

**Why:** Tauri may already produce a working `.app` with zero config changes. Learn the real behavior (does `beforeBundleCommand`'s release build of `tuxlink-gps-fix` succeed on macOS? is the `.app` produced? is it ad-hoc-signed or unsigned?) before changing config, so Phase 2 only fixes real gaps.

**Files:** none modified (read-only recon).

- [ ] **Step 1 — Confirm the gps-fix release build compiles on macOS (the `beforeBundleCommand`).**
  Run (capture log, preserve exit — do NOT judge by a piped tail):
  ```bash
  . "$HOME/.cargo/env"
  cargo build --release --manifest-path src-tauri/Cargo.toml --bin tuxlink-gps-fix > /tmp/gpsfix.log 2>&1; echo "gpsfix-exit=$?"
  tail -5 /tmp/gpsfix.log
  ```
  Expected: `gpsfix-exit=0` and `Finished \`release\`` (the bin is plain `std`; the systemctl/apt/usermod paths are runtime `Path::exists()` lookups, not compile deps). If exit≠0, that is a real blocker → record in Discoveries and gate `beforeBundleCommand` for macOS in Phase 2.

- [ ] **Step 2 — Attempt the default `.app` bundle (no config change yet).**
  Run (background; release build of ~760 crates, several min). Build **only `app`** here — NOT `dmg` — because the dmg step shells to `hdiutil` + AppleScript/Finder and can hang or fail in a headless/remote-control session; we isolate the `.app` (the real deliverable) first. Use `--ci --no-sign` so the build is non-interactive and never attempts code signing:
  `pnpm tauri build --ci --no-sign --bundles app > /tmp/recon-bundle.log 2>&1; echo "exit=$?"`
  Judge success by `exit=0` AND the artifact check below — not by any piped status. Watch the log for terminal markers: `error[`, `could not compile`, `failed to bundle`, panic, OR `Finished` + `Bundling` + a `.app` path. This run is also the **real config-schema gate** (it loads `tauri.conf.json`).

- [ ] **Step 3 — Record the outcome.**
  Capture: did `.app` get produced? at what path (`src-tauri/target/release/bundle/macos/tuxlink.app`)? Signing state via `codesign -dv --verbose=2 <app> 2>&1`. Any errors.
  Update this phase's banner with findings; add a **Discoveries** subsection if anything unexpected (e.g. a default-config bundling error). This decides Phase 2 scope.

**Done when:** the recon log shows whether a default `.app` builds, where it lands, and its signing state — with findings recorded in the banner.

---

## Phase 2 — Add a minimal, macOS-scoped `bundle.macOS` config

**Execution Status:** ✅ DONE (2026-06-28) — `"macOS": { "minimumSystemVersion": "11.0" }` added; JSON parses; diff is purely additive (bundle.linux untouched).

**Why:** Make the bundle proper and reproducible: pin a minimum macOS version and (only if Phase 1 showed it necessary) address signing/gps-fix. Keep it minimal — **no entitlements file** (a network-client app with outbound TCP/TLS + Keychain needs none for an ad-hoc/unsigned local build; entitlements + hardened runtime only matter for notarization, which is out of scope). Do **NOT** touch `bundle.linux`.

**Files:**
- Modify: `src-tauri/tauri.conf.json` — add a `bundle.macOS` object.

- [ ] **Step 1 — Add the `bundle.macOS` block.**
  Insert into `bundle` (sibling of the existing `"linux"` key) exactly:
  ```json
  "macOS": {
    "minimumSystemVersion": "11.0"
  }
  ```
  Rationale: 11.0 (Big Sur) is the floor for Apple Silicon and a sane modern Intel floor. `category`, `icon` (incl. `icon.icns`), and `identifier` (`com.tuxlink.app`) are already set at the top level and apply to macOS. **Do NOT** add `signingIdentity`, `entitlements`, `providerShortName`, or `hardenedRuntime` — those are notarization concerns, explicitly out of scope.

- [ ] **Step 2 — Verify the config parses (JSON only here; real validation is the build).**
  Run: `python3 -c "import json; json.load(open('src-tauri/tauri.conf.json')); print('json ok')"`
  Expected: `json ok`.
  Note (review round 2): `tauri build --help` does NOT load/validate the config, so it cannot catch a wrong key (e.g. `macos` vs `macOS`). The authoritative validation is the Phase 3 build itself **plus** the Phase 3 `LSMinimumSystemVersion == "11.0"` assertion — if the `macOS` block were mis-cased and silently ignored, that assertion fails. Do not treat JSON-parse as proof the block took effect.

- [ ] **Step 3 — (Conditional) gps-fix `beforeBundleCommand`.**
  ONLY if Phase 1 Step 1 showed the gps-fix bin fails to compile on macOS: it would break every macOS bundle. In that case, document the failure in Discoveries and STOP — gating a single-string `beforeBundleCommand` per-platform is a non-trivial design decision that the operator should weigh (it is Linux-deb/rpm-only payload). Do NOT silently delete it. If Phase 1 Step 1 PASSED (expected), no action — note "gps-fix compiles on macOS; harmless unused build step" and move on.

**Done when:** `tauri.conf.json` has the macOS block, parses as JSON, and the CLI accepts it; `bundle.linux` is byte-identical to before (verify with `git diff` showing only an added `macOS` block).

---

## Phase 3 — Build the bundle and verify headlessly

**Execution Status:** ✅ DONE (2026-06-28) — `.app` built + verified (arm64, identifier/version/minOS exact, resources present, ad-hoc/unsigned, `spctl` rejects as expected). `.dmg` ❌ `bundle_dmg.sh` exit 1 (documented non-blocker).

**Why:** Produce the actual `.app` + `.dmg` and prove they are well-formed without ever launching a window.

**Files:** none modified (produces build artifacts under `src-tauri/target/release/bundle/`, which is gitignored).

- [ ] **Step 1a — Build the `.app` (PRIMARY deliverable).**
  Run (background; mostly re-bundles since cargo already compiled in Phase 1):
  `pnpm tauri build --ci --no-sign --bundles app > /tmp/bundle-app.log 2>&1; echo "app-exit=$?"`
  Expected: `app-exit=0` and a `Bundling … tuxlink.app` line. Judge by `app-exit` + the Step 2 artifact check, never a piped status. **This is the success-defining artifact.**

- [ ] **Step 1b — Attempt the `.dmg` (BEST-EFFORT, real watchdog).**
  The dmg step shells to `hdiutil` + AppleScript/Finder and can hang under remote-control/headless. Bound it with a real `timeout` (Homebrew coreutils, verified present) — it sends SIGTERM at the deadline and exits 124:
  ```bash
  timeout -k 30 360 pnpm tauri build --ci --no-sign --bundles dmg > /tmp/bundle-dmg.log 2>&1; echo "dmg-exit=$?"   # -k 30: SIGKILL 30s after SIGTERM if hdiutil/osascript ignores TERM
  # 0 -> produced; 124 -> timed out (Finder/AppleScript hang); other -> bundler error
  ```
  A non-zero `dmg-exit` (incl. 124) is a **documented non-blocker**: capture `tail -20 /tmp/bundle-dmg.log` and record "dmg not produced headlessly (Finder/AppleScript; exit <N>) — defer to operator" in §V. Do NOT block the phase on it.

- [ ] **Step 2 — Verify the artifacts exist (fail-hard on the `.app`).**
  ```bash
  set -uo pipefail
  APP=src-tauri/target/release/bundle/macos/tuxlink.app
  DMG=$(ls src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1)
  test -d "$APP" || { echo "APP MISSING — primary deliverable FAILED"; exit 1; }
  echo "APP OK: $APP"
  if [ -n "$DMG" ] && [ -f "$DMG" ]; then echo "DMG OK: $DMG"; else echo "DMG absent (acceptable; see Step 1b)"; fi
  ```
  The `exit 1` makes a missing `.app` a hard failure (not a printed-string false pass). `DMG absent` is acceptable and MUST be recorded.

- [ ] **Step 3 — Verify `.app` structure, Info.plist keys, and architecture (exact, fail-hard).**
  Use `plutil -extract <key> raw` for an exact value per key (a `grep` over `plutil -p` can pass when only one key exists — review round 2). `set -euo pipefail` aborts on any failed assertion, so there is no printed-"FAIL"-but-exit-0 false pass:
  ```bash
  set -euo pipefail
  APP=src-tauri/target/release/bundle/macos/tuxlink.app
  BIN="$APP/Contents/MacOS/tuxlink"; PL="$APP/Contents/Info.plist"
  test -f "$BIN"
  test "$(plutil -extract CFBundleIdentifier raw "$PL")" = "com.tuxlink.app"
  test "$(plutil -extract CFBundleShortVersionString raw "$PL")" = "0.78.0"
  test "$(plutil -extract LSMinimumSystemVersion raw "$PL")" = "11.0"   # proves the bundle.macOS block took effect (catches mis-casing)
  lipo -archs "$BIN"                                                     # expect: arm64 (host build; NOT universal)
  file "$BIN" | grep -q 'arm64'
  echo "STRUCT_OK"
  ```
  `arm64`-only (not universal) is expected and is a documented fact for §V.

- [ ] **Step 4 — Verify bundled resources landed (else the app is structurally present but runtime-broken — review round 2).**
  `bundle.resources` declares wle-forms, the propagation `ssn-forecast.json`, and basemap. Confirm they are under `Contents/Resources` (match by dir / known filename, not a guessed exact name):
  ```bash
  set -euo pipefail
  RES="src-tauri/target/release/bundle/macos/tuxlink.app/Contents/Resources"
  find "$RES" -type d -name wle-forms      | grep -q . && echo "wle-forms OK"
  find "$RES" -name 'ssn-forecast.json'    | grep -q . && echo "ssn-forecast OK"
  find "$RES" -type d -name basemap        | grep -q . && echo "basemap dir OK"
  find "$RES" -path '*basemap*' -type f | head -1 | grep -q . && echo "basemap has >=1 file"
  ```
  Expected: all four `OK`. Any miss = a real macOS resource-bundling gap → record in Discoveries; do not silently pass.

- [ ] **Step 5 — Classify signing state (expected: unsigned, or a linker ad-hoc sig).**
  `--no-sign` skips Tauri's Developer-ID/notarization signing — but on Apple Silicon the **linker still applies a mandatory ad-hoc signature to the arm64 Mach-O**, so the inner binary runs. The bundle may therefore read as unsigned-at-the-bundle-level or ad-hoc. Only REQUIRE `codesign --verify` to pass IF the bundle claims to be signed — don't conflate "unsigned" with "invalid signature" (review round 2):
  ```bash
  APP=src-tauri/target/release/bundle/macos/tuxlink.app
  if codesign -dv "$APP" 2>/dev/null; then
    echo "signed/ad-hoc -> verify MUST pass:"; codesign --verify --strict "$APP" && echo "SIG_VERIFY_OK"
  else
    echo "bundle UNSIGNED (expected with --no-sign; inner arm64 binary still linker-ad-hoc)"
  fi
  spctl --assess --type execute "$APP" 2>&1 || echo "spctl: rejected (EXPECTED — not Developer-ID/notarized)"
  ```
  Expected: either `SIG_VERIFY_OK` (linker ad-hoc) or `bundle UNSIGNED`, **plus** `spctl: rejected`. All are correct and out of scope (notarization needs a paid Apple Developer account). Record the exact state for §V.

- [ ] **Step 6 — Do NOT launch the app.** (No `open`, no window — operator is remote without Screen Recording.) The dev-run already proved runtime launch (§V.7); this phase proves the *bundle*, not another launch.

**Done when:** the `.app` exists with verified identifier / version / `LSMinimumSystemVersion` / arm64 binary + bundled resources; the signing state (unsigned) is recorded; and the `.dmg` is either produced or its headless failure is documented as a non-blocker.

---

## Phase 4 — Document in §V and commit

**Execution Status:** ✅ DONE (2026-06-28) — assessment §V.10 added; config + docs committed on `feat/macos-build-assessment`.

**Why:** Capture exactly what the macOS bundle build required (the doc is the deliverable per the operator's standing instruction), and land the config + doc changes per project commit discipline.

**Files:**
- Modify: `docs/design/2026-06-28-macos-ios-portability-assessment.md` — extend §V (add the bundle result; move "release bundle" out of "still not exercised").
- Modify: `src-tauri/tauri.conf.json` (committed here if not already in Phase 2's own commit).

- [ ] **Step 1 — Update §V.** Add a subsection (e.g. §V.10 "Release bundle (`.app`/`.dmg`)") stating: the exact commands (`pnpm tauri build --ci --no-sign --bundles app`, and the best-effort `timeout 360 … --bundles dmg`), the `bundle.macOS.minimumSystemVersion` addition, the artifact path(s), the verified Info.plist keys + verified bundled resources, the binary architecture (**arm64-only**, not universal), the **signing state** (**unsigned** via `--no-sign`; `spctl` rejects — notarization needs a paid Apple Developer account, out of scope), the actual **dmg outcome** (produced, or headless failure deferred to operator), and that the gps-fix `beforeBundleCommand` compiles-but-is-unused on macOS. Flip the §V.7 "release bundle" bullet from "still not exercised" to verified.

- [ ] **Step 2 — Commit** (conventional commit + moniker; `core.hooksPath` unset so enforcement is manual — include the trailer anyway):
  ```bash
  git add src-tauri/tauri.conf.json docs/design/2026-06-28-macos-ios-portability-assessment.md docs/plans/2026-06-28-macos-app-bundle-plan.md
  git commit -F - <<'EOF'
  build(bundle): add macOS bundle config + verify .app/.dmg packaging

  <body: what was needed, signing state, headless verification>

  Agent: willow-marten-fjord
  Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
  EOF
  ```
  (Two logical commits are fine: one `build(bundle)` for the config, one `docs(design)` for §V. Do NOT `git commit --amend` or `rebase -i` — both are hook-banned.)

- [ ] **Step 3 — Confirm Linux is unaffected (the change is purely additive).**
  Diff the **working-tree** change against committed state (no hardcoded SHA — review round 2). Run this BEFORE staging the config:
  ```bash
  git diff -- src-tauri/tauri.conf.json > /tmp/conf.diff
  # A pure addition (only the macOS block) means bundle.linux + all existing keys are byte-identical.
  if grep -E '^-[^-]' /tmp/conf.diff >/dev/null; then echo "WARN: removed/changed lines present — inspect before commit"; grep -E '^-[^-]' /tmp/conf.diff; else echo "pure-addition: bundle.linux + all existing keys untouched"; fi
  ```
  Expected: `pure-addition…`. Any `-` line (other than the diff `---` header) means a pre-existing key was altered — inspect and revert that part; the only intended change is an added `macOS` block. This protects the primary platform / Linux CI. (If run after staging, use `git diff --cached -- src-tauri/tauri.conf.json`.)

**Done when:** §V records the bundle result, the config + doc are committed with proper trailers, and the Linux bundle config is confirmed unchanged.

---

## Boundaries (do NOT)

- Do NOT add notarization, `signingIdentity`, hardened-runtime, or an entitlements file — out of scope; an **unsigned** (`--no-sign`) local bundle is the goal.
- Do NOT build `--target universal-apple-darwin` (would need the x86_64 target + a double build) — arm64-only is acceptable and documented.
- Do NOT launch the bundled app (no Screen Recording; runtime already proven in §V.7).
- Do NOT modify `bundle.linux`, Rust code, or TS code — keep the change to `bundle.macOS` + docs so Linux CI is untouched.
- Do NOT delete or rewrite the `beforeBundleCommand` unless Phase 1 proves it breaks the macOS bundle (then STOP and flag for the operator).
- Do NOT push or open a PR — the operator will decide that (per the prior turn's open question).
