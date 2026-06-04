//! The 5-byte HID feature report that drives the CM108-family GPIO.
//!
//! Layout is taken verbatim from Direwolf's `cm108.c` (per the
//! bench-rig spec — see crate docs for the rationale). The two
//! payload bytes that matter:
//!
//! - **iodata** (`report[2]`): per-pin state bits. Bit `N-1` set → pin N driven high.
//! - **iomask** (`report[3]`): per-pin direction bits. Bit `N-1` set → pin N is an OUTPUT.
//!
//! Bytes 0, 1, and 4 are reserved; the kernel-side hidraw driver
//! accepts the report only with these exact zeros at those offsets
//! (verified by reading `kernel/drivers/hid/hidraw.c` — feature reports
//! with non-zero reserved bytes get silently truncated on some chip
//! revisions).
//!
//! ## Why we always write the mask
//!
//! Each assert/release writes both the data byte AND the mask byte
//! together. On a fresh chip after USB enumeration the GPIO pins
//! default to input direction; the first feature report establishes
//! the output direction along with the initial state. Subsequent
//! reports re-affirm both — costing nothing — so we don't have to
//! distinguish "first write" from "subsequent" and don't risk a stale
//! direction config.

use super::error::Cm108Result;
use super::ptt::{GpioPin, PttState};

/// Total byte count of a CM108 HID feature report.
pub const REPORT_SIZE: usize = 5;

/// A built HID feature report, ready to write to `/dev/hidraw*`.
///
/// Stored as a fixed-size array because the report is byte-exact and
/// allocating it on the heap would waste two cycles for no reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cm108Report(pub [u8; REPORT_SIZE]);

impl Cm108Report {
    /// Build the feature report for the given pin + state. Per the
    /// Direwolf cm108.c layout:
    ///
    /// ```text
    /// byte 0:  0x00   (reserved)
    /// byte 1:  0x00   (reserved)
    /// byte 2:  iodata = state << (pin-1)
    /// byte 3:  iomask = 1 << (pin-1)   (always an output)
    /// byte 4:  0x00   (reserved)
    /// ```
    ///
    /// Returns [`Cm108Error::InvalidPin`] if `pin` is outside the
    /// chip's 1..=8 range.
    pub fn build(pin: GpioPin, state: PttState) -> Cm108Result<Self> {
        let pin_index = pin.shift()?;
        let bit_mask: u8 = 1 << pin_index;
        let state_bit: u8 = match state {
            PttState::Asserted => 1 << pin_index,
            PttState::Released => 0,
        };
        Ok(Self([0, 0, state_bit, bit_mask, 0]))
    }

    /// Borrow the byte slice for [`std::io::Write::write`].
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::error::Cm108Error;
    use super::super::ptt::{GpioPin, PttState};
    use super::*;

    // The bench-rig reference pin: the DRA-100-DIN6 wires CM119A GPIO3
    // through a 2N2222 buffer to the radio PTT line. These two tests
    // pin the byte layout that the DRA-100 expects.

    #[test]
    fn gpio3_assert_matches_direwolf_cm108_c() {
        // From Direwolf cm108.c: for pin=3, asserted:
        //   iodata = 1 << (3-1) = 0x04
        //   iomask = 1 << (3-1) = 0x04
        let report = Cm108Report::build(GpioPin::new(3).unwrap(), PttState::Asserted).unwrap();
        assert_eq!(report.0, [0x00, 0x00, 0x04, 0x04, 0x00]);
    }

    #[test]
    fn gpio3_release_keeps_mask_clears_data() {
        // Release keeps the pin as an output (mask still 0x04) but
        // drives it low (data 0x00). This matches Direwolf: a release
        // is iodata=0, iomask unchanged.
        let report = Cm108Report::build(GpioPin::new(3).unwrap(), PttState::Released).unwrap();
        assert_eq!(report.0, [0x00, 0x00, 0x00, 0x04, 0x00]);
    }

    // Coverage for the other commonly-used PTT pins on CM108 variants.
    // GPIO1 is the SignaLink-USB convention; GPIO4 is used by some
    // CM108AH/CM108B builds (where GPIO2 is N.C.).

    #[test]
    fn gpio1_assert_uses_bit_0() {
        let report = Cm108Report::build(GpioPin::new(1).unwrap(), PttState::Asserted).unwrap();
        assert_eq!(report.0, [0x00, 0x00, 0x01, 0x01, 0x00]);
    }

    #[test]
    fn gpio4_assert_uses_bit_3() {
        let report = Cm108Report::build(GpioPin::new(4).unwrap(), PttState::Asserted).unwrap();
        assert_eq!(report.0, [0x00, 0x00, 0x08, 0x08, 0x00]);
    }

    #[test]
    fn gpio8_max_pin_uses_bit_7() {
        // CM108-family chips expose 8 GPIO bits; pin 8 → bit 7.
        let report = Cm108Report::build(GpioPin::new(8).unwrap(), PttState::Asserted).unwrap();
        assert_eq!(report.0, [0x00, 0x00, 0x80, 0x80, 0x00]);
    }

    #[test]
    fn pin_zero_is_invalid() {
        // Pins are 1-indexed per the C-Media datasheet (and Direwolf).
        // 0 is NOT a valid pin number.
        let err = GpioPin::new(0).unwrap_err();
        assert!(matches!(err, Cm108Error::InvalidPin { pin: 0 }));
    }

    #[test]
    fn pin_nine_is_invalid() {
        let err = GpioPin::new(9).unwrap_err();
        assert!(matches!(err, Cm108Error::InvalidPin { pin: 9 }));
    }

    #[test]
    fn report_size_is_five_bytes() {
        // Sanity-check the constant: 5 bytes per Direwolf cm108.c.
        // If a future kernel ever wanted a different size, this would
        // surface as a compile-time mismatch with the array literal.
        assert_eq!(REPORT_SIZE, 5);
        let report = Cm108Report::build(GpioPin::new(3).unwrap(), PttState::Asserted).unwrap();
        assert_eq!(report.as_bytes().len(), 5);
    }
}
