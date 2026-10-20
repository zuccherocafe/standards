# GDScript standard: Design

Part of the [GDScript standard](README.md). Types, records, closed sets, invariants, and parsing at boundaries.

## Types and records

### Typed everything

*Always.* Every variable, constant, parameter, return, `for` variable, signal parameter, lambda parameter, and lambda return has a static type. Read no member and call no method absent from the static type. Typed code receives the engine's available checks and editor completion. It is also faster ([Performance](performance.md#performance)). Write `Variant` only where a value genuinely has several possible types. Examples are an unread value inside a parser or an engine API that returns `Variant`.

The table shows what Error-level warnings catch and what they do not catch:

| Caught at compile time | Not caught; fails at run time or not at all (measured) |
|---|---|
| An untyped declaration, `for` variable, lambda signature | `var count: int = raw` from a `Variant`: no warning; a wrong type is a script error at run time |
| A method or property not on the static type of a `Variant` or a supertype (`unsafe_method_access`, `unsafe_property_access`) | Dot access on a typed `Dictionary` (`record.stopped`): no warning; a missing key is a script error at run time |
| A `Variant` passed to a typed parameter (`unsafe_call_argument`) | `call("name")`, `get("name")`, `set("name", …)` on a typed object: no warning |
| `as` on a `Variant` (`unsafe_cast`) | A coroutine run through `Callable.call()` without `await`: no `missing_await` |
| An enum variable without a default when `0` is not a member | An enum variable without a default when `0` is a member |

