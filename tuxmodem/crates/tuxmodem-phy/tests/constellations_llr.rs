use num_complex::Complex;
use tuxmodem_phy::constellations::{Constellation, Mapper};

#[test]
fn bpsk_llr_sign_matches_hard_decision() {
    let mapper = Mapper::new(Constellation::Bpsk);
    let syms = vec![Complex::new(0.8, 0.0), Complex::new(-0.6, 0.0)];
    let n0 = 0.1; // noise variance
    let llrs = mapper.compute_llr(&syms, n0);
    // bit 0 (sym +0.8) → LLR positive, hard-decision 0
    assert!(llrs[0] > 0.0);
    // bit 1 (sym -0.6) → LLR negative, hard-decision 1
    assert!(llrs[1] < 0.0);
}

#[test]
fn qpsk_llr_sign_per_bit() {
    let mapper = Mapper::new(Constellation::Qpsk);
    let syms = vec![Complex::new(0.5, -0.3)];
    let llrs = mapper.compute_llr(&syms, 0.2);
    // I positive → b0=0 favoured → LLR_b0 > 0
    assert!(llrs[0] > 0.0);
    // Q negative → b1=1 favoured → LLR_b1 < 0
    assert!(llrs[1] < 0.0);
}

#[test]
fn llr_length_matches_bits_per_symbol() {
    for c in [
        Constellation::Bpsk,
        Constellation::Qpsk,
        Constellation::Qam16,
        Constellation::Qam64,
    ] {
        let mapper = Mapper::new(c);
        let syms = vec![Complex::new(0.1, 0.1); 8];
        let llrs = mapper.compute_llr(&syms, 0.5);
        assert_eq!(llrs.len(), syms.len() * c.bits_per_symbol());
    }
}
