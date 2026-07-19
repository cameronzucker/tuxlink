# 2026-07-18 — fox-cypress-pika (day 2): CI-red triage, ribbon fixes shipped, quality-gates program tabled, README pass resumed

Continuation of the same session as
`2026-07-17-fox-cypress-pika-v0921-hotfix-routines-fidelity-demo-polish.md`.

## Shipped

- **v0.93.0 nightly** cut (operator-approved off-cadence) carrying the full
  Routines demo polish. Post-merge main CI red was triaged to TWO flakes,
  neither in the polish diff: ConsentGate mount-recovery races (fixed +
  merged, PR #1141, tuxlink-7kv0q closed) and the ui_commands real-config-path
  race (filed tuxlink-of8ee, P2, open). Reruns confirmed; the Node 20→24 CI
  bump (#1136) merged mid-day was the scheduler change that exposed the first.
- **Ribbon fixes, PR #1150 merged** (tuxlink-t698l + tuxlink-tg6ow closed):
  UTC 3-row wrap, clipped "Download all", grid margins, Connect crowding —
  fixed and visually verified at 950/1200/1440/1920 (captures
  `dev/scratch/routines-fidelity-2026-07-17/ribbon-v4-950.png`,
  `ribbon-v5-*.png`). "FT-8" chip relabeled **STATION INTEL**
  (operator-chosen wording; testids unchanged). Architecture: rigid cells +
  width-tier degradation ladder + a `.dash-cells` clipping middle so the
  Elmer chip and Connect ALWAYS render down to the 900px window minimum.
- **Pitfall found the hard way, twice-documented in commit messages
  (36c4e5fa, 604c8e8f):** `@media display:none` tiers silently lose
  equal-specificity cascade ties to later base rules; elements look hidden
  but are overflow-clipped. Double-class tier subjects. Also: the
  AppShell.compact.test.tsx literal-CSS guards fail CI on any ribbon value
  change (scoped-local-vitest trap, again).

## Tabled (durably): velocity-constrained UI quality gates

**tuxlink-w9vof** carries the complete state: architecture v2 (constraint
package → advisory admission-controlled checklist-as-code → entropy
instrument on trial → rare human calibration waves), two GPT-5.6-sol
adversarial rounds (operator override of ADR 0023; transcripts local at
`dev/adversarial/2026-07-18-inverse-adrev-idea-gpt56sol-*OPERATOR-OVERRIDE.md`),
literature grounding, and the three open operator decisions (escape
tolerance, admission bar, first blocking rule). Do NOT rebuild this from
scratch — read the issue.

The reusable wiring from that thread: **Codex CLI → OpenRouter** for
cross-vendor rounds on models the ChatGPT plan gates:
`-c model_providers.openrouter.base_url=https://openrouter.ai/api/v1
-c model_providers.openrouter.env_key=OPENROUTER_API_KEY
-c model_providers.openrouter.wire_api=responses
-c model_provider=openrouter -c model=openai/gpt-5.6-sol`, key via
`secret-tool lookup service elmer-openrouter account teacher`.

## In progress: README pass (tuxlink-d8f3l), resumed at operator direction

Branch `bd-tuxlink-d8f3l/readme-elmer-pass`, worktree
`worktrees/bd-tuxlink-d8f3l-readme-elmer-pass`, merged with main @da0cb390
(includes ribbon fixes + contacts restoration + audio/compat/popout waves).
Text, ELMER.md, fact ledger, and two Codex disposition commits are DONE and
pushed. REMAINING: task 4 (five screenshots: multiwindow hero, elmer,
routines-designer, ft8-waterfall, vara-setup + staleness pass on six
keep-if-current images; capture rules in
`docs/superpowers/plans/2026-07-17-readme-repositioning-plan.md` — privacy
pass, receive-only, pngquant, ≤500KB each) and task 6 (gates, PR, ship).
Screenshot binary: do NOT compile on the Pi (operator strong preference,
recorded in memory) — use a CI-built arm64 artifact or the next nightly.
Capture tooling that works: XTest driver + EWMH resizer in the session
scratchpad (`drive-x0.py`, `resize-ewmh.py` patterns; PID-targeted variants
proven on both machines), grim on the Pi's XWayland `:0`, GDK capture on the
R2's X11 `:1`.

## Session-wide safety notes for successors

- The Bash cwd resets to the MAIN CHECKOUT after many operations. `cd` as a
  SOLO command before any write, and `pwd`-guard destructive scripts. This
  bit five separate times today; the assert-before-write python pattern is
  what kept the main checkout unharmed.
- `pkill -f` patterns match your OWN command line. Kill by exact PID.
- A plain `cargo build` debug binary loads devUrl (whatever vite answers),
  NOT embedded assets. Provenance-check any binary you screenshot via a
  UI affordance unique to your branch.
- Other live sessions during this window: Routines/Contacts repair agent
  (contacts restoration merged), iizmk (compat tree), hq9g0 (audio), dwcqx
  (popout). Coordinate via bd; check claims before touching their surfaces.

## Open items

- tuxlink-of8ee (ui_commands config-path race, P2) — unclaimed.
- tuxlink-w9vof (quality gates, tabled) — awaits operator decisions.
- Old repo-wide stashes (7, May–June, other sessions') — not mine to clear.
- Local branch `bd-tuxlink-j1f30/fix-scheduler-spawn` (merged; branch ops on
  main checkout hook-locked) — cosmetic.
- `dev/scratch/hc-theme-mock/` — the "signal lamp" high-contrast theme
  (verified WCAG table in the HTML header); operator may want it as a real
  scheme block post-quality-wave.

## NEXT SESSION STARTS HERE (operator-directed handoff mid-resume)

The operator redirected screenshot capture to the R2 (his call, recorded in
memory: the R2 is the agent's full-freedom bench, higher resolution; the Pi
is his daily driver — do not compile or install there).

State at handoff:
1. A RELEASE build of this branch (d8f3l merged with main @da0cb390) was
   launched on the R2 (`pnpm tauri build --no-bundle`, nohup, log at
   `/tmp/fidelity/readme-build.log` on the R2) and will likely be FINISHED
   when you read this: `~/Code/tuxlink/src-tauri/target/release/tuxlink`
   (x86, embedded assets — verify provenance by checking the ribbon says
   STATION INTEL, which only exists post-#1150).
2. Capture plan (plan task 4, rules in
   docs/superpowers/plans/2026-07-17-readme-repositioning-plan.md): five
   missing shots (multiwindow hero, elmer, routines-designer, ft8-waterfall,
   vara-setup) + staleness pass on six existing images. Use the R2's REAL
   HOME (operator blessed the box; his real R2 config incl. VARA), display
   :1 (VNC 2160x1440; consider a 1920x1080 window for image weight).
   RECEIVE-ONLY: never Connect, never arm, never beacon. Routines-designer
   shot: build an UNSAVED draft via the palette (zero persistence against
   his real config). Privacy pass on every frame; pngquant; <=500KB each.
3. Capture tooling on the R2 at /tmp/fidelity/: drive2.py (PID-targeted
   XTest clicks/typing + GDK shots) and resize-ewmh.py (EWMH window
   sizing). If /tmp was cleaned, both patterns are reproducible from this
   session's scratchpad or rewritten in ~50 lines each (python-xlib is
   installed userland at ~/.local/pylibs on BOTH machines).
4. Then plan task 6: gates (lint:docs, zero em-dashes, badge row, image
   weights), PR, CI, ship, close tuxlink-d8f3l, dispose the worktree
   (archive dev/scratch/d8f3l-facts.md + adrev transcripts per plan).
