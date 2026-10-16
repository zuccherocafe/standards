# The GDScript standard

This standard governs GDScript in zucchero✿café projects using Godot 4.7. It applies [general.md](../general.md) to GDScript and adds GDScript-specific rules. [review.md](../review.md) turns both standards into review questions. [templates/godot/](../templates/godot/) contains the engine-check configuration in `project.godot`'s `[debug]` block and the formatter configuration.

GDScript is gradually typed, and the engine's analyser checks it. It has no generics, nullability, sum types, interfaces beyond abstract classes, exhaustive `match`, or exceptions. When a resource loads, its typed exports are converted rather than checked. The analyser's warnings cover much, but not all, of the type system. Some dynamic access compiles without a diagnostic and fails only at run time. This standard identifies those gaps and the alternatives: boundary parsers, record classes, review conventions, or an explicit lack of checks.

References name paths in public repositories and pin commits so the line numbers remain valid. The references are the [Godot 4.7 manual](https://docs.godotengine.org/en/4.7/), [godot](https://github.com/godotengine/godot) at `4.7.1-stable` (`a13da4feb`), and its tests in `modules/gdscript/tests/scripts/`. They also include [gdUnit4](https://github.com/godot-gdunit-labs/gdUnit4) at `0b1748a9`, [godot-demo-projects](https://github.com/godotengine/godot-demo-projects) at `b761b4cd`, and [godot-open-rpg](https://github.com/gdquest-demos/godot-open-rpg) at `d59f2f71`. Other references are [phantom-camera](https://github.com/ramokz/phantom-camera) at `21d37624`, [Pixelorama](https://github.com/Orama-Interactive/Pixelorama) at `578d058a`, and [beehave](https://github.com/bitbrain/beehave) at `fe589153`. These references illustrate the patterns discussed below. Where a reference falls short of a rule, the rule identifies that limitation. Examples are short and invented except for [tdollhouse](https://github.com/zuccherocafe/tdollhouse) examples. Introduce those with "for example, in tdollhouse:" and retain them only for what the references do not teach. Behaviour marked "(measured)" was run on Godot `4.7.1.stable.official.a13da4feb`. Every complete GDScript block below compiles under the template's `[debug]` block. Blocks marked "sketch" are fragments.

Each rule ends with a Checked by entry naming the engine, a project check, or review. An engine check means an Error-level warning or a load-time or run-time error caught by the project's error filter. Where the engine checks only part of a rule, the entry names that part.

## Toolchain and warnings

### The engine is the checker

Each project pins an exact engine build, such as `4.7.1.stable.official.a13da4feb`. Run checks with the editor binary or a debug template, because GDScript warnings exist only in debug builds. The analyser is the compiler in `general.md`'s sense. It understands the pinned version's syntax, needs no external maintenance, and stores its settings in `project.godot`. The editor therefore shows the same errors that fail checks. No external linter is a check. gdtoolkit accepts syntax only up to Godot 4.5 and rejects valid 4.7 code. Its size thresholds are forbidden by `general.md`. The formatter is the only external tool in the checks. It decides layout, not correctness ([Names and order](#names-and-order)).

**Checked by:** the engine.

### Engine upgrades

A project adopts every stable engine release, patch or minor, soon after release. Frequent upgrades stay small and deliver fixes while the affected code is familiar. They also keep the warning list and workarounds current. Each upgrade is one change that follows these steps:

1. Move the engine pin in the project's `AGENTS.md`, the engine binary, and the export templates together. Record their versions and digests in the development shell's flake ([Toolchain](../general.md#toolchain)). The checks, editor, and exported program must use one build.
2. When upgrading the engine, move any GDExtension binding, such as gdext, and its engine-API feature together. For a minor release, wait until the binding supports its API. Do not run a binding built against the previous API on the new engine.
3. Re-import the project, so every cache is rebuilt by the new engine.
4. Compare the release's warning list with the previous list. Set every new warning to Error or off ([An engine upgrade reviews the warning list](#an-engine-upgrade-reviews-the-warning-list)).
5. Re-check every recorded engine workaround. Remove each workaround fixed by the release, together with its note.
6. Run the formatter's `--check`, since the formatter's pin may move with the engine's syntax.
7. Land the change with the full checks, a report of what changed, and the performance report. Review engine-induced differences before further work depends on them.

**Checked by:** review of the upgrade change against these steps; the project's checks.

### Every warning is Error or off

Warnings have three levels: 0 Ignore, 1 Warn, and 2 Error. At Error, the parser reports `Parse Error: … (Warning treated as error.)`, and the script does not load. At Warn, only a debugger receives the warning. The editor's script panel and a run with `-d` display it. An ordinary `--headless` run prints nothing and exits 0 (measured). A check cannot run under `-d`. The local debugger turns the first script error into an interactive break and aborts without input (measured). Project checks deliberately provoke script errors. In checks, only Error-level warnings are visible. Warn is for editor users. A report of Warn-level occurrences under `-d` is a manual tool, never a check.

The canonical block is [templates/godot/project.godot](../templates/godot/project.godot). When the editor saves a setting, it rewrites `project.godot` without comments or values equal to engine defaults. The block survives a save unchanged (measured). It lists only non-default settings, with their reasons here. Merge it into the project's `[debug]` section rather than replacing the section. Four warnings default to Error: `inference_on_variant`, `native_method_override`, `get_node_default_without_onready`, and `onready_with_export`. Two remain at their default of off: `inferred_declaration` and `return_value_discarded`. The block sets every other warning to Error. The 44 Error-level warnings fall into these groups:

- **Typing.** `untyped_declaration`, the four `unsafe_*` warnings, and `inference_on_variant` enforce static typing. All except `inference_on_variant` default to Ignore because "static typing is optional". Here, static typing is required ([Types and records](#types-and-records)).
- **Dead or confused code.** These warnings identify statements that do not behave as written: `unassigned_variable`, `unassigned_variable_op_assign`, the five `unused_*`, `unreachable_code`, `unreachable_pattern`, `standalone_expression`, and `standalone_ternary`. They also include the three `shadowed_*`, five `confusable_*`, `redundant_await`, `redundant_static_unload`, `empty_file`, `assert_always_true`, `assert_always_false`, and `deprecated_keyword`. Name an override-required or signal-required unused parameter `_name`. `assert_always_false` exempts literal `false`, so the asserting default arm in [Closed sets](#closed-sets) compiles.
- **Numbers and enums.** These warnings are `integer_division`, `narrowing_conversion`, `int_as_enum_without_cast`, `int_as_enum_without_match`, `enum_variable_without_default`, and `incompatible_ternary`. GDScript has one `/`, so `1 / 2` silently produces `0`. An integer can enter an enum unchecked. An enum variable without a default starts at `0`, even if `0` is not a member. Write intended integer division under the exception form below, with a reason for truncation. Integer division truncates towards zero, as `-3 / 2` producing `-1` shows (measured). `floori` on a float does not have that behaviour.
- **Lifecycle and engine.** `missing_await`, `unsafe_void_return`, `static_called_on_instance`, `missing_tool`, `native_method_override`, `get_node_default_without_onready`, and `onready_with_export` detect engine-recognised mistakes. Three are errors by default.

Two warnings are off. `inferred_declaration` flags every `:=` and untyped constant except constants that import a type, such as `const Helper := preload(…)`. That restriction contradicts the style guide. `inference_on_variant` at Error already guarantees that inference never produces `Variant` ([Inference](#inference-and-the-type-on-the-line)). `return_value_discarded` flags every `Array.append`, `connect`, and `erase`. The engine leaves it off as "too spammy". Review instead checks its useful case, an ignored engine `Error` ([Engine errors are checked](#engine-errors-are-checked)). The settings `property_used_as_function`, `constant_used_as_function`, and `function_used_as_property` remain but never produce warnings. Leave them alone.

If a project has not reached a warning level, record the departure, warning count, and plan to reach zero in its `AGENTS.md`. Omit that warning's line from the block until its count is zero. In a project not typed from the start, introduce the expensive typing warnings in separate changes that fix their occurrences. Start with `untyped_declaration`, whose fixes add types to `for` variables, lambda signatures, and declarations. Next introduce `unsafe_property_access` and `unsafe_cast`. Then introduce `unsafe_method_access` and `unsafe_call_argument`, each after record classes replace the dictionaries that pass `Variant` into typed code and that warning's count is zero. Introduce every other warning together with fixes for its occurrences. Do not exclude tests to reduce the typing work. Exclusion disables every warning, including `missing_await`, which tests particularly need.

Ref: `modules/gdscript/gdscript_warning.h` (every warning, its default, and the engine's one-line reason); `modules/gdscript/gdscript_parser.cpp` `push_warning` and `apply_pending_warnings`; gdUnit4's `project.godot` (`untyped_declaration=2` and the addons rule).

**Checked by:** the engine, for every script it compiles.

### An engine upgrade reviews the warning list

The block is complete only for its engine version. A new warning starts at its default, usually Warn, which checks cannot see. In the engine-upgrade change, compare the new `gdscript_warning.h` with the old one. Set each new warning to Error or off in both the template and project. Record the reason here in that same change. Also remove settings for warnings removed by the engine, since the engine silently ignores those settings.

**Checked by:** review of the engine-upgrade change.

### Include the project's own addons

Godot's default `directory_rules` is `{"res://addons": 0}`. It suppresses every warning under `res://addons/`, including the four default errors. A project with code in `addons/` gets no warnings until it includes those addons. Follow the manual's advice to include the project's own addons rather than delete the rule. Keep `"res://addons": 0` and add `"res://addons/<name>": 1` for each project-owned addon. The most specific path wins (measured). Project-owned addons therefore receive every warning, while a vendored third-party addon remains excluded without an entry. Add a new project-owned addon to the rule in the change that creates it. `directory_rules` enables or disables every warning for a directory. It cannot change one warning's level by directory. Shipped code and tests therefore share warning levels unless tests are excluded from every warning.

Ref: ProjectSettings, `debug/gdscript/warnings/directory_rules`; `modules/gdscript/gdscript.cpp` (the default); gdUnit4's `project.godot`.

**Checked by:** the engine, once the rule is set; review of `project.godot`.

### Every script is compiled by the checks

A script compiles when something loads it. Import compiles required scripts, such as autoloads, but not every script. A script that no check loads may have an undetected parse error. After import, the project's checks load every `.gd` file once in one process. They fail on any engine error line or script that cannot instantiate. A broken script prints `ERROR: Failed to load script`, which fails the project's error filter. `load()` still returns a non-null `GDScript`, so `can_instantiate()` supplies a second signal rather than testing for null. Loading executes static initialisers ([Static state](#static-state)), which must be safe outside the game. The runner is typed because the settings also apply to it. Under `general.md`, this check is above a unit test and needs maintainer approval. It catches compilation failures and Error-level warnings in scripts no unit test loads.

Ref: `godot --headless --check-only` (one script per process; the alternative when there are few scripts).

**Checked by:** a project check.

### Exceptions to a warning

Put `@warning_ignore("one_warning")` on the narrowest statement, with a preceding comment naming the constraint code cannot show. Use one warning per annotation. On a `func`, the annotation covers only its signature and parameters, not its body (measured). Place it there only for a signature warning. Never use `@warning_ignore_start` or `@warning_ignore_restore`. An unmatched start extends to the file's end, and a region can hide new code inside it. Ignores also work at Error, so exceptions can weaken a strict warning table. The engine reports neither useless ignores nor missing reasons. A text scan therefore lists every annotation for review. Under `general.md`, the scan reports only until approved as a check by the maintainer. The engine does reject a misspelt warning name as a parse error.

```gdscript
# A majority of an even vote count is more than half, rounded down.
@warning_ignore("integer_division")
var majority := votes.size() / 2 + 1
```

A reason that describes a state or a mode ("when the chain is missing") points at a missing type, as in general.md.

Ref: `modules/gdscript/gdscript_parser.cpp` `warning_ignore_annotation` (what an ignore covers on each statement kind); `@GDScript` class reference, `@warning_ignore`.

**Checked by:** the engine (the name); a report-only text scan and review (placement and reason).

## Types and records

### Typed everything

Every variable, constant, parameter, return, `for` variable, signal parameter, lambda parameter, and lambda return has a static type. Read no member and call no method absent from the static type. Typed code receives the engine's available checks and editor completion. It also uses faster typed instructions when the analyser knows operand types. Write `Variant` only where a value genuinely has several possible types. Examples are an unread value inside a parser or an engine API that returns `Variant`.

The table shows what Error-level warnings catch and what they do not catch:

| Caught at compile time | Not caught; fails at run time or not at all (measured) |
|---|---|
| An untyped declaration, `for` variable, lambda signature | `var count: int = raw` from a `Variant`: no warning; a wrong type is a script error at run time |
| A method or property not on the static type of a `Variant` or a supertype (`unsafe_method_access`, `unsafe_property_access`) | Dot access on a typed `Dictionary` (`record.stopped`): no warning; a missing key is a script error at run time |
| A `Variant` passed to a typed parameter (`unsafe_call_argument`) | `call("name")`, `get("name")`, `set("name", …)` on a typed object: no warning |
| `as` on a `Variant` (`unsafe_cast`) | A coroutine run through `Callable.call()` without `await`: no `missing_await` |
| An enum variable without a default when `0` is not a member | An enum variable without a default when `0` is a member |

The engine enforces the first column, and review enforces the second. A typed declaration from a `Variant` follows an `is` test on the preceding line. Read a dictionary with `.get(key, default)` only when a default is intended. Never read it as a record through dot access. By-name access follows [No duck typing](#no-duck-typing). Inside `if value is T:`, the analyser does not narrow `value`. Bind it to a typed local to obtain checks on subsequent lines:

```gdscript
func length_of(value: Variant) -> float:
	if value is float:
		var metres: float = value
		return absf(metres)
	return 0.0
```

Typed global helpers replace untyped helpers. `absf`, `absi`, `clampf`, `clampi`, `lerpf`, `minf`, `maxf`, `signf`, `roundi`, `floori`, and `snappedf` return their stated type. `abs`, `clamp`, `lerp`, `min`, `max`, `sign`, `round`, `floor`, and `snapped` return `Variant`. Move an untyped result from `Array.map()` or `filter()` into a typed array with `.assign()`.

The objection to typing all GDScript is that missing type features make typed collections impractical. Without function types, `map()` returns an untyped `Array`. GDScript also lacks nested typed containers, narrowing, and unions, so a TypeScript union becomes `Variant`. The gaps table below gives the accepted workaround for each limitation. These workarounds do not provide Rust's checking. They let the engine check most code and confine unchecked constructs to identifiable locations: parsers, by-name engine APIs, and conversions after `.assign()`.

Ref: the manual's static typing page, "Global scope methods" and "Cases where you can't specify types"; ["GDScript progress report: Typed instructions"](https://godotengine.org/article/gdscript-progress-report-typed-instructions/) (the speed of typed code); ["GDScript: The Good, Bad, and Ugly Parts"](https://azhdarchid.com/gdscript-good-bad-ugly/) (the opposing case); gdUnit4's `addons/gdUnit4/src/` types all of its declarations.

**Checked by:** the engine for the first column; review for the second.

### Inference and the type on the line

`:=` is allowed wherever the analyser infers a type. `inference_on_variant` at Error guarantees that the inferred type is never `Variant`. When a reader cannot see the type on the line and the distinction matters, write the type explicitly. For example, use `var hit: Dictionary = space.intersect_ray(query)`. Also make ambiguous literals explicit. A float is `0.0`, never `0`, because `var speed := 0` infers `int` and later `speed = 0.5` assignments narrow. The style guide says "prefer `:=` when the type is written on the same line". No setting enforces that advice. `inferred_declaration` forbids `:=` almost everywhere, so review checks type visibility.

Ref: the style guide, "Static typing" and "Inferred types".

**Checked by:** the engine (no inferred `Variant`; `narrowing_conversion`); review (visibility of the type).

### A record is a class

Data with named fields passed between functions is a class, never a string-keyed `Dictionary`. Use an inner class, a `class_name` class extending `RefCounted`, or a custom `Resource` when saved or edited in the inspector. Nothing checks dictionary fields. `.get("finshed", false)` silently returns the default for a misspelt key. Dot or subscript access on a missing key causes a script error during the operation. Neither case causes a compile-time warning on a typed `Dictionary`, and each read needs a hash lookup. The parser checks class fields, the editor offers completion, `##` documents them, and their values are typed.

Before (sketch), a step result as a dictionary, so the shape of `result` is a convention between functions:

```gdscript
func stopping(delta: float) -> Dictionary:
	var result := _settle(delta)
	if result.has("error"):
		return result
	result.stopped = result.finished
	return result
```

After, the shape is a type the parser checks at every read:

```gdscript
## What one stopping step did: whether the body has stopped, and the refusal that ended it, if any.
class Step extends RefCounted:
	var stopped := false
	var finished := false
	var refusal := ""
	var transform := Transform3D.IDENTITY
```

A `Dictionary` is right in four places, and a project lists its own instances in its AGENTS.md:

- **A true map.** Use `Dictionary[K, V]` for keys and values of one type, available since 4.4. For example, `Dictionary[String, Image]` maps files to decoded images. It rejects wrong literal keys and mismatched typed dictionaries at compile time. Its value-returning methods still return `Variant`.
- **Inside a parser**, between its field readers, before the values become a record ([Parse once](#parse-once)).
- **A documented external interface.** Use a dictionary where the wire form requires one, such as a JSON protocol. Parse it into records at input and build it once from records at output.
- **Made-up raw inputs in tests** that model the boundary's own form.

Nested typed containers such as `Array[Array[int]]` and `Dictionary[String, Dictionary[String, int]]` are syntax errors. Use a record class for the inner value. Use `Dictionary[StringName, Clip]`, never `Dictionary[StringName, Dictionary]`. A class remains appropriate when it contains only a name, fields, and a constructor. Forty lines of typed fields is the reference form. A record class costs one object per instance. For tens of thousands of plain-number records, a `Packed*Array` per field is an alternative ([Performance](#performance)).

Ref: phantom-camera `addons/phantom_camera/scripts/resources/tween_resource.gd` (a typed data `Resource`); Pixelorama `src/Palette/Palette.gd:26` (`Dictionary[int, PaletteColor]`); the engine test `analyzer/errors/typed_dictionary.gd`; ["Add structs in GDScript"](https://github.com/godotengine/godot-proposals/issues/7329) (godot-proposals #7329), on why classes are heavy and dictionaries unchecked.

**Checked by:** review. The engine flags a dictionary only where its values flow into typed code through `Variant` (`unsafe_call_argument`, `unsafe_method_access`); dot access on a typed `Dictionary` compiles silently.

### Closed sets

A closed set is a named enum, never an anonymous `enum { A, B }`, bare `int`, or string compared with `==`. A named enum is a distinct static type. Assigning another enum or an anonymous enum's constant is a compile error. Give every enum variable an explicit default, even when `0` is a member. Enum values remain integers internally, so different enums can compare and add. `int_as_enum_without_cast` requires `as` for integer conversion, and `int_as_enum_without_match` rejects a constant integer absent from the enum. At the boundary, convert an external run-time integer by matching it to a member. Refuse unmatched values.

`match` is not exhaustive, and missing members produce no warning. A `match` on a project-owned enum lists every member and ends with a default arm that asserts. An added member then fails when execution first reaches a missing decision in a debug build. Names sent outside the program, such as status fields or records, use an explicit map with one entry per member. Never derive those names through reflection. `Phase.find_key(phase).to_lower()` silently changes the format when a variant is renamed.

```gdscript
enum Phase { STOPPED, PLANNING, MOVING, TURNING, SETTLING, ARRIVED }
## Each phase as the status reports it.
const PHASE_NAMES: Dictionary[Phase, String] = {Phase.STOPPED: "stopped", Phase.PLANNING: "planning",
	Phase.MOVING: "moving", Phase.TURNING: "turning", Phase.SETTLING: "settling", Phase.ARRIVED: "arrived"}

var _phase := Phase.STOPPED


## Whether the body is under way, from planning until it has arrived or stopped.
func under_way() -> bool:
	match _phase:
		Phase.STOPPED, Phase.ARRIVED:
			return false
		Phase.PLANNING, Phase.MOVING, Phase.TURNING, Phase.SETTLING:
			return true
		_:
			assert(false, "Phase has no case for %d" % _phase)
			return false
```

In debug builds, a failed `assert` prints a script error and abandons the function (measured). The checks' error filter rejects that error. Release builds strip the `assert` and return the arm's value. Those behaviours make `assert` suitable here but unsuitable for input ([Errors](#errors)). Choose the safest fallback the caller can use, never a guess at the missing case. A missing map member causes a lookup script error in both debug and release builds. Test every member's map entry.

A native enum such as `Environment.ToneMapper` has no `values()`. `ClassDB.class_get_enum_constants` returns names rather than values (measured). Test membership by mapping each name through `ClassDB.class_get_integer_constant`, or by comparison with a hand-written member list. Never use a numeric range that could silently exclude a new engine member.

Ref: the engine tests `analyzer/errors/enum_type_checking.gd` and `analyzer/warnings/enum_without_default_value.gd`; godot-demo-projects `audio/rhythm_game/game_state/note_manager.gd` `_get_curr_beat` (the `assert` default; its fallback guesses a case, which this rule does not) with `globals/enums.gd`; godot-open-rpg `src/common/directions.gd`.

**Checked by:** the engine (named enum types, `int_as_enum_*`, a missing default where `0` is no member); the checks' error filter (a missing arm or map entry, when reached); review (strings and ints where an enum fits, a missing default arm, defaults where `0` is a member).

### What typed GDScript cannot express

Use the agreed workaround for each gap, not a different workaround in each file. These are structural gaps. The maintainers' proposals for non-nullable objects, traits, structs, and a Godot 5 GDScript rewrite explain why the gaps remain in 4.x.

| Gap | What happens | Workaround |
|---|---|---|
| **No generics** | GDScript has no `Result<T, E>`, `Option<T>`, or typed callable. gdUnit4's `GdUnitResult` holds a `Variant` payload, so every caller casts. | Use a per-operation result record with typed fields ([Refusals are values](#refusals-are-values)). For a callback, use a `Callable` and document its signature in the `##` of the member that holds it. |
| **No sum types** | An enum is an integer without a payload, so a record cannot express alternative shapes. | Use a record with an enum field and the fields of every case. Document in `##` which fields each case uses. Construct it only through one static function per case, so a caller never assembles a combination by hand. The other combinations remain representable, so review keeps them out. |
| **No nullability** | Object-typed variables may be `null`. Built-in-typed variables cannot hold `null` and default to their zero. | Represent an absent object as `null` and test it where it is returned. Put an absent built-in value in a record field that is `null` when absent. Never use a sentinel such as `-1`, `INF`, or `Vector3.ZERO`. A single refusal is a `String` that is empty when nothing was refused, because a refusal is never empty text. The function's `##` says so. |
| **No interfaces** | GDScript has no traits or structural typing and supports only single inheritance. | Use an `@abstract` class with `@abstract func`s, available since 4.5, extended by each implementation. Never use `has_method` ([No duck typing](#no-duck-typing)). As in rust.md, an abstract base needs more than one implementation or a dependency that it inverts. |
| **Invariant typed arrays** | `Array[Node3D]` does not fit `Array[Node]`. An untyped `Array` holding integers does not fit `Array[int]` when assigned at run time. | Fill a typed array with `.assign()`, which copies and converts. Build typed containers through their typed field, never by assigning an untyped variable to them. |
| **`as` returns null** | `node as Node3D` returns `null` on mismatch without an error or warning. | If the value must have a particular type, use a typed declaration such as `var mesh: MeshInstance3D = node`, so a mismatch causes a run-time error. Use `as` only when `null` is an expected outcome, and test it on the next line. For a `Variant`, test with `is` first, then assign to a typed local. |
| **No exhaustive `match`** | A missing enum member falls through a `match` without a diagnostic. | Use the default arm that asserts ([Closed sets](#closed-sets)). |
| **Untyped signal emission** | The analyser does not check `emit` arguments against the declaration. A wrong type fails at the receiver at run time (measured). | Declare typed signal parameters as documentation. Emit each signal from one place, with typed locals ([Signals](#signals)). |
| **No typed nested containers** | `Array[Array[int]]` is a syntax error | A record class for the inner value |
| **No private members** | A leading underscore is a convention. | Methods keep the invariants, and review checks those methods ([Invariants](#invariants)). |
| **Freed objects** | A reference to a freed node compares `== null` and still has `TYPE_OBJECT`. Applying `is` to it raises an error (measured). | Where `null` has meaning, test `typeof(value) == TYPE_NIL`. Enter an object branch only after `typeof(value) == TYPE_OBJECT` and `is_instance_valid` pass, before any class test, in one helper that every caller uses. |

The style guide and static typing page disagree about `as` versus a typed declaration. The style guide calls `get_node(...) as ProgressBar` "more type-safe". The static typing page calls a typed declaration "more reliable" because a mismatch "will be detected immediately". It says `as` "will be silently cast to null". A project that requires an explicit mismatch error uses the typed declaration.

Ref: the manual's static typing page ("Type casting", "Safe lines") and GDScript reference ("Typed arrays", "Abstract classes and methods"); godot-open-rpg `src/combat/actions/battler_action.gd` (an `@abstract` resource as an interface); gdUnit4 `addons/gdUnit4/src/core/GdUnitResult.gd` (how far a result goes without generics, and where it stops); ["Make objects non-nullable by default"](https://github.com/godotengine/godot-proposals/issues/11054) (godot-proposals #11054), on GDScript's three kinds of null.

**Checked by:** the engine (`unsafe_cast`, a nested typed container); review for the rest.

### Invariants

A class establishes its invariant in one constructor and preserves it on every path. GDScript has no private members, so methods enforce the invariant by convention. Prefix affected fields with underscores and read them through accessors. Make every change through a method that preserves the invariant. Construct the value through a `static func` that returns an instance or refusal, never through `new()` followed by optional caller assignments. A caller can still write `_low` directly, so review must prevent that access. State this limitation in the class's `##`. Where the engine constructs a class, as with inspector-created `Resource` objects or `duplicate()`, `_init` parameters need defaults. For those classes, the static function is the checked constructor. An exported property preserves its invariant through a setter, including changes from the inspector.

```gdscript
## A hinge's limits in radians; the low limit never exceeds the high one. Read and change the limits only through
## these methods.
class Limits extends RefCounted:
	var _low := 0.0
	var _high := 0.0

	static func spanning(first: float, second: float) -> Limits:
		var limits := Limits.new()
		limits.widen_to(first, second)
		return limits

	func low() -> float:
		return _low

	func high() -> float:
		return _high

	## Sets the limits to span both angles, whichever order they come in.
	func widen_to(first: float, second: float) -> void:
		_low = minf(first, second)
		_high = maxf(first, second)
```

Ref: godot-open-rpg `src/field/gameboard/gameboard_properties.gd` (a setter enforcing its own invariant and emitting a change signal); gdUnit4 `addons/gdUnit4/src/core/GdUnit4Version.gd` (`static func parse` and private fields).

**Checked by:** review.

## Parsing at boundaries

### Parse once

This section applies `general.md`'s boundary rule to GDScript. Parse authored files, build records, JSON requests, and native-library values into typed objects where they enter. Every refusal names the file, line, and field there. The boundary is a parser component, not a single function. Its parser object and field readers may exchange raw values. No raw `Dictionary` or `Variant` leaves that component for a consumer. Collect refusals with a field path such as `slots.left.priority` and a line when the reader supplies one. `JSON.parse` reports syntax-error lines through `get_error_line()`, but does not report value locations. For JSON values, the dotted field path supplies the location. `JSON.parse_string` returns `null` on failure without a reason. Use a `JSON` instance.

Check the `Variant` before assignment. If `text` contains an array, `var parsed: Dictionary = JSON.parse_string(text)` fails at assignment with a script error. The refusal never runs. Test a field value's type before binding it to a typed local. That order also satisfies `unsafe_call_argument`:

```gdscript
## A whole number: an int, or a JSON number with no fraction within 2^53; otherwise refused at [param at].
func whole(value: Variant, at: String) -> int:
	if value is int:
		return value
	if value is float:
		var number: float = value
		if is_finite(number) and number == floorf(number) and absf(number) <= 9007199254740992.0:
			return int(number)
	refuse(at, "must be a whole number")
	return 0
```

Read a disk record once into its class. Represent a nested optional part with a record that is `null` when absent. Pass the resulting class to consumers. No consumer reads `record.get("failure", {}).get("cache_key")`.

Ref: gdUnit4 `addons/gdUnit4/src/cmd/CmdArgumentParser.gd` (one entry point, refusals returned as a result rather than printed; its payload is still an untyped `Array`); godot-demo-projects `networking/webrtc_signaling/server/ws_webrtc_server.gd:204-217` (checking parsed JSON's shape, and the assign-before-check trap at `:206-207`); Pixelorama `src/Classes/Project.gd:458-495` (the anti-pattern: parsing and building interleaved, nested JSON read by dot access mid-load). For example, in tdollhouse: `GFParse` (`addons/dolls/contract/parse.gd`) has the shape: one parser per file, `refuse` naming file and field path, `record` refusing unknown fields.

**Checked by:** the engine, partly (`unsafe_call_argument` where a field reader passes a `Variant` on untested); unit tests of each refusal on made-up input; review (that no raw value leaves the parser).

### JSON numbers are floats

JSON parses every number as `float`. Membership is type-strict, so `400 in [400.0]` is false (measured), and a `Dictionary` keyed by `int` misses a `float` key. Check and convert a whole-number field once at the boundary, as `whole()` does above. JSON cannot retain `Vector3`, `Color`, `StringName`, or class values. The writer and parser agree on one encoding per type, such as three numbers for a vector. Each new field in a record the program reads must have an explicit type expressible in plain JSON.

Ref: the `JSON` class reference; the manual's "Saving games", "JSON limitations".

**Checked by:** unit tests on made-up records; review.

### Typed exports are not validation

When `ResourceLoader` loads a `.tres` or `.tscn`, it constructs each stored value. It converts containers to their declared type and sets each value through the script's property setter. Scalars convert when the engine supports the conversion. Otherwise, the default remains. The following results were measured on 4.7.1 with a hand-written `.tres`:

| Stored value | Export | Result |
|---|---|---|
| `"bad"` | `int` (default 7) | `0`, silently: a `String` converts to an `int` by parsing |
| `"x"` | `float` | `0.0`, silently |
| `"false"` | `bool` (default `true`) | `true`, silently: no `String`-to-`bool` conversion, so the value is refused and the default kept |
| `"nope"` | `Vector3` (default `ONE`) | the default, silently |
| `99` | an enum with two members | `99`, silently |
| `[1, 2]` | `Array[int]` (default `[9]`) | `[1, 2]`: converted |
| `Array[float]([1.0, 2.5])` | `Array[int]` | `[1, 2]`, silently: converted element by element, losing data |
| `[1.5, "x"]` | `Array[int]` | `[]`, with `ERROR: Unable to convert array index 1 from "String" to "int"` |
| `Array[String](["a"])` | `Array[int]` | `[]`, with `ERROR: Cannot assign contents of "Array[String]" to "Array[int]"` |
| `{"a": "x"}` | `Dictionary[String, int]` | `{}`, with `ERROR: Unable to convert value at key "a" from "String" to "int"` |

A typed `@export` documents intent but does not protect hand-written file values. A wrong scalar silently becomes zero, the default, or an out-of-range value. A convertible container converts silently. Only an inconvertible container produces an engine error. A check catches that error only when it loads the file. `@export_range`, `@export_flags_*`, and enum hints constrain the inspector, not file values.

A supplied resource can contain code. A `.tres` can embed a GDScript sub-resource as its script. That script's `_init` runs inside `ResourceLoader.load`, before the caller can inspect the loaded resource (measured). The resource can also reference external resources by path.

These conversion and code-execution risks determine when typed exports are safe to load. Safety here does not prove domain invariants, which the class must still preserve. The following cases distinguish safe and unsafe sources:

- **Safe generated resource.** Only the project's own tool, such as a bake or build, writes the resource from parsed values. It is typed, trusted generated output under `general.md`. Typed exports load what the tool wrote.
- **Safe inspector-edited resource.** A person edits the resource only in the inspector, which writes declared types.
- **Unsafe text-edited resource.** A person writes or edits the resource as text, and the program loads it through `ResourceLoader`. The resource itself is the authored boundary. Its exports are `Variant`, with `##` comments stating authored types. A `validate()` refuses each incorrect type by field before anything reads the values. The root fix is a plain record parsed into typed resources by the project's parser, as for any authored file. Keep the `Variant` form only while the project itself authors the resource.
- **Never load an external resource through `ResourceLoader`.** External resources include downloads, users' saves, and imported packages. Outside data uses a plain format parsed by the project, never a resource passed to `ResourceLoader`.

Never edit generated resources by hand. Never add a hand-written resource where a typed generated one exists.

Ref: the engine, `scene/resources/resource_format_text.cpp` (container conversion while loading) and `modules/gdscript/gdscript.cpp` `GDScriptInstance::set`; the manual's "Resources", "Creating your own resources"; the [godot-safe-resource-loader](https://github.com/derkork/godot-safe-resource-loader) README, on embedded scripts.

**Checked by:** unit tests of `validate()` on made-up resources; the error filter, for an inconvertible container in a resource a check loads; review.

### Resources are shared

Loading a resource twice from one path returns one object, so every user sees every mutation. A user that changes a loaded resource changes its own copy made with `duplicate()`. `duplicate()` is shallow. `duplicate(true)` copies nested arrays, dictionaries, and internal sub-resources. Sub-resources saved in their own files remain shared with the original (measured). A deep copy therefore does not isolate external sub-resources. Copy the resources that will change. State which parts remain shared in `##`. An inner class extending `Resource` does not serialise its properties. A saved resource is its own file.

Ref: `Resource.duplicate` and `Resource.duplicate_deep` in the class reference; the manual's "Resources", "Nodes and resources".

**Checked by:** review.

## Errors

GDScript has no exceptions. A run-time script error prints `SCRIPT ERROR`, abandons the function, and returns `null` or the zero value. Examples include wrong-type assignments, missing keys, and calls on `null`. The caller continues without notification of the failure (measured). Return a failure as a value when a caller must handle it. For a bug, produce an error in debug builds, where checks run. Its printed error line fails the checks.

### Refusals are values

Decide an error's form by who can fix it, as `rust.md` does.

- **The author or environment can fix it.** Return a refusal value for an authored file, missing file, failed process, or external request. One option is a record with an `errors: PackedStringArray` field. Set it through a named static constructor, such as `static func refused(reasons: PackedStringArray) -> ImportInput`, so construction occurs in one place. Alternatively, return the typed record or `null` from a parse function and store reasons in the caller-supplied parser object. A program-matched refusal, such as an interface refusal or protocol error, carries a stable `snake_case` code beside its message in separate fields. Never read a code back from message text. A human-readable refusal names the file, line, and field.
- **Our own code breaks a promise.** Use a type that rules out the failure. Otherwise, use `assert` with a message. Its error fails checks, and a release build strips the assertion.
- **Accepted input fails numerically.** Return an outcome that the caller handles, never a refusal or silent `NaN`. Cases include a non-finite result or a degenerate input. A comparison with `NaN` is false, so `if error > limit:` accepts `NaN`. Test `is_finite` where the value is produced.

Never return a `Variant` containing either the value or a refusal string, distinguished with `is String`. The following sketch shows the rejected form:

```gdscript
static func for_limb(skeleton: Skeleton3D, limb: String) -> Variant:
	var chain := Chain.new()
	var error := chain.configure(skeleton, limb)
	if not error.is_empty():
		return error
	return chain
```

After:

```gdscript
## A limb's chain installed on its skeleton, or the refusal that leaves the limb off; [member chain] is null exactly
## when [member refusal] is not empty.
class Installed extends RefCounted:
	var chain: Chain
	var refusal := ""
```

A refused record has the same type as a complete one. GDScript cannot express that distinction as a type. The record's `##` says which fields are meaningful when `errors` is empty. Callers test `errors` first.

Ref: gdUnit4 `addons/gdUnit4/src/core/GdUnitResult.gd` (a result object, with its `int` state as the weakness to avoid: use a named enum).

**Checked by:** the engine, partly (`unsafe_*` on a `Variant` result); review.

### Report at the host's edge

`push_error` belongs at the host that decides which refusals reach a person. Hosts include editor plugins, shipped scenes, test benches, and command-line entry points. A library function never both logs a refusal and returns `null`. That combination gives its caller no reason and leaves only a log line. `printerr` and `print` are not error channels.

Before (sketch):

```gdscript
static func assemble(parent: Node3D, config: AssemblyConfig) -> MeshInstance3D:
	if config == null:
		push_error("assemble: no config")
		return null
```

After: `assemble` returns an `Assembled` record with `errors`, and the host that called it pushes the errors it decides to show.

**Checked by:** review; a text scan of `push_error` outside hosts is a report-only candidate.

### Assert is never for input

Release builds remove `assert`, including its condition and message. Never use it to check input, cause a side effect, or provide the only protection before using an invalid value. It states a promise internal to the code, such as an asserting default arm or a precondition between one class's functions. In tests, use the probe's check helper to report failure. `assert` is not a test assertion.

Ref: the GDScript reference, "Assert keyword".

**Checked by:** the engine (`assert_always_*`); review.

### Engine errors are checked

An engine call that can fail reports failure as an `Error`, `null` with a separate reason, or an empty value. `ResourceSaver.save`, `DirAccess.make_dir_recursive_absolute`, and `JSON.parse` return `Error`. `FileAccess.open` returns `null`, with its reason available through `FileAccess.get_open_error()`. Bind the result at the call site. Test it there. Convert failure into a refusal naming the path. With `return_value_discarded` off, review must catch a discarded `Error`. A signal connection's `Error` and an `Array.append`'s `bool` may be discarded.

**Checked by:** review.

## Classes, files, and organisation

### One job per class

A script is a class. Each class has one job stated by its first `##` line. Cooperating inner classes may share a file, such as a record and parser or a check and its cases. A large job may occupy a large file. `general.md` imposes no size thresholds. Merge a file too small to stand alone into its owner. Group flat directories by subject. Behaviour that reads one class's data is a method on that class. A pure function over plain values is a `static func` in the class whose job it serves.

Data classes extend `RefCounted`, which is freed when its last reference ends. Use `Resource` only for data saved or edited in the inspector. Use `Node` only for an object in the scene tree. Never use bare `Object`, which needs manual `free()`. A class that needs an autoload or scene tree cannot be tested alone. Pass collaborators through `configure(...)` or a scene-set `@export var x: Type`, rather than looking them up.

Ref: the manual's "When and how to avoid using nodes for everything" and "Scene organisation"; godot-open-rpg `src/field/gameboard/pathfinder.gd:20-38` (the anti-pattern: a data class connecting to autoloads in `_init`); phantom-camera's 3,000-line `phantom_camera_3d.gd` (2D and 3D near-duplicates: the cost of not splitting a job).

**Checked by:** review.

### Names and order

Files and folders use `snake_case` because exports are case-sensitive even when the editor's file system is not. A file need not repeat its class prefix. Classes and nodes use `PascalCase`. Functions and variables use `snake_case`. Constants and enum members use `CONSTANT_CASE`. Enums use singular `PascalCase`. A leading underscore marks a member private to its class or a virtual called by the engine. Another instance of that class may use it. Signals use past-tense names, such as `door_opened`. Within each member group, put public members before private members. Engine virtuals include `_init`, `_enter_tree`, `_ready`, `_process`, and `_physics_process`. Follow the style guide's member order:

1. `@tool`, `@icon`, and `@abstract`
2. `class_name`
3. `extends`
4. The `##` class documentation
5. Signals
6. Enums
7. Constants
8. Static variables
9. `@export` variables
10. Other variables
11. `@onready` variables
12. `_static_init`
13. Static methods
14. Engine virtuals
15. Overridden methods
16. Other methods
17. Inner classes

[GDScript-formatter](https://github.com/GDQuest/GDScript-formatter) governs layout, as rustfmt does for Rust. Pin it to an exact release like the engine. It decides indentation, spacing, blank lines, wrapping, quotes, and trailing commas in wrapped literals. Never adjust its output by hand. It follows the style guide except for `general.md`'s 120-column limit. It reads `.editorconfig` settings (measured), whose canonical form is [templates/godot/.editorconfig](../templates/godot/.editorconfig). Those settings use tabs, 120 columns, and double quotes, except for strings containing double quotes, which keep single quotes. Do not use `--reorder-code`. The formatter does not reorder members or rewrite expressions. Review therefore enforces `and`, `or`, and `not` instead of `&&`, `||`, and `!`, and `0.5` instead of `.5`. Run `--check` over every script in the project's checks. On adoption, reformat all scripts in one commit containing nothing else. A line the formatter cannot wrap, such as a long string, stays long. A project with a line-length check must keep that check.

Ref: the style guide, "Naming conventions", "Code order" and "Formatting"; the formatter's `--help`, and its `--debug`, which prints the settings it read for each file.

**Checked by:** the formatter's `--check` in the project's checks (layout); review for names, order and the expressions the formatter leaves alone.

### class_name and inner classes

`class_name` registers a type in a global namespace shared with the engine, addons, and other project classes. A shipped class name therefore carries the project's prefix. `shadowed_global_identifier` detects a local that reuses a global name, not competing script class names. Every script used as a type by another file has a `class_name`. Entry scripts, such as checks or `--script` commands, have none and load by path, keeping them out of the global class cache. A test helper is used as a type through `const Helper := preload("res://…")`. A Godot 5 proposal removes `preload`. Such a migration would affect each of those constants.

An inner class implements a record or helper used by its outer class and that class's callers. External callers name it through the outer class. An inner class never extends `Resource` for saving. A script naming a native-extension class fails to load without the library. Confine such access through `ClassDB` to as few files as possible. List those files in the project's `AGENTS.md`.

Ref: the GDScript reference, "Registering named classes" and "Inner classes"; ["Migrate GDScript to the GDExtension mechanism"](https://github.com/godotengine/godot-proposals/issues/14652) (godot-proposals #14652), for `preload`'s removal.

**Checked by:** the engine (a duplicate global name is an error); review.

### No duck typing

Where the type is known, do not use by-name access that the analyser cannot see, except for the two documented cases below. The prohibited forms are `has_method`, `call("name")`, object `get("name")` or `set("name")`, `emit_signal(name)`, and `connect(name, …)`. A rename or typo can compile and fail only at execution. A missing method or property causes a script error, while a guarded `has_method` can skip the call. Test and bind the type with `is`, or declare an `@abstract` base. Two by-name cases are legitimate and require a comment explaining why. One is an engine API with path keys, such as `AnimationTree.set("parameters/…")`. The other is a native-extension class that a file may not name.

Before (sketch), where every node of that name is known to be a `MaskRig`:

```gdscript
for found: Node in root.find_children("MaskViewport", "SubViewport", true, false):
	if found.has_method("refresh"):
		found.call("refresh")
```

After, a node of that name that is not a `MaskRig` is a script error the checks see, not a silent skip:

```gdscript
for found: Node in root.find_children("MaskViewport", "SubViewport", true, false):
	var rig: MaskRig = found
	rig.refresh()
```

Where a node with that name may have another type, use `as`. Test the result on the next line.

Ref: ["A Peek Under the Hood: Technical Learnings from Halls of Torment"](https://www.youtube.com/watch?v=3BIteSVcO28) (GodotFest 2025): a shipped game's component system on `has_method` and signals by name, and what it cost (names kept consistent by hand, unchecked signatures, lookups slow enough to need caching); godot-open-rpg `src/combat/battlers/battler_stats.gd:128-136` (`get("base_" + prop_name)`, guarded by `assert`).

**Checked by:** the engine for calls through a `Variant` (`unsafe_method_access`) and for a signal emitted only by a non-constant name (`unused_signal`; a constant name counts as a use); review for `call`, `get`, and `set` by name on a typed object, which compile silently.

### Signals

Declare typed signal parameters. Emit signals with `name.emit(...)` and connect them with `name.connect(callable)`. The analyser does not check `emit` arguments against the declaration. Emit each signal from one place using typed local arguments. Type connected lambdas, as in `func(count: int) -> void:`. A signal payload is a record class, not a `Dictionary`, except on a documented external interface. A signal declared in one class but emitted only from another carries `@warning_ignore("unused_signal")` with its reason.

Emission is synchronous. Connected callables run in connection order before `emit` returns. A handler can re-enter the emitter, so handlers determine the emitter's state after emission. Change state before emitting. Never rely on that state afterwards. A connection to an object's method ends when the object is freed. If a callable outlives an object it reads, such as a lambda capturing a node, its creator disconnects it when that object's lifetime ends. Parents initiate work by calling children. Children report events by signalling parents.

Before (sketch), outcome signals relayed by a name the analyser cannot see:

```gdscript
lifecycle.event.connect(func(name: String, id: int, record: Dictionary) -> void: emit_signal(name, id, record))
```

After: the lifecycle reports a `State` enum, and one `match` emits each signal by its own name (`State.COMPLETED: completed.emit(id, record)`), with the loud default arm.

Ref: the manual's "Using signals" and the GDScript reference, "Signals"; `Signal.connect` (a connection is lost when its callable's object is freed); godot-demo-projects `audio/rhythm_game/game_state/note_manager.gd:4-6` (typed signals with enum-typed parameters).

**Checked by:** the engine (`untyped_declaration` on signal and lambda parameters; `unused_signal`); review.

## Lifetime, concurrency, and the editor

### Ownership of nodes and objects

Each node has one lifetime owner. Its creator owns it until it enters the tree, where its parent owns it. The creator frees a node that was never added or was removed with `remove_child`, which does not free it. Forgotten orphans appear in the engine's leaked-instance report at exit. `Node.owner` instead determines which nodes a scene saves. Set it only for nodes belonging in a saved scene.

Use `queue_free()` by default. It removes the node at frame end, after callbacks that hold it. `free()` is immediate and applies only to objects nothing else can hold. Freeing an object during signal emission or a call reports `Object is locked and can't be freed` and leaves it alive (measured). Test freed-node references as the gaps table specifies. A `RefCounted` cycle, such as records referring back to each other, is never freed. Use `weakref()` on one side or clear the ordinary reference during teardown.

Ref: `Node.queue_free`, `Node.owner`, `Object.free`, and `RefCounted` in the class reference; the manual's "Nodes and scene instances".

**Checked by:** the engine's leak report at exit and the error filter (a locked free); review.

### Coroutines

A function containing `await` is a coroutine. Calling it without `await` runs it detached. `missing_await` at Error refuses that call unless it has `@warning_ignore("missing_await")` and a comment explaining the detachment (measured). A call through `Callable.call()` escapes that check. Name the detached coroutine's owner in the comment. The owner must end the coroutine or outlive it.

Across an `await`, the tree, collaborators, and current request may change. Before using them after resumption, check that they still exist with `is_instance_valid` and belong to the same request. A coroutine waiting for a freed object's signal never resumes, and nothing reports the failure (measured). Await only signals from objects that outlive the coroutine, unless whatever frees the object also ends the coroutine. A script error after `await` abandons the coroutine without reaching a caller. Only the error filter detects it.

Initialisation runs member defaults, `_init`, exported values, `@onready`, and `_ready`, in that order. An export read in `_init` still has its default. In an `extends SceneTree` script's `_init`, `root` exists but is not yet inside the tree (measured). Initialisers do not run setters. Later assignments do, even inside the class.

Ref: the GDScript reference, "Awaiting signals or coroutines" and "Initialization order"; the manual's "Godot notifications".

**Checked by:** the engine (`missing_await`, `redundant_await`, `get_node_default_without_onready`, `onready_with_export`); the error filter; review (detached owners, checks after resuming).

### Threads

A `Thread` or `WorkerThreadPool` worker takes plain inputs and returns a plain result. It touches no active-tree node, rendering or GPU state, or resource or object the main thread may hold. Exceptions require the manual's "Thread-safe APIs" to declare the API safe. The main thread collects the result, including refusals, by polling or through `call_deferred`. Only the main thread publishes it to the tree. An owner waits for every started thread or task, no later than when the owner leaves the tree. A worker never waits for main-thread work, preventing deadlock during that wait. Treat worker failures, including a failed start, as refusals.

For example, in tdollhouse: character refresh reads and decodes each source on one `Thread`. Scene and GPU construction remain on the main thread. A failed `Thread.start` becomes a refused input. `_exit_tree` waits for the thread.

Ref: the manual's "Using multiple threads" and "Thread-safe APIs".

**Checked by:** review.

### Static state

A `static var` lasts as long as its script. In 4.7, that is the whole process, regardless of `@static_unload`. Checks run in one process share that state. Each static variable has an owner that documents its reset time in `##`. A check that changes a static variable restores it. Static initialisers run on script load (measured), including compile-check loading. They must need no running game, node, file write, or possibly absent engine singleton. A Godot 5 proposal allows only constant static initialisers and removes `_static_init`. Keep static initialisers and `_static_init` small.

Ref: the GDScript reference, "Static variables" and "Static constructor"; godot-proposals #14652.

**Checked by:** review.

### Tool scripts

A `@tool` script runs in the editor. Its `_init`, setters, `_ready`, and `_process`, and those of its resources, can run when a scene opens or a resource is inspected. Every subclass is also `@tool`, enforced by `missing_tool`. Guard game-only behaviour with `Engine.is_editor_hint()`. Never write a saved scene, resource, or file merely because the script was opened or inspected. Never free or reparent nodes owned by the edited scene. Create permitted editor changes, such as previews or unsaved generated children, without `owner`, so they are not saved. Tool-script errors appear only in the editor unless a check loads the script too.

Ref: the manual's "Running code in the editor".

**Checked by:** the engine (`missing_tool`); review.

## Documentation

This section applies `general.md`'s documentation rule to GDScript. After `extends`, each class starts with a `##` comment containing a one-line brief. Then describe its job, what it never knows, and its design at the required length. Every public member of a class used by other code has a `##` comment. Public means no leading underscore. Document its purpose, invariants, units, and failure behaviour, including refusal returns or `errors` values. `##` supplies editor help and export tooltips. BBCode references such as `[method read]`, `[member errors]`, and `[param at]` create links. Documentation comments have no line cap. Comments inside a function body or top-level script code are at most six lines under `general.md`. A project's comment check exempts `##`. Comment a private member with `#` only for a constraint the code cannot show.

A comment stating a type, such as "float ≥ 0 — key light energy" above a `Variant` export, indicates a type that admits invalid values. Where `Variant` is deliberate ([Typed exports are not validation](#typed-exports-are-not-validation)), the `##` states the authored type and the refusal that enforces it.

```gdscript
class_name ValueParser
extends RefCounted
## Reads loose authored values into typed ones; every refusal names the file and the field.
##
## One instance reads one file. Field readers refuse what they cannot read and return a neutral value, so a caller
## reads every field and then tests [method ok].

## The refusals so far, each "file: field.path: message", without duplicates.
var errors: PackedStringArray = []
```

Ref: the manual's "GDScript documentation comments"; godot-open-rpg `src/combat/actions/battler_action.gd` (a `##` class doc and BBCode references, though not on every member).

**Checked by:** a project's comment-budget check (the in-body cap); review (presence and content).

## Tests

### Plain headless probes

Tests are plain headless scripts using `extends SceneTree`, one check per file named for its subject. Put cases in an inner class whose `run()` returns an exit code. Failure lines state the broken promise, such as "a valid record refused", never a value dump. A runner may batch checks in one process if it restores static state between them ([Static state](#static-state)). Use no test framework. GUT and gdUnit4 mainly add doubles, partial doubles, and stubs that test calls rather than function promises. A framework also adds hundreds of vendored scripts to scan or exclude, another runner and output format, and a dependency pinned per engine minor. Plain scripts lack readable diffs and orphan reports. A small helper supplies readable diffs. Compare `Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT)` before a case and after its deferred frees for an orphan report. That report covers detached nodes only, not nodes leaked inside the tree.

**Checked by:** the project's checks.

### The error filter

A test passes only if the engine prints no unexpected error. Each `ERROR:` or `SCRIPT ERROR:` line fails the check unless the case declares that error as intentional. Script errors abandon functions and return defaults that can resemble success ([Errors](#errors)). Match declared errors to printed errors one to one. An expected error must neither excuse another error nor go missing unnoticed. A check that crashes or does not report fails.

**Checked by:** the project's checks.

### Inputs and expectations

This section applies `general.md`'s test rules to GDScript. Unit tests build small made-up inputs inline, such as a `const` record or case table, only as large as the promise needs. Real inputs belong to higher test layers. Write expected values by hand from the input's source, a specification, or a law. Authored values include counts from made-up inputs, specified lengths, and tolerances at perceptible differences. Never put a number copied from a run into a check. Compare exact wording when it is the promise, such as `"made_up.json: slots.left.priority: must be a whole number"`. A reviewed snapshot of exact text or structured output whose shape is the promise also counts as authored. Print measurements intended for people as report lines for the project's change report. A case table identifies the failed row. Keep tests for promises real data relies on and name them for those promises. Do not write checks of other checks.

Before (hypothetical), a value read off a run: `_check(clip.length == 1.2333, "length")`. After, the value the made-up clip was built with: `_check(is_equal_approx(clip.length, FRAMES / RATE), "a clip lasts its frames over its rate")`.

Ref: gdUnit4 `addons/gdUnit4/test/cmd/CmdArgumentParserTest.gd` (invented argument lists, one behaviour per case, refusals beside acceptances); beehave `test/nodes/decorators/inverter_test.gd` with `test/actions/count_up_action.gd` (a hand-written fake whose status the test sets).

**Checked by:** review.

## Performance

The general.md performance rules apply. These are the costs particular to GDScript.

- **Typed code is faster.** The analyser emits typed instructions for operands whose types are known at compile time. Untyped arithmetic and calls use `Variant` dispatch. The gain is greatest for engine calls and smallest for attribute access.
- **Records are checked. Dictionaries are not.** Records primarily improve checking and completion. A dictionary read also hashes a key. A property read through `Object.get` or `Variant` searches the script and ClassDB at each inheritance level.
- **Use packed arrays for bulk numbers.** `PackedFloat32Array`, `PackedVector3Array`, and the other packed arrays are contiguous and smaller than an `Array` of the same values.
- **Every call costs.** Per-entity `_process` calls across thousands of nodes cost more than one loop over those nodes.
- **Per-frame code allocates nothing it can keep.** `_process`, `_physics_process`, and their callees reuse class-owned arrays and records rather than constructing a `Dictionary` or `Array` each frame. Resolve `get_node` and `$` once into `@onready` members. Use `StringName` literals, such as `&"parameters/blend"`, for repeated name lookups. Format strings per frame only for diagnostics.
- **Heavy maths does not belong in GDScript.** There is no JIT. A solver, search, or per-vertex loop moves behind a typed native-library binding when a profile identifies it on a hot path. Measure per-frame binding calls too, since crossing the binding has a cost.

Before making a performance claim, measure the path with the engine profiler or `Time.get_ticks_usec()`. Report the result. A timing never fails a check.

Ref: the manual's static typing page and "CPU optimisation", "GDScript"; "GDScript progress report: Typed instructions"; the Halls of Torment talk (batching `_process`, moving hot code to native, measuring the binding).

**Checked by:** review and the project's performance report.

## For a new project: not yet decided

A project answers these open questions in its `AGENTS.md` when it needs the relevant capability, with prior art. These are not rules yet.

- **A naming linter.** The engine checks no naming convention. The formatter's `lint` subcommand does. Its `private-access` rule treats `_name` as object-private, unlike this standard.
- **A test framework.** Decide whether doubles are needed for behaviour that plain probes cannot test.
- **Autoloads.** A project without autoloads forbids them. A project that needs global state prefers a `static func` library, static variables, or a shared resource, as the manual advises.
- **C# or another scripting language.** Decide whether to use one alongside GDScript.
