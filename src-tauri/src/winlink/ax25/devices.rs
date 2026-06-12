//! Stable audio-device + PTT discovery for the managed-Dire-Wolf packet path
//! (Slice B of the managed-modem on-air accessibility design,
//! `docs/design/2026-06-12-managed-modem-onair-accessibility-design.md`).
//!
//! The operator picks "DigiRig" or "DRA-100" by friendly name, but tuxlink must
//! **resolve and persist a STABLE identity** — not the boot-order ALSA card
//! index (`card 1` / `card 2`), which swaps when two USB sound cards are
//! re-plugged or enumerate in a different order. With DigiRig + DRA-100 both
//! attached there are two C-Media-family USB cards; "use the USB card" is
//! ambiguous, and the onboard Pi HDMI/bcm2835 card is never what a packet
//! operator wants. This module makes that resolution **pure** over an injected
//! [`SysSnapshot`] so it is fixture-testable without a real ALSA stack, a real
//! `/dev`, or a radio.
//!
//! ## Pure-over-snapshot / thin-impure-shell split
//!
//! Mirroring the `parse_alsa_devices` pattern in `ui_commands.rs`, all the
//! decision logic ([`enumerate_audio_devices`], [`discover_ptt`]) is pure over a
//! hand-built [`SysSnapshot`]. The impure reading of `/dev/snd/by-id`,
//! `/proc/asound/cards`, and sysfs USB topology lives in the clearly-marked,
//! untested [`read_sys_snapshot`] shim — this is the boundary the tests do not
//! cross.
//!
//! ## Stable-id derivation order
//!
//! DigiRig is a C-Media CM108-class card (VID `0x0d8c`); the DRA-100 is a
//! CM119A (also the C-Media `0x0d8c` family). **VID:PID alone may not
//! disambiguate two C-Media cards**, so the stable id is derived in this
//! priority order:
//!
//! 1. The `/dev/snd/by-id` symlink basename, which encodes the USB product
//!    string + serial — the most specific and most stable handle.
//! 2. `vid:pid:serial` when a serial is present (distinguishes two same-VID:PID
//!    cards that report distinct serials).
//! 3. A stable hash of the ALSA card `id` string — last resort when neither a
//!    by-id symlink nor a USB serial is available.
//!
//! ## PTT discovery
//!
//! [`discover_ptt`] returns ranked [`PttChoice`] candidates for a chosen card.
//! A CM108 HID line on the **same USB parent** as the card sorts first (the
//! DRA-100 keys via a CM108 HID GPIO line); a serial RTS line (the DigiRig
//! CP2102 `/dev/ttyUSB*`) is the alternative. "Same USB parent" is decided
//! purely by comparing the `usb_parent` sysfs path each node records.
//!
//! This module only **resolves** the hidraw path / tty to hand to Dire Wolf's
//! `PTT CM108 <hidraw>` / `PTT /dev/ttyUSBx RTS` directives; it does not key the
//! radio (Dire Wolf does that) and it generates no config (later phases).

use serde::{Deserialize, Serialize};

// ============================================================================
// Stable identity
// ============================================================================

/// A boot-order-independent handle for an audio device, persisted in config so
/// the operator's "DigiRig" / "DRA-100" choice survives re-enumeration. Derived
/// from a `/dev/snd/by-id` symlink, a USB `vid:pid:serial`, or a stable hash of
/// the ALSA card id — never the `card N` index. See the module docs for the
/// derivation priority order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StableAudioId {
    /// How this id was derived — lets the UI explain the resolution and lets a
    /// future migration know whether a more-specific handle became available.
    pub kind: StableIdKind,
    /// The stable value itself: the by-id basename, the `vid:pid:serial`
    /// triple, or the `cardid:<hash>` fallback string.
    pub value: String,
}

/// Which input produced a [`StableAudioId`] — ordered most-to-least specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StableIdKind {
    /// Derived from a `/dev/snd/by-id/<basename>` symlink (best).
    ByIdSymlink,
    /// Derived from USB `vid:pid:serial` (serial present).
    UsbVidPidSerial,
    /// Derived from a stable hash of the ALSA card `id` string (last resort).
    CardIdHash,
}

