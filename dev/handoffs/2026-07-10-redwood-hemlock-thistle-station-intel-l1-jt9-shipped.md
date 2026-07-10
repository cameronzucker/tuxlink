# Handoff — 2026-07-10 (redwood-hemlock-thistle): Station Intelligence L1 shipped — managed-jt9 decode service merged

One workstream this session: executed the L1 plan
(`docs/superpowers/plans/2026-07-10-station-intel-l1-jt9-decode-service.md`)
end-to-end via subagent-driven development from the bd-claimed worktree.

## What shipped (PR #1070, branch `bd-tuxlink-b026z.2/station-intel-jt9`)

- New std-only leaf crate `src-tauri/tuxlink-jt9`: parse (jt9 stdout grammar,
  non-finite DT rejected), message (pinned FT8 field grammar, single-modifier
  CQ), wav (slot-WAV preflight — jt9 cannot be trusted to reject bad input),
  types (L1↔L2 API: taxonomy, N=5/k=20 thresholds, `SLOT_DECODE_TIMEOUT_SECS
  = 12`, BadWav stable strings, dropped-slots counter note), discover
  (override>PATH, bounded 2 s version probe), runner (the ONLY jt9 spawn site;
  preflight → spawn → bounded drains → wait-or-kill → 7-arm classification;
  sentinel-aware partial salvage; decode_slot returns bounded on ALL paths).
- 46 tests at merge: 24 unit + 18 fake-jt9 lifecycle (every taxonomy arm and
  cross-arm tiebreak pinned, both grandchild-pipe-holder wedge cases) + 4
  real-jt9 e2e (shared warm data dir + prewarm-once + serialized decodes —
  arm64 CI flake preempted). Clippy `-D warnings` clean both crates.
- tuxlink-ft8 fixture refs regenerated at production flags (`-8 -d 3 -p 15
  -w 1`): crowded 14 / ordinary 6 / busier 4 / quiet 4; oracle.rs doc corrected
  (the old 3-column docstring was stale — refs carry the HHMMSS column +
  DecodeFinished trailer); `floor_calibration_diag` re-indexed + ~-gated.
- CI: wsjtx via the cached apt action both arches (salt v3→v4);
  `scripts/check-jt9-provenance.sh` GPL-boundary guard wired before the Rust
  steps — 8/8 deny-patterns live-trip-tested both directions. Closes the
  b026z.7 scope.
- Packaging: deb/rpm `Recommends: wsjtx >= 2.5`; `bundle.license` GPL→AGPL
  fix; install.md prerequisites sentence.
- Fix-forward during CI: the deb-install smoke's blanket "no system hamlib"
  assertion now (a) asserts the tuxlink .deb's own Depends field carries no
  hamlib (the real tuxlink-hs2k contract) and (b) attributes any installed
  libhamlib-utils to the wsjtx Recommends chain. All 14 checks green at
  0cd2f785 on both arches before merge.

## Process record

- SDD with per-task spec+quality reviewer gates; plan review gates A/B/C ran
  3–4 rounds each (findings persisted to `dev/scratch/b026z.2-gate-{A,B,C}-
  findings.md` — dev/scratch is gitignored; archived in the worktree tarball
  per ADR 0009). Final whole-branch review (fable) verdict: ready to merge
  after one polish commit (bounded clean-exit drains + L2-contract pins).
- Substantive catches by the gates (beyond per-task reviews): NaN/inf DT
  acceptance; multi-modifier CQ over-acceptance (plan's own example code
  disagreed with its pinned prose — prose enforced); sentinel-aware `partial`;
  unbounded version probe; per-test cold-FFTW e2e flake risk; unpinned
  taxonomy arms; clean-exit drain wedge; ci.yml comment inaccuracy.
- Known blemish: commit c3464f93 carries a stray `Agent: osprey-delta-gulch`
  trailer (a subagent invented a moniker; amend is hook-banned). It is
  redwood-hemlock-thistle session work.

## bd state

- CLOSED this session (after merge): `tuxlink-b026z.2` (L1), `tuxlink-b026z.7`
  (provenance guard scope).
- FILED this session: `tuxlink-iy1av` (verify parser vs wsjtx 2.5-era stdout —
  parse.rs carries the dated note), `tuxlink-b026z.8` (residual grandchild
  pipe-fd leak bound; L2 mitigation options), `tuxlink-gujnz` (design
  question: salvage-on-signal parity — current discard behavior is deliberate
  and test-pinned; decide at L2 wiring or earlier).
- NEXT epic child: `tuxlink-b026z.3` (L2: live audio capture + slot-timing
  decode service — greenfield cpal/ALSA; the delta's L2 seam section is its
  spec seed; consumes types.rs N=5/k=20 + SLOT_DECODE_TIMEOUT_SECS).

## L2 kickoff notes (carry these in)

1. `Jt9Runner::decode_slot` is blocking — wrap in `spawn_blocking`; construct
   with `Duration::from_secs(types::SLOT_DECODE_TIMEOUT_SECS)`.
2. The N=5 degraded counter folds DROPPED slots (backpressure) per the delta —
   types.rs doc says so now.
3. Residual accepted bound: detached drain threads + pipe read-fds leak until
   pipe EOF if a killed OR cleanly-exited jt9 left a pipe-holding grandchild
   (b026z.8; process-group kill needs libc/nix — L2's call).
4. `raw_lines` (unparseable-line count) is internal to the runner; L2 only
   sees slot-level ParseError — noted at final review as acceptable.
5. Wire-walk: L1 is not user-reachable by design; the gate runs when L3/L4
   make FT8 user-reachable.
6. The plan doc retains the old `jt9-ap-off` ref-regeneration recipe as a
   point-in-time record — the live recipe is in the fixtures README.

## Worktree / branch state at handoff

- `worktrees/bd-tuxlink-b026z.2-station-intel-jt9`: disposed per ADR 0009
  after merge (inventory → tarball archive incl. gitignored dev/scratch gate
  findings + .superpowers SDD ledger/reports + node_modules EXCLUDED →
  rm -rf → prune). Archive at `.claude/worktree-archives/` on this Pi.
- Branch `bd-tuxlink-b026z.2/station-intel-jt9`: merged via PR #1070
  (merge-commit per ADR 0010), remote branch deleted, now merged-dead.
- Main checkout: untouched all session (on `bd-tuxlink-ant8s/ardop-connect-
  fixes`, another session's state).
- No stashes created anywhere.
