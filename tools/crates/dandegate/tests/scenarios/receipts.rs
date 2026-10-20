//! dandegate uses a kept entry only when it is whole, keeps only complete successes, and loses only reuse when it
//! cannot write the store.

use std::fs;
use std::path::PathBuf;

use crate::selection::started;
use crate::support::{Fixture, png};

const VISUAL: &str = r#"
[[step]]
name = "visual"
reuse = true
run = ["sh", "{adapters}/step.sh", "visual", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]
"#;

/// A change to a kept entry's folder.
type Damage = fn(&PathBuf);

/// Every kept entry's folder, staging folders aside.
fn entries(fixture: &Fixture) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(fixture.results())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_missing_changed_extra_malformed_or_old_format_entry_is_a_miss() {
    let fixture = Fixture::new("damaged");
    fixture.write("dandegate.toml", VISUAL);
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m\n",
    );
    let only = fixture.commit("before");
    fixture.commit("after");
    fixture.run("first", &["--before", &only]).page();
    assert_eq!(entries(&fixture).len(), 2);

    let damages: [(&str, Damage, &str); 5] = [
        (
            "missing",
            |entry| fs::remove_file(entry.join("files/log.txt")).expect("a kept file"),
            "its files are not the ones its receipt lists",
        ),
        (
            "changed",
            |entry| {
                fs::write(
                    entry.join("files/out/clips.log"),
                    "MEASURE: clips.walk.twitch 9 m\n",
                )
                .expect("a kept file");
            },
            "its files are not the ones its receipt lists",
        ),
        (
            "extra",
            |entry| fs::write(entry.join("files/out/extra.txt"), "more").expect("a new file"),
            "its files are not the ones its receipt lists",
        ),
        (
            "malformed",
            |entry| fs::write(entry.join("receipt.json"), "{").expect("the receipt"),
            "its receipt is not one",
        ),
        (
            "old",
            |entry| {
                let receipt = fs::read_to_string(entry.join("receipt.json")).expect("the receipt");
                fs::write(
                    entry.join("receipt.json"),
                    receipt.replace("\"format\": 1,", "\"format\": 0,"),
                )
                .expect("the receipt");
            },
            "its receipt is in format 0, not 1",
        ),
    ];
    for (name, damage, why) in damages {
        for entry in entries(&fixture) {
            damage(&entry);
        }
        fixture.clear_events();
        let run = fixture.run(name, &["--before", &only]);
        let page = run.page();
        assert_eq!(started(&fixture).len(), 2, "{name}");
        assert!(
            page.contains(&format!(
                "<td>after</td><td>visual</td><td>runs fresh: {why}"
            )),
            "{name}: {page}"
        );
        assert_eq!(
            run.file("steps/after/visual/out/clips.log"),
            "MEASURE: clips.walk.twitch 0.5 m\n"
        );
    }
}

