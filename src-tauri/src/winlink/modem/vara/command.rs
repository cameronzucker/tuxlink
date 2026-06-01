//! VARA TCP command-socket message types — parse inbound modem messages
//! and encode outbound setter commands.
//!
//! VARA wire format: each command is an ASCII line terminated by `\r`
//! (carriage return). Inbound and outbound use the same syntax. The
//! command socket carries control + status; the separate data socket
//! (cmd_port + 1) carries the connected-mode byte stream.

/// VARA bandwidth selector. Three modes are commonly supported:
/// VARA HF Narrow (500 Hz), Standard (2300 Hz), and Wide / Tactical
/// (2750 Hz). The wire form is `BW500`, `BW2300`, or `BW2750`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    /// 500 Hz narrow mode.
    Bw500,
    /// 2300 Hz standard mode.
    Bw2300,
    /// 2750 Hz wide / tactical mode.
    Bw2750,
}

impl Bandwidth {
    /// Wire-form token (e.g. `BW2300`).
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Bw500 => "BW500",
            Self::Bw2300 => "BW2300",
            Self::Bw2750 => "BW2750",
        }
    }

    /// Hz value of this bandwidth.
    pub fn hz(self) -> u32 {
        match self {
            Self::Bw500 => 500,
            Self::Bw2300 => 2300,
            Self::Bw2750 => 2750,
        }
    }
}

/// Outbound command (client → VARA). Encoded via [`OutboundCommand::as_wire`]
/// (returns the line WITHOUT the trailing `\r`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundCommand {
    /// `MYCALL <callsign>` — set the operator's callsign.
    MyCall(String),
    /// `MYAUX <call,call,...>` — set secondary callsigns.
    MyAux(Vec<String>),
    /// `BW500` / `BW2300` / `BW2750` — set bandwidth.
    Bw(Bandwidth),
    /// `LISTEN ON` / `LISTEN OFF` — toggle listen mode.
    Listen(bool),
    /// `CONNECT <mycall> <target>` — initiate ARQ connection. The
    /// `target` is the peer callsign (Winlink RMS or peer station).
    Connect {
        /// Local callsign (must match a previously-set `MYCALL`).
        mycall: String,
        /// Peer callsign to dial.
        target: String,
    },
    /// `DISCONNECT` — graceful tear-down of the current ARQ link.
    Disconnect,
    /// `ABORT` — hard tear-down (interrupts any in-flight TX).
    Abort,
    /// `COMPRESSION <mode>` — set payload compression (`TEXT`,
    /// `BINARY`, `AUTO`, `OFF`).
    Compression(Compression),
    /// `CWID ON/OFF` — toggle CW identifier transmission.
    CwId(bool),
    /// `PUBLIC ON/OFF` — toggle public mode (advertised on busy
    /// channels).
    Public(bool),
    /// Arbitrary verbatim command (escape hatch for commands the
    /// enum doesn't model yet).
    Raw(String),
}

/// VARA payload compression mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Plain text compression (LZ-class for ASCII).
    Text,
    /// Binary compression.
    Binary,
    /// Auto-select per payload.
    Auto,
    /// No compression.
    Off,
}

impl Compression {
    /// Wire-form keyword.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Text => "TEXT",
            Self::Binary => "BINARY",
            Self::Auto => "AUTO",
            Self::Off => "OFF",
        }
    }
}

impl OutboundCommand {
    /// Render to its wire-form ASCII line, WITHOUT the trailing `\r`.
    /// The caller appends the terminator before writing to the socket.
    pub fn as_wire(&self) -> String {
        match self {
            Self::MyCall(call) => format!("MYCALL {call}"),
            Self::MyAux(calls) => format!("MYAUX {}", calls.join(",")),
            Self::Bw(bw) => bw.as_wire().to_string(),
            Self::Listen(true) => "LISTEN ON".into(),
            Self::Listen(false) => "LISTEN OFF".into(),
            Self::Connect { mycall, target } => format!("CONNECT {mycall} {target}"),
            Self::Disconnect => "DISCONNECT".into(),
            Self::Abort => "ABORT".into(),
            Self::Compression(c) => format!("COMPRESSION {}", c.as_wire()),
            Self::CwId(true) => "CWID ON".into(),
            Self::CwId(false) => "CWID OFF".into(),
            Self::Public(true) => "PUBLIC ON".into(),
            Self::Public(false) => "PUBLIC OFF".into(),
            Self::Raw(s) => s.clone(),
        }
    }
}