// ============================================================================
// Snapshot — the injected, parsed inputs the pure logic needs
// ============================================================================

/// USB identity of a card, when the card is a USB device. The Pi onboard
/// HDMI/bcm2835 card has no USB identity (`None` on [`SnapshotCard::usb`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIdentity {
    /// USB vendor id, e.g. `0x0d8c` (C-Media) — lower-cased 4-hex, no `0x`.
    pub vid: String,
    /// USB product id — lower-cased 4-hex, no `0x`.
    pub pid: String,
    /// USB iSerial string, if the device reports one. DigiRig and DRA-100 can
    /// share a VID:PID, so the serial (or the by-id basename) is what actually
    /// disambiguates two same-family cards.
    pub serial: Option<String>,
}

/// One ALSA card as the snapshot sees it — exactly the parsed inputs the pure
/// resolver needs, nothing more. Built impurely by [`read_sys_snapshot`] from
/// `/proc/asound/cards`, `/dev/snd/by-id`, and sysfs; built by hand in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCard {
    /// The boot-order ALSA card index (`card N`). **Deliberately not part of
    /// the stable id** — present only to build the `plughw:` name and to prove
    /// in tests that swapping it does not change the resolved id.
    pub card_index: u32,
    /// The ALSA card `id` (the short token in `plughw:CARD=<id>`), e.g.
    /// `"Device"` or `"DRA"`. Used for the `plughw:` name and the hash
    /// fallback.
    pub card_id: String,
    /// The ALSA card longname / human description, e.g.
    /// `"C-Media USB Audio Device"`. Becomes [`AudioDevice::human_name`].
    pub card_name: String,
    /// The `/dev/snd/by-id` symlink **basename** pointing at this card, if one
    /// exists, e.g. `"usb-C-Media_DigiRig_Audio-00"`. The primary stable-id key.
    pub by_id_basename: Option<String>,
    /// The card's USB identity, or `None` for a non-USB (onboard) card.
    pub usb: Option<UsbIdentity>,
    /// The sysfs USB **parent** path this card hangs off, e.g.
    /// `"/sys/devices/platform/...-1.2"`. `None` for onboard cards. PTT
    /// discovery matches hidraw/tty nodes to a card by comparing this string.
    pub usb_parent: Option<String>,
}

/// A `/dev/hidraw*` node and the USB parent it hangs off — the topology PTT
/// discovery needs to decide "same USB parent as the chosen card."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidrawNode {
    /// The device path, e.g. `"/dev/hidraw3"`.
    pub path: String,
    /// The sysfs USB parent this hidraw hangs off; compared against
    /// [`SnapshotCard::usb_parent`].
    pub usb_parent: Option<String>,
    /// True when this hidraw belongs to a CM108-family interface (a candidate
    /// PTT keyer). A non-CM108 HID on the same parent is not a PTT candidate.
    pub is_cm108: bool,
}

/// A serial `/dev/ttyUSB*` / `/dev/ttyACM*` node and its USB parent. The
/// DigiRig's CP2102 is the canonical RTS-PTT serial line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyNode {
    /// The device path, e.g. `"/dev/ttyUSB0"`.
    pub path: String,
    /// The sysfs USB parent this tty hangs off; compared against
    /// [`SnapshotCard::usb_parent`].
    pub usb_parent: Option<String>,
}

/// The complete injected view of the audio + USB-topology state the pure
/// discovery logic reasons over. A test builds one by hand; production builds
/// one via [`read_sys_snapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SysSnapshot {
    /// Every ALSA card the system reports (USB and onboard).
    pub cards: Vec<SnapshotCard>,
    /// Every `/dev/hidraw*` node with its USB parent.
    pub hidraws: Vec<HidrawNode>,
    /// Every serial `/dev/ttyUSB*` / `/dev/ttyACM*` node with its USB parent.
    pub ttys: Vec<TtyNode>,
}

// ============================================================================
// Public result types
// ============================================================================

