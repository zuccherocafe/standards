//! Runs the GDScript scans as the built-in `gdscript-scans` step, which dandegate adds when `dandegate.toml` names the
//! Godot project; [`crate::plan`] describes the file.
//!
//! The after revision's `gdscript-scans` reads both checkouts, in the after checkout's development shell, as the API
//! diff's `cargo-public-api` does. This is the exception to each revision running its own pinned tools: one scanner
//! judges both sides, so a change to the scanner never shows as findings the change made, and a before revision whose
//! shell predates the scanner is still read. A test folder is passed only on a side that holds it, and a side without
//! the project folder has no findings.
//!
//! The section lists the findings the change added, then those it removed, by their key (scan, file, symbol), so a
//! finding whose line moved is still present. A file unknown on either side is not compared: its findings are neither
//! added nor removed, as the API diff treats an incomplete listing as incomplete, never as empty. Nor, then, are the
//! unused and write-only findings of any file, since each rests on every file's references: an unknown file's names
//! can hide or reveal a finding anywhere. Only bare-type, which reads one file, is still compared, file by file,
//! unless `project.godot` is the unknown file: its autoload names are part of the symbols of scripts without a
//! `class_name`, so then nothing is compared. A file deleted in the after tree is not unknown, so its findings are removed. The section then gives the count still
//! present and each side's unknown files. It fails nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::built_in::{Context, Stop};
use crate::plan::GodotProject;
use crate::revision::{PerSide, Side};

/// What identifies a finding from one revision to the next.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    scan: String,
    file: String,
    symbol: String,
}

/// What a finding shows besides its key.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shown {
    line: u32,
    detail: String,
}

/// The project file, relative to the project, whose autoload names are part of some findings' symbols.
const PROJECT_FILE: &str = "project.godot";

/// The scans whose findings rest on every file's references, so an unknown file anywhere leaves them incomplete.
const CROSS_FILE: [&str; 2] = ["unused", "write-only"];

/// One line gdscript-scans prints: a finding, or an unknown file, told apart by the `unknown` field.
#[derive(Debug)]
enum Printed {
    Finding(PrintedFinding),
    Unknown(PrintedUnknown),
}

impl<'de> Deserialize<'de> for Printed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(PrintedVisitor)
    }
}

struct PrintedVisitor;

