//! `radio.connect` / `radio.listen` / `radio.aprs_send` — spec §6 "Radio
//! actions" (plan Task 4a). Every impl here delegates transport work through
//! the narrow `ConnectService`/`ListenService`/`AprsService` ports declared
//! in `actions/mod.rs`; NONE of this file re-implements ARQ/APRS/audio-
//! capture logic — that lives behind the real seam a `Monolith*Service`
//! adapter below wraps.
//!
//! ## Recon: the real transport seams (plan Task 4a)
//!
//! - **CMS Telnet** (`ui_commands::cms_connect`) is Part-15 internet-only —
//!   it never touches a rig, so it is not a `needs_radio` action and is out
//!   of scope for `radio.connect` entirely. `TransportConfig::Cms` in
//!   `winlink_backend.rs` has no station/band concept at all — it always
//!   dials the one configured CMS host.
//! - **Packet** (`ui_commands::packet_connect` /
//!   `packet_transport_from_config`) IS a clean, already-abstracted RF
//!   delegation: `WinlinkBackend::connect(TransportConfig::Packet { role:
//!   DialTo { call, path }, .. }, None)` does dial + B2F exchange in one
//!   call, exactly the "connect, forward staged outbox traffic" shape spec
//!   §6 wants. [`MonolithConnectService`] below wires this fully and for
//!   real — no fake, no stub.
//! - **ARDOP HF** (`modem_commands::modem_ardop_connect` /
//!   `modem_ardop_b2f_exchange`) and **VARA HF**
//!   (`winlink::modem::vara::commands::{vara_open_session,
//!   modem_vara_b2f_exchange}`) are the transports spec §6's own example
//!   (`"bands": ["40m", "80m"]`) is clearly aimed at — but BOTH require a
//!   concrete dial frequency (`freq_hz`), and nothing in the codebase maps
//!   a band label ("40m") + a gateway callsign to that gateway's actual
//!   operating frequency. `tuxlink_capture::bands::dial_hz` is the WRONG
//!   table for this (it is the FT8 calling-frequency table, not a Winlink
//!   RMS gateway's channel list) — using it would tune the rig to a
//!   frequency the target gateway is almost certainly NOT listening on,
//!   which is worse than an honest gap: a routine author would see
//!   `connected: true`-shaped code paths that silently never reach the
//!   gateway on air. The real per-gateway channel list lives in
//!   `catalog::stations::Gateway.frequencies_khz`, populated by
//!   `catalog::stations_cache` — but that cache is a network-fetching
//!   service Task 4c (`data.rs`, `data.stationlist_update`) owns; wiring it
//!   into `radio.connect` here would blur the 4a/4c file boundary the plan
//!   drew, and a `needs_radio` action triggering its OWN network fetch
//!   mid-lease is a separate design question Task 4c/5 should settle
//!   deliberately, not inherit implicitly from this file.
//!
//!   **What Task 5's session wiring must provide:** a way for
//!   [`MonolithConnectService::connect_attempt`] to resolve `(station,
//!   band)` → `freq_hz` for the ARDOP/VARA HF case — most likely a small
//!   `GatewayFrequencyResolver` seam reading `catalog::stations_cache`'s
//!   ALREADY-populated cache (never triggering its own fetch from inside a
//!   `needs_radio` action's lease) plus a documented in-band frequency
//!   filter.
//!
//!   **The band axis is transport-aware (P1 fix, this revision).**
//!   `ConnectParams.bands` is now `Vec<String>` and MAY be empty — an empty
//!   list drives exactly ONE `connect_attempt(station, None)` per station
//!   (the packet-dial shape: a packet channel is fixed by TNC config, so a
//!   band label is meaningless for it). A non-empty `bands` list still
//!   drives the station×band walk as before.
//!   [`MonolithConnectService::connect_attempt`] determines the CONFIGURED
//!   transport first (`cfg.packet.link` — the same config source
//!   `packet_transport_from_config` reads): when packet IS configured, it
//!   dials via the existing wired path regardless of whether `band` is
//!   `Some(..)` or `None` (a caller passing a spec'd `bands` list against a
//!   packet-only station is not an error — the supplied band just has
//!   nothing to do). Only when packet is NOT the configured transport AND a
//!   band was supplied does `connect_attempt` return the HARD `Err` (a
//!   `StepError::Action`, verbatim, per the Global Constraints) naming this
//!   gap, rather than silently mis-tuning a rig it cannot correctly tune.
//!   **Before this fix**, `connect_attempt` hard-errored on ANY
//!   `Some(band)` before checking transport at all — and since
//!   `ConnectParams.bands` was required non-empty, `radio.connect` always
//!   passed `Some(band)`, making the fully-wired real packet path dead code
//!   no routine could ever reach. See
//!   [`MonolithConnectService::connect_attempt`]'s own doc comment for the
//!   exact branch.
//! - **APRS send** (`ui_commands::aprs_send` /
//!   `winlink::aprs::engine::AprsState::send`) is a clean, already-sync,
//!   single-call seam — [`MonolithAprsService`] wraps it directly.
//! - **Listen / channel-busy RMS** — no existing "listen for N seconds,
//!   return a busy metric" primitive exists anywhere in the codebase; the
//!   closest real audio touchpoint is `ft8::alsa_source::AlsaSource`
//!   (whose own module doc says "CI-COMPILE-CHECKED ONLY... needs real ALSA
//!   hardware" — the same caveat applies to [`MonolithListenService`]
//!   below, by construction — it cannot be exercised against real hardware
//!   in CI or on this dev Pi). `MonolithListenService` opens the
//!   CONFIGURED ARDOP capture device (`config_get_ardop().capture_device` —
//!   a `plughw:` string) directly via `AlsaSource::open` and accumulates
//!   RMS over the dwell window. This deliberately reuses the ARDOP capture
//!   device rather than the FT8 pipeline's numeric-`hw:`-index
//!   device-resolution machinery (`ft8::traits::Ft8Platform` +
//!   `StableAudioId`) — a coarse busy/silence RMS reading tolerates ALSA's
//!   `plug` resampling plugin (indeed `AlsaSource::open`'s strict
//!   exact-48kHz-native-rate assertion is EASIER to satisfy through a
//!   `plughw:` device, which always reports back the requested rate); FT8's
//!   bit-exact decode does not, which is why that pipeline insists on raw
//!   `hw:`. RMS is reported LINEAR, normalized `0.0..=1.0`
//!   (`sample.abs() / i16::MAX`), not dBFS — a deliberate, documented unit
//!   choice that keeps the busy-threshold constant intuitive at silence
//!   (dBFS has no finite floor at zero signal).
//!
//! Plan: `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`
//! Task 4. Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §6, §9.
//!
//! ## RESOLVED (plan 2 Task 5c) — the ARDOP/VARA gateway-frequency gap above
//!
//! [`GatewayFrequencyResolver`] below IS the seam the recon above asked for:
//! it reads `catalog::stations_cache::StationsCache::peek_by_mode` (a
//! cache-only read added this task — never triggers a fetch), filters
//! `Gateway.frequencies_khz` to the requested band's edges (the SAME
//! `crate::mcp_ports::BANDS` table the station-finder's client-side BAND
//! filter uses — reused, not duplicated), and returns every matching
//! frequency in Hz. [`MonolithConnectService::connect_attempt`] now drives a
//! REAL ARDOP or VARA dial (`modem_commands::modem_ardop_connect` +
//! `modem_ardop_b2f_exchange`, or `vara::commands::vara_open_session` +
//! `modem_vara_b2f_exchange`) over each resolved frequency in order,
//! whichever ONE of ARDOP/VARA is configured (`cfg.modem_ardop`/
//! `cfg.modem_vara`). Two honest gaps remain, both HARD errors naming the
//! gap rather than a silent guess:
//!
//! - **No cached frequency data for the station/band** (cache empty, or no
//!   gateway/frequency in range) — the routine author is told to run
//!   `data.stationlist_update` first.
//! - **Both ARDOP and VARA configured simultaneously** — `radio.connect` has
//!   no way to know which HF modem the routine author intends for THIS
//!   station; the error names the ambiguity rather than guessing (guessing
//!   wrong would key/tune a live radio via the wrong modem program).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor};
use tuxlink_routines::error::StepError;

