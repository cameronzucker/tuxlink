# Rig-control panel redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the unusable shipped rig control (hand-typed CAT port, FT-710-only model list, inert "CAT backend" label, stacked checkboxes, a Part-97-non-compliant QSY checkbox) with one radio-centric "Radio & audio" surface: a runtime-queried searchable model picker, a detected-serial-port picker, an operator-settable data Mode, and override-respecting per-radio pre-fill.

**Architecture:** The shared `RigControlSection` component (rendered by both the ARDOP and VARA panels) is the single anchor — all rig-config improvements land there, so VARA inherits them with zero VARA-specific change (operator decision 2026-06-27: VARA inherits the shared pickers). The model list comes from a new `rig_list_models()` Tauri command parsing `rigctl -l` (no list for us to maintain). A bundled, documented profile table pre-fills per-radio known-good values, applied only to fields the operator has not overridden; overrides persist in an additive `Config.rig.rig_field_overrides` set. ARDOP additionally merges its audio + PTT rows and the rig-config rows into one collapsible group; VARA's group stays the rig-config rows only.

**Tech Stack:** Tauri 2.x · Rust backend (`src-tauri/`, crate `tux-rig` for CAT) · React 18 + TypeScript frontend (Vite, WebKitGTK) · vitest + @testing-library/react · `cargo test` / clippy in CI.

## Global Constraints

