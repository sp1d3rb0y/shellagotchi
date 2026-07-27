//! Tokio Unix domain socket server for the newline-delimited JSON IPC
//! protocol defined in [`crate::daemon::ipc::protocol`].
//!
//! This module is split into two layers on purpose:
//!
//! - [`handle_request`] is a synchronous, pure-ish dispatch function that
//!   takes a [`Request`] and shared [`ServerState`] and returns a
//!   [`Response`]. It has no knowledge of sockets, framing, or async I/O,
//!   so it's directly unit-testable.
//! - [`serve`] is the async transport wrapper: it binds the socket,
//!   accepts connections, frames/parses/writes newline-delimited JSON,
//!   and calls [`handle_request`] for each parsed line.
//!
//! [`handle_request`] is wired to the real pet engine: it mutates a
//! shared [`ServerState::pet`] via `crate::pet::engine`'s `catch_up`,
//! `feed`, `clean`, and `pet_interaction`, persisting the pet to disk
//! after each mutating request.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::DateTime;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::daemon::ipc::protocol::{Request, RequestOp, Response};
use crate::pet::engine::{self, FeedEvent};
use crate::pet::state::PetState;

/// Maximum length (in bytes) of a single newline-delimited line the
/// server will accept. Guards against a misbehaving/malicious client
/// sending an unbounded line and exhausting memory.
const MAX_LINE_BYTES: usize = 4096;

/// How long a connection may sit idle (no complete line received) before
/// the server drops it.
const IDLE_TIMEOUT: Duration = Duration::from_secs(2);

/// Shared, thread-safe state the server dispatches requests against.
///
/// Holds the live, in-memory [`PetState`] the daemon owns, the resolved
/// [`Config`], and the path the pet should be persisted to after each
/// mutating request.
#[derive(Debug)]
pub struct ServerState {
    pub pet: Mutex<PetState>,
    pub config: Config,
    pub state_path: PathBuf,
}

/// Saves `pet` to `state.state_path`, logging (but not propagating) any
/// failure -- the in-memory state is still correct even if the
/// save-to-disk failed, so a persistence error must never fail the
/// request itself.
fn persist(state: &ServerState, pet: &PetState) {
    if let Err(err) = crate::daemon::persist::save(pet, &state.state_path) {
        tracing::warn!(
            "failed to persist pet state to {:?}: {err}",
            state.state_path
        );
    }
}

/// Handles ONE already-parsed [`Request`], returning the [`Response`] to
/// send back. Mutating ops (`feed`/`clean`/`pet`/`status`) first call
/// [`engine::catch_up`] to bring the pet up to date with elapsed time,
/// then apply their own effect, then persist the result to disk.
pub fn handle_request(state: &ServerState, req: &Request) -> Response {
    let now = SystemClock.now();

    match &req.op {
        RequestOp::Ping => Response::ok_empty(),
        RequestOp::Status => {
            let mut pet = state.pet.lock().unwrap();
            let mut rng = rand::rng();
            engine::catch_up(&mut pet, now, &state.config, &mut rng);
            persist(state, &pet);
            Response::ok_state(pet.clone())
        }
        RequestOp::Feed {
            exit_code,
            duration_ms: _,
            argv0,
            ts,
        } => {
            let mut pet = state.pet.lock().unwrap();
            let mut rng = rand::rng();
            engine::catch_up(&mut pet, now, &state.config, &mut rng);

            let feed_now = DateTime::from_timestamp(*ts, 0).unwrap_or(now);
            engine::feed(
                &mut pet,
                FeedEvent {
                    exit_code: *exit_code,
                    argv0,
                    now: feed_now,
                },
                &state.config,
                &mut rng,
            );
            persist(state, &pet);
            Response::ok_empty()
        }
        RequestOp::Clean => {
            let mut pet = state.pet.lock().unwrap();
            let mut rng = rand::rng();
            engine::catch_up(&mut pet, now, &state.config, &mut rng);
            engine::clean(&mut pet);
            persist(state, &pet);
            Response::ok_empty()
        }
        RequestOp::Pet => {
            let mut pet = state.pet.lock().unwrap();
            let mut rng = rand::rng();
            engine::catch_up(&mut pet, now, &state.config, &mut rng);
            engine::pet_interaction(&mut pet, now);
            persist(state, &pet);
            Response::ok_empty()
        }
        RequestOp::Prompt { format: _ } => {
            let mut pet = state.pet.lock().unwrap();
            let mut rng = rand::rng();
            engine::catch_up(&mut pet, now, &state.config, &mut rng);
            persist(state, &pet);
            // Minimal placeholder rendering; full prompt rendering is a
            // later task.
            Response::ok_prompt(format!("{}% happy", pet.happiness.get()))
        }
        RequestOp::Hatch { species: _ } => {
            // Hatch/death-recovery UX is later plan territory -- not
            // implemented yet.
            Response::err("not implemented yet")
        }
        RequestOp::Unknown => Response::err("unsupported op"),
    }
}