/// Inbound message received on the VARA cmd socket. Covers the
/// variants observed across VARA HF, VARA FM, and VARA Satellite as
/// of v4.x; unknown tokens map to [`InboundCommand::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundCommand {
    /// `READY` — modem ready for commands (sent after startup
    /// handshake completes).
    Ready,
    /// `CONNECTED <mycall> <target> [bw]` — ARQ link established.
    Connected {
        /// Local callsign (matches the MYCALL set before CONNECT).
        mycall: String,
        /// Peer callsign on the established ARQ link.
        target: String,
        /// Negotiated bandwidth in Hz, if VARA reports it.
        bandwidth_hz: Option<u32>,
    },
    /// `DISCONNECTED` — ARQ link torn down.
    Disconnected,
    /// `PTT ON` / `PTT OFF` — modem's request to assert / release PTT.
    Ptt(bool),
    /// `BUFFER <bytes>` — TX buffer fill report.
    Buffer(u32),
    /// `PENDING` — connection request in progress.
    Pending,
    /// `CANCELPENDING` — pending connection request canceled.
    CancelPending,
    /// `LINK REGISTERED` — registration with the link-layer succeeded.
    LinkRegistered,
    /// `IAMALIVE` — keep-alive ping from the modem.
    IAmAlive,
    /// `MISSING SOUNDCARD` — modem cannot find the configured audio
    /// device.
    MissingSoundcard,
    /// `WRONG CALLSIGN` — registration rejected the supplied callsign.
    WrongCallsign,
    /// `OFFLINE` — modem reports it's offline.
    Offline,
    /// Unrecognized command, captured verbatim for forensics.
    Unknown(String),
}

/// Error parsing a VARA cmd-socket line.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommandParseError {
    /// Input was empty after trimming.
    #[error("empty command line")]
    Empty,
    /// A command's arguments were malformed.
    #[error("malformed value for {cmd}: {detail}")]
    Malformed {
        /// Command name whose arguments failed parsing.
        cmd: String,
        /// Detail about what failed.
        detail: String,
    },
}

