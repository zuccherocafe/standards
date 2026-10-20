//! One read, parsed: its header fields, whether its "After the last phase" section fits its `Previous` field, and its
//! disagreements table. Every refusal names the line and the field; the caller names the file.
//!
//! The header is the `- Field: value` lines before the first level-two heading. Lines there that name no field are
//! prose. The table is the first table in the "Disagreements" section; a read that found none says `None found.` there
//! instead.

use std::collections::BTreeMap;
use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::markdown::{self, Line, Section, Sections};

/// The table's columns, in order.
const COLUMNS: [&str; 7] = [
    "ID",
    "Concept",
    "Owner",
    "Sites",
    "Consequence",
    "Done when",
    "Status",
];
const PREVIOUS: &str = "Previous";
const AFTER: &str = "After the last phase";
const DISAGREEMENTS: &str = "Disagreements";
const NONE_FOUND: &str = "None found.";

/// A read whose shape is right: its header fields are present once each, its after-note fits its previous read, and
/// its table's rows are complete with unique IDs.
#[derive(Debug)]
pub(crate) struct Read {
    previous: Option<Previous>,
    rows: Vec<Row>,
    /// The line of the "Disagreements" heading.
    line: usize,
}

impl Read {
    /// The read in `text`, or every refusal of its shape.
    pub(crate) fn parse(text: &str) -> Result<Self, Vec<Refusal>> {
        let sections = Sections::of(text);
        let mut refusals = Vec::new();
        let previous = header(&sections.lead, &mut refusals);
        after_note(&sections, &previous, &mut refusals);
        let mut found = sections.titled(DISAGREEMENTS);
        let Some(section) = found.next() else {
            refusals.push(Refusal::at(
                1,
                DISAGREEMENTS,
                "missing; a read has a `## Disagreements` section",
            ));
            return Err(refusals);
        };
        for again in found {
            refusals.push(again_refusal(again.line, DISAGREEMENTS, section.line));
        }
        let earlier = refusals.len();
        let table = table(section, &mut refusals);
        // A table whose rows are refused has rows; saying it has none would add a second, wrong refusal.
        let rowless = table.rows.is_empty() && refusals.len() == earlier;
        match (rowless, table.none_found) {
            (true, None) => refusals.push(Refusal::at(
                section.line,
                DISAGREEMENTS,
                "has no rows; a read that found none says `None found.`",
            )),
            (false, Some(line)) => {
                refusals.push(Refusal::at(
                    line,
                    DISAGREEMENTS,
                    "says `None found.` beside rows",
                ));
            }
            (true, Some(_)) | (false, None) => {}
        }
        let mut first = BTreeMap::new();
        for row in &table.rows {
            if let Some(line) = first.insert(&row.id, row.line) {
                refusals.push(Refusal::at(
                    row.line,
                    "ID",
                    format!("{} is also on line {line}", row.id),
                ));
            }
        }
        let previous = match previous {
            Named::Previous(previous) => Some(previous),
            Named::First => None,
            Named::Refused => return Err(refusals),
        };
        if refusals.is_empty() {
            Ok(Self {
                previous,
                rows: table.rows,
                line: section.line,
            })
        } else {
            Err(refusals)
        }
    }

    /// The previous read the header names, or none for a first read.
    pub(crate) fn previous(&self) -> Option<&Previous> {
        self.previous.as_ref()
    }

    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// A refusal of the table as a whole, at the "Disagreements" heading.
    pub(crate) fn refuse(&self, reason: String) -> Refusal {
        Refusal::at(self.line, DISAGREEMENTS, reason)
    }

    /// The count of rows by status, or that the read found none.
    pub(crate) fn summary(&self) -> String {
        if self.rows.is_empty() {
            return "none found".to_owned();
        }
        let count = |status: Status| self.rows.iter().filter(|row| row.status == status).count();
        let statuses = [Status::New, Status::Open, Status::Resolved, Status::Parked];
        let counts: Vec<String> = statuses
            .iter()
            .map(|&status| format!("{} {status}", count(status)))
            .collect();
        format!("disagreements: {}", counts.join(", "))
    }
}

