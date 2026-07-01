# Handoff — 2026-07-01 — peregrine-tamarack-sycamore

Frontend design-system epic: **froze the React `Button`/`Select`/`Field` wrapper API**
(`tuxlink-3m0vx`) and shipped it. **PR #995 merged to main** (12:56Z).

## Shipped this session

**PR #995** (`tuxlink-3m0vx`) — typed `Button`/`Select`/`Field` wrappers over a normalized
`controls.css`, adopted on the reviewed ribbon + radio-pane surfaces.

- **Model:** `tone{neutral,primary,danger} × emphasis{solid,soft,outline} × size{xs,sm,md}`.
  Color resolves via a `--ctl-accent` / `-soft` / `-fg` context-token trio (amber at `:root`,
  green inside `.radio-panel`) — `tone="primary"` is context-correct with no per-call-site branching.
- **Adoption:** 31 footer buttons + 47 config `Select`/`Field` controls across the radio panes,
  + ribbon Connect/Abort. Dead `.radio-panel-btn*` CSS removed. `src/controls/` is the new home;
  frozen API reference at `docs/design/control-wrappers.md`.
- **Normalized scale (Hybrid)** was operator-approved via a WebKitGTK current-vs-normalized mock
  (`dev/render-harness/button-compare.html`, committed). Only three visible deltas: Connect/Abort
  padding, Connect solid-fg, Open WebGUI border `currentColor→--border-strong`.

**PR #970** (earlier this session) — render-harness fidelity fix (faithful `.ribbon-with-search`
wrapper + `?running=1` fixture). Merged.

**Two real defects caught in review, both fixed:**
1. The migration orphaned the compact **44px touch-target a11y floor** (keyed to the now-dead
   `.radio-panel-btn`) — retargeted to `.radio-panel .tux-btn`, contract tests updated to guard the
   live selector (operator chose Option B).
2. The **render harness never imported `controls.css`** — so every early visual re-verify was on
   *unstyled* buttons (a blind gate). Fixed the harness import; the shipped app was always correct
   (`App.tsx` loads controls.css globally) and full vitest passed throughout.

## How it was built (process)

Brainstorming → spec (`docs/superpowers/specs/2026-06-29-react-control-wrapper-api-freeze-design.md`)
→ plan (`docs/superpowers/plans/2026-06-29-react-control-wrapper-api-freeze.md`) → **10-task
subagent-driven execution** (Sonnet implementers + per-task spec+quality reviewers) → final
whole-branch review (**READY-WITH-FOLLOWUPS**, no Critical/Important).

**Gates:** full `pnpm vitest run` after every CSS change (final: **3565 pass / 304 files** post-merge),
typecheck, build, WebKitGTK re-verify in **dark + daylight**, CI green both arches.

## Branch / worktree / tracker state

- **main:** wrapper-freeze merged (merge commit on PR #995). Also merged origin/main (79 commits,
  Elmer onboarding) *into* the branch before the final merge — resolved conflicts in
  `implementation-log.md` (both entries kept) + `harness.tsx` (both change-sets kept).
- **Worktrees:** `bd-tuxlink-3m0vx-*` and `bd-tuxlink-ppnui-*` disposed (ADR 0009) + pruned. This
  handoff was written in a throwaway `bd-tuxlink-f0ycs-session-handoff` worktree (dispose after).
  Many OTHER sessions' worktrees remain under `worktrees/` — left untouched (not mine).
- **Stashes** (`stash@{0..6}`) are pre-existing from other branches — **not mine; do not clear.**
- **bd:** `tuxlink-3m0vx` + `tuxlink-ppnui` CLOSED. `tuxlink-f0ycs` OPEN — non-blocking wrapper-freeze
  polish (tighten a Button test to `toBe`, add a Select label-path test, fix a phantom spec `size` prop,
  comment the `.radio-panel-input` co-existence, and change `.tux-btn--neutral.tux-btn--outline:hover`
  from hard-coded `--modem-accent` to `--ctl-accent`).

## Still open (the rest of the design-system epic)

The other Phase-2 follow-ups filed earlier remain OPEN and unblocked by this freeze:
`tuxlink-zzh9w` (wizard gradient/animation/radius), `tuxlink-hx0vg` (`.tux-dialog` dedup),
`tuxlink-ivzut` (sparkline gradients), `tuxlink-jk70c` (sessionlog glyphs), `tuxlink-1sukw`
(padding/margin spacing scale-out), `tuxlink-0i3om` (stylelint warn→error).

## Also this session (context, not code)

- Evaluated **impeccable.style** (design-skills plugin) at the operator's request. Ran its
  deterministic detector on `src/` — 13 findings (6 false-positive image-util noise, 6 side-tab
  severity borders, 1 Inter font) = a clean result. **Not adopted** (operator's call); fully removed.
- Node-upgrade question: system Node is Debian-packaged (20.19.2, security-patched); Trixie offers no
  Node 24. No Node/npm security filing found (open security items are Rust `rmcp`/`glib` + app-sec
  path-traversal). Deferred — impeccable ran fine on Node 20 anyway.
