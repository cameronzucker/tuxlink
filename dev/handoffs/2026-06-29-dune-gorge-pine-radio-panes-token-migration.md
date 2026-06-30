# Handoff — 2026-06-29 — dune-gorge-pine

Frontend design-system **Phase 2 — radio panes + non-enumerated ribbon token migration**
(`tuxlink-zj9se`, follow-up to closed `tuxlink-9q6ly`). Shipped as **PR #968**.

## Shipped this session (PR #968, awaiting CI→merge at handoff time)

Branch `bd-tuxlink-zj9se/design-system-radio-panes` (off `origin/main`, rebased current),
in worktree `worktrees/bd-tuxlink-zj9se-design-system-radio-panes`. 9 implementation
commits + this handoff:

1. **Harness** — `?view=radio-ardop|radio-vara|radio-telnet` mounts in `dev/render-harness/harness.tsx`,
   wrapped in `.layout-b > .panes--with-dock`. Extended the Tauri IPC shim with the panes'
   mount-time reads (`modem_get_status` STOPPED default, `platform_info`, `vara_status`,
   `config_get_*`, `favorites_read` StationsFile shape, `plugin:event|listen` no-op,
   `*_allowed_stations_get` allow-all, a representative `ArdopFullConfig`). Also enriched the
   ribbon fixture (seg/APRS/Elmer/egress props + `?connecting=1`) so all non-enumerated
   controls render.
2. **Non-enumerated ribbon `dash-*`** (`src/shell/AppShell.css` + `src/elmer/ElmerPane.css`):
   Connect/Abort, APRS control + unread badge, Elmer×Agent-send chip, egress chip + arm-popover
   family, Review|Download seg, grid-pick-map → tokens.
3. **RadioPanel.css chrome** + **flex:1 kill** (`.radio-panel-btn` only; correct fills left).
4. **ARDOP/VARA mode panels**, 5. **radio-pane sections**, 6. **InboundSelectionPanel** → tokens.

## Scale decision (the bd-flagged "off-scale radii" call) — RESOLVED

**Snap onto the existing scale, NO new token.** controls `4/5/7px → --radius-control (3px)`,
surfaces/popovers `8px → --radius-panel (6px)`, count badges `9/12px → --radius-pill`. Rationale:
radio panes already de-facto used 3/6/12; pilot set 3px for ribbon controls; snapping unifies all
three surfaces with zero new tokens. The most visible change (30px Elmer/egress chips 7→3px) was
render-checked — **not boxy**, no 6px fallback needed. **This sets precedent for the rest of the
design system.** Trivially reversible CSS if the operator prefers a different radius treatment after
a screenshot-review pass.

## Verification

- **Real WebKitGTK before/after** (`libwebkit2gtk-4.1`, the gate Chromium can't cover) for ribbon
  (idle + connecting) + all 3 radio panes. Radii unified, no boxiness, no layout shift; footer
  buttons content-sized with primary green emphasis (flex:1 tell gone). PNGs were at `/tmp/zj9se-*.png`
  (git-ignored).
- `pnpm typecheck` ✓ · `pnpm build` ✓ · vitest shell (34) + radio (516) ✓.
- **Codex adversarial review** (1 round, proportionate — additive/revertible CSS, not a fragile
  rewrite): "CSS token mappings consistent with the stated scale and the intended raw icon/display
  leaves remain intact." One P2 (empty ARDOP harness config printed `undefined`) — fixed. Transcript
  at `dev/adversarial/2026-06-29-radio-panes-token-migration-codex.md` (git-ignored, local only).

## Deliberately NOT done (out of scope per design doc)

- Left raw px: icon glyphs (`.diag-icon` 16px, `.diag-dismiss` 14px, inbound `.close` 20px) +
  `.qv` 32px display stat.
- compactShell.css responsive overrides: connect/abort only set `min-height:44px` (no font/radius),
  so nothing to migrate there for this task; broader compactShell tokenization is a noted follow-up
  (needs narrow-viewport renders).
- **React `Button`/`Select`/`Field` wrappers NOT frozen** — the plan's gate: they wait until BOTH
  ribbon and radio panes pass screenshot review. This PR completes the radio-pane half. **The wrapper
  freeze is now the next eligible task once the operator screenshot-reviews both surfaces.**
- Follow-ups (separate bd issues, AFTER screenshot review): wizard gradient/animation/radius,
  `.tux-dialog` dedup, Sparkline gradients, SessionLog glyphs, padding/margin scale-out, flipping
  stylelint to error.

## Branch / worktree / tracker state

- **PR #968** open, base `main`. CI (verify + build-linux, both arches) was pending at handoff;
  repo has no required checks / no auto-merge (memory `repo-no-auto-merge`) → **watch CI, then
  `gh pr merge 968 --merge`** (no-ff per ADR 0010; avoid `--auto` and `--delete-branch`).
- **Worktree** `worktrees/bd-tuxlink-zj9se-design-system-radio-panes` — dispose per ADR 0009 ritual
  after merge. Gitignored-stateful on disk: `node_modules/`, `dist/` (my `pnpm build`),
  `dev/adversarial/` (Codex transcript). No tracked-dirty, no untracked source.
- **Repo-global stashes** (`stash@{0..6}`) are pre-existing from OTHER branches/sessions
  (task-amd-main-ui, bd-tuxlink-fl6e, main) — **not mine; do not clear.**
- **RELEASE_FREEZE is active** on main ("freeze automated releases during AI + UI refactor",
  PR #967) — governs the release PR only; feature PRs merge as normal.
- **bd** `tuxlink-zj9se` in_progress, notes updated; close after merge.

## Pending operator decision

- Screenshot-review of the radius treatment (3px control radius across ribbon + panes) and the
  flex:1-kill button layout. Reversible if a different treatment is wanted. This review unblocks
  the React wrapper-API freeze.
