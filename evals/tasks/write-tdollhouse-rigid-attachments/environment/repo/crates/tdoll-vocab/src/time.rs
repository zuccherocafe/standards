//! Durations in seconds and sample rates in whole samples a second.

use std::fmt;
use std::num::NonZeroU32;
use std::time::Duration;

/// A duration in seconds: a time constant, a window, a time from a clip's first sample. Finite, at least zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Seconds(f64);

impl Seconds {
    /// `seconds` as a duration; none unless finite and at least zero. `-0.0` is accepted as `0.0`, so no duration
    /// carries a sign bit.
    #[must_use]
    pub const fn new(seconds: f64) -> Option<Self> {
        if seconds.is_finite() && seconds >= 0.0 {
            Some(Self(seconds.abs()))
        } else {
            None
        }
    }

    /// The seconds.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// How many samples this lasts at `rate`, unrounded: finite unless the duration is beyond any clip's.
    #[must_use]
    pub fn samples(self, rate: Rate) -> f64 {
        self.0 * f64::from(rate.hz())
    }
}

impl TryFrom<f64> for Seconds {
    type Error = InvalidSeconds;

    fn try_from(seconds: f64) -> Result<Self, InvalidSeconds> {
        Self::new(seconds).ok_or(InvalidSeconds)
    }
}

impl From<Seconds> for f64 {
    fn from(seconds: Seconds) -> Self {
        seconds.0
    }
}

impl From<Duration> for Seconds {
    /// A measured duration, which is finite and at least zero whatever its length.
    fn from(duration: Duration) -> Self {
        Self(duration.as_secs_f64())
    }
}

/// A duration that is negative or not finite, refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSeconds;

impl fmt::Display for InvalidSeconds {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a duration is a finite number of seconds, at least zero")
    }
}

impl std::error::Error for InvalidSeconds {}

/// Samples a second: a whole number above zero, with no upper bound of its own (a solver's limit is its own outcome).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u32", into = "u32"))]
pub struct Rate(NonZeroU32);

impl Rate {
    /// `hz` as a rate; none for zero.
    #[must_use]
    pub const fn new(hz: u32) -> Option<Self> {
        match NonZeroU32::new(hz) {
            Some(hz) => Some(Self(hz)),
            None => None,
        }
    }

    /// The samples a second.
    #[must_use]
    pub const fn hz(self) -> u32 {
        self.0.get()
    }

    /// The time of the `sample`th sample after the first, which any count over a rate above zero keeps finite and at
    /// least zero.
    #[must_use]
    pub fn time_of(self, sample: usize) -> Seconds {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count above 2^53 rounds to the nearest f64, still finite; below it the time is exact"
        )]
        let sample = sample as f64;
        Seconds(sample / f64::from(self.hz()))
    }
}

impl TryFrom<u32> for Rate {
    type Error = ZeroRate;

    fn try_from(hz: u32) -> Result<Self, ZeroRate> {
        Self::new(hz).ok_or(ZeroRate)
    }
}

impl From<Rate> for u32 {
    fn from(rate: Rate) -> Self {
        rate.hz()
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} Hz", self.hz())
    }
}

/// A rate of zero samples a second, refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZeroRate;

impl fmt::Display for ZeroRate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a rate is at least one sample a second")
    }
}

impl std::error::Error for ZeroRate {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Rate, Seconds, ZeroRate};

    #[test]
    fn a_rate_is_a_whole_number_of_samples_a_second_above_zero() {
        assert_eq!(Rate::new(60).map(Rate::hz), Some(60));
        assert_eq!(Rate::new(60).unwrap().to_string(), "60 Hz");
        assert_eq!(Rate::try_from(0), Err(ZeroRate));
        assert_eq!(u32::from(Rate::try_from(u32::MAX).unwrap()), u32::MAX);
    }

    #[test]
    fn a_sample_lies_its_count_over_the_rate_after_the_first() {
        let rate = Rate::new(30).unwrap();
        assert_eq!(rate.time_of(0), Seconds::new(0.0).unwrap());
        assert_eq!(rate.time_of(45).get(), 1.5);
        assert_eq!(
            Rate::new(1).unwrap().time_of(usize::MAX).get(),
            2.0_f64.powi(64)
        );
    }

    #[test]
    fn a_measured_duration_is_its_seconds() {
        assert_eq!(Seconds::from(Duration::from_millis(1500)).get(), 1.5);
        assert!(Seconds::from(Duration::MAX).get().is_finite());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_duration_is_read_as_its_seconds_and_a_negative_one_is_refused() {
        use serde::Deserialize as _;
        use serde::de::value::{Error, F64Deserializer};
        let read = |seconds| Seconds::deserialize(F64Deserializer::<Error>::new(seconds));
        assert_eq!(read(0.5).unwrap(), Seconds::new(0.5).unwrap());
        assert!(read(-0.5).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_rate_is_read_as_its_hertz_and_zero_is_refused() {
        use serde::Deserialize as _;
        use serde::de::value::{Error, U32Deserializer};
        let read = |hz| Rate::deserialize(U32Deserializer::<Error>::new(hz));
        assert_eq!(read(30).unwrap(), Rate::new(30).unwrap());
        assert!(read(0).is_err());
    }
}
