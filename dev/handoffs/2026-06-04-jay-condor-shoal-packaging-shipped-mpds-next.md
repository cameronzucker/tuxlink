# Handoff — jay-condor-shoal — packaging pipeline shipped end-to-end; mpds is next

> **Date:** 2026-06-04 · **Agent:** `jay-condor-shoal` · **Machine:** pandora
>
> **Arc:** Multi-day session that started as documentation cleanup in response to David KI6ZHD x5159's complaint about the README ignoring Pat, then expanded through: README voice/em-dash/Latinate polish (5 PRs), Linux packaging pipeline ground-up build (deb/rpm/AppImage × amd64+arm64 on Tauri 2.11.2's native bundler), CI verification gate (ci.yml on amd64+arm64 matrix), and the release-please PAT wiring that finally connected the pipeline end-to-end. v0.25.0 now ships with 8 binary assets — the first tagged release on this project with installable artifacts. Final unresolved thread: duplicate Tuxlink menu entry from the dual-`.desktop` workaround, queued for next session as `tuxlink-mpds`.
>
> **Status at handoff:** All session PRs merged. Pipeline verified end-to-end on v0.25.0 (operator-confirmed install + run from the .deb on Pi). One open architectural follow-up filed (tuxlink-mpds) with operator-confirmed direction: do the proper Rust-side `app_id` setter, not Pi-side diagnosis.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Claim tuxlink-mpds: `bd update tuxlink-mpds --claim`
3. Investigation phase BEFORE touching code:
   - Read tauri-runtime-wry 2.11.2 source to determine current
     GApplication.application_id behavior. The xcay bd from 2026-06-02
     found Tauri was setting app_id to the binary name (tuxlink), not
     the bundle identifier (com.tuxlink.app). Version drift may have
     changed this; confirm with source, NOT with host probing on this Pi.
   - Survey the three override-mechanism candidates (Tauri .setup() hook
     + gtk_window.set_wmclass / force g_application_set_application_id
     before Tauri inits GApplication / submit upstream Tauri PR with
     linux.appId config field).
4. Codex consult on the chosen mechanism BEFORE implementation. This is
   architectural and load-bearing for taskbar icon matching across
   compositors. Operator instruction: "It's a research and engineering
   task for you" — autonomous execution authorized after Codex consult
   doesn't fundamentally disagree.
5. Verification matrix is REQUIRED: 2-of-4 of (labwc+wf-panel-pi, sway,
   X11+xfce4/mate-panel, GNOME). Operator-confirmed acceptance criterion.
