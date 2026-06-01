use num_complex::Complex;
use tuxmodem_phy::constellations::{Constellation, Mapper};

#[test]
fn bpsk_maps_bits_to_unit_circle_and_back() {
    let mapper = Mapper::new(Constellation::Bpsk);
    let bits = [0u8, 1, 1, 0, 1, 0];
    let syms: Vec<Complex<f32>> = mapper.map(&bits);
    let recovered = mapper.hard_demap(&syms);
    assert_eq!(bits.to_vec(), recovered);
    // BPSK symbols sit at +/-1.0 on the real axis.
    for s in &syms {
        assert!((s.norm() - 1.0).abs() < 1e-6);
    }
}

#[test]
fn qpsk_maps_bit_pairs_to_quadrants() {
    let mapper = Mapper::new(Constellation::Qpsk);
    let bits = [0u8, 0, 0, 1, 1, 0, 1, 1];
    let syms = mapper.map(&bits);
    assert_eq!(syms.len(), 4);
    // QPSK symbols sit on the unit circle at +/-(1/sqrt2) +/- j(1/sqrt2)
    for s in &syms {
        assert!((s.norm() - 1.0).abs() < 1e-6);
    }
    let recovered = mapper.hard_demap(&syms);
    assert_eq!(bits.to_vec(), recovered);
}
