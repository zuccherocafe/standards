//! The three scans over a project's declarations and references, and the records they print. Functions over values:
//! [`crate::project`] reads the files.

use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::declarations::{Declaration, Kind, Unparsed, declarations};
use crate::lexer::{Use, occurrences};
use crate::project::ProjectFile;

/// A scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Scan {
    /// A public member no file outside the test folders names.
    Unused,
    /// A variable only ever assigned with a plain `=`.
    WriteOnly,
    /// A public function with an untyped, `Dictionary`, `Array` or `Variant` part in its signature.
    BareType,
}

/// What identifies a finding from one revision to the next: never its line, which any edit above it moves.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct Key {
    pub(crate) scan: Scan,
    /// Relative to the project folder.
    pub(crate) file: Utf8PathBuf,
    /// `<Class>.<member>`.
    pub(crate) symbol: String,
}

/// One candidate for a person to judge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Finding {
    #[serde(flatten)]
    pub(crate) key: Key,
    /// For display only.
    pub(crate) line: u32,
    pub(crate) detail: String,
}

/// A file whose declarations the scans could not read, so its findings are unknown rather than none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Unknown {
    #[serde(rename = "unknown")]
    pub(crate) file: Utf8PathBuf,
    pub(crate) reason: String,
}

/// Every script's declarations, the occurrences of each name, and the findings and unknowns they give.
#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) findings: Vec<Finding>,
    pub(crate) unknowns: Vec<Unknown>,
}

/// Where a name occurs, outside the declarations.
#[derive(Debug, Default)]
struct Occurrences {
    /// In the files outside the test folders, by how.
    uses: Vec<Use>,
    /// In the test folders.
    in_tests: usize,
}

/// The three scans over `files`. A script without a `class_name` is named by `autoloads`' name for it, or else by its
/// file stem.
pub(crate) fn scan(files: &[ProjectFile], autoloads: &BTreeMap<Utf8PathBuf, String>) -> Report {
    let mut report = Report::default();
    let declared = declared(files, autoloads, &mut report.unknowns);
    let found = named(files, &declared);
    let none = Occurrences::default();
    for (file, declaration) in &declared {
        let seen = found.get(declaration.name.as_str()).unwrap_or(&none);
        report
            .findings
            .extend(judged(declaration, seen).map(|(scan, detail)| Finding {
                key: Key {
                    scan,
                    file: (*file).to_owned(),
                    symbol: declaration.symbol.clone(),
                },
                line: declaration.line,
                detail,
            }));
    }
    report
        .findings
        .sort_by(|left, right| left.key.cmp(&right.key));
    report
        .unknowns
        .sort_by(|left, right| left.file.cmp(&right.file));
    report
}

/// Every declaration of the scripts outside the test folders, with its file. A file that is not UTF-8, or a script
/// the grammar cannot parse, is unknown instead.
fn declared<'a>(
    files: &'a [ProjectFile],
    autoloads: &BTreeMap<Utf8PathBuf, String>,
    unknowns: &mut Vec<Unknown>,
) -> Vec<(&'a Utf8Path, Declaration)> {
    let mut found = Vec::new();
    for file in files.iter().filter(|file| !file.test) {
        let Some(text) = &file.text else {
            unknowns.push(Unknown {
                file: file.path.clone(),
                reason: "it is not UTF-8".to_owned(),
            });
            continue;
        };
        if !file.is_script() {
            continue;
        }
        let stem = file.path.file_stem().unwrap_or_default();
        let class = autoloads.get(&file.path).map_or(stem, String::as_str);
        match declarations(text, class) {
            Ok(declarations) => found.extend(
                declarations
                    .into_iter()
                    .map(|declaration| (file.path.as_path(), declaration)),
            ),
            Err(why @ (Unparsed::Error(_) | Unparsed::Grammar)) => unknowns.push(Unknown {
                file: file.path.clone(),
                reason: why.to_string(),
            }),
        }
    }
    found
}

