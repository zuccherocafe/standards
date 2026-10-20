//! The command's most common use, a report between two committed revisions, and every refusal that stops it before
//! any step runs.

use std::fs;

use crate::support::{Fixture, files_under, png};

/// A per-revision step with images and measures, and a once step with a section, each running the generic step.
pub(crate) const PROJECT: &str = r#"
[[steps.step]]
name = "visual"
run = ["sh", "{adapters}/step.sh", "visual", "{tree}", "{commit}", "{out}"]
output = [
    { kind = "images", folder = "{out}/images/", expected = "{out}/expected.txt" },
    { kind = "measures", from = "{out}/clips.log" },
]

[[steps.step]]
name = "notes"
runs = "once"
run = ["sh", "{adapters}/step.sh", "notes", "{after}", "{after-commit}", "{out}"]
output = [{ kind = "section", file = "{out}/notes.md" }]
"#;

#[test]
fn a_report_compares_two_committed_revisions_with_after_defaulting_to_head() {
    let fixture = Fixture::new("report");
    fixture.write("dandegate.toml", PROJECT);
    fixture.write("fixture/visual/expected.txt", "cell.png\n");
    fixture.write_bytes(
        "fixture/visual/images/cell.png",
        &png(1, 1, &[10, 20, 30, 255]),
    );
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m higher-is-worse\n",
    );
    fixture.write("fixture/notes/notes.md", "# Notes\n\nNothing to see.\n");
    fixture.write("fixture/notes/notes.md.status", "complete\n");
    let before = fixture.commit("before");
    fixture.git(&["branch", "base"]);
    fixture.write_bytes(
        "fixture/visual/images/cell.png",
        &png(1, 1, &[13, 20, 27, 255]),
    );
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.75 m higher-is-worse\n",
    );
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", "base"]);
    let page = run.page();

    assert!(
        page.contains(&format!(
            "<title>Changes from base ({before}) to HEAD ({after})</title>"
        )),
        "{page}"
    );
    assert!(run.file("report.log").starts_with(&format!(
        "dandegate: before base ({before}), after HEAD ({after})\n"
    )));
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    // |10 - 13| + |20 - 20| + |30 - 27| + |255 - 255| = 6, over four samples.
    assert!(page.contains("<code>cell.png</code>: 1.50"), "{page}");
    assert_eq!(
        run.printed("DANDEGATE-WORSE"),
        [
            "clips.walk.twitch 0.5 m (higher-is-worse) -> 0.75 m (higher-is-worse) (+0.25, 50.0%, moved, worse (higher-is-worse))"
        ]
    );
    assert!(
        page.contains("<pre># Notes\n\nNothing to see.\n</pre>"),
        "{page}"
    );
    // Each side's step was given its own commit, and ran in a checkout of it. The once step was given the after one.
    let events = fixture.events();
    for expected in [
        format!("start visual {before} {before}"),
        format!("start visual {after} {after}"),
        format!("start notes {after} {after}"),
    ] {
        assert!(events.contains(&expected), "{expected} in {events:?}");
    }
    // The checkouts are gone, and the repository has only its own worktree.
    assert_eq!(fixture.git(&["worktree", "list"]).lines().count(), 1);
    // What dandegate keeps is ignored, so the repository's status is clean.
    assert_eq!(
        fs::read_to_string(fixture.repo().join(".cache/dandegate/.gitignore"))
            .expect("an ignore file"),
        "*\n"
    );
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert_eq!(
        files_under(&fixture.repo().join(".cache/dandegate/scratch")),
        Vec::<String>::new()
    );
}

#[test]
fn a_step_runs_the_tools_of_its_revisions_shell_and_never_the_callers() {
    let fixture = Fixture::new("strict-path");
    let writes = "#!/bin/sh\necho written > \"$1\"\n";
    fixture.tool("pinned", writes);
    fixture.caller_tool("unpinned", writes);
    fixture.write(
        "dandegate.toml",
        r#"
[[steps.step]]
name = "pinned"
runs = "once"
run = ["pinned", "{out}/written.txt"]

[[steps.step]]
name = "unpinned"
runs = "once"
run = ["unpinned", "{out}/written.txt"]
"#,
    );
    let only = fixture.commit("only");

    let run = fixture.run("out", &["--before", &only]);
    run.page();
    assert_eq!(run.file("steps/both/pinned/out/written.txt"), "written\n");
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        ["both unpinned: failed: exited 127"]
    );
    assert!(!run.exists("steps/both/unpinned/out/written.txt"));
    let log = run.file("steps/both/unpinned/log.txt");
    assert!(log.contains("unpinned: not found"), "{log}");
}

