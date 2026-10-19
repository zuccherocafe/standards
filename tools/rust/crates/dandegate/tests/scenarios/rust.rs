//! A Rust workspace's built-in reports: scans, line counts, the mutation report and the optional API diff. The report
//! tools are stand-ins that record their arguments. `cargo metadata` and the toolchain identity use the real Cargo.

use crate::selection::started;
use crate::support::Fixture;

/// Prints the checkout's `fixture/scans.txt`, failing when it has `fixture/scans.fail`.
const SCANS: &str = r#"#!/bin/sh
printf 'scans %s\n' "$*" >> "$EVENTS/events"
cat fixture/scans.txt
[ ! -f fixture/scans.fail ]
"#;

const MUTANTS: &str = r#"#!/bin/sh
printf 'mutants %s\n' "$*" >> "$EVENTS/events"
echo "missed, to judge: crates/alpha/src/lib.rs:2:5: replace alpha -> u8 with 0"
echo "caught: 3 missed: 1 timeout: 0 unviable: 0"
"#;

/// The real Cargo, except:
///
/// - `cargo rustdoc` writes the checkout's `fixture/api/<package>.txt` as the package's rustdoc JSON, or what the
///   program `RUSTDOC` names prints, and fails when there is `fixture/api/<package>.fail`.
/// - `cargo metadata` fails when there is `fixture/metadata.fail`.
/// - `cargo config` fails with 7 when `CARGO_CONFIG_FAILS` is set.
const CARGO: &str = r#"#!/bin/sh
if [ "$1" = metadata ] && [ -f fixture/metadata.fail ]; then echo "no metadata" >&2; exit 1; fi
if [ "$2" = config ] && [ -n "$CARGO_CONFIG_FAILS" ]; then exit 7; fi
if [ "$1" != rustdoc ]; then exec "$CARGO" "$@"; fi
package=$(printf '%s\n' "$@" | sed -n '/^-p$/{n;p;}')
printf 'rustdoc %s %s\n' "$package" "$CARGO_TARGET_DIR" >> "$EVENTS/events"
[ -f "fixture/api/$package.fail" ] && exit 1
mkdir -p "$CARGO_TARGET_DIR/doc"
document="$CARGO_TARGET_DIR/doc/$(echo "$package" | tr - _).json"
if [ -n "$RUSTDOC" ]; then "$RUSTDOC" > "$document"; else cp "fixture/api/$package.txt" "$document"; fi
"#;

/// Lists the items the fake rustdoc JSON holds, and diffs two of them by line.
const PUBLIC_API: &str = r#"#!/bin/sh
printf 'public-api %s\n' "$*" >> "$EVENTS/events"
[ "$1" = -ss ] || exit 9
if [ "$4" = --rustdoc-json ]; then cat "$5"; exit 0; fi
[ "$4" = diff ] || exit 8
echo "Removed items from the public API"
echo "================================="
grep -vxF -f "$6" "$5" | sed 's/^/-/'
echo
echo "Changed items in the public API"
echo "==============================="
echo "(none)"
echo
echo "Added items to the public API"
echo "============================="
grep -vxF -f "$5" "$6" | sed 's/^/+/'
exit 0
"#;

const ALPHA_BEFORE: &str = "pub fn alpha() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn one() {}\n}\n";

