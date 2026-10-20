# GDScript standard: Tests

Part of the [GDScript standard](README.md). Tests of GDScript code.

## Tests

### Plain headless probes

*Default.* Tests are plain headless scripts using `extends SceneTree`, one check per file named for its subject. Put cases in an inner class whose `run()` returns an exit code. Failure lines state the broken promise, such as "a valid record refused", never a value dump. A runner may batch checks in one process if it restores static state between them ([Static state](lifetime.md#static-state)). Use no test framework. GUT and gdUnit4 mainly add doubles, partial doubles, and stubs that test calls rather than function promises. A framework also adds hundreds of vendored scripts to scan or exclude, another runner and output format, and a dependency pinned per minor engine release. What plain scripts lack is readable diffs and orphan reports: a small helper supplies readable diffs, and comparing `Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT)` before a case and after its deferred frees reports detached orphan nodes (though not nodes leaked inside the tree).

**Checked by:** the project's checks.

### The error filter

*Always.* A test passes only if the engine prints no unexpected error. Each `ERROR:` or `SCRIPT ERROR:` line fails the check unless the case declares that error as intentional. Script errors abandon functions and return defaults that can resemble success ([Errors](errors.md#errors)). Match declared errors to printed errors one to one. An expected error must neither excuse another error nor go missing unnoticed. A check that crashes or does not report fails.

**Checked by:** the project's checks.

### Inputs and expectations

*Always.* [Checks](../../general.md#checks) governs GDScript tests as written. Build a unit test's made-up input inline, as a `const` record or case table. Real inputs belong to higher test layers. Authored values include counts from made-up inputs, specified lengths, and tolerances set where a person watching would notice a difference. Compare exact wording when it is the promise, such as `"made_up.json: slots.left.priority: must be a whole number"`. A measurement for people is a printed report line that the change report reads. A case table reports as [What a test is kept for](../rust/tests.md#what-a-test-is-kept-for) says.

A test that copies a value from a previous run writes `_check(clip.length == 1.2333, "length")`. Write the value from the inputs that built the made-up clip instead: `_check(is_equal_approx(clip.length, FRAMES / RATE), "a clip lasts its frames over its rate")`.

Ref: gdUnit4 `addons/gdUnit4/test/cmd/CmdArgumentParserTest.gd` (invented argument lists, one behaviour per case, refusals beside acceptances); beehave `test/nodes/decorators/inverter_test.gd` with `test/actions/count_up_action.gd` (a hand-written fake whose status the test sets).

**Checked by:** review.
