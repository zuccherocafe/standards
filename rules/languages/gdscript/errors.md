# GDScript standard: Errors

Part of the [GDScript standard](README.md). Refusals as values, reporting at the host's edge, assert and engine errors.

## Errors

GDScript has no exceptions. A run-time script error prints `SCRIPT ERROR`, abandons the function, and returns `null` or the zero value. Examples include wrong-type assignments, missing keys, and calls on `null`. The caller continues without notification of the failure (measured). Return a failure as a value when a caller must handle it. For a bug, produce an error in debug builds, where checks run. Its printed error line fails the checks.

### Refusals are values

*Default.* Decide an error's form by who can fix it, as the [Rust standard](../rust/errors.md#who-fixes-it) does.

- **The author or environment can fix it.** Return a refusal value for an authored file, missing file, failed process, or external request. One option is a record with an `errors: PackedStringArray` field. Set it through a named static constructor, such as `static func refused(reasons: PackedStringArray) -> ImportInput`, so construction occurs in one place. Alternatively, return the typed record or `null` from a parse function and store reasons in the caller-supplied parser object. A program-matched refusal, such as an interface refusal or protocol error, carries a stable `snake_case` code beside its message in separate fields ([Messages and codes](../rust/errors.md#messages-and-codes)). A human-readable refusal names its place as [Parse at the boundary](../../general.md#design) says.
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

*Default.* `push_error` belongs at the host that decides which refusals reach a person. Hosts include editor plugins, shipped scenes, test benches, and command-line entry points. A library function never both logs a refusal and returns `null`. That combination gives its caller no reason and leaves only a log line. `printerr` and `print` are not error channels.

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

*Always.* Release builds remove `assert`, including its condition and message. Never use it to check input, cause a side effect, or provide the only protection before using an invalid value. It states a promise internal to the code, such as an asserting default arm or a precondition between one class's functions. In tests, use the probe's check helper to report failure. `assert` is not a test assertion.

Ref: the GDScript reference, "Assert keyword".

**Checked by:** the engine (`assert_always_*`); review.

### Engine errors are checked

*Always.* An engine call that can fail reports failure as an `Error`, `null` with a separate reason, or an empty value. `ResourceSaver.save`, `DirAccess.make_dir_recursive_absolute`, and `JSON.parse` return `Error`. `FileAccess.open` returns `null`, with its reason available through `FileAccess.get_open_error()`. Bind the result at the call site. Test it there. Convert failure into a refusal naming the path. With `return_value_discarded` off, review must catch a discarded `Error`. A signal connection's `Error` and an `Array.append`'s `bool` may be discarded.

**Checked by:** review.
