# Handoff — 2026-07-13 (crag-fox-savanna): QA round-3 SHIPPED (#1086 merged); Contacts/Favorites/Heard consolidation on PR #1089

Session arc: resumed Station Intelligence QA round-3 → root-caused + fixed all
8 findings (renders operator-approved) → **PR #1086 MERGED** → mid-session the
operator reported the ARDOP arm-listener/two-step defect (findings-only, no
code, per operator instruction) → operator requested + approved the
Contacts/Favorites/Heard consolidation → implemented, Codex adrev round
applied → **PR #1089 OPEN** (CI pending at handoff time).

## Shipped: QA round-3 (#1086, merged)

- **F5 root cause**: the wave-2 `z-index:1100` was inserted ABOVE a leftover
  `z-index: 7` in the SAME CSS rule — last declaration wins, fix dead on
  arrival. Reproduced live on the R2 (0.89.1) first. Removed + CSS-source
  regression test (`StationFinderPanel.css.test.ts`, raw-import per TEST-1)
  pinning exactly ONE z-index > 1000 per overlay rule. Overlays offset to
  clear Leaflet zoom/scale controls (collisions invisible until they painted).
- **F1 (operator ruling)**: FT-8 listening is SESSION-SCOPED — boot autostart
  removed (lib.rs), start/stop no longer persist `ft8.enabled` (retired to
  parse-compat), regression test, L2 spec §Autostart marked RETIRED.
- **F2**: setup surface is the panel FULL BODY (firstrun-v2 mock) with
  "← back to finder" (needs-setup includes non-fixable-in-place blockers:
  wsjtx-absent, unsupported-sample-rate). Strip's needs-setup arm = one-line
  "Open setup →" re-entry; `setupSurface` slot prop retired.
- **F3** popover opens upward; **F4** ribbon blocked labels sentence-cased;
  **F6** "Refresh from WWV" + offair-row CSS; **F7** Live-decodes tab
  `si-count` NN/min badge wired (existed only in mockups); **F8** = F5's pill
  + data-real gridless peers (R2 has only an SMTP pseudo-contact).
- New harness: `view=finder` (whole StationFinderPanel via Ft8ListenerProvider
  + canned reads; `?state=setup`; viewport pre-seed), `?ft8state=` on ribbon,
  `snap-click.py` (click-then-snapshot for click-gated states).

## OPEN: Contacts/Favorites/Heard consolidation — PR #1089 (tuxlink-sbf03)

Operator: the three surfaces "look like three completely different features
when they're not" — Favorites ⊂ Contacts; Heard/Recents = candidate Contacts;
groups/sorting seemed absent. Approved design:
`docs/design/mockups/2026-07-13-contacts-unified/contacts-unified-v1.html`.

- ContactsPanel = THE address surface. Scope pills (All / ★ Favorites /
  Heard) + sort select. **FavoritesPanel deleted**; the Favorites
  pseudo-folder mounts ContactsPanel `initialScope='favorites'` (same
  AppShell Connect handler; contact-less favorites keep rows).
- One row anatomy: avatar (dashed = not saved) · callsign+name · reach dot ·
  last-heard age · ★; EMAIL chip for SMTP pseudo-contacts; contained "+N"
  group avatar stacks. "Heard — not saved" merges unconfirmed contacts +
  suggestions. Detail pane gains per-dial ★ (a starred dial IS a Favorite —
  favoriteKey find-or-create) beside Connect.
- Latent bugs fixed en route: "Last heard" sort was a silent no-op (the
  LastHeardMap was never supplied to buildContactTree).
- **Codex adrev round 1** (dev/adversarial/2026-07-13-contacts-unified-codex.md,
  local-only): 1 P1 + 5 P2; five applied (remount keys on the pseudo-folder
  mounts; star double-click guard; MHz freq convention via exported `mhz`;
  scope-switch clears selection/editor; contact_id-authoritative favorite
  matching, SSID-safe). ONE REJECTED with rationale: per-freq favoriteKey
  would diverge from the backend's per-mode-per-gateway unit identity.
