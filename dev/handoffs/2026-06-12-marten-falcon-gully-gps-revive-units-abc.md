# Handoff — GPS setup-assistance revival (tuxlink-9xy1) + s0r1 shipped — marten-falcon-gully

Date: 2026-06-12 · Agent: marten-falcon-gully

## TL;DR

Two threads this session:

1. **Find-a-Station 3 real-app fixes (tuxlink-s0r1) — SHIPPED.** PR #618 merged to
   main (07:02Z). NOT yet in a release (it landed ~1s after v0.55.0 was tagged);
   the next release-please run cuts it (no release PR was open at handoff — may
   need `gh workflow run release-please.yml`).
2. **GPS setup assistance (tuxlink-9xy1) — REVIVED, 3 of 4 units done + CI-green;
   wizard chrome remains.** PR #631 (draft).

## tuxlink-9xy1 — GPS setup assistance

### Why this exists
Linux GPS (dialout group, gpsd, ModemManager grabbing the serial port) is a
notorious first-run wall — a real win over legacy WLE. The work was filed as a
4-slice epic, slice 1 = `tuxlink-9xy1`. It had been **claimed then stalled**: the
old branch `bd-tuxlink-9xy1/gps-foundation` (agent magpie-isthmus-gorge,
2026-06-05) committed only the `WizardPhase` foundation, then went 989 commits
behind main, never merged. The design docs (`2026-06-04-gps-foundation-design.md`,
`2026-06-05-gps-setup-ux-design.md`) were **never committed** — the design
survives only in the bd issue bodies (9xy1 + slices m9ej/ley0/gnws).

### Evaluation of the recovered shape (operator asked)
The `WizardPhase` model (None→Identity→Complete, replacing the all-or-nothing
`wizard_completed` boolean so Location is a resumable phase) is **good** —
Codex-reviewed ("CODEX-1"), tested, careful legacy-migration compat in
`useWizardPhase.ts`. But it's foundation-only (~20%) and 989 behind main, so the
call was **re-implement the proven design against current main**, not rebase the
corpse. The recovered code is safe on origin `bd-tuxlink-9xy1/gps-foundation`:
- `src-tauri/src/wizard_phase.rs` (54 lines, applies ~verbatim)
- `src/wizard/useWizardPhase.ts` (117 lines — the dual-probe legacy-compat hook)

### DONE this session (PR #631, branch `bd-tuxlink-9xy1/gps-setup-assist`, CI-green)
- **Unit A — detection probes** (`src-tauri/src/position/probe.rs`): `gps_probe_gpsd`
  (TCP 2947, 200ms), `gps_probe_serial_devices` (udevadm vendor/model),
  `gps_probe_dialout`, `gps_probe_modemmanager`. Shells `udevadm`/`id`/`getent`/
  `systemctl` — **no libudev dep** (the stalled branch's udev crate was never
  CI-verified). Pure parsers unit-tested. Registered in lib.rs.
- **Unit B — `GpsSourcePicker`** (`src/location/`): `gpsProbes.ts`
  (bindings + pure `classifyGpsSources`) + the component (source cards / triage
  cards with copy-pasteable fix commands / manual-grid). "Fix it for me" ships
  **disabled** (slice 2 = `tuxlink-m9ej` pkexec helper). 14 tests.
- **Unit C — Settings → Location** (`src/location/LocationSettings.tsx` rendered in
  `src/shell/SettingsPanel.tsx`): **GPS setup assistance is now REACHABLE +
  working in the shipped app.** Picking a source → `position_set_source` (live
  arbiter switch); editing grid → `config_set_grid` (validated). 23 tests green
  incl. the unbroken existing SettingsPanel suite.
- Gates: tsc green; vitest green; CI verify (clippy+cargo+vitest) PASS amd64 +
  build-linux PASS arm64 (other two arches pending, same code).

### REMAINING — Unit D: wizard Location step (the second chrome)
The operator wants GPS assistance in **both** Settings (done) **and** the wizard.
The wizard chrome is the remaining piece. It's wizard-flow surgery + a decision,
deliberately left for fresh focus rather than the tail of a marathon session.

**Decision to make first:** D-minimal vs D-full.
- **D-minimal:** add a `'location'` step to the existing `WizardStep` reducer +
  render `GpsSourcePicker` in a wizard chrome; persist grid on Continue. No
  backend phase machine. Gets GPS into onboarding (the operator's core ask).
- **D-full:** also port `WizardPhase` (from `bd-tuxlink-9xy1/gps-foundation`) for
  resumable onboarding. More work; honors the recovered design.

**Integration points (all identified):**
- `src/App.tsx:71` — wizard-vs-shell routing on `invoke('get_wizard_completed')`.
  D-full swaps this for the recovered `useWizardPhase` dual-probe.
- `src/wizard/types.ts` — `WizardStep` union (account/credentials/offline_identity/
  cms_verify/complete) → insert `'location'`.
- `src/wizard/wizardReducer.ts` — flow transitions; insert location after identity.
- **Grid currently lives in `Step2Credentials.tsx` + `Step2OfflineIdentity.tsx`.**
  The recovered design moves it OUT into the dedicated Location step. That touches
  those steps' tests — budget for it.
- `GpsSourcePicker` is parent-controlled (`grid` + `selectedSource`), so the
  wizard chrome wires its own persistence (the wizard's grid-persist path; see
  `wizard_persist_offline`/`wizard_persist_cms`).

**Then:** mark PR #631 ready; real-WebKitGTK smoke the new UI (Settings →
"Location & GPS source" + the wizard step) — it's brand-new render surface; the
s0r1 lesson is Chromium/vitest miss WebKitGTK render/CSP issues.

## Worktrees
- **Active:** `worktrees/bd-tuxlink-9xy1-gps-setup-assist` (main root, off main,
  PR #631). `node_modules` installed. No `target/` yet (Rust verified via CI).
- **Stale:** `worktrees/bd-tuxlink-9xy1-gps-foundation` (989 behind, the recovered
  4 commits — safe on origin). Dispose via the ADR-0009 ritual when convenient.
- s0r1 worktree is merged → disposable.

## Operator notes
- Local cargo is unusable under the ~6-session Pi contention — all Rust
  verification is via CI on the draft PR ([[feedback_no_cold_cargo_on_contended_pi]]).
- s0r1's three fixes need the operator's real-app smoke + (if wanted) a
  release-please nudge to cut a version containing them.
