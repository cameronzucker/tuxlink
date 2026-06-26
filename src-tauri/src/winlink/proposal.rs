//! B2F proposals — the lines that offer messages to the other side of an
//! exchange.
//!
//! To send messages, one `F<code> ...` line is sent per message, then a
//! checksum line `F> <hex>` ends the batch. The other side replies with
//! `FS <one answer character per proposal>` (accept / reject / defer).

/// One proposal: an offer to send a single message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Format code: `'C'` = standard Winlink-v2 compressed, `'D'` = gzip.
    pub code: char,
    /// Message type: `"EM"` (encapsulated message) or `"CM"`.
    pub msg_type: String,
    /// The message's unique id.
    pub mid: String,
    /// Uncompressed size in bytes.
    pub size: usize,
    /// Compressed size in bytes (what actually transfers over the link).
    pub compressed_size: usize,
}

impl Proposal {
    /// The line that offers this message:
    /// `F<code> <type> <mid> <size> <compressed_size> 0`.
    ///
    /// The trailing `0` is an unused offset field. The carriage return that
    /// terminates the line on the wire is added by the caller.
    pub fn line(&self) -> String {
        format!(
            "F{} {} {} {} {} 0",
            self.code, self.msg_type, self.mid, self.size, self.compressed_size
        )
    }

    /// Parse a proposal the other side offered us:
    /// `F<code> <type> <mid> <size> <compressed-size> <offset>`.
    ///
    /// The trailing offset field is read but not kept — resuming a partial
    /// transfer at an offset is a later step. The caller is responsible for
    /// having already routed control lines (`FF`, `FQ`, `F>`) elsewhere; this
    /// only handles the `FA`/`FB`/`FC`/`FD` proposal lines.
    pub fn parse(line: &str) -> Result<Proposal, ProposalParseError> {
        let bytes = line.as_bytes();
        if bytes.first() != Some(&b'F') {
            return Err(ProposalParseError::NotAProposalLine);
        }
        let code = bytes.get(1).copied().ok_or(ProposalParseError::Malformed)? as char;
        if code != 'C' && code != 'D' {
            return Err(ProposalParseError::UnsupportedFormat(code));
        }

        // After "F<code> " come the space-separated fields.
        let rest = line.get(3..).ok_or(ProposalParseError::Malformed)?;
        let parts: Vec<&str> = rest.split(' ').collect();
        if parts.len() != 5 {
            return Err(ProposalParseError::Malformed);
        }

        let msg_type = parts[0];
        if msg_type != "EM" && msg_type != "CM" {
            return Err(ProposalParseError::UnexpectedMessageType(msg_type.to_string()));
        }
        let size = parts[2].parse::<usize>().map_err(|_| ProposalParseError::Malformed)?;
        let compressed_size = parts[3]
            .parse::<usize>()
            .map_err(|_| ProposalParseError::Malformed)?;

        Ok(Proposal {
            code,
            msg_type: msg_type.to_string(),
            mid: parts[1].to_string(),
            size,
            compressed_size,
        })
    }
}

/// One entry from the CMS's pending-message manifest — a `;PM:` line the CMS
/// sends up front listing every message awaiting download, BEFORE it negotiates
/// the actual download in small `FC` blocks (tuxlink-9u07u).
///
/// The manifest is the parity lever: WLE renders its "Review Pending Messages"
/// pane from these lines (all messages at once, with sender + subject), then
/// answers the `FC` blocks underneath. tuxlink historically skipped every
/// `;`-prefixed line, so it only ever saw the `FC` blocks (≈3 at a time) and
/// prompted once per block. A [`PendingMessage`] carries the sender and subject
/// that the `FC` [`Proposal`] line lacks — that richer data lives ONLY here.
///
/// Wire format: `;PM: <recipient> <mid> <size> <sender> <subject…>`
/// e.g. `;PM: N7CPZ WCCJR0N74QU3 764 SERVICE@winlink.org INQUIRY - https://…`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMessage {
    /// The recipient callsign the CMS holds this message for.
    pub recipient: String,
    /// The message's unique id (matches the `FC` proposal's MID).
    pub mid: String,
    /// Uncompressed size in bytes.
    pub size: usize,
    /// The originating address (e.g. `SERVICE@winlink.org`).
    pub sender: String,
    /// The subject line. May be empty and may contain spaces.
    pub subject: String,
}

