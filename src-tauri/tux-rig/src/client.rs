//! `rigctld` TCP client. One request line → one reply (set: `RPRT n`;
//! get: value line(s)). The client opens a short-lived line exchange per call.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::protocol::{self, CMD_GET_FREQ, CMD_GET_MODE, CMD_GET_PTT};
use crate::{Mode, Rig, RigError, RigStatus};

/// A connected rigctld control client.
pub struct RigctldClient {
    stream: TcpStream,
}

impl RigctldClient {
    /// Connect to a running rigctld at `host:port`.
    pub fn connect(host: &str, port: u16) -> Result<Self, RigError> {
        let stream = TcpStream::connect((host, port))?;
        Ok(Self { stream })
    }

    /// Send one command and return the first reply line, trimmed.
    fn exchange(&mut self, cmd: &str) -> Result<String, RigError> {
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.flush()?;
        let mut reader = BufReader::new(&self.stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        Ok(line.trim_end().to_string())
    }
}

impl Rig for RigctldClient {
    fn set_freq(&mut self, hz: u64) -> Result<(), RigError> {
        let reply = self.exchange(&protocol::cmd_set_freq(hz))?;
        protocol::parse_rprt(&reply)
    }

    fn set_mode(&mut self, mode: Mode) -> Result<(), RigError> {
        let reply = self.exchange(&protocol::cmd_set_mode(mode))?;
        protocol::parse_rprt(&reply)
    }

    fn ptt(&mut self, on: bool) -> Result<(), RigError> {
        let reply = self.exchange(&protocol::cmd_set_ptt(on))?;
        protocol::parse_rprt(&reply)
    }

    fn read_status(&mut self) -> Result<RigStatus, RigError> {
        let freq = protocol::parse_freq(&self.exchange(CMD_GET_FREQ)?)?;
        let mode = Mode::from_rigctl(&self.exchange(CMD_GET_MODE)?);
        let ptt = self.exchange(CMD_GET_PTT)?.trim() == "1";
        Ok(RigStatus { freq_hz: freq, mode, ptt })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    /// Spawn a one-shot fake rigctld that answers `set` with `RPRT 0` and the
    /// three getters with fixed values. Returns the bound port.
    fn fake_rigctld() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 {
                let cmd = line.trim_end();
                let reply = match cmd.chars().next() {
                    Some('F') | Some('M') | Some('T') => "RPRT 0\n".to_string(),
                    Some('f') => "7102000\n".to_string(),
                    Some('m') => "PKTUSB\n3000\n".to_string(),
                    Some('t') => "0\n".to_string(),
                    _ => "RPRT -1\n".to_string(),
                };
                writer.write_all(reply.as_bytes()).unwrap();
                writer.flush().unwrap();
                line.clear();
            }
        });
        port
    }

    #[test]
    fn set_freq_succeeds_against_fake() {
        let port = fake_rigctld();
        let mut c = RigctldClient::connect("127.0.0.1", port).unwrap();
        c.set_freq(7_102_000).unwrap();
    }

    #[test]
    fn set_mode_succeeds_against_fake() {
        let port = fake_rigctld();
        let mut c = RigctldClient::connect("127.0.0.1", port).unwrap();
        c.set_mode(Mode::PktUsb).unwrap();
    }

    #[test]
    fn read_status_parses_freq_and_mode() {
        let port = fake_rigctld();
        let mut c = RigctldClient::connect("127.0.0.1", port).unwrap();
        let s = c.read_status().unwrap();
        assert_eq!(s.freq_hz, 7_102_000);
        assert_eq!(s.mode, Some(Mode::PktUsb));
        assert!(!s.ptt);
    }
}
