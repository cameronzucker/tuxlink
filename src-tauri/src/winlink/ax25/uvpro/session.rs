//! UV-Pro control session driver (tuxlink-nx95).
//!
//! [`Driver`] is the synchronous, hardware-free core: it owns a [`ByteLink`]
//! (the RFCOMM socket in production, an in-memory fake in tests), a GAIA
//! deframer, and the cached radio state. All socket access is serialized — a
//! command does a single bounded request/reply, draining + applying any pushed
//! events it sees along the way (so there is no second reader racing the socket).
//! [`UvproSession`] is the managed wrapper: it holds the driver behind a mutex,
//! owns the single-Bluetooth-host [`UvproLinkLock`], and exposes a status
//! snapshot. There is NO auto-reconnect — a dropped link goes to `Disconnected`
//! and the operator re-connects.
//!
//! RADIO-1: the driver issues only control/telemetry commands; none key the
//! transmitter, and no transmit command exists. Disconnect drops the socket.

use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::link::ByteLink;
use super::super::rfcomm::{resolve_audio_channels, resolve_spp_channel, RfcommSocket};
use super::audio::codec::SbcCodec;
use super::audio::transport::{AudioTransport, KeyingMode};
use super::gaia::{gaia_wrap, GaiaDeframer};
use super::message::{
    decode_frame, encode_get_dev_info, encode_get_ht_status, encode_ht_send_data,
    encode_read_battery_pct, encode_read_rf_ch, encode_read_settings, encode_register_notification,
    encode_write_rf_ch, encode_write_settings, Event, EventType, Frame,
};
use super::model::{ConnState, UvproChannel, UvproStatus};
use super::rf_ch::{Bandwidth, Modulation, RfCh};
use super::settings::{self, Vfo};
use super::tncdata::{fragment_ax25, Reassembler};
use super::UvproError;

/// Per-command request/reply timeout. The radio answers control commands
/// promptly; a miss means a wedged or disconnected link.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
/// Socket read poll timeout (also the abort-observation granularity).
const READ_POLL: Duration = Duration::from_millis(200);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Single-Bluetooth-host owner lock
// ---------------------------------------------------------------------------

/// Process-global owner of the UV-Pro Bluetooth link. The native control profile
/// and the KISS/packet path cannot both hold the radio (one RFCOMM connection at
/// a time). Acquire returns a guard that releases on drop.
#[derive(Default)]
pub struct UvproLinkLock {
    holder: Mutex<Option<String>>,
}

impl UvproLinkLock {
    pub fn acquire(self: &Arc<Self>, who: &str) -> Result<LinkGuard, UvproError> {
        let mut h = self.holder.lock().unwrap();
        match h.as_ref() {
            Some(existing) => Err(UvproError::LinkBusy { holder: existing.clone() }),
            None => {
                *h = Some(who.to_string());
                Ok(LinkGuard { lock: Arc::clone(self) })
            }
        }
    }

    pub fn holder(&self) -> Option<String> {
        self.holder.lock().unwrap().clone()
    }
}

/// Releases the [`UvproLinkLock`] on drop — covering every disconnect path
/// (clean disconnect, socket death, error unwind).
pub struct LinkGuard {
    lock: Arc<UvproLinkLock>,
}

impl Drop for LinkGuard {
    fn drop(&mut self) {
        *self.lock.holder.lock().unwrap() = None;
    }
}

// ---------------------------------------------------------------------------
// Driver — synchronous core
// ---------------------------------------------------------------------------

/// The synchronous protocol driver over one [`ByteLink`].
pub struct Driver {
    link: Box<dyn ByteLink>,
    deframer: GaiaDeframer,
    status: UvproStatus,
    channels: Vec<RfCh>,
    settings_raw: Option<Vec<u8>>,
    /// Reassembles inbound `DATA_RXD` fragments into whole AX.25 frames.
    reassembler: Reassembler,
    /// Completed inbound APRS frames are forwarded here for the APRS engine
    /// (installed at connect; `None` for a control-only / test driver).
    aprs_tx: Option<Sender<Vec<u8>>>,
}

impl Driver {
    pub fn new(link: Box<dyn ByteLink>) -> Self {
        Self {
            link,
            deframer: GaiaDeframer::new(),
            status: UvproStatus { state: ConnState::Connecting, ..Default::default() },
            channels: Vec::new(),
            settings_raw: None,
            reassembler: Reassembler::new(),
            aprs_tx: None,
        }
    }

    /// Install the channel that completed inbound APRS frames are forwarded on.
    pub fn set_aprs_sender(&mut self, tx: Sender<Vec<u8>>) {
        self.aprs_tx = Some(tx);
    }

