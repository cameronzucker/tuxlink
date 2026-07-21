# Handoff: post-ship defect cycle: viewport scaling, dock header, vocabulary purge, SI pop-out

- **Agent:** sorrel-redwood-marsh (same session that shipped SI via PR #1181)
- **Date:** 2026-07-21 (arc spanned 07-20 afternoon through 07-21)
- **Scope:** operator-reported defects against the shipped Station Intelligence plus a product-wide vocabulary decree and the SI pop-out capability.

## Merged to main this arc

1. **PR #1207 (tuxlink-qldzn, CLOSED)**: SI panel scales with the viewport (96vw x 92vh, rail capped 560px, surplus to the map). Root cause: viewport-independent px caps validated only at 1366x800; R2 (2160x1440) got a squashed island. Render gates now shoot both sizes. Operator merged this one directly.
2. **PR #1209 (tuxlink-mxqjp, CLOSED but SUPERSEDED, see Corrections)**: dock header two-row split.
3. **PR #1211 (tuxlink-lfrzq, CLOSED)**: "bundle" purged from product vocabulary: Routines run export renamed Run Artifact end-to-end (incl. Tauri command, exported-file kind, mcp-core forbidden-surface guard), solar provenance token bundled->shipped, agent-visible descriptions + user guide + README + SECURITY swept. Deliberate exclusions recorded on the issue (map-tile cache token, internal identifiers, CHANGELOG).
4. **PR #1212 (tuxlink-9obx2, CLOSED)**: Station Intelligence pop-out window, the 5th poppable surface (Elmer landed in parallel via tuxlink-mfssz/PR #1210, other session). Completed from an auth-interrupted partial run; the completion audit reconciled against the meanwhile-landed 5-surface registry and fixed a real out-of-bounds panic, then review rounds fixed a stale-handlers-closure dead menu action (mutation-verified regression test) and a latent app-wide handlers-memoization defeater (unmemoized report-issue controller). Config schema v9, additive migration.

## Corrections the operator made (learn from these)

- **mxqjp REJECTED as restyled jank** and superseded by tuxlink-w68mb (another session): the dock header is back to ONE row in every state; the "Pop out" entry point moved onto the map surface itself (AprsPositionsMap), popped pathway compacted to "Map ↗" + a dock-back glyph, tab group scrolls at the window floor. Lesson persisted to memory: when a row overflows, move controls to the surface they act on before adding structure; new rows/bars need operator eyes first.
- Flow-3 of the SI wire-walk: operator ruled "writing mistake, approved as-built" (map pin population IS the gateway/peer surface).

## Environment/process facts proven this arc

- R2 capture recipe (in the grim memory): ssh r2-poe, DISPLAY=:1 xwd -root, ffmpeg converts (no ImageMagick on the Pi; PIL cannot read xwd). Check provenance via /proc/PID/cwd (the app runs from .local/converge-build-worktree = origin/main).
- Vite in this repo pins port 1420 with strictPort in vite.config; `pnpm dev -- --port N` does NOT override, `pnpm exec vite --port N --strictPort` does.
- An empty `gh run list --commit` on a PR head can mean GitHub built no merge ref (CONFLICTING PR), not slow CI: check mergeable state first.
- CI flake filed: tuxlink-mddgd (managed_direwolf spawn_bind_timeout on loaded amd64 runners; rerun cleared it). Siblings: p0vdm, rd1rx.

## Open threads

- tuxlink-h790k (P3): sweep em-dash separators from the four older popped window titles (SI's new one is a plain hyphen).
- tuxlink-mddgd (P2): the direwolf CI flake needs a condition-based wait.
- The GPT-5.6 shadow-adrev follow-up for the SI ship diff (filed 07-20) is still open; none of this arc's four PRs ran a shadow round either (recipe requires OpenRouter; batch them in one session).
- R2 visual verification: the operator should eyeball, after the next converged rebuild: viewport scaling, the w68mb dock header, the SI pop-out window (pop, focus, dock-back, and the 1400x900 default), and the Run Artifact export label.
- Worktree worktrees/bd-tuxlink-6i0ie-si-operational-usability: now on the follow-up handoff branch; gitignored-stateful content per ADR 0009: .superpowers/sdd/* (full SDD + pop-out audit trail incl. si-popout-report.md), dev/scratch/{si-wirewalk-20260719.md, si-redesign/, si-containment/, si-viewport/}, dev/adversarial/2026-07-19-si-usability-codex.md, node_modules. Archive before disposal if the audit trail is wanted.
- Remote branches not deleted at merge (repo convention avoids --delete-branch): bd-tuxlink-6i0ie/si-operational-usability, agent-sorrel-redwood-marsh/si-ship-handoff, bd-qldzn/si-viewport-scale, bd-mxqjp/dock-surface-row, bd-lfrzq/no-bundle-vocabulary, bd-9obx2/si-popout-surface; prune with `git push origin --delete <branch>` at leisure.
