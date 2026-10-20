---
name: repository-create
description: "Set up a new zucchero✿café repository with jj colocated with git. Copy the `AGENTS.md` template and configure the Nix development shell with the standards flake input. Apply Rust and Godot templates where used, choose licences, and make standards skills available to agents. Use when asked to create or start a new zucchero✿café repository or project. For a checkout of an existing one, use repository-clone."
---

# Create a zucchero✿café repository

First pull the standards checkout to the latest `main`. Paths below are relative to that checkout. Before starting, read `rules/general.md`'s Toolchain section, which says why a project copies tool configuration but pins shared tools. Read every template before copying it. Copy templates rather than reconstructing them from memory.

## Create the repository

1. Create the repository folder and run `jj git init --colocate` in it, as [Version control](../../rules/general.md#version-control) specifies.
2. Add no remote. Push nothing. The owner adds the remote when the owner asks for one.

## Write AGENTS.md

1. Copy `templates/AGENTS.md` to the repository root. Replace `<project>`.
2. Keep the Standards section as written. If the project has departures, list them. Otherwise, delete the placeholder line and its lead-in sentence.
3. Keep the Skills section as written.
4. Fill Toolchain, Checks, Layout, Dependency rules, and Commands with what exists today. Write "none yet" in a section with nothing to say.
5. Once the first step exists, write the orientation section, "How <the input> becomes <the output>". Until then, keep the heading and one sentence stating the intended input and output.

## Add the development shell

1. Copy `templates/nix/flake.nix` to the repository root and replace `<project>` in `description`. Use `github:` or a tarball URL for flake inputs, never `git+https`.
2. Keep the `standards` flake input, `github:zuccherocafe/standards`, together with `dandegate`, `rustScans`, `rustMutants`, `pinAge`, `designRead`, and the `shellHook` that links skills. Add `pin-age --lock flake.lock` to the quick checks in `AGENTS.md`, and for a Rust project also pass `--toolchain <workspace>/rust-toolchain.toml` ([Toolchain](../../rules/general.md#toolchain)). For a Rust project, write a root `dandegate.toml` whose `[project.rust]` table names the workspace folder, as `workspace = "rust/"`, which turns on the Rust reports. [dandegate's plan reader](../../tools/crates/dandegate/src/plan.rs) describes the file's other tables and its path rules. A step finds only the tools its revision's shell puts on `PATH`, so the shell holds every tool the steps run, except host programs such as a GPU binary or a compositor, which go in the `host` list of `dandegate.toml`'s `[project]` table. For any other shared tool, use the package that `standards` exports, or wrap its script with `pkgs.writeShellScriptBin`.
3. If the project uses Rust, choose its workspace directory as [Copy the Rust template](#copy-the-rust-template) says, and copy `templates/rust/rust-toolchain.toml` into that workspace. Track the copied `rust-toolchain.toml` with `git add` and point `rust` at it now, because the shell reads that file the first time it is entered. Otherwise, remove `rust-overlay`, `rust`, and `rustScans`, and keep the `standards` input.
4. Add each tool the checks run to `packages`, pinned with the `crate` helper for crates.io binaries or the `official` helper for publisher binaries ([Toolchain](../../rules/general.md#toolchain)). For a published binary, use the digest its publisher prints. Otherwise, set `lib.fakeHash`, enter the shell, and copy the hash from the error message. For a Rust project, pin `cargo-mutants` for `rust-mutants` and `cargo-public-api` for dandegate's `api` step this way ([Toolchain and lints](../../rules/languages/rust/README.md#toolchain-and-lints)). Export any environment variable that project scripts use to locate a binary, such as `ENGINE_BIN`, from the shell.
5. Track `flake.nix`, create the lockfile, and track `flake.lock` by running `git add flake.nix && nix flake lock && git add flake.lock`.
6. Run `nix develop -c true` to show the shell builds.

## Copy the Rust template

For a project without Rust, skip this section.

1. Use the repository root as the Rust workspace, or `rust/` if the repository also contains other languages. Copy the rest of `templates/rust/` into that workspace, beside the `rust-toolchain.toml` the shell already reads.
2. Rename `crates/example` to the first real crate. Update `members` and that crate's `description`.
3. Fill in `Cargo.toml` for the project: list its crates in `members`, set the workspace `license`, and add any project-specific lints after the shared table. Follow [Toolchain and lints](../../rules/languages/rust/README.md#toolchain-and-lints) for `[workspace.dependencies]` and [Crates](../../rules/languages/rust/design.md#crates) for each crate's `description`.
4. Inside the shell, run `cargo build`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` in the workspace.

## Merge the Godot template

For a project without Godot, skip this section.

1. Merge `templates/godot/project.godot`'s `[debug]` section into the project's `project.godot`. Configure `directory_rules` and any staged typing-warning departures as [Warnings](../../rules/languages/gdscript/warnings.md#warnings) specifies.
2. Merge the `[*.gd]` section of `templates/godot/.editorconfig` into the root `.editorconfig` rather than replacing an existing file.
3. Pin the engine and the GDScript formatter in `flake.nix` with the `official` helper.

## Add the Python configuration

For a project without Python, skip this section.

1. Copy `templates/python/ruff.toml` and `templates/python/pyrefly.toml` to the repository root. Set the target Python version, the folders the checker reads in `project-includes`, `search-path`, and any host-specific configuration as [Toolchain](../../rules/languages/python/README.md#toolchain) specifies.
2. Pin the Python interpreter, Ruff, and Pyrefly in `flake.nix` as [Toolchain](../../rules/languages/python/README.md#toolchain) specifies.
3. Inside the shell, run `ruff format --check`, `ruff check`, and `pyrefly check`, and name all three in `AGENTS.md`'s checks.

## Add the licences

1. If the owner has not specified the licence, ask which licence the project uses.
2. Put the licence's full text in the root `LICENSE`. Set `[workspace.package]`'s `license` to its SPDX name. The template default is `GPL-3.0-or-later`.
3. If a crate uses another licence, give it its own `LICENSE` and `license` field. An example is an engine-independent core under MIT.
4. Add a README section named "Licence" that says which licence covers what, including any third-party material.

## Make the skills available to agents

1. Keep the template flake's `shellHook`. On `nix develop`, it links skills from the pinned `standards` input into `.agents/skills/`, which Codex and Cursor read. It removes links absent from the pin. Git ignores those links because they point to store paths.
2. The same hook links `.claude/skills` to `.agents/skills` because Claude Code reads only `.claude/skills/`. Do not commit those links. `.claude` is usually git-ignored, and the links follow the pin. Enter the shell once before starting a skill-loading tool in a new checkout.
3. When you set up, check both paths against each tool's own documentation. The vendors' documentation is the authority, and a path can move.
4. Keep `AGENTS.md`'s Skills section for tools that load no skills, such as pi. It names the file each role reads.

## Remove the template's instructions

Keep only what applies to the project in each copied file. Delete every template instruction and placeholder, and follow [Configuration files carry few comments](../../rules/general.md#writing).

## Make the first commit

1. Commit with a message as `general.md` says, such as `repository: add the starting layout`, and no attribution trailer.
2. Report what you created, which template sections still say "none yet", and the licence you set.