impl PendingMessage {
    /// Parse a `;PM:` manifest line. Returns `None` for any line that is not a
    /// well-formed `;PM:` entry (a different `;` control line, a malformed size,
    /// or a missing required field) so callers can try this on every received
    /// line and ignore non-matches.
    ///
    /// The first four whitespace-delimited fields are `recipient mid size
    /// sender`; everything after the fourth space is the subject verbatim
    /// (so subjects containing spaces survive intact). The subject may be empty.
    pub fn parse(line: &str) -> Option<PendingMessage> {
        let rest = line.strip_prefix(";PM:")?.trim_start();
        let mut fields = rest.splitn(5, ' ');
        let recipient = fields.next()?.to_string();
        let mid = fields.next()?.to_string();
        let size = fields.next()?.parse::<usize>().ok()?;
        let sender = fields.next()?.to_string();
        // Subject is optional and keeps its internal spaces; trailing CR/space
        // is already stripped by the line reader, so only trim the right edge
        // defensively.
        let subject = fields.next().unwrap_or("").trim_end().to_string();
        if recipient.is_empty() || mid.is_empty() || sender.is_empty() {
            return None;
        }
        Some(PendingMessage {
            recipient,
            mid,
            size,
            sender,
            subject,
        })
    }
}

/// One side's answer to a single proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The other side will receive this message. `resume_offset` is how many
    /// bytes of the compressed data it already has — usually 0, meaning send the
    /// whole message. The `A`/`!` answer forms carry a non-zero value to resume
    /// an interrupted transfer partway through.
    Accept { resume_offset: usize },
    /// The other side already has this message; don't send it.
    Reject,
    /// The other side is putting this message off to a later turn.
    Defer,
}

/// Parse the `FS <answers>` line sent in reply to a batch of proposals: one
/// answer per proposal, in the same order the proposals were offered.
///
/// Each answer is a single character, except an accept-at-offset, which is an
/// `A`/`a`/`!` followed by the byte offset to resume from. A letter form
/// (`Y` accept, `N`/`R` reject, `L`/`H` defer) and a symbol form (`+`/`-`/`=`)
/// both appear in the wild and mean the same things.
///
/// For an offset accept we read the leading run of digits as the offset, then
/// continue with the next answer character.
pub fn parse_answers(line: &str) -> Result<Vec<Answer>, AnswerParseError> {
    let body = line
        .strip_prefix("FS ")
        .ok_or(AnswerParseError::NotAnAnswerLine)?;

    let mut answers = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        let answer = match c {
            'Y' | 'y' | '+' => Answer::Accept { resume_offset: 0 },
            'N' | 'n' | 'R' | 'r' | '-' => Answer::Reject,
            'L' | 'l' | '=' | 'H' | 'h' => Answer::Defer,
            'A' | 'a' | '!' => {
                let mut digits = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        digits.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let resume_offset = digits
                    .parse::<usize>()
                    .map_err(|_| AnswerParseError::MissingOffset)?;
                Answer::Accept { resume_offset }
            }
            other => return Err(AnswerParseError::UnexpectedCharacter(other)),
        };
        answers.push(answer);
    }
    Ok(answers)
}

/// Why an `FS` answer line could not be parsed.
#[derive(Debug, PartialEq, Eq)]
pub enum AnswerParseError {
    /// The line did not start with the `FS ` that an answer line begins with.
    NotAnAnswerLine,
    /// An accept-at-offset (`A`/`a`/`!`) was not followed by the byte offset.
    MissingOffset,
    /// A character that is not a valid answer appeared in the line.
    UnexpectedCharacter(char),
}

