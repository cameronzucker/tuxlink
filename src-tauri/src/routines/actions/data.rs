//! `data.spacewx_wwv` / `data.spacewx_swpc` / `data.stationlist_update` /
//! `data.read` — spec §6 "Update space weather from WWV" (radio actions
//! table), "Update space weather (SWPC)" / "Update station list" (internet
//! actions table), and "Read data" (local actions table) (plan Task 4c).
//! Every impl here delegates through the narrow [`super::DataService`] port
//! declared in `actions/mod.rs`; NONE of this file re-implements WWV
//! capture/STT, the NOAA SWPC fetch, or the Winlink catalog poll — those
//! live behind the real seams [`MonolithDataService`] below wraps.
//!
//! ## Recon: the real seams (plan Task 4c)
//!
//! - **WWV off-air decode** (`wwv_offair::commands::wwv_offair_refresh`) is
//!   the ALREADY-SHIPPED off-air capture-decode flow: tune (if CAT
//!   configured), capture 70 s via `arecord`, restore, transcribe via
//!   Whisper, ingest into the persisted solar snapshot. **It captures
//!   IMMEDIATELY when invoked** — it has NO notion of the WWV (:18) / WWVH
//!   (:45) broadcast schedule (see that module's own doc comment). The
//!   schedule lives entirely on the FRONTEND today
//!   (`src/wwv/window.ts`'s `nextCapture` + `src/wwv/useWwvOffair.ts`'s
//!   `arm()`/`setTimeout`) — a routine step calling `wwv_offair_refresh`
//!   directly at an arbitrary instant would almost always capture silence
//!   between bulletins and come back `no_copy`.
//!
//!   Spec §6's WWV row is explicit that the schedule IS part of this
//!   action's contract ("capture at :18/:45"), so [`SpaceWxWwv::execute`]
//!   below ports `nextCapture`'s pure scheduling math to Rust
//!   ([`next_capture`], unit-tested against the same fixtures as
//!   `src/wwv/window.test.ts`) and sleeps (cancellably) until the window
//!   before calling [`DataService::wwv_capture`] — a REAL schedule-aware
//!   wait+capture, not a half-wired immediate call dressed up as the
//!   spec'd behavior. **This has a real, documented consequence for routine
//!   authors:** the engine's default step timeout (300 s,
//!   `actions/mod.rs`'s `DEFAULT_TIMEOUT_S`) is far shorter than the
//!   longest possible wait for the next window (up to ~3600 s) — an author
//!   MUST set `timeout_s` to cover the worst-case wait plus the ~70 s dwell
//!   plus STT model load (a generous `timeout_s: 3900` covers it), or the
//!   engine's own `tokio::time::timeout` (`executor.rs`) aborts the step
//!   before the window ever arrives. This is stated here and in
//!   [`SpaceWxWwv`]'s own doc, not silently discovered by a routine author
//!   at 03:00.
//!
//!   The arbiter lease is acquired ONLY for the tune/capture/restore cycle
//!   itself, not for the wait — spec §6's own wording ("RX-only but seizes
//!   the rig") describes the capture, not an idle wait; holding the lease
//!   for up to an hour of waiting would needlessly block the operator's own
//!   interactive rig use and any other routine step for no operational
//!   reason.
//!
//! - **SWPC online fetch** (`propagation::commands::propagation_update_solar`)
//!   is a clean, already-wired, no-`State`-param async command: fetches
//!   NOAA's `predicted-solar-cycle.json` + `wwv.txt`, applies + persists via
//!   `solar_update::apply_swpc_update`. [`MonolithDataService::swpc_refresh`]
//!   calls it directly.
//!
//! - **Winlink gateway status API refresh**
//!   (`catalog::commands::catalog_fetch_stations`) is the polite-client
//!   cache-backed poll of `cms.winlink.org:444/listings/*.aspx` — the SAME
//!   seam Find-a-Station uses. Each fetched `StationListing.gateways` entry
//!   carries `frequencies_khz: Vec<f64>` — **this is the exact per-gateway
//!   HF channel data `radio.rs`'s module doc names as the missing input for
//!   the ARDOP/VARA gateway-frequency resolver** (`radio.rs`'s "What Task
//!   5's session wiring must provide" note). This action is what actually
//!   populates that cache; a future `GatewayFrequencyResolver` reads
//!   `catalog::stations_cache::StationsCache`'s already-populated entries
//!   (never triggering its own fetch from inside `radio.connect`'s lease),
//!   fed by whatever `data.stationlist_update` (this action) or the
//!   Find-a-Station UI most recently stored there.
//!
//! - **`data.read`'s `inbox_summary`** delegates to
//!   `ui_core::mailbox::list_mailbox` (the same core fn `mailbox_list`
//!   itself calls) against the Inbox folder, reducing `MessageMetaDto.unread`
//!   into a count. **`grid`** delegates to `ui_commands::position_status`'s
//!   `ui_grid` field — per `feedback_location_grid_source`, the status-bar's
//!   grid comes from LIVE `position_status`, never `config_read`.
//!   **`space_weather`** delegates to `wwv_offair::commands::wwv_offair_snapshot_read`
//!   (the same persisted `SolarSnapshot` the WWV panel's conditions readout
//!   shows — provenance-tagged `"swpc"` / `"rf-wwv"` / `"rf-wwv-voice"` /
//!   `"rf-wwv-manual"`, whichever wrote it last).
//!
//! - **`data.read`'s `heard_stations` has NO backend seam — genuine,
//!   documented gap.** APRS heard-station positions are accumulated
//!   CLIENT-SIDE ONLY, in `src/aprs/useAprsPositions.ts`'s React state, from
//!   the `aprs-position:new` event stream. There is no backend store a Rust
//!   action can read; [`DataRead::execute`] below does not fake one.
//!
//! - **`data.read`'s `last_connected_gateway` — RESOLVED (plan 2 Task 5c).**
//!   The peer/gateway of the last successful connect used to exist only
//!   TRANSIENTLY, inside `BackendStatus::Connected { peer, since_iso }`
//!   (`winlink_backend.rs`) while the session was live. `crate::connection_history`
//!   now persists `{callsign, transport, at_unix}` to
//!   `last-connected-gateway.json` (beside `config.json`) on every
//!   successful packet dial (`winlink_backend.rs`'s `packet_connect_inner`),
//!   ARDOP B2F exchange (`modem_commands.rs`'s `modem_ardop_b2f_exchange`),
//!   and VARA B2F exchange (`vara/commands.rs`'s `modem_vara_b2f_exchange`)
//!   — the SAME three "session/exchange completion" chokepoints
//!   `radio.rs`'s HF gateway-frequency resolver reads for its OWN gateway
//!   data. `DataRead::execute` still returns an honest error (not a silent
//!   `null`) when nothing has EVER been recorded yet (a fresh install/config
//!   dir) — see [`LAST_CONNECTED_GATEWAY_NO_RECORD`].
//!
//! Plan: `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`
//! Task 4. Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §6.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor};
use tuxlink_routines::error::StepError;

