//! The little of Markdown a read's checks need: level-two sections, fenced code, and table rows split into cells.
//! Everything else is prose and stays text.

/// One line of a file, numbered from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line<'a> {
    pub(crate) number: usize,
    pub(crate) text: &'a str,
}

/// A file split at its level-two headings: the lines before the first, then each section in order. A heading inside
/// fenced code is code, not a heading.
#[derive(Debug)]
pub(crate) struct Sections<'a> {
    pub(crate) lead: Vec<Line<'a>>,
    pub(crate) sections: Vec<Section<'a>>,
}

/// One level-two section: its heading's title and line, and the lines under it. Lines inside fenced code are left out,
/// so a table or field shown as an example is never read as the read's own.
#[derive(Debug)]
pub(crate) struct Section<'a> {
    pub(crate) title: &'a str,
    pub(crate) line: usize,
    pub(crate) body: Vec<Line<'a>>,
}

impl<'a> Sections<'a> {
    pub(crate) fn of(text: &'a str) -> Self {
        let mut lead = Vec::new();
        let mut sections: Vec<Section<'a>> = Vec::new();
        let mut open: Option<Fence> = None;
        for (index, text) in text.lines().enumerate() {
            let line = Line {
                number: index + 1,
                text,
            };
            match (open, Fence::on(text)) {
                (Some(inside), Some((fence, rest))) => {
                    if fence.closes(inside) && rest.trim().is_empty() {
                        open = None;
                    }
                    continue;
                }
                (Some(_), None) => continue,
                (None, Some((fence, info))) if fence.marker != '`' || !info.contains('`') => {
                    open = Some(fence);
                    continue;
                }
                (None, Some(_) | None) => {}
            }
            if let Some(title) = text.strip_prefix("## ") {
                sections.push(Section {
                    title: title.trim(),
                    line: line.number,
                    body: Vec::new(),
                });
            } else if let Some(section) = sections.last_mut() {
                section.body.push(line);
            } else {
                lead.push(line);
            }
        }
        Self { lead, sections }
    }

    /// Every section titled `title`, in order.
    pub(crate) fn titled(&self, title: &str) -> impl Iterator<Item = &Section<'a>> {
        self.sections
            .iter()
            .filter(move |section| section.title == title)
    }
}

/// A run of three or more backticks or tildes, indented by at most three spaces, that opens or closes fenced code.
#[derive(Debug, Clone, Copy)]
struct Fence {
    marker: char,
    length: usize,
}

impl Fence {
    /// The fence `text` begins with, and the rest of the line after it.
    fn on(text: &str) -> Option<(Self, &str)> {
        let rest = text.trim_start_matches(' ');
        if text.len() - rest.len() > 3 {
            return None;
        }
        let marker = rest
            .chars()
            .next()
            .filter(|marker| matches!(marker, '`' | '~'))?;
        let after = rest.trim_start_matches(marker);
        let length = rest.len() - after.len();
        (length >= 3).then_some((Self { marker, length }, after))
    }

    /// Whether this fence closes the block `open` began: the same marker, at least as long.
    fn closes(self, open: Self) -> bool {
        self.marker == open.marker && self.length >= open.length
    }
}

/// Whether `text` is a table row: its first character past indentation is a pipe.
pub(crate) fn is_row(text: &str) -> bool {
    text.trim_start().starts_with('|')
}

/// A table row's cells, each trimmed. A backslash-escaped pipe stays in its cell as a pipe.
pub(crate) fn cells(row: &str) -> Vec<String> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = match row.strip_suffix('|') {
        Some(inner) if !inner.ends_with('\\') => inner,
        _ => row,
    };
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut characters = row.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&'|') => {
                characters.next();
                cell.push('|');
            }
            '|' => cells.push(std::mem::take(&mut cell)),
            other => cell.push(other),
        }
    }
    cells.push(cell);
    cells
        .into_iter()
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// Whether `cells` are a table's delimiter row, each cell dashes with an optional colon at either end.
pub(crate) fn is_delimiter(cells: &[String]) -> bool {
    cells.iter().all(|cell| {
        let dashes = cell.strip_prefix(':').unwrap_or(cell);
        let dashes = dashes.strip_suffix(':').unwrap_or(dashes);
        !dashes.is_empty() && dashes.chars().all(|character| character == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_in_fenced_code_starts_no_section_and_its_lines_are_left_out() {
        let sections = Sections::of("# t\n- a\n## One\nx\n```\n## Not\n| a |\n```\ny\n## Two\n");
        assert_eq!(
            sections
                .lead
                .iter()
                .map(|line| line.text)
                .collect::<Vec<_>>(),
            ["# t", "- a"]
        );
        let titles: Vec<_> = sections
            .sections
            .iter()
            .map(|section| (section.title, section.line))
            .collect();
        assert_eq!(titles, [("One", 3), ("Two", 10)]);
        let body: Vec<_> = sections.sections[0]
            .body
            .iter()
            .map(|line| (line.number, line.text))
            .collect();
        assert_eq!(body, [(4, "x"), (9, "y")]);
    }

    #[test]
    fn fenced_code_ends_only_at_a_matching_fence_at_least_as_long() {
        let text = "````markdown\n```rust\n## A\n```\n## A2\n~~~\n## A3\n```` x\n````\n## B\n   ~~~\n## C\n~~~~\n    ~~~\n## D\n";
        let titles: Vec<_> = Sections::of(text)
            .sections
            .iter()
            .map(|section| section.title)
            .collect();
        assert_eq!(titles, ["B", "D"]);
        let inline = Sections::of("``` a `b` ```\n## E\n");
        assert_eq!(
            inline
                .sections
                .iter()
                .map(|section| section.title)
                .collect::<Vec<_>>(),
            ["E"]
        );
    }

    #[test]
    fn a_row_splits_at_unescaped_pipes_and_keeps_an_escaped_one() {
        assert_eq!(cells("| a | `b\\|c` |  | d |"), ["a", "`b|c`", "", "d"]);
        assert_eq!(cells("a | b"), ["a", "b"]);
        assert_eq!(cells("| `C:\\x` | a\\ |"), ["`C:\\x`", "a\\"]);
        assert_eq!(cells("| a \\|"), ["a |"]);
    }

    #[test]
    fn a_delimiter_row_is_dashes_with_optional_colons() {
        let row = |text: &str| is_delimiter(&cells(text));
        assert!(row("|---|:--|--:|:-:|"));
        assert!(!row("|---|x|"));
        assert!(!row("|---||"));
    }
}
