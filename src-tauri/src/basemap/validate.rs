//! PMTiles header + schema validation (tuxlink-ndi4, plan A4/A10).
//!
//! Validates that a file is a PMTiles-spec-v3 archive carrying the Protomaps
//! basemaps vector schema this app renders, BEFORE it is served (bundled) or
//! committed to the packs dir (downloaded, phase 4 — pre-rename). The checks:
//!
//! - magic `"PMTiles"` + spec version byte `0x03` (reject v1/v2);
//! - header-declared directory / metadata / tile-data extents all lie within the
//!   file (catches truncated downloads);
//! - `tile_type == 1` (MVT) — a raster PMTiles is the wrong artifact for a vector
//!   basemap;
//! - the metadata `vector_layers` set is a SUPERSET of the 13 ids the Protomaps
//!   schema this app's styles target use (plan A10 — pinned against the real
//!   pack's metadata, not hand-maintained prose);
//! - a size budget (defence against an over-large download).
//!
//! The 13-id set + schema-version key were extracted from the real Protomaps
//! planet build (`version: "3.7.1"`, `planetiler:version` present). Phase 4 pins
//! ONE planet-build hash for the bundle AND every catalog pack so schemas can't
//! diverge (plan A10); this validator is the runtime gate that enforces the id set.

use std::io::Read;

use super::PmtilesArchive;

/// PMTiles v3 fixed header length in bytes.
const HEADER_LEN: usize = 127;

/// The PMTiles spec version this app supports.
const SUPPORTED_SPEC_VERSION: u8 = 3;

/// PMTiles `tile_type` for Mapbox Vector Tiles. A vector basemap MUST be MVT.
const TILE_TYPE_MVT: u8 = 1;

/// PMTiles `internal_compression` codes for the directory + metadata blocks.
const COMPRESSION_NONE: u8 = 1;
const COMPRESSION_GZIP: u8 = 2;

/// The Protomaps basemaps vector-layer ids this app's light/dark styles render.
/// Locked against the real planet-build metadata (plan A10); a pack missing any
/// of these would leave style layers unpainted, so it is rejected.
pub const REQUIRED_LAYER_IDS: [&str; 13] = [
    "boundaries",
    "buildings",
    "earth",
    "landcover",
    "landuse",
    "natural",
    "physical_line",
    "physical_point",
    "places",
    "pois",
    "roads",
    "transit",
    "water",
];

/// Facts extracted from a validated archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PmtilesValidation {
    /// The Protomaps schema version (metadata `version`, e.g. `"3.7.1"`).
    pub schema_version: String,
    /// The planetiler version that produced the pack, if recorded.
    pub planetiler_version: Option<String>,
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// The archive's total length in bytes.
    pub len: u64,
    /// All vector-layer ids present (a superset of [`REQUIRED_LAYER_IDS`]).
    pub layer_ids: Vec<String>,
}

/// Why an archive was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Shorter than the fixed PMTiles header, or a declared extent runs past EOF.
    Truncated,
    /// Magic bytes were not `"PMTiles"`.
    BadMagic,
    /// Spec version byte was not `0x03` (e.g. a v1/v2 archive).
    UnsupportedVersion(u8),
    /// `tile_type` was not MVT — a raster PMTiles, wrong for a vector basemap.
    NotVectorTiles(u8),
    /// Metadata could not be read / decompressed / parsed as the expected JSON.
    MetadataDecode,
    /// Metadata was valid JSON but missing one or more required vector-layer ids.
    MissingLayers(Vec<String>),
    /// The archive exceeds the configured size budget.
    SizeBudgetExceeded { actual: u64, budget: u64 },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Truncated => write!(f, "PMTiles archive is truncated or too short"),
            ValidationError::BadMagic => write!(f, "not a PMTiles archive (bad magic)"),
            ValidationError::UnsupportedVersion(v) => {
                write!(f, "unsupported PMTiles spec version {v} (expected 3)")
            }
            ValidationError::NotVectorTiles(t) => {
                write!(f, "archive tile_type {t} is not MVT (vector) tiles")
            }
            ValidationError::MetadataDecode => write!(f, "PMTiles metadata could not be decoded"),
            ValidationError::MissingLayers(ids) => {
                write!(f, "PMTiles metadata missing required layers: {}", ids.join(", "))
            }
            ValidationError::SizeBudgetExceeded { actual, budget } => {
                write!(f, "PMTiles archive is {actual} bytes, over the {budget}-byte budget")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

fn read_u64_le(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buf[offset..offset + 8].try_into().expect("8-byte slice"))
}

