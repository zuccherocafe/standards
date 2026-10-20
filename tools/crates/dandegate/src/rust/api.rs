//! The public API report's records and text. This module writes each side's listing record, reads the output of
//! `cargo-public-api diff` into its three parts, and writes the report that a reviewer reads before the implementation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::revision::PerSide;

/// The record format a side's listings are written in.
pub(crate) const LISTINGS_FORMAT: &str = "dandegate-api-listings-v1";

const REMOVED: &str = "Removed items from the public API";
const CHANGED: &str = "Changed items in the public API";
const ADDED: &str = "Added items to the public API";

/// A side's listings record, `listings.json`: its format, and its workspace's library crates or why they are unknown.
///
/// ```json
/// {"format": "dandegate-api-listings-v1",
///  "workspace": {"listed": {"crates": {"alpha": {"listed": {"items": 2}}, "beta": {"failed": {"why": "…"}}}}}}
/// {"format": "dandegate-api-listings-v1", "workspace": {"unknown": {"why": "…"}}}
/// ```
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Listings {
    pub(crate) format: String,
    pub(crate) workspace: Workspace,
}

/// A side's workspace: its library crates, by package name, each listed or with why it was not, or why the crates
/// could not be found at all. Unknown crates are never an empty workspace.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Workspace {
    Listed { crates: BTreeMap<String, Listed> },
    Unknown { why: String },
}

/// One library crate's listing on a side.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Listed {
    /// Listed: its items in `<package>.txt` and its rustdoc JSON in `<package>.json`.
    Listed { items: usize },
    /// Not listed, and why.
    Failed { why: String },
}

/// One crate's items that only the before side has, that changed signature, and that only the after side has.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Diff {
    pub(crate) removed: Vec<String>,
    pub(crate) changed: Vec<PerSide<String>>,
    pub(crate) added: Vec<String>,
}

impl Diff {
    fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.changed.is_empty() && self.added.is_empty()
    }
}

/// Lines in order, without repeats: `cargo-public-api` prints some items twice, and an item is one promise however
/// often it is printed.
pub(crate) fn distinct<T: PartialEq>(lines: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut found = Vec::new();
    for line in lines {
        if !found.contains(&line) {
            found.push(line);
        }
    }
    found
}

/// `cargo-public-api diff`'s three sections. A section is `(none)` or one item a line, `-` for the before side and `+`
/// for the after, and a changed item is its `-` line followed by its `+` line.
///
/// # Errors
/// What is not that shape.
pub(crate) fn parse_diff(text: &str) -> Result<Diff, String> {
    let mut sections: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut heading = None;
    for line in text.lines() {
        if [REMOVED, CHANGED, ADDED].contains(&line) {
            heading = Some(line);
            sections.insert(line, Vec::new());
        } else if let Some(current) = heading
            && (line.starts_with('-') || line.starts_with('+'))
        {
            sections.entry(current).or_default().push(line);
        }
    }
    let (Some(removed), Some(changed), Some(added)) = (
        sections.get(REMOVED),
        sections.get(CHANGED),
        sections.get(ADDED),
    ) else {
        return Err(format!(
            "expected the sections {REMOVED:?}, {CHANGED:?} and {ADDED:?}"
        ));
    };
    let pairs = changed.chunks(2).map(|pair| match pair {
        [old, new] if old.starts_with('-') && new.starts_with('+') => Ok(PerSide {
            before: old[1..].to_owned(),
            after: new[1..].to_owned(),
        }),
        _ => Err("a changed item is a `-` line followed by its `+` line".to_owned()),
    });
    Ok(Diff {
        removed: distinct(removed.iter().map(|line| line[1..].to_owned())),
        changed: distinct(pairs.collect::<Result<Vec<_>, _>>()?),
        added: distinct(added.iter().map(|line| line[1..].to_owned())),
    })
}

/// One crate's place in the report.
#[derive(Debug, Default)]
pub(crate) struct Crate {
    pub(crate) items: PerSide<Option<usize>>,
    pub(crate) diff: Option<Diff>,
    pub(crate) missing: Vec<String>,
}

