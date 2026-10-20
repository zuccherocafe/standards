//! rust-mutants through its command, on a small Git repository whose workspace is `rust/`, below its root, and whose
//! crate has a neighbour outside the workspace. Stand-ins take the place of cargo-mutants, rustc, `systemd-run` and
//! `unshare` (`support`). Its uses, most common first:
//!
//! 1. A change's report: each shard mutates the change in its own copy of the whole repository, with a private target
//!    folder, and the missed mutants are printed and counted.
//! 2. On a machine with memory-capped scopes and PID namespaces, each shard runs capped with its share of the CPUs and
//!    each test binary contained; without either, the report says so and runs on.
//! 3. A report run again into the same folder prints only its own mutants and leaves the folder's other contents.
//! 4. A shard that fails, or an interrupt, ends the run with exit code 1; every shard is stopped and the copies are
//!    removed.

#[path = "scenarios/support.rs"]
mod support;

use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

use rustix::process::Signal;

use crate::support::{FAILS, Fixture, NAMESPACES, Running, SCOPES};

/// A mutant each stand-in shard reports missed.
const MISSED: &str = "crates/core/src/lib.rs:1:22: replace one -> u8 with 0";

const NO_SCOPE: &str = "rust_mutants: systemd-run --user cannot start a scope here, so the shards run without a memory cap";

const NO_NAMESPACE: &str =
    "rust_mutants: a PID namespace cannot be made here, so the test binaries run uncontained";

const NAMESPACE: &str = "unshare --user --map-root-user --pid --fork --mount-proc --kill-child";

/// A fixture on a machine that starts every scope and makes every namespace.
fn capable(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.program("systemd-run", SCOPES);
    fixture.program("unshare", NAMESPACES);
    fixture
}

