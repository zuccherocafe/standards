//! Steps run side by side, four at once unless `--steps-at-once` says otherwise, with exclusive steps last and alone.
//! Every process a step starts ends with its step: when it runs out of time, when it exits and leaves children behind,
//! and when dandegate itself is stopped.

use std::fmt::Write as _;

use rustix::process::{Signal, WaitOptions, getpid, set_child_subreaper, waitpid};

use crate::support::{Fixture, Reporter, Run};

/// Holds each wide step until four have started, so the first four are running together when they finish.
const WIDE: &str = r#". "$(dirname "$0")/lib.sh"
four_started() { [ "$(started wide)" -ge 4 ]; }
event "start $1"
wait_until four_started
event "finish $1"
"#;

const ALONE: &str = r#". "$(dirname "$0")/lib.sh"
event "start $1"
event "finish $1"
"#;

/// Starts a child that would outlive the step, records both, and waits for it.
const SLOW: &str = r#". "$(dirname "$0")/lib.sh"
sleep 600 &
record child $!
record shell $$
event "start slow"
wait
"#;

/// Runs rust-mutants, whose shards start sessions of their own, and waits for it once both shards have started.
const HELPER: &str = r#". "$(dirname "$0")/lib.sh"
two_shards() { [ -f "$EVENTS/shard-0" ] && [ -f "$EVENTS/shard-1" ]; }
"$1" --workspace "$2" --package toy --out "$3" &
wait_until two_shards
event "start helper"
wait
"#;

/// A stand-in cargo-mutants shard: records its number and start time, then holds.
const SHARD: &str = r#"#!/bin/sh
shard=$(printf '%s\n' "$@" | sed -n '/^--shard$/{n;p;}' | cut -d / -f 1)
printf '%s %s\n' $$ "$(cut -d ' ' -f 22 /proc/$$/stat)" > "$EVENTS/shard-$shard"
exec sleep 600
"#;

/// Starts a child, records it, and exits 0 at once, leaving the child running.
const LEAVES: &str = r#". "$(dirname "$0")/lib.sh"
sleep 600 &
record child $!
event "start leaves"
"#;

/// Starts a child, records it and the step's supervisor, its parent, and waits.
const HOLDS: &str = r#". "$(dirname "$0")/lib.sh"
sleep 600 &
record child $!
record supervisor $PPID
event "start holds"
wait
"#;

#[test]
fn independent_steps_run_four_at_once_and_exclusive_steps_last_and_alone() {
    let fixture = Fixture::new("parallel");
    fixture.write("dandegate/wide.sh", WIDE);
    fixture.write("dandegate/alone.sh", ALONE);
    let mut plan = String::new();
    for index in 1..=6 {
        // A scheduler that never runs four at once would hold these until their time runs out.
        let _ = write!(
            plan,
            "[[steps.step]]\nname = \"wide{index}\"\nruns = \"once\"\ntimeout = \"60s\"\n\
             run = [\"sh\", \"{{adapters}}/wide.sh\", \"wide{index}\"]\n\n"
        );
    }
    for index in 1..=2 {
        let _ = write!(
            plan,
            "[[steps.step]]\nname = \"alone{index}\"\nruns = \"once\"\nexclusive = true\n\
             run = [\"sh\", \"{{adapters}}/alone.sh\", \"alone{index}\"]\n\n"
        );
    }
    // An ordinary step declared after the exclusive ones, ready only once every wide step has finished.
    plan.push_str(
        "[[steps.step]]\nname = \"late\"\nruns = \"once\"\n\
         needs = [\"wide1\", \"wide2\", \"wide3\", \"wide4\", \"wide5\", \"wide6\"]\n\
         run = [\"sh\", \"{adapters}/alone.sh\", \"late\"]\n",
    );
    fixture.write("dandegate.toml", &plan);
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    run.page();
    assert_eq!(run.printed("DANDEGATE-STEP-FAILED"), Vec::<String>::new());
    // The report's log records each step's start when it is launched and its outcome when its process has ended.
    let log = run.file("report.log");
    let instances: Vec<&str> = log
        .lines()
        .filter_map(|line| line.strip_prefix("=== both "))
        .collect();
    // The scheduler launches a step before its process can end, so the log never holds more than four launches
    // without their ends.
    let mut launched = 0_i32;
    let mut most_launched = 0;
    for line in &instances {
        launched += if line.ends_with(": started") { 1 } else { -1 };
        most_launched = most_launched.max(launched);
    }
    assert_eq!(most_launched, 4, "{instances:?}");
    for name in ["alone1", "alone2"] {
        let start = instances
            .iter()
            .position(|line| *line == format!("{name}: started"))
            .expect("it started");
        // Every ordinary step had ended before it was launched.
        let ordinary_ended = instances
            .iter()
            .position(|line| line.starts_with("late: exited"));
        let last_wide_ended = instances
            .iter()
            .rposition(|line| line.starts_with("wide") && !line.ends_with(": started"));
        assert!(
            ordinary_ended < Some(start) && last_wide_ended < Some(start),
            "{name} started early: {instances:?}"
        );
        // Nothing was launched or ended between its launch and its end.
        assert!(
            instances[start + 1].starts_with(&format!("{name}: exited 0 in ")),
            "{name} ran beside another: {instances:?}"
        );
    }
    let events = fixture.events();
    let mut running = 0_i32;
    let mut most = 0;
    for event in &events {
        running += if event.starts_with("start ") { 1 } else { -1 };
        most = most.max(running);
    }
    assert_eq!(most, 4, "{events:?}");
    let last_wide = events
        .iter()
        .rposition(|event| event.starts_with("finish wide"))
        .expect("the wide steps finished");
    for name in ["alone1", "alone2"] {
        let start = events
            .iter()
            .position(|event| *event == format!("start {name}"))
            .expect("it started");
        assert!(
            start > last_wide,
            "{name} started before every other step finished: {events:?}"
        );
        assert_eq!(
            events[start + 1],
            format!("finish {name}"),
            "{name} ran beside another: {events:?}"
        );
    }
}

