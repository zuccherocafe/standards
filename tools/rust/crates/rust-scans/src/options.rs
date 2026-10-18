//! The command line, parsed once into checked values: crate names, kernel module paths and unit suffixes are each a
//! type whose constructor refuses what cannot name one, so clap reports a malformed argument and exits 2 before any
//! work starts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;

/// The command line as clap reads it, gathered into [`Options`] by [`Arguments::into_parts`].
#[derive(Debug, Parser)]
#[command(
    name = "rust-scans",
    version,
    about = "The Rust standard's report scans over a Cargo workspace's source; they report and fail nothing",
    long_about = LONG_ABOUT
)]
pub(crate) struct Arguments {
    /// The folder holding the workspace Cargo.toml.
    #[arg(long, value_name = "FOLDER", default_value = ".")]
    workspace: Utf8PathBuf,
    /// A core crate, which H1 and H4 read; repeat for each.
    #[arg(long = "core", value_name = "CRATE", value_parser = CrateName::new)]
    cores: Vec<CrateName>,
    /// A crate's kernel modules, which H2 leaves out, each a path under the crate's src/ such as arms/sqp; repeat
    /// for each crate, and a crate named twice keeps both lists.
    #[arg(long = "kernel", value_name = "CRATE=MODULE[,MODULE...]", value_parser = kernel)]
    kernels: Vec<(CrateName, Vec<ModulePath>)>,
    /// A unit suffix H2 reads besides metres, seconds, degrees, radians and hz; repeat for each.
    #[arg(long = "unit", value_name = "SUFFIX", value_parser = UnitSuffix::new)]
    units: Vec<UnitSuffix>,
}

const LONG_ABOUT: &str = "\
The Rust standard's report scans over each workspace member's src/, read as Rust syntax:

  H1  file, process, environment and clock access in a core crate
  H2  a primitive field or parameter named with a unit suffix, outside kernels and test code
  H3  a code in an #[error(...)] message
  H4  an async runtime, network or async I/O path, or async syntax, in a core crate

Each finding is printed as `<scan> <path>:<line>: <what>`, with paths relative to the current folder, then a count \
per scan and the time taken. Findings in test code are marked in H1 and H4. The scans fail nothing: the exit status \
is 0 whatever they find, 1 when the workspace cannot be read, and 2 for a malformed argument. Needs `cargo` on PATH.";

impl Arguments {
    /// The workspace folder, which only discovery reads, and the options the scans take.
    pub(crate) fn into_parts(self) -> (Utf8PathBuf, Options) {
        let mut kernels: BTreeMap<CrateName, Vec<ModulePath>> = BTreeMap::new();
        for (name, modules) in self.kernels {
            kernels.entry(name).or_default().extend(modules);
        }
        let options = Options {
            cores: self.cores.into_iter().collect(),
            kernels,
            units: self.units,
        };
        (self.workspace, options)
    }
}

/// What the scans take from the command line: the core crates, each crate's kernel modules and the project's units.
#[derive(Debug, Default)]
pub(crate) struct Options {
    cores: BTreeSet<CrateName>,
    kernels: BTreeMap<CrateName, Vec<ModulePath>>,
    units: Vec<UnitSuffix>,
}

impl Options {
    /// Whether the crate was named as a core.
    pub(crate) fn is_core(&self, name: &CrateName) -> bool {
        self.cores.contains(name)
    }

    /// The crate's kernel modules; none for a crate no `--kernel` named.
    pub(crate) fn kernels(&self, name: &CrateName) -> &[ModulePath] {
        self.kernels.get(name).map_or(&[], Vec::as_slice)
    }

    /// The project's unit suffixes, read besides the built-in ones.
    pub(crate) fn units(&self) -> &[UnitSuffix] {
        &self.units
    }
}

/// A package's name, as Cargo prints it: letters, digits, `-` and `_`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CrateName(Box<str>);

impl CrateName {
    /// # Errors
    /// [`BadName`] for an empty name or one with another character.
    pub(crate) fn new(name: &str) -> Result<Self, BadName> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(BadName(name.into()));
        }
        Ok(Self(name.into()))
    }
}

impl fmt::Display for CrateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A module's path under its crate's `src/`, as `/`-joined identifiers such as `arms/sqp`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ModulePath(Vec<Box<str>>);

impl ModulePath {
    /// # Errors
    /// [`BadModule`] for an empty path or a segment that is not an identifier.
    pub(crate) fn parse(text: &str) -> Result<Self, BadModule> {
        let segments: Option<Vec<Box<str>>> = text
            .split('/')
            .map(|segment| identifier(segment).then(|| segment.into()))
            .collect();
        segments.map(Self).ok_or_else(|| BadModule(text.into()))
    }

