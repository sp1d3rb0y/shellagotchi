use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

/// A bounded stat value, clamped to the range 0..=100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[allow(dead_code)]
pub struct Stat(u8);

#[allow(dead_code)]
impl Stat {
    pub const MAX: u8 = 100;

    /// Creates a new `Stat`, clamping the given value to 0..=100.
    pub fn new(v: u16) -> Self {
        Self(v.min(Self::MAX as u16) as u8)
    }

    /// Returns the inner value.
    pub fn get(&self) -> u8 {
        self.0
    }
}

impl Add<u8> for Stat {
    type Output = Stat;

    fn add(self, rhs: u8) -> Stat {
        Stat::new(self.0 as u16 + rhs as u16)
    }
}

impl Sub<u8> for Stat {
    type Output = Stat;

    fn sub(self, rhs: u8) -> Stat {
        Stat(self.0.saturating_sub(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_clamps_at_bounds() {
        assert_eq!(Stat::new(200).get(), 100);
        assert_eq!((Stat::new(3) - 10).get(), 0);
        assert_eq!((Stat::new(98) + 10).get(), 100);
    }
    #[test]
    fn stat_serde_roundtrip() {
        let s: Stat = serde_json::from_str("42").unwrap();
        assert_eq!(serde_json::to_string(&s).unwrap(), "42");
    }
}
