//! Print the release-pinned weights table in `sha256sum --check` format, one
//! line per file, named by release asset:
//!
//! ```text
//! 094f…4750  bge-small-en-v1.5--config.json
//! ```
//!
//! The release workflow fetches the upstream files, renames them to asset
//! names, and verifies them against THIS output before attaching them to the
//! release — so the assets a release serves and the digests the app enforces
//! can never drift: both come from [`tuxlink_classify::pins`].
//!
//! Deliberately needs no features (`--no-default-features` works): pins are
//! pure data, and the workflow shouldn't build candle to read three lines.

fn main() {
    for model in &tuxlink_classify::pins::PINNED_MODELS {
        for file in &model.files {
            println!("{}  {}", file.sha256, model.asset_name(file));
        }
    }
}
