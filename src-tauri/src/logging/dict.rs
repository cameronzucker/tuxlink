//! Bundled zstd dictionary loader with validation (spec §7.5).

use once_cell::sync::OnceCell;

/// v1 dictionary bytes embedded at build time. Filename changes per training
/// version; the constant name stays.
const EVENT_DICT_V1: &[u8] = include_bytes!("../../assets/logging/tuxlink-events-v1.zdict");

pub const DICT_VERSION: u32 = 1;

/// Zstd trained-dictionary magic number.
/// Per the zstd format specification (RFC 8878, §4.1), the first 4 bytes of
/// every valid zstd dictionary are the magic number `0xEC30A437` stored
/// little-endian on disk, i.e., `[0x37, 0xA4, 0x30, 0xEC]`. Raw bytes that
/// lack this prefix are not zstd dictionaries; accepting them silently causes
/// compressed output that decompresses with the wrong dictionary and produces
/// corrupted data.
const ZSTD_DICT_MAGIC: [u8; 4] = [0x37, 0xA4, 0x30, 0xEC];

static VALIDATED: OnceCell<Result<&'static [u8], DictError>> = OnceCell::new();

#[derive(Debug, thiserror::Error, Clone)]
pub enum DictError {
    #[error("dictionary asset is empty (build configuration error)")]
    Empty,
    #[error("dictionary failed zstd validation: {0}")]
    Invalid(String),
}

/// Validate that `bytes` starts with the zstd dictionary magic number and is
/// non-empty. This is the first-line check; the roundtrip in `load_validated`
/// is defense-in-depth.
///
/// Extracted as a separate helper so unit tests can exercise the magic-byte
/// check with arbitrary inputs (EVENT_DICT_V1 is a const baked at compile
/// time and cannot be swapped in place tests).
pub fn validate_dict_bytes(bytes: &[u8]) -> Result<(), DictError> {
    if bytes.is_empty() {
        return Err(DictError::Empty);
    }
    if bytes.len() < 4 || bytes[..4] != ZSTD_DICT_MAGIC {
        return Err(DictError::Invalid(
            "missing zstd dictionary magic bytes (expected 0xEC 0x30 0xA4 0x37)".into(),
        ));
    }
    Ok(())
}

/// Validate the bundled dictionary once and cache the result.
///
/// Per plan-adrev v2 §1 Finding "Dictionary validation is claimed but not
/// actually possible via this call": `zstd::dict::DecoderDictionary::copy`
/// does NOT return a `Result` — it cannot signal "the bytes are not a valid
/// zstd dictionary." Real validation uses a known-input compress + decompress
/// roundtrip; if either step errors, the dictionary is treated as invalid
/// and callers fall back to dictionary-free compression (spec §7.5).
///
/// Codex P2 #7: magic-byte check is now the FIRST guard before the roundtrip,
/// to prevent corrupt non-dict bytes from round-tripping as "valid."
pub fn load_validated() -> Result<&'static [u8], DictError> {
    use std::io::{Read, Write};
    VALIDATED
        .get_or_init(|| {
            // Magic-byte check: reject anything that doesn't start with the
            // zstd dictionary magic. This prevents garbage bytes from passing
            // the roundtrip by accident (zstd's Encoder::with_dictionary is
            // permissive about the bytes it accepts; the roundtrip alone is
            // not sufficient because any bytes may happen to roundtrip).
            validate_dict_bytes(EVENT_DICT_V1)?;

            const PROBE: &[u8] = b"tuxlink-dict-validation-probe-2026";
            let compressed = (|| -> Result<Vec<u8>, std::io::Error> {
                let mut e = zstd::stream::Encoder::with_dictionary(Vec::new(), 1, EVENT_DICT_V1)?;
                e.write_all(PROBE)?;
                e.finish()
            })()
            .map_err(|e| DictError::Invalid(format!("compress: {e}")))?;

            let decompressed = (|| -> Result<Vec<u8>, std::io::Error> {
                let mut d = zstd::stream::Decoder::with_dictionary(compressed.as_slice(), EVENT_DICT_V1)?;
                let mut out = Vec::new();
                d.read_to_end(&mut out)?;
                Ok(out)
            })()
            .map_err(|e| DictError::Invalid(format!("decompress: {e}")))?;

            if decompressed != PROBE {
                return Err(DictError::Invalid("roundtrip mismatch".into()));
            }
            Ok(EVENT_DICT_V1)
        })
        .clone()
}

/// Returns the embedded dictionary bytes for embedding INTO the archive as
/// `dict.zdict`. Returns `None` when the dictionary failed validation; the
/// archive omits the `dict.zdict` member in that case.
pub fn for_archive() -> Option<&'static [u8]> {
    load_validated().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_validates_on_load() {
        assert!(load_validated().is_ok());
    }

    #[test]
    fn dict_is_nontrivial() {
        let d = load_validated().expect("dict should validate");
        assert!(d.len() > 1024, "dict should be larger than 1 KB; got {}", d.len());
        assert!(d.len() < 64 * 1024, "dict should be smaller than 64 KB; got {}", d.len());
    }

    /// Codex P2 #7: corrupt / non-magic bytes MUST be rejected by the magic-byte
    /// check before reaching the roundtrip. `validate_dict_bytes` is the helper
    /// testable with arbitrary bytes (EVENT_DICT_V1 is const and cannot be swapped).
    #[test]
    fn corrupt_bytes_fail_magic_check() {
        // All-0xFF bytes: not a zstd dictionary.
        let bad: &[u8] = &[0xFF; 128];
        let result = validate_dict_bytes(bad);
        assert!(
            matches!(result, Err(DictError::Invalid(_))),
            "random non-magic bytes must return DictError::Invalid; got {result:?}"
        );
    }

    #[test]
    fn empty_bytes_fail_with_empty_error() {
        let result = validate_dict_bytes(&[]);
        assert!(
            matches!(result, Err(DictError::Empty)),
            "empty slice must return DictError::Empty; got {result:?}"
        );
    }

    #[test]
    fn wrong_magic_bytes_fail_validation() {
        // Use correct length but wrong magic: first 4 bytes = 0x00 0x01 0x02 0x03
        let bad: Vec<u8> = (0u8..128).collect();
        let result = validate_dict_bytes(&bad);
        assert!(
            matches!(result, Err(DictError::Invalid(_))),
            "bytes without zstd magic must fail; got {result:?}"
        );
    }

    #[test]
    fn real_dict_passes_magic_check() {
        // The real EVENT_DICT_V1 must pass the magic-byte check (it's a
        // legitimate trained zstd dictionary).
        let result = validate_dict_bytes(EVENT_DICT_V1);
        assert!(result.is_ok(), "real dict must pass magic check: {result:?}");
    }
}
