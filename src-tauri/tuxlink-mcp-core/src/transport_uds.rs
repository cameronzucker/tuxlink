//! Unix-domain-socket transport for the Tuxlink MCP server (phase 3.1, plan
//! Task 3).
//!
//! [`serve`] binds a `UnixListener`, hardens its permissions (`0600` on the
//! socket file; refuses to bind under a group/world-writable parent directory),
//! and runs a SINGLE-ACTIVE-SESSION accept loop: exactly one connection is
//! served at a time (design Q6 — single-caller keeps caller identity trivial for
//! audit, and avoids interleaving two agents over one inert spine). Unlike a
//! serve-to-completion loop, the loop keeps `accept()`-ing while a session is
//! active and immediately CLOSES any extra caller (rather than letting it sit in
//! the kernel backlog and be served later under stale context). The socket file
//! is unlinked on shutdown (best-effort, via a [`SocketCleanup`] drop guard).
//!
//! Hardening highlights:
//! - Refuses to bind if another LIVE server already owns the path (probes via a
//!   `connect()` before any unlink — never hijacks a live rendezvous).
//! - Closes the bind→chmod permission window by setting umask `0o077` across the
//!   bind so the socket inode is created already owner-only.
//! - Drops a connection whose MCP handshake stalls (bounded serve-establishment
//!   timeout) so a wedged client cannot hold the single slot forever; an
//!   established, healthy session is NEVER time-capped.
//!
//! The transport itself carries no tests of the MCP protocol round-trip — that
//! is exercised at tier-2 against the standalone testserver. The unit tests here
//! cover the security-salient behavior: the `0600` mode, the world-writable
//! parent refusal, live-socket refusal, and single-active-session rejection.

use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rmcp::service::ServiceExt;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;

use crate::router::TuxlinkMcp;

/// How long a freshly-accepted connection has to complete the MCP handshake
/// (`serve` establishment) before it is dropped to free the single session slot.
/// This bounds ONLY handshake establishment; an established, healthy session
/// (the `waiting()` phase) is never time-capped (FIX 5).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait on a liveness probe (`connect()`) of an existing socket
/// inode before treating it as dead/stale (FIX 2).
const LIVENESS_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

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

/// Probe whether a LIVE server already owns `sock_path` (FIX 2). Attempts a
/// short-timeout `connect()`:
/// - `Ok(_)` — a live peer accepted the connection → caller MUST refuse to bind
///   (returns `true`).
/// - connect error (ECONNREFUSED: socket inode exists but no listener;
///   ENOENT: gone) or timeout → treat as dead/stale (returns `false`).
///
/// We drop the probe stream immediately; we only care that the handshake
/// at the socket layer succeeded.
async fn socket_is_live(sock_path: &Path) -> bool {
    match tokio::time::timeout(LIVENESS_PROBE_TIMEOUT, UnixStream::connect(sock_path)).await {
        Ok(Ok(_stream)) => true,
        // connect refused / path gone / any connect error → not live.
        Ok(Err(_)) => false,
        // timed out establishing the connection → treat as not live (a healthy
        // listener accepts a local UDS connection effectively instantly).
        Err(_elapsed) => false,
    }
}