/// The rows of the table in another read's "Disagreements" section, for comparison; the reason when it has no table
/// whose rows can be read. Nothing else in that read is checked.
pub(crate) fn rows_of(text: &str) -> Result<Vec<Row>, String> {
    let sections = Sections::of(text);
    let section = sections
        .titled(DISAGREEMENTS)
        .next()
        .ok_or("it has no `## Disagreements` section")?;
    let mut refusals = Vec::new();
    let table = table(section, &mut refusals);
    match refusals.first() {
        Some(refusal) => Err(refusal.to_string()),
        None => Ok(table.rows),
    }
}

/// The previous read a header names, as written, relative to the read naming it.
#[derive(Debug)]
pub(crate) struct Previous {
    path: Utf8PathBuf,
    line: usize,
}

impl Previous {
    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// A refusal of the `Previous` field.
    pub(crate) fn refuse(&self, reason: String) -> Refusal {
        Refusal::at(self.line, PREVIOUS, reason)
    }
}

/// One row of a disagreements table.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub(crate) id: RowId,
    pub(crate) status: Status,
    pub(crate) line: usize,
}

/// A disagreement's ID, qualified by the read that found it: `<read>-D<n>`, the read's name in lowercase letters,
/// digits and hyphens, and `n` a positive number. The qualifier keeps two reads' IDs apart, so an ID is not reused by
/// accident.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RowId(String);

impl RowId {
    fn new(text: &str) -> Option<Self> {
        let (read, number) = text.rsplit_once("-D")?;
        let read_ok = !read.is_empty()
            && !read.starts_with('-')
            && !read.ends_with('-')
            && read
                .chars()
                .all(|character| matches!(character, 'a'..='z' | '0'..='9' | '-'));
        let number_ok = !number.starts_with('0')
            && !number.is_empty()
            && number.chars().all(|digit| digit.is_ascii_digit());
        (read_ok && number_ok).then(|| Self(text.to_owned()))
    }
}

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a disagreement stands in this read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    /// Found by this read; not in the previous read.
    New,
    /// From the previous read, still unresolved, in this read's area.
    Open,
    /// From the previous read, and its completion condition is met.
    Resolved,
    /// From the previous read, still unresolved, outside this read's area; its `Done when` says where it is tracked.
    Parked,
}

impl Status {
    fn new(text: &str) -> Option<Self> {
        match text {
            "new" => Some(Self::New),
            "open" => Some(Self::Open),
            "resolved" => Some(Self::Resolved),
            "parked" => Some(Self::Parked),
            _ => None,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::New => "new",
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Parked => "parked",
        })
    }
}

/// Why a read is refused, at one line and field of it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    line: usize,
    field: &'static str,
    reason: String,
}

