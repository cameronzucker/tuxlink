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
    /// The LIVE boot-order `card<N>` index backing this device. Carried so
    /// [`resolve_managed_device`] can hand the live index to the lifecycle layer
    /// (`ManagedDireWolfCfg::card_index`, which the device-busy / release probes
    /// read `/proc/asound/card<N>/...` with) WITHOUT re-walking the snapshot. The
    /// stable id deliberately excludes this index; it is present here only as the
    /// live handle, resolved fresh at connect time, never persisted.
    #[serde(skip)]
    pub card_index: u32,
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
            card_index: card.card_index,
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

    let same_parent =
        |node_parent: &Option<String>| -> bool { node_parent.as_deref() == Some(card_parent) };

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
// Managed-device resolution — stable id → live plughw + card index (PURE)
// ============================================================================

/// The live handles the lifecycle layer needs to bring up a managed Dire Wolf
/// against a previously-persisted [`StableAudioId`]: the ALSA `plughw:` name for
/// `ADEVICE` and the boot-order `card<N>` index the device-busy / release probes
/// read `/proc/asound/card<N>/...` with. Both are resolved FRESH at connect time
/// — the stable id is what persists; this is its live projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedManagedDevice {
    /// The live ALSA `plughw:CARD=<id>,DEV=0` name for `ADEVICE`.
    pub alsa_plughw: String,
    /// The live boot-order `card<N>` index backing `alsa_plughw`.
    pub card_index: u32,
}

/// Resolve a persisted [`StableAudioId`] against a LIVE [`SysSnapshot`] to its
/// current `plughw:` name + `card<N>` index. Returns `None` when no enumerated
/// device carries that stable id — the device-unplugged case the caller surfaces
/// as a clear "configured sound card not found" error rather than spawning Dire
/// Wolf against the wrong card.
///
/// Pure over `snapshot` — the impure live read is the caller's
/// [`read_sys_snapshot`]. The match is on the STABLE id (index-independent), so a
/// re-plug that swaps `card 1`/`card 2` still resolves to the same physical card,
/// now reporting whichever live index it landed on.
pub fn resolve_managed_device(
    stable_id: &StableAudioId,
    snapshot: &SysSnapshot,
) -> Option<ResolvedManagedDevice> {
    enumerate_audio_devices(snapshot)
        .into_iter()
        .find(|d| d.stable_id == *stable_id)
        .map(|d| ResolvedManagedDevice {
            alsa_plughw: d.alsa_plughw,
            card_index: d.card_index,
        })
}

// ============================================================================
// Impure shim — reads the real system into a SysSnapshot. UNTESTED by design.
// ============================================================================

