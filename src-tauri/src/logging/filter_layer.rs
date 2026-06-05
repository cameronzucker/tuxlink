//! Per-target Filter Layer wired with a reload handle for atomic Detailed-mode
//! swaps (spec §4.1, §6.5).

use tracing_subscriber::{
    filter::EnvFilter,
    reload::{Handle, Layer as ReloadLayer},
    Registry,
};

/// Build the Standard-mode filter directive string per the §4.1 matrix.
pub fn standard_directive() -> String {
    // Transport / modem / AX.25 / listener clusters at debug
    // Everything else at info
    [
        "tuxlink::winlink::session=debug",
        "tuxlink::winlink::secure=debug",
        "tuxlink::winlink::handshake=debug",
        "tuxlink::winlink::transfer=debug",
        "tuxlink::winlink::wire=debug",
        "tuxlink::winlink::lzhuf=debug",
        "tuxlink::winlink::telnet=debug",
        "tuxlink::winlink::telnet_listen=debug",
        "tuxlink::winlink::telnet_p2p=debug",
        "tuxlink::winlink::telnet_p2p_login=debug",
        "tuxlink::winlink::modem::ardop=debug",
        "tuxlink::winlink::modem::vara=debug",
        "tuxlink::winlink::modem::process=debug",
        "tuxlink::winlink::ax25=debug",
        "tuxlink::winlink::listener=debug",
        "info",
    ]
    .join(",")
}

/// Build the Detailed-mode filter directive string per the §4.1 matrix.
pub fn detailed_directive() -> String {
    // Transport / modem / AX.25 / listener clusters at trace
    // Everything else at debug
    [
        "tuxlink::winlink::session=trace",
        "tuxlink::winlink::secure=trace",
        "tuxlink::winlink::handshake=trace",
        "tuxlink::winlink::transfer=trace",
        "tuxlink::winlink::wire=trace",
        "tuxlink::winlink::lzhuf=trace",
        "tuxlink::winlink::telnet=trace",
        "tuxlink::winlink::telnet_listen=trace",
        "tuxlink::winlink::telnet_p2p=trace",
        "tuxlink::winlink::telnet_p2p_login=trace",
        "tuxlink::winlink::modem::ardop=trace",
        "tuxlink::winlink::modem::vara=trace",
        "tuxlink::winlink::modem::process=trace",
        "tuxlink::winlink::ax25=trace",
        "tuxlink::winlink::listener=trace",
        "debug",
    ]
    .join(",")
}

/// Build the reload-wrapped filter for Standard mode.
/// Returns the layer (insert into Subscriber) + handle (call set_filter for swap).
pub fn build() -> (ReloadLayer<EnvFilter, Registry>, Handle<EnvFilter, Registry>) {
    let filter = EnvFilter::try_new(standard_directive())
        .expect("standard directive must parse");
    ReloadLayer::new(filter)
}

/// Swap the filter to Detailed mode.
pub fn set_detailed(handle: &Handle<EnvFilter, Registry>) -> Result<(), String> {
    let filter = EnvFilter::try_new(detailed_directive())
        .map_err(|e| format!("detailed directive parse failure: {e}"))?;
    handle
        .modify(|f| *f = filter)
        .map_err(|e| format!("filter reload failure: {e}"))
}

/// Swap the filter back to Standard mode.
pub fn set_standard(handle: &Handle<EnvFilter, Registry>) -> Result<(), String> {
    let filter = EnvFilter::try_new(standard_directive())
        .map_err(|e| format!("standard directive parse failure: {e}"))?;
    handle
        .modify(|f| *f = filter)
        .map_err(|e| format!("filter reload failure: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_directive_parses() {
        assert!(EnvFilter::try_new(standard_directive()).is_ok());
    }

    #[test]
    fn detailed_directive_parses() {
        assert!(EnvFilter::try_new(detailed_directive()).is_ok());
    }

    #[test]
    fn build_returns_layer_and_handle() {
        let (_layer, _handle) = build();
        // Just verifying the pair constructs without panic.
    }
}
