use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct PatSpawnOptions {
    pub binary: PathBuf,
    /// Destination path for the Pat config rendered at spawn time. Pre-z5f
    /// semantics: "existing file Pat reads." Post-tuxlink-756 semantics:
    /// "where PatProcess WRITES the rendered Pat config before exec."
    pub config_path: PathBuf,
    pub mbox_dir: PathBuf,
    pub http_listen_port: u16, // 0 = ephemeral
    pub pid_file: PathBuf,
    /// Optional sink for Pat stderr lines. When `Some`, the dedicated reader
    /// thread forwards EVERY stderr line into the sender (one `send` per line,
    /// newline trimmed) — including the announce line and any lines emitted
    /// BEFORE it, so Pat's full startup is visible downstream (tuxlink-22l
    /// §11.1, adrev #1: pre-announce lines were previously discarded). When
    /// `None`, the reader thread still drains stderr (so a chatty Pat cannot
    /// fill the OS pipe buffer and block — adrev #18) but discards the lines.
    /// Per tuxlink-z5f v2 P1 #6 — required so `PatBackend::stream_log` can
    /// multiplex Pat's log output to its subscribers.
    pub log_sink: Option<std::sync::mpsc::Sender<String>>,
    /// Tuxlink's config; rendered into Pat's config.json at `config_path`
    /// before exec. Per tuxlink-756 v2: post-cred-refactor, the wizard
    /// writes tuxlink config + keyring entry but NOT Pat's config. This
    /// field carries the tuxlink config so PatProcess can render Pat's
    /// config from it via `crate::pat_config::write_pat_config_atomic`.
    pub tuxlink_config: crate::config::Config,
    /// How long `spawn` waits for Pat to announce its HTTP listen port on
    /// stderr before giving up (killing the child + returning `TimedOut`).
    /// The app/bootstrap passes `Duration::from_secs(10)` (the historical
    /// value); tests pass a short value to exercise the timeout quickly.
    /// Per tuxlink-22l §11.1 (adrev #7): the deadline is now enforced via
    /// `mpsc::Receiver::recv_timeout`, which CANNOT block past the deadline
    /// even if Pat stays alive and never emits a newline — the prior loop
    /// only re-checked an `Instant` deadline BETWEEN `read_line` calls and
    /// so hung indefinitely mid-line.
    pub http_announce_timeout: std::time::Duration,
}

pub struct PatProcess {
    child: Option<Child>,
    pid_file: PathBuf,
    http_port: u16,
    /// Handle to the dedicated stderr reader thread (tuxlink-22l §11.1). The
    /// thread owns the stderr `BufReader` for the whole process lifetime and
    /// exits on EOF / read error / receiver-drop. We retain the handle rather
    /// than detaching it so the thread is not silently orphaned; `spawn` does
    /// NOT join it (it runs as long as Pat does). When the child is killed
    /// (`shutdown`/`Drop`), Pat's stderr closes → the reader sees EOF → the
    /// thread exits.
    _reader_thread: Option<JoinHandle<()>>,
}

