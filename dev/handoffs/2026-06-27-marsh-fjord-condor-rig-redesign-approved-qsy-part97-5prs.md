# Handoff: rig-control redesign spec APPROVED + QSY Part-97 fix + 5 PRs

**Date:** 2026-06-27 · **Agent:** marsh-fjord-condor

## TL;DR
Long session. Shipped/teed-up **5 PRs** (2 merged, 3 ready for operator merge, 1 closed-as-zombie), found + mitigated a **Part 97 / radio-safety violation** in the shipped QSY-on-fail, and **brainstormed + got operator approval on a rig-control panel redesign spec**. The next session's main job: **turn the approved spec into a plan and implement it** (writing-plans → subagent-driven-development). Do NOT re-brainstorm — the spec is approved.

## Merge queue — operator merges (no-squash, ADR 0010); agents never merge
- **#907** `bd-tuxlink-5xxq/ardop-connect-fail-terminate` — ARDOP failed-connect self-terminate. Was left draft inadvertently by a prior session; investigated, marked ready, resolved the post-#922 conflict (keep-both ArdopRadioPanel), CI green, MERGEABLE/CLEAN.
- **#929** `bd-tuxlink-wxwlr/mcp-cat-levers` — MCP CAT levers (rig_status, config_get_rig, rig_tune on the existing armed-egress gate, freq parity on ardop_connect/vara_b2f). Codex caught a **P1 transmit-bypass** (read-only rig_status spawned a command-capable rigctld) — remediated. CI green, ready.
- **#935** `bd-tuxlink-qevsf/qsy-part97-mitigation` — **SAFETY/Part 97**, EXPEDITE. `verify` green both arches (build-linux finishing at handoff). See below.
- Already MERGED this session: **#922** (VARA rig parity + 5 fixes), **#927** (receiving-end XSS/RCE audit + js_escape hardening).
- **#933 CLOSED unmerged** (tuxlink-9grg APRS tab desync) — operator flagged it a zombie/unconfirmed issue; backed out, parked 9grg at P4 "reproduce-first." Branch retained.

## The Part 97 finding (do not lose this)
The shipped QSY-on-fail (#922) walked an ordered candidate list and CAT-tuned + keyed on frequencies the operator **never saw or authorized** — a Part 97 control-operator violation + safety hazard. Confirmed against the **decompiled WLE artifact** (`library-of-hamexandria/winlink-re/decompiled/RMS Express`): WLE's `AllowHFAutoConnect = serviceCode AND (IsSHARES OR WDTcallsign)` — HF auto-connect is gated to non-amateur SHARES/dev authority; a Part 97 ham picks each channel from a visible frequency list (`HFChannelSelector`). Our auto-QSY was *more permissive than WLE*.
- **#935 mitigation:** clamp the connect-candidate list to the operator-selected first element in both ARDOP + VARA connect paths; QSY checkbox removed from UI. (Keep WLE-decompiled specifics OUT of the public repo — CUI rule.)
- **Full resolution = a SEPARATE spec (not yet written):** make **Find a Station the operator-driven Channel Selection** (ranked channels shown WITH frequencies, operator selects each) — WLE parity. Tracked under tuxlink-qevsf. This is the compliant way to restore multi-station calling.

## NEXT SESSION'S MAIN WORK — implement tuxlink-31c63 (rig-control redesign)
**Spec APPROVED:** `docs/superpowers/specs/2026-06-27-rig-control-panel-redesign.md` (on this branch, `bd-tuxlink-31c63/rig-panel-redesign`, worktree `worktrees/bd-tuxlink-31c63-rig-panel-redesign`).
- Brainstorm is DONE + approved — go straight to **writing-plans → subagent-driven-development**. Do not re-open the design.
- Design in one line: one collapsible **Radio & audio** group (all fields equal-weight, no "Advanced"); **full-hamlib radio picker** via `rigctl -l` grouped+searchable with **NO personal pins**; **CAT-port picker** (detected ports); **documented per-radio pre-fill applied only to non-overridden fields** (persisted override set); **Tune inline** with Frequency; **QSY removed**.
- **Resolved on approval:** VARA inherits the shared rig-config pickers (same fields, no behavior change, no VARA-specific work). The audio/PTT rows are ARDOP-only.
- **Hard constraints:** PRODUCT not personalized — never ship the operator's radio/settings as pins/defaults (see memory `feedback_personal_setup_is_not_product_default`). No auto-QSY (Part 97). On-air validation is operator-only (RADIO-1) on the operator's **G90+Digirig+VARA** path (memory `project_rig_test_path_g90_digirig_vara`) — that's *his validation rig*, NOT a product binding.
- Depends on #935 landing (so main is compliant first).

## Follow-ups filed
- **tuxlink-qevsf** — full Part-97-compliant multi-station (Find a Station = Channel Selection); needs its own spec.
- Safe live `rig_status` VFO readout (the wxwlr rig_status reports config-only after the P1 fix removed the unsafe probe).

## Operating notes / lessons (this session)
- **Cold worktrees can't compile Rust** here, but a warm `target/` (e.g. the 8fkkk worktree, warmed by an earlier Codex `cargo check`) let me run `cargo clippy --all-targets` + `cargo test` locally to batch CI failures. Fresh worktrees → CI is the only Rust gate. Frontend always verifies locally (tsc + vitest).
- `clippy --all-targets` compiles `src-tauri/tests/` — a wide trait/struct change must update every impl + mock + caller + test literal (bit me 3×).
- Codex cross-provider adrev earned its keep twice (VARA abort-window bug; the MCP P1 transmit-bypass) — run it for security/RF surfaces.
- Don't pick up unconfirmed/"not reproduced" bd issues + build speculatively (the 9grg zombie). Don't turn operator FYI into scope (the VARA-redesign overreach). Memories written: `feedback_verify_bd_issue_is_real_before_fixing`, `feedback_personal_setup_is_not_product_default`.
- nvye + 3ij0v reconciled as resolved-by-#922.

## Worktree inventory (per ADR 0009)
- **Active/keep:** `bd-tuxlink-31c63-rig-panel-redesign` (next session implements). `bd-tuxlink-5xxq`, `bd-tuxlink-wxwlr`, `bd-tuxlink-qevsf` — keep until their PRs (#907/#929/#935) merge, then dispose.
- **Disposable now** (PRs merged/closed): `bd-tuxlink-8fkkk` (#922 merged), `bd-tuxlink-2590` (#927 merged), `bd-tuxlink-9grg` (#933 closed). All have node_modules + gitignored `.beads/embeddeddolt/`-class content; branches are on origin. Dispose via the ADR 0009 ritual when convenient.
- All worktrees clean (no uncommitted tracked changes at handoff). Visual-companion brainstorm server stopped.

## Branch/tree state
Everything committed + pushed. main has #922 + #927. Three feature branches pushed + ready (#907/#929/#935). The 31c63 redesign branch has only the approved spec committed (no implementation yet).
