//! Quantities, directions, sides and names more than one tdollhouse crate shares, over plain values; it never reads a
//! file, a clock or the engine, and knows no other workspace crate.
#![forbid(unsafe_code)]
#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#![warn(clippy::indexing_slicing)]

mod angle;
mod direction;
mod length;
mod name;
mod side;
mod time;

pub use angle::{Degrees, NotFinite, Radians};
pub use direction::{Direction, Horizontal, NotHorizontal, NotUnit};
pub use length::{InvalidLength, Length, Metres};
pub use name::{BoneName, EmptyName};
pub use side::{PerSide, Side};
pub use time::{InvalidSeconds, Rate, Seconds, ZeroRate};
