//! Async client for the newline-delimited JSON IPC protocol defined in
//! [`crate::daemon::ipc::protocol`].
//!
//! This is the code path used by the CLI's `feed` subcommand (and future
//! subcommands) to talk to the daemon. It is invoked synchronously from a
//! shell hook (`PROMPT_COMMAND`) after every command the user runs, so it
//! has a hard, non-negotiable latency budget: it must never hang, and
//! must always return within [`CLIENT_TIMEOUT`] even if the daemon is
//! completely absent (no socket file, stale socket, wedged daemon, etc).

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::daemon::ipc::protocol::{Request, Response};

/// Hard upper bound on the total time [`send_request`] may take, covering
/// connect + write + read. Chosen to be small enough that a shell prompt
/// hook invoking this never produces a perceptible delay, even if the
/// daemon is present but slow/wedged.
pub const CLIENT_TIMEOUT: Duration = Duration::from_millis(100);

/// Sends `req` to the Unix socket at `socket_path` and returns the parsed
/// `Response`, or an error if the connection/write/read fails or times
/// out. The entire operation (connect, write, read one line, parse) is
/// wrapped in a single [`CLIENT_TIMEOUT`] deadline.
pub async fn send_request(socket_path: &Path, req: &Request) -> anyhow::Result<Response> {
    tokio::time::timeout(CLIENT_TIMEOUT, send_request_inner(socket_path, req))
        .await
        .map_err(|_| anyhow::anyhow!("IPC request timed out after {:?}", CLIENT_TIMEOUT))?
}

async fn send_request_inner(socket_path: &Path, req: &Request) -> anyhow::Result<Response> {
    let stream = UnixStream::connect(socket_path).await?;
    let mut reader = BufReader::new(stream);

    let mut json = serde_json::to_string(req)?;
    json.push('\n');
    reader.get_mut().write_all(json.as_bytes()).await?;

    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        anyhow::bail!("daemon closed connection without a response");
    }

    let response: Response = serde_json::from_str(line.trim_end())?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::ipc::protocol::RequestOp;
    use crate::daemon::ipc::server;
    use std::sync::Arc;
    use std::time::{Duration as StdDuration, Instant};

    async fn spawn_test_server(
        socket_path: std::path::PathBuf,
    ) -> (
        tokio::task::JoinHandle<anyhow::Result<()>>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(server::ServerState::default());

        let handle = tokio::spawn(async move {
            server::serve_with_ready_signal(&socket_path, state, shutdown_rx, Some(ready_tx)).await
        });

        let _ = tokio::time::timeout(StdDuration::from_secs(2), ready_rx).await;

        (handle, shutdown_tx)
    }

    #[tokio::test]
    async fn send_request_succeeds_against_real_server() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let (handle, shutdown_tx) = spawn_test_server(socket_path.clone()).await;

        let req = Request::new(RequestOp::Ping);
        let response = send_request(&socket_path, &req)
            .await
            .expect("send_request should succeed against a real server");
        assert!(response.ok);

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(StdDuration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn send_request_fails_fast_when_socket_missing() {
        let dir = tempfile::tempdir().unwrap();
        // Never bound by any server.
        let socket_path = dir.path().join("nonexistent.sock");

        let req = Request::new(RequestOp::Ping);
        let start = Instant::now();
        let result = send_request(&socket_path, &req).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        assert!(
            elapsed < StdDuration::from_millis(150),
            "send_request took too long against a missing socket: {elapsed:?}"
        );
    }
}
