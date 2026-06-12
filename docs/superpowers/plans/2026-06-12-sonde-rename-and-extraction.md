# Sonde Rebrand + Repo Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan operation-by-operation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebrand the unwired modem workspace `tuxmodem/` (6 crates) to **Sonde** (from *radiosonde* — Software-Optimized Narrowband Data Exchange), in place within tuxlink first, then extract it to its own history-preserving repo before the alpha.

**Architecture:** Two-phase, never combined (combining a rename with a history-preserving split makes the diff un-bisectable). **Phase A** renames in place on a tuxlink branch and merges normally. **Phase B** runs `git subtree split` on the renamed `sonde/` prefix to seed a new repo with history, wires that repo's own CI (the real build/test gate, which tuxlink CI never provided), then removes `sonde/` from tuxlink. Each operation is independently self-adrev'd (Codex) before the next.

**Tech Stack:** Rust (Cargo workspace, 6 member crates), `git mv` + scoped `sed`, `git subtree split`, GitHub.

**Operator decision (Option A):** the *whole* workspace is Sonde. `tux-rig-*` → `sonde-rig-*`. Rationale: dependency trace shows `tux-rig` is consumed **only** by the Sonde TX path (`tuxmodem-tx` → `tux-rig-rts`, spawns `tux-rig-watchdog`); `src-tauri/` has **zero** references to it, so the app loses nothing.

---

## Reference Map (research deliverable — authoritative as of `origin/main`)

### Crates (6) and binaries

| Family | Crate (old → new) | Binaries (old → new) |
|---|---|---|
| Modem | `tuxmodem-phy` → `sonde-phy` | `tuxmodem-audio-play` → `sonde-audio-play` |
| Modem | `tuxmodem-fec` → `sonde-fec` | — (lib only) |
| Modem | `tuxmodem-rx` → `sonde-rx` | `tuxmodem-rx` → `sonde-rx` |
| Modem | `tuxmodem-tx` → `sonde-tx` | `tuxmodem-tx` → `sonde-tx` |
| Rig/PTT | `tux-rig-cm108` → `sonde-rig-cm108` | `tux-rig-cm108` → `sonde-rig-cm108` |
| Rig/PTT | `tux-rig-rts` → `sonde-rig-rts` | `tux-rig-rts` → `sonde-rig-rts`; `tux-rig-watchdog` → `sonde-rig-watchdog` |

Directory renames: `tuxmodem/` → `sonde/`; each `tuxmodem/crates/<old>` → `sonde/crates/<new>`.

### Token substitutions (case- and separator-aware; both tokens are whole-word, no partial-word collisions)

```
tuxmodem   → sonde      tuxmodem_  → sonde_      tuxmodem-  → sonde-
Tuxmodem   → Sonde      TuxModem   → Sonde       TUXMODEM   → SONDE
tux-rig    → sonde-rig  tux_rig    → sonde_rig   TuxRig     → SondeRig   (verify TuxRig/SondeRig actually occurs before applying)
```

### Files touched, by category

- **A — Inside workspace (56 files):** all of `tuxmodem/**` — Cargo.tomls (crate names, member list, path-deps, `[[bin]]` names+paths), `.rs` source/tests/examples (`use tuxmodem_phy`, `tux_rig_rts::`), per-crate READMEs/LICENSE headers, `tuxmodem/README.md`, `tuxmodem/Cargo.toml`.
- **B — Live code/scripts outside workspace (8 files):**
  - `src-tauri/src/modem_status.rs`, `src-tauri/src/winlink/modem/mod.rs`, `src-tauri/src/winlink_backend.rs` — 3 passing comments only (no code dep).
  - `scripts/tuxmodem-loopback-smoke.sh` — content **and** filename → `scripts/sonde-loopback-smoke.sh`.
  - `hf-channel-sim/Cargo.toml`, `hf-channel-sim/src/lib.rs` — attribution string `tuxmodem contributors` → `sonde contributors` (hf-channel-sim itself is NOT renamed/moved in Phase A).
  - `README.md` (root) — Architecture section describing the workspace + CLIs.
  - `CHANGELOG.md` — **do not** rewrite past version entries; add ONE new entry (Op 4).
