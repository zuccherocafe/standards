# zucchero✿café standards

These are the engineering standards for zucchero✿café projects. They cover how we write, check, and review code, and hold the templates and agent skills a new project starts from.

## Reading them

Start with [general.md](general.md), which applies to every project. Then read the standard for your language in [languages/](languages/). [review.md](review.md) lists the questions every review answers.

## Using them in a project

A project's `AGENTS.md` links to these files and lists any departure from them with its reason. Where the two disagree, the project's `AGENTS.md` wins for that project. Its development shell pins this repository as a flake input, which provides the shared tools and the skills. The [setup-repository](skills/setup-repository/SKILL.md) skill sets a new project up this way.

## Licence

The prose is [CC-BY-4.0](LICENSE). The templates and tools are MIT-0. The skills copied from pstack keep their MIT licence ([skills/README.md](skills/README.md)).
