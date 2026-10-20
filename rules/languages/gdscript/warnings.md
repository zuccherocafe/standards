# GDScript standard: Warnings

Part of the [GDScript standard](README.md). Warning levels, exceptions, the project's addons, and script compilation.

## Warnings

### Every warning is Error or off

*Always.* Warnings have three levels: 0 Ignore, 1 Warn, and 2 Error. At Error, the parser reports `Parse Error: … (Warning treated as error.)`, and the script does not load. At Warn, only a debugger receives the warning. The editor's script panel and a run with `-d` display it. An ordinary `--headless` run prints nothing and exits 0 (measured). A check cannot run under `-d` because the local debugger turns the first script error into an interactive break and aborts without input (measured), while project checks deliberately provoke script errors. In checks, only Error-level warnings are visible. Warn is for editor users, and a report of Warn-level occurrences under `-d` is a manual tool, never a check.

The canonical block is [templates/godot/project.godot](../../../templates/godot/project.godot). When the editor saves a setting, it rewrites `project.godot` without comments or values equal to engine defaults. The block survives a save unchanged (measured). It lists only non-default settings, with their reasons here. Merge it into the project's `[debug]` section rather than replacing the section. Four warnings default to Error: `inference_on_variant`, `native_method_override`, `get_node_default_without_onready`, and `onready_with_export`. Two remain at their default of off: `inferred_declaration` and `return_value_discarded`. The block sets every other warning to Error. The 44 Error-level warnings fall into these groups:

- **Typing.** `untyped_declaration`, the four `unsafe_*` warnings, and `inference_on_variant` enforce static typing. All except `inference_on_variant` default to Ignore because "static typing is optional". This standard requires static typing ([Types and records](design.md#types-and-records)).
- **Dead or confused code.** These warnings identify statements that do not behave as written: `unassigned_variable`, `unassigned_variable_op_assign`, the five `unused_*`, `unreachable_code`, `unreachable_pattern`, `standalone_expression`, and `standalone_ternary`. They also include the three `shadowed_*`, five `confusable_*`, `redundant_await`, `redundant_static_unload`, `empty_file`, `assert_always_true`, `assert_always_false`, and `deprecated_keyword`. Name an override-required or signal-required unused parameter `_name`. `assert_always_false` exempts literal `false`, so the asserting default arm in [Closed sets](design.md#closed-sets) compiles.
- **Numbers and enums.** These warnings are `integer_division`, `narrowing_conversion`, `int_as_enum_without_cast`, `int_as_enum_without_match`, `enum_variable_without_default`, and `incompatible_ternary`. GDScript has one `/`, so `1 / 2` silently produces `0`. An integer can enter an enum unchecked. An enum variable without a default starts at `0`, even if `0` is not a member. Write intended integer division under the exception form below, with a reason for truncation. Integer division truncates towards zero, as `-3 / 2` producing `-1` shows (measured). `floori` on a float does not have that behaviour.
- **Lifecycle and engine.** `missing_await`, `unsafe_void_return`, `static_called_on_instance`, `missing_tool`, `native_method_override`, `get_node_default_without_onready`, and `onready_with_export` detect engine-recognised mistakes. Three are errors by default.

Two warnings are off. `inferred_declaration` flags every `:=` and untyped constant except constants that import a type, such as `const Helper := preload(…)`. That restriction contradicts the style guide. `inference_on_variant` at Error already guarantees that inference never produces `Variant` ([Inference](design.md#inference-and-the-type-on-the-line)). `return_value_discarded` flags every `Array.append`, `connect`, and `erase`. The engine leaves it off as "too spammy". Review instead checks its useful case, an ignored engine `Error` ([Engine errors are checked](errors.md#engine-errors-are-checked)). The settings `property_used_as_function`, `constant_used_as_function`, and `function_used_as_property` remain but never produce warnings. Leave them alone.

If a project has not reached a warning level, record the departure, warning count, and plan to reach zero in its `AGENTS.md`. Omit that warning's line from the block until its count is zero. In a project not typed from the start, introduce the expensive typing warnings in separate changes that fix their occurrences. Start with `untyped_declaration`, whose fixes add types to `for` variables, lambda signatures, and declarations. Next introduce `unsafe_property_access` and `unsafe_cast`. Then introduce `unsafe_method_access` and `unsafe_call_argument`, each after record classes replace the dictionaries that pass `Variant` into typed code and that warning's count is zero. Introduce every other warning together with fixes for its occurrences. Do not exclude tests to reduce the typing work. Exclusion disables every warning, including `missing_await`, which tests particularly need.

Ref: `modules/gdscript/gdscript_warning.h` (every warning, its default, and the engine's one-line reason); `modules/gdscript/gdscript_parser.cpp` `push_warning` and `apply_pending_warnings`; gdUnit4's `project.godot` (`untyped_declaration=2` and the addons rule).

**Checked by:** the engine, for every script it compiles.

### An engine upgrade reviews the warning list

*Always.* The block is complete only for its engine version. A new warning starts at its default, usually Warn, which checks cannot see. In the engine-upgrade change, compare the new `gdscript_warning.h` with the old one. Set each new warning to Error or off in both the template and the project, and record the reason here in that same change. Also remove settings for warnings that the engine has removed, since the engine silently ignores those settings.

**Checked by:** review of the engine-upgrade change.

### Include the project's own addons

*Always.* Godot's default `directory_rules` is `{"res://addons": 0}`. It suppresses every warning under `res://addons/`, including the four default errors. A project with code in `addons/` gets no warnings until it includes those addons. Follow the manual's advice to include the project's own addons rather than delete the rule. Keep `"res://addons": 0` and add `"res://addons/<name>": 1` for each project-owned addon. The most specific path wins (measured). Project-owned addons therefore receive every warning, while `"res://addons": 0` still excludes any vendored third-party addon without an entry. Add a new project-owned addon to the rule in the change that creates it. `directory_rules` enables or disables every warning for a directory. It cannot change one warning's level by directory. Shipped code and tests therefore share warning levels unless `directory_rules` excludes the tests from every warning.

Ref: ProjectSettings, `debug/gdscript/warnings/directory_rules`; `modules/gdscript/gdscript.cpp` (the default); gdUnit4's `project.godot`.

**Checked by:** the engine, once the rule is set; review of `project.godot`.

### The checks compile every script

*Default.* A script compiles when something loads it. Import compiles required scripts, such as autoloads, but not every script. A script that no check loads may have an undetected parse error. After import, the project's checks load every `.gd` file once in one process. They fail on any engine error line or script that cannot instantiate. A broken script prints `ERROR: Failed to load script`, which fails the project's error filter. `load()` still returns a non-null `GDScript`, so `can_instantiate()` supplies a second signal rather than testing for null. Loading executes static initialisers ([Static state](lifetime.md#static-state)), which must be safe outside the game. Write this check script with full static types because the project's warning settings apply to it too. This check sits above a unit test and needs maintainer approval ([A check above a unit test needs the maintainer's approval](../../general.md#checks)). It catches compilation failures and Error-level warnings in scripts no unit test loads.

Ref: `godot --headless --check-only` (one script per process; the alternative when there are few scripts).

**Checked by:** a project check.

### Exceptions to a warning

*Always.* Put `@warning_ignore("one_warning")` on the narrowest statement, with a preceding comment that gives its reason ([Describe each check at its real strength](../../general.md#checks)). Use one warning per annotation. On a `func`, the annotation covers only its signature and parameters, not its body (measured). Place it there only for a signature warning. Never use `@warning_ignore_start` or `@warning_ignore_restore`. An unmatched start extends to the file's end, and a region can hide new code inside it. Ignores also work at Error, so exceptions can weaken a strict warning table. The engine reports neither useless ignores nor missing reasons. A report-only text scan therefore lists every `@warning_ignore` annotation for review ([Describe each check at its real strength](../../general.md#checks)). The engine does reject a misspelt warning name as a parse error.

```gdscript
# A majority of an even vote count is more than half, rounded down.
@warning_ignore("integer_division")
var majority := votes.size() / 2 + 1
```

Ref: `modules/gdscript/gdscript_parser.cpp` `warning_ignore_annotation` (what an ignore covers on each statement kind); `@GDScript` class reference, `@warning_ignore`.

**Checked by:** the engine (the name); a report-only text scan and review (placement and reason).

### Report scans

*Always.* On a Godot project whose `dandegate.toml` has `[project.godot]`, the change report runs `gdscript-scans` and lists what a change added and removed. You can also run `gdscript-scans` from the development shell as a review aid, and it causes no check failures. It runs three scans:

- **`unused`.** Lists public members that nothing outside the test folders names.
- **`write-only`.** Lists properties written but never read.
- **`bare-type`.** Lists public functions with an untyped, bare `Dictionary`, bare `Array`, or `Variant` parameter or return ([Typed everything](design.md#typed-everything), [A record is a class](design.md#a-record-is-a-class)).

Every finding is a candidate for review, not a fault, because name matching misses and over-reports in known ways. The crate documentation in [tools/crates/gdscript-scans/src/main.rs](../../../tools/crates/gdscript-scans/src/main.rs) gives the exact rules and limits.

**Checked by:** review of the findings that a change adds.