/// True when `offset + length` overflows or runs past `file_len`.
fn extent_out_of_bounds(offset: u64, length: u64, file_len: u64) -> bool {
    match offset.checked_add(length) {
        Some(end) => end > file_len,
        None => true,
    }
}

/// Validate `archive` as a renderable Protomaps vector basemap within `max_bytes`.
pub fn validate(archive: &PmtilesArchive, max_bytes: u64) -> Result<PmtilesValidation, ValidationError> {
    let file_len = archive.len();
    if (file_len as usize) < HEADER_LEN {
        return Err(ValidationError::Truncated);
    }

    let header = archive
        .read_at(0, HEADER_LEN)
        .map_err(|_| ValidationError::Truncated)?;
    if header.len() < HEADER_LEN {
        return Err(ValidationError::Truncated);
    }

    if &header[0..7] != b"PMTiles" {
        return Err(ValidationError::BadMagic);
    }
    let version = header[7];
    if version != SUPPORTED_SPEC_VERSION {
        return Err(ValidationError::UnsupportedVersion(version));
    }

    let root_dir_offset = read_u64_le(&header, 8);
    let root_dir_length = read_u64_le(&header, 16);
    let metadata_offset = read_u64_le(&header, 24);
    let metadata_length = read_u64_le(&header, 32);
    let leaf_dirs_offset = read_u64_le(&header, 40);
    let leaf_dirs_length = read_u64_le(&header, 48);
    let tile_data_offset = read_u64_le(&header, 56);
    let tile_data_length = read_u64_le(&header, 64);

    let internal_compression = header[97];
    let tile_type = header[99];
    let min_zoom = header[100];
    let max_zoom = header[101];

    // Every declared extent must lie within the file (catches truncation).
    for (off, len) in [
        (root_dir_offset, root_dir_length),
        (metadata_offset, metadata_length),
        (leaf_dirs_offset, leaf_dirs_length),
        (tile_data_offset, tile_data_length),
    ] {
        if extent_out_of_bounds(off, len, file_len) {
            return Err(ValidationError::Truncated);
        }
    }

    if tile_type != TILE_TYPE_MVT {
        return Err(ValidationError::NotVectorTiles(tile_type));
    }

    if file_len > max_bytes {
        return Err(ValidationError::SizeBudgetExceeded {
            actual: file_len,
            budget: max_bytes,
        });
    }

    // Read + decompress the metadata block.
    let raw_meta = archive
        .read_at(metadata_offset, metadata_length as usize)
        .map_err(|_| ValidationError::MetadataDecode)?;
    if raw_meta.len() != metadata_length as usize {
        return Err(ValidationError::MetadataDecode);
    }
    let meta_json = match internal_compression {
        COMPRESSION_NONE => raw_meta,
        COMPRESSION_GZIP => {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&raw_meta[..])
                .read_to_end(&mut out)
                .map_err(|_| ValidationError::MetadataDecode)?;
            out
        }
        _ => return Err(ValidationError::MetadataDecode),
    };

    let meta: serde_json::Value =
        serde_json::from_slice(&meta_json).map_err(|_| ValidationError::MetadataDecode)?;

    let layer_ids: Vec<String> = meta
        .get("vector_layers")
        .and_then(|v| v.as_array())
        .ok_or(ValidationError::MetadataDecode)?
        .iter()
        .filter_map(|l| l.get("id").and_then(|i| i.as_str()).map(String::from))
        .collect();

    let present: std::collections::HashSet<&str> = layer_ids.iter().map(String::as_str).collect();
    let missing: Vec<String> = REQUIRED_LAYER_IDS
        .iter()
        .filter(|id| !present.contains(**id))
        .map(|id| id.to_string())
        .collect();
    if !missing.is_empty() {
        return Err(ValidationError::MissingLayers(missing));
    }

    let schema_version = meta
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let planetiler_version = meta
        .get("planetiler:version")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(PmtilesValidation {
        schema_version,
        planetiler_version,
        min_zoom,
        max_zoom,
        len: file_len,
        layer_ids,
    })
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::io::Write;

    /// Builder for synthetic PMTiles archives used across the basemap tests.
    /// Defaults produce a well-formed Protomaps-vector archive with all 13 ids.
    pub struct TestPmtiles {
        pub magic: [u8; 7],
        pub version: u8,
        pub internal_compression: u8,
        pub tile_type: u8,
        pub min_zoom: u8,
        pub max_zoom: u8,
        pub layer_ids: Vec<String>,
        pub schema_version: String,
        pub planetiler_version: Option<String>,
        /// Override the declared `tile_data_length`; if larger than the real tile
        /// block, the archive looks truncated.
        pub declared_td_len_override: Option<u64>,
        pub root_dir_len: usize,
        pub tile_data_len: usize,
    }

    impl Default for TestPmtiles {
        fn default() -> Self {
            Self {
                magic: *b"PMTiles",
                version: 3,
                internal_compression: 2, // gzip
                tile_type: 1,            // MVT
                min_zoom: 0,
                max_zoom: 6,
                layer_ids: super::REQUIRED_LAYER_IDS.iter().map(|s| s.to_string()).collect(),
                schema_version: "3.7.1".to_string(),
                planetiler_version: Some("0.8-SNAPSHOT".to_string()),
                declared_td_len_override: None,
                root_dir_len: 16,
                tile_data_len: 32,
            }
        }
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn put_u64(buf: &mut [u8], offset: usize, value: u64) {
        buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    impl TestPmtiles {
        /// Assemble the full archive byte stream.
        pub fn build(&self) -> Vec<u8> {
            let layers: Vec<serde_json::Value> = self
                .layer_ids
                .iter()
                .map(|id| serde_json::json!({ "id": id, "fields": {} }))
                .collect();
            let mut meta = serde_json::json!({
                "vector_layers": layers,
                "version": self.schema_version,
                "type": "baselayer",
                "name": "tuxlink-test",
            });
            if let Some(pv) = &self.planetiler_version {
                meta["planetiler:version"] = serde_json::Value::String(pv.clone());
            }
            let meta_json = serde_json::to_vec(&meta).unwrap();
            let meta_bytes = match self.internal_compression {
                2 => gzip(&meta_json),
                _ => meta_json, // none or "unsupported" — written raw
            };

            let header_len = 127u64;
            let root_dir_offset = header_len;
            let root_dir_length = self.root_dir_len as u64;
            let metadata_offset = root_dir_offset + root_dir_length;
            let metadata_length = meta_bytes.len() as u64;
            let leaf_dirs_offset = metadata_offset + metadata_length;
            let leaf_dirs_length = 0u64;
            let tile_data_offset = leaf_dirs_offset;
            let real_td_len = self.tile_data_len as u64;
            let declared_td_len = self.declared_td_len_override.unwrap_or(real_td_len);

            let mut header = vec![0u8; 127];
            header[0..7].copy_from_slice(&self.magic);
            header[7] = self.version;
            put_u64(&mut header, 8, root_dir_offset);
            put_u64(&mut header, 16, root_dir_length);
            put_u64(&mut header, 24, metadata_offset);
            put_u64(&mut header, 32, metadata_length);
            put_u64(&mut header, 40, leaf_dirs_offset);
            put_u64(&mut header, 48, leaf_dirs_length);
            put_u64(&mut header, 56, tile_data_offset);
            put_u64(&mut header, 64, declared_td_len);
            header[96] = 1; // clustered
            header[97] = self.internal_compression;
            header[98] = 2; // tile_compression: gzip (unused by validator)
            header[99] = self.tile_type;
            header[100] = self.min_zoom;
            header[101] = self.max_zoom;

            let mut out = header;
            out.resize(out.len() + self.root_dir_len, 0u8);
            out.extend_from_slice(&meta_bytes);
            out.resize(out.len() + self.tile_data_len, 0u8);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::TestPmtiles;
    use super::*;
    use crate::basemap::PmtilesArchive;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const BUDGET: u64 = 200 * 1024 * 1024;

    fn archive_of(bytes: &[u8]) -> (NamedTempFile, PmtilesArchive) {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        let a = PmtilesArchive::open(f.path()).unwrap();
        (f, a)
    }

    #[test]
    fn accepts_well_formed_vector_archive() {
        let (_f, a) = archive_of(&TestPmtiles::default().build());
        let v = validate(&a, BUDGET).unwrap();
        assert_eq!(v.schema_version, "3.7.1");
        assert_eq!(v.planetiler_version.as_deref(), Some("0.8-SNAPSHOT"));
        assert_eq!(v.min_zoom, 0);
        assert_eq!(v.max_zoom, 6);
        assert_eq!(v.layer_ids.len(), 13);
    }

    #[test]
    fn accepts_uncompressed_metadata() {
        let bytes = TestPmtiles {
            internal_compression: 1,
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert!(validate(&a, BUDGET).is_ok());
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = TestPmtiles {
            magic: *b"NOTPMTl",
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert_eq!(validate(&a, BUDGET), Err(ValidationError::BadMagic));
    }

    #[test]
    fn rejects_unsupported_spec_version() {
        let bytes = TestPmtiles {
            version: 2,
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert_eq!(validate(&a, BUDGET), Err(ValidationError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_raster_tile_type() {
        let bytes = TestPmtiles {
            tile_type: 2, // PNG
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert_eq!(validate(&a, BUDGET), Err(ValidationError::NotVectorTiles(2)));
    }

    #[test]
    fn rejects_truncated_tile_data() {
        // Declare far more tile data than the file actually contains.
        let bytes = TestPmtiles {
            tile_data_len: 32,
            declared_td_len_override: Some(10_000_000),
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert_eq!(validate(&a, BUDGET), Err(ValidationError::Truncated));
    }

    #[test]
    fn rejects_too_short_for_header() {
        let (_f, a) = archive_of(&[0u8; 50]);
        assert_eq!(validate(&a, BUDGET), Err(ValidationError::Truncated));
    }

    #[test]
    fn rejects_missing_required_layer() {
        let mut ids: Vec<String> = REQUIRED_LAYER_IDS.iter().map(|s| s.to_string()).collect();
        ids.retain(|id| id != "water" && id != "roads");
        let bytes = TestPmtiles {
            layer_ids: ids,
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        match validate(&a, BUDGET) {
            Err(ValidationError::MissingLayers(missing)) => {
                assert!(missing.contains(&"water".to_string()));
                assert!(missing.contains(&"roads".to_string()));
            }
            other => panic!("expected MissingLayers, got {other:?}"),
        }
    }

    #[test]
    fn accepts_superset_of_required_layers() {
        let mut ids: Vec<String> = REQUIRED_LAYER_IDS.iter().map(|s| s.to_string()).collect();
        ids.push("extra_custom_layer".to_string());
        let bytes = TestPmtiles {
            layer_ids: ids,
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert!(validate(&a, BUDGET).is_ok());
    }

    #[test]
    fn rejects_over_size_budget() {
        let (_f, a) = archive_of(&TestPmtiles::default().build());
        let tiny_budget = 100u64;
        match validate(&a, tiny_budget) {
            Err(ValidationError::SizeBudgetExceeded { budget, .. }) => {
                assert_eq!(budget, tiny_budget);
            }
            other => panic!("expected SizeBudgetExceeded, got {other:?}"),
        }
    }

    #[test]
    fn rejects_undecodable_metadata() {
        // Claim gzip compression but the builder wrote raw JSON (compression=4
        // path writes raw) → gzip decode fails.
        let bytes = TestPmtiles {
            internal_compression: 4, // unsupported → MetadataDecode
            ..Default::default()
        }
        .build();
        let (_f, a) = archive_of(&bytes);
        assert_eq!(validate(&a, BUDGET), Err(ValidationError::MetadataDecode));
    }
}
