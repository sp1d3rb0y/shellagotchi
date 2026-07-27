//! XDG-ish path resolution for shellagotchi's config, state, and runtime dirs.
//!
//! Notes on choices:
//! - Config dir: `directories::ProjectDirs` gives us `~/.config/shellagotchi/` on Linux.
//! - State dir: `directories::ProjectDirs` has no cross-platform "state dir" concept, so
//!   (Linux-only project) we manually join `$HOME/.local/state/shellagotchi` via
//!   `directories::BaseDirs::home_dir()`.
//! - Runtime dir: prefer `$XDG_RUNTIME_DIR` if set; otherwise fall back to
//!   `/tmp/shellagotchi-<user>` using `$USER` as a disambiguator (avoids pulling in a
//!   new dependency just to read the numeric uid).

use directories::{BaseDirs, ProjectDirs};
use std::path::PathBuf;

const QUALIFIER: &str = "";
const ORGANIZATION: &str = "";
const APPLICATION: &str = "shellagotchi";

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
}

/// Returns the path to the config file, honouring the `SHELLAGOTCHI_CONFIG` env
/// override if set.
pub fn config_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("SHELLAGOTCHI_CONFIG") {
        return PathBuf::from(path);
    }
    let dir = project_dirs()
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".config/shellagotchi"));
    dir.join("config.toml")
}

/// Returns the path to the state directory (`~/.local/state/shellagotchi/`).
pub fn state_dir() -> PathBuf {
    if let Some(base) = BaseDirs::new() {
        base.home_dir().join(".local/state/shellagotchi")
    } else {
        PathBuf::from(".local/state/shellagotchi")
    }
}

/// Returns the path to the persisted pet state file.
pub fn state_file_path() -> PathBuf {
    state_dir().join("pet.json")
}

/// Returns the runtime directory, preferring `$XDG_RUNTIME_DIR` if set.
pub fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("shellagotchi");
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    PathBuf::from(format!("/tmp/shellagotchi-{user}"))
}

/// Returns the path to the daemon's Unix domain socket, inside the
/// runtime directory.
pub fn socket_path() -> PathBuf {
    runtime_dir().join("sock")
}

/// Ensures the config, state, and runtime directories exist on disk.
pub fn ensure_dirs_exist() -> std::io::Result<()> {
    if let Some(parent) = config_file_path().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(state_dir())?;
    std::fs::create_dir_all(runtime_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_is_honoured() {
        let custom = "/tmp/some/custom/path.toml";
        // SAFETY-ish: known minor test-ordering risk since env vars are process-global
        // and tests run in parallel; this is the only paths.rs test relying on this var.
        // SAFETY: test-only env mutation; no other threads read this var concurrently
        // within this process's lifetime in a way that would cause a data race here.
        unsafe {
            std::env::set_var("SHELLAGOTCHI_CONFIG", custom);
        }
        let resolved = config_file_path();
        unsafe {
            std::env::remove_var("SHELLAGOTCHI_CONFIG");
        }
        assert_eq!(resolved, PathBuf::from(custom));
    }

    #[test]
    fn state_file_path_ends_with_pet_json() {
        assert_eq!(state_file_path().file_name().unwrap(), "pet.json");
    }

    #[test]
    fn runtime_dir_uses_xdg_runtime_dir_when_set() {
        // SAFETY: test-only env mutation, same rationale as above.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", "/tmp/xdg-test-runtime");
        }
        let dir = runtime_dir();
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        assert_eq!(dir, PathBuf::from("/tmp/xdg-test-runtime/shellagotchi"));
    }
}
