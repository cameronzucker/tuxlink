//! VARA host-side PTT keying (tuxlink-yrrjq).
//!
//! VARA is a soundcard modem with **no ability to key a radio**: it raises
//! `PTT ON` / `PTT OFF` on the TCP command socket and the *host application*
//! must key the rig. Until this module, Tuxlink parsed those events
//! ([`InboundCommand::Ptt`](super::command::InboundCommand)) and discarded
//! them everywhere — VARA modulated into a transmitter that stayed in RX, so
//! no VARA dial could ever reach the air. This module is the missing half:
//! a keyer resolved **fail-closed** from the operator's existing PTT config,
//! driven for the entire session (dial + B2F exchange) by the callers in
//! `commands.rs`.
//!
//! ## Keying backends (map 1:1 onto [`PttMethod`])
//!
//! - [`PttMethod::CatCommand`] → momentary open/write/close of the CAT key /
//!   unkey commands (FT-710 `TX1;` / `TX0;`) on the rig CAT serial. The port
//!   is **closed between events** — the close-serial discipline proven on air
//!   2026-06-23 (holding the CP2105 open concurrent with audio streaming
//!   resets the C-Media codec; momentary touches do not).
//! - [`PttMethod::SerialRts`] → hold the PTT serial open for the session and
//!   assert/deassert RTS. Port loss or drop deasserts RTS — fails toward
//!   unkey (the safe direction; contrast the 2026-06-25 stuck-TX incident
//!   where a lost close-serial port could not send `TX0;`).
//! - [`PttMethod::Vox`] → explicit no-op: an external audio-derived keyer
//!   (VOX interface) keys the radio. Loud session-log warning at resolve so
//!   an operator who *thought* Tuxlink would key is not silently dead-aired.
//!
//! ## RADIO-1
//!
//! Nothing in this module transmits on its own. `set_ptt(true)` keys the
//! radio ONLY when called from the consent-gated VARA dial/exchange path in
//! response to VARA's `PTT ON`. The resolve-time serial probe opens and
//! closes the port without writing a byte.

use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use crate::config::{ArdopUiConfig, PttMethod, RigUiConfig};

/// Per-event serial I/O timeout for CAT-PTT writes. A keying write is ~5
/// bytes at ≥4800 baud (≈10 ms); 500 ms absorbs USB-serial latency without
/// letting a wedged port stall the keying pump into VARA's timing budget.
const CAT_PTT_IO_TIMEOUT: Duration = Duration::from_millis(500);

/// Post-write settle before closing the CAT port, so the final bytes drain
/// the USB-serial FIFO before the close. Mirrors the proven on-air bridge
/// (`catptt_bridge.py` slept 70 ms; 30 ms is ample at 38400 baud for 5
/// bytes and keeps per-keying latency low for VARA's frame cadence).
const CAT_PTT_SETTLE: Duration = Duration::from_millis(30);

/// A host-side PTT keyer the VARA session drives. `set_ptt` is called from
/// the dial loop and (concurrently with the B2F exchange) from the PTT pump
/// thread — callers serialize access through a [`Mutex`].
pub trait PttSink: Send {
    /// Key (`true`) or unkey (`false`) the radio. An `Err` from a `true`
    /// call means the radio was NOT keyed (dead-air risk → caller aborts
    /// the dial). An `Err` from a `false` call means the radio may be STUCK
    /// keyed — callers log loudly and keep the unkey guard armed.
    fn set_ptt(&mut self, on: bool) -> Result<(), String>;

    /// One-line description for the session log ("what will key the radio").
    fn describe(&self) -> String;
}

/// The concrete keyer resolved from the operator's config.
pub enum VaraPtt {
    /// External VOX keying — Tuxlink does not key. Explicit operator choice.
    Vox,
    /// Momentary close-serial CAT keying (open → write `TX1;`/`TX0;` → close).
    CatSerial {
        path: String,
        baud: u32,
        key_cmd: String,
        unkey_cmd: String,
    },
    /// Held-port RTS keying. The port stays open for the session; drop
    /// deasserts RTS (fail-safe unkey).
    SerialRts {
        path: String,
        port: Box<dyn serialport::SerialPort>,
    },
}