#[test]
fn only_complete_successes_are_kept() {
    let fixture = Fixture::new("complete");
    let plan = r#"
[[step]]
name = "good"
reuse = true
run = ["sh", "{adapters}/step.sh", "good", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]

[[step]]
name = "failing"
reuse = true
run = ["sh", "{adapters}/step.sh", "failing", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]

[[step]]
name = "partial"
reuse = true
run = ["sh", "{adapters}/step.sh", "partial", "{tree}", "{commit}", "{out}"]
output = [{ kind = "section", file = "{out}/report.md" }]

[[step]]
name = "malformed"
reuse = true
run = ["sh", "{adapters}/step.sh", "malformed", "{tree}", "{commit}", "{out}"]
output = [{ kind = "measures", from = "{out}/clips.log" }]

[[step]]
name = "unrendered"
reuse = true
run = ["sh", "{adapters}/step.sh", "unrendered", "{tree}", "{commit}", "{out}"]
output = [{ kind = "images", folder = "{out}/images", expected = "{out}/expected.txt" }]

[[step]]
name = "corrupt"
reuse = true
run = ["sh", "{adapters}/step.sh", "corrupt", "{tree}", "{commit}", "{out}"]
output = [{ kind = "images", folder = "{out}/images", expected = "{out}/expected.txt" }]
"#;
    fixture.write("dandegate.toml", plan);
    fixture.write("fixture/good/clips.log", "MEASURE: good 1 m\n");
    fixture.write("fixture/failing/clips.log", "MEASURE: failing 1 m\n");
    fixture.write("fixture/failing.fail", "");
    fixture.write("fixture/partial/report.md", "# Partial\n");
    fixture.write(
        "fixture/partial/report.md.status",
        "incomplete: one of two parts\n",
    );
    fixture.write("fixture/malformed/clips.log", "MEASURE: malformed one m\n");
    fixture.write("fixture/unrendered/expected.txt", "a.png\nb.png\n");
    fixture.write_bytes(
        "fixture/unrendered/images/a.png",
        &png(1, 1, &[0, 0, 0, 255]),
    );
    fixture.write("fixture/corrupt/expected.txt", "a.png\n");
    fixture.write("fixture/corrupt/images/a.png", "not a picture\n");
    let only = fixture.commit("before");
    fixture.commit("after");

    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    assert_eq!(entries(&fixture).len(), 2, "only good's two sides are kept");
    for name in ["failing", "partial", "malformed", "unrendered", "corrupt"] {
        assert!(
            page.contains(&format!(
                "<td>after</td><td>{name}</td><td>runs fresh: nothing is kept for it; not kept: only a complete success \
                 is kept</td>"
            )),
            "{name}: {page}"
        );
    }
    fixture.clear_events();
    fixture.run("again", &["--before", &only]).page();
    let mut ran: Vec<String> = started(&fixture)
        .into_iter()
        .map(|event| event.split(' ').next().unwrap_or_default().to_owned())
        .collect();
    ran.sort();
    ran.dedup();
    assert_eq!(
        ran,
        ["corrupt", "failing", "malformed", "partial", "unrendered"]
    );
}

#[test]
fn a_store_that_cannot_be_written_costs_only_its_reuse() {
    let fixture = Fixture::new("unwritable");
    fixture.write("dandegate.toml", VISUAL);
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m\n",
    );
    let only = fixture.commit("only");
    fs::create_dir_all(fixture.repo().join(".cache/dandegate")).expect("a folder");
    fs::write(fixture.results(), "a file where the store's folder goes").expect("a file");

    let run = fixture.run("out", &["--before", &only]);
    let page = run.page();
    assert!(page.contains("<td>after</td><td>visual</td><td>runs fresh: nothing is kept for it; not kept: could not keep"), "{page}");
    assert!(
        page.contains("(os error"),
        "the page gives the store's own error: {page}"
    );
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    assert!(page.contains("<td>0.5 m</td><td>0.5 m</td>"), "{page}");
}

#[test]
fn a_staging_folder_is_never_read_and_none_is_left_behind() {
    let fixture = Fixture::new("staging");
    fixture.write("dandegate.toml", VISUAL);
    fixture.write(
        "fixture/visual/clips.log",
        "MEASURE: clips.walk.twitch 0.5 m\n",
    );
    let only = fixture.commit("before");
    fixture.commit("after");
    fixture.run("first", &["--before", &only]).page();
    let mut staged = Vec::new();
    for entry in entries(&fixture) {
        let name = entry
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        let staging = fixture.results().join(format!(".{name}-interrupted"));
        fs::rename(&entry, &staging).expect("a rename");
        staged.push(staging);
    }

    fixture.clear_events();
    fixture.run("second", &["--before", &only]).page();
    assert_eq!(
        started(&fixture).len(),
        2,
        "a staging folder is not an entry"
    );
    let mut dotted: Vec<PathBuf> = fs::read_dir(fixture.results())
        .expect("the store")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    dotted.sort();
    staged.sort();
    assert_eq!(dotted, staged, "the run left no staging folder of its own");
}