use crate::routines::arbiter::RadioArbiter;

use super::{
    busy_policy_from_params, rig_id_from_params, run_holder_from_params, step_timeout_from_params,
    DataService, InboxSummaryDto, LastConnectedGatewayDto, StationlistOutcome, SwpcOutcome,
    WwvCaptureOutcome,
};

// ============================================================================
// WWV/WWVH broadcast-window scheduling — a Rust port of `src/wwv/window.ts`'s
// `nextCapture`. Same constants, same decision logic, unit-tested against
// the same fixtures as `src/wwv/window.test.ts` so the two stay in sync by
// construction rather than by hoping nobody edits one without the other.
// ============================================================================

/// WWV's space-weather voice bulletin airs at :18 past the hour; WWVH's at
/// :45. Start capture 5 s early to catch the whole ~45 s announcement.
const WWV_START_S: u64 = 18 * 60 - 5; // 1075
const WWVH_START_S: u64 = 45 * 60 - 5; // 2695
/// Matches the backend capture dwell (`wwv_offair_refresh`'s `Duration::from_secs(70)`).
const CAPTURE_SPAN_S: u64 = 70;
const HOUR_S: u64 = 3600;

pub(crate) struct NextCapture {
    /// Seconds to sleep before firing capture; `0` when a window is active
    /// right now.
    pub delay_s: u64,
    /// `"WWV :18"` | `"WWVH :45"` — echoed into the honest-failure/journal
    /// context, mirrors `window.ts`'s `NextCapture.label`.
    pub label: &'static str,
}

/// When to fire the off-air capture, given `now_s` (unix seconds). If
/// already inside a window's capture span, fire now (`delay_s: 0`);
/// otherwise schedules to the nearest upcoming window start (this hour or
/// next). Direct port of `src/wwv/window.ts`'s `nextCapture` — see that
/// file's doc comment for the schedule rationale.
pub(crate) fn next_capture(now_s: u64) -> NextCapture {
    let into_hour = now_s % HOUR_S;
    let hour_start = now_s - into_hour;
    let windows: [(u64, &str); 2] = [(WWV_START_S, "WWV :18"), (WWVH_START_S, "WWVH :45")];

    for (start, label) in windows {
        if into_hour >= start && into_hour < start + CAPTURE_SPAN_S {
            return NextCapture { delay_s: 0, label };
        }
    }

    let mut best: Option<(u64, &str)> = None;
    for (start, label) in windows {
        let abs = if into_hour < start {
            hour_start + start
        } else {
            hour_start + HOUR_S + start
        };
        let replace = match best {
            Some((b, _)) => abs < b,
            None => true,
        };
        if replace {
            best = Some((abs, label));
        }
    }
    let (abs, label) = best.expect("windows is non-empty, so best is always set");
    NextCapture {
        delay_s: abs - now_s,
        label,
    }
}

/// Real wall-clock `now` in unix ms, for [`build_registry`]'s
/// [`SpaceWxWwv::new`] wiring. A plain `fn() -> u64` (not a closure) so it
/// can be swapped for a fixed test clock the same way
/// [`crate::routines::arbiter::RadioArbiter::new`] takes a `now: fn() -> i64`
/// — tests never sleep real wall-clock seconds waiting for a broadcast
/// window.
pub(crate) fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ============================================================================
// data.spacewx_wwv
// ============================================================================

const DATA_SPACEWX_WWV: &str = "data.spacewx_wwv";

/// `data.spacewx_wwv` — schedule-aware off-air WWV/WWVH capture-decode. Waits
/// (cancellably) until the next :18/:45 broadcast window, then arms the real
/// capture cycle via [`DataService::wwv_capture`]. `needs_radio: true`,
/// `transmits: false`. See this module's doc comment for the full schedule
/// rationale and the `timeout_s` sizing an author MUST set (the engine's
/// 300 s default is not enough to cover a worst-case ~3600 s wait).
///
/// **Cancellation** of an in-flight capture acknowledges only after the
/// capture cycle completes — see this struct's `execute` impl below for why
/// (the underlying blocking capture has no abort hook; the lease must not
/// lie about physical rig use). **Pause is not observed** anywhere in this
/// action: the capture has no safe mid-flight checkpoint to pause at (same
/// reasoning as cancellation), and the broadcast-window wait holds no lease
/// at all, so honoring pause there would have nothing to protect — pause is
/// moot for the whole action, not silently dropped.
pub struct SpaceWxWwv {
    arbiter: Arc<RadioArbiter>,
    data: Arc<dyn DataService>,
    now_ms: fn() -> u64,
}

impl SpaceWxWwv {
    pub fn new(
        arbiter: Arc<RadioArbiter>,
        data: Arc<dyn DataService>,
        now_ms: fn() -> u64,
    ) -> Self {
        Self {
            arbiter,
            data,
            now_ms,
        }
    }
}