impl VaraPtt {
    /// Momentary open/write/close of one CAT command. Clears RTS/DTR on
    /// open so the open itself cannot key a rig wired for line-PTT.
    fn cat_write(path: &str, baud: u32, cmd: &str) -> Result<(), String> {
        let mut port = serialport::new(path, baud)
            .timeout(CAT_PTT_IO_TIMEOUT)
            .open()
            .map_err(|e| format!("open {path} failed: {e}"))?;
        let _ = port.write_data_terminal_ready(false);
        let _ = port.write_request_to_send(false);
        port.write_all(cmd.as_bytes())
            .map_err(|e| format!("write {cmd:?} to {path} failed: {e}"))?;
        port.flush()
            .map_err(|e| format!("flush {path} failed: {e}"))?;
        std::thread::sleep(CAT_PTT_SETTLE);
        Ok(())
    }
}

// Manual Debug: `SerialRts` holds a `Box<dyn serialport::SerialPort>`, which
// is not `Debug`, so derive is unavailable. `describe()` is the right
// rendering anyway (used by tests' `expect_err` bounds).
impl std::fmt::Debug for VaraPtt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}

/// Tail hold before every unkey: VARA raises `PTT OFF` when it has finished
/// *writing* the frame's samples, but under Wine → PipeWire → USB codec the
/// last ~65–170 ms of audio is still buffered in flight (measured off-air via
/// a monitor receiver, 2026-07-09 kestrel-butte-granite self-decode rig — the
/// RF envelope died mid-waveform on every ConReq while the sink monitor
/// showed the intended audio outliving the carrier by a median 127 ms).
/// Unkeying immediately amputates the frame tail and breaks remote decode.
/// 250 ms covers the measured deficit with margin — the same "TX tail" knob
/// every hardware soundcard interface exposes for exactly this reason.
const PTT_TAIL_HOLD_MS: u64 = 250;

impl PttSink for VaraPtt {
    fn set_ptt(&mut self, on: bool) -> Result<(), String> {
        if !on {
            std::thread::sleep(std::time::Duration::from_millis(PTT_TAIL_HOLD_MS));
        }
        match self {
            VaraPtt::Vox => Ok(()),
            VaraPtt::CatSerial {
                path,
                baud,
                key_cmd,
                unkey_cmd,
            } => {
                let cmd = if on {
                    key_cmd.as_str()
                } else {
                    unkey_cmd.as_str()
                };
                Self::cat_write(path, *baud, cmd)
            }
            VaraPtt::SerialRts { path, port } => port.write_request_to_send(on).map_err(|e| {
                format!(
                    "RTS {} on {path} failed: {e}",
                    if on { "assert" } else { "deassert" }
                )
            }),
        }
    }

    fn describe(&self) -> String {
        match self {
            VaraPtt::Vox => "VOX/external — Tuxlink will NOT key the radio".into(),
            VaraPtt::CatSerial {
                path,
                baud,
                key_cmd,
                ..
            } => format!("CAT command {key_cmd:?} on {path} @ {baud} (close-serial)"),
            VaraPtt::SerialRts { path, .. } => format!("serial RTS on {path} (held)"),
        }
    }
}

