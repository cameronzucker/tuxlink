//! Phase 10.1 acceptance tests: FecCodec contract surface + identity
//! pass-through.

use tuxmodem_phy::coded_modulation::{CodeRate, FecCodec, IdentityFec};

#[test]
fn identity_fec_round_trips_bits_unchanged() {
    let fec = IdentityFec::new(64);
    let info = vec![1u8, 0, 1, 1, 0, 0, 1, 0];
    let encoded = fec.encode(&info);
    assert_eq!(encoded, info);
    // Build pseudo-LLR vector that hard-decodes to `info`:
    let llrs: Vec<f32> = info
        .iter()
        .map(|&b| if b == 0 { 1.0 } else { -1.0 })
        .collect();
    let recovered = fec.decode_soft(&llrs).unwrap();
    assert_eq!(recovered, info);
}

#[test]
fn code_rate_one_indicates_no_redundancy() {
    let r = CodeRate { num: 1, den: 1 };
    assert!((r.value() - 1.0).abs() < 1e-9);
}
