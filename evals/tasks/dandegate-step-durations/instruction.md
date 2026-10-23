# Print each step's duration in dandegate's report

`tools/crates/dandegate` is dandegate, a command that compares two revisions of a project and writes an HTML report of what changed. It runs the steps a project's `dandegate.toml` names, on each revision or once for both, and writes the report's page, `index.html`, and its log, `report.log`, into the output folder. Its crate documentation and its scenario tests describe how.

A report says what each step did but not how long it took, so finding where a slow report's time went means reading timestamps out of each step's own output. Make dandegate time each step instance that runs, from its start to its outcome, and show that time in two places:

- In `report.log`, on the instance's outcome line, after the outcome: `=== <side> <step>: <outcome> in <seconds> s`, for example `=== after visual: exited 0 in 61.3 s` or `=== both notes: failed: exited 1 in 0.4 s`. The line that says the instance started does not change.
- On the page, in a new `Time` column of the step table, directly after `Outcome` and before `Log`, so the table's columns begin `Side`, `Step`, `Outcome`, `Time`, `Log`. Its cell holds the same text as the log, such as `61.3 s`.

A duration is written in seconds to a tenth, with a dot, a space and `s`: `0.0 s`, `2.5 s`, `61.3 s`. Every instance that runs in this report has one, whether it succeeded, failed or ran out of time. An instance that did not run in this report, because it was not selected, not needed, skipped or reused from a kept result, has an empty `Time` cell.

Nothing else changes: no section, measure, printed line or other part of the page or the log, and no other column.

Existing tests must still pass. Update an existing test only where it asserts the exact text that this changes. Add tests for the new behaviour beside the crate's own scenario tests.

`AGENTS.md` at the repository root says how to build and test it.
