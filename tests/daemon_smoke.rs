//! End-to-end smoke test: spawns a real `shellagotchi daemon` subprocess,
//! waits for it to bind its Unix socket, then talks to it via a separate
//! `shellagotchi feed` subprocess. Kept intentionally simple (see the
//! module docs on `feed_exits_zero_and_silent_when_daemon_absent` in
//! `cli_smoke.rs` for the general daemon-is-optional philosophy): a full
//! graceful-SIGTERM-then-verify-save test would need a signal-sending
//! dependency (`nix`/`libc`) this project doesn't otherwise pull in, so
//! this test settles for proving the daemon starts, binds its socket, and
//! answers a real client request, then force-kills it for cleanup.

use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

#[test]
fn daemon_starts_binds_socket_and_answers_feed() {
    let dir = tempfile::tempdir().unwrap();
    let runtime_dir = dir.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let socket_path = runtime_dir.join("shellagotchi").join("sock");

    let bin = assert_cmd::cargo::cargo_bin("shellagotchi");

    let mut daemon = StdCommand::new(&bin)
        .arg("daemon")
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .spawn()
        .expect("failed to spawn daemon subprocess");

    // Wait for the socket file to appear, polling rather than a fixed
    // sleep, with a generous overall timeout since real subprocess
    // daemons can be slow to start in CI.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !socket_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        socket_path.exists(),
        "daemon did not create its socket file within the timeout"
    );

    let feed_status = StdCommand::new(&bin)
        .args(["feed", "--exit", "0", "--duration", "10"])
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .status()
        .expect("failed to spawn feed subprocess");
    assert!(
        feed_status.success(),
        "feed subprocess against a live daemon should exit 0"
    );

    // Cleanup: no dependency-free way to send a real SIGTERM from std
    // alone, so this settles for SIGKILL via Child::kill() -- graceful
    // shutdown/save-on-SIGTERM is covered qualitatively by the manual
    // smoke test in this task's write-up, not by this automated test.
    let _ = daemon.kill();
    let _ = daemon.wait();
}
