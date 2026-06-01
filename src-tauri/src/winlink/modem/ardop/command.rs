//! ARDOP command-socket message types — parse inbound TNC messages and encode
//! outbound setter commands.
//!
//! Parsing follows the rules in wl2k-go `transport/ardop/command.go`
//! (`parseCtrlMsg`): trim whitespace; tolerate the `now ` echo-back prefix;
//! match the uppercased command token to a known variant.

/// TNC state as reported via `NEWSTATE` messages.
///
/// Mirrors wl2k-go's `State` type and `stateMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Offline,
    Disc,
    Idle,
    /// Information Sending Station.
    Iss,
    /// Information Receiving Station.
    Irs,
    FecSend,
    FecRcv,
}

impl State {
    fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "OFFLINE" => State::Offline,
            "DISC" => State::Disc,
            "IDLE" => State::Idle,
            "ISS" => State::Iss,
            "IRS" => State::Irs,
            "FECSEND" => State::FecSend,
            "FECRCV" => State::FecRcv,
            _ => return None,
        })
    }
}

/// Inbound message received on the ARDOP cmd socket.
///
/// Covers the variants used by the connect/exchange/disconnect flow.
/// Echo-backs acknowledge outbound setter commands (TNC repeats the command
/// name to confirm it was applied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    NewState(State),
    Connected { peer_call: String, bandwidth_hz: u32 },
    Disconnected,
    Fault(String),
    Ptt(bool),
    Buffer(u32),
    Busy(bool),
    Status(String),
    /// `PINGACK <SNdB> <Quality>` — ardopcf's response to an operator-issued
    /// ping. `sn_db` is signed (negative on poor links); `quality` is
    /// ardopcf's 0..=100 score derived from the ping decoder's
    /// confidence measurement. Closes tuxlink-1637.
    PingAck { sn_db: i32, quality: u32 },
    /// `PING <caller>><target> <SNdB> <Quality>` — ardopcf decoded an
    /// incoming PING frame addressed to (or witnessed by) the local TNC.
    /// caller/target are amateur callsigns or self-IDs without the '>'
    /// separator. Closes tuxlink-1637.
    Ping {
        caller: String,
        target: String,
        sn_db: i32,
        quality: u32,
    },
    /// Echo-back acknowledgment of a setter command.
    EchoBack(String),
}

/// Error parsing an ARDOP cmd-socket line.
#[derive(Debug, thiserror::Error)]
pub enum CommandParseError {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("malformed value for {cmd}: {detail}")]
    Malformed { cmd: String, detail: String },
}

