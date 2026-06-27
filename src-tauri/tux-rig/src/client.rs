//! `rigctld` TCP client. One request line → one reply (set: `RPRT n`;
//! get: value line(s)). A persistent `BufReader` ensures bytes from a
//! multi-line reply (e.g. the two-line `m` response) are never lost between
//! calls.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::protocol::{self, CMD_GET_FREQ, CMD_GET_MODE, CMD_GET_PTT};
use crate::{Mode, Rig, RigError, RigStatus};

/// A connected rigctld control client.
///
/// Holds a persistent `BufReader` so that buffered bytes from a prior reply
/// are never lost between `exchange` calls.
pub struct RigctldClient {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl RigctldClient {
    /// Connect to a running rigctld at `host:port`.
    pub fn connect(host: &str, port: u16) -> Result<Self, RigError> {
        let stream = TcpStream::connect((host, port))?;
        let writer = stream.try_clone()?;
        let reader = BufReader::new(stream);
        Ok(Self { reader, writer })
    }

    /// Connect to a running rigctld at `host:port`, installing a read timeout on
    /// the underlying socket so a hung rigctld cannot wedge the caller's thread
    /// indefinitely.
    ///
    /// Used by the live-VFO poll thread (which runs an independent client on the
    /// DRA-100 keep-serial path): if rigctld stops answering, the next
    /// [`Rig::read_status`] read returns a `WouldBlock`/`TimedOut` I/O error
    /// instead of blocking forever, so the poll loop can observe the failure and
    /// exit. The managed client (`ManagedRig`) keeps the unbounded [`connect`]
    /// behavior — its calls are operator-synchronous and short-lived.
    ///
    /// The timeout governs each individual socket read. A multi-line reply (e.g.
    /// the two-line `m` response) is several reads, so a complete
    /// [`Rig::read_status`] round-trip can take up to a few multiples of
    /// `read_timeout` in the worst case; size the timeout with that in mind.
    pub fn connect_with_timeout(
        host: &str,
        port: u16,
        read_timeout: Duration,
    ) -> Result<Self, RigError> {
        let stream = TcpStream::connect((host, port))?;
        stream.set_read_timeout(Some(read_timeout))?;
        let writer = stream.try_clone()?;
        let reader = BufReader::new(stream);
        Ok(Self { reader, writer })
    }

    /// Send one command and return the first reply line, trimmed.
    fn exchange(&mut self, cmd: &str) -> Result<String, RigError> {
        self.writer.write_all(cmd.as_bytes())?;
        self.writer.flush()?;
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
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
        // rigctld answers `m` with two lines: mode then passband; consume the passband.
        let mode_line = self.exchange(CMD_GET_MODE)?;
        let mode = Mode::from_rigctl(&mode_line);
        let mut passband = String::new();
        self.reader.read_line(&mut passband)?;
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
                    // PTT=1 so that if the passband line ("3000") is mis-read as the
                    // `t` reply the assertion `ptt == true` fails (3000 != "1").
                    Some('t') => "1\n".to_string(),
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
        // PTT is 1 in the fake; if the passband line ("3000") leaked into the `t`
        // exchange, ptt would parse as false (trim "3000" != "1") — catching the bug.
        assert!(s.ptt);
    }

    #[test]
    fn connect_with_timeout_still_round_trips() {
        let port = fake_rigctld();
        // A generous timeout: the fake answers promptly, so the read completes
        // well inside it. The point is that installing a read timeout does not
        // break a normal status round-trip — the poll thread's bounded client
        // reads the same values the unbounded client would.
        let mut c = RigctldClient::connect_with_timeout(
            "127.0.0.1",
            port,
            Duration::from_secs(2),
        )
        .unwrap();
        let s = c.read_status().unwrap();
        assert_eq!(s.freq_hz, 7_102_000);
        assert_eq!(s.mode, Some(Mode::PktUsb));
        assert!(s.ptt);
    }
}
