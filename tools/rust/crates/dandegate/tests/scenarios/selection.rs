//! Optional steps, needs and live steps, and which steps run, which are skipped, and why.

use crate::support::Fixture;

/// A step `name` of kind `runs` running the generic step, with extra lines.
pub(crate) fn step(name: &str, runs: &str, extra: &str) -> String {
    let (tree, commit) = if runs == "once" {
        ("{after}", "{after-commit}")
    } else {
        ("{tree}", "{commit}")
    };
    format!(
        "[[step]]\nname = \"{name}\"\nruns = \"{runs}\"\nrun = [\"sh\", \"{{adapters}}/step.sh\", \"{name}\", \"{tree}\", \
         \"{commit}\", \"{{out}}\"]\n{extra}\n"
    )
}

/// The steps whose start the events record, in order, with the commit each was given.
pub(crate) fn started(fixture: &Fixture) -> Vec<String> {
    fixture
        .events()
        .iter()
        .filter_map(|event| event.strip_prefix("start "))
        .map(|event| event.split(' ').take(2).collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn optional_steps_run_only_when_selected_or_needed() {
    let fixture = Fixture::new("optional");
    let plan = [
        step("base", "per-revision", ""),
        step("prep", "after", "optional = true"),
        step(
            "extra",
            "after",
            "optional = true\nneeds = [\"prep\", \"base\"]",
        ),
        step("lone", "once", "optional = true"),
    ]
    .concat();
    fixture.write("dandegate.toml", &plan);
    let before = fixture.commit("before");
    let after = fixture.commit("after");

    let run = fixture.run("plain", &["--before", &before]);
    let page = run.page();
    let mut ran = started(&fixture);
    ran.sort();
    let mut expected = [format!("base {after}"), format!("base {before}")];
    expected.sort();
    assert_eq!(ran, expected);
    for name in ["prep", "extra", "lone"] {
        assert!(
            page.contains(&format!("<td>{name}</td><td>not selected</td>")),
            "{name}: {page}"
        );
    }

    fixture.clear_events();
    let run = fixture.run("selected", &["--before", &before, "--with", "extra"]);
    run.page();
    let ran = started(&fixture);
    // extra waits for both its needs on its own side: prep and base on the after side.
    let position = |name: &str| {
        ran.iter()
            .position(|event| *event == format!("{name} {after}"))
    };
    let extra = position("extra").expect("extra ran");
    assert!(position("prep").expect("prep ran, as extra needs it") < extra);
    assert!(position("base").expect("base ran") < extra);
    assert!(
        !ran.iter().any(|event| event.starts_with("lone")),
        "{ran:?}"
    );
}

#[test]
fn a_failed_need_skips_its_dependant_while_independent_work_continues() {
    let fixture = Fixture::new("failed-need");
    let plan = [
        step("prep", "per-revision", ""),
        step("visual", "per-revision", "needs = [\"prep\"]"),
        step("other", "once", ""),
    ]
    .concat();
    fixture.write("dandegate.toml", &plan);
    let before = fixture.commit("before");
    fixture.write("fixture/prep.fail", "");
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    let ran = started(&fixture);
    assert!(ran.contains(&format!("visual {before}")), "{ran:?}");
    assert!(!ran.contains(&format!("visual {after}")), "{ran:?}");
    assert!(ran.contains(&format!("other {after}")), "{ran:?}");
    assert_eq!(
        run.printed("DANDEGATE-STEP-FAILED"),
        [
            format!("after prep HEAD ({after}): failed: exited 1"),
            format!("after visual HEAD ({after}): skipped: prep did not succeed"),
        ]
    );
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        [
            "after prep: failed: exited 1",
            "after visual: skipped: prep did not succeed"
        ]
    );
    assert!(
        page.contains("<td>after</td><td>visual</td><td>skipped: prep did not succeed</td>"),
        "{page}"
    ); // A failed step's row is marked; a step that did its work is not.
    assert!(
        page.contains("<tr class=\"failed\"><td>after</td><td>prep</td>"),
        "{page}"
    );
    assert!(page.contains("<tr><td>before</td><td>prep</td>"), "{page}");
}

#[test]
fn a_reused_step_skips_its_needs_and_a_live_step_refreshes_only_the_sides_it_uses() {
    let fixture = Fixture::new("live");
    let plan = [
        step("prepare", "per-revision", "reuse = true"),
        step(
            "visual",
            "per-revision",
            "reuse = true\nneeds = [\"prepare\"]",
        ),
        step(
            "rebuild",
            "after",
            "optional = true\nlive = true\nneeds = [\"prepare\"]",
        ),
        step(
            "crossings",
            "once",
            "optional = true\nlive = true\nneeds = [\"prepare\"]",
        ),
    ]
    .concat();
    fixture.write("dandegate.toml", &plan);
    let before = fixture.commit("before");
    let after = fixture.commit("after");
    let prepared = |fixture: &Fixture| {
        let mut sides: Vec<String> = started(fixture)
            .into_iter()
            .filter_map(|event| event.strip_prefix("prepare ").map(str::to_owned))
            .collect();
        sides.sort();
        sides
    };
    let unneeded = |side: &str| {
        format!("<td>{side}</td><td>prepare</td><td>not run: nothing running needs it</td>")
    };

    fixture.run("first", &["--before", &before]).page();
    let mut both = vec![before.clone(), after.clone()];
    both.sort();
    assert_eq!(prepared(&fixture), both);

    fixture.clear_events();
    let run = fixture.run("reused", &["--before", &before]);
    let page = run.page();
    assert_eq!(fixture.events(), Vec::<String>::new());
    assert_eq!(
        run.printed("DANDEGATE-REUSED"),
        [
            format!("before visual {before} ({before})"),
            format!("after visual HEAD ({after})")
        ]
    );
    assert!(page.contains(&unneeded("before")), "{page}");
    assert!(page.contains(&unneeded("after")), "{page}");

    fixture.clear_events();
    let run = fixture.run("rebuild", &["--before", &before, "--with", "rebuild"]);
    let page = run.page();
    // The after side's prepare runs fresh although it is kept, since rebuild needs its real checkout. The before
    // side's stays unneeded.
    assert_eq!(prepared(&fixture), std::slice::from_ref(&after));
    assert!(
        page.contains("runs fresh: a live step needs its checkout from this run"),
        "{page}"
    );
    assert!(started(&fixture).contains(&format!("rebuild {after}")));
    assert!(page.contains(&unneeded("before")), "{page}");

    fixture.clear_events();
    fixture
        .run("crossings", &["--before", &before, "--with", "crossings"])
        .page();
    assert_eq!(prepared(&fixture), both);
    assert!(started(&fixture).contains(&format!("crossings {after}")));
}

#[test]
fn needs_a_step_cannot_wait_for_are_refused_with_their_line() {
    let fixture = Fixture::new("needs");
    let cases = [
        (
            [
                step("late", "after", ""),
                step("early", "per-revision", "needs = [\"late\"]"),
            ]
            .concat(),
            "dandegate.toml:10: step \"early\": a per-revision step cannot need \"late\", an after step",
        ),
        (
            [
                step("pair", "once", ""),
                step("early", "per-revision", "needs = [\"pair\"]"),
            ]
            .concat(),
            "dandegate.toml:10: step \"early\": a per-revision step cannot need \"pair\", a once step",
        ),
        (
            [
                step("pair", "once", ""),
                step("late", "after", "needs = [\"pair\"]"),
            ]
            .concat(),
            "dandegate.toml:10: step \"late\": an after step cannot need \"pair\", a once step",
        ),
        (
            step("lonely", "per-revision", "needs = [\"nobody\"]"),
            "dandegate.toml:5: step \"lonely\": needs \"nobody\", which no step is called",
        ),
        (
            [
                step("a", "per-revision", "needs = [\"b\"]"),
                step("b", "per-revision", "needs = [\"a\"]"),
            ]
            .concat(),
            "dandegate.toml:2: the steps need each other in a cycle: a -> b -> a",
        ),
        (
            [
                step("timed", "once", "exclusive = true"),
                step("plain", "once", "needs = [\"timed\"]"),
            ]
            .concat(),
            "dandegate.toml:10: step \"plain\": needs \"timed\", which is exclusive and so runs after every step that \
             is not",
        ),
    ];
    let base = fixture.commit("base");
    for (index, (plan, expected)) in cases.iter().enumerate() {
        fixture.write("dandegate.toml", plan);
        let after = fixture.commit("plan");
        let run = fixture.run(
            &format!("out-{index}"),
            &["--before", &base, "--after", &after],
        );
        assert_eq!(run.status(), 1, "{expected}");
        assert_eq!(run.stderr(), format!("dandegate: {expected}\n"));
        assert_eq!(fixture.events(), Vec::<String>::new());
    }
}

#[test]
fn a_failed_render_keeps_its_measures_and_the_rebuild_still_runs() {
    let fixture = Fixture::new("failed-render");
    // The native build succeeds. The visual step's first import fails after its clips report wrote its measures.
    let plan = [
        step("prepare", "per-revision", ""),
        step(
            "visual",
            "per-revision",
            "needs = [\"prepare\"]\noutput = [{ kind = \"measures\", from = \"{out}/clips.log\" }]",
        ),
        step(
            "rebuild",
            "after",
            "optional = true\nlive = true\nneeds = [\"prepare\"]",
        ),
    ]
    .concat();
    fixture.write("dandegate.toml", &plan);
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m\n",
    );
    let before = fixture.commit("before");
    fixture.write("fixture/visual.fail", "");
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", &before, "--with", "rebuild"]);
    let page = run.page();
    assert!(started(&fixture).contains(&format!("rebuild {after}")));
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        ["after visual: failed: exited 1"]
    );
    assert!(
        page.contains("<td><code>clips.walk.twitch</code></td><td>0.5 m</td><td>0.5 m</td>"),
        "{page}"
    );
}
