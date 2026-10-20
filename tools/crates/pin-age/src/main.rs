//! Says which toolchain pins are due to move: one line for each pin published more than 30 days ago, and nothing when
//! none is due (rules/general.md, Toolchain). It advises and never fails on a pin's age, since a check that failed on
//! age would fail an old commit on a later date.
//!
//! ```text
//! pin-age [--toolchain <rust-toolchain.toml>]... [--lock <flake.lock>]
//! ```
//!
//! A toolchain file's dated nightly, `nightly-YYYY-MM-DD`, was published that day; a numbered release carries no date
//! and is not aged ([`toolchain`]). A lock's pins are the direct inputs of its root node, each dated by its
//! `locked.lastModified` as a UTC day; an input that follows another is that other input's pin, and a node without
//! `lastModified`, such as a path input, has no date ([`lock`]). Paths are read and named with their `.` components,
//! repeated separators and trailing separator removed ([`arguments`]); any bytes the system takes are accepted.
//!
//! Exits 0 whatever the ages. A file that cannot be read or parsed, or a channel that is not a pin, exits 2 with one
//! line naming the file and field: that wiring is broken on any date. A malformed command line also exits 2. The one
//! clock read is today's UTC day, in [`main`].

mod arguments;
mod date;
mod error;
mod lock;
mod pins;
mod toolchain;

use std::io::{self, Write as _};
use std::process::ExitCode;

use crate::arguments::Arguments;
use crate::date::Date;
use crate::error::PinError;
use crate::lock::lock_pins;
use crate::pins::advice;
use crate::toolchain::toolchain_pin;

/// What begins each line the tool prints.
const LEAD: &str = "pin-age: ";

/// The exit status for a pin file that cannot be used.
const BROKEN: u8 = 2;

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
    match run(&arguments, Date::today()) {
        Ok(advice) => match io::stdout().write_all(advice.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                let _ = writeln!(io::stderr(), "{LEAD}could not print the advice: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            let _ = writeln!(io::stderr(), "{LEAD}{error}");
            ExitCode::from(BROKEN)
        }
    }
}

/// The advice on `today` for every pin the arguments name, the toolchain files in the order given and then the lock;
/// refused for the first file that cannot be used.
fn run(arguments: &Arguments, today: Date) -> Result<String, PinError> {
    let mut pins = Vec::new();
    for path in &arguments.toolchain {
        pins.extend(toolchain_pin(path)?);
    }
    if let Some(lock) = &arguments.lock {
        pins.extend(lock_pins(lock)?);
    }
    Ok(advice(&pins, today))
}

/// A new, empty folder in the system's temporary folder for one test, removed with everything in it when dropped. A
/// folder is taken by making it, since a test binary in a PID namespace of its own can share its process number with
/// another.
#[cfg(test)]
pub(crate) struct Scratch(std::path::PathBuf);

#[cfg(test)]
impl Scratch {
    pub(crate) fn new(name: &str) -> Self {
        let mut number = 0_u32;
        loop {
            let folder = std::env::temp_dir()
                .join(format!("pin-age-{}-{number}-{name}", std::process::id()));
            match std::fs::create_dir(&folder) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => number += 1,
                made => {
                    made.expect("the temporary folder is writable");
                    return Self(folder);
                }
            }
        }
    }

    /// Writes `text` to the file `name` in the folder and gives its path.
    pub(crate) fn write(&self, name: &str, text: &str) -> std::path::PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).expect("the scratch folder is writable");
        path
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(test)]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A toolchain file pinning `channel`.
#[cfg(test)]
pub(crate) fn toolchain(channel: &str) -> String {
    format!("[toolchain]\nchannel = \"{channel}\"\ncomponents = [\"rustfmt\", \"clippy\"]\n")
}

/// A lock whose root node, named `root`, has the `inputs` object, beside the other nodes, each a name and its object,
/// in the order given.
#[cfg(test)]
pub(crate) fn lock(root: &str, inputs: &str, nodes: &[(&str, &str)]) -> String {
    use std::fmt::Write as _;
    let mut text = format!("{{\"nodes\": {{\"{root}\": {{\"inputs\": {inputs}}}");
    for (name, node) in nodes {
        write!(text, ", \"{name}\": {node}").expect("a String takes any text");
    }
    write!(text, "}}, \"root\": \"{root}\", \"version\": 7}}").expect("a String takes any text");
    text
}

/// A locked node whose `lastModified` is the JSON `last_modified`.
#[cfg(test)]
pub(crate) fn node(last_modified: &str) -> String {
    format!("{{\"locked\": {{\"lastModified\": {last_modified}, \"type\": \"github\"}}}}")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// 2023-11-14 22:13:20 UTC.
    const LATE_ON_THE_14TH: &str = "1700000000";

    fn new_years_day_2024() -> Date {
        Date::new(2024, 1, 1).expect("a real day")
    }

    fn arguments(toolchains: &[&PathBuf], lock: Option<&PathBuf>) -> Arguments {
        Arguments {
            toolchain: toolchains.iter().map(|&path| path.clone()).collect(),
            lock: lock.cloned(),
        }
    }

    #[test]
    fn an_old_nightly_and_an_old_input_each_get_one_line() {
        let folder = Scratch::new("old");
        let old = folder.write("rust-toolchain.toml", &toolchain("nightly-2020-01-01"));
        let flake = folder.write(
            "flake.lock",
            &lock(
                "root",
                r#"{"nixpkgs": "nixpkgs"}"#,
                &[("nixpkgs", &node(LATE_ON_THE_14TH))],
            ),
        );
        assert_eq!(
            run(&arguments(&[&old], Some(&flake)), new_years_day_2024()).expect("read"),
            format!(
                "pin-age: the Rust nightly-2020-01-01 in {} is 1461 days old; move it (rules/general.md, Toolchain)\n\
                 pin-age: flake input nixpkgs in {} is 48 days old; move it (rules/general.md, Toolchain)\n",
                old.display(),
                flake.display()
            )
        );
    }

    #[test]
    fn a_nightly_thirty_days_old_and_a_numbered_release_print_nothing() {
        let folder = Scratch::new("recent");
        let recent = folder.write("recent.toml", &toolchain("nightly-2023-12-02"));
        let release = folder.write("release.toml", &toolchain("1.92.0"));
        assert_eq!(
            run(&arguments(&[&recent, &release], None), new_years_day_2024()).expect("read"),
            ""
        );
    }

    #[test]
    fn a_broken_pin_file_is_refused_naming_it_and_nothing_is_advised() {
        let folder = Scratch::new("broken");
        let old = folder.write("old.toml", &toolchain("nightly-2020-01-01"));
        let stable = folder.write("stable.toml", &toolchain("stable"));
        assert_eq!(
            run(&arguments(&[&old, &stable], None), new_years_day_2024())
                .expect_err("refused")
                .to_string(),
            format!(
                "{}: [toolchain] channel: \"stable\" is not a dated nightly or a numbered release",
                stable.display()
            )
        );
    }
}
