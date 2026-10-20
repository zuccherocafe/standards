//! A Godot project's GDScript scans: the after revision's scanner reads both checkouts, and the section lists what the
//! change added and removed. The scanner is a stand-in that prints the project's `scans.jsonl` and records what it
//! was given.

use std::fs;

use crate::rust::section;
use crate::support::Fixture;

/// Records the commit of the project it reads and whether it was given a test folder, then prints the project's
/// `scans.jsonl`.
const SCANNER: &str = r#"#!/bin/sh
project=$2
tests=none
if [ "$3" = --tests ]; then tests=${4#"$project"/}; fi
printf 'gdscript-scans %s tests=%s\n' "$(git -C "$project" rev-parse HEAD)" "$tests" >> "$EVENTS/events"
cat "$project/scans.jsonl"
"#;

fn finding(scan: &str, file: &str, symbol: &str, line: u32) -> String {
    format!(
        "{{\"scan\":\"{scan}\",\"file\":\"{file}\",\"symbol\":\"{symbol}\",\"line\":{line},\"detail\":\"seen\"}}\n"
    )
}

#[test]
fn the_after_scanner_reads_both_sides_and_the_section_lists_what_the_change_added_and_removed() {
    let fixture = Fixture::new("godot");
    fixture.tool("gdscript-scans", SCANNER);
    fixture.write("godot/project.godot", "");
    fixture.write("godot/a.gd", "");
    fixture.write(
        "dandegate.toml",
        "[project.godot]\nproject = \"godot/\"\ntests = [\"godot/tests/\"]\n",
    );
    // The test folder exists only after, so the plan's check of the after revision passes and the before side reads
    // without it.
    fixture.write(
        "godot/scans.jsonl",
        &[
            finding("bare-type", "a.gd", "A.f", 3),
            finding("bare-type", "gone.gd", "Gone.flag", 4),
            finding("bare-type", "b.gd", "B.g", 5),
            finding("bare-type", "c.gd", "C.h", 6),
            finding("unused", "a.gd", "A.reset", 7),
        ]
        .concat(),
    );
    let before = fixture.commit("before");
    fixture.write("godot/tests/check.gd", "");
    // c.gd becomes unknown, so the unused finding is not compared and A.reset is not called removed.
    fixture.write(
        "godot/scans.jsonl",
        &[
            finding("bare-type", "a.gd", "A.f", 9),
            finding("bare-type", "b.gd", "B.new", 2),
            "{\"unknown\":\"c.gd\",\"reason\":\"the grammar cannot parse it at line 1\"}\n"
                .to_owned(),
        ]
        .concat(),
    );
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    let scans = section(&page, "gdscript-scans (both)");
    for part in [
        "The unused and write-only scans are not compared: each rests on every file's references, and `c.gd` is \
         unknown on a side. Only bare-type findings are compared, file by file.\n",
        "## Added (1)\n\n- bare-type `b.gd:2` B.new: seen\n",
        "## Removed (2)\n\n- bare-type `b.gd:5` B.g: seen\n- bare-type `gone.gd:4` Gone.flag: seen\n",
        "## Still present\n\n1 finding.\n",
        "The before side: none.\n",
        "The after side: 1 file.\n\n- `c.gd`: the grammar cannot parse it at line 1\n",
        "Not compared: `c.gd`.\n",
    ] {
        assert!(scans.contains(part), "{part} in {scans}");
    }
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event.starts_with("gdscript-scans "))
            .collect::<Vec<_>>(),
        [
            &format!("gdscript-scans {before} tests=none"),
            &format!("gdscript-scans {after} tests=tests"),
        ]
    );
    // Both launches entered the after checkout's shell, though the scanner read the before checkout once.
    let shells =
        fs::read_to_string(fixture.events_folder().join("nix")).expect("the stand-in nix's record");
    let shells: Vec<&str> = shells.lines().collect();
    assert_eq!(shells.len(), 2, "{shells:?}");
    assert!(
        shells.iter().all(|checkout| checkout.ends_with("/after")),
        "{shells:?}"
    );
    assert!(run.exists("steps/both/gdscript-scans/out/before.jsonl"));
}
