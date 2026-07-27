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

use chrono::{DateTime, Duration, Timelike, Utc};
use rand::RngExt;

use crate::config::Config;
use crate::pet::state::{Activity, PetState};
use crate::pet::stats::Stat;

/// An event emitted by a tick, for logging/UI purposes. Empty for now;
/// later tasks will add variants (fed, pooped, slept, got sick, etc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Event {}

/// A single shell command completion, fed to [`feed`] to update the pet's
/// stats. One `FeedEvent` corresponds to one shell command's exit.
#[allow(dead_code)]
pub struct FeedEvent<'a> {
    pub exit_code: i32,
    pub argv0: &'a str,
    pub now: DateTime<Utc>,
}

/// Per-hour decay rates for time-based stat loss.
#[allow(dead_code)]
const SATIETY_LOSS_PER_HOUR: f64 = 3.0;
#[allow(dead_code)]
const ENERGY_LOSS_PER_HOUR: f64 = 2.0;
#[allow(dead_code)]
const HYGIENE_LOSS_PER_HOUR: f64 = 1.0;
#[allow(dead_code)]
const ENERGY_GAIN_PER_HOUR_ASLEEP: f64 = 8.0;

/// Determines whether `now`'s hour-of-day falls within the configured sleep
/// window (`cfg.sleep_start_hour` to `cfg.wake_hour`), correctly handling a
/// window that crosses midnight (e.g. 23 -> 7).
///
/// Simplification: treats the stored UTC timestamp's hour-of-day as the
/// sleep boundary; full local-timezone handling can be layered on later
/// without changing this contract.
fn is_within_sleep_window(now: DateTime<Utc>, cfg: &Config) -> bool {
    let hour = now.hour();
    if cfg.sleep_start_hour <= cfg.wake_hour {
        hour >= cfg.sleep_start_hour && hour < cfg.wake_hour
    } else {
        hour >= cfg.sleep_start_hour || hour < cfg.wake_hour
    }
}

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
///
/// Simplification: uses the activity determined by `now`'s hour for the
/// ENTIRE elapsed duration, not per-minute — a tick spanning a sleep/wake
/// boundary applies one rate uniformly. Acceptable for v1 since ticks are
/// frequent (~60s) relative to the 8h window.
#[allow(dead_code)]
pub fn tick(state: &mut PetState, now: DateTime<Utc>, cfg: &Config) -> Vec<Event> {
    let elapsed = now - state.last_tick;
    if elapsed <= Duration::zero() {
        return Vec::new();
    }

    // Determine the target activity based on `now`'s hour, before applying
    // decay, so the decay math below uses the correct (awake/asleep) rates.
    if state.activity != Activity::Dead {
        state.activity = if state.energy.get() == 0 || is_within_sleep_window(now, cfg) {
            Activity::Asleep
        } else {
            Activity::Awake
        };
    }

    let hours = elapsed.num_seconds() as f64 / 3600.0;

    match state.activity {
        Activity::Dead => {
            // Dead pets don't decay further. Full death handling is a later
            // task; here we simply skip stat decay.
            state.last_tick = now;
            return Vec::new();
        }
        Activity::Asleep => {
            let satiety_loss = ((SATIETY_LOSS_PER_HOUR / 2.0) * hours).round() as u8;
            let hygiene_loss = (HYGIENE_LOSS_PER_HOUR * hours).round() as u8;
            let energy_gain = (ENERGY_GAIN_PER_HOUR_ASLEEP * hours).round() as u8;

            state.satiety = state.satiety - satiety_loss;
            state.hygiene = state.hygiene - hygiene_loss;
            state.energy = state.energy + energy_gain;
        }
        Activity::Awake | Activity::Sick => {
            // TODO(Task 11): sick decay rates
            let satiety_loss = (SATIETY_LOSS_PER_HOUR * hours).round() as u8;
            let energy_loss = (ENERGY_LOSS_PER_HOUR * hours).round() as u8;
            let hygiene_loss = (HYGIENE_LOSS_PER_HOUR * hours).round() as u8;

            state.satiety = state.satiety - satiety_loss;
            state.energy = state.energy - energy_loss;
            state.hygiene = state.hygiene - hygiene_loss;
        }
    }

    // Uncleaned poops drain happiness continuously, independent of
    // awake/asleep — pets don't care about mess while sleeping, they just
    // still smell it. Hygiene itself was already dropped at poop-creation
    // time in `feed`; this is an ongoing happiness penalty for letting
    // poops sit uncleaned.
    if !state.poops.is_empty() {
        let poop_penalty = (state.poops.len() as f64 * hours).round() as u8;
        state.happiness = state.happiness - poop_penalty;
    }

    // Boredom only accrues while the pet is awake and notices idle time.
    if state.activity == Activity::Awake {
        let last_activity = state
            .last_pet_interaction
            .map_or(state.last_command_at, |t| t.max(state.last_command_at));
        let idle = now - last_activity;
        let threshold = Duration::minutes(cfg.boredom_after_minutes as i64);

        if idle > threshold {
            let idle_past_threshold = idle - threshold;
            let quarter_hours = idle_past_threshold.num_minutes() as f64 / 15.0;
            let boredom_gain = (5.0 * quarter_hours).round() as u16;
            state.boredom = Stat::new(boredom_gain);
        } else {
            state.boredom = Stat::new(0);
        }

        if state.boredom.get() > 70 {
            let happiness_loss = (2.0 * (elapsed.num_minutes() as f64 / 15.0)).round() as u8;
            state.happiness = state.happiness - happiness_loss;
        }
    }

    state.last_tick = now;

    Vec::new()
}

