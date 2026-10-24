//! The host programs `dandegate.toml` names: every command, collector and built-in command finds them after its
//! revision shell's own tools, and finds nothing else from the caller's `PATH`. A name dandegate cannot find is a
//! warning on the page; a step that runs it finds it only if its shell provides it. A step that runs a host program is
//! stopped with everything it started, as any other step is.

use rustix::process::{Signal, WaitOptions, getpid, set_child_subreaper, waitpid};

use crate::support::{Fixture, Reporter};

/// Writes `$2` into the file `$1`.
const WRITES: &str = "#!/bin/sh\necho \"$2\" > \"$1\"\n";

/// Prints a `name=value` line for an identity collector, or writes `collected` into the file `$1`.
const NAMED: &str =
    "#!/bin/sh\nif [ -n \"$1\" ]; then echo collected > \"$1\"; else echo named=1; fi\n";

/// Starts a child that would outlive it, records both it and its parent, the step's supervisor, then waits.
const HOLD: &str = r#"#!/bin/sh
sleep 600 &
printf '%s %s\n' $! "$(cut -d ' ' -f 22 /proc/$!/stat)" > "$EVENTS/child"
printf '%s %s\n' $PPID "$(cut -d ' ' -f 22 /proc/$PPID/stat)" > "$EVENTS/supervisor"
printf '%s\n' "start hold" >> "$EVENTS/events"
wait
"#;

#[test]
fn a_step_finds_its_named_host_programs_after_its_shells_tools_and_nothing_else_of_the_callers() {
    let fixture = Fixture::new("host");
    fixture.tool("both", &WRITES.replace("$2", "shell"));
    fixture.caller_tool("both", &WRITES.replace("$2", "host"));
    fixture.caller_tool("named", NAMED);
    fixture.caller_tool("unnamed", &WRITES.replace("$2", "host"));
    fixture.write(
        "dandegate.toml",
        r#"[project]
host = ["both", "named"]

[[steps.step]]
name = "both"
runs = "once"
run = ["both", "{out}/written.txt"]

[[steps.step]]
name = "named"
runs = "once"
reuse = true
run = ["named", "{out}/written.txt"]
identity = { collect = ["named"] }

[[steps.step]]
name = "unnamed"
runs = "once"
run = ["unnamed", "{out}/written.txt"]
"#,
    );
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    assert_eq!(run.file("steps/both/both/out/written.txt"), "shell\n");
    assert_eq!(run.file("steps/both/named/out/written.txt"), "collected\n");
    assert!(
        run.file("report.log")
            .contains("=== both named: kept for reuse"),
        "the collector found the host program"
    );
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        ["both unnamed: failed: exited 127"]
    );
    assert!(!run.exists("steps/both/unnamed/out/written.txt"));
    assert!(
        run.file("steps/both/unnamed/log.txt")
            .contains("dandegate: unnamed: "),
        "the missing command is in the step's log"
    );
    assert!(!page.contains("class=\"warning host\""), "{page}");
}

#[test]
fn a_host_program_not_found_is_one_warning_and_fails_only_the_step_that_runs_it() {
    let fixture = Fixture::new("host-missing");
    fixture.caller_tool("named", NAMED);
    fixture.write(
        "dandegate.toml",
        r#"[project]
host = ["absent", "named"]

[[steps.step]]
name = "absent"
runs = "once"
run = ["absent"]

[[steps.step]]
name = "named"
runs = "once"
run = ["named", "{out}/written.txt"]

[[steps.step]]
name = "plain"
runs = "once"
run = ["sh", "-c", "echo plain > \"$0\"", "{out}/written.txt"]
"#,
    );
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    assert_eq!(
        page.matches("<div class=\"warning host\">").count(),
        1,
        "{page}"
    );
    assert!(
        page.contains(
            "<div class=\"warning host\"><p>host program absent is not on dandegate&#39;s PATH</p></div>"
        ),
        "{page}"
    );
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        ["both absent: failed: exited 127"]
    );
    assert_eq!(run.file("steps/both/named/out/written.txt"), "collected\n");
    assert_eq!(run.file("steps/both/plain/out/written.txt"), "plain\n");
}