- **C — Live docs (21 files):** `docs/superpowers/specs/*clean-sheet-modem*` (overview + 1,3,4,5,7,8), `docs/superpowers/plans/*clean-sheet-modem*` (1,3,4,5,6,7,8), `docs/superpowers/specs/2026-06-04-alpha-logging-design.md`, `docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md`, `docs/superpowers/plans/2026-05-27-ardop-mvp-transport.md`, `docs/design/ardop-deployment-findings.md`, `docs/research/modem-foundations.md`, `docs/hardware/bench-rig-two-host-topology.md`. **ADR 0015** is handled specially (Op 4) — decision records are not rewritten.

### Explicitly NOT touched (forensic / historical record)

- `dev/handoffs/**` (9 files incl. 3 with `tuxmodem` in the filename) — dated session logs.
- Past `CHANGELOG.md` version entries.
- ADR 0015 body — gets a forward-pointing supersession note only; the rename gets its own new ADR.

### Preconditions / known facts

- **No in-flight modem work:** worktrees `i3bz/tuxmodem-tx`, `xvrb/tuxmodem-rx`, `ixjb/readme-tuxmodem` are clean, 0 ahead of `origin/main`, bd issues CLOSED. No conflict hazard.
- **tuxlink CI does not build this workspace** (`tuxmodem/` is not a root-workspace member; no workflow references it). ⇒ Phase A's gate is **structural** (no stale refs + manifests resolve); full build/test is the **Phase B** new-repo CI gate. Honors the no-cold-cargo-on-the-contended-Pi rule.
- **Work happens in worktree** `worktrees/bd-tuxlink-twx0-sonde-rename` (branch `bd-tuxlink-twx0/sonde-rename`, off `origin/main`) — the main checkout is 1353 commits behind and on an unrelated handoff branch.

---

## Phase A — Rename in place (merges into tuxlink `main`)

All paths below are relative to the worktree root.

### Op 1: Rename the workspace (dirs + crate names + identifiers + bins)

**Files:** all of `tuxmodem/**` (becomes `sonde/**`).

- [ ] **Step 1: Move directories with git (preserves history/rename detection)**

```bash
cd worktrees/bd-tuxlink-twx0-sonde-rename
git mv tuxmodem sonde
git mv sonde/crates/tuxmodem-phy  sonde/crates/sonde-phy
git mv sonde/crates/tuxmodem-fec  sonde/crates/sonde-fec
git mv sonde/crates/tuxmodem-rx   sonde/crates/sonde-rx
git mv sonde/crates/tuxmodem-tx   sonde/crates/sonde-tx
git mv sonde/crates/tux-rig-cm108 sonde/crates/sonde-rig-cm108
git mv sonde/crates/tux-rig-rts   sonde/crates/sonde-rig-rts
```

- [ ] **Step 2: Rename the binary source files**

```bash
git mv sonde/crates/sonde-phy/src/bin/tuxmodem-audio-play.rs sonde/crates/sonde-phy/src/bin/sonde-audio-play.rs
git mv sonde/crates/sonde-rx/src/bin/tuxmodem-rx.rs           sonde/crates/sonde-rx/src/bin/sonde-rx.rs
git mv sonde/crates/sonde-tx/src/bin/tuxmodem-tx.rs           sonde/crates/sonde-tx/src/bin/sonde-tx.rs
git mv sonde/crates/sonde-rig-cm108/src/bin/tux-rig-cm108.rs  sonde/crates/sonde-rig-cm108/src/bin/sonde-rig-cm108.rs
git mv sonde/crates/sonde-rig-rts/src/bin/tux-rig-rts.rs      sonde/crates/sonde-rig-rts/src/bin/sonde-rig-rts.rs
git mv sonde/crates/sonde-rig-rts/src/bin/tux-rig-watchdog.rs sonde/crates/sonde-rig-rts/src/bin/sonde-rig-watchdog.rs
```