/// Resolve the VARA host-side keyer from the operator's persisted PTT config
/// — **fail-closed**: if the configured method cannot key, return an
/// actionable `Err` so the dial refuses BEFORE any `CONNECT` is sent, rather
/// than dead-airing (Codex 2026-07-09 finding #6).
///
/// The PTT method + key/unkey commands live in [`ArdopUiConfig`] today (they
/// are radio-level facts stored under the modem-ardop section for historical
/// reasons; the CAT serial itself is canonical on [`RigUiConfig`] after the
/// tuxlink-8fkkk heal). VARA reads the same source so both modems key the
/// same radio the same way — relocating the fields is a separate concern.
///
/// - `CatCommand` — requires `rig.cat_serial_path`; probes an open/close of
///   the port (no bytes written) so a missing device or a permission problem
///   (the `dialout` class) surfaces HERE as an actionable error instead of a
///   silent dead-air dial. A momentary probe is the same touch class as a
///   proven keying event (close-serial discipline).
/// - `SerialRts` — requires `ptt_serial_path`; opens and HOLDS the port with
///   RTS deasserted.
/// - `Vox` — allowed (external keyer), but the caller must surface the
///   loud "Tuxlink will not key" line to the session log.
pub fn resolve_vara_ptt(ardop_ui: &ArdopUiConfig, rig: &RigUiConfig) -> Result<VaraPtt, String> {
    match ardop_ui.ptt_method {
        PttMethod::Vox => Ok(VaraPtt::Vox),
        PttMethod::CatCommand => {
            let path = rig
                .cat_serial_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| {
                    "PTT method is CAT command but no CAT serial port is configured — \
                     set the rig CAT serial device (Settings → Radio)"
                        .to_string()
                })?;
            // Fail-closed probe: open + close, no bytes. Surfaces missing
            // device / EACCES (dialout) before any RF-side effect.
            let probe = serialport::new(&path, rig.cat_baud)
                .timeout(CAT_PTT_IO_TIMEOUT)
                .open()
                .map_err(|e| {
                    format!(
                        "CAT-PTT serial {path} cannot be opened ({e}) — check the device \
                         path and that the app user can access serial ports (dialout group)"
                    )
                })?;
            drop(probe);
            Ok(VaraPtt::CatSerial {
                path,
                baud: rig.cat_baud,
                key_cmd: ardop_ui.cat_key_cmd.clone(),
                unkey_cmd: ardop_ui.cat_unkey_cmd.clone(),
            })
        }
        PttMethod::SerialRts => {
            let path = ardop_ui
                .ptt_serial_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| {
                    "PTT method is serial RTS but no PTT serial port is configured — \
                     set the PTT serial device (Settings → Radio)"
                        .to_string()
                })?;
            let mut port = serialport::new(&path, 9600)
                .timeout(CAT_PTT_IO_TIMEOUT)
                .open()
                .map_err(|e| {
                    format!(
                        "RTS-PTT serial {path} cannot be opened ({e}) — check the device \
                         path and that the app user can access serial ports (dialout group)"
                    )
                })?;
            port.write_request_to_send(false)
                .map_err(|e| format!("RTS deassert on {path} at open failed: {e}"))?;
            let _ = port.write_data_terminal_ready(false);
            Ok(VaraPtt::SerialRts { path, port })
        }
    }
}

/// The shared keyer handle the dial loop and the exchange-window PTT pump
/// serialize through. Boxed trait object so tests can substitute a recording
/// sink for the hardware-backed [`VaraPtt`].
pub type SharedPtt = Mutex<Box<dyn PttSink>>;

/// Poison-tolerant lock over the shared keyer: an unkey MUST proceed even if
/// the pump thread panicked while holding the lock (the poisoned state is a
/// bookkeeping artifact; the serial handle underneath is still valid).
pub fn lock_ptt(ptt: &SharedPtt) -> std::sync::MutexGuard<'_, Box<dyn PttSink>> {
    ptt.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// RAII unkey: guarantees a best-effort `set_ptt(false)` on EVERY exit from
/// the VARA dial/exchange path — success, error return, or panic-unwind —
/// so no path can leave the transmitter keyed (Codex 2026-07-09 finding #2).
pub struct UnkeyGuard<'a> {
    ptt: &'a SharedPtt,
}

impl<'a> UnkeyGuard<'a> {
    pub fn new(ptt: &'a SharedPtt) -> Self {
        Self { ptt }
    }
}

