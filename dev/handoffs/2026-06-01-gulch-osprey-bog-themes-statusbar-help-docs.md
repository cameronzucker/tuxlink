# Handoff — gulch-osprey-bog — UI shell: themes + status-bar + help+docs

> **Date:** 2026-06-01 · **Agent:** `gulch-osprey-bog` · **Machine:** pandora
>
> **Arc:** Operator asked for four UI shell features while away on day-job
> hours. Three PRs shipped: light themes + theme designer (PR #212), status
> bar redesign (PR #213), Help menu wiring + user guide (PR #214). All
> pre-alpha-safe; no RF / live-CMS work touched. Every PR is **operator-
> browser-smoke pending** before merge.

---

## 0. Critical first action — next session

```
1. Read THIS handoff. Skim §2 (PR status) + §3 (browser-smoke checklist) — the three open PRs need eyes-on-pi-LCD validation before merge.
2. Browser-smoke each PR per §3. Order: #212 themes → #214 help+docs → #213 status bar (last because the bar is most peripheral).
3. Merge in any order once smoked. Conflicts expected only on dispatchMenuAction.ts + menuModel.ts (both #212 and #213 touch the same files); rebase the second-to-merge cleanly off main.
4. Dispose the three worktrees per ADR 0009 once their PRs land.
5. Close the five bd issues (tuxlink-c22r / tuxlink-vgth / tuxlink-qxqj / tuxlink-35g0 / tuxlink-gq74) on merge.
```

Paste-ready next-session prompt at the bottom of this doc.

---

## 1. Session arc (compressed)

1. **Session moniker:** `gulch-osprey-bog` (via the script).
2. **Backlog priming:** read the two most recent handoffs (the overnight
   briefing and bison-condor-grouse's tracks-A+B handoff) to verify the
   live worktree inventory. The bison-condor-grouse Track A position-
   restoration and Track B radio-panel-400 work are still in-flight on
   their own worktrees — untouched by this session.
3. **5 bd issues filed.** `tuxlink-c22r` (light presets), `tuxlink-vgth`
   (theme designer), `tuxlink-qxqj` (status bar redesign), `tuxlink-35g0`
   (Help menu wiring), `tuxlink-gq74` (user-guide content). Dep edges
   added: `vgth` blocks on `c22r`, `35g0` blocks on `gq74`.
4. **Track 1+2 — Themes (PR #212).** Added three new light-mode presets
   (Daylight / High contrast / Paper) and an inline Theme Designer at
   View → Color Scheme → Customize…. Custom theme tokens persist as
   localStorage JSON and apply via inline-style override that beats the
   `[data-theme=preset]` selector cleanly. 31 new tests; full vitest
   736/736 green; type-check clean.
5. **Track 3 — Status bar (PR #213).** Dropped the duplicated connection
   chip (it repeated DashboardRibbon); reframed the bar as a mailbox-
   queue indicator: `N to send · M unread                vX.Y.Z`.
   Outbox segment hides when queue is 0 (no zero-state noise). Renamed
   the menu item to "Toggle Mailbox Bar" while keeping the action id,
   CSS class, and data-testid stable. 699/699 green.
6. **Track 4+5 — Help menu + docs (PR #214).** Wired the Help menu's
   three previously-stub items: About Tuxlink (inline dialog with
   version / license / disclaimer), Documentation (inline two-pane panel
   showing bundled user-guide markdown), Report Issue (shellOpen to the
   issue tracker). Authored 10 user-guide topics under `docs/user-guide/`.
   Hand-rolled minimal markdown parser; no markdown library added.
   README updated to drop the stale "v0.2.0 features" section and add
   the User guide pointer. 730/730 green.

---

## 2. PR state

| PR | Branch | State |
|---|---|---|
| [#212](https://github.com/cameronzucker/tuxlink/pull/212) | `bd-tuxlink-c22r/themes-presets-and-designer` | Open, awaiting browser smoke. Closes tuxlink-c22r + tuxlink-vgth. |
| [#213](https://github.com/cameronzucker/tuxlink/pull/213) | `bd-tuxlink-qxqj/mailbox-bar-redesign` | Open, awaiting browser smoke. Closes tuxlink-qxqj. |
| [#214](https://github.com/cameronzucker/tuxlink/pull/214) | `bd-tuxlink-35g0/help-menu-and-docs` | Open, awaiting browser smoke. Closes tuxlink-35g0 + tuxlink-gq74. |

None of the three PRs are drafts (per `feedback_no_draft_pr_parking`).
Every PR's body has a checklist; the unchecked items are the operator's
browser-smoke gate.

**Other in-flight PRs from prior sessions are untouched by this work.**
Specifically:
- The bison-condor-grouse Track A (`bd-tuxlink-c79g/position-subsystem-restoration`) is mid-flight per its handoff; this session did not touch it.
- The bison-condor-grouse Track B (`bd-tuxlink-jmfm/radio-panel-400px-controls-relocate`) is mid-flight per its handoff; this session did not touch it.
- The HTML Forms P1/P2/P3 work surfaced in the overnight-briefing remains as-handed-off; this session did not touch it.

---

## 3. Browser-smoke checklist (operator-pending)

The three PRs ship UI work that vitest-with-jsdom can verify structurally
but cannot validate visually. The operator must run `pnpm tauri dev` once
per PR and walk the smoke list.

### PR #212 — themes + designer

```
pnpm -C worktrees/bd-tuxlink-c22r-themes-presets-and-designer install
pnpm -C worktrees/bd-tuxlink-c22r-themes-presets-and-designer tauri dev
```

- [ ] View → Color Scheme: 6 presets visible (Default dark, Daylight, High contrast (light), Paper, Night/tactical red, Grayscale) + a separator + "My custom theme" (greyed when none saved) + "Customize…".
- [ ] Pick **Daylight**: window surface goes off-white; scrollbars/native selects render in light mode (the `color-scheme: light` is the load-bearing bit).
- [ ] Pick **High contrast (light)**: pure white surfaces, near-black text — should be readable in direct sun.
- [ ] Pick **Paper**: warm beige with brown accent.
- [ ] View → Color Scheme → Customize…: designer opens. Pick base "Daylight", tweak `accent` to a green via the color picker; verify the WHOLE app re-paints live.
- [ ] Click Save; designer closes; "My custom theme" entry in View → Color Scheme now appears as the active scheme.
- [ ] Switch to another preset, then back to "My custom theme" — colors restore from localStorage.
- [ ] Cancel path: open designer, change colors, Esc — the prior scheme restores; no save happened.

### PR #213 — status / mailbox bar

```
pnpm -C worktrees/bd-tuxlink-qxqj-mailbox-bar-redesign install
pnpm -C worktrees/bd-tuxlink-qxqj-mailbox-bar-redesign tauri dev
```

- [ ] View menu shows "Toggle Mailbox Bar" (not "Toggle Status Bar").
- [ ] Bottom bar shows `N unread                vX.Y.Z` when Outbox empty.
- [ ] Queue a message in the Outbox (Compose → save draft → connect cancellation, or any path that leaves a message in `outbox`); the bar now shows `N to send · M unread          vX.Y.Z`.
- [ ] No connection-state chip or dot appears in the bar (it lives in the DashboardRibbon up top).

### PR #214 — Help menu + docs

```
pnpm -C worktrees/bd-tuxlink-35g0-help-menu-and-docs install
pnpm -C worktrees/bd-tuxlink-35g0-help-menu-and-docs tauri dev
```

- [ ] Help → About Tuxlink: dialog opens; version reads `v0.11.0` (or current); License / Source / Changelog / Issues links all render.
- [ ] Click a link (e.g. Source) — opens in the default browser, NOT in the in-app webview.
- [ ] Esc / backdrop / × close the dialog.
- [ ] Help → Documentation: panel opens on "Getting started"; topic list shows 10 entries.
- [ ] Click "Connections" → content swaps; clicking the inline "[Settings](07-settings.md)" link inside the rendered markdown swaps the active topic (not the browser).
- [ ] Help → Report Issue: a new browser tab opens the GitHub issue-new page.

---

## 4. Worktree inventory (this session's three, plus the ~14 from prior sessions)

This session added three worktrees:

| Worktree | Branch | bd issue (status) |
|---|---|---|
| `worktrees/bd-tuxlink-c22r-themes-presets-and-designer/` | `bd-tuxlink-c22r/themes-presets-and-designer` | tuxlink-c22r + tuxlink-vgth (in_progress; close on merge of #212) |
| `worktrees/bd-tuxlink-qxqj-mailbox-bar-redesign/` | `bd-tuxlink-qxqj/mailbox-bar-redesign` | tuxlink-qxqj (in_progress; close on merge of #213) |
| `worktrees/bd-tuxlink-35g0-help-menu-and-docs/` | `bd-tuxlink-35g0/help-menu-and-docs` | tuxlink-35g0 + tuxlink-gq74 (in_progress; close on merge of #214) |

**Worktree state at handoff** (each):
- `git status --short` — clean (committed + pushed).
- `git ls-files --others --exclude-standard` — only `node_modules/` (regenerable).
- `git ls-files --others --ignored --exclude-standard` — `node_modules/` and Vite build cache only.
- `git stash list` — empty.

Safe to dispose per [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md) after the
three PRs merge:

```bash
# After each PR is merged + branch deleted:
cd /home/administrator/Code/tuxlink
rm -rf worktrees/bd-tuxlink-<id>-<slug>
git worktree prune
```

**The ~14 worktrees inherited from prior sessions are NOT this session's
property** — see the prior handoff (`2026-06-01-bison-condor-grouse-…md`)
for their disposition.

---

## 5. bd state

Filed + claimed by this session:

| Issue | Title | Status | Closes on |
|---|---|---|---|
| tuxlink-c22r | Light-mode theme presets | in_progress | PR #212 merge |
| tuxlink-vgth | Inline theme designer | in_progress | PR #212 merge |
| tuxlink-qxqj | Status-bar redesign | in_progress | PR #213 merge |
| tuxlink-35g0 | Wire Help menu | in_progress | PR #214 merge |
| tuxlink-gq74 | User-facing documentation | in_progress | PR #214 merge |

Dep edges added:
- `tuxlink-vgth` → blocks-on → `tuxlink-c22r`
- `tuxlink-35g0` → blocks-on → `tuxlink-gq74`

No follow-up issues filed for the in-app docs panel — coverage gaps will
surface during browser smoke and can be filed then. One potential follow-up
to flag pre-emptively: when a custom theme has been deleted from
localStorage but `tuxlink.colorScheme` still points at `custom`,
`applyColorScheme` correctly falls back to default but **the menu still
shows "My custom theme" greyed** — this is a UX nit, not a bug; file
post-smoke if the operator wants it changed.

---

## 6. Code paths touched

- `src/shell/colorScheme.ts` — new types (`PresetScheme`, `CustomTheme`,
  `CustomThemeToken`); new functions (`loadCustomTheme`, `saveCustomTheme`,
  `clearCustomTheme`, `tokensForBase`, `isPresetScheme`); extended apply
  semantics (inline `--<token>` style for `custom`).
- `src/App.css` — three new `[data-theme=…]` blocks for the light
  presets + a `[data-theme=custom]` safety-defaults block.
- `src/shell/ThemeDesigner.tsx` (new) — inline designer panel with live
  preview, base picker, mode toggle, per-token color pickers.
- `src/shell/ThemeDesigner.css` (new).
- `src/shell/StatusBar.tsx` — rewritten for new prop shape
  (`outboxQueued: number` replaces `state` + `packet`).
- `src/shell/AboutDialog.tsx` (new) + `.css`.
- `src/shell/HelpPanel.tsx` (new) + `.css`.
- `src/shell/markdownRender.ts` (new) — minimal markdown → block parser.
- `src/shell/chrome/menuModel.ts` — extended Color Scheme submenu with
  3 light presets + Customize; renamed "Toggle Status Bar" → "Toggle
  Mailbox Bar".
- `src/shell/chrome/dispatchMenuAction.ts` — added `openThemeDesigner`,
  `openAbout`, `openHelp`, `reportIssue` handlers; added case arms for
  `menu:view:customize_theme`, `menu:help:about`, `menu:help:docs`,
  `menu:help:report_issue`.
- `src/shell/AppShell.tsx` — mounts the three new overlays
  (ThemeDesigner / AboutDialog / HelpPanel); passes `outboxQueued` to
  the new StatusBar.
- `docs/user-guide/01-…-10-troubleshooting.md` (new, 10 files).
- `README.md` — refreshed features list; added User guide section.
- Tests updated/added across colorScheme / ThemeDesigner / StatusBar /
  AboutDialog / HelpPanel / markdownRender / dispatchMenuAction /
  menuModel / AppShell.

---

## 7. Discipline notes

- **No build-robust-features pipeline** per `feedback_discipline_triage_rule`
  — every change is plumbing-class (CSS tokens, menu wiring, inline
  overlays, bundled docs). No Codex adrev rounds either: no hard-to-undo
  architectural decisions.
- **Inline overlays only** per `feedback_inline_ui_no_window_clutter` —
  no new OS-level windows.
- **Declarative voice** per `feedback_writing_voice_no_first_person`
  throughout the user-guide docs + README additions.
- **Browser smoke deferred** per `feedback_browser_smoke_before_ship` —
  this session shipped via vitest gates; operator-on-Pi smoke is the
  pre-merge gate.
- **No RF / live-CMS work** — RADIO-1 untouched; the session never
  needed transmission consent.
- **Push as you go** per `feedback_never_hold_a_push` — every commit
  pushed before moving to the next track. No held local commits at
  session end.

---

## 8. Next-session paste-ready prompt

```
Resume the UI shell sprint from gulch-osprey-bog's 2026-06-01 handoff.

Handoff doc: dev/handoffs/2026-06-01-gulch-osprey-bog-themes-statusbar-help-docs.md
READ IT FIRST — especially §0 (critical first action) and §3 (browser-smoke checklist for each open PR).

State:
- PR #212 (themes + designer), #213 (mailbox bar), #214 (help menu + docs) are all open, awaiting operator browser smoke. None are drafts. Each PR body has the unchecked operator-smoke gate.
- Five bd issues claimed in_progress: tuxlink-c22r, tuxlink-vgth, tuxlink-qxqj, tuxlink-35g0, tuxlink-gq74. Close on their PR's merge.
- Three worktrees from this session ready for ADR-0009 disposal after merge.
- bison-condor-grouse's Tracks A+B work remains in-flight on separate worktrees — NOT touched by this session; see their handoff.

Do BROWSER SMOKE FIRST. The three PRs ship inline overlays and CSS tokens that vitest cannot visually verify. Walk §3's per-PR smoke list on `pnpm tauri dev` per worktree before merging.

After merging: dispose the three worktrees per the §4 disposal recipe.
```

---

Agent: gulch-osprey-bog
