# Handoff: Station Intelligence SHIPPED (Tasks 1-13, PR #1181 -> a28ff6dd); race-hook mechanism corrected

- **Agent:** sorrel-redwood-marsh
- **Date:** 2026-07-20 (session spanned 07-19 evening through 07-20 morning)
- **Scope:** resumed the harrier-sandbar-cardinal SI work: rescued its stranded Task 1 + handoff, corrected the block-main-checkout-race.sh mis-diagnosis, executed SDD Tasks 2-13 of docs/superpowers/plans/2026-07-19-station-intelligence-operational-usability.md, ran all ship gates, integrated 31 commits of parallel main, and MERGED PR #1181 to main as a28ff6dd.

## Shipped

Station Intelligence operational usability, whole (ADR 0022 compliant), 24 branch commits:
- Setup takeover DELETED (net -1.7k lines); map + rail + strip mount unconditionally; setup lives in the strip (OS-convention device select).
- Three map layers: FT-8 heard (SNR-ramped, default on), heat choropleth (default off), evidence filter (ghost 0.2, SNR slider, honest note chip, 60 s staleness tick).
- Winlink channels JSON API: per-channel dial frequency (API serves dial Hz; the plan's center-offset assumption was WRONG, fixture-wins) + bandwidth + VARA FM end-to-end (chips both locations one state, rail, prefill).
- Frequency hero + channel-row badges (comma-grouped format per plan; computed USB center value per approved mock).
- Containment: measured mechanism first (unbounded si-feed grew the strip +125px, propagated to the panel until the 92vh clamp), fixed with a --live-scoped 200px body + fixed panel height; WWV row contained.
- MCP/agent parity: evidence + channels over find_stations, StationModeDto vara-fm, canonical ListingMode::expand_selector shared by MCP + routines (text-cache path keeps ALL deliberately).

## Gates (all passed; evidence local in dev/scratch/)

- Wire-walk: 3 operator flows (supplied greenfield in-session) + 7 spec exits, all wired, log at dev/scratch/si-wirewalk-20260719.md. Operator ruled flow 3 "approved as-built" (map pin population IS the gateway/peer surface; no textual list intended).
- Codex adrev: GPT-5.5 (pinned; first attempt was an argparse stub, re-run per CLAUDE.md), real 26k-line review, 5 P2 findings ALL fixed (fallback ChannelsCache panic, VARA FM absent from empty-mode expansion incl. routines, channel-conjunctive band+bandwidth in MCP, shared start guard, meter release before listener start). NOTE: the ADR 0023 GPT-5.6 SHADOW round did NOT run; the amendment (2026-07-19) postdates this session's start and was discovered at close. Follow-up: run the shadow round + ledger entry over the merged diff, or record the skip in the ledger.
- Full suite 4686/4686 pre-merge, 4692/4692 post-merge; typecheck clean; lint:docs clean; fixture parity cmp identical (sha 01677b80).
- CI green both arches on the final head 62d46126; operator visual approval on dev/scratch/si-redesign/final-approval-1366x800.png.

## Integration note

Main advanced 31 commits (parallel routines/MCP session) while the branch ran; GitHub could not build the merge ref (CONFLICTING) so CI silently created NO runs for the last two commits: an empty `gh run list --commit` on a PR head means CHECK MERGEABILITY, not "CI is slow". Resolved by merging origin/main in (c801bcc4): three import-union conflicts kept BOTH sides verbatim; one true semantic break (main's new StationFilterParams test initializer predating our three fields, E0063) fixed forward in 62d46126.

## Race-hook correction (memory + tuxlink-0dp5l updated)

The 07-19 "payload cwd pinned to launch dir" diagnosis was WRONG. The hook reads the Bash tool's persistent shell cwd at call time. Real traps: compound `cd X && git ...` one-liners present the pre-cd cwd; the shell resets to the session primary dir after denials AND at turn boundaries. Discipline that worked all session: standalone `cd` first, then bare git; absolute paths for file ops; staged-diff em-dash gate before every commit (caught 2 violations at commit time).

## State for the next session

- Branch bd-tuxlink-6i0ie/si-operational-usability: MERGED-DEAD (ADR 0017). Remote branch still exists (merge done without --delete-branch per repo convention); delete with `git push origin --delete bd-tuxlink-6i0ie/si-operational-usability` if desired.
- Worktree worktrees/bd-tuxlink-6i0ie-si-operational-usability: still on disk, now on the follow-up branch agent-sorrel-redwood-marsh/si-ship-handoff. Gitignored-stateful content (ADR 0009 enumeration): .superpowers/sdd/{progress.md, task-N-{brief,report}.md, codex-fixes-report.md, review-*.diff} (full SDD audit trail), dev/scratch/si-wirewalk-20260719.md + si-redesign/ + si-containment/ PNGs, dev/adversarial/2026-07-19-si-usability-codex.md, node_modules. Dispose via the ADR 0009 ritual after archiving .superpowers/sdd and dev/scratch if the audit trail is wanted.
- bd: tuxlink-6i0ie, hcmfb, nkzng, 1w0d0 CLOSED with ship notes; 9obx2 unblocked (commented). Filed nothing new beyond the GPT-5.6 shadow-round follow-up noted above (operator may want a bd issue for it).
- Main checkout: untouched all session (its dirty state incl. README + PNGs predates this session); still on bd-tuxlink-ant8s/ardop-connect-fixes.
- dev/implementation-log.md does not exist yet (CLAUDE.md "once created"); this handoff carries the entry-equivalent.