/// Bind the MCP server on a Unix socket at `sock_path` and serve callers with a
/// SINGLE active session, rejecting extras, forever (until the future is dropped
/// or an accept fails).
///
/// Hardening:
/// - Refuses to bind if the parent directory is group/world-writable.
/// - Refuses to bind if another LIVE server already owns the path (FIX 2): a
///   liveness `connect()` probe precedes any unlink, so we never hijack a live
///   rendezvous. Only a dead/stale *socket* inode is unlinked and rebound; a
///   non-socket file at the path is left in place and surfaces as a bind error,
///   so we never clobber a regular file or directory.
/// - Sets umask `0o077` across the bind (FIX 3) so the socket inode is created
///   already owner-only, closing the window between bind and the explicit chmod
///   `0600` (kept as belt-and-suspenders).
/// - Single active session (FIX 4): the loop always `accept()`s; a second caller
///   while a session is active is immediately closed (clean disconnect) rather
///   than queued in the kernel backlog and served later under stale context.
/// - Bounded handshake (FIX 5): a freshly-accepted connection has
///   [`HANDSHAKE_TIMEOUT`] to complete the MCP handshake; a stalled handshake is
///   dropped so it cannot wedge the single slot. An ESTABLISHED, healthy session
///   (`waiting()`) is NEVER time-capped.
pub async fn serve(router: TuxlinkMcp, sock_path: &Path) -> io::Result<()> {
    assert_parent_dir_not_writable_by_others(sock_path)?;

    // FIX 2: before touching an existing inode, refuse if a LIVE server owns it.
    // Probe liveness via connect(); a successful connect means another instance
    // is already serving here — do NOT unlink (that would hijack the rendezvous).
    if socket_is_live(sock_path).await {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!(
                "another live MCP server already owns the socket {}; refusing to bind",
                sock_path.display()
            ),
        ));
    }

    // The probe said dead/stale. Re-verify the inode is STILL a socket (it may
    // have changed under us between probe and now) and only then unlink. Never
    // clobber a regular file/dir; a non-socket at the path surfaces as a bind
    // error below.
    if let Ok(meta) = std::fs::symlink_metadata(sock_path) {
        if meta.file_type().is_socket() {
            std::fs::remove_file(sock_path)?;
        }
    }

    // FIX 3: close the bind→chmod permission window. Between UnixListener::bind
    // and the explicit chmod 0600 the socket inode exists with umask-default
    // perms. Force umask 0o077 immediately before bind so the inode is created
    // already owner-only, and restore the previous umask immediately after.
    // umask() is process-global, so the window is kept as small as possible.
    //
    // SAFETY: `umask(2)` takes a mode_t and returns the previous mask; it has
    // no failure mode and no memory-safety preconditions. We restore the saved
    // value right after bind.
    let prev_umask = unsafe { libc::umask(0o077) };
    let bind_result = UnixListener::bind(sock_path);
    // SAFETY: same as above — restore the previously-installed umask. Done
    // unconditionally (even on bind error) so we never leak the tightened mask.
    unsafe {
        libc::umask(prev_umask);
    }
    let listener = bind_result?;

    // chmod 0600 the socket file now that it exists. Owner-only rw closes the
    // local-IPC surface to other users on the box. Belt-and-suspenders on top of
    // the umask above.
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(sock_path, perms)?;

    // Unlink the socket when this future is dropped / returns.
    let _cleanup = SocketCleanup {
        path: sock_path.to_path_buf(),
    };

    // FIX 4: single active session. One permit; a caller that cannot acquire it
    // (a session is in flight) is rejected immediately. The permit is released
    // when the spawned session task completes.
    let session_slot = Arc::new(Semaphore::new(1));

    loop {
        let (stream, _addr) = listener.accept().await?;

        // Try to claim the single session slot WITHOUT blocking. If a session
        // is already active, reject this extra caller right away (clean
        // disconnect) instead of letting it queue and be served under stale
        // context later.
        let permit = match session_slot.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_busy) => {
                // Busy: give the second caller a clean shutdown + close. Best
                // effort — the peer just sees a closed connection.
                let mut extra = stream;
                let _ = extra.shutdown().await;
                drop(extra);
                continue;
            }
        };

        // Slot acquired: serve this one connection on its own task so the accept
        // loop keeps draining the backlog (and rejecting extras) concurrently.
        // The owned permit moves into the task and is dropped (released) when the
        // session ends. router is Clone, so the task is Send.
        let router = router.clone();
        tokio::spawn(async move {
            // The permit is held for the lifetime of this task; drop releases it.
            let _permit = permit;
            let (read_half, write_half) = stream.into_split();

            // FIX 5: bound ONLY the handshake/serve-establishment. rmcp's
            // serve() resolves once the session is established (returning the
            // RunningService); waiting() is the long-lived part. A stalled
            // handshake is dropped here; an established session's waiting() is
            // left UNBOUNDED so a legitimate long-lived agent is never killed.
            let established =
                tokio::time::timeout(HANDSHAKE_TIMEOUT, router.serve((read_half, write_half)))
                    .await;

            match established {
                Ok(Ok(running)) => {
                    // Handshake done. Block until the session finishes (peer
                    // closes / quits) — NO timeout on this phase.
                    let _ = running.waiting().await;
                }
                Ok(Err(_init_err)) => {
                    // Handshake failed (bad init, peer closed mid-init). Drop
                    // this caller; the slot is freed when `_permit` drops.
                }
                Err(_elapsed) => {
                    // Handshake stalled past HANDSHAKE_TIMEOUT. Drop the wedged
                    // caller so it cannot hold the single slot. The stream is
                    // dropped with the timed-out future; the slot is freed.
                }
            }
        });
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
        // Route through the shared test mock-port builder so all McpState fields
        // (incl. the phase-3.2 ports) are populated; the guard uses the fixed
        // clock so any armed-grant assertions stay deterministic.
        let state = Arc::new(crate::test_support::state_with_guard(
            EgressGuard::with_clock(fixed_1000),
        ));
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
            std::fs::symlink_metadata(&sock)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
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
    async fn single_caller_rejects_extra_while_one_active() {
        // (fixture hardened below; see private_tempdir doc)
        // FIX 4: exactly one active session; a second caller is rejected
        // (closed) rather than queued. Both clients send NO MCP init, so the
        // first connection occupies the single slot for the full handshake
        // window (it never completes the handshake). We assert the observable
        // shape: the second connect's read returns EOF (the server closed it)
        // promptly, while the server task stays alive.
        let dir = private_tempdir();
        let sock = dir.path().join("mcp.sock");
        let sock_for_task = sock.clone();

        let handle = tokio::spawn(async move {
            let router = test_router();
            serve(router, &sock_for_task).await
        });
        wait_for_socket_mode(&sock, 0o600).await;

        // First connect: claims the single session slot (held through the
        // handshake window since this client sends no MCP init).
        let _c1 = tokio::net::UnixStream::connect(&sock).await.unwrap();

        // Give the server a beat to accept c1 and claim the permit.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Second connect: the server accepts it, fails to acquire the slot, and
        // closes it. A read on the rejected stream should observe EOF (0 bytes)
        // promptly — proving the extra caller was dropped, not queued+served.
        let mut c2 = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(5), {
            use tokio::io::AsyncReadExt as _;
            c2.read(&mut buf)
        })
        .await
        .expect("rejected caller should be closed within timeout");
        assert_eq!(
            read.unwrap(),
            0,
            "the second caller must see EOF (server closed it), not queued service"
        );

        // The server task must still be running (a rejected extra does not tear
        // the listener down).
        assert!(
            !handle.is_finished(),
            "single-active-session serve must keep running after rejecting an extra"
        );

        handle.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bind_refuses_live_socket() {
        // FIX 2: if another LIVE server already owns the path, a second serve()
        // must REFUSE to bind (not unlink + hijack the live rendezvous).
        let dir = private_tempdir();
        let sock = dir.path().join("mcp.sock");
        let sock_for_task = sock.clone();

        // Start the first (live) server and wait for its bound 0600 socket.
        let live = tokio::spawn(async move {
            let router = test_router();
            serve(router, &sock_for_task).await
        });
        wait_for_socket_mode(&sock, 0o600).await;

        // A second serve() on the same live path must error (AddrInUse), and
        // must NOT have unlinked the live socket.
        let router2 = test_router();
        let err = serve(router2, &sock).await.unwrap_err();
        assert_eq!(
            err.kind(),
            io::ErrorKind::AddrInUse,
            "second serve over a live socket must refuse with AddrInUse, got: {err}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("another live MCP server"),
            "expected a live-socket refusal message, got: {msg}"
        );
        // The live server's socket must still be present and a socket (not
        // clobbered by the refused second bind).
        let meta = std::fs::symlink_metadata(&sock).unwrap();
        assert!(
            meta.file_type().is_socket(),
            "the live socket must survive a refused second bind"
        );

        live.abort();
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
        panic!("socket {} never reached mode {:o}", p.display(), want_mode);
    }
}