- **MSRV 1.75** — clippy `incompatible_msrv` is denied. Do NOT use `Result::inspect_err` (1.76), `Option::is_none_or` (1.82), etc. Use the pre-1.76 idioms (`if let Err(ref e) = r`, explicit matches).
- **clippy `-D warnings`** — use `slice.first()` not `.get(0)`, `x.is_some_and(..)` not `.map_or(false, ..)`, `io::Error::other(..)` not `Error::new(ErrorKind::Other, ..)`. No `#[allow]` without a one-line justification comment.
- **The dev Pi cannot finish a cold `cargo` build/test locally.** Backend test "run" steps are gated by CI on a draft PR — "Expected" describes the CI result, not a local run. Frontend (`pnpm typecheck`, `pnpm vitest run <file>`) DOES run locally and must be run.
- **No auto-QSY.** This plan only *removes* the non-compliant QSY-on-fail control. It must NOT add any auto-frequency-change UI or behavior. `qsy_on_fail` stays in the DTO + backend walk (per the #935 mitigation); only its UI checkbox is removed.
- **Product, not personalized.** The model list, manufacturer grouping, and the profile table are objective product data (documented hardware behavior). No operator-preference radios shipped as pins or as defaults. No "popular"/curated list — full A–Z-by-manufacturer + search only.
- **VARA inherits, with zero VARA-specific or behavioral change.** Do not add/remove fields from VARA's rig group beyond what flows through the shared component. Do not touch VARA's audio/host/bandwidth rows.
- **Depends on #935 (QSY Part-97 mitigation) landing first** so `main` is compliant in the interim. Build proceeds on the `bd-tuxlink-31c63/rig-panel-redesign` branch regardless; the operator manages merge ordering.
- **Commit trailers:** every commit ends with `Agent: kestrel-esker-grouse` and the `Co-Authored-By:` trailer. Conventional-commit `type:` must match intent (`feat:` for new UI/commands, `refactor:` for the layout merge, `test:` only for test-only commits).
- **Worktree:** all work happens in `worktrees/bd-tuxlink-31c63-rig-panel-redesign/` on branch `bd-tuxlink-31c63/rig-panel-redesign`.

---

## File Structure

**New files:**
- `src-tauri/tux-rig/src/list.rs` — `RigModel` struct + `list_models()` (runs `rigctl -l`) + the column parser + unit tests.
- `src/radio/modes/rigProfiles.ts` — the bundled documented per-radio profile table + `getRigProfile()`.
- `src/radio/modes/rigProfiles.test.ts` — profile-table tests.

**Modified files:**
- `src-tauri/tux-rig/src/lib.rs` — `mod list; pub use list::{RigModel, list_models};`
- `src-tauri/src/config.rs` — add `data_mode` + `rig_field_overrides` to `RigUiConfig` (+ default fns, `Default` impl, round-trip tests).
- `src-tauri/src/modem_commands.rs` — new `rig_list_models` command + `RigModelDto`; swap the two `ardop_data_mode()` tune call sites to resolve `cfg.rig.data_mode`.
- `src-tauri/src/lib.rs` — register `rig_list_models` in `generate_handler!`.
- `src/radio/modes/RigControlSection.tsx` — the heart: model picker (from `rig_list_models`), CAT-port picker (from `packet_list_serial_devices`), Mode row, remove "CAT backend" label + QSY checkbox, pre-fill + override tracking, optional `variant="bare"` render mode + `onPttPrefill`/`onPttOverride` callbacks.
- `src/radio/modes/RigControlSection.test.tsx` — extend for all new behavior.
- `src/radio/modes/ArdopRadioPanel.tsx` — merge the "Radio" expander + the rig section into one "Radio & audio" group; mark `ptt_method` overridden on manual edit; pre-fill ptt on radio change.
- `src/radio/modes/ArdopRadioPanel.test.tsx` — assert the merged group + Tune-inline regression + ptt pre-fill/override.
- `src/radio/modes/VaraRadioPanel.tsx` — no behavioral change; verify it renders the redesigned shared section (regression test only).

---

## Task 1: Backend — `rig_list_models()` (parse `rigctl -l`)

**Files:**
- Create: `src-tauri/tux-rig/src/list.rs`
- Modify: `src-tauri/tux-rig/src/lib.rs` (add `mod list;` + re-export)
- Modify: `src-tauri/src/modem_commands.rs` (add `rig_list_models` command + `RigModelDto`)
- Modify: `src-tauri/src/lib.rs` (register command in `generate_handler!`)

**Interfaces:**
- Produces (tux-rig): `pub struct RigModel { pub id: u32, pub manufacturer: String, pub model: String }`; `pub fn list_models(rigctl_binary: &str) -> Result<Vec<RigModel>, RigError>`.
- Produces (Tauri command): `rig_list_models() -> Vec<RigModelDto>` where `RigModelDto { id: u32, manufacturer: String, model: String }` (serde). Returns `[]` on any failure (degrade-to-manual; never errors to the frontend).
- Consumes: `tux_rig::RigError` (existing), `std::process::Command`.

- [ ] **Step 1: Write the failing parser unit tests** in `src-tauri/tux-rig/src/list.rs`

```rust
//! `rigctl -l` model enumeration — the installed hamlib's supported rigs.
//!
//! The model list is queried at runtime from the installed hamlib rather than
//! maintained in tuxlink, so it is always accurate to the operator's hamlib.
//! Columns in `rigctl -l` are separated by runs of 2+ spaces; single spaces
//! appear only WITHIN a model name ("NET rigctl"), so a 2+-space column split
//! is robust. The header line's first column is "Rig #", which fails the u32
//! parse and is therefore skipped without a special case.

use std::process::{Command, Stdio};

use crate::RigError;

/// One supported rig as reported by `rigctl -l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigModel {
    pub id: u32,
    pub manufacturer: String,
    pub model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
 Rig #  Mfg                    Model                       Version         Status   Macro
     1  Hamlib                 Dummy                       20231112.0      Stable   RIG_MODEL_DUMMY
     2  Hamlib                 NET rigctl                  20231112.0      Stable   RIG_MODEL_NETRIGCTL
  1049  Yaesu                  FT-710                      20240514.0      Stable   RIG_MODEL_FT710
  3073  Icom                   IC-7300                     20231112.0      Stable   RIG_MODEL_IC7300
";

    #[test]
    fn parses_id_mfg_and_multiword_model() {
        let got = parse_rig_list(SAMPLE);
        assert_eq!(
            got,
            vec![
                RigModel { id: 1, manufacturer: "Hamlib".into(), model: "Dummy".into() },
                RigModel { id: 2, manufacturer: "Hamlib".into(), model: "NET rigctl".into() },
                RigModel { id: 1049, manufacturer: "Yaesu".into(), model: "FT-710".into() },
                RigModel { id: 3073, manufacturer: "Icom".into(), model: "IC-7300".into() },
            ],
        );
    }

    #[test]
    fn skips_header_and_blank_lines() {
        // Header line ("Rig #  Mfg ...") + a blank line produce no entries.
        assert!(parse_rig_list(" Rig #  Mfg  Model\n\n").is_empty());
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(parse_rig_list("").is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (CI)**

Run (CI, on the draft PR): `cargo test --manifest-path src-tauri/Cargo.toml -p tux-rig --locked`
Expected: FAIL — `parse_rig_list` not defined.

- [ ] **Step 3: Implement the parser + `list_models`** in `src-tauri/tux-rig/src/list.rs` (above the `#[cfg(test)]` block)

```rust
/// Split a `rigctl -l` row into columns on runs of 2+ spaces. Single spaces
/// are preserved inside a column (multi-word model names). Leading indentation
/// is ignored.
fn split_columns(line: &str) -> Vec<String> {
    let mut cols: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut space_run = 0usize;
    for ch in line.chars() {
        if ch == ' ' {
            space_run += 1;
        } else {
            if space_run >= 2 && !cur.is_empty() {
                cols.push(cur.trim().to_string());
                cur = String::new();
            } else if space_run == 1 && !cur.is_empty() {
                cur.push(' ');
            }
            space_run = 0;
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        cols.push(cur.trim().to_string());
    }
    cols
}

/// Parse one row into a [`RigModel`], or `None` if it is not a data row (header,
/// blank, or malformed). The header's first column "Rig #" fails the u32 parse.
fn parse_line(line: &str) -> Option<RigModel> {
    let cols = split_columns(line);
    let id: u32 = cols.first()?.parse().ok()?;
    let manufacturer = cols.get(1)?.clone();
    let model = cols.get(2)?.clone();
    if manufacturer.is_empty() || model.is_empty() {
        return None;
    }
    Some(RigModel { id, manufacturer, model })
}

/// Parse the full stdout of `rigctl -l` into the supported-model list.
fn parse_rig_list(stdout: &str) -> Vec<RigModel> {
    stdout.lines().filter_map(parse_line).collect()
}

/// Query the installed hamlib for its supported rig models by running
/// `<rigctl_binary> -l`. Returns the parsed list. Errors (binary missing,
/// non-UTF-8 output) map to [`RigError::Spawn`]; the Tauri command layer
/// converts any error to an empty list so the picker degrades to manual entry.
pub fn list_models(rigctl_binary: &str) -> Result<Vec<RigModel>, RigError> {
    let output = Command::new(rigctl_binary)
        .arg("-l")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| RigError::Spawn(format!("failed to run {rigctl_binary} -l: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_rig_list(&stdout))
}
```

- [ ] **Step 4: Wire the module into `tux-rig`** — in `src-tauri/tux-rig/src/lib.rs`, after the existing `mod managed;` / `pub use managed::...` block, add:

```rust
mod list;
pub use list::{list_models, RigModel};
```

- [ ] **Step 5: Add the Tauri command + DTO** in `src-tauri/src/modem_commands.rs`, immediately after `config_set_rig` (the `#[tauri::command] pub fn config_set_rig` block ends ~line 152):

```rust
/// Wire DTO for a hamlib-supported rig model (tuxlink-31c63). Field names are
/// single words, so serde's default (no rename) already matches the frontend's
/// `{ id, manufacturer, model }`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RigModelDto {
    pub id: u32,
    pub manufacturer: String,
    pub model: String,
}

/// List the installed hamlib's supported rig models for the radio picker
/// (tuxlink-31c63). Runs `rigctl -l` (the companion to `rigctld`) and parses
/// its table. Returns an empty list on ANY failure (rigctl absent, parse
/// empty) so the picker degrades to a manual hamlib-model-number entry rather
/// than erroring — there is no model list for tuxlink to maintain.
#[tauri::command]
pub fn rig_list_models() -> Vec<RigModelDto> {
    tux_rig::list_models("rigctl")
        .map(|models| {
            models
                .into_iter()
                .map(|m| RigModelDto {
                    id: m.id,
                    manufacturer: m.manufacturer,
                    model: m.model,
                })
                .collect()
        })
        .unwrap_or_default()
}
```

- [ ] **Step 6: Register the command** — in `src-tauri/src/lib.rs`, inside the `generate_handler!` macro, next to the existing rig commands (`config_get_rig` / `config_set_rig`, ~lines 1647–1648), add a line:

```rust
            crate::modem_commands::rig_list_models,      // tuxlink-31c63
```

- [ ] **Step 7: Run tests + clippy to verify pass (CI)**

Run (CI): `cargo test --manifest-path src-tauri/Cargo.toml -p tux-rig --locked` then `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`
Expected: PASS — 3 parser tests green; clippy clean (note `.first()` used, not `.get(0)`).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/tux-rig/src/list.rs src-tauri/tux-rig/src/lib.rs src-tauri/src/modem_commands.rs src-tauri/src/lib.rs
git commit -m "feat(rig): rig_list_models command parsing rigctl -l

Adds tux-rig::list_models + the rig_list_models Tauri command so the radio
picker is sourced from the installed hamlib at runtime (no maintained list).
Degrades to an empty list (manual entry) when rigctl is unavailable.

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Backend — extend `RigUiConfig` (`data_mode` + `rig_field_overrides`)

**Files:**
- Modify: `src-tauri/src/config.rs` (struct fields + default fns + `Default` impl + round-trip tests)
- Modify: `src-tauri/src/modem_commands.rs` (resolve `cfg.rig.data_mode` in the two tune call sites)

**Interfaces:**
- Produces: `RigUiConfig.data_mode: String` (default `"PKTUSB"`) and `RigUiConfig.rig_field_overrides: Vec<String>` (default `[]`). Both additive + `#[serde(default)]` → migration-free for configs that predate them.
- Consumes: `tux_rig::Mode::from_rigctl(&str) -> Option<Mode>` (existing), `ardop_data_mode()` (existing, becomes the fallback).

- [ ] **Step 1: Write the failing config round-trip test** in `src-tauri/src/config.rs` (in the existing `#[cfg(test)] mod tests`, near the existing `RigUiConfig` round-trip test ~line 2387)

```rust
    #[test]
    fn rig_ui_config_data_mode_and_overrides_round_trip() {
        let cfg = RigUiConfig {
            data_mode: "USB-D".to_string(),
            rig_field_overrides: vec!["cat_baud".to_string(), "ptt_method".to_string()],
            ..RigUiConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: RigUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data_mode, "USB-D");
        assert_eq!(back.rig_field_overrides, vec!["cat_baud", "ptt_method"]);
    }

    #[test]
    fn rig_ui_config_defaults_new_fields_when_absent() {
        // A config JSON that predates data_mode / rig_field_overrides fills both
        // from their #[serde(default)]s.
        let back: RigUiConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(back.data_mode, "PKTUSB");
        assert!(back.rig_field_overrides.is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails (CI)**

Run (CI): `cargo test --manifest-path src-tauri/Cargo.toml --locked rig_ui_config`
Expected: FAIL — `data_mode` / `rig_field_overrides` are not fields of `RigUiConfig`.

- [ ] **Step 3: Add the default fn** in `src-tauri/src/config.rs`, next to `default_cat_baud` (~line 958):

```rust
/// Default rig data mode token (the value rigctld's `M` command sets). PKTUSB
/// is the HF Winlink default and the proven FT-710 data mode. Backs
/// [`RigUiConfig`] (tuxlink-31c63).
fn default_data_mode() -> String {
    "PKTUSB".to_string()
}
```

- [ ] **Step 4: Add the two fields** to `pub struct RigUiConfig` (after the `cat_baud` field, ~line 1342):

```rust
    /// Rig data mode token (e.g. "PKTUSB", "USB-D") rigctld sets on tune. Default
    /// "PKTUSB". Parsed via `tux_rig::Mode::from_rigctl`; an unrecognised token
    /// falls back to the ardop default at tune time. (tuxlink-31c63)
    #[serde(default = "default_data_mode")]
    pub data_mode: String,
    /// Logical keys of profile-managed fields the operator has hand-edited, so a
    /// later radio change does NOT overwrite them with the new radio's profile
    /// value. Keys: "ptt_method", "data_mode", "cat_baud", "close_serial".
    /// Additive; empty by default. (tuxlink-31c63)
    #[serde(default)]
    pub rig_field_overrides: Vec<String>,
```

- [ ] **Step 5: Add the fields to the `Default` impl** (`impl Default for RigUiConfig`, ~line 1345), after `cat_baud: default_cat_baud(),`:

```rust
            data_mode: default_data_mode(),
            rig_field_overrides: Vec::new(),
```

- [ ] **Step 6: Resolve `data_mode` in the tune paths.** In `src-tauri/src/modem_commands.rs`, add a helper next to `ardop_data_mode()` (~line 1888):

```rust
/// Resolve the operator-configured rig data mode, falling back to the ardop
/// default when the persisted token is unrecognised (e.g. hand-edited config).
/// (tuxlink-31c63)
pub(crate) fn rig_data_mode(rig: &RigUiConfig) -> tux_rig::Mode {
    tux_rig::Mode::from_rigctl(&rig.data_mode).unwrap_or_else(ardop_data_mode)
}
```

Then change the two tune call sites to use it:
- In `tune_rig_for_connect` (~line 1928): `rig.tune(hz, ardop_data_mode())` → `rig.tune(hz, rig_data_mode(rig_cfg))` — use the function's `&RigUiConfig` parameter name (read the signature at ~line 1917; it is the `rig`/`rig_cfg` arg the function already takes).
- In `ardop_tune_rig` (~line 1985): `rig.tune(freq_hz, ardop_data_mode())` → `rig.tune(freq_hz, rig_data_mode(&cfg.rig))` (this fn reads `cfg` via `config::read_config()`; use `&cfg.rig`).

> Read both functions' bodies before editing so the borrow names match. Do not change any other behavior.

- [ ] **Step 7: Add a tune-mode resolution unit test** in `src-tauri/src/modem_commands.rs` tests (find the existing `#[cfg(test)] mod tests`):

```rust
    #[test]
    fn rig_data_mode_falls_back_on_unknown_token() {
        let mut rig = crate::config::RigUiConfig::default();
        rig.data_mode = "NONSENSE".into();
        assert_eq!(super::rig_data_mode(&rig), super::ardop_data_mode());
        rig.data_mode = "USB-D".into();
        assert_eq!(super::rig_data_mode(&rig), tux_rig::Mode::DataU);
    }
```

- [ ] **Step 8: Run tests + clippy to verify pass (CI)**

Run (CI): `cargo test --manifest-path src-tauri/Cargo.toml --locked rig_ui_config rig_data_mode` then clippy `-D warnings`.
Expected: PASS — round-trip + default + fallback tests green; clippy clean.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/src/modem_commands.rs
git commit -m "feat(rig): operator-settable data_mode + rig_field_overrides on RigUiConfig

data_mode (default PKTUSB) now drives the pre-audio tune mode instead of a
hardcoded constant; rig_field_overrides records hand-edited fields so a later
radio change won't clobber them. Both additive + serde(default) (migration-free).

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Frontend — bundled rig profile table

**Files:**
- Create: `src/radio/modes/rigProfiles.ts`
- Create: `src/radio/modes/rigProfiles.test.ts`

**Interfaces:**
- Produces: `RigProfilePttMethod` (union mirroring backend `PttMethod`), `RigProfile` (all fields optional), `RIG_PROFILES: Record<number, RigProfile>`, `getRigProfile(modelId: number | null | undefined): RigProfile | undefined`.
- Consumed by: `RigControlSection.tsx` (rig fields) and `ArdopRadioPanel.tsx` (ptt_method) in later tasks.

- [ ] **Step 1: Write the failing test** in `src/radio/modes/rigProfiles.test.ts`

```ts
import { describe, it, expect } from 'vitest';
import { getRigProfile, RIG_PROFILES } from './rigProfiles';

describe('rigProfiles', () => {
  it('returns the documented FT-710 profile (model 1049)', () => {
    const p = getRigProfile(1049);
    expect(p).toEqual({
      ptt_method: 'cat_command',
      data_mode: 'PKTUSB',
      cat_baud: 38400,
      close_serial_sequencing: true,
    });
  });

  it('returns undefined for an unprofiled model', () => {
    expect(getRigProfile(99999)).toBeUndefined();
  });

  it('returns undefined for null/unset', () => {
    expect(getRigProfile(null)).toBeUndefined();
    expect(getRigProfile(undefined)).toBeUndefined();
  });

  it('table is keyed by numeric hamlib model id', () => {
    expect(Object.prototype.hasOwnProperty.call(RIG_PROFILES, 1049)).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/radio/modes/rigProfiles.test.ts`
Expected: FAIL — cannot resolve `./rigProfiles`.

- [ ] **Step 3: Implement** `src/radio/modes/rigProfiles.ts`

```ts
// src/radio/modes/rigProfiles.ts
//
// Bundled, documented per-radio pre-fill profiles (tuxlink-31c63). Keyed by
// hamlib model id. This is OBJECTIVE PRODUCT DATA — documented known-good
// ARDOP/VARA settings for a radio (e.g. a radio whose internal codec resets
// when the CAT serial is held open during audio needs close-serial sequencing).
// It is NOT any operator's personal tuning, and carries no "popular"/preferred
// ranking. A model absent from this table simply gets no pre-fill.
//
// Criterion to add an entry: the value must be DOCUMENTED known-good for that
// radio's ARDOP/VARA operation (a hardware datasheet, a manufacturer note, or a
// reproduced-and-recorded on-air result). Do not guess.

/** Mirrors the backend `PttMethod` (config.rs / ArdopUiConfig.ptt_method). */
export type RigProfilePttMethod = 'vox' | 'serial_rts' | 'cat_command';

/** A per-radio pre-fill profile. Every field optional — only documented fields
 *  are present, and pre-fill skips any field the profile omits. */
export interface RigProfile {
  ptt_method?: RigProfilePttMethod;
  data_mode?: string;
  cat_baud?: number;
  close_serial_sequencing?: boolean;
}

/** model id → documented profile. */
export const RIG_PROFILES: Record<number, RigProfile> = {
  // Yaesu FT-710 (hamlib 1049): the internal SCU-LAN/codec resets if the CAT
  // serial is held open during audio, so it keys ONLY by CAT command and needs
  // close-serial sequencing. 38400 is the Enhanced-port default. Documented +
  // reproduced (project_ft710_internal_codec_tx_reset).
  1049: {
    ptt_method: 'cat_command',
    data_mode: 'PKTUSB',
    cat_baud: 38400,
    close_serial_sequencing: true,
  },
};

/** Look up a radio's profile by hamlib model id; undefined when unset or
 *  unprofiled. */
export function getRigProfile(modelId: number | null | undefined): RigProfile | undefined {
  if (modelId === null || modelId === undefined) return undefined;
  return RIG_PROFILES[modelId];
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm vitest run src/radio/modes/rigProfiles.test.ts`
Expected: PASS — all 4 tests green.

- [ ] **Step 5: Typecheck**

Run: `pnpm typecheck`
Expected: PASS — no type errors.

- [ ] **Step 6: Commit**

```bash
git add src/radio/modes/rigProfiles.ts src/radio/modes/rigProfiles.test.ts
git commit -m "feat(rig): bundled documented per-radio pre-fill profile table

Objective product data (documented known-good ARDOP/VARA settings), keyed by
hamlib model id; seeds the FT-710 close-serial/CAT-PTT profile. Unprofiled
models get no pre-fill. No curated/preferred ranking.

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Frontend — `RigControlSection` field redesign (pickers, Mode, removals)

This task replaces the hardcoded model `<select>`, the hand-typed CAT-port `<input>`, adds a Mode row, and deletes the inert "CAT backend" label + the QSY checkbox. No pre-fill yet (Task 5). The component keeps its own `<details>` expander (the `variant="bare"` mode is added in Task 6).

**Files:**
- Modify: `src/radio/modes/RigControlSection.tsx`
- Modify: `src/radio/modes/RigControlSection.test.tsx`

**Interfaces:**
- Consumes: `rig_list_models` (Task 1) returning `RigModel[]`; `packet_list_serial_devices` returning `SerialDeviceDto[]` (existing, `{ path, kind, label }`); `RigConfig` (extend the TS interface with `data_mode: string` + `rig_field_overrides: string[]` to mirror Task 2).
- Produces (DOM testids): `rig-model` (now a searchable select sourced from rig_list_models), `rig-model-refresh`, `rig-cat-port` (now a `<select>`), `rig-cat-port-manual` (manual fallback input), `rig-cat-port-refresh`, `rig-data-mode`, `rig-cat-baud`, `rig-close-serial`, `rig-live-vfo`. REMOVED: the `CAT backend` label row, `rig-qsy-on-fail`.

- [ ] **Step 1: Extend the `RigConfig` TS interface + default** (top of `RigControlSection.tsx`, the `export interface RigConfig` at lines 17–27 and `DEFAULT_RIG_CONFIG` at 29–39) — add the two fields mirroring Task 2:

```ts
export interface RigConfig {
  rig_hamlib_model: number | null;
  rigctld_host: string;
  rigctld_port: number;
  rigctld_binary: string;
  close_serial_sequencing: boolean;
  live_vfo_poll: boolean;
  qsy_on_fail: boolean;
  cat_serial_path: string | null;
  cat_baud: number;
  data_mode: string;
  rig_field_overrides: string[];
}
```

In `DEFAULT_RIG_CONFIG` add: `data_mode: 'PKTUSB',` and `rig_field_overrides: [],`.

- [ ] **Step 2: Write the failing tests** in `RigControlSection.test.tsx` (add to the existing `describe`; mock `rig_list_models` + `packet_list_serial_devices` in the `invoke` mock — follow the existing `config_get_rig` mock pattern)

```ts
  it('renders models from rig_list_models, grouped by manufacturer', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, rig_hamlib_model: null };
      if (cmd === 'rig_list_models') return [
        { id: 1049, manufacturer: 'Yaesu', model: 'FT-710' },
        { id: 3073, manufacturer: 'Icom', model: 'IC-7300' },
      ];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-model')).toBeInTheDocument();
    });
    // both manufacturers' models are options
    expect(screen.getByRole('option', { name: /FT-710/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /IC-7300/ })).toBeInTheDocument();
  });

  it('renders detected serial ports in the CAT-port picker', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [
        { path: '/dev/ttyUSB0', kind: 'usb', label: 'CP2102 USB-UART' },
      ];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-cat-port')).toBeInTheDocument();
    });
    expect(screen.getByRole('option', { name: /\/dev\/ttyUSB0/ })).toBeInTheDocument();
  });

  it('renders a Mode row bound to data_mode', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, data_mode: 'USB-D' };
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-data-mode') as HTMLSelectElement).value).toBe('USB-D');
    });
  });

  it('no longer renders the QSY-on-fail control or the CAT backend label', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    expect(screen.queryByTestId('rig-qsy-on-fail')).not.toBeInTheDocument();
    expect(screen.queryByText('CAT backend')).not.toBeInTheDocument();
  });
