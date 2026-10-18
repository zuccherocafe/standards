//! What the page says about each side's images and measures, and about measuring code that changed.

use std::fs;

use crate::invocation::PROJECT;
use crate::support::{Fixture, png};

#[test]
fn images_are_compared_by_name_with_every_missing_unexpected_and_unreadable_one_listed() {
    let fixture = Fixture::new("images");
    fixture.write("dandegate.toml", PROJECT);
    let expected = "same.png\nshift.png\nsize.png\nboth-missing.png\ncorrupt.png\n";
    fixture.write("fixture/visual/expected.txt", expected);
    fixture.write_bytes("fixture/visual/images/same.png", &png(1, 1, &[1, 2, 3, 4]));
    fixture.write_bytes(
        "fixture/visual/images/shift.png",
        &png(1, 1, &[10, 20, 30, 255]),
    );
    fixture.write_bytes(
        "fixture/visual/images/size.png",
        &png(1, 1, &[0, 0, 0, 255]),
    );
    fixture.write_bytes(
        "fixture/visual/images/corrupt.png",
        &png(1, 1, &[0, 0, 0, 255]),
    );
    fixture.write("fixture/visual/clips.log", "");
    fixture.write("fixture/notes/notes.md", "# Notes\n");
    fixture.write("fixture/notes/notes.md.status", "complete\n");
    let before = fixture.commit("before");
    fixture.write_bytes(
        "fixture/visual/images/shift.png",
        &png(1, 1, &[13, 20, 27, 255]),
    );
    fixture.write_bytes(
        "fixture/visual/images/size.png",
        &png(2, 1, &[0, 0, 0, 255, 0, 0, 0, 255]),
    );
    fixture.write_bytes("fixture/visual/images/corrupt.png", b"not a picture");
    fixture.write_bytes(
        "fixture/visual/images/surprise.png",
        &png(1, 1, &[0, 0, 0, 255]),
    );
    // Only PNGs are images. Anything else in the folder is not compared.
    fixture.write("fixture/visual/images/notes.txt", "not a cell\n");
    fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    let incomplete = run.printed("DANDEGATE-INCOMPLETE");
    assert_eq!(
        incomplete,
        [
            "before visual: images/both-missing.png is missing",
            "after visual: images/both-missing.png is missing",
            "visual image both-missing.png: missing on the before side",
            "visual image both-missing.png: missing on the after side",
            "visual image corrupt.png: unreadable on the after side: Invalid PNG signature.",
            "visual image surprise.png: not expected on the after side",
        ],
    );
    let captions: Vec<&str> = page
        .split("<figcaption>")
        .skip(1)
        .map(|rest| rest.split("</figcaption>").next().unwrap_or_default())
        .collect();
    assert_eq!(
        captions,
        [
            "<code>both-missing.png</code>: missing on the before side; missing on the after side",
            "<code>corrupt.png</code>: unreadable on the after side: Invalid PNG signature.",
            "<code>surprise.png</code>: not expected on the after side",
            // Different sizes count as the largest difference.
            "<code>size.png</code>: 255.00",
            // |10 - 13| + |20 - 20| + |30 - 27| + |255 - 255| = 6, over four samples.
            "<code>shift.png</code>: 1.50",
            "<code>same.png</code>: 0.00",
        ]
    );
}

const BEFORE_MEASURES: &str = "\
MEASURE: same.value 1 m
MEASURE: moved.small 100 m higher-is-worse
MEASURE: moved.big 1 m lower-is-worse
MEASURE: gone.value 3 s
MEASURE: unit.value 1 m
MEASURE: conflict.value 1 m
MEASURE: zero.value 0 m higher-is-worse
MEASURE: huge.value -1.7e308 m higher-is-worse
MEASURE: borrowed.value 2 m
MEASURE: disagree.value 2 m higher-is-worse
MEASURE: words.value 5 mm before closure
MEASURE: undirected.value 1 m
";

const AFTER_MEASURES: &str = "\
MEASURE: same.value 1 m
MEASURE: moved.small 101 m higher-is-worse
MEASURE: moved.big 2 m lower-is-worse
MEASURE: unit.value 1 mm
MEASURE: conflict.value 1 m
MEASURE: conflict.value 2 m
MEASURE: zero.value 1 m higher-is-worse
MEASURE: huge.value 1.7e308 m higher-is-worse
MEASURE: borrowed.value 3 m higher-is-worse
MEASURE: disagree.value 3 m lower-is-worse
MEASURE: words.value 5 mm before closure
MEASURE: undirected.value 9 m
MEASURE: new.value 4 s
";

