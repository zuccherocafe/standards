# Give dandegate's steps a build cache that survives reports

`tools/crates/dandegate` is dandegate, a command that reports what a change moved between two revisions of a project. It checks both revisions out afresh for every report and runs the steps a project's `dandegate.toml` declares in each checkout. Its `src/main.rs` long help, `src/plan.rs` and the module documentation describe the steps file and its placeholders, and its scenario tests under `tests/scenarios/` show how it is used.

Because every report checks the revisions out afresh, a step that builds starts cold every time: on one project a step's release build of each side takes about two minutes, every report. Give project steps a build cache that is kept from one report to the next.

## What to build

- A new placeholder, `{cache}`, open to every kind of step (per-revision, after and once steps alike), expands to the absolute path of a folder kept from one report to the next for that step at that place: `<repository>/.cache/dandegate/builds/<step>/<place>/`, where the place is `before` or `after` for a step that runs on each revision and `both` for a step that runs once. dandegate makes the folder when a report needs it. Only an enabled step whose command or environment names `{cache}` has one; a step that names none, or an optional step not selected, gets no folder.
- One report at a time owns all the caches. A report whose enabled steps name `{cache}` locks the caches before it checks out either revision, so its checkouts' files are always newer than anything the caches hold, and Cargo, for one, builds the workspace's own crates again and reuses only their dependencies. The lock must be held until every process the report started has ended. Say in the documentation that this is why a cache reuses only dependencies.
- When another report holds the caches, this report does not wait and does not share them: every step that names `{cache}` gets a new, empty folder of this report's own instead, a separate one for each step and place, removed when the report ends, and the page notes, for each such step, that the build caches are in use by another report (the note contains the words "in use by another report").
- The caches together are bounded at 32 GiB, counting the lengths of the files they hold (a file's apparent length, not the disk blocks it uses). At the end of each report that holds them, dandegate removes whole `<step>/<place>` folders, least recently taken first (the time a report last took each folder for a step), until the total is no more than 32 GiB, and names each removed folder by its path in that report's `report.log`. Deleting `.cache/dandegate/builds/` by hand while no report runs clears them all; say so in the documentation.
- `{cache}` is written by name in a step's identity, as `{out}` is, so where the folder lies never changes whether a kept result is reused. The refusal of an unknown placeholder lists `{cache}` among the placeholders a step has.

Document the placeholder and the caches where dandegate documents the steps file and its long help.

Everything else dandegate does stays as it is. Its existing tests must still pass, updating only an existing test whose asserted output this change alters. Add tests for the new behaviour beside the crate's own.

`AGENTS.md` at the repository root says how to build and test it.
