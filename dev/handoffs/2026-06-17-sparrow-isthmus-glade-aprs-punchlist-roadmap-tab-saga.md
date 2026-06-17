# Handoff — sparrow-isthmus-glade — APRS punch-list + roadmap + the tab-button saga

**Date:** 2026-06-17 (work spanned 2026-06-16) · **Agent:** sparrow-isthmus-glade

## One-sentence frame
Executed the link-config-persist remediation plan, then chipped a large operator
APRS/Winlink punch list end-to-end, filed the forward roadmap, and finally
root-caused + fixed the long-running "dock tabs look like default buttons" bug
(which a prior fix of mine got wrong). **Everything below is merged to `main`.**

## Branch / tree state
- **All 7 session PRs are MERGED** (CI green on each). No open PRs from this session.
- My task worktrees (`worktrees/bd-tuxlink-{hoi1,dwzu,1sro,zvif,rypw,gq0d,sdjd}-*`)
  are now **merged-dead** — dispose per the ADR 0009 ritual when convenient
  (`git worktree remove` is hook-banned; use inventory → rm -rf → prune).
- This handoff lives on `bd-tuxlink-alsl/session-handoff` (worktree
  `worktrees/bd-tuxlink-alsl-session-handoff`), bound to the next-session issue.

## Shipped this session (all merged to main)
| PR | Issue | What |
|----|-------|------|
| #746 | tuxlink-hoi1 | Link/transport config persistence — `packet_config_set` preserves the saved link on a `link_kind`-less write (B1) + emits `packet_config:change` (B5); APRS picker prop-seeding (B2); optimistic-write rollback + panel resync (B3/B4); guarded rollback vs newer-write clobber (Codex/Claude adrev). |
| #747 | tuxlink-dwzu | Map remembers + restores the prior viewport (APRS + Find-a-Station); `usePersistedViewport`; operator-centered first-run; "center on me" control. |
| #753 | tuxlink-1sro | APRS map operator "you" pin + closer local zoom (Z6→Z10). |
| #760 | tuxlink-zvif | `appearance:none` on the global `button{}` — **WRONG root cause for the tabs** (kept as harmless defensive hygiene; superseded by #779). |
| #764 | tuxlink-rypw | APRS link-setup catch-22 fixed (radio-link picker now in **Settings → APRS**, decoupled from connect) + triple-"APRS" header dedup. |
| #771 | tuxlink-gq0d | APRS tac-map perf (P1) — ported the `tuxlink-vnk7` render pattern the map was missing (two-effect `usePushData`; staleness via feature-state, not a 30s full FC rebuild). |
| #779 | tuxlink-sdjd | **The real dock-tab fix** — `border-radius:0` on `.aprs-dock-tab/.aprs-dock-maptoggle/.aprs-dock-close`. |

All the above bd issues are **closed**.

## ⚠️ The tab-button saga — read this (it's the durable lesson)
The APRS/Map/Modem dock tabs were reported as "Claude default-style buttons" **five
times**. My #760 (`appearance:none`) was a **wrong fix shipped as verified** — I
trusted an isolated hand-built CSS mock rendered in Chromium/WebKit that didn't
reproduce the real cascade.

Root cause, found by dumping `getComputedStyle()` on the tabs in the **real
production bundle** rendered through WebKit2GTK: the tabs **already** had
`appearance:none` + `border:none` + transparent/surface bg (so #760 was a visual
no-op; before/after #760 render identically). The actual cause was the global
`input,textarea,select,button { border-radius: 6px }` rounding the active tab's
fill + the Map toggle into **buttons**. Fix = `border-radius:0` (#779).

- Verification artifacts (gitignored `dev/scratch/`, local disk only):
  `webkit-tabs-before.png` (operator's pre-fix build = boxes), `webkit-tabs-after.png`
  (post-#760 = identical), `webkit-tabs-FIXED-rebuilt.png` (with #779, rebuilt
  bundle = clean rail).
- The `WEBKIT-1` implementation-pitfall was **corrected** in #779 (it had blamed
  `appearance`). New rule: when a control "looks like a default button," dump
  `getComputedStyle()` from the **real built bundle** on the real engine before
  deciding the culprit — never reason from an isolated mock.
- Lightweight real-engine check (no full app run): headless WebKit2GTK 4.1 via
  python-gi — `LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
  GALLIUM_DRIVER=llvmpipe python3 <Gtk.OffscreenWindow + WebKit2.WebView snapshot>`.

## 🔧 OPERATOR ACTION REQUIRED before judging any of this
The converged build the operator was testing was on `5b97cce0` (Merge #753) —
**older than #760/#764/#771/#779**. Much of the "still broken" confusion was
build provenance. **`pnpm dev:converged` to rebuild from current `main`** before
evaluating the tabs / Settings link picker / map perf. Then promote the stable
release (operator-only; `promote-release.yml`) once satisfied.

## Forward roadmap filed (label `roadmap-2026-06-16`)
`bd list --label roadmap-2026-06-16` → 12 issues: the full-APRS epic (tuxlink-18q2),
telemetry, second-monitor split, APRSLink-done-well, Winlink-on-map (reinstate the
lost design; see `connection-history-map-render.png`), HF-prediction truing, wider
Winlink catalog reports, APRS terminal (OH), APRSIS decompile research, PPP (OH),
and the next-session security-audit task (**tuxlink-alsl**).

## In-progress / pending decision
- **Operator validation** still owed on the merged work: real-build smoke of the
  tabs + Settings link picker + map perf; on-air UV-Pro round for the link-config
  persistence (RADIO-1, operator-only).
- **Stable release** promotion is the operator's call.

## Next session
**tuxlink-alsl** — security-audit posture of popular ham software (JS8Call, WSJT-X,
APRSIS-CE/APRSIS-32, mmSSTV, CHIRP) for the operator's **main personal laptop**.
This is research (not tuxlink code) — likely an office-hours / `deep-research`
opener. Per-app: provenance/maintenance, memory-safety, network/listening sockets,
parser attack surface (ADIF/Cabrillo/APRS/CAT/radio images), update+signing,
sandboxing, known CVEs, native-Linux-vs-Wine, and a "safe on the main laptop?"
rating + mitigations.
