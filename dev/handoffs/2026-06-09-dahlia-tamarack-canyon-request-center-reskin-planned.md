# 2026-06-09 dahlia-tamarack-canyon — Request Center shipped (PR #513), then visual-rejected → re-skin designed + planned (tuxlink-hbbw)

## Session arc (long session, two phases)
**Phase 1 — feature build:** executed the full Request Center plan (tuxlink-eymu) Groups C–F via subagent-driven-development (per-task spec+quality reviews, 3-lens group review loops, final whole-feature audit). Shipped **PR #513 → merged to main**; resolved post-merge conflicts when main advanced to v0.40.0 (the E3 production-mount test caught a stale `useMoveUserFolder` mock the merge introduced — fixed).

**Phase 2 — visual rejection + redesign:** operator smoked the merged Request Center and rejected the look ("only vaguely resembles our approved mocks" — flat grey cards, no icons, no hierarchy, barely any accent). Root cause: the original approved mock was a browser-companion artifact **never saved to the repo**, so the implementers had no pixel reference. Re-mocked together; operator approved **Direction C ("location hero + compact catalog")** at true window size (1200×820), scaled to fill. Wrote the spec + a 7-task implementation plan. **The re-skin is NOT YET BUILT** — handed off for a fresh session to execute.

## Where the re-skin lives (resume here)
- **Branch:** `bd-tuxlink-hbbw/request-center-reskin` (off main, pushed). bd issue **tuxlink-hbbw** (in_progress, claims the worktree).
- **Worktree (deps installed, ready):** `worktrees/bd-tuxlink-eymu-request-center/worktrees/bd-tuxlink-hbbw-request-center-reskin` — note it's NESTED inside the (now-dead) eymu worktree because the helper ran from there. It's fully functional (`node_modules` installed, plan committed). **Resume the re-skin IN THIS worktree.** Everything is committed + pushed, so if you prefer a clean top-level worktree you can make one from the existing branch — but the nested one works as-is.
- **Committed on the branch:** the approved mock `docs/design/mockups/2026-06-09-request-center-redesign-final.html` (the pixel source of truth, 1200×820) + the exploration mock; the spec `docs/superpowers/specs/2026-06-09-request-center-visual-redesign-design.md`; the plan `docs/superpowers/plans/2026-06-09-request-center-visual-redesign-plan.md`.
- Mock still served at `http://pandora:8137/2026-06-09-request-center-redesign-final.html` (background `python3 -m http.server` from the eymu worktree's mockups dir; restart with that command if gone).

## NEXT SESSION — execute the re-skin (operator chose subagent-driven)
1. Read the plan (`docs/superpowers/plans/2026-06-09-request-center-visual-redesign-plan.md`) + the spec + open the mock. The plan's "Source of truth + binding rules" header is load-bearing.
2. Run `superpowers:subagent-driven-development` from Task 1 through Task 7 in the reskin worktree.
3. **Critical, easy-to-drop rules** (the whole reason the build is presentation-only):
   - **Preserve EVERY `data-testid` + aria-label.** The existing 123 `src/request/` tests + `src/shell/chrome` contract tests are the regression gate (there's no new behavior to TDD). Grep testids before editing each file; keep them.
   - Map the mock's hardcoded hexes back to the app `--tokens` (`#f59f3c`→`var(--accent)`, etc.) — don't hardcode colors.
   - WebKitGTK: no fixed heights on layout containers (GRIB map viewport is the one exception); `.content-inner` max-width ~840px so a maximized window won't stretch.
   - The **grim WebKitGTK smoke (Task 7) is the priority verification** — it's the check that actually catches the fit problem that caused this whole redesign. Do it pre-merge if `:1420` is free; else flag for post-merge.
4. Open a PR (base main); CI verify (clippy --all-targets + full vitest, both arches) is the merge gate.

## Branch / worktree / disposal state
- **main:** has the merged Request Center (PR #513) and has advanced past it (tiles/multiselect/folders/read-unread, v0.40.0).
- **Dead worktrees to dispose (ADR 0009 ritual — NOT `git worktree remove`):**
  - `worktrees/bd-tuxlink-eymu-request-center` — branch `bd-tuxlink-eymu/request-center` is **merged-dead** (PR #513). Its only untracked content was the 2 mock HTMLs, now safely committed on the reskin branch. **DO NOT dispose it while the reskin worktree is nested inside it** — dispose the reskin worktree first (after it merges), then eymu. Gitignored: node_modules, dev/adversarial, the nested reskin worktree.
- **bd:** `tuxlink-eymu` = the shipped feature (close if not already auto-closed on merge). `tuxlink-hbbw` = the re-skin, in_progress.

## Main checkout
On `bd-tuxlink-xygm/recover-handoffs` (operator state). This handoff + the earlier `2026-06-09-dahlia-tamarack-canyon-request-center-shipped-pr513.md` are untracked in `dev/handoffs/` for the operator's batch-commit.

## Out of scope / fast-follows (carried from the feature, still true)
METAR-by-station form; request draft/history persistence (basket clears on close); grid→NWS-office hazardous. Post-merge grim smoke of the ORIGINAL feature was also deferred — moot now since the re-skin replaces the visual layer and its own grim smoke (Task 7) covers it.