- [ ] **Step 3: Substitute all tokens inside the workspace**

```bash
# Order: rig tokens first, then modem tokens. Both are whole-word; no overlap.
grep -rlZ -e 'tuxmodem' -e 'Tuxmodem' -e 'TUXMODEM' -e 'TuxModem' -e 'tux-rig' -e 'tux_rig' sonde/ \
  | xargs -0 sed -i \
    -e 's/tux-rig/sonde-rig/g' \
    -e 's/tux_rig/sonde_rig/g' \
    -e 's/TuxModem/Sonde/g' \
    -e 's/Tuxmodem/Sonde/g' \
    -e 's/TUXMODEM/SONDE/g' \
    -e 's/tuxmodem/sonde/g'
```

- [ ] **Step 4: Structural gate — no stale refs, manifests consistent**

```bash
# (a) zero stale references in the workspace:
git grep -in -e tuxmodem -e tux-rig -e tux_rig -- sonde/ ; echo "exit=$?  (want: no output, exit=1)"
# (b) every workspace member + [[bin]] path exists:
for m in sonde-phy sonde-fec sonde-rx sonde-tx sonde-rig-cm108 sonde-rig-rts; do
  test -f "sonde/crates/$m/Cargo.toml" && echo "OK member $m" || echo "MISSING $m"
done
# (c) manifest resolves (skip if registry index is cold/offline — (a)+(b)+self-adrev still gate it):
cargo metadata --no-deps --offline --manifest-path sonde/Cargo.toml --format-version 1 >/dev/null && echo "metadata OK" || echo "metadata needs network — rely on (a)+(b)+adrev"
```

Expected: (a) no output, (b) all OK, (c) `metadata OK` or the documented fallback.

- [ ] **Step 5: Self-adrev (Codex) on the Op 1 diff**

```bash
cat > /tmp/codex-op1.txt <<'EOF'
Adversarial review of an in-place crate rename. Run `git diff origin/main..HEAD` in this
worktree. The change renames a Cargo workspace from `tuxmodem`/`tux-rig` to `sonde`/`sonde-rig`.
Audit ONLY for rename-correctness defects: (1) any surviving `tuxmodem`/`tux-rig`/`tux_rig`
identifier, path, or string; (2) Cargo `[[bin]]` name/path or workspace-member path that no
longer points at an existing file; (3) a `use`/`extern crate`/path that references an old crate
name; (4) a doc-comment code example that would now fail to compile. Output findings as markdown.
EOF
cat /tmp/codex-op1.txt | npx --yes @openai/codex review - 2>&1 | tee dev/adversarial/2026-06-12-sonde-op1-codex.md
wc -l dev/adversarial/2026-06-12-sonde-op1-codex.md   # >5 lines ⇒ real review, not an argparse stub
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(sonde): rename tuxmodem workspace + crates to Sonde

Whole-workspace rebrand per tuxlink-twx0 Option A: tuxmodem-* → sonde-*,
tux-rig-* → sonde-rig-*. Dir/bin renames via git mv (history preserved);
identifier/string substitution workspace-wide. No behavior change.

Agent: pine-arroyo-delta
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Op 2: Live cross-references (app comments, smoke script, sim attribution, root README)

**Files:** `src-tauri/src/{modem_status.rs,winlink/modem/mod.rs,winlink_backend.rs}`, `scripts/tuxmodem-loopback-smoke.sh` (+rename), `hf-channel-sim/{Cargo.toml,src/lib.rs}`, `README.md`.

- [ ] **Step 1: Rename + update the smoke script**

```bash
git mv scripts/tuxmodem-loopback-smoke.sh scripts/sonde-loopback-smoke.sh
sed -i -e 's/tux-rig/sonde-rig/g; s/tux_rig/sonde_rig/g; s/tuxmodem/sonde/g; s/Tuxmodem/Sonde/g' scripts/sonde-loopback-smoke.sh
```

- [ ] **Step 2: Update the 3 src-tauri comments + sim attribution + root README**

```bash
sed -i -e 's/tuxmodem/sonde/g' \
  src-tauri/src/modem_status.rs src-tauri/src/winlink/modem/mod.rs src-tauri/src/winlink_backend.rs