    /// Send a raw AX.25 APRS frame over the native link as `HT_SEND_DATA`
    /// fragments — the unified-model data path sharing this control connection.
    /// Each fragment is acknowledged by a `SendDataReply`; a non-zero status or
    /// an unexpected reply aborts the send. Live caller: `UvproSession::send_aprs_frame`
    /// (via the native driver's `AprsFrameTx` impl), driven by `AprsState::start_native`.
    pub fn send_aprs_frame(&mut self, ax25: &[u8]) -> Result<(), UvproError> {
        for frag in fragment_ax25(ax25) {
            match self.send_and_wait(&encode_ht_send_data(&frag), COMMAND_TIMEOUT)? {
                Frame::SendDataReply { reply_status: 0 } => {}
                Frame::SendDataReply { reply_status } => {
                    return Err(UvproError::RadioRejected(format!(
                        "send data status {reply_status}"
                    )))
                }
                other => {
                    return Err(UvproError::Protocol(format!(
                        "expected send-data reply, got {other:?}"
                    )))
                }
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> UvproStatus {
        self.status.clone()
    }

    pub fn channels(&self) -> Vec<UvproChannel> {
        self.channels.iter().map(UvproChannel::from_rfch).collect()
    }

    /// Write a request and return the next NON-event reply frame, applying any
    /// pushed events seen while waiting. `None`-reply (fire-and-forget) callers
    /// use [`send_no_reply`] instead.
    fn send_and_wait(&mut self, req: &[u8], timeout: Duration) -> Result<Frame, UvproError> {
        self.link
            .write_all(&gaia_wrap(req))
            .map_err(|e| UvproError::Io(e.to_string()))?;
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 256];
        loop {
            if Instant::now() >= deadline {
                return Err(UvproError::Timeout);
            }
            match self.link.read(&mut buf) {
                Ok(0) => return Err(UvproError::Io("link closed".into())),
                Ok(n) => {
                    for raw in self.deframer.push(&buf[..n]) {
                        match decode_frame(&raw) {
                            Frame::Event(ev) => self.apply_event(&ev),
                            other => return Ok(other),
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(e) => return Err(UvproError::Io(e.to_string())),
            }
        }
    }

    fn send_no_reply(&mut self, req: &[u8]) -> Result<(), UvproError> {
        self.link
            .write_all(&gaia_wrap(req))
            .map_err(|e| UvproError::Io(e.to_string()))
    }

    /// Read once (non-command) and apply any pushed events. Returns whether any
    /// event was applied (so the caller can decide to re-emit). Used by the status
    /// poller to pick up channel/status changes made on the radio itself.
    pub fn pump_events(&mut self, timeout: Duration) -> Result<bool, UvproError> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 256];
        let mut applied = false;
        loop {
            if Instant::now() >= deadline {
                return Ok(applied);
            }
            match self.link.read(&mut buf) {
                Ok(0) => return Err(UvproError::Io("link closed".into())),
                Ok(n) => {
                    for raw in self.deframer.push(&buf[..n]) {
                        if let Frame::Event(ev) = decode_frame(&raw) {
                            self.apply_event(&ev);
                            applied = true;
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Ok(applied);
                }
                Err(e) => return Err(UvproError::Io(e.to_string())),
            }
        }
    }

    fn apply_event(&mut self, ev: &Event) {
        match ev {
            Event::ChannelChanged { channel } => {
                self.upsert_channel(channel.clone());
                self.status.apply_channel(channel);
            }
            Event::StatusChanged { status } => {
                self.status.apply_status(status);
                self.apply_current_channel_to_status();
            }
            // Inbound APRS data: reassemble fragments and forward each completed
            // AX.25 frame to the APRS engine (tuxlink-7my9). A dropped sender (no
            // engine listening) is non-fatal — the frame is simply discarded.
            Event::DataReceived { fragment } => {
                if let Some(frame) = self.reassembler.push(fragment) {
                    if let Some(tx) = &self.aprs_tx {
                        let _ = tx.send(frame);
                    }
                }
            }
            Event::OtherIgnored { .. } => {}
        }
    }

    fn upsert_channel(&mut self, ch: RfCh) {
        match self.channels.iter_mut().find(|c| c.channel_id == ch.channel_id) {
            Some(slot) => *slot = ch,
            None => self.channels.push(ch),
        }
    }

    fn apply_current_channel_to_status(&mut self) {
        if let Some(id) = self.status.current_channel_id {
            if let Some(ch) = self.channels.iter().find(|c| c.channel_id as u16 == id) {
                let ch = ch.clone();
                self.status.apply_channel(&ch);
            }
        }
    }

    /// Run the connect → hydrate sequence: device info, the channel table,
    /// settings, status, then subscribe to push notifications.
    pub fn hydrate(&mut self) -> Result<(), UvproError> {
        self.hydrate_with_timeout(COMMAND_TIMEOUT)
    }

    pub fn hydrate_with_timeout(&mut self, timeout: Duration) -> Result<(), UvproError> {
        let info = match self.send_and_wait(&encode_get_dev_info(), timeout)? {
            Frame::DevInfoReply { info: Some(info), .. } => info,
            Frame::DevInfoReply { reply_status, .. } => {
                return Err(UvproError::RadioRejected(format!("dev info status {reply_status}")))
            }
            other => return Err(UvproError::Protocol(format!("expected dev info, got {other:?}"))),
        };
        self.status.apply_dev_info(&info);

        self.channels.clear();
        for i in 0..info.channel_count {
            match self.send_and_wait(&encode_read_rf_ch(i), timeout)? {
                Frame::ChannelReply { channel: Some(ch), .. } => self.channels.push(ch),
                // A radio may report fewer usable channels than channel_count; a
                // rejected slot is skipped, not fatal.
                Frame::ChannelReply { .. } => {}
                other => {
                    return Err(UvproError::Protocol(format!(
                        "expected channel {i}, got {other:?}"
                    )))
                }
            }
        }

        if let Frame::SettingsReply { settings_raw, reply_status: 0 } =
            self.send_and_wait(&encode_read_settings(), timeout)?
        {
            self.settings_raw = Some(settings_raw);
        }

        if let Frame::StatusReply { status: Some(st), .. } =
            self.send_and_wait(&encode_get_ht_status(), timeout)?
        {
            self.status.apply_status(&st);
            self.apply_current_channel_to_status();
        }

        // Initial battery reading (best-effort — absence is non-fatal; the
        // broadcaster refreshes it on a bounded cadence thereafter).
        let _ = self.refresh_battery(timeout);

        // Subscribe to push notifications (fire-and-forget — no reply frame).
        self.send_no_reply(&encode_register_notification(EventType::HtStatusChanged))?;
        // Subscribe to inbound APRS data so DATA_RXD events stream to the engine.
        self.send_no_reply(&encode_register_notification(EventType::DataRxd))?;

        self.status.state = ConnState::Connected;
        Ok(())
    }

    /// Poll the live status (drains pending events too). Returns the snapshot.
    pub fn refresh_status(&mut self, timeout: Duration) -> Result<UvproStatus, UvproError> {
        if let Frame::StatusReply { status: Some(st), .. } =
            self.send_and_wait(&encode_get_ht_status(), timeout)?
        {
            self.status.apply_status(&st);
            self.apply_current_channel_to_status();
        }
        Ok(self.status.clone())
    }

    /// Poll the battery percentage and fold it into the status.
    pub fn refresh_battery(&mut self, timeout: Duration) -> Result<(), UvproError> {
        if let Frame::BatteryReply { reply_status: 0, value } =
            self.send_and_wait(&encode_read_battery_pct(), timeout)?
        {
            self.status.battery_percent = Some(value);
        }
        Ok(())
    }

    fn channel_clone(&self, channel_id: u8) -> Result<RfCh, UvproError> {
        self.channels
            .iter()
            .find(|c| c.channel_id == channel_id)
            .cloned()
            .ok_or_else(|| UvproError::Protocol(format!("unknown channel {channel_id}")))
    }

    fn write_channel(&mut self, ch: RfCh, timeout: Duration) -> Result<(), UvproError> {
        match self.send_and_wait(&encode_write_rf_ch(&ch), timeout)? {
            Frame::WriteRfChReply { reply_status: 0, .. } => {
                self.upsert_channel(ch);
                self.apply_current_channel_to_status();
                Ok(())
            }
            Frame::WriteRfChReply { reply_status, .. } => {
                Err(UvproError::RadioRejected(format!("write channel status {reply_status}")))
            }
            other => Err(UvproError::Protocol(format!("expected write reply, got {other:?}"))),
        }
    }

    /// Set the RX (and optional TX) frequency of a channel via read-modify-write.
    pub fn set_frequency(
        &mut self,
        channel_id: u8,
        rx_mhz: f64,
        tx_mhz: Option<f64>,
    ) -> Result<(), UvproError> {
        let mut ch = self.channel_clone(channel_id)?;
        ch.rx_freq_hz = RfCh::mhz_to_hz(rx_mhz);
        ch.tx_freq_hz = RfCh::mhz_to_hz(tx_mhz.unwrap_or(rx_mhz));
        self.write_channel(ch, COMMAND_TIMEOUT)
    }

    /// Set the modulation + optional bandwidth of a channel via read-modify-write.
    pub fn set_mode(
        &mut self,
        channel_id: u8,
        mode: Modulation,
        bandwidth: Option<Bandwidth>,
    ) -> Result<(), UvproError> {
        let mut ch = self.channel_clone(channel_id)?;
        ch.rx_mod = mode;
        ch.tx_mod = mode;
        if let Some(bw) = bandwidth {
            ch.bandwidth = bw;
        }
        self.write_channel(ch, COMMAND_TIMEOUT)
    }

    /// Select the active memory channel by writing `Settings.channel_a`/`_b`.
    pub fn set_channel(&mut self, channel_id: u8, vfo: Vfo) -> Result<(), UvproError> {
        let raw = self
            .settings_raw
            .as_ref()
            .ok_or(UvproError::NotConnected)?
            .clone();
        let patched = settings::patch_channel(&raw, vfo, channel_id)
            .ok_or_else(|| UvproError::Protocol("settings block malformed".into()))?;
        match self.send_and_wait(&encode_write_settings(&patched), COMMAND_TIMEOUT)? {
            Frame::WriteSettingsReply { reply_status: 0 } => {
                self.settings_raw = Some(patched);
                // Reflect the new active channel in the status snapshot.
                self.status.current_channel_id = Some(channel_id as u16);
                self.apply_current_channel_to_status();
                Ok(())
            }
            Frame::WriteSettingsReply { reply_status } => {
                Err(UvproError::RadioRejected(format!("write settings status {reply_status}")))
            }
            other => Err(UvproError::Protocol(format!("expected settings reply, got {other:?}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// UvproSession — managed wrapper
// ---------------------------------------------------------------------------

/// The managed control session (one per app). Holds the driver behind a mutex
/// and owns the single-Bluetooth-host lock.
pub struct UvproSession {
    inner: Mutex<Option<Driver>>,
    guard: Mutex<Option<LinkGuard>>,
    lock: Arc<UvproLinkLock>,
    snapshot: Mutex<UvproStatus>,
    /// Receiver for completed inbound APRS frames, set at connect. `take_aprs_receiver`
    /// hands it to the native APRS driver (`AprsState::start_native`) to feed the chat
    /// engine.
    aprs_rx: Mutex<Option<Receiver<Vec<u8>>>>,
    /// MAC of the connected radio, cached at connect so the SSTV audio channel can be
    /// opened to the SAME device (tuxlink-bcsy). `None` when disconnected.
    mac: Mutex<Option<String>>,
    /// The SSTV audio transport over a SECOND RFCOMM channel (tuxlink-bcsy), when a
    /// send/receive is active. Independent of the control `inner` driver — a separate
    /// socket multiplexed over the same Bluetooth ACL link, part of the same locked
    /// native session.
    audio: Mutex<Option<AudioTransport>>,
}

impl Default for UvproSession {
    fn default() -> Self {
        Self::new()
    }
}

impl UvproSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
            guard: Mutex::new(None),
            lock: Arc::new(UvproLinkLock::default()),
            snapshot: Mutex::new(UvproStatus::default()),
            aprs_rx: Mutex::new(None),
            mac: Mutex::new(None),
            audio: Mutex::new(None),
        }
    }

    pub fn link_lock(&self) -> Arc<UvproLinkLock> {
        Arc::clone(&self.lock)
    }

    pub fn status_snapshot(&self) -> UvproStatus {
        self.snapshot.lock().unwrap().clone()
    }

    fn set_snapshot(&self, s: UvproStatus) {
        *self.snapshot.lock().unwrap() = s;
    }

    /// Connect over RFCOMM to `mac`, acquire the link, hydrate, and cache state.
    pub fn connect(&self, mac: &str) -> Result<UvproStatus, UvproError> {
        // Acquire the single-host lock first so a busy radio fails fast.
        let guard = self.lock.acquire("uvpro-native")?;
        let channel = resolve_spp_channel(mac);
        let socket = RfcommSocket::connect(mac, channel, READ_POLL, WRITE_TIMEOUT)
            .map_err(|e| UvproError::Io(e.to_string()))?;
        let mut driver = Driver::new(Box::new(socket));
        // Wire the inbound APRS channel before hydrate so any DATA_RXD that
        // arrives during/after subscription is captured for the engine.
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        driver.set_aprs_sender(tx);
        driver.hydrate()?;
        let snap = driver.snapshot();
        *self.inner.lock().unwrap() = Some(driver);
        *self.guard.lock().unwrap() = Some(guard);
        *self.aprs_rx.lock().unwrap() = Some(rx);
        *self.mac.lock().unwrap() = Some(mac.to_string());
        self.set_snapshot(snap.clone());
        Ok(snap)
    }

    /// Disconnect: abort any in-flight audio (RADIO-1: halt TX first), drop the
    /// driver (closing the control socket), and release the link.
    pub fn disconnect(&self) {
        self.abort_audio(); // halt + drop the audio socket before the control link
        *self.inner.lock().unwrap() = None;
        *self.guard.lock().unwrap() = None; // releases the lock via LinkGuard::drop
        *self.aprs_rx.lock().unwrap() = None;
        *self.mac.lock().unwrap() = None;
        self.set_snapshot(UvproStatus::default());
    }

    /// Take the inbound-APRS-frame receiver (once, after connect). The APRS
    /// native driver owns it for the connection's lifetime; a second caller
    /// gets `None`. Live caller: `AprsState::start_native`.
    pub fn take_aprs_receiver(&self) -> Option<Receiver<Vec<u8>>> {
        self.aprs_rx.lock().unwrap().take()
    }

    /// Send a raw AX.25 APRS frame over the native link (fragmented as
    /// `HT_SEND_DATA`). Shares the one connection with control commands. Live caller:
    /// the native driver's `AprsFrameTx` impl, driven by `AprsState::start_native`.
    pub fn send_aprs_frame(&self, ax25: &[u8]) -> Result<(), UvproError> {
        self.with_driver(|d| d.send_aprs_frame(ax25))
    }

    pub fn is_connected(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    // -----------------------------------------------------------------------
    // SSTV audio channel (tuxlink-bcsy) — a SECOND RFCOMM socket to the same
    // radio, multiplexed over the same ACL link as the GAIA control channel.
    // -----------------------------------------------------------------------

    /// Open the SSTV audio channel to the connected radio: resolve the audio-gateway
    /// RFCOMM channel from the live SDP record, connect a SECOND socket, and build the
    /// [`AudioTransport`] with the supplied SBC `codec` (injected because the codec is
    /// a sibling sub-project, `tuxlink-vgvn`). Keying defaults to `Implicit` (benlink's
    /// working POC keys TX by streaming on the audio channel, sending no `c1.TX_AUDIO`).
    ///
    /// Requires an active control connection — the audio channel belongs to the same
    /// already-locked native session, so it does NOT re-acquire [`UvproLinkLock`] (a
    /// 2nd RFCOMM channel to the same radio is multiplexed, not a competing host).
    /// Tries the advertised audio-gateway channels in order; the first to connect wins.
    pub fn open_audio(&self, codec: Arc<dyn SbcCodec>) -> Result<(), UvproError> {
        let mac = self
            .mac
            .lock()
            .unwrap()
            .clone()
            .ok_or(UvproError::NotConnected)?;
        let channels = resolve_audio_channels(&mac);
        if channels.is_empty() {
            return Err(UvproError::Protocol(
                "radio advertises no audio-gateway RFCOMM service".into(),
            ));
        }
        let mut last_err: Option<String> = None;
        for ch in channels {
            match RfcommSocket::connect(&mac, ch, READ_POLL, WRITE_TIMEOUT) {
                Ok(socket) => {
                    let transport = AudioTransport::new(
                        Box::new(socket),
                        Arc::clone(&codec),
                        KeyingMode::Implicit,
                    );
                    *self.audio.lock().unwrap() = Some(transport);
                    return Ok(());
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        Err(UvproError::Io(format!(
            "failed to open any audio-gateway RFCOMM channel: {}",
            last_err.unwrap_or_default()
        )))
    }

    /// Send one PCM chunk over the open audio channel (encoded + framed by the
    /// transport). `Err(NotConnected)` if no audio channel is open.
    pub fn send_audio_pcm(&self, pcm: &[u8]) -> Result<(), UvproError> {
        let mut g = self.audio.lock().unwrap();
        let t = g.as_mut().ok_or(UvproError::NotConnected)?;
        t.send_pcm(pcm).map_err(UvproError::Io)
    }

    /// Clean end of an audio transmission: send `AudioEnd`, drop the audio socket.
    /// `Err(NotConnected)` if no audio channel is open.
    pub fn finish_audio(&self) -> Result<(), UvproError> {
        let mut g = self.audio.lock().unwrap();
        let t = g.as_mut().ok_or(UvproError::NotConnected)?;
        let r = t.finish().map_err(UvproError::Io);
        *g = None; // transport is single-use; drop after finish
        r
    }

    /// RADIO-1 working abort for the audio path: best-effort `AudioEnd` then DROP the
    /// audio socket so no further audio can be transmitted. Safe to call with no audio
    /// open (no-op). Also invoked by [`UvproSession::disconnect`].
    pub fn abort_audio(&self) {
        if let Some(mut t) = self.audio.lock().unwrap().take() {
            t.abort();
        }
    }

    /// True when an audio transport is open.
    pub fn audio_active(&self) -> bool {
        self.audio.lock().unwrap().is_some()
    }

    /// Test-only: inject a pre-built [`AudioTransport`] (over a fake `ByteLink`) so the
    /// session-level audio lifecycle can be exercised without real Bluetooth — the
    /// real socket path (`open_audio`) is operator-smoked, like `connect`.
    #[cfg(test)]
    fn inject_audio_for_test(&self, transport: AudioTransport) {
        *self.audio.lock().unwrap() = Some(transport);
    }

    /// Run `op` against the connected driver, refreshing the cached snapshot.
    fn with_driver<T>(
        &self,
        op: impl FnOnce(&mut Driver) -> Result<T, UvproError>,
    ) -> Result<T, UvproError> {
        let mut g = self.inner.lock().unwrap();
        let driver = g.as_mut().ok_or(UvproError::NotConnected)?;
        let out = op(driver);
        let snap = driver.snapshot();
        drop(g);
        self.set_snapshot(snap);
        out
    }

    pub fn channels(&self) -> Result<Vec<UvproChannel>, UvproError> {
        let g = self.inner.lock().unwrap();
        Ok(g.as_ref().ok_or(UvproError::NotConnected)?.channels())
    }

    pub fn set_frequency(
        &self,
        channel_id: u8,
        rx_mhz: f64,
        tx_mhz: Option<f64>,
    ) -> Result<UvproStatus, UvproError> {
        self.with_driver(|d| d.set_frequency(channel_id, rx_mhz, tx_mhz))?;
        Ok(self.status_snapshot())
    }

    pub fn set_mode(
        &self,
        channel_id: u8,
        mode: Modulation,
        bandwidth: Option<Bandwidth>,
    ) -> Result<UvproStatus, UvproError> {
        self.with_driver(|d| d.set_mode(channel_id, mode, bandwidth))?;
        Ok(self.status_snapshot())
    }

    pub fn set_channel(&self, channel_id: u8, vfo: Vfo) -> Result<UvproStatus, UvproError> {
        self.with_driver(|d| d.set_channel(channel_id, vfo))?;
        Ok(self.status_snapshot())
    }

    /// Poll status (+ drain pending events) and update the snapshot — for the
    /// background broadcaster. Returns the fresh snapshot.
    pub fn poll_tick(&self) -> Result<UvproStatus, UvproError> {
        self.with_driver(|d| d.refresh_status(COMMAND_TIMEOUT))?;
        Ok(self.status_snapshot())
    }

    /// Poll the battery percentage on the broadcaster's bounded cadence (battery
    /// has no push event). Best-effort: returns the fresh snapshot.
    pub fn poll_battery(&self) -> Result<UvproStatus, UvproError> {
        self.with_driver(|d| d.refresh_battery(COMMAND_TIMEOUT))?;
        Ok(self.status_snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    // Golden reply fixtures (benlink-generated).
    const DEVINFO_REPLY: &str = "00 02 80 04 00 01 12 34 02 01 07 40 12 02 10";
    const RF_CH0_REPLY: &str =
        "00 02 80 0d 00 00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00";
    const RF_CH1_REPLY: &str =
        "00 02 80 0d 00 01 1a 95 6b 80 1a 95 6b 80 00 00 00 00 40 00 55 48 46 00 00 00 00 00 00 00";
    const SETTINGS_REPLY: &str =
        "00 02 80 0a 00 00 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00";
    const STATUS_REPLY: &str = "00 02 80 14 00 b4 3c c0 00";
    const WRITE_RF_CH_REPLY: &str = "00 02 80 0e 00 00";
    const WRITE_SETTINGS_REPLY: &str = "00 02 80 0b 00";
    const CH_CHANGED_EVENT: &str =
        "00 02 00 09 05 05 1a 95 6b 80 1a 95 6b 80 00 00 00 00 40 00 55 48 46 00 00 00 00 00 00 00";

    /// In-memory `ByteLink` that answers requests with canned replies and lets a
    /// test inject unsolicited events. `silent=true` answers nothing (timeout).
    #[derive(Clone)]
    struct FakePeer {
        out: Arc<StdMutex<VecDeque<u8>>>, // bytes the driver will read
        last_write: Arc<StdMutex<Vec<u8>>>,
        silent: bool,
    }

    impl FakePeer {
        fn new(silent: bool) -> Self {
            Self {
                out: Arc::new(StdMutex::new(VecDeque::new())),
                last_write: Arc::new(StdMutex::new(Vec::new())),
                silent,
            }
        }
        fn enqueue(&self, msg: &[u8]) {
            let framed = gaia_wrap(msg);
            self.out.lock().unwrap().extend(framed);
        }
        fn push_event(&self, msg: &str) {
            self.enqueue(&hex(msg));
        }
        fn last_written_message(&self) -> Vec<u8> {
            // strip the 4-byte GAIA header from the last write
            let w = self.last_write.lock().unwrap();
            if w.len() > 4 {
                w[4..].to_vec()
            } else {
                w.clone()
            }
        }
    }

    impl Read for FakePeer {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut q = self.out.lock().unwrap();
            if q.is_empty() {
                return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "empty"));
            }
            let n = buf.len().min(q.len());
            for b in buf.iter_mut().take(n) {
                *b = q.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for FakePeer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            *self.last_write.lock().unwrap() = buf.to_vec();
            if !self.silent {
                // command id is at wrapped[7] (ff 01 00 n 00 02 00 CMD)
                let cmd = buf.get(7).copied().unwrap_or(0);
                match cmd {
                    0x04 => self.enqueue(&hex(DEVINFO_REPLY)),
                    0x0d => {
                        let ch = buf.get(8).copied().unwrap_or(0);
                        if ch == 0 {
                            self.enqueue(&hex(RF_CH0_REPLY));
                        } else {
                            self.enqueue(&hex(RF_CH1_REPLY));
                        }
                    }
                    0x0a => self.enqueue(&hex(SETTINGS_REPLY)),
                    0x14 => self.enqueue(&hex(STATUS_REPLY)),
                    0x05 => self.enqueue(&hex("00 02 80 05 00 00 04 49")),
                    0x0e => self.enqueue(&hex(WRITE_RF_CH_REPLY)),
                    0x0b => self.enqueue(&hex(WRITE_SETTINGS_REPLY)),
                    0x1f => self.enqueue(&hex("00 02 80 1f 00")), // HT_SEND_DATA reply: SUCCESS
                    0x06 => {} // REGISTER_NOTIFICATION: fire-and-forget
                    _ => {}
                }
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn driver_with(peer: FakePeer) -> Driver {
        Driver::new(Box::new(peer))
    }

    // --- SSTV audio channel (tuxlink-bcsy) ---------------------------------
    use super::super::audio::codec::NullSbcCodec;
    use super::super::audio::framing::AudioMessage;

    /// Minimal fake audio `ByteLink`: captures writes (TX), reads as EOF.
    #[derive(Clone, Default)]
    struct AudioSink {
        buf: Arc<StdMutex<Vec<u8>>>,
    }
    impl AudioSink {
        fn written(&self) -> Vec<u8> {
            self.buf.lock().unwrap().clone()
        }
    }
    impl Read for AudioSink {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }
    impl Write for AudioSink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.buf.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn audio_transport(sink: AudioSink) -> AudioTransport {
        AudioTransport::new(Box::new(sink), Arc::new(NullSbcCodec), KeyingMode::Implicit)
    }

    #[test]
    fn open_audio_errors_not_connected_without_a_control_session() {
        let s = UvproSession::new();
        // mac is None (never connected) → NotConnected before any socket work.
        assert_eq!(
            s.open_audio(Arc::new(NullSbcCodec)).unwrap_err().kind(),
            "NotConnected"
        );
    }

    #[test]
    fn send_audio_pcm_errors_when_no_audio_channel_open() {
        let s = UvproSession::new();
        assert_eq!(s.send_audio_pcm(&[0x00]).unwrap_err().kind(), "NotConnected");
        assert!(!s.audio_active());
    }

    #[test]
    fn finish_audio_sends_end_and_clears_the_transport() {
        let s = UvproSession::new();
        let sink = AudioSink::default();
        s.inject_audio_for_test(audio_transport(sink.clone()));
        assert!(s.audio_active());
        s.finish_audio().unwrap();
        assert_eq!(sink.written(), AudioMessage::End.to_bytes());
        assert!(!s.audio_active()); // single-use: dropped after finish
    }

    #[test]
    fn abort_audio_halts_and_clears_the_transport() {
        let s = UvproSession::new();
        let sink = AudioSink::default();
        s.inject_audio_for_test(audio_transport(sink.clone()));
        s.abort_audio();
        // RADIO-1: best-effort AudioEnd emitted, then the socket is dropped.
        assert_eq!(sink.written(), AudioMessage::End.to_bytes());
        assert!(!s.audio_active());
        s.abort_audio(); // idempotent / safe with no audio open
    }

    #[test]
    fn disconnect_tears_down_an_open_audio_channel() {
        let s = UvproSession::new();
        let sink = AudioSink::default();
        s.inject_audio_for_test(audio_transport(sink.clone()));
        assert!(s.audio_active());
        s.disconnect();
        assert!(!s.audio_active()); // disconnect aborts + drops audio (TX halted first)
        assert_eq!(sink.written(), AudioMessage::End.to_bytes());
    }

    #[test]
    fn send_aprs_frame_one_fragment_emits_ht_send_data() {
        let peer = FakePeer::new(false);
        let mut driver = driver_with(peer.clone());
        driver.send_aprs_frame(&[0x41, 0x42, 0x43]).unwrap();
        // The single (final) fragment encodes to exactly this HT_SEND_DATA body.
        let frag = super::super::tncdata::TncDataFragment {
            is_final: true,
            fragment_id: 0,
            channel_id: None,
            data: vec![0x41, 0x42, 0x43],
        };
        assert_eq!(peer.last_written_message(), encode_ht_send_data(&frag));
    }

    #[test]
    fn send_aprs_frame_fragments_long_frame_and_acks_all() {
        let mut driver = driver_with(FakePeer::new(false));
        // 120 bytes -> 3 fragments (53,53,14); each acked SUCCESS -> Ok overall.
        assert!(driver.send_aprs_frame(&[0xAA; 120]).is_ok());
    }

    #[test]
    fn data_rxd_fragments_reassemble_to_the_aprs_channel() {
        let peer = FakePeer::new(true); // silent: events are injected manually
        let mut driver = driver_with(peer.clone());
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        driver.set_aprs_sender(tx);
        // Two DATA_RXD events: frag0 [11 22] (not final, id 0), frag1 [33] (final, id 1).
        peer.push_event("00 02 00 09 02 00 11 22");
        peer.push_event("00 02 00 09 02 81 33");
        driver.pump_events(Duration::from_millis(50)).unwrap();
        assert_eq!(rx.try_recv().unwrap(), vec![0x11, 0x22, 0x33]);
    }

    #[test]
    fn hydrate_populates_state_and_channels() {
        let peer = FakePeer::new(false);
        let mut d = driver_with(peer);
        d.hydrate().unwrap();
        let st = d.snapshot();
        assert_eq!(st.state, ConnState::Connected);
        assert_eq!(d.channels().len(), 2);
        assert!(st.rssi.is_some());
        assert_eq!(st.current_channel_id, Some(3));
    }

    #[test]
    fn set_frequency_emits_write_rf_ch() {
        let peer = FakePeer::new(false);
        let probe = peer.clone();
        let mut d = driver_with(peer);
        d.hydrate().unwrap();
        d.set_frequency(0, 147.0, None).unwrap();
        let sent = probe.last_written_message();
        assert_eq!(&sent[..4], &hex("00 02 00 0e")[..]); // WRITE_RF_CH header
        // the encoded channel should carry the new frequency
        let ch = RfCh::decode(&sent[4..]).unwrap();
        assert_eq!(ch.rx_freq_hz, 147_000_000);
    }

    #[test]
    fn channel_changed_event_updates_state() {
        let peer = FakePeer::new(false);
        let probe = peer.clone();
        let mut d = driver_with(peer);
        d.hydrate().unwrap();
        probe.push_event(CH_CHANGED_EVENT);
        d.pump_events(Duration::from_millis(10)).unwrap();
        // channel 5 ("UHF") was upserted
        assert!(d.channels().iter().any(|c| c.channel_id == 5 && c.name == "UHF"));
    }

    #[test]
    fn set_channel_writes_settings_and_updates_active() {
        let peer = FakePeer::new(false);
        let mut d = driver_with(peer);
        d.hydrate().unwrap();
        d.set_channel(1, Vfo::A).unwrap();
        assert_eq!(d.snapshot().current_channel_id, Some(1));
    }

    #[test]
    fn command_times_out_when_silent() {
        let peer = FakePeer::new(true);
        let mut d = driver_with(peer);
        let err = d.hydrate_with_timeout(Duration::from_millis(40)).unwrap_err();
        assert!(matches!(err, UvproError::Timeout));
    }

    #[test]
    fn link_lock_is_exclusive_and_releases_on_drop() {
        let lock = Arc::new(UvproLinkLock::default());
        {
            let _g = lock.acquire("uvpro-native").unwrap();
            assert!(matches!(
                lock.acquire("uvpro-native"),
                Err(UvproError::LinkBusy { .. })
            ));
        }
        // guard dropped → reacquire succeeds
        assert!(lock.acquire("again").is_ok());
    }

    #[test]
    fn session_set_before_connect_is_not_connected() {
        let sess = UvproSession::new();
        assert_eq!(sess.status_snapshot().state, ConnState::Disconnected);
        assert!(matches!(
            sess.set_frequency(0, 146.52, None),
            Err(UvproError::NotConnected)
        ));
    }

    #[test]
    fn session_connect_link_busy_when_lock_held() {
        let sess = UvproSession::new();
        let lock = sess.link_lock();
        let _g = lock.acquire("kiss-packet").unwrap();
        assert!(matches!(
            sess.connect("38:D2:00:01:55:5C"),
            Err(UvproError::LinkBusy { .. })
        ));
    }
}
