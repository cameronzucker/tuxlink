//! tuxlink-mcp — transparent stdio↔UDS byte-pump for `claude mcp`.
//!
//! `claude mcp` launches an MCP server as a child process and speaks framed
//! JSON-RPC over the child's stdin/stdout. The tuxlink MCP endpoint, however, is
//! a Unix-domain socket inside the running Tauri app (see the transport-spine
//! plan, Task 3). This shim bridges the two: it connects a `UnixStream` to that
//! socket and pumps bytes in both directions — stdin → socket and socket → stdout
//! — until either side closes. It is intentionally dumb: it never parses or
//! interprets the MCP payload, so it carries no rmcp / serde dependency.
//!
//! Termination: a clean EOF on either direction is normal shutdown (the client
//! closed stdin, or the server closed the socket); the shim flushes stdout and
//! exits 0. A missing / unconnectable socket is a real error → exit non-zero with
//! a clear message.

use std::io;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;

const SOCK_ENV: &str = "TUXLINK_MCP_SOCK";

#[tokio::main]
async fn main() {
    let sock_path = match resolve_sock_path(std::env::args().nth(1), std::env::var(SOCK_ENV).ok()) {
        Some(p) => p,
        None => {
            eprintln!(
                "tuxlink-mcp: no MCP socket path provided.\n\
                 Usage: tuxlink-mcp <socket-path>\n\
                 Or set the {SOCK_ENV} environment variable to the socket path."
            );
            std::process::exit(2);
        }
    };

    let stream = match UnixStream::connect(&sock_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "tuxlink-mcp: failed to connect to MCP socket {sock_path:?}: {e}\n\
                 Is the Tuxlink app running and the socket path correct?"
            );
            std::process::exit(1);
        }
    };

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    match pump(stream, stdin, stdout).await {
        Ok(()) => {}
        Err(e) => {
            eprintln!("tuxlink-mcp: pump error: {e}");
            std::process::exit(1);
        }
    }
}

/// Resolve the socket path: argv[1] takes precedence, then the env var.
/// Returns `None` when neither is present (caller emits usage + exits non-zero).
fn resolve_sock_path(arg: Option<String>, env: Option<String>) -> Option<String> {
    match arg {
        Some(p) if !p.is_empty() => Some(p),
        _ => match env {
            Some(p) if !p.is_empty() => Some(p),
            _ => None,
        },
    }
}

