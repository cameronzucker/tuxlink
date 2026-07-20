//! Fixture-driven integration tests for the station-listing parser, against REAL captured
//! `/listings/` responses (grounded live 2026-06-07). Integration-test crate prefix is
//! `tuxlink_lib` (the lib name; the package is `tuxlink`).

use tuxlink_lib::catalog::channels_api::{
    join_channels, mode_code_to_detail, parse_channels_feed, synthesize_vara_fm_listing,
};
use tuxlink_lib::catalog::{parse_listing, ListingMode};

#[test]
fn parses_real_ardop_listing_fixture() {
    let body = include_str!("fixtures/catalog/listing-ardop-hf.txt");
    let listing = parse_listing(body, ListingMode::ArdopHf);
    assert!(listing.parsed_ok, "real fixture should parse at least one gateway");
    assert!(
        listing.gateways.len() >= 5,
        "expected several gateways, got {}",
        listing.gateways.len()
    );
    for g in &listing.gateways {
        assert!(!g.channel.is_empty());
        assert!(!g.callsign.is_empty(), "empty callsign for channel {:?}", g.channel);
    }
    // ARDOP HF gateways carry HF-band kHz frequencies (1.8–30 MHz).
    let has_hf = listing
        .gateways
        .iter()
        .flat_map(|g| &g.frequencies_khz)
        .any(|&f| (1800.0..=30_000.0).contains(&f));
    assert!(has_hf, "ARDOP HF gateways should have HF-band kHz frequencies");
}

#[test]
fn parses_all_confirmed_mode_fixtures() {
    for (mode, fixture) in [
        (ListingMode::VaraHf, include_str!("fixtures/catalog/listing-vara-hf.txt")),
        (ListingMode::Packet, include_str!("fixtures/catalog/listing-packet.txt")),
        (ListingMode::Pactor, include_str!("fixtures/catalog/listing-pactor.txt")),
        (ListingMode::RobustPacket, include_str!("fixtures/catalog/listing-robust-packet.txt")),
    ] {
        let listing = parse_listing(fixture, mode);
        assert!(listing.parsed_ok, "{:?} fixture should parse", mode);
        assert!(!listing.gateways.is_empty(), "{:?} fixture had no gateways", mode);
    }
}

#[test]
fn vara_fm_404_html_degrades_to_raw() {
    let body = include_str!("fixtures/catalog/listing-vara-fm-404.html");
    let listing = parse_listing(body, ListingMode::Packet);
    assert!(!listing.parsed_ok, "a 404 HTML page must not parse as a listing");
    assert!(listing.gateways.is_empty());
    assert_eq!(listing.raw, body);
}

#[test]
fn packet_fixture_includes_multibyte_station_without_corruption() {
    let body = include_str!("fixtures/catalog/listing-packet.txt");
    let listing = parse_listing(body, ListingMode::Packet);
    assert!(listing.parsed_ok);
    // The curated fixture includes the PI1ZTM block whose sysop name carries a non-ASCII byte.
    // GROUNDING NOTE: the upstream /listings/ endpoint serves THIS station double-encoded
    // (Latin-1-as-UTF-8 → "AndrÃ©"). The parser must preserve the bytes FAITHFULLY (no panic,
    // no strip-to-empty) — it is not the parser's job to "fix" upstream mojibake. The unit test
    // `multibyte_sysop_name_does_not_panic_and_parses` proves correct UTF-8 "André" round-trips.
    if let Some(g) = listing.gateways.iter().find(|g| g.callsign == "PI1ZTM") {
        let name = g.sysop_name.as_deref().unwrap_or("");
        assert!(!name.is_empty(), "multibyte sysop name must be preserved, not stripped");
        assert!(!name.is_ascii(), "non-ASCII byte(s) must be retained intact");
    }
}

// ---- Channels JSON API (tuxlink-nkzng) -------------------------------------
//
// End-to-end against the REAL captured channels-status fixture, exercised through
// the public `catalog::channels_api` surface (the same path Task 9's frontend and
// the Tauri command consume). See the fixture README for capture provenance and
// the FIXTURE-WINS divergences from the plan sketch.

const CHANNELS_SAMPLE: &str = include_str!("fixtures/catalog/channels-status-sample.json");

#[test]
fn channels_feed_dial_conversion_matches_text_listing_dial() {
    // The channels API reports the DIAL frequency directly in Hz (no 1500 Hz
    // audio-center offset): AI4Y's VARA channel at Frequency=3589000 Hz must land
    // at 3589.0 kHz, the SAME dial the text VARA HF listing shows for AI4Y.
    let feed = parse_channels_feed(CHANNELS_SAMPLE).expect("fixture must parse");
    let ai4y = feed.get("AI4Y").expect("AI4Y present in the feed");
    assert!(
        ai4y
            .iter()
            .any(|d| d.mode == ListingMode::VaraHf && (d.frequency_khz - 3589.0).abs() < 1e-9),
        "dial conversion must be Frequency/1000 with no offset"
    );

    // Cross-check the SAME dial appears in the text VARA HF listing for AI4Y.
    let text = parse_listing(
        include_str!("fixtures/catalog/listing-vara-hf.txt"),
        ListingMode::VaraHf,
    );
    if let Some(g) = text.gateways.iter().find(|g| g.callsign == "AI4Y") {
        assert!(
            g.frequencies_khz.iter().any(|&f| (f - 3589.0).abs() < 1e-9),
            "text listing dial and API dial must agree"
        );
    }
}

#[test]
fn channels_feed_mode_codes_map_as_documented() {
    assert_eq!(mode_code_to_detail(50), Some((ListingMode::VaraHf, Some(2300))));
    assert_eq!(mode_code_to_detail(51), Some((ListingMode::VaraFm, None)));
    assert_eq!(mode_code_to_detail(52), Some((ListingMode::VaraFm, None)));
    assert_eq!(mode_code_to_detail(22), None, "WINMOR is dropped");
}

#[test]
fn join_enriches_matching_gateway_from_real_fixture() {
    // 8P6BWS is present in BOTH the text VARA HF listing and the channels fixture.
    let feed = parse_channels_feed(CHANNELS_SAMPLE).unwrap();
    let mut vara = parse_listing(
        include_str!("fixtures/catalog/listing-vara-hf.txt"),
        ListingMode::VaraHf,
    );
    join_channels(&mut vara, &feed);
    let bws = vara
        .gateways
        .iter()
        .find(|g| g.callsign == "8P6BWS")
        .expect("8P6BWS in the text VARA HF listing");
    assert!(!bws.channel_details.is_empty(), "join must attach VARA channels");
    assert!(
        bws.channel_details.iter().all(|d| d.mode == ListingMode::VaraHf),
        "only the listing's own mode is attached"
    );
}

#[test]
fn synthesize_produces_vara_fm_listing_from_real_fixture() {
    let feed = parse_channels_feed(CHANNELS_SAMPLE).unwrap();
    let synth = synthesize_vara_fm_listing(&feed, 1_000);
    assert_eq!(synth.mode, ListingMode::VaraFm);
    assert!(synth.parsed_ok);
    assert!(!synth.gateways.is_empty(), "fixture has VARA FM stations");
    for g in &synth.gateways {
        assert!(
            g.channel_details.iter().all(|d| d.mode == ListingMode::VaraFm),
            "synthesized gateways carry only VARA FM channels"
        );
    }
}