```

> Update `knownConfig` at the top of the test file to include `data_mode: 'PKTUSB'` and `rig_field_overrides: []` so it type-checks against the extended interface.

- [ ] **Step 3: Run tests to verify they fail**

Run: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx`
Expected: FAIL — new testids absent; QSY/CAT-backend still present.

- [ ] **Step 4: Implement the redesign** in `RigControlSection.tsx`. Read the current file (1–239) first. Make these changes inside the component:

(a) Add imports + state for the model list and serial list. After the existing `import { invoke } ...`:

```ts
import { useCallback, useEffect, useMemo, useState } from 'react';
```
(replace the existing `import { useEffect, useState } from 'react';`)

(b) Add a `RigModel` type near `RigConfig`:

```ts
/** Mirror of the backend RigModelDto from rig_list_models. */
interface RigModel {
  id: number;
  manufacturer: string;
  model: string;
}
```

(c) Inside the component, after the existing `catBaudInput` state, add:

```ts
  const [models, setModels] = useState<RigModel[]>([]);
  const [serialPorts, setSerialPorts] = useState<{ path: string; kind: string; label: string }[]>([]);

  const loadModels = useCallback(() => {
    void invoke<RigModel[]>('rig_list_models')
      .then((list) => setModels(list ?? []))
      .catch(() => setModels([]));
  }, []);
  const loadSerialPorts = useCallback(() => {
    void invoke<{ path: string; kind: string; label: string }[]>('packet_list_serial_devices')
      .then((list) => setSerialPorts(list ?? []))
      .catch(() => setSerialPorts([]));
  }, []);

  useEffect(() => {
    loadModels();
    loadSerialPorts();
  }, [loadModels, loadSerialPorts]);

  // Group + sort models by manufacturer for the picker (A–Z; no curated pins).
  const groupedModels = useMemo(() => {
    const byMfg = new Map<string, RigModel[]>();
    for (const m of models) {
      const arr = byMfg.get(m.manufacturer) ?? [];
      arr.push(m);
      byMfg.set(m.manufacturer, arr);
    }
    return [...byMfg.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([mfg, list]) => ({
        mfg,
        list: list.slice().sort((a, b) => a.model.localeCompare(b.model)),
      }));
  }, [models]);
```

