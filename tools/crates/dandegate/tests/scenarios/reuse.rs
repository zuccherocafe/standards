//! dandegate reuses a kept result while everything the result depends on is unchanged, and misses when anything
//! changes.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::selection::started;
use crate::support::{Fixture, LIB, STEP};

/// A reusable per-revision step with measures, running the generic step through `visual.sh`, which sources
/// `helper.sh`.
const VISUAL: &str = r#"
[[steps.step]]
name = "visual"
reuse = true
run = ["sh", "{adapters}/visual.sh", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]
"#;

const VISUAL_SH: &str = r#". "$(dirname "$0")/helper.sh"
sh "$(dirname "$0")/step.sh" visual "$1" "$2" "$3"
"#;

/// The commits the visual step ran for, sorted.
fn visual_ran(fixture: &Fixture) -> Vec<String> {
    let mut commits: Vec<String> = started(fixture)
        .into_iter()
        .filter_map(|event| event.strip_prefix("visual ").map(str::to_owned))
        .collect();
    commits.sort();
    commits
}

fn project(fixture: &Fixture, plan: &str) {
    fixture.write("dandegate.toml", plan);
    fixture.write("dandegate/visual.sh", VISUAL_SH);
    fixture.write("dandegate/helper.sh", "helper=1\n");
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m\n",
    );
}

#[test]
fn a_kept_result_is_reused_into_a_new_output_folder() {
    let fixture = Fixture::new("reused");
    project(&fixture, VISUAL);
    let before = fixture.commit("before");
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.75 m\n",
    );
    let after = fixture.commit("after");

    let first = fixture.run("first", &["--before", &before]);
    first.page();
    let mut both = vec![before.clone(), after.clone()];
    both.sort();
    assert_eq!(visual_ran(&fixture), both);
    assert_eq!(first.printed("DANDEGATE-REUSED"), Vec::<String>::new());

    fixture.clear_events();
    let second = fixture.run("second", &["--before", &before]);
    let page = second.page();
    assert_eq!(visual_ran(&fixture), Vec::<String>::new());
    assert_eq!(
        second.printed("DANDEGATE-REUSED"),
        [
            format!("before visual {before} ({before})"),
            format!("after visual HEAD ({after})")
        ]
    );
    assert!(page.contains("<td>0.5 m</td><td>0.75 m</td>"), "{page}");
    assert_eq!(
        second.file("steps/after/visual/out/clips.log"),
        "MEASURE: clips.walk.twitch 0.75 m\n"
    );
}

#[test]
fn another_build_of_dandegate_misses_the_kept_result() {
    let fixture = Fixture::new("version");
    project(&fixture, VISUAL);
    let only = fixture.commit("only");
    fixture.run("one", &["--before", &only]).page();

    // The same program with one byte more at its end, which the loader ignores, so its digest differs.
    let other = fixture.outside("dandegate-other");
    let mut bytes = fs::read(env!("CARGO_BIN_EXE_dandegate")).expect("the binary");
    bytes.push(0);
    fs::write(&other, bytes).expect("a copy");
    fs::set_permissions(&other, fs::Permissions::from_mode(0o755)).expect("an executable copy");
    fixture.clear_events();
    fixture
        .run_program(&other, "other", &["--before", &only], &[])
        .page();
    assert_eq!(
        visual_ran(&fixture).len(),
        2,
        "another dandegate measures afresh"
    );
    fixture.clear_events();
    fixture.run("same", &["--before", &only]).page();
    assert_eq!(
        visual_ran(&fixture),
        Vec::<String>::new(),
        "the first build's results are still kept"
    );
}

