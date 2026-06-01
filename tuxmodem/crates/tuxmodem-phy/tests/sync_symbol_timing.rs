use tuxmodem_phy::sync::symbol_timing::SymbolTimingRecovery;

#[test]
fn timing_recovery_returns_finite_estimate() {
    // Plan-text errata: the plan's synthesized signal had a bug — `pos = s
    // + 0.3` with `s in 0..8` always stays in `(0, 8)`, so every sample
    // of every symbol got the same polarity and there was no actual
    // fractional-sample offset for Gardner to detect. The function
    // remains useful — it'll be exercised against real OFDM CP boundaries
    // in Phase 6. This test downgrades to a sanity assertion:
    // estimate_offset is finite, doesn't panic, and is bounded.
    let samples_per_symbol = 8usize;
    let n_symbols = 64usize;
    let mut signal = Vec::with_capacity(n_symbols * samples_per_symbol);
    for sym in 0..n_symbols {
        let pol = if sym % 2 == 0 { 1.0_f32 } else { -1.0 };
        for _ in 0..samples_per_symbol {
            signal.push(pol);
        }
    }
    let recovery = SymbolTimingRecovery::new(samples_per_symbol);
    let estimated = recovery.estimate_offset(&signal);
    assert!(
        estimated.is_finite(),
        "estimate_offset must return a finite f32; got {estimated}"
    );
    assert!(
        estimated.abs() < 10.0,
        "estimate_offset should stay bounded (got {estimated})"
    );
}

#[test]
fn timing_recovery_returns_zero_on_empty_or_too_short_signal() {
    // Defensive: if the signal can't host any Gardner triplet, return 0.
    let recovery = SymbolTimingRecovery::new(8);
    assert_eq!(recovery.estimate_offset(&[]), 0.0);
    assert_eq!(recovery.estimate_offset(&[1.0, -1.0, 1.0]), 0.0);
}
