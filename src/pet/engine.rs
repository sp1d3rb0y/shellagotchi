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
pub enum Event {}

/// A single shell command completion, fed to [`feed`] to update the pet's
/// stats. One `FeedEvent` corresponds to one shell command's exit.
pub struct FeedEvent<'a> {
    pub exit_code: i32,
    pub argv0: &'a str,
    pub now: DateTime<Utc>,
}

/// Per-hour decay rates for time-based stat loss.
const SATIETY_LOSS_PER_HOUR: f64 = 3.0;
const ENERGY_LOSS_PER_HOUR: f64 = 2.0;
const HYGIENE_LOSS_PER_HOUR: f64 = 1.0;
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
pub fn tick(
    state: &mut PetState,
    now: DateTime<Utc>,
    cfg: &Config,
    rng: &mut impl rand::Rng,
) -> Vec<Event> {
    let elapsed = now - state.last_tick;
    if elapsed <= Duration::zero() {
        return Vec::new();
    }

    // Cure check: a Sick pet recovers once hygiene and satiety are both
    // healthy again (a proxy for "the owner cleaned up and fed the pet").
    // This must run before the sleep/wake reassignment below, so a freshly
    // cured pet is correctly re-evaluated as Awake/Asleep by hour rather
    // than staying pinned to a now-stale Sick value.
    if state.activity == Activity::Sick && state.hygiene.get() > 80 && state.satiety.get() > 60 {
        state.activity = Activity::Awake;
    }

    // Determine the target activity based on `now`'s hour, before applying
    // decay, so the decay math below uses the correct (awake/asleep) rates.
    // Dead and (still) Sick pets are never overridden by this hour-based
    // logic — Sick persists until explicitly cured above, and Dead is
    // terminal.
    if state.activity != Activity::Dead && state.activity != Activity::Sick {
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

    // Poop-driven sickness: an independent risk source from the bad-food
    // sickness roll in `feed`. Once at least 3 uncleaned poops have piled
    // up, each tick rolls a probability proportional to both the pile size
    // and elapsed hours, capped so it never becomes a certainty.
    if state.poops.len() >= 3 {
        let p = (0.05 * state.poops.len() as f64 * hours).min(0.9);
        if rng.random_bool(p) {
            state.health = state.health - 5;
            state.activity = Activity::Sick;
        }
    }

    // Ongoing health drain while Sick, compounding with any acute hit from
    // the poop-driven roll above in the same tick.
    if state.activity == Activity::Sick {
        state.health = state.health - (5.0 * hours).round() as u8;
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

    // Death: either the poop-driven acute hit or the ongoing sick drain
    // above (or both, compounded) may have brought health to 0.
    if state.health.get() == 0 {
        state.alive = false;
        state.activity = Activity::Dead;
    }

    Vec::new()
}

/// Size of each simulated chunk in [`catch_up`]'s replay loop.
///
/// Note: this is deliberately 1 hour, not the 5-minute granularity a naive
/// reading of "5-minute chunked replay" might suggest. `tick()` rounds its
/// per-call decay to the nearest whole stat point; at the current decay
/// rates (e.g. 3/hour satiety), a 5-minute slice is only 0.25 points, which
/// rounds to *zero* on every single call — chunking that finely would
/// silently produce no decay at all, no matter how long the gap. Hourly
/// chunks avoid that rounding trap (whole-hour decay amounts round
/// losslessly) while still being fine-grained enough to correctly
/// re-evaluate the sleep/wake window per chunk, since that determination
/// (`is_within_sleep_window`) is itself hour-of-day based.
const CATCH_UP_CHUNK: Duration = Duration::hours(1);

/// Advances `state` from `state.last_tick` to `now`, handling large gaps
/// (e.g. laptop suspend, daemon downtime) by capping the simulated elapsed
/// time at `cfg.max_offline_hours` and replaying it in coarse chunks rather
/// than a single naive tick. This prevents a week-long gap from either
/// nuking the pet's stats in one shot or requiring absurd numbers of tiny
/// ticks.
///
/// Two-level capping:
/// 1. The total amount of elapsed time actually *simulated* is capped at
///    `cfg.max_offline_hours` (or unlimited when `None`), protecting the pet
///    from unrealistic multi-day decay in one shot.
/// 2. Whatever amount is simulated is replayed via repeated `tick()` calls
///    in [`CATCH_UP_CHUNK`]-sized chunks, so sleep/wake windows within the
///    gap are correctly re-evaluated per chunk rather than using a single
///    hour-of-day determination for the whole span.
///
/// Regardless of capping, `state.last_tick` is always set to the real `now`
/// at the end — a capped (discarded) portion of the gap is never re-offered
/// to a subsequent `catch_up`/`tick` call.
pub fn catch_up(
    state: &mut PetState,
    now: DateTime<Utc>,
    cfg: &Config,
    rng: &mut impl rand::Rng,
) -> Vec<Event> {
    let real_elapsed = now - state.last_tick;
    if real_elapsed <= Duration::zero() {
        return Vec::new();
    }

    let simulated_elapsed = match cfg.max_offline_hours {
        Some(max_h) => {
            let cap = Duration::milliseconds((max_h * 3600.0 * 1000.0) as i64);
            real_elapsed.min(cap)
        }
        None => real_elapsed,
    };

    let effective_now = state.last_tick + simulated_elapsed;

    let mut events = Vec::new();
    let mut checkpoint = state.last_tick;
    while checkpoint < effective_now {
        checkpoint = (checkpoint + CATCH_UP_CHUNK).min(effective_now);
        events.extend(tick(state, checkpoint, cfg, rng));
    }

    state.last_tick = now;

    events
}

/// Cooldown window for `pet_interaction`, to prevent spamming boredom resets.
const PET_INTERACTION_COOLDOWN_MINUTES: i64 = 10;

/// Processes an explicit "pet"/"play" command, resetting boredom to 0.
///
/// Subject to a 10-minute cooldown: if called again before the cooldown
/// elapses (relative to `state.last_pet_interaction`), it's a no-op — the
/// pet's boredom and `last_pet_interaction` are left unchanged.
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

/// Processes a single shell command's exit as a "feeding" event, updating
/// satiety/happiness/streaks and rolling the "bad food" sickness risk.
///
/// Pure like [`tick`]: never reads the clock itself; `event.now` is supplied
/// by the caller. `rng` must be supplied by the caller for deterministic,
/// testable sickness rolls.
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

    if cfg.clean_commands.iter().any(|c| c == event.argv0) {
        // Running `clean` (or a configured alias) from the shell always
        // tidies up the pet's habitat, regardless of the command's own
        // exit code -- even `clean: command not found` (exit 127) still
        // clears the mess. The happiness bonus, however, is only awarded
        // when the command genuinely succeeded.
        apply_clean(state, event.exit_code == 0);
    }

    // Death: the bad-food acute health hit above may have brought health
    // to 0.
    if state.health.get() == 0 {
        state.alive = false;
        state.activity = Activity::Dead;
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

/// Empties `state.poops` and maxes out hygiene, always; optionally applies
/// the small "tidy habitat" happiness bonus on top.
///
/// Shared by the explicit standalone [`clean`] (which always awards the
/// bonus) and the argv0-triggered auto-clean path inside [`feed`] (which
/// only awards the bonus when the triggering command actually succeeded).
fn apply_clean(state: &mut PetState, award_happiness_bonus: bool) {
    state.poops.clear();
    state.hygiene = Stat::new(Stat::MAX as u16);
    if award_happiness_bonus {
        state.happiness = state.happiness + 5;
    }
}

/// Cleans up all uncleaned poops: empties `state.poops`, maxes out hygiene,
/// and gives a small happiness bonus for a tidy habitat.
///
/// This is the explicit, user-invoked path (`shellagotchi clean` / the IPC
/// `Clean` op) and always awards the happiness bonus, unconditionally.
pub fn clean(state: &mut PetState) -> Vec<Event> {
    apply_clean(state, true);

    Vec::new()
}

/// Produces a brand-new newborn pet, replacing a dead one.
///
/// This is a thin wrapper around [`PetState::newborn`] rather than a call
/// site inlining it directly, so that "how a pet comes into existence"
/// stays a single, auditable engine-level concern alongside `feed`/`tick`/
/// `clean` -- callers (the IPC server) are responsible for the *policy*
/// question of when hatching is allowed (only once the previous pet is
/// dead) and for archiving the old state; this function only knows how to
/// construct the new one.
pub fn hatch(name: String, species: crate::pet::state::Species, now: DateTime<Utc>) -> PetState {
    PetState::newborn(name, species, now)
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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::hours(1), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::seconds(1), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);
        let now = start + Duration::hours(2);

        tick(&mut state, now, &cfg, &mut rng);
        let satiety_after_first = state.satiety.get();
        let energy_after_first = state.energy.get();
        let hygiene_after_first = state.hygiene.get();

        tick(&mut state, now, &cfg, &mut rng);

        assert_eq!(state.satiety.get(), satiety_after_first);
        assert_eq!(state.energy.get(), energy_after_first);
        assert_eq!(state.hygiene.get(), hygiene_after_first);
    }

    #[test]
    fn sleeps_at_night() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 1, 23, 30, 0).unwrap(),
            &cfg,
            &mut rng,
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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::hours(1), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::hours(1), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 1, 7, 30, 0).unwrap(),
            &cfg,
            &mut rng,
        );

        assert_eq!(state.activity, Activity::Awake);
    }

    #[test]
    fn sleep_window_crosses_midnight_correctly() {
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        // Hour 23: inside the window (23 -> 7).
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 22, 30, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap(),
            &cfg,
            &mut rng,
        );
        assert_eq!(state.activity, Activity::Asleep, "hour 23 should be asleep");

        // Hour 3: inside the window (crosses midnight).
        let start = Utc.with_ymd_and_hms(2026, 1, 2, 2, 30, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 2, 3, 0, 0).unwrap(),
            &cfg,
            &mut rng,
        );
        assert_eq!(state.activity, Activity::Asleep, "hour 3 should be asleep");

        // Hour 12: clearly outside the window.
        let start = Utc.with_ymd_and_hms(2026, 1, 2, 11, 30, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).unwrap(),
            &cfg,
            &mut rng,
        );
        assert_eq!(state.activity, Activity::Awake, "hour 12 should be awake");
    }

    #[test]
    fn forced_nap_when_energy_zero() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 14, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.energy = Stat::new(0);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::minutes(5), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(
            &mut state,
            start + Duration::minutes(45 + 15),
            &cfg,
            &mut rng,
        );

        assert_eq!(state.boredom.get(), 5);
    }

    #[test]
    fn no_boredom_within_threshold() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.last_command_at = start;
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::minutes(30), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::minutes(15), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::minutes(30), &cfg, &mut rng);

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
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::hours(24), &cfg, &mut rng);

        assert!(state.poops.is_empty());
    }

    #[test]
    fn uncleaned_poops_persist_across_ticks() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed_n_successes(&mut state, &cfg, 40);
        assert_eq!(state.poops.len(), 1);

        tick(&mut state, start + Duration::hours(1), &cfg, &mut rng);
        assert_eq!(state.poops.len(), 1);

        tick(&mut state, start + Duration::hours(2), &cfg, &mut rng);
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

    #[test]
    fn feed_with_clean_argv0_cleans_pet_on_success() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed_n_successes(&mut state, &cfg, 40);
        assert_eq!(state.poops.len(), 1);
        assert!(state.hygiene.get() < 100);

        // Pin happiness away from the ceiling so the expected +6 delta
        // (success-food +1, clean bonus +5) is actually observable rather
        // than swallowed by the Stat::MAX clamp.
        state.happiness = Stat::new(50);
        let happiness_before = state.happiness.get();

        feed(
            &mut state,
            FeedEvent {
                exit_code: 0,
                argv0: "clean",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        assert!(state.poops.is_empty());
        assert_eq!(state.hygiene.get(), 100);
        // Success-food gain (+1) plus the clean bonus (+5).
        assert_eq!(state.happiness.get(), happiness_before + 1 + 5);
    }

    #[test]
    fn feed_with_clean_argv0_cleans_but_no_bonus_on_failure() {
        let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
        let cfg = default_cfg_for_feed();
        let mut rng = StdRng::seed_from_u64(0);

        feed_n_successes(&mut state, &cfg, 40);
        assert_eq!(state.poops.len(), 1);
        assert!(state.hygiene.get() < 100);

        let happiness_before = state.happiness.get();

        feed(
            &mut state,
            FeedEvent {
                exit_code: 127,
                argv0: "clean",
                now: fixed_now(),
            },
            &cfg,
            &mut rng,
        );

        // Clean effect applies unconditionally, even on a failing exit code.
        assert!(state.poops.is_empty());
        assert_eq!(state.hygiene.get(), 100);
        // No clean happiness bonus was awarded -- happiness should be
        // unchanged from the pre-feed value (a bad-food failure carries no
        // happiness delta of its own, only satiety/failure_streak/bad_food_meter
        // effects, which don't touch happiness).
        assert_eq!(state.happiness.get(), happiness_before);
    }

    // --- Task 11: sickness (poop-driven), ongoing sick drain, cure, death ---

    #[test]
    fn poop_sickness_can_trigger_with_many_poops() {
        let cfg = Config::default();
        let mut triggered = false;

        for seed in 0..50 {
            let start = fixed_now();
            let mut state = PetState::newborn("T".into(), Species::Blob, start);
            state.poops = vec![start, start, start, start, start];
            let mut rng = StdRng::seed_from_u64(seed);

            tick(&mut state, start + Duration::hours(1), &cfg, &mut rng);

            if state.activity == Activity::Sick && state.health.get() < 100 {
                triggered = true;
                break;
            }
        }

        assert!(
            triggered,
            "expected poop-driven sickness to trigger at least once across 50 seeds"
        );
    }

    #[test]
    fn sick_pet_loses_health_over_time() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.activity = Activity::Sick;
        state.health = Stat::new(50);
        state.hygiene = Stat::new(50);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::hours(2), &cfg, &mut rng);

        assert!(
            state.health.get() < 50,
            "expected sick health drain, got {}",
            state.health.get()
        );
    }

    #[test]
    fn sick_activity_is_not_overridden_by_sleep_wake_logic() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.activity = Activity::Sick;
        state.hygiene = Stat::new(50);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(
            &mut state,
            Utc.with_ymd_and_hms(2026, 1, 2, 8, 0, 0).unwrap(),
            &cfg,
            &mut rng,
        );

        assert_eq!(state.activity, Activity::Sick);
    }

    #[test]
    fn sick_pet_cured_when_hygiene_and_satiety_recover() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.activity = Activity::Sick;
        state.hygiene = Stat::new(90);
        state.satiety = Stat::new(70);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::minutes(5), &cfg, &mut rng);

        assert_eq!(state.activity, Activity::Awake);
    }

    #[test]
    fn death_when_health_reaches_zero_via_tick() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        state.activity = Activity::Sick;
        state.health = Stat::new(3);
        state.hygiene = Stat::new(50);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        tick(&mut state, start + Duration::hours(3), &cfg, &mut rng);

        assert_eq!(state.health.get(), 0);
        assert!(!state.alive);
        assert_eq!(state.activity, Activity::Dead);
    }

    #[test]
    fn death_when_health_reaches_zero_via_feed() {
        let cfg = default_cfg_for_feed();
        let mut triggered = false;

        for seed in 0..50 {
            let mut state = PetState::newborn("T".into(), Species::Blob, fixed_now());
            state.health = Stat::new(5);
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

            if !state.alive {
                assert_eq!(state.activity, Activity::Dead);
                assert_eq!(state.health.get(), 0);
                triggered = true;
                break;
            }
        }

        assert!(
            triggered,
            "expected death via feed() to trigger at least once across 50 seeds"
        );
    }

    // --- catch_up tests (Task 12) ---

    #[test]
    fn small_gap_behaves_like_normal_tick() {
        let start = fixed_now();
        let cfg = Config::default();

        let mut via_catch_up = PetState::newborn("T".into(), Species::Blob, start);
        let mut rng1 = StdRng::seed_from_u64(0);
        catch_up(
            &mut via_catch_up,
            start + Duration::hours(1),
            &cfg,
            &mut rng1,
        );

        let mut via_tick = PetState::newborn("T".into(), Species::Blob, start);
        let mut rng2 = StdRng::seed_from_u64(0);
        tick(&mut via_tick, start + Duration::hours(1), &cfg, &mut rng2);

        assert_eq!(via_catch_up.satiety.get(), via_tick.satiety.get());
        assert_eq!(via_catch_up.energy.get(), via_tick.energy.get());
        assert_eq!(via_catch_up.hygiene.get(), via_tick.hygiene.get());
        assert_eq!(via_catch_up.last_tick, via_tick.last_tick);
    }

    #[test]
    fn gap_is_capped_at_max_offline_hours() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config {
            max_offline_hours: Some(12.0),
            ..Config::default()
        };
        let mut rng = StdRng::seed_from_u64(0);

        let now = start + Duration::hours(24 * 3);
        catch_up(&mut state, now, &cfg, &mut rng);

        assert_eq!(state.last_tick, now);
        assert!(
            state.satiety.get() > 0,
            "expected only ~12h of decay to be applied, got satiety {}",
            state.satiety.get()
        );
    }

    #[test]
    fn max_offline_hours_none_disables_cap() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config {
            max_offline_hours: None,
            ..Config::default()
        };
        let mut rng = StdRng::seed_from_u64(0);

        let now = start + Duration::hours(20);
        catch_up(&mut state, now, &cfg, &mut rng);

        assert_eq!(state.last_tick, now);
        assert!(
            state.satiety.get() < 70,
            "expected decay over the full uncapped 20h gap, got satiety {}",
            state.satiety.get()
        );
    }

    #[test]
    fn nights_within_gap_are_simulated_as_sleep() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config {
            max_offline_hours: Some(30.0),
            ..Config::default()
        };
        let mut rng = StdRng::seed_from_u64(0);

        let now = start + Duration::hours(24);
        catch_up(&mut state, now, &cfg, &mut rng);

        assert_eq!(state.last_tick, now);
        assert!(
            state.energy.get() > 22,
            "expected the ~8h night window within the gap to be simulated as \
             sleep (boosting energy), but energy was {} (<=22 would suggest \
             the pet was awake the whole time)",
            state.energy.get()
        );
    }

    #[test]
    fn zero_or_negative_gap_is_noop() {
        let start = fixed_now();
        let mut state = PetState::newborn("T".into(), Species::Blob, start);
        let cfg = Config::default();
        let mut rng = StdRng::seed_from_u64(0);

        let before = state.clone();
        catch_up(&mut state, start, &cfg, &mut rng);

        assert_eq!(state, before);
    }

    #[test]
    fn hatch_produces_a_fresh_newborn() {
        let now = fixed_now();
        let hatched = hatch("Rusty".into(), Species::Dragon, now);

        assert_eq!(
            hatched,
            PetState::newborn("Rusty".into(), Species::Dragon, now)
        );
        assert!(hatched.alive);
        assert_eq!(hatched.activity, Activity::Awake);
        assert_eq!(hatched.satiety.get(), 70);
        assert_eq!(hatched.health.get(), 100);
        assert!(hatched.poops.is_empty());
    }
}