The engine enforces the first column, and review enforces the second. A typed declaration from a `Variant` follows an `is` test on the preceding line. Read a dictionary with `.get(key, default)` only when you want a default, and never read it as a record through dot access. By-name access follows [No duck typing](organisation.md#no-duck-typing). Inside `if value is T:`, the analyser does not narrow `value`. Bind it to a typed local so the analyser checks later lines:

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

**Checked by:** the engine for the first column; review for the second, and the bare-type scan for bare types in public signatures.

### Inference and the type on the line

*Default.* `:=` is allowed wherever the analyser infers a type. `inference_on_variant` at Error guarantees that the inferred type is never `Variant`. When a reader cannot see the type on the line and the distinction matters, write the type explicitly. For example, use `var hit: Dictionary = space.intersect_ray(query)`. Also make ambiguous literals explicit. A float is `0.0`, never `0`, because `var speed := 0` infers `int` and later `speed = 0.5` assignments narrow. The style guide says "prefer `:=` when the type is written on the same line". No setting enforces that advice. `inferred_declaration` forbids `:=` almost everywhere, so review checks type visibility.

Ref: the style guide, "Static typing" and "Inferred types".

**Checked by:** the engine (no inferred `Variant`; `narrowing_conversion`); review (visibility of the type).

### A record is a class

*Default.* Data with named fields passed between functions is a class, never a string-keyed `Dictionary`. Use an inner class, a `class_name` class extending `RefCounted`, or a custom `Resource` when saved or edited in the inspector. Nothing checks dictionary fields. `.get("finshed", false)` silently returns the default for a misspelt key. Dot or subscript access on a missing key causes a script error during the operation. Neither case causes a compile-time warning on a typed `Dictionary`. The parser checks class fields, the editor offers completion, `##` documents them, and their values are typed.

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

A `Dictionary` fits four cases, and a project lists its own uses in `AGENTS.md`:

- **A true map.** Use `Dictionary[K, V]` for keys and values of one type, available since 4.4. For example, `Dictionary[String, Image]` maps files to decoded images. It rejects wrong literal keys and mismatched typed dictionaries at compile time. Its value-returning methods still return `Variant`.
- **Inside a parser.** Field readers may exchange a dictionary before the values become a record ([Parse once](#parse-once)).
- **A documented external interface.** Use a dictionary where the wire form requires one, such as a JSON protocol. Parse it into records at input and build it once from records at output.
- **Made-up raw inputs in tests.** Tests may pass a dictionary to model the boundary's raw input.

Nested typed containers such as `Array[Array[int]]` and `Dictionary[String, Dictionary[String, int]]` are syntax errors. Use a record class for the inner value. Use `Dictionary[StringName, Clip]`, never `Dictionary[StringName, Dictionary]`. A class remains appropriate when it contains only a name, fields, and a constructor. Forty lines of typed fields is the reference form. A record class costs one object per instance. For tens of thousands of plain-number records, a `Packed*Array` per field is an alternative ([Performance](performance.md#performance)).

Ref: phantom-camera `addons/phantom_camera/scripts/resources/tween_resource.gd` (a typed data `Resource`); Pixelorama `src/Palette/Palette.gd:26` (`Dictionary[int, PaletteColor]`); the engine test `analyzer/errors/typed_dictionary.gd`; ["Add structs in GDScript"](https://github.com/godotengine/godot-proposals/issues/7329) (godot-proposals #7329), on why classes are heavy and dictionaries unchecked.

**Checked by:** the bare-type scan and review. The engine flags a dictionary only where its values flow into typed code through `Variant` (`unsafe_call_argument`, `unsafe_method_access`); dot access on a typed `Dictionary` compiles silently.

### Closed sets

*Default.* A closed set is a named enum, never an anonymous `enum { A, B }`, bare `int`, or string compared with `==`. A named enum is a distinct static type. Assigning another enum or an anonymous enum's constant is a compile error. Give every enum variable an explicit default, even when `0` is a member. Enum values remain integers internally, so different enums can compare and add. `int_as_enum_without_cast` requires `as` for integer conversion, and `int_as_enum_without_match` rejects a constant integer absent from the enum. At the boundary, convert an external run-time integer by matching it to a member. Refuse unmatched values.

`match` is not exhaustive, and missing members produce no warning. A `match` on a project-owned enum lists every member and ends with a default arm that asserts. An added member then fails when execution first reaches a missing decision in a debug build. Names sent outside the program, such as status fields or records, use an explicit map with one entry per member. Never derive those names through reflection. Renaming a variant makes `Phase.find_key(phase).to_lower()` silently change the wire format.

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

In debug builds, a failed `assert` prints a script error and abandons the function (measured). The checks' error filter rejects that error. Release builds strip the `assert` and return the arm's value. Those behaviours make `assert` suitable here but unsuitable for input ([Errors](errors.md#errors)). Choose the safest fallback the caller can use, never a guess at the missing case. A missing map member causes a lookup script error in both debug and release builds. Test every member's map entry.

A native enum such as `Environment.ToneMapper` has no `values()`. `ClassDB.class_get_enum_constants` returns names rather than values (measured). Test membership by mapping each name through `ClassDB.class_get_integer_constant`, or by comparison with a hand-written member list. Never use a numeric range that could silently exclude a new engine member.

Ref: the engine tests `analyzer/errors/enum_type_checking.gd` and `analyzer/warnings/enum_without_default_value.gd`; godot-demo-projects `audio/rhythm_game/game_state/note_manager.gd` `_get_curr_beat` (the `assert` default; its fallback guesses a case, which this rule does not) with `globals/enums.gd`; godot-open-rpg `src/common/directions.gd`.

**Checked by:** the engine (named enum types, `int_as_enum_*`, a missing default where `0` is no member); the checks' error filter (a missing arm or map entry, when reached); review (strings and ints where an enum fits, a missing default arm, defaults where `0` is a member).

### What typed GDScript cannot express

*Default.* Use the agreed workaround for each gap, not a different workaround in each file. These are structural gaps. The maintainers' proposals for non-nullable objects, traits, structs, and a Godot 5 GDScript rewrite explain why the gaps remain in 4.x.

| Gap | What happens | Workaround |
|---|---|---|
| **No generics** | GDScript has no `Result<T, E>`, `Option<T>`, or typed callable. gdUnit4's `GdUnitResult` holds a `Variant` payload, so every caller casts. | Use a per-operation result record with typed fields ([Refusals are values](errors.md#refusals-are-values)). For a callback, use a `Callable` and document its signature in the `##` of the member that holds it. |
| **No sum types** | An enum is an integer without a payload, so a record cannot express alternative shapes. | Use a record with an enum field and the fields of every case. Document in `##` which fields each case uses. Construct it only through one static function per case, so a caller never assembles a combination by hand. The other combinations remain representable, so review keeps them out. |
| **No nullability** | Object-typed variables may be `null`. Built-in-typed variables cannot hold `null` and default to their zero. | Represent an absent object as `null` and test it where a function returns it. Put an absent built-in value in a record field that is `null` when absent. Never use a sentinel such as `-1`, `INF`, or `Vector3.ZERO`. A single refusal is a `String` that is empty when nothing was refused, because a refusal is never empty text. State that convention in the function's `##`. |
| **No interfaces** | GDScript has no traits or structural typing and supports only single inheritance. | Use an `@abstract` class with `@abstract func`s, available since 4.5, extended by each implementation. Never use `has_method` ([No duck typing](organisation.md#no-duck-typing)). As in the [Rust standard](../rust/design.md#traits), an abstract base needs more than one implementation or a dependency that it inverts. |
| **Invariant typed arrays** | `Array[Node3D]` does not fit `Array[Node]`. An untyped `Array` holding integers does not fit `Array[int]` when assigned at run time. | Fill a typed array with `.assign()`, which copies and converts. Build typed containers through their typed field, never by assigning an untyped variable to them. |
| **`as` returns null** | `node as Node3D` returns `null` on mismatch without an error or warning. | If the value must have a particular type, use a typed declaration such as `var mesh: MeshInstance3D = node`, so a mismatch causes a run-time error. Use `as` only when `null` is an expected outcome, and test it on the next line. For a `Variant`, test with `is` first, then assign to a typed local. |
| **No exhaustive `match`** | A missing enum member falls through a `match` without a diagnostic. | Use the default arm that asserts ([Closed sets](#closed-sets)). |
| **Untyped signal emission** | The analyser does not check `emit` arguments against the declaration. A wrong type fails at the receiver at run time (measured). | Declare typed signal parameters as documentation, and emit as [Signals](organisation.md#signals) says. |
| **No typed nested containers** | `Array[Array[int]]` is a syntax error. | Use a record class for the inner value ([A record is a class](#a-record-is-a-class)). |
| **No private members** | A leading underscore is a convention. | Methods keep the invariants, and review checks those methods ([Invariants](#invariants)). |
| **Freed objects** | A reference to a freed node compares `== null` and still has `TYPE_OBJECT`. Applying `is` to it raises an error (measured). | Where `null` has meaning, test `typeof(value) == TYPE_NIL`. Enter an object branch only after `typeof(value) == TYPE_OBJECT` and `is_instance_valid` pass, before any class test, in one helper that every caller uses. |

The style guide and the static typing page disagree about `as` versus a typed declaration. The style guide calls `get_node(...) as ProgressBar` "more type-safe". The static typing page calls a typed declaration "more reliable" because a mismatch "will be detected immediately". The static typing page adds that `as` "will be silently cast to null". A project that requires an explicit mismatch error uses the typed declaration.

Ref: the manual's static typing page ("Type casting", "Safe lines") and GDScript reference ("Typed arrays", "Abstract classes and methods"); godot-open-rpg `src/combat/actions/battler_action.gd` (an `@abstract` resource as an interface); gdUnit4 `addons/gdUnit4/src/core/GdUnitResult.gd` (how far a result goes without generics, and where it stops); ["Make objects non-nullable by default"](https://github.com/godotengine/godot-proposals/issues/11054) (godot-proposals #11054), on GDScript's three kinds of null.

**Checked by:** the engine (`unsafe_cast`, a nested typed container); review for the rest.

### Invariants

*Default.* A class establishes its invariant in one constructor and preserves it on every path. GDScript has no private members, so methods enforce the invariant by convention. Prefix affected fields with underscores and read them through accessors. Make every change through a method that preserves the invariant. Construct the value through a `static func` that returns an instance or refusal, never through `new()` followed by optional caller assignments. A caller can still write `_low` directly, so review must prevent that access. State this limitation in the class's `##`. Where the engine constructs a class, as with inspector-created `Resource` objects or `duplicate()`, `_init` parameters need defaults. For those classes, the static function is the checked constructor. An exported property preserves its invariant through a setter, including changes from the inspector.

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

*Always.* [Parse at the boundary](../../general.md#design) governs authored files, build records, JSON requests, and native-library values. In GDScript, the boundary is a parser component, not a single function. Its parser object and field readers may exchange raw values. No raw `Dictionary` or `Variant` leaves that component for a consumer. Collect refusals with a field path such as `slots.left.priority` and a line when the reader supplies one. `JSON.parse` reports syntax-error lines through `get_error_line()`, but does not report value locations. For JSON values, the dotted field path supplies the location. `JSON.parse_string` returns `null` on failure without a reason. Use a `JSON` instance.

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

Ref: gdUnit4 `addons/gdUnit4/src/cmd/CmdArgumentParser.gd` (one entry point, refusals returned as a result rather than printed; its payload is still an untyped `Array`); godot-demo-projects `networking/webrtc_signaling/server/ws_webrtc_server.gd:204-217` (checking parsed JSON's shape, and the assign-before-check trap at `:206-207`); Pixelorama `src/Classes/Project.gd:458-495` (the anti-pattern: parsing and building interleaved, nested JSON read by dot access mid-load). For example, in tdollhouse: `GFParse` (`addons/dolls/contract/parse.gd`) uses one parser per file, `refuse` names the file and the field path, and `record` refuses unknown fields.

**Checked by:** the engine, partly (`unsafe_call_argument` where a field reader passes a `Variant` on untested); unit tests of each refusal on made-up input; review (that no raw value leaves the parser).

### JSON numbers are floats

*Always.* JSON parses every number as `float`. Membership is type-strict, so `400 in [400.0]` is false (measured), and a `Dictionary` keyed by `int` misses a `float` key. Check and convert a whole-number field once at the boundary, as `whole()` does above. JSON cannot retain `Vector3`, `Color`, `StringName`, or class values. The writer and parser agree on one encoding per type, such as three numbers for a vector. Each new field in a record the program reads must have an explicit type expressible in plain JSON.

Ref: the `JSON` class reference; the manual's "Saving games", "JSON limitations".

**Checked by:** unit tests on made-up records; review.

### Typed exports are not validation

*Always.* When `ResourceLoader` loads a `.tres` or `.tscn`, it constructs each stored value. It converts containers to their declared type and sets each value through the script's property setter. Scalars convert when the engine supports the conversion. Otherwise, the default remains. The following results were measured on 4.7.1 with a hand-written `.tres`:

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

- **Safe generated resource.** Only the project's own tool, such as a bake or build, writes the resource from parsed values. It is typed, trusted generated output under [Generated output is typed and trusted](../../general.md#design). Typed exports load what the tool wrote.
- **Safe inspector-edited resource.** A person edits the resource only in the inspector, which writes declared types.
- **Unsafe text-edited resource.** A person writes or edits the resource as text, and the program loads it through `ResourceLoader`. The resource itself is the authored boundary. Its exports are `Variant`, with `##` comments stating authored types. A `validate()` refuses each incorrect type by field before anything reads the values. The root fix is to store a plain record that the project's parser reads into typed resources, as for any authored file. Keep the `Variant` form only while the project itself authors the resource.
- **External resource.** Never pass downloads, users' saves, or imported packages to `ResourceLoader`. Store outside data in a plain format that the project's parser reads.

Never add a hand-written resource where a typed generated one exists.

Ref: the engine, `scene/resources/resource_format_text.cpp` (container conversion while loading) and `modules/gdscript/gdscript.cpp` `GDScriptInstance::set`; the manual's "Resources", "Creating your own resources"; the [godot-safe-resource-loader](https://github.com/derkork/godot-safe-resource-loader) README, on embedded scripts.

**Checked by:** unit tests of `validate()` on made-up resources; the error filter, for an inconvertible container in a resource a check loads; review.

### Resources are shared

*Always.* Loading a resource twice from one path returns one object, so every user sees every mutation. A user that changes a loaded resource changes its own copy made with `duplicate()`. `duplicate()` is shallow. `duplicate(true)` copies nested arrays, dictionaries, and internal sub-resources. Sub-resources saved in their own files remain shared with the original (measured). A deep copy therefore does not isolate external sub-resources. Copy the resources that will change. State which parts remain shared in `##`. An inner class extending `Resource` does not serialise its properties, so an inner class never extends `Resource` for saving. A saved resource is its own file.

Ref: `Resource.duplicate` and `Resource.duplicate_deep` in the class reference; the manual's "Resources", "Nodes and resources".

**Checked by:** review.