#[test]
fn every_refusal_comes_before_any_step_runs() {
    let fixture = Fixture::new("refusals");
    fixture.write(
        "dandegate.toml",
        &format!(
            "[steps.inputs]\nrenders = {{ default = \"0\", help = \"frames to render\" }}\n{PROJECT}"
        ),
    );
    let valid = fixture.commit("valid");
    fixture.write("dandegate.toml", "[[steps.step]\nname = \"visual\"\n");
    let syntax = fixture.commit("syntax");
    fixture.write(
        "dandegate.toml",
        "\n[[steps.step]]\nname = \"visual\"\nrun = [\"sh\", \"{before}\"]\n",
    );
    let placeholder = fixture.commit("placeholder");
    fs::create_dir(fixture.outside("existing")).expect("a folder");

    let cases: [(&[&str], &str, &str); 7] = [
        (
            &["--before", &valid, "--after", &syntax],
            "syntax",
            "dandegate: dandegate.toml:1: ",
        ),
        (
            &["--before", &valid, "--after", &placeholder],
            "placeholder",
            "dandegate: dandegate.toml:4: step \"visual\": run: {before} is not a placeholder this kind of step has; \
             it has {tree}, {commit}, {out}, {adapters}, {cache}, {cpus} and {input.<name>}",
        ),
        (
            &["--before", &valid, "--after", &valid, "--with", "nope"],
            "selection",
            "dandegate: --with: \"nope\" names no step; the steps are visual, notes",
        ),
        (
            &["--before", &valid, "--after", &valid, "--input", "nope=1"],
            "input",
            "dandegate: --input: \"nope\" is not an input dandegate.toml declares; it declares renders (frames to render)",
        ),
        (
            &["--before", &valid, "--after", &valid, "--input", "renders"],
            "input-syntax",
            "dandegate: --input: \"renders\" is not <name>=<value>",
        ),
        (
            &["--before", "nope"],
            "revision",
            "dandegate: \"nope\" names no commit",
        ),
        (
            &["--before", &valid],
            "existing",
            "exists; give a new folder",
        ),
    ];
    for (arguments, out, expected) in cases {
        let run = fixture.run(out, arguments);
        assert_eq!(run.status(), 1, "{out}: {}", run.stdout());
        assert!(run.stderr().contains(expected), "{out}: {}", run.stderr());
        assert_eq!(run.stdout(), "", "{out}");
        assert_eq!(fixture.events(), Vec::<String>::new(), "{out}");
        if out == "existing" {
            assert_eq!(files_under(&run.out), Vec::<String>::new());
        } else {
            assert!(!run.out.exists(), "{out}");
        }
    }
}

#[test]
fn an_output_folder_inside_the_repository_is_refused() {
    let fixture = Fixture::new("inside");
    fixture.write("dandegate.toml", PROJECT);
    let only = fixture.commit("only");
    let report = fixture.repo().join("report");
    let run = fixture
        .command(&["--before", &only, "--out", report.to_str().expect("UTF-8")])
        .output()
        .expect("the binary starts");
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("is inside the repository"));
    assert!(!report.exists());

    // A path that reaches the repository through a link outside it is inside it too.
    let link = fixture.outside("link");
    std::os::unix::fs::symlink(fixture.repo(), &link).expect("a link");
    let through = link.join("report");
    let run = fixture
        .command(&["--before", &only, "--out", through.to_str().expect("UTF-8")])
        .output()
        .expect("the binary starts");
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("is inside the repository"));
    assert!(!fixture.repo().join("report").exists());
}

#[test]
fn revisions_are_frozen_when_the_report_starts_while_a_branch_moves() {
    let fixture = Fixture::new("frozen");
    // The mover moves main to a new commit while the report runs. The late step starts only after it.
    let plan = r#"
[[steps.step]]
name = "mover"
runs = "once"
run = ["sh", "-c", "git -C \"$DANDEGATE_REPOSITORY\" commit --quiet --allow-empty --message moved"]

[[steps.step]]
name = "late"
runs = "once"
needs = ["mover"]
run = ["sh", "{adapters}/step.sh", "late", "{after}", "{after-commit}", "{out}"]
"#;
    fixture.write("dandegate.toml", plan);
    let before = fixture.commit("before");
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", &before, "--after", "main"]);
    let page = run.page();
    let moved = fixture.git(&["rev-parse", "main"]).trim().to_owned();
    assert_ne!(moved, after, "the branch moved during the report");
    assert!(
        fixture
            .events()
            .contains(&format!("start late {after} {after}")),
        "{:?}",
        fixture.events()
    );
    assert!(
        page.contains(&format!("to main ({after})</title>")),
        "{page}"
    );
}