sed -i -e 's/tuxmodem contributors/sonde contributors/g' hf-channel-sim/Cargo.toml hf-channel-sim/src/lib.rs
# README.md: hand-edit (prose) — update the workspace path (tuxmodem/ → sonde/), crate list, and CLI names.
```

Edit `README.md` by hand (Read it first): replace `tuxmodem/` paths, the 6 crate names, the binary names, and any `tux-rig` mentions. Prose, so no blind sed on the human-facing description.

- [ ] **Step 3: Gate — live refs clean (workspace + handoffs excluded)**

```bash
git grep -in -e tuxmodem -e tux-rig -e tux_rig -- \
  src-tauri/ scripts/ hf-channel-sim/ README.md ; echo "exit=$? (want: empty)"
```

- [ ] **Step 4: Self-adrev (Codex)** — same pattern as Op 1 Step 5, prompt scoped to "stale references / broken script bin paths in src-tauri, scripts, hf-channel-sim, README", tee to `dev/adversarial/2026-06-12-sonde-op2-codex.md`.

- [ ] **Step 5: Commit** (`refactor(sonde): update live cross-references to renamed workspace` + trailers).

### Op 3: Live design docs

**Files:** the 21 docs listed in the Reference Map (category C) **except ADR 0015** (Op 4).

- [ ] **Step 1: Substitute across the doc set (excluding ADR 0015 + handoffs)**

```bash
git grep -lZ -e tuxmodem -e tux-rig -e tux_rig -- 'docs/**' ':!docs/adr/0015-modem-integration-and-rig-control-foundation.md' \
  | xargs -0 sed -i -e 's/tux-rig/sonde-rig/g; s/tux_rig/sonde_rig/g; s/TuxModem/Sonde/g; s/Tuxmodem/Sonde/g; s/TUXMODEM/SONDE/g; s/tuxmodem/sonde/g'