/// Holds each step a little, so a second step started beside it would overlap it.
const PAUSED: &str = r#". "$(dirname "$0")/lib.sh"
event "start $1"
sleep 0.3
event "finish $1"
"#;

#[test]
fn with_one_step_at_once_no_two_steps_overlap_and_zero_is_refused() {
    let fixture = Fixture::new("one-at-once");
    fixture.write("dandegate/paused.sh", PAUSED);
    let mut plan = String::new();
    for index in 1..=4 {
        let _ = write!(
            plan,
            "[[steps.step]]\nname = \"paused{index}\"\nruns = \"once\"\n\
             run = [\"sh\", \"{{adapters}}/paused.sh\", \"paused{index}\"]\n\n"
        );
    }
    fixture.write("dandegate.toml", &plan);
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only, "--steps-at-once", "1"]);
    run.page();
    assert_eq!(run.printed("DANDEGATE-STEP-FAILED"), Vec::<String>::new());
    let events = fixture.events();
    assert_eq!(events.len(), 8, "{events:?}");
    for pair in events.chunks(2) {
        let name = pair[0].strip_prefix("start ").expect("a start first");
        assert_eq!(
            pair[1],
            format!("finish {name}"),
            "two steps overlapped: {events:?}"
        );
    }

    let zero = fixture.outside("zero");
    let refused = fixture
        .command(&[
            "--before",
            &only,
            "--out",
            zero.to_str().expect("UTF-8"),
            "--steps-at-once",
            "0",
        ])
        .output()
        .expect("the binary starts");
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("0 steps at once would run nothing"));
    assert!(!zero.exists());
}

/// The workspace's rust-mutants as its source now stands, built by the Cargo running these tests, which reports where
/// it put the binary. dandegate cannot depend on another package's binary, so Cargo would not build it otherwise.
fn rust_mutants() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let built = std::process::Command::new(env!("CARGO"))
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest)
        .args([
            "--locked",
            "--package",
            "rust-mutants",
            "--bin",
            "rust-mutants",
        ])
        .arg("--message-format=json-render-diagnostics")
        .stderr(std::process::Stdio::inherit())
        .output()
        .expect("cargo starts");
    assert!(built.status.success(), "rust-mutants did not build");
    String::from_utf8(built.stdout)
        .expect("cargo prints UTF-8")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|message| {
            let executable = message.get("executable")?.as_str()?;
            (message.get("reason")?.as_str()? == "compiler-artifact").then(|| executable.into())
        })
        .expect("cargo reports the rust-mutants binary")
}

#[test]
fn a_step_out_of_time_is_stopped_with_its_children_while_the_rest_finishes() {
    let fixture = Fixture::new("timeout");
    fixture.write("dandegate/slow.sh", SLOW);
    fixture.write("dandegate/helper.sh", HELPER);
    fixture.tool("cargo-mutants", SHARD);
    // rust-mutants gives cargo-mutants the Cargo on PATH; the stand-in shard never runs it.
    fixture.tool("cargo", "#!/bin/sh\nexit 1\n");
    let helper = rust_mutants();
    fixture.write(
        "dandegate.toml",
        &r#"
[[steps.step]]
name = "slow"
runs = "once"
timeout = "1s"
run = ["sh", "{adapters}/slow.sh"]

# Long enough for the helper to start both shards, which the step waits for before it waits for the helper.
[[steps.step]]
name = "helper"
runs = "once"
timeout = "5s"
run = ["sh", "{adapters}/helper.sh", "HELPER", "{after}", "{out}"]

[[steps.step]]
name = "quick"
runs = "once"
run = ["sh", "{adapters}/step.sh", "quick", "{after}", "{after-commit}", "{out}"]
output = [{ kind = "section", file = "{out}/quick.md" }]
"#
        .replace("HELPER", helper.to_str().expect("UTF-8")),
    );
    fixture.write("crates/toy/src/lib.rs", "pub fn one() -> u8 {\n    1\n}\n");
    fixture.write("fixture/quick/quick.md", "# Quick\n");
    fixture.write("fixture/quick/quick.md.status", "complete\n");
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        [
            "both slow: stopped after 1 s, its time limit",
            "both helper: stopped after 5 s, its time limit"
        ]
    );
    assert!(page.contains("<pre># Quick\n</pre>"), "{page}");
    assert!(fixture.ended("shell"), "the step's shell was not reaped");
    assert!(fixture.ended("child"), "the step's child was not reaped");
    assert!(fixture.events().contains(&"start helper".to_owned()));
    for shard in ["shard-0", "shard-1"] {
        assert!(
            fixture.ended(shard),
            "rust-mutants' {shard}, in its own session, was not reaped"
        );
    }
}

