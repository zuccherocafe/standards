//! design-read through its command, on the reads under `tests/fixtures/`. Its uses, most common first:
//!
//! 1. A coordinator checks a read before a writer starts: a first read passes, and so does a follow-up that carries
//!    forward every disagreement its previous read left unresolved.
//! 2. A read with a fault: one line per refusal on standard error, naming the file, line and field, and exit 1. Each
//!    fixture under `refused/` breaks one rule and is refused for that one alone.
//! 3. A read that cannot be read, or a malformed command line: exit 2.

use std::path::Path;
use std::process::{Command, Output};

fn design_read(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_design-read"))
        .args(arguments)
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
        .output()
        .expect("design-read runs")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("UTF-8")
}

#[test]
fn a_first_read_and_its_follow_up_pass_with_their_rows_counted() {
    for (read, summary) in [
        (
            "mm4/design-read.md",
            "disagreements: 7 new, 0 open, 0 resolved, 0 parked",
        ),
        (
            "mm5/design-read.md",
            "disagreements: 1 new, 1 open, 4 resolved, 2 parked",
        ),
        (
            "previous/design-read.md",
            "disagreements: 2 new, 0 open, 0 resolved, 0 parked",
        ),
        ("none-found/design-read.md", "none found"),
    ] {
        let output = design_read(&["check", read]);
        assert_eq!(text(&output.stderr), "", "{read}");
        assert_eq!(text(&output.stdout), format!("{read}: passes: {summary}\n"));
        assert_eq!(output.status.code(), Some(0), "{read}");
    }
}

#[test]
fn each_broken_rule_is_refused_alone_at_its_line_and_field() {
    for (read, refusal) in [
        (
            "missing-field",
            "1: Inspected: missing; a read's header has `- Inspected: ...`",
        ),
        ("repeated-field", "7: Area: given again; first on line 3"),
        ("empty-field", "3: Area: empty"),
        (
            "short-revision",
            "4: Revision: `80adaf9` is not a full commit id, 40 lowercase hexadecimal digits",
        ),
        (
            "previous-missing",
            "5: Previous: ../nowhere/design-read.md cannot be read: No such file or directory (os error 2)",
        ),
        (
            "previous-without-table",
            "5: Previous: ../previous/notes.md has no table to compare with: it has no `## Disagreements` section",
        ),
        (
            "wrong-columns",
            "10: Disagreements: columns are `ID | Concept | Owner | Sites | Consequence | Fix | Status`; \
             a read's are `ID | Concept | Owner | Sites | Consequence | Done when | Status`",
        ),
        (
            "missing-delimiter",
            "11: Disagreements: the header row is not followed by a delimiter row, `|---|` for each of the 7 columns",
        ),
        ("cell-count", "12: Disagreements: has 6 cells; a row has 7"),
        ("empty-cell", "12: Owner: empty"),
        ("repeated-id", "13: ID: r-D1 is also on line 12"),
        (
            "unqualified-id",
            "12: ID: `D1` is not qualified by its read as `<read>-D<n>`, such as `mm4-D1`",
        ),
        (
            "unknown-status",
            "12: Status: `closed` is not new, open, resolved or parked",
        ),
        (
            "new-for-carried-id",
            "16: Status: `new`, but prev-D1 is in the previous read; carry it as open, resolved or parked",
        ),
        (
            "carried-status-for-new-id",
            "12: Status: `open`, but r-D1 is not in a previous read; a disagreement this read found is new",
        ),
        (
            "unresolved-id-omitted",
            "12: Disagreements: prev-D1 is new in the previous read and missing here; \
             carry it as open, resolved or parked",
        ),
        (
            "after-missing",
            "5: After the last phase: missing; a read with a previous read begins with what changed since it",
        ),
        ("after-empty", "8: After the last phase: empty"),
        (
            "after-in-first-read",
            "8: After the last phase: present in a first read; only a read with a previous read has one",
        ),
        (
            "no-rows",
            "8: Disagreements: has no rows; a read that found none says `None found.`",
        ),
        (
            "none-found-beside-rows",
            "14: Disagreements: says `None found.` beside rows",
        ),
        (
            "second-table",
            "16: Disagreements: a second table; a read has one disagreements table",
        ),
        (
            "missing-disagreements",
            "1: Disagreements: missing; a read has a `## Disagreements` section",
        ),
    ] {
        let file = format!("refused/{read}.md");
        let output = design_read(&["check", &file]);
        assert_eq!(
            text(&output.stderr),
            format!("{file}:{refusal}\n"),
            "{read}"
        );
        assert_eq!(text(&output.stdout), "", "{read}");
        assert_eq!(output.status.code(), Some(1), "{read}");
    }
}

#[test]
fn a_read_that_cannot_be_read_or_a_malformed_command_line_exits_2() {
    let missing = design_read(&["check", "nowhere.md"]);
    assert_eq!(
        text(&missing.stderr),
        "nowhere.md: cannot be read: No such file or directory (os error 2)\n"
    );
    assert_eq!(missing.status.code(), Some(2));
    for arguments in [
        &[][..],
        &["check"],
        &["check", "a.md", "b.md"],
        &["verify", "a.md"],
    ] {
        assert_eq!(
            design_read(arguments).status.code(),
            Some(2),
            "{arguments:?}"
        );
    }
}