impl Drop for UnkeyGuard<'_> {
    fn drop(&mut self) {
        if let Err(e) = lock_ptt(self.ptt).set_ptt(false) {
            tracing::error!(
                target: "tuxlink::winlink::modem::vara",
                error = %e,
                "VARA unkey guard: final PTT-off failed — radio may still be keyed; \
                 check the rig and power off TX if needed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ardop_ui(method: PttMethod, ptt_serial: Option<&str>) -> ArdopUiConfig {
        ArdopUiConfig {
            ptt_method: method,
            ptt_serial_path: ptt_serial.map(str::to_string),
            ..Default::default()
        }
    }

    fn rig(cat_serial: Option<&str>) -> RigUiConfig {
        RigUiConfig {
            cat_serial_path: cat_serial.map(str::to_string),
            ..Default::default()
        }
    }

    // ── resolve: fail-closed ────────────────────────────────────────────

    #[test]
    fn resolve_vox_is_allowed_noop() {
        let k = resolve_vara_ptt(&ardop_ui(PttMethod::Vox, None), &rig(None))
            .expect("VOX resolves (external keyer)");
        assert!(matches!(k, VaraPtt::Vox));
        assert!(
            k.describe().contains("NOT key"),
            "VOX description must warn loudly"
        );
    }

    #[test]
    fn resolve_cat_command_without_serial_fails_closed_with_actionable_error() {
        let err = resolve_vara_ptt(&ardop_ui(PttMethod::CatCommand, None), &rig(None))
            .expect_err("no CAT serial must refuse");
        assert!(
            err.contains("CAT serial"),
            "error must name the missing setting: {err}"
        );
    }

    #[test]
    fn resolve_cat_command_with_blank_serial_fails_closed() {
        let err = resolve_vara_ptt(&ardop_ui(PttMethod::CatCommand, None), &rig(Some("  ")))
            .expect_err("blank CAT serial must refuse");
        assert!(err.contains("CAT serial"), "{err}");
    }

    #[test]
    fn resolve_cat_command_with_missing_device_fails_closed_and_mentions_dialout() {
        let err = resolve_vara_ptt(
            &ardop_ui(PttMethod::CatCommand, None),
            &rig(Some("/dev/tuxlink-test-no-such-tty")),
        )
        .expect_err("nonexistent device must refuse at resolve, not dead-air at dial");
        assert!(
            err.contains("dialout"),
            "error must point at the serial-access class: {err}"
        );
    }

    #[test]
    fn resolve_serial_rts_without_path_fails_closed() {
        let err = resolve_vara_ptt(&ardop_ui(PttMethod::SerialRts, None), &rig(None))
            .expect_err("no PTT serial must refuse");
        assert!(err.contains("PTT serial"), "{err}");
    }

    #[test]
    fn resolve_serial_rts_with_missing_device_fails_closed() {
        let err = resolve_vara_ptt(
            &ardop_ui(PttMethod::SerialRts, Some("/dev/tuxlink-test-no-such-tty")),
            &rig(None),
        )
        .expect_err("nonexistent RTS device must refuse");
        assert!(err.contains("dialout"), "{err}");
    }

    // ── keying: error paths observable, no silent no-op ────────────────

    #[test]
    fn cat_serial_key_against_missing_device_returns_err_not_silent_noop() {
        let mut k = VaraPtt::CatSerial {
            path: "/dev/tuxlink-test-no-such-tty".into(),
            baud: 38400,
            key_cmd: "TX1;".into(),
            unkey_cmd: "TX0;".into(),
        };
        let err = k
            .set_ptt(true)
            .expect_err("keying a missing device must surface");
        assert!(err.contains("open"), "{err}");
    }

    #[test]
    fn vox_set_ptt_is_noop_ok() {
        let mut k = VaraPtt::Vox;
        assert!(k.set_ptt(true).is_ok());
        assert!(k.set_ptt(false).is_ok());
    }

    // ── UnkeyGuard: unkeys on drop, tolerates poisoned lock ────────────

    // The guard is written against Mutex<VaraPtt>; observe Drop behavior
    // without hardware via Vox (a no-op success). Poison tolerance is the
    // load-bearing property: a pump-thread panic must not block the unkey.
    #[test]
    fn unkey_guard_unkeys_on_drop_even_with_poisoned_lock() {
        let ptt: SharedPtt = Mutex::new(Box::new(VaraPtt::Vox));
        // Poison the mutex: panic while holding the guard on another thread.
        let _ = std::thread::scope(|s| {
            s.spawn(|| {
                let _g = ptt.lock().unwrap();
                panic!("poison the ptt lock (simulated pump panic)");
            })
            .join()
        });
        assert!(ptt.lock().is_err(), "precondition: lock is poisoned");
        // Guard drop must still lock (poison-tolerant) and unkey without panic.
        drop(UnkeyGuard::new(&ptt));
        // Reaching here without panic IS the assertion; also verify the
        // keyer is still usable through lock_ptt.
        assert!(lock_ptt(&ptt).set_ptt(false).is_ok());
    }
}