use crate::routines::arbiter::RadioArbiter;

use super::{
    busy_policy_from_params, rig_id_from_params, run_holder_from_params, step_timeout_from_params,
    AprsService, ConnectOutcome, ConnectService, ListenService,
};

/// Busy-channel RMS threshold (linear, `0.0..=1.0`) for `radio.listen`'s
/// `channel_busy` output and `radio.connect`'s optional pre-flight dwell.
/// **DATA-GATED — placeholder pending on-air calibration.** Per project
/// discipline, RF behavior is never assumed from source reading alone
/// (`feedback_ai_amateur_radio_reliability`); `0.02` is a conservative
/// guess (a silent HF receiver's noise floor sits well below this on a
/// normalized 16-bit scale), but an operator with a live rig MUST
/// recalibrate against real band noise before `radio.listen`'s busy signal
/// is trusted operationally. Public so a future calibration pass (or a
/// per-rig override) can reference/override it without duplicating the
/// magic number.
pub const CHANNEL_BUSY_RMS_THRESHOLD: f32 = 0.02;

// ============================================================================
// GatewayFrequencyResolver (plan 2 Task 5c) — see this module's "RESOLVED"
// doc note above for the full recon/rationale.
// ============================================================================

/// Look up `label` (e.g. `"40m"`) in [`crate::mcp_ports::BANDS`] — the SAME
/// inclusive-edge table the station-finder's client-side BAND filter uses —
/// and return its `(lo_khz, hi_khz)` range. Case-insensitive (`"40M"`
/// matches `"40m"`, mirroring `mcp_ports::any_freq_in_bands`). `None` for an
/// unrecognized label.
pub(crate) fn band_range(label: &str) -> Option<(f64, f64)> {
    crate::mcp_ports::BANDS
        .iter()
        .find(|(_, _, l)| l.eq_ignore_ascii_case(label))
        .map(|(lo, hi, _)| (*lo, *hi))
}

/// Honest failure from [`GatewayFrequencyResolver::resolve`] — no dial
/// frequency could be resolved for `station`+`band`. Distinguishes "the
/// cache has never been populated for this mode at all" (`cache_age_s:
/// None`) from "the cache has data, but none of it matches this
/// station/band" (`cache_age_s: Some`, the freshest matching listing's age)
/// — both cases point the routine author at the same fix
/// (`data.stationlist_update`), but the age tells them whether they need a
/// first-ever fetch or a re-fetch of stale data.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoFrequencyError {
    pub station: String,
    pub band: String,
    pub cache_age_s: Option<i64>,
}

impl NoFrequencyError {
    /// Operator/routine-author-facing message — verbatim per Global
    /// Constraints (never paraphrased downstream), names station/band/
    /// cache-age exactly as plan 2 Task 5c's ledger requires.
    pub fn message(&self) -> String {
        match self.cache_age_s {
            None => format!(
                "radio.connect: no cached gateway-frequency data for station {} band {:?} — \
                 run data.stationlist_update first",
                self.station, self.band
            ),
            Some(age_s) => format!(
                "radio.connect: no {} frequency found for gateway {} in the cached station \
                 list (cache age {age_s}s) — the gateway may not operate on {}, or the cached \
                 list is stale; re-run data.stationlist_update",
                self.band, self.station, self.band
            ),
        }
    }
}

/// Resolves `(station, band)` → candidate dial frequencies in Hz, reading
/// ONLY `catalog::stations_cache::StationsCache`'s already-populated cache
/// (`peek_by_mode` — never triggers a fetch; see this module's "RESOLVED"
/// doc note above). `mode` picks which listing to search
/// (`ListingMode::ArdopHf` vs `ListingMode::VaraHf`) — a gateway's ARDOP
/// channel list and VARA channel list are DISTINCT listings, not a shared
/// frequency set, so the caller (which already knows which HF modem is
/// configured) must say which one it wants.
pub(crate) struct GatewayFrequencyResolver {
    cache: Arc<crate::catalog::stations_cache::StationsCache>,
}

impl GatewayFrequencyResolver {
    pub fn new(cache: Arc<crate::catalog::stations_cache::StationsCache>) -> Self {
        Self { cache }
    }

    /// `now_ms` is injected (not read internally) so this stays a pure,
    /// deterministic function over `self.cache`'s current contents — the
    /// same testability discipline `RadioArbiter::new`'s `now: fn() -> i64`
    /// param and `data.rs`'s `SpaceWxWwv::now_ms` already establish in this
    /// module family.
    pub fn resolve(
        &self,
        mode: crate::catalog::stations::ListingMode,
        station: &str,
        band: &str,
        now_ms: u64,
    ) -> Result<Vec<u64>, NoFrequencyError> {
        let listings = self.cache.peek_by_mode(mode);
        let no_freq = || NoFrequencyError {
            station: station.to_string(),
            band: band.to_string(),
            cache_age_s: None,
        };
        if listings.is_empty() {
            return Err(no_freq());
        }

        let Some((lo, hi)) = band_range(band) else {
            return Err(no_freq());
        };

        let mut freqs_hz: Vec<u64> = Vec::new();
        let mut freshest_ms: Option<u64> = None;
        for listing in &listings {
            if let Some(f) = listing.fetched_at_ms {
                freshest_ms = Some(freshest_ms.map_or(f, |cur| cur.max(f)));
            }
            for gateway in &listing.gateways {
                if !gateway.callsign.eq_ignore_ascii_case(station) {
                    continue;
                }
                for &khz in &gateway.frequencies_khz {
                    if khz >= lo && khz <= hi {
                        let hz = (khz * 1000.0).round() as u64;
                        if !freqs_hz.contains(&hz) {
                            freqs_hz.push(hz);
                        }
                    }
                }
            }
        }

        if freqs_hz.is_empty() {
            let cache_age_s = freshest_ms.map(|f| (now_ms.saturating_sub(f) / 1000) as i64);
            return Err(NoFrequencyError {
                station: station.to_string(),
                band: band.to_string(),
                cache_age_s,
            });
        }

        Ok(freqs_hz)
    }
}

// ============================================================================
// radio.connect
// ============================================================================

#[derive(Debug, Deserialize)]
struct ConnectParams {
    stations: Vec<String>,
    /// Station×band walk (spec §6's `"bands": ["40m", "80m"]` example) when
    /// non-empty. **`#[serde(default)]` — empty/absent is valid and
    /// intentional**, not a caller omission: it selects the packet-dial
    /// shape, one attempt per station with `band: None` (see this module's
    /// doc comment — a packet channel is fixed by TNC config, so a band
    /// label doesn't apply). Only `stations` is required non-empty.
    #[serde(default)]
    bands: Vec<String>,
    #[serde(default)]
    listen_before_tx_s: Option<u64>,
}

const RADIO_CONNECT: &str = "radio.connect";

/// `radio.connect` — spec §6 "Connect attempt": station set × band set in
/// order; forwards staged outbox traffic; outputs `connected`, `station`,
/// `band`, `gateway`, or a verbatim last-attempt failure. `band` is `null`
/// in the output when `ConnectParams.bands` was empty/absent (the
/// packet-dial shape — one band-less attempt per station), and the
/// supplied band string otherwise.
pub struct RadioConnect {
    arbiter: Arc<RadioArbiter>,
    connect: Arc<dyn ConnectService>,
    listen: Arc<dyn ListenService>,
}

impl RadioConnect {
    pub fn new(
        arbiter: Arc<RadioArbiter>,
        connect: Arc<dyn ConnectService>,
        listen: Arc<dyn ListenService>,
    ) -> Self {
        Self {
            arbiter,
            connect,
            listen,
        }
    }
}

#[async_trait]
impl Action for RadioConnect {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RADIO_CONNECT,
            needs_radio: true,
            transmits: true,
            needs_internet: false,
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, StepError> {
        let parsed: ConnectParams =
            serde_json::from_value(params.clone()).map_err(|e| StepError::Action {
                action: RADIO_CONNECT.to_string(),
                cause: format!("invalid params: {e}"),
            })?;
        if parsed.stations.is_empty() {
            return Err(StepError::Action {
                action: RADIO_CONNECT.to_string(),
                cause: "stations must have at least one entry".to_string(),
            });
        }

        let rig = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RADIO_CONNECT);