(d) Replace the **Rig model** `<label>` block (current lines 130–144) with a `rigctl -l`-sourced picker + refresh + manual fallback. When `models` is empty, render a manual model-number input + a note (degrade path):

```tsx
      {/* Radio model — sourced from the installed hamlib via rig_list_models
          (rigctl -l), grouped by manufacturer, A–Z. No curated pins. Empty
          model list degrades to a manual hamlib-model-# entry. */}
      <label className="radio-panel-input-row">
        <span>Radio</span>
        {models.length > 0 ? (
          <select
            className="radio-panel-input"
            data-testid="rig-model"
            value={rigConfig?.rig_hamlib_model ?? ''}
            onChange={(e) => {
              const v = e.target.value;
              persistRig({ rig_hamlib_model: v === '' ? null : Number(v) });
            }}
          >
            <option value="">None / unset</option>
            {groupedModels.map((g) => (
              <optgroup key={g.mfg} label={g.mfg}>
                {g.list.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.manufacturer} {m.model} ({m.id})
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        ) : (
          <input
            type="text"
            inputMode="numeric"
            className="radio-panel-input"
            data-testid="rig-model-manual"
            value={rigConfig?.rig_hamlib_model ?? ''}
            placeholder="hamlib model # (rigctl unavailable)"
            spellCheck={false}
            onChange={(e) => {
              const n = Number(e.target.value.trim());
              persistRig({ rig_hamlib_model: Number.isInteger(n) && n > 0 ? n : null });
            }}
          />
        )}
        <button
          type="button"
          className="radio-panel-btn-sm"
          data-testid="rig-model-refresh"
          onClick={loadModels}
          aria-label="Refresh radio model list"
        >
          ↻
        </button>
      </label>
```