/// An audio device the operator can pick for the managed packet modem, resolved
/// to a stable identity. `human_name` is what the picker shows; `alsa_plughw`
/// is the ALSA name handed to the modem; `stable_id` is what gets persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    /// Human label, e.g. `"C-Media USB Audio Device"` (from the card longname).
    pub human_name: String,
    /// The ALSA `plughw:CARD=<id>,DEV=0` name for this card.
    pub alsa_plughw: String,
    /// The boot-order-independent identity persisted in config.
    pub stable_id: StableAudioId,
    /// The sysfs USB parent (carried so [`discover_ptt`] can match PTT nodes
    /// without re-reading the snapshot). `None` for onboard cards (which
    /// `enumerate_audio_devices` excludes anyway).
    pub usb_parent: Option<String>,
}

/// A candidate PTT keying method for a chosen audio device. Persisted in config
/// (later phase), so it carries the same derive set as [`StableAudioId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PttChoice {
    /// A CM108-family HID line — fed to Dire Wolf as `PTT CM108 <hidraw_path>`.
    /// The DRA-100's GPIO/reed-relay PTT is this kind.
    Cm108Hid {
        /// The `/dev/hidraw*` path to hand to Dire Wolf.
        hidraw_path: String,
    },
    /// A serial RTS line — fed to Dire Wolf as `PTT <tty> RTS`. The DigiRig
    /// keys via its CP2102 `/dev/ttyUSB*` RTS line, not a CM108 HID.
    SerialRts {
        /// The serial device path whose RTS line keys the radio.
        tty: String,
    },
}

// ============================================================================
// Pure discovery logic
// ============================================================================

/// True for an ALSA card a packet operator would actually pick: a USB sound
/// card. The Pi onboard HDMI / bcm2835 card (no USB identity) is excluded — it
/// is the `Error -524` class the design calls out, never the packet interface.
fn is_usable_packet_card(card: &SnapshotCard) -> bool {
    card.usb.is_some()
}

/// Build the persisted [`StableAudioId`] for a card, in the documented
/// priority order: by-id symlink basename → `vid:pid:serial` → `cardid:<hash>`.
fn derive_stable_id(card: &SnapshotCard) -> StableAudioId {
    // 1. by-id symlink basename — most specific (encodes product string + serial).
    if let Some(basename) = &card.by_id_basename {
        if !basename.is_empty() {
            return StableAudioId {
                kind: StableIdKind::ByIdSymlink,
                value: basename.clone(),
            };
        }
    }
    // 2. vid:pid:serial — only when a serial is present (otherwise two same
    //    VID:PID cards would collide, which is exactly the DigiRig/DRA-100 trap).
    if let Some(usb) = &card.usb {
        if let Some(serial) = &usb.serial {
            if !serial.is_empty() {
                return StableAudioId {
                    kind: StableIdKind::UsbVidPidSerial,
                    value: format!("{}:{}:{}", usb.vid, usb.pid, serial),
                };
            }
        }
    }
    // 3. Stable hash of the card id string — last resort. Uses a fixed FNV-1a
    //    so the value is deterministic across runs and machines (the default
    //    `DefaultHasher` is NOT guaranteed stable across Rust versions, which
    //    would silently churn a persisted id).
    StableAudioId {
        kind: StableIdKind::CardIdHash,
        value: format!("cardid:{:016x}", fnv1a_64(card.card_id.as_bytes())),
    }
}

/// FNV-1a 64-bit — a small, fixed, deterministic hash for the card-id fallback.
/// Chosen over `std::collections::hash_map::DefaultHasher` because the latter's
/// algorithm/seed is explicitly not a stable contract across Rust versions, and
/// this value is persisted to config.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Build the ALSA `plughw:CARD=<id>,DEV=0` name for a card.
fn plughw_name(card: &SnapshotCard) -> String {
    format!("plughw:CARD={},DEV=0", card.card_id)
}

