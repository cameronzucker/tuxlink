# Handoff — 2026-07-10 (slate-fox-tanager): CMS acceptance packet shipped · gbb05 fixed · FT8 L1 plan executor-ready

Three workstreams this session. All work pushed; no stashes created; no
uncommitted agent changes anywhere.

## 1. Winlink CMS acceptance packet (SHIPPED, parked on Rob)

- The WDT onboarding doc (v20260600) gates production access on a 4-part
  acceptance run, not a bare whitelisting request. Packet:
  `dev/winlink-cms-acceptance-packet.md` (merged, PR #1067) — requirement
  matrix, temp-RMS plan (site call N7CPZ / on-air channel call **N7CPZ-1**,
  Trimode-under-WINE on R2 primary, LinBPQ+ardopcf fallback), open WDT
  questions, ready-to-send cover email.
- **Operator SENT the cover email to Rob (WDT/CMS point of contact)
  2026-07-10.** Awaiting: N7CPZ-1 whitelist on cms-z + routing guidance,
  Tuxlink access key, account-test target, Priority:Normal confirmation.
- Graph: `ie7dy` = tracking (blocked by all tests) ← `o7fct` telnet test
  (**unblocked, runnable any time**), `0zngx` account CRUD (← `lu7t` key),
  `iiqoy` OTA (← `dzp9n` RMS stand-up; gbb05 prerequisite RESOLVED),
  `4d5w6` binaries. `hmoz8` = quality track, not a gate. Rob memory saved
  (`reference_rob_winlink_cms_poc`).
- While parked: telnet test and R2 Trimode install are doable without Rob.

## 2. gbb05 SSID-stripping fix (SHIPPED)

- Root cause pinned + fixed: `stations.rs` split channel tokens on '.' AND
  '-', so `KB2PCN-5.WINLINK` → callsign `KB2PCN`. Fix (split '.' only) merged
  to main via PR #1068 (`002b5fc1`, merge `23af2e34`), full TDD (standalone
  rustc harness red→green), CI green both arches, issue closed. Frontend was
  already SSID-ready (`channel.ssid ?? baseCallsign`); packet dialing fixed
  too (`PI1ZTM-12`). NOTE for the OTA test: use a release cut AFTER
  2026-07-10 (verify build provenance).
- Adjacent compliance finding filed: `tuxlink-y0z5h` (ardopcf GPLv3 §6
  corresponding-source offer).

## 3. Station Intelligence / FT8 epic (PLANNED — execution is the next session)

- Epic `tuxlink-b026z` re-baselined to the operator's recorded L0 NO-GO
  decision: engine = **managed jt9 subprocess** (wsjtx package). `.1` closed;
  stale M3 worktree disposed (Codex transcripts archived).
- **Design delta v2** (5 adversarial rounds — subprocess lifecycle, DSP/slot
  timing, product integration, Codex, licensing — every claim verified
  against the real jt9 2.7.0 + committed fixtures):
  `docs/design/2026-07-10-station-intel-jt9-engine-delta.md`. Highlights:
  jt9 segfaults instead of erroring (stderr capture + WAV preflight
  mandatory); persistent FFTW-wisdom dir + prewarm; tmpfs slot WAVs; shmem
  mode BANNED (GPL boundary) → hashed-callsign amnesia accepted+surfaced;
  wall-clock-true slot timeline with zero-filled xruns; 3-axis state machine;
  snake_case MCP tools + ft8_status; slot-batched events; hand-rolled
  Maidenhead density layer (L.heatLayer isn't installed); deb+rpm Recommends
  wsjtx >= 2.5 + AGPL license-metadata fix.
- **L1 implementation plan** (3 review rounds; round 2 compiled the embedded
  code in a scratch crate and ran the real binary — 19/19 lib tests, 4/4 e2e):
  `docs/superpowers/plans/2026-07-10-station-intel-l1-jt9-decode-service.md`.
  9 tasks + T8.5 packaging, complete code every step, new std-only leaf crate
  `src-tauri/tuxlink-jt9` so the whole layer red-greens locally.
- Branch `bd-tuxlink-b026z.2/station-intel-jt9` (worktree
  `worktrees/bd-tuxlink-b026z.2-station-intel-jt9`, claimed by `b026z.2`,
  in_progress): commits 78311e34 (delta), ea1fe6ab (plan), this handoff.
  node_modules installed (pre-push lint needs it). No PR yet — opens at plan
  Task 9.

## Worktree / branch state at handoff

- `worktrees/bd-tuxlink-b026z.2-station-intel-jt9` — LIVE, bd-claimed
  (b026z.2), clean tree, pushed. Gitignored-on-disk: `node_modules/`,
  `dev/adversarial/2026-07-10-jt9-delta-codex.md` (Codex transcript,
  local-only by policy).
- Packet worktree (ie7dy) and gbb05 worktree: disposed per ADR 0009 after
  merge; registry pruned. Main checkout untouched (operator state, other
  live sessions on it).
- bd jsonl: synced to the b026z.2 branch up through the plan-ready note.

## Next session

Execute the L1 plan task-by-task via subagent-driven-development from the
b026z.2 worktree. The plan is self-contained for zero-context executors
(absolute paths, trailer discipline, strict task ordering, review gates A/B/C
with findings persistence). After L1 merges: L2 plan (capture subsystem —
greenfield cpal/ALSA; the delta's L2 seam section is its spec seed).