#[test]
fn what_a_finished_step_leaves_running_is_stopped_with_it() {
    let fixture = Fixture::new("leaves");
    fixture.write("dandegate/leaves.sh", LEAVES);
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"leaves\"\nruns = \"once\"\nrun = [\"sh\", \"{adapters}/leaves.sh\"]\n",
    );
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    run.page();
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    assert!(fixture.ended("child"), "the step's child was not reaped");
}

#[test]
fn a_step_ended_by_a_signal_its_supervisor_ignores_exits_as_a_shell_reports_it() {
    let fixture = Fixture::new("signalled");
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"piped\"\nruns = \"once\"\nrun = [\"sh\", \"-c\", \"kill -PIPE $$\"]\n",
    );
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    run.page();
    // The supervisor, as a Rust program, ignores SIGPIPE, so it exits with 128 and the signal's number instead.
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        ["both piped: failed: exited 141"]
    );
}

/// Copies the measured tree's `fixture/data/` into its output folder, first waiting for `$EVENTS/release` when `HOLD`
/// is set.
const HELD: &str = r#". "$(dirname "$0")/lib.sh"
released() { [ -f "$EVENTS/release" ]; }
event "start held"
if [ -n "${HOLD:-}" ]; then wait_until released; fi
cp -R "$1/fixture/data/." "$2/"
"#;

#[test]
fn two_reports_on_one_repository_each_in_its_own_pid_namespace_keep_their_own_checkouts() {
    let fixture = Fixture::new("namespaces");
    fixture.write("dandegate/held.sh", HELD);
    fixture.write(
        "dandegate.toml",
        r#"
[[steps.step]]
name = "held"
runs = "once"
run = ["sh", "{adapters}/held.sh", "{after}", "{out}"]
output = [{ kind = "section", file = "{out}/data.md" }]
"#,
    );
    fixture.write("fixture/data/data.md", "# Data\n");
    fixture.write("fixture/data/data.md.status", "complete\n");
    let only = fixture.commit("only");
    let out = |name: &str| fixture.outside(name).to_str().expect("UTF-8").to_owned();
    let (first_out, second_out) = (out("first"), out("second"));

    // Both binaries are process 1 of their namespaces. The first holds its step while the second runs to its end.
    let mut held = fixture.namespaced(&["--before", &only, "--out", &first_out]);
    held.env("HOLD", "1");
    let mut first = Reporter::start(held);
    fixture.wait_for_event("start held", &mut first);
    let second =
        Reporter::start(fixture.namespaced(&["--before", &only, "--out", &second_out])).finish();
    std::fs::write(fixture.events_folder().join("release"), "").expect("the release");
    let first = first.finish();

    for (name, output) in [("second", second), ("first", first)] {
        let run = Run {
            output,
            out: fixture.outside(name),
        };
        let page = run.page();
        assert_eq!(
            run.printed("DANDEGATE-INCOMPLETE"),
            Vec::<String>::new(),
            "{name}"
        );
        assert!(page.contains("<pre># Data\n</pre>"), "{name}: {page}");
    }
    let scratch = fixture.repo().join(".cache/dandegate/scratch");
    assert_eq!(
        crate::support::files_under(&scratch),
        Vec::<String>::new(),
        "each run removed what it made"
    );
}

#[test]
fn stopping_dandegate_stops_every_step_it_started() {
    let fixture = Fixture::new("stopped");
    fixture.write("dandegate/holds.sh", HOLDS);
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"holds\"\nruns = \"once\"\nrun = [\"sh\", \"{adapters}/holds.sh\"]\n",
    );
    let only = fixture.commit("only");
    // The step's supervisor becomes this process's child when dandegate ends, so this test can wait for it.
    set_child_subreaper(Some(getpid())).expect("this process can adopt orphans");
    let out = fixture.outside("out");
    let mut reporter = Reporter::start(fixture.command(&[
        "--before",
        &only,
        "--out",
        out.to_str().expect("UTF-8"),
    ]));
    fixture.wait_for_event("start holds", &mut reporter);

    let output = reporter.stop(Signal::TERM);
    assert!(!output.status.success(), "dandegate was not stopped");
    let (supervisor, _) = fixture.recorded("supervisor");
    // The supervisor exits only once it has stopped and reaped everything below it.
    waitpid(Some(supervisor), WaitOptions::empty())
        .expect("the supervisor is this process's child");
    assert!(fixture.ended("child"), "the step's child was not reaped");
}
