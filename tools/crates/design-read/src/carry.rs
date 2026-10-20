//! What a read carries forward from its previous read. A disagreement keeps its ID from read to read, so comparing IDs
//! is enough: an ID is `new` exactly when the previous read lacks it, and every ID the previous read left `new`, `open`
//! or `parked` appears again, whatever its status now. Leaving one out would read as resolved without saying so.
//!
//! A first read compares with no rows, so every row in it is `new`.

use std::collections::{BTreeMap, BTreeSet};

use crate::read::{Read, Refusal, Row, Status};

/// Every way `read` fails to carry forward `previous`, the rows of its previous read.
pub(crate) fn refusals(read: &Read, previous: &[Row]) -> Vec<Refusal> {
    let before: BTreeMap<_, _> = previous.iter().map(|row| (&row.id, row.status)).collect();
    let mut refusals = Vec::new();
    for row in read.rows() {
        let reason = match (row.status, before.contains_key(&row.id)) {
            (Status::New, true) => {
                format!(
                    "`new`, but {} is in the previous read; carry it as open, resolved or parked",
                    row.id
                )
            }
            (Status::Open | Status::Resolved | Status::Parked, false) => {
                format!(
                    "`{}`, but {} is not in a previous read; a disagreement this read found is new",
                    row.status, row.id
                )
            }
            (Status::New, false) | (Status::Open | Status::Resolved | Status::Parked, true) => {
                continue;
            }
        };
        refusals.push(Refusal::at(row.line, "Status", reason));
    }
    let here: BTreeSet<_> = read.rows().iter().map(|row| &row.id).collect();
    for (id, status) in before {
        let unresolved = match status {
            Status::New | Status::Open | Status::Parked => true,
            Status::Resolved => false,
        };
        if unresolved && !here.contains(id) {
            refusals.push(read.refuse(format!(
                "{id} is {status} in the previous read and missing here; carry it as open, resolved or parked"
            )));
        }
    }
    refusals
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn read(previous: &str, rows: &[(&str, &str)]) -> Read {
        let mut text = format!(
            "- Area: a\n- Revision: {}\n- Previous: {previous}\n- Inspected: i\n",
            "f".repeat(40)
        );
        if previous != "none" {
            text.push_str("## After the last phase\nn\n");
        }
        text.push_str("## Disagreements\n| ID | Concept | Owner | Sites | Consequence | Done when | Status |\n|---|---|---|---|---|---|---|\n");
        for (id, status) in rows {
            writeln!(text, "| {id} | c | o | s | x | d | {status} |")
                .expect("a String takes any text");
        }
        if rows.is_empty() {
            text.push_str("None found.\n");
        }
        Read::parse(&text).expect("a well-shaped read")
    }

    fn previous(rows: &[(&str, &str)]) -> Vec<Row> {
        read("none", rows).rows().to_vec()
    }

    fn reasons(read: &Read, previous: &[Row]) -> Vec<String> {
        refusals(read, previous)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn every_unresolved_id_carried_with_any_status_and_new_ones_new_passes() {
        let before = previous(&[
            ("a-D1", "new"),
            ("a-D2", "open"),
            ("a-D3", "parked"),
            ("a-D4", "new"),
        ]);
        let after = read(
            "p.md",
            &[
                ("a-D1", "open"),
                ("a-D2", "resolved"),
                ("a-D3", "parked"),
                ("a-D4", "open"),
                ("b-D1", "new"),
            ],
        );
        assert_eq!(reasons(&after, &before), Vec::<String>::new());
    }

    #[test]
    fn an_id_the_previous_read_resolved_may_be_left_out() {
        let before = previous(&[("a-D1", "new"), ("a-D2", "resolved")]);
        assert_eq!(
            reasons(&read("p.md", &[("a-D1", "open")]), &before),
            Vec::<String>::new()
        );
    }

    #[test]
    fn an_unresolved_id_left_out_is_refused_whatever_it_was() {
        let before = previous(&[("a-D1", "new"), ("a-D2", "open"), ("a-D3", "parked")]);
        let mut refused = reasons(&read("p.md", &[]), &before);
        refused.sort();
        assert_eq!(
            refused,
            [
                "7: Disagreements: a-D1 is new in the previous read and missing here; carry it as open, resolved or parked",
                "7: Disagreements: a-D2 is open in the previous read and missing here; carry it as open, resolved or parked",
                "7: Disagreements: a-D3 is parked in the previous read and missing here; carry it as open, resolved or parked",
            ]
        );
    }

    #[test]
    fn new_is_refused_for_a_carried_id_and_the_other_statuses_for_a_new_one() {
        let before = previous(&[("a-D1", "new")]);
        let after = read(
            "p.md",
            &[
                ("a-D1", "new"),
                ("b-D1", "open"),
                ("b-D2", "resolved"),
                ("b-D3", "parked"),
            ],
        );
        let mut refused = reasons(&after, &before);
        refused.sort();
        assert_eq!(
            refused,
            [
                "10: Status: `new`, but a-D1 is in the previous read; carry it as open, resolved or parked",
                "11: Status: `open`, but b-D1 is not in a previous read; a disagreement this read found is new",
                "12: Status: `resolved`, but b-D2 is not in a previous read; a disagreement this read found is new",
                "13: Status: `parked`, but b-D3 is not in a previous read; a disagreement this read found is new",
            ]
        );
    }

    #[test]
    fn a_first_read_may_hold_only_new_rows() {
        let first = read("none", &[("a-D1", "new"), ("a-D2", "open")]);
        assert_eq!(
            reasons(&first, &[]),
            [
                "9: Status: `open`, but a-D2 is not in a previous read; a disagreement this read found is new"
            ]
        );
    }
}
