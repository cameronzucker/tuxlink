# Handoff — 2026-07-14 — `sumac-magnolia-fen` — dependabot sweep, whisper-rs 0.16, release freeze

Continuation of the same session as
[`2026-07-13-sumac-magnolia-fen-elmer-knowledge-tier-ft8-wwv-mcp.md`](2026-07-13-sumac-magnolia-fen-elmer-knowledge-tier-ft8-wwv-mcp.md)
(Elmer knowledge tier + FT-8/WWV MCP surface — all merged). This doc covers what
happened after it.

**Everything below is merged to main. No PRs of mine are open. Zero dependabot PRs open.**

## 1. RELEASE FREEZE IS ACTIVE — read this first

`.github/RELEASE_FREEZE` is on main (PR #1116), at operator request.

- **STOPS:** release-please opening/updating the release PR; the nightly `release-merge`
  cron merging + tagging.
- **DOES NOT STOP:** feature-branch PRs merging to main as normal.

**Why:** Routines is a six-part plan landing in pieces (1/6 core engine and 3/6
validator+dry-run landed; 2/6 `tuxlink-ofw4s` and 4/6 `tuxlink-oiigb` in progress;
5/6 and 6/6 not filed). **v0.91.0 already shipped carrying parts of it.**

Routines is not an ordinary half-feature: it **schedules RF actions** — scheduler, radio
arbiter, three-layer validator, Part 97 consent-closure. Shipping the scheduling half
ahead of its guards is the wrong half to ship first.

Freezing now was cheap because Routines is still **inert in the shipped binary** — the
crate is present but `routines` appears **zero** times in `src-tauri/src/lib.rs`, so no
Tauri mount, no UI surface. **Part 2/6 IS the Tauri mount.** The moment it lands the
feature becomes partially *reachable*.

**Do NOT lift the freeze** until all six parts land and Routines is wire-walked end to
end. Per ADR 0022, completeness is an invariant — "most of Routines" is not a reason to
unfreeze. The file itself says so.

## 2. Dependabot: all five resolved

Only ONE of five was an actual dependency problem. Worth internalizing before trusting a
red X.

| PR | Bump | What was actually wrong | Outcome |
|---|---|---|---|
| #1103 | alsa 0.9→0.12 | **Nothing.** All checks passed; read as UNSTABLE only because a *publish* job legitimately skips | merged |
| #1095 | rmcp 2.1→2.2 | **Infra.** "Rust tests" died with no failing test and no compile error — log stops mid-compile (killed runner). R2 ran 3162/3162 clean | re-run → merged |
| #1102 | tsx 4.23.0→4.23.1 | **Stale branch**, not a real conflict — main moved under it | `@dependabot rebase` → merged |
| #1096 | pbf 4→5 | **Real break** (below) | closed + ignore rule #1107 |
| #1110 | whisper-rs 0.14→0.16 | **Real break** (below) | migrated in #1112 → merged |

### pbf (#1096 closed, ignore rule in #1107)

`pbf@5` is ESM-only and **removed the default export**, splitting `Pbf` into named
`PbfReader`/`PbfWriter`. Our **vendored** `src/vendor/protomaps-leaflet/index.js` does
`import Pbf from "pbf"` and constructs it → vite build fails outright.

**The trap, written into the ignore rule so nobody "tidies" it:** `package.json` pins
`pbf: ^4` while `@mapbox/vector-tile@3` itself depends on `pbf: ^5`. pnpm installs
**both** — the vendored bundle resolves to our pbf 4 (default export intact) and
vector-tile uses pbf 5 internally. Bumping our direct pin is exactly what collapses them
to one pbf 5 and breaks the bundle. **The `^4` pin is load-bearing, not stale.**

Remove the ignore only when protomaps-leaflet is re-vendored against pbf 5.

## 3. whisper-rs 0.16 (#1112) — this one repaired a live bug

0.16 reworked the segment/token API into borrowed objects; dependabot only bumped the
version, so `tuxlink-stt` failed to compile four ways
(`full_n_segments` no longer returns a Result; `full_get_segment_text` / `full_n_tokens` /
`full_get_token_prob` replaced by `get_segment(i) -> Option<WhisperSegment>`).

**The part that matters:** 0.16 exposes the per-segment **no-speech probability** that
0.14 did not — and our own code said so:

```rust
// NOTE: whisper-rs 0.14.4 exposes NO per-segment no-speech probability
// getter ... no_speech_prob is reported as 0.0 (neutral).
```

`is_confident()` gates on `no_speech_prob < 0.8 && avg_logprob > -0.8` and exists (per its
comment) to *"reject hallucinated transcripts from noise instead of emitting confident
nonsense."* With `no_speech_prob` hardcoded to `0.0`, **that clause was always true — half
the gate has been dead since it shipped.** It is the half that matters: WWV is decoded
off-air from a noisy HF channel. Now populated with the worst-case across segments.

**OPERATOR-VISIBLE BEHAVIOUR CHANGE.** The WWV decode can now **reject** transcripts it
previously accepted — a capture that used to return SFI/A/K may come back `no_copy`. That
is the gate working as designed. **If it over-rejects on real signal, the knob is the 0.8
threshold, not the getter.**

**CI cannot catch this:** `tuxlink-stt`'s `transcribes_fixture` test is `#[ignore]`d (needs
a ggml model + fixture WAV), so the real transcription path is never exercised. Operator
plans to validate on air when weather permits the delta loop to go up.

