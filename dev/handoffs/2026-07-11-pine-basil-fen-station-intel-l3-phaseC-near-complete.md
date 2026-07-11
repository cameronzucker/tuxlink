# Handoff — 2026-07-11 (pine-basil-fen): Station Intelligence L3 — Phases A+B done+CI-green, Phase C ~complete (C7 in-flight), Phase D pending

Executed the L3 plan (`docs/superpowers/plans/2026-07-11-station-intel-l3-panel.md`)
via subagent-driven-development: one implementer subagent per task, a two-stage
task review after each (spec + quality), parent commits every task (subagents
CODE + STOP in the worktree). **Phases A and B are complete and CI-green on both
arches. Phase C is all-but-one component done; C7 is mid-finalization. Phase D
(integration + exit gates) has not started.**

## Branch / worktree / PR

- Worktree: `worktrees/bd-tuxlink-b026z.4-station-intel-l3-panel/` — KEEP (next
  session executes here; node_modules installed). Branch
  `bd-tuxlink-b026z.4/station-intel-l3-panel`.
- **HEAD `ed1213de`, PUSHED** (origin up to date at ed1213de). Draft **PR #1076**.
- **CI is running on `ed1213de`** (pushed just now). **FIRST NEXT ACTION after
  reading: verify CI green on both arches by SHA** — see "CI" below.
- Base: `origin/main` at `f0431195` (L2-shipped). bd `tuxlink-b026z.4` = IN_PROGRESS.

## Git mechanics (LEARNED THE HARD WAY — do not skip)

- Each turn-resume resets the shell cwd to the **main checkout**; the
  `block-main-checkout-race.sh` hook is active (sibling sessions live). So:
  **`cd <worktree>` as its OWN standalone Bash call, THEN bare `git …` in a
  SEPARATE call.** Never `cd && git`, never a newline-joined `cd`+write-git in
  one block (the hook false-positives and denies the commit). `cd`+read-git
  (status/diff/log) in one block is fine.
- **Always `git add <explicit per-task files>`** — parallel subagents share the
  worktree, so `git add -A` would cross-contaminate tasks. Check `git status`
  and stage only the task's files.

## The SDD ledger — READ IT FIRST

`.superpowers/sdd/progress.md` (gitignored, local to this Pi) is the **complete
per-task trail**: every commit SHA, every review verdict, every fix, and all
cross-task notes. It is the source of truth for what happened. This handoff
summarizes; the ledger has the detail. Helper scripts live in `.superpowers/sdd/`:
`extract-task.sh <plan> <TASKID>` (writes a per-task brief for dispatch),
`review-package <base> <head>` (SDD skill dir), `progress.md` (ledger).

## What is COMMITTED + reviewed (all pushed at ed1213de)

- **Phase A (A1–A7) + backend**: additive snapshot fields; `ft8_set_sweep_bands`
  + shared `Ft8CmdError`; device meter + listener-priority `DeviceReservation`
  (meter preempts on listener claim); `ft8_cat_probe`; offline
  `magnetic_declination` (WMM2025, real NOAA vectors); waterfall FFT thread +
  token subscriptions; `set_device` emit + hoi1 guards. **CI-green both arches**
  (gate had 2 CI-only compile errors — unused import + E0716 — fixed).
- **Phase B (B1–B3 + wiring)**: `useFt8Listener` hook + complete `ft8Types.ts`
  wire contract; `deriveUiState` (9-member total mapping); `deriveBandActivity`
  (evidence-only, **10-min window**) + `stripStats`. Hook wired, 94/94 ft8ui
  tests. **CI-green both arches (29219a28).**
- **Phase C components (all reviewed ✅ except C7)**: C1 renames (c34990f1),
  C2 provider+ribbon 9→4 map (6e71a929), C4 openness chips (289d95c2 + B3
  docstring fix 7792c1f9), C5 rail tabs+LiveDecodesTab (5b52268b), C6 aim
  hero+declination (283ba2f4), C10 BandSubsetPopover (89a6fa2b + mode-interp fix
  59b3e5bc), C11 DecodeFeed+map housing (2548117b), C9a setup device-picker
  (0283a8dd), C3 BandMatrix (b87d2c7a), C8 Waterfall (0ac71147 + gap-slack fix
  ed1213de), **backend FFT magnitude normalize (de97e4bd, CI-PENDING Rust)**,
  C9b rig-control+TestCAT+CTA (2ff260c7).

