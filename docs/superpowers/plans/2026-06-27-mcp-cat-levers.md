# MCP CAT-control levers (tuxlink-wxwlr) — implementation plan

Session: marsh-fjord-condor · bd tuxlink-wxwlr · branch `bd-tuxlink-wxwlr/mcp-cat-levers` (off merged main).
Operator decision (2026-06-27): FULL scope C — read-only `rig_status` + `config_read_rig`, egress
connect parity (freqHz/qsyCandidates), and a `rig_tune` egress tool. **`rig_tune` rides the SAME
armed-send-authority gate as the other egress tools** (one operator surface) — tuning exposes the
same "could transmit" authority class as sending.

## Global constraints
- Rust compiles in CI, not on this Pi (cold worktree). Author + tests; CI gates clippy `-D warnings` (MSRV 1.75) + cargo test. `pnpm vitest`/`tsc` only if TS changes (this is backend-only — no TS).
- Subagent edits + STOPs dirty; PARENT commits (main-checkout hook).
- Adding a port-trait method forces edits at the monolith impl AND the testserver mock — do BOTH or it won't compile.

## Design decisions (locked)
- **qsy_candidates DTO** = `QsyCandidateDto { target: String, freq_hz: Option<u64> }` mirroring backend `crate::modem_commands::DialCandidate` (serde: no rename_all → snake_case `freq_hz`, `target`). MCP tool param `qsy_candidates: Option<Vec<QsyCandidateDto>>`; map to `Vec<DialCandidate>` at the monolith impl. Empty/None → single dial (today's behavior).
- **rig_status source** = a new `#[tauri::command] rig_probe_status() -> Result<RigProbe, String>` in modem_commands.rs that mirrors `ardop_tune_rig`'s spawn pattern but calls `rig.status()` instead of `tune()`, then drops. Returns the live VFO/mode/PTT. The StatusPort `rig_status()` calls it best-effort: on ANY error (unconfigured, serial busy, rigctld absent) report `vfo_hz/mode/ptt = None` + `configured` from config. NEVER transmits (CAT read only). Document the serial-contention caveat (a live DRA-100 session holding rigctld → probe fails → None) in the tool description.

## Tasks (one cohesive pass; the Explore map has every file:line + pattern)

### T1 — DTOs + port-trait methods (mcp-core: src-tauri/tuxlink-mcp-core/src/ports.rs)
- Add `RigStatusDto { vfo_hz: Option<u64>, mode: Option<String>, ptt: Option<bool>, configured: bool }`.
- Add `RigConfigDto { rig_hamlib_model: Option<u32>, rigctld_host: String, rigctld_port: u16, rigctld_binary: String, close_serial_sequencing: bool, live_vfo_poll: bool, qsy_on_fail: bool, cat_serial_path: Option<String>, cat_baud: u32 }`.
- Add `QsyCandidateDto { target: String, freq_hz: Option<u64> }` (derive Serialize+Deserialize+JsonSchema as the others; snake_case).
- `StatusPort`: add `async fn rig_status(&self) -> Result<RigStatusDto, PortError>;`
- `ConfigPort`: add `async fn rig(&self) -> Result<RigConfigDto, PortError>;`
- `EgressPort`: add `async fn rig_tune(&self, freq_hz: u64) -> Result<(), EgressPortError>;` AND extend the 3 connect/b2f methods with `freq_hz: Option<u64>, qsy_candidates: Option<Vec<QsyCandidateDto>>`:
  - `ardop_connect(&self, target, freq_hz, qsy_candidates)`
  - `ardop_b2f_exchange(&self, target, intent, freq_hz, qsy_candidates)`
  - `vara_b2f_exchange(&self, target, intent, freq_hz, qsy_candidates)`
  (rig_tune takes only freq_hz — a single tune target; no candidate walk for a bare tune.)

### T2 — Router tools (mcp-core: src-tauri/tuxlink-mcp-core/src/router.rs)
- `rig_status` (read-only) — mirror `modem_get_status`; desc: "Report the rig's configured state and, best-effort, its live VFO frequency/mode/PTT via a transient rigctld read (never transmits; may report nulls if the rig is unconfigured or its serial is busy with an active session). Read-only."
- `config_get_rig` (read-only) — mirror `config_get_ardop`; desc: "Read the non-secret radio-level rig config (hamlib model, rigctld endpoint, CAT serial, close-serial/live-vfo/qsy flags). Read-only; no secrets." (Name it `config_get_rig` to match the `config_get_ardop/vara/packet` naming already in router.rs.)
- `rig_tune` (EGRESS) — mirror `ardop_connect`; `Parameters<RigTuneParams { freq_hz: u64 }>`; desc: "Tune the rig to a frequency (set VFO + data mode) over CAT. EGRESS (commands the radio — same authority class as a transmit): requires armed send-authority and an un-tainted session; denied otherwise."
- Extend `TargetParams` (+ `ExchangeParams`) with `#[serde(default)] freq_hz: Option<u64>` and `#[serde(default)] qsy_candidates: Option<Vec<QsyCandidateDto>>`; pass them through in `ardop_connect`/`ardop_b2f_exchange`/`vara_b2f_exchange` tool bodies.
- Tools auto-register via `#[tool_router]` — no manual list.

### T3 — Monolith impls (src-tauri/src/mcp_ports.rs)
- `MonolithStatusPort::rig_status` — call the new `rig_probe_status` backend command best-effort; build RigStatusDto (configured from `read_config().rig`).
- `MonolithConfigPort::rig` — read `config::read_config()?.rig`, map to RigConfigDto.
- `MonolithEgressPort::rig_tune` — `guarded_egress(&self.guard, EgressAuthority::Agent, "rig_tune", &audit, || async move { ardop_tune_rig(freq_hz).map_err(Failed) })` — the EXACT gate pattern as `ardop_connect`.
- Thread freq_hz + qsy_candidates through `ardop_connect` (→ modem_ardop_connect), `ardop_b2f_exchange`, `vara_b2f_exchange` (map `Option<Vec<QsyCandidateDto>>` → `Option<Vec<DialCandidate>>`). Replace today's hardcoded `None, None`.

### T4 — Backend probe command (src-tauri/src/modem_commands.rs)
- `#[tauri::command] pub fn rig_probe_status() -> Result<RigProbe, String>` mirroring `ardop_tune_rig`: `rig_config_from(&cfg.rig)?` → `ManagedRig::spawn` → `rig.status()` → drop. Return a small `RigProbe { vfo_hz: u64, mode: Option<String>, ptt: bool }` (serde). Register in lib.rs generate_handler! (it's also a usable GUI command). NEVER transmits.

### T5 — Testserver mocks (src-tauri/tuxlink-mcp-testserver/src/mocks.rs)
- MockStatus::rig_status, MockConfig::rig, MockEgress::rig_tune (gated("rig_tune")), + update the 3 connect/b2f mock signatures for the new params. Use realistic fixture values.

### T6 — Tests (mcp-core router tests + any port tests)
- rig_status / config_get_rig return JSON (mock-backed).
- rig_tune is EGRESS-gated: denied when not armed, allowed when armed (mirror the existing ardop_connect gate test — find it in the router/integration tests + add a rig_tune case).
- The parity params deserialize (a connect call with freq_hz + qsy_candidates parses).

## After: gates
- Push draft PR (CI compiles the Rust). Cross-provider **Codex adrev** (egress/security surface — the rig_tune gate is the focus). Wire-walk note: the MCP "flow" = an armed agent calls rig_tune → gate → ardop_tune_rig (registration is auto; the gate is the reachability+safety check). CI green → mark ready → operator merge.
