# FT-8 MCP Surface Implementation Plan

**Goal:** Give Elmer tools to arm, configure, and read the FT-8 listener, so "who am I hearing on 20m?" is answered from real decodes instead of fabricated.

**Issue:** `tuxlink-dof5j` (P1). Branch `bd-tuxlink-dof5j/ft8-mcp-surface`.

**Architecture:** FT-8 is fully built at the Tauri layer (12 commands over a managed `Arc<Ft8ListenerState>`), and **none** of it is bridged to MCP. Add an `Ft8Port` trait + `MonolithFt8Port` impl reusing the same `*_inner(&Ft8ListenerState)` seam the Tauri commands use, and register six agent-shaped tools.

## Global Constraints

- **Worktree:** `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-dof5j-ft8-mcp-surface`. Pin absolute paths; the shell cwd silently reverts to the main checkout.
- **DO NOT COMMIT** (subagents). Write code, STOP, report. The parent commits.
- **DO NOT RUN cargo.** This Pi cannot finish a cold build. CI is the compiler. Verify by reading.
- **MSRV 1.75.** Clippy `-D warnings` (no unused imports).
- **`Agent: sumac-magnolia-fen`** trailer, inline heredoc (parent only).

## Load-bearing decisions (already made — do not relitigate)

**1. FT-8 decodes do NOT taint. No new `TaintReason` variant.**
Operator-corrected. FT-8's payload is 77 bits over a fixed message-type set
(`tuxlink-ft8/src/message.rs:131`); `Standard` messages are packed callsign/grid/report
FIELDS, and `FreeText` is hard-capped — `message.rs:175` rejects anything that "exceeded
13 characters or held an out-of-alphabet character." A prompt injection does not fit in 13
chars of a restricted alphabet. Tainting would block egress after listening, breaking the
actual FT-8 loop (listen, then work the station you heard) to defend a threat the channel
cannot carry. **Calibrate the threat model to the channel's capacity, not the field's type.**

**2. Nothing here is egress/arm-gated.** The FT-8 listener is receive-only; it never keys
the transmitter. RADIO-1 governs transmit. `ft8_set_band` DOES move the dial via CAT — a
real-world side effect but not a transmission, and `rig_tune` is already on the agent
surface with exactly that character.

**3. Do NOT expose `Ft8Snapshot` raw.** It is a UI struct: 40 `SlotRecord`s, health flags,
sweep-dwell progress, device lists. Handing that to a small local model is a context bomb of
telemetry noise. The agent gets purpose-shaped DTOs instead.

## Tools (6)

| Tool | Kind | Taint |
|---|---|---|
| `ft8_status` | read — is it listening, on what band/dial, what's blocking it | no |
| `ft8_heard_stations` | read — **the value tool**: deduped stations heard | no |
| `ft8_start_listening` | control (RX only) | no |
| `ft8_stop_listening` | control | no |
| `ft8_set_band` | control (QSYs via CAT) | no |
| `ft8_list_audio_devices` | read — for "which device do I use?" | no |

---

### Task 1: `Ft8Port` trait + DTOs

**Files:** `src-tauri/tuxlink-mcp-core/src/ports.rs`

**Produces** (later tasks use these exact names):