```

These dated plans/specs describe *unbuilt forward* subsystems (`sonde-arq`, `sonde-link`, `sonde-host-proto`, `sonde-linkadapt`) that will be created under the new name, so they are live forward-governing docs, not history.

- [ ] **Step 2: Gate** — `git grep -in -e tuxmodem -e tux-rig -- 'docs/**' ':!docs/adr/0015*' ':!dev/handoffs'` empty.
- [ ] **Step 3: Self-adrev (Codex)** on the docs diff (scope: broken internal links, renamed-doc cross-refs, stale crate names), tee to `dev/adversarial/2026-06-12-sonde-op3-codex.md`.
- [ ] **Step 4: Commit** (`docs(sonde): rename modem references in live design docs` + trailers).

### Op 4: New ADR + CHANGELOG entry + parity check

**Files:** `docs/adr/0019-sonde-rebrand-and-extraction.md` (new), `docs/adr/0015-*.md` (one-line note), `CHANGELOG.md`, AGENTS.md/CLAUDE.md (parity check only).

- [ ] **Step 1: Write ADR 0019** — decision record: the rebrand (name + *radiosonde* etymology + SONDE backronym), Option A boundary (whole workspace incl. `sonde-rig-*`, with the dependency-trace evidence), the two-phase in-place-then-extract approach, the crates.io name reservation (`sonde-*` free; bare `sonde` taken → prefixed names), and the pending extraction (Phase B). Supersede the naming portion of ADR 0015.
- [ ] **Step 2: ADR 0015** — add a single italic note at the top: *Superseded in part by ADR 0019: the `tuxmodem`/`tux-rig` crates were renamed to `sonde`/`sonde-rig`. The integration/rig-control decisions here still stand.* Do not rewrite the body.
- [ ] **Step 3: CHANGELOG** — one new top entry under the appropriate version: `Renamed the (unreleased, unwired) modem workspace from tuxmodem to Sonde.` Leave historical entries untouched.
- [ ] **Step 4: Propagation-contract + AGENTS.md parity check** — confirm no CLAUDE.md/AGENTS.md *rule* references `tuxmodem` (verified: none do). Record "parity check: no rule-bearing reference; no update needed" in the PR body.
- [ ] **Step 5: Commit** (`docs(adr): ADR 0019 — Sonde rebrand + extraction; supersede 0015 naming` + trailers).

### Phase A close

- [ ] Push `bd-tuxlink-twx0/sonde-rename`; open PR to `main`. PR body: summary, the 4 ops, the structural-gate rationale (CI doesn't build the workspace), self-adrev dispositions, parity-check note.
- [ ] Merge per project policy (`--merge --delete-branch`, no squash per ADR 0010).
- [ ] `bd update tuxlink-twx0` — note Phase A done; Phase B pending.

---

## Phase B — Extract to its own repo (after Phase A merges)

Runs from an up-to-date tuxlink checkout that contains the renamed `sonde/`. New bd issue (e.g. `… extract Sonde to own repo`), new worktree.

- [ ] **Op B1 — History-preserving split.** `git subtree split --prefix=sonde -b sonde-extract` (filter-repo/filter-branch are hook-banned; `subtree split` is allowed and preserves the `sonde/` subtree history). Verify the branch builds the workspace at its root.
- [ ] **Op B2 — Resolve the `hf-channel-sim` path-dep.** `sonde/Cargo.toml` has `hf-channel-sim = { path = "../hf-channel-sim" }`, which breaks once `sonde/` is a repo root. Decide (operator): (a) move `hf-channel-sim` into the Sonde repo as a workspace member, or (b) publish `hf-channel-sim` to crates.io and switch to `version = "0.1"`. It is a dev-dependency used only by the modem ⇒ option (a) is the low-friction default.
- [ ] **Op B3 — Create the GitHub repo** `cameronzucker/sonde` (AGPL-3.0-only; the workspace already carries its own LICENSE). **Confirm with operator before creating** (outward-facing). Push `sonde-extract` as `main`.
- [ ] **Op B4 — Add CI to the new repo** — `cargo build --workspace` + `cargo test --workspace` + `cargo clippy --all-targets -D warnings`. This is the build/test gate that tuxlink CI never provided; it retroactively validates the Phase A rename by compiling the renamed workspace for the first time.
- [ ] **Op B5 — Remove `sonde/` from tuxlink.** It was unwired (no root-workspace membership, no `src-tauri` dep), so removal is a clean delete + a pointer in the README/ADR 0019 to the new repo. Update the 3 `src-tauri` comments to reference the external repo if useful. Commit, PR, merge.
- [ ] **Op B6 — Reserve crates.io names** (optional, when ready to publish): `sonde-phy`, `sonde-fec`, `sonde-rx`, `sonde-tx`, `sonde-rig-cm108`, `sonde-rig-rts` (all free as of 2026-06-12).

---

## Self-Review (writing-plans checklist, run against this plan)

1. **Spec coverage:** Every reference category from the inventory maps to an op — workspace (Op 1), live code/scripts/sim/README (Op 2), live docs (Op 3), ADR/CHANGELOG/parity (Op 4), extraction + the hf-channel-sim path-dep + CI gap (Phase B). ✅
2. **Placeholder scan:** No "TBD"/"handle edge cases"; every step has the exact command or a named hand-edit with its scope. ✅
3. **Name consistency:** old→new pairs are fixed in the Reference Map table and reused verbatim in every op (`sonde-phy`, `sonde-rig-rts`, `sonde-rig-watchdog`, etc.). The `sed` token list matches the variants found by `git grep` (`tuxmodem`, `Tuxmodem`, `TUXMODEM`, `TuxModem`, `tux-rig`, `tux_rig`). ✅
4. **Gate honesty:** Phase A cannot fully compile (CI gap + no-cold-cargo); its gate is explicitly structural, with the real build/test gate deferred to Phase B CI and called out as such. ✅