        // Acquired ONCE for the whole attempt loop (plan Task 4's explicit
        // lease-discipline instruction) — released/reacquired only on an
        // observed pause request between attempts, never mid-attempt.
        let mut lease = self
            .arbiter
            .acquire(&rig, holder.clone(), policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RADIO_CONNECT.to_string(),
                cause: e.to_string(),
            })?;

        let mut last_error: Option<String> = None;

        for station in &parsed.stations {
            // Station-major (`for station { for band { .. } }`, not the
            // reverse): exhaust one gateway's whole band list — or its
            // single band-less packet-dial attempt when `bands` is empty —
            // before moving to the next gateway. Spec §6 doesn't pin the
            // order down; this is the more intuitive "try to reach THIS
            // gateway everywhere before giving up on it" reading (plan
            // Task 4a's original design-decision note), and it composes
            // cleanly with the packet-shape case below: a band-less station
            // gets exactly one attempt, no inner loop restructuring needed.
            let band_attempts: Vec<Option<&str>> = if parsed.bands.is_empty() {
                vec![None]
            } else {
                parsed.bands.iter().map(|b| Some(b.as_str())).collect()
            };

            for band in band_attempts {
                if cancel.is_cancelled() {
                    return Err(StepError::Cancelled);
                }

                // Operator pause contract (spec §9 / arbiter Task 2's
                // `RadioLease::pause_requested`): observed BETWEEN attempts,
                // never mid-attempt — release the current lease and
                // re-acquire a fresh one before the next dial. A fresh
                // lease's pause token is never itself cancelled (arbiter
                // mints it at grant time), so this loop makes forward
                // progress even under repeated operator_take signals.
                if lease.pause_requested().is_cancelled() {
                    drop(lease);
                    lease = self
                        .arbiter
                        .acquire(&rig, holder.clone(), policy, timeout, &cancel)
                        .await
                        .map_err(|e| StepError::Action {
                            action: RADIO_CONNECT.to_string(),
                            cause: e.to_string(),
                        })?;
                }

                if let Some(secs) = parsed.listen_before_tx_s {
                    if secs > 0 {
                        match self.listen.sample_rms(&rig, secs, cancel.clone()).await {
                            Ok(rms) if rms > CHANNEL_BUSY_RMS_THRESHOLD => {
                                last_error = Some(format!(
                                    "channel busy before dial (rms {rms:.4} > threshold {CHANNEL_BUSY_RMS_THRESHOLD:.4})"
                                ));
                                continue;
                            }
                            Ok(_) => {}
                            Err(cause) => {
                                return Err(StepError::Action {
                                    action: RADIO_CONNECT.to_string(),
                                    cause,
                                });
                            }
                        }
                    }
                }

                let attempt = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(StepError::Cancelled),
                    res = self.connect.connect_attempt(station, band) => res,
                };

                match attempt {
                    Ok(ConnectOutcome {
                        connected: true,
                        gateway,
                        ..
                    }) => {
                        return Ok(json!({
                            "connected": true,
                            "station": station,
                            // `null` for the packet-dial (band-less) shape —
                            // see ConnectParams.bands' doc comment.
                            "band": band,
                            "gateway": gateway,
                        }));
                    }
                    Ok(ConnectOutcome {
                        connected: false,
                        error,
                        ..
                    }) => {
                        last_error = Some(error.unwrap_or_else(|| {
                            "connect attempt failed with no detail".to_string()
                        }));
                    }
                    Err(cause) => {
                        return Err(StepError::Action {
                            action: RADIO_CONNECT.to_string(),
                            cause,
                        });
                    }
                }
            }
        }

        // Exhaustion is an OUTPUT, not a step error (plan Task 4's explicit
        // contract) — the branches downstream drive retry/escalation logic.
        Ok(json!({
            "connected": false,
            "last_error": last_error
                .unwrap_or_else(|| "no station/band combination configured".to_string()),
        }))
    }
}

// ============================================================================
// radio.listen
// ============================================================================

#[derive(Debug, Deserialize)]
struct ListenParams {
    seconds: u64,
}

const RADIO_LISTEN: &str = "radio.listen";

/// `radio.listen` — dwell on `rig` for `seconds`, reporting whether the
/// channel reads busy. `needs_radio: true`, `transmits: false` (RX-only,
/// but still seizes the rig — spec §6 "Update space weather from WWV" row
/// documents the same RX-only-but-seizes-the-rig posture).
pub struct RadioListen {
    arbiter: Arc<RadioArbiter>,
    listen: Arc<dyn ListenService>,
}

impl RadioListen {
    pub fn new(arbiter: Arc<RadioArbiter>, listen: Arc<dyn ListenService>) -> Self {
        Self { arbiter, listen }
    }
}

#[async_trait]
impl Action for RadioListen {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RADIO_LISTEN,
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, StepError> {
        let parsed: ListenParams =
            serde_json::from_value(params.clone()).map_err(|e| StepError::Action {
                action: RADIO_LISTEN.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        let rig = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RADIO_LISTEN);

        let _lease = self
            .arbiter
            .acquire(&rig, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RADIO_LISTEN.to_string(),
                cause: e.to_string(),
            })?;

        let mut cancelled_during_sample = false;
        let mut sample_fut = self.listen.sample_rms(&rig, parsed.seconds, cancel.clone());
        let sample_result = loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled(), if !cancelled_during_sample => {
                    cancelled_during_sample = true;
                }
                res = &mut sample_fut => break res,
            }
        };

        if cancelled_during_sample {
            return Err(StepError::Cancelled);
        }

        let rms = sample_result.map_err(|cause| StepError::Action {
            action: RADIO_LISTEN.to_string(),
            cause,
        })?;

        Ok(json!({
            "channel_busy": rms > CHANNEL_BUSY_RMS_THRESHOLD,
            "rms": rms,
        }))
        // `_lease` drops here — released after the dwell completes.
        // This is also true of the early `return Err(StepError::Cancelled)`
        // above: it only runs after `sample_result` already resolved, so
        // `_lease` never releases before the physical capture is done, on
        // any code path.
    }
}

// ============================================================================
// radio.aprs_send
// ============================================================================

#[derive(Debug, Deserialize)]
struct AprsSendParams {
    #[serde(default)]
    to: Option<String>,
    text: String,
}

const RADIO_APRS_SEND: &str = "radio.aprs_send";

/// `radio.aprs_send` — delegates to [`AprsService`] verbatim (plan Task 4's
/// explicit instruction). `needs_radio: true`, `transmits: true`.
pub struct RadioAprsSend {
    arbiter: Arc<RadioArbiter>,
    aprs: Arc<dyn AprsService>,
}

impl RadioAprsSend {
    pub fn new(arbiter: Arc<RadioArbiter>, aprs: Arc<dyn AprsService>) -> Self {
        Self { arbiter, aprs }
    }
}

#[async_trait]
impl Action for RadioAprsSend {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RADIO_APRS_SEND,
            needs_radio: true,
            transmits: true,
            needs_internet: false,
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, StepError> {
        let parsed: AprsSendParams =
            serde_json::from_value(params.clone()).map_err(|e| StepError::Action {
                action: RADIO_APRS_SEND.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        let rig = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RADIO_APRS_SEND);

        let _lease = self
            .arbiter
            .acquire(&rig, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RADIO_APRS_SEND.to_string(),
                cause: e.to_string(),
            })?;

        let msgid = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.aprs.send(parsed.to, parsed.text) => res,
        }
        .map_err(|cause| StepError::Action {
            action: RADIO_APRS_SEND.to_string(),
            cause,
        })?;

        Ok(json!({ "msgid": msgid }))
        // `_lease` drops here — released after the send completes.
    }
}

// ============================================================================
// Real seam adapters — MonolithConnectService / MonolithAprsService /
// MonolithListenService. See this module's doc comment for the full recon
// behind each and (for Connect) the precise ARDOP/VARA gap Task 5 must
// close. All three follow the `mcp_ports.rs` egress-port pattern: hold an
// `AppHandle`, resolve `.state::<T>()` fresh at call time (never cache a
// borrowed `State`, which cannot outlive the call) — the same pattern
// `EgressPorts::vara_b2f_exchange`/`ardop_b2f_exchange` already use to call
// into the exact same command functions this file's actions ultimately
// drive.
// ============================================================================

