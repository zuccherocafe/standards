//! pin-age through its command, on made-up pin files in a temporary folder, which each scenario removes. Its uses, most
//! common first:
//!
//! 1. A project's checks on its own pins, all recent: nothing is printed and the run passes.
//! 2. A pin over a month old: one line names it and its age, and the run still passes.
//! 3. A pin file that cannot be used: one line on standard error names it, and the run exits 2.
//! 4. A command line naming no file: a usage error, exit 2.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A scenario's folder, removed when it ends.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        // A test binary in a PID namespace of its own can share its process number with another, so a folder is taken
        // by making it.
        static FOLDERS: AtomicUsize = AtomicUsize::new(0);
        loop {
            let number = FOLDERS.fetch_add(1, Ordering::Relaxed);
            let folder = std::env::temp_dir().join(format!(
                "pin-age-scenario-{}-{number}-{name}",
                std::process::id()
            ));
            match fs::create_dir(&folder) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                made => {
                    made.expect("the temporary folder is writable");
                    return Self(folder);
                }
            }
        }
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).expect("the scenario's folder is writable");
        path
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn toolchain(channel: &str) -> String {
    format!("[toolchain]\nchannel = \"{channel}\"\ncomponents = [\"rustfmt\", \"clippy\"]\n")
}

/// A lock whose one input, `nixpkgs`, was last modified `seconds` after 1970-01-01T00:00:00Z.
fn lock(seconds: u64) -> String {
    format!(
        r#"{{"nodes": {{"nixpkgs": {{"locked": {{"lastModified": {seconds}, "type": "github"}}}}, "root": {{"inputs": {{"nixpkgs": "nixpkgs"}}}}}}, "root": "root", "version": 7}}"#
    )
}

/// This instant a day ago, in seconds after 1970-01-01T00:00:00Z: yesterday in UTC, or the day before when the run
/// crosses midnight.
fn a_day_ago() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970");
    now.as_secs() - 86_400
}

fn pin_age(arguments: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pin-age"))
        .args(arguments)
        .output()
        .expect("pin-age runs")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("UTF-8")
}

#[test]
fn recent_pins_print_nothing_and_pass() {
    let folder = Folder::new("recent");
    let release = folder.write("rust-toolchain.toml", &toolchain("1.92.0"));
    let flake = folder.write("flake.lock", &lock(a_day_ago()));
    let run = pin_age(&[
        Path::new("--toolchain"),
        &release,
        Path::new("--lock"),
        &flake,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    assert_eq!(text(&run.stdout), "");
    assert_eq!(text(&run.stderr), "");
}

#[test]
fn an_old_pin_gets_a_line_with_its_age_and_the_run_passes() {
    let folder = Folder::new("old");
    let old = folder.write("rust-toolchain.toml", &toolchain("nightly-2020-01-01"));
    let flake = folder.write("flake.lock", &lock(a_day_ago()));
    let run = pin_age(&[Path::new("--toolchain"), &old, Path::new("--lock"), &flake]);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    let lines: Vec<&str> = text(&run.stdout).lines().collect();
    let [line] = lines[..] else {
        panic!("one line: {lines:?}");
    };
    let age = line
        .strip_prefix(&format!(
            "pin-age: the Rust nightly-2020-01-01 in {} is ",
            old.display()
        ))
        .and_then(|rest| rest.strip_suffix(" days old; move it (rules/general.md, Toolchain)"))
        .unwrap_or_else(|| panic!("the advice names the pin: {line}"));
    // 2020-01-01 to 2026-10-01, the earliest day these scenarios were written for, is 2465 days.
    let age: u32 = age.parse().expect("a whole number of days");
    assert!(age >= 2465, "{line}");
    assert_eq!(text(&run.stderr), "");
}

#[test]
fn a_pin_file_that_cannot_be_used_is_named_on_standard_error_and_exits_2() {
    let folder = Folder::new("broken");
    let stable = folder.write("rust-toolchain.toml", &toolchain("stable"));
    let run = pin_age(&[Path::new("--toolchain"), &stable]);
    assert_eq!(run.status.code(), Some(2));
    assert_eq!(text(&run.stdout), "");
    assert_eq!(
        text(&run.stderr),
        format!(
            "pin-age: {}: [toolchain] channel: \"stable\" is not a dated nightly or a numbered release\n",
            stable.display()
        )
    );
}

#[test]
fn naming_no_file_is_a_usage_error() {
    let run = pin_age(&[]);
    assert_eq!(run.status.code(), Some(2));
    assert_eq!(text(&run.stdout), "");
    assert!(
        text(&run.stderr).contains("give at least one --toolchain or --lock"),
        "{}",
        text(&run.stderr)
    );
}
