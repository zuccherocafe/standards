//! Writes the report's page, `index.html`. The page is plain HTML with its style inline. It loads nothing from outside,
//! reads in light and dark themes, and links only to files inside the output folder.
//!
//! The page's sections, in order, are what makes the report incomplete, the host programs not found, the reader's
//! notices about the steps file, the measuring-code warning, the measures that got worse, every step's outcome, what dandegate reused or kept, the steps'
//! sections, every measure, and the images.

use std::fmt::Write as _;

use crate::compare::image::{Cell, ImageState};
use crate::compare::measure::MeasureRow;
use crate::revision::{PerSide, Revision, Side};

/// One step instance's row.
#[derive(Debug)]
pub(crate) struct Shown {
    pub(crate) place: String,
    pub(crate) step: String,
    pub(crate) outcome: String,
    pub(crate) failed: bool,
    /// Its log, relative to the output folder, when it has one.
    pub(crate) log: Option<String>,
    /// What happened about reuse, for a reusable step.
    pub(crate) reuse: Option<String>,
}

/// One linked section.
#[derive(Debug)]
pub(crate) struct Section {
    pub(crate) title: String,
    /// The file, relative to the output folder.
    pub(crate) link: String,
    pub(crate) text: Option<String>,
    pub(crate) problem: Option<String>,
}

/// One step's images: each side's folder, relative to the output folder, and the cells.
#[derive(Debug)]
pub(crate) struct Images {
    pub(crate) step: String,
    pub(crate) folders: PerSide<String>,
    pub(crate) cells: Vec<Cell>,
}

/// Everything the page shows.
#[derive(Debug)]
pub(crate) struct Page<'a> {
    pub(crate) revisions: &'a PerSide<Revision>,
    pub(crate) incomplete: &'a [String],
    /// One warning for each host program not found, or why none can be given.
    pub(crate) host: &'a [String],
    /// What the reader noticed about the steps file that changes no step.
    pub(crate) notices: &'a [String],
    pub(crate) measure_code: Option<&'a [String]>,
    pub(crate) measure_code_error: Option<&'a str>,
    pub(crate) affected: &'a [&'a MeasureRow],
    pub(crate) worse: &'a [&'a MeasureRow],
    pub(crate) steps: &'a [Shown],
    pub(crate) sections: &'a [Section],
    pub(crate) measures: &'a [MeasureRow],
    pub(crate) images: &'a [Images],
}

