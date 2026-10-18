//! Small git repositories in the system's temporary folder, with step scripts and stand-ins, and the binary run
//! against them.
//!
//! Each fixture has two folders of programs. `bin/`, first on the binary's `PATH`, holds the stand-in `nix` and any
//! tool only the caller has. `shell/` is the stand-in development shell: links to the system tools step scripts use,
//! and the stand-ins a scenario adds. The stand-in `nix` runs `develop [--unset PATH] <checkout> -c <command>` as
//! `<command>` with the `PATH` `nix develop` gives: `shell/` alone when `PATH` is unset, and otherwise `shell/` before
//! the caller's. A step so runs as it would in a development shell without building one. It records each checkout it
//! was asked for. Step scripts write events to the folder `EVENTS` names, and wait on each other's files rather than on
//! time.
//!
//! A run of the binary is a [`Reporter`], which stops the binary by its process number if the scenario fails while it
//! runs. The binary's supervisors then stop every step it started.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustix::process::{Pid, Signal, kill_process};

/// The stand-in for `nix develop [--unset PATH] <checkout> -c <command>`, given the stand-in shell's folder.
fn nix(shell: &Path) -> String {
    format!(
        r#"#!/bin/sh
[ "$1" = develop ] || {{ echo "the stand-in nix runs only develop" >&2; exit 97; }}
shift
inherited=$PATH
if [ "$1" = --unset ] && [ "$2" = PATH ]; then inherited=; shift 2; fi
printf '%s\n' "$1" >> "$EVENTS/nix"
shift
[ "$1" = -c ] || {{ echo "the stand-in nix needs -c" >&2; exit 98; }}
shift
export PATH="{}${{inherited:+:$inherited}}"
exec "$@"
"#,
        shell.display()
    )
}

/// The system tools the stand-in shell links, each where the tests find it.
const SHELL_TOOLS: &str = "sh bash git cp cat grep sleep dirname mkdir rm cut tr sha256sum kill python3 env ls mv \
                           touch head sed wc true printf cargo rustc rustdoc";

/// The shell functions step scripts share, `dandegate/lib.sh` in every fixture: `event <words>` records an event;
/// `wait_until <command>` runs the command until it succeeds, without a deadline of its own. `started <prefix>` counts
/// the start events whose words begin with the prefix. `record <name> <pid>` writes a process's number and start time
/// to the events folder's file `<name>`, for [`Fixture::ended`].
pub(crate) const LIB: &str = r#"event() { printf '%s\n' "$*" >> "$EVENTS/events"; }
wait_until() { until "$@"; do sleep 0.01; done; }
started() { grep -c "^start $1" "$EVENTS/events" 2>/dev/null || true; }
record() { printf '%s %s\n' "$2" "$(cut -d ' ' -f 22 "/proc/$2/stat")" > "$EVENTS/$1"; }
"#;

/// The generic step, `dandegate/step.sh <name> <tree> <commit> <out>`: records its start with the commit it was given
/// and the one checked out, copies the measured tree's `fixture/<name>/` into its output folder, records its finish,
/// and fails when the tree has `fixture/<name>.fail`.
pub(crate) const STEP: &str = r#". "$(dirname "$0")/lib.sh"
name=$1 tree=$2 commit=$3 out=$4
event "start $name $commit $(git -C "$tree" rev-parse HEAD)"
if [ -d "$tree/fixture/$name" ]; then cp -R "$tree/fixture/$name/." "$out/"; fi
event "finish $name $commit"
if [ -f "$tree/fixture/$name.fail" ]; then exit 1; fi
"#;

/// A scenario's repository, stand-ins and events, removed when it ends.
pub(crate) struct Fixture {
    root: PathBuf,
}

/// One run of the binary.
pub(crate) struct Run {
    pub(crate) output: Output,
    pub(crate) out: PathBuf,
}

