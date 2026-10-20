//! Lists a Godot project's unused public GDScript members, write-only properties and bare-typed signatures: the
//! GDScript standard's report scans. Each finding is a candidate for a person to judge, never a claim that deleting
//! or retyping it is safe. The scans fail nothing. This documentation is the scans' reference.
//!
//! # What it reads
//!
//! `gdscript-scans --project <folder> [--tests <folder>]…` reads every `.gd`, `.tscn`, `.tres` and `.godot` file below
//! the project folder, skipping folders whose names start with `.` (Godot's `.godot` cache among them), folders that
//! hold a `.gdignore`, which Godot does not import, and symbolic links. Each `--tests` folder must lie inside the
//! project. A file below one is test code: its declarations are never candidates, and its references never count as
//! uses. Paths are taken relative to the working folder.
//!
//! Declarations come from the GDScript grammar (crates.io `tree-sitter-gdscript`): each `func`, `var`, `const`,
//! `signal` and inner `class` at a script's top level or in an inner class's body, at any depth. References come from
//! a small lexer over every file instead, so a script the grammar cannot parse still contributes its references. The
//! lexer keeps whole identifier tokens and each identifier-shaped word inside a string, StringName (`&"…"`) or
//! NodePath (`^"…"`) literal, and skips comments (`#` in scripts, doc comments such as `[member enabled]` among them;
//! `;` in scenes, resources and `project.godot`), numbers and operators. Matching is by name alone. A declaration's
//! own name is not a use of it, nor of another member with the same name.
//!
//! # Names
//!
//! A member's symbol is `<Class>.<member>`. The class is the script's `class_name`; else the name `project.godot`'s
//! `[autoload]` section gives the script; else the script's file stem. An inner class adds its name to its outer
//! class's: `GFThing.Inner.field`, and the inner class itself is `GFThing.Inner`. Public means no leading underscore.
//!
//! # The scans
//!
//! - **unused**: a public `func`, `var`, `const`, `signal` or inner `class` whose name appears nowhere else in any
//!   file outside the test folders. A word in a string counts as a use, so a call by string (`call`, `connect`,
//!   `has_method`, `Callable(self, "name")`) and a scene's `[connection … method="name"]` keep a member used. The
//!   detail says whether the name appears only in the test folders or nowhere else. No engine virtual is ever a
//!   candidate: every virtual method of Godot 4.7's API (`extension_api.json`, `is_virtual`) begins with `_`, and the
//!   engine refuses a script method that overrides a native one that is not (`native_method_override`, an error
//!   unless a project lowers it), so a public member never names one.
//! - **write-only**: a script-level or inner-class `var`, public or private, whose every other occurrence by
//!   identifier is the direct target of a plain `=` (`x = …`, `self.x = …`, `obj.x = …`, or a scene's or resource's
//!   `x = …` line), with at least one such occurrence. A compound assignment (`+=` and the like) reads, `items[i] = v`
//!   reads `items`, and a read in an initialiser or accessor counts. A word in a string neither reads nor writes,
//!   since `fields.get("name")` is as often a dictionary's key as a property's. A variable exported to the inspector
//!   (`@export` or an `@export_*` form) or with a setter is never a candidate, since the inspector, scenes or the
//!   setter's effect use it. `@export_storage` only serialises a value and shows nothing, so a stored variable can be
//!   one; the grouping annotations (`@export_group`, `@export_subgroup`, `@export_category`) export nothing.
//! - **bare-type**: a public function, static or not, with a parameter or return that is untyped, typed
//!   `Dictionary` or `Array` with no element type, or typed `Variant`. An inferred parameter (`bound := 0.0`) is
//!   typed, a missing `-> T` is an untyped return, and a variadic parameter (`...rest: Array`) is never bare, since an
//!   `Array` is all it can be. There is one finding per function, whose detail lists each bare part in order.
//!
//! # Known limits
//!
//! - A name built at run time (`call("set_" + name)`, `get(property)`) cannot be followed: a member used only that
//!   way is a false candidate.
//! - An unrelated local, parameter, dictionary key, word in a string or member of another class with the same name
//!   hides a real candidate, since matching is by name alone. For write-only, a read of any same-named identifier
//!   outside the tests hides it.
//! - A property read by string (`get("name")`, `get_indexed`) is not a read, so such a variable can be a false
//!   write-only candidate.
//! - A dictionary written as `{name = value}` reads as an assignment to `name`.
//! - Escapes inside a string literal are not decoded, so a name written with them (`"r\u0065set"`) is not seen.
//! - References in other file kinds (`.json`, `.cfg`, shaders, binary `.res` and `.scn`) are not read.
//!
//! # Output
//!
//! One JSON object per line, findings first, in key order (scan, file, symbol), then unknowns in file order:
//!
//! - `{"scan": "unused" | "write-only" | "bare-type", "file": "<path relative to the project>", "symbol":
//!   "<Class>.<member>", "line": <n>, "detail": "<one human line>"}`. The key is (scan, file, symbol); `line` is for
//!   display only, so an edit above a finding does not change it.
//! - `{"unknown": "<path>", "reason": "<text>"}` for a file whose findings are unknown: a script the grammar cannot
//!   parse (any `ERROR` or missing node), whose declarations are then unread although its references count, or a
//!   file that is not UTF-8, whose references are unread. Either way the unused and write-only findings of every other
//!   file are incomplete too, since they rest on every file's references; bare-type findings are per file.
//!
//! The exit status is 0 when the scan ran, whatever it found; 1 when the output could not be written; and 2 for a
//! malformed argument or a project that cannot be read.
//!
//! [`project`] reads the files; [`declarations`], [`lexer`] and [`scan`] are functions over their text, tested on
//! made-up scripts; this file runs them, prints, and is the one place errors are rendered.

