//! Close-serial CAT-PTT bridge (tuxlink-wu0k).
//!
//! Some radios — notably the Yaesu FT-710 — key ONLY by a CAT command
//! (`TX1;` / `TX0;`), and on a single-cable USB tree (hub → CP2105 serial +
//! C-Media codec) the audio codec RESETS if the serial port is held OPEN while
//! audio streams. The fix proven on air 2026-06-23 (FT-710 + Pi 5): key by CAT,
//! then CLOSE the serial port during audio — the radio stays CAT-latched in TX
//! — reopening only momentarily to relay each keystring.
//!
//! ardopcf is spawned with `-c TCP:<port> -k <hex(key)> -u <hex(unkey)>` (see
//! `crate::modem_commands::build_ardop_extra_args`), which makes it send its
//! keystring over a TCP "CAT" socket instead of toggling a serial RTS line.
//! This module owns the process that serves that socket: a tiny, audited Python
//! script (`resources/cat-ptt/catptt_bridge.py`) that does the momentary
//! open/write/close. The script is embedded in the binary via `include_str!`
//! and run with the system `python3`, so the bridge needs no extra Rust crate
//! (no cold-compile risk) and behaves identically in a dev run and a packaged
//! build (no Tauri resource-path resolution to get wrong).
//!
//! # Lifecycle
//!
//! [`CatPttBridge::spawn`] writes the embedded script to a private temp file and
//! launches `python3 <script> --port … --serial … --baud … --key … --unkey …`
//! under a [`ManagedModem`] supervisor, then waits for the loopback port to
//! accept connections. The caller
//! ([`super::transport::ArdopTransport::with_managed_modem`]) starts the bridge
//! BEFORE ardopcf and stops it via [`CatPttBridge::stop`] on shutdown; `Drop`
//! is a backstop. The Python script sends the unkey command on every connection
//! teardown as its own failsafe, so a dropped ardopcf socket cannot leave the
//! radio latched in TX.
//!
//! # RADIO-1
//!
//! Spawning the bridge does NOT key the radio: ardopcf only emits a keystring
//! after a CONNECT, which is behind the existing consent gate. The bridge only
//! relays what ardopcf sends. No tuxlink-added airtime cap or timer lives here.

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::super::process::ManagedModem;

/// The embedded close-serial CAT-PTT bridge script. Tracked at
/// `src-tauri/resources/cat-ptt/catptt_bridge.py`; embedded so the bridge runs
/// identically in dev and packaged builds without Tauri resource-path lookups.
const BRIDGE_SCRIPT: &str = include_str!("../../../../resources/cat-ptt/catptt_bridge.py");

/// Total time to wait for the bridge to bind its loopback port after spawn.
const BIND_WAIT: Duration = Duration::from_secs(5);
/// Poll interval while waiting for the bridge port to come up.
const BIND_POLL: Duration = Duration::from_millis(50);

/// Parameters for the close-serial CAT-PTT bridge (tuxlink-wu0k). Built from
/// [`crate::config::ArdopUiConfig`] when `ptt_method == CatCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatBridgeSpec {
    /// Loopback TCP port the bridge listens on; ardopcf connects via
    /// `-c TCP:<port>`. Distinct from the ARDOP cmd/data ports.
    pub bridge_port: u16,
    /// Serial device for CAT (e.g. `/dev/ttyUSB0`).
    pub serial_path: String,
    /// Serial baud rate (e.g. 38400 for the FT-710 Enhanced port).
    pub baud: u32,
    /// CAT key command (e.g. `TX1;`).
    pub key_cmd: String,
    /// CAT unkey command (e.g. `TX0;`).
    pub unkey_cmd: String,
}

impl CatBridgeSpec {
    /// Build the `python3` argv for this spec (after the script path). Pure, so
    /// it is unit-testable without spawning anything.
    pub fn python_args(&self, script_path: &str) -> Vec<String> {
        vec![
            script_path.to_string(),
            "--port".to_string(),
            self.bridge_port.to_string(),
            "--serial".to_string(),
            self.serial_path.clone(),
            "--baud".to_string(),
            self.baud.to_string(),
            "--key".to_string(),
            self.key_cmd.clone(),
            "--unkey".to_string(),
            self.unkey_cmd.clone(),
        ]
    }
}

