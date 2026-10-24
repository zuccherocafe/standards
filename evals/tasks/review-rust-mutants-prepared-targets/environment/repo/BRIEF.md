# Prepared mutation targets

## Goal

Add `--prepared-target PATH` to prebuild the unmutated subject once and seed mutation workers from that result. The observable promise is the same caught/survived classification as cold isolated runs. `Prepared` must mean that the matching build completed successfully before a shard can execute it. Each concurrent worker owns its mutable build outputs.

## Scope

This executable workspace contains a compact slice of `standards/tools/crates/rust-mutants`. The CLI consumes authored replacement `src/lib.rs` files rather than discovering Rust mutations. The subject is a standalone Cargo package containing `Cargo.toml` and `src/lib.rs`, including its tests and `mutants` profile. Cases are valid Rust source files in one directory. Compilation and test failures count as caught mutations; process-launch and filesystem failures abort the run. Source discovery and richer mutation outcomes are outside this change.

## Decided

Use a named `mutants` profile inheriting `test` with optimisation level one, both for prebuilding and mutation execution. Use scoped threads and return classifications in case-name order. A prepared directory is an immutable seed after prebuilding; worker-local copies may change independently. Keep unconditional deletion of each worker's `mutants` outputs before replacing a case so Cargo cannot reuse a result from another mutation. This safety step deliberately leaves dependency reuse as the main intended saving.

## Evidence

Read `docs/design-reads/rust-mutants.md` and `evidence/checks.log`. Checks cover command arguments, partitioning and CLI parsing; they do not establish concurrent prepared-target isolation. Timings are reports only and never acceptance thresholds.