/// Where each declared name occurs in `files`, leaving out every declaration's own name.
fn named<'a>(
    files: &[ProjectFile],
    declared: &'a [(&Utf8Path, Declaration)],
) -> BTreeMap<&'a str, Occurrences> {
    let sites: BTreeSet<(&Utf8Path, usize)> = declared
        .iter()
        .map(|(file, declaration)| (*file, declaration.offset))
        .collect();
    let names: BTreeSet<&str> = declared
        .iter()
        .map(|(_, declaration)| declaration.name.as_str())
        .collect();
    let mut found: BTreeMap<&str, Occurrences> = BTreeMap::new();
    for file in files {
        let Some(text) = &file.text else {
            continue;
        };
        for occurrence in occurrences(text, file.dialect()) {
            let Some(name) = names.get(occurrence.name.as_str()) else {
                continue;
            };
            if occurrence.how != Use::Quoted
                && sites.contains(&(file.path.as_path(), occurrence.offset))
            {
                continue;
            }
            let entry = found.entry(name).or_default();
            if file.test {
                entry.in_tests += 1;
            } else {
                entry.uses.push(occurrence.how);
            }
        }
    }
    found
}

/// What each scan finds in one declaration, given where its name occurs: the scan and the finding's detail.
fn judged(declaration: &Declaration, seen: &Occurrences) -> impl Iterator<Item = (Scan, String)> {
    let mut found = Vec::new();
    let name = &declaration.name;
    if declaration.public() && seen.uses.is_empty() {
        let place = if seen.in_tests == 0 {
            "nowhere else"
        } else {
            "only in the test folders"
        };
        found.push((
            Scan::Unused,
            format!("{} {name} is named {place}", keyword(declaration.kind)),
        ));
    }
    // A word in a string neither reads nor writes: `fields.get("name")` is as likely a dictionary's key.
    let tokens = seen.uses.iter().filter(|how| **how != Use::Quoted);
    let assigned = tokens.clone().count();
    if declaration.kind == Kind::Variable
        && !declaration.used_by_engine
        && assigned > 0
        && tokens.into_iter().all(|how| *how == Use::Assigned)
    {
        let times = if assigned == 1 {
            "once".to_owned()
        } else {
            format!("{assigned} times")
        };
        found.push((
            Scan::WriteOnly,
            format!("var {name} is assigned {times} and never read"),
        ));
    }
    if declaration.kind == Kind::Function && declaration.public() && !declaration.bare.is_empty() {
        found.push((
            Scan::BareType,
            format!("func {name}: {}", declaration.bare.join(", ")),
        ));
    }
    found.into_iter()
}