## IN-FLIGHT / uncommitted at handoff (COLLECT THESE FIRST)

1. **C7 LiveBandStrip — uncommitted, mid-finalization.** Files written in the
   worktree (unstaged): `src/ft8ui/LiveBandStrip.tsx`, `.css`, `.test.tsx`. The
   C7 implementer agent had written the files and was running tests when the
   session ended — **no `task-C7-report.md` yet**, so it is NOT verified. NEXT:
   `pnpm vitest run src/ft8ui/LiveBandStrip.test.tsx` + `pnpm typecheck`, confirm
   green (if not, resume/fix), then commit (parent), then review-package + task
   reviewer. C7 must satisfy: **wedged = RED dot + restart banner** (strip
   distinguishes severity, unlike the ribbon's flat amber); flags-overlay renders
   OVER the live body (clockUnsynced amber banner + dot; jt9Degraded dot + chip
   showing `snapshot.lastFailure`; catFixedBand OPERATOR-ASSERTED/UNCONFIRMED
   chip); force-expand on needs-setup/wedged/device-lost beats persisted-collapse
   (`tuxlink:ft8:strip`); composes Waterfall/DecodeFeed/BandSubsetPopover; accepts
   an optional `blockingSessionMode` prop to pass to the popover.
2. **C9b review — verdict uncollected.** C9b code is committed (2ff260c7) and
   green (61/61 + 165/165 consumer suites; commitNow optional/backward-compat,
   commitNow-before-probe, CTA disable-reason matrix all confirmed by the
   implementer). A task-reviewer subagent was running its verdict when the
   session ended. NEXT: re-dispatch the C9b task reviewer (base `de97e4bd`, head
   `2ff260c7`, package already at `.superpowers/sdd/review-de97e4bd..2ff260c7.diff`)
   and disposition, OR read its agent output if recoverable.

## CI — verify before trusting Phase C

`gh run list --branch bd-tuxlink-b026z.4/station-intel-l3-panel --json workflowName,status,conclusion,headSha`
— the **`CI` workflow for headSha `ed1213de` must be `success` on BOTH arches**.
The **backend magnitude fix (de97e4bd) is CI-PENDING Rust** (this Pi cannot
cold-compile) — watch clippy + cargo-test. If CI fails: `gh run view <id>
--log-failed | sed 's/\x1b\[[0-9;]*m//g'`, fix (Rust→CI, TS→vitest local),
re-push (cd-standalone + bare git), re-verify by SHA.

## REMAINING WORK (in order)

1. **Collect C7 + C9b review** (above). After both: all 11 Phase C components
   done.
2. **Phase C review gate** (holistic, ≥2–3 rounds; persist findings to
   `dev/scratch/b026z.4-phaseC-gate-round*.md` per ORCH-1): cross-component type
   coherence; sibling-☆ contract; force-expand-beats-collapse; flags-overlay-
   over-live-body; wedged=RED-in-strip; untrusted-input guards; no dead L5
   control; the new transparent controls use project button classes (feeds D3).
   Fix Critical/Important; push; **verify CI green by SHA**.
3. **Phase D (D1–D6)** — NOT started. Consult the ledger's cross-task notes:
   - **D1 wire panel body**: mount `LiveBandStrip` in `StationFinderPanel` below
     the body; connect the hook's `decodesRing`/`bandActivity`/`onPanToGrid` to
     StationRail + BandMatrix (both currently take these as optional props,
     unwired); render `Ft8SetupSurface` as the strip body in setup states via
     `deriveUiState().state`; **LAYOUT-1** min-height relax when force-expanded
     (setup CTA never clipped at ~700px); **align the aim-hero bearing's
     `operatorGrid` to the live `useStatusData().grid`** (C6 review: bearing uses
     a mount-only stale grid while declination uses the live one — fix in
     StationFinderPanel.tsx); **wire C10's `blockingSessionMode`** from active-
     modem state (`useActiveModemMode(activeConnection)?.kind` → display label);
     **memoize `flattenDecodeFeed`** with useMemo (C11 note); App-level
     production-mount test.
   - **D2** render harness (`dev/render-harness/harness.tsx`): per-uiState
     falsifiable `data-state` + a PNG per state via the WebKitGTK harness;
     needs-setup at 1024×700 not clipped.
   - **D3** WebKit computed-style gate: rail tabs / `si-collapse` / `chip-use` /
     `rf-test` (appearance/border/border-radius ≠ native GTK) + every dropdown =
     `.tux-select`.
   - **D4** waterfall perf (converged build, operator/loopback): paint headroom,
     decode non-starvation, A6 zero-FFT counter; **visually tune the backend
     magnitude fix's dB range** (currently ÷WINDOW/2 + [0,96]dB→[0,255]; the
     exact range is a D4 visual call).
   - **D5** wire-walk gate (`.claude/skills/wire-walk`): **OPERATOR supplies Flow
     1's non-heatmap clauses GREENFIELD** (do NOT draft them), trace verbatim to
     `file:line`. Heatmap clause defers to L5, Flow 2 to L4. Any broken primary
     clause = NOT shipped.
   - **D6** ship: exit-gate-5 rename-diff grep (no weakened selectors — C1's
     review already confirmed this for the rename commit); `dev/implementation-
     log.md` top entry; **AGENTS.md parity check**; README maturity-matrix note;
     PR ready + merge per ADR 0010 (no-squash merge-commit).
