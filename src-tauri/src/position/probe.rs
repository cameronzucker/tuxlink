//! GPS source detection probes (tuxlink-9xy1 slice 1).
//!
//! Unprivileged probes the GPS source-picker runs to decide which sources are
//! working (→ source cards) vs blocked (→ triage cards with a fix command).
//! Everything here is read-only and needs no elevation:
//!
//! - **gpsd** — is a gpsd daemon reachable on `127.0.0.1:2947`? (bounded 200 ms)
//! - **serial devices** — what `/dev/ttyACM*` / `/dev/ttyUSB*` exist, with their
//!   udev vendor/model strings (so the picker can say "u-blox GNSS receiver" not
//!   "/dev/ttyACM0").
//! - **dialout membership** — is the current user in the `dialout` group? Not
//!   being in it is the #1 reason a present GPS serial device can't be opened.
//! - **ModemManager** — is `ModemManager.service` active? It grabs serial ports
//!   on connect and is a common cause of "my GPS device exists but nothing reads
//!   it." The triage card offers to mask it (the actual mask lands in slice 2).
//!
//! Parsing is factored into pure functions (unit-tested); the `#[tauri::command]`
//! wrappers only do the I/O and run blocking shell-outs on a blocking thread.
//! We shell `udevadm` / `id` / `getent` / `systemctl` rather than linking
//! `libudev` to avoid a new build-time system dependency.

use serde::Serialize;

use super::gpsd::GPSD_DEFAULT_ADDR;

// ---------------------------------------------------------------------------
// Result types (serialize to the GpsSourcePicker frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GpsdProbe {
    /// A gpsd daemon accepted a TCP connection on 127.0.0.1:2947.
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialDevice {
    /// Device node, e.g. `/dev/ttyACM0`.
    pub path: String,
    /// Human vendor string from the udev hwdb, if known (`ID_VENDOR_FROM_DATABASE`).
    pub vendor: Option<String>,
    /// Human model string from the udev hwdb, if known (`ID_MODEL_FROM_DATABASE`).
    pub model: Option<String>,
    /// USB vendor id, e.g. `1546` (`ID_VENDOR_ID`).
    pub vendor_id: Option<String>,
    /// USB product id, e.g. `01a8` (`ID_MODEL_ID`).
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SerialProbe {
    pub devices: Vec<SerialDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialoutProbe {
    /// The current user is a member of `dialout`.
    pub member: bool,
    /// The `dialout` group exists on this system at all.
    pub group_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModemManagerProbe {
    /// `ModemManager.service` is active (it may be holding the serial port).
    pub active: bool,
}

// ---------------------------------------------------------------------------
// Pure parsers (unit-tested; no I/O)
// ---------------------------------------------------------------------------

/// Keep only `/dev/ttyACM*` and `/dev/ttyUSB*` from a list of `/dev` entry names,
/// returned as full device paths, sorted for stable UI ordering.
pub fn serial_device_paths_from_names<I, S>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = names
        .into_iter()
        .filter_map(|n| {
            let n = n.as_ref();
            (n.starts_with("ttyACM") || n.starts_with("ttyUSB")).then(|| format!("/dev/{n}"))
        })
        .collect();
    out.sort();
    out
}

/// Parse the `KEY=VALUE` lines of `udevadm info -q property -n <path>` into the
/// vendor/model fields we surface. Unknown keys are ignored; missing keys → None.
pub fn parse_udevadm_properties(path: &str, output: &str) -> SerialDevice {
    let mut dev = SerialDevice {
        path: path.to_string(),
        vendor: None,
        model: None,
        vendor_id: None,
        product_id: None,
    };
    for line in output.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.trim() {
                "ID_VENDOR_FROM_DATABASE" => dev.vendor = Some(value.to_string()),
                "ID_MODEL_FROM_DATABASE" => dev.model = Some(value.to_string()),
                "ID_VENDOR_ID" => dev.vendor_id = Some(value.to_string()),
                "ID_MODEL_ID" => dev.product_id = Some(value.to_string()),
                _ => {}
            }
        }
    }
    dev
}

/// `id -nG` prints space-separated group names. True iff `dialout` is present.
pub fn id_groups_contains_dialout(output: &str) -> bool {
    output.split_whitespace().any(|g| g == "dialout")
}

