# Count the lines of every valid Rust file

`tools/rust/crates/dandegate` is dandegate, a command that reports what a change moved between two revisions of a project. For a Rust workspace, its `lines` report counts each crate's production and test lines on both revisions. `src/rust/lines.rs` does the counting, and its module documentation states the rules: which lines are test lines, and that braces inside comments and literals do not count.

The report must count every valid Rust source file by those rules, whatever text its comments, string literals and character literals hold, including characters outside ASCII, and finish with counts for every crate, never stopping on a file or leaving the report unfinished. dandegate does not keep this promise today. Make it keep it. How dandegate recovers from a panic in general is outside this change.

Everything else dandegate does stays as it is, and its existing tests must still pass. You need not add tests, though you may.

`AGENTS.md` at the repository root says how to build and test it.
