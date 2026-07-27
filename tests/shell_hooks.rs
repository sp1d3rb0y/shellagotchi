//! Critical integration tests exercising the shell hooks against real
//! bash/zsh/fish subprocesses. These verify the plan's single
//! non-negotiable property: `shellagotchi init <shell>`'s hook must
//! preserve the user's own `$?`/`$status` unchanged.
//!
//! All tests point `XDG_RUNTIME_DIR` at a fresh tempdir with no daemon
//! listening, so the `feed` calls inside each hook fail fast (bounded
//! by the IPC client's 100ms timeout) rather than hanging the test.

use std::process::Command;

fn shellagotchi_bin() -> &'static str {
    env!("CARGO_BIN_EXE_shellagotchi")
}

#[test]
fn bash_hook_preserves_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let script = format!(
        r#"eval "$({} init bash)"; false; echo "exit=$?""#,
        shellagotchi_bin()
    );

    let output = Command::new("bash")
        .arg("-c")
        .arg(script)
        .env("XDG_RUNTIME_DIR", dir.path())
        .output()
        .expect("failed to spawn bash");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("exit=1"),
        "expected exit=1 in bash stdout, got: {stdout:?} (stderr: {})",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn zsh_hook_preserves_exit_code() {
    if Command::new("which")
        .arg("zsh")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("skipping zsh_hook_preserves_exit_code: zsh not installed");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let script = format!(
        r#"eval "$({} init zsh)"; false; echo "exit=$?""#,
        shellagotchi_bin()
    );

    let output = Command::new("zsh")
        .arg("-c")
        .arg(script)
        .env("XDG_RUNTIME_DIR", dir.path())
        .output()
        .expect("failed to spawn zsh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("exit=1"),
        "expected exit=1 in zsh stdout, got: {stdout:?} (stderr: {})",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fish_hook_does_not_crash_shell() {
    if Command::new("which")
        .arg("fish")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("skipping fish_hook_does_not_crash_shell: fish not installed");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let script = format!(r#"{} init fish | source; echo hello"#, shellagotchi_bin());

    let output = Command::new("fish")
        .arg("-c")
        .arg(script)
        .env("XDG_RUNTIME_DIR", dir.path())
        .output()
        .expect("failed to spawn fish");

    assert!(
        output.status.success(),
        "fish exited non-zero after sourcing the hook: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
}

#[test]
fn doctor_exits_nonzero_when_daemon_down_and_prints_report() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("config-home-does-not-exist");

    let output = Command::new(shellagotchi_bin())
        .arg("doctor")
        .env("XDG_RUNTIME_DIR", dir.path())
        .env("HOME", &config_dir)
        .output()
        .expect("failed to spawn shellagotchi doctor");

    assert!(
        !output.status.success(),
        "expected doctor to exit non-zero when the daemon/socket are absent"
    );
    assert!(
        !output.stdout.is_empty(),
        "expected doctor to print a diagnostic report to stdout"
    );
}
