use tuxmodem_phy::constellations::{Constellation, Mapper};

#[test]
fn qam16_round_trip_clean() {
    let mapper = Mapper::new(Constellation::Qam16);
    let bits: Vec<u8> = (0..4 * 64).map(|i| (i % 2) as u8).collect();
    let syms = mapper.map(&bits);
    assert_eq!(syms.len(), bits.len() / 4);
    let recovered = mapper.hard_demap(&syms);
    assert_eq!(recovered, bits);
}

#[test]
fn qam64_round_trip_clean() {
    let mapper = Mapper::new(Constellation::Qam64);
    let bits: Vec<u8> = (0..6 * 64).map(|i| (i % 2) as u8).collect();
    let syms = mapper.map(&bits);
    assert_eq!(syms.len(), bits.len() / 6);
    let recovered = mapper.hard_demap(&syms);
    assert_eq!(recovered, bits);
}

#[test]
fn qam_constellations_are_unit_average_energy() {
    // Uniform average over all constellation points — drive every Gray
    // pattern at least once so the average isn't biased toward the subset
    // a non-uniform bit stream would sample (the plan-text version used
    // alternating bits, which hits only a single 16-QAM Gray symbol and
    // produces a misleading 0.2 average).
    for c in [Constellation::Qam16, Constellation::Qam64] {
        let mapper = Mapper::new(c);
        let bps = c.bits_per_symbol();
        let n_patterns = 1usize << bps;
        let mut bits: Vec<u8> = Vec::with_capacity(n_patterns * bps);
        for code in 0..n_patterns {
            for i in 0..bps {
                bits.push(((code >> (bps - 1 - i)) & 1) as u8);
            }
        }
        let syms = mapper.map(&bits);
        let energy: f32 = syms.iter().map(|s| s.norm_sqr()).sum::<f32>() / syms.len() as f32;
        assert!((energy - 1.0).abs() < 0.05, "{c:?} energy = {energy}, want ~1.0");
    }
}
