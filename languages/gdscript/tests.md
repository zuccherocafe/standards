# GDScript standard: Tests

Part of the [GDScript standard](README.md). Tests of GDScript code.

## Tests

### Plain headless probes

Tests are plain headless scripts using `extends SceneTree`, one check per file named for its subject. Put cases in an inner class whose `run()` returns an exit code. Failure lines state the broken promise, such as "a valid record refused", never a value dump. A runner may batch checks in one process if it restores static state between them ([Static state](lifetime.md#static-state)). Use no test framework. GUT and gdUnit4 mainly add doubles, partial doubles, and stubs that test calls rather than function promises. A framework also adds hundreds of vendored scripts to scan or exclude, another runner and output format, and a dependency pinned per engine minor. Plain scripts lack readable diffs and orphan reports. A small helper supplies readable diffs. Compare `Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT)` before a case and after its deferred frees for an orphan report. That report covers detached nodes only, not nodes leaked inside the tree.

**Checked by:** the project's checks.

### The error filter

A test passes only if the engine prints no unexpected error. Each `ERROR:` or `SCRIPT ERROR:` line fails the check unless the case declares that error as intentional. Script errors abandon functions and return defaults that can resemble success ([Errors](errors.md#errors)). Match declared errors to printed errors one to one. An expected error must neither excuse another error nor go missing unnoticed. A check that crashes or does not report fails.

**Checked by:** the project's checks.

### Inputs and expectations

This section applies `general.md`'s test rules to GDScript. Unit tests build small made-up inputs inline, such as a `const` record or case table, only as large as the promise needs. Real inputs belong to higher test layers. Write expected values by hand from the input's source, a specification, or a law. Authored values include counts from made-up inputs, specified lengths, and tolerances at perceptible differences. Never put a number copied from a run into a check. Compare exact wording when it is the promise, such as `"made_up.json: slots.left.priority: must be a whole number"`. A reviewed snapshot of exact text or structured output whose shape is the promise also counts as authored. Print measurements intended for people as report lines for the project's change report. A case table identifies the failed row. Keep tests for promises real data relies on and name them for those promises. Do not write checks of other checks.

Before (hypothetical), a value read off a run: `_check(clip.length == 1.2333, "length")`. After, the value the made-up clip was built with: `_check(is_equal_approx(clip.length, FRAMES / RATE), "a clip lasts its frames over its rate")`.

Ref: gdUnit4 `addons/gdUnit4/test/cmd/CmdArgumentParserTest.gd` (invented argument lists, one behaviour per case, refusals beside acceptances); beehave `test/nodes/decorators/inverter_test.gd` with `test/actions/count_up_action.gd` (a hand-written fake whose status the test sets).

**Checked by:** review.