#[async_trait]
impl Action for SpaceWxWwv {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: DATA_SPACEWX_WWV,
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let rig = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, DATA_SPACEWX_WWV);

        let sched = next_capture((self.now_ms)() / 1000);
        if sched.delay_s > 0 {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(StepError::Cancelled),
                _ = tokio::time::sleep(Duration::from_secs(sched.delay_s)) => {}
            }
        }

        // Acquired ONLY for the tune/capture/restore cycle itself — see this
        // module's doc comment for why the wait above does NOT hold the
        // lease (spec §6: "RX-only but seizes the rig" describes the
        // capture, not an idle wait for a broadcast window).
        let _lease = self
            .arbiter
            .acquire(&rig, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: DATA_SPACEWX_WWV.to_string(),
                cause: e.to_string(),
            })?;

        // Re-read wall time: the wait above may have consumed real seconds,
        // and `wwv_offair_refresh` itself uses `now_ms` to pick the UTC-hour
        // frequency and provenance timestamp.
        let fire_now_ms = (self.now_ms)();

        // Do NOT abandon the capture on cancel: the real capture runs in
        // `spawn_blocking` (`wwv_offair::commands::wwv_offair_refresh`'s
        // tune / arecord / restore cycle) and has no abort hook. Racing
        // `cancel` against the capture future in a single `select!` and
        // returning as soon as cancellation is observed — the way the
        // schedule-wait above does — would drop this await while that
        // blocking work keeps physically driving the rig, AFTER `_lease`
        // above has already released (its scope ends at the early
        // `return`). That is a lying lease: it would stop covering the
        // moment CAT-tune / arecord / CAT-restore are still in flight.
        // Instead: keep polling BOTH the capture and cancellation every
        // loop iteration, but once cancellation is observed, only note it
        // and keep polling the capture — never break the loop until the
        // capture itself resolves (bounded — ~70 s dwell plus rig
        // save/restore). `_lease`'s scope then truthfully spans every
        // moment of physical rig activity; cancellation of a WWV capture
        // acknowledges only once the in-flight capture cycle completes,
        // never mid-flight.
        let mut cancelled_during_capture = false;
        let mut capture_fut = self.data.wwv_capture(fire_now_ms);
        let capture_result = loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled(), if !cancelled_during_capture => {
                    cancelled_during_capture = true;
                }
                res = &mut capture_fut => break res,
            }
        };

        if cancelled_during_capture {
            return Err(StepError::Cancelled);
        }

        let outcome = capture_result.map_err(|cause| StepError::Action {
            action: DATA_SPACEWX_WWV.to_string(),
            cause,
        })?;

        if outcome.no_copy {
            // Not a verbatim underlying-system error (the capture + STT
            // calls all returned Ok) — this action's own diagnostic,
            // mirroring `rig.apply_preset`'s "verification mismatch"
            // precedent for a real, non-exceptional failure condition.
            return Err(StepError::Action {
                action: DATA_SPACEWX_WWV.to_string(),
                cause: format!(
                    "WWV capture at the {} window completed but produced no confident \
                     transcript (no_copy) — clip kept at {:?} for operator playback/manual entry",
                    sched.label, outcome.wav_path
                ),
            });
        }

        serde_json::to_value(&outcome).map_err(|e| StepError::Action {
            action: DATA_SPACEWX_WWV.to_string(),
            cause: format!("outcome serialize: {e}"),
        })
        // `_lease` drops here — released after capture+restore completes.
        // This is also true of the early `return Err(StepError::Cancelled)`
        // above: it only runs after `capture_result` already resolved, so
        // `_lease` never releases before the physical capture is done, on
        // any code path.
    }
}

// ============================================================================
// data.spacewx_swpc
// ============================================================================

const DATA_SPACEWX_SWPC: &str = "data.spacewx_swpc";

/// `data.spacewx_swpc` — the online NOAA SWPC fetch. `needs_internet: true`;
/// no rig involvement, no arbiter lease.
pub struct SpaceWxSwpc {
    data: Arc<dyn DataService>,
}

impl SpaceWxSwpc {
    pub fn new(data: Arc<dyn DataService>) -> Self {
        Self { data }
    }
}

#[async_trait]
impl Action for SpaceWxSwpc {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: DATA_SPACEWX_SWPC,
            needs_radio: false,
            transmits: false,
            needs_internet: true,
        }
    }

    async fn execute(&self, _params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let outcome = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.data.swpc_refresh() => res,
        }
        .map_err(|cause| StepError::Action {
            action: DATA_SPACEWX_SWPC.to_string(),
            cause,
        })?;

        serde_json::to_value(&outcome).map_err(|e| StepError::Action {
            action: DATA_SPACEWX_SWPC.to_string(),
            cause: format!("outcome serialize: {e}"),
        })
    }
}

// ============================================================================
// data.stationlist_update
// ============================================================================

const DATA_STATIONLIST_UPDATE: &str = "data.stationlist_update";

fn default_history_hours() -> u32 {
    168
}

#[derive(Debug, Deserialize)]
struct StationlistUpdateParams {
    /// Modes to refresh (wire values match `ListingMode`'s kebab-case tokens,
    /// e.g. `"vara-hf"`). Empty/absent refreshes every confirmed-endpoint
    /// mode (`ListingMode::ALL`) — the common "just update the catalog" case.
    #[serde(default)]
    modes: Vec<crate::catalog::stations::ListingMode>,
    #[serde(default = "default_history_hours")]
    history_hours: u32,
}

/// `data.stationlist_update` — the Winlink gateway status API refresh.
/// `needs_internet: true`; no rig involvement, no arbiter lease. Outputs
/// `{"updated": true, "station_count": N}` on success, or the verbatim
/// underlying fetch failure.
pub struct StationlistUpdate {
    data: Arc<dyn DataService>,
}

impl StationlistUpdate {
    pub fn new(data: Arc<dyn DataService>) -> Self {
        Self { data }
    }
}

#[async_trait]
impl Action for StationlistUpdate {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: DATA_STATIONLIST_UPDATE,
            needs_radio: false,
            transmits: false,
            needs_internet: true,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: StationlistUpdateParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: DATA_STATIONLIST_UPDATE.to_string(),
                cause: format!("invalid params: {e}"),
            })?;
        let modes = if parsed.modes.is_empty() {
            crate::catalog::stations::ListingMode::ALL.to_vec()
        } else {
            parsed.modes
        };

        let outcome = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.data.stationlist_refresh(modes, parsed.history_hours) => res,
        }
        .map_err(|cause| StepError::Action {
            action: DATA_STATIONLIST_UPDATE.to_string(),
            cause,
        })?;

        Ok(json!({
            "updated": true,
            "station_count": outcome.station_count,
            "modes": outcome.modes,
        }))
    }
}

// ============================================================================
// data.read
// ============================================================================

const DATA_READ: &str = "data.read";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReadSource {
    InboxSummary,
    SpaceWeather,
    HeardStations,
    Grid,
    LastConnectedGateway,
}

#[derive(Debug, Deserialize)]
struct ReadParams {
    source: ReadSource,
}

/// Verbatim honest-gap message for `source: "heard_stations"` — see this
/// module's doc comment ("NO backend seam") for the full recon.
const HEARD_STATIONS_UNSUPPORTED: &str =
    "data.read source=heard_stations: tuxlink does not keep a queryable record of currently-heard \
     APRS stations. Heard-station positions show live in the APRS Tac Chat view, but nothing \
     persists them anywhere a routine step can read them from — this source stays unavailable \
     until a backend-side heard-stations record exists.";

