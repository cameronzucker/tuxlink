# Rig Control — VARA Parity + Gate Fixes — Implementation Plan (extension)

> Extends `2026-06-26-rig-control-single-pane.md` (Tasks 1–12 + live-VFO, all DONE).
> Session: marsh-fjord-condor · bd tuxlink-8fkkk · branch `bd-tuxlink-8fkkk/rig-control-single-pane`.
> Source: handoff `2026-06-27-butte-crag-marten-ardop-cat-rig-done-vara-parity-next.md`.

**Goal:** bring VARA to CAT-rig parity with ARDOP, wire the inert QSY-on-fail
control for both modes, and land the 4 Codex/review fixes (C1–C4). After all
tasks: final whole-branch review → wire-walk (BOTH ARDOP + VARA) → Codex adrev → CI green.

## Global Constraints (inherit all from the base plan)

- **Rust compiles in CI, not on the Pi.** Author tests; CI runs them. `pnpm vitest run` + `pnpm exec tsc --noEmit` verify TS locally.
- **Subagents do NOT commit** (main-checkout hook resets cwd) — implementers edit + STOP dirty; the PARENT commits from the worktree cwd.
- **Clippy `-D warnings`** is the gate: `io::Error::other`, `is_some_and`/`is_none_or` (MSRV 1.75 permitting — `is_none_or` is 1.82, DO NOT use it), no needless clones, snake_case test names, struct-update syntax (no field-reassign-after-default), type aliases for complex closure returns.
- **No tuxlink-added safeguards** — mirror WLE. The existing VARA_CONNECT_DEADLINE (120 s) is legacy-parity and stays.
- **RADIO-1:** no agent runs transmit code. Author + commit; the licensee runs on-air.
- Every commit carries `Agent: marsh-fjord-condor` + the `Co-Authored-By` trailer.

## Architecture decision — CAT/rig config is radio-level (one radio, all modes)

The 7 rig fields (`rig_hamlib_model`, `rigctld_host`, `rigctld_port`,
`rigctld_binary`, `close_serial_sequencing`, `live_vfo_poll`, `qsy_on_fail`)
plus the CAT serial link (`cat_serial_path`, `cat_baud`) describe ONE physical
radio and are hoisted to a new `RigUiConfig` at top-level `Config.rig`,
consumed by both ARDOP and VARA. **Migration scope is two fields:** the 7 rig
fields are unreleased (added in this PR's Task 6, never shipped under
`[modem_ardop]`), so only `cat_serial_path` + `cat_baud` need a legacy lift.
The ARDOP CAT-PTT bridge (`cat_bridge_spec_from`) keeps `cat_key_cmd` /
`cat_unkey_cmd` / `cat_bridge_port` on `ArdopUiConfig` (PTT-method-specific) and
reads serial+baud from `Config.rig`.

---

# Task A1 — Backend: hoist rig config to shared `RigUiConfig` (+ C1 port fix)

**Files:** `src-tauri/src/config.rs`, `src-tauri/src/modem_commands.rs`, `src-tauri/src/lib.rs` (command registration).

**Interfaces produced:**
- `pub struct RigUiConfig { rig_hamlib_model: Option<u32>, rigctld_host: String, rigctld_port: u16, rigctld_binary: String, close_serial_sequencing: bool, live_vfo_poll: bool, qsy_on_fail: bool, cat_serial_path: Option<String>, cat_baud: u32 }` with `Default` + serde.
- `Config.rig: RigUiConfig` (`#[serde(default)]`).
- `#[tauri::command] config_get_rig() -> RigUiConfig` and `config_set_rig(cfg: RigUiConfig) -> Result<(), String>` (mirror the `config_get_ardop`/`config_set_ardop` pattern — read-modify-write the whole `Config`).
- `rig_config_from(rig: &RigUiConfig) -> Option<tux_rig::RigConfig>` (signature changes from `&ArdopUiConfig` to `&RigUiConfig`).
- `cat_bridge_spec_from(ardop_ui: &ArdopUiConfig, rig: &RigUiConfig) -> Result<Option<CatBridgeSpec>, String>` (now takes the rig config for serial+baud).

