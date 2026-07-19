# Handoff — Routines round 2 executed: compat tree + ADR 0024, observability wire-walk + fixes, engine links green, mock v2 implemented

- **Agent:** chasm-marsh-heron (2026-07-18, single long session)
- **Open work item:** bd **tuxlink-iizmk** (IN_PROGRESS; notes rewritten this session, prior plan is in dolt history)
- **Operator interjections handled mid-session:** tuxlink-hq9g0 (audio tooling), tuxlink-iww9r awareness (parallel agent), R2 "internal error" popups (root-caused + fixed).

## Merged this session

- **PR #1157** (merged): fake-jt9 shims die by SIGKILL not SIGSEGV. Root cause of the
  R2 "Ubuntu has experienced an internal error" popups: the test suite deliberately
  core-dumped /bin/dash six times per run and apport reported it. Also removes the
  kernel core-dump pipeline from the suite (plausible, unproven, mechanism for the
  tuxlink-860t9 / tuxlink-b5qfw deadline flakes). Stale /var/crash artifact cleared on R2.

## PRs open at session end (merge each on CI green; merge = bare `gh pr merge <n> --merge`, one command, nothing chained)

- **PR #1155** — `bd-tuxlink-hq9g0/unified-audio-list`: station-level `list_audio_devices`
  MCP tool; `ardop_list_audio_devices` becomes a deprecated alias (byte-identical payload,
  test-pinned). ft8's list deliberately NOT aliased (different DTO/port). First arm64 verify
  run failed on the KNOWN ConsentGate arm64 load flake (tuxlink-2h16p, 1 of 4578, file
  untouched by the PR); rerun was in flight at handoff time.
- **PR #1159** — `bd-tuxlink-iizmk/composability-proof`: the two round-2 engine links
  (nested `$`-paths; branch comparison form op/value) + decree journal events
  (`branch_taken`, `step_skipped`). Negative proofs flipped green; bare-numeric-branch
  strictness negative retained. 199/199 engine tests + full-workspace clippy clean on R2.
- **PR #1160** — `bd-tuxlink-iizmk/compat-tree`: compat-tree spec + ADR 0024 (Proposed) +
  observability wire-walk spec + real-run harness fixture (`&real=1`) + History step list
  (O5/O6/O7) + the full mock-v2 human-scale redesign (designer + dashboard).
  300/300 routines vitest; typecheck clean; WebKitGTK renders inspected.

#1159 and #1160 share no files and can merge in either order.

## The three commitments, disposition

1. **Compat tree (DONE, doc merged via #1160 when it lands):**
   `docs/superpowers/specs/2026-07-18-routines-round2-compat-tree.md`. 0 of 24 scenario
   cells are human-actionable via Routines today; four read actions unblock 21, the first
   config write unblocks the rest. The ranked missing-action list (ranks 1-11, incl. the
   iww9r VARA-setup action) IS the round-2 functional requirements backlog: **the next
   build sessions implement ranks 1-5** (status reads, data.find_stations, config reads,
   docs_search, first config-write family with consent parity). ADR 0024 is Proposed and
   needs an operator accept/reject.
2. **Observability decree (wire-walk DONE; engine O1/O2 + UI O5/O6/O7 IMPLEMENTED;
   validation gate remains):** `docs/superpowers/specs/2026-07-18-routines-observability-wirewalk.md`.
   First-ever real-run validation of History (probe routine `heron-observability-probe`,
   run-1784416315-0000, executed through the LIVE R2 converge build via the MCP shim).
   O3 (call steps lack child run id) and O4 (anonymous end steps) are small engine
   follow-ons, NOT yet implemented. **Decree gate:** after #1159+#1160 merge and a new
   converge build, re-run the same wire-walk (probe + branch/skip-rich routine) against
   the REAL app and confirm the step list shows everything; the harness fixture makes
   the render half reproducible.
3. **Mock v2 (IMPLEMENTED, operator approval on the live build pending):** scoped
   13px-floor ramp, settings grid under canvas (Settings tab eliminated; two tabs:
   Design/History), clickable fact-chips, card nodes w/ 26px delete, 340px palette,
   dashboard row cards with per-row Run/History/menu. Punch-list items 1-6, 9, 11
   addressed here; 7, 8, 10 were fixed earlier in #1154. **tuxlink-iizmk closes only
   after the operator validates the redesign on a live converge build.**

## Environment / worktree state (ADR 0009 enumeration)

Pi (`~/Code/tuxlink/worktrees/`):
- `bd-tuxlink-iizmk-compat-tree` — branch pushed (PR #1160). node_modules present.
  Untracked: none beyond gitignored node_modules/.
- `bd-tuxlink-iizmk-engine-links` — proof branch pushed (PR #1159). node_modules present.
- `bd-tuxlink-hq9g0` — branch pushed (PR #1155). node_modules present.
- `jt9-test-signals` — branch MERGED (#1157) → dead per ADR 0017; dispose via the
  4-step ritual next session (nothing untracked of value).
- `heron-handoff` — this handoff's detached scratch; dispose after push.

R2:
- `~/Code/tuxlink/worktrees/bd-tuxlink-iizmk-routines-round2` — **dirty scratch union
  tree**: base detached 7d5a2c76 with this session's engine + mcp-core + jt9 edits
  rsynced over it (all of which are committed on the pushed branches; nothing unique
  lives here). Warm cargo cache: keep for the rank-1..5 build sessions.
- Live app (pid 28620, converge build 7d801187) kept running, UNTOUCHED. It predates
  the engine changes, so its journals have no branch/skip events.
- **OPERATOR ACTION NEEDED:** the probe routine staged a real message in the Outbox
  ("Observability probe" to N0RNG, mid AKQHQ5KF7FR7). Delete it before the next real
  CMS connection or it will be forwarded. The `heron-observability-probe` routine
  definition (disabled, manual-only) and its one failed run journal remain on the R2
  config dir; keep (they are the decree's ground truth) or delete after the re-walk.
- MCP client scratch at /tmp/mcp_client.py on R2 (harmless, temp).

## Also filed / updated

- bd tuxlink-b5qfw (fake_jt9 flake): noted the SIGKILL change as plausible fix; if
  flakes recur post-merge, capture names and attack the 2s deadline directly.
- bd tuxlink-hq9g0: grounded scope split (list side = PR #1155; set side belongs on
  iww9r's VARA.ini module; dep edge added). The issue text's "VaraUiConfig audio_device"
  claim was wrong on origin/main; verified and recorded.
- ADR 0024 index entry added to docs/adr/README.md (via #1160).

## Verification provenance

Engine: R2 worktree union tree (see above), `cargo test -p tuxlink-routines` 199/199,
workspace clippy `--all-targets -D warnings` clean, workspace tests green minus the
pre-existing fake_jt9 flake (now fixed + merged). Frontend: Pi compat-tree worktree,
`pnpm typecheck` + `pnpm vitest run src/routines` 300/300 (re-run independently after
the subagent implementation). UI evidence: WebKitGTK harness renders (software GL) of
dashboard, designer (tall), and History against the captured REAL journal. No claim is
made about the operator's converge build; it must be rebuilt post-merge.