/// Real [`ConnectService`]. Packet transport is fully wired (delegates to
/// the existing `WinlinkBackend::connect` abstraction via
/// `ui_commands::packet_transport_from_config`, exactly the mechanism
/// `ui_commands::packet_connect` itself uses).
///
/// `connect_attempt` determines the CONFIGURED transport before it does
/// anything else with `band` (the P1 fix this revision makes — see this
/// module's doc comment for the full before/after). Packet
/// (`cfg.packet.link.is_some()`) always dials via the wired path below,
/// whether `band` is `Some(..)` or `None` — packet's channel is fixed by
/// TNC config, so a supplied band is inert, never an error. Only when
/// packet is NOT configured AND a band WAS supplied does this return a
/// HARD error naming the unresolved ARDOP/VARA gateway-frequency gap; it
/// does not attempt a possibly-wrong CAT tune. This transport check reads
/// `crate::config::read_config()` (global state) directly rather than
/// taking an injectable seam — per plan Task 4a's test contract, that
/// keeps this adapter thin and pushes coverage of the *decision* (band vs.
/// no-band, transport-aware) onto `radio.rs`'s action-layer tests via
/// `FakeConnectService`; this branch itself is CI-compile-verified +
/// operator-smoke territory, same posture as [`MonolithListenService`]'s
/// ALSA I/O below.
pub struct MonolithConnectService {
    app: AppHandle,
}

impl MonolithConnectService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl ConnectService for MonolithConnectService {
    async fn connect_attempt(
        &self,
        station: &str,
        band: Option<&str>,
    ) -> Result<ConnectOutcome, String> {
        let backend = self
            .app
            .state::<crate::app_backend::BackendState>()
            .current()
            .ok_or_else(|| "backend offline".to_string())?;
        let cfg = crate::config::read_config().map_err(|e| e.to_string())?;

        // Determine the CONFIGURED transport FIRST, before doing anything
        // band-shaped with `band` — this is the P1 fix: the old code
        // hard-errored on ANY `Some(band)` before ever checking whether
        // packet was actually configured, which made the fully-wired
        // packet path below unreachable dead code (`ConnectParams.bands`
        // was required non-empty, so `radio.connect` always passed
        // `Some(band)`). `cfg.packet.link` is the same config source
        // `packet_transport_from_config` itself reads below — if packet IS
        // configured, a supplied `band` is simply inert (packet's channel
        // is fixed by TNC config) and this falls through to the real dial.
        if cfg.packet.link.is_some() {
            let transport = crate::ui_commands::packet_transport_from_config(
                &cfg,
                station.to_string(),
                Vec::new(),
                crate::winlink::session::SessionIntent::Cms,
            )
            .map_err(|e| format!("{e:?}"))?;

            return match backend.connect(transport, None).await {
                Ok(session) => {
                    // Packet is a transient dial-exchange-disconnect, same
                    // shape as `cms_connect`/`packet_connect` — best-effort
                    // close; a close failure doesn't undo the successful
                    // exchange that already happened.
                    let _ = backend.disconnect(session).await;
                    Ok(ConnectOutcome {
                        connected: true,
                        gateway: Some(station.to_string()),
                        error: None,
                    })
                }
                Err(e) => Ok(ConnectOutcome {
                    connected: false,
                    gateway: None,
                    error: Some(e.to_string()),
                }),
            };
        }

        // Packet is NOT configured. A band-less attempt against a non-packet
        // station has nothing to dial — HF (ARDOP/VARA) always needs a band
        // to pick a channel.
        let Some(band) = band else {
            return Err(format!(
                "radio.connect: station {station} was given band-less (packet-shaped) params \
                 but no packet KISS link is configured; supply a band to dial ARDOP/VARA HF, \
                 or configure packet"
            ));
        };

        // Exactly ONE of ARDOP/VARA must be configured — `radio.connect` has
        // no basis to guess between two configured HF modems (see this
        // module's "RESOLVED" doc note above).
        let (mode, dial_ardop) = match (cfg.modem_ardop.is_some(), cfg.modem_vara.is_some()) {
            (true, false) => (crate::catalog::stations::ListingMode::ArdopHf, true),
            (false, true) => (crate::catalog::stations::ListingMode::VaraHf, false),
            (true, true) => {
                return Err(format!(
                    "radio.connect: both ARDOP and VARA are configured (station {station} band \
                     {band:?}) — radio.connect cannot determine which HF modem to dial; \
                     configure only one HF modem for automated routines, or dial this gateway \
                     from the ARDOP/VARA panel directly"
                ));
            }
            (false, false) => {
                return Err(format!(
                    "radio.connect: no packet KISS link, ARDOP, or VARA is configured — station \
                     {station} band {band:?} has no transport to dial (Settings)"
                ));
            }
        };

        let cache = self
            .app
            .state::<Arc<crate::catalog::stations_cache::StationsCache>>();
        let resolver = GatewayFrequencyResolver::new(cache.inner().clone());
        let now_ms = super::data::system_now_ms();
        let freqs_hz = resolver
            .resolve(mode, station, band, now_ms)
            .map_err(|e| e.message())?;

        let mut last_error: Option<String> = None;
        for freq_hz in freqs_hz {
            let outcome = if dial_ardop {
                self.ardop_connect_and_exchange(station, freq_hz).await
            } else {
                self.vara_connect_and_exchange(station, freq_hz).await
            };
            match outcome {
                Ok(()) => {
                    return Ok(ConnectOutcome {
                        connected: true,
                        gateway: Some(station.to_string()),
                        error: None,
                    });
                }
                Err(e) => last_error = Some(e),
            }
        }

        Ok(ConnectOutcome {
            connected: false,
            gateway: None,
            error: last_error,
        })
    }
}

impl MonolithConnectService {
    /// Real ARDOP HF dial + B2F exchange for ONE resolved `freq_hz`,
    /// mirroring `mcp_ports.rs`'s `ardop_connect` + `ardop_b2f_exchange`
    /// egress-port call shape exactly (the same two command functions, the
    /// same `Arc<ModemSession>` state resolution). Always tears the modem
    /// session down afterward (`modem_ardop_disconnect`, best-effort) —
    /// `radio.connect` is a transient dial-exchange-disconnect per attempt
    /// (this module's doc comment), so the NEXT resolved frequency (or the
    /// next station/band combination) always starts from a clean, unheld
    /// modem. A CONNECT-class failure never reaches the disconnect call
    /// (the connect command has already reset/dropped the transport
    /// internally on that path) — see `modem_commands.rs`'s
    /// `modem_ardop_connect_post_consume_with_factory` doc.
    async fn ardop_connect_and_exchange(&self, station: &str, freq_hz: u64) -> Result<(), String> {
        let app = self.app.clone();
        crate::modem_commands::modem_ardop_connect(
            app.clone(),
            app.state::<Arc<crate::modem_status::ModemSession>>(),
            app.state::<Arc<crate::routines::arbiter::RadioArbiter>>(),
            station.to_string(),
            Some(freq_hz),
            None,
        )
        .await?;

        let exchange_result = crate::modem_commands::modem_ardop_b2f_exchange(
            app.clone(),
            app.state::<Arc<crate::modem_status::ModemSession>>(),
            station.to_string(),
            crate::winlink::session::SessionIntent::Cms,
            crate::winlink::listener::transport::TransportKind::Ardop,
        )
        .await;

        let _ = crate::modem_commands::modem_ardop_disconnect(
            app.clone(),
            app.state::<Arc<crate::modem_status::ModemSession>>(),
            // plan 2 Task 5c fix round 2: listener-armed gate param.
            app.state::<Arc<crate::ui_commands::ArdopListenState>>(),
            app.state::<Arc<crate::routines::arbiter::RadioArbiter>>(),
        )
        .await;

        exchange_result
    }

