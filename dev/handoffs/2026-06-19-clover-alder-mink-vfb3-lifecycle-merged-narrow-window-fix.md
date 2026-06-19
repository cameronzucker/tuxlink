# Handoff — clover-alder-mink — vfb3 CMS account lifecycle SHIPPED + narrow-window bug real-fixed

**Agent:** clover-alder-mink · **Date:** 2026-06-19
**Next focus (operator request):** APRS follow-up features (new bd issues filed 2026-06-19).

## TL;DR

1. **vfb3 (CMS account lifecycle) is fully merged to main** — sub-projects 0–3, two PRs (#787, #798). Create / read / update (password + recovery email) / delete / forgot-password recovery are all built and wired, dormant-and-safe until a key exists.
2. **Alpha-tester layout bug fixed (the REAL fix) in PR #822 — CI-green, AWAITING OPERATOR SMOKE before merge.** Root cause was `.layout-b { width:100vw; height:100vh }` resolving to SCREEN size (not window size) under WebKitGTK.
3. Next session: move the new **APRS** bd issues forward (operator's ask).

## What shipped this session

### vfb3 CMS account lifecycle — MERGED
- **#787 (merged, `adf03e2a`):** sub-project 0 (account-API command layer: `account_create/exists/validate_password/set_recovery_email/send_recovery/remove` + corrected `change_password`, ServiceStack envelope, global mutation lock, fail-closed parse, `UnknownOutcome` timeout reconciliation, real amateur-callsign grammar) + sub-project 1 (in-app wizard account creation, login-form pattern, mandatory recovery email).
- **#798 (merged):** sub-project 2 (forgot-password recovery in the status-bar `IdentitySwitcher` unlock form → `account_send_recovery`) + sub-project 3 (Settings → Winlink Account: `CmsRecoveryEmail` set-recovery + `CmsAccountDelete` **wired** behind a typed-confirmation gate).
- **Everything is gated on `TUXLINK_WINLINK_ACCESS_CODE`** — the open build ships no key, so all these controls render nothing / degrade. Live exercise is blocked on **tuxlink-lu7t** (obtain a Tuxlink-issued Winlink access key — operator action, downstream of the build, NOT a build gate; see memory [[feedback_no_gating_on_external_acceptance]]).
- Two design specs on main: `docs/superpowers/specs/2026-06-17-cms-account-api-command-layer-design.md` (v2.1) + `…-cms-account-wizard-creation-design.md`.

### Narrow-window control-clipping bug — REAL fix in #822 (NOT yet merged)
- **Symptom (alpha tester):** when Tuxlink is not maximized (esp. half-tiled), the titlebar window controls (min/max/close) AND the ribbon Connect/Stop are clipped off the window's right edge, unreachable.
- **Root cause (confirmed by grimming the live half-tiled window):** `.layout-b` (the shell) used `width:100vw; height:100vh`. Under **WebKitGTK these resolve to the OUTPUT (screen) size, not the window's client size.** A half-tiled window (full screen height, half screen width) rendered the shell the full screen width (~1920px) inside a ~960px window → the right half (Connect/Stop + window controls) fell off the window. Vertical looked fine only because a half-tile is full screen *height*.
- **Fix (#822, branch `bd-tuxlink-be2q/narrow-window-vw-fix`):** `width:100%; height:100%` (of `#root` = 100% of the WINDOW) + `min-width:0; max-width:100%`. **CI-green; AppShell guard suites green locally.**
- **#812 (already merged) was the WRONG first fix** — mis-diagnosed as in-shell content overflow; it shrank contents WITHIN the still-screen-wide shell (no visible effect). Its `minmax()` panes columns + search-zone shrink remain as genuine graceful-narrowing and compose cleanly with #822. The CSS regression-guard tests (tuxlink-h7q7/8rng/40u8) were updated for the minmax templates.
- **Lesson:** trust the operator's grim observation; `100vw`≠window under WebKitGTK with `decorations:false`. The first CSS attempt was shipped on reasoning without a real render and was wrong — the operator's smoke caught it.

## Branch / working-tree state

- **Current worktree:** `worktrees/bd-tuxlink-be2q-narrow-window-clip` on branch `bd-tuxlink-be2q/narrow-window-vw-fix` (#822), HEAD `538a50fb`, tree clean, pushed, up to date with origin.
- This handoff is committed on a dedicated `…/session-handoff-0619` branch off `main` (NOT on #822 — per [[feedback_no_pr_for_handoffs]]), pushed directly. Land it on main by ff when convenient so the session-start hook surfaces it.

## In-flight worktrees + their gitignored-stateful content (ADR 0009)

- `worktrees/bd-tuxlink-be2q-narrow-window-clip/` — **active**, owns #822 (bd tuxlink-be2q). Gitignored on disk: `node_modules/`, `dev/scratch/` (the diagnostic grims `grim*.png`, crops, `narrow-window-degrade-mock.{html,png}`), `.beads/embeddeddolt/`. Keep until #822 merges.
- `worktrees/bd-tuxlink-vfb3-account-mgmt-ui/` — **MERGED-DEAD** (its branch `bd-tuxlink-vfb3/account-mgmt-ui` merged via #798). **Dispose per the ADR 0009 ritual** (inventory → `cd` to main repo → `rm -rf` → `git worktree prune`). Gitignored on disk: `node_modules/`, cargo `target/`, `dev/scratch/vfb3-sp1-mock*.{html,png}`, `.beads/embeddeddolt/`. Nothing un-propagated (all tracked content merged).
- The original `bd-tuxlink-vfb3-cms-password-change` worktree was already disposed this sprint.

## Pending / next-session

### Owed before closing tuxlink-be2q
- **Operator smoke of #822** at a narrow / half-tiled window (snap Tuxlink to a screen half). Confirm the window controls + Connect/Stop now sit inside the window. Then merge #822 and close tuxlink-be2q. **Do NOT merge #822 on the agent's say-so — the first fix was wrong; the smoke is the gate.**

### Operator/external-gated (not agent build gates)
- **tuxlink-lu7t** — obtain the Tuxlink-issued Winlink access key from the CMS team; set `TUXLINK_WINLINK_ACCESS_CODE`; then operator-run live validation of the whole vfb3 lifecycle (create → validate → set-recovery → change → forgot → delete) against a throwaway callsign. `account_remove`'s privileged endpoint invocability is proven here.

### NEXT FOCUS — APRS features (operator filed these 2026-06-19)
Highest-value APRS-ready work:
- **tuxlink-9grg (P1 bug, ready):** switching APRS dock tabs shows "APRS Off/Disconnected" while still connected — frontend listening-state desync. Good first item.
- **APRS map feature batch (P2, 2026-06-19):** tuxlink-8fjx (station category filter controls), tuxlink-cn84 (animated receive/digipeat path on station hover, aprs.fi-style), tuxlink-ni5b (weather mode WX overlay, printable), tuxlink-hepq (APRS→Winlink local-area weather situation report).
- Context epic: **tuxlink-18q2** ([EPIC] full APRS rich experience). Related older APRS items: tuxlink-wiww (channel-monitor + per-SSID/category filtering), tuxlink-2phz (telemetry panel), tuxlink-rpx3 (SSTV into APRS chat).
- Also today: tuxlink-c6m7 (P1, Contacts panel click does nothing — unwired), tuxlink-1ckp (Favorites visual fidelity).
- New UI features want a **brainstorm + mockups (visual companion) before UI code** (project gate); the map-feature batch especially.

## Process notes for next agent
- Memory updated this session: [[feedback_no_gating_on_external_acceptance]] (don't gate building on an unissued key / external acceptance / live validation — build full functionality; those are downstream).
- `100vw`/`100vh` ≠ window under WebKitGTK (`decorations:false`) — use `100%`. Worth a pitfalls entry if it recurs.
- The Pi can't render the live shell headless (App gates on a Tauri backend call; Chromium isn't a WebKitGTK layout proxy). Visual verification = operator grim/smoke on the packaged/converged build, OR grim the running app (`grim out.png`) + crop with PIL.
