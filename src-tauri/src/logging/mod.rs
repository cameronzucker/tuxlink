//! Diagnostic logging — alpha-logging spec §2.
//!
//! Wiring is exposed via `init(app) -> LoggingHandle` (Task 6.x) and the
//! Tauri command handlers in `commands` (Task 6.x). The Subscriber composition
//! lives in `subscriber`; the Fanout Layer + redacting Visit live in `fanout`
//! + `visit`; redaction policy in `redact` + `wire_sanitize`.

pub mod event;
pub mod fanout;
pub mod filter_layer;
pub mod redact;
pub mod subscriber;
pub mod visit;
pub mod wire_sanitize;

pub use fanout::AttemptIdExt;