    /// Real VARA HF dial + B2F exchange for ONE resolved `freq_hz`, mirroring
    /// `mcp_ports.rs`'s `vara_open_session` + `vara_b2f_exchange` egress-port
    /// call shape. `TransportKind::VaraHf` (not `VaraFm`) — a Winlink
    /// gateway B2F dial, not a peer/digipeater channel. Always closes the
    /// session afterward (`vara_close_session`, best-effort) — same
    /// transient-per-attempt rationale as the ARDOP path above.
    async fn vara_connect_and_exchange(&self, station: &str, freq_hz: u64) -> Result<(), String> {
        use crate::winlink::listener::transport::TransportKind;
        use crate::winlink::modem::vara::VaraSession;
        use crate::winlink::session::SessionIntent;

        let app = self.app.clone();
        crate::winlink::modem::vara::commands::vara_open_session(
            app.clone(),
            app.state::<Arc<VaraSession>>(),
            app.state::<Arc<crate::session_log::SessionLogState>>(),
            app.state::<Arc<crate::ui_commands::VaraListenState>>(),
            app.state::<Arc<crate::routines::arbiter::RadioArbiter>>(),
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .await?;

        let exchange_result = crate::winlink::modem::vara::commands::modem_vara_b2f_exchange(
            app.clone(),
            app.state::<Arc<crate::session_log::SessionLogState>>(),
            app.state::<Arc<VaraSession>>(),
            station.to_string(),
            SessionIntent::Cms,
            TransportKind::VaraHf,
            Some(freq_hz),
            None,
            None,
        )
        .await;

        let _ = crate::winlink::modem::vara::commands::vara_close_session(
            app.clone(),
            app.state::<Arc<VaraSession>>(),
            app.state::<Arc<crate::session_log::SessionLogState>>(),
            app.state::<Arc<crate::ui_commands::VaraListenState>>(),
            app.state::<Arc<crate::routines::arbiter::RadioArbiter>>(),
        );

        exchange_result
    }
}

/// Real [`AprsService`]: delegates directly to the already-live
/// `AprsState::send` — the SAME call `ui_commands::aprs_send` makes.
/// Requires the APRS engine to already be listening (`aprs_listen_start`);
/// surfaces `AprsState::send`'s own error verbatim otherwise (e.g. "not
/// listening").
pub struct MonolithAprsService {
    app: AppHandle,
}

impl MonolithAprsService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl AprsService for MonolithAprsService {
    async fn send(&self, to: Option<String>, text: String) -> Result<String, String> {
        self.app
            .state::<crate::winlink::aprs::engine::AprsState>()
            .send(to, text)
    }
}

/// Real [`ListenService`]: opens the CONFIGURED ARDOP capture device
/// directly via `ft8::alsa_source::AlsaSource` and accumulates linear RMS
/// over the dwell window. `rig` is currently ignored (v1 has exactly one
/// physical capture device configured) — see the module doc's unit/device
/// choice rationale. **Not unit-tested past its own pure math**
/// ([`rms_linear`]) — opening a real ALSA device needs real hardware,
/// matching `alsa_source.rs`'s own "CI-COMPILE-CHECKED ONLY" precedent.
#[derive(Default)]
pub struct MonolithListenService;

impl MonolithListenService {
    pub fn new() -> Self {
        Self
    }
}

/// Pure, unit-testable core: linear RMS (`0.0..=1.0`) of a batch of `i16`
/// samples. Split out of the ALSA loop so the math has a hardware-free
/// test; the loop itself (below) only accumulates sums and calls this once
/// at the end.
pub(crate) fn rms_linear(sum_sq: f64, count: u64) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let mean_sq = sum_sq / count as f64;
    (mean_sq.sqrt() / f64::from(i16::MAX)) as f32
}

#[async_trait]
impl ListenService for MonolithListenService {
    async fn sample_rms(
        &self,
        _rig: &str,
        seconds: u64,
        cancel: CancellationToken,
    ) -> Result<f32, String> {
        let device = crate::modem_commands::config_get_ardop().capture_device;
        if device.is_empty() {
            return Err(
                "ARDOP capture device not configured — open Settings → ARDOP first".to_string(),
            );
        }

        // Blocking ALSA I/O — off the async executor, mirroring the
        // `spawn_blocking` discipline `modem_ardop_connect` already uses for
        // its own blocking modem calls.
        let handle = tokio::task::spawn_blocking(move || -> Result<f32, String> {
            use crate::ft8::alsa_source::AlsaSource;
            use crate::ft8::traits::SampleSource;

            let mut source = AlsaSource::open(&device).map_err(|e| format!("{e:?}"))?;
            let mut buf = [0i16; 4_800];
            let mut sum_sq: f64 = 0.0;
            let mut count: u64 = 0;
            let start = std::time::Instant::now();
            while start.elapsed().as_secs() < seconds {
                match source.read(&mut buf) {
                    Ok(batch) => {
                        for &s in &buf[..batch.frames] {
                            sum_sq += f64::from(s) * f64::from(s);
                        }
                        count += batch.frames as u64;
                    }
                    Err(e) => return Err(format!("{e:?}")),
                }
            }
            Ok(rms_linear(sum_sq, count))
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("listen cancelled".to_string()),
            joined = handle => match joined {
                Ok(inner) => inner,
                Err(e) => Err(format!("listen task panicked: {e}")),
            },
        }
        // Note: cancellation returns promptly to the caller but does not
        // abort the spawned blocking task, which keeps running until its
        // own `seconds` deadline (bounded, not unbounded) — matching the
        // ARDOP connect walk's own close-generation-check discipline rather
        // than a hard `JoinHandle::abort()` mid-ALSA-call, which could leave
        // the PCM handle in an inconsistent state.
    }
}

// ============================================================================
// Tests — trait fakes, no hardware/tauri. Per plan Task 4's test contract:
// happy path output shape, verbatim error passthrough, lease
// acquired-before/released-after, pause-observed-between-attempts, listen
// threshold boundary.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use tuxlink_routines::types::BusyPolicy as TestBusyPolicy;

    thread_local! {
        static TEST_CLOCK: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    }
    fn test_now() -> i64 {
        TEST_CLOCK.with(|c| c.get())
    }

    fn arbiter() -> Arc<RadioArbiter> {
        Arc::new(RadioArbiter::new(test_now))
    }

    // ---- FakeConnectService -------------------------------------------

    type ConnectFn = dyn Fn(&str, Option<&str>) -> Result<ConnectOutcome, String> + Send + Sync;

    struct FakeConnectService {
        f: Box<ConnectFn>,
    }

    impl FakeConnectService {
        fn new(
            f: impl Fn(&str, Option<&str>) -> Result<ConnectOutcome, String> + Send + Sync + 'static,
        ) -> Self {
            Self { f: Box::new(f) }
        }

        fn always_hard_error(msg: &'static str) -> Self {
            Self::new(move |_s, _b| Err(msg.to_string()))
        }

        fn always_connects(gateway: &'static str) -> Self {
            Self::new(move |_s, _b| {
                Ok(ConnectOutcome {
                    connected: true,
                    gateway: Some(gateway.to_string()),
                    error: None,
                })
            })
        }
    }

    #[async_trait]
    impl ConnectService for FakeConnectService {
        async fn connect_attempt(
            &self,
            station: &str,
            band: Option<&str>,
        ) -> Result<ConnectOutcome, String> {
            (self.f)(station, band)
        }
    }

    // ---- FakeListenService ----------------------------------------------

    struct FakeListenService {
        rms: f32,
        delay: Option<std::time::Duration>,
        on_complete: Option<Arc<Mutex<bool>>>,
    }

    impl FakeListenService {
        fn always(rms: f32) -> Self {
            Self {
                rms,
                delay: None,
                on_complete: None,
            }
        }

        fn with_delay_and_completion_flag(
            rms: f32,
            delay: std::time::Duration,
            on_complete: Arc<Mutex<bool>>,
        ) -> Self {
            Self {
                rms,
                delay: Some(delay),
                on_complete: Some(on_complete),
            }
        }
    }

    #[async_trait]
    impl ListenService for FakeListenService {
        async fn sample_rms(
            &self,
            _rig: &str,
            _seconds: u64,
            _cancel: CancellationToken,
        ) -> Result<f32, String> {
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            if let Some(ref flag) = self.on_complete {
                *flag.lock().unwrap() = true;
            }
            Ok(self.rms)
        }
    }

    struct FailingListenService;

    #[async_trait]
    impl ListenService for FailingListenService {
        async fn sample_rms(
            &self,
            _rig: &str,
            _seconds: u64,
            _cancel: CancellationToken,
        ) -> Result<f32, String> {
            Err("capture device busy".to_string())
        }
    }