#[test]
fn a_built_in_command_finds_the_host_programs() {
    let fixture = Fixture::new("host-built-in");
    crate::rust::workspace(&fixture);
    fixture.caller_tool("listed", "#!/bin/sh\necho 'pub fn toy::host()'\n");
    fixture.write(
        "dandegate.toml",
        "[project]\nhost = [\"listed\"]\n[project.rust]\nworkspace = \"./\"\n[reports.rust]\nscans = [\"--core\", \"alpha\"]\n",
    );
    let only = fixture.commit("only");

    // The stand-in cargo runs the program RUSTDOC names, here by its bare name.
    let run = fixture.run_with(
        "out",
        &["--before", &only, "--with", "api"],
        &[("RUSTDOC", "listed")],
    );
    run.page();
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    assert_eq!(
        run.file("steps/after/api-listings/out/alpha.txt"),
        "pub fn toy::host()\n"
    );
}

#[test]
fn a_host_program_out_of_time_is_stopped_with_its_children() {
    let fixture = Fixture::new("host-timeout");
    fixture.caller_tool("hold", HOLD);
    fixture.write(
        "dandegate.toml",
        "[project]\nhost = [\"hold\"]\n[[steps.step]]\nname = \"hold\"\nruns = \"once\"\ntimeout = \"1s\"\nrun = [\"hold\"]\n",
    );
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    run.page();
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        ["both hold: stopped after 1 s, its time limit"]
    );
    assert!(fixture.ended("child"), "the program's child was not reaped");
}

#[test]
fn stopping_dandegate_stops_a_host_program_and_everything_it_started() {
    let fixture = Fixture::new("host-stopped");
    fixture.caller_tool("hold", HOLD);
    fixture.write(
        "dandegate.toml",
        "[project]\nhost = [\"hold\"]\n[[steps.step]]\nname = \"hold\"\nruns = \"once\"\nrun = [\"hold\"]\n",
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
    fixture.wait_for_event("start hold", &mut reporter);

    let output = reporter.stop(Signal::TERM);
    assert!(!output.status.success(), "dandegate was not stopped");
    // The program's parent is the supervisor itself: entering the shell with the host programs adds no process.
    let (supervisor, _) = fixture.recorded("supervisor");
    waitpid(Some(supervisor), WaitOptions::empty())
        .expect("the supervisor is this process's child");
    assert!(fixture.ended("child"), "the program's child was not reaped");
}

#[test]
fn a_kept_result_misses_when_the_after_revision_changes_only_its_host_programs() {
    let fixture = Fixture::new("host-reuse");
    fixture.caller_tool("named", NAMED);
    let step = "[[steps.step]]\nname = \"named\"\nreuse = true\nrun = [\"named\", \"{out}/written.txt\"]\n";
    fixture.write(
        "dandegate.toml",
        &format!("[project]\nhost = [\"named\"]\n{step}"),
    );
    let before = fixture.commit("before");
    fixture.write("README", "the after revision\n");
    let declared = fixture.commit("declared");
    fixture.write("dandegate.toml", step);
    let undeclared = fixture.commit("undeclared");

    let first = fixture.run("first", &["--before", &before, "--after", &declared]);
    first.page();
    assert_eq!(first.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    let same = fixture.run("same", &["--before", &before, "--after", &declared]);
    same.page();
    assert!(
        same.printed("DANDEGATE-REUSED")
            .iter()
            .any(|line| line.starts_with("before named")),
        "{:?}",
        same.printed("DANDEGATE-REUSED")
    );

    // Only the after revision's plan changed, and with it what the before side's command can run.
    let changed = fixture.run("changed", &["--before", &before, "--after", &undeclared]);
    changed.page();
    assert_eq!(changed.printed("DANDEGATE-REUSED"), Vec::<String>::new());
    assert_eq!(
        changed.printed("DANDEGATE-INCOMPLETE"),
        [
            "before named: failed: exited 127",
            "after named: failed: exited 127"
        ]
    );
}
