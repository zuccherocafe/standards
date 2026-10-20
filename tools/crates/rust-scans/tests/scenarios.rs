//! rust-scans through its command, on tiny workspaces written to the system's temporary folder, outside any Cargo
//! workspace, with the real `cargo metadata`. Its uses, most common first:
//!
//! 1. A project's report: a workspace in a folder below the current one, with cores and kernels named, prints each
//!    finding and a count per scan and exits 0.
//! 2. A workspace whose root is itself a package, with a member excluded: Cargo's members are scanned, in folder order.
//! 3. A tree that cannot be read, or a source file that cannot, exits 1 and says why on standard error.
//! 4. A malformed argument exits 2.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// A new, empty folder for one scenario, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let folder = std::env::temp_dir().join(format!("rust-scans-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("the scratch folder is writable");
        Self(folder)
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().expect("a file has a folder"))
            .expect("the scratch folder is writable");
        fs::write(path, text).expect("the scratch folder is writable");
    }

    fn crate_at(&self, folder: &str, name: &str, lib: &str) {
        self.write(
            &format!("{folder}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nedition = \"2024\"\n"),
        );
        self.write(&format!("{folder}/src/lib.rs"), lib);
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rust-scans"))
            .args(arguments)
            .current_dir(&self.0)
            .output()
            .expect("the binary starts")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Standard output's lines without the last, the time taken, which is checked for its shape.
fn lines(output: &Output) -> Vec<String> {
    let text = String::from_utf8(output.stdout.clone()).expect("the output is UTF-8");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let seconds = lines.pop().expect("a last line");
    let (label, value) = seconds.split_once(' ').expect("a label and a value");
    assert_eq!(label, "seconds:");
    let (whole, decimals) = value.split_once('.').expect("a decimal point");
    assert!(
        whole.parse::<u64>().is_ok() && decimals.len() == 3,
        "{seconds}"
    );
    lines
}

#[test]
fn a_projects_report_prints_each_finding_and_every_count() {
    let scratch = Scratch::new("report");
    scratch.write(
        "rust/Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
    );
    scratch.crate_at(
        "rust/crates/toy-core",
        "toy-core",
        "use std::fs;\npub struct Reach { pub margin_metres: f64 }\nmod solve;\n#[cfg(test)]\nmod checks;\n",
    );
    scratch.write(
        "rust/crates/toy-core/src/solve.rs",
        "pub(crate) fn solve(step_seconds: f64) {}\n",
    );
    scratch.write(
        "rust/crates/toy-core/src/checks.rs",
        "async fn fixture(span_seconds: f64) {}\n",
    );
    scratch.crate_at(
        "rust/crates/toy-shell",
        "toy-shell",
        "#[error(\"{code}: failed\")]\npub struct Failed;\npub fn wait(timeout_seconds: f64) {}\n",
    );
    scratch.write(
        "rust/crates/toy-shell/src/table.json",
        "{\"not\": \"rust\"}\n",
    );
    let output = scratch.run(&[
        "--workspace",
        "rust",
        "--core",
        "toy-core",
        "--kernel",
        "toy-core=solve",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        lines(&output),
        [
            "H1 rust/crates/toy-core/src/lib.rs:1: std::fs in toy-core",
            "H2 rust/crates/toy-core/src/lib.rs:2: margin_metres: f64",
            "H2 rust/crates/toy-shell/src/lib.rs:3: timeout_seconds: f64",
            "H3 rust/crates/toy-shell/src/lib.rs:1: a code in #[error(...)]",
            "H4 rust/crates/toy-core/src/checks.rs:1: async fn in toy-core (test code)",
            "H1: 1",
            "H2: 2",
            "H3: 1",
            "H4: 1",
        ]
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn a_root_package_is_scanned_with_the_members_and_an_excluded_crate_is_not() {
    let scratch = Scratch::new("rooted");
    let gap = "pub struct Gap { pub width_metres: f64 }\n";
    scratch.write(
        "Cargo.toml",
        "[package]\nname = \"toy-root\"\nedition = \"2024\"\n\n\
         [workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/toy-out\"]\n",
    );
    scratch.write("src/lib.rs", gap);
    scratch.crate_at("crates/toy-in", "toy-in", gap);
    scratch.crate_at("crates/toy-out", "toy-out", gap);
    let output = scratch.run(&[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        lines(&output),
        [
            "H2 src/lib.rs:1: width_metres: f64",
            "H2 crates/toy-in/src/lib.rs:1: width_metres: f64",
            "H1: 0",
            "H2: 2",
            "H3: 0",
            "H4: 0",
        ]
    );
}

#[test]
fn a_folder_without_a_workspace_exits_1_and_says_why() {
    let scratch = Scratch::new("empty");
    let output = scratch.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8(output.stderr).expect("the output is UTF-8");
    assert!(
        stderr.starts_with("rust-scans: cargo metadata failed in .: "),
        "{stderr}"
    );
}

#[test]
fn a_file_that_is_not_rust_exits_1_naming_it() {
    let scratch = Scratch::new("broken");
    scratch.crate_at(".", "toy-broken", "pub fn fine() {}\nlet broken = 1;\n");
    let output = scratch.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("the output is UTF-8");
    assert!(
        stderr.starts_with("rust-scans: could not parse "),
        "{stderr}"
    );
    assert!(stderr.contains("src/lib.rs:2:1 as Rust\n"), "{stderr}");
    assert!(stderr.contains("\n  caused by: "), "{stderr}");
}

#[test]
fn a_source_file_that_cannot_be_read_exits_1_naming_it() {
    let scratch = Scratch::new("unreadable");
    scratch.crate_at(".", "toy-unreadable", "mod missing;\n");
    std::os::unix::fs::symlink(
        scratch.0.join("nowhere.rs"),
        scratch.0.join("src/missing.rs"),
    )
    .expect("the scratch folder takes a link");
    let output = scratch.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("the output is UTF-8");
    assert!(
        stderr.starts_with("rust-scans: could not read "),
        "{stderr}"
    );
    assert!(stderr.contains("src/missing.rs\n  caused by: "), "{stderr}");
}

#[test]
fn a_malformed_argument_exits_2() {
    let scratch = Scratch::new("arguments");
    assert_eq!(
        scratch.run(&["--kernel", "toy-core"]).status.code(),
        Some(2)
    );
    assert_eq!(scratch.run(&["--unit", "Newtons"]).status.code(), Some(2));
}
