//! Dockable surfaces — shell capability (Routines plan 6/6, bd tuxlink-dmwte).
//! Spec: docs/superpowers/specs/2026-07-15-dockable-surfaces-design.md §3.
//! The wire-contract table in spec §3 is NORMATIVE; the strings below are
//! copied from it, never derived (label/route drop the underscore).
//!
//! `SurfaceId::StationIntelligence` (bd tuxlink-9obx2) extends the registry
//! the spec's §9 non-goals section named as the growth path: "wiring another
//! surface is adding a registry entry plus its pathway affordances" (Elmer,
//! bd tuxlink-mfssz, was the fourth; Station Intelligence is the fifth).
//! Its window label diverges from the drop-the-underscore form the other
//! surfaces use (`pop-station-intelligence`, not `pop-stationintelligence`),
//! deliberately, since the wire table is a lookup, not a formula (spec
//! §3), and the two-word compound reads better hyphenated.

pub mod commands;
pub mod park_notify;
pub mod registry;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceId {
    Routines,
    TacMap,
    AprsChat,
    Elmer,
    StationIntelligence,
}

impl SurfaceId {
    pub const ALL: [SurfaceId; 5] = [
        SurfaceId::Routines,
        SurfaceId::TacMap,
        SurfaceId::AprsChat,
        SurfaceId::Elmer,
        SurfaceId::StationIntelligence,
    ];

    pub fn window_label(self) -> &'static str {
        match self {
            SurfaceId::Routines => "pop-routines",
            SurfaceId::TacMap => "pop-tacmap",
            SurfaceId::AprsChat => "pop-aprschat",
            SurfaceId::Elmer => "pop-elmer",
            SurfaceId::StationIntelligence => "pop-station-intelligence",
        }
    }

    pub fn route(self) -> &'static str {
        match self {
            SurfaceId::Routines => "/pop/routines",
            SurfaceId::TacMap => "/pop/tacmap",
            SurfaceId::AprsChat => "/pop/aprschat",
            SurfaceId::Elmer => "/pop/elmer",
            SurfaceId::StationIntelligence => "/pop/station-intelligence",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            SurfaceId::Routines => "Routines — Tuxlink",
            SurfaceId::TacMap => "Tac Map — Tuxlink",
            SurfaceId::AprsChat => "APRS Chat — Tuxlink",
            SurfaceId::Elmer => "Elmer — Tuxlink",
            SurfaceId::StationIntelligence => "Station Intelligence - Tuxlink",
        }
    }

    pub fn from_window_label(label: &str) -> Option<SurfaceId> {
        SurfaceId::ALL.into_iter().find(|s| s.window_label() == label)
    }

    /// Dense `0..5` index for this surface, matching [`SurfaceId::ALL`]'s order
    /// (`Routines`=0, `TacMap`=1, `AprsChat`=2, `Elmer`=3,
    /// `StationIntelligence`=4). Used to index the registry's per-surface
    /// pop-generation array the same way [`DockSurfaces`] maps a surface to
    /// its field.
    pub(crate) fn index(self) -> usize {
        match self {
            SurfaceId::Routines => 0,
            SurfaceId::TacMap => 1,
            SurfaceId::AprsChat => 2,
            SurfaceId::Elmer => 3,
            SurfaceId::StationIntelligence => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DockMode {
    #[default]
    Docked,
    Popped,
}

/// The persisted half of the snapshot — this exact shape is the config `dock`
/// section (spec §3 JSON literal; Task 2). `station_intelligence` (bd
/// tuxlink-9obx2) is additive: `#[serde(default)]` migrates pre-field configs
/// (absent → `Docked`), which is why adding it bumps `CONFIG_SCHEMA_VERSION`
/// 8 → 9 (config.rs); `deny_unknown_fields` below means an older binary
/// would otherwise fail outright on a config a newer build wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DockSurfaces {
    #[serde(default)]
    pub routines: DockMode,
    #[serde(default)]
    pub tac_map: DockMode,
    #[serde(default)]
    pub aprs_chat: DockMode,
    /// tuxlink-mfssz. `#[serde(default)]` keeps every pre-existing config's
    /// `dock` section parsing (absent key = docked).
    #[serde(default)]
    pub elmer: DockMode,
    /// bd tuxlink-9obx2. Same additive-migration rationale as `elmer` above.
    #[serde(default)]
    pub station_intelligence: DockMode,
}

impl DockSurfaces {
    pub fn get(&self, s: SurfaceId) -> DockMode {
        match s {
            SurfaceId::Routines => self.routines,
            SurfaceId::TacMap => self.tac_map,
            SurfaceId::AprsChat => self.aprs_chat,
            SurfaceId::Elmer => self.elmer,
            SurfaceId::StationIntelligence => self.station_intelligence,
        }
    }
    pub fn set(&mut self, s: SurfaceId, m: DockMode) {
        match s {
            SurfaceId::Routines => self.routines = m,
            SurfaceId::TacMap => self.tac_map = m,
            SurfaceId::AprsChat => self.aprs_chat = m,
            SurfaceId::Elmer => self.elmer = m,
            SurfaceId::StationIntelligence => self.station_intelligence = m,
        }
    }
}

/// Runtime-only continuity tokens (spec §7) — never persisted to config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DockContext {
    pub routines: Option<serde_json::Value>,
    pub tac_map: Option<serde_json::Value>,
    pub aprs_chat: Option<serde_json::Value>,
    #[serde(default)]
    pub elmer: Option<serde_json::Value>,
    #[serde(default)]
    pub station_intelligence: Option<serde_json::Value>,
}

