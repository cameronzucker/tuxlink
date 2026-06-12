//! `direwolf.conf` generation for the managed-Dire-Wolf packet path (Slice B,
//! Phase 2 of the managed-modem on-air accessibility design,
//! `docs/design/2026-06-12-managed-modem-onair-accessibility-design.md`).
//!
//! tuxlink spawns Dire Wolf as a **dumb 1200-baud KISS modem** and runs the
//! connected-mode AX.25 protocol ITSELF over the localhost KISS-over-TCP pipe.
//! Everything that makes AX.25 *work on air* — the connected-mode state machine,
//! the source/destination addressing, retries, and the TNC timing parameters —
//! lives in tuxlink ([`super::datalink`], [`super::frame`], [`super::kiss`]), not
//! in Dire Wolf. So the generated conf is deliberately the **minimum** that turns
//! Dire Wolf into a KISS soundmodem keyed off the operator's chosen audio device
//! and PTT line, and nothing else.
//!
//! ## What the conf contains, and only this
//!
//! Exactly six directives, in a fixed order:
//!
//! ```text
//! ADEVICE  <alsa device>
//! CHANNEL  0
//! MYCALL   <base callsign>
//! MODEM    1200
//! PTT      <CM108 hidraw | tty RTS>
//! KISSPORT <localhost port>
//! ```
//!
//! ## `MYCALL` is the BASE callsign — this layer never touches the SSID
//!
//! In KISS pass-through mode Dire Wolf does **not** drive the on-air source
//! address: tuxlink builds every AX.25 frame's source [`super::frame::Address`]
//! (callsign **and** SSID) itself in [`super::datalink`] and hands the fully
//! addressed frame to Dire Wolf to transmit verbatim. Dire Wolf's `MYCALL` is
//! used only for things tuxlink does not delegate to it (e.g. an APRS beacon,
//! which tuxlink does not configure here). It therefore takes the operator's
//! BASE callsign verbatim. **This layer does not parse, append, strip, or
//! validate an SSID** — it renders `mycall` exactly as the caller passes it. If
//! a caller hands `"N0CALL"`, the line is `MYCALL   N0CALL`; the connected-mode
//! `N0CALL-10` source address is set later by [`super::datalink`], on the wire,
//! not here.
//!
//! ## TNC timing / FEC params are absent on purpose
//!
//! `TXDELAY`, `PERSIST`, `SLOTTIME`, and any FX.25 / IL2P FEC directives are
//! **deliberately NOT emitted.** tuxlink pushes TXDELAY / persistence / slot time
//! to the TNC as KISS *parameter frames* (KISS command `0x01`/`0x02`/`0x03`) at
//! connect time — see [`super::datalink`]'s `push_kiss_params`. Setting them in
//! the conf as well would fight the wire: the conf value and the runtime KISS
//! frame would disagree, and the runtime frame is the one tuxlink controls. The
//! modem is plain AX.25 Bell-202 1200-baud packet (`MODEM 1200`); tuxlink's AX.25
//! stack implements neither FX.25 nor IL2P, so emitting those would advertise a
//! capability that does not exist. The tests below assert the ABSENCE of each.
//!
//! ## Pure string generation only
//!
//! This module generates a `String`. It does not write a file (Phase 4), it does
//! not run `direwolf -t 0` to validate (Phase 3), and it does not spawn the
//! process. Mirrors the pure-fn + dense-doc-comment style of `parse_alsa_devices`
//! in `ui_commands.rs` and of [`super::devices`].

use super::devices::PttChoice;

