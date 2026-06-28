/// MCP connection-info: read-only path resolver + Tauri command for the
/// "Connect an AI agent" feature (tuxlink-l9sq4 Task 2).
///
/// `mcp_socket_path` is a pure query function — it does NOT create or harden
/// directories (that side effect lives in the app setup in `lib.rs`). It
/// resolves where the MCP Unix-domain socket WOULD be (and IS, when the server
/// is running), using the same env+uid logic the setup path uses, so the two
/// stay consistent.
use std::path::PathBuf;
use serde::Serialize;

/// Resolve the MCP socket path: `$XDG_RUNTIME_DIR/tuxlink/mcp.sock` when the
/// runtime dir is private (0700, uid-owned, non-symlink per PR #924), else the
/// hardened private `/tmp/tuxlink-<uid>/tuxlink/mcp.sock` fallback.
///
/// This is a read-only path computation — it does NOT create or chmod any
/// directories (the app setup path in `lib.rs` is responsible for that). The
/// result is always a valid `PathBuf`; `.exists()` on the returned path
/// indicates whether the MCP server is currently bound.
#[cfg(target_os = "linux")]
pub fn mcp_socket_path() -> PathBuf {
    // SAFETY: `getuid(2)` takes no arguments and cannot fail (POSIX).
    let my_uid = unsafe { libc::getuid() };

    // The hardened temp fallback path — used both when XDG_RUNTIME_DIR is
    // unset/empty AND when it is set but fails the private-dir check.
    let temp_fallback_path = |uid: u32| -> PathBuf {
        std::env::temp_dir()
            .join(format!("tuxlink-{uid}"))
            .join("tuxlink")
            .join("mcp.sock")
    };

    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if !dir.is_empty() => {
            let base = PathBuf::from(dir);
            if crate::mcp_dir_is_safe(&base, my_uid) {
                // XDG_RUNTIME_DIR is private: socket lives under it.
                base.join("tuxlink").join("mcp.sock")
            } else {
                // Set-but-not-private: same fallback the setup uses.
                temp_fallback_path(my_uid)
            }
        }
        // XDG_RUNTIME_DIR unset/empty.
        _ => temp_fallback_path(my_uid),
    }
}

/// DTO returned by `mcp_connection_info`. Serde renames to camelCase so the
/// TypeScript side sees `socketPath` / `shimPath` / `serverRunning` — matching
/// Task 1's `McpConnectionInfo` interface.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectionInfoDto {
    pub socket_path: String,
    pub shim_path: String,
    pub server_running: bool,
}

/// Read-only Tauri command: returns the MCP socket path, the bundled shim
/// path, and whether the MCP server is currently running (socket inode
/// present → server has bound it).
#[tauri::command]
pub fn mcp_connection_info(_app: tauri::AppHandle) -> McpConnectionInfoDto {
    #[cfg(target_os = "linux")]
    let socket = mcp_socket_path();
    #[cfg(not(target_os = "linux"))]
    let socket = PathBuf::from("mcp.sock"); // non-Linux stub; app ships on Linux

    // The shim ships beside the app binary (externalBin in tauri.conf.json).
    // Resolve the current executable's directory and append the shim name.
    let shim = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tuxlink-mcp")))
        .unwrap_or_else(|| PathBuf::from("tuxlink-mcp"));

    let server_running = socket.exists(); // socket inode present ⇒ server bound it

    McpConnectionInfoDto {
        socket_path: socket.to_string_lossy().into_owned(),
        shim_path: shim.to_string_lossy().into_owned(),
        server_running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn socket_path_ends_with_tuxlink_mcp_sock() {
        let p = mcp_socket_path();
        assert!(p.ends_with("tuxlink/mcp.sock"), "got {p:?}");
    }
}