    /// The path the module's file has under `src` without its `.rs`, which is also its folder.
    pub(crate) fn under(&self, src: &Utf8Path) -> Utf8PathBuf {
        let mut path = src.to_owned();
        path.extend(self.0.iter().map(AsRef::<str>::as_ref));
        path
    }
}

/// Whether `text` is a Rust identifier: a letter or `_` then letters, digits and `_`, and not `_` alone.
fn identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
        && text != "_"
}

/// A suffix a name ends with, after `_`, to say its unit, such as `metres` in `margin_metres`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct UnitSuffix(Box<str>);

impl UnitSuffix {
    /// # Errors
    /// [`BadUnit`] for an empty suffix or one with a character other than a lowercase letter or a digit.
    pub(crate) fn new(text: &str) -> Result<Self, BadUnit> {
        if text.is_empty()
            || !text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(BadUnit(text.into()));
        }
        Ok(Self(text.into()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One `--kernel` value, `<crate>=<module>[,<module>...]`.
fn kernel(text: &str) -> Result<(CrateName, Vec<ModulePath>), BadKernel> {
    let (name, modules) = text
        .split_once('=')
        .ok_or_else(|| BadKernel::Shape(text.into()))?;
    let modules = modules
        .split(',')
        .map(ModulePath::parse)
        .collect::<Result<_, _>>()?;
    Ok((CrateName::new(name)?, modules))
}

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a crate name: one or more letters, digits, `-` and `_`")]
pub(crate) struct BadName(Box<str>);

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a module path: identifiers joined by `/`, such as arms/sqp")]
pub(crate) struct BadModule(Box<str>);

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a unit suffix: one or more lowercase letters and digits")]
pub(crate) struct BadUnit(Box<str>);

#[derive(Debug, thiserror::Error)]
enum BadKernel {
    #[error("`{0}` is not <crate>=<module>[,<module>...]")]
    Shape(Box<str>),
    #[error(transparent)]
    Name(#[from] BadName),
    #[error(transparent)]
    Module(#[from] BadModule),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(arguments: &[&str]) -> Result<Options, clap::Error> {
        let arguments = Arguments::try_parse_from(
            std::iter::once("rust-scans").chain(arguments.iter().copied()),
        )?;
        Ok(arguments.into_parts().1)
    }

    fn name(text: &str) -> CrateName {
        CrateName::new(text).expect("a crate name")
    }

    fn module(text: &str) -> ModulePath {
        ModulePath::parse(text).expect("a module path")
    }

    #[test]
    fn the_workspace_defaults_to_the_current_folder() {
        let arguments = Arguments::try_parse_from(["rust-scans"]).expect("no argument is required");
        assert_eq!(arguments.into_parts().0, Utf8PathBuf::from("."));
    }

    #[test]
    fn cores_repeat_and_kernels_for_one_crate_accumulate_with_nested_paths() {
        let options = options(&[
            "--core",
            "toy-core",
            "--core",
            "toy_vocab",
            "--kernel",
            "toy-core=qp,arms/sqp",
            "--kernel",
            "toy-core=roots",
        ])
        .expect("valid arguments");
        assert!(options.is_core(&name("toy-core")) && options.is_core(&name("toy_vocab")));
        assert!(!options.is_core(&name("toy-shell")));
        assert_eq!(
            options.kernels(&name("toy-core")),
            [module("qp"), module("arms/sqp"), module("roots")]
        );
        assert_eq!(options.kernels(&name("toy-shell")), []);
    }

    #[test]
    fn a_kernel_missing_its_crate_or_modules_is_a_usage_error() {
        for value in [
            "toy-core",
            "=qp",
            "toy-core=",
            "toy-core=qp,,roots",
            "toy-core=arms/",
            "toy-core=a-b",
        ] {
            let error = options(&["--kernel", value]).expect_err(value);
            assert_eq!(error.exit_code(), 2, "{value}");
        }
    }

    #[test]
    fn a_crate_name_is_letters_digits_hyphens_and_underscores() {
        assert!(CrateName::new("tdoll-vocab_2").is_ok());
        assert!(CrateName::new("").is_err());
        assert!(CrateName::new("toy core").is_err());
    }

    #[test]
    fn a_module_path_is_identifiers_joined_by_slashes_under_src() {
        assert_eq!(
            module("arms/sqp").under(Utf8Path::new("src")),
            Utf8PathBuf::from("src/arms/sqp")
        );
        for bad in ["", "arms//sqp", "_", "1arms", "arms.rs"] {
            assert!(ModulePath::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_unit_is_lowercase_letters_and_digits_and_units_repeat() {
        let options = options(&["--unit", "newtons", "--unit", "m2"]).expect("valid units");
        let units: Vec<&str> = options.units().iter().map(UnitSuffix::as_str).collect();
        assert_eq!(units, ["newtons", "m2"]);
        for bad in ["", "Newtons", "per_second"] {
            assert!(UnitSuffix::new(bad).is_err(), "{bad}");
        }
    }
}