impl<'de> Visitor<'de> for PrintedVisitor {
    type Value = Printed;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "a finding {\"scan\", \"file\", \"symbol\", \"line\", \"detail\"} \
             or an unknown file {\"unknown\", \"reason\"}",
        )
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Printed, A::Error> {
        let mut fields = serde_json::Map::new();
        while let Some((name, value)) = map.next_entry::<String, serde_json::Value>()? {
            if fields.contains_key(&name) {
                return Err(de::Error::custom(format!("duplicate field `{name}`")));
            }
            fields.insert(name, value);
        }
        let unknown = fields.contains_key("unknown");
        let value = serde_json::Value::Object(fields);
        if unknown {
            PrintedUnknown::deserialize(value).map(Printed::Unknown)
        } else {
            PrintedFinding::deserialize(value).map(Printed::Finding)
        }
        .map_err(de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrintedFinding {
    scan: String,
    file: String,
    symbol: String,
    line: u32,
    detail: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrintedUnknown {
    unknown: String,
    reason: String,
}

/// One side's findings by key, and its unknown files with why.
#[derive(Debug, Default, PartialEq, Eq)]
struct Listing {
    findings: BTreeMap<Key, Shown>,
    unknown: BTreeMap<String, String>,
}

/// What gdscript-scans printed, read into a listing.
fn listing(text: &str) -> Result<Listing, String> {
    let mut read = Listing::default();
    for (number, line) in text.lines().enumerate() {
        let printed: Printed = serde_json::from_str(line)
            .map_err(|error| format!("its line {} cannot be read: {error}", number + 1))?;
        match printed {
            Printed::Finding(finding) => {
                read.findings.insert(
                    Key {
                        scan: finding.scan,
                        file: finding.file,
                        symbol: finding.symbol,
                    },
                    Shown {
                        line: finding.line,
                        detail: finding.detail,
                    },
                );
            }
            Printed::Unknown(unknown) => {
                read.unknown.insert(unknown.unknown, unknown.reason);
            }
        }
    }
    Ok(read)
}

/// The two sides compared.
#[derive(Debug, Default, PartialEq, Eq)]
struct Comparison {
    added: Vec<(Key, Shown)>,
    removed: Vec<(Key, Shown)>,
    still: usize,
    /// The files unknown on either side, whose findings are not compared.
    not_compared: BTreeSet<String>,
    /// Whether any finding is compared: not when `project.godot` is unknown, since the autoload names in it identify
    /// the scripts without a `class_name`.
    identities: bool,
    /// Whether the unused and write-only findings are compared: only when neither side has an unknown file.
    cross_file: bool,
}

fn compare(before: &Listing, after: &Listing) -> Comparison {
    let not_compared: BTreeSet<String> = before
        .unknown
        .keys()
        .chain(after.unknown.keys())
        .cloned()
        .collect();
    let cross_file = not_compared.is_empty();
    let identities = !not_compared.contains(PROJECT_FILE);
    let compared = |key: &&Key| {
        !not_compared.contains(&key.file)
            && identities
            && (cross_file || !CROSS_FILE.contains(&key.scan.as_str()))
    };
    let only = |side: &Listing, other: &Listing| -> Vec<(Key, Shown)> {
        side.findings
            .iter()
            .filter(|(key, _)| compared(key) && !other.findings.contains_key(key))
            .map(|(key, shown)| (key.clone(), shown.clone()))
            .collect()
    };
    Comparison {
        added: only(after, before),
        removed: only(before, after),
        still: after
            .findings
            .keys()
            .filter(|key| compared(key) && before.findings.contains_key(key))
            .count(),
        not_compared,
        identities,
        cross_file,
    }
}

/// Run the scans over both checkouts and write the section.
///
/// # Errors
/// [`Stop`] when its time runs out or its files cannot be written. A side whose scan fails makes a partial section
/// instead.
pub(crate) fn scans(context: &Context<'_, GodotProject>) -> Result<(), Stop> {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    let mut sides = PerSide::<Option<Listing>>::default();
    for side in Side::BOTH {
        match scan_side(context, side, &mut notes)? {
            Ok(read) => sides[side] = Some(read),
            Err(why) => problems.push(format!(
                "the {} side's scans did not finish: {why}",
                side.as_str()
            )),
        }
    }
    let text = section(context, &sides, &notes);
    context.write("gdscript-scans.md", &text, &problems)
}

/// One side's listing, or why there is none.
fn scan_side(
    context: &Context<'_, GodotProject>,
    side: Side,
    notes: &mut Vec<String>,
) -> Result<Result<Listing, String>, Stop> {
    let checkout = match &context.checkouts[side] {
        Ok(checkout) => checkout,
        Err(why) => return Ok(Err(format!("its checkout could not be made: {why}"))),
    };
    let project = checkout.join(context.settings.project().as_path());
    if !project.is_dir() {
        notes.push(format!(
            "The {} revision has no {}, so it has no findings.",
            side.as_str(),
            context.settings.project()
        ));
        return Ok(Ok(Listing::default()));
    }
    let mut command = vec![
        "gdscript-scans".to_owned(),
        "--project".to_owned(),
        project.to_string(),
    ];
    for tests in context.settings.tests() {
        let folder = checkout.join(tests.as_path());
        if folder.is_dir() {
            command.extend(["--tests".to_owned(), folder.to_string()]);
        }
    }
    let words: Vec<&str> = command.iter().map(String::as_str).collect();
    let printed = context.out.join(format!("{}.jsonl", side.as_str()));
    if let Err(error) = context.shell(Side::After, Utf8Path::new("."), &words, &[], &printed) {
        return Ok(Err(error.partial()?));
    }
    let text = fs::read_to_string(&printed).map_err(Stop::Write)?;
    Ok(listing(&text))
}

fn section(
    context: &Context<'_, GodotProject>,
    sides: &PerSide<Option<Listing>>,
    notes: &[String],
) -> String {
    let mut text = format!(
        "# GDScript scans from {} to {}\n\nThe GDScript standard's report scans list candidates for review and fail \
         nothing. The after revision's gdscript-scans read both revisions, and `before.jsonl` and `after.jsonl` hold \
         what it printed. A finding is known by its scan, file and symbol, so one whose line moved is still present. \
         A file unknown on either side is not compared.\n",
        context.revisions.before, context.revisions.after
    );
    for note in notes {
        let _ = write!(text, "\n{note}\n");
    }
    let (Some(before), Some(after)) = (&sides.before, &sides.after) else {
        text.push_str(
            "\nNothing is compared: a side's scans did not finish. The step's log says why.\n",
        );
        return text;
    };
    let compared = compare(before, after);
    if !compared.identities {
        text.push_str(
            "\nNothing is compared: `project.godot` is unknown on a side, so the autoload names that identify the \
             scripts without a `class_name` are unavailable.\n",
        );
    } else if !compared.cross_file {
        let files: Vec<String> = compared
            .not_compared
            .iter()
            .map(|file| format!("`{file}`"))
            .collect();
        let _ = write!(
            text,
            "\nThe unused and write-only scans are not compared: each rests on every file's references, and {} \
             unknown on a side. Only bare-type findings are compared, file by file.\n",
            match files.as_slice() {
                [one] => format!("{one} is"),
                _ => format!("{} are", files.join(", ")),
            }
        );
    }
    let list = |text: &mut String, title: &str, findings: &[(Key, Shown)]| {
        let _ = write!(text, "\n## {title} ({})\n\n", findings.len());
        if findings.is_empty() {
            text.push_str("None.\n");
        }
        for (key, shown) in findings {
            let _ = writeln!(
                text,
                "- {} `{}:{}` {}: {}",
                key.scan, key.file, shown.line, key.symbol, shown.detail
            );
        }
    };
    list(&mut text, "Added", &compared.added);
    list(&mut text, "Removed", &compared.removed);
    let _ = write!(
        text,
        "\n## Still present\n\n{}.\n\n## Unknown\n",
        counted(compared.still, "finding")
    );
    for side in Side::BOTH {
        let listing = if side == Side::Before { before } else { after };
        let _ = write!(text, "\nThe {} side: ", side.as_str());
        if listing.unknown.is_empty() {
            text.push_str("none.\n");
        } else {
            let _ = writeln!(text, "{}.\n", counted(listing.unknown.len(), "file"));
            for (file, reason) in &listing.unknown {
                let _ = writeln!(text, "- `{file}`: {reason}");
            }
        }
    }
    if !compared.not_compared.is_empty() {
        let files: Vec<String> = compared
            .not_compared
            .iter()
            .map(|file| format!("`{file}`"))
            .collect();
        let _ = write!(text, "\nNot compared: {}.\n", files.join(", "));
    }
    text
}

/// `count` and the noun, plural unless the count is one.
fn counted(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(scan: &str, file: &str, symbol: &str, line: u32) -> String {
        format!(
            r#"{{"scan":"{scan}","file":"{file}","symbol":"{symbol}","line":{line},"detail":"d"}}"#
        )
    }

    fn read(lines: &[String]) -> Listing {
        listing(&lines.join("\n")).expect("a listing")
    }

    fn keys(findings: &[(Key, Shown)]) -> Vec<String> {
        findings
            .iter()
            .map(|(key, _)| format!("{} {}", key.scan, key.symbol))
            .collect()
    }

    #[test]
    fn an_unchanged_finding_whose_line_moved_is_still_present() {
        let before = read(&[finding("unused", "a.gd", "A.f", 3)]);
        let after = read(&[finding("unused", "a.gd", "A.f", 9)]);
        let compared = compare(&before, &after);
        assert_eq!(
            (compared.added.len(), compared.removed.len(), compared.still),
            (0, 0, 1)
        );
    }

    #[test]
    fn a_finding_on_one_side_only_is_added_or_removed() {
        let before = read(&[finding("unused", "a.gd", "A.old", 3)]);
        let after = read(&[
            finding("unused", "a.gd", "A.new", 3),
            finding("bare-type", "a.gd", "A.old", 3),
        ]);
        let compared = compare(&before, &after);
        assert_eq!(keys(&compared.added), ["bare-type A.old", "unused A.new"]);
        assert_eq!(keys(&compared.removed), ["unused A.old"]);
    }

    #[test]
    fn a_deleted_files_findings_are_removed() {
        let before = read(&[finding("write-only", "gone.gd", "Gone.flag", 4)]);
        let compared = compare(&before, &Listing::default());
        assert_eq!(keys(&compared.removed), ["write-only Gone.flag"]);
        assert!(compared.not_compared.is_empty());
    }

    #[test]
    fn a_file_unknown_on_one_side_is_not_compared_on_either() {
        let before = read(&[
            finding("bare-type", "a.gd", "A.f", 3),
            finding("bare-type", "b.gd", "B.g", 3),
        ]);
        let after = read(&[
            finding("bare-type", "b.gd", "B.g", 3),
            unknown("a.gd", "the grammar cannot parse it at line 2"),
        ]);
        let compared = compare(&before, &after);
        assert_eq!(
            (compared.added.len(), compared.removed.len(), compared.still),
            (0, 0, 1)
        );
        assert_eq!(compared.not_compared, BTreeSet::from(["a.gd".to_owned()]));
    }

    fn unknown(file: &str, reason: &str) -> String {
        format!(r#"{{"unknown":"{file}","reason":"{reason}"}}"#)
    }

    #[test]
    fn an_unknown_file_leaves_another_files_cross_file_findings_uncompared() {
        // Both resets are unused before; after, b.gd cannot be parsed, so its reset reads as a use of a.gd's.
        for reason in ["the grammar cannot parse it at line 3", "it is not UTF-8"] {
            let before = read(&[
                finding("unused", "a.gd", "A.reset", 2),
                finding("unused", "b.gd", "B.reset", 2),
                finding("bare-type", "a.gd", "A.parse", 4),
                finding("bare-type", "c.gd", "C.old", 4),
            ]);
            let after = read(&[
                finding("bare-type", "a.gd", "A.parse", 4),
                finding("bare-type", "c.gd", "C.new", 4),
                unknown("b.gd", reason),
            ]);
            let compared = compare(&before, &after);
            assert!(!compared.cross_file, "{reason}");
            assert_eq!(keys(&compared.removed), ["bare-type C.old"], "{reason}");
            assert_eq!(keys(&compared.added), ["bare-type C.new"], "{reason}");
            assert_eq!(compared.still, 1, "{reason}");
        }
    }

    #[test]
    fn an_unknown_project_file_leaves_even_bare_type_uncompared() {
        // registry.gd has no class_name, so its class is its autoload name only while project.godot can be read.
        let before = read(&[finding("bare-type", "registry.gd", "Registry.lookup", 3)]);
        let after = read(&[
            finding("bare-type", "registry.gd", "registry.lookup", 3),
            unknown("project.godot", "it is not UTF-8"),
        ]);
        let compared = compare(&before, &after);
        assert!(!compared.identities);
        assert_eq!(
            (compared.added.len(), compared.removed.len(), compared.still),
            (0, 0, 0)
        );
    }

    #[test]
    fn a_duplicate_field_is_refused() {
        let refused = listing(
            r#"{"scan":"unused","file":"a.gd","file":"b.gd","symbol":"A.f","line":1,"detail":"d"}"#,
        )
        .expect_err("a refusal");
        assert!(refused.contains("duplicate field `file`"), "{refused}");
    }

    #[test]
    fn a_line_that_is_not_an_object_names_the_two_shapes_expected() {
        let refused = listing("[1]\n").expect_err("a refusal");
        assert!(
            refused.contains(r#"expected a finding {"scan", "file", "symbol", "line", "detail"} or an unknown file"#),
            "{refused}"
        );
        let missing = listing("{\"unknown\":\"a.gd\"}\n").expect_err("a refusal");
        assert!(missing.contains("missing field `reason`"), "{missing}");
        let extra = listing(&format!(
            "{}\n",
            finding("unused", "a.gd", "A.f", 1).replace('}', r#","colour":"red"}"#)
        ))
        .expect_err("a refusal");
        assert!(extra.contains("unknown field `colour`"), "{extra}");
    }

    #[test]
    fn a_line_that_is_neither_a_finding_nor_an_unknown_is_refused() {
        assert_eq!(
            listing("{\"scan\":\"unused\"}\n")
                .map_err(|why| why.starts_with("its line 1 cannot be read")),
            Err(true)
        );
    }
}
