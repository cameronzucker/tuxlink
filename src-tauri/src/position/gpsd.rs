//! gpsd TPV client: parse gpsd JSON fix reports into position `Fix`es and
//! drive the async watch loop that feeds them to the `PositionArbiter`.

use std::sync::Arc;
use std::time::Duration;
use crate::position::{lat_lon_to_grid, Fix, PositionArbiter};

// ---------------------------------------------------------------------------
// Backoff constants — private (module-internal; tests reach via `super::`)
// ---------------------------------------------------------------------------

const GPSD_BACKOFF_MIN: Duration = Duration::from_secs(1);
const GPSD_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Default gpsd address — overridden by `TUXLINK_GPSD_ADDR` env var.
pub(crate) const GPSD_DEFAULT_ADDR: &str = "127.0.0.1:2947";

/// Capped exponential backoff: ZERO (just-connected / first-try) → 1 s, then
/// double up to a 30 s cap. Pure function; unit-tested.
fn next_backoff(prev: Duration) -> Duration {
    if prev.is_zero() {
        GPSD_BACKOFF_MIN
    } else {
        (prev * 2).min(GPSD_BACKOFF_MAX)
    }
}

/// Parse ONE gpsd JSON line into a `Fix`. Accepts only a TPV report with a usable
/// fix: `class == "TPV"` AND `mode >= 2` (2 = 2D, 3 = 3D; 0/1 = no fix) AND both
/// `lat`/`lon` present. Returns `None` for anything else (non-TPV, no-fix, malformed
/// JSON, missing fields). Uses `serde_json::Value` (already a dependency) to avoid a
/// rigid struct.
pub(crate) fn parse_tpv(line: &str) -> Option<Fix> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("class")?.as_str()? != "TPV" { return None; }
    if v.get("mode")?.as_i64()? < 2 { return None; }   // 0/1 = no fix
    let lat = v.get("lat")?.as_f64()?;
    let lon = v.get("lon")?.as_f64()?;
    Some(Fix { grid: lat_lon_to_grid(lat, lon), received: std::time::Instant::now() })
}

// ---------------------------------------------------------------------------
// Async watch loop (Task 10)
// ---------------------------------------------------------------------------

/// Connect to gpsd at `addr`, enable JSON watch, and feed every usable TPV fix to
/// the arbiter. Runs forever: on connect failure or EOF, reconnects with capped
/// exponential backoff (1 s → 30 s). gpsd being absent is a NORMAL "no GPS" state —
/// logged once per unavailability period (flag resets when a connection succeeds, so new outages are reported again) to avoid stderr spam — never a hard error.
///
/// `addr` is an explicit parameter (not hard-coded) so the gpsfake integration test
/// (Task 12) can point it at a test gpsd instance.
pub async fn run_gpsd_client(arbiter: Arc<PositionArbiter>, addr: String) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    let mut backoff = Duration::ZERO;
    let mut logged_down = false;

    loop {
        match TcpStream::connect(&addr).await {
            Ok(mut stream) => {
                backoff = Duration::ZERO;   // reset on successful connect — if connection succeeds then immediately drops, backoff resets to 1s (gpsd is up/restarting → fast reconnect is right); only CONSECUTIVE connect failures escalate toward the 30s cap
                logged_down = false;
                tracing::info!(
                    target: "tuxlink::position::gpsd",
                    addr = %addr,
                    "gpsd connected",
                );
                // Enable JSON watch mode, then read fixes until EOF / error.
                if stream
                    .write_all(b"?WATCH={\"enable\":true,\"json\":true}\n")
                    .await
                    .is_ok()
                {
                    let mut fix_count: u64 = 0;
                    let mut lines = BufReader::new(stream).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(fix) = parse_tpv(&line) {
                            fix_count += 1;
                            tracing::debug!(
                                target: "tuxlink::position::gpsd",
                                grid = %fix.grid,
                                fix_count,
                                "GPS fix received",
                            );
                            arbiter.apply_gps_fix(fix);
                        }
                    }
                }
                tracing::info!(
                    target: "tuxlink::position::gpsd",
                    addr = %addr,
                    "gpsd connection closed; will reconnect",
                );
                // Fell out of the read loop — connection closed; reconnect after backoff.
            }
            Err(e) => {
                if !logged_down {
                    tracing::warn!(
                        target: "tuxlink::position::gpsd",
                        addr = %addr,
                        error = %e,
                        "gpsd unavailable — will keep retrying (normal when no GPS is connected)",
                    );
                    eprintln!(
                        "gpsd unavailable at {addr}: {e} — \
                         will keep retrying (this is normal when no GPS is connected)"
                    );
                    logged_down = true;
                }
            }
        }

        backoff = next_backoff(backoff);
        tokio::time::sleep(backoff).await;
    }
}

/// Spawn the gpsd watch task onto Tauri's global async runtime (valid post-`.setup()`).
/// Reads `TUXLINK_GPSD_ADDR` (default `127.0.0.1:2947`) — the dev-override idiom
/// matching `TUXLINK_CMS_HOST`. Fire-and-forget: the task runs for the app's lifetime.
pub fn spawn_gpsd_client(arbiter: Arc<PositionArbiter>) {
    let addr = std::env::var("TUXLINK_GPSD_ADDR")
        .unwrap_or_else(|_| GPSD_DEFAULT_ADDR.to_string());
    tauri::async_runtime::spawn(run_gpsd_client(arbiter, addr));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_3d_tpv_into_a_grid() {
        let line = r#"{"class":"TPV","mode":3,"lat":48.143,"lon":11.608}"#;
        let fix = parse_tpv(line).unwrap();
        assert_eq!(fix.grid, "JN58td");
    }
    #[test]
    fn rejects_no_fix_and_non_tpv() {
        assert!(parse_tpv(r#"{"class":"TPV","mode":1}"#).is_none());   // no fix (mode 1)
        assert!(parse_tpv(r#"{"class":"SKY"}"#).is_none());            // not a fix report
        assert!(parse_tpv("not json").is_none());                      // not JSON
    }

    // ----- next_backoff -----
    #[test]
    fn backoff_starts_at_one_second() {
        assert_eq!(next_backoff(Duration::ZERO), Duration::from_secs(1));
    }
    #[test]
    fn backoff_doubles() {
        assert_eq!(next_backoff(Duration::from_secs(1)), Duration::from_secs(2));
        assert_eq!(next_backoff(Duration::from_secs(8)), Duration::from_secs(16));
    }
    #[test]
    fn backoff_caps_at_thirty_seconds() {
        assert_eq!(next_backoff(Duration::from_secs(16)), Duration::from_secs(30)); // 32 → capped
        assert_eq!(next_backoff(Duration::from_secs(30)), Duration::from_secs(30)); // stays capped
    }
}
