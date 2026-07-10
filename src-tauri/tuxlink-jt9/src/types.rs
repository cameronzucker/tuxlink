//! Decode-service data types (Station Intelligence L1).

/// One decoded FT8 message. `slot_utc_ms` is stamped by the HOST slot
/// scheduler — jt9's stdout timestamp is always `000000` for our filenames
/// and is never used (delta §Grounded facts).
#[derive(Debug, Clone, PartialEq)]
pub struct Ft8Decode {
    pub slot_utc_ms: u64,
    pub snr_db: i32,
    pub dt_s: f64,
    pub freq_hz: u32,
    pub message: String,
    /// None when the sender is an unresolved hashed callsign (`<...>`) —
    /// per-slot jt9 spawn cannot resolve cross-slot hashes (accepted
    /// regression, delta §Revised L1). Such decodes are excluded from
    /// ft8_who_can_i_hear downstream.
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
    /// True when this record was salvaged from a timed-out run's partial
    /// stdout (no `<DecodeFinished>` sentinel seen).
    pub partial: bool,
}

/// Per-slot failure classification (delta §failure taxonomy). Feeds the
/// jt9-degraded health flag upstream: N consecutive non-`Decoded`/`BandDead`
/// outcomes degrade; the first good slot clears.
/// Degraded-flag thresholds (consumed by the L2 plan's slot scheduler; the
/// delta requires them pinned here): jt9-degraded after N = 5 consecutive
/// non-Decoded/non-BandDead outcomes, clearing on the first good slot;
/// band-dead after k = 20 consecutive zero-decode slots (5 minutes).
#[derive(Debug, Clone, PartialEq)]
pub enum SlotFailure {
    /// Preflight rejection — never spawned. STABLE-STRING CONTRACT: the exact
    /// strings "not found" and "permission denied" are API — L2's mid-run
    /// disappearance detection (consecutive not-found → degraded) matches on
    /// them. Other WAV defects carry free-text diagnostics.
    BadWav(String),
    /// jt9 died by signal (its common failure mode: Fortran error + SIGSEGV).
    Signal { signal: String, stderr_tail: String },
    /// Killed at the deadline with zero decode lines salvaged.
    Timeout,
    /// jt9's `EOF on input file` on stderr: a capture bug, NOT a quiet band.
    StderrEof,
    /// Exited zero, produced output, but not a single line parsed.
    ParseError,
    /// The OS could not spawn the process at all.
    SpawnFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotOutcome {
    Decoded(Vec<Ft8Decode>),
    /// Clean exit, zero decodes: a quiet band — explicitly NOT a failure.
    BandDead,
    Failed(SlotFailure),
}