const STYLE: &str = "<style>
:root { color-scheme: light dark; --text: #1d1d1f; --muted: #5f5f66; --background: #fbfbfa; --line: #d4d4d8;
  --warn: #b42318; --warn-background: #fdecea; }
@media (prefers-color-scheme: dark) {
  :root { --text: #ececee; --muted: #a1a1aa; --background: #18181b; --line: #3f3f46; --warn: #f97066;
    --warn-background: #3a1714; }
}
body { font: 15px/1.5 system-ui, sans-serif; margin: 0 auto; padding: 16px; max-width: 72rem; color: var(--text);
  background: var(--background); }
h1 { font-size: 1.4rem; } h2 { font-size: 1.15rem; margin-top: 2rem; }
table { border-collapse: collapse; display: block; overflow-x: auto; }
td, th { border: 1px solid var(--line); padding: 3px 8px; text-align: left; vertical-align: top; }
pre { overflow-x: auto; border: 1px solid var(--line); padding: 8px; }
code, pre { font: 13px/1.4 ui-monospace, monospace; }
.incomplete, .warning { border: 2px solid var(--warn); background: var(--warn-background); padding: 6px 12px; }
.notice { border: 1px solid var(--line); padding: 6px 12px; }
.failed td { color: var(--warn); }
.muted { color: var(--muted); }
figure { margin: 16px 0; } figcaption { margin-bottom: 4px; }
.pair { display: flex; gap: 8px; flex-wrap: wrap; } .pair > * { flex: 1 1 18rem; min-width: 0; }
img { max-width: 100%; border: 1px solid var(--line); }
</style>";

/// Text made safe inside HTML.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn table(html: &mut String, header: &[&str], rows: impl IntoIterator<Item = (bool, Vec<String>)>) {
    html.push_str("<table><tr>");
    for cell in header {
        let _ = write!(html, "<th>{}</th>", escape(cell));
    }
    html.push_str("</tr>\n");
    for (failed, cells) in rows {
        html.push_str(if failed {
            "<tr class=\"failed\">"
        } else {
            "<tr>"
        });
        for cell in cells {
            let _ = write!(html, "<td>{cell}</td>");
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>\n");
}

fn observations(row: &MeasureRow, side: Side) -> String {
    escape(
        &row.sides()[side]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | "),
    )
}

fn measure_rows<'a>(rows: impl IntoIterator<Item = &'a MeasureRow>) -> Vec<(bool, Vec<String>)> {
    rows.into_iter()
        .map(|row| {
            (
                false,
                vec![
                    format!("<code>{}</code>", escape(row.name().as_str())),
                    observations(row, Side::Before),
                    observations(row, Side::After),
                    escape(&row.change()),
                ],
            )
        })
        .collect()
}

/// The page's HTML.
pub(crate) fn render(page: &Page<'_>) -> String {
    let title = format!(
        "Changes from {} to {}",
        page.revisions.before, page.revisions.after
    );
    let mut html = format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{0}</title>\n{STYLE}</head>\n\
         <body>\n<h1>{0}</h1>\n",
        escape(&title)
    );
    incomplete(&mut html, page.incomplete);
    for warning in page.host {
        let _ = writeln!(
            html,
            "<div class=\"warning host\"><p>{}</p></div>",
            escape(warning)
        );
    }
    for notice in page.notices {
        let _ = writeln!(
            html,
            "<div class=\"notice\"><p>{}</p></div>",
            escape(notice)
        );
    }
    measure_code(&mut html, page);
    worse(&mut html, page.worse);
    steps(&mut html, page.steps);
    sections(&mut html, page.sections);
    measures(&mut html, page.measures);
    for images in page.images {
        let _ = writeln!(
            html,
            "<h2 id=\"images-{0}\">Images: {0}</h2>\n<p>Each number is the mean absolute \
             difference of the two images' RGBA samples, from 0 to 255, and 255 when their sizes differ. Missing, \
             unexpected and unreadable images come first, then the largest differences.</p>",
            escape(&images.step)
        );
        for cell in &images.cells {
            figure(&mut html, &images.folders, cell);
        }
    }
    html.push_str("</body></html>\n");
    html
}

fn incomplete(html: &mut String, reasons: &[String]) {
    if reasons.is_empty() {
        return;
    }
    html.push_str(
        "<div class=\"incomplete\" id=\"incomplete\"><p><strong>Incomplete.</strong> Read what follows as partial \
         evidence, not as \"nothing changed\":</p><ul>\n",
    );
    for reason in reasons {
        let _ = writeln!(html, "<li>{}</li>", escape(reason));
    }
    html.push_str("</ul></div>\n");
}

fn measure_code(html: &mut String, page: &Page<'_>) {
    if let Some(error) = page.measure_code_error {
        let _ = writeln!(
            html,
            "<div class=\"warning\" id=\"measure-code\"><p>dandegate could not tell whether the \
             measuring code differs: {}</p></div>",
            escape(error)
        );
    }
    let Some(files) = page.measure_code.filter(|files| !files.is_empty()) else {
        return;
    };
    let listed: Vec<String> = files
        .iter()
        .map(|file| format!("<code>{}</code>", escape(file)))
        .collect();
    let _ = write!(
        html,
        "<div class=\"warning\" id=\"measure-code\"><p>The measuring code differs between the revisions, so a \
         measure can move with the code rather than with what it measures: {}.</p>",
        listed.join(", ")
    );
    if page.affected.is_empty() {
        html.push_str("<p>No measure it takes moved.</p>");
    } else {
        let names: Vec<String> = page
            .affected
            .iter()
            .map(|row| format!("<code>{}</code>", escape(row.name().as_str())))
            .collect();
        let _ = write!(
            html,
            "<p>Measures it takes that moved: {}.</p>",
            names.join(", ")
        );
    }
    html.push_str("</div>\n");
}

fn worse(html: &mut String, rows: &[&MeasureRow]) {
    html.push_str("<h2 id=\"worse\">Got worse</h2>\n");
    if rows.is_empty() {
        html.push_str("<p>No measure moved the way its line says is worse.</p>\n");
    } else {
        html.push_str("<p>Measures that moved the way their line says is worse, largest relative change first.</p>\n");
        table(
            html,
            &["Measure", "Before", "After", "Change"],
            measure_rows(rows.iter().copied()),
        );
    }
}

fn steps(html: &mut String, shown: &[Shown]) {
    html.push_str("<h2 id=\"steps\">Steps</h2>\n");
    table(
        html,
        &["Side", "Step", "Outcome", "Log"],
        shown.iter().map(|shown| {
            let log = shown.log.as_ref().map_or_else(String::new, |log| {
                format!("<a href=\"{0}\">{0}</a>", escape(log))
            });
            (
                shown.failed,
                vec![
                    escape(&shown.place),
                    escape(&shown.step),
                    escape(&shown.outcome),
                    log,
                ],
            )
        }),
    );
    let reused: Vec<&Shown> = shown.iter().filter(|shown| shown.reuse.is_some()).collect();
    if reused.is_empty() {
        return;
    }
    html.push_str(
        "<h2 id=\"reuse\">Reuse</h2>\n<p>dandegate keeps each reusable step's complete success in the \
         repository's <code>.cache/dandegate/</code>, under the identity of everything the result depends on. \
         <code>--no-reuse</code> runs every step afresh.</p>\n",
    );
    table(
        html,
        &["Side", "Step", "Reuse"],
        reused.iter().map(|shown| {
            (
                false,
                vec![
                    escape(&shown.place),
                    escape(&shown.step),
                    escape(shown.reuse.as_deref().unwrap_or_default()),
                ],
            )
        }),
    );
}

fn sections(html: &mut String, sections: &[Section]) {
    if sections.is_empty() {
        return;
    }
    html.push_str("<h2 id=\"sections\">Sections</h2>\n");
    for section in sections {
        let _ = write!(
            html,
            "<h3>{}</h3>\n<p><a href=\"{1}\">{1}</a>",
            escape(&section.title),
            escape(&section.link)
        );
        if let Some(problem) = &section.problem {
            let _ = write!(
                html,
                " <strong class=\"failed\">{}</strong>",
                escape(problem)
            );
        }
        html.push_str("</p>\n");
        if let Some(text) = &section.text {
            let _ = writeln!(html, "<pre>{}</pre>", escape(text));
        }
    }
}

fn measures(html: &mut String, rows: &[MeasureRow]) {
    html.push_str("<h2 id=\"measures\">Measures</h2>\n");
    if rows.is_empty() {
        html.push_str("<p>No measures on either side.</p>\n");
    } else {
        html.push_str(
            "<p>Moved means more than 5% from the before value. It marks movement, not significance. A side with no \
             direction borrows the other side's direction.</p>\n",
        );
        table(
            html,
            &["Measure", "Before", "After", "Change"],
            measure_rows(rows),
        );
    }
}

fn figure(html: &mut String, folders: &PerSide<String>, cell: &Cell) {
    let mut notes: Vec<String> = cell.problems().iter().map(ToString::to_string).collect();
    if let Some(difference) = cell.difference() {
        notes.push(format!("{:.2}", difference.get()));
    }
    let _ = write!(
        html,
        "<figure><figcaption><code>{}</code>: {}</figcaption><div class=\"pair\">",
        escape(cell.name()),
        escape(&notes.join("; "))
    );
    for side in Side::BOTH {
        match &cell.image()[side] {
            ImageState::Absent => {
                let _ = write!(
                    html,
                    "<span class=\"muted\">no {} image</span>",
                    side.as_str()
                );
            }
            ImageState::Unreadable(_) | ImageState::Readable { .. } => {
                let _ = write!(
                    html,
                    "<img src=\"{}/{}\" alt=\"{} {}\">",
                    escape(&folders[side]),
                    escape(cell.name()),
                    side.as_str(),
                    escape(cell.name())
                );
            }
        }
    }
    html.push_str("</div></figure>\n");
}
