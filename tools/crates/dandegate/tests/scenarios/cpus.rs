//! A step with a CPU share runs on CPUs of its own beside the other steps, which run on the rest, or alone when the
//! CPUs are too few to split. Each scenario narrows the CPUs dandegate may use, as its caller can, so the split does
//! not depend on the machine.

use rustix::thread::{CpuSet, sched_getaffinity, sched_setaffinity};

use crate::support::Fixture;

/// Records the CPUs it runs on and the `{cpus}` it was given, then waits until the step named by `$3` has started too,
/// when one is named, so the two run at once.
const CPUS: &str = r#". "$(dirname "$0")/lib.sh"
allowed=$(grep Cpus_allowed_list /proc/self/status | cut -f 2)
event "start $1 $allowed $2"
if [ -n "$3" ]; then wait_until grep -q "^start $3 " "$EVENTS/events"; fi
event "finish $1"
"#;

/// Narrow this thread's CPUs, which the binary it starts inherits, to the first `count` it may use, and give them, or
/// none when it may use fewer.
fn narrow(count: usize) -> Option<Vec<usize>> {
    let mut allowed = sched_getaffinity(None).expect("this thread's CPUs");
    let mut kept = Vec::new();
    for cpu in 0..CpuSet::MAX_CPU {
        if allowed.is_set(cpu) {
            if kept.len() == count {
                allowed.unset(cpu);
            } else {
                kept.push(cpu);
            }
        }
    }
    if kept.len() < count {
        return None;
    }
    sched_setaffinity(None, &allowed).expect("this thread's CPUs can be narrowed");
    Some(kept)
}

