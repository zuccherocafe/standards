//! Comparisons of the two sides. [`measure`] compares measures by name, with their directions and checked arithmetic.
//! [`image`] compares images by name, and lists every missing, unexpected and unreadable image before the differences.

pub(crate) mod image;
pub(crate) mod measure;