/// `systemctl is-active ModemManager` prints `active` / `inactive` / `failed` /
/// `unknown`. Only a trimmed `active` counts as holding the port.
pub fn modemmanager_is_active(systemctl_output: &str) -> bool {
    systemctl_output.trim() == "active"
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

fn run_capture(cmd: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

fn probe_serial_blocking() -> SerialProbe {
    let names: Vec<String> = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let devices = serial_device_paths_from_names(names)
        .into_iter()
        .map(|path| match run_capture("udevadm", &["info", "-q", "property", "-n", &path]) {
            Some(out) => parse_udevadm_properties(&path, &out),
            None => SerialDevice {
                path,
                vendor: None,
                model: None,
                vendor_id: None,
                product_id: None,
            },
        })
        .collect();
    SerialProbe { devices }
}

fn probe_dialout_blocking() -> DialoutProbe {
    let member = run_capture("id", &["-nG"])
        .map(|o| id_groups_contains_dialout(&o))
        .unwrap_or(false);
    // `getent group dialout` exits 0 (and prints a line) iff the group exists.
    let group_exists = std::process::Command::new("getent")
        .args(["group", "dialout"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    DialoutProbe { member, group_exists }
}

fn probe_modemmanager_blocking() -> ModemManagerProbe {
    let active = run_capture("systemctl", &["is-active", "ModemManager"])
        .map(|o| modemmanager_is_active(&o))
        .unwrap_or(false);
    ModemManagerProbe { active }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// gpsd reachability on 127.0.0.1:2947, bounded to 200 ms so the wizard probe
/// never stalls onboarding when no daemon is running.
#[tauri::command]
pub async fn gps_probe_gpsd() -> GpsdProbe {
    use tokio::net::TcpStream;
    use tokio::time::{timeout, Duration};
    let reachable = matches!(
        timeout(Duration::from_millis(200), TcpStream::connect(GPSD_DEFAULT_ADDR)).await,
        Ok(Ok(_))
    );
    GpsdProbe { reachable }
}

/// Enumerate candidate GPS serial devices with their udev vendor/model strings.
#[tauri::command]
pub async fn gps_probe_serial_devices() -> SerialProbe {
    tokio::task::spawn_blocking(probe_serial_blocking)
        .await
        .unwrap_or(SerialProbe { devices: Vec::new() })
}

/// Whether the current user can open serial devices (dialout membership).
#[tauri::command]
pub async fn gps_probe_dialout() -> DialoutProbe {
    tokio::task::spawn_blocking(probe_dialout_blocking)
        .await
        .unwrap_or(DialoutProbe { member: false, group_exists: false })
}

/// Whether ModemManager is active (and may be grabbing the GPS serial port).
#[tauri::command]
pub async fn gps_probe_modemmanager() -> ModemManagerProbe {
    tokio::task::spawn_blocking(probe_modemmanager_blocking)
        .await
        .unwrap_or(ModemManagerProbe { active: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_and_sorts_serial_devices() {
        let names = vec!["ttyUSB1", "sda", "ttyACM0", "null", "ttyUSB0", "ttyS0"];
        assert_eq!(
            serial_device_paths_from_names(names),
            vec!["/dev/ttyACM0", "/dev/ttyUSB0", "/dev/ttyUSB1"]
        );
    }

    #[test]
    fn ttys_and_random_devices_are_not_serial_gps_candidates() {
        // ttyS0 (16550 UART) and ttyAMA0 are not USB serial — excluded.
        let names = vec!["ttyS0", "ttyAMA0", "ttyprintk"];
        assert!(serial_device_paths_from_names(names).is_empty());
    }

    #[test]
    fn parses_udevadm_vendor_model_ids() {
        let out = "\
ID_VENDOR_FROM_DATABASE=u-blox AG
ID_MODEL_FROM_DATABASE=u-blox GNSS receiver
ID_VENDOR_ID=1546
ID_MODEL_ID=01a8
DEVNAME=/dev/ttyACM0
SUBSYSTEM=tty";
        let dev = parse_udevadm_properties("/dev/ttyACM0", out);
        assert_eq!(dev.vendor.as_deref(), Some("u-blox AG"));
        assert_eq!(dev.model.as_deref(), Some("u-blox GNSS receiver"));
        assert_eq!(dev.vendor_id.as_deref(), Some("1546"));
        assert_eq!(dev.product_id.as_deref(), Some("01a8"));
        assert_eq!(dev.path, "/dev/ttyACM0");
    }

    #[test]
    fn missing_or_empty_udev_keys_yield_none() {
        let dev = parse_udevadm_properties("/dev/ttyUSB0", "ID_VENDOR_ID=\nSUBSYSTEM=tty");
        assert_eq!(dev.vendor, None);
        assert_eq!(dev.vendor_id, None, "empty value must not become Some(\"\")");
    }

    #[test]
    fn detects_dialout_membership() {
        assert!(id_groups_contains_dialout("administrator dialout sudo plugdev"));
        assert!(!id_groups_contains_dialout("administrator sudo plugdev"));
        // Substring must not false-match (e.g. a hypothetical "dialout-admins").
        assert!(!id_groups_contains_dialout("dialout-admins wheel"));
    }

    #[test]
    fn modemmanager_active_only_for_exact_active() {
        assert!(modemmanager_is_active("active\n"));
        assert!(!modemmanager_is_active("inactive\n"));
        assert!(!modemmanager_is_active("failed"));
        assert!(!modemmanager_is_active("unknown\n"));
    }
}
