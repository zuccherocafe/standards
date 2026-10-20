//! A page stands alone once the cache and scratch are gone, says when a section is partial, and saves measures that a
//! later report reads back. dandegate also reads the shapes earlier reports saved.

use std::fs;

use crate::invocation::PROJECT;
use crate::support::{Fixture, png};

/// Every `href` and `src` the page links.
fn links(page: &str) -> Vec<String> {
    page.split(['"'])
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|pair| pair[0].ends_with("href=") || pair[0].ends_with("src="))
        .map(|pair| pair[1].replace("&amp;", "&"))
        .collect()
}

#[test]
fn fresh_and_reused_pages_stand_alone_once_the_cache_and_scratch_are_gone() {
    let fixture = Fixture::new("stand-alone");
    fixture.write(
        "dandegate.toml",
        &PROJECT.replace("name = \"visual\"", "name = \"visual\"\nreuse = true"),
    );
    fixture.write("fixture/visual/expected.txt", "cell.png\n");
    fixture.write_bytes(
        "fixture/visual/images/cell.png",
        &png(1, 1, &[1, 2, 3, 255]),
    );
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 1 m\n",
    );
    fixture.write("fixture/notes/notes.md", "# Notes\n");
    fixture.write("fixture/notes/notes.md.status", "complete\n");
    let before = fixture.commit("before");
    fixture.commit("after");
    let fresh = fixture.run("fresh", &["--before", &before]);
    fresh.page();
    let reused = fixture.run("reused", &["--before", &before]);
    assert_eq!(reused.printed("DANDEGATE-REUSED").len(), 2);
    reused.page();
    fs::remove_dir_all(fixture.repo().join(".cache")).expect("the cache");

    for run in [&fresh, &reused] {
        let page = fs::read_to_string(run.out.join("index.html")).expect("the page");
        let linked = links(&page);
        for expected in [
            "steps/before/visual/out/images/cell.png",
            "steps/after/visual/out/images/cell.png",
            "steps/both/notes/out/notes.md",
            "steps/before/visual/log.txt",
            "steps/after/visual/log.txt",
            "steps/both/notes/log.txt",
        ] {
            assert!(
                linked.iter().any(|link| link == expected),
                "{expected} in {linked:?}"
            );
        }
        for link in &linked {
            assert!(
                !link.starts_with('/') && !link.contains(".."),
                "{link} leaves the output folder"
            );
            assert!(
                run.out.join(link).is_file(),
                "{link} is not in {}",
                run.out.display()
            );
        }
        assert!(run.exists("report.log") && run.exists("measures.json"));
    }
}

#[test]
fn a_missing_or_malformed_section_status_makes_the_page_incomplete() {
    let fixture = Fixture::new("status");
    let plan = r#"
[[steps.step]]
name = "notes"
runs = "once"
run = ["sh", "{adapters}/step.sh", "notes", "{after}", "{after-commit}", "{out}"]
output = [{ kind = "section", file = "{out}/notes.md" }]
"#;
    fixture.write("dandegate.toml", plan);
    fixture.write("fixture/notes/notes.md", "# Notes\n");
    let base = fixture.commit("base");
    for (status, expected) in [
        (None, "both notes: notes.md.status is missing"),
        (
            Some("done\n"),
            "both notes: notes.md.status cannot be read: a status is `complete` or `incomplete: <why>` on one line",
        ),
        (
            Some("incomplete: one part of two\n"),
            "both notes: notes.md is incomplete: one part of two",
        ),
    ] {
        if let Some(text) = status {
            fixture.write("fixture/notes/notes.md.status", text);
        }
        let after = fixture.commit("status");
        let run = fixture.run(
            &format!("out-{}", after.get(..8).unwrap_or_default()),
            &["--before", &base, "--after", &after],
        );
        let page = run.page();
        assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), [expected]);
        assert!(page.contains("id=\"incomplete\""));
    }
}

