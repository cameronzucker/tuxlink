//! Managed rigctld subprocess + tune/close-serial lifecycle.
//!
//! On internal-codec radios, `release_serial` STOPS rigctld after tuning so the
//! CAT serial is free before audio streams (see project_ft710_internal_codec_tx_reset).
//! On the DRA-100 path the caller simply never calls `release_serial`.

use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{Mode, RigError, RigStatus, RigctldClient, Rig};

/// How rigctld is invoked + where its control socket lives.
#[derive(Debug, Clone)]
pub struct RigConfig {
    pub binary: String,
    pub model: u32,
    pub serial_path: String,
    pub baud: u32,
    pub host: String,
    pub port: u16,
}

impl RigConfig {
    /// Argv (after the binary) for `rigctld -m <model> -r <serial> -s <baud> -t <port>`.
    pub fn rigctld_args(&self) -> Vec<String> {
        vec![
            "-m".into(), self.model.to_string(),
            "-r".into(), self.serial_path.clone(),
            "-s".into(), self.baud.to_string(),
            "-t".into(), self.port.to_string(),
        ]
    }
}

/// A spawned rigctld plus a connected client. Stops the subprocess on drop.
pub struct ManagedRig {
    child: Option<Child>,
    client: RigctldClient,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_POLL: Duration = Duration::from_millis(100);
const STOP_GRACE: Duration = Duration::from_millis(500);
/// Read timeout for the managed client's CAT socket (tuxlink-8fkkk C3).
/// Bounds each socket read so a hung rigctld cannot wedge the caller's thread
/// indefinitely during the pre-audio tune or live-VFO poll. Sized to be
/// generous relative to the CAT round-trip (~ms) yet short enough to surface
/// a stall within the operator's attention window.
const RIG_READ_TIMEOUT: Duration = Duration::from_secs(5);

impl ManagedRig {
    /// Spawn rigctld and connect a control client once its socket accepts.
    pub fn spawn(cfg: RigConfig) -> Result<Self, RigError> {
        let mut child = Command::new(&cfg.binary)
            .args(cfg.rigctld_args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RigError::Spawn(format!("failed to spawn {}: {e}", cfg.binary)))?;

        // tuxlink-8fkkk C3: switch to `connect_with_timeout` so the managed
        // client carries a read timeout on its CAT socket. The retry poll
        // loop on connection refusal (rigctld startup lag) is preserved — only
        // the *successful* client is bounded. A hung rigctld therefore blocks
        // at most one `RIG_READ_TIMEOUT` per CAT command rather than
        // indefinitely.
        let start = Instant::now();
        let client = loop {
            match RigctldClient::connect_with_timeout(&cfg.host, cfg.port, RIG_READ_TIMEOUT) {
                Ok(c) => break c,
                Err(_) if start.elapsed() < CONNECT_TIMEOUT => {
                    thread::sleep(CONNECT_POLL);
                }
                Err(e) => {
                    // Connect timed out: kill + reap the spawned rigctld before
                    // returning. `Child::drop` neither kills nor reaps, so a bare
                    // return would orphan a live rigctld holding the CAT serial.
                    // Per-dial-candidate spawn retries (Task 8/9) would otherwise
                    // accumulate orphans and block the port.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e);
                }
            }
        };

        Ok(Self { child: Some(child), client })
    }

    /// Set frequency (Hz) then mode. Order matters: freq before mode mirrors WLE.
    pub fn tune(&mut self, hz: u64, mode: Mode) -> Result<(), RigError> {
        self.client.set_freq(hz)?;
        self.client.set_mode(mode)?;
        Ok(())
    }

    /// Set frequency (Hz) only, leaving mode untouched. Used to restore the
    /// operator's VFO when the saved mode is unknown (outside `tux_rig::Mode`,
    /// e.g. AM/CW/FM) so at least the frequency comes back.
    pub fn set_freq(&mut self, hz: u64) -> Result<(), RigError> {
        self.client.set_freq(hz)
    }

    /// Read the current rig state.
    pub fn status(&mut self) -> Result<RigStatus, RigError> {
        self.client.read_status()
    }

    /// Close-serial: stop rigctld so the CAT serial is released before audio.
    /// Idempotent. After this, `tune`/`status` will fail until `spawn` is called
    /// again (the caller re-spawns on the next connect).
    pub fn release_serial(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ManagedRig {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort SIGKILL + reap within grace; rigctld has no clean-stop
            // protocol and holds only the serial, so kill is safe.
            let _ = child.kill();
            let deadline = Instant::now() + STOP_GRACE;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ft710_args() {
        let cfg = RigConfig {
            binary: "rigctld".into(),
            model: 1049,
            serial_path: "/dev/ttyUSB0".into(),
            baud: 38400,
            host: "127.0.0.1".into(),
            port: 4532,
        };
        assert_eq!(
            cfg.rigctld_args(),
            vec!["-m", "1049", "-r", "/dev/ttyUSB0", "-s", "38400", "-t", "4532"],
        );
    }

    /// Write a tiny shell script that behaves like rigctld: bind the `-t <port>`
    /// TCP port and answer set/get. Returns its path (in a unique temp dir).
    fn fake_rigctld_script(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("fake-rigctld.sh");
        let script = r#"#!/usr/bin/env bash
# crude rigctld: parse -t PORT, listen, answer one client with fixed replies.
port=4532
while [ $# -gt 0 ]; do case "$1" in -t) port="$2"; shift 2;; *) shift;; esac; done
# Use ncat/nc if present; emit RPRT 0 for sets, values for gets.
exec 1>/dev/null 2>&1
# Loop forever serving; the test only needs one connection.
while true; do
  { while read -r line; do
      case "$line" in
        F*|M*|T*) printf 'RPRT 0\n';;
        f*) printf '7102000\n';;
        m*) printf 'PKTUSB\n3000\n';;
        t*) printf '0\n';;
        *) printf 'RPRT -1\n';;
      esac
    done; } | nc -l 127.0.0.1 "$port" 2>/dev/null || sleep 0.2
done
"#;
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    // Gated: requires `nc` (netcat). Skips cleanly if absent so CI without nc
    // still passes the pure tests. Run explicitly with `--ignored` where nc exists.
    #[test]
    #[ignore = "requires netcat; run where available"]
    fn spawn_tune_and_release() {
        let tmp = std::env::temp_dir().join(format!("tuxrig-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let script = fake_rigctld_script(&tmp);
        // pick an ephemeral-ish high port
        let port = 45_321;
        let cfg = RigConfig {
            binary: script.to_string_lossy().into_owned(),
            model: 1049,
            serial_path: "/dev/null".into(),
            baud: 38400,
            host: "127.0.0.1".into(),
            port,
        };
        let mut rig = ManagedRig::spawn(cfg).expect("spawn fake rigctld");
        rig.tune(7_102_000, Mode::PktUsb).expect("tune");
        rig.release_serial();
        // After release, a tune attempt errors (socket gone).
        assert!(rig.tune(7_102_000, Mode::PktUsb).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
