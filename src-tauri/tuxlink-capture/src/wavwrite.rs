//! Canonical slot-WAV writer (spec §WAV writeout).
//!
//! Emits the exact 44-byte-header RIFF/WAVE PCM16 mono 12 kHz layout that
//! `tuxlink_jt9::wav::preflight_slot_wav` validates — the round-trip is a
//! unit test in this module (dev-dependency). Exactly `OUT_SLOT_FRAMES`
//! frames; any other length errors with `ErrorKind::InvalidInput` BEFORE
//! creating the file.

use std::path::Path;

/// Frames per slot at the decimated output rate: 15.000 s × 12 kHz.
pub const OUT_SLOT_FRAMES: usize = 180_000;
/// Decimated output rate.
pub const OUT_RATE_HZ: u32 = 12_000;

pub fn write_slot_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    use std::io::Write;
    if samples.len() != OUT_SLOT_FRAMES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "slot WAV requires exactly {OUT_SLOT_FRAMES} frames, got {}",
                samples.len()
            ),
        ));
    }
    let data_len: u32 = (OUT_SLOT_FRAMES as u32) * 2;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&OUT_RATE_HZ.to_le_bytes())?;
    f.write_all(&(OUT_RATE_HZ * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    let mut pcm = Vec::with_capacity(data_len as usize);
    for s in samples {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    f.write_all(&pcm)?;
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("tuxlink-capture-wavwrite-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn ramp() -> Vec<i16> {
        (0..OUT_SLOT_FRAMES)
            .map(|i| (i as i32 % 32_768 - 16_384) as i16)
            .collect()
    }

    #[test]
    fn wrong_lengths_are_rejected_before_any_file_exists() {
        for n in [0usize, OUT_SLOT_FRAMES - 1, OUT_SLOT_FRAMES + 1] {
            let p = tmp(&format!("wrong-{n}.wav"));
            let err = write_slot_wav(&p, &vec![0i16; n]).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "len {n}");
            assert!(!p.exists(), "len {n}: no file may be created on rejection");
        }
    }

    #[test]
    fn header_fields_are_byte_exact_canonical() {
        let p = tmp("header.wav");
        write_slot_wav(&p, &ramp()).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert_eq!(b.len(), 44 + OUT_SLOT_FRAMES * 2, "total file size");
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 36 + 360_000);
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes([b[16], b[17], b[18], b[19]]), 16); // fmt size
        assert_eq!(u16::from_le_bytes([b[20], b[21]]), 1); // PCM
        assert_eq!(u16::from_le_bytes([b[22], b[23]]), 1); // mono
        assert_eq!(u32::from_le_bytes([b[24], b[25], b[26], b[27]]), 12_000); // rate
        assert_eq!(u32::from_le_bytes([b[28], b[29], b[30], b[31]]), 24_000); // byte rate
        assert_eq!(u16::from_le_bytes([b[32], b[33]]), 2); // block align
        assert_eq!(u16::from_le_bytes([b[34], b[35]]), 16); // bits
        assert_eq!(&b[36..40], b"data");
        assert_eq!(u32::from_le_bytes([b[40], b[41], b[42], b[43]]), 360_000);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn payload_round_trips_sample_exact() {
        let p = tmp("payload.wav");
        let samples = ramp();
        write_slot_wav(&p, &samples).unwrap();
        let b = std::fs::read(&p).unwrap();
        let got: Vec<i16> = b[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(got, samples);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn output_passes_the_l1_preflight_round_trip() {
        // THE contract test (spec §WAV): our writer's output must be
        // accepted by tuxlink_jt9::wav::preflight_slot_wav verbatim.
        let p = tmp("preflight.wav");
        write_slot_wav(&p, &ramp()).unwrap();
        assert_eq!(tuxlink_jt9::wav::preflight_slot_wav(&p), Ok(()));
        let _ = std::fs::remove_file(&p);
    }
}
