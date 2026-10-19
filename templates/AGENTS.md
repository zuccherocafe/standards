# Working in <project>

This file gives instructions for coding agents. The README provides the human overview. This file is the operating manual. Read the governing documents before changing what they cover.

## Standards

This project follows [zuccherocafe/standards](https://github.com/zuccherocafe/standards) at the latest commit on `main`. Before changing code, read [general.md](https://github.com/zuccherocafe/standards/blob/main/rules/general.md). Also read the standard for the changed language, [the Rust standard](https://github.com/zuccherocafe/standards/tree/main/rules/languages/rust), [the GDScript standard](https://github.com/zuccherocafe/standards/tree/main/rules/languages/gdscript) or [the Python standard](https://github.com/zuccherocafe/standards/tree/main/rules/languages/python). Review against [review.md](https://github.com/zuccherocafe/standards/blob/main/rules/review.md). For offline reading, use a sibling checkout at `../standards`. Pull it first.

Where this file and the standards disagree, this file wins for this project. Each departure is listed here with its reason:

- <rule and file>: <what this project does instead>, because <reason>.

## Skills

The [standards skills](https://github.com/zuccherocafe/standards/tree/main/skills) define each role's work. A tool that loads skills loads them by name. Other agents read the file for their role before starting, as follows:

- Writing a change from a brief: [change-write](https://github.com/zuccherocafe/standards/blob/main/skills/change-write/SKILL.md).
- Reviewing a change: [change-review](https://github.com/zuccherocafe/standards/blob/main/skills/change-review/SKILL.md).
- Checking fixes and landing a change: [change-land](https://github.com/zuccherocafe/standards/blob/main/skills/change-land/SKILL.md).
- Any prose, commit message, or report: [technical-writing](https://github.com/zuccherocafe/standards/blob/main/skills/technical-writing/SKILL.md) and [unslop](https://github.com/zuccherocafe/standards/blob/main/skills/unslop/SKILL.md).

The skills' quick and full checks refer to commands in this file's Checks section. This file's house rules apply to every role, including resource limits and prohibitions on running or modifying specified things.

## Authorities

| Topic | Document |
|---|---|
| <topic> | <document> |

## Toolchain

With Lix, enter the development shell using `nix develop`, or run one command using `nix develop -c <command>`. The shell provides every tool below at its pinned version. Track `flake.nix` and `flake.lock` in git, since a flake sees only tracked files. Without the shell, use the fallback steps below.

<List the exact tool versions the project needs and their sources. Include the language toolchain pinned in `rust-toolchain.toml`, tools used by checks, and manually installed tools. State which component reads each environment variable.>

## Checks

Run full checks at the change's tip after its last commit and after every rebase. They must pass before anyone declares the change complete. Use this acceptance command:

    <the one command that runs every check>

The quick checks run before every commit and while iterating. They are never the acceptance signal:

    <the command that runs the quick checks>

<Describe each check layer's coverage and running time. State which layers the quick checks run. Keep the general checking policy in `general.md`'s Checks section. Record only project-specific details here.>

## How <the input> becomes <the output>

<In about thirty lines, orient the reader to where work starts. The current design belongs in each module's `//!` documentation and types. Link to that design without repeating any type, invariant, or signature.>

<In one paragraph, describe what a person supplies and how it becomes the project's output.>

**<Step>.** <Name the trigger, entry function, and file. State what the step reads, writes, and refuses. Triggers may include a command, script, or engine.>

**<Step>.** <Repeat for each step in data-flow order.>

The following rules hold at every step:

- <List the few checkable rules shared by all steps. Identify components that compute over values and components that do input and output. State where inputs are parsed, how outputs are installed, and who writes each output.>

## Layout

<Show a directory tree with one line per directory stating its single job. Identify generated files and each file's sole writer.>

## Dependency rules

<State which components may depend on which others and which check enforces those dependencies.>

## Conventions

<State only project-specific additions, such as line lengths, file kinds, and naming conventions. Do not repeat the standards.>

## Commands

    # <what it does>
    <command>
