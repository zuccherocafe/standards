use super::read::timeout;
use super::repo::{PathProblem, RepoFile};
use super::*;

fn read(text: &str) -> Result<Plan, PlanError> {
    within(&Tree::default(), text)
}

/// `text` read against an after revision holding `tree`.
fn within(tree: &Tree, text: &str) -> Result<Plan, PlanError> {
    Plan::read(
        text.as_bytes(),
        Utf8Path::new("dandegate.toml"),
        &[],
        &[],
        tree,
    )
}

/// A tree of `files`, with a Cargo workspace in each of `workspaces`.
fn tree(files: &[&str], workspaces: &[&str]) -> Tree {
    Tree::new(
        files.iter().map(Utf8PathBuf::from),
        workspaces.iter().map(Utf8PathBuf::from),
    )
}

fn refusal(text: &str) -> String {
    read(text).expect_err("a refusal").to_string()
}

const MINIMAL: &str =
    "[[steps.step]]\nname = \"visual\"\nrun = [\"sh\", \"{adapters}/visual.sh\", \"{tree}\"]\n";

#[test]
fn a_step_that_says_only_its_name_and_command_takes_every_default() {
    let plan = read(MINIMAL).expect("a plan");
    let step = &plan.steps()[&StepName::new("visual").expect("a name")];
    assert_eq!(step.kind(), Kind::PerRevision);
    assert_eq!(step.timeout(), Duration::from_mins(15));
    assert!(step.needs().is_empty() && step.outputs().is_empty());
    assert!(step.enabled() && !step.live() && !step.reuse() && !step.exclusive());
    let patterns: Vec<String> = step
        .identity_env()
        .iter()
        .map(EnvPattern::written)
        .collect();
    assert_eq!(patterns, ["CARGO*", "RUST*", "LD_LIBRARY_PATH"]);
    assert!(plan.rust().is_none());
}

#[test]
fn a_timeout_is_a_whole_positive_number_of_seconds_minutes_or_hours() {
    for (text, seconds) in [("90s", 90), ("30m", 1800), ("2h", 7200)] {
        assert_eq!(timeout(text), Some(Duration::from_secs(seconds)), "{text}");
    }
    for text in ["0m", "15", "m", "1d", "1.5h", "-1m", ""] {
        assert_eq!(timeout(text), None, "{text}");
    }
    assert!(
        refusal(&format!("{MINIMAL}timeout = \"soon\"\n"))
            .starts_with("dandegate.toml:4: step \"visual\": timeout:")
    );
}

#[test]
fn a_name_is_lowercase_letters_digits_and_hyphens_unique_and_not_reserved() {
    assert_eq!(
        refusal("[[steps.step]]\nname = \"Visual\"\nrun = [\"x\"]\n"),
        "dandegate.toml:2: step name: \"Visual\" is not lowercase letters, digits and hyphens"
    );
    assert_eq!(
        refusal(&format!("{MINIMAL}\n{MINIMAL}")),
        "dandegate.toml:6: step \"visual\" is declared twice"
    );
    for reserved in ["rust-scans", "lines", "mutants", "api", "api-listings"] {
        assert_eq!(
            refusal(&format!(
                "[[steps.step]]\nname = \"{reserved}\"\nrun = [\"x\"]\n"
            )),
            format!(
                "dandegate.toml:2: step \"{reserved}\" is a name dandegate's built-in reports use"
            )
        );
    }
}

#[test]
fn an_unknown_key_or_value_is_refused_with_its_line() {
    assert!(
        refusal(&format!("{MINIMAL}colour = \"blue\"\n"))
            .starts_with("dandegate.toml:4: unknown field `colour`")
    );
    assert!(
        refusal(&format!("{MINIMAL}runs = \"sometimes\"\n"))
            .starts_with("dandegate.toml:4: unknown variant `sometimes`")
    );
    assert_eq!(
        refusal(&format!("{MINIMAL}[steps.step.identity]\ncollect = []\n")),
        "dandegate.toml:5: step \"visual\": identity.collect: names no command"
    );
    assert_eq!(
        refusal("[[steps.step]]\nname = \"empty\"\nrun = []\n"),
        "dandegate.toml:3: step \"empty\": run: names no command"
    );
}