impl Fixture {
    pub(crate) fn new(name: &str) -> Self {
        // A test binary in a PID namespace of its own has the same process number as one in another, so a folder
        // is taken by making it, never by its name alone.
        static FIXTURES: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let number = FIXTURES.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("dandegate-{}-{number}-{name}", std::process::id()));
            match fs::create_dir(&root) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                made => {
                    made.expect("the scratch folder can be made");
                    break root;
                }
            }
        };
        for folder in ["repo", "bin", "shell", "events"] {
            fs::create_dir_all(root.join(folder)).expect("the scratch folder is writable");
        }
        let fixture = Self { root };
        fixture.caller_tool("nix", &nix(&fixture.root.join("shell")));
        // The toolchain's own programs come from the folder of the Cargo running the tests, not from a proxy that
        // needs the caller's PATH.
        let toolchain = std::env::var_os("CARGO")
            .map(PathBuf::from)
            .and_then(|cargo| cargo.parent().map(Path::to_owned));
        let path = std::env::var_os("PATH").unwrap_or_default();
        let folders: Vec<PathBuf> = toolchain
            .into_iter()
            .chain(std::env::split_paths(&path))
            .collect();
        for tool in SHELL_TOOLS.split_whitespace() {
            let found = folders
                .iter()
                .map(|folder| folder.join(tool))
                .find(|file| file.is_file());
            if let Some(found) = found {
                std::os::unix::fs::symlink(found, fixture.root.join("shell").join(tool))
                    .expect("the stand-in shell is writable");
            }
        }
        fixture.git(&["init", "--quiet", "--initial-branch", "main"]);
        fixture.git(&["config", "user.name", "Dandegate Tests"]);
        fixture.git(&["config", "user.email", "tests@example.invalid"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture.write("dandegate/lib.sh", LIB);
        fixture.write("dandegate/step.sh", STEP);
        fixture
    }

    pub(crate) fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    /// A path outside the repository, for `--out` and other files.
    pub(crate) fn outside(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub(crate) fn events_folder(&self) -> PathBuf {
        self.root.join("events")
    }

    /// Put an executable stand-in called `name` in the stand-in development shell.
    pub(crate) fn tool(&self, name: &str, script: &str) {
        self.program_in("shell", name, script);
    }

    /// Put an executable stand-in called `name` first on the binary's own `PATH`, outside the development shell.
    pub(crate) fn caller_tool(&self, name: &str, script: &str) {
        self.program_in("bin", name, script);
    }

    fn program_in(&self, folder: &str, name: &str, script: &str) {
        let path = self.root.join(folder).join(name);
        // A link to a system tool is replaced, never written through.
        let _ = fs::remove_file(&path);
        fs::write(&path, script).expect("the stand-in is writable");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
    }

    pub(crate) fn write(&self, path: &str, text: &str) {
        self.write_bytes(path, text.as_bytes());
    }

    pub(crate) fn write_bytes(&self, path: &str, bytes: &[u8]) {
        let path = self.repo().join(path);
        fs::create_dir_all(path.parent().expect("a file has a folder"))
            .expect("the repository is writable");
        fs::write(path, bytes).expect("the repository is writable");
    }

    pub(crate) fn remove(&self, path: &str) {
        fs::remove_file(self.repo().join(path)).expect("the file exists");
    }

    /// Commit everything and give the commit.
    pub(crate) fn commit(&self, message: &str) -> String {
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "--allow-empty", "--message", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    pub(crate) fn git(&self, arguments: &[&str]) -> String {
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
        String::from_utf8(done.stdout).expect("git prints UTF-8")
    }

    /// Run the binary in the repository with `arguments`, `--out` set to the folder `out` outside it.
    pub(crate) fn run(&self, out: &str, arguments: &[&str]) -> Run {
        self.run_with(out, arguments, &[])
    }

    /// Run the binary as [`Fixture::run`] does, with `env` set.
    pub(crate) fn run_with(&self, out: &str, arguments: &[&str], env: &[(&str, &str)]) -> Run {
        self.run_program(
            Path::new(env!("CARGO_BIN_EXE_dandegate")),
            out,
            arguments,
            env,
        )
    }

    /// Run `program`, a dandegate, as [`Fixture::run`] runs the binary under test, with `env` set.
    pub(crate) fn run_program(
        &self,
        program: &Path,
        out: &str,
        arguments: &[&str],
        env: &[(&str, &str)],
    ) -> Run {
        let out = self.outside(out);
        let mut all: Vec<&str> = arguments.to_vec();
        let out_text = out.to_str().expect("the scratch path is UTF-8").to_owned();
        all.extend(["--out", &out_text]);
        let mut command = self.program(program, &all);
        command.envs(env.iter().copied());
        let output = Reporter::start(command).finish();
        Run { output, out }
    }

    /// The binary's command in the repository, without `--out`.
    pub(crate) fn command(&self, arguments: &[&str]) -> Command {
        self.program(Path::new(env!("CARGO_BIN_EXE_dandegate")), arguments)
    }

    /// The binary's command in the repository, without `--out`, as the first process of a PID namespace of its own,
    /// where its process number is 1 like that of any other binary started this way.
    pub(crate) fn namespaced(&self, arguments: &[&str]) -> Command {
        let mut all = vec![
            "--user",
            "--map-root-user",
            "--pid",
            "--fork",
            "--mount-proc",
            "--kill-child",
            env!("CARGO_BIN_EXE_dandegate"),
        ];
        all.extend_from_slice(arguments);
        self.program(Path::new("unshare"), &all)
    }

    fn program(&self, program: &Path, arguments: &[&str]) -> Command {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![self.root.join("bin")];
        paths.extend(std::env::split_paths(&path));
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(self.repo())
            .env("PATH", std::env::join_paths(paths).expect("a PATH"))
            .env("EVENTS", self.events_folder());
        command
    }

    /// The events steps recorded, in order.
    pub(crate) fn events(&self) -> Vec<String> {
        fs::read_to_string(self.events_folder().join("events"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn clear_events(&self) {
        let _ = fs::remove_file(self.events_folder().join("events"));
        let _ = fs::remove_file(self.events_folder().join("nix"));
    }

    /// Wait until a step has recorded `event`, while `reporter` runs.
    pub(crate) fn wait_for_event(&self, event: &str, reporter: &mut Reporter) {
        while !self.events().iter().any(|found| found == event) {
            assert!(reporter.running(), "dandegate ended before {event}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// The process a step recorded under `name`.
    pub(crate) fn recorded(&self, name: &str) -> (Pid, String) {
        let text =
            fs::read_to_string(self.events_folder().join(name)).expect("the step recorded it");
        let (pid, started) = text
            .trim()
            .split_once(' ')
            .expect("a number and a start time");
        let pid = Pid::from_raw(pid.parse().expect("a process number")).expect("not zero");
        (pid, started.to_owned())
    }

    /// Whether the process a step recorded under `name` has ended and been reaped: no process has its number and
    /// start time, not even a zombie.
    pub(crate) fn ended(&self, name: &str) -> bool {
        let (pid, started) = self.recorded(name);
        let stat =
            fs::read_to_string(format!("/proc/{}/stat", pid.as_raw_pid())).unwrap_or_default();
        let now = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.split(' ').nth(19))
            .unwrap_or_default();
        now != started
    }

    /// The kept results' folder.
    pub(crate) fn results(&self) -> PathBuf {
        self.repo().join(".cache/dandegate/results")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A running binary, stopped by its process number and reaped if the scenario ends before it does.
pub(crate) struct Reporter {
    child: Option<Child>,
}

impl Reporter {
    /// Start `command` with its output kept.
    pub(crate) fn start(mut command: Command) -> Self {
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary starts");
        Self { child: Some(child) }
    }

    /// Whether the binary is still running.
    pub(crate) fn running(&mut self) -> bool {
        let child = self.child.as_mut().expect("still owned");
        matches!(child.try_wait(), Ok(None))
    }

    /// Wait for the binary to end, and give what it printed.
    pub(crate) fn finish(mut self) -> Output {
        self.child
            .take()
            .expect("still owned")
            .wait_with_output()
            .expect("the binary is waited for")
    }

    /// Send the binary `signal`, wait for it to end, and give what it printed before.
    pub(crate) fn stop(mut self, signal: Signal) -> Output {
        let child = self.child.take().expect("still owned");
        kill_process(Pid::from_child(&child), signal).expect("the binary has not been reaped");
        child.wait_with_output().expect("the binary is waited for")
    }
}

impl Drop for Reporter {
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

    /// The printed lines starting with `prefix` and `: `, without it.
    pub(crate) fn printed(&self, prefix: &str) -> Vec<String> {
        let lead = format!("{prefix}: ");
        self.stdout()
            .lines()
            .filter_map(|line| line.strip_prefix(&lead).map(str::to_owned))
            .collect()
    }

    /// Asserts the page was written and gives it.
    pub(crate) fn page(&self) -> String {
        assert_eq!(self.status(), 0, "{}\n{}", self.stdout(), self.stderr());
        let index = self.out.join("index.html");
        assert_eq!(
            self.printed("DANDEGATE"),
            [index.to_str().expect("UTF-8").to_owned()]
        );
        fs::read_to_string(index).expect("the page")
    }

    pub(crate) fn file(&self, path: &str) -> String {
        fs::read_to_string(self.out.join(path)).expect("the output folder has the file")
    }

    pub(crate) fn exists(&self, path: &str) -> bool {
        self.out.join(path).exists()
    }
}

/// An 8-bit RGBA PNG of `width` by `height` with the samples given.
pub(crate) fn png(width: u32, height: u32, samples: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("a header");
    writer.write_image_data(samples).expect("the samples");
    writer.finish().expect("a whole image");
    bytes
}

/// Every file under `folder`, relative to it.
pub(crate) fn files_under(folder: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_owned()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(
                    path.strip_prefix(folder)
                        .expect("inside")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    found.sort();
    found
}