```rust
/// One station heard on FT-8, aggregated across the decode ring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ft8HeardStationDto {
    pub call: String,
    pub grid: Option<String>,
    /// Best (highest) SNR seen for this station, in dB.
    pub best_snr_db: i32,
    /// Audio frequency of the most recent decode, in Hz.
    pub freq_hz: u32,
    pub band: String,
    pub last_heard_utc_ms: u64,
    /// How many times this station was decoded in the retained window.
    pub times_heard: u32,
}

/// Listener state, agent-shaped (NOT the UI's Ft8Snapshot).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ft8StatusDto {
    /// "stopped" | "starting" | "listening" | "yielded" | "blocked" | "stopping"
    pub state: String,
    /// Present only when `state == "blocked"`; why it cannot listen.
    pub blocked_reason: Option<String>,
    pub band: String,
    pub dial_hz: u64,
    pub sweep_enabled: bool,
    pub device_name: Option<String>,
    pub last_slot_utc_ms: Option<u64>,
    pub last_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ft8AudioDeviceDto {
    pub human_name: String,
    pub stable_id: String,
}

/// FT-8 listener. Receive-only: nothing here keys the transmitter, so nothing
/// here is egress-gated. Decodes do not taint (see plan §Load-bearing decisions).
#[async_trait]
pub trait Ft8Port: Send + Sync {
    async fn status(&self) -> Result<Ft8StatusDto, PortError>;
    async fn heard_stations(&self) -> Result<Vec<Ft8HeardStationDto>, PortError>;
    async fn start(&self) -> Result<(), PortError>;
    async fn stop(&self) -> Result<(), PortError>;
    async fn set_band(&self, band: &str) -> Result<(), PortError>;
    async fn list_audio_devices(&self) -> Result<Vec<Ft8AudioDeviceDto>, PortError>;
}
```