#[test]
fn a_changed_input_value_misses_the_kept_result() {
    let fixture = Fixture::new("input");
    let plan = VISUAL.replace("\"{out}\"]", "\"{out}\", \"{input.renders}\"]");
    project(
        &fixture,
        &format!("[steps.inputs]\nrenders = {{ default = \"0\", help = \"frames\" }}\n{plan}"),
    );
    let only = fixture.commit("only");

    fixture
        .run("one", &["--before", &only, "--input", "renders=1"])
        .page();
    assert_eq!(visual_ran(&fixture).len(), 2);
    fixture.clear_events();
    fixture
        .run("two", &["--before", &only, "--input", "renders=2"])
        .page();
    assert_eq!(visual_ran(&fixture).len(), 2, "a new value runs afresh");
    fixture.clear_events();
    let again = fixture.run("again", &["--before", &only, "--input", "renders=1"]);
    again.page();
    assert_eq!(
        visual_ran(&fixture),
        Vec::<String>::new(),
        "the first value's result is still kept"
    );
    assert_eq!(again.printed("DANDEGATE-REUSED").len(), 2);
}

#[test]
fn changed_adapter_or_helper_content_misses_and_other_changes_do_not() {
    let fixture = Fixture::new("adapters");
    project(&fixture, VISUAL);
    let before = fixture.commit("before");
    let after = fixture.commit("after");
    fixture
        .run("first", &["--before", &before, "--after", &after])
        .page();

    // Only the after revision changes, yet the before side's kept result follows from the adapters it ran with.
    fixture.write("dandegate/helper.sh", "helper=2\n");
    let helper = fixture.commit("helper");
    fixture.clear_events();
    fixture
        .run("helper", &["--before", &before, "--after", &helper])
        .page();
    assert!(
        visual_ran(&fixture).contains(&before),
        "a changed helper misses"
    );

    fixture.write("README", "unrelated\n");
    let unrelated = fixture.commit("unrelated");
    fixture.clear_events();
    fixture
        .run("unrelated", &["--before", &before, "--after", &unrelated])
        .page();
    assert!(
        !visual_ran(&fixture).contains(&before),
        "a change outside the adapters keeps the before side"
    );

    fixture.write("dandegate/visual.sh", &format!("{VISUAL_SH}# changed\n"));
    let adapter = fixture.commit("adapter");
    fixture.clear_events();
    fixture
        .run("adapter", &["--before", &before, "--after", &adapter])
        .page();
    assert!(
        visual_ran(&fixture).contains(&before),
        "a changed adapter misses"
    );
}

#[test]
fn changed_external_file_content_at_the_same_path_misses() {
    let fixture = Fixture::new("external");
    let plan = VISUAL.replace("\"{out}\"]", "\"{out}\", \"{input.baseline}\"]");
    project(
        &fixture,
        &format!(
            "[steps.inputs]\nbaseline = {{ default = \"\", help = \"a baseline file\" }}\n{plan}"
        ),
    );
    let only = fixture.commit("only");
    let baseline = fixture.outside("baseline.json");
    let input = format!("baseline={}", baseline.to_str().expect("UTF-8"));

    fs::write(&baseline, "1").expect("a file");
    fixture
        .run("one", &["--before", &only, "--input", &input])
        .page();
    fixture.clear_events();
    fixture
        .run("same", &["--before", &only, "--input", &input])
        .page();
    assert_eq!(
        visual_ran(&fixture),
        Vec::<String>::new(),
        "the same content reuses"
    );
    fs::write(&baseline, "2").expect("a file");
    fixture
        .run("changed", &["--before", &only, "--input", &input])
        .page();
    assert_eq!(
        visual_ran(&fixture).len(),
        2,
        "new content at the same path misses"
    );
}

