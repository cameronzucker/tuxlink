# Handoff — 2026-07-05 (bog-kingfisher-delta)

Session shipped the NWS-catalog weather-report regression fix and repaired a
repo-wide CI outage. **Next up: the FT8-family feature (epic `tuxlink-u3m0g`).**

## ⭐ NEXT SESSION STARTS HERE: FT8-family feature

**Epic `tuxlink-u3m0g`** — "Find-a-Station: PSKReporter-trued fused propagation
reachability engine." Fuses three propagation truth sources into one per-gateway
reachability score: VOACAP (model) + PSKReporter (global live FT8/WSPR spots,
~88.7% FT8→Winlink reachability by SNR-proxy) + FT8 Radiosonde (local
TX-confirmed). Online-enrichment, offline-graceful.

- **5 shippable layers, 0/5 done. Start at L1:**
  - `tuxlink-u3m0g.1` — **L1: PSKReporter client** (rate-limited ≤1 query/5min,
    compressed, cached spot fetcher). ← begin here.
  - `.2` L2: Winlink-mode reachability proxy (FT8/WSPR SNR → required-SNR offset).
  - `.3` L3: Fused reachability scorer (VOACAP + PSKReporter + FT8-radiosonde).
  - `.4` L4: Infrastructure sensor (PSKReporter activity density → gateway lit/dark).
  - `.5` L5: Offline degradation + opt-in contribute-back to PSKReporter.
- **Design doc (READ FIRST):**
  `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-xygm-recover-handoffs-design-20260625-pskreporter-prop-truing.md`
- **HARD CONSTRAINT — PSKReporter API etiquette:** ≤1 query/5min, gzip
  compression, caching, attribution, optional contribute-back. Do not hammer it.
- Related: `tuxlink-s0r1` (configurable propagation model).
- **Gate:** this is a feature with an existing design doc. Read the design doc +
  `bd show tuxlink-u3m0g`, confirm scope/wedge with the operator, then build L1
  via `build-robust-features` / TDD. L1 is a pure fetch/cache client — testable
  with mocks/fixtures, no radio. CI is healthy (see below), so PRs will pass.

## What shipped this session (all merged to `main`)
- **`tuxlink-kfcwc` — NWS catalog SFT weather blank (PR #1008).** `parse_tabular`
  was fail-closed; `is_value_cell("")` rejected the empty "Today" low (`/104`)
  that every morning-issued SFT carries → whole forecast `Forecast::None` →
  blank report ("Show full text" only). One-line fix (allow empty cell, which the
  data model already documents). Real-data regression tests from the operator's
  N7CPZ inbox (pulled off the R2 over SSH: KPSR + KABQ fixtures).
- **`tuxlink-84vzn` — repo-wide CI outage (PRs #1009 + #1010).** 2026-07-05 the
  runner's baked apt index went stale (Ubuntu point-released `libnghttp2`, pulled
  the old `.deb`); the cache-apt-pkgs-action pins versions and never ran
  `apt-get update`, so the fetch 404'd and aborted the whole deps install → glib
  missing → every Rust build failed. Fixed by adding `apt-get update` before the
  deps step in **both** ci.yml (verify) and release.yml (build-linux) + salt bump.
  Earlier attempt (add packages) was wrong; the log corrected it.
- Earlier in the session: 3 Elmer fixes (#1002 — object-form tool-args for Gemini,
  detected-model save, error persistence), the **"New conversation" reset**
  feature (#1005, `tuxlink-vbv2k`), and **cut + promoted 0.80.0 to Latest**.

## ⚠️ PENDING DECISION: 0.80.1 not cut
The weather fix is on `main` but **not in any release** — `releases/latest` is
still **v0.80.0**, `version.txt` still 0.80.0. The operator has not decided
whether to cut 0.80.1. Options: (a) cut+promote 0.80.1 now (unfreeze not needed —
freeze is already off; `gh workflow run release-merge.yml` → tag → verify
artifacts → `promote-release.yml -f tag=vX`), or (b) let it ride the nightly
pre-release (won't be Latest until promoted). **The operator's install won't have
the weather fix until a release is cut.**

## Open follow-up issues filed this session
- `tuxlink-gag8u` (P2, feature): Elmer pre-warm the model KV cache on pane open
  (snappy first message for local 20b). Level-2 (llama.cpp slot save/restore) noted.
- `tuxlink-le3t2` (P2, feature): Elmer surface a "switch provider" affordance on a
  backend crash/500 (like the rate-limit callout).
- `tuxlink-fzj9a`/`erqzx`/`6ompo` — the 3 Elmer fixes (closed, in #1002).

## Operator's parallel thread (not blocking FT8)
Framework 13 ollama crashes `llama-server` (`exit 0xe06d7363`) on `gpt-oss:20b`
tool queries. NOT a Tuxlink bug — operator is investigating on their side. Root
cause is a stale apt index-class... no: it's a runner-side inference crash;
evidence-gathering paused (need the Framework `server.log` crash line; TDR ruled
out — no Display/4101 events). See memory `project_elmer_finetune_gptoss20b_direction`.

## Worktree state (mine — all merged/dead, safe to dispose per ADR 0009)
- `worktrees/bd-tuxlink-erqzx-elmer-model-switch` (PR #1002, merged)
- `worktrees/bd-tuxlink-vbv2k-elmer-newconv` (PR #1005, merged)
- `worktrees/bd-tuxlink-kfcwc-catalog-sft-empty-cell` (PR #1008, merged)
- `worktrees/bd-tuxlink-84vzn-ci-glib-deps` (PR #1009, merged)
- `worktrees/bd-tuxlink-84vzn-ci-glib-release` (PR #1010, merged)
- `worktrees/bd-tuxlink-rkxu9-handoff-20260705` (this doc — dispose after push)
All have no uncommitted tracked work beyond what's merged; `.beads/embeddeddolt/`
gitignored state is the only at-risk class (per ADR 0009). Many OLDER orphan
worktrees exist from prior sessions (not this session's — left as-is).

## Working tree / branch
`main` has all this session's merges (HEAD ~`675e0050` catalog merge +
`e1aff8be`/`cc55164c` CI fixes). The operator's main checkout is on branch
`bd-tuxlink-ant8s/ardop-connect-fixes` (their state — untouched).