/// The inputs needed to render a managed-Dire-Wolf `direwolf.conf`. Every field
/// is rendered verbatim into one fixed directive; there is no parsing or
/// validation here (callers resolve these upstream — `adevice` from
/// [`super::devices::AudioDevice::alsa_plughw`], `ptt` from
/// [`super::devices::discover_ptt`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwParams {
    /// The ALSA device name for `ADEVICE`, e.g.
    /// `"plughw:CARD=Device,DEV=0"` — comes from
    /// [`super::devices::AudioDevice::alsa_plughw`].
    pub adevice: String,
    /// The operator's BASE callsign for `MYCALL`, e.g. `"N0CALL"`, with NO SSID.
    /// Rendered verbatim — see the module docs on the SSID contract: the SSID is
    /// set by tuxlink's AX.25 stack on the wire, never appended here.
    pub mycall: String,
    /// The resolved PTT keying method. Reuses [`super::devices::PttChoice`]
    /// (the same value [`super::devices::discover_ptt`] returns) — rendered to a
    /// `PTT` directive by [`render_ptt`].
    pub ptt: PttChoice,
    /// The localhost TCP port Dire Wolf serves KISS on (`KISSPORT`), which
    /// tuxlink's KISS link connects to.
    pub kiss_port: u16,
}

/// Render a [`PttChoice`] to the value that follows `PTT ` in `direwolf.conf`.
///
/// - [`PttChoice::Cm108Hid`] → `CM108 <hidraw_path>` (e.g. `CM108 /dev/hidraw3`).
/// - [`PttChoice::SerialRts`] → `<tty> RTS` (e.g. `/dev/ttyUSB0 RTS`).
///
/// One-to-one with the two `PttChoice` variants, which is why this layer reuses
/// `PttChoice` directly rather than defining a parallel directive enum.
fn render_ptt(ptt: &PttChoice) -> String {
    match ptt {
        PttChoice::Cm108Hid { hidraw_path } => format!("CM108 {hidraw_path}"),
        PttChoice::SerialRts { tty } => format!("{tty} RTS"),
    }
}

/// Generate the complete managed-Dire-Wolf `direwolf.conf` as a `String`.
///
/// Emits exactly the six directives documented at the module level, in that
/// fixed order, each directive name padded to a common column for readability,
/// and terminates with a single trailing newline. No TNC timing / FEC params are
/// emitted (tuxlink pushes those as runtime KISS frames — see module docs).
///
/// Pure over `params` — no I/O, no validation, no process spawn.
pub fn generate_direwolf_conf(params: &DwParams) -> String {
    format!(
        "ADEVICE  {adevice}\n\
         CHANNEL  0\n\
         MYCALL   {mycall}\n\
         MODEM    1200\n\
         PTT      {ptt}\n\
         KISSPORT {kiss_port}\n",
        adevice = params.adevice,
        mycall = params.mycall,
        ptt = render_ptt(&params.ptt),
        kiss_port = params.kiss_port,
    )
}

