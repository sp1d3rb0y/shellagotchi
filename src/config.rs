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
    pub sleep_start_hour: u32,
    pub wake_hour: u32,
    pub poop_interval_commands: u32,
    pub unicode: bool,
    pub junk_food_commands: Vec<String>,
    /// Commands whose argv0, when detected via the shell hook, trigger an
    /// automatic pet cleanup (poops cleared, hygiene maxed) in [`crate::pet::engine::feed`],
    /// with no extra user setup required beyond the normal shell hook.
    pub clean_commands: Vec<String>,
    pub ignored_exit_codes: Vec<i32>,
    pub bad_food_meter_decay_per_hour: u32,
    /// Caps how many hours of elapsed real time [`crate::pet::engine::catch_up`]
    /// will simulate in one go, protecting the pet from unrealistic
    /// single-shot decay after a long laptop-suspend or daemon-downtime gap.
    /// `None` disables the cap entirely ("hardcore mode": the full gap is
    /// simulated, however long it was).
    pub max_offline_hours: Option<f64>,
    /// How often (in seconds) the daemon's main loop ticks the pet forward
    /// via `catch_up`, independent of any IPC requests.
    pub tick_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            pet_name: "Pet".to_string(),
            boredom_after_minutes: 45,
            sleep_start_hour: 23,
            wake_hour: 7,
            poop_interval_commands: 40,
            unicode: true,
            junk_food_commands: vec![
                "rm".to_string(),
                "kill".to_string(),
                "pkill".to_string(),
                "dd".to_string(),
            ],
            clean_commands: vec!["clean".to_string(), "shellagotchi-clean".to_string()],
            ignored_exit_codes: vec![130],
            bad_food_meter_decay_per_hour: 1,
            max_offline_hours: Some(12.0),
            tick_interval_secs: 60,
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