/// IMPURE SHIM — reads the real `/proc/asound/cards`, `/dev/snd/by-id`, and
/// sysfs USB topology into a [`SysSnapshot`] for the pure logic above. This is
/// the only part of the module that touches the filesystem; it is deliberately
/// thin and is NOT unit-tested (mirrors the `arecord -L` shell-out shim in
/// `ui_commands.rs`, which is also impure-and-untested). Every PARSE step it
/// delegates to a pure, fixture-tested helper below ([`parse_proc_asound_cards`],
/// [`card_index_from_symlink_target`], [`hex4_from_sysfs`], [`is_cm108_usb`]); the
/// shim itself only does the I/O the tests cannot.
///
/// Soft-failure posture matches `ardop_list_audio_devices`: a missing path or a
/// read error yields an empty/partial snapshot (the picker shows "no devices —
/// plug one in and refresh"), never an `Err` and never a panic. Each sub-read is
/// independently best-effort: an unreadable `/dev/snd/by-id` still yields cards
/// (just without by-id basenames), an unreadable sysfs node just leaves that
/// card's USB identity `None`.
///
/// OPERATOR-SMOKE-VALIDATED: the filesystem layout this walks (`/proc/asound`,
/// `/dev/snd/by-id` symlink shapes, sysfs USB attribute placement) cannot be
/// exercised in a unit test without a real ALSA/USB stack, so the reader's
/// correctness against a live DigiRig + DRA-100 is confirmed by the operator's
/// on-air smoke, not by CI. The pure parse helpers below ARE fixture-tested.
#[allow(dead_code)]
pub fn read_sys_snapshot() -> SysSnapshot {
    use std::fs;

    // 1. Cards from /proc/asound/cards (index, id, longname). Pure-parsed.
    let mut cards: Vec<SnapshotCard> = match fs::read_to_string("/proc/asound/cards") {
        Ok(text) => parse_proc_asound_cards(&text)
            .into_iter()
            .map(|(card_index, card_id, card_name)| SnapshotCard {
                card_index,
                card_id,
                card_name,
                by_id_basename: None,
                usb: None,
                usb_parent: None,
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    // 2. /dev/snd/by-id symlink basenames → card index (resolve symlink target,
    //    extract card<N> via the pure helper). Best-effort.
    if let Ok(entries) = fs::read_dir("/dev/snd/by-id") {
        for entry in entries.flatten() {
            let basename = entry.file_name().to_string_lossy().into_owned();
            // The symlink target points at e.g. ../controlC1 / ../pcmC1D0c.
            let Ok(target) = fs::read_link(entry.path()) else {
                continue;
            };
            let target_str = target.to_string_lossy();
            if let Some(idx) = card_index_from_symlink_target(&target_str) {
                if let Some(card) = cards.iter_mut().find(|c| c.card_index == idx) {
                    // First control-node symlink wins (don't overwrite with a later pcm one).
                    if card.by_id_basename.is_none() {
                        card.by_id_basename = Some(basename);
                    }
                }
            }
        }
    }

    // 3. sysfs USB topology per card: idVendor/idProduct/serial + USB parent.
    //    /sys/class/sound/card<N>/device is the symlink into the USB tree; the
    //    USB device dir holds idVendor/idProduct/serial, and its PARENT dir is
    //    the hub-port the card hangs off (what PTT discovery matches on).
    for card in cards.iter_mut() {
        let sys_device = format!("/sys/class/sound/card{}/device", card.card_index);
        // Canonicalize the symlink into the real /sys/devices/.../usbX/... path.
        let Ok(usb_dev_dir) = fs::canonicalize(&sys_device) else {
            continue; // onboard cards have no USB device dir → stays None.
        };
        let vid = hex4_from_sysfs(&usb_dev_dir.join("idVendor"));
        let pid = hex4_from_sysfs(&usb_dev_dir.join("idProduct"));
        if let (Some(vid), Some(pid)) = (vid, pid) {
            let serial = fs::read_to_string(usb_dev_dir.join("serial"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            card.usb = Some(UsbIdentity { vid, pid, serial });
            // The USB PARENT (the hub-port path) is what hidraw/tty nodes compare
            // against — the card's own device dir is one level deeper than the
            // shared parent the PTT line also hangs off.
            card.usb_parent = usb_dev_dir
                .parent()
                .map(|p| p.to_string_lossy().into_owned());
        }
    }

    // 4. /dev/hidraw* nodes + CM108 classification, with USB parent.
    let mut hidraws: Vec<HidrawNode> = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/hidraw") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned(); // e.g. "hidraw3"
            let dev_path = format!("/dev/{name}");
            // /sys/class/hidraw/hidrawN/device → the HID interface; canonicalize to
            // find the owning USB device + its parent + its vid/pid.
            let hid_device = entry.path().join("device");
            let (usb_parent, is_cm108) = match fs::canonicalize(&hid_device) {
                Ok(hid_dev_dir) => {
                    // Walk up to the USB device dir that carries idVendor/idProduct.
                    let usb_dev_dir = nearest_usb_device_dir(&hid_dev_dir);
                    let parent = usb_dev_dir
                        .as_ref()
                        .and_then(|d| d.parent())
                        .map(|p| p.to_string_lossy().into_owned());
                    let cm108 = usb_dev_dir
                        .as_ref()
                        .map(|d| {
                            let vid = hex4_from_sysfs(&d.join("idVendor"));
                            let pid = hex4_from_sysfs(&d.join("idProduct"));
                            is_cm108_usb(vid.as_deref(), pid.as_deref())
                        })
                        .unwrap_or(false);
                    (parent, cm108)
                }
                Err(_) => (None, false),
            };
            hidraws.push(HidrawNode {
                path: dev_path,
                usb_parent,
                is_cm108,
            });
        }
    }

    // 5. /dev/ttyUSB* + /dev/ttyACM* serial nodes with their USB parent.
    let mut ttys: Vec<TtyNode> = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/tty") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("ttyUSB") || name.starts_with("ttyACM")) {
                continue;
            }
            let dev_path = format!("/dev/{name}");
            let tty_device = entry.path().join("device");
            let usb_parent = match fs::canonicalize(&tty_device) {
                Ok(tty_dev_dir) => nearest_usb_device_dir(&tty_dev_dir)
                    .as_ref()
                    .and_then(|d| d.parent())
                    .map(|p| p.to_string_lossy().into_owned()),
                Err(_) => None,
            };
            ttys.push(TtyNode {
                path: dev_path,
                usb_parent,
            });
        }
    }

    SysSnapshot {
        cards,
        hidraws,
        ttys,
    }
}