// ============================================================================
// Tests — pure string generation. No real /dev, no Dire Wolf, no radio.
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// CM108 HID PTT (the DRA-100 case): exact-string assertion of the full conf.
    #[test]
    fn cm108_conf_is_exact() {
        let params = DwParams {
            adevice: "plughw:CARD=DRA,DEV=0".into(),
            mycall: "N0CALL".into(),
            ptt: PttChoice::Cm108Hid {
                hidraw_path: "/dev/hidraw3".into(),
            },
            kiss_port: 8001,
        };
        let conf = generate_direwolf_conf(&params);
        let expected = "ADEVICE  plughw:CARD=DRA,DEV=0\n\
                        CHANNEL  0\n\
                        MYCALL   N0CALL\n\
                        MODEM    1200\n\
                        PTT      CM108 /dev/hidraw3\n\
                        KISSPORT 8001\n";
        assert_eq!(conf, expected);
    }

    /// Serial-RTS PTT (the DigiRig case): exact-string assertion of the full conf.
    #[test]
    fn serial_rts_conf_is_exact() {
        let params = DwParams {
            adevice: "plughw:CARD=Device,DEV=0".into(),
            mycall: "N0CALL".into(),
            ptt: PttChoice::SerialRts {
                tty: "/dev/ttyUSB0".into(),
            },
            kiss_port: 8001,
        };
        let conf = generate_direwolf_conf(&params);
        let expected = "ADEVICE  plughw:CARD=Device,DEV=0\n\
                        CHANNEL  0\n\
                        MYCALL   N0CALL\n\
                        MODEM    1200\n\
                        PTT      /dev/ttyUSB0 RTS\n\
                        KISSPORT 8001\n";
        assert_eq!(conf, expected);
    }

    /// The two PTT directive forms render exactly as Dire Wolf expects.
    #[test]
    fn render_ptt_maps_both_variants() {
        assert_eq!(
            render_ptt(&PttChoice::Cm108Hid {
                hidraw_path: "/dev/hidraw7".into()
            }),
            "CM108 /dev/hidraw7"
        );
        assert_eq!(
            render_ptt(&PttChoice::SerialRts {
                tty: "/dev/ttyUSB2".into()
            }),
            "/dev/ttyUSB2 RTS"
        );
    }

    /// MODEM is exactly `1200` — plain AX.25 Bell-202 packet, no 9600, no FEC.
    #[test]
    fn modem_is_1200_plain_packet() {
        let conf = generate_direwolf_conf(&DwParams {
            adevice: "plughw:CARD=Device,DEV=0".into(),
            mycall: "N0CALL".into(),
            ptt: PttChoice::SerialRts {
                tty: "/dev/ttyUSB0".into(),
            },
            kiss_port: 8001,
        });
        assert!(conf.contains("MODEM    1200\n"));
        assert!(!conf.contains("9600"));
    }

    /// SSID contract: a base call renders verbatim — this layer never appends an
    /// SSID. `"N0CALL"` in → `MYCALL   N0CALL` out, no `-7` / `-10` suffix.
    #[test]
    fn mycall_is_base_callsign_verbatim_no_ssid() {
        let conf = generate_direwolf_conf(&DwParams {
            adevice: "plughw:CARD=Device,DEV=0".into(),
            mycall: "N0CALL".into(),
            ptt: PttChoice::SerialRts {
                tty: "/dev/ttyUSB0".into(),
            },
            kiss_port: 8001,
        });
        assert!(conf.contains("MYCALL   N0CALL\n"));
        // No SSID appended by this layer (the connected-mode source SSID is set
        // by tuxlink's AX.25 stack on the wire, not in the conf).
        assert!(!conf.contains("N0CALL-"));
    }

    /// CRITICAL negative asserts: TNC timing + FEC params are NEVER in the conf;
    /// tuxlink pushes TXDELAY / persist / slot as KISS parameter frames at connect
    /// (`datalink::Ax25Stream::push_kiss_params`), and the AX.25 stack implements
    /// neither FX.25 nor IL2P. Emitting them here would fight the wire.
    #[test]
    fn conf_omits_tnc_timing_and_fec_params() {
        // Exercise both PTT variants so the absence holds regardless of PTT type.
        for ptt in [
            PttChoice::Cm108Hid {
                hidraw_path: "/dev/hidraw3".into(),
            },
            PttChoice::SerialRts {
                tty: "/dev/ttyUSB0".into(),
            },
        ] {
            let conf = generate_direwolf_conf(&DwParams {
                adevice: "plughw:CARD=Device,DEV=0".into(),
                mycall: "N0CALL".into(),
                ptt,
                kiss_port: 8001,
            });
            for forbidden in [
                "TXDELAY", "PERSIST", "SLOTTIME", "SLOT", "FX25", "FX.25", "IL2P",
            ] {
                assert!(
                    !conf.contains(forbidden),
                    "conf must not contain {forbidden}; tuxlink owns timing/FEC, not the conf:\n{conf}"
                );
            }
        }
    }

    /// The conf is exactly six lines and ends in a single trailing newline.
    #[test]
    fn conf_is_six_lines_with_single_trailing_newline() {
        let conf = generate_direwolf_conf(&DwParams {
            adevice: "plughw:CARD=Device,DEV=0".into(),
            mycall: "N0CALL".into(),
            ptt: PttChoice::Cm108Hid {
                hidraw_path: "/dev/hidraw3".into(),
            },
            kiss_port: 8001,
        });
        assert!(conf.ends_with('\n'));
        assert!(!conf.ends_with("\n\n"));
        // Six directive lines (split on '\n' yields 6 non-empty + 1 trailing "").
        let lines: Vec<&str> = conf.lines().collect();
        assert_eq!(lines.len(), 6);
    }
}
