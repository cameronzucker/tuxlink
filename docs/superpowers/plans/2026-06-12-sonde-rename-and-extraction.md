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

## Phase B — Extract to its own repo

> **MUST run from a session rooted OUTSIDE tuxlink** (e.g. cwd `/home/administrator/Code/sonde`).
> Empirically confirmed 2026-06-12: tuxlink's session hooks block `git commit` by
> command pattern + main-checkout lease **regardless of cwd**, so building the new
> repo's commits from a tuxlink-rooted session is denied. This is the documented
> sibling-repo rule — relaunch in the new repo, do not worktree/lease/end-run.
> A fresh `git clone` of tuxlink does NOT inherit `core.hooksPath`, and a session
> rooted in the new repo does not load tuxlink's `.claude` session hooks, so git
> ops there are free.

> **History note (confirmed 2026-06-12):** `git subtree split --prefix=sonde` on the
> post-rename branch yields only **1 commit** — the modem's development history all
> lives under the old `tuxmodem/` path. To preserve it, split `--prefix=tuxmodem`
> from a pre-rename ref (`origin/main`, since Phase A may not be merged yet) and apply
> the rename in the new repo. The new repo's history then honestly shows
> tuxmodem→sonde. (`git filter-repo`, which could rewrite names through all history,
> is hook-banned.)

**Locked decisions:** full history (Option B above); `hf-channel-sim` moves into the
Sonde repo as a workspace member (it is a modem-only dev-dependency); new repo created
**private** first (reversible default — make public when ready).

Prereq: Phase A PR #639 merged (so `sonde/` is on `origin/main`) **and** its deferred
Codex self-adrev has run. The split below reads `origin/main`, so it works whether or
not #639 is merged, but Op B5 (removing `sonde/` from tuxlink) requires #639 merged.

```bash
# ---- Run from ~/Code in a Sonde-rooted (non-tuxlink) session ----
TUX=/home/administrator/Code/tuxlink

# B1 — full-history split of the modem (from a fresh clone so no tuxlink hooks bind)
git clone "$TUX" /tmp/sonde-src && cd /tmp/sonde-src
git checkout origin/main                                    # pre-rename: has tuxmodem/
git subtree split --prefix=tuxmodem -b modem-hist           # SLOW (~minutes); modem history, root-level, OLD names
git subtree split --prefix=hf-channel-sim -b hfsim-hist     # SLOW; hf-channel-sim history

# B2 — seed the new repo from the modem split, graft hf-channel-sim under hf-channel-sim/
mkdir -p /home/administrator/Code/sonde && cd /home/administrator/Code/sonde
git init -b main
git pull /tmp/sonde-src modem-hist                          # workspace at root (tuxmodem-* names)
git subtree add --prefix=hf-channel-sim /tmp/sonde-src hfsim-hist

# B3 — apply the rename AT ROOT (Phase A Op 1, minus the sonde/ prefix)
git mv crates/tuxmodem-phy crates/sonde-phy && git mv crates/tuxmodem-fec crates/sonde-fec
git mv crates/tuxmodem-rx crates/sonde-rx && git mv crates/tuxmodem-tx crates/sonde-tx
git mv crates/tux-rig-cm108 crates/sonde-rig-cm108 && git mv crates/tux-rig-rts crates/sonde-rig-rts
git mv crates/sonde-phy/src/bin/tuxmodem-audio-play.rs crates/sonde-phy/src/bin/sonde-audio-play.rs
git mv crates/sonde-rx/src/bin/tuxmodem-rx.rs crates/sonde-rx/src/bin/sonde-rx.rs
git mv crates/sonde-tx/src/bin/tuxmodem-tx.rs crates/sonde-tx/src/bin/sonde-tx.rs
git mv crates/sonde-rig-cm108/src/bin/tux-rig-cm108.rs crates/sonde-rig-cm108/src/bin/sonde-rig-cm108.rs
git mv crates/sonde-rig-rts/src/bin/tux-rig-rts.rs crates/sonde-rig-rts/src/bin/sonde-rig-rts.rs
git mv crates/sonde-rig-rts/src/bin/tux-rig-watchdog.rs crates/sonde-rig-rts/src/bin/sonde-rig-watchdog.rs
# token substitution (robust while-loop — NOT `grep -rlZ | xargs -0`, that mismatched newline/NUL in Phase A):
grep -rl -e tuxmodem -e Tuxmodem -e TUXMODEM -e TuxModem -e tux-rig -e tux_rig . \
  | while IFS= read -r f; do sed -i -e 's/tux-rig/sonde-rig/g; s/tux_rig/sonde_rig/g; s/TuxModem/Sonde/g; s/Tuxmodem/Sonde/g; s/TUXMODEM/SONDE/g; s/tuxmodem/sonde/g' "$f"; done
# fix the path-dep now that hf-channel-sim is in-repo, and add it as a workspace member:
sed -i 's#hf-channel-sim = { path = "../hf-channel-sim" }#hf-channel-sim = { path = "hf-channel-sim" }#' Cargo.toml
#   add "hf-channel-sim" to [workspace] members in Cargo.toml (hand-edit).

# B4 — structural gate + CI
git grep -in -e tuxmodem -e tux-rig -e tux_rig                         # want: empty
cargo metadata --no-deps --offline --format-version 1 >/dev/null && echo OK
#   write .github/workflows/ci.yml: cargo build --workspace, cargo test --workspace, cargo clippy --all-targets -D warnings
git add -A && git commit -m "refactor(sonde): rename to Sonde + vendor hf-channel-sim + CI"

# B5 — create the repo (PRIVATE) and push; CI runs the FIRST real compile/test of the renamed workspace
gh repo create cameronzucker/sonde --private --source=. --push --description "Sonde — clean-sheet HF data modem (AGPLv3)"
gh run watch   # the real build/test gate
```

- [ ] **Op B6 — Remove `sonde/` from tuxlink** (separate tuxlink PR, after #639 merges): delete `sonde/`, leave a README/ADR-0019 pointer to `github.com/cameronzucker/sonde`. Unwired ⇒ clean delete; `hf-channel-sim/` also leaves (now vendored in Sonde). New bd issue + worktree.
- [ ] **Op B7 — Reserve crates.io names** (optional, when publishing): `sonde-phy`, `sonde-fec`, `sonde-rx`, `sonde-tx`, `sonde-rig-cm108`, `sonde-rig-rts` (all free as of 2026-06-12).

---

## Self-Review (writing-plans checklist, run against this plan)

1. **Spec coverage:** Every reference category from the inventory maps to an op — workspace (Op 1), live code/scripts/sim/README (Op 2), live docs (Op 3), ADR/CHANGELOG/parity (Op 4), extraction + the hf-channel-sim path-dep + CI gap (Phase B). ✅
2. **Placeholder scan:** No "TBD"/"handle edge cases"; every step has the exact command or a named hand-edit with its scope. ✅
3. **Name consistency:** old→new pairs are fixed in the Reference Map table and reused verbatim in every op (`sonde-phy`, `sonde-rig-rts`, `sonde-rig-watchdog`, etc.). The `sed` token list matches the variants found by `git grep` (`tuxmodem`, `Tuxmodem`, `TUXMODEM`, `TuxModem`, `tux-rig`, `tux_rig`). ✅
4. **Gate honesty:** Phase A cannot fully compile (CI gap + no-cold-cargo); its gate is explicitly structural, with the real build/test gate deferred to Phase B CI and called out as such. ✅
