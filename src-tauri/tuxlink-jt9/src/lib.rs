//! Managed-jt9 FT8 decode service (Station Intelligence L1, tuxlink-b026z.2).
//!
//! jt9 is invoked strictly as a subprocess: WAV file + argv in, stdout/stderr
//! out. The `-s`/`--shmem` mode is banned (GPL boundary — see
//! docs/design/2026-07-10-station-intel-jt9-engine-delta.md §GPL boundary).

pub mod parse;
pub mod message;
