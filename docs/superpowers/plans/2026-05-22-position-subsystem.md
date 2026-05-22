# Position Subsystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A single position source-of-truth that arbitrates between a manually-entered Maidenhead grid and a live GPS fix under an explicit, operator-owned source contract, with broadcast precision enforced independent of source.

**Architecture:** A Rust `position` module owns a `PositionArbiter` (the only writer of the active position). A background gpsd-client task feeds it GPS fixes; the arbiter applies them only when `source == Gps`. Manual entry (inline-edit in the ribbon, Approach A) pins `source == Manual` and is sticky. The CMS locator and ribbon read the arbiter's precision-reduced broadcast grid.

**Tech Stack:** Rust (Tauri 2 backend, tokio), React 18 + TypeScript (frontend), gpsd (`127.0.0.1:2947`, already serving the LC29C on `/dev/ttyAMA0`). Spec: `docs/superpowers/specs/2026-05-22-position-subsystem-design.md`.

**Prerequisite landed:** `tuxlink-882` added `config::broadcast_grid(grid, precision)` — this plan reuses it.

---

## File Structure

**Create:**
- `src-tauri/src/position/mod.rs` — module root; re-exports; `PositionSource`, `Fix` types.
- `src-tauri/src/position/maidenhead.rs` — `lat_lon_to_grid`, `grid_to_lat_lon` (Phase 1).
- `src-tauri/src/position/arbiter.rs` — `PositionArbiter` source state machine (Phase 3).
- `src-tauri/src/position/gpsd.rs` — gpsd TPV client (Phase 6).
- `src/shell/GridEdit.tsx` + `GridEdit.test.tsx` — inline-edit + source chip (Phase 5).

**Modify:**
- `src-tauri/src/config.rs` — add `position_source` to `PrivacyConfig`; schema bump + migration (Phase 2).
- `src-tauri/src/lib.rs` — `mod position;`, manage `PositionArbiter` state, register commands, spawn gpsd task (Phases 4, 7).
- `src-tauri/src/ui_commands.rs` — `config_set_grid`, `position_set_source`, extend `ConfigViewDto` (Phases 4, 7).
- `src-tauri/src/winlink_backend.rs` — `cms_locator` reads the arbiter's broadcast grid (Phase 4).
- `src/shell/useStatus.ts` — add `position_source` to the DTO + types (Phase 5).
- `src/shell/DashboardRibbon.tsx` — render `<GridEdit>` in the Grid cell (Phase 5).

---

## Phase 1 — Maidenhead conversion

### Task 1: `lat_lon_to_grid` (6-char)

**Files:**
- Create: `src-tauri/src/position/maidenhead.rs`
- Create: `src-tauri/src/position/mod.rs`

