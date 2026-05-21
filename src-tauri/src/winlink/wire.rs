//! Low-level line framing shared by the handshake and the message exchange.
//!
//! Winlink protocol lines end with a carriage return (`\r`). When the link uses
//! CRLF, reading up to the `\r` leaves the `\n` at the front of the next line,
//! which is trimmed away. Stray null bytes that some servers pad with are also
//! stripped. Mirrors `wl2k-go/fbb/helpers.go`.

use std::io::{self, BufRead};

/// Read one protocol line (up to and including the terminating `\r`), returning
/// it trimmed. Returns an error if the stream ends before any byte is read.
pub fn read_line<R: BufRead>(reader: &mut R) -> io::Result<String> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\r', &mut buf)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed while reading a line",
        ));
    }
    Ok(clean_line(&String::from_utf8_lossy(&buf)).to_string())
}

/// Trim surrounding whitespace and strip a stray leading or trailing null byte.
pub fn clean_line(line: &str) -> &str {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('\0').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('\0').unwrap_or(trimmed);
    trimmed.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_carriage_return_terminated_lines() {
        let data = b"[WL2K-5.0-B2FWIHJM$]\r;PQ: 12345678\rCMS>\r";
        let mut cursor = Cursor::new(&data[..]);
        assert_eq!(read_line(&mut cursor).unwrap(), "[WL2K-5.0-B2FWIHJM$]");
        assert_eq!(read_line(&mut cursor).unwrap(), ";PQ: 12345678");
        assert_eq!(read_line(&mut cursor).unwrap(), "CMS>");
    }

    #[test]
    fn trims_the_leading_newline_left_by_crlf_framing() {
        let data = b"line one\r\nline two\r\n";
        let mut cursor = Cursor::new(&data[..]);
        assert_eq!(read_line(&mut cursor).unwrap(), "line one");
        assert_eq!(read_line(&mut cursor).unwrap(), "line two");
    }

    #[test]
    fn reports_an_error_at_end_of_stream() {
        let mut cursor = Cursor::new(&b""[..]);
        assert!(read_line(&mut cursor).is_err());
    }

    #[test]
    fn clean_line_strips_whitespace_and_stray_nulls() {
        assert_eq!(clean_line("  hello \r\n"), "hello");
        assert_eq!(clean_line("\0;PQ: 99\0"), ";PQ: 99");
    }
}
