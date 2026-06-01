use tuxmodem_phy::sync::frame_sync::{FrameSync, FrameSyncState};

#[test]
fn frame_sync_state_machine_advances_on_preamble() {
    let mut fs = FrameSync::new();
    assert_eq!(fs.state(), FrameSyncState::Searching);
    // Simulate preamble found event.
    fs.notify_preamble_found(/*start_sample=*/ 1_200, /*snr_db=*/ 15.0);
    assert_eq!(fs.state(), FrameSyncState::Acquired);
    fs.notify_frame_complete();
    assert_eq!(fs.state(), FrameSyncState::Searching);
}

#[test]
fn frame_sync_returns_to_search_on_decode_failure() {
    let mut fs = FrameSync::new();
    fs.notify_preamble_found(0, 12.0);
    fs.notify_decode_failed();
    assert_eq!(fs.state(), FrameSyncState::Searching);
}
