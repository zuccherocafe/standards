# GDScript standard: Performance

Part of the [GDScript standard](README.md). Per-frame code.

## Performance

*Default.* The general.md performance rules apply. These are the costs particular to GDScript.

- **Typed code is faster.** The analyser emits typed instructions for operands whose types are known at compile time. Untyped arithmetic and calls use `Variant` dispatch. The gain is greatest for engine calls and smallest for attribute access.
- **Dictionary and by-name reads cost lookups.** A dictionary read hashes a key. A property read through `Object.get` or `Variant` searches the script and ClassDB at each inheritance level. A record avoids both ([A record is a class](design.md#a-record-is-a-class)).
- **Use packed arrays for bulk numbers.** `PackedFloat32Array`, `PackedVector3Array`, and the other packed arrays are contiguous and smaller than an `Array` of the same values.
- **Every call costs.** Per-entity `_process` calls across thousands of nodes cost more than one loop over those nodes.
- **Per-frame code allocates nothing it can keep.** `_process`, `_physics_process`, and their callees reuse class-owned arrays and records rather than constructing a `Dictionary` or `Array` each frame. Resolve `get_node` and `$` once into `@onready` members. Use `StringName` literals, such as `&"parameters/blend"`, for repeated name lookups. Format strings per frame only for diagnostics.
- **Heavy maths does not belong in GDScript.** There is no JIT. A solver, search, or per-vertex loop moves behind a typed native-library binding when a profile identifies it on a hot path. Measure per-frame binding calls too, since crossing the binding has a cost.

To make a performance claim, measure the path with the engine profiler or `Time.get_ticks_usec()`, as [Measure before claiming](../../general.md#performance) says.

Ref: the manual's static typing page and "CPU optimisation", "GDScript"; "GDScript progress report: Typed instructions"; the Halls of Torment talk (batching `_process`, moving hot code to native, measuring the binding).

**Checked by:** review and the project's performance report.
