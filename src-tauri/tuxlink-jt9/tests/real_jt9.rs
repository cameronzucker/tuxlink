//! End-to-end against the REAL jt9 on the committed SDR fixtures.
//! Skips (with a printed notice) when jt9 is absent; CI installs wsjtx so
//! these always run there. Locally on the dev Pi they run in seconds.
//!
//! All 4 tests share ONE warmed FFTW wisdom directory (see `SHARED` below)
//! instead of each paying its own cold FFTW plan under the full 12s
//! production timeout — 4 independent cold plans running concurrently was a
//! latent flake source on slow arm64 CI runners. Decodes are additionally
//! serialized through a shared `Mutex` so two jt9 processes never write
//! `jt9_wisdom.dat` into the shared data dir concurrently.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tuxlink_jt9::discover::{discover_jt9, Jt9Binary};
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SlotOutcome;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tuxlink-ft8/tests/fixtures/sdr").join(name)
}

/// Base dir for the shared e2e state. Pid-suffixed so concurrent `cargo
/// test` runs from different worktrees on this shared machine cannot
/// collide. `OnceLock` cannot run a destructor, so this directory is
/// intentionally leaked for the test process's lifetime (acceptable: it is
/// a handful of small files under the OS temp dir, not committed state).
fn shared_base() -> PathBuf {
    std::env::temp_dir().join(format!("tuxlink-jt9-e2e-shared-{}", std::process::id()))
}

/// (jt9 binary, shared warmed data dir, decode-serialization lock).
/// `None` means jt9 is not installed — every test-site short-circuits to a
/// skip, matching the pre-restructure behavior.
static SHARED: OnceLock<Option<(Jt9Binary, PathBuf, Mutex<()>)>> = OnceLock::new();

fn init_shared() -> Option<(Jt9Binary, PathBuf, Mutex<()>)> {
    let Ok(bin) = discover_jt9(None) else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — real_jt9 e2e suite");
        return None;
    };
    let base = shared_base();
    let data = base.join("data");
    std::fs::create_dir_all(&data).expect("create shared e2e data dir");
    // One-time cold FFTW plan against the shared data dir; every test below
    // reuses the warmed wisdom this leaves behind. jt9 being installed but
    // this failing is a real regression, not an absence — panic loudly
    // rather than silently skipping the whole suite.
    let warmup = Jt9Runner::new(bin.clone(), data.clone(), Duration::from_secs(12));
    warmup.prewarm().expect("one-time shared prewarm must succeed when jt9 is installed");
    Some((bin, data, Mutex::new(())))
}

fn shared() -> Option<(&'static Jt9Binary, &'static Path, &'static Mutex<()>)> {
    SHARED.get_or_init(init_shared).as_ref().map(|(bin, data, lock)| (bin, data.as_path(), lock))
}

/// Fresh per-test slot dir under the shared base; cleaned up by the caller.
fn slot_dir(tag: &str) -> PathBuf {
    let d = shared_base().join(format!("slot-{tag}"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ordinary_fixture_decodes_at_least_the_depth1_reference_set() {
    let Some((bin, data, lock)) = shared() else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — ordinary");
        return;
    };
    let runner = Jt9Runner::new(bin.clone(), data.to_path_buf(), Duration::from_secs(12));
    let slot = slot_dir("ordinary");
    let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
    match runner.decode_slot(&fixture("ft8-40m-ordinary-20260706T121215Z.wav"), &slot, 42) {
        SlotOutcome::Decoded(recs) => {
            // Depth-1 reference = 5 messages; -d 3 found 6 on 2.7.0. Floor at
            // the depth-1 count so a wsjtx-version delta cannot flake this.
            assert!(recs.len() >= 5, "got {} decodes", recs.len());
            assert!(recs.iter().all(|r| r.slot_utc_ms == 42));
            assert!(recs.iter().any(|r| r.message.contains("K5OJT")), "known strong signal missing");
            assert!(recs.iter().all(|r| !r.partial));
        }
        other => panic!("want Decoded, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&slot);
}

#[test]
fn quiet_fixture_decodes_both_reference_messages() {
    let Some((bin, data, lock)) = shared() else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — quiet");
        return;
    };
    let runner = Jt9Runner::new(bin.clone(), data.to_path_buf(), Duration::from_secs(12));
    let slot = slot_dir("quiet");
    let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
    match runner.decode_slot(&fixture("ft8-20m-quiet-20260706T121400Z.wav"), &slot, 0) {
        SlotOutcome::Decoded(recs) => assert!(recs.len() >= 2, "got {}", recs.len()),
        other => panic!("want Decoded, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&slot);
}

#[test]
fn prewarm_persists_fftw_wisdom_into_the_data_dir() {
    let Some((_bin, data, _lock)) = shared() else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — wisdom");
        return;
    };
    // The one-time shared-state prewarm (`init_shared`) already ran before
    // any test could reach here; this pins the artifact it must leave
    // behind (successful completion writes FFTW wisdom into the persistent
    // -a dir) without paying a second cold FFTW plan.
    assert!(data.join("jt9_wisdom.dat").exists(),
        "successful completion must write FFTW wisdom into the persistent -a dir");
}

#[test]
fn silence_is_band_dead() {
    let Some((bin, data, lock)) = shared() else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — silence");
        return;
    };
    let runner = Jt9Runner::new(bin.clone(), data.to_path_buf(), Duration::from_secs(12));
    let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
    // prewarm()'s silence decode returns BandDead through the same path; this
    // pins it as the public contract for a truly quiet slot.
    match runner.prewarm() {
        Ok(()) => {}
        Err(f) => panic!("silence must be clean BandDead/Decoded, got {f:?}"),
    }
}
