//! Injectable clock abstraction.
//!
//! Engine logic must never read wall-clock time directly (enforced by
//! `clippy.toml`'s `disallowed-methods` for `chrono::Utc::now` /
//! `chrono::Local::now`). Instead, code should depend on the [`Clock`] trait,
//! which is implemented by [`SystemClock`] for production use and by
//! [`FakeClock`] for deterministic tests.

use std::cell::Cell;

use chrono::{DateTime, Duration, Utc};

/// Abstraction over "now" so callers can be tested deterministically.
pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

/// The crate's single sanctioned wall-clock read site.
pub struct SystemClock;

impl Clock for SystemClock {
    // This is the ONLY place in the whole crate allowed to call `Utc::now()`
    // directly; everywhere else must go through the injected `Clock` trait.
    #[allow(clippy::disallowed_methods)]
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A controllable clock for deterministic tests.
pub struct FakeClock(Cell<DateTime<Utc>>);

impl FakeClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self(Cell::new(start))
    }

    /// Move the internal time forward by `duration`.
    pub fn advance(&self, duration: Duration) {
        let current = self.0.get();
        self.0.set(current + duration);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn fake_clock_advances_deterministically() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let clock = FakeClock::new(start);
        assert_eq!(clock.now(), start);
        clock.advance(Duration::hours(3));
        assert_eq!(clock.now(), start + Duration::hours(3));
    }
}
