//! The framed block transfer that carries one message's compressed body.
//!
//! Once a proposal is accepted, the message body (already lzhuf-compressed)
//! moves across the link in a small frame:
//!
//! ```text
//! SOH <header-length> <title> NUL <offset> NUL   header
//! STX <chunk-length> <bytes>                      one or more data chunks
//! ...
//! EOT <checksum>                                  end + checksum of the data
//! ```
//!
//! The control bytes are the ASCII start-of-heading (SOH), start-of-text (STX),
//! end-of-transmission (EOT), and a null separator. Data is split into chunks of
//! at most 125 bytes so it rides comfortably even on packet links. The checksum
//! is the same negated-byte-sum used elsewhere: the data bytes plus the checksum
//! byte add up to zero (mod 256).
//!
//! Verified against `la5nta/wl2k-go`'s `fbb` block transfer; no Go ships.

use std::io::Read;

const SOH: u8 = 1;
const STX: u8 = 2;
const EOT: u8 = 4;
const NUL: u8 = 0;

/// The largest data chunk we put after a single STX. wl2k-go uses 125 (rather
/// than the protocol's 255) so a chunk fits within a 128-byte packet frame.
const MAX_CHUNK: usize = 125;

/// A message body received as a framed block: its title and the still-compressed
/// body bytes (the caller decompresses with [`super::lzhuf`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedBlock {
    /// The title field from the header (the message subject). Carried verbatim;
    /// non-ASCII titles arrive word-encoded and are decoded by a later step.
    pub title: String,
    /// The compressed body bytes gathered from the data chunks.
    pub data: Vec<u8>,
}

/// Wrap one message's compressed body in the framed, chunked form for sending.
///
/// `offset` is how many bytes the other side already has (normally 0). `title`
/// must already be ASCII; word-encoding a non-ASCII subject is a later step.
pub fn frame_block(title: &str, offset: usize, data: &[u8]) -> Vec<u8> {
    let offset_str = offset.to_string();
    let title_bytes = title.as_bytes();
    let header_len = title_bytes.len() + offset_str.len() + 2;

    let mut out = Vec::new();
    out.push(SOH);
    out.push(header_len as u8);
    out.extend_from_slice(title_bytes);
    out.push(NUL);
    out.extend_from_slice(offset_str.as_bytes());
    out.push(NUL);

    let mut sum: u32 = 0;
    for chunk in data.chunks(MAX_CHUNK) {
        out.push(STX);
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
        for &b in chunk {
            sum = sum.wrapping_add(u32::from(b));
        }
    }

    let checksum = (sum.wrapping_neg() & 0xff) as u8;
    out.push(EOT);
    out.push(checksum);
    out
}

/// Read one framed block from `reader`, returning the title and the compressed
/// body bytes, after confirming the trailing checksum.
pub fn read_block<R: Read>(reader: &mut R) -> Result<ReceivedBlock, TransferError> {
    let soh = read_u8(reader)?;
    if soh != SOH {
        return Err(TransferError::ExpectedHeader(soh));
    }
    let header_len = read_u8(reader)? as usize;
    let title = read_until_nul(reader)?;
    let offset = read_until_nul(reader)?;
    if title.len() + offset.len() + 2 != header_len {
        return Err(TransferError::HeaderLengthMismatch);
    }

    let mut data = Vec::new();
    let mut sum: u32 = 0;
    loop {
        match read_u8(reader)? {
            STX => {
                // A chunk length byte of 0 means a full 256-byte chunk.
                let mut len = read_u8(reader)? as usize;
                if len == 0 {
                    len = 256;
                }
                for _ in 0..len {
                    let b = read_u8(reader)?;
                    data.push(b);
                    sum = sum.wrapping_add(u32::from(b));
                }
            }
            EOT => {
                let checksum = read_u8(reader)?;
                sum = sum.wrapping_add(u32::from(checksum));
                if sum & 0xff != 0 {
                    return Err(TransferError::BadChecksum);
                }
                break;
            }
            other => return Err(TransferError::UnexpectedByte(other)),
        }
    }

    Ok(ReceivedBlock {
        title: String::from_utf8_lossy(&title).into_owned(),
        data,
    })
}

fn read_u8<R: Read>(reader: &mut R) -> Result<u8, TransferError> {
    let mut b = [0u8; 1];
    reader
        .read_exact(&mut b)
        .map_err(|_| TransferError::UnexpectedEnd)?;
    Ok(b[0])
}

fn read_until_nul<R: Read>(reader: &mut R) -> Result<Vec<u8>, TransferError> {
    let mut bytes = Vec::new();
    loop {
        let b = read_u8(reader)?;
        if b == NUL {
            return Ok(bytes);
        }
        bytes.push(b);
    }
}

/// Why a framed block could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    /// The block did not begin with the start-of-heading byte.
    ExpectedHeader(u8),
    /// The header's stated length did not match the title and offset read.
    HeaderLengthMismatch,
    /// A byte appeared where a chunk marker (STX) or end marker (EOT) was due.
    UnexpectedByte(u8),
    /// The data bytes and the trailing checksum did not add up to zero.
    BadChecksum,
    /// The stream ended in the middle of a block.
    UnexpectedEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_a_block_with_header_data_and_checksum() {
        let bytes = frame_block("Hi", 0, &[0x01, 0x02, 0x03]);
        // SOH, header-len (2 title + 1 offset + 2 nulls = 5), "Hi", NUL, "0",
        // NUL, STX, chunk-len 3, the data, EOT, checksum = -(1+2+3) & 0xff = 250.
        assert_eq!(
            bytes,
            vec![1, 5, b'H', b'i', 0, b'0', 0, 2, 3, 0x01, 0x02, 0x03, 4, 250]
        );
    }

    #[test]
    fn reads_back_a_framed_block() {
        let data: Vec<u8> = (0..300u32).map(|i| i as u8).collect();
        let bytes = frame_block("Subject line", 0, &data);
        let mut cursor = std::io::Cursor::new(bytes);
        let block = read_block(&mut cursor).unwrap();
        assert_eq!(block.title, "Subject line");
        assert_eq!(block.data, data);
    }

    #[test]
    fn splits_data_into_chunks_of_at_most_125_bytes() {
        let data = vec![0xAB; 250];
        let bytes = frame_block("x", 0, &data);
        // Header: SOH, len (1+1+2=4), "x", NUL, "0", NUL — six bytes.
        assert_eq!(&bytes[0..6], &[1, 4, b'x', 0, b'0', 0]);
        // First chunk: STX then a length byte of 125.
        assert_eq!((bytes[6], bytes[7]), (2, 125));
        // Second chunk begins after STX + length + 125 data bytes = offset 133.
        assert_eq!((bytes[133], bytes[134]), (2, 125));
        // After the two 125-byte chunks (offset 260) comes EOT.
        assert_eq!(bytes[260], 4);
    }

    #[test]
    fn rejects_a_block_with_a_bad_checksum() {
        let mut bytes = frame_block("Hi", 0, &[1, 2, 3]);
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        let mut cursor = std::io::Cursor::new(bytes);
        assert_eq!(read_block(&mut cursor), Err(TransferError::BadChecksum));
    }

    #[test]
    fn rejects_a_block_that_does_not_start_with_the_header_byte() {
        let mut cursor = std::io::Cursor::new(vec![0x09, 0x00]);
        assert_eq!(read_block(&mut cursor), Err(TransferError::ExpectedHeader(9)));
    }
}
