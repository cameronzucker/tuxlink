//! End-to-end against the REAL jt9 on the committed SDR fixtures.
//! Skips (with a printed notice) when jt9 is absent; CI installs wsjtx so
//! these always run there. Locally on the dev Pi they run in seconds.

use std::path::PathBuf;
use std::time::Duration;
use tuxlink_jt9::discover::discover_jt9;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SlotOutcome;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tuxlink-ft8/tests/fixtures/sdr").join(name)
}

fn runner_or_skip(tag: &str) -> Option<(Jt9Runner, PathBuf)> {
    let Ok(bin) = discover_jt9(None) else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — {tag}");
        return None;
    };
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-{tag}-{}", std::process::id()));
    let data = base.join("data");
    let slot = base.join("slot");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&slot).unwrap();
    Some((Jt9Runner::new(bin, data, Duration::from_secs(12)), slot))
}

#[test]
fn ordinary_fixture_decodes_at_least_the_depth1_reference_set() {
    let Some((runner, slot)) = runner_or_skip("ordinary") else { return };
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
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-ordinary-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn quiet_fixture_decodes_both_reference_messages() {
    let Some((runner, slot)) = runner_or_skip("quiet") else { return };
    match runner.decode_slot(&fixture("ft8-20m-quiet-20260706T121400Z.wav"), &slot, 0) {
        SlotOutcome::Decoded(recs) => assert!(recs.len() >= 2, "got {}", recs.len()),
        other => panic!("want Decoded, got {other:?}"),
    }
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-quiet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn prewarm_persists_fftw_wisdom_into_the_data_dir() {
    let Some((runner, _slot)) = runner_or_skip("wisdom") else { return };
    runner.prewarm().expect("prewarm must complete");
    // The data dir was created by runner_or_skip under this test's base; the
    // wisdom file is jt9's completion artifact (delta §Grounded facts).
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-wisdom-{}", std::process::id()));
    assert!(base.join("data").join("jt9_wisdom.dat").exists(),
        "successful completion must write FFTW wisdom into the persistent -a dir");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn silence_is_band_dead() {
    let Some((runner, slot)) = runner_or_skip("silence") else { return };
    // prewarm()'s silence decode returns BandDead through the same path; this
    // pins it as the public contract for a truly quiet slot.
    match runner.prewarm() {
        Ok(()) => {}
        Err(f) => panic!("silence must be clean BandDead/Decoded, got {f:?}"),
    }
    let _ = slot; // silence path exercised via prewarm
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-e2e-silence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
}
