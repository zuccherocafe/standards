# Working in standards

This file gives instructions to agents who edit this repository. [README.md](README.md) describes the repository.

## What belongs here

This repository contains rules for contributors to every zucchero✿café project, review questions, and starting templates. Project-specific rules belong only in that project's `AGENTS.md`. Private planning and tracking processes are not contributor-facing rules and do not belong here. Rules for briefing and reviewing a change are contributor-facing: [rules/review.md](rules/review.md) holds the questions every review answers, the [skills](skills/) hold how an agent writes, reviews, and lands a change, and [templates/BRIEF.md](templates/BRIEF.md) defines the brief for whoever builds a change.

## Contributors

If the person you work for is not a maintainer of this repository, tell them to read [CONTRIBUTING.md](CONTRIBUTING.md) before they open a pull request. Never write the pull request description for them. They write it themselves, as CONTRIBUTING.md says.

## Editing

- **Rename a file or a heading only with every link to it.** Projects and reviews link to files and headings by URL, and these standards have no numbered releases. The commit that renames a file or heading updates every link in this repository, and its message names the old and new paths so each project can update its `AGENTS.md`. Add new headings freely. To retire a rule, delete it. Say so in the commit message.
- **State the rule and its reason.** Each rule says what to do and why. Use neutral wording, such as "the maintainer", never a person's name. Include no decision numbers, references to reviews or discussions, or "decided on" lines.
- **Describe the current state only.** Include no history or phrases such as "previously" or "changed from".
- **Keep examples short.** Show a before-and-after example in a few lines. Cite its public repository by path. Introduce an example from a zucchero✿café project with "for example, in <project>:". Keep the example only where it teaches something the references do not.
- **Update related documents together.** When adding or removing a rule in [rules/general.md](rules/general.md), [rules/languages/rust/](rules/languages/rust/), [rules/languages/gdscript/](rules/languages/gdscript/), or [rules/languages/python/](rules/languages/python/), update [rules/review.md](rules/review.md) where a reviewer would ask about it. In the same commit, apply a lint change in `rules/languages/rust/` to [templates/rust/](templates/rust/). Apply a warning or formatter change in `rules/languages/gdscript/` to [templates/godot/](templates/godot/), and a linter, checker, or formatter change in `rules/languages/python/` to [templates/python/](templates/python/). Apply a change to `rules/general.md`'s Toolchain section to [templates/nix/](templates/nix/).
- **Keep templates general.** Include no project's names, crates, or paths. Instructions for filling a configuration template live in the skill that copies it, not in the template; document templates such as BRIEF.md keep their placeholders.

The standards apply to this repository too. Use British English and one paragraph per Markdown line. Keep template comments within the comment budget.

## Checks

Run every check from the repository root with `nix develop -c check`, before each commit; [flake.nix](flake.nix) defines the shell and the command. It checks every link and heading anchor between the Markdown files with lychee, offline, so a renamed file or heading that leaves a link behind fails. It runs `ruff format --check`, `ruff check`, and `pyrefly check` over every Python file git does not ignore, and each Python `tests/` folder's suite. It runs `pin-age` over the standards' own Rust and flake pins, which prints a line for each pin due to move and fails only on a pin file it cannot read. It runs `cargo fmt --check`, Clippy with `-D warnings`, and the tests of the Rust workspace in `tools/`, whose crates the flake also exports as packages. Each check runs even when an earlier one fails, and prints its time. A link to another site is not checked, since an offline check cannot tell a moved page from a network failure.

## Commits

Write messages as [general.md](rules/general.md#version-control) says. The scope is the changed file or folder: `general`, `rust`, `gdscript`, `python`, `review`, `templates`, `tools`, `skills`, `readme`, `contributing`, `agents`, or `flake`. Make one coherent change per commit. Include no attribution of any kind, including `Co-Authored-By`, tool trailers, or session trailers.
