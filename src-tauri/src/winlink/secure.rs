//! The Winlink secure-login response.
//!
//! When the server sends a password challenge (`;PQ:` during the handshake),
//! the client must answer with a token derived from the challenge, the station
//! password, and a fixed salt. The token is the MD5 of those three joined,
//! reduced to eight decimal digits.
//!
//! Ported from `la5nta/wl2k-go`'s `fbb/secure.go` (itself ported from
//! paclink-unix). The password is supplied by the caller and never stored here.

use md5::{Digest, Md5};

/// The fixed salt mixed into every secure-login token (from paclink-unix).
const WINLINK_SECURE_SALT: [u8; 64] = [
    77, 197, 101, 206, 190, 249, 93, 200, 51, 243, 93, 237, 71, 94, 239, 138, 68, 108, 70, 185,
    225, 137, 217, 16, 51, 122, 193, 48, 194, 195, 198, 175, 172, 169, 70, 84, 61, 62, 104, 186,
    114, 52, 61, 168, 66, 129, 192, 208, 187, 249, 232, 193, 41, 113, 41, 45, 240, 16, 29, 228,
    208, 228, 61, 20,
];

/// Compute the secure-login response for a password challenge.
///
/// `challenge` is the value the server sent after `;PQ:`. `password` is the
/// station password (supplied by the caller, e.g. from the OS keyring). The
/// result is the eight-digit token the client sends back as `;PR: <token>`.
pub fn secure_login_response(challenge: &str, password: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(challenge.as_bytes());
    hasher.update(password.as_bytes());
    hasher.update(WINLINK_SECURE_SALT);
    let sum = hasher.finalize();

    // Take 30 bits across the first four MD5 bytes (the top byte masked to 6
    // bits), assembled most-significant-byte first.
    let mut pr: i32 = (sum[3] & 0x3f) as i32;
    for i in (0..=2).rev() {
        pr = (pr << 8) | sum[i] as i32;
    }

    // Render as decimal and keep the last eight digits.
    let digits = format!("{pr:08}");
    let token = &digits[digits.len() - 8..];

    // Defense-in-depth: log only lengths, never the challenge value or the token.
    // The token IS the credential the wire sanitizer must redact in ;PR: lines.
    tracing::debug!(
        target: "tuxlink::winlink::secure",
        challenge_len = challenge.len(),
        response_len = token.len(),
        "secure-login response computed",
    );

    token.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_the_reference_login_responses() {
        // Vectors from wl2k-go's secure_test.go — note the password is
        // case-sensitive (the two differ only in case yet give different tokens).
        assert_eq!(secure_login_response("23753528", "FOOBAR"), "72768415");
        assert_eq!(secure_login_response("23753528", "FooBar"), "95074758");
    }
}
