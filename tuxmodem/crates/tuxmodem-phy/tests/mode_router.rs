use tuxmodem_phy::modes::{ModeFamily, ModeHint, ModeTable};

#[test]
fn default_mode_table_has_two_families() {
    let table = ModeTable::default();
    let families = table.distinct_families();
    assert!(families.contains(&ModeFamily::OfdmMain));
    assert!(families.contains(&ModeFamily::RobustnessFloor));
}

#[test]
fn floor_family_default_is_wideband_lowdensity_not_fsk() {
    let table = ModeTable::default();
    let hint = ModeHint::Floor;
    let resolved = table.resolve(hint, None);
    assert_eq!(resolved.family(), ModeFamily::RobustnessFloor);
    // Per overview §5.A.1: default robustness mode is the wide-band
    // low-density OFDM, NOT narrow-FSK. Narrow-FSK is situational.
    assert_eq!(resolved.short_name(), "floor-wblo");
}

#[test]
fn narrow_fsk_only_resolves_when_hinted_crowded_band() {
    let table = ModeTable::default();
    let resolved = table.resolve(ModeHint::FloorCrowdedBand, None);
    assert_eq!(resolved.short_name(), "floor-nfsk");
}