**Steps (TDD):**
1. **Remove** the 7 rig fields from `ArdopUiConfig` (config.rs:998–1025), its Shadow struct (1108-ish), `Default` (1049–1082), and the hand-written `Deserialize` field copies (1144–1178). Remove `default_rigctld_*` fns ONLY after moving them (they move to RigUiConfig's defaults). Keep `cat_serial_path` + `cat_baud` deserialization on the ArdopUiConfig Shadow for MIGRATION (see step 4) but do NOT keep them as live fields if moved — see step 4 for the migration shape.
2. **Add `RigUiConfig`** near `VaraUiConfig` (config.rs ~1182). Use the **same hand-written-Deserialize + Shadow pattern** the codebase uses for config structs if any field needs migration defaulting; otherwise a derived `Deserialize` with `#[serde(default = ...)]` per field is acceptable (match `VaraUiConfig`'s derived style — it has no hand-written impl). Defaults: host `127.0.0.1`, **port `4534`** (C1 FIX — must differ from `default_cat_bridge_port` = 4532; add a test asserting `RigUiConfig::default().rigctld_port != ArdopUiConfig::default().cat_bridge_port`), binary `rigctld`, flags `false`, model `None`, `cat_serial_path None`, `cat_baud 38400`.
3. **Add `rig: RigUiConfig`** to `Config` (config.rs ~194-308) with `#[serde(default)]` so old configs without `[rig]` deserialize.
4. **Migration** for the two released fields: when `Config` deserializes and `[rig]` is absent/default BUT `[modem_ardop]` carries a legacy `cat_serial_path`/`cat_baud`, lift them into `config.rig`. Implement in the `Config` load path (the existing config read/migration site — find where ArdopUiConfig's ptt_method migration runs and where Config-level post-deserialize fix-ups live; if none, do it in `read_config`/`load` after deserialize). Write tests: (a) a legacy JSON with `modem_ardop.cat_serial_path` and no `[rig]` ends up with `config.rig.cat_serial_path == Some(...)`; (b) a config with explicit `[rig]` is untouched; (c) the 7 rig fields under a legacy `[modem_ardop]` are IGNORED (unreleased — `deny_unknown_fields`? verify ArdopUiConfig does NOT use deny_unknown_fields, else legacy stray fields break load — if it does, the Shadow must `#[serde(default)]`-absorb them).
5. **Update `rig_config_from`** (modem_commands.rs:1725–1739) to take `&RigUiConfig`; read serial_path/baud/model/host/port/binary from it. Update its callers (`tune_rig_for_connect` ~1771, `ardop_tune_rig` ~989-998, the live-VFO poll spawn). Update tests (rig_config_present/absent — now build a RigUiConfig).
6. **Update `cat_bridge_spec_from`** (modem_commands.rs:1148–1170) to take `(&ArdopUiConfig, &RigUiConfig)` and read serial_path+baud from the rig config; key/unkey/bridge_port stay from ardop_ui. Update its callers in the ARDOP connect path + tests.
7. **Register** `config_get_rig` + `config_set_rig` in the `generate_handler!` list (lib.rs).
8. Tune-only command `ardop_tune_rig` (~989) reads `config.rig` instead of `cfg.modem_ardop`.

**Note:** every `ArdopUiConfig { ... }` literal in tests across config.rs + modem_commands.rs (the grep showed ~25 sites) must drop the removed fields. Use struct-update `..Default::default()` where the test only sets a few fields; otherwise delete the removed field lines. This is mechanical but wide — the implementer must compile-check mentally / let CI catch stragglers.

---

# Task C23 — Backend connect-safety fixes (C2 + C3)

**Files:** `src-tauri/src/modem_commands.rs` (C2), `src-tauri/tux-rig/src/managed.rs` (C3).

**C2 — abort-generation re-check before `connect_arq` (ARDOP).** The tune step
(modem_commands.rs ~602) adds latency between the abort snapshot and
`connect_arq` (~634), widening the abort-miss window. After the tune returns and
BEFORE `connect_arq`, re-check the close generation (the `walk_gen` /
`current_close_generation()` pattern at ~476/487) and bail to a clean
"aborted" outcome if it changed. Add a test exercising the decision (a pure
helper `aborted_since(snapshot, current) -> bool` if one doesn't exist, or assert
the walk stops when generation bumped between tune and dial).

**C3 — bound the tune reads.** `ManagedRig::spawn`/`tune` currently use
`RigctldClient::connect` (no read timeout, managed.rs:668) — a hung rigctld
blocks the connect. Switch the managed client to `connect_with_timeout` (client.rs:46)
with a sane default (e.g. 5 s, reuse `CONNECT_TIMEOUT` or a new const). Verify the
existing `spawn`/lifecycle tests still pass; add a const + keep the poll-thread
path (which already uses connect_with_timeout) consistent.

---

# Task A2 — VARA connect: pre-audio tune + candidate walk + abort recheck

**Files:** `src-tauri/src/winlink/modem/vara/commands.rs`.

The VARA connect lives in `modem_vara_b2f_exchange` (commands.rs:1541) →
`run_vara_b2f_with_transport` (~1709), which sends ONE `CONNECT <mycall> <target>`
(~1766) then waits for CONNECTED (~1770) then runs the B2F exchange.

**Interfaces:**
- `modem_vara_b2f_exchange` gains `freq_hz: Option<u64>` and `qsy_candidates: Option<Vec<DialCandidate>>` params (mirror the ARDOP command's optional candidates; reuse `crate::modem_commands::DialCandidate` — make it `pub` if not already, or define a VARA-local equivalent and convert).
- New inner that walks candidates: for each candidate (gated by `config.rig.qsy_on_fail` via `walk_candidates` from modem_commands — reuse it), tune (pre-CONNECT) then `CONNECT mycall candidate.target` + `wait_for_connected`. Stop at first success; run B2F over that transport. If none connect, return the connect error.

**Steps:**
1. Thread `freq_hz` + `qsy_candidates` through `modem_vara_b2f_exchange` → `run_vara_b2f_with_transport`. Back-compat: `None`/empty candidates → single-target behavior using `freq_hz` (today's path).
2. **Tune step:** immediately before the `CONNECT` send (~1766), if `rig_config_from(&cfg.rig)` is `Some` and a target freq is known, spawn `ManagedRig`, `tune(hz, ardop_data_mode())`, then `should_release_after_tune(&cfg.rig)` → release before CONNECT (audio) on internal-codec, else hold the `ManagedRig` as a local for the synchronous exchange (it drops — and stops rigctld — when the function returns, which is the correct DRA-100 session-scoped lifetime for VARA's single-call connect+exchange+disconnect). Reuse `ardop_data_mode()` (mode-agnostic).
3. **Candidate walk:** restructure so the connect attempt (tune + CONNECT + wait_for_connected) runs per candidate via `walk_candidates(&candidates, qsy_on_fail, |idx, c| { ... })`. Build a 1-element candidate vec from `(target, freq_hz)` when `qsy_candidates` is absent.
4. **Abort recheck (C2 for VARA):** re-check `session.current_close_generation()` against `close_gen_snapshot` after each tune, before each CONNECT send; bail clean if it changed (operator hit Close mid-walk).
5. Tests: the existing VARA command tests + a new test that `qsy_candidates` of 1 element with a freq behaves like the legacy single dial; `walk_candidates` reuse is unit-tested already. Rig-touching paths use the existing test seams (no real rig).

**Note:** `wait_for_connected`/`OutboundCommand::Connect` are the existing primitives — do not change their signatures; the walk wraps them.

---

# Task A1UI — Shared `RigControlSection` component (both panels)

**Files:** new `src/radio/modes/RigControlSection.tsx` (+ `.test.tsx`), `src/radio/modes/ArdopRadioPanel.tsx`, `src/radio/modes/VaraRadioPanel.tsx`.

The ARDOP panel has an inline Rig control expander (ArdopRadioPanel.tsx:382-391
state + ~1070+ render) bound to `ArdopFullConfig` rig fields. Extract it into a
shared component that reads/writes `Config.rig` via the new `config_get_rig` /
`config_set_rig` commands, and render it in BOTH panels so VARA reaches the same
rig config.

**Interfaces:**
- `interface RigConfig { rig_hamlib_model: number | null; rigctld_host: string; rigctld_port: number; rigctld_binary: string; close_serial_sequencing: boolean; live_vfo_poll: boolean; qsy_on_fail: boolean; cat_serial_path: string | null; cat_baud: number; }` (TS mirror of `RigUiConfig`).
- `<RigControlSection storageKeyPrefix="ardop"|"vara" />` — owns its own load (`config_get_rig` on mount) + persist (`config_set_rig` on blur/change), collapse state in localStorage keyed by prefix, collapsed by default.

**Steps:**
1. Create `RigControlSection.tsx`: model/CAT-serial/baud/close-serial/live-VFO/QSY fields, loading from `config_get_rig`, persisting via `config_set_rig`. Mirror the existing expander's field set + styling (reuse classes; no new CSS unless unavoidable).
2. Remove the rig fields from `ArdopFullConfig` (ArdopRadioPanel.tsx:262-311) and the inline rig expander; render `<RigControlSection storageKeyPrefix="ardop" />` in its place. Move CAT serial (`cat_serial_path`/`cat_baud`) editing into RigControlSection too (it's now radio-level) — but the ARDOP CAT-PTT bridge still needs the operator to set serial; RigControlSection is where they set it now. Verify the ARDOP Radio-config section's PTT serial input is distinct from CAT serial (PTT serial = `ptt_serial_path`, separate field — keep it).
3. Render `<RigControlSection storageKeyPrefix="vara" />` in VaraRadioPanel.
4. Vitest: RigControlSection loads from `config_get_rig`, renders fields, persists on change. Keep ArdopRadioPanel tests green (update for the removed inline expander).

---

# Task A3 — VaraRadioPanel: frequency element + Tune + prefill + send

**Files:** `src/radio/modes/VaraRadioPanel.tsx` (+ test).

Mirror ARDOP Tasks 10/11. VaraRadioPanel.handlePrefill is ~142-152; its connect
invoke is `modem_vara_b2f_exchange` (~405-409).

**Steps:**
1. Add a `freqMhz` state + `freqHz` memo (copy the ARDOP pattern at 335-342, but apply the C4 normalization from Task B — see below; keep A3 and the C4 helper consistent, ideally import the shared helper Task B introduces).
2. Add a frequency input + a "Tune" button (invokes `ardop_tune_rig` — it's mode-agnostic Tune-only, reads `config.rig`; reuse it, do NOT add a `vara_tune_rig`). Disabled when `freqHz === null`.
3. `handlePrefill`: set freq from `dial.freq` (via the shared normalize helper) AND clear it when `dial.freq` is absent (C4 clear-on-empty).
4. Send `freqHz` (+ `qsyCandidates` from Task B) on the `modem_vara_b2f_exchange` invoke.
5. Vitest: prefill sets freq; Tune disabled when empty; connect sends freqHz.

---

# Task B — Wire `qsyCandidates` from Find a Station ranked channels (+ C4)

**Files:** `src/catalog/channelGrouping.ts` (or a new `ranking.ts`), `src/favorites/types.ts`, `src/catalog/prefillEvent.ts`, `src/catalog/StationRail.tsx`, `src/AppShell.tsx`, both panels, a shared freq-normalize helper.

Backend is READY: `modem_ardop_connect` accepts `qsyCandidates` (modem_commands.rs:1375)
and `modem_vara_b2f_exchange` accepts it after Task A2. `qsy_on_fail` is read from
`config.rig` by the backend. B feeds the ordered list.

**Steps:**
1. **Ranking helper** — `rankedDialsFor(station, mode, prediction?, utcHour?) -> FavoriteDial[]`: the station's channels for `mode`, ordered by Find-a-Station ranking (reliability desc via `channelReliability`/`PathPrediction` when available, else frequency ascending — the existing `groupChannelsByMode` order). Cap to a sane top-N (e.g. 5). Unit-test the ordering (reliability beats frequency; fallback is frequency-asc).
2. **Carry candidates through prefill** — extend the prefill payload (FavoriteDial-based event in `prefillEvent.ts`) to carry an ordered `candidates?: FavoriteDial[]` alongside the primary `dial`. Update `emitGatewayPrefill` + `listenGatewayPrefill` signatures.
3. **StationRail.onUse / AppShell.handleStationUse** (StationRail.tsx:79-87 → AppShell.tsx:1415-1424): compute `rankedDialsFor(...)` and pass it as `candidates` through `emitGatewayPrefill`.
4. **Panels store + send** — both panels' `handlePrefill` stores `candidates`; `doConnect`/the connect invoke sends `qsyCandidates = candidates.map(d => ({ target: d.gateway, freq_hz: hzFromDial(d) }))`. When no candidates (manual target), send the single `{target, freqHz}` as today (1-element).
5. **C4 — freq normalize + clear-on-empty** (shared helper `dialFreqToMhz(dial): string | null` and `parseFreqToHz(input): number | null`): the prefill parse must handle both MHz display strings ("7.103") AND kHz values from saved favorites ("14105.0") — normalize by magnitude (>= ~1000 ⇒ kHz ⇒ ÷1000 to MHz; else MHz) OR, preferred, carry a numeric `freqHz`/`freqKhz` on the dial and avoid re-parsing a display string. Apply in BOTH panels' handlePrefill; replace the raw `/[\d.]+/` regex at ArdopRadioPanel.tsx:444-447. Clear `freqMhz` when `dial.freq` (and any numeric freq) is absent. Unit-test: kHz favorite → correct MHz; MHz dial → unchanged; absent → cleared.

---

# After all tasks — gates (do not claim "done" before all pass)

1. Final whole-branch review (most capable model) over `git merge-base main HEAD`..HEAD.
2. **wire-walk** skill — trace BOTH the ARDOP and VARA flows end-to-end to code (operator supplies flows; do not draft them).
3. Cross-provider **Codex** adrev (`dev/adversarial/2026-06-27-vara-parity-codex.md`, gitignored).
4. CI green on the final HEAD (verify + build-linux, both arches).
5. Mark PR #922 ready; operator merges (no-squash, ADR 0010).
