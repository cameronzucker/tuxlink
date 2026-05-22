//! gpsfake end-to-end integration test: gpsd → PositionArbiter → grid chain.
//!
//! bd issue: tuxlink-686 (Task 12)
//!
//! # Why this test is gated
//!
//! This test spawns a `gpsfake`/`gpsd` process and takes several seconds.
//! It is skipped unless BOTH:
//!   1. `TUXLINK_GPSFAKE_TEST=1` env var is set (explicit opt-in), AND
//!   2. `gpsfake` is present on PATH.
//!
//! Default `cargo test` and CI environments without gpsd skip it cleanly.
//!
//! # Fixture
//!
//! The NMEA fixture encodes Munich (~48.143°N, 11.608°E).  The expected
//! grid is computed at test runtime via `tuxlink_lib::position::lat_lon_to_grid`
//! so the assertion always tracks the live conversion implementation.
//!
//! # Safety
//!
//! `gpsfake` replays a canned NMEA file into a private gpsd on loopback port 2948.
//! No real GPS is read, no radio transmission is involved.  The system gpsd on
//! port 2947 (serving the LC29C) is left untouched.

use std::io::Write;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tuxlink_lib::config::{PositionPrecision, PositionSource};
use tuxlink_lib::position::{lat_lon_to_grid, PositionArbiter};
use tuxlink_lib::position::gpsd::run_gpsd_client;

// ---------------------------------------------------------------------------
// Fixture port — must NOT collide with the system gpsd on 2947
// ---------------------------------------------------------------------------
const GPSFAKE_PORT: u16 = 2948;

// ---------------------------------------------------------------------------
// NMEA fixture: Munich (48.143°N, 11.608°E)
//   In NMEA DDmm.mmmm notation: 4808.5800,N / 01136.4800,E
//   lat_lon_to_grid(48.143, 11.608) → "JN58td"  (verified Task 9 + unit test)
//   Checksums verified against gpsd 3.25 (bad checksum = no TPV, see iteration notes).
// ---------------------------------------------------------------------------
const LAT: f64 = 48.143;
const LON: f64 = 11.608;

const NMEA_FIXTURE: &str = "\
$GPGGA,120000.00,4808.5800,N,01136.4800,E,1,08,1.0,520.0,M,47.0,M,,*62\r\n\
$GPRMC,120000.00,A,4808.5800,N,01136.4800,E,0.0,0.0,220526,0.0,E*5A\r\n\
$GPGGA,120001.00,4808.5800,N,01136.4800,E,1,08,1.0,520.0,M,47.0,M,,*63\r\n\
$GPRMC,120001.00,A,4808.5800,N,01136.4800,E,0.0,0.0,220526,0.0,E*5B\r\n\
$GPGGA,120002.00,4808.5800,N,01136.4800,E,1,08,1.0,520.0,M,47.0,M,,*60\r\n\
$GPRMC,120002.00,A,4808.5800,N,01136.4800,E,0.0,0.0,220526,0.0,E*58\r\n";