#[test]
fn an_output_is_a_plain_path_under_out_of_a_kind_its_step_produces() {
    let output = |runs: &str, output: &str| {
        refusal(&format!(
            "[[steps.step]]\nname = \"s\"\nruns = \"{runs}\"\nrun = [\"x\"]\noutput = [{output}]\n"
        ))
    };
    assert_eq!(
        output(
            "per-revision",
            "{ kind = \"measures\", from = \"{tree}/clips.log\" }"
        ),
        "dandegate.toml:5: step \"s\": output: from: \"{tree}/clips.log\" is not a plain path under {out}/"
    );
    assert_eq!(
        output(
            "per-revision",
            "{ kind = \"section\", file = \"{out}/../escape.md\" }"
        ),
        "dandegate.toml:5: step \"s\": output: file: \"{out}/../escape.md\" is not a plain path under {out}/"
    );
    assert_eq!(
        output(
            "per-revision",
            "{ kind = \"images\", folder = \"{out}/images/\" }"
        ),
        "dandegate.toml:5: step \"s\": output: an output of kind images needs expected"
    );
    assert_eq!(
        output(
            "per-revision",
            "{ kind = \"section\", file = \"{out}/a.md\", from = \"{out}/b\" }"
        ),
        "dandegate.toml:5: step \"s\": output: an output of kind section has no from"
    );
    assert_eq!(
        output(
            "per-revision",
            "{ kind = \"images\", folder = \"{out}/images\", expected = \"{out}/e\" }"
        ),
        "dandegate.toml:5: step \"s\": output: folder: \"{out}/images\" names a folder, so it ends in /"
    );
    assert_eq!(
        output(
            "per-revision",
            "{ kind = \"section\", file = \"{out}/a.md/\" }"
        ),
        "dandegate.toml:5: step \"s\": output: file: \"{out}/a.md/\" names a file, so it does not end in /"
    );
    let images = read(
        "[[steps.step]]\nname = \"s\"\nrun = [\"x\"]\n\
         output = [{ kind = \"images\", folder = \"{out}/a/b/\", expected = \"{out}/e\" }]\n",
    )
    .expect("a plan");
    assert_eq!(
        images.steps()[&StepName::new("s").expect("a name")].outputs(),
        [Output::Images {
            folder: OutPath(Utf8PathBuf::from("a/b")),
            expected: OutPath(Utf8PathBuf::from("e")),
        }]
    );
    assert!(
        output(
            "after",
            "{ kind = \"images\", folder = \"{out}/i/\", expected = \"{out}/e\" }"
        )
        .starts_with(
            "dandegate.toml:5: step \"s\": output: an after step has no output of kind images"
        )
    );
    assert!(
        output("once", "{ kind = \"measures\", from = \"{out}/m\" }").starts_with(
            "dandegate.toml:5: step \"s\": output: a once step has no output of kind measures"
        )
    );
    assert!(read("[[steps.step]]\nname = \"s\"\nruns = \"after\"\nrun = [\"x\"]\noutput = [{ kind = \"measures\", from = \"{out}/m\" }]\n").is_ok());
}

#[test]
fn an_environment_name_is_a_variable_a_step_may_set() {
    for name in ["1X", "A-B", "DANDEGATE_MEASURES"] {
        assert!(
            refusal(&format!("{MINIMAL}env = {{ \"{name}\" = \"x\" }}\n"))
                .contains("is not an environment variable's name"),
            "{name}"
        );
    }
    assert!(
        read(&format!(
            "{MINIMAL}env = {{ _X = \"{{out}}\", VK_LOADER_DEBUG = \"\" }}\n"
        ))
        .is_ok()
    );
    assert!(
        refusal(&format!(
            "{MINIMAL}[steps.step.identity]\nenv = [\"GODOT*X\"]\n"
        ))
        .contains("identity.env: \"GODOT*X\" is not a variable's name or a prefix ending in `*`")
    );
}

