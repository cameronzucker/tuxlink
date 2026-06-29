# Handoff — 2026-06-29 — birch-oak-poplar

Frontend design-system **Phase 0 + dashboard-ribbon pilot** (tuxlink-9q6ly) executed via
subagent-driven-development and **merged to main** (PR #953, merge commit `1f451241`).

## Shipped this session (merged to main)

Six implementation commits (`f67a37e7..9b6ae779`) on `bd-tuxlink-9q6ly/frontend-design-system`,
merged via no-ff merge commit `1f451241` (ADR 0010). PR #953 was retitled from docs-only to the
bundled feature (operator chose to bundle docs + impl into one PR rather than merge docs first).

- **Task 1** — 25 scale tokens (`--space-*`, `--ctl-h-*`, `--ctl-pad-x-*`, `--type-*`, `--radius-*`)
  appended additively to the first `:root` in `src/App.css`. Zero consumers at add time.
- **Task 2** — `src/styles/controls.css`: `.tux-btn`/`.tux-btn-sm`/`.tux-btn-primary`/`.tux-field`/
  `.tux-select` on the tokens. **Deliberately unadopted** (plan-mandated reviewable foundation for the
  later React wrappers). Imported in `App.tsx` after `App.css`.
- **Task 3** — stylelint **warn mode** + `lint:css`. Config uses top-level `"defaultSeverity": "warning"`
  (a deliberate deviation from the plan's literal JSON, which would have exited non-zero — see Decisions).
  `pnpm lint:css` → exit 0, 3855 warnings / 0 errors.
- **Task 4** — `?view=ribbon` mount in `dev/render-harness/harness.tsx` (dev-only). Wraps the render root
  in `QueryClientProvider` (the tree now needs react-query — without it the harness rendered **blank for
  all views**, caught only by the real-WebKitGTK render). Imports `AppShell.css` + wraps the ribbon in
  `.layout-b` so the `.dashboard` rules apply.
- **Task 5 (the pilot)** — migrated the enumerated `.layout-b .dashboard .dash-*` rules in
  `src/shell/AppShell.css` onto the tokens (11 font-size, 4 radius, 3 padding).

## Verification

- **Real WebKitGTK before/after** (the gate Chromium/jsdom can't cover, memory `chromium-not-webkitgtk-proxy`):
  rendered `?view=ribbon` in `libwebkit2gtk-4.1` before + after Task 5. AFTER is intentionally near-identical
  with only the two sanctioned deltas; no clipping/misalignment/layout shift; Connect button still pinned right.
  (PNGs are git-ignored, were at `/tmp/ribbon-before.png` + `/tmp/ribbon-after.png`.)
- typecheck ✓ · `pnpm build` ✓ · 28 DashboardRibbon + 20 AppShell.compact tests ✓ · `lint:css` exit 0.
- Per-task spec+quality reviews (all clean, fresh subagent each) + final whole-branch review on opus =
  **Ready to merge** (0 Critical, 0 Important).

## Decisions resolved

- **Callsign 14px → `var(--type-body)` (13px)**, prominence kept via the existing `font-weight: 700` +
  accent color. The plan's stated default (weight-only), **not** a new `--type-strong` step.
- **Source segments 9px → `var(--type-micro)` (10px)** — the one deliberate +1px legibility change.
- **stylelint `defaultSeverity: warning`** — the plan's literal config (extends `stylelint-config-standard`
  + one per-rule warning) would have failed its own exit-0 requirement (standard ships ~60 error-default
  rules the CSS violates). `defaultSeverity:warning` is the faithful realization of "WARN-mode only / do not
  flip to error" and preserves signal. (An implementer first disabled 24 rules; the controller corrected it.)

## Branch / worktree / tracker state

- **main**: contains the merged work (`1f451241`). Post-merge CI on main: see "Open / next" — I merged with
  the PR's CI still pending (this repo has `allow_auto_merge:false` + no required-check branch protection, so
  `gh pr merge --auto` merged immediately rather than on-green). The merge-commit CI on main is the gate now.
- **Feature branch** `bd-tuxlink-9q6ly/frontend-design-system`: merged-dead; remote deleted on merge
  (`delete_branch_on_merge:true`).
- **This handoff** rides `agent-birch-oak-poplar/handoff` (off merged main) → its own small merge, because the
  feature branch is dead and `main` is checked out in another worktree (`bd-tuxlink-qjgx-alpha-logging`), so the
  handoff couldn't be committed directly on the feature branch or on main.
- **Worktree** `worktrees/bd-tuxlink-9q6ly-frontend-design-system`: to be **disposed** at end of session per
  ADR 0009 (work merged, branch dead). Gitignored-stateful content: `node_modules/` (build cache),
  `.superpowers/sdd/` (SDD scratch: briefs, reports, review packages, ledger). No tracked dirty files, no stash.
- **bd**: `tuxlink-9q6ly` **closed**. Follow-up `tuxlink-zj9se` (P3) filed + depends-on 9q6ly. bd state is in the
  local Dolt store only — no dolt remote is configured (`bd dolt push` printed setup help), so the tracker
  state is not pushed to a remote; this is the project's normal state.

## Open / next

1. **Confirm the post-merge CI on main is green** (run 28374221439 + the build-linux/ECT runs). I merged before
   the PR's CI finished; risk is low (frontend-only CSS/config/dev-harness, no Rust touched, locally verified),
   but verify and fix-forward if red.
2. **tuxlink-zj9se** — migrate the non-enumerated ribbon `dash-*` (connect/abort buttons, aprs/egress chips,
   seg, grid-pick-map; several off-scale radii 4/5/7/8/9px need a scale decision) **+ the radio panes**, then the
   rest of the plan's Out-of-scope list. Per the plan, do this **after the radio panes also survive WebKitGTK
   screenshot review**.
3. **React `Button`/`Select`/`Field` wrapper API** is deliberately **not frozen** — it waits until the ribbon
   AND radio panes both pass screenshot review (plan constraint). `src/styles/controls.css` is the foundation.
4. The render harness now has a reusable `?view=ribbon`; add `?view=` mounts for the radio panes the same way
   (QueryClientProvider + AppShell.css + the pane's layout wrapper) for their before/after.

## Wire-walk note

No new user flow shipped: Phase 0 is additive foundation (zero user-facing), Task 2's classes are unadopted by
design, Task 4 is dev-only, Task 5 refactors the already-reachable dashboard ribbon. The appropriate reachability
check for a CSS refactor — "does the ribbon still render correctly in the production WebKitGTK engine?" — was
satisfied by the before/after render gate.
