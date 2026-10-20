//! Pins, which of them are due to move on a day, and the advice that says so.

use std::fmt::{self, Write as _};

use crate::LEAD;
use crate::date::Date;

/// A pin is due once it is older than this many days: toolchains move monthly.
const DUE_AFTER_DAYS: u32 = 30;

/// One pinned toolchain or input and the day it was published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pin {
    /// The pin and the file it is in, as the advice names them.
    what: String,
    published: Date,
}

impl Pin {
    pub(crate) fn new(what: String, published: Date) -> Self {
        Self { what, published }
    }
}

/// A whole number of days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Days(u32);

impl fmt::Display for Days {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A pin over 30 days old; only [`due`] makes one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Due<'a> {
    pin: &'a Pin,
    age: Days,
}

impl fmt::Display for Due<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is {} days old; move it (rules/general.md, Toolchain)",
            self.pin.what, self.age
        )
    }
}

/// The pins published more than 30 days before `today`, in the order given. A pin published after `today` is not due.
pub(crate) fn due(pins: &[Pin], today: Date) -> impl Iterator<Item = Due<'_>> {
    pins.iter().filter_map(move |pin| {
        let age = u32::try_from(today.days_since(pin.published)).ok()?;
        (age > DUE_AFTER_DAYS).then_some(Due {
            pin,
            age: Days(age),
        })
    })
}

/// What the tool prints on `today` for `pins`: a line for each due pin, in the order given, or nothing.
pub(crate) fn advice(pins: &[Pin], today: Date) -> String {
    let mut advice = String::new();
    for due in due(pins, today) {
        writeln!(advice, "{LEAD}{due}").expect("a String takes any text");
    }
    advice
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: u16, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("a real day")
    }

    fn pin(what: &str, published: Date) -> Pin {
        Pin::new(what.to_owned(), published)
    }

    #[test]
    fn a_pin_is_due_only_after_thirty_days() {
        let thirty = pin("thirty", day(2026, 9, 1));
        let thirty_one = pin("thirty-one", day(2026, 8, 31));
        let pins = [thirty, thirty_one];
        assert_eq!(
            due(&pins, day(2026, 10, 1)).collect::<Vec<_>>(),
            [Due {
                pin: &pins[1],
                age: Days(31)
            }]
        );
    }

    #[test]
    fn the_age_counts_days_across_a_year() {
        let old = pin("old", day(2025, 12, 31));
        assert_eq!(
            due(std::slice::from_ref(&old), day(2026, 3, 1)).collect::<Vec<_>>(),
            [Due {
                pin: &old,
                age: Days(60)
            }]
        );
    }

    #[test]
    fn a_pin_published_after_today_is_not_due() {
        assert_eq!(
            due(&[pin("future", day(2026, 12, 1))], day(2026, 10, 1)).collect::<Vec<_>>(),
            []
        );
    }

    #[test]
    fn the_advice_is_a_line_for_each_due_pin_in_the_order_given() {
        let pins = [
            pin("the Rust nightly-2026-01-01 in a.toml", day(2026, 1, 1)),
            pin("flake input recent in flake.lock", day(2026, 9, 30)),
            pin("flake input nixpkgs in flake.lock", day(2025, 12, 31)),
        ];
        assert_eq!(
            advice(&pins, day(2026, 10, 1)),
            "pin-age: the Rust nightly-2026-01-01 in a.toml is 273 days old; move it (rules/general.md, Toolchain)\n\
             pin-age: flake input nixpkgs in flake.lock is 274 days old; move it (rules/general.md, Toolchain)\n"
        );
        assert_eq!(advice(&pins[1..2], day(2026, 10, 1)), "");
    }
}