    // ---- FakeAprsService --------------------------------------------------

    struct FakeAprsService {
        result: Result<String, String>,
    }

    #[async_trait]
    impl AprsService for FakeAprsService {
        async fn send(&self, _to: Option<String>, _text: String) -> Result<String, String> {
            self.result.clone()
        }
    }

    // ======================================================================
    // radio.connect
    // ======================================================================

    #[tokio::test]
    async fn connect_happy_path_output_shape() {
        let arb = arbiter();
        let action = RadioConnect::new(
            arb,
            Arc::new(FakeConnectService::always_connects("W7DEF-10")),
            Arc::new(FakeListenService::always(0.0)),
        );
        let out = action
            .execute(
                json!({"stations": ["W7DEF-10"], "bands": ["40m"]}),
                CancellationToken::new(),
            )
            .await
            .expect("happy path must succeed");
        assert_eq!(out["connected"], json!(true));
        assert_eq!(out["station"], json!("W7DEF-10"));
        assert_eq!(out["band"], json!("40m"));
        assert_eq!(out["gateway"], json!("W7DEF-10"));
    }

    #[tokio::test]
    async fn connect_exhaustion_is_output_not_step_error() {
        let arb = arbiter();
        let action = RadioConnect::new(
            arb,
            Arc::new(FakeConnectService::new(|_s, _b| {
                Ok(ConnectOutcome {
                    connected: false,
                    gateway: None,
                    error: Some("no ACK".to_string()),
                })
            })),
            Arc::new(FakeListenService::always(0.0)),
        );
        let out = action
            .execute(
                json!({"stations": ["W7DEF-10", "K7ABC-10"], "bands": ["40m", "80m"]}),
                CancellationToken::new(),
            )
            .await
            .expect("exhaustion must be Ok, not a StepError");
        assert_eq!(out["connected"], json!(false));
        assert_eq!(out["last_error"], json!("no ACK"));
    }

