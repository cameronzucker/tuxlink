//! Fixture-driven integration tests for the station-listing parser, against REAL captured
//! `/listings/` responses (grounded live 2026-06-07). Integration-test crate prefix is
//! `tuxlink_lib` (the lib name; the package is `tuxlink`).

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
