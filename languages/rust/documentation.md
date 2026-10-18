# Rust standard: Lints and documentation

Part of the [Rust standard](README.md). Lint exceptions, the report scans and documentation.

## Lints and documentation

### Lint exceptions

Put `#[expect(lint, reason = "…")]` on the narrowest item. `allow_attributes` and `allow_attributes_without_reason` enforce that form. The reason names a constraint the code cannot show. Set no size thresholds in `clippy.toml`. Keep `too_many_arguments` at Clippy's default. If required inputs exceed that limit, use an `expect` rather than a tuple or one-off struct. Do not enable `wildcard_enum_match_arm` or `use_debug`. The first flags foreign enums, and diagnostic `Debug` output is legitimate.

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

[tools/rust_scans.py](../../tools/rust_scans.py) provides three text scans for cases the lints cannot detect. The first reports file, process, environment, and clock tokens in project-named core crates ([Purity](design.md#purity)). The second reports a primitive field or parameter whose name contains a unit, outside named kernels and tests ([Quantities](design.md#quantities)). The third reports codes in `#[error(...)]` messages ([Messages and codes](errors.md#messages-and-codes)). These scans recognise spelling only. An aliased import escapes the first scan, and a renamed field escapes the second. Run them from the development shell as review aids. They cause no check failures.

**Checked by:** review, for whether the scans ran with the project's cores and kernels and stayed report-only.

### Documentation

This section applies `general.md`'s comment rule to Rust. Each crate's and module's `//!` describes its job, what it never knows, its design, and its reasoning at the necessary length. rust-analyzer and ripgrep put architecture in crate documentation. Document every public item's purpose and invariants. Include `# Errors` for a fallible function and `# Panics` for one that may panic. The relevant lints are `missing_docs`, `clippy::missing_errors_doc`, and `clippy::missing_panics_doc`. Include a runnable example for each public core entry point and each text format, as nextest and jiff do. Only comments inside function bodies have a length cap (`general.md`).

```rust
//! Keeping a character's limbs clear of its own body over plain values; it never reads a file, a clock or the
//! engine, and knows no bone's name.
```

Ref: ripgrep's `grep-matcher` crate doc (`crates/matcher/src/lib.rs`); jiff's doc examples, each the documented common use.

Document the current design in the owning module's `//!`, its types, and their documentation. External documents contain what the code cannot: research behind a choice, an unbuilt plan, an externally read format, and user documentation. The project's `AGENTS.md` gives a short orientation with the system's data flow and each step's entry point. It helps readers find where to start. It repeats no type, invariant, or signature, since duplicate descriptions can become stale. Do not create a separate architecture document.

**Checked by:** rustc's `missing_docs` and Clippy's `missing_errors_doc` and `missing_panics_doc`; rustdoc runs the examples; review for module documentation and whether examples show the common use.
