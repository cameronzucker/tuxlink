//! VARA cmd-socket wire framing — `\r`-terminated ASCII lines.
//!
//! The command socket carries one command per line. VARA uses `\r`
//! (CR, 0x0D) as the terminator; `\n` (LF) is NOT used. The line
//! reader here accumulates bytes until it sees `\r`, then emits the
//! line (without the terminator).
//!
//! Inbound buffering follows the same pattern wl2k-go uses for ARDOP
//! (which is `\r`-terminated as well): pull bytes via blocking
//! reads, split on `\r`, return complete lines.

use std::io::{self, BufRead, BufReader, Read, Write};

const CR: u8 = b'\r';

/// Reader for `\r`-terminated VARA command-socket lines.
pub struct LineReader<R: Read> {
    inner: BufReader<R>,
}

impl<R: Read> LineReader<R> {
    /// Wrap a blocking byte stream with a line-buffered reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner: BufReader::new(inner),
        }
    }

    /// Read one `\r`-terminated line. Returns the line content
    /// without the terminator. Returns `Ok(None)` on EOF.
    pub fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut buf: Vec<u8> = Vec::new();
        let n = self.inner.read_until(CR, &mut buf)?;
        if n == 0 {
            return Ok(None);
        }
        // Strip the trailing `\r` if present (it always is, except on
        // a premature EOF mid-line — in which case we still return the
        // partial bytes so the caller can decide what to do).
        if buf.last() == Some(&CR) {
            buf.pop();
        }
        let line = String::from_utf8(buf).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("VARA cmd line is not UTF-8: {e}"),
            )
        })?;
        Ok(Some(line))
    }
}

/// Write one VARA command line, appending the `\r` terminator.
pub fn write_line<W: Write>(w: &mut W, line: &str) -> io::Result<()> {
    w.write_all(line.as_bytes())?;
    w.write_all(&[CR])?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_line_strips_cr() {
        let data = b"READY\rPTT ON\rDISCONNECTED\r";
        let mut reader = LineReader::new(Cursor::new(data));
        assert_eq!(reader.read_line().unwrap().as_deref(), Some("READY"));
        assert_eq!(reader.read_line().unwrap().as_deref(), Some("PTT ON"));
        assert_eq!(reader.read_line().unwrap().as_deref(), Some("DISCONNECTED"));
        assert_eq!(reader.read_line().unwrap(), None); // EOF
    }

    #[test]
    fn read_line_handles_partial_eof() {
        // No trailing CR — should still return the partial content.
        let data = b"READY";
        let mut reader = LineReader::new(Cursor::new(data));
        assert_eq!(reader.read_line().unwrap().as_deref(), Some("READY"));
        assert_eq!(reader.read_line().unwrap(), None);
    }

    #[test]
    fn write_line_appends_cr() {
        let mut buf: Vec<u8> = Vec::new();
        write_line(&mut buf, "MYCALL N0CALL").unwrap();
        assert_eq!(buf, b"MYCALL N0CALL\r");
    }
}