impl InboundCommand {
    /// Parse one VARA cmd-socket line (without the trailing `\r`).
    /// Tolerates leading/trailing whitespace.
    pub fn parse(line: &str) -> Result<Self, CommandParseError> {
        let line = line.trim();
        if line.is_empty() {
            return Err(CommandParseError::Empty);
        }
        let mut parts = line.splitn(2, ' ');
        let head = parts.next().unwrap_or("").to_ascii_uppercase();
        let rest = parts.next().map(str::trim);

        Ok(match head.as_str() {
            "READY" => Self::Ready,
            "DISCONNECTED" => Self::Disconnected,
            "PENDING" => Self::Pending,
            "CANCELPENDING" => Self::CancelPending,
            "IAMALIVE" => Self::IAmAlive,
            "OFFLINE" => Self::Offline,
            "PTT" => match rest {
                Some("ON") | Some("on") => Self::Ptt(true),
                Some("OFF") | Some("off") => Self::Ptt(false),
                Some(other) => {
                    return Err(CommandParseError::Malformed {
                        cmd: "PTT".into(),
                        detail: format!("expected ON or OFF, got {other:?}"),
                    });
                }
                None => {
                    return Err(CommandParseError::Malformed {
                        cmd: "PTT".into(),
                        detail: "missing ON/OFF".into(),
                    });
                }
            },
            "BUFFER" => {
                let bytes = rest
                    .and_then(|s| s.parse::<u32>().ok())
                    .ok_or_else(|| CommandParseError::Malformed {
                        cmd: "BUFFER".into(),
                        detail: format!("non-integer arg: {rest:?}"),
                    })?;
                Self::Buffer(bytes)
            }
            "CONNECTED" => {
                let rest = rest.ok_or_else(|| CommandParseError::Malformed {
                    cmd: "CONNECTED".into(),
                    detail: "missing args".into(),
                })?;
                let tokens: Vec<&str> = rest.split_whitespace().collect();
                if tokens.len() < 2 {
                    return Err(CommandParseError::Malformed {
                        cmd: "CONNECTED".into(),
                        detail: format!("need at least 2 args (mycall target), got {tokens:?}"),
                    });
                }
                let bandwidth_hz = tokens.get(2).and_then(|t| t.parse::<u32>().ok());
                Self::Connected {
                    mycall: tokens[0].to_string(),
                    target: tokens[1].to_string(),
                    bandwidth_hz,
                }
            }
            "LINK" => match rest {
                Some(rest) if rest.eq_ignore_ascii_case("REGISTERED") => Self::LinkRegistered,
                _ => Self::Unknown(line.to_string()),
            },
            "MISSING" => match rest {
                Some(rest) if rest.eq_ignore_ascii_case("SOUNDCARD") => Self::MissingSoundcard,
                _ => Self::Unknown(line.to_string()),
            },
            "WRONG" => match rest {
                Some(rest) if rest.eq_ignore_ascii_case("CALLSIGN") => Self::WrongCallsign,
                _ => Self::Unknown(line.to_string()),
            },
            _ => Self::Unknown(line.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_mycall_renders() {
        assert_eq!(OutboundCommand::MyCall("N0CALL".into()).as_wire(), "MYCALL N0CALL");
    }

    #[test]
    fn outbound_bw_variants() {
        assert_eq!(OutboundCommand::Bw(Bandwidth::Bw500).as_wire(), "BW500");
        assert_eq!(OutboundCommand::Bw(Bandwidth::Bw2300).as_wire(), "BW2300");
        assert_eq!(OutboundCommand::Bw(Bandwidth::Bw2750).as_wire(), "BW2750");
    }

    #[test]
    fn outbound_connect_renders() {
        let c = OutboundCommand::Connect {
            mycall: "N0CALL".into(),
            target: "W1AW".into(),
        };
        assert_eq!(c.as_wire(), "CONNECT N0CALL W1AW");
    }

    #[test]
    fn outbound_myaux_joins_with_comma() {
        let c = OutboundCommand::MyAux(vec!["AA1A".into(), "AA1B".into(), "AA1C".into()]);
        assert_eq!(c.as_wire(), "MYAUX AA1A,AA1B,AA1C");
    }

    #[test]
    fn outbound_compression_modes() {
        assert_eq!(
            OutboundCommand::Compression(Compression::Auto).as_wire(),
            "COMPRESSION AUTO"
        );
        assert_eq!(
            OutboundCommand::Compression(Compression::Off).as_wire(),
            "COMPRESSION OFF"
        );
    }

    #[test]
    fn inbound_ready() {
        assert_eq!(InboundCommand::parse("READY").unwrap(), InboundCommand::Ready);
    }

    #[test]
    fn inbound_ptt_on_off() {
        assert_eq!(InboundCommand::parse("PTT ON").unwrap(), InboundCommand::Ptt(true));
        assert_eq!(InboundCommand::parse("PTT OFF").unwrap(), InboundCommand::Ptt(false));
    }

    #[test]
    fn inbound_buffer_parses_integer() {
        assert_eq!(InboundCommand::parse("BUFFER 1234").unwrap(), InboundCommand::Buffer(1234));
    }

    #[test]
    fn inbound_connected_with_bw() {
        let parsed = InboundCommand::parse("CONNECTED N0CALL W1AW 2300").unwrap();
        assert_eq!(
            parsed,
            InboundCommand::Connected {
                mycall: "N0CALL".into(),
                target: "W1AW".into(),
                bandwidth_hz: Some(2300),
            }
        );
    }

    #[test]
    fn inbound_connected_without_bw() {
        let parsed = InboundCommand::parse("CONNECTED N0CALL W1AW").unwrap();
        assert_eq!(
            parsed,
            InboundCommand::Connected {
                mycall: "N0CALL".into(),
                target: "W1AW".into(),
                bandwidth_hz: None,
            }
        );
    }

    #[test]
    fn inbound_link_registered() {
        assert_eq!(
            InboundCommand::parse("LINK REGISTERED").unwrap(),
            InboundCommand::LinkRegistered
        );
    }

    #[test]
    fn inbound_iamalive() {
        assert_eq!(InboundCommand::parse("IAMALIVE").unwrap(), InboundCommand::IAmAlive);
    }

    #[test]
    fn inbound_unknown_captured_verbatim() {
        assert_eq!(
            InboundCommand::parse("SOMETHING NOVEL").unwrap(),
            InboundCommand::Unknown("SOMETHING NOVEL".into())
        );
    }

    #[test]
    fn inbound_empty_is_error() {
        assert_eq!(InboundCommand::parse("").unwrap_err(), CommandParseError::Empty);
        assert_eq!(InboundCommand::parse("   ").unwrap_err(), CommandParseError::Empty);
    }

    #[test]
    fn inbound_tolerates_whitespace() {
        assert_eq!(
            InboundCommand::parse("  READY  ").unwrap(),
            InboundCommand::Ready
        );
    }
}