## 4. r2-poe is a Rust compile box — USE IT

Operator surfaced this mid-session after I had burned several 15-minute CI round-trips on
trivial compile errors. Recipe is in memory (`project_r2_rust_compile_box.md`).

```bash
rsync -az --delete --exclude node_modules --exclude target --exclude .git --exclude .beads \
  --exclude worktrees --exclude .local  <worktree>/  r2-poe:~/build/<name>/

ssh r2-poe 'export PATH=~/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH
cd ~/build/<name>
cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml --locked'
```

**Two traps:**
1. `/usr/bin/cargo` on R2 is the **distro 1.75** and CANNOT parse modern dep manifests
   (`idna_adapter` declares `edition = "2024"`); it fails with a misleading *"failed to
   download"*. The **rustup 1.96 toolchain is installed but not on `$PATH`**.
2. **CI lints with `--workspace`.** Without it, member crates' **test targets go
   unlinted** — a local run is a false green. That is exactly where the `E0422` in the WWV
   branch was hiding.

Clippy is ~1-3 min warm. This turns a 15-min CI round-trip into ~2 min.

## 5. Known flake

`verify (arm64) -> Frontend tests (vitest)` failed on PR #1116, whose entire diff is a
single non-code marker file. main at the same base SHA is green with the same suite, and
the failing job's log was **empty** (killed runner — this repo has a known vitest
worker-leak). Merged over it. **Expect this to recur on unrelated PRs.** Worth a bd issue
if it gets noisy.

## Worktree state

All nine of my worktrees are **clean** (no tracked-dirty, no untracked):
`dep-rmcp`, `dep-pbf`, `dep-alsa`, `dep-tsx`, `dep-whisper`, `dep-fix`, `whisper-mig`,
`freeze`, `handoff-tmp`. Safe to dispose per ADR 0009. R2 has build scratch under
`~/build/tuxlink-*`, `~/build/dep-*`, `~/build/whisper-mig` — pure build output, safe to
delete.

Note the repo has ~271 worktrees registered overall, most predating this session.

## Open

- **`tuxlink-0mudm` (P0)** — stays open deliberately. Its tool half (`docs_read`) and
  prompt half (grounding clause) shipped in PR #1091. Its clause (4) **gold-gen coverage
  gap** in `dev/elmer-distill/` did NOT (no PRODUCT/HELP question family). Closing it
  would misreport that.
- **`tuxlink-lc4k6` (P3)** — `docs_read` has no router-level test; `MockSearch::doc`
  always returns `Some`, so the unknown-slug steering branch is untestable at the tool
  boundary. Not a correctness defect.
- **Qwen3-Coder-Next run** — the one thing no test can prove: does a small model actually
  CHAIN `docs_search` -> `docs_read` rather than answering from the 12-token snippet? The
  tool descriptions carry that protocol deliberately. Unverified against a live model.

Agent: sumac-magnolia-fen