#[test]
fn saved_measures_round_trip_their_directions() {
    let fixture = Fixture::new("round-trip");
    let plan = r#"
[[steps.step]]
name = "clips"
runs = "after"
run = ["sh", "{adapters}/step.sh", "clips", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]
"#;
    fixture.write("dandegate.toml", plan);
    fixture.write(
        "fixture/clips/clips.log",
        "MEASURE: walk.twitch 1 m higher-is-worse\nMEASURE: walk.reach 2 mm before closure lower-is-worse\n\
         MEASURE: walk.turn 3 deg\n",
    );
    let first = fixture.commit("first");
    let saved = fixture.run("first", &["--before", &first]);
    saved.page();
    assert_eq!(
        saved.file("measures.json"),
        r#"{
  "format": "dandegate-measures-v1",
  "measures": {
    "walk.reach": [
      {
        "value": 2.0,
        "unit": "mm before closure",
        "worse": "lower"
      }
    ],
    "walk.turn": [
      {
        "value": 3.0,
        "unit": "deg"
      }
    ],
    "walk.twitch": [
      {
        "value": 1.0,
        "unit": "m",
        "worse": "higher"
      }
    ]
  }
}
"#
    );

    // The new revision's lines carry no direction, so each borrows the saved one.
    fixture.write(
        "fixture/clips/clips.log",
        "MEASURE: walk.twitch 2 m\nMEASURE: walk.reach 1 mm before closure\nMEASURE: walk.turn 4 deg\n",
    );
    fixture.commit("second");
    let previous = saved.out.to_str().expect("UTF-8").to_owned();
    let run = fixture.run("second", &["--before", &first, "--previous", &previous]);
    run.page();
    assert_eq!(
        run.printed("DANDEGATE-WORSE"),
        [
            "walk.twitch 1 m (higher-is-worse) -> 2 m (+1, 100.0%, moved, worse (higher-is-worse))",
            "walk.reach 2 mm before closure (lower-is-worse) -> 1 mm before closure (-1, 50.0%, moved, worse \
             (lower-is-worse))",
        ]
    );
}

#[test]
fn an_earlier_reports_measures_are_read_in_each_shape_it_saved() {
    let fixture = Fixture::new("earlier");
    let plan = r#"
[[steps.step]]
name = "clips"
runs = "after"
run = ["sh", "{adapters}/step.sh", "clips", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]
"#;
    fixture.write("dandegate.toml", plan);
    fixture.write(
        "fixture/clips/clips.log",
        "MEASURE: a 1 m\nMEASURE: b 2 m\nMEASURE: c 4 m\nMEASURE: d 5 m\n",
    );
    let only = fixture.commit("only");
    // Today's list of [value, unit] pairs, and the two shapes before it: [value, unit], and [[values], unit] for a
    // conflict.
    let earlier = fixture.outside("earlier");
    fs::create_dir(&earlier).expect("a folder");
    fs::write(
        earlier.join("measures.json"),
        r#"{"a": [[1.0, "m"]], "b": [[2.0, "m"], [3.0, "m"]], "c": [4.0, "m"], "d": [[5.0, 6.0], "m"]}"#,
    )
    .expect("a record");
    let run = fixture.run(
        "out",
        &[
            "--before",
            &only,
            "--previous",
            earlier.to_str().expect("UTF-8"),
        ],
    );
    let page = run.page();
    for row in [
        "<td><code>a</code></td><td>1 m</td><td>1 m</td><td></td>",
        "<td><code>b</code></td><td>2 m | 3 m</td><td>2 m</td><td>conflict</td>",
        "<td><code>c</code></td><td>4 m</td><td>4 m</td><td></td>",
        "<td><code>d</code></td><td>5 m | 6 m</td><td>5 m</td><td>conflict</td>",
    ] {
        assert!(page.contains(row), "{row}: {page}");
    }

    fs::write(earlier.join("measures.json"), r#"{"a": "one metre"}"#).expect("a record");
    let refused = fixture.run(
        "refused",
        &[
            "--before",
            &only,
            "--previous",
            earlier.to_str().expect("UTF-8"),
        ],
    );
    assert_eq!(refused.status(), 1);
    assert!(
        refused.stderr().contains("is not a saved measures record"),
        "{}",
        refused.stderr()
    );
    assert!(
        refused
            .stderr()
            .contains("caused by: a: is not a list of observations"),
        "{}",
        refused.stderr()
    );
    assert!(!refused.out.exists());
}
