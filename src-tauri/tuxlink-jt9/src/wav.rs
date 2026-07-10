//! Host-side slot-WAV validation. jt9 cannot be trusted to reject bad input:
//! it segfaults on missing/corrupt files, ignores the sample-rate header, and
//! silently under-decodes truncated audio (delta §Grounded facts). Contract:
//! canonical RIFF/WAVE, PCM (format 1), mono, 16-bit, 12000 Hz, exactly
//! 180_000 frames (15.000 s).

use std::io::Read;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum WavError {
    NotFound,
    Permission,
    Malformed(String),
    WrongFormat(String),
}

pub const SLOT_FRAMES: u32 = 180_000;
pub const SLOT_RATE_HZ: u32 = 12_000;

pub fn preflight_slot_wav(path: &Path) -> Result<(), WavError> {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(WavError::NotFound),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(WavError::Permission),
        Err(e) => return Err(WavError::Malformed(e.to_string())),
    };
    let mut hdr = [0u8; 44];
    f.read_exact(&mut hdr).map_err(|_| WavError::Malformed("short header".into()))?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..16] != b"WAVEfmt " {
        return Err(WavError::Malformed("not RIFF/WAVE".into()));
    }
    let fmt = u16::from_le_bytes([hdr[20], hdr[21]]);
    let channels = u16::from_le_bytes([hdr[22], hdr[23]]);
    let rate = u32::from_le_bytes([hdr[24], hdr[25], hdr[26], hdr[27]]);
    let bits = u16::from_le_bytes([hdr[34], hdr[35]]);
    let data_len = u32::from_le_bytes([hdr[40], hdr[41], hdr[42], hdr[43]]);
    if &hdr[36..40] != b"data" {
        return Err(WavError::Malformed("no canonical data chunk at offset 36".into()));
    }
    let want = format!("PCM mono 16-bit {SLOT_RATE_HZ} Hz, {SLOT_FRAMES} frames");
    if fmt != 1 || channels != 1 || bits != 16 || rate != SLOT_RATE_HZ {
        return Err(WavError::WrongFormat(format!(
            "got fmt={fmt} ch={channels} bits={bits} rate={rate}; want {want}"
        )));
    }
    if data_len != SLOT_FRAMES * 2 {
        return Err(WavError::WrongFormat(format!(
            "got {} data bytes ({} frames); want {want}", data_len, data_len / 2
        )));
    }
    // Header can lie about truncated-on-disk files — verify actual size.
    let actual = std::fs::metadata(path)
        .map_err(|e| WavError::Malformed(e.to_string()))?
        .len();
    if actual != 44 + u64::from(data_len) {
        return Err(WavError::WrongFormat(format!(
            "file is {actual} bytes; header promises {}", 44 + u64::from(data_len)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Minimal canonical WAV writer for tests: 44-byte header + PCM.
    fn write_wav(path: &Path, rate: u32, channels: u16, bits: u16, frames: u32) {
        let mut f = std::fs::File::create(path).unwrap();
        let block_align = channels * (bits / 8);
        let data_len = frames * block_align as u32;
        let byte_rate = rate * block_align as u32;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&byte_rate.to_le_bytes()).unwrap();
        f.write_all(&block_align.to_le_bytes()).unwrap();
        f.write_all(&bits.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        f.write_all(&vec![0u8; data_len as usize]).unwrap();
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join("tuxlink-jt9-wavtest");
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn accepts_canonical_slot_wav() {
        let p = tmp("good.wav");
        write_wav(&p, 12_000, 1, 16, 180_000);
        assert_eq!(preflight_slot_wav(&p), Ok(()));
    }

    #[test]
    fn rejects_missing_file() {
        assert_eq!(preflight_slot_wav(Path::new("/nonexistent/slot.wav")), Err(WavError::NotFound));
    }

    #[test]
    fn rejects_wrong_rate_channels_bits_and_length() {
        let cases: [(&str, u32, u16, u16, u32); 4] = [
            ("rate48k.wav", 48_000, 1, 16, 180_000),
            ("stereo.wav", 12_000, 2, 16, 180_000),
            ("bits8.wav", 12_000, 1, 8, 180_000),
            ("short.wav", 12_000, 1, 16, 24_000),
        ];
        for (name, rate, ch, bits, frames) in cases {
            let p = tmp(name);
            write_wav(&p, rate, ch, bits, frames);
            assert!(matches!(preflight_slot_wav(&p), Err(WavError::WrongFormat(_))), "{name}");
        }
    }

    #[test]
    fn rejects_garbage_and_truncated_header() {
        let p = tmp("garbage.wav");
        std::fs::write(&p, b"not a wav at all").unwrap();
        assert!(matches!(preflight_slot_wav(&p), Err(WavError::Malformed(_))));
        let p = tmp("tiny.wav");
        std::fs::write(&p, b"RIFF").unwrap();
        assert!(matches!(preflight_slot_wav(&p), Err(WavError::Malformed(_))));
    }

    #[test]
    fn rejects_truncated_data_with_intact_header() {
        // The capture-bug class jt9 itself cannot catch: header claims
        // 180,000 frames, file was truncated on disk. Preflight must compare
        // actual size to the header's data_len.
        let p = tmp("lying-header.wav");
        write_wav(&p, 12_000, 1, 16, 180_000);
        let full = std::fs::read(&p).unwrap();
        std::fs::write(&p, &full[..full.len() / 2]).unwrap();
        assert!(matches!(preflight_slot_wav(&p), Err(WavError::WrongFormat(_))));
    }

    #[test]
    fn unreadable_file_is_permission() {
        use std::os::unix::fs::PermissionsExt;
        let p = tmp("noperm.wav");
        write_wav(&p, 12_000, 1, 16, 180_000);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = preflight_slot_wav(&p);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(r, Err(WavError::Permission));
    }

    #[test]
    fn committed_sdr_fixtures_are_canonical_slot_wavs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tuxlink-ft8/tests/fixtures/sdr");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().is_some_and(|e| e == "wav") {
                assert_eq!(preflight_slot_wav(&p), Ok(()), "fixture {p:?}");
                checked += 1;
            }
        }
        assert!(checked >= 4, "expected the 4 committed SDR fixtures, found {checked}");
    }
}
