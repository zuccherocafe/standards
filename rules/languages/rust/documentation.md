# Rust standard: Lints and documentation

Part of the [Rust standard](README.md). This file covers lint exceptions, the report scans, and documentation.

## Lints and documentation

### Lint exceptions

*Always.* Write every lint exception as `#[expect(lint, reason = "...")]`, enforced by Clippy's `allow_attributes` and `allow_attributes_without_reason`, following [Describe each check at its real strength](../../general.md#checks). Leave `clippy.toml` without custom size thresholds and keep `clippy::too_many_arguments` at its default, adding `#[expect(clippy::too_many_arguments, reason = "...")]` when a function genuinely needs more arguments. Leave `clippy::wildcard_enum_match_arm` and `clippy::use_debug` disabled: the first flags wildcard arms on foreign enums, and diagnostic `Debug` output in logs and tests is valid ([Debug](design.md#debug)).

Before, inputs packed to satisfy a threshold:

```rust
fn ended(&mut self, met: &mut Met, (problem, outcome): (&Problem<'_>, &Solved), gripped: Gripped) -> Record
```

After:

```rust
fn ended(&mut self, met: &mut Met, problem: &Problem<'_>, outcome: &Solved, gripped: Gripped) -> Record
```

Ref: nextest `TestRunnerBuilder::build` takes its required inputs under `#[expect(clippy::too_many_arguments)]` (`nextest-runner/src/runner/imp.rs`).

**Checked by:** Clippy's `allow_attributes` for outer `#[allow]` attributes and `allow_attributes_without_reason` for a missing reason, and rustc's `unfulfilled_lint_expectations` for an `expect` that no longer applies; review for an exception's scope, its reason's substance, inner `#![allow]` attributes, and the lint configuration itself.

### Report scans

*Always.* `rust-scans` ([tools/crates/rust-scans/](../../../tools/crates/rust-scans/), exported by the standards' flake) parses each workspace member's `src/` as Rust syntax and provides four scans for cases that lints cannot detect. Run them from the development shell as review aids; dandegate's built-in `scans` step runs them on both revisions of a change report with the arguments in `dandegate.toml`'s `[rust]` table, and they cause no check failures:

- **H1 (purity).** Reports file, process, environment, and clock paths in project-named core crates ([Purity](design.md#purity)).
- **H2 (units).** Reports primitive fields and parameters whose names end in a unit, outside named numerical kernels and test code ([Quantities](design.md#quantities)). Pass `--unit` to add a project's own units to the built-in ones.
- **H3 (error codes).** Reports error codes written inside `#[error(...)]` messages ([Messages and codes](errors.md#messages-and-codes)).
- **H4 (async in cores).** Reports paths to async runtimes, network clients, or async I/O, as well as any `async fn`, `async` block or closure, and `.await` inside core crates ([Keep computation synchronous](async.md#keep-computation-synchronous)).

Because `rust-scans` parses syntax rather than types, it ignores comments and string literals and tracks renamed or grouped `use` items within the same file. It approximates imports file-wide: each `use` binds its name for the whole file regardless of module or block scope, follows import chains up to eight steps, and treats any single-segment name with no matching `use` as local and never reports it. As a result, a scan can report a shadowed local name, such as an outer `struct Instant` when an inner module imports `std::time::Instant`, or a local `mod tokio` called as `tokio::f()`, and it misses cross-file re-exports, names brought in by a glob, macro arguments, and renamed fields.

**Checked by:** review, for whether the scans ran with the project's cores and kernels and stayed report-only.

### Documentation

*Default.* Write crate and module documentation with `//!` as [Documentation lives beside what it documents](../../general.md#writing) specifies, as rust-analyzer and ripgrep put architecture in crate documentation, rather than creating a separate architecture document. Document fallible public functions with `# Errors` and panicking public functions with `# Panics` (`missing_docs`, `clippy::missing_errors_doc`, and `clippy::missing_panics_doc`), and include a runnable example for each public core entry point and each text format, as nextest and jiff do.

```rust
//! Keeps a character's limbs clear of its body over plain values. It never reads a file, a clock, or the
//! engine, and knows no bone names.
```

Ref: ripgrep's `grep-matcher` crate doc (`crates/matcher/src/lib.rs`); jiff's doc examples, each showing the most common use.

Keep external documents for material that the code cannot hold: research behind a choice, unbuilt plans, externally read formats, and user documentation. In the project's `AGENTS.md`, give a short orientation with the system's data flow and each step's entry point so readers know where to start, without repeating types, invariants, or signatures from the code.

**Checked by:** rustc's `missing_docs` and Clippy's `missing_errors_doc` and `missing_panics_doc`; rustdoc runs the examples; review for module documentation and whether examples show the common use.
