//! A small Git repository in the system's temporary folder, stand-ins for the programs the binary runs, and the binary
//! run against them.
//!
//! The repository's workspace is `rust/`, below its root, and its crate's neighbour `shared/table.json` lies outside
//! the workspace. The `lib` commit holds the crate; the working tree changes it, and its Git settings ask for diffs
//! without prefixes, which the binary must override.
//!
//! The binary's `PATH` is the fixture's `bin/` alone: the stand-ins a scenario puts there, and links to `git` and the
//! system tools the stand-ins use. A program the scenario leaves out cannot be found. Each stand-in writes what it was
//! given to the folder `EVENTS` names.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustix::process::{Pid, Signal, kill_process};

/// The system tools the binary and the stand-ins run, linked into every fixture's `bin/`.
const SYSTEM_TOOLS: [&str; 8] = ["sh", "git", "mkdir", "cat", "sed", "cut", "sleep", "true"];

/// A stand-in cargo-mutants shard. It adds its shard's number to the calls in `cargo-mutants`, and records its
/// arguments, its folder, its target, runner and Cargo variables, and what its copy of the repository holds, under its
/// shard's number. With `HOLD` set it records its process and holds; otherwise it writes `MISSED` as its missed list
/// and exits with `CODE`.
pub(crate) const CARGO_MUTANTS: &str = r#"#!/bin/sh
shard=$(printf '%s\n' "$@" | sed -n '/^--shard$/{n;p;}' | cut -d / -f 1)
printf '%s\n' "$shard" >> "$EVENTS/cargo-mutants"
output=$(printf '%s\n' "$@" | sed -n '/^--output$/{n;p;}')
printf '%s\n' "$@" > "$EVENTS/shard-$shard.arguments"
pwd -P > "$EVENTS/shard-$shard.folder"
printf '%s\n' "${CARGO_TARGET_DIR-unset}" "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER-unset}" "${CARGO-unset}" \
    > "$EVENTS/shard-$shard.environment"
if [ -f ../shared/table.json ]; then echo shared; else echo missing; fi > "$EVENTS/shard-$shard.copy"
cat crates/core/src/lib.rs >> "$EVENTS/shard-$shard.copy"
if [ -n "${HOLD:-}" ]; then
    printf '%s %s\n' $$ "$(cut -d ' ' -f 22 /proc/$$/stat)" > "$EVENTS/shard-$shard.process"
    exec sleep 600
fi
mkdir -p "$output/mutants.out"
printf '%s' "${MISSED:-}" > "$output/mutants.out/missed.txt"
exit "${CODE:-0}"
"#;

/// rustc as it names its host.
pub(crate) const RUSTC: &str =
    "#!/bin/sh\nprintf 'rustc 1.100.0-nightly\\nhost: x86_64-unknown-linux-gnu\\n'\n";

/// A `systemd-run` that starts every scope: it records its options, one call a line, and runs the command after `--`.
pub(crate) const SCOPES: &str = r#"#!/bin/sh
options=
while [ "$1" != -- ]; do options="$options $1"; shift; done
shift
printf '%s\n' "${options# }" >> "$EVENTS/systemd-run"
exec "$@"
"#;

/// An `unshare` that makes every namespace and runs the command, its last argument here.
pub(crate) const NAMESPACES: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$EVENTS/unshare"
for last; do :; done
exec "$last"
"#;

/// A program that fails whatever it is asked.
pub(crate) const FAILS: &str = "#!/bin/sh\nexit 1\n";

/// A scenario's repository, programs and events, removed when it ends.
pub(crate) struct Fixture {
    root: PathBuf,
}

/// One run of the binary.
pub(crate) struct Run {
    pub(crate) output: Output,
}

