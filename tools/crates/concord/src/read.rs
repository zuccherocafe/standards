//! One concord note's shape: its header fields and its disagreements table. Every refusal names the line and the field;
//! the caller names the file.
//!
//! The header is the `- Field: value` lines before the first level-two heading, and holds exactly the fields in
//! [`FIELDS`]. Lines there that are not `- Field: value` are prose. The table is the first table in the "Disagreements" section; a concord note with no open disagreements says
//! `None found.` there instead.

use std::collections::BTreeMap;
use std::fmt;

use crate::markdown::{self, Line, Section, Sections};

/// The table's columns, in order.
const COLUMNS: [&str; 5] = ["Concept", "Owner", "Sites", "Consequence", "Done when"];
const LAST_FULL_READ: &str = "Last full read";
/// The header's fields; any other is refused.
const FIELDS: [&str; 3] = ["Area", LAST_FULL_READ, "Inspected"];
const DISAGREEMENTS: &str = "Disagreements";
const NONE_FOUND: &str = "None found.";

/// Why a concord note is refused, at one line and field of it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    line: usize,
    field: String,
    reason: String,
}

impl Refusal {
    fn at(line: usize, field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            line,
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn line(&self) -> usize {
        self.line
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.line, self.field, self.reason)
    }
}

/// Every refusal of the shape of the concord note in `text`; none when it passes.
pub(crate) fn refusals(text: &str) -> Vec<Refusal> {
    let sections = Sections::of(text);
    let mut refusals = Vec::new();
    header(&sections.lead, &mut refusals);
    let mut found = sections.titled(DISAGREEMENTS);
    let Some(section) = found.next() else {
        refusals.push(Refusal::at(
            1,
            DISAGREEMENTS,
            "missing; a concord note has a `## Disagreements` section",
        ));
        return refusals;
    };
    for again in found {
        refusals.push(again_refusal(again.line, DISAGREEMENTS, section.line));
    }
    let earlier = refusals.len();
    let table = table(section, &mut refusals);
    // A table left after deleting its last row is still a table, so `None found.` beside it is refused.
    let present = section.body.iter().any(|line| markdown::is_row(line.text));
    // A table whose rows are refused has rows; saying it has none would add a second, wrong refusal.
    let rowless = table.rows.is_empty() && refusals.len() == earlier;
    match (present, table.none_found) {
        (true, Some(line)) => refusals.push(Refusal::at(
            line,
            DISAGREEMENTS,
            "says `None found.` beside a table",
        )),
        (_, None) if rowless => refusals.push(Refusal::at(
            section.line,
            DISAGREEMENTS,
            "has no rows; a concord note with no open disagreements says `None found.` instead of a table",
        )),
        (_, None) | (false, Some(_)) => {}
    }
    // Briefs and prose cite a row by its Concept, so two rows that differ only in case cannot be told apart.
    let mut first = BTreeMap::new();
    for row in table.rows.iter().filter(|row| !row.concept.is_empty()) {
        if let Some(line) = first.insert(row.concept.to_lowercase(), row.line) {
            refusals.push(Refusal::at(
                row.line,
                "Concept",
                format!("`{}` is also on line {line}", row.concept),
            ));
        }
    }
    refusals
}

/// One row of a disagreements table: its Concept, trimmed, and its line.
struct Row {
    concept: String,
    line: usize,
}

fn again_refusal(line: usize, field: &'static str, first: usize) -> Refusal {
    Refusal::at(line, field, format!("given again; first on line {first}"))
}