/// The report's Markdown, crates by name, with why a side's crates are unknown.
pub(crate) fn report(title: &str, crates: &BTreeMap<String, Crate>, unknown: &[String]) -> String {
    let mut parts = vec![
        format!("# {title}"),
        "This report lists every library crate's public items, without blanket and auto-trait impls (`-ss`). \
         Each side's `api-listings` step holds its full listings. A changed item is one whose signature changed. \
         A renamed or moved item shows as removed and added."
            .to_owned(),
    ];
    let notes: Vec<String> = unknown
        .iter()
        .map(|why| format!("- {why}"))
        .chain(crates.iter().flat_map(|(name, found)| {
            found
                .missing
                .iter()
                .map(move |why| format!("- {name}: {why}"))
        }))
        .collect();
    if !notes.is_empty() {
        parts.push(format!(
            "This report is incomplete. A crate whose listing is missing below has no diff.\n\n{}",
            notes.join("\n")
        ));
    }
    let mut table = vec![
        "| Crate | Items before | Items after | Removed | Changed | Added |".to_owned(),
        "|---|---|---|---|---|---|".to_owned(),
    ];
    for (name, found) in crates {
        let size =
            |items: Option<usize>| items.map_or_else(|| "-".to_owned(), |items| items.to_string());
        let counts = found.diff.as_ref().map_or_else(
            || ["-".to_owned(), "-".to_owned(), "-".to_owned()],
            |diff| {
                [
                    diff.removed.len().to_string(),
                    diff.changed.len().to_string(),
                    diff.added.len().to_string(),
                ]
            },
        );
        table.push(format!(
            "| {name} | {} | {} | {} | {} | {} |",
            size(found.items.before),
            size(found.items.after),
            counts[0],
            counts[1],
            counts[2]
        ));
    }
    parts.push(table.join("\n"));
    for (name, found) in crates {
        let Some(diff) = found.diff.as_ref().filter(|diff| !diff.is_empty()) else {
            continue;
        };
        let mut section = vec![format!("## {name}")];
        let groups = [
            (
                "Removed",
                diff.removed
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>(),
            ),
            (
                "Changed",
                diff.changed
                    .iter()
                    .flat_map(|item| [format!("- {}", item.before), format!("+ {}", item.after)])
                    .collect(),
            ),
            (
                "Added",
                diff.added.iter().map(|item| format!("+ {item}")).collect(),
            ),
        ];
        for (title, lines) in groups {
            if !lines.is_empty() {
                section.push(format!("### {title}\n\n```diff\n{}\n```", lines.join("\n")));
            }
        }
        parts.push(section.join("\n\n"));
    }
    parts.join("\n\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_reads_as_removed_changed_and_added_items() {
        let text = "Removed items from the public API\n=================================\n-pub fn a::gone()\n\n\
                    Changed items in the public API\n===============================\n-pub fn a::f(u8)\n+pub fn a::f(u16)\n\n\
                    Added items to the public API\n=============================\n+pub fn a::new()\n+pub fn a::new()\n";
        assert_eq!(
            parse_diff(text),
            Ok(Diff {
                removed: vec!["pub fn a::gone()".to_owned()],
                changed: vec![PerSide {
                    before: "pub fn a::f(u8)".to_owned(),
                    after: "pub fn a::f(u16)".to_owned(),
                }],
                added: vec!["pub fn a::new()".to_owned()],
            })
        );
    }

    #[test]
    fn a_listings_record_holds_its_crates_or_why_they_are_unknown() {
        let listed = r#"{"format": "dandegate-api-listings-v1",
            "workspace": {"listed": {"crates": {"alpha": {"listed": {"items": 2}}, "beta": {"failed": {"why": "no"}}}}}}"#;
        let record: Listings = serde_json::from_str(listed).expect("a record");
        let Workspace::Listed { crates } = record.workspace else {
            panic!("listed crates");
        };
        assert!(matches!(crates["alpha"], Listed::Listed { items: 2 }));
        assert!(matches!(&crates["beta"], Listed::Failed { why } if why == "no"));
        let unknown = r#"{"format": "dandegate-api-listings-v1", "workspace": {"unknown": {"why": "no metadata"}}}"#;
        let record: Listings = serde_json::from_str(unknown).expect("a record");
        assert!(matches!(record.workspace, Workspace::Unknown { why } if why == "no metadata"));
        // A record without its workspace's state is not read as an empty workspace.
        let bare = r#"{"format": "dandegate-api-listings-v1", "crates": {}}"#;
        assert!(serde_json::from_str::<Listings>(bare).is_err());
    }

    #[test]
    fn a_crate_whose_items_did_not_change_has_a_row_and_no_section() {
        let mut crates = BTreeMap::new();
        crates.insert(
            "same".to_owned(),
            Crate {
                items: PerSide {
                    before: Some(1),
                    after: Some(1),
                },
                diff: Some(Diff::default()),
                missing: Vec::new(),
            },
        );
        let text = report("Public API", &crates, &[]);
        assert!(text.contains("| same | 1 | 1 | 0 | 0 | 0 |"), "{text}");
        assert!(!text.contains("## same"), "{text}");
    }

    #[test]
    fn a_diff_without_its_sections_or_with_an_unpaired_change_is_refused() {
        assert!(parse_diff("Removed items from the public API\n(none)\n").is_err());
        let unpaired = "Removed items from the public API\n(none)\nChanged items in the public API\n+pub fn f()\n\
                        Added items to the public API\n(none)\n";
        assert!(parse_diff(unpaired).is_err());
        let reversed = "Removed items from the public API\n(none)\nChanged items in the public API\n+pub fn f(u16)\n\
                        -pub fn f(u8)\nAdded items to the public API\n(none)\n";
        assert!(parse_diff(reversed).is_err());
    }
}