- Verification: typecheck clean; 231 tests green across contacts/favorites/
  shell/channelGrouping (and the earlier full 4111-test run pre-#1086).
  Renders (operator-approved): `dev/scratch/qa-r3-renders/new-*.png`.
- **Next session: check #1089 CI, merge on green (operator pre-authorized the
  PR; merge needed explicit say-so this session), then wire-walk the three
  flows on the R2 live** (browse/save-heard, star-a-dial → ribbon target,
  Favorites-folder Connect).

## Filed, untouched (operator: findings before changes)

**tuxlink-r788i (P1)** — ARDOP/VARA arm/session UX: (a) arming spawns the
modem and the panel footer keys ONLY on `isStopped` → arming presents the
live-session footer (Send/Receive + Stop); (b) Stop tears the armed
listener's modem down while the arm record stays armed; (c) the two-step
Start/Send encodes RADIO-1 consent framing into product UX (VARA comments
literally say "Part 97 consent click"); ARDOP's Start does a full on-air ARQ
connect that then idles. The one-click design already exists (ribbon
`connectFor` chains connect+exchange). Packet/Telnet are single-action.
Needs an operator-approved design + renders before any code.

## Branch / repo state

- **origin/main** includes #1086 (all QA r3 fixes).
- **PR #1089 open**: branch `bd-tuxlink-sbf03/contacts-unified` (this handoff
  rides on it). CI pending at handoff.
- Worktree `worktrees/bd-tuxlink-b026z.4-station-intel-qa-r3` now hosts the
  sbf03 branch (claim noted on BOTH bd issues). The QA-r3 branch
  `bd-tuxlink-b026z.4/station-intel-qa-r3` is merged-dead. Gitignored state:
  node_modules only. No stashes.
- bd: **tuxlink-b026z.4** notes current (D4 waterfall judgment + D5 wire-walk
  formalization still pending as phase gates); **tuxlink-sbf03** in_progress
  (claim + design/render approvals noted); **tuxlink-r788i** open P1;
  **tuxlink-nkzng** (VARA bandwidth classes) untouched.
- v0.89.1 installed on the R2 (operator did it mid-session). QA-r3 fixes are
  post-0.89.1 — the next nightly pre-release carries them; live re-verify of
  F5/F2 belongs to the operator's next QA round on that build.
- R2 debug tooling from the vetch handoff still valid (r2-poe SSH, xwd
  screenshot pattern, /tmp/xclick.py survives).

## Working discipline (KEEP)

- Render-first stands: every product-surface change this session was
  WebKitGTK-rendered and operator-approved BEFORE its PR.
- Codex adrev on substantive diffs; record REJECTED findings with rationale.
- The main checkout is operator state; this session read main via `git show`
  and worked entirely in the worktree.

## Addendum — v0.90.0 release-PR CI failure (investigated + fixed, PR #1090)

The release PR (#1088) failed `verify (amd64)` on
`tuxlink-jt9::discover::tests::override_wins_and_version_comes_from_sibling`
("WSJT-X 2.7.0" expected, UNKNOWN fallback got). Pre-existing flake, NOT
release content: `probe_version` treats any spawn error as the UNKNOWN
fallback, and multi-threaded cargo test's fork→exec window intermittently
yields ETXTBSY on the just-installed fake script. Fix (PR #1090, branch
`bd-tuxlink-qxyim/jt9-probe-deflake`, bd tuxlink-qxyim): bounded ETXTBSY
retry inside the existing 2 s PROBE_DEADLINE (raw errno 26 — the ErrorKind
postdates the 1.75 MSRV). Verified locally on the leaf crate: 24/24 tests +
clippy clean. The failed #1088 job was RE-RUN (in progress at handoff) —
merge #1090 on green regardless, so the flake stops recurring; #1088 goes
green on the rerun or, failing that, after #1090 lands and it re-triggers.
