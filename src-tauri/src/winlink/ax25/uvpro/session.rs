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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::link::ByteLink;
use super::super::rfcomm::{resolve_spp_channel, RfcommSocket};
use super::gaia::{gaia_wrap, GaiaDeframer};
use super::message::{
    decode_frame, encode_get_dev_info, encode_get_ht_status, encode_read_battery_pct,
    encode_read_rf_ch, encode_read_settings, encode_register_notification, encode_write_rf_ch,
    encode_write_settings, Event, EventType, Frame,
};
use super::model::{ConnState, UvproChannel, UvproStatus};
use super::rf_ch::{Bandwidth, Modulation, RfCh};
use super::settings::{self, Vfo};
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
}

impl Driver {
    pub fn new(link: Box<dyn ByteLink>) -> Self {
        Self {
            link,
            deframer: GaiaDeframer::new(),
            status: UvproStatus { state: ConnState::Connecting, ..Default::default() },
            channels: Vec::new(),
            settings_raw: None,
        }
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
        driver.hydrate()?;
        let snap = driver.snapshot();
        *self.inner.lock().unwrap() = Some(driver);
        *self.guard.lock().unwrap() = Some(guard);
        self.set_snapshot(snap.clone());
        Ok(snap)
    }

    /// Disconnect: drop the driver (closing the socket) and release the link.
    pub fn disconnect(&self) {
        *self.inner.lock().unwrap() = None;
        *self.guard.lock().unwrap() = None; // releases the lock via LinkGuard::drop
        self.set_snapshot(UvproStatus::default());
    }

    pub fn is_connected(&self) -> bool {
        self.inner.lock().unwrap().is_some()
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