#[test]
fn every_step_gets_the_repository_its_inputs_verbatim_and_the_measures_file_only_when_given() {
    let fixture = Fixture::new("step-environment");
    let plan = r#"
[steps.inputs]
source-checkout = { default = ".", help = "where untracked sources are read" }

[[steps.step]]
name = "environment"
runs = "once"
reuse = true
run = ["sh", "{adapters}/environment.sh", "{input.source-checkout}"]
env = { STEP_MODE = "{input.source-checkout}-mode" }

[steps.step.identity]
env = ["DANDEGATE_*"]
"#;
    fixture.write("dandegate.toml", plan);
    fixture.write(
        "dandegate/environment.sh",
        "printf '%s|%s|%s|%s\\n' \"$DANDEGATE_REPOSITORY\" \"${DANDEGATE_MEASURES-absent}\" \"$1\" \"$STEP_MODE\" \
         >> \"$EVENTS/environment\"\n",
    );
    let only = fixture.commit("only");
    fs::write(fixture.outside("gate.log"), "MEASURE: a 1 m\n").expect("a log");

    // What dandegate inherits under its own variables' names reaches neither a step nor its identity.
    let stale = fixture.run_with(
        "plain",
        &["--before", &only],
        &[
            ("DANDEGATE_MEASURES", "/old/report/gate.log"),
            ("DANDEGATE_REPOSITORY", "/elsewhere"),
        ],
    );
    stale.page();
    let log = stale.file("report.log");
    assert!(log.contains("both environment: kept for reuse"), "{log}");
    fixture
        .run(
            "given",
            &[
                "--before",
                &only,
                "--measures",
                "../gate.log",
                "--input",
                "source-checkout=../elsewhere",
            ],
        )
        .page();
    let repository = fixture.repo().canonicalize().expect("the repository");
    let gate_log = fixture.outside("gate.log").canonicalize().expect("the log");
    let recorded = fs::read_to_string(fixture.events_folder().join("environment"))
        .expect("the steps' records");
    assert_eq!(
        recorded,
        format!(
            "{0}|absent|.|.-mode\n{0}|{1}|../elsewhere|../elsewhere-mode\n",
            repository.display(),
            gate_log.display()
        )
    );
}

/// Whether `text` is a duration as the page and log show one: seconds, to a tenth.
fn is_seconds(text: &str) -> bool {
    text.strip_suffix(" s")
        .and_then(|number| number.split_once('.'))
        .is_some_and(|(whole, tenth)| {
            !whole.is_empty()
                && whole.chars().all(|digit| digit.is_ascii_digit())
                && tenth.len() == 1
                && tenth.chars().all(|digit| digit.is_ascii_digit())
        })
}

#[test]
fn each_step_that_runs_has_its_duration_in_the_log_and_on_the_page() {
    let fixture = Fixture::new("durations");
    fixture.write(
        "dandegate.toml",
        &format!(
            "{PROJECT}\n[[steps.step]]\nname = \"spare\"\nruns = \"once\"\noptional = true\nrun = [\"true\"]\n"
        ),
    );
    fixture.write("fixture/notes/notes.md", "# Notes\n");
    fixture.write("fixture/notes/notes.md.status", "complete\n");
    let only = fixture.commit("only");
    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    let log = run.file("report.log");
    for instance in ["before visual", "after visual", "both notes"] {
        let lead = format!("=== {instance}: ");
        let line = log
            .lines()
            .find_map(|line| {
                line.strip_prefix(&lead)
                    .filter(|rest| rest.contains(" in "))
            })
            .unwrap_or_else(|| panic!("{instance} has no outcome line: {log}"));
        let (_, took) = line.rsplit_once(" in ").expect("a duration");
        assert!(is_seconds(took), "{line}");
        let (place, step) = instance.split_once(' ').expect("a place and a step");
        let row = format!("<td>{place}</td><td>{step}</td><td>");
        let cells = &page[page.find(&row).expect("the step's row") + row.len()..];
        let cells: Vec<&str> = cells
            .split("</tr>")
            .next()
            .expect("the row")
            .split("</td><td>")
            .collect();
        assert!(is_seconds(cells[1]), "{instance}: {cells:?}");
    }
    assert!(
        page.contains("<td>both</td><td>spare</td><td>not selected</td><td></td>"),
        "a step that did not run has no duration: {page}"
    );
}