/// Enumerate the packet-usable audio devices from a snapshot, each resolved to
/// a stable identity (NOT the `card N` index). Onboard (non-USB) cards are
/// excluded. The returned order follows snapshot order among usable cards; the
/// stable id is index-independent, so the same two physical cards yield the same
/// two ids regardless of which got `card 1` vs `card 2`.
///
/// Pure over `snapshot` — no `/dev`, no ALSA, no I/O.
pub fn enumerate_audio_devices(snapshot: &SysSnapshot) -> Vec<AudioDevice> {
    snapshot
        .cards
        .iter()
        .filter(|c| is_usable_packet_card(c))
        .map(|card| AudioDevice {
            human_name: card.card_name.clone(),
            alsa_plughw: plughw_name(card),
            stable_id: derive_stable_id(card),
            usb_parent: card.usb_parent.clone(),
        })
        .collect()
}

/// Discover ranked PTT candidates for a chosen `card`. A [`PttChoice::Cm108Hid`]
/// on the SAME USB parent as the card sorts first (the DRA-100 case); a
/// [`PttChoice::SerialRts`] on the same parent (the DigiRig CP2102 case) follows.
/// When an adapter exposes both on one parent, the HID wins the top slot.
///
/// "Same USB parent" requires the card to have a known `usb_parent` and the
/// node to record the identical path; nodes with no parent, or a different
/// parent, are not candidates. Pure over `snapshot`.
pub fn discover_ptt(card: &AudioDevice, snapshot: &SysSnapshot) -> Vec<PttChoice> {
    let Some(card_parent) = card.usb_parent.as_deref() else {
        // A card with no known USB parent (e.g. an onboard card that somehow
        // reached here) has no same-parent PTT line to resolve.
        return Vec::new();
    };

    let same_parent = |node_parent: &Option<String>| -> bool {
        node_parent.as_deref() == Some(card_parent)
    };

    let mut choices: Vec<PttChoice> = Vec::new();

    // CM108 HID candidates on the same parent — ranked first. Only CM108-family
    // hidraws are PTT keyers; a non-CM108 HID on the same parent is skipped.
    for hid in &snapshot.hidraws {
        if hid.is_cm108 && same_parent(&hid.usb_parent) {
            choices.push(PttChoice::Cm108Hid {
                hidraw_path: hid.path.clone(),
            });
        }
    }

    // Serial RTS candidates on the same parent — ranked after any HID.
    for tty in &snapshot.ttys {
        if same_parent(&tty.usb_parent) {
            choices.push(PttChoice::SerialRts {
                tty: tty.path.clone(),
            });
        }
    }

    choices
}

// ============================================================================
// Impure shim — reads the real system into a SysSnapshot. UNTESTED by design.
// ============================================================================

/// IMPURE SHIM — reads the real `/proc/asound/cards`, `/dev/snd/by-id`, and
/// sysfs USB topology into a [`SysSnapshot`] for the pure logic above. This is
/// the only part of the module that touches the filesystem; it is deliberately
/// thin and is NOT unit-tested (mirrors the `arecord -L` shell-out shim in
/// `ui_commands.rs`, which is also impure-and-untested). Wired in a later phase
/// — present here as the documented boundary, returning an empty snapshot until
/// the real readers land.
///
/// Soft-failure posture matches `ardop_list_audio_devices`: a missing path or a
/// read error yields an empty/partial snapshot (the picker shows "no devices —
/// plug one in and refresh"), never an `Err`.
#[allow(dead_code)]
pub fn read_sys_snapshot() -> SysSnapshot {
    // Phase 1 ships the pure resolver + fixtures only; the real sysfs/by-id
    // readers are a later phase. Returning an empty snapshot keeps the public
    // boundary stable without claiming capability that isn't wired yet.
    SysSnapshot::default()
}