/// A running CAT-PTT bridge process plus the temp script file it runs from.
///
/// Owns a [`ManagedModem`] (the supervised `python3` process) and the temp
/// script path. `stop` terminates the process; `Drop` removes the temp file and
/// is a backstop terminator via `ManagedModem`'s own `Drop`.
#[derive(Debug)]
pub struct CatPttBridge {
    process: ManagedModem,
    script_path: PathBuf,
    bridge_port: u16,
}

impl CatPttBridge {
    /// Write the embedded script to a private temp file, spawn
    /// `python3 <script> …`, and wait for the loopback port to accept.
    ///
    /// Returns an error if the temp file cannot be written, `python3` cannot be
    /// spawned, or the port never binds within [`BIND_WAIT`].
    pub fn spawn(spec: &CatBridgeSpec) -> io::Result<CatPttBridge> {
        // Port-collision pre-check, BEFORE any side effects. The default bridge
        // port (4532) is hamlib rigctld's upstream default; tuxlink defaults its
        // own rigctld to 4534 to avoid this exact collision (C1 fix,
        // tuxlink-8fkkk), but a manually-launched rigctld or any other process
        // may still hold 4532. If another process already holds the port, the
        // post-spawn TCP probe below would accept against THAT listener — and
        // ardopcf would key the wrong process while our bridge silently failed
        // to bind, with its unkey failsafe out of the path. Fail loudly instead.
        // The listener is dropped immediately so the bridge child can bind the
        // port within milliseconds.
        match std::net::TcpListener::bind(("127.0.0.1", spec.bridge_port)) {
            Ok(listener) => drop(listener),
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!(
                        "CAT-PTT bridge port {} is unavailable ({e}) — rigctld \
                         defaults to 4532, or another session holds it; choose a \
                         different CAT bridge port",
                        spec.bridge_port
                    ),
                ));
            }
        }

        let script_path = write_bridge_script()?;
        let script_str = script_path.to_string_lossy().into_owned();

        let args = spec.python_args(&script_str);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let process = ManagedModem::spawn("python3", &arg_refs).map_err(|e| {
            // Best-effort cleanup of the temp script if the spawn failed.
            let _ = std::fs::remove_file(&script_path);
            io::Error::other(format!("failed to spawn CAT-PTT bridge (python3): {e}"))
        })?;

        let mut bridge = CatPttBridge {
            process,
            script_path,
            bridge_port: spec.bridge_port,
        };

        // Wait for the loopback port to come up so ardopcf's `-c TCP:<port>`
        // connect (which follows immediately) does not race the bind.
        bridge.wait_for_port(BIND_WAIT)?;
        Ok(bridge)
    }

    /// The loopback port the bridge listens on.
    pub fn bridge_port(&self) -> u16 {
        self.bridge_port
    }

    /// Poll `TcpStream::connect` to the bridge port until it accepts or the
    /// timeout elapses. Each successful probe connection is immediately closed.
    ///
    /// Also checks the spawned child is still alive on each poll: if the bridge
    /// process exited during startup (e.g. a serial-open or late bind failure),
    /// fail immediately rather than wait out the timeout — and never report a
    /// dead bridge as ready.
    fn wait_for_port(&mut self, timeout: Duration) -> io::Result<()> {
        let addr = format!("127.0.0.1:{}", self.bridge_port);
        let start = Instant::now();
        loop {
            if !self.process.is_running() {
                return Err(io::Error::other(format!(
                    "CAT-PTT bridge process exited during startup (port {}) — \
                     check the serial device and that the port is free",
                    self.bridge_port
                )));
            }
            if TcpStream::connect_timeout(
                &addr.parse().map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidInput, format!("bad bridge addr: {e}"))
                })?,
                BIND_POLL,
            )
            .is_ok()
            {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "CAT-PTT bridge did not bind port {} within {timeout:?}",
                        self.bridge_port
                    ),
                ));
            }
            std::thread::sleep(BIND_POLL);
        }
    }

    /// Stop the bridge process gracefully (SIGINT → grace → SIGKILL) and remove
    /// the temp script file. Idempotent. On stop the Python script's connection
    /// teardown failsafe has already sent the unkey command for any open socket.
    pub fn stop(&mut self, grace: Duration) -> io::Result<()> {
        let result = self
            .process
            .stop(grace)
            .map_err(|e| io::Error::other(format!("CAT-PTT bridge stop failed: {e}")));
        // Remove the temp script regardless of stop outcome.
        let _ = std::fs::remove_file(&self.script_path);
        result
    }
}

