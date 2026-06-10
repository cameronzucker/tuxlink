use tauri::{AppHandle, Emitter};

use crate::session_log::SessionLogState;
use crate::winlink_backend::{LogLevel, LogSource};

/// Append an explicit operator-visible session-log line, then notify any live
/// UI listeners with the same raw/stored line. Export/share paths redact at
/// their own boundary.
pub fn emit(
    app: &AppHandle,
    buffer: &SessionLogState,
    level: LogLevel,
    source: LogSource,
    message: impl AsRef<str>,
) {
    let line = buffer.append_operator_line(level, source, message);
    let _ = app.emit(
        "session_log:line",
        crate::ui_commands::LogLineDto::from(line),
    );
}
