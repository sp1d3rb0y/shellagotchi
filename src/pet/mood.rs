use serde::{Deserialize, Serialize};

use crate::pet::state::{Activity, PetState};

/// The pet's overall mood, derived purely from its current `PetState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Mood {
    Ecstatic,
    Happy,
    Content,
    Meh,
    Sad,
    Miserable,
    Sick,
    Dead,
}

/// Derives the pet's `Mood` from its current state.
///
/// This is a pure function with no I/O and no time reads. `Dead` and `Sick`
/// are overriding states that take precedence regardless of the numeric
/// stats; otherwise mood is derived from the `happiness` stat band.
#[allow(dead_code)]
pub fn derive_mood(state: &PetState) -> Mood {
    if !state.alive {
        return Mood::Dead;
    }
    if state.activity == Activity::Sick {
        return Mood::Sick;
    }
    match state.happiness.get() {
        90..=100 => Mood::Ecstatic,
        70..=89 => Mood::Happy,
        50..=69 => Mood::Content,
        35..=49 => Mood::Meh,
        20..=34 => Mood::Sad,
        0..=19 => Mood::Miserable,
        _ => Mood::Miserable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use crate::pet::stats::Stat;
    use chrono::DateTime;

    fn make_state(alive: bool, activity: Activity, happiness: u16) -> PetState {
        let fixed_now = DateTime::from_timestamp(0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now);
        state.alive = alive;
        state.activity = activity;
        state.happiness = Stat::new(happiness);
        state
    }

    #[test]
    fn mood_bands_table_driven() {
        let cases = [
            (false, Activity::Awake, 100u16, Mood::Dead),
            (true, Activity::Sick, 100, Mood::Sick),
            (true, Activity::Awake, 95, Mood::Ecstatic),
            (true, Activity::Awake, 75, Mood::Happy),
            (true, Activity::Awake, 60, Mood::Content),
            (true, Activity::Awake, 40, Mood::Meh),
            (true, Activity::Awake, 25, Mood::Sad),
            (true, Activity::Awake, 5, Mood::Miserable),
        ];

        for (alive, activity, happiness, expected) in cases {
            let state = make_state(alive, activity, happiness);
            assert_eq!(
                derive_mood(&state),
                expected,
                "alive={alive}, activity={activity:?}, happiness={happiness} -> expected {expected:?}"
            );
        }
    }
}