impl Command {
    /// Parse one cmd-socket line (without the trailing `\r`).
    ///
    /// Tolerates wl2k-go's observed quirks:
    /// - Leading/trailing whitespace (ARDOPc adds a trailing space to NEWSTATE).
    /// - `now <value>` echo-back prefix on setter acknowledgments.
    pub fn parse(line: &str) -> Result<Self, CommandParseError> {
        let line = line.trim();
        let mut parts = line.splitn(2, ' ');
        let head = parts.next().unwrap_or("").to_ascii_uppercase();
        // Strip the wl2k-go "now " echo-back prefix if present.
        let rest = parts
            .next()
            .map(|s| s.trim_start_matches("now ").trim_start_matches("NOW "));

        match head.as_str() {
            "NEWSTATE" => {
                let tok = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "NEWSTATE".into(),
                    detail: "missing state token".into(),
                })?;
                let st =
                    State::from_token(&tok.trim().to_ascii_uppercase()).ok_or_else(|| {
                        CommandParseError::Malformed {
                            cmd: "NEWSTATE".into(),
                            detail: format!("unknown state: {tok}"),
                        }
                    })?;
                Ok(Command::NewState(st))
            }
            "CONNECTED" => {
                let rest = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "CONNECTED".into(),
                    detail: "missing args".into(),
                })?;
                let mut toks = rest.split_whitespace();
                let peer_call =
                    toks.next()
                        .ok_or_else(|| CommandParseError::Malformed {
                            cmd: "CONNECTED".into(),
                            detail: "missing peer call".into(),
                        })?
                        .to_string();
                let bw = toks
                    .next()
                    .unwrap_or("0")
                    .parse::<u32>()
                    .map_err(|e| CommandParseError::Malformed {
                        cmd: "CONNECTED".into(),
                        detail: e.to_string(),
                    })?;
                Ok(Command::Connected {
                    peer_call,
                    bandwidth_hz: bw,
                })
            }
            "DISCONNECTED" => Ok(Command::Disconnected),
            "FAULT" => Ok(Command::Fault(rest.unwrap_or("").to_string())),
            "PTT" => Ok(Command::Ptt(
                rest.map(|s| s.trim().eq_ignore_ascii_case("TRUE"))
                    .unwrap_or(false),
            )),
            "BUSY" => Ok(Command::Busy(
                rest.map(|s| s.trim().eq_ignore_ascii_case("TRUE"))
                    .unwrap_or(false),
            )),
            "BUFFER" => {
                let n = rest
                    .unwrap_or("0")
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<u32>()
                    .map_err(|e| CommandParseError::Malformed {
                        cmd: "BUFFER".into(),
                        detail: e.to_string(),
                    })?;
                Ok(Command::Buffer(n))
            }
            "STATUS" => Ok(Command::Status(rest.unwrap_or("").to_string())),
            "PINGACK" => {
                // "PINGACK <SNdB> <Quality>" — both ints, SN may be negative.
                let rest = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PINGACK".into(),
                    detail: "missing args".into(),
                })?;
                let mut toks = rest.split_whitespace();
                let sn_db_tok = toks.next().ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PINGACK".into(),
                    detail: "missing SN dB".into(),
                })?;
                let sn_db: i32 = sn_db_tok.parse().map_err(|e: std::num::ParseIntError| {
                    CommandParseError::Malformed {
                        cmd: "PINGACK".into(),
                        detail: format!("SN dB: {e}"),
                    }
                })?;
                let q_tok = toks.next().ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PINGACK".into(),
                    detail: "missing quality".into(),
                })?;
                let quality: u32 = q_tok.parse().map_err(|e: std::num::ParseIntError| {
                    CommandParseError::Malformed {
                        cmd: "PINGACK".into(),
                        detail: format!("quality: {e}"),
                    }
                })?;
                Ok(Command::PingAck { sn_db, quality })
            }
            "PING" => {
                // "PING <caller>><target> <SNdB> <Quality>". The first
                // whitespace-delimited token is the callsign pair joined by
                // '>'. Without the '>' the line is malformed.
                let rest = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PING".into(),
                    detail: "missing args".into(),
                })?;
                let mut toks = rest.split_whitespace();
                let cg = toks.next().ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PING".into(),
                    detail: "missing caller>target".into(),
                })?;
                let (caller, target) =
                    cg.split_once('>').ok_or_else(|| CommandParseError::Malformed {
                        cmd: "PING".into(),
                        detail: format!("expected caller>target, got: {cg}"),
                    })?;
                let sn_db_tok = toks.next().ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PING".into(),
                    detail: "missing SN dB".into(),
                })?;
                let sn_db: i32 = sn_db_tok.parse().map_err(|e: std::num::ParseIntError| {
                    CommandParseError::Malformed {
                        cmd: "PING".into(),
                        detail: format!("SN dB: {e}"),
                    }
                })?;
                let q_tok = toks.next().ok_or_else(|| CommandParseError::Malformed {
                    cmd: "PING".into(),
                    detail: "missing quality".into(),
                })?;
                let quality: u32 = q_tok.parse().map_err(|e: std::num::ParseIntError| {
                    CommandParseError::Malformed {
                        cmd: "PING".into(),
                        detail: format!("quality: {e}"),
                    }
                })?;
                Ok(Command::Ping {
                    caller: caller.to_string(),
                    target: target.to_string(),
                    sn_db,
                    quality,
                })
            }
            // Setter echo-backs: TNC echoes the command name to acknowledge.
            other if is_setter_echo_back(other) => Ok(Command::EchoBack(other.to_string())),
            _ => Err(CommandParseError::Unknown(head)),
        }
    }
}