#[test]
fn a_changed_identity_environment_or_collected_line_misses() {
    let fixture = Fixture::new("environment");
    let plan = format!(
        "{VISUAL}env = {{ VISUAL_LEVEL = \"fixed\" }}\n[steps.step.identity]\nenv = [\"VISUAL_*\"]\ncollect = [\"sh\", \"-c\", \"cat \\\"$DANDEGATE_REPOSITORY/../toolchain\\\"\"]\n"
    );
    project(&fixture, &plan);
    let only = fixture.commit("only");
    let toolchain = fixture.outside("toolchain");
    fs::write(&toolchain, "rustc=1\n").expect("a file");

    fixture
        .run_with(
            "one",
            &["--before", &only],
            &[
                ("VISUAL_QUALITY", "1"),
                ("VISUAL_LEVEL", "1"),
                ("OTHER", "a"),
            ],
        )
        .page();
    fixture.clear_events();
    fixture
        .run_with(
            "other",
            &["--before", &only],
            &[
                ("VISUAL_QUALITY", "1"),
                ("VISUAL_LEVEL", "2"),
                ("OTHER", "b"),
            ],
        )
        .page();
    assert_eq!(
        visual_ran(&fixture),
        Vec::<String>::new(),
        "a variable no pattern names, or one the step sets itself, changes nothing"
    );
    fixture
        .run_with(
            "quality",
            &["--before", &only],
            &[("VISUAL_QUALITY", "2"), ("OTHER", "b")],
        )
        .page();
    assert_eq!(
        visual_ran(&fixture).len(),
        2,
        "a variable the identity names misses"
    );
    fixture.clear_events();
    fs::write(&toolchain, "rustc=2\n").expect("a file");
    fixture
        .run_with(
            "collected",
            &["--before", &only],
            &[("VISUAL_QUALITY", "2"), ("OTHER", "b")],
        )
        .page();
    assert_eq!(
        visual_ran(&fixture).len(),
        2,
        "a changed collected line misses"
    );
}

