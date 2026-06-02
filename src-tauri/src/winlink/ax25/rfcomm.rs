//! In-app Bluetooth RFCOMM-socket transport for the KISS byte-pipe (tuxlink-nx2).
//!
//! The original Bluetooth path bound a kernel rfcomm TTY (`sudo rfcomm bind
//! /dev/rfcommN <mac> <ch>`) and opened it through the `serialport` crate. That TTY
//! open reconfigures the line (baud, exclusive lock, termios/modem-control), and the
//! UV-Pro's SPP/KISS service reacts by tearing the session down — the first KISS
//! write then fails with `Broken pipe`. Windows, Android (WoAD), and the native
//! Winlink client all avoid this by opening a real RFCOMM *socket*; this module does
//! the same: an `AF_BLUETOOTH`/`BTPROTO_RFCOMM` `SOCK_STREAM` connected directly to
//! the radio's `MAC` + SPP channel — no `rfcomm bind`, no root, no TTY, no termios.
//!
//! The connected socket is a plain Read+Write byte stream, so it drops into the
//! existing `ByteLink` trait exactly like `TcpStream`. The SPP channel is read from
//! the radio's SDP record at connect time (it rotates per registration), falling
//! back to channel 1.

/// Parse an `"AA:BB:CC:DD:EE:FF"` MAC string into the 6-byte address the kernel's
/// `sockaddr_rc` expects.
///
/// **Byte order is the footgun:** BlueZ stores a `bdaddr_t` little-endian — least
/// significant byte first — i.e. the human-readable string bytes *reversed*. So
/// `38:D2:00:01:55:5C` becomes `[0x5C, 0x55, 0x01, 0x00, 0xD2, 0x38]`. Returns
/// `None` for anything that is not exactly six colon-separated hex octets.
pub fn parse_bdaddr(mac: &str) -> Option<[u8; 6]> {
    let mut octets = [0u8; 6];
    let mut n = 0;
    for part in mac.split(':') {
        if n >= 6 || part.len() != 2 {
            return None;
        }
        octets[n] = u8::from_str_radix(part, 16).ok()?;
        n += 1;
    }
    if n != 6 {
        return None;
    }
    octets.reverse(); // string is MSB-first; sockaddr_rc wants LSB-first
    Some(octets)
}

/// Extract the RFCOMM channel of the SPP (Serial Port, 0x1101) service from
/// `sdptool records <mac>` output. The channel rotates per registration, so it must
/// be read fresh at connect time rather than hardcoded.
///
/// A device can advertise several RFCOMM services (the UV-Pro also exposes audio
/// gateways), so this picks, in order of preference: a Serial-Port service whose
/// name mentions "SPP", else any Serial-Port (0x1101) service, else `None` (the
/// caller then falls back to channel 1). Returns the first match.
pub fn parse_spp_channel(records: &str) -> Option<u8> {
    // Walk the records as blocks delimited by "Service Name:" headers, tracking the
    // current block's name / Serial-Port-ness / channel. Collect candidates and rank.
    #[derive(Default)]
    struct Block {
        name: String,
        is_serial_port: bool,
        channel: Option<u8>,
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut cur = Block::default();
    let mut have_block = false;

    let flush = |blocks: &mut Vec<Block>, cur: &mut Block, have: &mut bool| {
        if *have {
            blocks.push(std::mem::take(cur));
        }
        *have = false;
    };

    for raw in records.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Service Name:") {
            flush(&mut blocks, &mut cur, &mut have_block);
            cur.name = rest.trim().to_string();
            have_block = true;
        } else if line.contains("(0x1101)") {
            // "Serial Port" (0x1101) in the Service Class ID List.
            cur.is_serial_port = true;
        } else if let Some(rest) = line.strip_prefix("Channel:") {
            if let Ok(ch) = rest.trim().parse::<u8>() {
                cur.channel = Some(ch);
            }
        }
    }
    flush(&mut blocks, &mut cur, &mut have_block);

    // Prefer an SPP-named serial port, then any serial port with a channel.
    blocks
        .iter()
        .find(|b| b.is_serial_port && b.name.to_ascii_uppercase().contains("SPP"))
        .and_then(|b| b.channel)
        .or_else(|| {
            blocks
                .iter()
                .find(|b| b.is_serial_port && b.channel.is_some())
                .and_then(|b| b.channel)
        })
}

// AF_BLUETOOTH / BTPROTO_RFCOMM are not in every libc release's constant set, so
// pin them here (stable kernel ABI values).
const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_RFCOMM: libc::c_int = 3;

/// The kernel's `struct sockaddr_rc` (bluetooth/rfcomm.h). NOT packed: natural
/// alignment gives `sizeof == 10` (2 family + 6 bdaddr + 1 channel + 1 pad), which
/// is exactly what `connect()` expects for `addrlen`.
#[repr(C)]
struct SockaddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: [u8; 6],
    rc_channel: u8,
}

/// A connected Bluetooth RFCOMM socket presented as a Read+Write byte pipe — the
/// drop-in `ByteLink` replacement for the old `rfcomm bind` + serialport TTY path.
pub struct RfcommSocket {
    fd: libc::c_int,
}

