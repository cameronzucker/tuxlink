//! Winlink gateway **channels** JSON API adapter (tuxlink-nkzng).
//!
//! The text `/listings/<Mode>Listing.aspx` pages (see [`super::stations`]) give a
//! gateway's dial list but not the per-channel BANDWIDTH, and VARA FM has no text
//! page at all. `POST https://api.winlink.org/gateway/status.json` returns a
//! structured, per-channel feed that fills both gaps: each channel carries its
//! numeric `Mode` code (→ transport + bandwidth), dial `Frequency`, operating
//! hours, and grid.
//!
//! Flow:
//! 1. [`fetch_channels_feed`] POSTs the form and hands the body to
//!    [`parse_channels_feed`], which maps the raw JSON into a callsign-keyed
//!    [`ChannelsFeed`] (`HashMap<CALLSIGN_UPPER, Vec<ChannelDetail>>`), dropping
//!    channels whose `Mode` code has no [`ListingMode`] mapping.
//! 2. [`join_channels`] enriches an already-parsed text [`StationListing`] by
//!    attaching each gateway's channels FOR THAT LISTING'S MODE (a VARA HF
//!    listing gets its VARA channels, an ARDOP listing its ARDOP channels, etc.).
//! 3. [`synthesize_vara_fm_listing`] builds a whole `VaraFm` [`StationListing`]
//!    out of the feed's VARA FM channels, since VARA FM has no text page.
//!
//! ## Fixture-grounded facts (see `tests/fixtures/catalog/channels-status-sample.README`)
//!
//! - The API `Frequency` is the **DIAL frequency in Hz**, already offset-free
//!   (grounded against the text-listing dials for the same station), so
//!   `frequency_khz = Frequency / 1000.0` with NO 1500 Hz audio-center subtraction.
//! - The top-level payload is `{ "Gateways": [ { "Callsign", "GatewayChannels":
//!   [ { "Mode", "Frequency", "OperatingHours", "Gridsquare" } ] } ] }`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::catalog::stations::{ChannelDetail, Gateway, ListingMode, StationListing};
use crate::ui_commands::UiError;

/// The gateway channels JSON API endpoint (POST, form-encoded).
pub const CHANNELS_API_URL: &str = "https://api.winlink.org/gateway/status.json";

/// Pat's public WDT AccessKey (la5nta/pat internal/cmsapi). Public directory
/// access token, NOT a credential; the operator can override it via the keyring
/// (`winlink::credentials::channels_api_key_read`).
pub const DEFAULT_CHANNELS_API_KEY: &str = "1880278F11684B358F36845615BD039A";

const CHANNELS_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// A callsign-keyed view of the channels feed: `CALLSIGN (with SSID, uppercased)`
/// → the gateway's channels across every mapped mode.
pub type ChannelsFeed = HashMap<String, Vec<ChannelDetail>>;

// ---------------------------------------------------------------------------
// Mode-code table
// ---------------------------------------------------------------------------

