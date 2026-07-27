use assert_cmd::Command;
use std::time::Instant;

#[test]
fn feed_exits_zero_and_silent_when_daemon_absent() {
    // Point the runtime dir at a tempdir that definitely has no daemon
    // listening (paths.rs resolves the runtime dir from XDG_RUNTIME_DIR).
    let dir = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let assert = Command::cargo_bin("shellagotchi")
        .unwrap()
        .env("XDG_RUNTIME_DIR", dir.path())
        .args(["feed", "--exit", "1", "--duration", "50"])
        .assert();
    let elapsed = start.elapsed();
    assert.success().stdout("").stderr("");
    assert!(
        elapsed.as_millis() < 500,
        "feed took too long: {:?}",
        elapsed
    );
}
