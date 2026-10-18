---
name: repository-create
description: "Set up a new zucchero✿café repository with jj colocated with git. Copy the `AGENTS.md` template and configure the Nix development shell with the standards flake input. Apply Rust and Godot templates where used, choose licences, and make standards skills available to agents. Use when asked to create or start a new zucchero✿café repository or project. For a checkout of an existing one, use repository-clone."
---

# Create a zucchero✿café repository

First pull the standards checkout to the latest `main`. Paths below are relative to that checkout. Before starting, read `general.md`'s Toolchain section, which says why a project copies tool configuration but pins shared tools. Read every template before copying it. Copy templates rather than reconstructing them from memory.

## Create the repository

1. Create the repository folder. Run `jj git init --colocate` in it. Use `jj` for history and `git` where a tool requires it. Both see the same commits.
2. Add no remote. Push nothing. The owner adds the remote when the owner asks for one.

## Write AGENTS.md

1. Copy `templates/AGENTS.md` to the repository root. Replace `<project>`.
2. Keep the Standards section as written. If the project has departures, list them. Otherwise, delete the placeholder line and its lead-in sentence.
3. Keep the Skills section as written.
4. Fill Toolchain, Checks, Layout, Dependency rules, and Commands with what exists today. Write "none yet" in a section with nothing to say.
5. Once the first step exists, write the orientation section, "How <the input> becomes <the output>". Until then, keep the heading and one sentence stating the intended input and output.

## Add the development shell

1. Copy `templates/nix/flake.nix` to the repository root. Replace `<project>` in `description`.
2. Keep the `standards` input with `github:zuccherocafe/standards` and `flake = false`. Keep the `rustScans` wrapper, which puts `rust-scans` on `PATH`. Keep the `shellHook` that links skills. For each other shared tool used from the standards' `tools/`, add a wrapper in the same way.
3. If the project uses Rust, point `rust` at its `rust-toolchain.toml`. Otherwise, remove `rust-overlay`, `rust`, and `rustScans`. Keep the `standards` input.
4. Add each tool used by checks with the `crate` or `official` helper. Use an exact version and hash, as the file header specifies.
5. Track `flake.nix`, create the lock, and then track `flake.lock` by running `git add flake.nix && nix flake lock && git add flake.lock`. A flake sees only tracked files.
6. Run `nix develop -c true` to show the shell builds.

## Copy the Rust template

For a project without Rust, skip this section.

1. Use the repository root as the Rust workspace, or `rust/` if the repository also contains other languages. Copy everything from `templates/rust/` except `tools/` into that workspace.
2. Rename `crates/example` to the first real crate. Update `members` and that crate's `description`.
3. Copy `templates/rust/tools/api.py` and `api_units.py` into the repository's `tools/`. Set `WORKSPACE` in `api.py` to the workspace folder.
4. Add the project's own entries only below the lines marked "project-specific".
5. Inside the shell, run `cargo build`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` in the workspace.

## Merge the Godot template

For a project without Godot, skip this section.

1. Merge `templates/godot/project.godot`'s `[debug]` section into the project's `project.godot`. Include each project-owned addon once in `directory_rules`.
2. Merge the `[*.gd]` section of `templates/godot/.editorconfig` into the root `.editorconfig`.
3. Pin the engine and the GDScript formatter in the flake with the `official` helper, by their publishers' digests.

## Add the licences

1. If the owner has not specified the licence, ask which licence the project uses.
2. Put the licence's full text in the root `LICENSE`. Set `[workspace.package]`'s `license` to its SPDX name. The template default is `GPL-3.0-or-later`.
3. If a crate uses another licence, give it its own `LICENSE` and `license` field. An example is an engine-independent core under MIT.
4. Add a README section named "Licence" that says which licence covers what, including any third-party material.

## Make the skills available to agents

1. Keep the template flake's `shellHook`. On `nix develop`, it links skills from the pinned `standards` input into `.agents/skills/`, which Codex and Cursor read. It removes links absent from the pin. Git ignores those links because they point to store paths.
2. The same hook links `.claude/skills` to `.agents/skills` because Claude Code reads only `.claude/skills/`. None of these links is committed. `.claude` is usually git-ignored, and the links follow the pin. Before starting a skill-loading tool in a new checkout, enter the shell once.
3. When you set up, check both paths against each tool's own documentation. The vendors' documentation is the authority, and a path can move.
4. Keep `AGENTS.md`'s Skills section for tools that load no skills, such as pi. It names the file each role reads.

## Make the first commit

1. Commit with a message as `general.md` says, such as `repository: add the starting layout`, and no attribution trailer.
2. Report what you created, which template sections still say "none yet", and the licence you set.
