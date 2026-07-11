//! E2E (spec §Testing strategy): committed 12 kHz SDR fixture → ZOH ×4
//! upsample to 48 kHz → SampleSource-faked capture with SYNTHETIC time →
//! assembler → WAV → REAL jt9 decode; ≥ 90 % of the fixture's committed
//! reference decode count.
//!
//! Validity argument (spec, verbatim — NOT "band-limited by construction",
//! which is false for an SDR capture): ZOH images of ≤ 4 kHz content land
//! ≥ 8 kHz (FIR stopband, ≥ 60 dB); 4–6 kHz baseband content stays above
//! jt9's 4007 Hz ceiling; ZOH sinc droop at 4 kHz ≈ −0.10 dB — negligible.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::config::Ft8Config;
use crate::ft8::records::RingOutcome;
use crate::ft8::service::{Ft8Deps, Ft8ListenerState};
use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink, SourceStep};
use crate::ft8::traits::{Ft8Platform, Jt9Engine}; // trait in scope: p.wisdom_dir() below
use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SLOT_DECODE_TIMEOUT_SECS;

const SLOT_MS: u64 = 15_000;
const IN_SLOT_FRAMES: usize = tuxlink_capture::slot::IN_SLOT_FRAMES; // 720_000

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tuxlink-ft8/tests/fixtures/sdr")
}

/// Minimal canonical-WAV reader for the committed fixture (44-byte header,
/// PCM16 mono 12 kHz — the same shape wavwrite emits and preflight pins).
fn read_fixture_samples(name: &str) -> Vec<i16> {
    let bytes = std::fs::read(fixture_dir().join(name)).expect("committed fixture");
    assert_eq!(&bytes[0..4], b"RIFF", "fixture is canonical RIFF");
    let data = &bytes[44..];
    data.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// Reference decode count: the committed jt9 -d3 output's DECODE lines.
/// The `<DecodeFinished>` trailer is a sentinel, not a decode — counting it
/// would turn the 90 % floor into ceil(15×0.9)=14 of 14, i.e. a silent
/// 100 % floor that flakes on any marginal decode (T18 parent review).
fn reference_count(name: &str) -> usize {
    std::fs::read_to_string(fixture_dir().join(name))
        .expect("committed reference list")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('<'))
        .count()
}

/// ZOH ×4: each 12 kHz sample repeated 4×.
fn zoh_x4(samples_12k: &[i16]) -> Vec<i16> {
    let mut out = Vec::with_capacity(samples_12k.len() * 4);
    for &s in samples_12k {
        out.extend_from_slice(&[s, s, s, s]);
    }
    out
}

#[test]
fn e2e_fixture_through_capture_path_decodes_90_percent_of_reference() {
    // Gate exactly like L1's real_jt9.rs: skip loudly when jt9 is absent.
    let Ok(_bin) = tuxlink_jt9::discover::discover_jt9(None) else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — ft8 capture e2e");
        return;
    };

    let wav_12k = read_fixture_samples("ft8-40m-crowded-20260706T121300Z.wav");
    assert_eq!(wav_12k.len(), 180_000, "fixture is exactly one 15 s slot at 12 kHz");
    let reference = reference_count("ft8-40m-crowded-20260706T121300Z.jt9-d3-ap-off.txt");
    assert!(reference >= 10, "crowded fixture reference sanity");
    let want = (reference * 9).div_ceil(10); // ≥ 90 %

    let p = FakePlatform::happy();
    // REAL engine over the fake platform's tmp wisdom dir.
    let wisdom = p.wisdom_dir();
    std::fs::create_dir_all(&wisdom).unwrap();
    let bin = tuxlink_jt9::discover::discover_jt9(None).unwrap();
    let engine = Arc::new(Jt9Engine::new(Jt9Runner::new(
        bin,
        wisdom,
        Duration::from_secs(SLOT_DECODE_TIMEOUT_SECS),
    )));
    *p.engine.lock().unwrap() = engine;

    // happy()'s clock epoch (1_760_000_000_000 ms) does NOT start on a
    // boundary: mod 15_000 = 5_000, so it sits 5 s past one (10 s before
    // the next). Snap the ONE shared clock (platform + source both hold
    // it) back onto the boundary so the fixture audio is slot-aligned —
    // time is injected data (Step 1's helper).
    p.align_clock_to_slot_boundary();
    assert_eq!(p.clock.utc_ms() % SLOT_MS, 0);

    // Script: slot A = silence (absorbs whichever first-slot semantics the
    // Phase-A assembler pinned: full-silence BandDead or first-slot
    // discard), slot B = the upsampled fixture, then a tail of silence so
    // the B boundary definitely closes.
    let upsampled = zoh_x4(&wav_12k);
    assert_eq!(upsampled.len(), IN_SLOT_FRAMES);
    {
        let mut steps = p.source_steps.lock().unwrap();
        for _ in 0..(IN_SLOT_FRAMES / 4_800) {
            steps.push_back(SourceStep::Frames { frames: 4_800, value: 0, gap: None });
        }
        for chunk in upsampled.chunks(4_800) {
            steps.push_back(SourceStep::Samples { samples: chunk.to_vec(), gap: None });
        }
        for _ in 0..4 {
            steps.push_back(SourceStep::Frames { frames: 4_800, value: 0, gap: None });
        }
    }

    // Struct-literal init (not default-then-reassign) to avoid clippy's
    // field_reassign_with_default, which is denied under -D warnings.
    let cfg = Ft8Config {
        device: Some(StableAudioId {
            kind: StableIdKind::ByIdSymlink,
            value: "usb-DRA-100-00".into(),
        }),
        ..Ft8Config::default()
    };
    let state = Ft8ListenerState::new(
        Ft8Deps {
            platform: p.clone(),
            clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
            sink: Arc::new(RecordingSink::default()),
        },
        cfg,
    );
    state.test_run_sequence();

    // Wait for the fixture slot's Decoded record (real jt9: allow generous
    // wall time — prewarm + decode ≤ ~15 s on a cold arm64 runner).
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let decoded_count = loop {
        let found = {
            let snap = state.snapshot();
            snap.ring_tail.iter().find_map(|r| match &r.outcome {
                RingOutcome::Decoded => Some(r.decodes.len()),
                _ => None,
            })
        };
        if let Some(n) = found {
            break n;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no Decoded record within the deadline; ring tail: {:?}",
            state
                .snapshot()
                .ring_tail
                .iter()
                .map(|r| r.outcome.clone())
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        decoded_count >= want,
        "decoded {decoded_count} < 90 % of reference ({want} of {reference}) — the \
         ZOH→FIR→assembler round trip lost decodes"
    );
    state.test_teardown();
}