/// Return true if `cmd` is one of the outbound setter commands that the TNC
/// acknowledges by echoing the command name back.
fn is_setter_echo_back(cmd: &str) -> bool {
    matches!(
        cmd,
        "INITIALIZE"
            | "MYCALL"
            | "GRIDSQUARE"
            | "PROTOCOLMODE"
            | "ARQTIMEOUT"
            | "ARQCALL"
            | "ARQBW"
            | "CODEC"
            | "LISTEN"
            | "DRIVELEVEL"
    )
}

/// Encode an outbound setter command for the ARDOP cmd socket.
///
/// Returns the wire string *without* the trailing `\r` — call
/// `wire::encode_cmd_line` to add that terminator before sending.
pub fn encode_setter(cmd: &str, arg: Option<&str>) -> String {
    match arg {
        Some(v) => format!("{cmd} {v}"),
        None => cmd.to_string(),
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_newstate_with_known_state() {
        // wl2k-go: cmdNewState parses parts[1] via stateMap -> State.
        let msg = Command::parse("NEWSTATE DISC").unwrap();
        assert!(matches!(msg, Command::NewState(State::Disc)));
    }

    #[test]
    fn parses_newstate_all_variants() {
        let cases = [
            ("NEWSTATE OFFLINE", State::Offline),
            ("NEWSTATE DISC", State::Disc),
            ("NEWSTATE IDLE", State::Idle),
            ("NEWSTATE ISS", State::Iss),
            ("NEWSTATE IRS", State::Irs),
            ("NEWSTATE FECSEND", State::FecSend),
            ("NEWSTATE FECRCV", State::FecRcv),
        ];
        for (input, expected) in cases {
            let msg = Command::parse(input).unwrap();
            assert!(
                matches!(msg, Command::NewState(s) if s == expected),
                "failed for input: {input}"
            );
        }
    }

    #[test]
    fn parses_newstate_trims_trailing_space() {
        // ARDOPc known quirk: trailing space on NEWSTATE lines.
        let msg = Command::parse("NEWSTATE DISC ").unwrap();
        assert!(matches!(msg, Command::NewState(State::Disc)));
    }

    #[test]
    fn parses_connected_call_and_bandwidth() {
        // "CONNECTED W7ABC 500" -> peer_call=W7ABC, bandwidth_hz=500
        let msg = Command::parse("CONNECTED W7ABC 500").unwrap();
        assert!(
            matches!(msg, Command::Connected { ref peer_call, bandwidth_hz: 500 } if peer_call == "W7ABC")
        );
    }

    #[test]
    fn parses_fault_carries_message() {
        let msg = Command::parse("FAULT not from state IRS").unwrap();
        assert!(matches!(msg, Command::Fault(ref s) if s == "not from state IRS"));
    }

    #[test]
    fn parses_ptt_bool() {
        assert!(matches!(Command::parse("PTT TRUE").unwrap(), Command::Ptt(true)));
        assert!(matches!(Command::parse("PTT FALSE").unwrap(), Command::Ptt(false)));
        // Case-insensitive
        assert!(matches!(Command::parse("PTT true").unwrap(), Command::Ptt(true)));
    }

    #[test]
    fn parses_buffer_int() {
        // BUFFER carries TNC outbound-queue stats; first int is bytes-pending.
        assert!(matches!(Command::parse("BUFFER 0").unwrap(), Command::Buffer(0)));
        assert!(matches!(Command::parse("BUFFER 1024").unwrap(), Command::Buffer(1024)));
    }

    #[test]
    fn parses_busy_bool() {
        assert!(matches!(Command::parse("BUSY TRUE").unwrap(), Command::Busy(true)));
        assert!(matches!(Command::parse("BUSY FALSE").unwrap(), Command::Busy(false)));
    }

    #[test]
    fn parses_status_carries_string() {
        let msg = Command::parse("STATUS CONNECT TO LA3F FAILED!").unwrap();
        assert!(matches!(msg, Command::Status(ref s) if s == "CONNECT TO LA3F FAILED!"));
    }

    #[test]
    fn parses_disconnected_no_args() {
        assert!(matches!(Command::parse("DISCONNECTED").unwrap(), Command::Disconnected));
    }

    #[test]
    fn parses_echo_backs_for_all_setters() {
        let setters = [
            "INITIALIZE",
            "MYCALL",
            "GRIDSQUARE",
            "PROTOCOLMODE",
            "ARQTIMEOUT",
            "ARQCALL",
            "ARQBW",
            "CODEC",
            "LISTEN",
            "DRIVELEVEL",
        ];
        for setter in setters {
            let msg = Command::parse(setter).unwrap();
            assert!(
                matches!(msg, Command::EchoBack(ref s) if s == setter),
                "EchoBack not matched for: {setter}"
            );
        }
    }

    #[test]
    fn parses_echo_back_with_now_prefix() {
        // wl2k-go strips "now " from the value: "MYCALL now N7CPZ" -> EchoBack
        let msg = Command::parse("MYCALL now N7CPZ").unwrap();
        assert!(matches!(msg, Command::EchoBack(ref s) if s == "MYCALL"));
    }

    #[test]
    fn unknown_command_yields_an_error() {
        assert!(Command::parse("MYSTERY 123").is_err());
    }

    // ── tuxlink-1637: PING / PINGACK parsing ────────────────────────────

    #[test]
    fn parses_pingack_with_sn_and_quality() {
        // ardopcf emits "PINGACK SNdB Quality" in response to an
        // operator-issued ping. Both values are space-separated ints; SN
        // may be negative on poor links.
        let parsed = Command::parse("PINGACK 12 87").unwrap();
        match parsed {
            Command::PingAck { sn_db, quality } => {
                assert_eq!(sn_db, 12);
                assert_eq!(quality, 87);
            }
            other => panic!("expected PingAck, got {other:?}"),
        }
    }

    #[test]
    fn parses_pingack_with_negative_sn() {
        // Negative S/N is common on noisy links — the parser MUST accept
        // it without rejecting the whole line.
        let parsed = Command::parse("PINGACK -3 42").unwrap();
        match parsed {
            Command::PingAck { sn_db, quality } => {
                assert_eq!(sn_db, -3);
                assert_eq!(quality, 42);
            }
            other => panic!("expected PingAck, got {other:?}"),
        }
    }

    #[test]
    fn parses_ping_with_caller_target_sn_quality() {
        // ardopcf emits "PING caller>target SNdB Quality" when an
        // incoming ping is decoded. The caller>target token uses '>'
        // as the separator.
        let parsed = Command::parse("PING W4PHS>W7RMS 10 75").unwrap();
        match parsed {
            Command::Ping {
                caller,
                target,
                sn_db,
                quality,
            } => {
                assert_eq!(caller, "W4PHS");
                assert_eq!(target, "W7RMS");
                assert_eq!(sn_db, 10);
                assert_eq!(quality, 75);
            }
            other => panic!("expected Ping, got {other:?}"),
        }
    }

    #[test]
    fn ping_missing_caller_target_separator_errs() {
        // Without the '>' the line is malformed.
        assert!(Command::parse("PING W4PHS 10 75").is_err());
    }

    #[test]
    fn pingack_with_non_numeric_quality_errs() {
        assert!(Command::parse("PINGACK 12 xx").is_err());
    }

    #[test]
    fn leading_whitespace_is_tolerated() {
        let msg = Command::parse("  NEWSTATE DISC  ").unwrap();
        assert!(matches!(msg, Command::NewState(State::Disc)));
    }
}

#[cfg(test)]
mod encode_tests {
    use super::*;

    #[test]
    fn encode_setter_with_arg() {
        assert_eq!(encode_setter("MYCALL", Some("N7CPZ")), "MYCALL N7CPZ");
        assert_eq!(encode_setter("ARQCALL", Some("W7ABC 3")), "ARQCALL W7ABC 3");
    }

    #[test]
    fn encode_setter_no_arg() {
        assert_eq!(encode_setter("INITIALIZE", None), "INITIALIZE");
    }

    #[test]
    fn encode_setter_arq_timeout() {
        assert_eq!(encode_setter("ARQTIMEOUT", Some("30")), "ARQTIMEOUT 30");
    }
}
