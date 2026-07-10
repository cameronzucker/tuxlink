//! Runner lifecycle tests against controllable fake jt9 shell scripts.
//! The REAL jt9 is exercised in tests/real_jt9.rs (Task 6).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tuxlink_jt9::discover::Jt9Binary;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::{SlotFailure, SlotOutcome};

const DECODE_LINE: &str = "000000 -14 -0.6 2093 ~  YB3BBF K5OJT -19";
const SENTINEL: &str = "<DecodeFinished>   0   1        0";

fn setup(name: &str, script: &str) -> (Jt9Runner, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-rt-{}-{}", name, std::process::id()));
    let bin_dir = base.join("bin");
    let data = base.join("data");
    let slot_tmp = base.join("slot");
    for d in [&bin_dir, &data, &slot_tmp] { std::fs::create_dir_all(d).unwrap(); }
    let fake = bin_dir.join("jt9");
    std::fs::write(&fake, script).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let runner = Jt9Runner::new(
        Jt9Binary { jt9_path: fake, engine_version: "fake".into() },
        data,
        Duration::from_secs(2), // short deadline for tests
    );
    let wav = base.join("slot.wav");
    write_canonical_wav(&wav);
    (runner, wav, slot_tmp)
}

/// Canonical 180,000-frame silence WAV (passes preflight).
fn write_canonical_wav(path: &Path) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    let data_len: u32 = 180_000 * 2;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&12_000u32.to_le_bytes()).unwrap();
    f.write_all(&24_000u32.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&vec![0u8; data_len as usize]).unwrap();
}

#[test]
fn happy_path_decodes_and_stamps_slot_utc() {
    let (runner, wav, tmp) = setup("happy", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho '{SENTINEL}'\nexit 0\n"));
    match runner.decode_slot(&wav, &tmp, 1_752_000_000_000) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert_eq!(recs[0].slot_utc_ms, 1_752_000_000_000);
            assert_eq!(recs[0].from_call.as_deref(), Some("K5OJT"));
            assert!(!recs[0].partial);
        }
        other => panic!("want Decoded, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn clean_zero_decode_is_band_dead_not_failure() {
    let (runner, wav, tmp) = setup("dead", &format!(
        "#!/bin/sh\necho '{SENTINEL}'\nexit 0\n"));
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::BandDead);
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn timeout_salvages_partial_decodes() {
    // Emits one decode, then hangs past the 2s deadline. Salvage keeps it.
    // `exec` so the sleep IS the killed process (no orphan grandchild).
    let (runner, wav, tmp) = setup("salvage", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\nexec sleep 30\n"));
    let t0 = std::time::Instant::now();
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert!(recs[0].partial, "salvaged records must be flagged partial");
        }
        other => panic!("want salvaged Decoded, got {other:?}"),
    }
    assert!(t0.elapsed() < Duration::from_secs(10), "kill must be prompt, no 30s wait");
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn timeout_with_no_output_is_timeout_failure() {
    let (runner, wav, tmp) = setup("hang", "#!/bin/sh\nexec sleep 30\n");
    let t0 = std::time::Instant::now();
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::Timeout));
    assert!(t0.elapsed() < Duration::from_secs(10));
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn signal_death_is_classified_with_stderr_tail() {
    // Reproduces jt9's real mode: stderr diagnostics then SIGSEGV.
    let (runner, wav, tmp) = setup("segv",
        "#!/bin/sh\necho 'Fortran runtime error: End of file simulation' 1>&2\nkill -SEGV $$\n");
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Failed(SlotFailure::Signal { signal, stderr_tail }) => {
            assert!(signal.contains("11") || signal.to_uppercase().contains("SEGV"), "{signal}");
            assert!(stderr_tail.contains("Fortran runtime error"));
        }
        other => panic!("want Signal, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn stderr_eof_on_clean_exit_is_a_capture_bug_not_band_dead() {
    let (runner, wav, tmp) = setup("eof", &format!(
        "#!/bin/sh\necho 'EOF on input file' 1>&2\necho '{SENTINEL}'\nexit 0\n"));
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::StderrEof));
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn bad_wav_never_spawns() {
    let (runner, _wav, tmp) = setup("badwav", "#!/bin/sh\ntouch spawned-marker\nexit 0\n");
    let missing = std::env::temp_dir().join("no-such-slot.wav");
    match runner.decode_slot(&missing, &tmp, 0) {
        SlotOutcome::Failed(SlotFailure::BadWav(_)) => {}
        other => panic!("want BadWav, got {other:?}"),
    }
    assert!(!tmp.join("spawned-marker").exists(), "preflight must gate the spawn");
    let _ = std::fs::remove_dir_all(tmp.parent().unwrap());
}

#[test]
fn hung_grandchild_holding_pipes_does_not_block_the_kill_path() {
    // A forked grandchild inherits the pipe write-ends; the runner must not
    // join the drain threads on the timeout path or this hangs 30s.
    let (runner, wav, tmp) = setup("grandchild", "#!/bin/sh\n( sleep 30 ) &\nsleep 30\n");
    let t0 = std::time::Instant::now();
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::Timeout));
    assert!(t0.elapsed() < Duration::from_secs(10), "must not block on grandchild pipes");
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn nonexistent_binary_is_spawn_failed() {
    let base = std::env::temp_dir().join(format!("tuxlink-jt9-rt-nospawn-{}", std::process::id()));
    let slot_tmp = base.join("slot");
    std::fs::create_dir_all(&slot_tmp).unwrap();
    let wav = base.join("slot.wav");
    write_canonical_wav(&wav);
    let runner = Jt9Runner::new(
        Jt9Binary { jt9_path: base.join("no-such-jt9"), engine_version: "fake".into() },
        base.join("data"),
        Duration::from_secs(2),
    );
    assert!(matches!(
        runner.decode_slot(&wav, &slot_tmp, 0),
        SlotOutcome::Failed(SlotFailure::SpawnFailed(_))
    ));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn garbage_output_without_decodes_is_parse_error() {
    let (runner, wav, tmp) = setup("garbage",
        "#!/bin/sh\necho 'random noise line'\necho 'more junk'\nexit 0\n");
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::Failed(SlotFailure::ParseError));
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn arg_builder_never_emits_shmem() {
    // Guard the GPL boundary at the unit level: the fake script fails loudly
    // if it ever sees -s/--shmem.
    let (runner, wav, tmp) = setup("noshm", &format!(
        "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in -s|--shmem) exit 97;; esac; done\necho '{SENTINEL}'\nexit 0\n"));
    assert_eq!(runner.decode_slot(&wav, &tmp, 0), SlotOutcome::BandDead);
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}