/// `source: "last_connected_gateway"` when `connection_history` has NO
/// record yet (plan 2 Task 5c: the gap is closed — persistence exists — but
/// a fresh install/config dir genuinely has nothing recorded until the
/// first successful packet/ARDOP/VARA session completes). An honest,
/// operator-facing "not yet" message, not the prior "unsupported" framing.
const LAST_CONNECTED_GATEWAY_NO_RECORD: &str =
    "data.read source=last_connected_gateway: tuxlink has not recorded a successful radio-gateway \
     connection yet. This source becomes available once a Packet, ARDOP, or VARA session \
     completes successfully.";

/// `data.read` — read-only tuxlink state. No capability flags (no rig, no
/// internet, no transmit) — every `source` is either a local read or an
/// honest-gap error, never a network/rig call.
pub struct DataRead {
    data: Arc<dyn DataService>,
}

impl DataRead {
    pub fn new(data: Arc<dyn DataService>) -> Self {
        Self { data }
    }
}

#[async_trait]
impl Action for DataRead {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: DATA_READ,
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: ReadParams = serde_json::from_value(params).map_err(|e| StepError::Action {
            action: DATA_READ.to_string(),
            cause: format!("invalid params: {e}"),
        })?;

        match parsed.source {
            ReadSource::InboxSummary => {
                let summary = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(StepError::Cancelled),
                    res = self.data.read_inbox_summary() => res,
                }
                .map_err(|cause| StepError::Action {
                    action: DATA_READ.to_string(),
                    cause,
                })?;
                serde_json::to_value(&summary).map_err(|e| StepError::Action {
                    action: DATA_READ.to_string(),
                    cause: format!("output serialize: {e}"),
                })
            }
            ReadSource::SpaceWeather => {
                let snapshot = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(StepError::Cancelled),
                    res = self.data.read_space_weather() => res,
                }
                .map_err(|cause| StepError::Action {
                    action: DATA_READ.to_string(),
                    cause,
                })?;
                serde_json::to_value(&snapshot).map_err(|e| StepError::Action {
                    action: DATA_READ.to_string(),
                    cause: format!("output serialize: {e}"),
                })
            }
            ReadSource::Grid => {
                let grid = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(StepError::Cancelled),
                    res = self.data.read_grid() => res,
                }
                .map_err(|cause| StepError::Action {
                    action: DATA_READ.to_string(),
                    cause,
                })?;
                Ok(json!({ "grid": grid }))
            }
            ReadSource::HeardStations => Err(StepError::Action {
                action: DATA_READ.to_string(),
                cause: HEARD_STATIONS_UNSUPPORTED.to_string(),
            }),
            ReadSource::LastConnectedGateway => {
                let record = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(StepError::Cancelled),
                    res = self.data.read_last_connected_gateway() => res,
                }
                .map_err(|cause| StepError::Action {
                    action: DATA_READ.to_string(),
                    cause,
                })?;
                match record {
                    Some(r) => serde_json::to_value(&r).map_err(|e| StepError::Action {
                        action: DATA_READ.to_string(),
                        cause: format!("output serialize: {e}"),
                    }),
                    None => Err(StepError::Action {
                        action: DATA_READ.to_string(),
                        cause: LAST_CONNECTED_GATEWAY_NO_RECORD.to_string(),
                    }),
                }
            }
        }
    }
}

// ============================================================================
// Real seam adapter — MonolithDataService. Follows the same `AppHandle` +
// `.state::<T>()`-resolved-fresh-per-call pattern as `radio.rs`'s
// Monolith*Service adapters. `wwv_capture`/`swpc_refresh`/`read_space_weather`
// call the real `#[tauri::command]` functions directly (they take no `State`
// params); `stationlist_refresh`/`read_inbox_summary`/`read_grid` resolve
// their `State<'_, T>` extractor via `self.app.state::<T>()` (the same
// pattern `EgressPorts` and this crate's other Monolith adapters use) and
// hand it straight to the real command function.
// ============================================================================

pub struct MonolithDataService {
    app: AppHandle,
}