**Steps:**
- [ ] Add the three DTOs + the trait to `ports.rs`, matching the file's existing `#[async_trait]` style exactly.
- [ ] Add `pub ft8: Arc<dyn Ft8Port>,` to `McpState` in `tuxlink-mcp-core/src/lib.rs`, with a doc comment: *"FT-8 listener. Receive-only; none taint, none egress-gated."*
- [ ] **EXHAUSTIVENESS (this broke CI last time):** `McpState` gains a field, so EVERY construction site must supply it. Run
      `grep -rn "McpState {" /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-dof5j-ft8-mcp-surface/src-tauri --include=*.rs`
      and add a mock `Ft8Port` to each (mcp-core's test module, `tuxlink-mcp-testserver/src/mocks.rs`, `scenario_ports.rs` if present, and the real wiring in `src-tauri/src/mcp_ports.rs`). Report every site you found.

---

### Task 2: `MonolithFt8Port` + the heard-stations aggregation

**Files:** `src-tauri/src/mcp_ports.rs`

**Consumes:** the trait + DTOs from Task 1. The FT-8 service seam:
- Managed state is `Arc<Ft8ListenerState>` (Tauri-managed; get it with `app.try_state::<Arc<Ft8ListenerState>>()`, degrade to `PortError::Unavailable` on miss — mirror `MonolithSearchPort`).
- `state.snapshot() -> Ft8Snapshot` (`src-tauri/src/ft8/service.rs:1369`).
- The Tauri commands are thin shells over `ft8_listener_start_inner(&state)`, `ft8_listener_stop_inner(&state)`, `ft8_set_band_inner(&state, band)` in `src-tauri/src/ft8/commands.rs` — **reuse those `_inner` fns**, do not reimplement. They are blocking; wrap in `tauri::async_runtime::spawn_blocking` exactly as the commands do.

**The aggregation is the substance of this task.** `Ft8Snapshot.ring_tail: Vec<SlotRecord>`; each `SlotRecord` has `decodes: Vec<DecodeDto>` (non-empty only when `outcome == Decoded`), plus `band`, `slot_utc_ms`. `DecodeDto` (`src-tauri/src/ft8/records.rs`) has `snr_db: i32`, `freq_hz: u32`, `from_call: Option<String>`, `grid: Option<String>`, `slot_utc_ms: u64`, `partial: bool`.

Fold the ring into deduped stations:
- Key on `from_call`; **skip decodes whose `from_call` is `None`** (unparsed/partial — a station we cannot name is not a heard station).
- `best_snr_db` = max SNR seen for that call.
- `times_heard` = number of decodes for that call.
- `grid` = the first `Some(grid)` seen (grids do not change; a later decode omitting it must not erase it).
- `freq_hz` / `band` / `last_heard_utc_ms` = from the **most recent** decode (highest `slot_utc_ms`).
- Sort the result **most-recently-heard first** — that is the order an operator asks in.

- [ ] Implement `MonolithFt8Port` with all six methods.
- [ ] Extract the fold into a **free function** `fn aggregate_heard(ring: &[SlotRecord]) -> Vec<Ft8HeardStationDto>` so it is unit-testable without Tauri state.
- [ ] Unit-test `aggregate_heard` directly. Cover: two decodes of the same call keep the BEST snr and the LATEST freq/time and `times_heard == 2`; a `from_call: None` decode is skipped; a grid seen once then absent is retained; output is sorted most-recent-first; an empty ring yields an empty vec.

---

### Task 3: Router tools

**Files:** `src-tauri/tuxlink-mcp-core/src/router.rs`

**Consumes:** `Ft8Port` (Task 1) via `self.state.ft8`.

Register six `#[tool]`s **inside** the existing `#[tool_router] impl TuxlinkMcp` block (a tool declared outside it is never registered — verify by line number). Mirror the `docs_search` / `vara_status` style. **None call `guard.taint(..)`. None call `guarded_egress(..)`.**

Descriptions must be self-explanatory to a small model with no domain knowledge — say what the tool answers, not just what it wraps:

- `ft8_status` — *"Report the FT-8 listener's state: whether it is listening, on which band and dial frequency, which audio device, and what is blocking it if it cannot start. Call this before ft8_heard_stations if no stations come back — the listener may simply not be running. Receive-only. Does not taint. Read-only."*
- `ft8_heard_stations` — *"List the amateur stations heard on FT-8 recently, deduplicated: callsign, Maidenhead grid, best signal-to-noise ratio in dB, how many times heard, and when last heard. This is how to answer 'who am I hearing' / 'what stations are on this band'. Requires the listener to be running (see ft8_start_listening). Returns an empty list if nothing has decoded yet. Read-only; does not taint."*
- `ft8_start_listening` — *"Start the FT-8 listener on the configured band and audio device. RECEIVE-ONLY: this does not transmit and does not require send authority. Returns an error naming what is missing if no audio device is configured."*
- `ft8_stop_listening` — *"Stop the FT-8 listener and release the audio device."*
- `ft8_set_band` — *"Set the FT-8 band (e.g. \"20m\", \"40m\"). If rig CAT control is configured this QSYs the radio's dial to that band's FT-8 frequency. Does not transmit."*
- `ft8_list_audio_devices` — *"List the audio capture devices the FT-8 listener can use, with the stable id to select. Use when ft8_status reports it is blocked needing a device."*

- [ ] Add a `BandParams { band: String }` params struct beside the existing param structs.
- [ ] Register all six tools.
- [ ] Add `ft8_status`, `ft8_heard_stations`, `ft8_list_audio_devices` to the existing `non_taint_tools_do_not_taint` test in `router.rs` — the point of the taint decision is that it is asserted, not merely intended.

---

### Task 4 (parent): FT-8 knowledge doc + wire-walk + PR

No FT-8 documentation exists anywhere in the corpus, so Elmer is blind in BOTH tiers — no tool AND no doc. The tool half without the doc half repeats the gap this branch exists to close.

- [ ] Write `docs/user-guide/37-ft8.md` (user-guide, so it also reaches the Help sidebar) covering: what FT-8 is and what it is for in an emcomm context (propagation sensing / who can hear whom — **not** a messaging mode), the listener's receive-only nature, band selection + sweep, audio-device selection, reading SNR, and the fact that FT-8 carries no message traffic (77-bit payload; free text capped at 13 chars).
- [ ] Register it in `BUNDLED_TOPICS` (`source: DocSource::UserGuide`) — the registry-drift test from PR #1091 FAILS if you forget, which is the point.
- [ ] Add it to a `SECTIONS` group in `src/help/topics.ts`, or `buildTopics()` throws and the Help window breaks.
- [ ] wire-walk, operator-supplied flows, then PR.
