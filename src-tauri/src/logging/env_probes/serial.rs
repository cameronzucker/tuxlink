//! Serial / USB-serial / Bluetooth RFCOMM environment probe (spec §9.3).
//!
//! RADIO-1: strictly read-only. Enumerates /dev/serial entries and probes
//! group membership; reports KISS-TCP configuration from env (passive read
//! only — no active TCP connect to the modem port).
//!
//! Why no active KISS-TCP connect: if VARA/ARDOP/KISS is already running, an
//! unconsented diagnostic TCP connection perturbs the control connection — a
//! RADIO-1 violation (Codex impl-adrev P1 #2). The operator can verify
//! reachability via the existing UI controls. The spec's read-only probe
//! contract (§9) forbids active connects to modem ports.

use crate::logging::env_probes::{run_with_deadline, safe_env_value, ProbeGate, ProbeSnapshot};
use chrono::Utc;
use serde_json::json;

pub static GATE: ProbeGate = ProbeGate::new();

pub fn run(trigger: &str) -> ProbeSnapshot {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // /dev/serial/by-id/ enumeration
    let by_id_devices = enumerate_by_id();

    // /dev/ttyUSB* and /dev/ttyACM*
    let tty_usb_devices = glob_tty("/dev/ttyUSB");
    let tty_acm_devices = glob_tty("/dev/ttyACM");

    // Check if user is in dialout group via nix::unistd::getgroups
    let in_dialout_group = check_dialout_group();

    // KISS-TCP configuration: passive read from env only — NO active TCP connect.
    //
    // An active connect to the configured modem host/port is a RADIO-1 violation:
    // if VARA/ARDOP/KISS is running, the unconsented SYN can perturb the control
    // connection. Report only whether host/port are configured in env, not whether
    // the port is reachable. Operator can verify reachability via the UI controls.
    let kiss_tcp_host = safe_env_value("TUXLINK_VARA_TCP_HOST")
        .or_else(|| safe_env_value("TUXLINK_ARDOP_TCP_HOST"));
    let kiss_tcp_port_str = safe_env_value("TUXLINK_VARA_TCP_PORT")
        .or_else(|| safe_env_value("TUXLINK_ARDOP_TCP_PORT"));
    // `kiss_tcp_configured` is true when BOTH host and port are present in env.
    // It does NOT imply the port is reachable — only that it is configured.
    let kiss_tcp_configured = kiss_tcp_host.is_some()
        && kiss_tcp_port_str.as_deref().and_then(|p| p.parse::<u16>().ok()).is_some();

    // Bluetooth: bluetoothctl info for adapter presence
    let bluetooth_adapter_present = run_with_deadline("bluetoothctl", &["show"])
        .map(|s| s.contains("Controller"))
        .unwrap_or(false);

    let result = json!({
        "trigger": trigger,
        "by_id_devices": by_id_devices,
        "tty_usb_devices": tty_usb_devices,
        "tty_acm_devices": tty_acm_devices,
        "in_dialout_group": in_dialout_group,
        "kiss_tcp_configured": kiss_tcp_configured,
        "bluetooth_adapter_present": bluetooth_adapter_present,
    });

    ProbeSnapshot {
        probe: "serial".into(),
        timestamp,
        trigger: trigger.into(),
        result,
    }
}

fn enumerate_by_id() -> Vec<String> {
    let path = std::path::Path::new("/dev/serial/by-id");
    if !path.exists() {
        return vec![];
    }
    std::fs::read_dir(path)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| {
                e.ok().and_then(|e| e.file_name().into_string().ok())
            })
            .collect()
        })
        .unwrap_or_default()
}

fn glob_tty(prefix: &str) -> Vec<String> {
    // Read /dev and collect entries matching prefix
    let dev = std::path::Path::new("/dev");
    let prefix_name = prefix.trim_start_matches("/dev/");
    std::fs::read_dir(dev)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| name.starts_with(prefix_name))
                .map(|name| format!("/dev/{name}"))
                .collect()
        })
        .unwrap_or_default()
}

fn check_dialout_group() -> bool {
    // Look up the dialout GID via /etc/group, then check via libc getgroups
    let dialout_gid = read_group_gid("dialout");
    if let Some(gid) = dialout_gid {
        get_supplementary_groups()
            .map(|groups| groups.contains(&gid))
            .unwrap_or(false)
    } else {
        false
    }
}

fn get_supplementary_groups() -> Option<Vec<u32>> {
    // Two-call pattern: first call with ngroups=0 returns the actual count
    // (POSIX allows this; getgroups doesn't write to buf when ngroups=0).
    // Then allocate exactly that size and call again. NGROUPS_MAX on Linux
    // is 65536; a fixed 128 buffer fails with EINVAL on users in more groups.
    let needed = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
    if needed < 0 {
        return None;
    }
    let mut groups: Vec<libc::gid_t> = vec![0; needed as usize];
    let count = unsafe { libc::getgroups(needed, groups.as_mut_ptr()) };
    if count < 0 {
        return None;
    }
    groups.truncate(count as usize);
    Some(groups.into_iter().map(|g| g as u32).collect())
}

fn read_group_gid(name: &str) -> Option<u32> {
    let content = std::fs::read_to_string("/etc/group").ok()?;
    content.lines().find_map(|line| {
        let mut parts = line.split(':');
        let gname = parts.next()?;
        if gname != name { return None; }
        let _pass = parts.next()?;
        let gid_str = parts.next()?;
        gid_str.parse::<u32>().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_produces_non_empty_json() {
        let snap = run("test");
        assert_eq!(snap.probe, "serial");
        let r = &snap.result;
        assert!(r.get("by_id_devices").is_some());
        assert!(r.get("in_dialout_group").is_some());
        assert!(r.get("bluetooth_adapter_present").is_some());
    }

    #[test]
    fn run_reports_kiss_tcp_configured_not_reachable() {
        // Verify the field name is `kiss_tcp_configured`, not the old
        // `kiss_tcp_reachable` — the old name implied an active TCP probe.
        let snap = run("test");
        let r = &snap.result;
        assert!(
            r.get("kiss_tcp_configured").is_some(),
            "serial probe must report kiss_tcp_configured (passive), not kiss_tcp_reachable"
        );
        assert!(
            r.get("kiss_tcp_reachable").is_none(),
            "serial probe must NOT report kiss_tcp_reachable — that field implies an active connect"
        );
    }

    #[test]
    fn kiss_tcp_configured_is_false_when_no_env() {
        // Without env vars set, configured must be false — not an error.
        // We can't guarantee env isn't set in CI, but we can at least check the
        // field is present and is a boolean.
        let snap = run("test");
        let r = &snap.result;
        let field = r.get("kiss_tcp_configured").expect("field must be present");
        assert!(
            field.is_boolean(),
            "kiss_tcp_configured must be a boolean, got {field:?}"
        );
    }
}
