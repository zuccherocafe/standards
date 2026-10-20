//! design-read through its command, on the area files under `tests/fixtures/`. Its uses, most common first:
//!
//! 1. A writer checks a project's area files with no argument: every `*.md` in `docs/design-reads/` but the README.
//! 2. A writer checks the files they name, one or several.
//! 3. An area file with a fault: one line per refusal on standard error, naming the file, line and field, and exit 1.
//!    Each fixture under `refused/` breaks one rule and is refused for that one alone.
//! 4. A missing folder, a file that cannot be read, or a malformed command line: exit 2.

use std::path::Path;
use std::process::{Command, Output};

/// Runs design-read with `arguments` in the fixtures folder `folder`.
fn design_read(folder: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_design-read"))
        .args(arguments)
        .current_dir(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(folder),
        )
        .output()
        .expect("design-read runs")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("UTF-8")
}

#[test]
fn with_no_file_every_area_file_in_the_folder_is_checked_and_the_readme_is_not() {
    let output = design_read("project", &["check"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(
        text(&output.stdout),
        "docs/design-reads/motion.md: passes\ndocs/design-reads/quiet.md: passes\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn named_files_are_each_checked_and_any_refusal_fails_the_run() {
    let passing = design_read(
        "project/docs/design-reads",
        &["check", "quiet.md", "motion.md"],
    );
    assert_eq!(text(&passing.stderr), "");
    assert_eq!(
        text(&passing.stdout),
        "quiet.md: passes\nmotion.md: passes\n"
    );
    assert_eq!(passing.status.code(), Some(0));

    let mixed = design_read(
        "",
        &[
            "check",
            "refused/empty-cell.md",
            "project/docs/design-reads/quiet.md",
        ],
    );
    assert_eq!(
        text(&mixed.stderr),
        "refused/empty-cell.md:11: Owner: empty\n"
    );
    assert_eq!(
        text(&mixed.stdout),
        "project/docs/design-reads/quiet.md: passes\n"
    );
    assert_eq!(mixed.status.code(), Some(1));
}

#[test]
fn each_broken_rule_is_refused_alone_at_its_line_and_field() {
    for (file, refusal) in [
        (
            "missing-field",
            "1: Inspected: missing; an area file's header has `- Inspected: ...`",
        ),
        ("repeated-field", "6: Area: given again; first on line 3"),
        ("empty-field", "3: Area: empty"),
        (
            "short-commit",
            "4: Last full read: `3ddf4c52` is not a full commit id, 40 lowercase hexadecimal digits",
        ),
        (
            "missing-disagreements",
            "1: Disagreements: missing; an area file has a `## Disagreements` section",
        ),
        (
            "repeated-disagreements",
            "13: Disagreements: given again; first on line 7",
        ),
        (
            "wrong-columns",
            "9: Disagreements: columns are `ID | Concept | Owner | Sites | Consequence | Done when`; \
             an area file's are `Concept | Owner | Sites | Consequence | Done when`",
        ),
        (
            "missing-delimiter",
            "10: Disagreements: the header row is not followed by a delimiter row, `|---|` for each of the 5 columns",
        ),
        ("cell-count", "11: Disagreements: has 4 cells; a row has 5"),
        ("empty-cell", "11: Owner: empty"),
        (
            "repeated-concept",
            "12: Concept: `A Concept` is also on line 11",
        ),
        (
            "no-rows",
            "7: Disagreements: has no rows; an area file with no open disagreements says `None found.` instead of a table",
        ),
        (
            "none-found-beside-rows",
            "13: Disagreements: says `None found.` beside a table",
        ),
        (
            "none-found-beside-empty-table",
            "12: Disagreements: says `None found.` beside a table",
        ),
        (
            "previous-field",
            "6: Previous: not a header field; an area file's header has exactly Area, Last full read and Inspected",
        ),
        (
            "second-table",
            "13: Disagreements: a second table; an area file has one disagreements table",
        ),
    ] {
        let file = format!("refused/{file}.md");
        let output = design_read("", &["check", &file]);
        assert_eq!(
            text(&output.stderr),
            format!("{file}:{refusal}\n"),
            "{file}"
        );
        assert_eq!(text(&output.stdout), "", "{file}");
        assert_eq!(output.status.code(), Some(1), "{file}");
    }
}

#[test]
fn a_missing_folder_an_unreadable_file_or_a_malformed_command_line_exits_2() {
    let no_folder = design_read("refused", &["check"]);
    assert_eq!(
        text(&no_folder.stderr),
        "docs/design-reads: cannot be listed: No such file or directory (os error 2)\n"
    );
    assert_eq!(text(&no_folder.stdout), "");
    assert_eq!(no_folder.status.code(), Some(2));

    let missing = design_read("", &["check", "nowhere.md", "refused/empty-cell.md"]);
    assert_eq!(
        text(&missing.stderr),
        "nowhere.md: cannot be read: No such file or directory (os error 2)\n\
         refused/empty-cell.md:11: Owner: empty\n"
    );
    assert_eq!(missing.status.code(), Some(2));

    for arguments in [&[][..], &["verify", "a.md"], &["check", "--all"]] {
        assert_eq!(
            design_read("", arguments).status.code(),
            Some(2),
            "{arguments:?}"
        );
    }
}
