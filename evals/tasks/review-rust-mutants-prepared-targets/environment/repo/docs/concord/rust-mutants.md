# Rust-mutants design read

## Models in play

`command::CargoCommand` describes a Cargo invocation, including its working directory, target and profile. `command::Prepared` wraps a command before execution. `shards::Shard` owns a private source workspace, mutable target and ordered mutation cases. The CLI coordinates planning and execution.

## Overlaps

The command name suggests a completed preparation stage, but its constructor only stores arguments. Shards currently build from cold private directories, so no build result witnesses preparation.

## Half-done transitions

Prepared-target support needs a successful prebuild result tied to the subject, selected profile and immutable seed directory. `Prepared` currently carries none of that evidence.

## What later work needs

Concurrent prepared execution must copy the immutable seed into each worker's target before mutation. A worker deletes its own `mutants` profile outputs before each case; no other worker may own that directory.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Prepared command | command | `tools/crates/rust-mutants/src/command.rs` `Prepared::new`; `tools/crates/rust-mutants/src/shards.rs` `run_shard` | The type can exist without a successful build and proves no preparation fact. | Successful prebuilding is the only source of a prepared value, tied to its subject, profile and seed. |
