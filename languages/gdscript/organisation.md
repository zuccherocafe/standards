# GDScript standard: Organisation

Part of the [GDScript standard](README.md). Classes, files, names, duck typing and documentation.

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

[GDScript-formatter](https://github.com/GDQuest/GDScript-formatter) governs layout, as rustfmt does for Rust. Pin it to an exact release like the engine. It decides indentation, spacing, blank lines, wrapping, quotes, and trailing commas in wrapped literals. Never adjust its output by hand. It follows the style guide except for `general.md`'s 120-column limit. It reads `.editorconfig` settings (measured), whose canonical form is [templates/godot/.editorconfig](../../templates/godot/.editorconfig). Those settings use tabs, 120 columns, and double quotes, except for strings containing double quotes, which keep single quotes. Do not use `--reorder-code`. The formatter does not reorder members or rewrite expressions. Review therefore enforces `and`, `or`, and `not` instead of `&&`, `||`, and `!`, and `0.5` instead of `.5`. Run `--check` over every script in the project's checks. On adoption, reformat all scripts in one commit containing nothing else. A line the formatter cannot wrap, such as a long string, stays long. A project with a line-length check must keep that check.

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

## Documentation

This section applies `general.md`'s documentation rule to GDScript. After `extends`, each class starts with a `##` comment containing a one-line brief. Then describe its job, what it never knows, and its design at the required length. Every public member of a class used by other code has a `##` comment. Public means no leading underscore. Document its purpose, invariants, units, and failure behaviour, including refusal returns or `errors` values. `##` supplies editor help and export tooltips. BBCode references such as `[method read]`, `[member errors]`, and `[param at]` create links. Documentation comments have no line cap. Comments inside a function body or top-level script code are at most six lines under `general.md`. A project's comment check exempts `##`. Comment a private member with `#` only for a constraint the code cannot show.

A comment stating a type, such as "float ≥ 0 — key light energy" above a `Variant` export, indicates a type that admits invalid values. Where `Variant` is deliberate ([Typed exports are not validation](design.md#typed-exports-are-not-validation)), the `##` states the authored type and the refusal that enforces it.

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
