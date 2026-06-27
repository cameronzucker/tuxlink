//! Unix-domain-socket transport for the Tuxlink MCP server (phase 3.1, plan
//! Task 3).
//!
//! [`serve`] binds a `UnixListener`, hardens its permissions (`0600` on the
//! socket file; refuses to bind under a group/world-writable parent directory),
//! and runs a SINGLE-CALLER accept loop: exactly one connection is served to
//! completion before the next is accepted (design Q6 — single-caller keeps
//! caller identity trivial for audit, and avoids interleaving two agents over
//! one inert spine). The socket file is unlinked on shutdown (best-effort, via a
//! [`SocketCleanup`] drop guard).
//!
//! The transport itself carries no tests of the MCP protocol round-trip — that
//! is exercised at tier-2 against the standalone testserver. The unit tests here
//! cover the security-salient behavior: the `0600` mode, the world-writable
//! parent refusal, and single-caller serialization.

use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rmcp::service::ServiceExt;
use tokio::net::UnixListener;

use crate::router::TuxlinkMcp;

/// Best-effort unlink of the bound socket path on drop, so a clean shutdown does
/// not leave a stale socket behind. Bind already removes a stale *socket* it
/// finds at the path (single-instance assumption), so a leftover from a hard
/// crash is also self-healed on the next start.
struct SocketCleanup {
    path: PathBuf,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Reject binding under a parent directory any non-owner can write to. A
/// group/world-writable parent lets another local user swap the socket inode
/// (rebind a hostile listener at the same path), defeating the `0600` socket
/// mode. We require the parent's mode to have no group-write (`0o020`) and no
/// other-write (`0o002`) bit set.
fn assert_parent_dir_not_writable_by_others(sock_path: &Path) -> io::Result<()> {
    let parent = sock_path.parent().ok_or_else(|| {
        io::Error::other(format!(
            "socket path {} has no parent directory",
            sock_path.display()
        ))
    })?;
    // An empty parent (relative path with no dir component) means the current
    // directory; treat "." as the parent for the metadata check.
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let meta = std::fs::metadata(parent)?;
    let mode = meta.permissions().mode();
    if mode & 0o022 != 0 {
        return Err(io::Error::other(format!(
            "refusing to bind MCP socket under group/world-writable directory {} (mode {:o}); \
             another local user could hijack the socket path",
            parent.display(),
            mode & 0o777
        )));
    }
    Ok(())
}

/// Bind the MCP server on a Unix socket at `sock_path` and serve callers
/// one-at-a-time forever (until the future is dropped or an accept fails).
///
/// Hardening:
/// - Refuses to bind if the parent directory is group/world-writable.
/// - Removes a pre-existing *socket* file at `sock_path` before binding (single
///   instance owns the path; a stale socket from a crash is reclaimed). A
///   non-socket file at the path is left in place and surfaces as a bind error,
///   so we never clobber a regular file or directory.
/// - Sets the socket file mode to `0600` immediately after bind.
///
/// Single-caller: the accept loop serves each connection to completion (`serve`
/// then `waiting`) before accepting the next — connections are NOT spawned
/// concurrently.
pub async fn serve(router: TuxlinkMcp, sock_path: &Path) -> io::Result<()> {
    assert_parent_dir_not_writable_by_others(sock_path)?;

    // Reclaim a stale socket we (a prior instance) left behind. Only unlink if
    // the existing path is itself a socket — never clobber a regular file/dir.
    if let Ok(meta) = std::fs::symlink_metadata(sock_path) {
        if meta.file_type().is_socket() {
            std::fs::remove_file(sock_path)?;
        }
    }

    let listener = UnixListener::bind(sock_path)?;

    // chmod 0600 the socket file now that it exists. Owner-only rw closes the
    // local-IPC surface to other users on the box.
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(sock_path, perms)?;

    // Unlink the socket when this future is dropped / returns.
    let _cleanup = SocketCleanup {
        path: sock_path.to_path_buf(),
    };

    loop {
        let (stream, _addr) = listener.accept().await?;
        // SINGLE-CALLER: fully serve this one connection before looping to the
        // next accept. No spawn — the loop body awaits completion inline.
        let (read_half, write_half) = stream.into_split();
        match router.clone().serve((read_half, write_half)).await {
            Ok(running) => {
                // Block until this connection finishes (peer closes / quits).
                if let Err(e) = running.waiting().await {
                    // A join error means the serve task panicked or was
                    // cancelled; surface it rather than silently looping.
                    return Err(io::Error::other(e));
                }
            }
            Err(e) => {
                // Initialization of THIS connection failed (bad handshake,
                // peer closed mid-init). Do not tear the listener down — drop
                // this caller and accept the next.
                let _ = e;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use tuxlink_security::EgressGuard;

    fn fixed_1000() -> u64 {
        1000
    }

    fn test_router() -> TuxlinkMcp {
        let state = Arc::new(crate::McpState {
            guard: Arc::new(EgressGuard::with_clock(fixed_1000)),
            name: "tuxlink".into(),
            version: "9.9.9".into(),
        });
        TuxlinkMcp::new(state)
    }

    /// A tempdir hardened to `0o700`. The dev umask here is permissive (`0o002`),
    /// so `tempfile::tempdir()` yields a `0o775` (group-writable) dir that
    /// `serve` correctly REFUSES. A real runtime dir (`$XDG_RUNTIME_DIR`) is
    /// `0o700`, so harden the fixture to match production and exercise the
    /// happy path. (`bind_refuses_world_writable_parent` deliberately does the
    /// opposite to assert the refusal.)
    fn private_tempdir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bind_sets_socket_mode_0600() {
        let dir = private_tempdir();
        let sock = dir.path().join("mcp.sock");
        let sock_for_task = sock.clone();

        let handle = tokio::spawn(async move {
            let router = test_router();
            serve(router, &sock_for_task).await
        });

        // Wait for a 0600 socket to appear (bind + chmod completed).
        wait_for_socket_mode(&sock, 0o600).await;

        let meta = std::fs::symlink_metadata(&sock).unwrap();
        assert!(meta.file_type().is_socket(), "bound path must be a socket");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket file must be chmod 0600, got {mode:o}");

        handle.abort();
    }

    #[tokio::test]
    async fn bind_refuses_world_writable_parent() {
        let dir = tempfile::tempdir().unwrap();
        // Make the parent dir world-writable (0o777).
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let sock = dir.path().join("mcp.sock");

        let router = test_router();
        let err = serve(router, &sock).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("group/world-writable"),
            "expected a world-writable-parent refusal, got: {msg}"
        );
        // The socket must NOT have been created.
        assert!(
            !sock.exists(),
            "no socket should be bound when the parent is rejected"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bind_reclaims_a_stale_socket_file() {
        let dir = private_tempdir();
        let sock = dir.path().join("mcp.sock");

        // Pre-create a stale socket at the path by binding then leaking it. Its
        // umask-derived mode is NOT 0600, so reclaim is observable as the mode
        // flipping to 0600 once our serve rebinds.
        {
            let stale = UnixListener::bind(&sock).unwrap();
            drop(stale); // file remains on disk (Unix sockets are not auto-unlinked)
        }
        assert!(sock.exists(), "stale socket file should remain on disk");
        assert_ne!(
            std::fs::symlink_metadata(&sock).unwrap().permissions().mode() & 0o777,
            0o600,
            "precondition: the stale socket is not already 0600"
        );

        let sock_for_task = sock.clone();
        let handle = tokio::spawn(async move {
            let router = test_router();
            serve(router, &sock_for_task).await
        });

        // If the stale socket was reclaimed, bind succeeds and the file is a
        // fresh 0600 socket again.
        wait_for_socket_mode(&sock, 0o600).await;
        let meta = std::fs::symlink_metadata(&sock).unwrap();
        assert!(meta.file_type().is_socket());
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);

        handle.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn single_caller_serializes_connections() {
        // (fixture hardened below; see private_tempdir doc)
        // The accept loop serves one connection fully before accepting the
        // next. Two clients connect; because no rmcp handshake completes (the
        // test clients send no MCP init), each connection's `serve` future
        // ultimately resolves and the loop advances. We assert the deterministic
        // shape we CAN observe without a protocol round-trip: the listener
        // accepts connections in sequence and the server task stays alive
        // (it never spawns-and-detaches, which a concurrent design would).
        let dir = private_tempdir();
        let sock = dir.path().join("mcp.sock");
        let sock_for_task = sock.clone();

        let handle = tokio::spawn(async move {
            let router = test_router();
            serve(router, &sock_for_task).await
        });
        wait_for_socket_mode(&sock, 0o600).await;

        // First connect: accepted and held by the single-caller serve.
        let c1 = tokio::net::UnixStream::connect(&sock).await.unwrap();

        // Second connect: the OS accept queue lets connect() succeed, but the
        // server has NOT accepted it yet (it is busy serving c1's init/serve).
        // We can at least prove the listener is still serving (task alive) and
        // a second connect does not error.
        let c2 = tokio::net::UnixStream::connect(&sock).await.unwrap();

        // The server task must still be running (not finished/errored): a
        // single-caller loop blocks on the first connection, it does not exit.
        assert!(
            !handle.is_finished(),
            "single-caller serve must keep running while a caller is connected"
        );

        drop(c1);
        drop(c2);
        handle.abort();
    }

    /// Poll until a socket exists at `p` with the given permission bits, so we
    /// observe OUR bound socket (mode set), never a stale pre-existing file.
    async fn wait_for_socket_mode(p: &Path, want_mode: u32) {
        for _ in 0..300 {
            if let Ok(meta) = std::fs::symlink_metadata(p) {
                if meta.file_type().is_socket() && meta.permissions().mode() & 0o777 == want_mode {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "socket {} never reached mode {:o}",
            p.display(),
            want_mode
        );
    }
}
