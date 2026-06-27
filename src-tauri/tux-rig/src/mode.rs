//! Radio data/voice modes and their Hamlib `rigctl` string forms.

/// A subset of Hamlib modes relevant to HF Winlink. `rigctl_str` is the exact
/// token rigctld expects after `M`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    PktUsb,
    Usb,
    Lsb,
    PktLsb,
    DataU,
    DataL,
}

impl Mode {
    /// The exact token rigctld's `M` command expects.
    pub fn rigctl_str(&self) -> &'static str {
        match self {
            Mode::PktUsb => "PKTUSB",
            Mode::Usb => "USB",
            Mode::Lsb => "LSB",
            Mode::PktLsb => "PKTLSB",
            Mode::DataU => "USB-D",
            Mode::DataL => "LSB-D",
        }
    }

    /// Parse a rigctld mode token back into a `Mode`.
    pub fn from_rigctl(s: &str) -> Option<Mode> {
        match s.trim() {
            "PKTUSB" => Some(Mode::PktUsb),
            "USB" => Some(Mode::Usb),
            "LSB" => Some(Mode::Lsb),
            "PKTLSB" => Some(Mode::PktLsb),
            "USB-D" => Some(Mode::DataU),
            "LSB-D" => Some(Mode::DataL),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ft710_data_mode_is_pktusb() {
        assert_eq!(Mode::PktUsb.rigctl_str(), "PKTUSB");
    }

    #[test]
    fn round_trips_through_rigctl_str() {
        for m in [Mode::PktUsb, Mode::Usb, Mode::Lsb, Mode::PktLsb, Mode::DataU, Mode::DataL] {
            assert_eq!(Mode::from_rigctl(m.rigctl_str()), Some(m));
        }
    }

    #[test]
    fn unknown_mode_is_none() {
        assert_eq!(Mode::from_rigctl("FM"), None);
    }
}
