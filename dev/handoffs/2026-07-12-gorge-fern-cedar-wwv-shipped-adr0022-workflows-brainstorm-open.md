# Handoff — WWV shipped whole, ADR 0022 landed, workflows brainstorm OPEN at question 1

- **Agent:** gorge-fern-cedar
- **Date:** 2026-07-12 (session ran 2026-07-11)
- **Ended:** operator hit weekly usage limit mid-brainstorm; interrupted cleanly.

## READ THIS FIRST — where to resume

The next session resumes an **in-progress `superpowers:brainstorming` session** for a new
feature (**bd tuxlink-03d39: workflow engine**). It was interrupted at the *first clarifying
question*. Do **not** restart the brainstorm from scratch and do **not** jump to design/code.
Re-open the brainstorming skill and ask the operator exactly this (it is the right question,
operator-confirmed):

> **In your own words: what are the 2–3 concrete, repeatable things you picture an operator
> running as a workflow?** Real EmComm scenarios — the "every morning I do X, then Y, then Z"
> or "when a net starts I always..." kind of thing. Don't worry about blocks or UI yet; I want
> the jobs that hurt to do by hand today, so the engine gets designed around real usage instead
> of a generic canvas.

Then continue the normal brainstorming flow (one question at a time → 2-3 approaches → design
sections → spec doc → writing-plans).

## The workflows idea (bd tuxlink-03d39)

**Operator's words:** *"we should have workflows. Much in the same way as Laserfiche Workflow
or Clade [Claude] workflows with GUI blocks representing actions, repeatably invocable."*

**Context already gathered (don't re-scan):**
- **No existing workflow / automation / macro / scheduler surface** in tuxlink. This is net-new.
- **A machine-invocable action library already exists**: ~200 Tauri commands registered in
  `src-tauri/src/lib.rs` (`generate_handler!`), plus an **MCP tool surface** (`src-tauri/src/mcp_ports.rs`)
  and a full agent runner (`src-tauri/tuxlink-agent-runner/`, `tuxlink-agent-frontend/`, Anthropic +
  Ollama providers, `mcp_client.rs`). Workflow "blocks" could map onto that existing surface rather
  than inventing a parallel action catalog. This is the biggest architectural asset — start there.
- **Key constraint surfaced (not yet discussed with the operator):** any workflow step that
  **transmits** (connect / send) collides with the Part 97 per-run consent gate (RADIO-1, ADR 0018).
  "Repeatably invocable" + auto-transmit is a regulatory line. Transmit blocks will need operator
  consent at *execution* time. Design around it; don't let it kill the feature (RX-only and
  local-only blocks are unconstrained).
- Relates to the existing **Elmer** LLM-assistant program and the **voice field-assistant**
  frontier (memory: `project_elmer_program`, `project_voice_field_assistant_frontier`). Workflows =
  deterministic, repeatable sequences; Elmer = dynamic reasoning. Worth positioning against each other.
- Likely needs **decomposition** (execution engine + action model / GUI block builder / triggers +
  scheduling). Flag that to the operator once the jobs are known — do not decompose before knowing
  the jobs.