pub(crate) fn workspace(fixture: &Fixture) {
    fixture.tool("rust-scans", SCANS);
    fixture.tool("rust-mutants", MUTANTS);
    fixture.tool("cargo", CARGO);
    fixture.tool("cargo-public-api", PUBLIC_API);
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
    );
    for (name, kind) in [("alpha", "lib"), ("beta", "lib"), ("tool", "bin")] {
        fixture.write(
            &format!("crates/{name}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        );
        let file = if kind == "lib" { "lib.rs" } else { "main.rs" };
        let text = if name == "alpha" {
            ALPHA_BEFORE
        } else if kind == "lib" {
            // A comment holding a character of several bytes, which the line reader steps over whole.
            "/* café */\npub fn f() {}\n"
        } else {
            "fn main() {}\n"
        };
        fixture.write(&format!("crates/{name}/src/{file}"), text);
    }
    fixture.write(
        "crates/alpha/tests/scenarios.rs",
        "#[test]\nfn works() {}\n",
    );
    fixture.write("crates/alpha/benches/speed.rs", "fn main() {}\n");
    fixture.write(
        "dandegate.toml",
        "[rust]\nscans = [\"--core\", \"alpha\"]\n",
    );
    fixture.write(
        "fixture/scans.txt",
        "H1 crates/alpha/src/lib.rs:1: std::fs\nH1: 1\nH2: 0\nseconds: 0.100\n",
    );
    fixture.write(
        "fixture/api/alpha.txt",
        "pub fn alpha::alpha() -> u8\npub fn alpha::old()\n",
    );
    fixture.write("fixture/api/beta.txt", "pub fn beta::f()\n");
}

/// A library crate called `name` whose listing is `items`.
fn library(fixture: &Fixture, name: &str, items: &str) {
    fixture.write(
        &format!("crates/{name}/Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    );
    fixture.write(&format!("crates/{name}/src/lib.rs"), "pub fn f() {}\n");
    fixture.write(&format!("fixture/api/{name}.txt"), items);
}

/// The after revision: alpha gains a proof module, which is test code, and its scans and API change.
fn change_rust(fixture: &Fixture) {
    fixture.write(
        "crates/alpha/src/lib.rs",
        &format!("{ALPHA_BEFORE}#[cfg(kani)]\nmod proofs;\n"),
    );
    fixture.write(
        "crates/alpha/src/proofs.rs",
        "#[kani::proof]\nfn proof() {\n}\n",
    );
    fixture.write(
        "fixture/scans.txt",
        "H2 crates/alpha/src/lib.rs:3: reach_metres: f64\nH1: 0\nH2: 1\nseconds: 0.100\n",
    );
    fixture.write(
        "fixture/api/alpha.txt",
        "pub fn alpha::alpha() -> u8\npub fn alpha::new()\n",
    );
}

/// The section a step wrote, from the page.
fn section(page: &str, title: &str) -> String {
    let start = page
        .find(&format!("<h3>{title}</h3>"))
        .expect("the page has the step's section");
    let rest = &page[start..];
    let pre = rest.find("<pre>").expect("the section's text") + 5;
    rest[pre..rest.find("</pre>").expect("its end")]
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

#[test]
fn a_root_level_rust_workspace_gets_scans_line_counts_and_the_mutation_report() {
    let fixture = Fixture::new("rust");
    workspace(&fixture);
    let before = fixture.commit("before");
    change_rust(&fixture);
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());

    let scans = section(&page, "scans (both)");
    assert!(
        scans.contains("| H1 | 1 | 0 |\n| H2 | 0 | 1 |\n"),
        "{scans}"
    );
    assert!(
        scans.contains("```text\nH2 crates/alpha/src/lib.rs:3: reach_metres: f64\n```"),
        "{scans}"
    );
    assert!(
        run.file("steps/both/scans/out/before.txt")
            .starts_with("H1 crates/alpha")
    );
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event.starts_with("scans "))
            .collect::<Vec<_>>(),
        [
            "scans --workspace . --core alpha",
            "scans --workspace . --core alpha"
        ]
    );

    // alpha's lib.rs: four production lines, the blank one included, and the five of its tests module. Then its
    // test and bench files, and after, the two lines declaring its proofs and the proofs' own three.
    let lines = section(&page, "lines (both)");
    for row in [
        "| alpha | 4 → 4 | 8 → 13 (+5) |",
        "| beta | 2 → 2 | 0 → 0 |",
        "| tool | 1 → 1 | 0 → 0 |",
        "| total | 7 → 7 | 8 → 13 (+5) |",
    ] {
        assert!(lines.contains(row), "{row} in {lines}");
    }

    let out = run.out.join("steps/both/mutants/out/run");
    assert!(fixture.events().contains(&format!(
        "mutants --workspace . --base {before} --out {} --",
        out.display()
    )));
    assert!(
        section(&page, "mutants (both)").contains("missed, to judge: crates/alpha/src/lib.rs:2:5")
    );
    for optional in ["api", "api-listings"] {
        assert!(
            page.contains(&format!("<td>{optional}</td><td>not selected</td>")),
            "{optional}"
        );
    }
    assert_ne!(before, after);
}

#[test]
fn the_mutation_report_is_not_run_when_no_rust_differs() {
    let fixture = Fixture::new("no-rust");
    workspace(&fixture);
    // A Rust workspace needs no steps file to get its reports.
    fixture.remove("dandegate.toml");
    let before = fixture.commit("before");
    fixture.write("README", "words only\n");
    fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    assert!(page.contains(
        "<td>both</td><td>mutants</td><td>not run: no Rust differs between the revisions</td>"
    ));
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    assert!(
        !fixture
            .events()
            .iter()
            .any(|event| event.starts_with("mutants"))
    );
}

#[test]
fn scans_that_fail_on_one_side_leave_a_partial_section_and_fail_nothing() {
    let fixture = Fixture::new("scans");
    workspace(&fixture);
    fixture.write("fixture/scans.fail", "");
    let before = fixture.commit("before");
    fixture.remove("fixture/scans.fail");
    change_rust(&fixture);
    fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    assert_eq!(
        run.printed("DANDEGATE-INCOMPLETE"),
        [
            "both scans: scans.md is incomplete: the before side's scans did not finish: `rust-scans --workspace . \
             --core alpha` failed: exit status: 1"
        ]
    );
    let scans = section(&page, "scans (both)");
    assert!(
        scans.contains("```text\nH2 crates/alpha/src/lib.rs:3: reach_metres: f64\n```"),
        "{scans}"
    );
    assert!(
        run.exists("steps/both/scans/out/before.txt")
            && run.exists("steps/both/scans/out/after.txt")
    );
}

#[test]
fn the_api_diff_is_optional_with_a_target_per_side_and_never_calls_a_failed_listing_removed() {
    let fixture = Fixture::new("api");
    workspace(&fixture);
    fixture.write("fixture/api/beta.fail", "");
    // delta is a library only the before revision has, gamma one only the after revision has.
    library(&fixture, "delta", "pub fn delta::d()\n");
    let before = fixture.commit("before");
    fixture.remove("fixture/api/beta.fail");
    for file in [
        "crates/delta/Cargo.toml",
        "crates/delta/src/lib.rs",
        "fixture/api/delta.txt",
    ] {
        fixture.remove(file);
    }
    library(&fixture, "gamma", "pub fn gamma::g()\n");
    change_rust(&fixture);
    let after = fixture.commit("after");

    let run = fixture.run("out", &["--before", &before, "--with", "api"]);
    let page = run.page();
    let events = fixture.events();
    let rustdoc: Vec<Vec<&str>> = events
        .iter()
        .filter_map(|event| event.strip_prefix("rustdoc "))
        .map(|event| event.split(' ').collect())
        .collect();
    let mut packages: Vec<&str> = rustdoc.iter().map(|event| event[0]).collect();
    packages.sort_unstable();
    assert_eq!(
        packages,
        ["alpha", "alpha", "beta", "beta", "delta", "gamma"],
        "library crates only, on the sides that have them"
    );
    let mut targets: Vec<&str> = rustdoc.iter().map(|event| event[1]).collect();
    targets.sort_unstable();
    targets.dedup();
    assert_eq!(
        targets.len(),
        2,
        "a target folder for each side: {targets:?}"
    );
    assert!(
        events
            .iter()
            .filter(|event| event.starts_with("public-api "))
            .all(|event| event.starts_with("public-api -ss "))
    );

    let api = section(&page, "api (both)");
    assert!(
        api.contains("- beta: not listed on the before side; see its api-listings step"),
        "{api}"
    );
    assert!(api.contains("| beta | - | 1 | - | - | - |"), "{api}");
    assert!(
        !api.contains("pub fn beta::f()"),
        "a crate that failed to list is not removed: {api}"
    );
    assert!(api.contains("| alpha | 2 | 2 | 1 | 0 | 1 |"), "{api}");
    assert!(api.contains("```diff\n- pub fn alpha::old()\n```"), "{api}");
    assert!(api.contains("```diff\n+ pub fn alpha::new()\n```"), "{api}");
    assert!(api.contains("| delta | 1 | - | 1 | 0 | 0 |"), "{api}");
    assert!(api.contains("```diff\n- pub fn delta::d()\n```"), "{api}");
    assert!(api.contains("| gamma | - | 1 | 0 | 0 | 1 |"), "{api}");
    assert!(api.contains("```diff\n+ pub fn gamma::g()\n```"), "{api}");
    let incomplete = run.printed("DANDEGATE-INCOMPLETE");
    assert!(incomplete.contains(&"both api: api.md is incomplete: beta: not listed on the before side; see its api-listings step".to_owned()), "{incomplete:?}");

    fixture.clear_events();
    let again = fixture.run("again", &["--before", &before, "--with", "api"]);
    again.page();
    assert!(
        again
            .printed("DANDEGATE-REUSED")
            .iter()
            .any(|line| line.starts_with("after api-listings"))
    );
    // The before side's listing was partial, so it was not kept and runs again. The after side's is reused.
    let rustdoc = fixture
        .events()
        .iter()
        .filter(|event| event.starts_with("rustdoc "))
        .count();
    assert_eq!(rustdoc, 3, "{:?}", fixture.events());
    assert_eq!(started(&fixture), Vec::<String>::new());

    a_side_whose_crates_cannot_be_found_has_none_removed(&fixture, &after);
}

/// The API scenario's last report: the after side's `cargo metadata` fails, so its crates are unknown.
fn a_side_whose_crates_cannot_be_found_has_none_removed(fixture: &Fixture, after: &str) {
    fixture.write("fixture/metadata.fail", "");
    let unlisted = fixture.commit("unlisted");
    let failed = fixture.run(
        "unlisted",
        &["--before", after, "--after", &unlisted, "--with", "api"],
    );
    let api = section(&failed.page(), "api (both)");
    assert!(
        api.contains(
            "- the after side: its crates could not be listed: `cargo metadata --no-deps --format-version 1 \
             --offline` failed: exit status: 1"
        ),
        "{api}"
    );
    assert!(
        api.contains("- alpha: not compared: the after side's crates are unknown"),
        "{api}"
    );
    assert!(api.contains("| alpha | 2 | - | - | - | - |"), "{api}");
    assert!(!api.contains("```diff"), "nothing is called removed: {api}");
    assert!(
        failed
            .file("steps/both/api/out/api.md.status")
            .starts_with("incomplete: "),
    );
    assert!(
        failed
            .file("steps/after/api-listings/out/listings.json")
            .contains("\"unknown\""),
    );
}

/// What a step's identity note says, from the report's log.
fn noted(run: &crate::support::Run, instance: &str) -> Vec<String> {
    let lead = format!("=== {instance}: ");
    run.file("report.log")
        .lines()
        .filter_map(|line| line.strip_prefix(&lead))
        .map(str::to_owned)
        .collect()
}

#[test]
fn listings_miss_when_the_rustdoc_cargo_runs_changes_at_the_same_path() {
    let fixture = Fixture::new("rustdoc");
    workspace(&fixture);
    let only = fixture.commit("only");
    let rustdoc = fixture.outside("rustdoc");
    let program = |item: &str| {
        std::fs::write(
            &rustdoc,
            format!("#!/bin/sh\necho 'pub fn toy::{item}()'\n"),
        )
        .expect("a program");
        std::fs::set_permissions(
            &rustdoc,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .expect("executable");
    };
    let rustdoc_path = rustdoc.to_str().expect("UTF-8");
    let report = |out: &str| {
        let run = fixture.run_with(
            out,
            &["--before", &only, "--with", "api"],
            &[("RUSTDOC", rustdoc_path)],
        );
        run.page();
        run
    };
    program("one");
    report("first");
    let same = report("same");
    assert!(
        same.printed("DANDEGATE-REUSED")
            .iter()
            .any(|line| line.starts_with("after api-listings")),
        "{:?}",
        same.printed("DANDEGATE-REUSED")
    );
    program("two");
    let changed = report("changed");
    assert!(
        !changed
            .printed("DANDEGATE-REUSED")
            .iter()
            .any(|line| line.contains("api-listings")),
        "{:?}",
        changed.printed("DANDEGATE-REUSED")
    );
    assert_eq!(
        changed.file("steps/after/api-listings/out/alpha.txt"),
        "pub fn toy::two()\n"
    );
}

#[test]
fn a_toolchain_that_cannot_be_identified_runs_the_listings_fresh_and_keeps_nothing() {
    for (case, env) in [
        ("configuration", ("CARGO_CONFIG_FAILS", "1")),
        ("wrapper", ("RUSTC_WRAPPER", "/nowhere/wrapper")),
    ] {
        let fixture = Fixture::new(case);
        workspace(&fixture);
        let only = fixture.commit("only");
        for out in ["first", "second"] {
            let run = fixture.run_with(out, &["--before", &only, "--with", "api"], &[env]);
            run.page();
            assert_eq!(
                run.printed("DANDEGATE-REUSED"),
                Vec::<String>::new(),
                "{case}"
            );
            let notes = noted(&run, "after api-listings");
            assert!(
                notes.iter().any(|note| note.starts_with(
                    "runs fresh and is not kept: its identity collector failed: exit status: "
                )),
                "{case}: {notes:?}"
            );
            assert!(
                !notes.iter().any(|note| note == "kept for reuse"),
                "{case}: {notes:?}"
            );
        }
    }
}

#[test]
fn a_workspace_in_a_folder_is_found_and_reported_there() {
    let fixture = Fixture::new("folder");
    fixture.tool("rust-scans", SCANS);
    fixture.tool("rust-mutants", MUTANTS);
    fixture.write(
        "rust/Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
    );
    fixture.write(
        "rust/crates/alpha/Cargo.toml",
        "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    fixture.write("rust/crates/alpha/src/lib.rs", "pub fn f() {}\n");
    fixture.write("fixture/scans.txt", "H1: 0\n");
    let before = fixture.commit("before");
    fixture.write(
        "rust/crates/alpha/src/lib.rs",
        "pub fn f() {}\npub fn g() {}\n",
    );
    fixture.commit("after");

    let run = fixture.run("out", &["--before", &before]);
    let page = run.page();
    assert_eq!(run.printed("DANDEGATE-INCOMPLETE"), Vec::<String>::new());
    assert!(
        fixture
            .events()
            .contains(&"scans --workspace rust".to_owned()),
        "{:?}",
        fixture.events()
    );
    assert!(section(&page, "lines (both)").contains("| alpha | 1 → 2 (+1) | 0 → 0 |"));
}
