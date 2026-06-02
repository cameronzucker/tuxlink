//! AX.25 connected-mode packet codec + (later) link layer.
//! P1 = wire codec only: addresses, paths, control fields, KISS framing.
//! KISS invariant: the TNC owns FCS/flags/bit-stuffing; the host frames carry
//! only [address-path][control][PID?][info?].

pub mod datalink;
pub mod frame;
pub mod kiss;
pub mod link;
pub mod params;
pub mod rfcomm;

// P3 public surface (consumed by config.rs, winlink_backend.rs, ui_commands.rs)
pub use frame::Address;
pub use link::KissLinkConfig;
pub use params::Ax25Params;
pub use datalink::{connect, answer, Ax25Stream};
pub use link::connect_link;
pub use link::connect_link_with_abort;
pub use link::LinkAbort;

#[cfg(test)]
mod module_smoke {
    use super::{frame, kiss};
    #[test]
    fn public_surface_is_reachable() {
        // Compile-touches public items from both submodules to confirm they are
        // exported and reachable from the parent.
        let _ = (frame::PID_NO_L3, kiss::FEND);
    }
}