/// Cooldown window for `pet_interaction`, to prevent spamming boredom resets.
const PET_INTERACTION_COOLDOWN_MINUTES: i64 = 10;

/// Processes an explicit "pet"/"play" command, resetting boredom to 0.
///
/// Subject to a 10-minute cooldown: if called again before the cooldown
/// elapses (relative to `state.last_pet_interaction`), it's a no-op — the
/// pet's boredom and `last_pet_interaction` are left unchanged.
#[allow(dead_code)]
pub fn pet_interaction(state: &mut PetState, now: DateTime<Utc>) -> Vec<Event> {
    let on_cooldown = state
        .last_pet_interaction
        .is_some_and(|last| now - last < Duration::minutes(PET_INTERACTION_COOLDOWN_MINUTES));
    if on_cooldown {
        return Vec::new();
    }

    state.boredom = Stat::new(0);
    state.last_pet_interaction = Some(now);

    Vec::new()
}

// TODO(future task): implement max_feeds_per_min rate limiting via a
// timestamp ring buffer in PetState (requires a new state field to track
// recent feed timestamps, which is out of scope for this task).

/// Processes a single shell command's exit as a "feeding" event, updating
/// satiety/happiness/streaks and rolling the "bad food" sickness risk.
///
/// Pure like [`tick`]: never reads the clock itself; `event.now` is supplied
/// by the caller. `rng` must be supplied by the caller for deterministic,
/// testable sickness rolls.
#[allow(dead_code)]
pub fn feed(
    state: &mut PetState,
    event: FeedEvent,
    cfg: &Config,
    rng: &mut impl rand::Rng,
) -> Vec<Event> {
    if !state.alive {
        return Vec::new();
    }

    // Commands issued while Asleep don't feed the pet at all. Per the
    // design, they should eventually contribute to a `sleep_disturbance`
    // stat, but that field doesn't exist yet — deferred to a later task.
    if state.activity == Activity::Asleep {
        return Vec::new();
    }

    // Every command counts towards the poop interval, whether or not it
    // carries any nutrition (per design: `commands_eaten` itself is the
    // pooping trigger signal, purely command-count driven — no time
    // component at all).
    state.commands_eaten += 1;
    state.last_command_at = event.now;
    maybe_poop(state, cfg, event.now);

    if cfg.ignored_exit_codes.contains(&event.exit_code) {
        // Neutral command (e.g. Ctrl-C / SIGINT): still "a command", but no
        // nutrition, streak, or bad-food effects.
        return Vec::new();
    }

    if event.exit_code == 0 {
        // Good food: small satiety/happiness gains, builds a success streak.
        state.satiety = state.satiety + 2;
        state.happiness = state.happiness + 1;
        state.success_streak += 1;
        state.failure_streak = 0;

        if state.success_streak.is_multiple_of(10) {
            state.happiness = state.happiness + 5;
        }
    } else {
        // Bad food: more filling, but risks sickness via bad_food_meter.
        state.satiety = state.satiety + 3;
        state.failure_streak += 1;
        state.success_streak = 0;
        state.bad_food_meter += 1;

        let p = (0.05 * state.bad_food_meter as f64).min(0.5);
        if rng.random_bool(p) {
            state.health = state.health - 10;
            state.happiness = state.happiness - 2;
            state.activity = Activity::Sick;
        }
    }

    if cfg.junk_food_commands.iter().any(|c| c == event.argv0) {
        // Junk food is always a bit risky, regardless of exit code.
        state.bad_food_meter += 1;
    }

    Vec::new()
}

