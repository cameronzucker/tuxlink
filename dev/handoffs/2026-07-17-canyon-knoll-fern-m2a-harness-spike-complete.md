# Handoff — M2a harness spike COMPLETE; recommendation: Pi extensions + Responses-route re-probe

- **Agent:** canyon-knoll-fern
- **Date:** 2026-07-17 (overnight, ~03:00Z–09:00Z)
- **Session scope:** merged PR #1128, disposed the M1 worktree, executed
  milestone 2a end-to-end (all 6 registered cells + 1 flagged post-hoc
  probe), built + deployed the operator-requested Spark dashboard, restored
  Spark to as-found state, disposed all worker worktrees.

## Results (canonical: dev/research/2026-07-17-m2a-harness-spike/)

- `report.md` — findings F1–F4 + verdict. Headline: **every cell FAILED
  both attempts under both new harnesses**, but the anatomy discriminates:
  - F1: Codex's reasoning-as-final-message seam did NOT reproduce; a
    sibling protocol-mismatch seam appeared once under Pi (model emitted
    XML-style pseudo-tool-calls → silent empty-final-message death).
  - F2 (the big one): E122's rung-5 key-exact ACL diagnosis was a
    **Responses-API route property** — under chat-completions (Pi and
    mini) the model produced near-zero reasoning tokens and confident
    wrong theories; `--thinking high` changed nothing (probe).
  - F3: the rung-3 30-min envelope failure reproduced 8/8 attempts —
    real, not a Codex artifact — though at-cap trees were consistently
    further along than Codex's (typecheck-green + tests vs broken).
  - F3b: 4/8 rung-3 attempts hit the same repo-idiom trap (mockReport
    hoisted-mock) across harnesses — model-prior, not harness.
  - **Verdict:** milestone 2 = Pi extensions, NOT a scratch loop. Two
    mandatory work items recorded: (1) re-probe E122 rung-5 through Pi's
    `api: "openai-responses"` before designing the supervision tier;
    (2) non-native tool-syntax detector/retry extension.
- `ledger.md` — every dispatch, verification, Spark state change,
  incident (pnpm workspace-walk pollution [reverted]; doubled-serve
  launch failure [fixed, incl. in dashboard]).
- Candidate diffs: LOCAL-ONLY never-merge branches
  `bd-tuxlink-7raoe/m2a-{pi,mini}-{cn-r3,q122-r3,e122-r5}` (ladder-arm
  pattern). Worker sdd forensics: `.claude/worktree-archives/
  bd-tuxlink-7raoe-m2a-*-sdd-forensics-*.tar.gz` (this machine).

## Spark dashboard (operator-requested mid-session, explicit deploy consent)

- Live: `https://inference.twin-bramble.ts.net:8443/` (tailnet-only).
  Source + git repo: `gx10-65aa:~/serving/spark-dashboard` (NOT in this
  repo). systemd unit `spark-dashboard.service`, enabled at boot.
- Status card, GPU/thermal/RAM/disk tiles, and a serialized profile
  switcher (CN + Q122 validated; NS120/GPT-OSS/Mistral marked
  experimental, never yet launched). Switch control live-tested by the
  CN restore (~4.5 min, clean).

## State at close

- **Spark:** serving `qwen3-coder-next` (as-found, verified via
  /v1/models + dashboard). `vllm-q122` container now EXISTS stopped
  (was removed at ladder close) — future swaps are `docker start`-cheap
  or one dashboard click.
- **Harness installs (this Pi, outside repo):**
  `~/.local/share/m2a-harnesses/{pi,node22,smoke}`; mini-swe-agent 2.4.5
  via `uv tool` (with fastapi+orjson injected); `MSWEA_CONFIGURED=true`
  in `~/.config/mini-swe-agent/.env`.
- **Worktrees:** only `worktrees/bd-tuxlink-7raoe-m2a-spike` (this
  branch) — dispose after PR merge. All 6 worker worktrees disposed per
  ADR 0009.
- **bd:** `tuxlink-7raoe` stays in_progress (M2a done; M2 next). Results
  comment added. The M1 note about the S5 harvest backlog (gac1d 1-line
  production fix etc.) remains OUTSTANDING from the previous handoff.
- **Rung-5 real bug:** still unfixed on main — the spike re-confirmed
  `src-tauri/capabilities/stations.json` lacks `core:event:allow-emit`
  (bd tuxlink-gac1d). Trivial, high-value, untouched by this spike per
  contamination rules.

## Next session

1. Read this handoff + `report.md` §Verdict.
2. Decide M2 scope with the operator: Pi-extension track (Responses-route
   re-probe first — it is cheap: one pi-e122-r5 run with an extension
   registering `api: "openai-responses"` against OpenRouter).
3. Consider harvesting tuxlink-gac1d (the capabilities fix) into a real
   PR — it ships user value and is now triple-confirmed by experiment.