impl MonolithDataService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl DataService for MonolithDataService {
    async fn wwv_capture(&self, now_ms: u64) -> Result<WwvCaptureOutcome, String> {
        let outcome = crate::wwv_offair::commands::wwv_offair_refresh(
            now_ms,
            self.app
                .state::<std::sync::Arc<crate::routines::arbiter::RadioArbiter>>(),
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        Ok(WwvCaptureOutcome {
            updated: outcome.updated,
            indices: outcome.indices,
            source: outcome.source,
            no_copy: outcome.no_copy,
            wav_path: outcome.wav_path,
        })
    }

    async fn swpc_refresh(&self) -> Result<SwpcOutcome, String> {
        let outcome = crate::propagation::commands::propagation_update_solar()
            .await
            .map_err(|e| format!("{e:?}"))?;
        Ok(SwpcOutcome {
            forecast_updated: outcome.forecast_updated,
            indices: outcome.indices,
        })
    }

    async fn stationlist_refresh(
        &self,
        modes: Vec<crate::catalog::stations::ListingMode>,
        history_hours: u32,
    ) -> Result<StationlistOutcome, String> {
        let mode_labels: Vec<String> = modes.iter().map(|m| m.label().to_string()).collect();
        let cache = self
            .app
            .state::<Arc<crate::catalog::stations_cache::StationsCache>>();
        let listings =
            crate::catalog::commands::catalog_fetch_stations(modes, Some(history_hours), cache)
                .await
                .map_err(|e| format!("{e:?}"))?;
        let station_count = listings.iter().map(|l| l.gateways.len()).sum();
        Ok(StationlistOutcome {
            station_count,
            modes: mode_labels,
        })
    }

    async fn read_inbox_summary(&self) -> Result<InboxSummaryDto, String> {
        let backend = self
            .app
            .state::<crate::app_backend::BackendState>()
            .current()
            .ok_or_else(|| "backend offline".to_string())?;
        let metas = crate::ui_core::mailbox::list_mailbox(
            &backend,
            crate::native_mailbox::FolderRef::System(crate::winlink_backend::MailboxFolder::Inbox),
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        let unread = metas.iter().filter(|m| m.unread).count();
        Ok(InboxSummaryDto {
            total: metas.len(),
            unread,
        })
    }

    async fn read_space_weather(
        &self,
    ) -> Result<Option<crate::propagation::solar_update::SolarSnapshot>, String> {
        crate::wwv_offair::commands::wwv_offair_snapshot_read()
            .await
            .map_err(|e| format!("{e:?}"))
    }

    async fn read_grid(&self) -> Result<Option<String>, String> {
        let arbiter = self.app.state::<Arc<crate::position::PositionArbiter>>();
        let status = crate::ui_commands::position_status(arbiter)
            .await
            .map_err(|e| format!("{e:?}"))?;
        Ok(if status.ui_grid.is_empty() {
            None
        } else {
            Some(status.ui_grid)
        })
    }

    async fn read_last_connected_gateway(&self) -> Result<Option<LastConnectedGatewayDto>, String> {
        Ok(crate::connection_history::read_last().map(|r| LastConnectedGatewayDto {
            callsign: r.callsign,
            transport: r.transport,
            at_unix: r.at_unix,
        }))
    }
}

// ============================================================================
// Tests — trait fakes, no hardware/tauri. Per plan Task 4's test contract:
// happy shapes, verbatim errors, capability flags, plus the WWV schedule
// math ported from window.test.ts and the lease-held-only-during-capture
// discipline.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::propagation::solar::SolarIndices;
    use crate::propagation::solar_update::SolarSnapshot;

    thread_local! {
        static TEST_CLOCK: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    }
    fn test_now() -> i64 {
        TEST_CLOCK.with(|c| c.get())
    }

    fn arbiter() -> Arc<RadioArbiter> {
        Arc::new(RadioArbiter::new(test_now))
    }

    // ---- FakeDataService ---------------------------------------------------
    // Builder-style: every method panics by default ("not expected in this
    // test") unless overridden, so a test that only exercises one method
    // fails loudly if the action under test calls a different one than
    // intended.

    type WwvFn = dyn Fn(u64) -> Result<WwvCaptureOutcome, String> + Send + Sync;
    type SwpcFn = dyn Fn() -> Result<SwpcOutcome, String> + Send + Sync;
    type StationlistFn = dyn Fn(Vec<crate::catalog::stations::ListingMode>, u32) -> Result<StationlistOutcome, String>
        + Send
        + Sync;
    type InboxFn = dyn Fn() -> Result<InboxSummaryDto, String> + Send + Sync;
    type SpaceWeatherFn = dyn Fn() -> Result<Option<SolarSnapshot>, String> + Send + Sync;
    type GridFn = dyn Fn() -> Result<Option<String>, String> + Send + Sync;
    type LastConnectedGatewayFn =
        dyn Fn() -> Result<Option<LastConnectedGatewayDto>, String> + Send + Sync;

    struct FakeDataService {
        wwv: Box<WwvFn>,
        // Simulates the real capture's ~70 s dwell: `wwv_capture` sleeps this
        // long BEFORE invoking `wwv`, giving cancellation tests a real
        // in-flight window to fire during (default `ZERO` — every other test
        // keeps resolving on first poll, unchanged from before this field
        // existed).
        wwv_delay: Duration,
        swpc: Box<SwpcFn>,
        stationlist: Box<StationlistFn>,
        inbox: Box<InboxFn>,
        space_weather: Box<SpaceWeatherFn>,
        grid: Box<GridFn>,
        last_connected_gateway: Box<LastConnectedGatewayFn>,
    }

    impl Default for FakeDataService {
        fn default() -> Self {
            Self {
                wwv: Box::new(|_| panic!("wwv_capture not expected in this test")),
                wwv_delay: Duration::ZERO,
                swpc: Box::new(|| panic!("swpc_refresh not expected in this test")),
                stationlist: Box::new(|_, _| {
                    panic!("stationlist_refresh not expected in this test")
                }),
                inbox: Box::new(|| panic!("read_inbox_summary not expected in this test")),
                space_weather: Box::new(|| panic!("read_space_weather not expected in this test")),
                grid: Box::new(|| panic!("read_grid not expected in this test")),
                last_connected_gateway: Box::new(|| {
                    panic!("read_last_connected_gateway not expected in this test")
                }),
            }
        }
    }

    impl FakeDataService {
        fn with_wwv(
            mut self,
            f: impl Fn(u64) -> Result<WwvCaptureOutcome, String> + Send + Sync + 'static,
        ) -> Self {
            self.wwv = Box::new(f);
            self
        }
        fn with_wwv_delay(mut self, delay: Duration) -> Self {
            self.wwv_delay = delay;
            self
        }
        fn with_swpc(
            mut self,
            f: impl Fn() -> Result<SwpcOutcome, String> + Send + Sync + 'static,
        ) -> Self {
            self.swpc = Box::new(f);
            self
        }
        fn with_stationlist(
            mut self,
            f: impl Fn(
                    Vec<crate::catalog::stations::ListingMode>,
                    u32,
                ) -> Result<StationlistOutcome, String>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            self.stationlist = Box::new(f);
            self
        }
        fn with_inbox(
            mut self,
            f: impl Fn() -> Result<InboxSummaryDto, String> + Send + Sync + 'static,
        ) -> Self {
            self.inbox = Box::new(f);
            self
        }
        fn with_space_weather(
            mut self,
            f: impl Fn() -> Result<Option<SolarSnapshot>, String> + Send + Sync + 'static,
        ) -> Self {
            self.space_weather = Box::new(f);
            self
        }
        fn with_grid(
            mut self,
            f: impl Fn() -> Result<Option<String>, String> + Send + Sync + 'static,
        ) -> Self {
            self.grid = Box::new(f);
            self
        }
        fn with_last_connected_gateway(
            mut self,
            f: impl Fn() -> Result<Option<LastConnectedGatewayDto>, String> + Send + Sync + 'static,
        ) -> Self {
            self.last_connected_gateway = Box::new(f);
            self
        }
    }

    #[async_trait]
    impl DataService for FakeDataService {
        async fn wwv_capture(&self, now_ms: u64) -> Result<WwvCaptureOutcome, String> {
            if !self.wwv_delay.is_zero() {
                tokio::time::sleep(self.wwv_delay).await;
            }
            (self.wwv)(now_ms)
        }
        async fn swpc_refresh(&self) -> Result<SwpcOutcome, String> {
            (self.swpc)()
        }
        async fn stationlist_refresh(
            &self,
            modes: Vec<crate::catalog::stations::ListingMode>,
            history_hours: u32,
        ) -> Result<StationlistOutcome, String> {
            (self.stationlist)(modes, history_hours)
        }
        async fn read_inbox_summary(&self) -> Result<InboxSummaryDto, String> {
            (self.inbox)()
        }
        async fn read_space_weather(&self) -> Result<Option<SolarSnapshot>, String> {
            (self.space_weather)()
        }
        async fn read_grid(&self) -> Result<Option<String>, String> {
            (self.grid)()
        }
        async fn read_last_connected_gateway(
            &self,
        ) -> Result<Option<LastConnectedGatewayDto>, String> {
            (self.last_connected_gateway)()
        }
    }

    // ======================================================================
    // next_capture — ported fixtures from src/wwv/window.test.ts
    // ======================================================================

    // Exact hour boundary (verified in window.test.ts: 1_783_512_000 % 3600 === 0).
    const HOUR_BOUNDARY_S: u64 = 1_783_512_000;

    #[test]
    fn next_capture_at_hour_boundary_schedules_wwv_18_this_hour() {
        let got = next_capture(HOUR_BOUNDARY_S);
        assert_eq!(got.label, "WWV :18");
        assert_eq!(got.delay_s, WWV_START_S);
    }

    #[test]
    fn next_capture_fires_immediately_inside_wwv_span() {
        // 5 s into the WWV_START_S=1075 window, still < 1075+70=1145.
        let got = next_capture(HOUR_BOUNDARY_S + 1080);
        assert_eq!(got.delay_s, 0);
        assert_eq!(got.label, "WWV :18");
    }

    #[test]
    fn next_capture_schedules_wwvh_after_wwv_span_before_wwvh_window() {
        // :20:00 — past WWV's 1075..1145 span.
        let got = next_capture(HOUR_BOUNDARY_S + 1200);
        assert_eq!(got.label, "WWVH :45");
        assert_eq!(got.delay_s, WWVH_START_S - 1200);
    }

    #[test]
    fn next_capture_rolls_into_next_hour_when_past_both_windows() {
        // :50:00 — past both WWV (1075..1145) and WWVH (2695..2765) spans.
        let got = next_capture(HOUR_BOUNDARY_S + 3000);
        assert_eq!(got.label, "WWV :18");
        assert_eq!(got.delay_s, HOUR_S + WWV_START_S - 3000);
    }

    #[test]
    fn next_capture_fires_immediately_inside_wwvh_span() {
        // 5 s into WWVH_START_S=2695, still < 2695+70=2765.
        let got = next_capture(HOUR_BOUNDARY_S + 2700);
        assert_eq!(got.delay_s, 0);
        assert_eq!(got.label, "WWVH :45");
    }

    // ======================================================================
    // data.spacewx_wwv
    // ======================================================================

    fn wwv_outcome(sfi: f64) -> WwvCaptureOutcome {
        WwvCaptureOutcome {
            updated: true,
            indices: Some(SolarIndices {
                sfi,
                a_index: Some(8.0),
                k_index: Some(2.0),
            }),
            source: "rf-wwv-voice".to_string(),
            no_copy: false,
            wav_path: None,
        }
    }

    #[tokio::test]
    async fn spacewx_wwv_fires_immediately_inside_a_window_no_sleep() {
        let arb = arbiter();
        let data = FakeDataService::default().with_wwv(|_now_ms| Ok(wwv_outcome(150.0)));
        // now_ms fixed inside the active WWV window (delay_s == 0) so the
        // test never actually sleeps.
        let now_ms_fn: fn() -> u64 = || (HOUR_BOUNDARY_S + 1080) * 1000;
        let action = SpaceWxWwv::new(arb, Arc::new(data), now_ms_fn);
        let out = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect("happy path must succeed");
        assert_eq!(out["updated"], json!(true));
        assert_eq!(out["source"], json!("rf-wwv-voice"));
        assert_eq!(out["indices"]["sfi"], json!(150.0));
    }

    #[tokio::test(start_paused = true)]
    async fn spacewx_wwv_waits_for_the_window_then_captures() {
        let arb = arbiter();
        let observed_fire_ms: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
        let of = observed_fire_ms.clone();
        let data = FakeDataService::default().with_wwv(move |now_ms| {
            *of.lock().unwrap() = Some(now_ms);
            Ok(wwv_outcome(120.0))
        });
        // now_ms just past the top of the hour — well before the WWV :18
        // window, so this exercises the real sleep path. Paused virtual time
        // (tokio::test(start_paused = true)) advances instantly.
        let now_ms_fn: fn() -> u64 = || HOUR_BOUNDARY_S * 1000 + 10_000;
        let action = SpaceWxWwv::new(arb, Arc::new(data), now_ms_fn);
        let out = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect("must succeed once the window arrives");
        assert_eq!(out["updated"], json!(true));
        // The fire time handed to wwv_capture is (self.now_ms)() re-read
        // AFTER the sleep — with a fixed now_ms fn (not advancing with
        // virtual time) it is still the same instant this test fixed, which
        // is enough to prove the call happened (not that real time passed).
        assert!(observed_fire_ms.lock().unwrap().is_some());
    }

    #[tokio::test]
    async fn spacewx_wwv_cancellation_during_wait_is_prompt() {
        let arb = arbiter();
        let data = FakeDataService::default(); // wwv_capture must never be called
        let cancel = CancellationToken::new();
        cancel.cancel();
        // now_ms far from any window so this would otherwise sleep.
        let now_ms_fn: fn() -> u64 = || HOUR_BOUNDARY_S * 1000 + 10_000;
        let action = SpaceWxWwv::new(arb, Arc::new(data), now_ms_fn);
        let err = action
            .execute(json!({}), cancel)
            .await
            .expect_err("a pre-cancelled token must not wait or capture");
        assert!(matches!(err, StepError::Cancelled));
    }

    // The MEDIUM review finding this test guards: a cancel that arrives
    // WHILE the physical capture is running must not abandon the capture
    // future (that would drop the lease while CAT-tune/arecord/CAT-restore
    // may still be driving the rig). `FakeDataService::with_wwv_delay`
    // stands in for the real ~70 s dwell so cancellation has a genuine
    // in-flight window to land in, under paused virtual time.
    #[tokio::test(start_paused = true)]
    async fn spacewx_wwv_cancel_during_capture_lets_it_finish_then_returns_cancelled() {
        let arb = arbiter();
        let capture_ran_to_completion = Arc::new(Mutex::new(false));
        let crc = capture_ran_to_completion.clone();
        let data = FakeDataService::default()
            .with_wwv_delay(Duration::from_secs(CAPTURE_SPAN_S))
            .with_wwv(move |_now_ms| {
                *crc.lock().unwrap() = true;
                Ok(wwv_outcome(150.0))
            });
        // now_ms fixed inside the active WWV window (delay_s == 0) so this
        // test's only sleep is the fake capture's simulated dwell, not the
        // schedule wait — isolating the capture-cancellation path.
        let now_ms_fn: fn() -> u64 = || (HOUR_BOUNDARY_S + 1080) * 1000;
        let action = SpaceWxWwv::new(arb, Arc::new(data), now_ms_fn);
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let handle = tokio::spawn(async move { action.execute(json!({}), cancel_for_task).await });

        // Let the capture start and get partway through its simulated dwell,
        // THEN cancel — well before the fake's 70 s completes. Paused time
        // advances exactly this far (not further) while this sleep is
        // pending, so the cancel genuinely lands mid-capture.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(
            !*capture_ran_to_completion.lock().unwrap(),
            "sanity: capture must still be in flight when cancel fires"
        );
        cancel.cancel();

        // `handle.await` drives paused time forward the remaining ~65 s the
        // fake capture is still sleeping through — proving execute() really
        // awaited the capture to completion instead of returning as soon as
        // cancellation was observed.
        let result = handle.await.expect("execute task must not panic");
        assert!(
            matches!(result, Err(StepError::Cancelled)),
            "cancellation during capture must still surface as Cancelled, got {result:?}"
        );
        assert!(
            *capture_ran_to_completion.lock().unwrap(),
            "the in-flight capture must run to completion — a cancelled lease must not lie \
             about physical rig use"
        );
    }