fn keyword(kind: Kind) -> &'static str {
    match kind {
        Kind::Function => "func",
        Kind::Variable => "var",
        Kind::Constant => "const",
        Kind::Signal => "signal",
        Kind::Class => "class",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project of `(path, text)` files, those under `tests/` marked as test files.
    fn project(files: &[(&str, &str)]) -> Vec<ProjectFile> {
        files
            .iter()
            .map(|(path, text)| ProjectFile {
                path: Utf8PathBuf::from(path),
                text: Some((*text).to_owned()),
                test: path.starts_with("tests/"),
            })
            .collect()
    }

    fn found(files: &[(&str, &str)], scan: Scan) -> Vec<(String, String)> {
        super::scan(&project(files), &BTreeMap::new())
            .findings
            .into_iter()
            .filter(|finding| finding.key.scan == scan)
            .map(|finding| (finding.key.symbol, finding.detail))
            .collect()
    }

    fn symbols(files: &[(&str, &str)], scan: Scan) -> Vec<String> {
        found(files, scan)
            .into_iter()
            .map(|(symbol, _)| symbol)
            .collect()
    }

    #[test]
    fn a_public_member_named_nowhere_else_is_unused_whatever_its_kind() {
        let script = "class_name GFThing\nsignal moved\nconst LIMIT := 1\nvar size := 1.0\nfunc run() -> void:\n\
                      \tpass\nclass Inner:\n\tpass\nfunc _ready() -> void:\n\tpass\n";
        assert_eq!(
            symbols(&[("thing.gd", script)], Scan::Unused),
            [
                "GFThing.Inner",
                "GFThing.LIMIT",
                "GFThing.moved",
                "GFThing.run",
                "GFThing.size"
            ]
        );
    }

    #[test]
    fn a_use_in_any_script_scene_or_resource_outside_the_tests_counts() {
        let thing = "class_name GFThing\nfunc in_script() -> void:\n\tpass\nfunc in_scene() -> void:\n\tpass\n\
                     func in_string() -> void:\n\tpass\nfunc in_tests() -> void:\n\tpass\n\
                     func in_comment() -> void:\n\tpass\n";
        let user = "extends Node\nfunc _ready() -> void:\n\tGFThing.new().in_script()\n\tcall(\"in_string\")\n\
                    \t# in_comment\n";
        let scene =
            "[connection signal=\"pressed\" from=\"Button\" to=\".\" method=\"in_scene\"]\n";
        let probe = "extends Node\nfunc _ready() -> void:\n\tGFThing.new().in_tests()\n";
        let files = [
            ("thing.gd", thing),
            ("user.gd", user),
            ("main.tscn", scene),
            ("tests/probe.gd", probe),
        ];
        assert_eq!(
            found(&files, Scan::Unused),
            [
                (
                    "GFThing.in_comment".to_owned(),
                    "func in_comment is named nowhere else".to_owned()
                ),
                (
                    "GFThing.in_tests".to_owned(),
                    "func in_tests is named only in the test folders".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_member_used_within_its_own_script_is_used() {
        let script = "class_name GFThing\nvar size := 1.0\nfunc _ready() -> void:\n\tprint(size)\n";
        assert_eq!(
            symbols(&[("thing.gd", script)], Scan::Unused),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_declaration_is_not_a_use_so_two_classes_declaring_one_unused_name_are_both_candidates() {
        let files = [
            ("a.gd", "class_name A\nfunc reset() -> void:\n\tpass\n"),
            ("b.gd", "class_name B\nfunc reset() -> void:\n\tpass\n"),
        ];
        assert_eq!(symbols(&files, Scan::Unused), ["A.reset", "B.reset"]);
    }

    #[test]
    fn an_unrelated_local_with_the_same_name_hides_a_candidate() {
        let files = [
            ("a.gd", "class_name A\nfunc reset() -> void:\n\tpass\n"),
            (
                "b.gd",
                "extends Node\nfunc _ready() -> void:\n\tvar reset := 1\n\tprint(reset)\n",
            ),
        ];
        assert_eq!(symbols(&files, Scan::Unused), Vec::<String>::new());
    }

    #[test]
    fn a_name_built_at_run_time_is_not_followed_so_its_member_is_a_candidate() {
        let script = "class_name GFThing\nfunc set_size() -> void:\n\tpass\n\
                      func apply(field: String) -> void:\n\tcall(\"set_\" + field)\n";
        assert_eq!(
            symbols(&[("thing.gd", script)], Scan::Unused),
            ["GFThing.apply", "GFThing.set_size"]
        );
    }

    #[test]
    fn a_dictionary_written_with_equals_reads_as_an_assignment() {
        let script = "class_name GFThing\nvar size := 1\nfunc f() -> void:\n\tsize = 2\n\tprint({size = 3})\n";
        assert_eq!(
            symbols(&[("thing.gd", script)], Scan::WriteOnly),
            ["GFThing.size"]
        );
    }

    #[test]
    fn a_private_member_and_test_code_are_never_unused_candidates() {
        let files = [
            ("a.gd", "class_name A\nvar _hidden := 1\n"),
            (
                "tests/check.gd",
                "extends Node\nfunc helper() -> void:\n\tpass\n",
            ),
        ];
        assert_eq!(symbols(&files, Scan::Unused), Vec::<String>::new());
    }

    #[test]
    fn a_variable_only_assigned_with_a_plain_equals_is_write_only() {
        let thing = "class_name GFThing\nvar stand_in := false\nvar counted := 0\nvar listed: Array = []\n\
                     var _private := 0\nfunc set_up(other: GFThing) -> void:\n\tself.stand_in = true\n\
                     \tother.stand_in = false\n\tcounted += 1\n\tlisted[0] = 1\n\t_private = 2\n";
        let resource = "[resource]\nstand_in = true\n";
        assert_eq!(
            found(
                &[("thing.gd", thing), ("thing.tres", resource)],
                Scan::WriteOnly
            ),
            [
                (
                    "GFThing._private".to_owned(),
                    "var _private is assigned once and never read".to_owned()
                ),
                (
                    "GFThing.stand_in".to_owned(),
                    "var stand_in is assigned 3 times and never read".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_read_in_an_initialiser_or_accessor_counts_and_a_string_neither_reads_nor_writes() {
        let thing = "class_name GFThing\nvar base := 1\nvar doubled := base * 2\nvar shown := 0:\n\tget:\n\
                     \t\treturn shown\nvar keyed := 0\nfunc f(d: Dictionary[String, int]) -> void:\n\
                     \tbase = 2\n\tshown = 3\n\tkeyed = d.get(\"keyed\", 0)\n";
        assert_eq!(
            symbols(&[("thing.gd", thing)], Scan::WriteOnly),
            ["GFThing.keyed"]
        );
    }

    #[test]
    fn an_exported_variable_or_one_with_a_setter_is_never_write_only_but_a_stored_one_can_be() {
        let thing = "class_name GFThing\n@export var shown := 0\n@export_storage var stored := 0\nvar guarded := 0:\n\
                     \tset(value):\n\t\tguarded = value\nfunc f() -> void:\n\tshown = 1\n\tstored = 1\n\tguarded = 1\n";
        assert_eq!(
            symbols(&[("thing.gd", thing)], Scan::WriteOnly),
            ["GFThing.stored"]
        );
    }

    #[test]
    fn a_test_folders_reads_do_not_count_against_write_only() {
        let files = [
            (
                "thing.gd",
                "class_name GFThing\nvar flag := false\nfunc f() -> void:\n\tflag = true\n",
            ),
            (
                "tests/check.gd",
                "extends Node\nfunc _ready() -> void:\n\tprint(GFThing.new().flag)\n",
            ),
        ];
        assert_eq!(symbols(&files, Scan::WriteOnly), ["GFThing.flag"]);
    }

    #[test]
    fn a_public_function_with_a_bare_part_is_one_finding_listing_each_part() {
        let script = "class_name GFThing\nfunc parse(fields: Dictionary, rest := 0.0) -> Variant:\n\treturn null\n\
                      func typed(fields: Dictionary[String, int]) -> Array[int]:\n\treturn []\n\
                      func _private(fields: Dictionary) -> void:\n\tpass\nclass Inner:\n\tfunc go(x):\n\t\tpass\n";
        assert_eq!(
            found(&[("thing.gd", script)], Scan::BareType),
            [
                (
                    "GFThing.Inner.go".to_owned(),
                    "func go: x untyped, returns untyped".to_owned()
                ),
                (
                    "GFThing.parse".to_owned(),
                    "func parse: fields Dictionary, returns Variant".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_script_without_a_class_name_is_named_by_its_autoload_or_its_stem() {
        let files = project(&[
            (
                "registry.gd",
                "extends Node\nfunc lookup() -> void:\n\tpass\n",
            ),
            (
                "tool.gd",
                "extends Node\nfunc lookup_tool() -> void:\n\tpass\n",
            ),
        ]);
        let autoloads =
            BTreeMap::from([(Utf8PathBuf::from("registry.gd"), "DollRegistry".to_owned())]);
        let symbols: Vec<String> = scan(&files, &autoloads)
            .findings
            .into_iter()
            .map(|finding| finding.key.symbol)
            .collect();
        assert_eq!(symbols, ["DollRegistry.lookup", "tool.lookup_tool"]);
    }

    #[test]
    fn a_script_the_grammar_cannot_parse_is_unknown_and_still_names_its_references() {
        let files = project(&[
            (
                "thing.gd",
                "class_name GFThing\nfunc used() -> void:\n\tpass\n",
            ),
            (
                "broken.gd",
                "extends Node\nfunc _ready() -> void:\n\tGFThing.new().used()\nvar broken = (\n",
            ),
        ]);
        let report = scan(&files, &BTreeMap::new());
        assert_eq!(report.findings, []);
        assert_eq!(
            report.unknowns,
            [Unknown {
                file: "broken.gd".into(),
                reason: "the grammar cannot parse it at line 4".to_owned()
            }]
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_is_unknown() {
        let files = vec![ProjectFile {
            path: "bytes.gd".into(),
            text: None,
            test: false,
        }];
        assert_eq!(
            scan(&files, &BTreeMap::new()).unknowns,
            [Unknown {
                file: "bytes.gd".into(),
                reason: "it is not UTF-8".to_owned()
            }]
        );
    }
}
