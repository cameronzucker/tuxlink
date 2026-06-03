//! Catalog file parser. Format (from WLE's `Winlink Queries.txt`):
//!
//! ```text
//! CATEGORY|FILENAME|DESCRIPTION|SIZE
//! ```
//!
//! UTF-8 with a leading BOM in WLE's bundled file. One entry per line.
//! 1477 entries / 127 categories in the empirical sample. The bundled file
//! is included at compile time via `include_str!` and exposed as
//! `BUNDLED_CATALOG`.

use serde::Serialize;

/// Bundled WLE catalog file as a static string. The Tauri command surfaces
/// this through `parse_catalog`; the file has not been edited from WLE's
/// shipped content (the catalog is data, not code).
pub const BUNDLED_CATALOG: &str = include_str!("../../resources/catalog/winlink-queries.txt");

/// A single inquiry entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogEntry {
    /// Tree-group category, e.g. `WL2K_RMS`, `PROPAGATION`, `WX_BUOY`.
    pub category: String,
    /// The inquiry filename — this is the literal string the operator's
    /// catalog request body needs to contain. Sample: `PUB_PACKET`.
    pub filename: String,
    /// Operator-facing one-line description.
    pub description: String,
    /// Approximate response size in bytes (informational, from the WLE
    /// shipped file; not authoritative on a given CMS).
    pub size_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogParseError {
    #[error("line {line}: expected 4 pipe-delimited fields, got {got}")]
    WrongFieldCount { line: usize, got: usize },
    #[error("line {line}: size field {raw:?} is not a non-negative integer")]
    BadSize { line: usize, raw: String },
}