(e) Replace the **CAT port** text `<input>` block (current lines 148–162) with a detected-ports `<select>` + refresh + manual fallback (mirror the ARDOP PTT picker idiom at ArdopRadioPanel.tsx:1135–1184):

```tsx
      {/* CAT port — detected serial ports (reuses packet_list_serial_devices,
          the AX.25/PTT enumeration). Manual row covers an unlisted device. */}
      <label className="radio-panel-input-row">
        <span>CAT port</span>
        <select
          className="radio-panel-input"
          data-testid="rig-cat-port"
          value={serialPorts.some((d) => d.path === catSerialInput) ? catSerialInput : ''}
          onChange={(e) => {
            const next = e.target.value;
            setCatSerialInput(next);
            persistRig({ cat_serial_path: next === '' ? null : next });
          }}
        >
          <option value="">Choose serial port…</option>
          {serialPorts.map((d) => (
            <option key={d.path} value={d.path}>
              {d.path} — {d.label}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="radio-panel-btn-sm"
          data-testid="rig-cat-port-refresh"
          onClick={loadSerialPorts}
          aria-label="Refresh CAT serial port list"
        >
          ↻
        </button>
      </label>
      <label className="radio-panel-input-row">
        <span>Manual</span>
        <input
          type="text"
          className="radio-panel-input"
          data-testid="rig-cat-port-manual"
          value={catSerialInput}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder="/dev/ttyUSB0 (unlisted)"
          onChange={(e) => setCatSerialInput(e.target.value)}
          onBlur={commitCatSerial}
        />
      </label>
```

(f) Add a **Mode** row immediately after the CAT-baud row (after current line 180). The options are the six `Mode::rigctl_str()` tokens:

```tsx
      {/* Data mode — the token rigctld sets on tune (Mode::rigctl_str). */}
      <label className="radio-panel-input-row">
        <span>Mode</span>
        <select
          className="radio-panel-input"
          data-testid="rig-data-mode"
          value={rigConfig?.data_mode ?? 'PKTUSB'}
          onChange={(e) => persistRig({ data_mode: e.target.value })}
        >
          <option value="PKTUSB">PKTUSB</option>
          <option value="PKTLSB">PKTLSB</option>
          <option value="USB-D">USB-D</option>
          <option value="LSB-D">LSB-D</option>
          <option value="USB">USB</option>
          <option value="LSB">LSB</option>
        </select>
      </label>
```

(g) **Delete** the inert "CAT backend" row (current lines 182–188) and the **QSY on fail** `<label>` block (current lines 224–236) entirely. Leave `close_serial_sequencing` (rename its visible label from "Close-serial sequencing" to "Close serial during audio" per spec) and `live_vfo_poll` rows intact.

- [ ] **Step 5: Run tests + typecheck to verify pass**

Run: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx` then `pnpm typecheck`
Expected: PASS — model picker, serial picker, Mode row present; QSY + CAT-backend gone. No type errors.

- [ ] **Step 6: Commit**

```bash
git add src/radio/modes/RigControlSection.tsx src/radio/modes/RigControlSection.test.tsx
git commit -m "feat(rig): runtime model picker + serial CAT-port picker + Mode row

Sources the radio list from rig_list_models (grouped A-Z, manual fallback when
rigctl is absent); replaces the hand-typed CAT port with a detected-ports
picker; adds a data-mode selector; removes the inert CAT-backend label and the
QSY-on-fail checkbox (Part 97 — control removed, DTO field retained).

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Frontend — per-radio pre-fill + override tracking (shared rig fields)

