# The GDScript standard

This standard governs GDScript in zucchero✿café projects using Godot 4.7. It applies [general.md](../../general.md) to GDScript and adds GDScript-specific rules. [review.md](../../review.md) turns both standards into review questions. [templates/godot/](../../../templates/godot/) contains the engine-check configuration in `project.godot`'s `[debug]` block and the formatter configuration.

GDScript is gradually typed, and the engine's analyser checks it. It has no generics, nullability, sum types, interfaces beyond abstract classes, exhaustive `match`, or exceptions. When a resource loads, its typed exports are converted rather than checked. The analyser's warnings cover much, but not all, of the type system. Some dynamic access compiles without a diagnostic and fails only at run time. This standard identifies those gaps and the alternatives: boundary parsers, record classes, review conventions, or an explicit lack of checks.

References name paths in public repositories and pin commits so the line numbers remain valid. The references are the [Godot 4.7 manual](https://docs.godotengine.org/en/4.7/), [godot](https://github.com/godotengine/godot) at `4.7.1-stable` (`a13da4feb`), and its tests in `modules/gdscript/tests/scripts/`. They also include [gdUnit4](https://github.com/godot-gdunit-labs/gdUnit4) at `0b1748a9`, [godot-demo-projects](https://github.com/godotengine/godot-demo-projects) at `b761b4cd`, and [godot-open-rpg](https://github.com/gdquest-demos/godot-open-rpg) at `d59f2f71`. Other references are [phantom-camera](https://github.com/ramokz/phantom-camera) at `21d37624`, [Pixelorama](https://github.com/Orama-Interactive/Pixelorama) at `578d058a`, and [beehave](https://github.com/bitbrain/beehave) at `fe589153`. These references illustrate the patterns discussed below. Where a reference falls short of a rule, the rule identifies that limitation. Examples are short and invented except for [tdollhouse](https://github.com/zuccherocafe/tdollhouse) examples. Introduce those with "for example, in tdollhouse:" and retain them only for what the references do not teach. Behaviour marked "(measured)" was run on Godot `4.7.1.stable.official.a13da4feb`. Every complete GDScript block below compiles under the template's `[debug]` block. Blocks marked "sketch" are fragments.

Each rule ends with a Checked by entry naming the engine, a project check, or review. An engine check means an Error-level warning or a load-time or run-time error caught by the project's error filter. Where the engine checks only part of a rule, the entry names that part.

The standard is in several files. Read this one first, then each file whose subject a change touches:

- [Warnings](warnings.md): Every warning Error or off, exceptions, the project's addons and compiling every script.
- [Design](design.md): Types, records, closed sets, invariants and parsing at boundaries.
- [Errors](errors.md): Refusals as values, reporting at the host's edge, assert and engine errors.
- [Organisation](organisation.md): Classes, files, names, duck typing and documentation.
- [Lifetime](lifetime.md): Lifetime, concurrency and the editor.
- [Tests](tests.md): Tests of GDScript code.
- [Performance](performance.md): Per-frame code.

## Toolchain and warnings

### The engine is the checker

Each project pins an exact engine build, such as `4.7.1.stable.official.a13da4feb`. Run checks with the editor binary or a debug template, because GDScript warnings exist only in debug builds. The analyser is the compiler in `general.md`'s sense. It understands the pinned version's syntax, needs no external maintenance, and stores its settings in `project.godot`. The editor therefore shows the same errors that fail checks. No external linter is a check. gdtoolkit accepts syntax only up to Godot 4.5 and rejects valid 4.7 code. Its size thresholds are forbidden by `general.md`. The formatter is the only external tool in the checks. It decides layout, not correctness ([Names and order](organisation.md#names-and-order)).

**Checked by:** the engine.

### Engine upgrades

A project adopts every stable engine release, patch or minor, soon after release. Frequent upgrades stay small and deliver fixes while the affected code is familiar. They also keep the warning list and workarounds current. Each upgrade is one change that follows these steps:

1. Move the engine pin in the project's `AGENTS.md`, the engine binary, and the export templates together. Record their versions and digests in the development shell's flake ([Toolchain](../../general.md#toolchain)). The checks, editor, and exported program must use one build.
2. When upgrading the engine, move any GDExtension binding, such as gdext, and its engine-API feature together. For a minor release, wait until the binding supports its API. Do not run a binding built against the previous API on the new engine.
3. Re-import the project, so every cache is rebuilt by the new engine.
4. Compare the release's warning list with the previous list. Set every new warning to Error or off ([An engine upgrade reviews the warning list](warnings.md#an-engine-upgrade-reviews-the-warning-list)).
5. Re-check every recorded engine workaround. Remove each workaround fixed by the release, together with its note.
6. Run the formatter's `--check`, since the formatter's pin may move with the engine's syntax.
7. Land the change with the full checks, a report of what changed, and the performance report. Review engine-induced differences before further work depends on them.

**Checked by:** review of the upgrade change against these steps; the project's checks.

## For a new project: not yet decided

A project answers these open questions in its `AGENTS.md` when it needs the relevant capability, with prior art. These are not rules yet.

- **A naming linter.** The engine checks no naming convention. The formatter's `lint` subcommand does. Its `private-access` rule treats `_name` as object-private, unlike this standard.
- **A test framework.** Decide whether doubles are needed for behaviour that plain probes cannot test.
- **Autoloads.** A project without autoloads forbids them. A project that needs global state prefers a `static func` library, static variables, or a shared resource, as the manual advises.
- **C# or another scripting language.** Decide whether to use one alongside GDScript.
