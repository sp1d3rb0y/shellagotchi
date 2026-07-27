use assert_cmd::Command;
use std::time::Instant;

#[test]
fn prompt_is_silent_and_fast_when_cache_missing() {
    // No cache file exists at all under this fresh runtime dir (daemon
    // never ran), so `prompt` must print nothing and exit success,
    // quickly.
    let dir = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let assert = Command::cargo_bin("shellagotchi")
        .unwrap()
        .env("XDG_RUNTIME_DIR", dir.path())
        .args(["prompt"])
        .assert();
    let elapsed = start.elapsed();
    assert.success().stdout("").stderr("");
    assert!(
        elapsed.as_millis() < 200,
        "prompt took too long: {:?}",
        elapsed
    );
}

#[test]
fn prompt_reads_cache_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let runtime_dir = dir.path().join("shellagotchi");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let cache_path = runtime_dir.join("prompt");
    std::fs::write(&cache_path, "compact-line\nminimal-line\nverbose-line").unwrap();

    let assert = Command::cargo_bin("shellagotchi")
        .unwrap()
        .env("XDG_RUNTIME_DIR", dir.path())
        .args(["prompt"])
        .assert();
    assert.success().stdout("compact-line\n");

    let assert = Command::cargo_bin("shellagotchi")
        .unwrap()
        .env("XDG_RUNTIME_DIR", dir.path())
        .args(["prompt", "--format", "verbose"])
        .assert();
    assert.success().stdout("verbose-line\n");
}

#[test]
fn prompt_shows_stale_marker_for_old_cache() {
    let dir = tempfile::tempdir().unwrap();
    let runtime_dir = dir.path().join("shellagotchi");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let cache_path = runtime_dir.join("prompt");
    std::fs::write(&cache_path, "compact-line\nminimal-line\nverbose-line").unwrap();

    // Force the staleness threshold to 0 seconds so the freshly-written
    // cache file is immediately considered stale, without needing to
    // sleep past a real threshold.
    let assert = Command::cargo_bin("shellagotchi")
        .unwrap()
        .env("XDG_RUNTIME_DIR", dir.path())
        .env("SHELLAGOTCHI_STALE_THRESHOLD_SECS", "0")
        .args(["prompt"])
        .assert();
    assert.success().stdout("?\n");
}

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
