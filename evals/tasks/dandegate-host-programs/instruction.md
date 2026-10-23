# Give dandegate's steps the host programs dandegate.toml names

`tools/rust/crates/dandegate` is dandegate, a command that reports what a change moved between two revisions of a project. It runs each step a project's `dandegate.toml` declares inside that revision's `nix develop` shell, with the shell's `PATH` only: nothing from the caller's `PATH` reaches a step, so every tool a step uses comes from the revision's shell. Its `src/report/step.rs` module documentation and `src/plan.rs` describe how steps are launched and read, and its scenario tests under `tests/scenarios/` show how it is used.

Some programs stay on the host and cannot come from a revision's shell, such as a compositor or a GPU program, so no step can reach them today. Let a project name them.

## What to build

- `dandegate.toml` takes a top-level `host` key, a list of program names, for example `host = ["gamescope"]`. With no `host` key, every step's command line and environment are exactly as today.
- Each entry is a bare program file name. An entry that is empty, `.` or `..`, or holds `/` or a NUL character is refused, and so is a name given twice. Like every other refusal of the steps file, it comes before any step runs, and names the file, the line of the entry and the field: `dandegate.toml:<line>: host: <reason>`.
- At the start of a run, dandegate looks for each named program on its own `PATH`, reading a relative or empty folder there from its own working directory as a shell does, and makes the programs it finds available through one folder of the run's scratch that holds exactly them, removed with the run.
- Every step command, every identity collector and every command of the built-in Rust reports sees the found host programs after the revision shell's own `PATH`: a tool of the same name in the revision's shell wins, and nothing else of the caller's `PATH` reaches the command. Steps keep stopping as they do today: when a step runs out of time or dandegate is stopped, everything the step started is stopped with it.
- A named program dandegate cannot find is one warning on the page and in the report's log, `host program <name> is not on dandegate's PATH`. The run goes on and no step fails in advance: dandegate cannot know which steps use it. A step that does run it, and whose shell lacks it, fails as a missing shell tool does today (exit 127, with the error in its log).

Document the key where dandegate documents the steps file, and how commands reach host programs where it documents launching steps.

Everything else dandegate does stays as it is. Its existing tests must still pass, updating only an existing test whose asserted output this change alters. Add tests for the new behaviour beside the crate's own: unit tests of the key's parsing and scenarios through the command (a shell tool and a host program of the same name, a missing host program, a step out of time with `host` set).

`AGENTS.md` at the repository root says how to build and test it.
