//! Soundcard modem integration.
//!
//! This module hosts the managed-spawn / external-TCP-modem client layer
//! (ADR 0015 decisions #1 and #2). Each supported modem is a submodule that
//! implements a `ModemTransport`-like interface driving the modem's TCP host
//! protocol while tuxlink owns the modem process lifecycle (spawn / supervise /
//! SIGINT-clean-stop / audio-device-release gate before swap).
//!
//! The concurrency model is synchronous `std::io` + `std::thread` — no Tokio
//! anywhere in this subtree (see plan concurrency-architecture note and
//! ADR 0015). Phase 1 (wire codec) is pure functions/structs; threads and
//! TCP connections arrive in Phase 2.

pub mod ardop;