/// Refuses each header field that is missing, repeated, empty or not one of [`FIELDS`], and a `Last full read` that
/// is not a full commit id.
fn header(lead: &[Line<'_>], refusals: &mut Vec<Refusal>) {
    for line in lead {
        if let Some((name, _)) = named(line.text)
            && !FIELDS.contains(&name)
        {
            refusals.push(Refusal::at(
                line.number,
                name,
                "not a header field; a concord note's header has exactly Area, Last full read and Inspected",
            ));
        }
    }
    for name in ["Area", "Inspected"] {
        field(lead, name, refusals);
    }
    if let Some((line, value)) = field(lead, LAST_FULL_READ, refusals)
        && !(value.len() == 40 && value.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
    {
        refusals.push(Refusal::at(
            line,
            LAST_FULL_READ,
            format!("`{value}` is not a full commit id, 40 lowercase hexadecimal digits"),
        ));
    }
}

/// The line and value of the header field `name`, the first where it is given more than once; refuses it missing,
/// repeated or empty, and gives nothing when missing or empty.
fn field<'a>(
    lead: &[Line<'a>],
    name: &'static str,
    refusals: &mut Vec<Refusal>,
) -> Option<(usize, &'a str)> {
    let mut given = lead.iter().filter_map(|line| {
        let (named, value) = named(line.text)?;
        (named == name).then_some((line.number, value))
    });
    let Some((line, value)) = given.next() else {
        refusals.push(Refusal::at(
            1,
            name,
            format!("missing; a concord note's header has `- {name}: ...`"),
        ));
        return None;
    };
    for (again, _) in given {
        refusals.push(again_refusal(again, name, line));
    }
    if value.is_empty() {
        refusals.push(Refusal::at(line, name, "empty"));
        return None;
    }
    Some((line, value))
}

/// The field name and value of a `- Field: value` line, each trimmed.
fn named(text: &str) -> Option<(&str, &str)> {
    let (name, value) = text.strip_prefix("- ")?.split_once(':')?;
    Some((name.trim(), value.trim()))
}

/// A disagreements section's rows, and the line saying `None found.` if any.
struct Table {
    rows: Vec<Row>,
    none_found: Option<usize>,
}

/// The rows of the first table in `section`, refusing its header, a missing or malformed delimiter row, every
/// incomplete row and any second table. A row with the right number of cells is kept, so the rest of the table is
/// still checked.
fn table(section: &Section<'_>, refusals: &mut Vec<Refusal>) -> Table {
    let none_found = section
        .body
        .iter()
        .find(|line| line.text.trim() == NONE_FOUND)
        .map(|line| line.number);
    let mut blocks = section
        .body
        .split(|line| !markdown::is_row(line.text))
        .filter(|block| !block.is_empty());
    let empty = Table {
        rows: Vec::new(),
        none_found,
    };
    let Some((header, lines)) = blocks.next().and_then(<[Line<'_>]>::split_first) else {
        return empty;
    };
    for again in blocks {
        refusals.push(Refusal::at(
            again[0].number,
            DISAGREEMENTS,
            "a second table; a concord note has one disagreements table",
        ));
    }
    let cells = markdown::cells(header.text);
    if cells != COLUMNS {
        refusals.push(Refusal::at(
            header.number,
            DISAGREEMENTS,
            format!(
                "columns are `{}`; a concord note's are `{}`",
                cells.join(" | "),
                COLUMNS.join(" | ")
            ),
        ));
        return empty;
    }
    let delimiter = lines.first().filter(|line| {
        let cells = markdown::cells(line.text);
        cells.len() == COLUMNS.len() && markdown::is_delimiter(&cells)
    });
    if delimiter.is_none() {
        let line = lines.first().unwrap_or(header).number;
        refusals.push(Refusal::at(
            line,
            DISAGREEMENTS,
            "the header row is not followed by a delimiter row, `|---|` for each of the 5 columns",
        ));
        return empty;
    }
    let rows = lines[1..]
        .iter()
        .filter_map(|&line| row(line, refusals))
        .collect();
    Table { rows, none_found }
}

/// The row on `line`, refusing a wrong number of cells and each empty cell.
fn row(line: Line<'_>, refusals: &mut Vec<Refusal>) -> Option<Row> {
    let cells = markdown::cells(line.text);
    let count = cells.len();
    let Ok(cells) = <[String; 5]>::try_from(cells) else {
        refusals.push(Refusal::at(
            line.number,
            DISAGREEMENTS,
            format!("has {count} cells; a row has 5"),
        ));
        return None;
    };
    for (column, cell) in COLUMNS.iter().zip(&cells) {
        if cell.is_empty() {
            refusals.push(Refusal::at(line.number, *column, "empty"));
        }
    }
    let [concept, ..] = cells;
    Some(Row {
        concept,
        line: line.number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const HEAD: &str =
        "| Concept | Owner | Sites | Consequence | Done when |\n|---|---|---|---|---|\n";

    fn area(last_full_read: &str, disagreements: &str) -> String {
        format!(
            "# t concord note\n\n- Area: a\n- Last full read: {last_full_read}\n- Inspected: i\n\n\
             ## Disagreements\n\n{disagreements}"
        )
    }

    fn reasons(text: &str) -> Vec<String> {
        refusals(text).iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_concord_note_with_rows_or_none_found_passes() {
        let rows = format!("{HEAD}| a | o | s | x | d |\n| b | o | s | x | d |\n");
        assert_eq!(reasons(&area(COMMIT, &rows)), Vec::<String>::new());
        assert_eq!(
            reasons(&area(COMMIT, "None found.\n")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_field_beyond_the_three_is_refused_and_prose_and_fenced_examples_are_not() {
        let text = area(COMMIT, "None found.\n").replace(
            "- Inspected: i\n",
            "- Inspected: i\n- Previous: old.md\nA note: prose.\n```\n- Revision: x\n```\n",
        );
        assert_eq!(
            reasons(&text),
            [
                "6: Previous: not a header field; a concord note's header has exactly Area, Last full read and Inspected"
            ]
        );
    }

    #[test]
    fn none_found_is_refused_beside_a_table_without_rows() {
        assert_eq!(
            reasons(&area(COMMIT, &format!("{HEAD}\nNone found.\n"))),
            ["12: Disagreements: says `None found.` beside a table"]
        );
    }

    #[test]
    fn the_last_full_read_is_forty_lowercase_hexadecimal_digits() {
        let upper = COMMIT.to_uppercase();
        let long = format!("{COMMIT}0");
        for bad in [&COMMIT[..39], &upper, &long, "main"] {
            assert_eq!(
                reasons(&area(bad, "None found.\n")),
                [format!(
                    "4: Last full read: `{bad}` is not a full commit id, 40 lowercase hexadecimal digits"
                )],
                "{bad}"
            );
        }
    }

    #[test]
    fn a_concept_is_compared_ignoring_case() {
        let rows =
            format!("{HEAD}| Read trigger | o | s | x | d |\n| read TRIGGER | o | s | x | d |\n");
        assert_eq!(
            reasons(&area(COMMIT, &rows)),
            ["12: Concept: `read TRIGGER` is also on line 11"]
        );
    }

    #[test]
    fn empty_concepts_are_refused_as_empty_and_not_as_repeated() {
        let rows = format!("{HEAD}|  | o | s | x | d |\n|  | o | s | x | d |\n");
        assert_eq!(
            reasons(&area(COMMIT, &rows)),
            ["11: Concept: empty", "12: Concept: empty"]
        );
    }

    #[test]
    fn a_table_inside_fenced_code_is_an_example_not_the_concord_notes_table() {
        let text = area(
            COMMIT,
            &format!("```\n{HEAD}| a | o | s | x | d |\n```\n\nNone found.\n"),
        );
        assert_eq!(reasons(&text), Vec::<String>::new());
    }

    #[test]
    fn a_delimiter_row_has_a_cell_for_each_column_and_is_required() {
        let refused =
            "the header row is not followed by a delimiter row, `|---|` for each of the 5 columns";
        let header = HEAD.lines().next().expect("a header row");
        for (table, line) in [
            (format!("{header}\n|---|\n| a | o | s | x | d |\n"), 10),
            (format!("{header}\n| a | o | s | x | d |\n"), 10),
            (format!("{header}\n"), 9),
        ] {
            assert_eq!(
                reasons(&area(COMMIT, &table)),
                [format!("{line}: Disagreements: {refused}")],
                "{table}"
            );
        }
    }
}
