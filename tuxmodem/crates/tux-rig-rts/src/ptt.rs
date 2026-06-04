//! `Ptt` trait + state.
//!
//! Duplicated from `tux-rig-cm108`'s `ptt` module so each backend
//! crate stands alone for now. The future `tux-rig` umbrella crate
//! (per ADR 0015 and the locked `tuxlink-5jb` decision) will lift
//! these into a `tux-rig-core` and re-export from both backend
//! crates; until that umbrella lands, the duplication is the
//! lowest-coupling option.

/// What we're telling the radio to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PttState {
    /// Transmit — radio is keyed.
    Asserted,
    /// Receive — radio is unkeyed.
    Released,
}

/// The PTT abstraction.
///
/// Implementations OWN the underlying hardware handle and guarantee
/// release-on-Drop. Callers MUST NOT share a `Ptt` across threads
/// without external synchronization — the RTS line's hardware state
/// is a single bit; a race between assert + release on two threads
/// produces an arbitrary final state.
pub trait Ptt {
    /// Error type the impl surfaces from assert/release.
    type Error;

    /// Assert PTT (key the transmitter).
    fn assert(&mut self) -> Result<(), Self::Error>;

    /// Release PTT (un-key the transmitter).
    fn release(&mut self) -> Result<(), Self::Error>;

    /// Current best-known state. `Released` if `release` succeeded
    /// or no assertion has been made yet; `Asserted` if the last
    /// successful operation was an assertion. Last-write-wins from
    /// our perspective — the hardware line may have been desynced
    /// by an out-of-band actor (impossible for RTS on a USB-serial
    /// chip we exclusively own, but the rule applies generally).
    fn state(&self) -> PttState;
}