/// Map a Winlink API numeric mode code onto a [`ListingMode`] and its occupied
/// bandwidth in Hz (when the mode implies a fixed one).
///
/// Grounded against the full live feed (see the fixture README's table). Codes
/// with no tuxlink transport - WINMOR (22, obsolete) and the `Unknown` sentinel
/// (1200) - and any code not listed return `None`; the channel is then dropped.
///
/// VARA FM narrow (51) and wide (52) both collapse to [`ListingMode::VaraFm`]
/// with `None` bandwidth (VARA FM does not carry one of the fixed HF bandwidths).
pub fn mode_code_to_detail(code: u32) -> Option<(ListingMode, Option<u32>)> {
    Some(match code {
        0..=5 => (ListingMode::Packet, None),
        12..=19 => (ListingMode::Pactor, None),
        30 => (ListingMode::RobustPacket, None),
        41 => (ListingMode::ArdopHf, Some(500)),
        42 => (ListingMode::ArdopHf, Some(1000)),
        43 => (ListingMode::ArdopHf, Some(2000)),
        50 => (ListingMode::VaraHf, Some(2300)),
        51 | 52 => (ListingMode::VaraFm, None),
        53 => (ListingMode::VaraHf, Some(500)),
        54 => (ListingMode::VaraHf, Some(2750)),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Raw wire DTOs (deserialize-only; PascalCase field names)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawFeed {
    #[serde(default)]
    gateways: Vec<RawGateway>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawGateway {
    #[serde(default)]
    callsign: String,
    #[serde(default)]
    gateway_channels: Vec<RawChannel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawChannel {
    #[serde(default)]
    mode: u32,
    /// Dial frequency in Hz (integer on the wire).
    #[serde(default)]
    frequency: i64,
    #[serde(default)]
    operating_hours: Option<String>,
    #[serde(default)]
    gridsquare: Option<String>,
}

// ---------------------------------------------------------------------------
// Parse / join / synthesize
// ---------------------------------------------------------------------------

/// Parse the raw channels-API JSON body into a callsign-keyed [`ChannelsFeed`].
///
/// Each channel's numeric `Mode` is mapped via [`mode_code_to_detail`]; channels
/// with no mapping are dropped. A gateway that ends up with zero mapped channels
/// is omitted from the feed entirely. Callsigns are trimmed + uppercased so the
/// join against the text listings' (also-uppercased) callsigns is case-stable.
///
/// A body that is not parseable JSON is surfaced as [`UiError::Unavailable`] so
/// the caller serves text-only (the channels join is best-effort, never fatal).
pub fn parse_channels_feed(json: &str) -> Result<ChannelsFeed, UiError> {
    let raw: RawFeed = serde_json::from_str(json).map_err(|e| UiError::Unavailable {
        reason: format!("channels API response was not parseable JSON: {e}"),
    })?;

    let mut feed: ChannelsFeed = HashMap::new();
    for gw in raw.gateways {
        let callsign = gw.callsign.trim().to_uppercase();
        if callsign.is_empty() {
            continue;
        }
        let mut details: Vec<ChannelDetail> = Vec::new();
        for ch in gw.gateway_channels {
            let Some((mode, bandwidth_hz)) = mode_code_to_detail(ch.mode) else {
                continue;
            };
            details.push(ChannelDetail {
                // The API reports the DIAL frequency directly in Hz (offset-free);
                // no 1500 Hz center subtraction. See the fixture README, divergence 1.
                frequency_khz: ch.frequency as f64 / 1000.0,
                bandwidth_hz,
                mode,
                operating_hours: ch.operating_hours.filter(|s| !s.trim().is_empty()),
                grid: ch.gridsquare.filter(|s| !s.trim().is_empty()),
            });
        }
        if details.is_empty() {
            continue;
        }
        feed.entry(callsign).or_default().extend(details);
    }
    Ok(feed)
}

/// Attach channel detail to every gateway in `listing` that the feed knows about,
/// filtered to the LISTING'S OWN MODE so the enriched data stays mode-coherent (a
/// VARA HF listing's gateways get their VARA channels - modes 50/53/54 - not the
/// same gateway's ARDOP/Pactor rows). Gateways absent from the feed, or present
/// but with no channel for this mode, keep an empty `channel_details`.
pub fn join_channels(listing: &mut StationListing, feed: &ChannelsFeed) {
    let mode = listing.mode;
    for gw in listing.gateways.iter_mut() {
        let key = gw.callsign.trim().to_uppercase();
        if let Some(details) = feed.get(&key) {
            gw.channel_details = details
                .iter()
                .filter(|d| d.mode == mode)
                .cloned()
                .collect();
        }
    }
}

/// Synthesize a `VaraFm` [`StationListing`] from the feed (VARA FM has no text
/// `/listings/` page). One gateway per callsign that advertises at least one VARA
/// FM channel; its `frequencies_khz` and `channel_details` carry only the VARA FM
/// rows, and its grid is taken from the first VARA FM channel that reports one.
/// Gateways are sorted by callsign so the output is deterministic (the feed's
/// `HashMap` iteration order is not).
pub fn synthesize_vara_fm_listing(feed: &ChannelsFeed, fetched_at_ms: u64) -> StationListing {
    let mut gateways: Vec<Gateway> = Vec::new();
    for (callsign, details) in feed {
        let fm: Vec<ChannelDetail> = details
            .iter()
            .filter(|d| d.mode == ListingMode::VaraFm)
            .cloned()
            .collect();
        if fm.is_empty() {
            continue;
        }
        let grid = fm.iter().find_map(|d| d.grid.clone());
        let frequencies_khz: Vec<f64> = fm.iter().map(|d| d.frequency_khz).collect();
        gateways.push(Gateway {
            channel: callsign.clone(),
            callsign: callsign.clone(),
            sysop_name: None,
            grid,
            location: None,
            frequencies_khz,
            last_update: None,
            email: None,
            homepage: None,
            antenna: None,
            channel_details: fm,
        });
    }
    gateways.sort_by(|a, b| a.callsign.cmp(&b.callsign));

    StationListing {
        mode: ListingMode::VaraFm,
        title: Some("WINLINK VARA FM CHANNEL LISTING (synthesized from channels API)".to_string()),
        gateways,
        raw: String::new(),
        parsed_ok: true,
        fetched_at_ms: Some(fetched_at_ms),
    }
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/// Descriptive, identifiable User-Agent so winlink ops can contact rather than
/// ban (mirrors `commands::catalog_user_agent`).
fn channels_user_agent() -> String {
    format!(
        "Tuxlink/{} ({}; {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// POST the channels API form and parse the response into a [`ChannelsFeed`].
///
/// Mirrors the `commands::fetch_listing_from_url` client config (identifiable
/// user-agent, 30 s timeout, https-only) but POSTs the form the API requires.
/// `service_codes` is the operator-configured directory filter (default
/// `PUBLIC`); `key` is the API access key. Any transport / non-2xx / unparseable
/// outcome is an `Err` so the caller skips the join and serves text-only.
pub async fn fetch_channels_feed(key: &str, service_codes: &str) -> Result<ChannelsFeed, UiError> {
    let client = reqwest::Client::builder()
        .user_agent(channels_user_agent())
        .timeout(CHANNELS_HTTP_TIMEOUT)
        .https_only(true)
        .build()
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;

    let resp = client
        .post(CHANNELS_API_URL)
        .form(&[
            ("Mode", "anyall"),
            ("ServiceCodes", service_codes),
            ("key", key),
            ("format", "json"),
        ])
        .send()
        .await
        .map_err(|e| UiError::Transport { reason: e.to_string() })?;

    if !resp.status().is_success() {
        return Err(UiError::Unavailable {
            reason: format!("channels API returned {}", resp.status()),
        });
    }

    let text = resp
        .text()
        .await
        .map_err(|e| UiError::Transport { reason: e.to_string() })?;

    parse_channels_feed(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../tests/fixtures/catalog/channels-status-sample.json");

    // ---- mode_code_to_detail table ------------------------------------------

    #[test]
    fn mode_code_table_maps_vara_and_ardop_with_bandwidth() {
        assert_eq!(mode_code_to_detail(50), Some((ListingMode::VaraHf, Some(2300))));
        assert_eq!(mode_code_to_detail(53), Some((ListingMode::VaraHf, Some(500))));
        assert_eq!(mode_code_to_detail(54), Some((ListingMode::VaraHf, Some(2750))));
        assert_eq!(mode_code_to_detail(51), Some((ListingMode::VaraFm, None)));
        assert_eq!(mode_code_to_detail(52), Some((ListingMode::VaraFm, None)));
        assert_eq!(mode_code_to_detail(41), Some((ListingMode::ArdopHf, Some(500))));
        assert_eq!(mode_code_to_detail(42), Some((ListingMode::ArdopHf, Some(1000))));
        assert_eq!(mode_code_to_detail(43), Some((ListingMode::ArdopHf, Some(2000))));
        assert_eq!(mode_code_to_detail(30), Some((ListingMode::RobustPacket, None)));
        assert_eq!(mode_code_to_detail(0), Some((ListingMode::Packet, None)));
        assert_eq!(mode_code_to_detail(18), Some((ListingMode::Pactor, None)));
    }

    #[test]
    fn mode_code_table_drops_unmapped_codes() {
        // WINMOR (22, obsolete) and the Unknown sentinel (1200) have no transport.
        assert_eq!(mode_code_to_detail(22), None);
        assert_eq!(mode_code_to_detail(1200), None);
        assert_eq!(mode_code_to_detail(999), None);
    }

    // ---- parse_channels_feed ------------------------------------------------

    #[test]
    fn parses_fixture_into_callsign_keyed_feed() {
        let feed = parse_channels_feed(SAMPLE).unwrap();
        // 8P6BWS in the fixture has Pactor(19)+ARDOP(43)+VARA-2750(54) across 5
        // dials plus one VARA FM(51): 5*3 + 1 = 16 mapped channels.
        let bws = feed.get("8P6BWS").expect("8P6BWS must be in the feed");
        assert_eq!(bws.len(), 16, "all 16 mapped channels retained");
    }

    #[test]
    fn dial_conversion_is_hz_over_1000_no_offset() {
        // FIXTURE WINS: AI4Y's mode-54 (VARA 2750) channel reports Frequency
        // 3589000 Hz; the dial is 3589.0 kHz (matches the text-listing dial), i.e.
        // Frequency / 1000 with NO 1500 Hz subtraction.
        let feed = parse_channels_feed(SAMPLE).unwrap();
        let ai4y = feed.get("AI4Y").expect("AI4Y must be in the feed");
        let ch = ai4y
            .iter()
            .find(|d| d.mode == ListingMode::VaraHf && (d.frequency_khz - 3589.0).abs() < 1e-9)
            .expect("AI4Y must have a VARA HF channel at the 3589.0 kHz dial");
        assert_eq!(ch.frequency_khz, 3589000.0 / 1000.0);
        assert_eq!(ch.bandwidth_hz, Some(2750)); // mode 54
    }

    #[test]
    fn vara_fm_channels_land_under_vara_fm_mode() {
        let feed = parse_channels_feed(SAMPLE).unwrap();
        // 4F1PUZ-10 is a VARA-FM-WIDE-only (mode 52) VHF gateway.
        let fm = feed.get("4F1PUZ-10").expect("4F1PUZ-10 must be in the feed");
        assert!(fm.iter().all(|d| d.mode == ListingMode::VaraFm));
        assert!(fm.iter().any(|d| d.frequency_khz > 100_000.0), "VHF dial in kHz");
        // VARA FM carries no fixed bandwidth.
        assert!(fm.iter().all(|d| d.bandwidth_hz.is_none()));
    }

    #[test]
    fn gateway_with_only_unmapped_modes_is_absent() {
        // K7NGS-5 advertises only the Unknown (1200) sentinel → no mapped channel
        // → dropped from the feed entirely.
        let feed = parse_channels_feed(SAMPLE).unwrap();
        assert!(!feed.contains_key("K7NGS-5"));
    }

    #[test]
    fn unmapped_channel_dropped_while_siblings_survive() {
        // WX4PCA-10 mixes a WINMOR(22) channel with Packet/ARDOP/VARA ones; the
        // WINMOR channel is dropped but the gateway survives with its others.
        let feed = parse_channels_feed(SAMPLE).unwrap();
        let wx = feed.get("WX4PCA-10").expect("WX4PCA-10 survives via its mapped channels");
        assert!(!wx.is_empty());
        // No VARA FM here, and every retained channel is a real mapped transport.
        assert!(wx.iter().any(|d| d.mode == ListingMode::VaraHf));
    }

    #[test]
    fn parse_rejects_non_json() {
        let err = parse_channels_feed("<html>not json</html>").unwrap_err();
        assert!(matches!(err, UiError::Unavailable { .. }));
    }

    // ---- join_channels ------------------------------------------------------

    fn gw(callsign: &str) -> Gateway {
        Gateway {
            channel: format!("{callsign}.WINLINK"),
            callsign: callsign.to_string(),
            sysop_name: None,
            grid: None,
            location: None,
            frequencies_khz: vec![],
            last_update: None,
            email: None,
            homepage: None,
            antenna: None,
            channel_details: vec![],
        }
    }

    fn listing(mode: ListingMode, callsigns: &[&str]) -> StationListing {
        StationListing {
            mode,
            title: None,
            gateways: callsigns.iter().map(|c| gw(c)).collect(),
            raw: String::new(),
            parsed_ok: true,
            fetched_at_ms: None,
        }
    }

    #[test]
    fn join_attaches_matching_mode_channels_and_leaves_others_empty() {
        let feed = parse_channels_feed(SAMPLE).unwrap();
        // A VARA HF listing with a known gateway (8P6BWS, has VARA 2750 channels)
        // and an unknown one (ZZ9ZZ, absent from the feed).
        let mut l = listing(ListingMode::VaraHf, &["8P6BWS", "ZZ9ZZ"]);
        join_channels(&mut l, &feed);

        let bws = l.gateways.iter().find(|g| g.callsign == "8P6BWS").unwrap();
        assert!(!bws.channel_details.is_empty(), "known gateway gets channels");
        assert!(
            bws.channel_details.iter().all(|d| d.mode == ListingMode::VaraHf),
            "only the listing's own mode is attached (VARA 50/53/54), not ARDOP/Pactor"
        );

        let zz = l.gateways.iter().find(|g| g.callsign == "ZZ9ZZ").unwrap();
        assert!(zz.channel_details.is_empty(), "unknown gateway stays empty");
    }

    #[test]
    fn join_is_case_insensitive_on_callsign() {
        let feed = parse_channels_feed(SAMPLE).unwrap();
        let mut l = listing(ListingMode::VaraHf, &["ai4y"]); // lower-case
        join_channels(&mut l, &feed);
        assert!(!l.gateways[0].channel_details.is_empty());
    }

    // ---- synthesize_vara_fm_listing -----------------------------------------

    #[test]
    fn synthesize_yields_vara_fm_listing_of_only_fm_callsigns() {
        let feed = parse_channels_feed(SAMPLE).unwrap();
        let synth = synthesize_vara_fm_listing(&feed, 42_000);
        assert_eq!(synth.mode, ListingMode::VaraFm);
        assert!(synth.parsed_ok);
        assert_eq!(synth.fetched_at_ms, Some(42_000));
        assert!(!synth.gateways.is_empty());
        // Every synthesized gateway must have at least one VARA FM channel and
        // ONLY VARA FM channels.
        for g in &synth.gateways {
            assert!(!g.channel_details.is_empty());
            assert!(g.channel_details.iter().all(|d| d.mode == ListingMode::VaraFm));
        }
        // 4F1PUZ-10 (FM-only) must appear; AC8NP (VARA-HF-only, mode 50) must not.
        let calls: Vec<&str> = synth.gateways.iter().map(|g| g.callsign.as_str()).collect();
        assert!(calls.contains(&"4F1PUZ-10"));
        assert!(!calls.contains(&"AC8NP"));
        // Deterministic ordering (sorted by callsign).
        let mut sorted = calls.clone();
        sorted.sort_unstable();
        assert_eq!(calls, sorted);
    }

    // ---- ChannelDetail wire shape (frontend contract) -----------------------

    #[test]
    fn channel_detail_round_trips_in_camel_case() {
        let d = ChannelDetail {
            frequency_khz: 7104.0,
            bandwidth_hz: Some(2300),
            mode: ListingMode::VaraHf,
            operating_hours: Some("00-23".to_string()),
            grid: Some("FN13".to_string()),
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "frequencyKhz": 7104.0,
                "bandwidthHz": 2300,
                "mode": "vara-hf",
                "operatingHours": "00-23",
                "grid": "FN13"
            })
        );
        let back: ChannelDetail = serde_json::from_value(v).unwrap();
        assert_eq!(back, d);
    }
}