    #[tokio::test]
    async fn connect_verbatim_hard_error_passthrough() {
        let arb = arbiter();
        let action = RadioConnect::new(
            arb,
            Arc::new(FakeConnectService::always_hard_error(
                "rig unreachable: fd closed",
            )),
            Arc::new(FakeListenService::always(0.0)),
        );
        let err = action
            .execute(
                json!({"stations": ["W7DEF-10"], "bands": ["40m"]}),
                CancellationToken::new(),
            )
            .await
            .expect_err("hard transport failure must be a StepError");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "radio.connect");
                assert_eq!(cause, "rig unreachable: fd closed");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn connect_invalid_params_is_a_step_error() {
        let arb = arbiter();
        let action = RadioConnect::new(
            arb,
            Arc::new(FakeConnectService::always_connects("X")),
            Arc::new(FakeListenService::always(0.0)),
        );
        let err = action
            .execute(
                json!({"stations": [], "bands": ["40m"]}),
                CancellationToken::new(),
            )
            .await
            .expect_err("empty stations must error");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[tokio::test]
    async fn connect_lease_is_held_during_and_released_after() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        let observed_during: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
        let od = observed_during.clone();
        let arb_for_fake = arb.clone();
        let fake = FakeConnectService::new(move |_s, _b| {
            let held = arb_for_fake.status(rig).is_some();
            *od.lock().unwrap() = Some(held);
            Ok(ConnectOutcome {
                connected: true,
                gateway: Some("X".to_string()),
                error: None,
            })
        });
        let action = RadioConnect::new(
            arb.clone(),
            Arc::new(fake),
            Arc::new(FakeListenService::always(0.0)),
        );

        assert!(arb.status(rig).is_none(), "nothing holds the rig yet");
        let _out = action
            .execute(
                json!({"stations": ["W7DEF-10"], "bands": ["40m"]}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            *observed_during.lock().unwrap(),
            Some(true),
            "lease must be held DURING connect_attempt"
        );
        assert!(
            arb.status(rig).is_none(),
            "lease must be released AFTER execute returns"
        );
    }

    #[tokio::test]
    async fn connect_pause_requested_between_attempts_releases_and_reacquires() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        let observed_pause_at_call: Arc<Mutex<Vec<bool>>> = Arc::new(Mutex::new(Vec::new()));
        let obs = observed_pause_at_call.clone();
        let arb_for_fake = arb.clone();
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();

        let fake = FakeConnectService::new(move |_s, _b| {
            let n = cc.fetch_add(1, Ordering::SeqCst);
            let pause_now = arb_for_fake
                .status(rig)
                .map(|s| s.pause_requested)
                .unwrap_or(false);
            obs.lock().unwrap().push(pause_now);
            if n == 0 {
                // Simulate the operator taking control mid-first-attempt —
                // the CURRENT lease's pause token gets cancelled. The
                // action's loop must observe this BEFORE attempt 2 and
                // cycle to a fresh lease (whose pause token starts
                // un-cancelled again).
                arb_for_fake.operator_take(rig);
                Ok(ConnectOutcome {
                    connected: false,
                    gateway: None,
                    error: Some("nope".to_string()),
                })
            } else {
                Ok(ConnectOutcome {
                    connected: true,
                    gateway: Some("W7DEF-10".to_string()),
                    error: None,
                })
            }
        });

        let action = RadioConnect::new(
            arb.clone(),
            Arc::new(fake),
            Arc::new(FakeListenService::always(0.0)),
        );
        let out = action
            .execute(
                json!({"stations": ["W7DEF-10"], "bands": ["40m", "80m"]}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["connected"], json!(true));
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        // Call 1 sees a fresh (never-paused) lease. Call 2 must ALSO see a
        // fresh lease (pause_requested == false) — proof the loop released
        // the pause-signalled lease and re-acquired a new one BEFORE the
        // second attempt. A buggy implementation that kept the stale,
        // already-cancelled lease would observe `true` at call 2 (a
        // `CancellationToken`'s cancellation is permanent/sticky).
        assert_eq!(*observed_pause_at_call.lock().unwrap(), vec![false, false]);
    }

    #[tokio::test]
    async fn connect_observes_cancellation_promptly() {
        let arb = arbiter();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let action = RadioConnect::new(
            arb,
            Arc::new(FakeConnectService::always_connects("X")),
            Arc::new(FakeListenService::always(0.0)),
        );
        let err = action
            .execute(json!({"stations": ["W7DEF-10"], "bands": ["40m"]}), cancel)
            .await
            .expect_err("a pre-cancelled token must not attempt a dial");
        assert!(matches!(err, StepError::Cancelled));
    }

    #[tokio::test]
    async fn connect_pre_flight_listen_busy_skips_attempt_then_recovers() {
        let arb = arbiter();
        let fake = FakeConnectService::always_connects("W7DEF-10");
        // First dwell reads busy (above threshold), second reads quiet.
        struct AlternatingListen {
            n: AtomicUsize,
        }
        #[async_trait]
        impl ListenService for AlternatingListen {
            async fn sample_rms(
                &self,
                _rig: &str,
                _seconds: u64,
                _cancel: CancellationToken,
            ) -> Result<f32, String> {
                let n = self.n.fetch_add(1, Ordering::SeqCst);
                Ok(if n == 0 { 0.5 } else { 0.0 })
            }
        }
        let action = RadioConnect::new(
            arb,
            Arc::new(fake),
            Arc::new(AlternatingListen {
                n: AtomicUsize::new(0),
            }),
        );
        let out = action
            .execute(
                json!({
                    "stations": ["W7DEF-10"],
                    "bands": ["40m", "80m"],
                    "listen_before_tx_s": 3
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["connected"], json!(true));
        assert_eq!(out["band"], json!("80m"), "40m must be skipped as busy");
    }

    // ---- P1 fix regression: bands is optional/transport-aware -----------

    #[tokio::test]
    async fn connect_packet_shape_absent_bands_drives_connect_attempt_with_band_none() {
        let arb = arbiter();
        let observed_band: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
        let ob = observed_band.clone();
        let fake = FakeConnectService::new(move |_s, b| {
            *ob.lock().unwrap() = Some(b.map(|s| s.to_string()));
            Ok(ConnectOutcome {
                connected: true,
                gateway: Some("W7DEF-10".to_string()),
                error: None,
            })
        });
        let action = RadioConnect::new(
            arb,
            Arc::new(fake),
            Arc::new(FakeListenService::always(0.0)),
        );
        // `bands` key entirely absent — this is what a packet-dial routine
        // step looks like: no HF band concept applies.
        let out = action
            .execute(json!({"stations": ["W7DEF-10"]}), CancellationToken::new())
            .await
            .expect("packet-shape (bands absent) dial must succeed, not error");
        assert_eq!(out["connected"], json!(true));
        assert_eq!(
            out["band"],
            json!(null),
            "band-less dial reports band: null in the output"
        );
        assert_eq!(
            *observed_band.lock().unwrap(),
            Some(None),
            "connect_attempt must be driven with band: None, not skipped/faked"
        );
    }

    #[tokio::test]
    async fn connect_explicit_empty_bands_array_is_also_packet_shape() {
        let arb = arbiter();
        let action = RadioConnect::new(
            arb,
            Arc::new(FakeConnectService::always_connects("W7DEF-10")),
            Arc::new(FakeListenService::always(0.0)),
        );
        // Explicit `"bands": []` (not just an absent key) must behave
        // identically — the old validation (`stations.is_empty() ||
        // bands.is_empty()`) treated this as a hard param error.
        let out = action
            .execute(
                json!({"stations": ["W7DEF-10"], "bands": []}),
                CancellationToken::new(),
            )
            .await
            .expect("explicit empty bands array must not be a param error");
        assert_eq!(out["connected"], json!(true));
        assert_eq!(out["band"], json!(null));
    }

    #[tokio::test]
    async fn connect_station_x_band_still_iterates_when_bands_present() {
        // Regression guard for the non-empty-bands path alongside the new
        // empty-bands shape: multiple stations, multiple bands, in
        // station-major order, still walks every combination until one
        // connects.
        let arb = arbiter();
        #[allow(clippy::type_complexity)] // seen-combos capture in a test
        let seen: Arc<Mutex<Vec<(String, Option<String>)>>> = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let fake = FakeConnectService::new(move |station, band| {
            s.lock()
                .unwrap()
                .push((station.to_string(), band.map(|b| b.to_string())));
            Ok(ConnectOutcome {
                connected: false,
                gateway: None,
                error: Some("no ACK".to_string()),
            })
        });
        let action = RadioConnect::new(
            arb,
            Arc::new(fake),
            Arc::new(FakeListenService::always(0.0)),
        );
        let out = action
            .execute(
                json!({
                    "stations": ["W7DEF-10", "K7ABC-10"],
                    "bands": ["40m", "80m"]
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["connected"], json!(false));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                ("W7DEF-10".to_string(), Some("40m".to_string())),
                ("W7DEF-10".to_string(), Some("80m".to_string())),
                ("K7ABC-10".to_string(), Some("40m".to_string())),
                ("K7ABC-10".to_string(), Some("80m".to_string())),
            ],
            "station-major order: exhaust W7DEF-10's bands before K7ABC-10's"
        );
    }

    // ======================================================================
    // radio.listen
    // ======================================================================

    #[tokio::test]
    async fn listen_happy_path_reports_quiet_channel() {
        let arb = arbiter();
        let action = RadioListen::new(arb, Arc::new(FakeListenService::always(0.001)));
        let out = action
            .execute(json!({"seconds": 5}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["channel_busy"], json!(false));
        assert_eq!(out["rms"], json!(0.001_f32));
    }

    #[tokio::test]
    async fn listen_threshold_boundary() {
        let arb = arbiter();
        // Exactly at the threshold is NOT busy (strict `>`).
        let action = RadioListen::new(
            arb.clone(),
            Arc::new(FakeListenService::always(CHANNEL_BUSY_RMS_THRESHOLD)),
        );
        let out = action
            .execute(json!({"seconds": 1}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            out["channel_busy"],
            json!(false),
            "exactly-at-threshold is not busy"
        );

        // Just above the threshold IS busy.
        let action = RadioListen::new(
            arb,
            Arc::new(FakeListenService::always(
                CHANNEL_BUSY_RMS_THRESHOLD + 0.0001,
            )),
        );
        let out = action
            .execute(json!({"seconds": 1}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["channel_busy"], json!(true));
    }

    #[tokio::test]
    async fn listen_verbatim_error_passthrough() {
        let arb = arbiter();
        let action = RadioListen::new(arb, Arc::new(FailingListenService));
        let err = action
            .execute(json!({"seconds": 5}), CancellationToken::new())
            .await
            .expect_err("listen failure must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "radio.listen");
                assert_eq!(cause, "capture device busy");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn listen_lease_is_held_during_and_released_after() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        struct ObservingListen {
            arbiter: Arc<RadioArbiter>,
            rig: &'static str,
            observed: Arc<Mutex<Option<bool>>>,
        }
        #[async_trait]
        impl ListenService for ObservingListen {
            async fn sample_rms(
                &self,
                _rig: &str,
                _seconds: u64,
                _cancel: CancellationToken,
            ) -> Result<f32, String> {
                *self.observed.lock().unwrap() = Some(self.arbiter.status(self.rig).is_some());
                Ok(0.0)
            }
        }
        let observed = Arc::new(Mutex::new(None));
        let action = RadioListen::new(
            arb.clone(),
            Arc::new(ObservingListen {
                arbiter: arb.clone(),
                rig,
                observed: observed.clone(),
            }),
        );
        action
            .execute(json!({"seconds": 1}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(*observed.lock().unwrap(), Some(true));
        assert!(arb.status(rig).is_none(), "lease released after listen");
    }

    #[tokio::test(start_paused = true)]
    async fn listen_cancel_during_sample_lets_it_finish_then_returns_cancelled() {
        let arb = arbiter();
        let sample_ran_to_completion = Arc::new(Mutex::new(false));
        let src = sample_ran_to_completion.clone();
        let listen = Arc::new(FakeListenService::with_delay_and_completion_flag(
            0.01,
            std::time::Duration::from_secs(20),
            src,
        ));
        let action = RadioListen::new(arb, listen);
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let handle = tokio::spawn(async move {
            action
                .execute(json!({"seconds": 20}), cancel_for_task)
                .await
        });

        // Let the sample start and get partway through its simulated dwell,
        // THEN cancel — well before the fake's 20 s completes. Paused time
        // advances exactly this far (not further) while this sleep is
        // pending, so the cancel genuinely lands mid-sample.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        assert!(
            !*sample_ran_to_completion.lock().unwrap(),
            "sanity: sample must still be in flight when cancel fires"
        );
        cancel.cancel();

        // `handle.await` drives paused time forward the remaining ~15 s the
        // fake sample is still sleeping through — proving execute() really
        // awaited the sample to completion instead of returning as soon as
        // cancellation was observed.
        let result = handle.await.expect("execute task must not panic");
        assert!(
            matches!(result, Err(StepError::Cancelled)),
            "cancellation during sample must still surface as Cancelled, got {result:?}"
        );
        assert!(
            *sample_ran_to_completion.lock().unwrap(),
            "the in-flight sample must run to completion — a cancelled lease must not lie \
             about physical rig use"
        );
    }

    // ======================================================================
    // radio.aprs_send
    // ======================================================================

    #[tokio::test]
    async fn aprs_send_happy_path_output_shape() {
        let arb = arbiter();
        let action = RadioAprsSend::new(
            arb,
            Arc::new(FakeAprsService {
                result: Ok("m123".to_string()),
            }),
        );
        let out = action
            .execute(
                json!({"to": "W7DEF-10", "text": "hello"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["msgid"], json!("m123"));
    }

    #[tokio::test]
    async fn aprs_send_verbatim_error_passthrough() {
        let arb = arbiter();
        let action = RadioAprsSend::new(
            arb,
            Arc::new(FakeAprsService {
                result: Err("APRS engine not listening".to_string()),
            }),
        );
        let err = action
            .execute(json!({"text": "hello"}), CancellationToken::new())
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "radio.aprs_send");
                assert_eq!(cause, "APRS engine not listening");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn aprs_send_lease_is_held_during_and_released_after() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        struct ObservingAprs {
            arbiter: Arc<RadioArbiter>,
            rig: &'static str,
            observed: Arc<Mutex<Option<bool>>>,
        }
        #[async_trait]
        impl AprsService for ObservingAprs {
            async fn send(&self, _to: Option<String>, _text: String) -> Result<String, String> {
                *self.observed.lock().unwrap() = Some(self.arbiter.status(self.rig).is_some());
                Ok("m1".to_string())
            }
        }
        let observed = Arc::new(Mutex::new(None));
        let action = RadioAprsSend::new(
            arb.clone(),
            Arc::new(ObservingAprs {
                arbiter: arb.clone(),
                rig,
                observed: observed.clone(),
            }),
        );
        action
            .execute(json!({"text": "hi"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(*observed.lock().unwrap(), Some(true));
        assert!(arb.status(rig).is_none(), "lease released after send");
    }

    // ======================================================================
    // GatewayFrequencyResolver (plan 2 Task 5c)
    // ======================================================================

    use crate::catalog::stations::{Gateway, ListingMode, StationListing};
    use crate::catalog::stations_cache::StationsCache;

    fn gateway(callsign: &str, freqs_khz: &[f64]) -> Gateway {
        Gateway {
            channel: format!("{callsign}.WINLINK"),
            callsign: callsign.to_string(),
            sysop_name: None,
            grid: None,
            location: None,
            frequencies_khz: freqs_khz.to_vec(),
            last_update: None,
            email: None,
            homepage: None,
            antenna: None,
        }
    }

    /// Builds a cache whose ONE stored entry's `fetched_at_ms` is exactly
    /// `fetched_at_ms` — `StationsCache::insert` always re-stamps from its
    /// clock, so the clock itself is fixed to that value (not passed
    /// through as a plain struct field, which `insert` would silently
    /// overwrite).
    fn cache_with(mode: ListingMode, gateways: Vec<Gateway>, fetched_at_ms: u64) -> Arc<StationsCache> {
        let cache = Arc::new(StationsCache::new(
            u64::MAX,
            0,
            Arc::new(FixedClock(fetched_at_ms)),
        ));
        let listing = StationListing {
            mode,
            title: None,
            gateways,
            raw: "test".to_string(),
            parsed_ok: true,
            fetched_at_ms: None,
        };
        cache.insert(
            crate::catalog::stations_cache::CacheKey {
                mode,
                service_codes: "PUBLIC".to_string(),
                history_hours: 168,
            },
            listing,
        );
        cache
    }

    struct FixedClock(u64);
    impl crate::catalog::stations_cache::Clock for FixedClock {
        fn now_millis(&self) -> u64 {
            self.0
        }
    }

    #[test]
    fn band_range_known_labels_case_insensitive() {
        assert_eq!(band_range("40m"), Some((7000.0, 7300.0)));
        assert_eq!(band_range("40M"), Some((7000.0, 7300.0)));
        assert_eq!(band_range("20m"), Some((14000.0, 14350.0)));
    }

    #[test]
    fn band_range_unknown_label_is_none() {
        assert_eq!(band_range("6m"), None);
        assert_eq!(band_range("not-a-band"), None);
    }

    #[test]
    fn resolve_filters_frequencies_to_the_requested_band() {
        // Two gateways, only one of which has a frequency actually inside
        // 40m; the other's frequencies are 80m/20m and must be excluded.
        let cache = cache_with(
            ListingMode::ArdopHf,
            vec![
                gateway("W7DEF-10", &[3589.0, 7101.6, 14096.4]),
                gateway("K7ABC-10", &[3600.0, 21050.0]),
            ],
            1_000,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let freqs = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "40m", 2_000)
            .expect("must resolve — 7101.6 kHz is in 40m");
        assert_eq!(freqs, vec![7_101_600]);
    }

    #[test]
    fn resolve_matches_callsign_case_insensitively() {
        let cache = cache_with(
            ListingMode::VaraHf,
            vec![gateway("w7def-10", &[7101.6])],
            0,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let freqs = resolver
            .resolve(ListingMode::VaraHf, "W7DEF-10", "40m", 0)
            .unwrap();
        assert_eq!(freqs, vec![7_101_600]);
    }

    #[test]
    fn resolve_dedupes_identical_frequencies() {
        let cache = cache_with(
            ListingMode::ArdopHf,
            vec![gateway("W7DEF-10", &[7101.6, 7101.6])],
            0,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let freqs = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "40m", 0)
            .unwrap();
        assert_eq!(freqs, vec![7_101_600]);
    }

    #[test]
    fn resolve_no_frequency_error_names_station_band_and_cache_age_when_cache_has_data() {
        let cache = cache_with(
            ListingMode::ArdopHf,
            // W7DEF-10 exists in the cache, but has no 40m frequency.
            vec![gateway("W7DEF-10", &[14096.4])],
            1_000,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let err = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "40m", 1_000 + 30_000)
            .expect_err("no matching frequency must error");
        assert_eq!(err.station, "W7DEF-10");
        assert_eq!(err.band, "40m");
        assert_eq!(err.cache_age_s, Some(30));
        let msg = err.message();
        assert!(msg.contains("W7DEF-10"));
        assert!(msg.contains("40m"));
        assert!(msg.contains("30s"));
        assert!(msg.contains("data.stationlist_update"));
    }

    #[test]
    fn resolve_no_frequency_error_when_station_not_in_cache_at_all() {
        let cache = cache_with(
            ListingMode::ArdopHf,
            vec![gateway("K7ABC-10", &[7101.6])],
            0,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let err = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "40m", 0)
            .expect_err("station not present must error");
        assert_eq!(err.station, "W7DEF-10");
        assert_eq!(
            err.cache_age_s,
            Some(0),
            "cache has SOME data for the mode (freshest listing tracked), just not this station"
        );
    }

    #[test]
    fn resolve_empty_cache_reports_no_cache_age() {
        let cache = Arc::new(StationsCache::new(u64::MAX, 0, Arc::new(FixedClock(0))));
        let resolver = GatewayFrequencyResolver::new(cache);
        let err = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "40m", 0)
            .expect_err("empty cache must error");
        assert_eq!(err.cache_age_s, None, "no entry at all — never fetched");
        assert!(err.message().contains("run data.stationlist_update"));
    }

    #[test]
    fn resolve_unrecognized_band_label_is_a_no_frequency_error() {
        let cache = cache_with(
            ListingMode::ArdopHf,
            vec![gateway("W7DEF-10", &[7101.6])],
            0,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let err = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "not-a-band", 0)
            .expect_err("unrecognized band label must error, not panic");
        assert_eq!(err.band, "not-a-band");
    }

    #[test]
    fn resolve_wrong_mode_finds_nothing_even_with_matching_station_in_another_mode() {
        // ARDOP and VARA channel lists are DISTINCT listings — a station
        // cached under VaraHf must not satisfy an ArdopHf resolve.
        let cache = cache_with(
            ListingMode::VaraHf,
            vec![gateway("W7DEF-10", &[7101.6])],
            0,
        );
        let resolver = GatewayFrequencyResolver::new(cache);
        let err = resolver
            .resolve(ListingMode::ArdopHf, "W7DEF-10", "40m", 0)
            .expect_err("wrong mode must not find the VARA-cached entry");
        assert_eq!(err.cache_age_s, None);
    }

    // ======================================================================
    // MonolithListenService's pure math core
    // ======================================================================

    #[test]
    fn rms_linear_of_silence_is_zero() {
        assert_eq!(rms_linear(0.0, 0), 0.0);
        assert_eq!(rms_linear(0.0, 100), 0.0);
    }

    #[test]
    fn rms_linear_of_full_scale_square_wave_is_one() {
        // Every sample at +/- i16::MAX: sum_sq = count * MAX^2 -> rms = MAX.
        let max = f64::from(i16::MAX);
        let count = 100u64;
        let sum_sq = max * max * count as f64;
        let rms = rms_linear(sum_sq, count);
        assert!((rms - 1.0).abs() < 1e-4, "got {rms}");
    }

    /// Regression guard: a plain unused-import check that `BusyPolicy` is
    /// reachable at the same path the envelope helpers deserialize against
    /// (a rename drift here would silently make `_radio_busy_policy` always
    /// fall back to the default instead of erroring loudly).
    #[test]
    fn busy_policy_wire_shape_is_lowercase() {
        let v = serde_json::to_value(TestBusyPolicy::Fail).unwrap();
        assert_eq!(v, json!("fail"));
    }
}