/// IMPURE helper: walk a canonicalized sysfs path UP until a directory that holds
/// both `idVendor` and `idProduct` (a USB *device* node, not an interface node).
/// Returns that directory, or `None` if none is found before the filesystem root.
/// Kept thin (just `.parent()` + existence checks); the CM108/vid-pid decision it
/// feeds is the pure [`is_cm108_usb`].
#[allow(dead_code)]
fn nearest_usb_device_dir(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(dir) = cur {
        if dir.join("idVendor").exists() && dir.join("idProduct").exists() {
            return Some(dir);
        }
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    None
}

/// IMPURE helper: read a sysfs `idVendor`/`idProduct` file and return the
/// lower-cased 4-hex token (no `0x`), or `None` if the file is absent/unreadable
/// or does not parse as 4-hex. The 4-hex normalization itself is the pure
/// [`normalize_hex4`].
#[allow(dead_code)]
fn hex4_from_sysfs(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| normalize_hex4(s.trim()))
}

// ============================================================================
// Pure parse helpers — fixture-tested; the impure reader delegates every parse.
// ============================================================================

/// Parse `/proc/asound/cards` text into `(card_index, card_id, card_longname)`
/// triples. Pure.
///
/// The file's stanza shape (two lines per card):
/// ```text
///  0 [vc4hdmi        ]: vc4-hdmi - vc4-hdmi
///                       vc4-hdmi
///  1 [Device         ]: USB-Audio - C-Media USB Audio Device
///                       C-Media USB Audio Device at usb-...
/// ```
/// The first line carries `<index> [<id>]: <driver> - <longname>`; the longname
/// after ` - ` is the human label. The continuation line is ignored.
fn parse_proc_asound_cards(text: &str) -> Vec<(u32, String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        // A card header line starts (after leading spaces) with the index digits,
        // then ` [id  ]: driver - longname`. Continuation lines have no `[`.
        let Some(lb) = line.find('[') else {
            continue;
        };
        let Some(rb) = line.find(']') else {
            continue;
        };
        if rb < lb {
            continue;
        }
        let index_part = line[..lb].trim();
        let Ok(card_index) = index_part.parse::<u32>() else {
            continue; // not a header line (continuation / blank)
        };
        let card_id = line[lb + 1..rb].trim().to_string();
        // After "]:" comes "<driver> - <longname>". Take the part after the first
        // " - "; fall back to the whole remainder if there is no " - ".
        let after = match line[rb + 1..].split_once(':') {
            Some((_, rest)) => rest,
            None => &line[rb + 1..],
        };
        let card_name = match after.split_once(" - ") {
            Some((_, longname)) => longname.trim().to_string(),
            None => after.trim().to_string(),
        };
        out.push((card_index, card_id, card_name));
    }
    out
}

