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
        // prevents pipe-full stalls on chatty output.
        let (line_tx, line_rx) = mpsc::channel::<String>();
        let stdout = child.stdout.take().expect("stdout piped");
        let stdout_thread = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut stderr = child.stderr.take().expect("stderr piped");
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            buf
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
        let stderr_text = if timed_out {
            // A grandchild of the killed process can hold the pipe write-ends
            // open indefinitely; NEVER block on the drain threads here. Give
            // the drains a beat to flush what the kernel buffered, then
            // collect via the channel; the threads exit on their own at pipe
            // EOF whenever the holder dies.
            std::thread::sleep(Duration::from_millis(50));
            String::new()
        } else {
            let _ = stdout_thread.join();
            stderr_thread.join().unwrap_or_default()
        };

        // Collect everything the drain saw.
        let mut decodes = Vec::new();
        let mut saw_sentinel = false;
        let mut raw_lines = 0usize;
        for line in line_rx.try_iter() {
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
                    d.partial = true;
                }
                SlotOutcome::Decoded(decodes)
            };
        }
        let status = status.expect("non-timeout path has a status");

        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
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
            return SlotOutcome::Failed(SlotFailure::Signal {
                signal: format!("exit {}", status.code().unwrap_or(-1)),
                stderr_tail: tail(&stderr_text, 300),
            });
        }
        if !decodes.is_empty() {
            return SlotOutcome::Decoded(decodes);
        }
        let _ = saw_sentinel; // completeness marker only matters on the timeout path
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
        match self.decode_slot(&wav, &dir, 0) {
            SlotOutcome::Decoded(_) | SlotOutcome::BandDead => Ok(()),
            SlotOutcome::Failed(f) => Err(f),
        }
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
