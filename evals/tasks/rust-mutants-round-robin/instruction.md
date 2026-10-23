# Shard rust-mutants round-robin on the CPUs it is given

`tools/crates/rust-mutants` is rust-mutants, a command that runs cargo-mutants over the Rust a change touched, split into shards that run at once, each in its own copy of the repository, and lists the mutants the tests missed. Its crate documentation and its scenario tests, which put stand-ins in place of cargo-mutants, `systemd-run` and `unshare`, describe how.

Two things make a report slower than it needs to be:

- cargo-mutants' default sharding gives each shard a consecutive slice of the mutant list, so the mutants of one slow function, which time out together, all land in one shard: on one project one shard ran 551 s and the other 218 s.
- The shards share the jobserver's tasks, which the tool counts as one per CPU online. A caller that gives the tool part of the machine, for example by running it under `taskset -c 0,1`, as a report that shares the CPUs out between its steps will, still gets as many tasks as the whole machine has CPUs.

Change both:

- Each shard's cargo-mutants command deals the mutants to the shards in turn, mutant `i` to shard `i % k`, by passing cargo-mutants `--sharding round-robin` beside the shard's `--shard <index>/<count>`. The set of mutants tested stays the same.
- The tool counts the CPUs this process may run on, its CPU affinity, in place of the CPUs online, and shares those among the shards as before: each shard's `--jobserver-tasks` is that count divided by the number of shards, and at least 1. Run under `taskset -c 0,1` with two shards, each shard gets 1 task; run on four CPUs, each gets 2.

Nothing else in the command a shard runs, or in what the tool prints, changes. The crate's documentation, which says the tasks are one per CPU online, says what it counts now.

Existing tests must still pass. Update an existing test only where it asserts something this changes, such as a shard's exact arguments or the CPUs it counts. Add tests for the new behaviour beside the crate's own.

`AGENTS.md` at the repository root says how to build and test it.
