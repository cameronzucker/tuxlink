# Session handoff — arroyo-tamarack-slate — 2026-06-12

Long "fix the partially-built / unwired backlog" session that turned into a P0
emergency (CMS telnet dead after reinstall), a disk-pressure cleanup, and a
uninstall-flow rework. **7 PRs merged to `main`**, bd reconciled, ~190 GB
reclaimed. Next session: **final alpha release-gate issues**.

## ⚠️ Read first — checkout state (probed directly at handoff)

- **Branch:** `bd-tuxlink-xygm/recover-handoffs` (the main checkout), **1301
  commits behind `origin/main`**, 38 ahead, no rebase in progress.
- **The main checkout is STALE.** All code reads this session used
  `git show origin/main:<path>` or worktrees off `origin/main`. Do the same.
  Reading the working tree directly = reading 1301-commits-old code (this bit
  the session at the start — the first `MessageList.tsx` read was pre-multi-select).
- **Working tree:** `.beads/issues.jsonl` modified (durable in Dolt; do NOT
  hand-commit the stale JSONL) + several pre-existing untracked handoff docs +
  `dev/tools/` + a design doc, all from prior sessions (not this one).

## Shipped this session (all merged to `origin/main`, CI green both arches)

| PR | bd | What |
|----|----|------|
| #603 | hh1j | Inbox multi-select drag moves the WHOLE selection (drag payload was single-id; l80q wired context-menu/bulk-bar but not drag) |
| #611 | o0c8 | VARA panel routes the sidebar-selected intent instead of hardcoded `cms` (fixed a real CMS-secure-login-password leak on P2P) |
| #613 | 8c9f | VARA dial field labels intent (peer vs RMS gateway) |
| #615 | 5ceg | Migration docs section → second in the in-app help nav |
| #616 | zmzx | README "Coming from Winlink Express?" migration pointer |
| #619 | **aw6g** | **P0 fix:** wizard installs the native backend in-session (no restart) |
| #624 | aip4 | Uninstall flow directs the whole process (data + package + verify); killed the "Missing wall" |

## The P0, root-caused + fixed (#619)

Symptom: after a documented uninstall/reinstall, CMS telnet "doesn't even start"
to any address/port. Root cause (confirmed from the operator's boot logs):
the native backend is installed only at **bootstrap**, only when on-disk config
already has `wizard_completed && connect_to_cms`. On a fresh install bootstrap
runs BEFORE the wizard → no backend; completing the wizard wrote a valid config
but never installed the backend → `cms_connect` → `state.current()==None` →
"backend offline". A restart re-ran bootstrap (`spawn`) and "fixed" it. Fix:
`wizard_persist_cms` now calls `bootstrap::install_native` after a successful
persist, guarded by pure `should_install_after_persist`.

## bd reconciliation (the macro-sweep follow-up)

Multi-agent workflow audited **362 issues** (196 closed-today + 165 open) against
`origin/main`: **29 false-opens closed, 4 reopened** (per operator approval).
Report committed on branch `bd-tuxlink-ahnz/bd-recon-audit`
(`dev/scratch/2026-06-11-bd-code-reconciliation.md`). bd now ~505 closed.

## Disk (was the emergency that forced this)

Was 84% (139G free); cleared ~270 GB of worktree `target/` caches (skipping live
builds) + node_modules → **62% (322G free)**. **143 worktree directories still
exist** (caches cleared, dirs remain) — disposal is operator's call (each needs
the ADR-0009 ritual; some belong to other live sessions). Live build at handoff:
`worktrees/bd-tuxlink-954o-forms-print` (another session — avoid).

## In-flight worktrees

- **Mine: all disposed.** hh1j, o0c8, 8c9f, 5ceg, zmzx, aw6g, aip4 — merged +
  removed. The `bd-tuxlink-ahnz` audit worktree was also disposed (report pushed
  to origin first).
- Other sessions' worktrees (0063, loc, nitb, 954o, …) left untouched.

## Pending — operator-only (I cannot drive a GUI/real build)

1. **P0 first-run smoke (#619):** fresh install (or `tuxlink cleanup --all` +
   reinstall) → complete wizard → click Start on CMS telnet **WITHOUT restart**.
   Should connect immediately. This is the real proof of the P0 fix.
2. **#624 uninstall UI:** quick WebKitGTK look at the new Part 2 block layout +
   the collapsed "Launcher entries" section.

## Open / not done (real, not false-open)

- **px36** — WLE mailbox-content import TOOL (genuinely unbuilt; the migration
  doc honestly describes the manual `native-mbox` copy path).
- **0ye6** — VARA+ARDOP panel umbrella. Operator decided panels stay SEPARATE
  (shared-RadioSessionPanel REJECTED; grounded in decompiled WLE). Functional
  gaps fixed (o0c8 + 8c9f). **Can likely be closed** — only the rejected
  shared-panel scope remained.
- FM-P2P multi-hop relays (VaraFMSession relay one/two) — advanced, deliberately
  unfiled (no concrete need yet).

## Gotchas worth carrying forward

- **CMS host:** operator config points `connect.host` at `server.winlink.org`
  (production), which rejects tuxlink's unregistered client SID. `cms-z.winlink.org`
  is the dev target that accepts it. Not a bug; relevant for any CMS connect test.
- **No cold cargo on this Pi** (new memory): don't cold-build/test Rust locally —
  open the PR (draft ok) and let GitHub CI compile. I wasted a build + disk this
  session ignoring this.
- **Main-checkout-race hook + cwd:** commits/pushes from a worktree get attributed
  to the main checkout when the bash payload cwd is main. Fix: a standalone
  `cd <worktree>` call FIRST, then the git op in the next call.

## Next session — final alpha release-gate issues

Start from `bd ready` filtered to alpha-gate scope. Two operator smokes above are
the only verification debt from this session. The grounding discipline that paid
off all session: for "does feature X work / exist," read the decompiled WLE
artifact (`~/Code/library-of-hamexandria/winlink-re/decompiled/`) and `origin/main`
code — not the working tree, not assumptions.

Agent: arroyo-tamarack-slate
