//! `AudioTransport` — TX/RX over the second RFCOMM audio channel (tuxlink-bcsy).
//!
//! Owns the audio `ByteLink` (a 2nd `RfcommSocket` in production, an in-memory fake
//! in tests), an `SbcCodec`, and an `AudioDeframer`. TX: PCM chunk → `codec.encode`
//! → `AudioMessage::Data` → `to_bytes` → `link.write`. Stop: send `AudioMessage::End`
//! and drop the link. RX: `link.read` → `deframer.push` → decode each `Data` to PCM.
//!
//! RADIO-1 / ADR 0018: [`AudioTransport::abort`] is the working abort — it sends a
//! best-effort `AudioEnd` (de-key) and then DROPS the link, so no further `AudioData`
//! can be written. Dropping the transport (vs a check-then-write flag gate) is the
//! "disarm the transport on abort" approach the KISS path's `link.rs` tuxlink-0ja
//! note calls the complete fix — there is no check-then-write window here. No
//! tuxlink-added airtime cap / TOT (no-added-safeguards rule).

use std::sync::Arc;

use super::codec::SbcCodec;
use super::framing::{AudioDeframer, AudioMessage};
use crate::winlink::ax25::link::ByteLink;

/// How the radio keys for transmit on the audio path.
///
/// `Implicit` (default): opening the audio channel and streaming `AudioData` keys
/// TX; `AudioEnd` de-keys. This is what benlink's working send POC does — it sends
/// NO `c1.TX_AUDIO` command. `Explicit`: send `c1.TX_AUDIO` / `TX_AUDIO_STOP` over
/// the GAIA control channel (via the injected [`KeyFn`]) around the audio stream.
/// `Explicit` is gated on the operator HCI-snoop confirming the vendor app keys via
/// GAIA; until then `Implicit` is the proven path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyingMode {
    Implicit,
    Explicit,
}

/// Session-provided closure that keys (`true`) / de-keys (`false`) the radio over
/// the GAIA control channel — it encodes a `c1` TX_AUDIO / TX_AUDIO_STOP command and
/// sends it on the control link the session owns. Only invoked in `Explicit` mode.
/// The transport deliberately knows nothing about GAIA command encoding; that is the
/// session's concern (the keying opcodes live in `audio::keying`).
pub type KeyFn = Box<dyn Fn(bool) -> Result<(), String> + Send>;

/// The audio transport. One instance per send-or-receive session; not reused after
/// `finish`/`abort` (the link is dropped).
pub struct AudioTransport {
    link: Option<Box<dyn ByteLink>>,
    codec: Arc<dyn SbcCodec>,
    deframer: AudioDeframer,
    mode: KeyingMode,
    key: Option<KeyFn>,
    keyed: bool,
}

impl AudioTransport {
    pub fn new(link: Box<dyn ByteLink>, codec: Arc<dyn SbcCodec>, mode: KeyingMode) -> Self {
        Self {
            link: Some(link),
            codec,
            deframer: AudioDeframer::new(),
            mode,
            key: None,
            keyed: false,
        }
    }

    /// Inject the GAIA keying callback (session-wired). Only used in `Explicit` mode.
    pub fn with_keying(mut self, key: KeyFn) -> Self {
        self.key = Some(key);
        self
    }

    /// True until `finish`/`abort` drops the link.
    pub fn is_connected(&self) -> bool {
        self.link.is_some()
    }

    /// Encode + frame + transmit one PCM chunk. In `Explicit` mode this keys TX
    /// before the first frame. `Err` if the link has been dropped (post finish/abort)
    /// — the transport cannot transmit after it is disarmed.
    pub fn send_pcm(&mut self, pcm: &[u8]) -> Result<(), String> {
        self.ensure_keyed()?;
        let sbc = self.codec.encode(pcm);
        let wire = AudioMessage::Data(sbc).to_bytes();
        self.write_all(&wire)
    }

    /// Clean end of transmission: send `AudioEnd`, de-key (Explicit), drop the link.
    pub fn finish(&mut self) -> Result<(), String> {
        let r = self.write_all(&AudioMessage::End.to_bytes());
        self.dekey();
        self.link = None;
        r
    }

    /// RADIO-1 working abort: best-effort `AudioEnd` (de-key the radio), then drop
    /// the link so no further `AudioData` can be transmitted. Infallible; never
    /// panics. This is the load-bearing stop.
    pub fn abort(&mut self) {
        if let Some(link) = self.link.as_mut() {
            let _ = link.write_all(&AudioMessage::End.to_bytes());
            let _ = link.flush();
        }
        self.dekey();
        self.link = None;
    }

    /// One bounded RX read → deframe → decode each `AudioData` to PCM via `on_pcm`.
    /// Returns `true` once an `AudioEnd` is observed (end of inbound image). The
    /// session calls this repeatedly in a poll loop, keeping it abort-observable
    /// (mirrors `aprs/native_driver.rs`'s 50 ms loop). A read error or timeout
    /// (`WouldBlock`) is "no data this tick" → returns `false`, so a quiet line does
    /// not end the loop; a clean EOF (`Ok(0)`) also returns `false` (the session
    /// detects a dropped link via `is_connected` / the read returning 0 repeatedly).
    pub fn pump_rx(&mut self, on_pcm: &mut dyn FnMut(&[u8])) -> bool {
        let mut buf = [0u8; 1024];
        let n = match self.link.as_mut() {
            Some(link) => match link.read(&mut buf) {
                Ok(0) => return false,
                Ok(n) => n,
                Err(_) => return false,
            },
            None => return false,
        };
        let mut ended = false;
        for msg in self.deframer.push(&buf[..n]) {
            match msg {
                AudioMessage::Data(sbc) => {
                    let pcm = self.codec.decode(&sbc);
                    if !pcm.is_empty() {
                        on_pcm(&pcm);
                    }
                }
                AudioMessage::End => ended = true,
                AudioMessage::Ack | AudioMessage::Unknown(..) => {}
            }
        }
        ended
    }

