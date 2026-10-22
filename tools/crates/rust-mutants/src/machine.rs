//! The machine's facts, discovered once before any command is built: the Cargo cargo-mutants runs, how many CPUs the
//! tool may run on, whether a memory-capped user scope can be started, and whether each test binary can run in a PID
//! namespace of its own.
//!
//! The Cargo is the first `cargo` on PATH, the one `cargo mutants` would have run, found as a program search finds it
//! ([`cargo`]). The run is refused without one.
//!
//! Each fact is found by trying it. A scope is available when `systemd-run --user --scope` runs `true`. Test binaries
//! are contained when rustc names its host, whose Cargo runner variable sets the runner, and `unshare` runs `true` in a
//! new PID namespace. A probe that cannot start counts as unavailable, and the run goes on without that fact.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::num::{NonZeroU32, NonZeroUsize};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;

use crate::command::scope;

/// Runs a program as the first process of a new PID namespace, as root of a user namespace mapped to the caller, with
/// its own `/proc`; killing `unshare` kills the program and so the whole namespace.
const NAMESPACE: [&str; 7] = [
    "unshare",
    "--user",
    "--map-root-user",
    "--pid",
    "--fork",
    "--mount-proc",
    "--kill-child",
];

/// What the machine offers the shards.
#[derive(Clone, Debug)]
pub(crate) struct Machine {
    /// The Cargo cargo-mutants runs, given to it as `CARGO`, as Cargo gives each of its subcommands.
    pub(crate) cargo: PathBuf,
    /// The CPUs this process may run on, shared among the shards as jobserver tasks.
    pub(crate) cpus: NonZeroUsize,
    /// Whether each shard runs in a memory-capped user scope.
    pub(crate) capped: bool,
    pub(crate) runner: Runner,
}

/// How each test binary runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Runner {
    /// As Cargo starts it, through any runner the environment already sets.
    Uncontained,
    /// In a PID namespace of its own, through Cargo's test runner for `host`, around the runner `existing` the
    /// environment already sets for it, if any.
    Namespaced {
        host: String,
        existing: Option<OsString>,
    },
}

impl Machine {
    /// The facts, found by trying each, with the Cargo already found; rustc runs in `workspace`, so the workspace's
    /// toolchain names the host.
    pub(crate) fn discover(workspace: &Path, cargo: PathBuf) -> Self {
        let mut scoped = scope(NonZeroU32::MIN);
        scoped.push("true".into());
        let capped = runs(&scoped);
        let mut namespaced: Vec<OsString> = NAMESPACE.map(OsString::from).into();
        namespaced.push("true".into());
        let runner =
            host(workspace)
                .filter(|_| runs(&namespaced))
                .map_or(Runner::Uncontained, |host| {
                    let existing =
                        env::var_os(runner_variable(&host)).filter(|runner| !runner.is_empty());
                    Runner::Namespaced { host, existing }
                });
        Self {
            cargo,
            cpus: cpus(),
            capped,
            runner,
        }
    }
}

impl Runner {
    /// Cargo's test runner variable for the host and the runner it is set to; none when uncontained.
    pub(crate) fn variable(&self) -> Option<(String, OsString)> {
        match self {
            Self::Uncontained => None,
            Self::Namespaced { host, existing } => {
                let mut runner = OsString::from(NAMESPACE.join(" "));
                if let Some(existing) = existing {
                    runner.push(" ");
                    runner.push(existing);
                }
                Some((runner_variable(host), runner))
            }
        }
    }
}

/// The first `cargo` in the folders of `path`, a PATH value, that the caller may run, as a program search finds it, with
/// a relative folder taken from `current`; none when no folder holds one. Its links are kept, so a rustup proxy stays
/// one and picks its toolchain as it would have.
pub(crate) fn cargo(path: &OsStr, current: &Path) -> Option<PathBuf> {
    env::split_paths(path)
        .map(|folder| current.join(folder).join("cargo"))
        .find(|file| {
            fs::metadata(file)
                .is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
        })
}