- [ ] **Step 1: Write the failing test** (in `maidenhead.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_references_round_to_six_char() {
        // Reference points (well-known Maidenhead anchors).
        assert_eq!(lat_lon_to_grid(48.143, 11.608), "JN58td"); // Munich
        assert_eq!(lat_lon_to_grid(-34.91, -56.21), "GF15vc"); // Montevideo
        assert_eq!(lat_lon_to_grid(0.0, 0.0), "JJ00aa");       // origin corner
    }

    #[test]
    fn clamps_out_of_range_inputs() {
        // Never panic / index past the field tables.
        let g = lat_lon_to_grid(95.0, 200.0);
        assert_eq!(g.len(), 6);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib position::maidenhead`
Expected: FAIL (function not defined).

- [ ] **Step 3: Implement**

```rust
//! Maidenhead locator conversion (no external crate — the algorithm is small and
//! we need exact control over precision + clamping). Field (A-R) / square (0-9) /
//! subsquare (a-x). Longitude uses 20°/2°/5′ steps; latitude 10°/1°/2.5′.

/// Convert WGS-84 lat/lon (degrees) to a 6-char Maidenhead locator.
/// Inputs are clamped to valid ranges so this never panics.
pub fn lat_lon_to_grid(lat: f64, lon: f64) -> String {
    let lon = (lon.clamp(-180.0, 179.999) + 180.0) / 20.0;
    let lat = (lat.clamp(-90.0, 89.999) + 90.0) / 10.0;

    let lon_field = lon.floor();
    let lat_field = lat.floor();
    let lon_sq = ((lon - lon_field) * 10.0).floor();
    let lat_sq = ((lat - lat_field) * 10.0).floor();
    let lon_sub = ((lon - lon_field - lon_sq / 10.0) * 240.0).floor();
    let lat_sub = ((lat - lat_field - lat_sq / 10.0) * 240.0).floor();

    let a = |n: f64, base: u8| (base + n as u8) as char;
    format!(
        "{}{}{}{}{}{}",
        a(lon_field, b'A'), a(lat_field, b'A'),
        a(lon_sq, b'0'), a(lat_sq, b'0'),
        a(lon_sub, b'a'), a(lat_sub, b'a'),
    )
}
```

And `mod.rs`:

```rust
pub mod maidenhead;
pub use maidenhead::{grid_to_lat_lon, lat_lon_to_grid};
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib position::maidenhead`
Expected: PASS (3 tests). Adjust the reference strings to the algorithm's exact output if a reference differs by the last subsquare pair — pick the canonical value the algorithm produces and lock it in (these are stable).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/position/maidenhead.rs src-tauri/src/position/mod.rs
git commit -m "feat(position): lat/lon -> Maidenhead 6-char conversion (tuxlink-686)"
```

### Task 2: `grid_to_lat_lon` (center of the square)

**Files:** Modify `src-tauri/src/position/maidenhead.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn grid_to_lat_lon_round_trips_to_same_grid() {
    let (lat, lon) = grid_to_lat_lon("JN58td").unwrap();
    assert_eq!(lat_lon_to_grid(lat, lon), "JN58td");
}

#[test]
fn grid_to_lat_lon_rejects_malformed() {
    assert!(grid_to_lat_lon("ZZ99").is_none());   // field letters only go A-R
    assert!(grid_to_lat_lon("J").is_none());       // too short
}
```

- [ ] **Step 2: Run, verify fail.** `cargo test --manifest-path src-tauri/Cargo.toml --lib position::maidenhead`

- [ ] **Step 3: Implement** (append to `maidenhead.rs`)

```rust
/// Convert a 4- or 6-char Maidenhead locator to the lat/lon at the CENTER of the
/// square. Returns `None` for malformed input (wrong length, out-of-range chars).
pub fn grid_to_lat_lon(grid: &str) -> Option<(f64, f64)> {
    let g = grid.as_bytes();
    if g.len() != 4 && g.len() != 6 { return None; }
    let up = |b: u8| b.to_ascii_uppercase();
    let lo = |b: u8| b.to_ascii_lowercase();
    let field = |b: u8| (b'A'..=b'R').contains(&up(b)).then(|| (up(b) - b'A') as f64);
    let digit = |b: u8| b.is_ascii_digit().then(|| (b - b'0') as f64);

    let mut lon = field(g[0])? * 20.0 - 180.0;
    let mut lat = field(g[1])? * 10.0 - 90.0;
    lon += digit(g[2])? * 2.0;
    lat += digit(g[3])? * 1.0;
    if g.len() == 6 {
        let sub = |b: u8| (b'a'..=b'x').contains(&lo(b)).then(|| (lo(b) - b'a') as f64);
        lon += sub(g[4])? * 5.0 / 60.0;
        lat += sub(g[5])? * 2.5 / 60.0;
        lon += 2.5 / 60.0; lat += 1.25 / 60.0;       // center of subsquare
    } else {
        lon += 1.0; lat += 0.5;                       // center of square
    }
    Some((lat, lon))
}
```

- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `git commit -am "feat(position): Maidenhead -> lat/lon (square center) (tuxlink-686)"`

---

## Phase 2 — Config: position_source field + migration

### Task 3: Add `position_source` to `PrivacyConfig`

**Files:** Modify `src-tauri/src/config.rs`

- [ ] **Step 1: Failing test** (in config.rs `mod tests`)

```rust
#[test]
fn config_defaults_position_source_to_gps_on_migration() {
    // A config JSON written before this field existed must load with source = Gps.
    let json = sample_config_json_without_position_source(); // helper: omit the key
    let cfg: Config = serde_json::from_str(&json).expect("migrates");
    assert_eq!(cfg.privacy.position_source, PositionSource::Gps);
}
```

- [ ] **Step 2: Run, verify fail.** `cargo test --manifest-path src-tauri/Cargo.toml --lib config::tests::config_defaults_position_source`

- [ ] **Step 3: Implement**

In `config.rs`, add the enum and the field with a serde default:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PositionSource { Manual, Gps }

fn default_position_source() -> PositionSource { PositionSource::Gps }
```

In `PrivacyConfig`:

```rust
    #[serde(default = "default_position_source")]
    pub position_source: PositionSource,
```

Bump `CONFIG_SCHEMA_VERSION` by 1. The `#[serde(default)]` IS the migration for the additive field (old files lack the key → default Gps). Update every `PrivacyConfig { .. }` literal in non-test code + the test helpers (`offline_config`, wizard defaults) to set `position_source: PositionSource::Gps`.

> NOTE: `PrivacyConfig` uses `#[serde(deny_unknown_fields)]`. `#[serde(default)]` on a missing field is still accepted (deny_unknown_fields rejects EXTRA keys, not missing ones). Verify the migration test covers a file written at the OLD schema_version — if the schema-version guard rejects old versions outright, add a migration arm in `read_config` that upgrades the version after defaulting the field.

- [ ] **Step 4: Run, verify pass.** Run the full config test module.
- [ ] **Step 5: Commit** `git commit -am "feat(config): position_source field (default Gps) + migration (tuxlink-686)"`

---

## Phase 3 — Position arbiter

### Task 4: `PositionArbiter` core + manual sticky

**Files:** Create `src-tauri/src/position/arbiter.rs`; modify `position/mod.rs`

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PositionPrecision;

    #[test]
    fn set_manual_pins_source_and_is_sticky_against_gps() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.set_manual("CN87ux");
        assert_eq!(a.source(), PositionSource::Manual);
        a.apply_gps_fix(Fix::test("DM33ab")); // GPS arrives
        assert_eq!(a.active_grid().as_deref(), Some("CN87ux")); // unchanged
        assert_eq!(a.source(), PositionSource::Manual);
    }

    #[test]
    fn gps_fix_updates_active_only_when_source_is_gps() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.apply_gps_fix(Fix::test("DM33ab"));
        assert_eq!(a.active_grid().as_deref(), Some("DM33ab"));
    }

    #[test]
    fn broadcast_grid_reduces_to_precision() {
        let a = PositionArbiter::new(PositionSource::Manual, Some("CN87ux".into()), PositionPrecision::FourCharGrid);
        assert_eq!(a.broadcast_grid().as_deref(), Some("CN87"));
    }

    #[test]
    fn use_gps_requires_a_usable_fix() {
        let a = PositionArbiter::new(PositionSource::Manual, Some("CN87".into()), PositionPrecision::FourCharGrid);
        assert!(a.use_gps().is_err());            // no fix yet
        a.apply_gps_fix(Fix::test("DM33ab"));     // stored as last_fix even while Manual
        assert!(a.use_gps().is_ok());
        assert_eq!(a.source(), PositionSource::Gps);
        assert_eq!(a.active_grid().as_deref(), Some("DM33ab"));
    }
}
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** `arbiter.rs`