/// Extract the `card<N>` index a `/dev/snd/by-id` symlink TARGET points at. Pure.
///
/// Targets look like `../controlC1`, `../pcmC1D0c`, `../pcmC1D0p`. The card index
/// is the run of digits immediately after the `C` in `controlC<N>` / `pcmC<N>D…`.
/// Returns `None` for a target that matches no known node shape.
fn card_index_from_symlink_target(target: &str) -> Option<u32> {
    // Take the basename (last path component) to avoid matching digits in the
    // `../` prefix or any directory names.
    let base = target.rsplit('/').next().unwrap_or(target);
    let rest = if let Some(r) = base.strip_prefix("controlC") {
        r
    } else if let Some(r) = base.strip_prefix("pcmC") {
        r
    } else {
        return None;
    };
    // Leading digits are the card index (pcmC1D0c → "1D0c" → 1).
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().ok()
}

/// Normalize a sysfs vid/pid token to a lower-cased bare 4-hex string. Pure.
/// Accepts an optional `0x` prefix and any case; rejects anything that is not
/// exactly four hex digits once normalized.
fn normalize_hex4(raw: &str) -> Option<String> {
    let s = raw.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(s.to_ascii_lowercase())
    } else {
        None
    }
}

/// Decide whether a USB vid/pid pair is a CM108-family HID PTT keyer. Pure.
///
/// The C-Media CM108/CM119/CM119A family (VID `0x0d8c`) is the canonical
/// hamlib/Dire Wolf `PTT CM108` GPIO keyer used by the DRA-100 and many DIY
/// interfaces. A `None` vid (unreadable sysfs) is NOT a CM108 (conservative: an
/// unknown HID is not offered as a PTT line).
fn is_cm108_usb(vid: Option<&str>, _pid: Option<&str>) -> bool {
    // C-Media VID. The whole 0d8c family exposes the CM108-style HID GPIO line
    // Dire Wolf keys via `PTT CM108`; we do not narrow by PID because the family
    // spans several PIDs and Dire Wolf's own CM108 support is VID-family-wide.
    matches!(vid, Some("0d8c"))
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
            cards: vec![digirig_card(1, parent_a), dra100_card(2, parent_b)],
            ..Default::default()
        };
        // Arrangement 2: the SAME two physical cards, indices swapped, and even
        // listed in the other order — id resolution must be invariant to both.
        let snap2 = SysSnapshot {
            cards: vec![dra100_card(1, parent_b), digirig_card(2, parent_a)],
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
        let set1: std::collections::HashSet<&str> = ids1.iter().map(|i| i.value.as_str()).collect();
        let set2: std::collections::HashSet<&str> = ids2.iter().map(|i| i.value.as_str()).collect();
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
            cards: vec![onboard_hdmi_card(0), digirig_card(1, "/sys/.../usb1/1-1.1")],
            ..Default::default()
        };
        let devices = enumerate_audio_devices(&snap);
        // Onboard excluded — only the USB card remains.
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].alsa_plughw, "plughw:CARD=Device,DEV=0");
        assert!(!devices.iter().any(|d| d.alsa_plughw.contains("vc4hdmi")));
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

    // ---- P6.A: resolve_managed_device (pure) --------------------------------

    /// A persisted stable id that IS present in the live snapshot resolves to the
    /// right plughw + the LIVE card index — even when the live index differs from
    /// whatever it was when first persisted (re-plug swapped the boot order).
    #[test]
    fn resolve_managed_device_present_resolves_plughw_and_live_index() {
        // DigiRig persisted when it was card 1; now it enumerates as card 2.
        let persisted = derive_stable_id(&digirig_card(1, "/sys/p/a"));
        let snap = SysSnapshot {
            cards: vec![dra100_card(1, "/sys/p/b"), digirig_card(2, "/sys/p/a")],
            ..Default::default()
        };
        let resolved = resolve_managed_device(&persisted, &snap)
            .expect("persisted DigiRig id must resolve against the live snapshot");
        assert_eq!(resolved.alsa_plughw, "plughw:CARD=Device,DEV=0");
        // The LIVE index (2), not the persist-time index (1).
        assert_eq!(resolved.card_index, 2);
    }

    /// A persisted stable id that is NOT present (device unplugged) resolves to
    /// `None` — the caller surfaces "configured sound card not found", never
    /// spawns Dire Wolf against the wrong card.
    #[test]
    fn resolve_managed_device_absent_is_none() {
        let persisted = derive_stable_id(&digirig_card(1, "/sys/p/a"));
        // Snapshot has only the DRA-100 — the DigiRig is unplugged.
        let snap = SysSnapshot {
            cards: vec![dra100_card(1, "/sys/p/b")],
            ..Default::default()
        };
        assert!(resolve_managed_device(&persisted, &snap).is_none());
    }

    // ---- P6.A: pure parse helpers (fixtures) --------------------------------

    /// `/proc/asound/cards` text parses to (index, id, longname) triples; the
    /// onboard + a USB card are both recognized, continuation lines ignored.
    #[test]
    fn parse_proc_asound_cards_extracts_index_id_longname() {
        let text = "\
 0 [vc4hdmi        ]: vc4-hdmi - vc4-hdmi
                      vc4-hdmi
 1 [Device         ]: USB-Audio - C-Media USB Audio Device
                      C-Media USB Audio Device at usb-0000:01:00.0-1.2
";
        let cards = parse_proc_asound_cards(text);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0], (0, "vc4hdmi".to_string(), "vc4-hdmi".to_string()));
        assert_eq!(
            cards[1],
            (
                1,
                "Device".to_string(),
                "C-Media USB Audio Device".to_string()
            )
        );
    }

    /// A by-id symlink TARGET (control or pcm node) yields the card index after
    /// the `C`; an unknown shape yields `None`.
    #[test]
    fn card_index_from_symlink_target_handles_control_and_pcm() {
        assert_eq!(card_index_from_symlink_target("../controlC1"), Some(1));
        assert_eq!(card_index_from_symlink_target("../pcmC2D0c"), Some(2));
        assert_eq!(card_index_from_symlink_target("../pcmC10D0p"), Some(10));
        // Bare basename (no ../) still works.
        assert_eq!(card_index_from_symlink_target("controlC3"), Some(3));
        // Unknown node shape → None.
        assert_eq!(card_index_from_symlink_target("../timer"), None);
        assert_eq!(card_index_from_symlink_target("../seq"), None);
    }

    /// vid/pid normalization: strips `0x`, lower-cases, rejects non-4-hex.
    #[test]
    fn normalize_hex4_strips_prefix_and_validates() {
        assert_eq!(normalize_hex4("0x0D8C"), Some("0d8c".to_string()));
        assert_eq!(normalize_hex4("0d8c"), Some("0d8c".to_string()));
        assert_eq!(normalize_hex4("  0D8C\n"), Some("0d8c".to_string()));
        assert_eq!(normalize_hex4("0d8"), None); // too short
        assert_eq!(normalize_hex4("0d8cc"), None); // too long
        assert_eq!(normalize_hex4("zzzz"), None); // not hex
    }

    /// CM108 classification keys off the C-Media VID family; other vids and an
    /// unreadable (None) vid are not CM108.
    #[test]
    fn is_cm108_usb_matches_cmedia_family() {
        assert!(is_cm108_usb(Some("0d8c"), Some("0012")));
        assert!(is_cm108_usb(Some("0d8c"), None));
        assert!(!is_cm108_usb(Some("10c4"), Some("ea60"))); // CP2102 (DigiRig serial), not a HID PTT
        assert!(!is_cm108_usb(None, None)); // unreadable → conservative not-a-PTT
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
