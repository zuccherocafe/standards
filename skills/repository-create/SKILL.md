---
name: repository-create
description: "Set up a new zucchero✿café repository with jj colocated with git. Copy the `AGENTS.md` template and configure the Nix development shell with the standards flake input. Apply Rust and Godot templates where used, choose licences, and make standards skills available to agents. Use when asked to create or start a new zucchero✿café repository or project. For a checkout of an existing one, use repository-clone."
---

# Create a zucchero✿café repository

First pull the standards checkout to the latest `main`. Paths below are relative to that checkout. Before starting, read `rules/general.md`'s Toolchain section, which says why a project copies tool configuration but pins shared tools. Read every template before copying it. Copy templates rather than reconstructing them from memory.

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

1. Copy `templates/nix/flake.nix` to the repository root. Replace `<project>` in `description`. Fetch inputs as tarballs (`github:` or a tarball URL), never `git+https`. The flake and its lock are tracked by git, since a flake sees only tracked files; the shell is written for Lix and kept to what other Nix implementations also evaluate.
2. Keep the `standards` input, `github:zuccherocafe/standards`, as a flake. Keep `dandegate`, the standards' change report, which puts `dandegate` on `PATH`. A project with steps of its own declares them in a root `dandegate.toml`, and a Rust workspace gets the Rust reports without one. A step finds only the tools its revision's shell puts on `PATH`, so the shell holds every tool the steps run. Keep `rustScans` and `rustMutants`, the standards' `rust-scans` and `rust-mutants` packages, which put those commands on `PATH`. Keep `pinAge`, the standards' `pin-age` package, which says when a pin is due to move ([Toolchain](../../rules/general.md#toolchain)). Add `pin-age --lock flake.lock` to the quick checks `AGENTS.md` names. A Rust project also passes `--toolchain <workspace>/rust-toolchain.toml`. Keep the `shellHook` that links skills. For each other shared tool, use the standards' package where the standards export one, and otherwise add a `pkgs.writeShellScriptBin` wrapper that runs its script.
3. If the project uses Rust, choose its workspace as [Copy the Rust template](#copy-the-rust-template) says, and copy `templates/rust/rust-toolchain.toml` into that workspace. Track the copied `rust-toolchain.toml` with `git add` and point `rust` at it now, because the shell reads that file the first time it is entered. Otherwise, remove `rust-overlay`, `rust`, and `rustScans`. Keep the `standards` input.
4. Add each tool used by checks with the `crate` helper (a cargo tool from crates.io) or the `official` helper (a published binary, such as an engine, fetched by its publisher's digest and installed unchanged), with an exact version and hash, and add it to `packages`. To learn a hash, write `lib.fakeHash`, enter the shell and copy the hash the error reports; for a published binary, prefer the digest its publisher prints. A Rust project pins cargo-mutants this way, for `rust-mutants`, and `cargo-public-api` at the release that reads its pinned nightly's rustdoc JSON, for dandegate's `api` step. The two pins move together. A Python project pins its interpreter, Ruff and Pyrefly, through the lock or these helpers. Give each program the project's scripts find by a variable, such as `ENGINE_BIN`, that variable in the shell. A pin moves in the same change as the file that checks it.
5. Track `flake.nix`, create the lock, and then track `flake.lock` by running `git add flake.nix && nix flake lock && git add flake.lock`. A flake sees only tracked files.
6. Run `nix develop -c true` to show the shell builds.

## Copy the Rust template

For a project without Rust, skip this section.

1. Use the repository root as the Rust workspace, or `rust/` if the repository also contains other languages. Copy the rest of `templates/rust/` into that workspace, beside the `rust-toolchain.toml` the shell already reads.
2. Rename `crates/example` to the first real crate. Update `members` and that crate's `description`.
3. Fill in the project's values: its crates in `members`, each crate's one-sentence `description` agreeing with the first line of its `//!` documentation, its licence, each dependency once in `[workspace.dependencies]` as `major.minor`, and any lints of its own after the shared ones.
4. Inside the shell, run `cargo build`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` in the workspace.

## Merge the Godot template

For a project without Godot, skip this section.

1. Merge `templates/godot/project.godot`'s `[debug]` section into the project's `project.godot`. Include each project-owned addon once in `directory_rules`; anything else under addons stays excluded. A project not yet clean of a typing warning leaves its line out, lists it as a departure in `AGENTS.md` with its count, and brings them in one at a time, each with its sites fixed: `untyped_declaration`; then `unsafe_property_access` and `unsafe_cast`; then `unsafe_method_access` and `unsafe_call_argument`.
2. Merge the `[*.gd]` section of `templates/godot/.editorconfig` into the root `.editorconfig`, rather than replacing an existing one.
3. Pin the engine and the GDScript formatter in the flake with the `official` helper, by their publishers' digests.

## Add the Python configuration

For a project without Python, skip this section.

1. Copy `templates/python/ruff.toml` and `pyrefly.toml` to the repository root. Set the target Python (the latest stable release the shell pins), the folders the checker reads (`project-includes`), and its `search-path` (each folder a script adds to its import path, and any host's type stubs). A folder of scripts that run inside a host gets its own `pyrefly.toml` with the host's Python, and Ruff's `per-file-target-version` sets it there too.
2. Pin the interpreter, Ruff and Pyrefly in the flake by exact version, through the lock or the `crate` or `official` helper.
3. Inside the shell, run `ruff format --check`, `ruff check`, and `pyrefly check`, and name all three in `AGENTS.md`'s checks.

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

## Remove the template's instructions

A copied file keeps only what applies to the project. Delete any instruction or placeholder the copy still carries, and keep a comment only where a value needs a reason a reader couldn't guess.

## Make the first commit

1. Commit with a message as `general.md` says, such as `repository: add the starting layout`, and no attribution trailer.
2. Report what you created, which template sections still say "none yet", and the licence you set.
