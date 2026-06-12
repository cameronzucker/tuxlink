//! Linux backend: raw `/dev/ttyUSB*` open + `TIOCMBIS` / `TIOCMBIC`
//! ioctls for modem-line toggling.
//!
//! No baud-rate, no termios framing configuration beyond the
//! defensive raw-mode pass. The only kernel-side behavior we
//! depend on is the modem-line ioctls.

#![allow(unsafe_code)]

use std::fs::OpenOptions;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use super::error::{RtsError, RtsResult};
use super::writer::{TtyOp, TtyWriter};

/// Linux `/dev/ttyUSB*` modem-line backend.
///
/// Owns the file descriptor for the lifetime of the writer. The fd
/// closes on Drop (returns the line to the kernel-side serial
/// driver's idle state), but our explicit `ReleaseRts` in
/// [`super::RtsPtt`]'s Drop runs FIRST so the line state is
/// well-defined at close time.
pub struct LinuxTty {
    fd: OwnedFd,
    path: String,
}

impl LinuxTty {
    /// Open the tty device at `path` and configure it for raw,
    /// flow-control-disabled, no-modem-line-management mode.
    ///
    /// As the very first post-configuration step, the modem-line
    /// register is cleared (RTS + DTR both low) via `TIOCMBIC` —
    /// this defuses the spurious-key-on-open failure mode that
    /// kernels historically exhibit.
    ///
    /// Opens with `O_RDWR | O_NOCTTY | O_NONBLOCK`. `O_NOCTTY`
    /// prevents the tty from becoming our controlling terminal
    /// (which would otherwise route signals like SIGHUP through
    /// it). `O_NONBLOCK` prevents a kernel-side blocking open from
    /// hanging if the device's driver is in an unexpected state.
    pub fn open(path: impl AsRef<Path>) -> RtsResult<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.display().to_string();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
            .open(path_ref)
            .map_err(|source| RtsError::OpenDevice {
                path: path_str.clone(),
                source,
            })?;
        let fd: OwnedFd = file.into();

        // Configure termios: raw mode + CLOCAL + no hardware flow
        // control. CRTSCTS is the load-bearing clear — if it's set,
        // the kernel uses RTS for hardware flow control and our
        // TIOCMBIS/TIOCMBIC calls race the kernel's RTS management.
        configure_termios_for_modem_line_only(&fd)?;

        let mut tty = Self { fd, path: path_str };
        // ALWAYS issue OpenClearBoth first. This is the single most
        // important safety invariant: opening the tty doesn't
        // assert PTT through us — every subsequent state change is
        // explicit.
        tty.modem_op(TtyOp::OpenClearBoth)?;
        Ok(tty)
    }

    /// The path this writer was opened with (for diagnostics).
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Apply the termios configuration we want for an RTS-only PTT
/// session: raw mode, CLOCAL (ignore modem-line state changes from
/// the device side), no flow control, and crucially **no CRTSCTS**
/// so the kernel-side serial driver does not hijack the RTS line
/// for hardware flow control.
fn configure_termios_for_modem_line_only(fd: &OwnedFd) -> RtsResult<()> {
    // SAFETY: `tcgetattr` reads the kernel's current termios into a
    // caller-allocated struct. We use a zeroed termios as the
    // staging buffer; the kernel overwrites every field we read
    // back. No aliasing concerns — the call is single-threaded.
    let mut tio: libc::termios = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::tcgetattr(fd.as_raw_fd(), &mut tio) };
    if r != 0 {
        return Err(RtsError::TermiosConfig(std::io::Error::last_os_error()));
    }

    // Raw mode: clear input/output processing, canonical mode,
    // echo, signal generation. cfmakeraw is the standard one-call
    // helper; it does NOT set CLOCAL or clear CRTSCTS, so we do
    // those explicitly below.
    unsafe { libc::cfmakeraw(&mut tio) };

    // CLOCAL ignores modem-line status changes from the device (we
    // only DRIVE lines, we don't READ them). CREAD is required by
    // POSIX for receive to be enabled — the kernel-side driver may
    // refuse to apply termios changes if CREAD is unset on some
    // drivers, so keep it.
    tio.c_cflag |= libc::CLOCAL | libc::CREAD;

    // The critical clear: CRTSCTS turns RTS + CTS into hardware
    // flow-control lines managed by the kernel. We need RTS as a
    // bare modem-control bit under our exclusive control.
    tio.c_cflag &= !libc::CRTSCTS;

    // SAFETY: `tcsetattr` accepts a const pointer to the termios
    // struct we built above. No aliasing or lifetime concerns.
    let r = unsafe { libc::tcsetattr(fd.as_raw_fd(), libc::TCSANOW, &tio) };
    if r != 0 {
        return Err(RtsError::TermiosConfig(std::io::Error::last_os_error()));
    }

    Ok(())
}

impl TtyWriter for LinuxTty {
    fn modem_op(&mut self, op: TtyOp) -> RtsResult<()> {
        let (request, bits) = match op {
            TtyOp::OpenClearBoth => {
                let bits: libc::c_int = libc::TIOCM_RTS | libc::TIOCM_DTR;
                (libc::TIOCMBIC, bits)
            }
            TtyOp::AssertRts => {
                let bits: libc::c_int = libc::TIOCM_RTS;
                (libc::TIOCMBIS, bits)
            }
            TtyOp::ReleaseRts => {
                let bits: libc::c_int = libc::TIOCM_RTS;
                (libc::TIOCMBIC, bits)
            }
        };
        // SAFETY: `ioctl` with `TIOCMBIS`/`TIOCMBIC` expects a
        // `const int *` argument pointing to the bit mask. We pass
        // a pointer to `bits` which lives for the duration of the
        // call (stack-allocated `c_int` above). No aliasing.
        let r = unsafe { libc::ioctl(self.fd.as_raw_fd(), request, &bits) };
        if r != 0 {
            return Err(RtsError::ModemLineIoctl(std::io::Error::last_os_error()));
        }
        Ok(())
    }
}
