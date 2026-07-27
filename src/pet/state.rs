use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::pet::stats::Stat;

/// The current on-disk schema version for `PetState`.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// The species of a pet, chosen randomly at hatch time (in a later task).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Species {
    Blob,
    Cat,
    Dragon,
    Ghost,
}

/// The current activity/state of a pet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Activity {
    Awake,
    Asleep,
    Sick,
    Dead,
}

/// The persisted state of a pet.
///
/// `schema_version` must be `1` for the current schema; future schema
/// changes should bump this and add migration logic when reading older
/// saves. Unknown fields are rejected at deserialization time so that
/// schema drift is caught early rather than silently ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub struct PetState {
    pub schema_version: u32,
    pub name: String,
    pub species: Species,
    pub born_at: DateTime<Utc>,
    pub last_tick: DateTime<Utc>,
    pub satiety: Stat,
    pub happiness: Stat,
    pub energy: Stat,
    pub hygiene: Stat,
    pub boredom: Stat,
    pub health: Stat,
    pub bad_food_meter: u32,
    pub poops: Vec<DateTime<Utc>>,
    pub commands_eaten: u64,
    pub success_streak: u32,
    pub failure_streak: u32,
    pub activity: Activity,
    pub alive: bool,
    pub last_command_at: DateTime<Utc>,
    pub last_pet_interaction: Option<DateTime<Utc>>,
}

#[allow(dead_code)]
impl PetState {
    /// Constructs a freshly-hatched pet with the documented newborn
    /// defaults: satiety/happiness/energy at 70, hygiene/health at 100,
    /// boredom at 0, and all counters/streaks/poops empty or zero.
    pub fn newborn(name: String, species: Species, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            name,
            species,
            born_at: now,
            last_tick: now,
            satiety: Stat::new(70),
            happiness: Stat::new(70),
            energy: Stat::new(70),
            hygiene: Stat::new(100),
            boredom: Stat::new(0),
            health: Stat::new(100),
            bad_food_meter: 0,
            poops: Vec::new(),
            commands_eaten: 0,
            success_streak: 0,
            failure_streak: 0,
            activity: Activity::Awake,
            alive: true,
            last_command_at: now,
            last_pet_interaction: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// A fixed, deterministic timestamp for tests. Tests must never call
    /// `Utc::now()`/`Local::now()` directly (clippy.toml disallows it
    /// project-wide, including test code, so behaviour stays reproducible).
    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn newborn_has_documented_defaults() {
        let now = fixed_now();
        let p = PetState::newborn("Rusty".into(), Species::Blob, now);
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.satiety.get(), 70);
        assert_eq!(p.happiness.get(), 70);
        assert_eq!(p.energy.get(), 70);
        assert_eq!(p.hygiene.get(), 100);
        assert_eq!(p.health.get(), 100);
        assert_eq!(p.boredom.get(), 0);
        assert_eq!(p.commands_eaten, 0);
        assert_eq!(p.success_streak, 0);
        assert_eq!(p.failure_streak, 0);
        assert_eq!(p.bad_food_meter, 0);
        assert!(p.poops.is_empty());
        assert_eq!(p.activity, Activity::Awake);
        assert!(p.alive);
    }

    #[test]
    fn json_roundtrip_is_stable() {
        let now = fixed_now();
        let p = PetState::newborn("Rusty".into(), Species::Dragon, now);
        let json = serde_json::to_string(&p).unwrap();
        let back: PetState = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn unknown_field_is_rejected_with_clear_error() {
        let now = fixed_now();
        let p = PetState::newborn("Rusty".into(), Species::Ghost, now);
        let mut value: serde_json::Value = serde_json::to_value(&p).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("totally_unknown_field".into(), serde_json::json!(true));
        let result: Result<PetState, _> = serde_json::from_value(value);
        assert!(
            result.is_err(),
            "expected deserialization to fail on unknown field"
        );
    }
}