#[test]
fn a_failed_collector_runs_the_step_fresh_and_keeps_nothing() {
    let fixture = Fixture::new("collector");
    for (index, (collector, why)) in [
        (r#"["sh", "-c", "exit 4"]"#, "runs fresh and is not kept: its identity collector failed: exit status: 4"),
        (
            r#"["sh", "-c", "echo no-equals"]"#,
            "runs fresh and is not kept: its identity collector printed \"no-equals\", not name=value",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        project(&fixture, &format!("{VISUAL}\n[steps.step.identity]\ncollect = {collector}\n"));
        let only = fixture.commit("collector");
        for attempt in ["first", "second"] {
            fixture.clear_events();
            let run = fixture.run(&format!("{attempt}-{index}"), &["--before", &only]);
            let page = run.page();
            assert_eq!(visual_ran(&fixture).len(), 2, "{collector}: {attempt}");
            assert!(page.contains(&format!("<td>after</td><td>visual</td><td>{}</td>", why.replace('"', "&quot;"))), "{page}");
        }
        assert!(!fixture.results().exists() || fs::read_dir(fixture.results()).expect("a folder").next().is_none());
    }
}

#[test]
fn no_reuse_runs_fresh_and_replaces_the_kept_result() {
    let fixture = Fixture::new("no-reuse");
    // The run count lives outside the repository, so it is not part of the identity.
    let counted = r#". "$(dirname "$0")/lib.sh"
event "start visual $2"
echo "MEASURE: runs $(cat "$EVENTS/count") runs" > "$3/clips.log"
"#;
    project(&fixture, VISUAL);
    fixture.write("dandegate/visual.sh", counted);
    let only = fixture.commit("only");
    let count = fixture.events_folder().join("count");

    fs::write(&count, "1").expect("a file");
    fixture.run("one", &["--before", &only]).page();
    fs::write(&count, "2").expect("a file");
    fixture.clear_events();
    let fresh = fixture.run("fresh", &["--before", &only, "--no-reuse"]);
    fresh.page();
    assert_eq!(visual_ran(&fixture).len(), 2);
    assert_eq!(fresh.printed("DANDEGATE-REUSED"), Vec::<String>::new());
    fixture.clear_events();
    let reused = fixture.run("reused", &["--before", &only]);
    reused.page();
    assert_eq!(visual_ran(&fixture), Vec::<String>::new());
    assert_eq!(
        reused.file("steps/after/visual/out/clips.log"),
        "MEASURE: runs 2 runs\n"
    );
}

#[test]
fn a_changed_prepare_recipe_variable_or_collector_misses_a_kept_visual() {
    let fixture = Fixture::new("prepare");
    // An unrelated step first, so the prepare step is not the first the plan declares.
    let prepare = r#"
[[steps.step]]
name = "unrelated"
run = ["sh", "{adapters}/step.sh", "unrelated", "{tree}", "{commit}", "{out}"]

[[steps.step]]
name = "prepare"
run = ["sh", "{adapters}/step.sh", "prepare", "{tree}", "{commit}", "{out}"]

[steps.step.identity]
env = ["BUILD_MODE"]
"#;
    let plan = format!("{prepare}{VISUAL}needs = [\"prepare\"]\n");
    project(&fixture, &plan);
    let before = fixture.commit("before");
    let after = fixture.commit("after");
    fixture
        .run("first", &["--before", &before, "--after", &after])
        .page();

    fixture.write("README", "unrelated\n");
    let unrelated = fixture.commit("unrelated");
    fixture.clear_events();
    fixture
        .run("unrelated", &["--before", &before, "--after", &unrelated])
        .page();
    assert!(
        !visual_ran(&fixture).contains(&before),
        "an unchanged prepare keeps the before side"
    );

    fixture.write(
        "dandegate.toml",
        &plan.replace(
            "\"prepare\", \"{tree}\", \"{commit}\", \"{out}\"]",
            "\"prepare\", \"{tree}\", \"{commit}\", \"{out}\", \"--fast\"]",
        ),
    );
    let changed = fixture.commit("changed prepare");
    fixture.clear_events();
    fixture
        .run("changed", &["--before", &before, "--after", &changed])
        .page();
    assert!(
        visual_ran(&fixture).contains(&before),
        "a changed prepare recipe misses the before side"
    );

    // A variable that only prepare's identity names belongs to the visual result that skips prepare.
    let arguments = ["--before", before.as_str(), "--after", changed.as_str()];
    fixture.clear_events();
    fixture
        .run_with("mode", &arguments, &[("BUILD_MODE", "2")])
        .page();
    assert!(
        visual_ran(&fixture).contains(&before),
        "a changed variable that prepare's identity names misses the before side"
    );
    fixture.clear_events();
    fixture
        .run_with("same mode", &arguments, &[("BUILD_MODE", "2")])
        .page();
    assert!(
        !visual_ran(&fixture).contains(&before),
        "the same variable reuses it"
    );

    // Without prepare's identity, a result that follows from it is neither reused nor kept.
    fixture.write(
        "dandegate.toml",
        &plan.replace(
            "env = [\"BUILD_MODE\"]",
            "env = [\"BUILD_MODE\"]\ncollect = [\"sh\", \"-c\", \"exit 3\"]",
        ),
    );
    let unidentified = fixture.commit("unidentified prepare");
    for out in ["unidentified", "unidentified again"] {
        fixture.clear_events();
        let page = fixture
            .run(out, &["--before", &before, "--after", &unidentified])
            .page();
        assert!(visual_ran(&fixture).contains(&before), "{out}");
        assert!(
            page.contains(
                "<td>before</td><td>visual</td><td>runs fresh and is not kept: before prepare, which it needs, \
                 cannot be identified: its identity collector failed: exit status: 3</td>"
            ),
            "{out}: {page}"
        );
    }
}

#[test]
fn a_before_revision_older_than_the_adapters_is_measured_then_reused() {
    let fixture = Fixture::new("older");
    fixture.remove("dandegate/lib.sh");
    fixture.remove("dandegate/step.sh");
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m\n",
    );
    let old = fixture.commit("before the adapters");
    fixture.write("dandegate/lib.sh", LIB);
    fixture.write("dandegate/step.sh", STEP);
    project(&fixture, VISUAL);
    let after = fixture.commit("the adapters");

    let first = fixture.run("first", &["--before", &old]);
    let page = first.page();
    assert!(visual_ran(&fixture).contains(&old));
    assert!(
        page.contains("<td><code>clips.walk.twitch</code></td><td>0.5 m</td><td>0.5 m</td>"),
        "{page}"
    );
    fixture.clear_events();
    let second = fixture.run("second", &["--before", &old]);
    second.page();
    assert_eq!(visual_ran(&fixture), Vec::<String>::new());
    assert_eq!(
        second.printed("DANDEGATE-REUSED"),
        [
            format!("before visual {old} ({old})"),
            format!("after visual HEAD ({after})")
        ]
    );
}
