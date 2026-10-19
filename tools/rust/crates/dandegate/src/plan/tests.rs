use super::read::timeout;
use super::*;

fn read(text: &str) -> Result<Plan, PlanError> {
    Plan::read(
        text.as_bytes(),
        Utf8Path::new("dandegate.toml"),
        &[],
        &[],
        &[],
    )
}

fn refusal(text: &str) -> String {
    read(text).expect_err("a refusal").to_string()
}

const MINIMAL: &str =
    "[[step]]\nname = \"visual\"\nrun = [\"sh\", \"{adapters}/visual.sh\", \"{tree}\"]\n";

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
        refusal("[[step]]\nname = \"Visual\"\nrun = [\"x\"]\n"),
        "dandegate.toml:2: step name: \"Visual\" is not lowercase letters, digits and hyphens"
    );
    assert_eq!(
        refusal(&format!("{MINIMAL}\n{MINIMAL}")),
        "dandegate.toml:6: step \"visual\" is declared twice"
    );
    for reserved in ["scans", "lines", "mutants", "api", "api-listings"] {
        assert_eq!(
            refusal(&format!("[[step]]\nname = \"{reserved}\"\nrun = [\"x\"]\n")),
            format!("dandegate.toml:2: step \"{reserved}\" is a name dandegate's Rust reports use")
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
        refusal(&format!("{MINIMAL}[step.identity]\ncollect = []\n")),
        "dandegate.toml:5: step \"visual\": identity.collect: names no command"
    );
    assert_eq!(
        refusal("[[step]]\nname = \"empty\"\nrun = []\n"),
        "dandegate.toml:3: step \"empty\": run: names no command"
    );
}

#[test]
fn an_output_is_a_plain_path_under_out_of_a_kind_its_step_produces() {
    let output = |runs: &str, output: &str| {
        refusal(&format!(
            "[[step]]\nname = \"s\"\nruns = \"{runs}\"\nrun = [\"x\"]\noutput = [{output}]\n"
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
            "{ kind = \"images\", folder = \"{out}/images\" }"
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
    assert!(
        output(
            "after",
            "{ kind = \"images\", folder = \"{out}/i\", expected = \"{out}/e\" }"
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
    assert!(read("[[step]]\nname = \"s\"\nruns = \"after\"\nrun = [\"x\"]\noutput = [{ kind = \"measures\", from = \"{out}/m\" }]\n").is_ok());
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
        refusal(&format!("{MINIMAL}[step.identity]\nenv = [\"GODOT*X\"]\n")).contains(
            "identity.env: \"GODOT*X\" is not a variable's name or a prefix ending in `*`"
        )
    );
}

#[test]
fn host_programs_are_bare_file_names_each_named_once_in_the_files_order() {
    let plan = read(&format!(
        "host = [\"gamescope\", \"Xvfb\", \"vk.info\"]\n{MINIMAL}"
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
            refusal(&format!("\nhost = [{written}]\n{MINIMAL}")),
            format!("dandegate.toml:2: host: {shown} is not a program's file name"),
            "{written}"
        );
    }
    assert_eq!(
        refusal(&format!(
            "host = [\"gamescope\",\n  \"gamescope\"]\n{MINIMAL}"
        )),
        "dandegate.toml:2: host: \"gamescope\" is named twice"
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
    let text = "[[step]]\nname = \"a\"\noptional = true\nrun = [\"x\"]\n\n\
                [[step]]\nname = \"b\"\noptional = true\nneeds = [\"a\"]\nrun = [\"x\"]\n\n\
                [[step]]\nname = \"c\"\noptional = true\nrun = [\"x\"]\n";
    let plan = Plan::read(
        text.as_bytes(),
        Utf8Path::new("dandegate.toml"),
        &["b".to_owned()],
        &[],
        &[],
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
    let text = format!("[inputs]\nrenders = {{ default = \"0\", help = \"frames\" }}\n{MINIMAL}");
    let set = |values: &[&str]| {
        let values: Vec<String> = values.iter().map(|value| (*value).to_owned()).collect();
        Plan::read(
            text.as_bytes(),
            Utf8Path::new("dandegate.toml"),
            &[],
            &values,
            &[],
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
fn the_rust_reports_follow_the_one_workspace_found_or_named() {
    let found = |workspaces: &[&str], text: &str| {
        let workspaces: Vec<Utf8PathBuf> = workspaces.iter().map(Utf8PathBuf::from).collect();
        Plan::read(
            text.as_bytes(),
            Utf8Path::new("dandegate.toml"),
            &[],
            &[],
            &workspaces,
        )
    };
    let plan = found(&["."], "").expect("a plan");
    let settings = plan.rust().expect("Rust reports");
    assert_eq!(settings.workspace, Utf8PathBuf::from("."));
    assert_eq!(settings.mutation, Vec::<String>::new());
    let names: Vec<&str> = plan.steps().keys().map(StepName::as_str).collect();
    assert_eq!(names, ["scans", "lines", "mutants", "api-listings", "api"]);
    let api = &plan.steps()[&Report::Api.step()];
    assert!(!api.enabled() && api.kind() == Kind::Once);
    assert!(plan.steps()[&Report::ApiListings.step()].reuse());

    assert_eq!(
        found(&["rust", "tools"], "")
            .expect_err("a refusal")
            .to_string(),
        "dandegate.toml: [rust]: the after revision has several Cargo workspaces (rust, tools); name one with \
         workspace = \"<folder>\""
    );
    assert_eq!(
        found(&["rust", "tools"], "[rust]\nworkspace = \"rust\"\n")
            .expect("a plan")
            .rust()
            .expect("Rust")
            .workspace,
        "rust"
    );
    assert!(
        found(&[], "[rust]\nscans = []\n")
            .expect_err("a refusal")
            .to_string()
            .contains("no Cargo.toml with a [workspace]")
    );
    assert!(
        found(&[], "[rust]\nworkspace = \"../elsewhere\"\n")
            .expect_err("a refusal")
            .to_string()
            .contains("is not a folder inside the repository")
    );
}