Adds the override-respecting pre-fill for the fields `RigControlSection` owns: `data_mode`, `cat_baud`, `close_serial_sequencing`. (PTT-method pre-fill is ARDOP-only → Task 7.)

**Files:**
- Modify: `src/radio/modes/RigControlSection.tsx`
- Modify: `src/radio/modes/RigControlSection.test.tsx`

**Interfaces:**
- Consumes: `getRigProfile` (Task 3), `RigConfig.rig_field_overrides` (Task 4).
- Override keys used here: `"data_mode"`, `"cat_baud"`, `"close_serial"`. (ARDOP adds `"ptt_method"` in Task 7.)

- [ ] **Step 1: Write the failing tests** in `RigControlSection.test.tsx`

```ts
  it('pre-fills non-overridden rig fields when a radio is selected', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, rig_hamlib_model: null, rig_field_overrides: [] };
      if (cmd === 'rig_list_models') return [{ id: 1049, manufacturer: 'Yaesu', model: 'FT-710' }];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '1049' } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({
            rig_hamlib_model: 1049,
            data_mode: 'PKTUSB',
            cat_baud: 38400,
            close_serial_sequencing: true,
          }),
        }),
      );
    });
  });

  it('editing a field marks it overridden and a later radio change leaves it untouched', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    // Start with cat_baud already overridden + a non-default value.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return {
        ...knownConfig, rig_hamlib_model: null, cat_baud: 9600, rig_field_overrides: ['cat_baud'],
      };
      if (cmd === 'rig_list_models') return [{ id: 1049, manufacturer: 'Yaesu', model: 'FT-710' }];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '1049' } });
    await waitFor(() => {
      // cat_baud is overridden → NOT clobbered by the FT-710 profile's 38400.
      const call = invokeMock.mock.calls.find((c) => c[0] === 'config_set_rig');
      expect(call?.[1].value.cat_baud).toBe(9600);
      // but data_mode (not overridden) IS pre-filled.
      expect(call?.[1].value.data_mode).toBe('PKTUSB');
    });
  });

  it('records an override key when the operator edits the Mode', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, rig_field_overrides: [] };
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-data-mode')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-data-mode'), { target: { value: 'USB-D' } });
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === 'config_set_rig');
      expect(call?.[1].value.rig_field_overrides).toContain('data_mode');
      expect(call?.[1].value.data_mode).toBe('USB-D');
    });
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx`
Expected: FAIL — no pre-fill / override behavior yet.

- [ ] **Step 3: Implement pre-fill + override** in `RigControlSection.tsx`:

(a) Import the profile helper:

```ts
import { getRigProfile } from './rigProfiles';
```

(b) Add an override-marking helper + a model-change handler. After `persistRig`:

```ts
  /** Persist a patch AND add `key` to the override set (idempotent). Used when
   *  the operator hand-edits a profile-managed field so a later radio change
   *  won't clobber it. */
  const persistRigWithOverride = (key: string, patch: Partial<RigConfig>) => {
    const base = rigConfig ?? DEFAULT_RIG_CONFIG;
    const overrides = base.rig_field_overrides.includes(key)
      ? base.rig_field_overrides
      : [...base.rig_field_overrides, key];
    persistRig({ ...patch, rig_field_overrides: overrides });
  };

  /** On radio selection: set the model, then apply the radio's documented
   *  profile to each shared field the operator has NOT overridden. */
  const onModelSelected = (modelId: number | null) => {
    const base = rigConfig ?? DEFAULT_RIG_CONFIG;
    const overrides = new Set(base.rig_field_overrides);
    const patch: Partial<RigConfig> = { rig_hamlib_model: modelId };
    const profile = getRigProfile(modelId);
    if (profile) {
      if (profile.data_mode !== undefined && !overrides.has('data_mode')) {
        patch.data_mode = profile.data_mode;
      }
      if (profile.cat_baud !== undefined && !overrides.has('cat_baud')) {
        patch.cat_baud = profile.cat_baud;
      }
      if (profile.close_serial_sequencing !== undefined && !overrides.has('close_serial')) {
        patch.close_serial_sequencing = profile.close_serial_sequencing;
      }
    }
    persistRig(patch);
    // keep the controlled baud input in sync if the profile changed it
    if (patch.cat_baud !== undefined) setCatBaudInput(String(patch.cat_baud));
    // notify ARDOP to pre-fill ptt_method (Task 7); no-op when prop absent
    if (onRadioSelected) onRadioSelected(modelId, overrides.has('ptt_method'));
  };
```

(c) Wire the handlers:
- Model `<select>` and the manual model `<input>` `onChange` → call `onModelSelected(...)` instead of `persistRig({ rig_hamlib_model })`.
- Mode `<select>` `onChange` → `persistRigWithOverride('data_mode', { data_mode: e.target.value })`.
- CAT-baud `commitCatBaud` → on a valid value, `persistRigWithOverride('cat_baud', { cat_baud: n })` (replace its `persistRig` call).
- Close-serial checkbox `onChange` → `persistRigWithOverride('close_serial', { close_serial_sequencing: checked, ...(checked ? { live_vfo_poll: false } : {}) })`.

> `live_vfo_poll` is NOT a profile-managed field → leave its handler as plain `persistRig`.

- [ ] **Step 4: Run tests + typecheck to verify pass**

Run: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx` then `pnpm typecheck`
Expected: PASS. (The `onRadioSelected` prop is added in Task 6; for now reference it via an optional prop — add `onRadioSelected?: (modelId: number | null, pttOverridden: boolean) => void;` to `RigControlSectionProps` in this task so it type-checks.)

- [ ] **Step 5: Commit**

```bash
git add src/radio/modes/RigControlSection.tsx src/radio/modes/RigControlSection.test.tsx
git commit -m "feat(rig): override-respecting per-radio pre-fill for shared rig fields

Selecting a radio applies its documented profile to data_mode/cat_baud/
close_serial unless the operator has overridden that field; hand-editing a
field records its override key so a later radio change leaves it untouched.

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Frontend — `variant="bare"` render mode for embedding

So ARDOP can render the rig rows INSIDE its merged "Radio & audio" expander (Task 7) rather than as a second sibling expander. VARA keeps the default `variant="expander"`.

**Files:**
- Modify: `src/radio/modes/RigControlSection.tsx`
- Modify: `src/radio/modes/RigControlSection.test.tsx`

**Interfaces:**
- Produces: `RigControlSectionProps.variant?: 'expander' | 'bare'` (default `'expander'`). In `'bare'` mode the component renders only the field rows (a fragment), no `<details>`/`<summary>`, and ignores `storageKeyPrefix` for collapse state.

- [ ] **Step 1: Write the failing test** in `RigControlSection.test.tsx`

```ts
  it('variant="bare" renders rows without the expander chrome', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" variant="bare" />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    expect(screen.queryByTestId('rig-control-expander')).not.toBeInTheDocument();
  });

  it('default variant still renders the expander', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="vara" />);
    await waitFor(() => expect(screen.getByTestId('rig-control-expander')).toBeInTheDocument());
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx`
Expected: FAIL — `variant` prop not supported.

