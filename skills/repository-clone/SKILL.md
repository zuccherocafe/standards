---
name: repository-clone
description: "Get a working checkout of an existing zucchero✿café repository: clone it with jj, enter the Nix development shell, run the project's one-time steps from its AGENTS.md, and prove the checkout with the project's quick checks. Use when asked to clone or check out an existing zucchero✿café project, or to get one working on a new machine."
---

# Clone a zucchero✿café repository

The project's `AGENTS.md` is the authority for everything specific to it: its tools, its one-time steps, and its checks. Read it before running anything.

## Check the machine

The machine needs Lix with the `nix-command` and `flakes` features, git, and `gh` with SSH access to GitHub. If one is missing, stop and report it: installing them is the owner's choice. Every other tool comes from the project's development shell. To clone before that shell exists, run jj once from nixpkgs with `nix shell nixpkgs#jujutsu`.

## Clone it

1. Clone the repository with `jj git clone git@github.com:zuccherocafe/<project>.git`. jj colocates with git by default, so jj and git see the same commits and tools that need git still work.
2. Push nothing until the owner asks.

## Enter the development shell

1. With Lix, run `nix develop` at the repository root. The first entry builds or fetches every pinned tool, so it takes a while.
2. Entering the shell also links the standards' skills into `.agents/skills/` and `.claude/skills`. Enter it once before you start a tool that loads skills.
3. If the shell does not build, follow the manual installation steps in `AGENTS.md`'s Toolchain section.

## Run the one-time steps

1. Read `AGENTS.md`'s Toolchain and Commands sections, and list every step it marks as once per clone. Typical steps fetch a pinned database, place an archive the project cannot download, run a first build, or import an engine project.
2. Run each step inside the shell, in the order `AGENTS.md` gives.
3. Place a file the project says to place by hand only from the source it names. Let the project's own check verify the file's checksum.
4. Commit nothing these steps produce. The project's ignore rules cover their outputs.

## Prove the checkout works

1. Inside the shell, run the quick checks `AGENTS.md` names.
2. Run `git status`. It shows no changes.
3. If a check fails, do not patch the project to make it pass. Report the failure and the one-time step that should have prepared for it.

## Report

Report the commit you cloned, the one-time steps you ran, and the quick checks' result.