/// Checks whether `state.commands_eaten` has just crossed a multiple of
/// `cfg.poop_interval_commands` and, if so, records a new poop and drops
/// hygiene by a flat amount. Commands-eaten driven only — no time
/// component whatsoever, per design.
fn maybe_poop(state: &mut PetState, cfg: &Config, now: DateTime<Utc>) {
    if state.commands_eaten > 0
        && state
            .commands_eaten
            .is_multiple_of(cfg.poop_interval_commands as u64)
    {
        state.poops.push(now);
        state.hygiene = state.hygiene - 15;
    }
}

/// Cleans up all uncleaned poops: empties `state.poops`, maxes out hygiene,
/// and gives a small happiness bonus for a tidy habitat.
#[allow(dead_code)]
pub fn clean(state: &mut PetState) -> Vec<Event> {
    state.poops.clear();
    state.hygiene = Stat::new(Stat::MAX as u16);
    state.happiness = state.happiness + 5;

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::{Activity, Species};
    use crate::pet::stats::Stat;
    use chrono::TimeZone;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

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

    #[test]
    fn sleeps_at_night() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();

        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 1, 23, 30, 0).unwrap(),
            &cfg,
        );

        assert_eq!(state.activity, Activity::Asleep);
    }

    #[test]
    fn energy_climbs_while_asleep() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.energy = Stat::new(50);
        state.last_tick = start;
        let cfg = Config::default();

        tick(&mut state, start + Duration::hours(1), &cfg);

        assert_eq!(state.energy.get(), 58);
        assert_eq!(state.activity, Activity::Asleep);
    }

    #[test]
    fn satiety_decay_halved_while_asleep() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.satiety = Stat::new(70);
        state.last_tick = start;
        let cfg = Config::default();

        tick(&mut state, start + Duration::hours(1), &cfg);

        assert!(
            state.satiety.get() >= 68,
            "expected halved decay (>=68), got {}",
            state.satiety.get()
        );
        assert_eq!(state.activity, Activity::Asleep);
    }

    #[test]
    fn wakes_at_morning() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 6, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.activity = Activity::Asleep;
        let cfg = Config::default();

        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 1, 7, 30, 0).unwrap(),
            &cfg,
        );

        assert_eq!(state.activity, Activity::Awake);
    }

    #[test]
    fn sleep_window_crosses_midnight_correctly() {
        let cfg = Config::default();

        // Hour 23: inside the window (23 -> 7).
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 22, 30, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap(),
            &cfg,
        );
        assert_eq!(state.activity, Activity::Asleep, "hour 23 should be asleep");

        // Hour 3: inside the window (crosses midnight).
        let start = Utc.with_ymd_and_hms(2026, 1, 2, 2, 30, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 2, 3, 0, 0).unwrap(),
            &cfg,
        );
        assert_eq!(state.activity, Activity::Asleep, "hour 3 should be asleep");

        // Hour 12: clearly outside the window.
        let start = Utc.with_ymd_and_hms(2026, 1, 2, 11, 30, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).unwrap(),
            &cfg,
        );
        assert_eq!(state.activity, Activity::Awake, "hour 12 should be awake");
    }

    #[test]
    fn forced_nap_when_energy_zero() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 14, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.energy = Stat::new(0);
        let cfg = Config::default();

        tick(&mut state, start + Duration::minutes(5), &cfg);

        assert_eq!(state.activity, Activity::Asleep);
    }

    fn default_cfg_for_feed() -> Config {
        Config::default()
    }

    #[test]
    fn feed_success_increases_satiety_and_happiness() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed(
            &mut state,
            FeedEvent {
                exit_code: 0,
                argv0: "cargo",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert_eq!(state.satiety.get(), 72);
        assert_eq!(state.happiness.get(), 71);
        assert_eq!(state.success_streak, 1);
        assert_eq!(state.failure_streak, 0);
        assert_eq!(state.commands_eaten, 1);
        assert_eq!(state.bad_food_meter, 0);
    }

    #[test]
    fn feed_ten_success_streak_gives_bonus() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        for _ in 0..10 {
            feed(
                &mut state,
                FeedEvent {
                    exit_code: 0,
                    argv0: "cargo",
                    now: fixed_now(),
                },
                &cfg,
                &mut rng,
            );
        }

        assert_eq!(state.success_streak, 10);
        assert_eq!(state.happiness.get(), 70 + 10 + 5);
    }

    #[test]
    fn feed_failure_raises_satiety_more_and_bumps_bad_food_meter() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        // Seed 0 chosen to not trigger the sickness roll at bad_food_meter=1
        // (p = 0.05), verified empirically.
        let mut rng = StdRng::seed_from_u64(0);

        feed(
            &mut state,
            FeedEvent {
                exit_code: 1,
                argv0: "cargo",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert_eq!(state.satiety.get(), 73);
        assert_eq!(state.failure_streak, 1);
        assert_eq!(state.success_streak, 0);
        assert_eq!(state.bad_food_meter, 1);
        assert_eq!(state.health.get(), 100);
        assert_eq!(state.commands_eaten, 1);
        assert_ne!(state.activity, Activity::Sick);
    }

    #[test]
    fn feed_ignored_exit_code_is_neutral() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed(
            &mut state,
            FeedEvent {
                exit_code: 130,
                argv0: "cargo",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert_eq!(state.satiety.get(), 70);
        assert_eq!(state.happiness.get(), 70);
        assert_eq!(state.bad_food_meter, 0);
        assert_eq!(state.commands_eaten, 1);
    }

    #[test]
    fn feed_junk_food_bumps_meter_regardless_of_exit_code() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed(
            &mut state,
            FeedEvent {
                exit_code: 0,
                argv0: "rm",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert_eq!(state.bad_food_meter, 1);
    }

    #[test]
    fn feed_while_asleep_is_full_noop() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        state.activity = Activity::Asleep;
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed(
            &mut state,
            FeedEvent {
                exit_code: 0,
                argv0: "cargo",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert_eq!(state.commands_eaten, 0);
        assert_eq!(state.satiety.get(), 70);
    }

    #[test]
    fn feed_dead_pet_is_noop() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        state.alive = false;
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed(
            &mut state,
            FeedEvent {
                exit_code: 0,
                argv0: "cargo",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert_eq!(state.commands_eaten, 0);
        assert_eq!(state.satiety.get(), 70);
    }

    #[test]
    fn sickness_can_trigger_with_high_bad_food_meter() {
        let cfg = default_cfg_for_feed();
        let mut triggered = false;

        for seed in 0..50 {
            let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
            state.bad_food_meter = 20; // p = min(0.05*20, 0.5) = 0.5
            let mut rng = StdRng::seed_from_u64(seed);

            feed(
                &mut state,
                FeedEvent {
                    exit_code: 1,
                    argv0: "cargo",
                    now: fixed_now(),
                },
                &cfg,
                &mut rng,
            );

            if state.health.get() < 100 && state.activity == Activity::Sick {
                triggered = true;
                break;
            }
        }

        assert!(
            triggered,
            "expected sickness to trigger at least once across 50 seeds with p=0.5"
        );
    }

    #[test]
    fn boredom_rises_after_idle_threshold() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.last_command_at = start;
        let cfg = Config::default();

        tick(&mut state, start + Duration::minutes(45 + 15), &cfg);

        assert_eq!(state.boredom.get(), 5);
    }

    #[test]
    fn no_boredom_within_threshold() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.last_command_at = start;
        let cfg = Config::default();

        tick(&mut state, start + Duration::minutes(30), &cfg);

        assert_eq!(state.boredom.get(), 0);
    }

    #[test]
    fn pet_interaction_resets_boredom() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.boredom = Stat::new(80);

        pet_interaction(&mut state, start);

        assert_eq!(state.boredom.get(), 0);
        assert_eq!(state.last_pet_interaction, Some(start));
    }

    #[test]
    fn pet_interaction_respects_cooldown() {
        let t0 = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, t0);

        pet_interaction(&mut state, t0);
        state.boredom = Stat::new(80);

        pet_interaction(&mut state, t0 + Duration::minutes(5));

        assert_eq!(state.boredom.get(), 80);
        assert_eq!(state.last_pet_interaction, Some(t0));
    }

    #[test]
    fn high_boredom_drains_happiness_over_time() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.boredom = Stat::new(80);
        state.last_command_at = start - Duration::hours(24);
        let cfg = Config::default();

        tick(&mut state, start + Duration::minutes(15), &cfg);

        assert!(
            state.happiness.get() < 70,
            "expected happiness drain, got {}",
            state.happiness.get()
        );
    }

    #[test]
    fn asleep_pets_dont_accrue_boredom() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.activity = Activity::Asleep;
        state.last_command_at = start - Duration::hours(5);
        let cfg = Config::default();

        tick(&mut state, start + Duration::minutes(30), &cfg);

        assert_eq!(state.boredom.get(), 0);
        assert_eq!(state.activity, Activity::Asleep);
    }

    fn feed_n_successes(state: &mut PetState, cfg: &Config, n: u32) {
        let mut rng = StdRng::seed_from_u64(0);
        for _ in 0..n {
            feed(
                state,
                FeedEvent {
                    exit_code: 0,
                    argv0: "cargo",
                    now: fixed_now(),
                },
                cfg,
                &mut rng,
            );
        }
    }

    #[test]
    fn poop_appears_after_interval_commands() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();

        feed_n_successes(&mut state, &cfg, 40);
        assert_eq!(state.poops.len(), 1);

        feed_n_successes(&mut state, &cfg, 39);
        assert_eq!(state.poops.len(), 1, "should still be 1 at 79 commands");

        feed_n_successes(&mut state, &cfg, 1);
        assert_eq!(state.poops.len(), 2, "should become 2 exactly at 80");
    }

    #[test]
    fn each_poop_drops_hygiene_by_fifteen() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();

        feed_n_successes(&mut state, &cfg, 40);

        assert_eq!(state.hygiene.get(), 100 - 15);
    }

    #[test]
    fn poop_does_not_appear_from_time_alone() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();

        tick(&mut state, start + Duration::hours(24), &cfg);

        assert!(state.poops.is_empty());
    }

    #[test]
    fn uncleaned_poops_persist_across_ticks() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = default_cfg_for_feed();

        feed_n_successes(&mut state, &cfg, 40);
        assert_eq!(state.poops.len(), 1);

        tick(&mut state, start + Duration::hours(1), &cfg);
        assert_eq!(state.poops.len(), 1);

        tick(&mut state, start + Duration::hours(2), &cfg);
        assert_eq!(state.poops.len(), 1);
    }

    #[test]
    fn clean_empties_poops_and_maxes_hygiene() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();

        feed_n_successes(&mut state, &cfg, 80);
        assert_eq!(state.poops.len(), 2);
        assert!(state.hygiene.get() < 100);

        state.happiness = Stat::new(50);
        let happiness_before = state.happiness.get();

        clean(&mut state);

        assert!(state.poops.is_empty());
        assert_eq!(state.hygiene.get(), 100);
        assert_eq!(state.happiness.get(), happiness_before + 5);
    }
}