impl Refusal {
    pub(crate) fn at(line: usize, field: &'static str, reason: impl Into<String>) -> Self {
        Self {
            line,
            field,
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

/// What a header's `Previous` field names.
enum Named {
    /// The field is refused, so nothing that depends on it is checked.
    Refused,
    /// `none`: this is a first read.
    First,
    Previous(Previous),
}

fn again_refusal(line: usize, field: &'static str, first: usize) -> Refusal {
    Refusal::at(line, field, format!("given again; first on line {first}"))
}

/// What the header's `Previous` field names. Refuses each field that is missing, repeated or empty, and a malformed
/// `Revision` or `Previous`.
fn header(lead: &[Line<'_>], refusals: &mut Vec<Refusal>) -> Named {
    for name in ["Area", "Inspected"] {
        field(lead, name, refusals);
    }
    if let Some((line, value)) = field(lead, "Revision", refusals)
        && !(value.len() == 40 && value.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
    {
        refusals.push(Refusal::at(
            line,
            "Revision",
            format!("`{value}` is not a full commit id, 40 lowercase hexadecimal digits"),
        ));
    }
    match field(lead, PREVIOUS, refusals) {
        None => Named::Refused,
        Some((_, "none")) => Named::First,
        Some((line, value)) if Utf8Path::new(value).is_absolute() => {
            refusals.push(Refusal::at(
                line,
                PREVIOUS,
                format!("`{value}` is absolute; name the previous read relative to this one"),
            ));
            Named::Refused
        }
        Some((line, value)) => Named::Previous(Previous {
            path: value.into(),
            line,
        }),
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
        let (named, value) = line.text.strip_prefix("- ")?.split_once(':')?;
        (named.trim() == name).then(|| (line.number, value.trim()))
    });
    let Some((line, value)) = given.next() else {
        refusals.push(Refusal::at(
            1,
            name,
            format!("missing; a read's header has `- {name}: ...`"),
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

/// Refuses an "After the last phase" section that does not fit the previous read: required and non-empty when there
/// is one, absent when there is none. Not checked when the `Previous` field is refused.
fn after_note(sections: &Sections<'_>, previous: &Named, refusals: &mut Vec<Refusal>) {
    let mut found = sections.titled(AFTER);
    let first = found.next();
    if let Some(first) = first {
        for again in found {
            refusals.push(again_refusal(again.line, AFTER, first.line));
        }
    }
    match (previous, first) {
        (Named::Previous(previous), None) => refusals.push(Refusal::at(
            previous.line,
            AFTER,
            "missing; a read with a previous read begins with what changed since it",
        )),
        (Named::Previous(_), Some(section))
            if section.body.iter().all(|line| line.text.trim().is_empty()) =>
        {
            refusals.push(Refusal::at(section.line, AFTER, "empty"));
        }
        (Named::First, Some(section)) => refusals.push(Refusal::at(
            section.line,
            AFTER,
            "present in a first read; only a read with a previous read has one",
        )),
        (Named::Previous(_), Some(_)) | (Named::First, None) | (Named::Refused, _) => {}
    }
}

/// A disagreements section's rows, and the line saying `None found.` if any.
struct Table {
    rows: Vec<Row>,
    none_found: Option<usize>,
}

/// The rows of the first table in `section`, refusing its header, a missing or malformed delimiter row, every
/// incomplete row and any second table. A row is kept when its ID and status parse, so the rest of the table is still
/// checked.
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
    let Some((header, lines)) = blocks.next().and_then(<[Line<'_>]>::split_first) else {
        return Table {
            rows: Vec::new(),
            none_found,
        };
    };
    for again in blocks {
        refusals.push(Refusal::at(
            again[0].number,
            DISAGREEMENTS,
            "a second table; a read has one disagreements table",
        ));
    }
    let empty = Table {
        rows: Vec::new(),
        none_found,
    };
    let cells = markdown::cells(header.text);
    if cells != COLUMNS {
        refusals.push(Refusal::at(
            header.number,
            DISAGREEMENTS,
            format!(
                "columns are `{}`; a read's are `{}`",
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
            "the header row is not followed by a delimiter row, `|---|` for each of the 7 columns",
        ));
        return empty;
    }
    let rows = lines[1..]
        .iter()
        .filter_map(|&line| row(line, refusals))
        .collect();
    Table { rows, none_found }
}

/// The row on `line`, refusing a wrong number of cells, each empty cell, and a malformed ID or status.
fn row(line: Line<'_>, refusals: &mut Vec<Refusal>) -> Option<Row> {
    let cells = markdown::cells(line.text);
    let count = cells.len();
    let Ok(cells) = <[String; 7]>::try_from(cells) else {
        refusals.push(Refusal::at(
            line.number,
            DISAGREEMENTS,
            format!("has {count} cells; a row has 7"),
        ));
        return None;
    };
    for (column, cell) in COLUMNS.iter().zip(&cells) {
        if cell.is_empty() {
            refusals.push(Refusal::at(line.number, column, "empty"));
        }
    }
    let [id, .., status] = &cells;
    let parsed_id = RowId::new(id);
    if parsed_id.is_none() && !id.is_empty() {
        refusals.push(Refusal::at(
            line.number,
            "ID",
            format!("`{id}` is not qualified by its read as `<read>-D<n>`, such as `mm4-D1`"),
        ));
    }
    let parsed_status = Status::new(status);
    if parsed_status.is_none() && !status.is_empty() {
        refusals.push(Refusal::at(
            line.number,
            "Status",
            format!("`{status}` is not new, open, resolved or parked"),
        ));
    }
    Some(Row {
        id: parsed_id?,
        status: parsed_status?,
        line: line.number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    fn read(previous: &str, sections: &str) -> String {
        format!(
            "# t design read\n\n- Area: a\n- Revision: {REVISION}\n- Previous: {previous}\n- Inspected: i\n\n{sections}"
        )
    }

    fn reasons(text: &str) -> Vec<String> {
        Read::parse(text)
            .expect_err("refused")
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    const HEAD: &str = "| ID | Concept | Owner | Sites | Consequence | Done when | Status |\n|---|---|---|---|---|---|---|\n";
    const TABLE: &str = concat!(
        "## Disagreements\n\n",
        "| ID | Concept | Owner | Sites | Consequence | Done when | Status |\n|---|---|---|---|---|---|---|\n"
    );

    #[test]
    fn a_first_read_gives_its_rows_with_their_lines() {
        let text = read(
            "none",
            &format!(
                "{TABLE}| a-D1 | c | o | s | x | d | new |\n| a-D2 | c | o | s | x | d | new |\n"
            ),
        );
        let parsed = Read::parse(&text).expect("passes");
        assert!(parsed.previous().is_none());
        let rows: Vec<_> = parsed
            .rows()
            .iter()
            .map(|row| (row.id.to_string(), row.status, row.line))
            .collect();
        assert_eq!(
            rows,
            [
                ("a-D1".to_owned(), Status::New, 12),
                ("a-D2".to_owned(), Status::New, 13)
            ]
        );
        assert_eq!(
            parsed.summary(),
            "disagreements: 2 new, 0 open, 0 resolved, 0 parked"
        );
    }

    #[test]
    fn a_follow_up_names_its_previous_read_as_written() {
        let text = read(
            "../x/design-read.md",
            &format!(
                "## After the last phase\n\nn\n\n{TABLE}| a-D1 | c | o | s | x | d | open |\n"
            ),
        );
        let parsed = Read::parse(&text).expect("passes");
        assert_eq!(
            parsed.previous().map(Previous::path),
            Some(Utf8Path::new("../x/design-read.md"))
        );
    }

    #[test]
    fn an_id_is_a_lowercase_read_name_then_d_and_a_positive_number() {
        for good in ["mm4-D1", "cm1-b-D12", "x-D9"] {
            assert!(RowId::new(good).is_some(), "{good}");
        }
        for bad in [
            "D1", "-D1", "MM4-D1", "mm4-d1", "mm4-D0", "mm4-D01", "mm4-D", "mm4--D1", "mm4-D1a",
            "mm 4-D1",
        ] {
            assert!(RowId::new(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_previous_field_with_a_value_is_refused_when_absolute() {
        assert_eq!(
            reasons(&read(
                "/reads/design-read.md",
                &format!("{TABLE}None found.\n")
            )),
            [
                "5: Previous: `/reads/design-read.md` is absolute; name the previous read relative to this one"
            ]
        );
    }

    #[test]
    fn a_table_inside_fenced_code_is_an_example_not_the_reads_table() {
        let text = read(
            "none",
            &format!(
                "## Disagreements\n\n```\n{HEAD}| a-D1 | c | o | s | x | d | new |\n```\n\nNone found.\n"
            ),
        );
        assert_eq!(Read::parse(&text).expect("passes").summary(), "none found");
    }

    #[test]
    fn a_delimiter_row_has_a_cell_for_each_column_and_is_required() {
        let refused =
            "the header row is not followed by a delimiter row, `|---|` for each of the 7 columns";
        let header = HEAD.lines().next().expect("a header row");
        for (table, line) in [
            (
                format!("{header}\n|---|\n| a-D1 | c | o | s | x | d | new |\n"),
                11,
            ),
            (
                format!("{header}\n| a-D1 | c | o | s | x | d | new |\n"),
                11,
            ),
            (format!("{header}\n"), 10),
        ] {
            let text = read("none", &format!("## Disagreements\n\n{table}"));
            assert_eq!(
                reasons(&text),
                [format!("{line}: Disagreements: {refused}")],
                "{table}"
            );
        }
    }

    #[test]
    fn rows_of_another_read_refuse_a_table_they_cannot_read() {
        assert_eq!(
            rows_of("# x\n").map(|rows| rows.len()),
            Err("it has no `## Disagreements` section".to_owned())
        );
        assert_eq!(
            rows_of(&format!("{TABLE}| a-D1 | c | o | s | x | d | closed |\n"))
                .map(|rows| rows.len()),
            Err("5: Status: `closed` is not new, open, resolved or parked".to_owned())
        );
    }
}
