# Let dandegate's caller set how many steps run at once

`tools/crates/dandegate` is dandegate, a command that compares two revisions of a project and writes an HTML report of what changed. It runs the steps a project's `dandegate.toml` names. Today it runs up to four ordinary steps at once, a fixed number; a step with a CPU share (`cpus = <count>`) runs beside them on CPUs of its own, and exclusive steps (`exclusive = true`) run last, one at a time. Its crate documentation and its scenario tests describe how.

A step can start several compilers, so four steps at once can pass the memory a report is given: on one project a cold report started two release builds, an API listing's build and two mutation shards together, passed its 16 GiB cap, and the kernel killed builds that other steps needed. A report run under a memory cap needs to trade its length for its peak memory.

Add a command-line option, `--steps-at-once <count>`, that sets how many ordinary steps run at once:

- Without the option the limit is 4, as today, so a report run without it behaves exactly as before.
- With it, no more than `<count>` ordinary steps run at the same time, and dandegate still starts as many as the limit and the steps' needs allow.
- `0` is refused, since it would run nothing, and so is anything that is not a whole number. Each is a malformed argument: dandegate names `--steps-at-once` in its message on standard error and exits with code 2, before it makes the output folder or runs any step.
- A step with a CPU share still runs beside the ordinary steps without counting towards the limit, and exclusive steps still run alone, after every other step.

The command's long help, the steps file's format reference and the crate's module documentation, which say four today, say how the option sets the number. Everything else dandegate does stays as it is.

Existing tests must still pass. Update an existing test only where it asserts something this changes. Add tests for the new behaviour beside the crate's own scenario tests: that with `--steps-at-once 1` no two steps overlap, and that `0` is refused before anything runs.

`AGENTS.md` at the repository root says how to build and test it.
