//! Calendar days: the proleptic Gregorian calendar from 0001-01-01 to 9999-12-31, and whole days between them.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds in a UTC day; a lock's times count no leap seconds.
const SECONDS_PER_DAY: i128 = 86_400;

/// The first and last days the calendar holds, counted from 1970-01-01.
const FIRST: i32 = days_from_civil(1, 1, 1);
const LAST: i32 = days_from_civil(9999, 12, 31);

/// A day from 0001-01-01 to 9999-12-31, held as days since 1970-01-01.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Date(i32);

impl Date {
    /// The day `year`-`month`-`day`, or `None` for a day the calendar does not have or a year outside 1 to 9999.
    pub(crate) fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let length = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        ((1..=9999).contains(&year) && (1..=length).contains(&day)).then(|| {
            Self(days_from_civil(
                i32::from(year),
                i32::from(month),
                i32::from(day),
            ))
        })
    }

    /// The UTC day holding the instant `seconds` after 1970-01-01T00:00:00Z, or `None` outside the calendar.
    pub(crate) fn from_unix_seconds(seconds: i128) -> Option<Self> {
        i32::try_from(seconds.div_euclid(SECONDS_PER_DAY))
            .ok()
            .filter(|days| (FIRST..=LAST).contains(days))
            .map(Self)
    }

    /// Today in UTC, by the system clock.
    ///
    /// # Panics
    ///
    /// When the clock is before 1970 or after 9999.
    pub(crate) fn today() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the system clock is after 1970");
        Self::from_unix_seconds(now.as_secs().into()).expect("the system clock is before 10000")
    }

    /// Whole days from `earlier` to this day: negative when `earlier` is the later day.
    pub(crate) fn days_since(self, earlier: Self) -> i32 {
        self.0 - earlier.0
    }
}

/// Days from 1970-01-01 to `year`-`month`-`day`, for a real day in years 1 to 9999. Years are counted in 400-year
/// eras starting each 1 March, so the leap day is the last day of its year.
const fn days_from_civil(year: i32, month: i32, day: i32) -> i32 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: u16, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("a real day")
    }

    #[test]
    fn a_day_the_calendar_lacks_is_none() {
        for (year, month, day) in [
            (2026, 2, 30),
            (2026, 4, 31),
            (2026, 13, 1),
            (2026, 0, 1),
            (2026, 1, 0),
            (2100, 2, 29),
            (0, 1, 1),
            (10_000, 1, 1),
        ] {
            assert_eq!(Date::new(year, month, day), None, "{year}-{month}-{day}");
        }
        for (year, month, day) in [(2024, 2, 29), (2000, 2, 29), (1, 1, 1), (9999, 12, 31)] {
            assert!(
                Date::new(year, month, day).is_some(),
                "{year}-{month}-{day}"
            );
        }
    }

    #[test]
    fn days_count_across_months_leap_days_and_years() {
        assert_eq!(day(1970, 1, 1), Date(0));
        assert_eq!(day(2024, 3, 1).days_since(day(2024, 2, 28)), 2);
        assert_eq!(day(2023, 3, 1).days_since(day(2023, 2, 28)), 1);
        assert_eq!(day(2026, 3, 1).days_since(day(2025, 12, 31)), 60);
        assert_eq!(day(2024, 1, 1).days_since(day(2020, 1, 1)), 1461);
        assert_eq!(day(2001, 1, 1).days_since(day(2000, 1, 1)), 366);
        assert_eq!(day(2101, 1, 1).days_since(day(2100, 1, 1)), 365);
        assert_eq!(day(2020, 1, 1).days_since(day(2024, 1, 1)), -1461);
    }

    #[test]
    fn an_instant_falls_on_its_utc_day_and_one_outside_the_calendar_on_none() {
        for (seconds, expected) in [
            (0, Some(day(1970, 1, 1))),
            (-1, Some(day(1969, 12, 31))),
            (1_700_000_000, Some(day(2023, 11, 14))),
            (1_700_006_400, Some(day(2023, 11, 15))),
            (-62_135_596_800, Some(day(1, 1, 1))),
            (-62_135_596_801, None),
            (253_402_300_799, Some(day(9999, 12, 31))),
            (253_402_300_800, None),
            (i128::from(u64::MAX), None),
        ] {
            assert_eq!(Date::from_unix_seconds(seconds), expected, "{seconds}");
        }
    }
}