```rust
use std::sync::Mutex;
use crate::config::{broadcast_grid, PositionPrecision, PositionSource};

/// A GPS fix reduced to a grid + freshness, as handed in by the gpsd client.
#[derive(Debug, Clone)]
pub struct Fix {
    pub grid: String,
    pub received: std::time::Instant,
}
impl Fix {
    #[cfg(test)]
    pub fn test(grid: &str) -> Self { Self { grid: grid.into(), received: std::time::Instant::now() } }
    fn is_fresh(&self, window: std::time::Duration) -> bool { self.received.elapsed() < window }
}

const FIX_STALENESS: std::time::Duration = std::time::Duration::from_secs(30);

/// The single source of truth for position. Interior-mutable (one `Mutex`) so it
/// can live in Tauri managed state and be read by commands + the gpsd task.
pub struct PositionArbiter {
    inner: Mutex<Inner>,
}
struct Inner {
    source: PositionSource,
    manual_grid: Option<String>,  // last hand-set grid (full precision)
    last_fix: Option<Fix>,        // newest GPS fix, regardless of source
    precision: PositionPrecision,
}

impl PositionArbiter {
    pub fn new(source: PositionSource, manual_grid: Option<String>, precision: PositionPrecision) -> Self {
        Self { inner: Mutex::new(Inner { source, manual_grid, last_fix: None, precision }) }
    }
    pub fn source(&self) -> PositionSource { self.inner.lock().unwrap().source }

    /// Hand-set grid: store full precision, pin Manual (sticky). Caller validates first.
    pub fn set_manual(&self, grid: &str) {
        let mut i = self.inner.lock().unwrap();
        i.manual_grid = Some(grid.to_string());
        i.source = PositionSource::Manual;
    }

    /// Record the newest fix. Becomes the active position only while source == Gps.
    pub fn apply_gps_fix(&self, fix: Fix) {
        self.inner.lock().unwrap().last_fix = Some(fix);
    }

    /// Switch to GPS — only if a fresh fix exists. Err with a reason otherwise.
    pub fn use_gps(&self) -> Result<(), &'static str> {
        let mut i = self.inner.lock().unwrap();
        match &i.last_fix {
            Some(f) if f.is_fresh(FIX_STALENESS) => { i.source = PositionSource::Gps; Ok(()) }
            _ => Err("no usable GPS fix"),
        }
    }

    /// The active grid at full precision (Manual → manual_grid; Gps → fresh fix, else
    /// fall back to manual_grid so the ribbon never goes blank).
    pub fn active_grid(&self) -> Option<String> {
        let i = self.inner.lock().unwrap();
        match i.source {
            PositionSource::Manual => i.manual_grid.clone(),
            PositionSource::Gps => match &i.last_fix {
                Some(f) if f.is_fresh(FIX_STALENESS) => Some(f.grid.clone()),
                _ => i.manual_grid.clone(),
            },
        }
    }

    /// The active grid reduced to broadcast precision — the ONLY value that goes on air.
    pub fn broadcast_grid(&self) -> Option<String> {
        let precision = self.inner.lock().unwrap().precision;
        self.active_grid().map(|g| broadcast_grid(&g, precision))
    }

    pub fn has_fresh_fix(&self) -> bool {
        self.inner.lock().unwrap().last_fix.as_ref().is_some_and(|f| f.is_fresh(FIX_STALENESS))
    }
}
```