/// Parse the catalog text into `Vec<CatalogEntry>`. Strips a leading UTF-8
/// BOM if present (WLE's bundled file has one). Blank lines are skipped.
/// Returns `Err` on the first malformed line so a bad bundled file fails
/// loudly at startup.
pub fn parse_catalog(text: &str) -> Result<Vec<CatalogEntry>, CatalogParseError> {
    // Strip UTF-8 BOM (U+FEFF). Three-byte EF BB BF.
    let stripped = text.strip_prefix('\u{FEFF}').unwrap_or(text);

    let mut out = Vec::new();
    for (idx, raw_line) in stripped.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        // SIZE is always numeric and trailing — parse from the rightmost
        // '|' so descriptions that contain literal pipes survive intact.
        // (Defensive: I haven't seen pipe-in-description in the bundled
        // file, but the format permits it and rsplit costs nothing.)
        let (head, size_str) = line.rsplit_once('|').ok_or(CatalogParseError::WrongFieldCount {
            line: idx + 1,
            got: 1,
        })?;
        let size_bytes: u64 = size_str.parse().map_err(|_| CatalogParseError::BadSize {
            line: idx + 1,
            raw: size_str.to_string(),
        })?;
        // Remaining: CATEGORY|FILENAME|DESCRIPTION. Description is the
        // tail (may itself contain '|'), so splitn(3) keeps it as a single
        // string.
        let head_parts: Vec<&str> = head.splitn(3, '|').collect();
        if head_parts.len() != 3 {
            return Err(CatalogParseError::WrongFieldCount {
                line: idx + 1,
                got: head_parts.len() + 1,
            });
        }
        out.push(CatalogEntry {
            category: head_parts[0].to_string(),
            filename: head_parts[1].to_string(),
            description: head_parts[2].to_string(),
            size_bytes,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handful of literal lines copied verbatim from
    /// `dev/scratch/winlink-re/install/.../Winlink Queries.txt`.
    const FIXTURE: &str = "\u{FEFF}ARCTIC_ICE|FICN10CWIS|Iceberg Canada East Coast Waters|769\n\
                           WL2K_RMS|PUB_PACKET|Packet Public Gateways Frequency List|219867\n\
                           WL2K_USERS|CMS_STATUS|Real time Operational Status of Winlink CMS's|2018\n\
                           S/PACIFIC_WX|CALEDONIA_1|South Pacific High Sea Report (New Caledonia included)|2565\n";

    #[test]
    fn parses_fixture_lines() {
        let entries = parse_catalog(FIXTURE).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].category, "ARCTIC_ICE");
        assert_eq!(entries[0].filename, "FICN10CWIS");
        assert_eq!(entries[0].size_bytes, 769);
        // RMS list filename — verified in N7CPZ's outbox fixture
        assert_eq!(entries[1].category, "WL2K_RMS");
        assert_eq!(entries[1].filename, "PUB_PACKET");
        // Category names with `/` are preserved literally.
        assert_eq!(entries[3].category, "S/PACIFIC_WX");
    }

    #[test]
    fn strips_utf8_bom() {
        // Single entry, with BOM
        let with_bom = "\u{FEFF}CAT|FILE|desc|42\n";
        let entries = parse_catalog(with_bom).unwrap();
        assert_eq!(entries[0].category, "CAT");
        // Without BOM still works
        let no_bom = "CAT|FILE|desc|42\n";
        let entries = parse_catalog(no_bom).unwrap();
        assert_eq!(entries[0].category, "CAT");
    }

    #[test]
    fn handles_crlf_line_endings() {
        // WLE's file may have CRLF; tolerate it.
        let crlf = "CAT|FILE|desc|42\r\nOTHER|X|y|0\r\n";
        let entries = parse_catalog(crlf).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].category, "CAT");
    }

    #[test]
    fn skips_blank_lines() {
        let with_blanks = "CAT|FILE|desc|42\n\nOTHER|X|y|0\n\n";
        let entries = parse_catalog(with_blanks).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn description_may_contain_pipes_or_special_chars() {
        // splitn(4) keeps a 4th-field description-with-pipes intact.
        let exotic = "WX_FAX|HFGULF.TXT|New Orleans Gulf | WEFAX schedule|4473\n";
        let entries = parse_catalog(exotic).unwrap();
        assert_eq!(entries[0].description, "New Orleans Gulf | WEFAX schedule");
        assert_eq!(entries[0].size_bytes, 4473);
    }

    #[test]
    fn missing_size_field_fails_loudly() {
        // Since SIZE is parsed from the rightmost '|' as numeric, a 3-field
        // line where the tail isn't numeric fails as BadSize rather than
        // WrongFieldCount — still loud, just a different variant.
        let bad = "ONLY|THREE|FIELDS\n";
        let err = parse_catalog(bad).unwrap_err();
        assert!(matches!(err, CatalogParseError::BadSize { line: 1, .. }));
    }

    #[test]
    fn missing_all_separators_fails_with_wrong_field_count() {
        // A line with no '|' at all fails at the initial rsplit.
        let bad = "no-separators-here\n";
        let err = parse_catalog(bad).unwrap_err();
        assert!(matches!(err, CatalogParseError::WrongFieldCount { line: 1, got: 1 }));
    }

    #[test]
    fn too_few_separators_with_numeric_tail_fails_loudly() {
        // 'A|123' — rsplit peels off 123 as size, leaves 'A' which can't
        // be split into 3 head fields → WrongFieldCount.
        let bad = "A|123\n";
        let err = parse_catalog(bad).unwrap_err();
        assert!(matches!(err, CatalogParseError::WrongFieldCount { line: 1, got: 2 }));
    }

    #[test]
    fn non_numeric_size_fails_loudly() {
        let bad = "CAT|FILE|desc|not-a-number\n";
        let err = parse_catalog(bad).unwrap_err();
        assert!(matches!(err, CatalogParseError::BadSize { line: 1, .. }));
    }

    #[test]
    fn bundled_catalog_parses_completely() {
        // Smoke against the actual bundled file: 1477 entries per the WLE
        // sample. Allow a small drift if the file is replaced/updated later
        // (catalog updates are operator-triggered; this just ensures the
        // bundled file is parseable end-to-end).
        let entries = parse_catalog(BUNDLED_CATALOG).expect("bundled catalog parses");
        assert!(entries.len() > 1000, "bundled catalog has fewer than 1000 entries: {}", entries.len());
        // Sanity: a known entry from the WLE fixture is present.
        assert!(
            entries.iter().any(|e| e.category == "WL2K_RMS" && e.filename == "PUB_PACKET"),
            "bundled catalog missing WL2K_RMS/PUB_PACKET"
        );
        assert!(
            entries.iter().any(|e| e.category == "WL2K_USERS" && e.filename == "CMS_STATUS"),
            "bundled catalog missing WL2K_USERS/CMS_STATUS"
        );
    }
}