mod declarations;
mod lexer;
mod project;
mod scan;

use std::collections::BTreeMap;
use std::error::Error;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;

use crate::project::{ProjectError, autoloads, files};
use crate::scan::Report;

/// The command line.
#[derive(Debug, Parser)]
#[command(
    name = "gdscript-scans",
    version,
    about = "The GDScript standard's report scans over a Godot project: unused public members, write-only \
             properties and bare-typed signatures; they report and fail nothing"
)]
struct Arguments {
    /// The Godot project folder, holding project.godot.
    #[arg(long, value_name = "FOLDER")]
    project: Utf8PathBuf,
    /// A folder of test code inside the project, whose declarations are never candidates and whose references never
    /// count; repeat for each.
    #[arg(long, value_name = "FOLDER")]
    tests: Vec<Utf8PathBuf>,
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    let report = match run(&arguments) {
        Ok(report) => report,
        Err(error) => {
            render(&error);
            return ExitCode::from(2);
        }
    };
    match print(&report) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            render(&PrintError(error));
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &Arguments) -> Result<Report, ProjectError> {
    let files = files(&arguments.project, &arguments.tests)?;
    let named = files
        .iter()
        .find(|file| file.path == "project.godot")
        .and_then(|file| file.text.as_deref())
        .map_or_else(BTreeMap::new, autoloads);
    Ok(scan::scan(&files, &named))
}

fn print(report: &Report) -> io::Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    for finding in &report.findings {
        serde_json::to_writer(&mut out, finding)?;
        writeln!(out)?;
    }
    for unknown in &report.unknowns {
        serde_json::to_writer(&mut out, unknown)?;
        writeln!(out)?;
    }
    out.flush()
}

/// The findings could not be written.
#[derive(Debug, thiserror::Error)]
#[error("could not print the findings")]
struct PrintError(#[source] io::Error);

/// The error and each of its causes, once, on standard error.
fn render(failure: &dyn Error) {
    let mut errors = io::stderr().lock();
    let mut cause: Option<&dyn Error> = Some(failure);
    let mut lead = "gdscript-scans: ";
    while let Some(error) = cause {
        // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
        let _ = writeln!(errors, "{lead}{error}");
        lead = "  caused by: ";
        cause = error.source();
    }
}
