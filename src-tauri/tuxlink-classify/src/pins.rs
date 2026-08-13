//! Release-pinned weight digests — the trusted expected values for content
//! verification (tuxlink-13ofm).
//!
//! [`hosting`](crate::hosting) deliberately stops at byte-length checks: a
//! digest is only as strong as WHERE its expected value comes from, and a
//! `manifest.json` that travels beside the weights is authored by whoever
//! authored the weights. These constants are the trusted source. They ride
//! the application release itself, so substituting them requires substituting
//! the binary — at which point every software check is moot anyway.
//!
//! Verification against these pins is therefore CONTENT-based and
//! transport-irrelevant: a file fetched from a GitHub release asset, copied
//! off a USB stick, or served over a LAN verifies identically, and a
//! mismatched file is refused BY NAME. Channel trust (TLS to some host) is
//! the weaker property and is not what the acquisition path relies on.
//!
//! Provenance of the current values (2026-08-13): `model.safetensors` is the
//! sha256 recorded in the upstream repository's git-lfs pointer
//! (BAAI/bge-small-en-v1.5 @ main — the digest HF itself verifies uploads
//! against); the two small files were hashed from the served bytes; all three
//! were cross-checked against the local working copy that produced the T1
//! eval numbers, and matched.
//!
//! Two consumers, one table: the app's acquisition pipeline enforces these at
//! download/import time, and the release workflow verifies the assets it
//! attaches against the very same constants via `examples/print_pins.rs`.
//!
//! CHANGE POLICY: never change a pinned digest under an EXISTING model id +
//! file name. Installed copies out in the field would silently lose their
//! strongest integrity tier and re-download over metered field links. New or
//! updated weights get a NEW model id (`bge-small-en-v1.5-r2`, a different
//! model, …) so old and new installs stay independently verifiable.

/// One required weights file and the exact content the release vouches for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFile {
    /// File name inside the model directory (matches [`REQUIRED_FILES`]).
    pub name: &'static str,
    /// Exact byte length.
    pub bytes: u64,
    /// Lowercase-hex sha256 of the full file contents.
    pub sha256: &'static str,
}

/// A model whose weights this release can acquire and verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedModel {
    /// Model id — also the directory name under a search root, and the asset
    /// name prefix on a release.
    pub model_id: &'static str,
    /// One entry per required file, in [`REQUIRED_FILES`] order.
    pub files: [PinnedFile; 3],
}

impl PinnedModel {
    /// Total payload size across all required files.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    /// The pin for one required file, if `name` is one.
    pub fn file(&self, name: &str) -> Option<&PinnedFile> {
        self.files.iter().find(|f| f.name == name)
    }

    /// Release-asset name for one file. GitHub release assets are a flat
    /// namespace, so the model id is folded into the name:
    /// `bge-small-en-v1.5--model.safetensors`.
    pub fn asset_name(&self, file: &PinnedFile) -> String {
        format!("{}--{}", self.model_id, file.name)
    }
}

/// Every model this release pins. One today; the array shape is what a second
/// model would extend.
pub const PINNED_MODELS: [PinnedModel; 1] = [PinnedModel {
    model_id: "bge-small-en-v1.5",
    files: [
        PinnedFile {
            name: "config.json",
            bytes: 743,
            sha256: "094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750",
        },
        PinnedFile {
            name: "tokenizer.json",
            bytes: 711_396,
            sha256: "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
        },
        PinnedFile {
            name: "model.safetensors",
            bytes: 133_466_304,
            sha256: "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad",
        },
    ],
}];

/// The pinned entry for `model_id`, if this release pins it.
pub fn pinned(model_id: &str) -> Option<&'static PinnedModel> {
    PINNED_MODELS.iter().find(|m| m.model_id == model_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosting::REQUIRED_FILES;

    #[test]
    fn every_pin_covers_exactly_the_required_files_in_order() {
        // The pins and the locator must agree on what a model directory IS.
        // A pin for a file the locator ignores would go unverified on disk;
        // a required file without a pin could never be acquired.
        for m in &PINNED_MODELS {
            let pinned_names: Vec<&str> = m.files.iter().map(|f| f.name).collect();
            assert_eq!(
                pinned_names,
                REQUIRED_FILES.to_vec(),
                "{} pins must match hosting::REQUIRED_FILES in order",
                m.model_id
            );
        }
    }

    #[test]
    fn digests_are_lowercase_hex_sha256() {
        for m in &PINNED_MODELS {
            for f in &m.files {
                assert_eq!(f.sha256.len(), 64, "{}/{}", m.model_id, f.name);
                assert!(
                    f.sha256
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "{}/{} digest must be lowercase hex",
                    m.model_id,
                    f.name
                );
            }
        }
    }

    #[test]
    fn byte_lengths_are_nonzero_and_the_known_total_holds() {
        for m in &PINNED_MODELS {
            for f in &m.files {
                assert!(f.bytes > 0, "{}/{}", m.model_id, f.name);
            }
        }
        // The number the design decision was made against (~134 MB).
        assert_eq!(pinned("bge-small-en-v1.5").unwrap().total_bytes(), 134_178_443);
    }

    #[test]
    fn model_ids_survive_the_locator_and_asset_names_are_flat() {
        for m in &PINNED_MODELS {
            // The id doubles as a directory segment; the locator's own rules
            // are the referee (a traversal-shaped id would never resolve).
            let status = crate::hosting::ModelLocator::new([]).locate(m.model_id);
            assert!(
                !format!("{status:?}").contains("UnsafeModelId"),
                "{} must be a safe path segment",
                m.model_id
            );
            for f in &m.files {
                let asset = m.asset_name(f);
                assert!(!asset.contains('/') && !asset.contains('\\'), "{asset}");
            }
        }
    }

    #[test]
    fn lookup_finds_the_primary_and_rejects_strangers() {
        assert!(pinned("bge-small-en-v1.5").is_some());
        assert!(pinned("all-MiniLM-L6-v2").is_none());
        assert!(pinned("").is_none());
    }
}