/// Binds a Unix domain socket at `socket_path`, sets its permissions to
/// `0600`, and serves connections until `shutdown` resolves.
///
/// Each connection is handled in its own spawned task; each line
/// (newline-delimited JSON) is parsed as a [`Request`], dispatched via
/// [`handle_request`], and the [`Response`] written back as a JSON line.
/// Malformed JSON on a line produces an error `Response` and does NOT
/// kill the connection. Lines longer than [`MAX_LINE_BYTES`] are rejected
/// with an error response. Each connection has an idle timeout of
/// [`IDLE_TIMEOUT`].
///
/// SOCKET STALENESS: if `socket_path` already exists when binding, this
/// function first attempts to CONNECT to it (a quick probe). If the
/// connect succeeds, a live daemon is already listening there and this
/// call returns an error. If the connect fails (stale socket file from a
/// crashed previous daemon, or a bogus non-socket file), the stale file
/// is unlinked before binding.
///
/// If `ready` is provided, a signal is sent on it immediately after the
/// listener is bound (before the accept loop starts), so callers/tests
/// can synchronize on "the socket is ready to accept" instead of
/// sleeping blindly.
pub async fn serve(
    socket_path: &Path,
    state: Arc<ServerState>,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    serve_with_ready_signal(socket_path, state, shutdown, None).await
}

/// Same as [`serve`], but additionally sends `()` on `ready` right after
/// the socket is bound, before entering the accept loop. Primarily meant
/// for tests that want deterministic synchronization instead of a sleep.
pub async fn serve_with_ready_signal(
    socket_path: &Path,
    state: Arc<ServerState>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> anyhow::Result<()> {
    if socket_path.exists() {
        if UnixStream::connect(socket_path).await.is_ok() {
            anyhow::bail!("a daemon is already listening on {}", socket_path.display());
        }
        // Stale socket file (or a bogus non-socket file) from a previous
        // crashed daemon -- remove it before binding.
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    let mut perms = std::fs::metadata(socket_path)?.permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o600);
    std::fs::set_permissions(socket_path, perms)?;

    if let Some(ready) = ready {
        let _ = ready.send(());
    }

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                break;
            }
            accepted = listener.accept() => {
                let (stream, _addr) = accepted?;
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    handle_connection(stream, state).await;
                });
            }
        }
    }

    Ok(())
}

/// Handles a single accepted connection: reads newline-delimited JSON
/// requests, dispatches them via [`handle_request`], and writes back
/// newline-delimited JSON responses, until the connection is closed or
/// goes idle for longer than [`IDLE_TIMEOUT`].
async fn handle_connection(stream: UnixStream, state: Arc<ServerState>) {
    let mut reader = BufReader::new(stream);

    loop {
        let mut line = String::new();
        let read_result = tokio::time::timeout(IDLE_TIMEOUT, reader.read_line(&mut line)).await;

        let bytes_read = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(_)) => break, // I/O error on the connection
            Err(_) => break,     // idle timeout elapsed
        };

        if bytes_read == 0 {
            break; // EOF: client closed the connection
        }

        if line.len() > MAX_LINE_BYTES {
            let resp = Response::err("line too long");
            if write_response(&mut reader, &resp).await.is_err() {
                break;
            }
            continue;
        }

        let response = match serde_json::from_str::<Request>(line.trim_end()) {
            Ok(req) => handle_request(&state, &req),
            Err(_) => Response::err("malformed request: invalid JSON"),
        };

        if write_response(&mut reader, &response).await.is_err() {
            break;
        }
    }
}

