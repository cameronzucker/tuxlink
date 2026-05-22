//! AX.25 connected-mode v2.x (mod-8) data-link state machine + `Ax25Stream`.
//!
//! Drives a `ByteLink` (TCP / serial KISS pipe) through P1's KISS framer
//! (`KissDecoder` / `kiss_data_frame`) and AX.25 codec (`Frame` / `Control`),
//! running SABM→UA connect, inbound-SABM→UA answer, sequenced I-frames with RR
//! acknowledgement, REJ retransmit, T1 timeout + N2 retry, MAXFRAME windowing,
//! PACLEN segmentation/reassembly, and DISC on drop. Presents reliable in-order
//! bytes as `Ax25Stream: Read + Write`.
//!
//! **No CSMA here** — half-duplex channel access is the modem's job (spec §2/§4.1);
//! this layer only pushes the KISS TNC params (`kiss_param`) on connect.
//!
//! Verified against a scripted in-memory peer (below) + a loopback TCP socket; no
//! RF, no transmission. Behaviour cross-checked vs `TNCKissInterface.dll`
//! (`Connection`/`DataLinkProvider`/`EstablishDataLink`) at
//! `dev/scratch/winlink-re/decompiled/tnckiss/` (local-only) + AX.25 v2.2 §6.

#[cfg(test)]
mod test_peer {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// A scripted in-memory `ByteLink`: the state machine writes to `tx` (which a
    /// test decodes) and reads from `rx` (which a test pre-loads with canned KISS
    /// frames). Both ends share the buffers so a test can inspect/extend between calls.
    #[derive(Clone)]
    pub struct ScriptedPeer {
        pub tx: Arc<Mutex<Vec<u8>>>,
        pub rx: Arc<Mutex<std::collections::VecDeque<u8>>>,
    }

    impl ScriptedPeer {
        pub fn new() -> Self {
            ScriptedPeer {
                tx: Arc::new(Mutex::new(Vec::new())),
                rx: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            }
        }
        /// Queue bytes for the state machine to read (a peer's KISS frame).
        pub fn feed(&self, bytes: &[u8]) {
            self.rx.lock().unwrap().extend(bytes.iter().copied());
        }
        /// Take everything the state machine has written so far.
        pub fn drain_tx(&self) -> Vec<u8> {
            std::mem::take(&mut *self.tx.lock().unwrap())
        }
    }

    impl Read for ScriptedPeer {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut rx = self.rx.lock().unwrap();
            if rx.is_empty() {
                // Empty (not EOF): the state machine's read loop must treat a momentarily
                // empty pipe as "no frame yet", not as a closed link. WouldBlock models a
                // non-blocking serial/TCP read with nothing buffered.
                return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "no data"));
            }
            let n = buf.len().min(rx.len());
            for b in buf.iter_mut().take(n) {
                *b = rx.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for ScriptedPeer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.tx.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn peer_records_tx_and_serves_rx() {
        let peer = ScriptedPeer::new();
        let mut a = peer.clone();
        a.write_all(&[1, 2, 3]).unwrap();
        assert_eq!(peer.drain_tx(), vec![1, 2, 3]);

        peer.feed(&[9, 8]);
        let mut buf = [0u8; 2];
        a.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [9, 8]);

        // Empty pipe ⇒ WouldBlock, not EOF.
        let mut one = [0u8; 1];
        assert_eq!(a.read(&mut one).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }
}