/// CPUs as dandegate's `{cpus}` writes them: numbers joined by commas.
fn commas(cpus: &[usize]) -> String {
    cpus.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// CPUs as the kernel's `Cpus_allowed_list` writes them: ranges joined by commas.
fn list(cpus: &[usize]) -> String {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &cpu in cpus {
        match ranges.last_mut() {
            Some((_, last)) if *last + 1 == cpu => *last = cpu,
            _ => ranges.push((cpu, cpu)),
        }
    }
    ranges
        .iter()
        .map(|&(first, last)| {
            if first == last {
                first.to_string()
            } else {
                format!("{first}-{last}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// A project of a step `own` with `share` CPUs and an ordinary step `other`, each recording its CPUs and, with `wait`,
/// waiting for the other to start.
fn project(fixture: &Fixture, share: usize, wait: bool) -> String {
    fixture.write("dandegate/cpus.sh", CPUS);
    let waits = |name: &'static str| if wait { name } else { "" };
    fixture.write(
        "dandegate.toml",
        &format!(
            "[[steps.step]]\nname = \"own\"\nruns = \"once\"\ncpus = {share}\ntimeout = \"60s\"\n\
             run = [\"sh\", \"{{adapters}}/cpus.sh\", \"own\", \"{{cpus}}\", \"{}\"]\n\n\
             [[steps.step]]\nname = \"other\"\nruns = \"once\"\ntimeout = \"60s\"\n\
             run = [\"sh\", \"{{adapters}}/cpus.sh\", \"other\", \"{{cpus}}\", \"{}\"]\n",
            waits("other"),
            waits("own"),
        ),
    );
    fixture.commit("only")
}

/// Each step's recorded start: the CPUs it ran on and the `{cpus}` it was given.
fn started(fixture: &Fixture, step: &str) -> (String, String) {
    let lead = format!("start {step} ");
    let event = fixture
        .events()
        .into_iter()
        .find_map(|event| event.strip_prefix(&lead).map(str::to_owned))
        .expect("the step started");
    let (allowed, given) = event.split_once(' ').expect("two lists");
    (allowed.to_owned(), given.to_owned())
}

#[test]
fn a_step_with_a_share_runs_on_cpus_of_its_own_while_the_others_run_on_the_rest() {
    let Some(given) = narrow(4) else {
        panic!("the test needs four CPUs it may use");
    };
    let fixture = Fixture::new("cpus-shared");
    let only = project(&fixture, 2, true);
    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    let (own, rest) = (&given[2..], &given[..2]);
    assert!(
        run.file("report.log").contains(&format!(
            "=== CPUs: dandegate may use {}; the steps with a share run on their own, the others on {}\n",
            commas(&given),
            commas(rest)
        )),
        "the log says how the CPUs were shared"
    );
    assert_eq!(started(&fixture, "own"), (list(own), commas(own)));
    assert_eq!(started(&fixture, "other"), (list(rest), commas(rest)));
    assert!(
        page.contains(&format!("{}, its own</td>", commas(own))),
        "{page}"
    );
    assert!(
        page.contains(&format!("<td>{}</td></tr>", commas(rest))),
        "the other step's row names the rest: {page}"
    );
}

#[test]
fn a_share_too_large_to_split_runs_alone_after_the_others_on_every_cpu() {
    let Some(given) = narrow(2) else {
        panic!("the test needs two CPUs it may use");
    };
    let fixture = Fixture::new("cpus-alone");
    let only = project(&fixture, 2, false);
    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    let all = (list(&given), commas(&given));
    assert!(
        run.file("report.log").contains(&format!(
            "=== CPUs: the shares ask for more than half of the CPUs dandegate may use, {}, so each step with a \
             share runs alone\n",
            all.1
        )),
        "the log says the shares run alone"
    );
    assert_eq!(started(&fixture, "own"), all);
    assert_eq!(started(&fixture, "other"), all);
    let events = fixture.events();
    let at = |event: &str| {
        events
            .iter()
            .position(|found| found.starts_with(event))
            .expect(event)
    };
    assert!(at("finish other") < at("start own"), "{events:?}");
    assert!(
        page.contains(&format!("{}, alone: too few to share", all.1)),
        "{page}"
    );
}

/// Writes the `{cpus}` it was given as its section.
const KEPT: &str = r#"printf '%s\n' "$1" > "$2/cpus.md"
echo complete > "$2/cpus.md.status"
"#;

#[test]
fn a_kept_result_that_names_its_cpus_misses_when_the_split_gives_it_others() {
    let Some(four) = narrow(4) else {
        panic!("the test needs four CPUs it may use");
    };
    let fixture = Fixture::new("cpus-kept");
    fixture.write("dandegate/kept.sh", KEPT);
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"own\"\nruns = \"once\"\ncpus = 1\nrun = [\"true\"]\n\n\
         [[steps.step]]\nname = \"kept\"\nruns = \"once\"\nreuse = true\n\
         run = [\"sh\", \"{adapters}/kept.sh\", \"{cpus}\", \"{out}\"]\n\
         output = [{ kind = \"section\", file = \"{out}/cpus.md\" }]\n\n\
         [[steps.step]]\nname = \"plain\"\nruns = \"once\"\nreuse = true\n\
         run = [\"sh\", \"{adapters}/kept.sh\", \"the same\", \"{out}\"]\n\
         output = [{ kind = \"section\", file = \"{out}/cpus.md\" }]\n",
    );
    let only = fixture.commit("only");
    let report = |out: &str| {
        let run = fixture.run(out, &["--before", &only]);
        run.page();
        let reused = |step: &str| {
            run.printed("DANDEGATE-REUSED")
                .iter()
                .any(|line| line.starts_with(&format!("both {step} ")))
        };
        // A step whose command names no CPUs is kept across any split.
        assert_eq!(reused("plain"), out != "four", "{out}");
        (run.file("steps/both/kept/out/cpus.md"), reused("kept"))
    };
    assert_eq!(report("four"), (format!("{}\n", commas(&four[..3])), false));
    assert_eq!(
        report("four-again"),
        (format!("{}\n", commas(&four[..3])), true)
    );
    let Some(two) = narrow(2) else {
        panic!("the test needs two CPUs it may use");
    };
    assert_eq!(report("two"), (format!("{}\n", commas(&two[..1])), false));
}

#[test]
fn a_kept_result_misses_when_a_step_it_needs_is_given_other_cpus() {
    let Some(four) = narrow(4) else {
        panic!("the test needs four CPUs it may use");
    };
    let fixture = Fixture::new("cpus-need");
    // prep writes its CPUs into the checkout; use, which names none, shows what prep left there.
    fixture.write(
        "dandegate.toml",
        "[[steps.step]]\nname = \"own\"\nruns = \"once\"\ncpus = 1\nrun = [\"true\"]\n\n\
         [[steps.step]]\nname = \"prep\"\nruns = \"after\"\n\
         run = [\"sh\", \"-c\", \"printf '%s\\\\n' \\\"$1\\\" > \\\"$2/prepared-cpus\\\"\", \"_\", \"{cpus}\", \"{tree}\"]\n\n\
         [[steps.step]]\nname = \"use\"\nruns = \"after\"\nreuse = true\nneeds = [\"prep\"]\n\
         run = [\"sh\", \"-c\", \"cp \\\"$1/prepared-cpus\\\" \\\"$2/cpus.md\\\" && echo complete > \\\"$2/cpus.md.status\\\"\", \
         \"_\", \"{tree}\", \"{out}\"]\n\
         output = [{ kind = \"section\", file = \"{out}/cpus.md\" }]\n",
    );
    let only = fixture.commit("only");
    let report = |out: &str| {
        let run = fixture.run(out, &["--before", &only]);
        run.page();
        let reused = run
            .printed("DANDEGATE-REUSED")
            .iter()
            .any(|line| line.starts_with("after use "));
        (run.file("steps/after/use/out/cpus.md"), reused)
    };
    assert_eq!(report("four"), (format!("{}\n", commas(&four[..3])), false));
    assert_eq!(
        report("four-again"),
        (format!("{}\n", commas(&four[..3])), true)
    );
    let Some(two) = narrow(2) else {
        panic!("the test needs two CPUs it may use");
    };
    assert_eq!(report("two"), (format!("{}\n", commas(&two[..1])), false));
}