impl DockContext {
    pub fn set(&mut self, s: SurfaceId, v: Option<serde_json::Value>) {
        match s {
            SurfaceId::Routines => self.routines = v,
            SurfaceId::TacMap => self.tac_map = v,
            SurfaceId::AprsChat => self.aprs_chat = v,
            SurfaceId::Elmer => self.elmer = v,
            SurfaceId::StationIntelligence => self.station_intelligence = v,
        }
    }
}

/// The `dock:changed` payload AND the `dock_state_get` return — always the full
/// snapshot (spec §3: windows replace wholesale; a missed event self-heals).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DockSnapshot {
    pub surfaces: DockSurfaces,
    pub context: DockContext,
}

/// Pure transition core (spec §3). Returns true iff the transition was
/// EFFECTIVE (state changed); callers suppress persist + emit on false.
pub fn apply_transition(surfaces: &mut DockSurfaces, surface: SurfaceId, target: DockMode) -> bool {
    if surfaces.get(surface) == target {
        return false;
    }
    surfaces.set(surface, target);
    true
}

/// Consent host resolution (spec §6). Rust is CANONICAL; the TS mirror in
/// src/dock/dockState.ts must match via the shared parity fixture (Task 6).
pub fn consent_host_window(routines_mode: DockMode) -> &'static str {
    match routines_mode {
        DockMode::Docked => "main",
        DockMode::Popped => SurfaceId::Routines.window_label(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wire contract, spec §3 table. Shape test per the standing serde-enum rule:
    /// explicit rename + assert the exact wire strings — the label/route forms
    /// drop the underscore and CANNOT be derived.
    #[test]
    fn surface_id_wire_contract() {
        assert_eq!(serde_json::to_string(&SurfaceId::Routines).unwrap(), "\"routines\"");
        assert_eq!(serde_json::to_string(&SurfaceId::TacMap).unwrap(), "\"tac_map\"");
        assert_eq!(serde_json::to_string(&SurfaceId::AprsChat).unwrap(), "\"aprs_chat\"");
        assert_eq!(
            serde_json::to_string(&SurfaceId::StationIntelligence).unwrap(),
            "\"station_intelligence\""
        );
        assert_eq!(SurfaceId::TacMap.window_label(), "pop-tacmap");
        assert_eq!(SurfaceId::TacMap.route(), "/pop/tacmap");
        assert_eq!(SurfaceId::AprsChat.window_label(), "pop-aprschat");
        assert_eq!(serde_json::to_string(&SurfaceId::Elmer).unwrap(), "\"elmer\"");
        assert_eq!(SurfaceId::Elmer.window_label(), "pop-elmer");
        assert_eq!(SurfaceId::Elmer.route(), "/pop/elmer");
        assert_eq!(SurfaceId::Elmer.title(), "Elmer — Tuxlink");
        assert_eq!(SurfaceId::Routines.title(), "Routines — Tuxlink");
        assert_eq!(SurfaceId::StationIntelligence.window_label(), "pop-station-intelligence");
        assert_eq!(SurfaceId::StationIntelligence.route(), "/pop/station-intelligence");
        assert_eq!(SurfaceId::StationIntelligence.title(), "Station Intelligence - Tuxlink");
        for s in SurfaceId::ALL {
            assert_eq!(SurfaceId::from_window_label(s.window_label()), Some(s));
        }
        assert_eq!(SurfaceId::from_window_label("main"), None);
        assert_eq!(serde_json::to_string(&DockMode::Popped).unwrap(), "\"popped\"");
        // Round-trip: the TS side sends these exact strings as invoke args.
        assert_eq!(serde_json::from_str::<SurfaceId>("\"tac_map\"").unwrap(), SurfaceId::TacMap);
    }

    /// DockSnapshot JSON shape — the dock:changed payload / dock_state_get return
    /// (spec §3 JSON literal). Full snapshot, never deltas.
    #[test]
    fn snapshot_json_shape() {
        let mut snap = DockSnapshot::default();
        snap.surfaces.set(SurfaceId::Routines, DockMode::Popped);
        snap.context.routines = Some(serde_json::json!({"view": "designer"}));
        let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
        assert_eq!(v["surfaces"]["routines"], "popped");
        assert_eq!(v["surfaces"]["tac_map"], "docked");
        assert_eq!(v["surfaces"]["aprs_chat"], "docked");
        assert_eq!(v["surfaces"]["elmer"], "docked");
        assert_eq!(v["surfaces"]["station_intelligence"], "docked");
        assert_eq!(v["context"]["routines"]["view"], "designer");
        assert!(v["context"]["tac_map"].is_null());
        assert!(v["context"]["elmer"].is_null());
        assert!(v["context"]["station_intelligence"].is_null());
    }

    /// Transition core (spec §3): effective vs no-op. No-op MUST return false so
    /// callers suppress persist+emit (double dock-back safety).
    #[test]
    fn transition_effectiveness() {
        let mut s = DockSurfaces::default();
        assert!(apply_transition(&mut s, SurfaceId::TacMap, DockMode::Popped));
        assert_eq!(s.get(SurfaceId::TacMap), DockMode::Popped);
        assert!(!apply_transition(&mut s, SurfaceId::TacMap, DockMode::Popped)); // no-op
        assert!(apply_transition(&mut s, SurfaceId::TacMap, DockMode::Docked));
        assert!(!apply_transition(&mut s, SurfaceId::TacMap, DockMode::Docked)); // double dock-back
        // Other surfaces untouched throughout.
        assert_eq!(s.get(SurfaceId::Routines), DockMode::Docked);
    }

    /// Consent host resolution (spec §6) — Rust is canonical; TS mirrors via the
    /// parity fixture (Task 6).
    #[test]
    fn consent_host_resolution() {
        assert_eq!(consent_host_window(DockMode::Docked), "main");
        assert_eq!(consent_host_window(DockMode::Popped), "pop-routines");
    }

    /// Cross-language wire-shape AND semantic parity (spec §10, the k61j
    /// composed-seam class; adrev R5-F10/F11). Both this test and the
    /// TS-side `dockParity.test.ts` assert against the SAME committed
    /// fixture — `src/dock/dock-wire-fixture.json` — so a drift between the
    /// two languages' understanding of the wire shape fails on whichever
    /// side changed without the other (`Value` equality for the round-trip,
    /// NOT string comparison: whitespace/key order must not matter). This
    /// test ALSO asserts, per variant, that `surfaces.routines` holds the
    /// mode the variant's name promises and that `consent_host_window`
    /// resolves the way `dockParity.test.ts` asserts on the TS side — a
    /// fixture edit that changes either now breaks the Rust side too, not
    /// just TS.
    #[test]
    fn wire_fixture_parity() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../src/dock/dock-wire-fixture.json"))
                .expect("fixture must be valid JSON");
        for (key, want_mode, want_host) in [
            ("routinesDocked", DockMode::Docked, "main"),
            ("routinesPopped", DockMode::Popped, "pop-routines"),
            // tuxlink-mfssz: elmer popped does NOT move the routines consent
            // host — the Elmer approval UX renders inside the pane itself.
            ("elmerPopped", DockMode::Docked, "main"),
        ] {
            let variant = fixture
                .get(key)
                .unwrap_or_else(|| panic!("fixture missing variant {key}"));
            let snap: DockSnapshot = serde_json::from_value(variant.clone())
                .unwrap_or_else(|e| panic!("{key} did not deserialize into DockSnapshot: {e}"));
            let round_tripped = serde_json::to_value(&snap).expect("DockSnapshot must serialize");
            assert_eq!(&round_tripped, variant, "{key} round-trip mismatch");
            assert_eq!(
                snap.surfaces.routines, want_mode,
                "{key}'s surfaces.routines must match what the variant name promises"
            );
            assert_eq!(
                consent_host_window(snap.surfaces.routines),
                want_host,
                "{key} must resolve consent_host_window to {want_host}"
            );
        }
    }
}
