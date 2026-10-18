# Working in standards

This file gives instructions to people and agents who edit this repository. [README.md](README.md) describes the repository.

## What belongs here

This repository contains rules for contributors to every zucchero✿café project, review questions, and starting templates. Project-specific rules belong only in that project's `AGENTS.md`. Private planning and tracking processes are not contributor-facing rules and do not belong here. Rules for briefing and reviewing a change are contributor-facing: [review.md](review.md) holds the questions every review answers, the [skills](skills/) hold how an agent writes, reviews, and lands a change, and [templates/BRIEF.md](templates/BRIEF.md) defines the brief for whoever builds a change.

## Editing

- **Rename a file or a heading only with every link to it.** Projects and reviews link to files and headings by URL, and these standards have no numbered releases. The commit that renames a file or heading updates every link in this repository, and its message names the old and new paths so each project can update its `AGENTS.md`. Add new headings freely. To retire a rule, delete it. Say so in the commit message.
- **State the rule and its reason.** Each rule says what to do and why. Use neutral wording, such as "the maintainer", never a person's name. Include no decision numbers, references to reviews or discussions, or "decided on" lines.
- **Describe the current state only.** Include no history or phrases such as "previously" or "changed from".
- **Keep examples short.** Show a before-and-after example in a few lines. Cite its public repository by path. Introduce an example from a zucchero✿café project with "for example, in <project>:". Keep the example only where it teaches something the references do not.
- **Update related documents together.** When adding or removing a rule in [general.md](general.md), [languages/rust/](languages/rust/), [languages/gdscript/](languages/gdscript/), or [languages/python/](languages/python/), update [review.md](review.md) where a reviewer would ask about it. In the same commit, apply a lint change in `languages/rust/` to [templates/rust/](templates/rust/). Apply a warning or formatter change in `languages/gdscript/` to [templates/godot/](templates/godot/), and a linter, checker, or formatter change in `languages/python/` to [templates/python/](templates/python/). Apply a change to `general.md`'s Toolchain section to [templates/nix/](templates/nix/).
- **Keep templates general.** Include no project's names, crates, or paths. Mark where project-specific entries go.

The standards apply to this repository too. Use British English and one paragraph per Markdown line. Keep template comments within the comment budget.

## Commits

Write messages as [general.md](general.md#version-control) says. The scope is the changed file or folder: `general`, `rust`, `gdscript`, `python`, `review`, `templates`, `tools`, `skills`, `readme`, or `agents`. Make one coherent change per commit. Include no attribution of any kind, including `Co-Authored-By`, tool trailers, or session trailers.
