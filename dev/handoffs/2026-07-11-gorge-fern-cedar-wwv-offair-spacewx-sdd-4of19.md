# Handoff — off-air WWV space-weather decode (tuxlink-xscum), SDD 4/19

- **Agent:** gorge-fern-cedar
- **Date:** 2026-07-11
- **bd:** tuxlink-xscum (in_progress)
- **Branch:** `bd-tuxlink-xscum/wwv-offair-spacewx` (off `origin/main`) — **draft PR #1074**
- **Worktree:** `worktrees/bd-tuxlink-xscum-wwv-offair-spacewx`

## What this session did

Brainstormed → designed → spec'd → planned → began subagent-driven execution of the
off-air WWV/WWVH space-weather decode feature (decode the NOAA SWPC bulletin from the
WWV :18 / WWVH :45 voice broadcast via the primary radio; internet-free; feeds the
propagation engine). RX-only (RADIO-1 does not gate).

### Grounding corrections made along the way (important — the design got more correct)
1. First Explore ran against the **stale feature-branch checkout** and wrongly reported
   rig control / propagation / solar didn't exist. **Always ground on `git grep origin/main`**,
   not the ambient checkout. On `origin/main` the rig-control (`tux-rig`), propagation
   engine, and `parse_wwv`/`derive_ssn_from_sfi`/`apply_rf_solar_reply` all exist.
2. **Engine compatibility verified:** the predict path reads `ssn-forecast.json` fresh per
   call; WWV SFI → `derive_ssn_from_sfi` → forecast → VOACAP `SUNSPOT` is real, tested logic.
   BUT `apply_rf_solar_reply` has **no caller** and there is **no update command / frontend** —
   so this feature builds the first command + UI, not "reuse an existing surface."
3. **whisper-rs has no GBNF grammar** — use `set_initial_prompt` biasing + post-parse
   enforcement (`parse_wwv` + SFI sanity bound + retry). DecodeMode variant is `WwvBiased`.
4. **`release_serial()` STOPS rigctld** — restore requires re-spawn; gated on
   `config.rig.close_serial_sequencing` (FT-710 class vs DRA-100).

### Artifacts (committed + pushed)
- Spec: `docs/superpowers/specs/2026-07-11-wwv-offair-spacewx-design.md` (corrected).
- Plan: `docs/superpowers/plans/2026-07-11-wwv-offair-spacewx.md` (19 tasks, 8 phases).
- SDD ledger: `.superpowers/sdd/progress.md` (gitignored; the durable recovery map).

### Tasks complete + reviewed clean (committed & pushed through `f1577f20`)
- **Task 1** `feat(stt): scaffold tuxlink-stt crate` (`8a3a3c9d`) — DecodeMode/SttResult/SttError. `tuxlink-stt` is a real workspace member of `src-tauri/Cargo.toml` (that manifest IS a workspace; "not a workspace root" in CLAUDE.md means not the *repo* root).
- **build(deps)** `regenerate Cargo.lock` (`18ae7979`) — CI `--locked` rejected the stale lock; regen via `cargo fetch` (no compile). Adds whisper-rs 0.14.4, whisper-rs-sys 0.13.1, bindgen 0.71.1. **Any new Rust dep needs this.**
- **Task 2** `refactor(propagation): apply_rf_solar_indices(source)` (`c7758555`) — provenance seam; `apply_rf_solar_reply` delegates; existing 7 tests unchanged.
- **Task 3** `feat(stt): 16kHz mono f32 WAV loader` (`56ec2a6f`).
- **Task 6** `feat(wwv): spoken-number normalizer` (`f1577f20`) — new `wwv_offair` module wired via `pub mod wwv_offair;`. Controller merged the number-word match arms to dodge clippy `unnecessary_unwrap`.

### Key result: **whisper-rs native build VALIDATED**
On the CI run for `18ae7979`, `build ECT .deb (arm64, heif off)` **passed (13m50s)** — that
compiles the whole workspace incl. `whisper-rs-sys` → whisper.cpp **on arm64**. The single
biggest technical risk of the feature is cleared. (Push of `f1577f20` restarted CI; the
`verify` job — clippy + `cargo test` on the new crate — was still running at handoff. **Check
it.**)

## State
- Working tree: clean (all committed + pushed). No stashes. `.superpowers/sdd/` (ledger + briefs/reports) and `.claude/` are gitignored scratch on disk in the worktree.
- **This Pi cannot cold-build Rust** — CI is the compile/test gate. `pnpm vitest run <file>` works locally (frontend Tasks 14–16).
- **Worktree git mechanics:** commit with a **standalone `cd` into the worktree, then bare git** — the `cd && git ...` compound misfires the main-checkout hook (it doesn't persist cwd for the hook). Push works from there.

## Next — resume at Task 4
Per the plan, remaining: **4** (WhisperStt wrapper — FIRST verify whisper-rs 0.14.4's actual
method names: `full_get_segment_no_speech_prob` / `full_get_token_prob` may differ by version;
read the installed crate source), **5** (noise gate), **7** (scheduler), **8** (freq), **9**
(CaptureSource — **first check whether the existing `tuxlink-capture` crate already provides a
capture primitive to reuse** before building the `arecord` shell-out), **10** (capture_cycle),
**11** (config), **12** (commands — also update `SolarSnapshot::source` doc comment to include
`"rf-wwv-voice"`, a deferred Task-2 Minor), **13** (registration), **14–16** (frontend, vitest-
local), **17** (model fetch), **18** (**wire-walk hard gate** — operator supplies flows), **19**
(PR ready + Codex adversarial round).

Continue via `superpowers:subagent-driven-development`; the ledger says which tasks are done —
**do not re-dispatch them.**