/// Bidirectional transparent pump between a connected socket and a client's
/// in/out streams.
///
/// Generic over the socket (any `AsyncRead + AsyncWrite`, e.g. `UnixStream`) and
/// over the client `reader`/`writer` so tests can substitute in-memory IO for the
/// process's real stdin/stdout. Splits the socket into read/write halves and runs
/// both copy directions concurrently.
///
/// Shutdown semantics mirror a transparent pipe over a duplex session:
/// - When stdin hits EOF (client→socket done), the socket's write half is shut
///   down to signal EOF to the server, but the socket→stdout direction keeps
///   draining so any in-flight reply still reaches the client.
/// - The pump returns once the socket→stdout direction completes (the server
///   closed the socket). The client→socket direction is allowed to keep running
///   until then (e.g. a long-lived client that never closes stdin); whichever of
///   the two terminal conditions — server closes the socket, or both directions
///   finish — is reached first ends the pump.
///
/// stdout is flushed before return.
async fn pump<S, R, W>(socket: S, mut reader: R, mut writer: W) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (mut sock_read, mut sock_write) = tokio::io::split(socket);

    let client_to_sock = async {
        tokio::io::copy(&mut reader, &mut sock_write).await?;
        // stdin closed: signal EOF to the server so it can finish cleanly, then
        // stop driving this direction (the reply drain below owns termination).
        tokio::io::AsyncWriteExt::shutdown(&mut sock_write).await
    };

    let sock_to_client = async {
        tokio::io::copy(&mut sock_read, &mut writer).await?;
        tokio::io::AsyncWriteExt::flush(&mut writer).await
    };

    tokio::pin!(client_to_sock);
    tokio::pin!(sock_to_client);

    let mut client_to_sock_done = false;
    loop {
        tokio::select! {
            // Server closed the socket (or a reply-write error): the session is
            // over — exit immediately, dropping the still-pending stdin copy.
            r = &mut sock_to_client => {
                r?;
                break;
            }
            // stdin closed: EOF has been signalled to the server. Keep draining
            // the reply direction; do not re-poll this branch (it would busy-loop
            // on the now-ready future), so disable it once complete.
            r = &mut client_to_sock, if !client_to_sock_done => {
                r?;
                client_to_sock_done = true;
            }
        }
    }

    // The terminal `sock_to_client` branch already flushed `writer` on completion
    // (see its `copy(..).await?; flush(..)`), so the reply is durable on stdout
    // before we return. No extra flush needed here (and it would conflict with
    // the still-live `sock_to_client` borrow of `writer`).
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    fn unique_sock_path() -> std::path::PathBuf {
        // Structural uniqueness via a per-process counter — the previous
        // clock-only nonce collided on a coarse-clock arm64 CI runner (two
        // tests read the same timestamp; the first test's socket FILE
        // lingers after close, so the second bind failed AddrInUse; run
        // 31995019336). The counter makes intra-process paths unique by
        // construction; the timestamp stays for cross-run readability.
        static SOCK_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        let nonce = format!(
            "tuxlink-mcp-test-{}-{}-{}.sock",
            process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            SOCK_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        p.push(nonce);
        // Belt-and-suspenders for a recycled pid meeting a leftover file
        // from a crashed prior run: a UDS bind fails AddrInUse on an
        // existing path even when nothing listens.
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn resolve_prefers_arg_then_env() {
        assert_eq!(
            resolve_sock_path(Some("/a".into()), Some("/b".into())),
            Some("/a".to_string())
        );
        assert_eq!(
            resolve_sock_path(None, Some("/b".into())),
            Some("/b".to_string())
        );
        assert_eq!(resolve_sock_path(None, None), None);
        // Empty strings are treated as absent so an empty arg falls through to env.
        assert_eq!(
            resolve_sock_path(Some(String::new()), Some("/b".into())),
            Some("/b".to_string())
        );
        assert_eq!(resolve_sock_path(Some(String::new()), None), None);
    }

    /// Round-trip through a real loopback `UnixListener` echo server: bytes the
    /// shim reads from the "stdin" buffer must arrive at the socket, and the
    /// socket's reply must arrive at the "stdout" buffer. Exercises the exact
    /// `pump` logic `main` runs, with in-memory IO standing in for stdio.
    #[tokio::test]
    async fn pump_round_trips_through_socket_echo() {
        let sock_path = unique_sock_path();
        let listener = UnixListener::bind(&sock_path).expect("bind listener");

        // Echo server: accept one connection, read until EOF, echo each chunk back.
        let server = tokio::spawn(async move {
            let (mut conn, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 1024];
            loop {
                let n = conn.read(&mut buf).await.expect("server read");
                if n == 0 {
                    break; // client (the shim) closed its write half
                }
                conn.write_all(&buf[..n]).await.expect("server write");
                conn.flush().await.expect("server flush");
            }
            // Drop conn → server closes the socket, which ends the shim's
            // socket→stdout direction.
        });

        let client = UnixStream::connect(&sock_path)
            .await
            .expect("client connect");

        // "stdin" = a fixed request the shim should forward to the socket.
        let request = b"hello mcp over uds\n".to_vec();
        let stdin = std::io::Cursor::new(request.clone());
        // "stdout" = a growable buffer capturing what the socket sends back.
        let mut stdout: Vec<u8> = Vec::new();

        pump(client, stdin, &mut stdout).await.expect("pump ok");

        server.await.expect("server task");
        let _ = std::fs::remove_file(&sock_path);

        assert_eq!(
            stdout, request,
            "echoed bytes from the socket must reach the shim's stdout verbatim"
        );
    }

    /// A clean EOF on stdin with no socket reply is normal termination, not an
    /// error — the shim must return `Ok(())`.
    #[tokio::test]
    async fn pump_clean_eof_is_ok() {
        let sock_path = unique_sock_path();
        let listener = UnixListener::bind(&sock_path).expect("bind listener");

        let server = tokio::spawn(async move {
            let (mut conn, _) = listener.accept().await.expect("accept");
            // Drain whatever the client sends, then close without replying.
            let mut buf = [0u8; 1024];
            loop {
                let n = conn.read(&mut buf).await.expect("server read");
                if n == 0 {
                    break;
                }
            }
        });

        let client = UnixStream::connect(&sock_path)
            .await
            .expect("client connect");
        let stdin = std::io::Cursor::new(Vec::new()); // immediate EOF
        let mut stdout: Vec<u8> = Vec::new();

        pump(client, stdin, &mut stdout).await.expect("clean eof ok");

        server.await.expect("server task");
        let _ = std::fs::remove_file(&sock_path);
        assert!(stdout.is_empty());
    }
}