#[test]
fn measures_are_compared_by_name_with_checked_arithmetic_and_their_own_directions() {
    let fixture = Fixture::new("measures");
    fixture.write("dandegate.toml", PROJECT);
    fixture.write("fixture/visual/clips.log", BEFORE_MEASURES);
    let before = fixture.commit("before");
    fixture.write("fixture/visual/clips.log", AFTER_MEASURES);
    fixture.commit("after");
    // A revision's own step measures stand for their names over imported ones.
    let gate_log = fixture.outside("gate.log");
    fs::write(
        &gate_log,
        "  MEASURE: same.value 99 m\nnoise\nMEASURE: imported.only 7 kg\n",
    )
    .expect("a log");
    let previous = fixture.outside("previous.log");
    fs::write(
        &previous,
        "MEASURE: same.value 50 m\nMEASURE: imported.only 6 kg\n",
    )
    .expect("a log");

    let run = fixture.run(
        "out",
        &[
            "--before",
            &before,
            "--measures",
            gate_log.to_str().expect("UTF-8"),
            "--previous",
            previous.to_str().expect("UTF-8"),
        ],
    );
    let page = run.page();
    assert_eq!(
        run.printed("DANDEGATE-WORSE"),
        [
            "huge.value -1.7e308 m (higher-is-worse) -> 1.7e308 m (higher-is-worse) (difference too large to \
             represent, relative change not representable, moved, worse (higher-is-worse))",
            "zero.value 0 m (higher-is-worse) -> 1 m (higher-is-worse) (+1, relative change not representable, moved, \
             worse (higher-is-worse))",
            "borrowed.value 2 m -> 3 m (higher-is-worse) (+1, 50.0%, moved, worse (higher-is-worse))",
            "moved.small 100 m (higher-is-worse) -> 101 m (higher-is-worse) (+1, 1.0%, worse (higher-is-worse))",
        ]
    );
    let table = page
        .split("<h2 id=\"measures\">")
        .nth(1)
        .expect("the measures section");
    let rows: Vec<String> = table
        .split("<tr><td><code>")
        .skip(1)
        .map(|row| {
            row.split("</tr>")
                .next()
                .unwrap_or_default()
                .replace("</code>", "")
        })
        .collect();
    assert_eq!(
        rows,
        [
            "borrowed.value</td><td>2 m</td><td>3 m (higher-is-worse)</td><td>+1, 50.0%, moved, worse (higher-is-worse)</td>",
            "conflict.value</td><td>1 m</td><td>1 m | 2 m</td><td>conflict</td>",
            "disagree.value</td><td>2 m (higher-is-worse)</td><td>3 m (lower-is-worse)</td><td>+1, 50.0%, moved, directions disagree</td>",
            "gone.value</td><td>3 s</td><td></td><td>gone</td>",
            "huge.value</td><td>-1.7e308 m (higher-is-worse)</td><td>1.7e308 m (higher-is-worse)</td><td>difference too large to represent, relative change not representable, moved, worse (higher-is-worse)</td>",
            "imported.only</td><td>6 kg</td><td>7 kg</td><td>+1, 16.7%, moved</td>",
            "moved.big</td><td>1 m (lower-is-worse)</td><td>2 m (lower-is-worse)</td><td>+1, 100.0%, moved</td>",
            "new.value</td><td></td><td>4 s</td><td>new</td>",
            "undirected.value</td><td>1 m</td><td>9 m</td><td>+8, 800.0%, moved</td>",
            "unit.value</td><td>1 m</td><td>1 mm</td><td>unit changed</td>",
            "zero.value</td><td>0 m (higher-is-worse)</td><td>1 m (higher-is-worse)</td><td>+1, relative change not representable, moved, worse (higher-is-worse)</td>",
            "moved.small</td><td>100 m (higher-is-worse)</td><td>101 m (higher-is-worse)</td><td>+1, 1.0%, worse (higher-is-worse)</td>",
            "same.value</td><td>1 m</td><td>1 m</td><td></td>",
            "words.value</td><td>5 mm before closure</td><td>5 mm before closure</td><td></td>",
        ]
    );
}

#[test]
fn changed_measuring_code_is_warned_of_beside_the_measures_it_takes() {
    let fixture = Fixture::new("measure-code");
    let rules = "[measures]\ncode = [\"probes/\", \"tools/clips.sh\"]\nexclude = [\"probes/selftest.sh\"]\n\
                 code_measures = [\"clips.\"]\n";
    fixture.write("dandegate.toml", &format!("{rules}{PROJECT}"));
    for file in [
        "probes/measures.sh",
        "probes/selftest.sh",
        "tools/clips.sh",
        "other.sh",
    ] {
        fixture.write(file, "1\n");
    }
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.a 1 m\nMEASURE: body.b 1 m\n",
    );
    let before = fixture.commit("before");
    fixture.write("probes/selftest.sh", "2\n");
    fixture.write("other.sh", "2\n");
    let unwatched = fixture.commit("unwatched");
    for file in ["probes/measures.sh", "tools/clips.sh"] {
        fixture.write(file, "2\n");
    }
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.a 2 m\nMEASURE: body.b 2 m\n",
    );
    fixture.commit("watched");

    let quiet = fixture.run("quiet", &["--before", &before, "--after", &unwatched]);
    assert!(!quiet.page().contains("id=\"measure-code\""));
    assert_eq!(
        quiet.printed("DANDEGATE-MEASURE-CODE"),
        Vec::<String>::new()
    );

    let run = fixture.run("warned", &["--before", &before]);
    let page = run.page();
    assert_eq!(
        run.printed("DANDEGATE-MEASURE-CODE"),
        ["probes/measures.sh", "tools/clips.sh"]
    );
    assert!(
        page.contains(
            "measure can move with the code rather than with what it measures: <code>probes/measures.sh</code>, \
             <code>tools/clips.sh</code>.</p><p>Measures it takes that moved: <code>clips.a</code>.</p>"
        ),
        "{page}"
    );
    let warning = page.find("id=\"measure-code\"").expect("the warning");
    assert!(warning < page.find("id=\"worse\"").expect("the worse section"));
}
