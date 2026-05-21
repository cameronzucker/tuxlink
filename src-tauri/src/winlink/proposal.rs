//! B2F proposals — the lines that offer messages to the other side of an
//! exchange.
//!
//! To send messages, one `F<code> ...` line is sent per message, then a
//! checksum line `F> <hex>` ends the batch. The other side replies with
//! `FS <one answer character per proposal>` (accept / reject / defer).

/// One proposal: an offer to send a single message.
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
}
