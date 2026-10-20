//! gdscript-scans through its command, on a tiny Godot project written to the system's temporary folder. Its uses, most
//! common first:
//!
//! 1. A change report: a project with a test folder prints each finding and each unknown file as a JSON line and
//!    exits 0, reading neither hidden nor ignored folders.
//! 2. A test folder outside the project, or a project that is not a folder, exits 2 and says why on standard error.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// A new, empty folder for one scenario, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let folder =
            std::env::temp_dir().join(format!("gdscript-scans-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("the scratch folder is writable");
        Self(folder)
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().expect("a file has a folder"))
            .expect("the scratch folder is writable");
        fs::write(path, text).expect("the scratch folder is writable");
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_gdscript-scans"))
            .args(arguments)
            .current_dir(&self.0)
            .output()
            .expect("the binary starts")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_project_prints_each_finding_then_each_unknown_file_as_a_json_line() {
    let scratch = Scratch::new("report");
    scratch.write(
        "game/project.godot",
        "[autoload]\nRegistry=\"*res://registry.gd\"\n",
    );
    scratch.write(
        "game/body/clips.gd",
        "class_name GFBodyClips\nextends Node\n\nvar stand_in := false\n\n\
         func install(node: Dictionary) -> void:\n\tstand_in = true\n\tprint(node)\n",
    );
    scratch.write(
        "game/registry.gd",
        "extends Node\n\nfunc lookup(name: StringName) -> Node:\n\treturn get_node(NodePath(name))\n\n\
         func forget() -> void:\n\tpass\n",
    );
    scratch.write(
        "game/main.tscn",
        "[connection signal=\"ready\" from=\".\" to=\".\" method=\"lookup\"]\n",
    );
    scratch.write(
        "game/broken.gd",
        "extends Node\n\nfunc _ready() -> void:\n\tvar broken = (\n",
    );
    scratch.write(
        "game/tests/check.gd",
        "extends Node\n\nfunc _ready() -> void:\n\tGFBodyClips.new().install({})\n",
    );
    // Godot's cache and a folder Godot does not import are not read, so their uses do not count.
    scratch.write(
        "game/.godot/cache.gd",
        "extends Node\nfunc f() -> void:\n\tprint(GFBodyClips.new().stand_in)\n",
    );
    scratch.write("game/captures/.gdignore", "");
    scratch.write(
        "game/captures/use.gd",
        "extends Node\nfunc f() -> void:\n\tprint(GFBodyClips.new().stand_in)\n",
    );
    // Nor is a file of another kind, or a symbolic link.
    scratch.write("game/notes.cfg", "read = stand_in\n");
    scratch.write(
        "outside.gd",
        "extends Node\nfunc f() -> void:\n\tprint(GFBodyClips.new().stand_in)\n",
    );
    std::os::unix::fs::symlink(
        scratch.0.join("outside.gd"),
        scratch.0.join("game/linked.gd"),
    )
    .expect("the scratch folder is writable");
    let output = scratch.run(&["--project", "game", "--tests", "game/tests"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("the output is UTF-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            r#"{"scan":"unused","file":"body/clips.gd","symbol":"GFBodyClips.install","line":6,"detail":"func install is named only in the test folders"}"#,
            r#"{"scan":"unused","file":"registry.gd","symbol":"Registry.forget","line":6,"detail":"func forget is named nowhere else"}"#,
            r#"{"scan":"write-only","file":"body/clips.gd","symbol":"GFBodyClips.stand_in","line":4,"detail":"var stand_in is assigned once and never read"}"#,
            r#"{"scan":"bare-type","file":"body/clips.gd","symbol":"GFBodyClips.install","line":6,"detail":"func install: node Dictionary"}"#,
            r#"{"unknown":"broken.gd","reason":"the grammar cannot parse it at line 4"}"#,
        ]
    );
}

#[test]
fn a_test_folder_outside_the_project_or_a_project_that_is_no_folder_exits_2() {
    let scratch = Scratch::new("refused");
    scratch.write("game/project.godot", "");
    scratch.write("tests/check.gd", "extends Node\n");
    let outside = scratch.run(&["--project", "game", "--tests", "tests"]);
    assert_eq!(outside.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&outside.stderr),
        "gdscript-scans: the test folder tests is not inside the project game\n"
    );
    let missing = scratch.run(&["--project", "absent"]);
    assert_eq!(missing.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        "gdscript-scans: absent is not a folder\n"
    );
    assert_eq!(scratch.run(&["--tests", "game"]).status.code(), Some(2));
}