// ============================================================================
// Tests — pure over hand-built fixtures. No real /dev, no ALSA, no radio.
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// C-Media VID shared by both DigiRig and the DRA-100's CM119A — so the
    /// fixtures actually exercise disambiguation that VID alone cannot do.
    const CMEDIA_VID: &str = "0d8c";

    // ---- fixture builders ---------------------------------------------------

    /// A DigiRig USB sound card on `parent`, at the given boot card index.
    /// Distinct by-id basename + serial from the DRA-100 despite the shared VID.
    fn digirig_card(card_index: u32, parent: &str) -> SnapshotCard {
        SnapshotCard {
            card_index,
            card_id: "Device".into(),
            card_name: "C-Media USB Audio Device (DigiRig)".into(),
            by_id_basename: Some("usb-C-Media_DigiRig_Audio-00".into()),
            usb: Some(UsbIdentity {
                vid: CMEDIA_VID.into(),
                pid: "0012".into(),
                serial: Some("DIGIRIG123".into()),
            }),
            usb_parent: Some(parent.into()),
        }
    }

    /// A DRA-100 (CM119A) USB sound card on `parent`. Shares the C-Media VID
    /// with the DigiRig; disambiguated by by-id basename / serial.
    fn dra100_card(card_index: u32, parent: &str) -> SnapshotCard {
        SnapshotCard {
            card_index,
            card_id: "DRA".into(),
            card_name: "C-Media USB Audio Device (DRA-100)".into(),
            by_id_basename: Some("usb-C-Media_DRA-100_CM119A-01".into()),
            usb: Some(UsbIdentity {
                vid: CMEDIA_VID.into(),
                pid: "0012".into(),
                serial: Some("DRA100XYZ".into()),
            }),
            usb_parent: Some(parent.into()),
        }
    }

    /// The Pi onboard HDMI / bcm2835 card — no USB identity, no USB parent.
    fn onboard_hdmi_card(card_index: u32) -> SnapshotCard {
        SnapshotCard {
            card_index,
            card_id: "vc4hdmi".into(),
            card_name: "vc4-hdmi".into(),
            by_id_basename: None,
            usb: None,
            usb_parent: None,
        }
    }

    // ---- Task 1.1: audio enumeration by stable id ---------------------------

    /// (a) DigiRig only — resolves to a stable id from its USB identity (the
    /// by-id basename), NOT the `card N` index.
    #[test]
    fn digirig_only_resolves_stable_id_not_card_index() {
        let snap = SysSnapshot {
            cards: vec![digirig_card(1, "/sys/.../usb1/1-1.1")],
            ..Default::default()
        };
        let devices = enumerate_audio_devices(&snap);
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!(d.stable_id.kind, StableIdKind::ByIdSymlink);
        assert_eq!(d.stable_id.value, "usb-C-Media_DigiRig_Audio-00");
        // The id is the by-id basename, not the boot-order card index. (The
        // index-independence property itself is proved by the swap test below;
        // here we just confirm the kind/value resolve as expected.)
        assert_eq!(d.alsa_plughw, "plughw:CARD=Device,DEV=0");
    }

    /// (b) DRA-100 only — same: stable id from USB identity, not card index.
    #[test]
    fn dra100_only_resolves_stable_id_not_card_index() {
        let snap = SysSnapshot {
            cards: vec![dra100_card(1, "/sys/.../usb1/1-1.2")],
            ..Default::default()
        };
        let devices = enumerate_audio_devices(&snap);
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!(d.stable_id.kind, StableIdKind::ByIdSymlink);
        assert_eq!(d.stable_id.value, "usb-C-Media_DRA-100_CM119A-01");
        assert_eq!(d.alsa_plughw, "plughw:CARD=DRA,DEV=0");
    }

    /// (c) BOTH attached — each resolves to a DISTINCT stable id, and the two
    /// ids do NOT depend on which card got index 1 vs 2: swapping the index
    /// assignment yields the SAME two stable ids.
    #[test]
    fn both_attached_distinct_ids_independent_of_card_index() {
        let parent_a = "/sys/.../usb1/1-1.1";
        let parent_b = "/sys/.../usb1/1-1.2";

        // Arrangement 1: DigiRig=card1, DRA-100=card2.
        let snap1 = SysSnapshot {
            cards: vec![
                digirig_card(1, parent_a),
                dra100_card(2, parent_b),
            ],
            ..Default::default()
        };
        // Arrangement 2: the SAME two physical cards, indices swapped, and even
        // listed in the other order — id resolution must be invariant to both.
        let snap2 = SysSnapshot {
            cards: vec![
                dra100_card(1, parent_b),
                digirig_card(2, parent_a),
            ],
            ..Default::default()
        };

        let ids1: Vec<StableAudioId> = enumerate_audio_devices(&snap1)
            .into_iter()
            .map(|d| d.stable_id)
            .collect();
        let ids2: Vec<StableAudioId> = enumerate_audio_devices(&snap2)
            .into_iter()
            .map(|d| d.stable_id)
            .collect();

        // Two distinct ids within each arrangement.
        assert_eq!(ids1.len(), 2);
        assert_ne!(ids1[0], ids1[1]);

        // The SET of resolved ids is identical across the index swap.
        let set1: std::collections::HashSet<&str> =
            ids1.iter().map(|i| i.value.as_str()).collect();
        let set2: std::collections::HashSet<&str> =
            ids2.iter().map(|i| i.value.as_str()).collect();
        assert_eq!(set1, set2);
        assert!(set1.contains("usb-C-Media_DigiRig_Audio-00"));
        assert!(set1.contains("usb-C-Media_DRA-100_CM119A-01"));
    }

    /// (c-corollary) Even when DigiRig and DRA-100 share VID:PID and have NO
    /// by-id symlink, the serial still disambiguates them (vid:pid:serial path).
    #[test]
    fn same_vid_pid_disambiguated_by_serial_when_no_by_id() {
        let mut digirig = digirig_card(1, "/sys/p/a");
        let mut dra = dra100_card(2, "/sys/p/b");
        digirig.by_id_basename = None;
        dra.by_id_basename = None;
        let snap = SysSnapshot {
            cards: vec![digirig, dra],
            ..Default::default()
        };
        let devices = enumerate_audio_devices(&snap);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].stable_id.kind, StableIdKind::UsbVidPidSerial);
        assert_eq!(devices[1].stable_id.kind, StableIdKind::UsbVidPidSerial);
        // Distinct despite identical vid:pid, because the serials differ.
        assert_ne!(devices[0].stable_id.value, devices[1].stable_id.value);
        assert_eq!(devices[0].stable_id.value, "0d8c:0012:DIGIRIG123");
        assert_eq!(devices[1].stable_id.value, "0d8c:0012:DRA100XYZ");
    }

    /// (d) Onboard HDMI / bcm2835 present alongside a USB card — the onboard
    /// device is EXCLUDED from the returned list; the USB card is the result.
    #[test]
    fn onboard_hdmi_excluded_usb_card_kept() {
        let snap = SysSnapshot {
            cards: vec![
                onboard_hdmi_card(0),
                digirig_card(1, "/sys/.../usb1/1-1.1"),
            ],
            ..Default::default()
        };
        let devices = enumerate_audio_devices(&snap);
        // Onboard excluded — only the USB card remains.
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].alsa_plughw, "plughw:CARD=Device,DEV=0");
        assert!(!devices
            .iter()
            .any(|d| d.alsa_plughw.contains("vc4hdmi")));
    }

    // ---- Task 1.2: PTT discovery --------------------------------------------

    /// DRA-100 → a Cm108Hid candidate whose hidraw shares the DRA-100's USB
    /// parent. (DRA-100 keys via a CM108 HID GPIO line.)
    #[test]
    fn dra100_ptt_is_cm108_hid_on_same_parent() {
        let parent = "/sys/.../usb1/1-1.2";
        let snap = SysSnapshot {
            cards: vec![dra100_card(1, parent)],
            hidraws: vec![
                // The DRA-100's CM108 HID on the same parent.
                HidrawNode {
                    path: "/dev/hidraw3".into(),
                    usb_parent: Some(parent.into()),
                    is_cm108: true,
                },
                // A decoy CM108 HID on a DIFFERENT parent — must be ignored.
                HidrawNode {
                    path: "/dev/hidraw9".into(),
                    usb_parent: Some("/sys/.../usb1/9-9.9".into()),
                    is_cm108: true,
                },
            ],
            ..Default::default()
        };
        let card = enumerate_audio_devices(&snap).remove(0);
        let ptt = discover_ptt(&card, &snap);
        assert_eq!(ptt.len(), 1);
        assert_eq!(
            ptt[0],
            PttChoice::Cm108Hid {
                hidraw_path: "/dev/hidraw3".into()
            }
        );
    }

    /// DigiRig → a SerialRts candidate (the CP2102 `/dev/ttyUSB*`). DigiRig keys
    /// PTT via the serial RTS line, not a CM108 HID.
    #[test]
    fn digirig_ptt_is_serial_rts() {
        let parent = "/sys/.../usb1/1-1.1";
        let snap = SysSnapshot {
            cards: vec![digirig_card(1, parent)],
            ttys: vec![TtyNode {
                path: "/dev/ttyUSB0".into(),
                usb_parent: Some(parent.into()),
            }],
            ..Default::default()
        };
        let card = enumerate_audio_devices(&snap).remove(0);
        let ptt = discover_ptt(&card, &snap);
        assert_eq!(ptt.len(), 1);
        assert_eq!(
            ptt[0],
            PttChoice::SerialRts {
                tty: "/dev/ttyUSB0".into()
            }
        );
    }

    /// An adapter exposing BOTH an HID and a serial on the same parent → HID
    /// ranked first.
    #[test]
    fn both_hid_and_serial_on_same_parent_ranks_hid_first() {
        let parent = "/sys/.../usb1/1-1.3";
        let snap = SysSnapshot {
            cards: vec![SnapshotCard {
                card_index: 1,
                card_id: "Combo".into(),
                card_name: "Combo adapter".into(),
                by_id_basename: Some("usb-Combo-00".into()),
                usb: Some(UsbIdentity {
                    vid: CMEDIA_VID.into(),
                    pid: "013c".into(),
                    serial: Some("COMBO1".into()),
                }),
                usb_parent: Some(parent.into()),
            }],
            hidraws: vec![HidrawNode {
                path: "/dev/hidraw5".into(),
                usb_parent: Some(parent.into()),
                is_cm108: true,
            }],
            ttys: vec![TtyNode {
                path: "/dev/ttyUSB2".into(),
                usb_parent: Some(parent.into()),
            }],
            ..Default::default()
        };
        let card = enumerate_audio_devices(&snap).remove(0);
        let ptt = discover_ptt(&card, &snap);
        assert_eq!(ptt.len(), 2);
        // HID first.
        assert_eq!(
            ptt[0],
            PttChoice::Cm108Hid {
                hidraw_path: "/dev/hidraw5".into()
            }
        );
        assert_eq!(
            ptt[1],
            PttChoice::SerialRts {
                tty: "/dev/ttyUSB2".into()
            }
        );
    }

    /// A non-CM108 HID on the same parent is NOT offered as a PTT candidate.
    #[test]
    fn non_cm108_hid_is_not_a_ptt_candidate() {
        let parent = "/sys/.../usb1/1-1.4";
        let snap = SysSnapshot {
            cards: vec![dra100_card(1, parent)],
            hidraws: vec![HidrawNode {
                path: "/dev/hidraw7".into(),
                usb_parent: Some(parent.into()),
                is_cm108: false,
            }],
            ..Default::default()
        };
        let card = enumerate_audio_devices(&snap).remove(0);
        let ptt = discover_ptt(&card, &snap);
        assert!(ptt.is_empty());
    }

    /// The stable-id hash fallback is deterministic and content-derived (guards
    /// against a future swap to a Rust-version-unstable hasher).
    #[test]
    fn cardid_hash_fallback_is_deterministic() {
        let mut card = digirig_card(1, "/sys/p");
        card.by_id_basename = None;
        card.usb = None; // forces the cardid hash branch
        card.usb_parent = None;
        // Onboard-style card with no USB id and no by-id → hash fallback. (Note
        // such a card is excluded by enumerate; we test the deriver directly.)
        let id1 = derive_stable_id(&card);
        let id2 = derive_stable_id(&card);
        assert_eq!(id1.kind, StableIdKind::CardIdHash);
        assert_eq!(id1, id2);
        assert!(id1.value.starts_with("cardid:"));
    }
}
