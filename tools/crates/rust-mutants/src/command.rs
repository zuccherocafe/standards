//! One shard's cargo-mutants command, built from prepared values only: the folder the tool owns, the selection, the
//! options after `--` and the machine's facts. Nothing here reads the environment or starts a process.
//!
//! Every mutant builds with the Cargo profile `mutants`, defined through Cargo's `--config` on cargo-mutants' build,
//! check and test calls, unless the options after `--` select a profile of their own ([`profile`]). Doctests are
//! skipped and each test binary stops at its first failure, whatever the profile.

use std::ffi::OsString;
use std::fmt;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::arguments::Package;
use crate::machine::Machine;

/// The memory every shard's scope may hold together: four times what a whole run over the standards' own tools needs.
const MEMORY: u64 = 16 * 1024 * 1024 * 1024;

/// The test harness's arguments, through cargo test: stop each test binary at its first failure.
const FAIL_FAST: [&str; 3] = ["--", "-Zunstable-options", "--fail-fast"];

/// Cargo's test targets without the doctests, through cargo test: the library, binaries and integration tests.
const NO_DOCTESTS: &str = "--tests";

/// cargo-mutants' options whose value, given as the next argument, may start with a hyphen, even be `--`.
const HYPHEN_VALUES: [&str; 3] = ["--cargo-arg", "-C", "--cargo-test-arg"];

/// The profile every mutant builds with unless the caller names another.
const PROFILE: &str = "mutants";

/// The profile's settings, as Cargo configuration keys under `profile.mutants`.
const SETTINGS: [&str; 7] = [
    r#"inherits="dev""#,
    "incremental=true",
    "codegen-units=256",
    "opt-level=1",
    r#"debug="none""#,
    r#"package."*".opt-level=3"#,
    r#"rustflags=["-Clink-arg=-fuse-ld=mold"]"#,
];

/// One shard of a run: its index, issued below its count, so `2/2` cannot be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shard {
    index: u32,
    count: NonZeroU32,
}

impl Shard {
    /// Every shard of a run of `count`, in index order.
    pub(crate) fn all(count: NonZeroU32) -> impl Iterator<Item = Self> {
        (0..count.get()).map(move |index| Self { index, count })
    }

    pub(crate) fn index(self) -> u32 {
        self.index
    }
}

impl fmt::Display for Shard {
    /// cargo-mutants' spelling, `index/count`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.index, self.count)
    }
}

/// The one folder under `--out` the tool writes and empties, `rust-mutants/`, and the paths it holds.
#[derive(Clone, Debug)]
pub(crate) struct Work {
    folder: PathBuf,
}

impl Work {
    /// The folder under `out`.
    pub(crate) fn under(out: &Path) -> Self {
        Self {
            folder: out.join("rust-mutants"),
        }
    }

    pub(crate) fn folder(&self) -> &Path {
        &self.folder
    }

    /// Where the change's diff is written.
    pub(crate) fn diff(&self) -> PathBuf {
        self.folder.join("change.diff")
    }

    /// The shard's copy of the repository.
    pub(crate) fn copy(&self, shard: Shard) -> PathBuf {
        self.folder.join(format!("copy-{}", shard.index))
    }

    /// The shard's output folder, which holds cargo-mutants' `mutants.out`.
    pub(crate) fn output(&self, shard: Shard) -> PathBuf {
        self.folder.join(format!("shard-{}", shard.index))
    }
}

/// What cargo-mutants is asked to mutate.
#[derive(Clone, Debug)]
pub(crate) enum Selection {
    /// The diff written to this file, in every workspace member.
    Diff(PathBuf),
    /// Every function in the crate.
    Package(Package),
}

/// Everything prepared before the shards start.
#[derive(Debug)]
pub(crate) struct Prepared {
    pub(crate) work: Work,
    /// The workspace's folder relative to the repository's root, as it is in each copy.
    pub(crate) workspace_in_repository: PathBuf,
    pub(crate) selection: Selection,
}

impl Prepared {
    /// The workspace's folder in the shard's copy.
    pub(crate) fn workspace(&self, shard: Shard) -> PathBuf {
        self.work.copy(shard).join(&self.workspace_in_repository)
    }
}

