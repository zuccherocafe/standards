//! Typecheck enum control flow and nullable text access.
//!
//! The core accepts source text and computes reports.
//! The CLI owns filesystem access; the core reads no files, clocks or environment.

pub mod ast;
pub mod infer;
pub mod parser;