impl Drop for CatPttBridge {
    fn drop(&mut self) {
        // The real Tauri disconnect path DROPS the transport (it holds it as a
        // `Box<dyn ModemTransport>` and `modem_ardop_disconnect_inner` drops it
        // rather than calling `ArdopTransport::shutdown`), so this Drop — not
        // `shutdown` — is the usual bridge teardown. Give the bridge a real grace
        // window to run its SIGINT unkey-failsafe (open serial → write TX0; →
        // close) instead of relying on `ManagedModem::Drop`'s short 200 ms
        // SIGINT→SIGKILL escalation, which can cut the unkey off mid-write and
        // strand the radio keyed. Best-effort; `ManagedModem::Drop` terminates the
        // process regardless. Idempotent with `stop` on the `shutdown` path.
        let _ = self.process.stop(Duration::from_secs(3));
        // Remove the temp script so we don't leak files across reconnects.
        let _ = std::fs::remove_file(&self.script_path);
    }
}

/// Write the embedded bridge script to a uniquely-named temp file and return
/// its path. The filename includes the PID so concurrent processes don't clash.
fn write_bridge_script() -> io::Result<PathBuf> {
    let mut dir = std::env::temp_dir();
    dir.push(format!("tuxlink-catptt-bridge-{}.py", std::process::id()));
    let mut f = std::fs::File::create(&dir)?;
    f.write_all(BRIDGE_SCRIPT.as_bytes())?;
    f.flush()?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> CatBridgeSpec {
        CatBridgeSpec {
            bridge_port: 4532,
            serial_path: "/dev/ttyUSB0".into(),
            baud: 38400,
            key_cmd: "TX1;".into(),
            unkey_cmd: "TX0;".into(),
        }
    }

    #[test]
    fn python_args_carries_every_parameter() {
        let args = spec().python_args("/tmp/bridge.py");
        assert_eq!(args[0], "/tmp/bridge.py");
        // Each flag/value pair must be present in order.
        assert!(args.windows(2).any(|w| w[0] == "--port" && w[1] == "4532"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--serial" && w[1] == "/dev/ttyUSB0"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--baud" && w[1] == "38400"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--key" && w[1] == "TX1;"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--unkey" && w[1] == "TX0;"), "{args:?}");
    }

    #[test]
    fn python_args_honors_custom_parameters() {
        let s = CatBridgeSpec {
            bridge_port: 4600,
            serial_path: "/dev/ttyUSB2".into(),
            baud: 9600,
            key_cmd: "RT1;".into(),
            unkey_cmd: "RT0;".into(),
        };
        let args = s.python_args("/tmp/b.py");
        assert!(args.windows(2).any(|w| w[0] == "--port" && w[1] == "4600"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--serial" && w[1] == "/dev/ttyUSB2"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--baud" && w[1] == "9600"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--key" && w[1] == "RT1;"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--unkey" && w[1] == "RT0;"), "{args:?}");
    }

    #[test]
    fn embedded_script_is_the_parameterized_bridge() {
        // Guard against the embed pointing at a stale/hardcoded script: the
        // parameterized bridge MUST parse argv and send the unkey failsafe.
        assert!(BRIDGE_SCRIPT.contains("argparse"), "bridge script must parse argv");
        assert!(BRIDGE_SCRIPT.contains("--unkey"), "bridge script must accept --unkey");
        assert!(
            BRIDGE_SCRIPT.contains("failsafe"),
            "bridge script must send the unkey failsafe on teardown"
        );
    }

    #[test]
    fn write_bridge_script_writes_the_embedded_content() {
        let path = write_bridge_script().expect("write temp script");
        let content = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(content, BRIDGE_SCRIPT);
        let _ = std::fs::remove_file(&path);
    }
}