#[test]
fn host_programs_are_bare_file_names_each_named_once_in_the_files_order() {
    let plan = read(&format!(
        "[project]\nhost = [\"gamescope\", \"Xvfb\", \"vk.info\"]\n{MINIMAL}"
    ))
    .expect("a plan");
    let named: Vec<&str> = plan.host().iter().map(HostProgram::as_str).collect();
    assert_eq!(named, ["gamescope", "Xvfb", "vk.info"]);
    assert_eq!(read(MINIMAL).expect("a plan").host(), []);
    // Each name as TOML writes it, then as the refusal shows it.
    let refused = [
        (r#""""#, r#""""#),
        (r#"".""#, r#"".""#),
        (r#""..""#, r#""..""#),
        (r#""/usr/bin/gamescope""#, r#""/usr/bin/gamescope""#),
        (r#""bin/gamescope""#, r#""bin/gamescope""#),
        (r#""a\u0000b""#, r#""a\0b""#),
    ];
    for (written, shown) in refused {
        assert_eq!(
            refusal(&format!("[project]\nhost = [{written}]\n{MINIMAL}")),
            format!("dandegate.toml:2: host: {shown} is not a program's file name"),
            "{written}"
        );
    }
    assert_eq!(
        refusal(&format!(
            "[project]\nhost = [\"gamescope\",\n  \"gamescope\"]\n{MINIMAL}"
        )),
        "dandegate.toml:3: host: \"gamescope\" is named twice"
    );
}

#[test]
fn a_pattern_names_a_variable_or_a_prefix() {
    let whole = EnvPattern::new("LD_LIBRARY_PATH").expect("a name");
    let prefix = EnvPattern::new("VK_*").expect("a prefix");
    assert!(whole.matches("LD_LIBRARY_PATH") && !whole.matches("LD_LIBRARY_PATH_2"));
    assert!(prefix.matches("VK_ICD_FILENAMES") && !prefix.matches("XVK_"));
    assert!(EnvPattern::new("*").is_none());
}

#[test]
fn with_enables_a_step_and_everything_it_needs() {
    let text = "[[steps.step]]\nname = \"a\"\noptional = true\nrun = [\"x\"]\n\n\
                [[steps.step]]\nname = \"b\"\noptional = true\nneeds = [\"a\"]\nrun = [\"x\"]\n\n\
                [[steps.step]]\nname = \"c\"\noptional = true\nrun = [\"x\"]\n";
    let plan = Plan::read(
        text.as_bytes(),
        Utf8Path::new("dandegate.toml"),
        &["b".to_owned()],
        &[],
        &Tree::default(),
    )
    .expect("a plan");
    let enabled: Vec<&str> = plan
        .steps()
        .iter()
        .filter(|(_, step)| step.enabled())
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(enabled, ["a", "b"]);
}

#[test]
fn an_input_takes_its_default_unless_set_once() {
    let text =
        format!("[steps.inputs]\nrenders = {{ default = \"0\", help = \"frames\" }}\n{MINIMAL}");
    let set = |values: &[&str]| {
        let values: Vec<String> = values.iter().map(|value| (*value).to_owned()).collect();
        Plan::read(
            text.as_bytes(),
            Utf8Path::new("dandegate.toml"),
            &[],
            &values,
            &Tree::default(),
        )
    };
    let renders = InputName::new("renders").expect("a name");
    assert_eq!(set(&[]).expect("a plan").inputs()[&renders].value(), "0");
    assert_eq!(
        set(&["renders=a=b"]).expect("a plan").inputs()[&renders].value(),
        "a=b"
    );
    assert_eq!(
        set(&["renders=1", "renders=2"])
            .expect_err("a refusal")
            .to_string(),
        "--input: \"renders\" is set twice"
    );
}

#[test]
fn the_rust_reports_run_on_the_workspace_project_rust_names() {
    let files = ["Cargo.toml", "crates/a/src/lib.rs", "rust/Cargo.toml"];
    let workspaces = tree(&files, &[".", "rust"]);
    let plan = within(
        &workspaces,
        "[project.rust]\nworkspace = \"./\"\n[reports.rust]\nscans = [\"--core\", \"a\"]\n",
    )
    .expect("a plan");
    let settings = plan.rust().expect("Rust reports");
    assert_eq!(settings.workspace.as_path(), ".");
    assert_eq!(settings.scans, ["--core", "a"]);
    assert_eq!(settings.mutation, Vec::<String>::new());
    let names: Vec<&str> = plan.steps().keys().map(StepName::as_str).collect();
    assert_eq!(
        names,
        ["rust-scans", "lines", "mutants", "api-listings", "api"]
    );
    let api = &plan.steps()[&Report::Api.step()];
    assert!(!api.enabled() && api.kind() == Kind::Once);
    assert!(plan.steps()[&Report::ApiListings.step()].reuse());

    let folder = within(&workspaces, "[project.rust]\nworkspace = \"rust/\"\n").expect("a plan");
    assert_eq!(folder.rust().expect("Rust").workspace.as_path(), "rust");
    // An empty [reports.rust] is the same as none.
    let empty = within(
        &workspaces,
        "[project.rust]\nworkspace = \"rust/\"\n[reports.rust]\n",
    )
    .expect("a plan");
    assert_eq!(empty.rust(), folder.rust());
}

#[test]
fn a_project_with_no_rust_table_gets_no_rust_reports_and_a_notice_naming_the_table() {
    let notices =
        |plan: &Plan| -> Vec<String> { plan.notices().iter().map(ToString::to_string).collect() };
    let root = within(&tree(&["Cargo.toml", "src/lib.rs"], &["."]), "").expect("a plan");
    assert!(root.rust().is_none() && root.steps().is_empty());
    assert_eq!(
        notices(&root),
        [
            "the Rust reports are off: the after revision has a Cargo workspace in ./, but dandegate.toml has no \
             [project.rust] table; add one with workspace = \"./\" to turn them on"
        ]
    );
    let two = tree(&["rust/Cargo.toml", "tools/Cargo.toml"], &["rust", "tools"]);
    assert_eq!(
        notices(&within(&two, "").expect("a plan")),
        [
            "the Rust reports are off: the after revision has Cargo workspaces in rust/, tools/, but dandegate.toml \
             has no [project.rust] table; add one with workspace = \"<folder>/\" to turn them on"
        ]
    );
    let named = within(&two, "[project.rust]\nworkspace = \"rust/\"\n").expect("a plan");
    assert!(named.rust().is_some() && named.notices().is_empty());
    assert_eq!(read("").expect("a plan").notices(), []);
    // The suggested value is the TOML string that names the folder, whatever its name holds.
    for (folder, shown) in [
        ("rust\"copy", r#"'rust"copy/'"#),
        ("rust\\tools", r"'rust\tools/'"),
    ] {
        let plan = within(&tree(&[], &[folder]), "").expect("a plan");
        let notice = notices(&plan).concat();
        assert!(
            notice.ends_with(&format!("add one with workspace = {shown} to turn them on")),
            "{notice}"
        );
        let suggested = &notice[notice.find("workspace = ").expect("the suggestion") + 12..];
        let value: toml::Table = toml::from_str(&format!(
            "workspace = {}",
            suggested.trim_end_matches(" to turn them on")
        ))
        .expect("valid TOML");
        assert_eq!(
            value["workspace"].as_str(),
            Some(format!("{folder}/").as_str())
        );
    }
}

#[test]
fn the_rust_workspace_is_a_folder_holding_a_cargo_workspace() {
    let files = tree(
        &["Cargo.toml", "crates/a/Cargo.toml", "crates/a/src/lib.rs"],
        &["."],
    );
    let refused = |text: &str| within(&files, text).expect_err("a refusal").to_string();
    assert_eq!(
        refused("[project.rust]\nworkspace = \"crates/a/\"\n"),
        "dandegate.toml:2: [project.rust]: workspace: \"crates/a/\" holds no Cargo.toml with a [workspace] table in \
         the after revision"
    );
    assert_eq!(
        refused("[project.rust]\nworkspace = \"crates\"\n"),
        "dandegate.toml:2: [project.rust]: workspace: \"crates\" names a folder, so it ends in /"
    );
    assert_eq!(
        refused("[project.rust]\nworkspace = \"rust/\"\n"),
        "dandegate.toml:2: [project.rust]: workspace: \"rust/\" names no folder in the after revision"
    );
    for outside in [
        "../elsewhere/",
        "/rust/",
        "./crates/",
        "crates//a/",
        "crates/../crates/",
        "",
    ] {
        assert_eq!(
            refused(&format!("[project.rust]\nworkspace = \"{outside}\"\n")),
            format!(
                "dandegate.toml:2: [project.rust]: workspace: \"{outside}\" is not a plain relative path inside the \
                 repository"
            ),
            "{outside}"
        );
    }
    assert!(refused("[project.rust]\n").starts_with("dandegate.toml:1: missing field `workspace`"));
}

#[test]
fn reports_rust_without_project_rust_is_refused_even_when_empty() {
    let files = tree(&["Cargo.toml"], &["."]);
    for text in [
        "[reports.rust]\nscans = [\"--core\", \"a\"]\n",
        "\n[reports.rust]\n",
    ] {
        let refused = within(&files, text).expect_err("a refusal").to_string();
        assert!(
            refused.ends_with(
                "[reports.rust]: tunes the Rust reports, which run only when [project.rust] names the workspace; \
                 add [project.rust] or remove [reports.rust]"
            ),
            "{refused}"
        );
        let line = text
            .lines()
            .position(|line| line == "[reports.rust]")
            .expect("its header")
            + 1;
        assert!(
            refused.starts_with(&format!("dandegate.toml:{line}: ")),
            "{refused}"
        );
    }
}

#[test]
fn the_godot_project_and_its_tests_are_folders_the_after_revision_holds() {
    let files = tree(&["godot/project.godot", "godot/tests/a.gd"], &[]);
    assert!(
        within(
            &files,
            "[project.godot]\nproject = \"godot/\"\ntests = [\"godot/tests/\"]\n"
        )
        .is_ok()
    );
    let refused = |text: &str| within(&files, text).expect_err("a refusal").to_string();
    assert_eq!(
        refused(
            "[project.godot]\nproject = \"godot/\"\ntests = [\"godot/tests/\", \"godot/gone/\"]\n"
        ),
        "dandegate.toml:3: [project.godot]: tests: \"godot/gone/\" names no folder in the after revision"
    );
    assert_eq!(
        refused("[project.godot]\nproject = \"godot/project.godot/\"\n"),
        "dandegate.toml:2: [project.godot]: project: \"godot/project.godot/\" names no folder in the after revision"
    );
    assert!(refused("[project.godot]\ntests = []\n").contains("missing field `project`"));
}

#[test]
fn measuring_code_is_written_as_what_it_is_and_may_name_what_the_change_removed() {
    let files = tree(&["probes/a.sh", "tools/clips.sh"], &[]);
    let rules = within(
        &files,
        "[reports.measures]\ncode = [\"probes/\", \"tools/clips.sh\", \"removed/\", \"removed.sh\"]\n\
         exclude = [\"probes/a.sh\"]\ncode-measures = [\"clips.\"]\n",
    )
    .expect("a plan")
    .measures()
    .clone();
    let written: Vec<String> = rules.code.iter().map(ToString::to_string).collect();
    assert_eq!(
        written,
        ["probes/", "tools/clips.sh", "removed/", "removed.sh"]
    );
    assert_eq!(rules.exclude[0].as_path(), "probes/a.sh");
    assert_eq!(rules.code_measures, ["clips."]);

    let refused = |text: &str| within(&files, text).expect_err("a refusal").to_string();
    assert_eq!(
        refused("[reports.measures]\ncode = [\"probes\"]\n"),
        "dandegate.toml:2: [reports.measures]: code: \"probes\" names a folder in the after revision; write it with \
         a trailing /"
    );
    assert_eq!(
        refused("[reports.measures]\nexclude = [\"tools/clips.sh/\"]\n"),
        "dandegate.toml:2: [reports.measures]: exclude: \"tools/clips.sh/\" names a file in the after revision; \
         write it without the trailing /"
    );
    assert!(
        refused("[reports.measures]\ncode = [\"../probes/\"]\n")
            .ends_with("\"../probes/\" is not a plain relative path inside the repository")
    );
    assert!(
        refused("[reports.measures]\ncode_measures = []\n")
            .contains("unknown field `code_measures`")
    );
}

#[test]
fn a_repository_path_is_plain_and_relative_with_a_slash_only_on_a_folder() {
    assert_eq!(
        RepoDir::parse("./").map(|folder| folder.to_string()),
        Ok("./".to_owned())
    );
    assert_eq!(
        RepoDir::parse("a/b/").map(|folder| folder.as_path().to_owned()),
        Ok("a/b".into())
    );
    assert_eq!(RepoDir::parse("a/b"), Err(PathProblem::FolderWithoutSlash));
    assert_eq!(
        RepoFile::parse("a/b").map(|file| file.to_string()),
        Ok("a/b".to_owned())
    );
    assert_eq!(RepoFile::parse("a/b/"), Err(PathProblem::FileWithSlash));
    for outside in ["", "/", "/a", "../a", "a/./b", "./a", "a//b", "a/.."] {
        assert_eq!(
            RepoFile::parse(outside),
            Err(PathProblem::Outside),
            "{outside}"
        );
        assert_eq!(
            RepoDir::parse(&format!("{outside}/")),
            Err(PathProblem::Outside),
            "{outside}/"
        );
    }
}

#[test]
fn a_folder_is_one_some_file_lies_below() {
    let files = tree(&["a/b/c.txt", "ab.txt", "a-b/d.txt"], &[]);
    assert!(files.holds_folder(Utf8Path::new("a")) && files.holds_folder(Utf8Path::new("a/b")));
    assert!(files.holds_folder(Utf8Path::new(".")));
    assert!(
        !files.holds_folder(Utf8Path::new("ab.txt")) && !files.holds_folder(Utf8Path::new("a/b/c"))
    );
    assert!(!Tree::default().holds_folder(Utf8Path::new(".")));
}

#[test]
fn the_groups_are_project_reports_and_steps() {
    for (old, key) in [
        ("host = [\"x\"]\n", "host"),
        ("[inputs]\n", "inputs"),
        ("[measures]\n", "measures"),
        ("[rust]\n", "rust"),
        ("[[step]]\nname = \"x\"\nrun = [\"x\"]\n", "step"),
        ("[step]\nidentity = {}\n", "step"),
    ] {
        let refused = refusal(old);
        assert!(
            refused.contains(&format!("unknown field `{key}`")),
            "{refused}"
        );
    }
}

#[test]
fn a_cpu_share_is_at_least_one_never_on_an_exclusive_step_and_needed_only_by_steps_that_can_run_last()
 {
    let share = |extra: &str| {
        format!(
            "[[steps.step]]\nname = \"perf\"\nruns = \"once\"\nrun = [\"x\", \"{{cpus}}\"]\n{extra}\n"
        )
    };
    let plan = read(&share("cpus = 4")).expect("a plan");
    assert_eq!(
        plan.steps()[&StepName::new("perf").expect("a name")].cpus(),
        Some(4)
    );
    assert_eq!(
        refusal(&share("cpus = 0")),
        "dandegate.toml:5: step \"perf\": cpus: a share is at least one CPU"
    );
    assert_eq!(
        refusal(&share("cpus = 2\nexclusive = true")),
        "dandegate.toml:5: step \"perf\": cpus: an exclusive step runs alone on every CPU, so it has no share of them"
    );
    let needing = |extra: &str| {
        format!(
            "{}[[steps.step]]\nname = \"after\"\nruns = \"once\"\nneeds = [\"perf\"]\nrun = [\"x\"]\n{extra}\n",
            share("cpus = 2")
        )
    };
    assert_eq!(
        refusal(&needing("")),
        "dandegate.toml:9: step \"after\": needs \"perf\", which has a CPU share and so can run after every step \
         without one"
    );
    read(&needing("exclusive = true")).expect("an exclusive step runs last too");
    read(&needing("cpus = 1"))
        .expect("a step with a share runs last too when the shares do not fit");
}
