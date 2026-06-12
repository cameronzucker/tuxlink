//! GAIA framing for the Benshi protocol over Bluetooth Classic / RFCOMM
//! (tuxlink-nx95). On RFCOMM the radio wraps every command `Message` in a GAIA
//! frame; on BLE the message is sent raw. tuxlink uses the RFCOMM path (the
//! on-air-proven UV-Pro transport), so framing is mandatory.
//!
//! Frame: `ff 01 <flags:u8> <n:u8> <data[n+4]> [csum:u8 if flags&CHECKSUM]`
//! where `data` is the full `Message` bytes (4-byte command header + payload) and
//! `n` is the payload length EXCLUDING those 4 command bytes. We transmit with
//! `flags = 0` (no checksum); the deframer still tolerates RX frames that set the
//! checksum flag so it cannot desync against a chattier firmware.

const START: u8 = 0xff;
const VERSION: u8 = 0x01;
const FLAG_CHECKSUM: u8 = 0x01;
/// Upper bound on buffered, un-deframed bytes. Bounds a desynced / garbage /
/// never-completing stream so a hostile or wedged peer can't grow the buffer
/// without limit. Far larger than any real frame (a `Message` is ≤ ~30 bytes).
const MAX_BUFFER: usize = 4096;

/// Wrap a complete `Message` (≥ 4 bytes: the command header) in a no-checksum
/// GAIA frame for transmission.
pub fn gaia_wrap(msg: &[u8]) -> Vec<u8> {
    debug_assert!(msg.len() >= 4, "Message must include its 4-byte command header");
    let n = (msg.len().saturating_sub(4)) as u8;
    let mut out = vec![START, VERSION, 0x00, n];
    out.extend_from_slice(msg);
    out
}

/// Streaming GAIA deframer: feed it arbitrary RFCOMM read chunks; it yields the
/// `data` (full `Message`) of each complete frame, retaining any partial tail.
#[derive(Default)]
pub struct GaiaDeframer {
    buf: Vec<u8>,
}

impl GaiaDeframer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn buffered_len(&self) -> usize {
        self.buf.len()
    }

    /// Append `data` and return every complete frame's payload now available.
    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);

        // Desync guard: if the buffer has grown past the cap without yielding a
        // frame, drop everything before the next plausible start sentinel (or all
        // of it if there is none).
        if self.buf.len() > MAX_BUFFER {
            match find_start(&self.buf) {
                Some(i) if i > 0 => {
                    self.buf.drain(..i);
                }
                Some(_) => {
                    // Starts at 0 but still over cap with no complete frame: an
                    // absurd `n` can't exceed 255+4+1, so this means junk after a
                    // false start — skip one byte to make progress on next resync.
                    self.buf.drain(..1);
                }
                None => self.buf.clear(),
            }
        }

        let mut out = Vec::new();
        loop {
            // Resync: make sure the buffer head is a frame start.
            match find_start(&self.buf) {
                Some(0) => {}
                Some(i) => {
                    self.buf.drain(..i);
                }
                None => {
                    // No start sentinel anywhere; keep at most the last byte in
                    // case it is the first half of a future `ff 01`.
                    if self.buf.len() > 1 {
                        let keep = self.buf[self.buf.len() - 1];
                        self.buf.clear();
                        if keep == START {
                            self.buf.push(keep);
                        }
                    }
                    break;
                }
            }

            if self.buf.len() < 4 {
                break; // need start, version, flags, n
            }
            let flags = self.buf[2];
            let n = self.buf[3] as usize;
            let csum = usize::from(flags & FLAG_CHECKSUM != 0);
            let total = 4 + (n + 4) + csum; // header + data(n+4) + optional checksum
            if self.buf.len() < total {
                break; // wait for the rest of this frame
            }
            let payload = self.buf[4..4 + n + 4].to_vec();
            self.buf.drain(..total);
            out.push(payload);
        }
        out
    }
}

/// Locate the next `ff 01` start sentinel.
fn find_start(b: &[u8]) -> Option<usize> {
    b.windows(2).position(|w| w[0] == START && w[1] == VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse space-separated hex into bytes (test helper).
    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    #[test]
    fn wraps_a_message_with_correct_n() {
        // 4-byte GET_HT_STATUS header → n = 0
        assert_eq!(
            gaia_wrap(&hex("00 02 00 14")),
            hex("ff 01 00 00 00 02 00 14")
        );
    }

    #[test]
    fn wraps_write_rf_ch_with_n_25() {
        // golden GAIA(WRITE_RF_CH): n = 0x19 = 25
        let msg = hex("00 02 00 0e 00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00");
        let wrapped = gaia_wrap(&msg);
        assert_eq!(wrapped[0..4], hex("ff 01 00 19")[..]);
        assert_eq!(&wrapped[4..], &msg[..]);
    }

    #[test]
    fn deframes_two_frames_from_one_buffer() {
        let buf = hex("ff 01 00 05 00 02 80 14 00 b4 3c c0 00 ff 01 00 00 00 02 00 14");
        let mut d = GaiaDeframer::new();
        let frames = d.push(&buf);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], hex("00 02 80 14 00 b4 3c c0 00")); // n=5 → 9 bytes
        assert_eq!(frames[1], hex("00 02 00 14")); // n=0 → 4 bytes
        assert_eq!(d.buffered_len(), 0);
    }

    #[test]
    fn reassembles_a_frame_split_across_two_pushes() {
        let mut d = GaiaDeframer::new();
        assert!(d.push(&hex("ff 01 00 00 00 02")).is_empty()); // partial
        let frames = d.push(&hex("00 14"));
        assert_eq!(frames, vec![hex("00 02 00 14")]);
    }

    #[test]
    fn resyncs_past_leading_garbage() {
        let mut d = GaiaDeframer::new();
        let frames = d.push(&hex("de ad ff 01 00 00 00 02 00 14"));
        assert_eq!(frames, vec![hex("00 02 00 14")]);
    }

    #[test]
    fn consumes_trailing_checksum_when_flagged() {
        let mut d = GaiaDeframer::new();
        // flags=1 → one trailing csum byte (0x99) after the 4-byte data
        let frames = d.push(&hex("ff 01 01 00 00 02 00 14 99"));
        assert_eq!(frames, vec![hex("00 02 00 14")]);
    }

    #[test]
    fn holds_partial_start_byte_for_next_push() {
        let mut d = GaiaDeframer::new();
        assert!(d.push(&hex("ff")).is_empty());
        let frames = d.push(&hex("01 00 00 00 02 00 14"));
        assert_eq!(frames, vec![hex("00 02 00 14")]);
    }
}