- [ ] **Step 3: Implement** — refactor `RigControlSection.tsx`:

(a) Extend props:

```ts
interface RigControlSectionProps {
  storageKeyPrefix: string;
  variant?: 'expander' | 'bare';
  /** ARDOP-only: called after a radio is selected so the panel can pre-fill the
   *  ARDOP ptt_method. `pttOverridden` is whether the operator overrode it. */
  onRadioSelected?: (modelId: number | null, pttOverridden: boolean) => void;
}
export function RigControlSection({ storageKeyPrefix, variant = 'expander', onRadioSelected }: RigControlSectionProps) {
```

(b) Extract the field rows into a local `const rows = (<>...</>)` fragment (everything currently between `<summary>...</summary>` and `</details>`). Then return:

```tsx
  if (variant === 'bare') {
    return rows;
  }
  return (
    <details className="expander" open={rigCfgOpen} onToggle={...} data-testid="rig-control-expander">
      <summary className="expander-summary" data-testid="rig-control-expander-summary">Rig control</summary>
      {rows}
    </details>
  );
```

> Keep the `useState`/`useEffect`/handlers above the `rows` definition unchanged. The collapse-state hooks are harmless in `bare` mode (just unused).

- [ ] **Step 4: Run tests + typecheck to verify pass**

Run: `pnpm vitest run src/radio/modes/RigControlSection.test.tsx` then `pnpm typecheck`
Expected: PASS — both variant tests green; all prior tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/radio/modes/RigControlSection.tsx src/radio/modes/RigControlSection.test.tsx
git commit -m "refactor(rig): add variant=bare render mode to RigControlSection

Lets the ARDOP panel embed the rig rows inside one merged Radio & audio group
(Task 7) instead of a second sibling expander. VARA keeps the default expander.

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Frontend — ARDOP "Radio & audio" merge + PTT pre-fill/override + Tune-inline regression

Merges ARDOP's "Radio" expander and the rig section into one collapsible "Radio & audio" group; wires ptt_method pre-fill (on radio change) + override (on manual ptt edit); locks the Tune-inline placement.

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx`
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx`

**Interfaces:**
- Consumes: `RigControlSection` with `variant="bare"` + `onRadioSelected` (Task 6); `getRigProfile` (Task 3); `config_get_rig`/`config_set_rig` (to read/write the shared override set for the `"ptt_method"` key); existing `persistArdop`, `onPttMethodChange`, `PttMethod`.

- [ ] **Step 1: Write the failing tests** in `ArdopRadioPanel.test.tsx` (extend the existing invoke mock to also answer `rig_list_models`→`[{id:1049,...}]` and `config_get_rig`/`config_set_rig`)

```ts
  it('renders one merged "Radio & audio" group containing audio, PTT, and rig rows', async () => {
    render(<ArdopRadioPanel /* existing required props */ />);
    await waitFor(() => expect(screen.getByTestId('ardop-config-expander')).toBeInTheDocument());
    const group = screen.getByTestId('ardop-config-expander');
    expect(within(group).getByText('Radio & audio')).toBeInTheDocument();
    // audio + ptt + rig rows all live inside the single group
    expect(within(group).getByTestId('ardop-capture-select')).toBeInTheDocument();
    expect(within(group).getByTestId('ardop-ptt-method-select')).toBeInTheDocument();
    expect(within(group).getByTestId('rig-model')).toBeInTheDocument();
    // the rig section is no longer its own expander
    expect(screen.queryByTestId('rig-control-expander')).not.toBeInTheDocument();
  });

  it('Tune button sits in the same row as the frequency input (inline)', async () => {
    render(<ArdopRadioPanel /* props */ />);
    await waitFor(() => expect(screen.getByTestId('ardop-freq')).toBeInTheDocument());
    const freqRow = screen.getByTestId('ardop-freq').closest('.radio-panel-input-row');
    expect(freqRow).not.toBeNull();
    expect(within(freqRow as HTMLElement).getByTestId('ardop-tune')).toBeInTheDocument();
  });

  it('pre-fills ptt_method from the radio profile when ptt is not overridden', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<ArdopRadioPanel /* props */ />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '1049' } });
    await waitFor(() => {
      // FT-710 profile → cat_command persisted to ArdopUiConfig
      const call = invokeMock.mock.calls.find(
        (c) => c[0] === 'config_set_ardop' && c[1]?.value?.ptt_method === 'cat_command',
      );
      expect(call).toBeTruthy();
    });
  });

  it('marks ptt_method overridden when the operator changes it manually', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<ArdopRadioPanel /* props */ />);
    await waitFor(() => expect(screen.getByTestId('ardop-ptt-method-select')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('ardop-ptt-method-select'), { target: { value: 'serial_rts' } });
    await waitFor(() => {
      const call = invokeMock.mock.calls.find(
        (c) => c[0] === 'config_set_rig' && c[1]?.value?.rig_field_overrides?.includes('ptt_method'),
      );
      expect(call).toBeTruthy();
    });
  });
```

> Read the existing `ArdopRadioPanel.test.tsx` render helper + props first; reuse them verbatim (the `/* props */` placeholders above stand for the existing test's render call).

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx`
Expected: FAIL — two expanders still present; "Radio & audio" label absent; no ptt pre-fill/override.

- [ ] **Step 3: Implement the merge** in `ArdopRadioPanel.tsx`:

(a) Rename the expander summary (line 1020) `<summary className="expander-summary">Radio</summary>` → `Radio &amp; audio`.

(b) Move the `<RigControlSection storageKeyPrefix="ardop" />` (line 1294, currently a sibling AFTER `</details>`) to INSIDE the expander, just before the closing `</details>` (line 1288), and switch to bare + wire callback:

```tsx
            <RigControlSection
              storageKeyPrefix="ardop"
              variant="bare"
              onRadioSelected={onRigRadioSelected}
            />
          </details>
        </section>
      )}
```

(Delete the old sibling `<RigControlSection storageKeyPrefix="ardop" />` at 1294 and its comment block at 1290–1293.)

(c) Add the ptt pre-fill handler. Near the other handlers (e.g. by `onPttMethodChange`), add:

```ts
  // tuxlink-31c63: when a radio is selected in the shared rig section, pre-fill
  // the ARDOP-only ptt_method from the radio's documented profile unless the
  // operator has overridden it. RigControlSection passes whether ptt is already
  // overridden (read from the shared Config.rig override set).
  const onRigRadioSelected = useCallback((modelId: number | null, pttOverridden: boolean) => {
    if (pttOverridden) return;
    const profile = getRigProfile(modelId);
    if (profile?.ptt_method) {
      setPttMethod(profile.ptt_method as PttMethod);
      persistArdop({ ptt_method: profile.ptt_method as PttMethod });
    }
  }, [persistArdop]);
