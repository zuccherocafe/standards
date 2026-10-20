//! A step's build cache, `{cache}`: kept from one report to the next for each step and place, and never used by two
//! reports at once.

use std::fs::File;

use rustix::fs::{FlockOperation, flock};
use rustix::process::{Signal, WaitOptions, getpid, kill_process, set_child_subreaper, waitpid};

use crate::support::{Fixture, Reporter, Run};

/// Starts a child that would outlive the step, records it and the step's supervisor, its parent, and waits.
const HOLDS: &str = r#". "$(dirname "$0")/lib.sh"
sleep 600 &
record child $!
record supervisor $PPID
event "start holds"
wait
"#;

/// Whether no process holds the lock file `lock`.
fn free(lock: &std::path::Path) -> bool {
    let file = File::open(lock).expect("the lock file");
    flock(&file, FlockOperation::NonBlockingLockExclusive).is_ok()
}

/// Counts its runs in the cache it is given and records the count, then, with `HOLD` set, holds until released.
const BUILD: &str = r#". "$(dirname "$0")/lib.sh"
released() { [ -f "$EVENTS/release" ]; }
count=$(cat "$1/count" 2>/dev/null || echo 0)
count=$((count + 1))
echo "$count" > "$1/count"
event "build $2 $1 $count"
if [ -n "${HOLD:-}" ]; then wait_until released; fi
"#;

fn project(fixture: &Fixture) -> String {
    fixture.write("dandegate/build.sh", BUILD);
    fixture.write(
        "dandegate.toml",
        r#"
[[steps.step]]
name = "build"
run = ["sh", "{adapters}/build.sh", "{cache}", "{tree}"]

[[steps.step]]
name = "plain"
runs = "once"
run = ["true"]

[[steps.step]]
name = "spare"
runs = "once"
optional = true
run = ["true", "{cache}"]
"#,
    );
    fixture.commit("only")
}

/// Each build event's side, cache folder and count, sorted.
fn builds(fixture: &Fixture) -> Vec<(String, String, String)> {
    let mut found: Vec<(String, String, String)> = fixture
        .events()
        .iter()
        .filter_map(|event| event.strip_prefix("build "))
        .map(|event| {
            let words: Vec<&str> = event.split(' ').collect();
            let side = if words[0].ends_with("/before") {
                "before"
            } else {
                "after"
            };
            (side.to_owned(), words[1].to_owned(), words[2].to_owned())
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_build_cache_is_kept_for_each_step_and_side_from_one_report_to_the_next() {
    let fixture = Fixture::new("cache-kept");
    let only = project(&fixture);
    let kept = |side: &str| {
        fixture
            .repo()
            .join(".cache/dandegate/builds/build")
            .join(side)
            .to_str()
            .expect("UTF-8")
            .to_owned()
    };
    for (out, count) in [("first", "1"), ("second", "2")] {
        fixture.clear_events();
        fixture.run(out, &["--before", &only]).page();
        assert_eq!(
            builds(&fixture),
            [
                ("after".to_owned(), kept("after"), count.to_owned()),
                ("before".to_owned(), kept("before"), count.to_owned()),
            ]
        );
    }
    // Only an enabled step that names its cache has one: not a step that names none, nor one not selected.
    let steps: Vec<String> = std::fs::read_dir(fixture.repo().join(".cache/dandegate/builds"))
        .expect("the caches")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(steps, ["build"]);
}

#[test]
fn a_build_cache_another_report_holds_is_not_shared_and_the_page_says_so() {
    let fixture = Fixture::new("cache-held");
    let only = project(&fixture);
    let out = |name: &str| fixture.outside(name).to_str().expect("UTF-8").to_owned();
    let (first_out, second_out) = (out("first"), out("second"));

    let mut held = fixture.command(&["--before", &only, "--out", &first_out]);
    held.env("HOLD", "1");
    let mut first = Reporter::start(held);
    while builds(&fixture).len() < 2 {
        assert!(first.running(), "the first report ended before both builds");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let second = Run {
        output: Reporter::start(fixture.command(&["--before", &only, "--out", &second_out]))
            .finish(),
        out: fixture.outside("second"),
    };
    std::fs::write(fixture.events_folder().join("release"), "").expect("the release");
    let first = Run {
        output: first.finish(),
        out: fixture.outside("first"),
    };
    first.page();
    let page = second.page();
    assert!(
        page.contains(
            "in use by another report, so it builds in a new folder of this report&#39;s"
        ),
        "{page}"
    );

    let found = builds(&fixture);
    assert_eq!(found.len(), 4, "{found:?}");
    let kept = fixture.repo().join(".cache/dandegate/builds/build");
    let in_kept = found
        .iter()
        .filter(|(_, folder, _)| folder.starts_with(kept.to_str().expect("UTF-8")))
        .count();
    assert_eq!(
        in_kept, 2,
        "only the first report used the kept folders: {found:?}"
    );
    let scratch = fixture.repo().join(".cache/dandegate/scratch");
    assert!(
        found.iter().all(
            |(_, folder, _)| folder.starts_with(kept.to_str().expect("UTF-8"))
                || folder.starts_with(scratch.to_str().expect("UTF-8"))
        ),
        "the second report's folders were its own scratch's: {found:?}"
    );
    assert!(
        found.iter().all(|(_, _, count)| count == "1"),
        "the second report's folders were new: {found:?}"
    );
}

#[test]
fn the_caches_stay_locked_until_every_process_of_a_killed_report_has_ended() {
    let fixture = Fixture::new("cache-inherited");
    fixture.write("dandegate/holds.sh", HOLDS);
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"holds\"\nruns = \"once\"\nrun = [\"sh\", \"{adapters}/holds.sh\", \"{cache}\"]\n",
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
    let (supervisor, _) = fixture.recorded("supervisor");
    // Stopped, the supervisor cannot stop the step when dandegate ends, so the step and its child live on.
    kill_process(supervisor, Signal::STOP).expect("the supervisor runs");
    reporter.stop(Signal::KILL);
    let lock = fixture.repo().join(".cache/dandegate/builds.lock");
    assert!(
        !free(&lock),
        "the step's processes still hold the lock dandegate took"
    );
    kill_process(supervisor, Signal::CONT).expect("the supervisor is stopped");
    waitpid(Some(supervisor), WaitOptions::empty())
        .expect("the supervisor is this process's child");
    assert!(fixture.ended("child"), "the step's child was not reaped");
    assert!(free(&lock), "the last process holding the lock has ended");
}

#[test]
fn a_report_whose_running_steps_name_no_cache_leaves_the_caches_alone() {
    let fixture = Fixture::new("cache-unnamed");
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"plain\"\nruns = \"once\"\nrun = [\"true\"]\n\n\
         [[steps.step]]\nname = \"spare\"\nruns = \"once\"\noptional = true\nrun = [\"true\", \"{cache}\"]\n",
    );
    let only = fixture.commit("only");
    fixture.run("out", &["--before", &only]).page();
    assert!(
        !fixture.repo().join(".cache/dandegate/builds.lock").exists(),
        "only a report with an enabled step that names {{cache}} takes the caches"
    );
}