```

---

## 1. Session arc (compressed)

1. **README correctness response.** External feedback from David KI6ZHD x5159 flagged the README's "no full native desktop Winlink application" claim while ignoring Pat (multi-platform Go client, ~7 years mature). Operator framing: not entirely intellectually honest from David, but the docs should address Pat intelligently anyway. **Shipped PR #302** with Pat acknowledgment, voice polish (active voice, Latinate diction per operator preference, no first person per `[[writing-voice-no-first-person]]`), em-dash reduction (26 → 0). Cross-posted firsthand exam: stood up Pat 1.0.0 from the Debian apt repo on the Pi, walked the config schema + transport URL pattern + mailbox layout, so the Pat characterization is grounded in firsthand inspection.

2. **Mid-rebase resolution.** Discovered the operator's main checkout was stuck mid-`git pull --rebase` from 12 hours prior (paused at the `willow-cypress-heron` duplicate handoff conflict on .beads/issues.jsonl). Codex consult caught a critical missed fact I'd glossed: an out-of-band `plover-willow-basalt` handoff commit had been made on the detached rebase HEAD during the pause. Resolved via **Path C — preserve + abort**: saved staged conflict to `dev/scratch/rebase-staged-jsonl-2026-06-03.patch`, tagged the orphan HEAD as `rebase-recovery/2026-06-03/detached-head`, then `git rebase --abort`. Zero data loss; all recovery artifacts in place.

3. **README polish follow-ups** (4 small PRs in sequence per operator notes): drop dead `pat`/`appimage` scope labels from CONTRIBUTING.md + voice/em-dash sweep across CONTRIBUTING/install/development/AGENTS (#306); drop kernel-AX.25 from Pat backend list per operator correctness note (#308); dedup OS-keyring claim from 4 mentions to 2 (#309); link build-from-source from the pre-alpha banner (#311). All merged.

4. **Linux packaging pipeline.** Operator scoping question on Flatpak effort; on inspection Tauri 2.11.2's bundler supports deb/rpm/AppImage natively. **Shipped PR #325** with the full pipeline: `pnpm tauri build --bundles deb,rpm,appimage` in CI; `softprops/action-gh-release@v2` on tag pushes; `actions/upload-artifact@v4` on PR runs. Local PoC on the Pi produced arm64 artifacts (deb 12M, rpm 12M, AppImage 104M). dpkg-deb -I confirmed libsecret-1-0 in Depends. Supersedes the stale tuxlink-cs7 (had bundled-Pat scope from before the 2026-05-30 strip).

5. **arm64 matrix expansion.** Operator pushed back on amd64-only ("we develop on a Pi because we primarily care about ARM"). **Shipped PR #330** wrapping the existing job in `strategy.matrix` for `ubuntu-latest` + `ubuntu-24.04-arm` (GitHub's free arm64 runners added 2024). Cache key includes `${{ matrix.arch }}` to prevent cross-arch target-dir pollution.

6. **CI verification gate (ci.yml).** Pivoted off operator's "4-6 parallel agents thrashing Pi CPU" framing. Codex consult caught that "amd64-only is acceptable" was undersold given VARA `#[cfg(target_arch)]` gating and RFCOMM `#[repr(C)]` Bluetooth socket constants; operator hard-rejected amd64-only for matrix-required. **Shipped PR #337** with the matrix verify workflow (`pnpm typecheck`/`pnpm vitest run`/`pnpm build`/`cargo clippy --all-targets --locked -- -D warnings`/`cargo test --locked --verbose`), separate cache namespace from release.yml, `concurrency.cancel-in-progress`, `permissions: contents: read`, `components: clippy` explicit, libsecret-1-dev correctly omitted (keyring is pure-Rust `sync-secret-service` + `crypto-rust`). The PR ALSO surfaced 15 clippy errors + 2 layers of pre-existing all-targets debt (examples missing `initial_listen`, integration tests missing `intent`) that the prior `cargo build --release`-only CI never exercised — all fixed in the same PR. CONTRIBUTING.md "Local verification" section rewritten as "Verification" reflecting the new CI-vs-local split.

7. **release-please PAT (the missing link).** End-of-session check showed v0.22.1 through v0.25.0 all had zero Release assets despite the pipeline being merged. Root-caused to GitHub's `GITHUB_TOKEN` anti-recursion policy: release-please-action's tag push couldn't trigger release.yml. **Shipped PR #345** adding `token: ${{ secrets.RELEASE_PLEASE_TOKEN }}` to release-please-action. Surfaced 5-step recipe to operator (food-poisoning-friendly format). Operator completed all 5 steps; verified end-to-end: v0.25.0 now has 8 assets (3 amd64 + 3 arm64 + 2 SHA256SUMS). Operator confirmed install + run on the Pi from the arm64 .deb.

