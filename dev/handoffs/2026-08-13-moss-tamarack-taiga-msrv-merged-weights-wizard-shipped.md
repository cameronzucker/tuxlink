# Handoff — moss-tamarack-taiga, 2026-08-13 (MSRV merged; classifier weights wizard shipped whole)

COMPACTION ANCHOR #3 for this session. Predecessor:
`2026-08-13-moss-tamarack-taiga-mutation-contract-msrv-and-wizard-design.md`.

## 1. MERGED to main this leg

**PR #1345 → `766e99ae`** — MSRV floor 1.95, `tuxlink-qt7zi` CLOSED. The first
`msrv` job run was log-verified installing rustc 1.95.0 (the
`dtolnay/rust-toolchain@stable` + `toolchain:` input override works as
documented). Second-order lesson worth keeping: **declared MSRV is an input to
clippy** — raising it made stable clippy demand the newly-permitted idioms
(`abs_diff`, `is_multiple_of`, `is_none_or`, `repeat_n`) across seven sites,
four of them in targets CI never reaches (its verify stops at the first failing
crate). Two converged `--all-targets` sweeps on R2 were the fix; a complete
MSRV change needs check+test at the pin AND stable clippy over the final tree.
One touched file (`elmer_battery.rs`) is the bench repo's vendored main.rs —
vendor drift noted for the bench pin move.

**PR #1346 → `16dc7105`** — the classifier weights feature (`tuxlink-13ofm`
CLOSED, whole per the operator's decisions):

- `tuxlink-classify/src/pins.rs`: sha256+bytes for every bge-small file,
  provenance = upstream git-lfs pointer digest cross-checked against R2's
  eval-producing copy. CHANGE POLICY in the module doc: never rotate a digest
  under an existing model id. `examples/print_pins.rs` feeds the same table to
  the release workflow.
- `src/classify_weights/` (app): ONE stage→hash→compare-to-pins→atomic-rename
  pipeline for GitHub/custom-URL/sideload (that identity IS the sideload
  security argument); persistent `.weights-job.json` + `.part` resume;
  class-aware failures (`network` auto-retries w/ capped backoff; `source`,
  `digest-mismatch`, `io`, `cancelled` do not); boot re-arm; desktop
  notification on completion.
- Wizard: new final step (`classifier_weights` after `vara_provision`),
  inline job + "Continue setup while it downloads" + first-class Skip.
  Elmer panel = post-wizard gate surface (same job, banner until ready).
- MCP: `classify_weights_status` / `classify_weights_download` /
  `classify_weights_cancel` (tool budget 92→95, agents guide + tool-surface
  corpus regenerated, enriched.rs consumer gate 92→95). Sideload import is
  UI-only; MCP exposure parked as `tuxlink-wvgon`.
- release.yml: tag-only `classifier-weights` job (fetch upstream → verify vs
  print_pins → attach assets); `release-assets-complete` now requires the
  three weights names. NOTE: the assets exist only from the NEXT release —
  a dev build's default URL 404s (Source-class failure with switch/sideload
  guidance; sideload + env-path cover dev).
- **Codex round (gpt-5.6 high): 4 P1 + 4 P2, ALL fixed** — mismatched finals
  quarantined (`.rejected`); skip-list abolished (every pass re-verifies);
  "digest-pinned" release-scoped; MCP download takes NO source URL (SSRF-1) and
  operator custom URLs fetch no-redirect + resolve-gated + address-pinned;
  size-gated hashing; corrupt job record = visible failure; failed job outranks
  ready in both surfaces; cancel persists immediately and beats Network
  classification. Transcript: `dev/adversarial/2026-08-13-classifier-weights-codex.md`
  (local-only).
- Verification at merge head `b38aa817`: R2 workspace 5248 passed / 0 failed,
  clippy `--all-targets -D warnings` clean; vitest 374 files / 4798 green;
  typecheck clean; all 9 PR checks green.

## 2. OPERATOR-PENDING (carried)

- **Sideload ratification**: he challenged unsigned USB weights; the shipped
  answer is digest pinning (content-based, transport-irrelevant). Built and
  merged UNDER that argument; his read is still owed. If he rejects sideload,
  the affordance is one button + one command to remove.
- **Mutation-contract slice (b)** (`tuxlink-fb0hc`): the `routines_save`
  plain-language readback. Wording proposals were delivered in the session's
  final message; PIN WITH HIM before building.
- **Bench re-measure**: paste-ready prompt for his bench-rooted agent was
  delivered in the final message (he relays; we can't cross repos).

## 3. BACKLOG STATE

- `tuxlink-efk3k` (classifier epic): OPEN — next seam is inference wiring
  (locator → CandleBert behind the ready-gate). `tuxlink-s3h20` (compiler)
  stays deliberately blocked on `tuxlink-10iw0`. `tuxlink-wvgon` (sideload
  MCP), `tuxlink-q7q4w` (drift notice), `tuxlink-izcq0` (dep vulns, 16 on
  dependabot) open.
- ADR 0030 threshold recalibration rides the bench re-measure (corpus grew
  92→95).

## 4. ENVIRONMENT

- Session worktree `worktrees/bd-tuxlink-efk3k-classifier-arch`, branch
  `bd-tuxlink-13ofm/classifier-weights-wizard` (merged-dead) at handoff time;
  the handoff itself rides `agent-moss-tamarack-taiga/handoff-3`.
- R2 `~/tux3ddk2` (disposable scratch clone): branch `wizard-build` @
  `b38aa817`, THREE stashes accumulated (untarred msrv manifests; two
  "(committed upstream)" lock/corpus copies) — all safe to drop with the clone.
  `~/msrv-check` still belongs to the other session: DO NOT TOUCH (its target
  dirs remain the warm shared CARGO_TARGET_DIR).
- **The shell cwd resets to the MAIN CHECKOUT mid-session, repeatedly** (three
  more times this leg). `cd <worktree> &&` on every mutating command; `pwd` +
  branch check before git writes; `--head` on `gh pr create`. Also: one edit
  round landed on the wrong BRANCH (wizard branch checked out before the MSRV
  fix was written) — carried across by dirty-tree checkout; check
  `git branch --show-current` before editing, not just before committing.
