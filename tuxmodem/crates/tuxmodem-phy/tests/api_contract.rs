use tuxmodem_phy::modes::ModeHint;
use tuxmodem_phy::phy_api::{ChannelQualityReport, NullPhy, PhyTransport};

#[test]
fn null_phy_round_trips_a_payload_through_loopback() {
    let mut phy = NullPhy::new();
    let payload = b"hello tuxmodem";
    let _token = phy.send_frame(payload, ModeHint::MainAuto).expect("tx");
    let rx = phy.poll_rx().expect("rx should be available immediately on null phy");
    assert_eq!(rx.payload(), payload);
    assert!(rx.decode_ok());
}

#[test]
fn channel_quality_report_is_readable_without_tx() {
    let phy = NullPhy::new();
    let q: ChannelQualityReport = phy.channel_quality();
    // Default report should be present even with no frames yet.
    assert!(q.aggregate_snr_db().is_finite() || q.aggregate_snr_db().is_nan());
}