/// Cargo's test runner variable for the target `triple`.
fn runner_variable(triple: &str) -> String {
    format!(
        "CARGO_TARGET_{}_RUNNER",
        triple.to_uppercase().replace(['-', '.'], "_")
    )
}

/// Whether the command `words` starts and exits 0.
fn runs(words: &[OsString]) -> bool {
    let Some((program, arguments)) = words.split_first() else {
        return false;
    };
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The target triple the workspace's rustc builds for by default, or none when rustc cannot say.
fn host(workspace: &Path) -> Option<String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .current_dir(workspace)
        .output()
        .ok()?;
    host_of(&output)
}

/// The host `rustc -vV` names in its `output`, or none when it failed or names none.
fn host_of(output: &Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|host| host.trim().to_owned())
}

/// The CPUs this process may run on, its affinity, as the standard library counts them; failing that, one. A caller
/// that gives the tool part of the machine, as dandegate does a step, so gives its shards that part.
fn cpus() -> NonZeroUsize {
    thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::ExitStatus;

    use super::*;

    fn rustc(code: i32, printed: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: printed.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn cargo_is_the_first_one_on_path_that_runs() {
        let root = crate::scratch("cargo-on-path");
        for (folder, mode) in [("skipped", 0o644), ("first", 0o755), ("second", 0o755)] {
            let file = root.join(folder).join("cargo");
            fs::create_dir(root.join(folder)).expect("the scratch folder is writable");
            fs::write(&file, "").expect("the scratch folder is writable");
            fs::set_permissions(&file, fs::Permissions::from_mode(mode)).expect("the file is ours");
        }
        fs::create_dir(root.join("empty")).expect("the scratch folder is writable");
        let path = env::join_paths(
            ["empty", "skipped", "first", "second"].map(|folder| root.join(folder)),
        )
        .expect("the folders join");
        assert_eq!(cargo(&path, Path::new("/")), Some(root.join("first/cargo")));
        let relative = env::join_paths(["skipped", "second"]).expect("the folders join");
        assert_eq!(cargo(&relative, &root), Some(root.join("second/cargo")));
        assert_eq!(
            cargo(&root.join("empty").into_os_string(), Path::new("/")),
            None
        );
        fs::remove_dir_all(root).expect("the scratch folder is ours");
    }

    #[test]
    fn the_host_is_rustcs_and_unknown_when_rustc_cannot_say() {
        let version =
            "rustc 1.100.0-nightly\nhost: x86_64-unknown-linux-gnu\nrelease: 1.100.0-nightly\n";
        assert_eq!(
            host_of(&rustc(0, version)).as_deref(),
            Some("x86_64-unknown-linux-gnu")
        );
        assert_eq!(host_of(&rustc(1, version)), None);
        assert_eq!(host_of(&rustc(0, "rustc 1.100.0-nightly\n")), None);
    }

    #[test]
    fn the_test_runner_runs_each_binary_in_its_own_namespace_around_any_runner_already_set() {
        let namespace = "unshare --user --map-root-user --pid --fork --mount-proc --kill-child";
        let name = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER";
        let namespaced = |host: &str, existing: Option<&str>| Runner::Namespaced {
            host: host.to_owned(),
            existing: existing.map(OsString::from),
        };
        assert_eq!(
            namespaced("x86_64-unknown-linux-gnu", None).variable(),
            Some((name.to_owned(), namespace.into()))
        );
        assert_eq!(
            namespaced("x86_64-unknown-linux-gnu", Some("valgrind -q")).variable(),
            Some((name.to_owned(), format!("{namespace} valgrind -q").into()))
        );
        assert_eq!(
            namespaced("thumbv7em-none-eabihf", None).variable(),
            Some((
                "CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER".to_owned(),
                namespace.into()
            ))
        );
        assert_eq!(Runner::Uncontained.variable(), None);
    }
}