    fn ensure_keyed(&mut self) -> Result<(), String> {
        if self.mode == KeyingMode::Explicit && !self.keyed {
            if let Some(k) = &self.key {
                k(true)?;
            }
            self.keyed = true;
        }
        Ok(())
    }

    fn dekey(&mut self) {
        if self.keyed {
            if let Some(k) = &self.key {
                let _ = k(false); // best-effort de-key
            }
            self.keyed = false;
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), String> {
        let link = self
            .link
            .as_mut()
            .ok_or_else(|| "audio link closed".to_string())?;
        link.write_all(bytes).map_err(|e| e.to_string())?;
        link.flush().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::ax25::uvpro::audio::codec::{NullSbcCodec, RecordingSbcCodec};
    use std::io::{self, Read, Write};
    use std::sync::Mutex;

    /// In-memory `ByteLink` that captures everything written (TX) and reads as EOF.
    #[derive(Clone, Default)]
    struct SharedSink {
        buf: Arc<Mutex<Vec<u8>>>,
    }
    impl SharedSink {
        fn written(&self) -> Vec<u8> {
            self.buf.lock().unwrap().clone()
        }
    }
    impl Write for SharedSink {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.buf.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Read for SharedSink {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    /// In-memory `ByteLink` that replays scripted bytes once (RX), then EOF.
    struct ScriptedReader {
        data: Vec<u8>,
        pos: usize,
    }
    impl ScriptedReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }
    impl Read for ScriptedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let n = (self.data.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }
    impl Write for ScriptedReader {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn send_pcm_encodes_frames_and_writes_audiodata() {
        let sink = SharedSink::default();
        let codec = Arc::new(RecordingSbcCodec::default());
        let mut tx =
            AudioTransport::new(Box::new(sink.clone()), codec.clone(), KeyingMode::Implicit);
        tx.send_pcm(&[0x11, 0x22]).unwrap();
        assert_eq!(sink.written(), AudioMessage::Data(vec![0x11, 0x22]).to_bytes());
        assert_eq!(codec.encoded_inputs(), vec![vec![0x11u8, 0x22]]);
    }

    #[test]
    fn finish_sends_audio_end_and_disarms() {
        let sink = SharedSink::default();
        let mut tx =
            AudioTransport::new(Box::new(sink.clone()), Arc::new(NullSbcCodec), KeyingMode::Implicit);
        tx.finish().unwrap();
        assert_eq!(sink.written(), AudioMessage::End.to_bytes());
        assert!(!tx.is_connected());
    }

    #[test]
    fn abort_sends_end_then_drops_link() {
        // RADIO-1 working abort: End is emitted (best-effort de-key) and the link is
        // released so no further AudioData can be written.
        let sink = SharedSink::default();
        let mut tx =
            AudioTransport::new(Box::new(sink.clone()), Arc::new(NullSbcCodec), KeyingMode::Implicit);
        tx.abort();
        assert_eq!(sink.written(), AudioMessage::End.to_bytes());
        assert!(!tx.is_connected());
        assert!(tx.send_pcm(&[0x00]).is_err()); // cannot transmit after abort
    }

    #[test]
    fn rx_pump_decodes_audiodata_to_pcm_until_end() {
        let mut script = AudioMessage::Data(vec![0xDE, 0xAD]).to_bytes();
        script.extend(AudioMessage::End.to_bytes());
        let link = ScriptedReader::new(script);
        let mut rx =
            AudioTransport::new(Box::new(link), Arc::new(NullSbcCodec), KeyingMode::Implicit);
        let mut pcm = Vec::new();
        let ended = rx.pump_rx(&mut |chunk| pcm.extend_from_slice(chunk));
        assert_eq!(pcm, vec![0xDE, 0xAD]);
        assert!(ended);
    }

    #[test]
    fn implicit_mode_never_invokes_keying() {
        let sink = SharedSink::default();
        let keys = Arc::new(Mutex::new(Vec::<bool>::new()));
        let keys_c = keys.clone();
        let mut tx =
            AudioTransport::new(Box::new(sink.clone()), Arc::new(NullSbcCodec), KeyingMode::Implicit)
                .with_keying(Box::new(move |on| {
                    keys_c.lock().unwrap().push(on);
                    Ok(())
                }));
        tx.send_pcm(&[1]).unwrap();
        tx.finish().unwrap();
        assert!(keys.lock().unwrap().is_empty()); // no c1 keying in Implicit mode
    }

    #[test]
    fn explicit_mode_keys_once_before_first_frame_and_dekeys_on_finish() {
        let sink = SharedSink::default();
        let keys = Arc::new(Mutex::new(Vec::<bool>::new()));
        let keys_c = keys.clone();
        let mut tx =
            AudioTransport::new(Box::new(sink.clone()), Arc::new(NullSbcCodec), KeyingMode::Explicit)
                .with_keying(Box::new(move |on| {
                    keys_c.lock().unwrap().push(on);
                    Ok(())
                }));
        tx.send_pcm(&[1]).unwrap();
        tx.send_pcm(&[2]).unwrap(); // second frame must NOT re-key
        tx.finish().unwrap();
        assert_eq!(*keys.lock().unwrap(), vec![true, false]); // key on, de-key off
    }
}