    #[tokio::test]
    async fn spacewx_wwv_no_copy_is_a_step_error_naming_the_window_and_clip() {
        let arb = arbiter();
        let data = FakeDataService::default().with_wwv(|_now_ms| {
            Ok(WwvCaptureOutcome {
                updated: false,
                indices: None,
                source: "rf-wwv-voice".to_string(),
                no_copy: true,
                wav_path: Some("/tmp/wwv-1-2-70.wav".to_string()),
            })
        });
        let now_ms_fn: fn() -> u64 = || (HOUR_BOUNDARY_S + 1080) * 1000;
        let action = SpaceWxWwv::new(arb, Arc::new(data), now_ms_fn);
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("no_copy must surface as a step error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.spacewx_wwv");
                assert!(cause.contains("no_copy"));
                assert!(cause.contains("wwv-1-2-70.wav"));
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn spacewx_wwv_verbatim_hard_error_passthrough() {
        let arb = arbiter();
        let data = FakeDataService::default()
            .with_wwv(|_now_ms| Err("rig unreachable: fd closed".to_string()));
        let now_ms_fn: fn() -> u64 = || (HOUR_BOUNDARY_S + 1080) * 1000;
        let action = SpaceWxWwv::new(arb, Arc::new(data), now_ms_fn);
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("hard failure must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.spacewx_wwv");
                assert_eq!(cause, "rig unreachable: fd closed");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn spacewx_wwv_lease_is_held_during_capture_and_released_after() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        let arb_for_fake = arb.clone();
        let observed_during: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
        let od = observed_during.clone();
        let data = FakeDataService::default().with_wwv(move |_now_ms| {
            *od.lock().unwrap() = Some(arb_for_fake.status(rig).is_some());
            Ok(wwv_outcome(150.0))
        });
        let now_ms_fn: fn() -> u64 = || (HOUR_BOUNDARY_S + 1080) * 1000;
        let action = SpaceWxWwv::new(arb.clone(), Arc::new(data), now_ms_fn);

        assert!(
            arb.status(rig).is_none(),
            "nothing holds the rig before capture"
        );
        action
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            *observed_during.lock().unwrap(),
            Some(true),
            "lease must be held DURING wwv_capture"
        );
        assert!(
            arb.status(rig).is_none(),
            "lease must be released AFTER execute returns"
        );
    }