/// Why a proposal line could not be parsed.
#[derive(Debug, PartialEq, Eq)]
pub enum ProposalParseError {
    /// The line did not begin with the `F` that every protocol line starts with.
    NotAProposalLine,
    /// The format code (the character after `F`) is one we don't handle. We send
    /// and receive the compressed `C` and `D` formats; the old basic `A`/`B`
    /// formats are not supported.
    UnsupportedFormat(char),
    /// The line did not have the five expected fields:
    /// `<type> <mid> <size> <compressed-size> <offset>`.
    Malformed,
    /// The message type was not `EM` or `CM`.
    UnexpectedMessageType(String),
}

/// The checksum line that ends a batch of proposals: `F> <hex>`.
///
/// The checksum covers the exact bytes that go on the wire: every byte of every
/// proposal line, plus the carriage return that terminates each line. Sum those
/// bytes, negate, keep the low 8 bits, and print as two uppercase hex digits.
/// The receiver re-sums the same bytes and adds this value back; a correct batch
/// sums to zero (mod 256), which is how the other side detects a garbled batch.
///
/// Proposal lines are ASCII, so summing bytes and summing characters give the
/// same result here.
pub fn batch_checksum_line(proposals: &[Proposal]) -> String {
    let mut sum: u32 = 0;
    for p in proposals {
        for b in p.line().bytes() {
            sum = sum.wrapping_add(u32::from(b));
        }
        sum = sum.wrapping_add(u32::from(b'\r'));
    }
    let checksum = sum.wrapping_neg() & 0xff;
    format!("F> {:02X}", checksum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_proposal_offer_line() {
        let p = Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid: "TJKYEIMMHSRB".to_string(),
            size: 527,
            compressed_size: 123,
        };
        assert_eq!(p.line(), "FC EM TJKYEIMMHSRB 527 123 0");
    }

    // --- tuxlink-9u07u: ;PM: pending-message manifest parsing ---

    #[test]
    fn parses_a_pm_manifest_line_with_sender_and_subject() {
        // Verbatim from an operator-captured CMS session (2026-06-26).
        let line = ";PM: N7CPZ WCCJR0N74QU3 764 SERVICE@winlink.org INQUIRY - https://services.swpc.noaa.gov/text/3-day-geomag-forecast.txt";
        let pm = PendingMessage::parse(line).expect("a well-formed ;PM: line parses");
        assert_eq!(pm.recipient, "N7CPZ");
        assert_eq!(pm.mid, "WCCJR0N74QU3");
        assert_eq!(pm.size, 764);
        assert_eq!(pm.sender, "SERVICE@winlink.org");
        assert_eq!(
            pm.subject,
            "INQUIRY - https://services.swpc.noaa.gov/text/3-day-geomag-forecast.txt",
            "subject keeps its internal spaces"
        );
    }

    #[test]
    fn pm_subject_may_be_empty() {
        let pm = PendingMessage::parse(";PM: N7CPZ ABC123XYZ456 100 W7AAA@winlink.org")
            .expect("a ;PM: line with no subject still parses");
        assert_eq!(pm.mid, "ABC123XYZ456");
        assert_eq!(pm.sender, "W7AAA@winlink.org");
        assert_eq!(pm.subject, "");
    }

    #[test]
    fn non_pm_lines_do_not_parse_as_manifest() {
        // FC proposal lines, other ; control lines, and a bare/empty ;PM: must
        // all return None so the receive loop can try parse() on every line.
        assert!(PendingMessage::parse("FC EM ABC123 1 2 0").is_none());
        assert!(PendingMessage::parse(";PR: 48796332").is_none());
        assert!(PendingMessage::parse(";PQ: 99864849").is_none());
        assert!(PendingMessage::parse(";PM:").is_none());
        assert!(PendingMessage::parse(";PM: N7CPZ").is_none(), "missing fields");
    }

    #[test]
    fn pm_with_non_numeric_size_is_rejected() {
        assert!(PendingMessage::parse(";PM: N7CPZ ABC123 notanumber W7AAA@winlink.org Subj").is_none());
    }

    #[test]
    fn parses_the_letter_form_of_an_answer_string() {
        let answers = parse_answers("FS YLA3350RH").unwrap();
        assert_eq!(
            answers,
            vec![
                Answer::Accept { resume_offset: 0 },
                Answer::Defer,
                Answer::Accept { resume_offset: 3350 },
                Answer::Reject,
                Answer::Defer,
            ]
        );
    }

    #[test]
    fn parses_the_symbol_form_of_an_answer_string() {
        let answers = parse_answers("FS +=!3350-+").unwrap();
        assert_eq!(
            answers,
            vec![
                Answer::Accept { resume_offset: 0 },
                Answer::Defer,
                Answer::Accept { resume_offset: 3350 },
                Answer::Reject,
                Answer::Accept { resume_offset: 0 },
            ]
        );
    }

    #[test]
    fn rejects_an_answer_line_without_the_fs_prefix() {
        assert_eq!(parse_answers("YLR"), Err(AnswerParseError::NotAnAnswerLine));
    }

    #[test]
    fn rejects_an_offset_accept_with_no_digits() {
        assert_eq!(parse_answers("FS A"), Err(AnswerParseError::MissingOffset));
    }

    #[test]
    fn rejects_an_unknown_answer_character() {
        assert_eq!(
            parse_answers("FS Y?"),
            Err(AnswerParseError::UnexpectedCharacter('?'))
        );
    }

    #[test]
    fn checksum_line_for_a_single_proposal_batch() {
        let p = Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid: "TJKYEIMMHSRB".to_string(),
            size: 527,
            compressed_size: 123,
        };
        assert_eq!(batch_checksum_line(&[p]), "F> 3B");
    }

    #[test]
    fn parses_an_inbound_proposal_line() {
        let p = Proposal::parse("FC EM TJKYEIMMHSRB 527 123 0").unwrap();
        assert_eq!(p.code, 'C');
        assert_eq!(p.msg_type, "EM");
        assert_eq!(p.mid, "TJKYEIMMHSRB");
        assert_eq!(p.size, 527);
        assert_eq!(p.compressed_size, 123);
    }

    #[test]
    fn rejects_a_line_that_does_not_start_with_f() {
        assert_eq!(
            Proposal::parse("XC EM ABC 1 1 0"),
            Err(ProposalParseError::NotAProposalLine)
        );
    }

    #[test]
    fn rejects_an_unsupported_proposal_format() {
        // 'B' is the old basic ASCII proposal; we only handle the compressed
        // 'C' and 'D' formats.
        assert_eq!(
            Proposal::parse("FB EM ABC 1 1 0"),
            Err(ProposalParseError::UnsupportedFormat('B'))
        );
    }

    #[test]
    fn rejects_a_proposal_with_too_few_fields() {
        assert_eq!(
            Proposal::parse("FC EM ABC 1"),
            Err(ProposalParseError::Malformed)
        );
    }

    #[test]
    fn rejects_an_unexpected_message_type() {
        assert_eq!(
            Proposal::parse("FC ZZ ABC 1 1 0"),
            Err(ProposalParseError::UnexpectedMessageType("ZZ".to_string()))
        );
    }

    #[test]
    fn checksum_line_covers_every_proposal_in_the_batch() {
        let first = Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid: "TJKYEIMMHSRB".to_string(),
            size: 527,
            compressed_size: 123,
        };
        let second = Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid: "ABCDEFGHIJKL".to_string(),
            size: 100,
            compressed_size: 50,
        };
        assert_eq!(batch_checksum_line(&[first, second]), "F> FF");
    }
}
