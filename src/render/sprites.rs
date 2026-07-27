//! ASCII sprites keyed by `(Species, Activity, Mood)`.
//!
//! Coverage policy (per the plan's documented relaxation): `Blob` has
//! 100% coverage of every `(Activity, Mood)` combination. The other
//! three species (`Cat`, `Dragon`, `Ghost`) may cover only a subset;
//! any combination not explicitly modeled for them falls back to
//! Blob's art for the same `(Activity, Mood)` pair. If somehow even
//! that lookup fails (it shouldn't, since Blob is exhaustive), a
//! generic placeholder is returned. [`sprite_for`] therefore never
//! panics and never returns an empty string for any valid enum
//! combination.

use crate::pet::mood::Mood;
use crate::pet::state::{Activity, Species};

const PLACEHOLDER: &str = "  (?)  \n (o.o) \n  /|\\  ";

/// Looks up ASCII art for a given `(species, activity, mood)` combination.
///
/// Fallback chain: exact species match -> Blob's art for the same
/// `(activity, mood)` -> a generic placeholder.
pub fn sprite_for(species: Species, activity: Activity, mood: Mood) -> &'static str {
    lookup(species, activity, mood)
        .or_else(|| lookup(Species::Blob, activity, mood))
        .unwrap_or(PLACEHOLDER)
}

fn lookup(species: Species, activity: Activity, mood: Mood) -> Option<&'static str> {
    match species {
        // Blob: exhaustive coverage of every (Activity, Mood) pair.
        // `Mood::Dead` is checked first in each arm's guard ordering so it
        // always takes precedence over activity-specific art (e.g. a dead
        // pet that also happens to be "asleep" still shows the dead sprite).
        Species::Blob => Some(match mood {
            Mood::Dead => "  x_x  \n /   \\ \n(_____)",
            Mood::Sick => " >.<   \n (o_o) \n  |||  ",
            _ => match activity {
                Activity::Asleep => "  z z  \n (-.-) \n  )_(  ",
                Activity::Sick => " >.<   \n (o_o) \n  |||  ",
                Activity::Dead => "  x_x  \n /   \\ \n(_____)",
                Activity::Awake => match mood {
                    Mood::Ecstatic => " \\o/   \n (^o^) \n  | |  ",
                    Mood::Happy => "  ___  \n (^_^) \n  | |  ",
                    Mood::Content => "  ___  \n (-_-) \n  | |  ",
                    Mood::Meh => "  ___  \n (._.) \n  | |  ",
                    Mood::Sad => "  ___  \n (;_;) \n  | |  ",
                    Mood::Miserable => "  ___  \n (T_T) \n  | |  ",
                    Mood::Sick | Mood::Dead => unreachable!("handled above"),
                },
            },
        }),
        // Cat: subset coverage -- only Asleep and Awake+(Happy/Ecstatic)
        // are modeled explicitly. Everything else falls back to Blob.
        Species::Cat => match (activity, mood) {
            (Activity::Asleep, _) => Some("  zzz  \n =^.^= \n  )_(  "),
            (Activity::Awake, Mood::Happy) | (Activity::Awake, Mood::Ecstatic) => {
                Some(" /\\_/\\ \n( ^.^ )\n > ^ < ")
            }
            _ => None,
        },
        // Dragon: subset coverage -- only Awake+(Happy/Ecstatic).
        Species::Dragon => match (activity, mood) {
            (Activity::Awake, Mood::Happy) | (Activity::Awake, Mood::Ecstatic) => {
                Some(" ^....^\n<(^_^)>\n  /||\\ ")
            }
            _ => None,
        },
        // Ghost: subset coverage -- only Awake+(Happy/Ecstatic).
        Species::Ghost => match (activity, mood) {
            (Activity::Awake, Mood::Happy) | (Activity::Awake, Mood::Ecstatic) => {
                Some("  .-.  \n (o o) \n  \\_/  ")
            }
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SPECIES: [Species; 4] =
        [Species::Blob, Species::Cat, Species::Dragon, Species::Ghost];
    const ALL_ACTIVITIES: [Activity; 4] = [
        Activity::Awake,
        Activity::Asleep,
        Activity::Sick,
        Activity::Dead,
    ];
    const ALL_MOODS: [Mood; 8] = [
        Mood::Ecstatic,
        Mood::Happy,
        Mood::Content,
        Mood::Meh,
        Mood::Sad,
        Mood::Miserable,
        Mood::Sick,
        Mood::Dead,
    ];

    #[test]
    fn every_combination_returns_non_empty_sprite() {
        for species in ALL_SPECIES {
            for activity in ALL_ACTIVITIES {
                for mood in ALL_MOODS {
                    let sprite = sprite_for(species, activity, mood);
                    assert!(
                        !sprite.is_empty(),
                        "expected non-empty sprite for ({species:?}, {activity:?}, {mood:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn blob_dead_mood_overrides_asleep_activity() {
        let dead_sprite = sprite_for(Species::Blob, Activity::Asleep, Mood::Dead);
        let asleep_sprite = sprite_for(Species::Blob, Activity::Awake, Mood::Content);
        assert_ne!(dead_sprite, asleep_sprite);
        assert_eq!(
            dead_sprite,
            sprite_for(Species::Blob, Activity::Awake, Mood::Dead)
        );
    }

    #[test]
    fn unmodeled_species_combo_falls_back_to_blob() {
        // Cat has no explicit entry for (Sick, Miserable), so it must
        // fall back to Blob's art for the same (activity, mood).
        let cat_sprite = sprite_for(Species::Cat, Activity::Sick, Mood::Miserable);
        let blob_sprite = sprite_for(Species::Blob, Activity::Sick, Mood::Miserable);
        assert_eq!(cat_sprite, blob_sprite);
    }
}