impl PatProcess {
    /// Spawn Pat. Blocks until Pat's HTTP server has announced its listening
    /// port on stderr (Pat 1.0.0 logs "Starting HTTP service (http://...)"
    /// on stderr). Returns after the announce is observed. Caller is
    /// responsible for ensuring `config_path` exists and is valid.
    ///
    /// pat 1.0.0 does NOT echo the resolved port when `--addr 127.0.0.1:0`
    /// is given — its log line repeats the literal input. To support the
    /// caller's "0 = ephemeral" request, this function pre-binds a
    /// `TcpListener` to learn what port the OS would assign, drops the
    /// listener, then passes that fixed port to pat. There is a tiny race
    /// window where another process could grab the same port before pat
    /// binds, but in practice it is safe for tests and dev.
    pub fn spawn(opts: PatSpawnOptions) -> std::io::Result<Self> {
        std::fs::create_dir_all(&opts.mbox_dir)?;
        if let Some(parent) = opts.pid_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Render Pat's config from tuxlink's config and atomically write to
        // opts.config_path BEFORE exec. Per tuxlink-756 v2: this fills the
        // gap left by the cred-handling refactor — the wizard writes
        // tuxlink config + keyring entry but not Pat's config; without this
        // step Pat would spawn with no callsign / no locator.
        //
        // Error mapping per spec §3.4 (Codex R1 P2 #2): preserve source
        // chain via std::io::Error::new(kind, e) for non-Io variants.
        crate::pat_config::write_pat_config_atomic(&opts.tuxlink_config, &opts.config_path)
            .map_err(|e| match e {
                crate::pat_config::PatConfigError::Io(io_err) => io_err,
                e @ crate::pat_config::PatConfigError::MissingRequiredField(_) => {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
                }
                e @ crate::pat_config::PatConfigError::OfflineModeNoConfigNeeded => {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
                }
                e @ crate::pat_config::PatConfigError::RenderFailed(_) => {
                    std::io::Error::new(std::io::ErrorKind::Other, e)
                }
            })?;

        let actual_port = if opts.http_listen_port == 0 {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let p = listener.local_addr()?.port();
            drop(listener);
            p
        } else {
            opts.http_listen_port
        };
        let listen = format!("127.0.0.1:{}", actual_port);

        // pat 1.0.0 CLI: `--config` and `--mbox` are GLOBAL flags that
        // appear BEFORE the subcommand; the http subcommand uses `--addr`
        // (not `--listen`, which is pat's radio-modes selector).
        //
        // stdout is set to `Stdio::null()` (tuxlink-22l §11.1, adrev #18):
        // pat logs to stderr, and nothing in tuxlink reads its stdout. With
        // `Stdio::piped()` and no reader draining it, a chatty pat would fill
        // the OS pipe buffer and block. stderr stays piped — the dedicated
        // reader thread below drains it.
        let mut cmd = Command::new(&opts.binary);
        cmd.arg("--config").arg(&opts.config_path)
            .arg("--mbox").arg(&opts.mbox_dir)
            .arg("http")
            .arg("--addr").arg(&listen)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;

        // ONE dedicated reader thread owns the stderr BufReader for the whole
        // process lifetime (tuxlink-22l §11.1). It is the SOLE reader of
        // stderr — announce detection happens INSIDE this thread (signalled
        // via `announce_tx`), not on a second reader. For each line it:
        //   - forwards the trimmed line to `log_sink` if present (adrev #1:
        //     EVERY line, pre- AND post-announce, so nothing is discarded);
        //   - on the first line containing the listen-address needle, signals
        //     announce ONCE via `announce_tx`, then keeps reading.
        // It exits on EOF / read error / log-sink receiver-drop.
        //
        // The main thread waits on `announce_rx.recv_timeout(..)` — a REAL
        // deadline that cannot be defeated by a child that stays alive and
        // never emits a newline (adrev #7: the prior in-loop `Instant`
        // deadline could not interrupt a blocking `read_line`).
        let stderr = child.stderr.take().expect("piped stderr");
        let needle = format!("127.0.0.1:{}", actual_port);
        let (announce_tx, announce_rx) = mpsc::channel::<()>();
        let log_sink = opts.log_sink;
        let reader_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut announce_tx = Some(announce_tx);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) => break, // EOF — child closed stderr / exited.
                    Ok(_) => {
                        // Signal announce once, on the first needle match.
                        if let Some(tx) = announce_tx.as_ref() {
                            if buf.contains(&needle) {
                                // Receiver may already be gone (e.g. spawn
                                // returned on timeout just before the line
                                // arrived); ignore the send error and stop
                                // trying to announce.
                                let _ = tx.send(());
                                announce_tx = None;
                            }
                        }
                        // Forward EVERY line (pre- and post-announce) to the
                        // sink. Stop forwarding if the receiver is dropped.
                        if let Some(tx) = log_sink.as_ref() {
                            let line = buf
                                .trim_end_matches('\n')
                                .trim_end_matches('\r')
                                .to_string();
                            if tx.send(line).is_err() {
                                break; // receiver dropped — nothing to do.
                            }
                        }
                    }
                    Err(_) => break, // read error — give up on this stream.
                }
            }
        });

        // Wait for the announce, bounded by the caller's timeout. This is the
        // real-timeout fix: `recv_timeout` returns at the deadline regardless
        // of whether the reader thread is mid-`read_line`.
        match announce_rx.recv_timeout(opts.http_announce_timeout) {
            Ok(()) => { /* announced — fall through to pid-file write. */ }
            Err(RecvTimeoutError::Timeout) => {
                // Pat is (or may be) alive but never announced. Kill it and
                // reap so we don't leak a child / zombie; the reader thread
                // then sees EOF and exits on its own.
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "pat did not announce HTTP listen port within {:?}",
                        opts.http_announce_timeout
                    ),
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                // The reader thread ended (EOF / read error) before announcing
                // — Pat exited or closed stderr without ever printing the
                // listen address. Kill (no-op if already dead) and reap.
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "pat exited before announcing HTTP listen port",
                ));
            }
        }

        std::fs::write(&opts.pid_file, child.id().to_string())?;

        Ok(PatProcess {
            child: Some(child),
            pid_file: opts.pid_file,
            http_port: actual_port,
            // Retain the reader thread's handle; it runs for the process
            // lifetime and is NOT joined here (would block spawn).
            _reader_thread: Some(reader_thread),
        })
    }

    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => false,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    pub fn shutdown(&mut self, timeout: Duration) -> std::io::Result<()> {
        if let Some(mut child) = self.child.take() {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let pid = Pid::from_raw(child.id() as i32);
            let _ = kill(pid, Signal::SIGTERM);

            let deadline = Instant::now() + timeout;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if Instant::now() > deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        let _ = std::fs::remove_file(&self.pid_file);
        Ok(())
    }
}

impl Drop for PatProcess {
    fn drop(&mut self) {
        if self.child.is_some() {
            // Best-effort SIGKILL; process must not outlive the struct.
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = std::fs::remove_file(&self.pid_file);
        }
    }
}
