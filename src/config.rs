//! Configuration loading with serde defaults and TOML partial-merge semantics.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Shellagotchi runtime configuration. Missing keys in a partial TOML file fall
/// back to these defaults (via `#[serde(default)]` at the struct level).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub pet_name: String,
    pub boredom_after_minutes: u32,
    pub max_feeds_per_min: u32,
    pub sleep_start_hour: u32,
    pub wake_hour: u32,
    pub poop_interval_commands: u32,
    pub unicode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            pet_name: "Pet".to_string(),
            boredom_after_minutes: 45,
            max_feeds_per_min: 30,
            sleep_start_hour: 23,
            wake_hour: 7,
            poop_interval_commands: 40,
            unicode: true,
        }
    }
}

/// Loads the config from the default resolved path (env override or XDG default).
pub fn load() -> anyhow::Result<Config> {
    load_from(None)
}

/// Loads the config from an explicit path, or from the default resolution when
/// `path` is `None`. Missing files yield `Config::default()`; malformed TOML
/// yields an `Err` whose message includes the file path.
pub fn load_from(path: Option<&Path>) -> anyhow::Result<Config> {
    let resolved = match path {
        Some(p) => p.to_path_buf(),
        None => crate::paths::config_file_path(),
    };

    if !resolved.exists() {
        return Ok(Config::default());
    }

    let contents = std::fs::read_to_string(&resolved)
        .with_context(|| format!("failed to read config at {}", resolved.display()))?;

    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config at {}", resolved.display()))?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let nonexistent = dir.path().join("does-not-exist.toml");
        let config = load_from(Some(&nonexistent)).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn partial_toml_merges_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"pet_name = "Rusty""#).unwrap();

        let config = load_from(Some(&path)).unwrap();
        assert_eq!(config.pet_name, "Rusty");
        assert_eq!(config.boredom_after_minutes, 45);
    }

    #[test]
    fn malformed_toml_errors_with_path_in_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "pet_name = [unterminated").unwrap();

        let err = load_from(Some(&path)).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains(&path.display().to_string()),
            "expected error message to contain path, got: {message}"
        );
    }
}
