//! `Ptt` trait + state + pin-number newtype.
//!
//! The trait is the abstraction other crates depend on; concrete
//! implementations sit at the writer layer ([`super::Cm108Ptt`] for
//! the real hidraw path, future hamlib/cm108 / `gpioctl`-style
//! backends will implement the same trait).

use super::error::{Cm108Error, Cm108Result};

/// What we're telling the radio to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PttState {
    /// Transmit — radio is keyed.
    Asserted,
    /// Receive — radio is unkeyed.
    Released,
}

/// 1-indexed GPIO pin on a CM108-family chip. The C-Media datasheet
/// (and Direwolf, Hamlib, fldigi) all use 1..=8 numbering — pin N
/// corresponds to bit `N-1` in the HID report's data + mask bytes.
///
/// Constructed via [`Self::new`] which rejects out-of-range values.
/// Cannot be constructed with bare struct syntax — the field is
/// private — so it's impossible to hold a `GpioPin` that names an
/// invalid pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpioPin(u8);

impl GpioPin {
    /// Construct a `GpioPin`. Returns [`Cm108Error::InvalidPin`] if
    /// `pin` is 0 or > 8.
    pub fn new(pin: u8) -> Cm108Result<Self> {
        if (1..=8).contains(&pin) {
            Ok(Self(pin))
        } else {
            Err(Cm108Error::InvalidPin { pin })
        }
    }

    /// The pin number as a `u8` (1..=8).
    pub fn number(self) -> u8 {
        self.0
    }

    /// The bit shift to use when encoding this pin into the HID
    /// report's data + mask bytes (`pin_number - 1`).
    pub fn shift(self) -> Cm108Result<u32> {
        // Construction guarantees 1..=8, so subtraction never underflows.
        // The fallible signature mirrors `new` so the call-site doesn't
        // have to teach Rust about the invariant.
        Ok(u32::from(self.0 - 1))
    }
}

/// The PTT abstraction.
///
/// Implementations OWN the underlying hardware handle and guarantee
/// release-on-Drop. Callers MUST NOT share a `Ptt` across threads
/// without external synchronization — the underlying chip latches
/// state, and a race between assert-and-release on two threads
/// produces an arbitrary final state.
pub trait Ptt {
    /// Assert PTT (key the transmitter).
    fn assert(&mut self) -> Cm108Result<()>;

    /// Release PTT (un-key the transmitter).
    fn release(&mut self) -> Cm108Result<()>;

    /// Current best-known state. `Released` if `release` succeeded or
    /// no assertion has been made yet; `Asserted` if the last write
    /// was an assertion. This is the CALLER'S view — the chip may
    /// have been desynced by an out-of-band actor, but for our
    /// purposes the last-write-wins rule applies.
    fn state(&self) -> PttState;
}