// ---------------------------------------------------------------------------
// KillOnDrop — RAII guard that kills the gpsfake child on every exit path
// ---------------------------------------------------------------------------
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait(); // reap so we don't leak a zombie
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gpsd_fake_fix_reaches_arbiter_as_expected_grid() {
    // ---- Gate 1: opt-in env flag -----------------------------------------------
    if std::env::var("TUXLINK_GPSFAKE_TEST").unwrap_or_default() != "1" {
        eprintln!(
            "skipping gpsd_fake e2e: set TUXLINK_GPSFAKE_TEST=1 and install gpsfake"
        );
        return;
    }

    // ---- Gate 2: gpsfake on PATH -----------------------------------------------
    let gpsfake_path = match which_gpsfake() {
        Some(p) => p,
        None => {
            eprintln!(
                "skipping gpsd_fake e2e: set TUXLINK_GPSFAKE_TEST=1 and install gpsfake"
            );
            return;
        }
    };

    // ---- Expected grid (computed from live implementation) ---------------------
    let expected_grid = lat_lon_to_grid(LAT, LON);
    eprintln!("Expected grid for ({LAT}, {LON}): {expected_grid}");
    assert_eq!(expected_grid.len(), 6, "lat_lon_to_grid should return 6-char grid");

    // ---- NMEA fixture file (tempfile, auto-deleted on drop) --------------------
    let mut fixture_file = tempfile::NamedTempFile::new()
        .expect("create temp NMEA fixture");
    fixture_file
        .write_all(NMEA_FIXTURE.as_bytes())
        .expect("write NMEA fixture");
    let fixture_path = fixture_file.path().to_path_buf();
    eprintln!("NMEA fixture: {}", fixture_path.display());

    // ---- Launch gpsfake --------------------------------------------------------
    // -P <port>  — listen on a non-default port (must not be 2947)
    // -n         — start reading immediately, don't wait for a client
    // -q         — quiet (suppress progress chatter)
    // Loop is the default (no -1 flag) so the fixture replays continuously.
    let child = Command::new(&gpsfake_path)
        .args([
            "-P",
            &GPSFAKE_PORT.to_string(),
            "-n",
            "-q",
            fixture_path.to_str().expect("fixture path is UTF-8"),
        ])
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn gpsfake {gpsfake_path}: {e}"));
    let _guard = KillOnDrop(child); // killed on any exit path, including panic

    // Give gpsfake/gpsd a moment to bind the port and start replaying
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // ---- Build arbiter (source = Gps so a fix becomes the active grid) ---------
    let arbiter = Arc::new(PositionArbiter::new(
        PositionSource::Gps,
        None,
        PositionPrecision::FourCharGrid, // broadcast precision 4-char
    ));

    // ---- Spawn gpsd client pointed at the fake gpsd ---------------------------
    let addr = format!("127.0.0.1:{GPSFAKE_PORT}");
    eprintln!("Connecting gpsd client to {addr}");
    let arbiter_for_task = Arc::clone(&arbiter);
    tokio::spawn(run_gpsd_client(arbiter_for_task, addr));

    // ---- Poll for fix with generous timeout ------------------------------------
    let timeout = Duration::from_secs(15);
    let poll_interval = Duration::from_millis(250);
    let deadline = Instant::now() + timeout;

    loop {
        if arbiter.has_fresh_fix() {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "gpsd_fake e2e: no GPS fix received within {:?}. \
                 Check gpsfake output above for errors.",
                timeout
            );
        }
        tokio::time::sleep(poll_interval).await;
    }

    eprintln!(
        "Fix arrived. active_grid={:?}  broadcast_grid={:?}",
        arbiter.active_grid(),
        arbiter.broadcast_grid()
    );

    // ---- Assertions ------------------------------------------------------------
    // Full 6-char active grid must match the computed expected grid
    assert_eq!(
        arbiter.active_grid().as_deref(),
        Some(expected_grid.as_str()),
        "active_grid should be the full 6-char grid for the Munich fix"
    );

    // broadcast_grid (FourCharGrid precision) must be the first 4 chars
    let expected_broadcast = &expected_grid[..4];
    assert_eq!(
        arbiter.broadcast_grid().as_deref(),
        Some(expected_broadcast),
        "broadcast_grid with FourCharGrid precision should be the first 4 chars"
    );

    eprintln!(
        "gpsd_fake e2e PASSED: active={} broadcast={}",
        expected_grid,
        expected_broadcast,
    );
    // _guard drops here → kills gpsfake/gpsd child
}

// ---------------------------------------------------------------------------
// Helper: find gpsfake binary on PATH
// ---------------------------------------------------------------------------
fn which_gpsfake() -> Option<String> {
    // Check common locations first
    for candidate in &["/usr/bin/gpsfake", "/usr/local/bin/gpsfake"] {
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    // Fall back to PATH search via `which`
    let out = Command::new("which").arg("gpsfake").output().ok()?;
    if out.status.success() {
        let path = String::from_utf8(out.stdout).ok()?;
        Some(path.trim().to_string())
    } else {
        None
    }
}