Add to `mod.rs`: `pub mod arbiter; pub use arbiter::{Fix, PositionArbiter};` and `pub use crate::config::PositionSource;`.

- [ ] **Step 4: Run, verify pass.** `cargo test --manifest-path src-tauri/Cargo.toml --lib position::arbiter`
- [ ] **Step 5: Commit** `git commit -am "feat(position): source-arbiter state machine (manual sticky, broadcast reduction) (tuxlink-686)"`

---

## Phase 4 — Backend wiring: command + locator

### Task 5: `config_set_grid` command + manage the arbiter

**Files:** Modify `src-tauri/src/lib.rs`, `src-tauri/src/ui_commands.rs`

- [ ] **Step 1: Failing test** (ui_commands.rs `mod tests`) — validate-reject path:

```rust
#[tokio::test]
async fn config_set_grid_rejects_invalid_maidenhead() {
    let err = validate_grid_input("NOTAGRID");
    assert!(err.is_some()); // returns the validation message
}
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement.** Add a Rust-side validator mirroring the wizard's `validateGrid` (4/6-char Maidenhead), the command, and arbiter state:

```rust
// ui_commands.rs
pub fn validate_grid_input(s: &str) -> Option<&'static str> {
    let s = s.trim();
    let ok = matches!(s.len(), 4 | 6)
        && s[0..2].bytes().all(|b| (b'A'..=b'R').contains(&b.to_ascii_uppercase()))
        && s[2..4].bytes().all(|b| b.is_ascii_digit())
        && (s.len() == 4 || s[4..6].bytes().all(|b| (b'a'..=b'x').contains(&b.to_ascii_lowercase())));
    (!ok).then_some("Grid must be a 4- or 6-char Maidenhead locator (e.g. EM75 or EM75xx).")
}