4. **Final whole-branch review** (most-capable model) + `superpowers:finishing-
   a-development-branch`.

## Cross-task notes / deferred items (do NOT drop — full list in the ledger)

- **B1 kind union**: `Ft8CmdError.kind` includes `internal-error`; `device-in-use`
  is NOT an error kind (busy = `MeterDto.state==='in-use'`, an Ok value).
- **Loading vs off**: pre-hydrate `snapshot===null` yields `uiState.state==='off'`;
  consumers prefer the derived non-null fields and treat null snapshot as loading.
- **C3 null-band regression (Important, edge-case, self-disclosed)**: a `Channel`
  with `band===null` (out-of-band/malformed freq) drops from every BandMatrix row
  (pre-C3 grouped list showed it). Decide at final review: add an "Other/unknown"
  catch-all row or file a bd follow-up. Not a binding-contract violation.
- Roll-up minors (final-review triage): C4 `--open-quiet` vs `--m-vara` blue
  proximity (palette tweak); C11 Gateways swatch reuses `--m-vara`; C6 helpers
  DOM-tested-only; StationRail "groups by mode" test now false-positive-by-proxy;
  A3 residual (meter single-read tail) + config-vs-resolved id keying; A1 double
  sysfs snapshot; A7 set_sweep emit not test-locked; Phase-B M1 (idle panel
  doesn't advance nowTick so a dot can stay dimmed past 10min until next event —
  a low-freq ticker in a consumer); M2 (seam-test spy restored inline — fold an
  `afterEach(vi.restoreAllMocks)` into a later frontend commit).
- **Commit-hygiene artifact**: BandMatrix CSS landed in the C11 commit (2548117b)
  because C3+C11 both touched StationFinderPanel.css concurrently. Functionally
  correct; attribution only.

## Worktree state (ADR 0009)

- Tracked: clean except C7's 3 uncommitted `LiveBandStrip.*` files (see above).
- Gitignored-stateful: `.superpowers/sdd/` (ledger + briefs + review packages —
  the recovery map), `dev/scratch/b026z.4-phase{A,B}-gate-*.md` (gate findings),
  `dev/adversarial/…` (spec-round Codex, from the prior session).
- No stashes of this session. bd `tuxlink-b026z.4` stays IN_PROGRESS.
