//! Native Winlink client — the Rust replacement for the Pat sidecar.
//!
//! Implements the winlink.org/B2F message exchange directly in Rust so tuxlink
//! no longer shells out to the Pat Go binary. The wire behaviour is verified
//! against the real Winlink CMS; `la5nta/wl2k-go` is read only as a reference
//! for the on-the-wire format — no Go code ships in tuxlink.
//!
//! Built bottom-up: the message structure first (this is what the mailbox and
//! the send/receive exchange both build on), then compression, the
//! back-and-forth exchange with the CMS, the telnet connection, and the
//! on-disk mailbox.

pub mod auth_taxonomy;
pub mod ax25;
pub mod b2f_events;
pub mod credentials;
pub mod modem;
pub mod compose;
pub mod handshake;
pub mod listener;
pub mod lzhuf;
pub mod message;
pub mod proposal;
pub mod redaction;
pub mod relay_banner;
pub mod secure;
pub mod session;
pub mod telnet;
pub mod telnet_listen;
pub mod telnet_p2p;
pub mod telnet_p2p_login;
pub mod transfer;
pub mod wire;

// Re-export cms_health at this level so the network probe can access it
// as crate::winlink::cms_health without touching crate::winlink::session::*
// (which is on the RADIO-1 forbidden-import list for probe modules).
pub use session::cms_health;
