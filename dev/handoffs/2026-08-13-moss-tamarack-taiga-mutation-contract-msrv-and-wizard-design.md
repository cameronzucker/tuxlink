# Handoff — moss-tamarack-taiga, 2026-08-13 (mutation contract landed; MSRV executed; wizard designed)

COMPACTION ANCHOR #2 for this session. Read before acting; do not re-derive.
Predecessor anchor: `2026-08-12-moss-tamarack-taiga-tool-surface-fixes-and-compiler-epic.md`.

## 1. MERGED to main since the last anchor

**PR #1343 → `b0ee8290`** — the routines form capability, WHOLE and operator-testable:
save a filled form as a draft in compose → the designer's `@draft:` chip (label +
form shown, UUID inserted) → validate/enable → the run stages a REAL form (XML
attachment via `forms/outbound.rs`, runtime fill of datetime/msgsender/position,
position through `effective_broadcast_locator`). Deletion refuses loudly at resolve
AND validate. Also in it: `tuxlink-bekbh`, the locator privacy fix — the
`grid_square` caller parameter is GONE everywhere (Tauri, MCP, DTO); envelope
locator derived; visible Grid Square pre-fills from `broadcast_grid`.
`tuxlink-3ddk2` + `tuxlink-bekbh` CLOSED. Spun out: `tuxlink-q7q4w` (drift notice
when a referenced draft changed after authoring — substrate shipped, surface not).

**PR #1344 → `e67eba4c`** — mutation-contract slice (a): `Disposition { Invalid,
Denied, Unavailable, Service, Internal }` on `StepError::Action` via constructors at
every real mint site; journaled additively; predicates `is_author_attributable` /
`is_retryable`; `WritePortError::Unavailable` + `classify_write_err` at the MCP
boundary. ONE deliberate wire-text change: unavailable now reads "unavailable right
now (not your call's fault; retry later): …". `tuxlink-2tdmi` CLOSED. Design
correction recorded: `Denied` has a real mint site (automatic-child consent-digest
refusal in `session.rs`) — gates do not only park.

## 2. IN FLIGHT

**PR #1345 (`bd-tuxlink-qt7zi/msrv-floor-1-95` @ `2345d154`)** — MSRV executed per
the operator-confirmed convergence, number corrected by measurement (Codex said
1.89; 1.89 does not compile). `[workspace.package] rust-version = "1.95"`, all 16
members inherit (cargo metadata verified), new `msrv` CI job (check+test at exactly
1.95.0, NO clippy at the pin), CLAUDE.md/AGENTS.md/docs updated with parity.
PROVEN: full workspace suite at 1.95.0 on R2 = 5215 passed, 0 failed, 71 suites.
**NEXT SESSION: merge on CI green (standing grant), then close `tuxlink-qt7zi`.**
Watch the first `msrv` job run — `dtolnay/rust-toolchain@stable` with
`toolchain: "1.95.0"` input (input overrides ref per its docs); if the job
misbehaves, that assumption is the first suspect.

## 3. EPIC STATE (the operator asked; answer stands)

- **Testable today, whole**: the routines draft flow above (PR #1343).
- **Mutation contract (`tuxlink-fb0hc`)**: slice (a) of ~3 landed. The
  operator-VISIBLE half — slice (b), `routines_save` returning an artifact-derived
  plain-language readback beside the digest — is NOT built. Recon on the epic notes:
  the digest half ALREADY EXISTS (`store.rs::revision_of` = sha256 of stored bytes,
  returned on every save, D7 CAS binds edits to it). (b) reduces to the renderer +
  returning/surfacing it. THE SUMMARY'S WORDING IS OPERATOR-FACING — pin with him
  before it ships (substrate-first rule). Seam-refinement backlog on the epic.
- **Compiler epic (`tuxlink-s3h20`)**: not started, DELIBERATELY blocked on
  `tuxlink-10iw0` (bench measurement). Do not start it around the block.
- **Classifier program (`tuxlink-efk3k`/`tuxlink-13ofm`)**: hosting substrate merged
  (#1342); wizard DESIGNED this session, NOT built. Decisions on `tuxlink-13ofm`
  notes: GitHub Release assets = default source (DECIDED); download = first-class
  persistent job, wizard shows inline + "continue setup" explicit, Elmer panel is
  the gate surface, notification on ready (redesign after operator rejected
  "background + progress somewhere"); sideload ships v1 IFF digest pinning lands
  (sha256 per file pinned IN the app release; verification content-based,
  transport-irrelevant — this answers his unsigned-USB-weights objection; he has
  NOT yet ratified the sideload conclusion).

## 4. STANDING DEBTS

- **Bench re-measure** (`tuxlink-0rc3h` context): 123/405 units hit a rejected tool
  call pre-fixes; unmeasured since. Needs its own session ROOTED IN
  `~/Code/tuxlink-bench`, the pin moved, and note: the `unavailable:` wire text and
  the manifest/lock mismatch recorded there. The disposition field means the bench
  can now READ our classification instead of reconstructing it.
- `tuxlink-izcq0` dependency vulns ("P1, not today"), `tuxlink-q7q4w` drift notice,
  mini_sbc future-incompat note (in qt7zi notes, unowned).

## 5. ENVIRONMENT NOTES

- **The shell cwd silently resets to the MAIN CHECKOUT (operator state, branch
  `bd-tuxlink-ant8s/ardop-connect-fixes`) repeatedly.** It burned four commands
  this session (a checkout git refused, a wrong-tree `git add`, a wrong-tree
  recon, a wrong-branch `gh pr create` that GitHub refused). RULE: `cd` into the
  worktree explicitly at the top of every mutating command; `pwd`-verify before
  git writes; pass `--head` to `gh pr create` always.
- Session worktree: `worktrees/bd-tuxlink-efk3k-classifier-arch`, currently on
  `bd-tuxlink-qt7zi/msrv-floor-1-95`. Prior branches merged-dead in place.
- R2 scratch: `~/tux3ddk2` (verify clone; branch `msrv-probe` = merged main +
  MSRV manifests untarred over it; one stash "slice-a copies; merged upstream" —
  disposable). `~/msrv-check` = the OTHER session's clone with ITS dirty state:
  DO NOT TOUCH. Warm shared target: `~/msrv-check/src-tauri/target` (stable) and
  `~/msrv-check/target-195` (1.95).
- R2 verify recipe: `env CARGO_TARGET_DIR=~/msrv-check/src-tauri/target cargo
  {clippy,test} --manifest-path src-tauri/Cargo.toml -j 6 --locked
  -- --skip native_read_state_tests` (the skip is the known R2-only KISS hang).
- `claude.ai Google Drive` MCP connector wants re-auth (claude.ai settings);
  nothing current needs it.

## 6. CAUTIONS EARNED THIS SESSION

- Verify counts BY NAME: a "3736 passed" that equals the pre-change total was two
  different suites coinciding; the new tests were confirmed by grepping their names.
- The consent-gate finding: authority machinery that PARKS in attended mode can
  REFUSE in automatic mode — check both modes before claiming a class has no
  mint site.
- The bench's outcome discipline mapped onto the product cleanly BECAUSE the
  substrate was already half-shaped for it (revision = sha256, transmit closure
  digests). Recon before design shrank every slice.