**Operator brainstorm preferences (CLAUDE.md + memory):** launch the **visual companion** for
visual questions (don't ask, just launch it — the block-canvas UX is inherently visual); token
budget is not a concern during design; ask open-ended, no option-menus; be decisive.

## What shipped this session (all merged to main, CI green both arches)

### 1. Off-air WWV/WWVH space-weather decode — **bd tuxlink-xscum, CLOSED, shipped WHOLE**
- **PR #1074** (implementation, merged `acd281b3`) + **PR #1079** (completion, merged `eb073f3e`).
- Decodes the NOAA SWPC bulletin off-air from the WWV (:18) / WWVH (:45) voice broadcast via the
  primary radio. Internet-free space weather; a real WLE gap.
- New: `tuxlink-stt` crate (whisper-rs, base.en q5_1, prompt-bias + RMS silence guard),
  `src-tauri/src/wwv_offair/` (capture, rig save/tune/restore orchestration, spoken-number
  normalizer, model resolution, commands), frontend `src/wwv/` (control in the Find-a-Station
  action row, :18/:45 window scheduling, arm/cancel, no-copy retry, **clip playback + manual
  entry**, provenance stamp), CSS, `scripts/fetch-stt-model.sh`, user-guide page 36.
- Reuses the existing propagation chain: `parse_wwv` → `derive_ssn_from_sfi` →
  `apply_rf_solar_indices` (`rf-wwv-voice` / `rf-wwv-manual`) → `ssn-forecast.json` → VOACAP.
- **Two Codex adversarial rounds** fixed real defects: a **P1 arbitrary-file-read** in the clip
  command (lexical path check → `canonicalize`), rig-not-restored-on-capture-error, the empty
  `arecord -D ""` default-config break, unbounded manual A/K, blob-URL race.
- **RX-only** (never keys TX; RADIO-1 does not gate it).

### 2. ADR 0018 number collision — resolved (**PR #1081**, merged `f15447a0`)
- Two ADRs claimed 0018. **RADIO-1 keeps 0018** (merged + propagated). The operator's
  *"Features are built whole: no arbitrary splitting, deferral, or delay"* ADR landed on main as
  **ADR 0022** (`docs/adr/0022-ban-autonomous-agent-issue-splitting-and-deferrals.md`), content
  faithful, with a numbering note. Propagated: `docs/adr/README.md` (also backfilled the missing
  0021 entry), one CLAUDE.md pointer, AGENTS.md parity.
- **OPERATOR ACTION:** when `bd-tuxlink-ant8s/ardop-connect-fixes` is next reconciled with main,
  **drop its `docs/adr/0018-ban-autonomous-agent-issue-splitting-and-deferrals.md`** — ADR 0022
  supersedes it.

## The one open validation (operator-only)

**Live `.deb` + on-air test of the WWV feature.** Only a licensed operator at a real radio can
prove the STT copes with noisy HF — no CI or reviewer can.

```bash
gh run download 29168943577 -n tuxlink-ect-arm64 -D ~/wwv-test-deb   # arm64, 87 MB, whole feature
sudo apt install ~/wwv-test-deb/*.deb                                 # debian trixie+
```
- STT model is **already provisioned** at `~/.local/share/tuxlink/models/ggml-base.en-q5_1.bin`
  (SHA-verified). On another machine: `bash scripts/fetch-stt-model.sh`.
- Set `wwv_offair.capture_device` in `config.json` to the radio's ALSA capture device (empty now
  falls back to the ALSA default).
- Drive it: **Find a Station → Refresh off-air** → arms for the nearest WWV :18 / WWVH :45 →
  tunes/captures/decodes (or prompts you to tune manually if CAT is unconfigured) → stamps the
  conditions bar. On no-copy you can play the clip and type the values.
- The STT **silence floor** (`SILENCE_FLOOR = 1e-4` in `tuxlink-stt/src/lib.rs`) is a data-gated
  constant, documented in-code: calibrate it against the first real on-air captures.

## Process lessons recorded (memory)

- **`feedback_features_built_whole_no_deferral`** (new): I first merged WWV with the CSS, the §9
  clip/manual-entry, and the §6.3 manual-tune fallback parked as follow-up bd issues, and called it
  shipped. The operator flagged it as an ADR violation. Correct. Buildable, spec'd pieces are part
  of the feature — never file them as follow-ups. The only legitimate "not now" is work genuinely
  gated on data that doesn't exist yet (e.g. the silence-floor constant, on-air validation).
- Reinforced: `Cargo.lock` must be regenerated (`cargo fetch`) for any new Rust dep or CI `--locked`
  fails; adding a field to a no-`Default` struct breaks every full struct literal repo-wide
  (14 sites here — `clippy --all-targets` on CI is the only thing that catches it); commit in a
  worktree with a **standalone `cd` then bare git** (the `cd &&` compound misfires the
  main-checkout hook).

## State

- Working tree clean; all WWV/ADR worktrees disposed and pruned. Branches merged-dead.
- bd: `tuxlink-xscum` CLOSED. `tuxlink-h9dpz` (ADR) CLOSED. **`tuxlink-03d39` (workflows) OPEN** —
  this is the next work.
- Three wrongly-filed WWV deferral issues (`tuxlink-qexuq`, `tuxlink-fcm6w`, `tuxlink-l0q50`) were
  CLOSED — their content was folded into the completion, per ADR 0022.
