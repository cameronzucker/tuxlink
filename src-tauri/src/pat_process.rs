use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct PatSpawnOptions {
    pub binary: PathBuf,
    pub config_path: PathBuf,
    pub mbox_dir: PathBuf,
    pub http_listen_port: u16, // 0 = ephemeral
    pub pid_file: PathBuf,
}

pub struct PatProcess {
    child: Option<Child>,
    pid_file: PathBuf,
    http_port: u16,
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
        let mut cmd = Command::new(&opts.binary);
        cmd.arg("--config").arg(&opts.config_path)
            .arg("--mbox").arg(&opts.mbox_dir)
            .arg("http")
            .arg("--addr").arg(&listen)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;

        // Read stderr (where pat logs) until we see our listen address
        // echoed in the "Starting HTTP service ..." line.
        let stderr = child.stderr.take().expect("piped stderr");
        let reader = BufReader::new(stderr);
        let needle = format!("127.0.0.1:{}", actual_port);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut announced = false;
        for line in reader.lines() {
            if Instant::now() > deadline {
                break;
            }
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.contains(&needle) {
                announced = true;
                break;
            }
        }
        if !announced {
            let _ = child.kill();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "pat did not announce HTTP listen port within 10s",
            ));
        }

        std::fs::write(&opts.pid_file, child.id().to_string())?;

        Ok(PatProcess {
            child: Some(child),
            pid_file: opts.pid_file,
            http_port: actual_port,
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
