//! jt9 subprocess runner. THE only place in the repository that spawns jt9.
//! Invocation: `<jt9> -8 -d 3 -p 15 -w 1 -a <data> -t <slot_tmp> <wav>`.
//! `-s`/`--shmem` is BANNED (GPL boundary; enforced by CI grep and a unit
//! test). See docs/design/2026-07-10-station-intel-jt9-engine-delta.md.

use crate::discover::Jt9Binary;
use crate::message::extract_fields;
use crate::parse::{parse_stdout_line, ParsedLine};
use crate::types::{Ft8Decode, SlotFailure, SlotOutcome};
use crate::wav::{preflight_slot_wav, WavError};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct Jt9Runner {
    binary: Jt9Binary,
    data_dir: PathBuf,
    timeout: Duration,
}

/// Kills + reaps the child if the runner unwinds mid-decode.
struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Jt9Runner {
    pub fn new(binary: Jt9Binary, data_dir: PathBuf, timeout: Duration) -> Self {
        Self { binary, data_dir, timeout }
    }

    pub fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome {
        match preflight_slot_wav(wav) {
            Ok(()) => {}
            Err(WavError::NotFound) => return SlotOutcome::Failed(SlotFailure::BadWav("not found".into())),
            Err(WavError::Permission) => return SlotOutcome::Failed(SlotFailure::BadWav("permission denied".into())),
            Err(WavError::Malformed(e)) | Err(WavError::WrongFormat(e)) => {
                return SlotOutcome::Failed(SlotFailure::BadWav(e))
            }
        }
        let child = Command::new(&self.binary.jt9_path)
            .args(["-8", "-d", "3", "-p", "15", "-w", "1"])
            .arg("-a").arg(&self.data_dir)
            .arg("-t").arg(slot_tmp)
            .arg(wav)
            .current_dir(slot_tmp)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return SlotOutcome::Failed(SlotFailure::SpawnFailed(e.to_string())),
        };

        // Drain threads: decode lines stream incrementally; draining also
        // prevents pipe-full stalls on chatty output. Both threads report
        // through channels (not join handles) so BOTH the timeout path and
        // the clean-exit path can collect with a bounded wait instead of a
        // blind join — a grandchild that inherits the pipe write-ends can
        // otherwise keep a thread parked in a blocking read indefinitely.
        let (line_tx, line_rx) = mpsc::channel::<String>();
        let stdout = child.stdout.take().expect("stdout piped");
        let _stdout_thread = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let (stderr_tx, stderr_rx) = mpsc::channel::<String>();
        let mut stderr = child.stderr.take().expect("stderr piped");
        let _stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf);
            let _ = stderr_tx.send(String::from_utf8_lossy(&buf).into_owned());
        });

        let mut guard = ChildGuard(Some(child));
        let deadline = Instant::now() + self.timeout;
        let mut timed_out = false;
        let status = loop {
            match guard.0.as_mut().expect("child present").try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        timed_out = true;
                        let mut c = guard.0.take().expect("child present");
                        let _ = c.kill();
                        let _ = c.wait();
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return SlotOutcome::Failed(SlotFailure::SpawnFailed(e.to_string())),
            }
        };
        guard.0 = None; // reaped on every path above

        // Bounded drain on EVERY path: `decode_slot` must always return. The
        // stdout/stderr threads exit once their pipe hits EOF, which happens
        // the instant the child exits in the ordinary case (the kernel
        // closes the last write-end). If jt9 — killed OR cleanly exited —
        // left behind a forked grandchild that inherited the pipe
        // write-ends, that write-end stays open and a blind join would wait
        // for the grandchild's own lifetime, wedging the caller forever.
        //
        // Accepted bound: if such a pipe-holding grandchild never exits on
        // its own, the two drain threads plus their two pipe read fds leak
        // for the remaining process lifetime (neither branch below ever
        // joins them). Accepted because (1) jt9 does not fork grandchildren
        // in practice (observed behavior, not a documented guarantee), and
        // (2) this crate is std-only — no libc, so no killpg to reap a
        // process group. Any future mitigation (process-group kill,
        // cgroup-based reaping, periodic fd/thread supervision) belongs to
        // L2's slot loop (tuxlink-b026z.3), which owns the long-running
        // process lifecycle this runner is invoked from; tracked at
        // tuxlink-b026z.8.
        let (stdout_lines, stderr_text): (Vec<String>, String) = if timed_out {
            // Give the drains a beat to flush what the kernel already
            // buffered, then collect non-blockingly via the channels; the
            // threads exit on their own at pipe EOF whenever the holder
            // dies.
            std::thread::sleep(Duration::from_millis(50));
            (line_rx.try_iter().collect(), stderr_rx.try_recv().unwrap_or_default())
        } else {
            // Clean-exit grace: bound the drain instead of blindly joining.
            // Ordinary case (no grandchild): the stdout sender drops the
            // instant the child exits, so `recv_timeout` returns
            // `Disconnected` essentially immediately and the stderr message
            // arrives immediately too — behavior identical to the old blind
            // join. Grandchild case: up to a 2s total grace to flush,
            // stopping the moment the deadline is hit.
            let grace_deadline = Instant::now() + Duration::from_secs(2);
            let mut lines = Vec::new();
            loop {
                let remaining = grace_deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match line_rx.recv_timeout(remaining) {
                    Ok(line) => lines.push(line),
                    Err(_) => break, // Disconnected (done) or Timeout (grace exhausted)
                }
            }
            let remaining = grace_deadline.saturating_duration_since(Instant::now());
            let stderr = stderr_rx.recv_timeout(remaining).unwrap_or_default();
            (lines, stderr)
        };

        // Collect everything the drain saw.
        let mut decodes = Vec::new();
        let mut saw_sentinel = false;
        let mut raw_lines = 0usize;
        for line in stdout_lines {
            match parse_stdout_line(&line) {
                ParsedLine::Decode { snr_db, dt_s, freq_hz, message } => {
                    let fields = extract_fields(&message);
                    decodes.push(Ft8Decode {
                        slot_utc_ms, snr_db, dt_s, freq_hz, message,
                        from_call: fields.from_call, to_call: fields.to_call,
                        grid: fields.grid, partial: false,
                    });
                }
                ParsedLine::DecodeFinished => saw_sentinel = true,
                ParsedLine::Other => {
                    if !line.trim().is_empty() {
                        raw_lines += 1;
                    }
                }
            }
        }

        if timed_out {
            return if decodes.is_empty() {
                SlotOutcome::Failed(SlotFailure::Timeout)
            } else {
                for d in &mut decodes {
                    // partial iff the sentinel was never seen: jt9 signals
                    // its own completeness with <DecodeFinished>, so a
                    // salvage AFTER the sentinel yields complete records.
                    d.partial = !saw_sentinel;
                }
                SlotOutcome::Decoded(decodes)
            };
        }
        let status = status.expect("non-timeout path has a status");

        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
                // Arm ordering pinned by the L2 spec (§gujnz): StderrEof
                // BEFORE salvage on ALL abnormal-termination arms — a
                // capture bug must never masquerade as decodes.
                if stderr_text.contains("EOF on input file") {
                    return SlotOutcome::Failed(SlotFailure::StderrEof);
                }
                if !decodes.is_empty() {
                    // Salvage-on-signal (tuxlink-gujnz): jt9's dominant
                    // real failure mode is decode-stream-then-SIGSEGV, and
                    // lines print only after jt9's internal CRC-14 accepts
                    // a candidate. partial mirrors the timeout arm: true
                    // iff the completeness sentinel was never seen.
                    for d in &mut decodes {
                        d.partial = !saw_sentinel;
                    }
                    return SlotOutcome::Decoded(decodes);
                }
                return SlotOutcome::Failed(SlotFailure::Signal {
                    signal: format!("signal {sig}"),
                    stderr_tail: tail(&stderr_text, 300),
                });
            }
        }
        if stderr_text.contains("EOF on input file") {
            return SlotOutcome::Failed(SlotFailure::StderrEof);
        }
        if !status.success() {
            if !decodes.is_empty() {
                // Nonzero-exit salvage: same rationale as the signal arm
                // (jt9 has no documented nonzero exits; a crash after
                // decodes is evidence the band is alive, not that the
                // parsed data is bad). The StderrEof check above already
                // ran — ordering pinned.
                for d in &mut decodes {
                    d.partial = !saw_sentinel;
                }
                return SlotOutcome::Decoded(decodes);
            }
            return SlotOutcome::Failed(SlotFailure::Signal {
                signal: format!("exit {}", status.code().unwrap_or(-1)),
                stderr_tail: tail(&stderr_text, 300),
            });
        }
        if !decodes.is_empty() {
            return SlotOutcome::Decoded(decodes);
        }
        if raw_lines == 0 {
            SlotOutcome::BandDead
        } else {
            SlotOutcome::Failed(SlotFailure::ParseError)
        }
    }

    pub fn prewarm(&self) -> Result<(), SlotFailure> {
        let dir = std::env::temp_dir().join(format!("tuxlink-jt9-prewarm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| SlotFailure::SpawnFailed(e.to_string()))?;
        let wav = dir.join("silence.wav");
        write_silence_wav(&wav).map_err(|e| SlotFailure::SpawnFailed(e.to_string()))?;
        let result = match self.decode_slot(&wav, &dir, 0) {
            SlotOutcome::Decoded(_) | SlotOutcome::BandDead => Ok(()),
            SlotOutcome::Failed(f) => Err(f),
        };
        // Best-effort cleanup on BOTH arms: the FFTW wisdom this decode
        // produces lands in self.data_dir (the -a dir), never in this
        // scratch dir, so the scratch dir is never needed again once
        // decode_slot returns, success or failure.
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

fn tail(s: &str, n: usize) -> String {
    let start = s.len().saturating_sub(n);
    // Snap to a char boundary.
    let mut i = start;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    s[i..].to_string()
}

fn write_silence_wav(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let data_len: u32 = crate::wav::SLOT_FRAMES * 2;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&crate::wav::SLOT_RATE_HZ.to_le_bytes())?;
    f.write_all(&(crate::wav::SLOT_RATE_HZ * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    f.write_all(&vec![0u8; data_len as usize])
}