impl RfcommSocket {
    /// Create an unconnected `AF_BLUETOOTH`/`BTPROTO_RFCOMM` stream socket. Verified
    /// creatable by a non-root user (uid 1000) — no `rfcomm bind`, no root.
    fn socket_fd() -> std::io::Result<libc::c_int> {
        // SAFETY: `socket(2)` with constant, valid domain/type/protocol; the return
        // is checked and any negative value is turned into the OS error.
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(fd)
    }

    /// Connect to `mac` on RFCOMM `channel`. `read_timeout` is applied as
    /// `SO_RCVTIMEO` so a `read` on an idle line returns `EAGAIN` (mapped to
    /// `WouldBlock`) instead of blocking — matching the serial link's poll contract
    /// that `recv_frame` relies on.
    pub fn connect(
        mac: &str,
        channel: u8,
        read_timeout: std::time::Duration,
        write_timeout: std::time::Duration,
    ) -> std::io::Result<Self> {
        let bdaddr = parse_bdaddr(mac).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid Bluetooth MAC address: {mac:?}"),
            )
        })?;
        let fd = Self::socket_fd()?;
        let addr = SockaddrRc {
            rc_family: AF_BLUETOOTH as libc::sa_family_t,
            rc_bdaddr: bdaddr,
            rc_channel: channel,
        };
        // SAFETY: `addr` is a fully-initialised SockaddrRc living on this stack frame
        // for the duration of the call; the pointer cast + size match the kernel ABI.
        let rc = unsafe {
            libc::connect(
                fd,
                &addr as *const SockaddrRc as *const libc::sockaddr,
                std::mem::size_of::<SockaddrRc>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            // SAFETY: close our own fd so a failed connect doesn't leak it.
            unsafe { libc::close(fd) };
            return Err(err);
        }
        let sock = RfcommSocket { fd };
        sock.set_timeout(libc::SO_RCVTIMEO, read_timeout)?;
        sock.set_timeout(libc::SO_SNDTIMEO, write_timeout)?;
        Ok(sock)
    }

    fn set_timeout(&self, opt: libc::c_int, d: std::time::Duration) -> std::io::Result<()> {
        let tv = libc::timeval {
            tv_sec: d.as_secs() as libc::time_t,
            tv_usec: d.subsec_micros() as libc::suseconds_t,
        };
        // SAFETY: setsockopt with a valid timeval of the declared length on our fd.
        let rc = unsafe {
            libc::setsockopt(
                self.fd,
                libc::SOL_SOCKET,
                opt,
                &tv as *const libc::timeval as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Format bytes as space-separated lowercase hex for the on-air diagnostic trace.
fn hex_dump(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Timestamped RX/TX byte trace for diagnosing the RFCOMM no-RX bug (tuxlink-4ef).
/// Opt-in via the `TUXLINK_RFCOMM_TRACE` env var (silent otherwise) so the operator's
/// ONE bounded + abortable on-air dial yields byte-level EVIDENCE instead of guesses:
/// if TX lines print but no RX line ever does, the socket receives nothing (an
/// RFCOMM/SPP transport issue, NOT a KISS-decode bug) → fall back to the proven TTY or
/// fix the socket; if RX bytes arrive but don't decode, it's the KISS/AX.25 layer.
/// Writes to stderr (the `tauri dev` console) — no session-log plumbing needed here.
fn trace_bytes(dir: &str, bytes: &[u8]) {
    if std::env::var_os("TUXLINK_RFCOMM_TRACE").is_none() {
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}.{:03}", d.as_secs(), d.subsec_millis()))
        .unwrap_or_default();
    eprintln!("[rfcomm {ts}] {dir} {} bytes: {}", bytes.len(), hex_dump(bytes));
}

impl std::io::Read for RfcommSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // SAFETY: read up to buf.len() bytes into the caller's buffer on our fd.
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            // SO_RCVTIMEO expiry surfaces as EAGAIN/EWOULDBLOCK, which Rust maps to
            // ErrorKind::WouldBlock — recv_frame treats that as "no frame yet".
            return Err(std::io::Error::last_os_error());
        }
        let n = n as usize;
        if n > 0 {
            trace_bytes("RX", &buf[..n]); // tuxlink-4ef on-air evidence
        }
        Ok(n)
    }
}

impl std::io::Write for RfcommSocket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // SAFETY: write buf.len() bytes from the caller's buffer on our fd.
        let n = unsafe { libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        trace_bytes("TX", &buf[..n as usize]); // tuxlink-4ef on-air evidence
        Ok(n as usize)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for RfcommSocket {
    fn drop(&mut self) {
        // SAFETY: close our own fd exactly once (RfcommSocket owns it, !Copy).
        unsafe { libc::close(self.fd) };
    }
}

/// Read the SPP RFCOMM channel from the radio's SDP record (`sdptool records
/// <mac>`). The channel rotates per registration, so it is resolved fresh at
/// connect time; falls back to channel 1 if the query fails or advertises no
/// Serial-Port service. A pure-D-Bus/FFI SDP query (no `sdptool` shell-out) is a
/// follow-up — `sdptool` is a read-only query here, not the `rfcomm bind` jank.
pub fn resolve_spp_channel(mac: &str) -> u8 {
    std::process::Command::new("sdptool")
        .args(["records", mac])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_spp_channel(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    // tuxlink-4ef: the on-air byte trace formats bytes as space-separated lowercase hex
    // so the operator can read TX/RX frames off the console during the diagnostic dial.
    #[test]
    fn hex_dump_formats_space_separated_lowercase() {
        assert_eq!(hex_dump(&[0xC0, 0x00, 0xAB, 0x0F]), "c0 00 ab 0f");
        assert_eq!(hex_dump(&[]), "");
    }

    #[test]
    fn parse_bdaddr_reverses_octets_for_little_endian_sockaddr() {
        // BlueZ bdaddr_t is LSB-first: the string bytes reversed.
        assert_eq!(
            parse_bdaddr("38:D2:00:01:55:5C"),
            Some([0x5C, 0x55, 0x01, 0x00, 0xD2, 0x38])
        );
    }

    #[test]
    fn parse_bdaddr_accepts_lowercase() {
        assert_eq!(
            parse_bdaddr("aa:bb:cc:dd:ee:ff"),
            Some([0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA])
        );
    }

    #[test]
    fn parse_bdaddr_rejects_malformed() {
        assert_eq!(parse_bdaddr(""), None);
        assert_eq!(parse_bdaddr("38:D2:00:01:55"), None); // 5 octets
        assert_eq!(parse_bdaddr("38:D2:00:01:55:5C:7A"), None); // 7 octets
        assert_eq!(parse_bdaddr("38-D2-00-01-55-5C"), None); // wrong sep
        assert_eq!(parse_bdaddr("3:D2:00:01:55:5C"), None); // short octet
        assert_eq!(parse_bdaddr("ZZ:D2:00:01:55:5C"), None); // non-hex
    }

    /// Representative `sdptool records 38:D2:00:01:55:5C` output for the UV-Pro:
    /// several RFCOMM services, SPP last, on a rotated channel.
    const UVPRO_RECORDS: &str = "\
Service Name: BS AOC
Service RecHandle: 0x10004
Service Class ID List:
  \"Headset Audio Gateway\" (0x1112)
Protocol Descriptor List:
  \"L2CAP\" (0x0100)
  \"RFCOMM\" (0x0003)
    Channel: 2

Service Name: Voice Gateway
Service RecHandle: 0x10005
Service Class ID List:
  \"Handsfree Audio Gateway\" (0x111f)
Protocol Descriptor List:
  \"L2CAP\" (0x0100)
  \"RFCOMM\" (0x0003)
    Channel: 3

Service Name: SPP Dev
Service RecHandle: 0x10006
Service Class ID List:
  \"Serial Port\" (0x1101)
Protocol Descriptor List:
  \"L2CAP\" (0x0100)
  \"RFCOMM\" (0x0003)
    Channel: 1
";

    #[test]
    fn parse_spp_channel_picks_the_spp_serial_port_not_the_audio_gateways() {
        assert_eq!(parse_spp_channel(UVPRO_RECORDS), Some(1));
    }

    #[test]
    fn parse_spp_channel_falls_back_to_any_serial_port_when_not_spp_named() {
        let records = "\
Service Name: Some Serial Thing
Service Class ID List:
  \"Serial Port\" (0x1101)
Protocol Descriptor List:
  \"RFCOMM\" (0x0003)
    Channel: 7
";
        assert_eq!(parse_spp_channel(records), Some(7));
    }

    #[test]
    fn parse_spp_channel_returns_none_when_no_serial_port_service() {
        let records = "\
Service Name: Voice Gateway
Service Class ID List:
  \"Handsfree Audio Gateway\" (0x111f)
Protocol Descriptor List:
  \"RFCOMM\" (0x0003)
    Channel: 3
";
        assert_eq!(parse_spp_channel(records), None);
    }

    #[test]
    fn rfcomm_socket_creation_succeeds_or_is_unsupported_in_ci() {
        // Verified on pandora (uid 1000): a non-root user CAN create an
        // AF_BLUETOOTH/BTPROTO_RFCOMM socket — the basis of tuxlink-nx2. On a CI host
        // with no Bluetooth stack, socket() fails with EAFNOSUPPORT/EPROTONOSUPPORT;
        // any OTHER errno means the FFI domain/type/protocol constants are wrong.
        match RfcommSocket::socket_fd() {
            Ok(fd) => {
                // SAFETY: close the fd we just created and own.
                unsafe { libc::close(fd) };
            }
            Err(e) => {
                let errno = e.raw_os_error().unwrap_or(0);
                assert!(
                    errno == libc::EAFNOSUPPORT || errno == libc::EPROTONOSUPPORT,
                    "unexpected socket() error (FFI constants wrong?): {e}"
                );
            }
        }
    }
}