/// The prefix that runs a shard of `count` in a user scope holding its part of the memory cap, with no swap; a
/// process the kernel kills there for memory leaves the rest of the shard running.
pub(crate) fn scope(count: NonZeroU32) -> Vec<OsString> {
    let memory = MEMORY / u64::from(count.get());
    [
        "systemd-run",
        "--user",
        "--scope",
        "--quiet",
        "--property",
        &format!("MemoryMax={memory}"),
        "--property",
        "MemorySwapMax=0",
        "--property",
        "OOMPolicy=continue",
        "--",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

/// Whether the options `passed` to cargo-mutants select a profile. An option that takes a hyphenated value consumes
/// the argument after it, even `--`; a `--` no option consumes ends cargo-mutants' options, and what follows is cargo
/// test's.
fn names_profile(passed: &[OsString]) -> bool {
    let mut options = passed.iter().map(|option| option.as_encoded_bytes());
    while let Some(option) = options.next() {
        if option == b"--" {
            return false;
        }
        if option == b"--profile" || option.starts_with(b"--profile=") {
            return true;
        }
        if HYPHEN_VALUES.iter().any(|takes| option == takes.as_bytes()) {
            options.next();
        }
    }
    false
}

/// cargo-mutants' options that define the tool's profile for its build, check and test calls and build with it, or
/// none when the options `passed` to cargo-mutants select a profile of their own.
fn profile(passed: &[OsString]) -> Vec<OsString> {
    if names_profile(passed) {
        return Vec::new();
    }
    let mut options = vec![OsString::from(
        "--cargo-arg=--config=unstable.profile-rustflags=true",
    )];
    options.extend(
        SETTINGS
            .iter()
            .map(|setting| format!("--cargo-arg=--config=profile.{PROFILE}.{setting}").into()),
    );
    options.extend(["--profile", PROFILE].map(OsString::from));
    options
}

/// One shard's cargo-mutants command, run in the shard's copy of the workspace with its own target folder: in a
/// capped scope when the machine has one, with its share of the CPUs as jobserver tasks, and with each test binary in
/// its own PID namespace when the machine can make one. `passed` follows the tool's own options unchanged.
pub(crate) fn command(
    passed: &[OsString],
    prepared: &Prepared,
    shard: Shard,
    machine: &Machine,
) -> Command {
    let mut words: Vec<OsString> = if machine.capped {
        scope(shard.count)
    } else {
        Vec::new()
    };
    words.extend(["cargo", "mutants"].map(OsString::from));
    match &prepared.selection {
        Selection::Diff(diff) => {
            words.extend(["--in-diff".into(), diff.into(), "--workspace".into()]);
        }
        Selection::Package(package) => {
            words.extend(["--package".into(), package.as_os_str().to_owned()]);
        }
    }
    let shards = NonZeroUsize::try_from(shard.count).expect("a u32 fits in a usize here");
    let tasks = (machine.cpus.get() / shards).max(1);
    words.extend([
        "--in-place".into(),
        "--shard".into(),
        shard.to_string().into(),
        "--output".into(),
        prepared.work.output(shard).into(),
        "--jobserver-tasks".into(),
        tasks.to_string().into(),
    ]);
    words.extend(profile(passed));
    words.push(format!("--cargo-test-arg={NO_DOCTESTS}").into());
    words.extend(
        FAIL_FAST
            .iter()
            .map(|argument| format!("--cargo-test-arg={argument}").into()),
    );
    words.extend(passed.iter().cloned());
    let (program, arguments) = words.split_first().expect("the command has a program");
    let workspace = prepared.workspace(shard);
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env("CARGO_TARGET_DIR", workspace.join("target"))
        .current_dir(workspace);
    if let Some((name, value)) = machine.runner.variable() {
        command.env(name, value);
    }
    command
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;
    use crate::machine::Runner;

    /// The options that build every mutant with the tool's profile, linked by mold, without doctests.
    const FAST: [&str; 11] = [
        "--cargo-arg=--config=unstable.profile-rustflags=true",
        r#"--cargo-arg=--config=profile.mutants.inherits="dev""#,
        "--cargo-arg=--config=profile.mutants.incremental=true",
        "--cargo-arg=--config=profile.mutants.codegen-units=256",
        "--cargo-arg=--config=profile.mutants.opt-level=1",
        r#"--cargo-arg=--config=profile.mutants.debug="none""#,
        r#"--cargo-arg=--config=profile.mutants.package."*".opt-level=3"#,
        r#"--cargo-arg=--config=profile.mutants.rustflags=["-Clink-arg=-fuse-ld=mold"]"#,
        "--profile",
        "mutants",
        "--cargo-test-arg=--tests",
    ];

    const FAIL_FAST_ARGUMENTS: [&str; 3] = [
        "--cargo-test-arg=--",
        "--cargo-test-arg=-Zunstable-options",
        "--cargo-test-arg=--fail-fast",
    ];

    fn count(count: u32) -> NonZeroU32 {
        NonZeroU32::new(count).expect("not zero")
    }

    fn shard(index: usize, of: u32) -> Shard {
        Shard::all(count(of)).nth(index).expect("in range")
    }

    fn machine(cpus: usize, capped: bool) -> Machine {
        Machine {
            cpus: NonZeroUsize::new(cpus).expect("not zero"),
            capped,
            runner: Runner::Uncontained,
        }
    }

    fn package(name: &str) -> Prepared {
        Prepared {
            work: Work::under(Path::new("/report")),
            workspace_in_repository: PathBuf::from("rust"),
            selection: Selection::Package(Package::from(OsString::from(name))),
        }
    }

    fn passed(options: &[&str]) -> Vec<OsString> {
        options.iter().map(OsString::from).collect()
    }

    fn strings(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    /// The command's program and arguments, as one list.
    fn built(command: &Command) -> Vec<OsString> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(OsStr::to_owned)
            .collect()
    }

    #[test]
    fn diff_mode_mutates_every_member_in_place_in_its_capped_shard_with_its_share_of_the_cpus() {
        let prepared = Prepared {
            selection: Selection::Diff(PathBuf::from("/report/rust-mutants/change.diff")),
            ..package("unused")
        };
        let command = command(
            &passed(&["--timeout", "60"]),
            &prepared,
            shard(1, 2),
            &machine(16, true),
        );
        let mut expected = vec![
            "systemd-run",
            "--user",
            "--scope",
            "--quiet",
            "--property",
            "MemoryMax=8589934592",
            "--property",
            "MemorySwapMax=0",
            "--property",
            "OOMPolicy=continue",
            "--",
            "cargo",
            "mutants",
            "--in-diff",
            "/report/rust-mutants/change.diff",
            "--workspace",
            "--in-place",
            "--shard",
            "1/2",
            "--output",
            "/report/rust-mutants/shard-1",
            "--jobserver-tasks",
            "8",
        ];
        expected.extend(FAST);
        expected.extend(FAIL_FAST_ARGUMENTS);
        expected.extend(["--timeout", "60"]);
        assert_eq!(built(&command), strings(&expected));
        assert_eq!(
            command.get_current_dir(),
            Some(Path::new("/report/rust-mutants/copy-1/rust"))
        );
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            [(
                OsStr::new("CARGO_TARGET_DIR"),
                Some(OsStr::new("/report/rust-mutants/copy-1/rust/target"))
            )]
        );
    }

    #[test]
    fn package_mode_names_the_crate_and_an_uncapped_shard_runs_cargo_mutants_itself() {
        let command = command(&[], &package("core"), shard(0, 3), &machine(2, false));
        let mut expected = vec![
            "cargo",
            "mutants",
            "--package",
            "core",
            "--in-place",
            "--shard",
            "0/3",
            "--output",
            "/report/rust-mutants/shard-0",
            "--jobserver-tasks",
            "1",
        ];
        expected.extend(FAST);
        expected.extend(FAIL_FAST_ARGUMENTS);
        assert_eq!(built(&command), strings(&expected));
    }

    #[test]
    fn a_profile_the_caller_names_turns_the_tools_profile_off_and_keeps_the_test_options() {
        for options in [
            &["--profile", "release"][..],
            &["--profile=release"],
            &["--profile", "mutants"],
            &["--no-shuffle", "--profile", "release"],
        ] {
            let command = command(
                &passed(options),
                &package("core"),
                shard(0, 2),
                &machine(2, false),
            );
            let mut expected = vec![
                "cargo",
                "mutants",
                "--package",
                "core",
                "--in-place",
                "--shard",
                "0/2",
                "--output",
                "/report/rust-mutants/shard-0",
                "--jobserver-tasks",
                "1",
                "--cargo-test-arg=--tests",
            ];
            expected.extend(FAIL_FAST_ARGUMENTS);
            expected.extend(options);
            assert_eq!(built(&command), strings(&expected), "{options:?}");
        }
    }

    #[test]
    fn a_separator_an_option_takes_as_its_value_does_not_hide_the_callers_profile() {
        let options = passed(&["--cargo-test-arg", "--", "--profile", "release"]);
        let command = command(&options, &package("core"), shard(0, 2), &machine(2, false));
        let built = built(&command);
        assert_eq!(built.iter().filter(|word| *word == "--profile").count(), 1);
        assert_eq!(built[built.len() - 4..], options);
        assert!(
            !built
                .iter()
                .any(|word| word.as_encoded_bytes().starts_with(b"--cargo-arg="))
        );
    }

    #[test]
    fn a_profile_option_meant_for_cargo_test_leaves_the_tools_profile_in_place() {
        let command = command(
            &passed(&["--", "--profile=release"]),
            &package("core"),
            shard(0, 2),
            &machine(2, false),
        );
        let built = built(&command);
        let at = built
            .iter()
            .position(|word| word == "--profile")
            .expect("a profile");
        assert_eq!(built[at..at + 2], strings(&["--profile", "mutants"]));
    }
}
