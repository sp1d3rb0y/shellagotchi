use std::path::Path;

use crate::pet::state::PetState;

/// Saves `state` to `path` atomically: writes to a `.tmp` sibling file,
/// fsyncs it, then renames it into place. This ensures a crash or power
/// loss mid-write never leaves a corrupt/partial save file at `path` — the
/// rename is atomic on the same filesystem, so `path` always contains
/// either the old complete state or the new complete state, never a mix.
/// Also copies the PREVIOUS contents of `path` (if it existed) to a
/// `.bak` sibling before overwriting, as a fallback if the new save is
/// somehow corrupt (defense in depth, not expected to normally be needed).
pub fn save(state: &PetState, path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        let backup_path = backup_path_for(path);
        if let Err(err) = std::fs::copy(path, &backup_path) {
            tracing::warn!(
                "failed to back up previous save file {} to {}: {err}",
                path.display(),
                backup_path.display()
            );
        }
    }

    let tmp_path = tmp_path_for(path);
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::File::open(&tmp_path)?.sync_all()?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Loads a `PetState` from `path`. If the primary file is missing, returns
/// `Ok(None)` (caller should then create a fresh pet). If the primary file
/// exists but fails to parse, attempts to fall back to the `.bak` sibling
/// (logging loudly that recovery was needed). If BOTH the primary and
/// backup fail to parse (or only primary exists and it's corrupt with no
/// backup), the corrupt primary file is renamed aside with a timestamp
/// suffix for forensics, and `Ok(None)` is returned (caller starts a fresh
/// pet) rather than erroring out — a daemon should never refuse to start
/// just because its save file rotted.
pub fn load(path: &Path) -> anyhow::Result<Option<PetState>> {
    let primary_contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    if let Ok(state) = serde_json::from_str::<PetState>(&primary_contents) {
        return Ok(Some(state));
    }

    // Primary failed to parse. Try the backup.
    let backup_path = backup_path_for(path);
    let recovered_from_backup = std::fs::read_to_string(&backup_path)
        .ok()
        .and_then(|contents| serde_json::from_str::<PetState>(&contents).ok());
    if let Some(state) = recovered_from_backup {
        tracing::warn!(
            "primary save file corrupt, recovered from backup: {}",
            path.display()
        );
        return Ok(Some(state));
    }

    // Both primary and backup are unusable. Rename the corrupt primary
    // aside for forensics and start fresh.
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let corrupt_path = path.with_file_name(format!(
        "{}.corrupt.{timestamp}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    tracing::warn!(
        "save file at {} (and backup, if any) is unrecoverably corrupt; \
         renaming aside to {} and starting fresh",
        path.display(),
        corrupt_path.display()
    );
    std::fs::rename(path, &corrupt_path)?;
    Ok(None)
}

fn tmp_path_for(path: &Path) -> std::path::PathBuf {
    path.with_extension(append_extension(path, "tmp"))
}

fn backup_path_for(path: &Path) -> std::path::PathBuf {
    path.with_extension(append_extension(path, "bak"))
}

/// Builds a new extension by appending `suffix` to whatever extension
/// `path` already has (e.g. `pet.json` -> `json.tmp`), so sibling files
/// stay next to the original with an obvious suffix.
fn append_extension(path: &Path, suffix: &str) -> String {
    match path.extension() {
        Some(ext) => format!("{}.{suffix}", ext.to_string_lossy()),
        None => suffix.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use chrono::{DateTime, TimeZone, Utc};

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");
        let state = PetState::newborn("Rusty".into(), Species::Blob, fixed_now());

        save(&state, &path).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded, Some(state));
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does_not_exist.json");

        let loaded = load(&path).unwrap();

        assert_eq!(loaded, None);
    }

    #[test]
    fn load_recovers_from_backup_when_primary_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");

        let first_state = PetState::newborn("Rusty".into(), Species::Blob, fixed_now());
        save(&first_state, &path).unwrap();

        let second_state = PetState::newborn("Buddy".into(), Species::Dragon, fixed_now());
        save(&second_state, &path).unwrap();

        // Corrupt the primary; the backup (from the first save) should still
        // hold valid content for the first state.
        std::fs::write(&path, b"{ not valid json").unwrap();

        let loaded = load(&path).unwrap();

        assert_eq!(loaded, Some(first_state));
    }

    #[test]
    fn load_handles_both_corrupt_by_renaming_and_returning_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");
        let backup_path = backup_path_for(&path);

        std::fs::write(&path, b"{ not valid json").unwrap();
        std::fs::write(&backup_path, b"{ also not valid json").unwrap();

        let loaded = load(&path).unwrap();

        assert_eq!(loaded, None);

        let has_corrupt_file = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("pet.json.corrupt.")
            });
        assert!(
            has_corrupt_file,
            "expected a pet.json.corrupt.<timestamp> file to be created"
        );
    }

    #[test]
    fn save_never_leaves_partial_file_visible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");
        let state = PetState::newborn("Rusty".into(), Species::Ghost, fixed_now());

        save(&state, &path).unwrap();

        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: PetState = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed, state);
    }
}