    #[test]
    fn spacewx_wwv_descriptor_flags() {
        let action = SpaceWxWwv::new(
            arbiter(),
            Arc::new(FakeDataService::default()),
            system_now_ms,
        );
        let d = action.descriptor();
        assert!(d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }

    // ======================================================================
    // data.spacewx_swpc
    // ======================================================================

    #[tokio::test]
    async fn spacewx_swpc_happy_path_output_shape() {
        let data = FakeDataService::default().with_swpc(|| {
            Ok(SwpcOutcome {
                forecast_updated: true,
                indices: Some(SolarIndices {
                    sfi: 133.0,
                    a_index: Some(6.0),
                    k_index: Some(1.0),
                }),
            })
        });
        let action = SpaceWxSwpc::new(Arc::new(data));
        let out = action
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["forecastUpdated"], json!(true));
        assert_eq!(out["indices"]["sfi"], json!(133.0));
    }

    #[tokio::test]
    async fn spacewx_swpc_verbatim_error_passthrough() {
        let data =
            FakeDataService::default().with_swpc(|| Err("could not reach NOAA SWPC".to_string()));
        let action = SpaceWxSwpc::new(Arc::new(data));
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.spacewx_swpc");
                assert_eq!(cause, "could not reach NOAA SWPC");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[test]
    fn spacewx_swpc_descriptor_flags_needs_internet() {
        let action = SpaceWxSwpc::new(Arc::new(FakeDataService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(d.needs_internet);
    }

    // ======================================================================
    // data.stationlist_update
    // ======================================================================

    #[tokio::test]
    async fn stationlist_update_happy_path_output_shape() {
        let data = FakeDataService::default().with_stationlist(|_modes, _hours| {
            Ok(StationlistOutcome {
                station_count: 42,
                modes: vec!["VARA HF".to_string()],
            })
        });
        let action = StationlistUpdate::new(Arc::new(data));
        let out = action
            .execute(json!({"modes": ["vara-hf"]}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["updated"], json!(true));
        assert_eq!(out["station_count"], json!(42));
    }

    #[tokio::test]
    async fn stationlist_update_empty_modes_defaults_to_all() {
        let observed_modes: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
        let om = observed_modes.clone();
        let data = FakeDataService::default().with_stationlist(move |modes, _hours| {
            *om.lock().unwrap() = Some(modes.len());
            Ok(StationlistOutcome {
                station_count: 0,
                modes: vec![],
            })
        });
        let action = StationlistUpdate::new(Arc::new(data));
        action
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            *observed_modes.lock().unwrap(),
            Some(crate::catalog::stations::ListingMode::ALL.len()),
            "empty/absent modes must default to every ListingMode"
        );
    }

    #[tokio::test]
    async fn stationlist_update_verbatim_error_passthrough() {
        let data = FakeDataService::default().with_stationlist(|_modes, _hours| {
            Err("listing response was not recognizable".to_string())
        });
        let action = StationlistUpdate::new(Arc::new(data));
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.stationlist_update");
                assert_eq!(cause, "listing response was not recognizable");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stationlist_update_invalid_params_is_a_step_error() {
        let action = StationlistUpdate::new(Arc::new(FakeDataService::default()));
        let err = action
            .execute(
                json!({"modes": ["not-a-real-mode"]}),
                CancellationToken::new(),
            )
            .await
            .expect_err("unknown mode token must fail to parse");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[test]
    fn stationlist_update_descriptor_flags_needs_internet() {
        let action = StationlistUpdate::new(Arc::new(FakeDataService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(d.needs_internet);
    }

    // ======================================================================
    // data.read
    // ======================================================================

    #[tokio::test]
    async fn read_inbox_summary_happy_path() {
        let data = FakeDataService::default().with_inbox(|| {
            Ok(InboxSummaryDto {
                total: 5,
                unread: 2,
            })
        });
        let action = DataRead::new(Arc::new(data));
        let out = action
            .execute(json!({"source": "inbox_summary"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["total"], json!(5));
        assert_eq!(out["unread"], json!(2));
    }

    #[tokio::test]
    async fn read_inbox_summary_verbatim_error_passthrough() {
        let data = FakeDataService::default().with_inbox(|| Err("backend offline".to_string()));
        let action = DataRead::new(Arc::new(data));
        let err = action
            .execute(json!({"source": "inbox_summary"}), CancellationToken::new())
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.read");
                assert_eq!(cause, "backend offline");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_space_weather_happy_path_present() {
        let snap = SolarSnapshot {
            indices: Some(SolarIndices {
                sfi: 150.0,
                a_index: Some(8.0),
                k_index: Some(2.0),
            }),
            updated_at_ms: 1_000,
            source: "rf-wwv-voice".to_string(),
            forecast_updated: true,
        };
        let expected = snap.clone();
        let data =
            FakeDataService::default().with_space_weather(move || Ok(Some(expected.clone())));
        let action = DataRead::new(Arc::new(data));
        let out = action
            .execute(json!({"source": "space_weather"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["source"], json!("rf-wwv-voice"));
        assert_eq!(out["indices"]["sfi"], json!(150.0));
    }

    #[tokio::test]
    async fn read_space_weather_none_serializes_null() {
        let data = FakeDataService::default().with_space_weather(|| Ok(None));
        let action = DataRead::new(Arc::new(data));
        let out = action
            .execute(json!({"source": "space_weather"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out, json!(null));
    }

    #[tokio::test]
    async fn read_grid_present() {
        let data = FakeDataService::default().with_grid(|| Ok(Some("EM75xx".to_string())));
        let action = DataRead::new(Arc::new(data));
        let out = action
            .execute(json!({"source": "grid"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["grid"], json!("EM75xx"));
    }

    #[tokio::test]
    async fn read_grid_absent_is_null_not_an_error() {
        let data = FakeDataService::default().with_grid(|| Ok(None));
        let action = DataRead::new(Arc::new(data));
        let out = action
            .execute(json!({"source": "grid"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["grid"], json!(null));
    }

    #[tokio::test]
    async fn read_heard_stations_is_documented_honest_gap() {
        let action = DataRead::new(Arc::new(FakeDataService::default()));
        let err = action
            .execute(
                json!({"source": "heard_stations"}),
                CancellationToken::new(),
            )
            .await
            .expect_err("heard_stations has no backend seam — must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.read");
                assert_eq!(cause, HEARD_STATIONS_UNSUPPORTED);
                assert!(
                    cause.contains("APRS Tac Chat"),
                    "must name the real seam gap in operator-facing terms"
                );
                assert!(
                    !cause.contains("useAprsPositions.ts") && !cause.contains(".rs"),
                    "must read as an operator diagnostic, not a source-file reference"
                );
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_last_connected_gateway_happy_path() {
        let data = FakeDataService::default().with_last_connected_gateway(|| {
            Ok(Some(LastConnectedGatewayDto {
                callsign: "W7DEF-10".to_string(),
                transport: "ardop-hf".to_string(),
                at_unix: 1_752_400_000,
            }))
        });
        let action = DataRead::new(Arc::new(data));
        let out = action
            .execute(
                json!({"source": "last_connected_gateway"}),
                CancellationToken::new(),
            )
            .await
            .expect("must succeed when a record exists");
        assert_eq!(out["callsign"], json!("W7DEF-10"));
        assert_eq!(out["transport"], json!("ardop-hf"));
        assert_eq!(out["atUnix"], json!(1_752_400_000_i64));
    }

    #[tokio::test]
    async fn read_last_connected_gateway_no_record_is_an_honest_error() {
        let data = FakeDataService::default().with_last_connected_gateway(|| Ok(None));
        let action = DataRead::new(Arc::new(data));
        let err = action
            .execute(
                json!({"source": "last_connected_gateway"}),
                CancellationToken::new(),
            )
            .await
            .expect_err("no record yet must error, not return null");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.read");
                assert_eq!(cause, LAST_CONNECTED_GATEWAY_NO_RECORD);
                assert!(
                    !cause.contains(".rs"),
                    "must read as an operator diagnostic, not a source-file reference"
                );
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_last_connected_gateway_verbatim_error_passthrough() {
        let data = FakeDataService::default()
            .with_last_connected_gateway(|| Err("disk read failed".to_string()));
        let action = DataRead::new(Arc::new(data));
        let err = action
            .execute(
                json!({"source": "last_connected_gateway"}),
                CancellationToken::new(),
            )
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.read");
                assert_eq!(cause, "disk read failed");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_invalid_source_is_a_step_error() {
        let action = DataRead::new(Arc::new(FakeDataService::default()));
        let err = action
            .execute(
                json!({"source": "not_a_real_source"}),
                CancellationToken::new(),
            )
            .await
            .expect_err("unknown source must fail to parse");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[test]
    fn read_descriptor_has_no_capabilities() {
        let action = DataRead::new(Arc::new(FakeDataService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }
}