8. **Duplicate menu entry.** Operator reported "Tuxlink twice in Hamradio menu" from the installed .deb. Traced to the intentional `tuxlink-xcay` dual-`.desktop` workaround (Tauri's runtime Wayland app_id was empirically the binary name `tuxlink`, not the identifier `com.tuxlink.app`, so the project shipped both filenames). Operator initially asked which to drop; operator then pushed back on host-specific diagnosis ("dangerous reliance on how Tuxlink is present on this specific host") and pivoted to the deferred proper fix from xcay's notes: set Wayland/X11 `app_id` from Rust at startup, collapse to one .desktop file. Filed as **tuxlink-mpds** with full scope (1-2 focused sessions, not "half a day" as I initially handwaved). Explicitly NOT for this session per operator's food-poisoning state.

---

## 2. Branch state

| Branch | State |
|---|---|
| `main` | Has all of this session's merges (PRs #302, #306, #308, #309, #311, #325, #330, #337, #345). Latest v* tag is v0.25.0 with 8 assets attached. |
| `bd-tuxlink-xygm/recover-handoffs` | **OPERATOR STATE** — currently checked out on the main checkout. Untracked: 2 plover-magnolia-salamander handoffs from other sessions today + listener-ui mockups + .beads/issues.jsonl staged. NOT TOUCHED by this session except for this handoff doc commit. |
| `task-amd-main-ui` | Settled at `3ba63bd` (the pre-rebase merge commit), 5 stashes preserved (including new `pre-recovery-2026-06-03`). NOT TOUCHED this session. |
| All bd-`*/*` branches from this session's PRs | Merged + deleted (branch lifecycle hook ensures auto-cleanup on `gh pr merge --delete-branch`). |

---

## 3. Open carry-over (bd issues from this session arc)

| Issue | Pri | What |
|---|---|---|
| **tuxlink-mpds** | P2 | **NEXT SESSION'S TARGET.** Set Wayland/X11 app_id from Rust at startup; collapse to one .desktop file. Supersedes the tuxlink-xcay workaround. Full scope in the bd issue body. |
| tuxlink-xcay | (existing, IN_PROGRESS) | Will close on tuxlink-mpds landing — it's the dual-install workaround that mpds supersedes. |

Issues closed this session:
- tuxlink-7bf4 (README Pat ack) — closed earlier in the day
- tuxlink-jl6w (docs sweep) — closed
- tuxlink-ccdk (kernel-AX.25 correction) — closed
- tuxlink-xk1u (OS-keyring dedup) — closed
- tuxlink-nrt0 (banner build-link) — closed
- tuxlink-qybc (packaging pipeline) — closed (supersedes cs7, noted)
- tuxlink-pbzf (arm64 matrix) — closed
- tuxlink-85wt (ci.yml) — closed at handoff time
- tuxlink-gp8f (release-please PAT) — closed at handoff time

---

## 4. Out-of-repo state changes

- **GitHub repo settings change** (operator-side, irreversible from agent): `RELEASE_PLEASE_TOKEN` secret added. Fine-grained PAT, repo-only access (cameronzucker/tuxlink), Contents + Pull requests read/write, 1-year expiration. Calendar a renewal in ~358 days.
- **`scripts/install-desktop-entry.sh` and `tauri.conf.json:linux.deb.files` still ship dual .desktop files.** Workaround is load-bearing for icon matching until tuxlink-mpds ships the proper fix.
- **Codex transcripts** at `dev/adversarial/2026-06-03-rebase-resolution-consult-codex.md` and `dev/adversarial/2026-06-03-ci-workflow-consult-codex.md` (gitignored; preserved on this Pi only).

---

## 5. Critical guidance for next session

1. **DO NOT diagnose tuxlink-mpds on this specific host.** Operator explicitly called the host-probing path "dangerous reliance." The fix is architectural: read tauri-runtime-wry source, decide the override mechanism, implement, verify on a matrix.

2. **Codex consult is mandatory before mpds implementation.** Operator memory `[[no-carveout-on-cross-provider-adrev]]` and the precedent from this session (the ci.yml workflow consult caught material issues I'd missed). For architectural decisions, the cross-provider check is the unique value.

3. **The verification matrix is non-negotiable.** 2-of-4 of (labwc+wf-panel-pi / sway / X11+xfce4 or mate-panel / GNOME) must demonstrably pass before merge. Operator confirmed acceptance criterion.

4. **CI is the verification gate now.** Per the merged CONTRIBUTING.md rewrite, push freely + rely on CI for tests/lint/typecheck/build. Only local UI smoke (`pnpm tauri dev`, `pnpm dev:converged`) remains required. This is a meaningful behavioral shift; the next agent should not run `cargo test`/`clippy`/`vitest`/`typecheck`/`build` locally as a pre-push ritual.

5. **`./scripts/converge-build.sh` remains canonical for `tauri dev` against origin/main.** The Codex audit of the env-lock pattern in PR #337 caught one similar pattern at `modem_commands::tests::env_lock`; if mpds's verification matrix needs `#[tokio::test]` integration tests, that env-lock pattern would benefit from the same `std::sync::Mutex → tokio::sync::Mutex` migration applied this session.

6. **Follow-up PR after mpds: drop `Upload PR artifacts` from release.yml.** Once tag pushes reliably produce Release assets (verified via v0.25.0 backfill tonight), the PR-artifact step in release.yml is dead weight in a solo + agents project. Mentioned in PR #345 body; file separately when mpds ships.

---

## 6. Session totals

- **9 PRs merged this session:** #302 (README + Pat ack), #306 (docs sweep), #308 (kernel-AX.25 correction), #309 (OS-keyring dedup), #311 (banner build-link), #325 (packaging pipeline), #330 (arm64 matrix), #337 (ci.yml verification gate), #345 (release-please PAT)
- **2 issues filed for next-session pickup:** tuxlink-gp8f (closed tonight after fix verified), tuxlink-mpds (NEXT)
- **First tagged release with binary assets:** v0.25.0 (3 amd64 + 3 arm64 + 2 SHA256SUMS = 8 assets)
- **Operator-confirmed install + run** from the arm64 .deb on the Pi
- **3 Codex cross-provider consults** this session (rebase resolution, ci.yml workflow design, all useful — none surfaced "fundamental disagreement" but two caught material issues I'd missed)
- **15 pre-existing clippy errors** surfaced by the new ci.yml gate and fixed in-PR; 2 layers of all-targets compile debt (missing struct fields in examples + integration tests) surfaced and fixed

---

## 7. Untouched state (operator owns)

- `task-amd-main-ui`: 5 stashes (`pre-recovery-2026-06-03`, `untracked-handoff-pre-rebase`, `bd-state-pre-rebase`, `bd export 2026-05-31 — pre-checkout`, `bd-jsonl-pre-main-switch`). Branch at `3ba63bd` post-rebase-abort.
- `bd-tuxlink-xygm/recover-handoffs`: operator's currently-checked-out branch. 4 untracked handoff/mockup files from other sessions today. .beads/issues.jsonl staged with this session's bd state changes (close ops + new issue).
- Multiple stale worktrees from earlier sessions (5vx, 61yg, 6qgn, 73ox, ...) — operator's to dispose at their cadence.

---

## 8. Next-session prompt (paste into a fresh session)

```
Resume tuxlink from the jay-condor-shoal 2026-06-04 packaging-shipped-mpds-next handoff.

Handoff doc: dev/handoffs/2026-06-04-jay-condor-shoal-packaging-shipped-mpds-next.md
READ IT FIRST.

State: v0.25.0 shipped with installable binaries (arm64 .deb + .rpm + .AppImage,
plus amd64 set). Full packaging pipeline verified end-to-end. Operator-reported
duplicate Tuxlink menu entry from the dual-.desktop workaround surfaced — proper
fix queued as tuxlink-mpds with operator-confirmed direction.

Your target: tuxlink-mpds. Set Wayland/X11 app_id from Rust at startup, collapse
the dual-.desktop workaround to a single file. Authorized for autonomous
execution after Codex consult per operator instruction.

CRITICAL FIRST GATE: do NOT diagnose on this specific Pi. Operator explicitly
rejected the host-probing path. The fix is architectural: read tauri-runtime-wry
2.11.2 source first, then pick the override mechanism, Codex-consult the chosen
mechanism, then implement. Verification matrix (2-of-4 compositors) is required
before merge.

Local verification rule changed in PR #337 (merged tonight): CI is the
non-GUI verification gate. Don't run cargo test/clippy/vitest/typecheck/build
locally as a pre-push ritual. Push and watch CI. Only `pnpm tauri dev` for UI
smoke remains local-required.
```

---

Agent: jay-condor-shoal