#[test]
fn a_change_is_mutated_in_whole_repository_copies_with_private_targets() {
    let fixture = capable("change");
    let missed = format!("{MISSED}\n");
    let run = fixture.run(&[("CODE", "2"), ("MISSED", &missed)]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    let work = fixture.work();
    let mut calls = fixture.lines("cargo");
    calls.sort();
    assert_eq!(calls, ["0", "1"]);
    for index in 0..2 {
        let copy = work.join(format!("copy-{index}"));
        let folder = copy.join("rust");
        assert_eq!(
            fixture.event(&format!("shard-{index}.folder")).trim_end(),
            folder.to_str().expect("UTF-8")
        );
        assert_eq!(
            fixture.lines(&format!("shard-{index}.environment"))[0],
            folder.join("target").to_str().expect("UTF-8")
        );
        let diff = work.join("change.diff");
        let shard = format!("{index}/2");
        assert_eq!(
            fixture.lines(&format!("shard-{index}.arguments"))[1..8],
            [
                "--in-diff",
                diff.to_str().expect("UTF-8"),
                "--workspace",
                "--in-place",
                "--shard",
                &shard,
                "--output",
            ]
        );
        assert!(!copy.exists(), "copy-{index} was not removed");
    }
    let diff = fs::read_to_string(work.join("change.diff")).expect("the diff was kept");
    assert!(diff.contains("+++ b/crates/core/src/lib.rs\n"), "{diff}");
    assert_eq!(
        run.printed(),
        [
            "rust_mutants: 1 Rust files changed since HEAD",
            &format!("missed, to judge: {MISSED}"),
            &format!("missed, to judge: {MISSED}"),
            "caught: 0 missed: 2 timeout: 0 unviable: 0",
        ]
    );
}

#[test]
fn each_test_binary_runs_in_a_namespace_or_uncontained_saying_so() {
    let fixture = capable("namespaced");
    let run = fixture.run(&[]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    for index in 0..2 {
        assert_eq!(
            fixture.lines(&format!("shard-{index}.environment"))[1],
            NAMESPACE
        );
    }
    assert!(!run.printed().contains(&NO_NAMESPACE.to_owned()));

    // A runner the caller already set runs inside the namespace; one set to nothing is no runner.
    let runner = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER";
    for (set, expected) in [
        ("valgrind -q", format!("{NAMESPACE} valgrind -q")),
        ("", NAMESPACE.to_owned()),
    ] {
        fixture.clear_events();
        let run = fixture.run(&[(runner, set)]);
        assert_eq!(run.status(), 0, "{}", run.stderr());
        for index in 0..2 {
            assert_eq!(
                fixture.lines(&format!("shard-{index}.environment"))[1],
                expected,
                "{set:?}"
            );
        }
    }

    fixture.clear_events();
    fixture.program("unshare", FAILS);
    let run = fixture.run(&[]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    for index in 0..2 {
        assert_eq!(
            fixture.lines(&format!("shard-{index}.environment"))[1],
            "unset"
        );
    }
    assert!(
        run.printed().contains(&NO_NAMESPACE.to_owned()),
        "{}",
        run.stdout()
    );
}

#[test]
fn a_namespace_is_available_only_when_unshare_makes_one() {
    let fixture = capable("unshare");
    let made = fixture.run(&[]);
    assert_eq!(
        fixture.lines("unshare"),
        ["--user --map-root-user --pid --fork --mount-proc --kill-child true"]
    );
    fixture.program("unshare", FAILS);
    let refused = fixture.run(&[]);
    fixture.remove_program("unshare");
    let missing = fixture.run(&[]);
    let said = |run: &support::Run| run.printed().contains(&NO_NAMESPACE.to_owned());
    assert_eq!(
        [said(&made), said(&refused), said(&missing)],
        [false, true, true]
    );
}

#[test]
fn a_scope_is_available_only_when_systemd_run_starts_one() {
    let fixture = capable("systemd-run");
    let started = fixture.run(&[]);
    assert_eq!(
        fixture.lines("systemd-run")[0],
        "--user --scope --quiet --property MemoryMax=17179869184 --property MemorySwapMax=0 \
         --property OOMPolicy=continue"
    );
    fixture.program("systemd-run", FAILS);
    let refused = fixture.run(&[]);
    fixture.remove_program("systemd-run");
    let missing = fixture.run(&[]);
    let said = |run: &support::Run| run.printed().contains(&NO_SCOPE.to_owned());
    assert_eq!(
        [said(&started), said(&refused), said(&missing)],
        [false, true, true]
    );
}

#[test]
fn each_shard_runs_in_a_capped_scope_with_its_share_of_the_cpus_or_uncapped_saying_so() {
    let fixture = capable("capped");
    let run = fixture.run(&[]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    // The probe's call first, then one call for each shard, each holding half of the 16 GiB cap.
    assert_eq!(
        fixture.lines("systemd-run")[1..],
        [
            "--user --scope --quiet --property MemoryMax=8589934592 --property MemorySwapMax=0 \
             --property OOMPolicy=continue",
            "--user --scope --quiet --property MemoryMax=8589934592 --property MemorySwapMax=0 \
             --property OOMPolicy=continue",
        ]
    );
    let online = Command::new("getconf")
        .arg("_NPROCESSORS_ONLN")
        .output()
        .expect("getconf starts");
    let online: usize = String::from_utf8(online.stdout)
        .expect("UTF-8")
        .trim()
        .parse()
        .expect("a count of CPUs");
    let share = (online / 2).max(1).to_string();
    for index in 0..2 {
        let arguments = fixture.lines(&format!("shard-{index}.arguments"));
        let at = arguments
            .iter()
            .position(|argument| argument == "--jobserver-tasks")
            .expect("jobserver tasks");
        assert_eq!(arguments[at + 1], share);
    }
    assert!(!run.printed().contains(&NO_SCOPE.to_owned()));

    fixture.clear_events();
    // A shard sent through this systemd-run would fail, so a run that finishes ran each shard's cargo-mutants itself.
    fixture.program("systemd-run", FAILS);
    let run = fixture.run(&[]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    assert_eq!(fixture.lines("cargo").len(), 2);
    assert!(
        run.printed().contains(&NO_SCOPE.to_owned()),
        "{}",
        run.stdout()
    );
}

#[test]
fn each_copy_holds_the_files_outside_the_workspace_and_the_uncommitted_change() {
    let fixture = capable("copies");
    let run = fixture.run(&[]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    for index in 0..2 {
        assert_eq!(
            fixture.event(&format!("shard-{index}.copy")),
            "shared\npub fn one() -> u8 { 2 }\n"
        );
    }
}

#[test]
fn a_rerun_reports_only_its_own_mutants_and_leaves_other_folders_alone() {
    let fixture = capable("rerun");
    let old = fixture.work().join("shard-0/mutants.out");
    fs::create_dir_all(&old).expect("writable");
    fs::write(
        old.join("missed.txt"),
        "crates/core/src/old.rs:1:1: an earlier miss\n",
    )
    .expect("writable");
    for name in ["copy-notes", "shard-analysis"] {
        fs::create_dir_all(fixture.out().join(name)).expect("writable");
        fs::write(fixture.out().join(name).join("kept.txt"), "mine\n").expect("writable");
    }
    let run = fixture.run(&[]);
    assert_eq!(run.status(), 0, "{}", run.stderr());
    // The stand-in shards find nothing, so a change with no mutant says so.
    assert_eq!(
        run.printed(),
        [
            "rust_mutants: 1 Rust files changed since HEAD",
            "caught: 0 missed: 0 timeout: 0 unviable: 0",
            "rust_mutants: the changed Rust holds no function cargo-mutants can mutate",
        ]
    );
    for name in ["copy-notes", "shard-analysis"] {
        assert_eq!(
            fs::read_to_string(fixture.out().join(name).join("kept.txt")).expect("kept"),
            "mine\n"
        );
    }
}

#[test]
fn a_failure_after_the_shards_started_stops_them_and_removes_the_copies() {
    let fixture = capable("interrupted");
    let mut running = Running::start(fixture.command(&[("HOLD", "1")]));
    while fixture.event("shard-0.process").is_empty() || fixture.event("shard-1.process").is_empty()
    {
        assert!(
            running.running(),
            "rust-mutants ended before both shards started"
        );
        thread::sleep(Duration::from_millis(10));
    }
    running.signal(Signal::INT);
    let output = running.finish();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "rust_mutants: stopped before every shard finished\n  caused by: interrupted\n"
    );
    for index in 0..2 {
        assert!(
            fixture.ended(&format!("shard-{index}.process")),
            "shard {index} was not stopped"
        );
        assert!(!fixture.work().join(format!("copy-{index}")).exists());
    }
}

#[test]
fn a_cargo_mutants_failure_is_reported_as_a_failure() {
    let fixture = capable("failure");
    let run = fixture.run(&[("CODE", "4")]);
    assert_eq!(run.status(), 1);
    assert!(!run.stdout().contains("missed:"), "{}", run.stdout());
    let work = fixture.work();
    assert_eq!(
        run.stderr(),
        format!(
            "rust_mutants: shard 0 stopped with exit code 4; its log is under {}\n\
             rust_mutants: shard 1 stopped with exit code 4; its log is under {}\n",
            work.join("shard-0/mutants.out").display(),
            work.join("shard-1/mutants.out").display()
        )
    );
}