impl Fixture {
    /// The repository, `bin/` with the system tools, the stand-ins for cargo-mutants and `rustc`, and a `cargo` the
    /// stand-in never runs, and an empty events folder. `systemd-run` and `unshare` are each the scenario's to add.
    pub(crate) fn new(name: &str) -> Self {
        // A test binary in a PID namespace of its own has the same process number as one in another, so a folder is
        // taken by making it, never by its name alone.
        static FIXTURES: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let number = FIXTURES.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "rust-mutants-{}-{number}-{name}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                made => {
                    made.expect("the scratch folder can be made");
                    break root.canonicalize().expect("the scratch folder has a path");
                }
            }
        };
        for folder in ["repo", "bin", "events"] {
            fs::create_dir(root.join(folder)).expect("the scratch folder is writable");
        }
        let fixture = Self { root };
        let path = std::env::var_os("PATH").unwrap_or_default();
        for tool in SYSTEM_TOOLS {
            let found = std::env::split_paths(&path)
                .map(|folder| folder.join(tool))
                .find(|file| file.is_file())
                .expect("each system tool is on the tests' PATH");
            std::os::unix::fs::symlink(found, fixture.root.join("bin").join(tool))
                .expect("bin/ is writable");
        }
        fixture.program("cargo-mutants", CARGO_MUTANTS);
        fixture.program("cargo", FAILS);
        fixture.program("rustc", RUSTC);
        fixture.git(&["init", "--quiet"]);
        fixture.git(&["config", "user.name", "Rust Mutants Tests"]);
        fixture.git(&["config", "user.email", "tests@example.invalid"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture.git(&["commit", "--quiet", "--allow-empty", "--message", "base"]);
        fixture.write("rust/crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
        fixture.write("shared/table.json", "{}\n");
        fixture.git(&["add", "--all"]);
        fixture.git(&["commit", "--quiet", "--message", "lib"]);
        fixture.git(&["config", "diff.noprefix", "true"]);
        fixture.write("rust/crates/core/src/lib.rs", "pub fn one() -> u8 { 2 }\n");
        fixture
    }

    /// The folder holding the stand-ins, the binary's whole PATH.
    pub(crate) fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub(crate) fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    /// The report folder, outside the repository.
    pub(crate) fn out(&self) -> PathBuf {
        self.root.join("report")
    }

    /// The folder the binary owns under the report folder.
    pub(crate) fn work(&self) -> PathBuf {
        self.out().join("rust-mutants")
    }

    /// Puts an executable `script` called `name` in `bin/`.
    pub(crate) fn program(&self, name: &str, script: &str) {
        let path = self.root.join("bin").join(name);
        fs::write(&path, script).expect("bin/ is writable");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
    }

    /// Removes `name` from `bin/`, so the binary cannot find it.
    pub(crate) fn remove_program(&self, name: &str) {
        fs::remove_file(self.root.join("bin").join(name)).expect("the program is in bin/");
    }

    pub(crate) fn write(&self, path: &str, text: &str) {
        let path = self.repo().join(path);
        fs::create_dir_all(path.parent().expect("a file has a folder"))
            .expect("the repository is writable");
        fs::write(path, text).expect("the repository is writable");
    }

    fn git(&self, arguments: &[&str]) {
        let done = Command::new("git")
            .arg("-C")
            .arg(self.repo())
            .args(arguments)
            .output()
            .expect("git starts");
        assert!(
            done.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&done.stderr)
        );
    }

    /// The binary's report on the change since the `lib` commit, in the repository, with `env` set.
    pub(crate) fn command(&self, env: &[(&str, &str)]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rust-mutants"));
        command
            .args(["--workspace", "rust", "--base", "HEAD", "--out"])
            .arg(self.out())
            .current_dir(self.repo())
            .env("PATH", self.root.join("bin"))
            .env("EVENTS", self.root.join("events"))
            .env_remove("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER")
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    /// Runs the report to its end.
    pub(crate) fn run(&self, env: &[(&str, &str)]) -> Run {
        let output = Running::start(self.command(env)).finish();
        Run { output }
    }

    /// What a stand-in wrote to the events file `name`, or nothing when it wrote none.
    pub(crate) fn event(&self, name: &str) -> String {
        fs::read_to_string(self.root.join("events").join(name)).unwrap_or_default()
    }

    /// The lines a stand-in wrote to the events file `name`.
    pub(crate) fn lines(&self, name: &str) -> Vec<String> {
        self.event(name).lines().map(str::to_owned).collect()
    }

    /// Removes every event, for a second run in the same fixture.
    pub(crate) fn clear_events(&self) {
        fs::remove_dir_all(self.root.join("events")).expect("the events folder exists");
        fs::create_dir(self.root.join("events")).expect("the scratch folder is writable");
    }

    /// Whether the process a stand-in recorded under `name` has ended and been reaped: no process has its number and
    /// start time, not even a zombie.
    pub(crate) fn ended(&self, name: &str) -> bool {
        let recorded = self.event(name);
        let (pid, started) = recorded
            .trim()
            .split_once(' ')
            .expect("a number and a start time");
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let now = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.split(' ').nth(19))
            .unwrap_or_default();
        now != started
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A running binary, stopped by its process number and reaped if the scenario ends before it does.
pub(crate) struct Running {
    child: Option<Child>,
}

impl Running {
    pub(crate) fn start(mut command: Command) -> Self {
        let child = command.spawn().expect("the binary starts");
        Self { child: Some(child) }
    }

    /// Whether the binary is still running.
    pub(crate) fn running(&mut self) -> bool {
        let child = self.child.as_mut().expect("still owned");
        matches!(child.try_wait(), Ok(None))
    }

    /// Sends the binary `signal`.
    pub(crate) fn signal(&self, signal: Signal) {
        let child = self.child.as_ref().expect("still owned");
        kill_process(Pid::from_child(child), signal).expect("the binary has not been reaped");
    }

    /// Waits for the binary to end, and gives what it printed.
    pub(crate) fn finish(mut self) -> Output {
        self.child
            .take()
            .expect("still owned")
            .wait_with_output()
            .expect("the binary is waited for")
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // The binary has not been reaped, so its number is still its own.
            let _ = kill_process(Pid::from_child(&child), Signal::TERM);
            let _ = child.wait();
        }
    }
}

impl Run {
    pub(crate) fn status(&self) -> i32 {
        self.output.status.code().expect("the binary exits")
    }

    pub(crate) fn stdout(&self) -> String {
        String::from_utf8(self.output.stdout.clone()).expect("the output is UTF-8")
    }

    pub(crate) fn stderr(&self) -> String {
        String::from_utf8(self.output.stderr.clone()).expect("the error output is UTF-8")
    }

    /// The printed lines.
    pub(crate) fn printed(&self) -> Vec<String> {
        self.stdout().lines().map(str::to_owned).collect()
    }
}
