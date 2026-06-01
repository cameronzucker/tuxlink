//! Log-likelihood-ratio (LLR) helpers.
//!
//! Convention (matches subsystem #3 PHY's `FecCodec::decode_soft`
//! contract per spec R2): `LLR(b) = log(P(b=0) / P(b=1))`.
//! Sign positive ⇒ hard-decision 0. Sign negative ⇒ hard-decision 1.
//! `|LLR|` is the confidence.

/// Hard-decide an LLR into a bit value (0 or 1 as `bool`).
pub fn hard_decide(llr: f32) -> bool {
    // Positive LLR → bit 0 (false); negative → bit 1 (true).
    llr < 0.0
}

/// Box-plus operator: combine two LLRs as if they were independent
/// observations of the same XOR sum. Used in the check-node update
/// step of SPA decoding:
///
/// `boxplus(a, b) = 2 · atanh(tanh(a/2) · tanh(b/2))`
///
/// The numerically-stable sign-and-min approximation is acceptable
/// for v0.5+; the exact `tanh` form here is the canonical reference.
/// Phase 8 profiling may swap for the approximation.
pub fn boxplus(a: f32, b: f32) -> f32 {
    let ta = (a / 2.0).tanh();
    let tb = (b / 2.0).tanh();
    let prod = (ta * tb).clamp(-0.999_999_94, 0.999_999_94);
    2.0 * prod.atanh()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_decide_positive_llr_is_zero() {
        assert!(!hard_decide(2.5));
    }

    #[test]
    fn hard_decide_negative_llr_is_one() {
        assert!(hard_decide(-2.5));
    }

    #[test]
    fn boxplus_identity_with_infinite_certain() {
        // boxplus with a high-confidence LLR preserves the other arg.
        let result = boxplus(1.0, 100.0);
        assert!(
            (result - 1.0).abs() < 0.01,
            "boxplus(1.0, 100.0) preserves 1.0: got {result}"
        );
    }

    #[test]
    fn boxplus_sign_xor() {
        // Box-plus of two same-sign LLRs is same-sign.
        assert!(boxplus(2.0, 3.0) > 0.0);
        assert!(boxplus(-2.0, -3.0) > 0.0); // (-)(-) = +
        // Mixed-sign → negative.
        assert!(boxplus(2.0, -3.0) < 0.0);
    }
}
