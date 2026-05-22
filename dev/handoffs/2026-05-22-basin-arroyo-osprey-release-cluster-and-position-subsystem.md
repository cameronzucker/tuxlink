# Handoff — 2026-05-22 — basin-arroyo-osprey — release finish + position-subsystem kickoff

## TL;DR
Diagnosed and prepared the **v0.0.1 release** (hand-cut; PR #106 merged, **tag not yet
cut**), shipped **lbg connect-hardening** (#107 merged) and the **882 CMS-locator privacy
fix** (#108 merged), then **brainstormed → spec'd → planned** the position subsystem
(`tuxlink-686`, PR #109) and **began its implementation** via subagent-driven-development:
**Tasks 1–4 of 12 done** (Maidenhead conversion + config field + the source arbiter), all
tested, the arbiter reviewed. Stopped at a clean reviewed-core checkpoint (extraordinary
session length). All branches pushed.

## 🚨 CRITICAL FIRST ACTIONS (next session)
1. **Read this handoff.** Then decide: finish the release, or resume the 686 implementation.
2. **The v0.0.1 release is NOT tagged yet** — operator action owed (see "Operator owes").
3. **686 resume gate:** the implementation is mid-flight on `bd-tuxlink-686/position-subsystem`
   (Tasks 1–4 done). Resume via **superpowers:subagent-driven-development at Task 5** of
   `docs/superpowers/plans/2026-05-22-position-subsystem.md`, IN the existing warm worktree
   `worktrees/bd-tuxlink-686-position-subsystem`. Do NOT restart the plan or re-brainstorm.

## Merged to main this session (by the operator, in parallel)
- **#106** `bd-tuxlink-7d6/release-v001` — v0.0.1 **hand-cut release**: curated `CHANGELOG.md`
  `## 0.0.1` section, `.github/.release-please-manifest.json` seeded `0.0.0`→`0.0.1`, new
  `version.txt`. (release-please could not cut it — the Actions PR-permission was off AND
  from the un-seeded 0.0.0 manifest it mis-computed `1.0.0`.)
- **#107** `bd-tuxlink-lbg/connect-hardening` — bounded DNS resolve (`resolve_with_timeout`),
  total connect deadline (`connect_with_deadline`), all-address error aggregation. Finding
  **#5 was reverted** after a Codex P2: the "only-Cancelled-on-teardown" abort fix broke the
  abort contract for pre-socket aborts; `aborting`-keyed mapping is contract-correct.
- **#108** `bd-tuxlink-882/locator-precision` — `config::broadcast_grid` + `cms_locator`;
  the CMS handshake locator is now reduced to `position_precision` (was transmitted at full
  precision — a real privacy gap found during the 686 brainstorm).

## Operator owes
**Release finish (v0.0.1 is merged to main but UNTAGGED):**
1. **Tag `v0.0.1` + GitHub release** at `origin/main` HEAD (commands in the #106 PR body).
   main HEAD now includes #106/#107/#108, so v0.0.1 will include the lbg + 882 fixes.
2. **Delete the orphan branch** `release-please--branches--main` on origin (carries the bogus
   1.0.0 release-please scratch commit). *The agent auto-mode classifier blocked me from
   deleting it — it's an operator action.*
3. **Enable repo setting** *Settings → Actions → General → "Allow GitHub Actions to create
   and approve pull requests"* (or wire a PAT). This is what blocked release-please; it's
   needed for release-please's first real auto-PR (v0.0.2).

**Owed GUI / live smokes (since granite-finch-spruce + this session):**
- gqo progress lines on a real Connect; 9z2 Abort mid-connect (recipe in
  `dev/handoffs/2026-05-21-granite-finch-spruce-connect-cluster-shipped.md`).
- **882:** live-smoke that the CMS accepts a **4-char locator** in the handshake (domain
  question, not code — Codex's amateur-radio knowledge is unreliable here).

## In-flight: tuxlink-686 position subsystem (PR #109, branch `bd-tuxlink-686/position-subsystem`)
**Design + plan (committed, pushed):**
- Spec: `docs/superpowers/specs/2026-05-22-position-subsystem-design.md`
- Plan: `docs/superpowers/plans/2026-05-22-position-subsystem.md` (12 tasks, 7 phases)

**Done + pushed + tested (181 lib tests green):**
| Task | What | Commit |
|---|---|---|
| 1 | `lat_lon_to_grid` (Maidenhead 6-char) + `position` module | `0c5684c` |
| 2 | `grid_to_lat_lon` (square center) | `db790a0` |
| — | merge `origin/main` → 686 (pulls `broadcast_grid` from #108) | `d5bc426` |
| 3 | config `position_source` field (default `Gps`) | `614d4ff` |
| 4 | `PositionArbiter` (manual sticky, broadcast reduction) — **reviewed** | `abb207f` |

**Remaining (Tasks 5–12 + final review):** config_set_grid command + manage the arbiter in
lib.rs (5); CMS locator from the arbiter superseding `cms_locator` (6); `position_source` in
the status DTO (7); `GridEdit` inline-edit + source chip (8); gpsd TPV parse (9); gpsd watch
task + backoff (10); `position_set_source` + spawn the client at startup (11); gpsfake e2e (12).

**RESUME:** in the warm worktree, run subagent-driven-development at Task 5. The cold-build
trap that tripped the first subagent is gone (target is warm); dispatch implementers told to
run cargo in the FOREGROUND.

**Deviation from the plan to carry forward:** Task 3 did **NOT** bump `CONFIG_SCHEMA_VERSION`
(the plan said to). config.rs has a strict `deserialize_schema_version` guard that rejects
version mismatches; an additive `#[serde(default)]` field migrates old configs transparently
without a bump. Tasks that reference "schema bump" should ignore that note.

## Worktree state (per ADR 0009)
- **`worktrees/bd-tuxlink-686-position-subsystem`** — ACTIVE. Tracked tree clean (Tasks 1–4
  committed + pushed). Gitignored on-disk (NOT pushed): `src-tauri/target/` (warm — keep for
  fast resume), `node_modules/`, `dev/scratch/` (codex/pr-body scratch), `.superpowers/`
  (brainstorm mockups; the companion server is STOPPED).
- **#106/#107/#108 worktrees** (`bd-tuxlink-7d6-release-v001`, `bd-tuxlink-lbg-connect-hardening`,
  `bd-tuxlink-882-locator-precision`) — all MERGED → **disposable** via the ADR 0009 ritual
  (left in place; no at-risk untracked content beyond standard target/node_modules).
- ~13 pre-existing stale worktrees from prior sessions (0ic, 22l, 2a7, 5jh, 8zt, 98g, f1a,
  fzm, g3d, h2y, khe, pqg, handoff) — not this session's; disposal owed when convenient.

## Decisions made this session
- v0.0.1 **hand-cut** (operator) vs release-please-cut (release-please mis-computed 1.0.0).
- lbg **#5 reverted** (Codex P2 — abort-contract regression).
- 882 filed + fixed (locator precision privacy gap).
- Position subsystem: **Approach A** (inline-edit + source chip); **build GPS now** via
  gpsd-client (LC29C already on `/dev/ttyAMA0`, gpsd serving; `gpsfake` for tests); source
  contract (Manual sticky, operator-only switch, precision-at-broadcast, source visible);
  **wizard grid = initial value, not a manual pin** (only runtime inline-edit pins Manual).
- Filed `tuxlink-882` (privacy gap) + `tuxlink-686` (umbrella, absorbs 2y5 + 2ob).
