//! The machine's facts, discovered once before any command is built: how many CPUs it has, whether a memory-capped
//! user scope can be started, and whether each test binary can run in a PID namespace of its own.
//!
//! Each fact is found by trying it. A scope is available when `systemd-run --user --scope` runs `true`. Test binaries
//! are contained when rustc names its host, whose Cargo runner variable sets the runner, and `unshare` runs `true` in a
//! new PID namespace. A probe that cannot start counts as unavailable, and the run goes on without that fact.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::Path;
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

/// Where Linux lists the CPUs online, as ranges such as `0-3,8-11`.
const ONLINE: &str = "/sys/devices/system/cpu/online";

/// What the machine offers the shards.
#[derive(Clone, Debug)]
pub(crate) struct Machine {
    /// The CPUs online, shared among the shards as jobserver tasks.
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
    /// The facts, found by trying each; rustc runs in `workspace`, so the workspace's toolchain names the host.
    pub(crate) fn discover(workspace: &Path) -> Self {
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

/// The CPUs online; where Linux does not list them, the CPUs this process may use; failing both, one.
fn cpus() -> NonZeroUsize {
    fs::read_to_string(ONLINE)
        .ok()
        .and_then(|list| online(&list))
        .or_else(|| thread::available_parallelism().ok())
        .unwrap_or(NonZeroUsize::MIN)
}

/// How many CPUs a list of ranges such as `0-3,8-11` names, or none for one that is malformed or names none.
fn online(ranges: &str) -> Option<NonZeroUsize> {
    let mut count = 0_usize;
    for range in ranges.trim().split(',') {
        let (first, last) = range.split_once('-').unwrap_or((range, range));
        let (first, last): (usize, usize) = (first.parse().ok()?, last.parse().ok()?);
        count = count.checked_add(last.checked_sub(first)?.checked_add(1)?)?;
    }
    NonZeroUsize::new(count)
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

    #[test]
    fn the_cpus_online_are_counted_from_their_ranges() {
        assert_eq!(online("0-15\n").map(NonZeroUsize::get), Some(16));
        assert_eq!(online("0\n").map(NonZeroUsize::get), Some(1));
        assert_eq!(online("0-3,8-11,14\n").map(NonZeroUsize::get), Some(9));
        assert_eq!(online("3-1\n"), None);
        assert_eq!(online("\n"), None);
    }
}
