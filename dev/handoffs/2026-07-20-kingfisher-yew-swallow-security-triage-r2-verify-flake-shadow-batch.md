# Handoff: security triage, R2 visual verification, title/flake fixes, SI-arc shadow-adrev batch

- **Agent:** kingfisher-yew-swallow
- **Date:** 2026-07-20 (evening) through 2026-07-21 UTC
- **Operator directive:** resolve the 3 outstanding quality/security issues first, then the sorrel-redwood-marsh handoff first-actions (confirm #1214, R2 visual verification, bd follow-ups h790k / mddgd / shadow-adrev batch).

## Merged to main this session

1. **PR #1217 (tuxlink-vpybd)**: brace-expansion ReDoS patch bumps (Dependabot #26/#27, both high). Lockfile-only; alerts auto-resolved on merge.
2. **PR #1218 (tuxlink-h790k)**: em-dash swept from ALL window-title chrome: the four dock popped titles + assertions, frontend SURFACE_REGISTRY parity + fixtures, the Compose window title, and the main custom titlebar separator. UI body-copy em-dash candidates recorded on the issue for an operator scope call (SI forecast strings, FT-8 footer, run-history header, VOX PTT label).
3. **PR #1220 (tuxlink-g9h4j)**: render harness fixes found while executing w68mb's acceptance note: (a) the map-popout route was dead code (mount gate matched only header-* prefixes; the route silently rendered Request Center); (b) ?w= was silently ignored (DockColumn hardcoded 400px), so every "floor width" verification including the shipping adrev's ran at 400px.
4. **PR #1221 (tuxlink-mddgd)**: direwolf lifecycle-test flake. Grounded from the attempt-1 CI log: NOT load timing; the hardcoded 5892x test ports sit in Linux's ephemeral range and collide with transient runner sockets (stub died EADDRINUSE). All five tests now use OS-allocated ports + a bounded condition-based sentinel wait. Pattern noted on siblings p0vdm/rd1rx.
5. **PR #1222 (this branch)**: ledger pairs 13-16 + this handoff.

## The 3 security/quality issues (operator's opening directive)

Three open Dependabot alerts. #26/#27 (brace-expansion, high) fixed via PR #1217. **#15 (glib, medium) needs ONE OPERATOR CLICK**: dismiss in the GitHub Security tab with reason "vulnerable code is not actually used" - VariantStrIter has zero usage; glib is pinned ^0.18 by Tauri's gtk-rs stack; tracked in tuxlink-t0adx. The agent's API dismissal was permission-classifier-blocked (correctly).

## R2 visual verification (sorrel first-action 2)

Converged build on R2 was already current (b19efaad) with the app running; drove the UI via XTEST (python-xlib shipped to /tmp/pylib on R2; xdotool absent). Captures in `dev/scratch/r2-verify-20260720/` (main checkout, dev Pi).

- **#1207 viewport scaling: PASS** at 2160x1440 (96vw x 92vh, rail capped, surplus to map).
- **#1212 SI pop-out: PASS on the exercised flows** - pop to 1400x900 (937 outer with mutter frame), plain-hyphen title, dock-back restores inline with state carried (selection, band, catalog). CAVEAT: Use/Connect was NOT clicked; the 5.5 retro round later proved it dead in the popped window (tuxlink-h9tdg below).
- **#1211 Run Artifact label: PASS** ("Export run artifact" in run history; no "bundle" in visible chrome).
- **w68mb dock header**: the live APRS pane is only reachable via the ribbon chip, which STARTS the APRS listener (shared-rig audio; left Off). The issue's actual acceptance ask is two RENDER-HARNESS routes; executing them found the harness defects (PR #1220). With the harness fixed: header-tacmap-popped at a TRUE 300px PASSES (single row, correct pin/scroll); **map-popout at a TRUE 240px FAILS - recenter/zoom overlap the Pop out chip (tuxlink-5m0qy, capture g9h4j-map-popout-240.png)**. The shipping adrev's "overlap-proof" claim was only ever verified at 400px because ?w= was ignored.
- App restored to found state (Routines list, Disconnected, APRS/SI Off). No Connect, no listener, no rig touch. R2 temp files cleaned (/tmp/pylib + uidrive.py left, tmpfs-transient).

## Shadow-adrev batch (ADR 0023 clause 5, tuxlink-46k66 + pal78)

Seven Codex rounds over the four SI-arc diffs (4x 5.6-sol via OpenRouter; 3x retro 5.5 for the PRs that had no per-PR round). Ledger pairs 13-16 in this PR. **Six real defects found, all grounded against source before filing:**

- tuxlink-qtim5 - FT-8 strip setup unsupported-sample-rate arm has no device picker (promises one "below"; old surface deleted).
- tuxlink-tteto - SI map heard/heat layers never age out on an idle ring (missing the evidenceNowMs tick pattern the ship itself added for the evidence filter).
- tuxlink-y0jk1 - the #1207 rail cap (560px) leaks into the compact FZ-M1 stacked layout (media rule resets only min-width). Consensus both models.
- tuxlink-8d2vr - mcp-testserver still emits solar source:"bundled" vs production "shipped". Consensus both models; not covered by lfrzq's recorded exclusions.
- tuxlink-h9tdg - **popped SI Use/Connect silently dead**: mount passes no onUse/onUsePeer; CustomEvent fallback cannot cross webviews. The one that matters most - the popped window's core action.
- tuxlink-5m0qy - map-popout 240px overlap (from the w68mb verification, not the model rounds).

**OpenRouter ran out of credits** during the last 5.6 round (402, si-popout): that round is recorded unusable-infrastructure in pair 16; **operator top-up needed** before any future shadow round (including the si-popout re-run).

## Housekeeping

- Closed stale tuxlink-hc9hd (PR #943 merged in June). Disposed worktrees per ADR 0009: hc9hd (nothing at risk), 6i0ie (SI audit trail archived first: `.claude/worktree-archives/bd-tuxlink-6i0ie-si-operational-usability-20260721T014825Z.tar.gz`, 75MB, 49 SDD files verified), vpybd, h790k, g9h4j, mddgd, plus a temporary verify worktree on R2's repo (bd-tuxlink-g9h4j-verify, vite :1421 stopped by exact PID).
- Adversarial transcripts (7 rounds) copied to the main checkout's `dev/adversarial/` (gitignored, local-only per policy).
- bd has no dolt remote; the issues.jsonl export rides in the operator's dirty main checkout (not touched - operator state on bd-tuxlink-ant8s).

## State for the next session

- **Worktree `worktrees/bd-tuxlink-pal78-shadow-adrev-batch`**: this branch (PR #1222). Gitignored-stateful: dev/adversarial/ transcripts (already copied to main checkout), node_modules. Dispose per ADR 0009 after #1222 merges; pal78 itself stays in_progress (standing program).
- **Operator actions pending**: (1) glib alert #15 one-click dismissal; (2) OpenRouter credit top-up; (3) taste-call on tuxlink-5m0qy's fix approach (design-sensitive surface - mxqjp history).
- **Next work**: the six filed defects above (h9tdg first - dead core action in a shipped feature), then the deeper bd ready backlog (Elmer P0s, t8c0 operator smoke).
- Remote branches not deleted at merge (repo convention): bd-tuxlink-vpybd/brace-expansion-redos, bd-tuxlink-h790k/popped-title-emdash, bd-tuxlink-g9h4j/harness-map-popout-gate, bd-tuxlink-mddgd/direwolf-flake-ports, plus the sorrel-era list in the #1214 handoff.
- Main checkout: untouched all session; still dirty on bd-tuxlink-ant8s/ardop-connect-fixes.

Agent: kingfisher-yew-swallow