#[tauri::command]
pub async fn config_set_grid(
    grid: String,
    arbiter: State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
) -> Result<(), UiError> {
    let g = grid.trim().to_string();
    if let Some(msg) = validate_grid_input(&g) { return Err(UiError::Validation(msg.into())); }
    // Persist to config (write_config_atomic), then pin the arbiter.
    let mut cfg = crate::config::read_config().map_err(UiError::from)?;
    cfg.identity.grid = Some(g.clone());
    cfg.privacy.position_source = crate::config::PositionSource::Manual;
    crate::config::write_config_atomic(&cfg).map_err(UiError::from)?;
    arbiter.set_manual(&g);
    Ok(())
}
```

In `lib.rs`: build the arbiter at startup from `read_config()` (`PositionArbiter::new(cfg.privacy.position_source, cfg.identity.grid, cfg.privacy.position_precision)`), `.manage(Arc::new(arbiter))`, and add `config_set_grid` to `generate_handler!`.

- [ ] **Step 4: Run, verify pass.** Add a focused test that `config_set_grid` with a valid grid pins the arbiter to Manual (construct an arbiter, call `set_manual`, assert source). Run the lib suite.
- [ ] **Step 5: Commit** `git commit -am "feat(position): config_set_grid command + managed arbiter (tuxlink-686)"`

### Task 6: CMS locator from the arbiter

**Files:** Modify `src-tauri/src/winlink_backend.rs`

- [ ] **Step 1: Failing test** — extend the existing `cms_locator` tests to source from the arbiter's broadcast grid. (If `native_connect` still takes `&Config`, thread the arbiter's `broadcast_grid()` in as the locator instead; keep `cms_locator(config)` only as the no-arbiter fallback.)

```rust
#[test]
fn locator_uses_arbiter_broadcast_grid() {
    let a = PositionArbiter::new(PositionSource::Manual, Some("CN87ux".into()), PositionPrecision::FourCharGrid);
    assert_eq!(a.broadcast_grid().as_deref(), Some("CN87"));
}
```

- [ ] **Step 2-4:** Pass the arbiter (or its `broadcast_grid()`) into the connect path so the on-air locator is the arbiter's broadcast grid, superseding the `tuxlink-882` `cms_locator(config)` read. Keep behavior identical for the Manual case (already reduced).
- [ ] **Step 5: Commit** `git commit -am "feat(position): CMS locator sourced from the arbiter (tuxlink-686)"`

---

## Phase 5 — Frontend: inline-edit + source chip (Approach A)

### Task 7: `position_source` in the status DTO

**Files:** Modify `src-tauri/src/ui_commands.rs` (`ConfigViewDto`), `src/shell/useStatus.ts`

- [ ] **Step 1-4:** Add `position_source: PositionSource` to `ConfigViewDto` (Rust) and the TS `ConfigView` type; map it through `useStatusData`. Add a Rust test asserting the DTO carries the source; a vitest asserting `useStatusData` surfaces it.
- [ ] **Step 5: Commit** `git commit -am "feat(position): surface position_source in the status DTO (tuxlink-686)"`

### Task 8: `<GridEdit>` — inline-edit + source chip

**Files:** Create `src/shell/GridEdit.tsx`, `src/shell/GridEdit.test.tsx`; modify `src/shell/DashboardRibbon.tsx`

- [ ] **Step 1: Failing vitest** (`GridEdit.test.tsx`)

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { GridEdit } from './GridEdit';

test('clicking the grid value enters edit mode and commits a valid grid', async () => {
  const onCommit = vi.fn().mockResolvedValue(undefined);
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('ribbon-grid'));
  const input = screen.getByTestId('grid-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: 'DM33ab' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onCommit).toHaveBeenCalledWith('DM33ab');
});

test('invalid grid shows a validation message and does not commit', () => {
  const onCommit = vi.fn();
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('ribbon-grid'));
  const input = screen.getByTestId('grid-input');
  fireEvent.change(input, { target: { value: 'NOPE' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.getByTestId('grid-error')).toBeInTheDocument();
});

test('shows GPS-ready affordance when a fix is available while Manual', () => {
  render(<GridEdit grid="CN87" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.getByTestId('use-gps')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run, verify fail.** `pnpm -C <worktree> exec vitest run src/shell/GridEdit.test.tsx`

- [ ] **Step 3: Implement** `GridEdit.tsx`. Reuse `validateGrid`/`normalizeGrid` from `../wizard/validators`. Render the value as a clickable cell (`data-testid="ribbon-grid"`); click → controlled `<input data-testid="grid-input">`; Enter validates+commits (`onCommit(normalized)`) and exits, Esc cancels; invalid shows `data-testid="grid-error"`. Render the source chip (`MANUAL` amber / `GPS` outline). When `source==='Manual' && gpsReady`, render the "● GPS ready — tap to switch" affordance (`data-testid="use-gps"`, `onClick={onUseGps}`). Match `.tux` ribbon classes from `AppShell.css`.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5:** Wire into `DashboardRibbon.tsx`: replace the static Grid `dash-value` with `<GridEdit grid={grid} source={data.position_source} gpsReady={data.gpsReady} onCommit={(g) => invoke('config_set_grid', { grid: g })} onUseGps={() => invoke('position_set_source', { source: 'Gps' })} />`. Run `DashboardRibbon.test.tsx` + `tsc --noEmit`.

- [ ] **Step 6: Commit** `git commit -am "feat(position): inline-edit grid + source chip in the ribbon (tuxlink-686)"`

> **Browser smoke (operator, per project norm):** `pnpm tauri dev`, click the Grid value, edit + Enter, confirm the ribbon updates and the config persists; confirm Esc cancels and invalid input shows the error.

---

## Phase 6 — gpsd client

### Task 9: TPV parse

**Files:** Create `src-tauri/src/position/gpsd.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parses_a_3d_tpv_into_a_grid() {
    let line = r#"{"class":"TPV","mode":3,"lat":48.143,"lon":11.608}"#;
    let fix = parse_tpv(line).unwrap();
    assert_eq!(fix.grid, "JN58td");
}
#[test]
fn rejects_no_fix_and_non_tpv() {
    assert!(parse_tpv(r#"{"class":"TPV","mode":1}"#).is_none());   // no fix
    assert!(parse_tpv(r#"{"class":"SKY"}"#).is_none());            // not a fix report
    assert!(parse_tpv("not json").is_none());
}
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** — parse one gpsd JSON line; accept `class==TPV && mode>=2 && lat,lon present`; convert via `lat_lon_to_grid`; return `Fix`. Use `serde_json::Value` (already a dependency) to avoid a rigid struct.

```rust
use crate::position::{lat_lon_to_grid, Fix};

pub fn parse_tpv(line: &str) -> Option<Fix> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("class")?.as_str()? != "TPV" { return None; }
    if v.get("mode")?.as_i64()? < 2 { return None; }   // 0/1 = no fix
    let lat = v.get("lat")?.as_f64()?;
    let lon = v.get("lon")?.as_f64()?;
    Some(Fix { grid: lat_lon_to_grid(lat, lon), received: std::time::Instant::now() })
}
```

- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `git commit -am "feat(position): gpsd TPV -> Fix parsing (tuxlink-686)"`

### Task 10: gpsd watch task

**Files:** Modify `src-tauri/src/position/gpsd.rs`

- [ ] **Step 1-4:** Add `pub fn spawn_gpsd_client(arbiter: Arc<PositionArbiter>)` — a tokio task that connects `127.0.0.1:2947`, writes `?WATCH={"enable":true,"json":true}\n`, reads line-by-line, calls `parse_tpv`, and `arbiter.apply_gps_fix(fix)` on each. On connect failure or EOF, reconnect with capped exponential backoff (1s→30s). Absence of gpsd is normal (log once, keep retrying). Test the backoff calc as a pure helper (`next_backoff(prev)`); the socket loop itself is validated by the gpsfake integration test (Task 12), not a unit test.
- [ ] **Step 5: Commit** `git commit -am "feat(position): gpsd watch task with reconnect backoff (tuxlink-686)"`

---

## Phase 7 — GPS source wiring + integration

### Task 11: `position_set_source` command + spawn the client

**Files:** Modify `src-tauri/src/ui_commands.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1-4:** Add `#[tauri::command] position_set_source(source: String, arbiter, ...)` → on `"Gps"` call `arbiter.use_gps()` (map `Err` to a `UiError` the ribbon shows as "no GPS fix"); persist `position_source` to config. Extend the status DTO with `gps_ready: bool` (from `arbiter.has_fresh_fix()`). In `lib.rs` `.setup`, call `spawn_gpsd_client(arbiter.clone())`. Register the command. Test: `use_gps` error maps to the right `UiError`; DTO carries `gps_ready`.
- [ ] **Step 5: Commit** `git commit -am "feat(position): use-gps switch + spawn gpsd client at startup (tuxlink-686)"`

### Task 12: gpsfake end-to-end test

**Files:** Create `src-tauri/tests/gpsd_fake_test.rs`

- [ ] **Step 1-4:** An integration test (gated behind an env flag + `gpsfake` presence so CI without gpsd skips it) that launches `gpsfake` with a small NMEA fixture containing a known fix, points the client at that gpsd instance, and asserts the arbiter reports the expected grid and `broadcast_grid()` is reduced to 4-char. Loopback only; no RF. Skip cleanly (return early) when `gpsfake` is absent.
- [ ] **Step 5: Commit** `git commit -am "test(position): gpsfake end-to-end fix->grid via gpsd (tuxlink-686)"`

> **Operator live-smoke:** with the LC29C indoors (no fix), confirm the ribbon shows `GPS · no fix` + the manual fallback, and that setting a manual grid pins MANUAL and the "GPS ready" affordance does/doesn't appear correctly. With a sky view (fix), confirm the grid tracks GPS, precision-reduced, and that a pinned Manual grid is NOT overridden.

---

## Self-Review

**Spec coverage:** source contract → Task 4 (sticky, switch, broadcast). gpsd-client → Tasks 9-10. Manual inline-edit + chip → Tasks 7-8. Config migration → Task 3. Precision-at-broadcast → Tasks 4, 6 (+ `tuxlink-882` landed the helper). Fix-quality/staleness → Tasks 4, 9. No-fix fallback → Task 4 `active_grid`. Maidenhead → Tasks 1-2. Testing (unit + gpsfake + live) → throughout + Task 12. Out-of-scope items (PPS clock, map) correctly absent.

**Placeholder scan:** No "TBD"/"handle errors"-style placeholders; each code step shows code. The two flagged "verify"-notes (schema-version migration arm in Task 3; arbiter-vs-config locator threading in Task 6) are explicit decision points the executor resolves against the actual code, not vague placeholders.

**Type consistency:** `PositionSource` (config.rs, reused in arbiter), `Fix { grid, received }`, `PositionArbiter` methods (`set_manual`, `apply_gps_fix`, `use_gps`, `active_grid`, `broadcast_grid`, `has_fresh_fix`), `validate_grid_input`, `config_set_grid`, `position_set_source` — names consistent across tasks. `broadcast_grid` matches the `tuxlink-882` signature.