```

> Read the panel's existing ptt state setter name (likely `setPttMethod`) + `persistArdop` signature and match them. Import `getRigProfile` from `./rigProfiles` and `useCallback` if not already imported.

(d) Mark ptt overridden on manual change. In `onPttMethodChange` (the handler bound at line 1128), after it persists the ardop ptt change, also add `"ptt_method"` to the shared `Config.rig.rig_field_overrides`:

```ts
  // (inside onPttMethodChange, after the existing persistArdop({ ptt_method }) )
  void invoke<RigConfig>('config_get_rig')
    .then((rig) => {
      const overrides = rig.rig_field_overrides.includes('ptt_method')
        ? rig.rig_field_overrides
        : [...rig.rig_field_overrides, 'ptt_method'];
      void invoke('config_set_rig', { value: { ...rig, rig_field_overrides: overrides } });
    })
    .catch(() => { /* config absent pre-wizard — nothing to mark */ });
```

> Import the `RigConfig` type from `./RigControlSection` (it is exported there). Read `onPttMethodChange` first; insert this AFTER its existing persist, do not replace the existing behavior.

- [ ] **Step 4: Run tests + typecheck to verify pass**

Run: `pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx` then `pnpm typecheck`
Expected: PASS — one merged group; Tune inline; ptt pre-fill + override wired.

- [ ] **Step 5: Commit**

```bash
git add src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/ArdopRadioPanel.test.tsx
git commit -m "feat(rig): merge ARDOP audio+PTT+rig into one Radio & audio group

Collapses the two overlapping expanders into a single collapsible group, embeds
the shared rig rows (variant=bare), and wires ptt_method pre-fill on radio
change + override on manual edit. Locks Tune-inline placement with a regression
test.

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Frontend — VARA inheritance regression test

VARA needs no source change (it already renders `<RigControlSection storageKeyPrefix="vara" />`, which now has the redesigned pickers). Lock that the inheritance holds and that VARA shows the rig-config rows but NOT the ARDOP-only audio/PTT rows.

**Files:**
- Modify: `src/radio/modes/VaraRadioPanel.test.tsx`

- [ ] **Step 1: Write the test** (extend the VARA panel test's invoke mock with `rig_list_models`→`[{id:1049,...}]`, `packet_list_serial_devices`→`[]`)

```ts
  it('VARA inherits the redesigned rig pickers (model + CAT-port + Mode), no ARDOP audio/PTT', async () => {
    render(<VaraRadioPanel /* existing props */ />);
    await waitFor(() => expect(screen.getByTestId('vara-rig-section')).toBeInTheDocument());
    const rig = screen.getByTestId('vara-rig-section');
    expect(within(rig).getByTestId('rig-model')).toBeInTheDocument();
    expect(within(rig).getByTestId('rig-cat-port')).toBeInTheDocument();
    expect(within(rig).getByTestId('rig-data-mode')).toBeInTheDocument();
    // ARDOP-only rows are NOT in the VARA rig group
    expect(screen.queryByTestId('ardop-capture-select')).not.toBeInTheDocument();
    expect(screen.queryByTestId('ardop-ptt-method-select')).not.toBeInTheDocument();
    // QSY control is gone here too
    expect(screen.queryByTestId('rig-qsy-on-fail')).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run test + typecheck**

Run: `pnpm vitest run src/radio/modes/VaraRadioPanel.test.tsx` then `pnpm typecheck`
Expected: PASS — VARA shows the inherited pickers; no ARDOP-only rows; no QSY.

- [ ] **Step 3: Commit**

```bash
git add src/radio/modes/VaraRadioPanel.test.tsx
git commit -m "test(rig): lock VARA inheritance of the redesigned shared rig pickers

Agent: kestrel-esker-grouse
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Final verification + full-suite gate

- [ ] **Step 1: Frontend full suite + typecheck + build (local)**

Run: `pnpm typecheck && pnpm vitest run && pnpm build`
Expected: PASS — all suites green; production build succeeds.

- [ ] **Step 2: Push the branch + open/refresh the draft PR so CI compiles the Rust**

```bash
git push origin bd-tuxlink-31c63/rig-panel-redesign
```
Expected: CI `verify` job (amd64 + arm64) runs `cargo clippy -D warnings` + `cargo test` + `pnpm vitest run` + `pnpm build`; all green. Watch with `gh pr checks`.

- [ ] **Step 3: Wire-walk gate (REQUIRED before any "done" claim)**

Run the `wire-walk` skill (`.claude/skills/wire-walk/SKILL.md`). The operator supplies the key user flows greenfield; trace each to code (`file:line`). Confirm the redesigned surface is reachable in a real build: model picker → select radio → pre-fill → CAT-port pick → Mode → Tune. Any broken primary flow means NOT shipped.

- [ ] **Step 4: Requesting code review** — invoke `superpowers:requesting-code-review` (and a Codex adversarial round per CLAUDE.md if available) on the branch diff before marking the PR ready.

---

## Self-Review (completed at plan-write time)

**Spec coverage:**
- Critique 1 (hand-typed CAT port) → Task 4 (detected-ports picker). ✓
- Critique 2 (QSY checkbox out of context + Part 97) → Task 4 removes the control; `qsy_on_fail` retained in DTO (Task 2 leaves it). ✓
- Critique 3 (stacked-checkbox dead space) → Task 4 (full-width rows; "Close serial during audio" single labeled control). ✓
- Critique 4 (FT-710 only) → Tasks 1 + 4 (rig_list_models picker). ✓
- Critique 5 (inert CAT backend field) → Task 4 deletes it. ✓
- Critique 6 (two competing dropdowns) → Task 7 (radio picker is the single anchor; PTT one field under it in the merged group). ✓
- Critique 7 (floating Tune) → already inline in code; Task 7 locks it with a regression test. ✓
- Model picker from `rigctl -l`, grouped, searchable-via-options, manual fallback → Tasks 1 + 4. ✓
- Per-radio pre-fill (override-respecting) + `rig_field_overrides` → Tasks 2, 3, 5, 7. ✓
- Mode row + `data_mode` threaded into tune → Tasks 2 + 4. ✓
- One "Radio & audio" group (ARDOP) → Task 7; VARA inherits → Tasks 6 + 8. ✓
- Error handling (rigctl missing → empty list/manual; no ports → manual; unprofiled → no pre-fill) → Tasks 1, 4, 3. ✓
- Testing (frontend vitest local; backend CI) → every task + Task 9. ✓

**Placeholder scan:** test render-call `/* props */` placeholders are deliberate (the existing test files' render helpers must be reused verbatim — the implementer reads them); all production code is complete.

**Type consistency:** `RigConfig` (TS) extended in Task 4 matches `RigUiConfig` (Rust) extended in Task 2 (`data_mode: string`/`String`, `rig_field_overrides: string[]`/`Vec<String>`). Override keys are the SAME four literals everywhere: `"ptt_method"`, `"data_mode"`, `"cat_baud"`, `"close_serial"`. `onRadioSelected` signature is identical in Tasks 5/6/7. `RigModel`/`RigModelDto` fields (`id`/`manufacturer`/`model`) match across Rust + TS.
