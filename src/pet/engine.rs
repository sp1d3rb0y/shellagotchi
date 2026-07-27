//! The pure pet simulation engine.
//!
//! `tick` advances a [`PetState`] to reflect elapsed wall-clock time. It is
//! deliberately pure: it never reads the clock itself (no `Utc::now()`
//! anywhere in this file — `now` is always supplied by the caller, typically
//! via a `Clock` impl), and it never performs I/O. This keeps it trivially
//! testable with fixed timestamps and safe to call repeatedly (idempotent
//! when `now` hasn't advanced past `state.last_tick`).
//!
//! This module currently implements only time-based stat decay (Task 6).
//! Later tasks will extend `tick` with sleep transitions, feeding, boredom,
//! pooping, and sickness logic, and will grow the `Event` enum accordingly.

use chrono::{DateTime, Duration, Utc};

use crate::config::Config;
use crate::pet::state::PetState;

/// An event emitted by a tick, for logging/UI purposes. Empty for now;
/// later tasks will add variants (fed, pooped, slept, got sick, etc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Event {}

/// Per-hour decay rates for time-based stat loss.
#[allow(dead_code)]
const SATIETY_LOSS_PER_HOUR: f64 = 3.0;
#[allow(dead_code)]
const ENERGY_LOSS_PER_HOUR: f64 = 2.0;
#[allow(dead_code)]
const HYGIENE_LOSS_PER_HOUR: f64 = 1.0;

/// Advances `state` to reflect elapsed wall-clock time up to `now`.
///
/// Pure with respect to time: never reads the clock itself; `now` must be
/// supplied by the caller (typically via a `Clock` impl). Idempotent when
/// called twice with the same `now` — the second call sees
/// `elapsed = now - state.last_tick == 0` (since the first call already
/// advanced `last_tick` to `now`) and is a no-op.
///
/// Decay is proportional to elapsed time (fractional hours apply fractional
/// decay, rounded to the nearest whole point) rather than only firing once
/// per full hour.
#[allow(dead_code)]
pub fn tick(state: &mut PetState, now: DateTime<Utc>, _cfg: &Config) -> Vec<Event> {
    let elapsed = now - state.last_tick;
    if elapsed <= Duration::zero() {
        return Vec::new();
    }

    let hours = elapsed.num_seconds() as f64 / 3600.0;

    let satiety_loss = (SATIETY_LOSS_PER_HOUR * hours).round() as u8;
    let energy_loss = (ENERGY_LOSS_PER_HOUR * hours).round() as u8;
    let hygiene_loss = (HYGIENE_LOSS_PER_HOUR * hours).round() as u8;

    state.satiety = state.satiety - satiety_loss;
    state.energy = state.energy - energy_loss;
    state.hygiene = state.hygiene - hygiene_loss;
    state.last_tick = now;

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use chrono::TimeZone;

    /// A fixed, deterministic timestamp for tests. Tests must never call
    /// `Utc::now()`/`Local::now()` directly (clippy.toml disallows it
    /// project-wide, so behaviour stays reproducible).
    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn decay_after_one_hour() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();

        tick(&mut state, start + Duration::hours(1), &cfg);

        assert_eq!(state.satiety.get(), 70 - 3);
        assert_eq!(state.energy.get(), 70 - 2);
        assert_eq!(state.hygiene.get(), 100 - 1);
        assert_eq!(state.last_tick, start + Duration::hours(1));
    }

    #[test]
    fn no_change_if_elapsed_less_than_a_tick() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();

        tick(&mut state, start + Duration::seconds(1), &cfg);

        // (3.0 * (1.0/3600.0)).round() == 0.0, same for energy/hygiene.
        assert_eq!(state.satiety.get(), 70);
        assert_eq!(state.energy.get(), 70);
        assert_eq!(state.hygiene.get(), 100);
        // last_tick still advances even though the decay rounded to zero.
        assert_eq!(state.last_tick, start + Duration::seconds(1));
    }

    #[test]
    fn idempotent_when_called_twice_with_same_now() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();
        let now = start + Duration::hours(2);

        tick(&mut state, now, &cfg);
        let satiety_after_first = state.satiety.get();
        let energy_after_first = state.energy.get();
        let hygiene_after_first = state.hygiene.get();

        tick(&mut state, now, &cfg);

        assert_eq!(state.satiety.get(), satiety_after_first);
        assert_eq!(state.energy.get(), energy_after_first);
        assert_eq!(state.hygiene.get(), hygiene_after_first);
    }
}
