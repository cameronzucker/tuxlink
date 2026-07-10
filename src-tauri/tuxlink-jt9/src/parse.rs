//! Line-level parser for jt9 FT8 file-mode stdout.
//!
//! Line grammar (verbatim capture, wsjtx 2.7.0, `jt9 -8 -d 3 -p 15 -w 1`):
//! ```text
//! 000000 -17 -0.9 2391 ~  CQ W5C/H
//! 000000 -14 -0.6 2093 ~  YB3BBF K5OJT -19
//! <DecodeFinished>   0   6        0
//! ```
//! Columns before the `~` (the FT8 sync marker, which cannot occur in an FT8
//! message charset): HHMMSS time (always `000000` for non-WSJT-X-named input
//! files — ignored; slot UTC comes from the host scheduler), SNR dB, DT s,
//! audio freq Hz. Everything after `~` is the message, trimmed.
//! Grammar lifted from tuxlink-ft8/src/oracle.rs (which discards the
//! metadata; this parser keeps it — that is why it exists).

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLine {
    Decode { snr_db: i32, dt_s: f64, freq_hz: u32, message: String },
    DecodeFinished,
    Other,
}

pub fn parse_stdout_line(line: &str) -> ParsedLine {
    let trimmed = line.trim_end();
    if trimmed.trim_start().starts_with("<DecodeFinished>") {
        return ParsedLine::DecodeFinished;
    }
    let Some((meta, msg)) = trimmed.split_once('~') else {
        return ParsedLine::Other;
    };
    let message = msg.trim().to_string();
    if message.is_empty() {
        return ParsedLine::Other;
    }
    // meta: "HHMMSS SNR DT FREQ" — whitespace-separated, HHMMSS ignored.
    let mut cols = meta.split_whitespace();
    let _time = cols.next();
    let (Some(snr), Some(dt), Some(freq)) = (cols.next(), cols.next(), cols.next()) else {
        return ParsedLine::Other;
    };
    let (Ok(snr_db), Ok(dt_s), Ok(freq_hz)) =
        (snr.parse::<i32>(), dt.parse::<f64>(), freq.parse::<u32>()) else {
        return ParsedLine::Other;
    };
    ParsedLine::Decode { snr_db, dt_s, freq_hz, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verbatim_decode_lines() {
        // Real capture, 2026-07-10, wsjtx 2.7.0+repack-1, ordinary fixture.
        let l = "000000 -17 -0.9 2391 ~  CQ W5C/H                                ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -17, dt_s: -0.9, freq_hz: 2391, message: "CQ W5C/H".into() }
        );
        let l = "000000 -14 -0.6 2093 ~  YB3BBF K5OJT -19                        ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -14, dt_s: -0.6, freq_hz: 2093, message: "YB3BBF K5OJT -19".into() }
        );
        let l = "000000 -16 -1.0  502 ~  K0BQB WD8ASA +09                        ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -16, dt_s: -1.0, freq_hz: 502, message: "K0BQB WD8ASA +09".into() }
        );
    }

    #[test]
    fn parses_decode_finished_sentinel() {
        assert_eq!(parse_stdout_line("<DecodeFinished>   0   6        0"), ParsedLine::DecodeFinished);
    }

    #[test]
    fn hashed_callsign_message_survives_verbatim() {
        let l = "000000 -12  0.3 1802 ~  <...> N4AHI EM73                        ";
        assert_eq!(
            parse_stdout_line(l),
            ParsedLine::Decode { snr_db: -12, dt_s: 0.3, freq_hz: 1802, message: "<...> N4AHI EM73".into() }
        );
    }

    #[test]
    fn malformed_lines_are_other_never_panic() {
        for l in ["", "garbage", "000000 -14", "000000 xx yy zz ~ MSG",
                  "Fortran runtime error: End of file", "~", "000000 -14 -0.6 2093 ~  "] {
            assert_eq!(parse_stdout_line(l), ParsedLine::Other, "line: {l:?}");
        }
    }
}
