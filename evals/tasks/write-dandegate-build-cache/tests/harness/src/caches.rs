//! A step's build cache, `{cache}`: kept from one report to the next for each step and place, and never used by two
//! reports at once.

use rustix::process::{Pid, Signal, WaitOptions, getpid, kill_process, set_child_subreaper, wait};

use crate::support::{Fixture, Reporter, Run};

/// Records the supplied cache and keeps descendants alive only in the report deliberately killed by the test.
const HOLDS: &str = r#". "$(dirname "$0")/lib.sh"
event "cache $1"
if [ -n "${HOLD:-}" ]; then
    sleep 600 &
    record child $!
    event "start holds"
    wait
fi
"#;

/// Frozen processes are killed and reaped even if a competing report fails an assertion.
struct Stopped(Vec<Pid>);

impl Stopped {
    fn tree(root: Pid) -> Self {
        let mut stopped = Self(Vec::new());
        let mut pending = vec![root];
        while let Some(pid) = pending.pop() {
            kill_process(pid, Signal::STOP).expect("a live process can be frozen");
            stopped.0.push(pid);
            // Freeze parents first, so no process can spawn a child after its children have been enumerated.
            let tasks = std::fs::read_dir(format!("/proc/{}/task", pid.as_raw_pid()))
                .expect("the process's threads");
            for task in tasks {
                let children =
                    std::fs::read_to_string(task.expect("a thread").path().join("children"))
                        .expect("the thread's children");
                for child in children.split_whitespace() {
                    let child =
                        Pid::from_raw(child.parse().expect("a process number")).expect("not zero");
                    if !stopped.0.contains(&child) && !pending.contains(&child) {
                        pending.push(child);
                    }
                }
            }
        }
        stopped
    }

    fn kill(&self) {
        for &pid in &self.0 {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
}

impl Drop for Stopped {
    fn drop(&mut self) {
        self.kill();
        loop {
            match wait(WaitOptions::empty()) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::CHILD) => break,
                Err(error) => panic!("cannot reap held descendants: {error}"),
            }
        }
    }
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
    let notice = page.to_lowercase();
    let ownership = ["another report", "other report", "another run", "other run"]
        .iter()
        .any(|text| notice.contains(text));
    let fallback = [
        "scratch",
        "temporary",
        "new folder",
        "new cache",
        "fresh folder",
    ]
    .iter()
    .any(|text| notice.contains(text));
    assert!(
        notice.contains("cache") && ownership && fallback,
        "the page must explain cache contention and the isolated fallback: {page}"
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
    set_child_subreaper(Some(getpid())).expect("this process can adopt orphans");
    let out = fixture.outside("out");
    let mut command = fixture.command(&["--before", &only, "--out", out.to_str().expect("UTF-8")]);
    command.env("HOLD", "1");
    let mut reporter = Reporter::start(command);
    fixture.wait_for_event("start holds", &mut reporter);
    let root = reporter.pid();
    let mut stopped = Stopped::tree(root);
    assert!(stopped.0.len() >= 3, "the report has command descendants");
    kill_process(root, Signal::KILL).expect("the reporter runs");
    while reporter.running() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    stopped.0.retain(|pid| *pid != root);
    fixture.clear_events();
    let competing = fixture.run("competing", &["--before", &only]);
    let temporary = fixture.events();
    stopped.kill();
    reporter.finish();
    drop(stopped);
    let (child, _) = fixture.recorded("child");
    assert!(
        fixture.ended("child"),
        "the held descendant was reaped: {:?}",
        std::fs::read_to_string(format!("/proc/{}/stat", child.as_raw_pid()))
    );
    competing.page();
    let cache = temporary
        .iter()
        .find_map(|event| event.strip_prefix("cache "))
        .expect("the competing step records its cache");
    assert!(
        std::path::Path::new(cache).starts_with(fixture.repo().join(".cache/dandegate/scratch")),
        "a report cannot take persistent storage while killed-report descendants remain: {cache}"
    );
    assert!(
        !std::path::Path::new(cache).exists(),
        "the private cache was cleaned up"
    );
    fixture.clear_events();
    fixture.run("released", &["--before", &only]).page();
    assert_eq!(
        fixture.events(),
        [format!(
            "cache {}",
            fixture
                .repo()
                .join(".cache/dandegate/builds/holds/both")
                .display()
        )],
        "persistent caches become available after all writers exit"
    );
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
    let lock = fixture.repo().join(".cache/dandegate/builds.lock");
    assert!(
        !lock.exists(),
        "only a report with an enabled step that names {{cache}} takes the caches"
    );
    let cache = fixture.repo().join(".cache/dandegate/builds/kept/before");
    std::fs::create_dir_all(&cache).expect("an existing cache");
    let payload = cache.join("sparse");
    std::fs::File::create(&payload)
        .and_then(|file| file.set_len(33_u64 << 30))
        .expect("a sparse cache above the byte bound");
    std::fs::write(&lock, "untouched").expect("an existing lock file");
    let used = cache
        .metadata()
        .expect("cache metadata")
        .modified()
        .expect("a use time");
    let locked = lock
        .metadata()
        .expect("lock metadata")
        .modified()
        .expect("a lock time");
    fixture.run("existing", &["--before", &only]).page();
    assert_eq!(
        payload.metadata().expect("cache preserved").len(),
        33_u64 << 30
    );
    assert_eq!(
        cache
            .metadata()
            .expect("cache metadata")
            .modified()
            .expect("a use time"),
        used
    );
    assert_eq!(
        lock.metadata()
            .expect("lock metadata")
            .modified()
            .expect("a lock time"),
        locked
    );
    assert_eq!(
        std::fs::read_to_string(lock).expect("lock preserved"),
        "untouched"
    );
}