/// Serializes `response` as a JSON line and writes it (with a trailing
/// `\n`) to the underlying stream of `reader`.
async fn write_response(
    reader: &mut BufReader<UnixStream>,
    response: &Response,
) -> std::io::Result<()> {
    let mut json = serde_json::to_string(response).expect("Response must always serialize");
    json.push('\n');
    reader.get_mut().write_all(json.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use chrono::{TimeZone, Utc};
    use std::time::Duration as StdDuration;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    fn new_state() -> (ServerState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("pet.json");
        // Use the real current time as the pet's baseline, not a fixed
        // past timestamp: `handle_request` calls `catch_up` against
        // `SystemClock::now()`, and a fixed past `last_tick` would
        // introduce a large, unpredictable simulated gap (potentially
        // landing the pet in the Asleep activity depending on what hour
        // that gap's end falls on), which would make ops like `feed`
        // silently no-op in a way unrelated to what this test is
        // actually checking.
        let now = SystemClock.now();
        let pet = PetState::newborn("T".into(), Species::Blob, now);
        let state = ServerState {
            pet: Mutex::new(pet),
            config: Config::default(),
            state_path,
        };
        (state, dir)
    }

    #[test]
    fn handle_request_ping_still_works() {
        let (state, _dir) = new_state();
        let req = Request::new(RequestOp::Ping);

        let resp = handle_request(&state, &req);

        assert!(resp.ok);
    }

    #[test]
    fn handle_request_feed_actually_feeds_the_pet() {
        let (state, _dir) = new_state();
        let req = Request::new(RequestOp::Feed {
            exit_code: 0,
            duration_ms: 0,
            argv0: "cargo".into(),
            ts: fixed_now().timestamp(),
        });

        let resp = handle_request(&state, &req);

        assert!(resp.ok);
        let pet = state.pet.lock().unwrap();
        assert_eq!(pet.commands_eaten, 1);
        assert!(pet.satiety.get() > 70);
    }

    #[test]
    fn handle_request_clean_actually_cleans() {
        let (state, _dir) = new_state();
        state.pet.lock().unwrap().poops.push(fixed_now());

        let req = Request::new(RequestOp::Clean);
        let resp = handle_request(&state, &req);

        assert!(resp.ok);
        assert!(state.pet.lock().unwrap().poops.is_empty());
    }

    #[test]
    fn handle_request_hatch_still_returns_not_implemented() {
        let (state, _dir) = new_state();
        let req = Request::new(RequestOp::Hatch {
            species: "blob".into(),
        });

        let resp = handle_request(&state, &req);

        assert!(!resp.ok);
        assert!(resp.error.is_some());
        assert!(!resp.error.unwrap().is_empty());
    }

    #[test]
    fn handle_request_unknown_op_returns_error() {
        let (state, _dir) = new_state();
        let req = Request::new(RequestOp::Unknown);

        let resp = handle_request(&state, &req);

        assert!(!resp.ok);
        assert!(resp.error.is_some());
    }

    fn new_state_arc() -> (Arc<ServerState>, tempfile::TempDir) {
        let (state, dir) = new_state();
        (Arc::new(state), dir)
    }

    async fn spawn_test_server(
        socket_path: std::path::PathBuf,
    ) -> (
        tokio::task::JoinHandle<anyhow::Result<()>>,
        tokio::sync::oneshot::Sender<()>,
        tempfile::TempDir,
    ) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (state, dir) = new_state_arc();

        let handle = tokio::spawn(async move {
            serve_with_ready_signal(&socket_path, state, shutdown_rx, Some(ready_tx)).await
        });

        // Wait for the ready signal instead of blind-sleeping; fall back
        // to a short timeout so a test never hangs forever if bind fails
        // silently in some unexpected way.
        let _ = tokio::time::timeout(StdDuration::from_secs(2), ready_rx).await;

        (handle, shutdown_tx, dir)
    }

    #[tokio::test]
    async fn serve_responds_to_ping_over_real_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let (handle, shutdown_tx, _state_dir) = spawn_test_server(socket_path.clone()).await;

        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let mut reader = BufReader::new(stream);
        reader
            .write_all(b"{\"v\":1,\"op\":\"ping\"}\n")
            .await
            .unwrap();

        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
        assert_eq!(value["ok"], true);

        let _ = shutdown_tx.send(());
        let result = tokio::time::timeout(StdDuration::from_secs(2), handle)
            .await
            .expect("server task should complete after shutdown signal")
            .expect("server task should not panic");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn serve_rejects_malformed_json_without_killing_connection() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let (handle, shutdown_tx, _state_dir) = spawn_test_server(socket_path.clone()).await;

        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let mut reader = BufReader::new(stream);

        reader.write_all(b"not valid json\n").await.unwrap();
        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["error"].is_string());

        // Connection must still be alive: a valid request afterward
        // should succeed.
        reader
            .write_all(b"{\"v\":1,\"op\":\"ping\"}\n")
            .await
            .unwrap();
        let mut resp_line2 = String::new();
        reader.read_line(&mut resp_line2).await.unwrap();
        let value2: serde_json::Value = serde_json::from_str(&resp_line2).unwrap();
        assert_eq!(value2["ok"], true);

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(StdDuration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn serve_unlinks_stale_socket_and_rebinds() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        // Simulate a stale socket file left behind by a crashed daemon:
        // a plain file at the socket path that isn't actually a
        // listening socket. Connecting to it will fail immediately.
        std::fs::write(&socket_path, b"").unwrap();

        let (handle, shutdown_tx, _state_dir) = spawn_test_server(socket_path.clone()).await;

        let stream = UnixStream::connect(&socket_path)
            .await
            .expect("serve() should have unlinked the stale file and rebound");
        let mut reader = BufReader::new(stream);
        reader
            .write_all(b"{\"v\":1,\"op\":\"ping\"}\n")
            .await
            .unwrap();

        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
        assert_eq!(value["ok"], true);

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(StdDuration::from_secs(2), handle).await;
    }
}
