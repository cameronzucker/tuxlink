//! AX.25 connected-mode packet codec + (later) link layer.
//! P1 = wire codec only: addresses, paths, control fields, KISS framing.
//! KISS invariant: the TNC owns FCS/flags/bit-stuffing; the host frames carry
//! only [address-path][control][PID?][info?].

pub mod datalink;
pub mod devices;
pub mod direwolf_conf;
pub mod direwolf_probe;
pub mod frame;
pub mod kiss;
pub mod link;
pub mod managed_direwolf;
pub mod params;
pub mod rfcomm;

// P3 public surface (consumed by config.rs, winlink_backend.rs, ui_commands.rs)
pub use frame::Address;
pub use link::KissLinkConfig;
pub use params::Ax25Params;
pub use datalink::{connect, answer, Ax25Stream};
pub use link::connect_link;
pub use link::connect_link_with_abort;

// Managed-Dire-Wolf device discovery (Slice B, Phase 1): stable audio-device
// identity + same-USB-parent PTT resolution.
pub use devices::{
    discover_ptt, enumerate_audio_devices, read_sys_snapshot, resolve_managed_device, AudioDevice,
    PttChoice, ResolvedManagedDevice, StableAudioId, SysSnapshot,
};

// Managed-Dire-Wolf config generation (Slice B, Phase 2): pure direwolf.conf
// string generation from a resolved audio device + PTT choice + KISS port.
pub use direwolf_conf::{generate_direwolf_conf, DwParams};

// Managed-Dire-Wolf pre-spawn probes (Slice B, Phase 3): presence + version,
// conf-validation gate (over an injected runner), and ALSA device-busy probe.
pub use direwolf_probe::{
    device_busy_from_status, device_busy_message, direwolf_presence, meets_min_version,
    validate_conf, CommandRunner, ConfError, DwPresence, SystemCommandRunner,
};

// Managed-Dire-Wolf process lifecycle (Slice B, Phase 4): spawn/supervise/
// clean-shutdown a Dire Wolf KISS soundmodem (wrapping `ManagedModem`), plus the
// pure sound-card arbitration decision.
pub use managed_direwolf::{
    arbitrate, pick_free_kiss_port, Arbitration, CardId, DwLifecycleError, ManagedDireWolf,
    ManagedDireWolfCfg, ManagedDireWolfGuard,
};

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
