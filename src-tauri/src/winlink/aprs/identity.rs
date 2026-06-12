//! APRS station identity — separate from the Winlink identity.
//!
//! An APRS transmission is addressed `<source> > <tocall> [via <path>]`:
//!   - `source` is the operator's base callsign plus an APRS-specific SSID
//!     (e.g. `-7` for a portable HT), independent of the Winlink callsign SSID.
//!   - `tocall` is the destination/“to” call that identifies the sending
//!     software (tuxlink uses `APZTUX`, an experimental `APZ…` prefix).
//!   - `path` is the digipeater alias list (`WIDE1-1,WIDE2-1`), 0..=2 hops —
//!     AX.25's address field allows at most 2 digipeaters after the
//!     source/destination pair.

use crate::winlink::ax25::frame::Address;

/// Resolved APRS station identity for one TX/RX session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AprsIdentity {
    pub source: Address,    // operator base call + APRS ssid
    pub tocall: Address,    // APZTUX, ssid 0
    pub path: Vec<Address>, // digipeater aliases, 0..=2
}

/// Parse a comma path like `"WIDE1-1,WIDE2-1"` into addresses. Errors if >2
/// digipeaters (AX.25 limit) or a token is malformed. An empty/whitespace input
/// yields an empty path (a direct, no-digi transmission).
pub fn parse_path(s: &str) -> Result<Vec<Address>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() > 2 {
        return Err(format!("APRS path has {} digipeaters; max is 2", parts.len()));
    }
    parts.iter().map(|tok| parse_addr(tok)).collect()
}

/// Parse one `"CALL-SSID"` token (SSID optional → 0). Callsign is uppercased and
/// must be 1..=6 chars; SSID must be 0..=15 (AX.25 4-bit field).
fn parse_addr(tok: &str) -> Result<Address, String> {
    let (call, ssid) = match tok.split_once('-') {
        Some((c, s)) => (c, s.parse::<u8>().map_err(|_| format!("bad SSID in '{tok}'"))?),
        None => (tok, 0),
    };
    if call.is_empty() || call.len() > 6 {
        return Err(format!("bad callsign in '{tok}'"));
    }
    if ssid > 15 {
        return Err(format!("SSID out of range in '{tok}'"));
    }
    Ok(Address { call: call.to_uppercase(), ssid })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_path_splits_wide_aliases() {
        let p = parse_path("WIDE1-1,WIDE2-1").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], Address { call: "WIDE1".into(), ssid: 1 });
        assert_eq!(p[1], Address { call: "WIDE2".into(), ssid: 1 });
    }

    #[test]
    fn parse_path_handles_no_ssid() {
        let p = parse_path("RELAY").unwrap();
        assert_eq!(p[0], Address { call: "RELAY".into(), ssid: 0 });
    }

    #[test]
    fn parse_path_empty_is_empty_vec() {
        assert_eq!(parse_path("").unwrap(), vec![]);
    }

    #[test]
    fn parse_path_rejects_more_than_two_digis() {
        assert!(parse_path("W1-1,W2-1,W3-1").is_err());
    }
}
