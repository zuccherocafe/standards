# Change process design read

- Area: the change process in zuccherocafe/standards, covering briefs, design reads and their check, writing, reviewing and landing a change, and the coherence rules behind them
- Last full read: 3ddf4c52ef5079f3f4f977b1e3c62721dccc8b1f
- Inspected: `templates/BRIEF.md`, `templates/DESIGN-READ.md`, `skills/design-read/SKILL.md`, `skills/change-write/SKILL.md`, `skills/change-review/SKILL.md`, `skills/change-land/SKILL.md`, `tools/crates/design-read/` (src, tests, fixtures, Cargo.toml), `rules/general.md` (Design, Writing, Version control), `rules/review.md` (intro and First), with `flake.nix`, `templates/nix/flake.nix` and `templates/AGENTS.md` for the places they mention the tool and the skills

## Models in play

- A brief: [`templates/BRIEF.md`](../../templates/BRIEF.md) defines `Size` in one place. A separate `Design read` line gives a path, or `none` with a reason, as [When a read is due](../../skills/design-read/SKILL.md#when-a-read-is-due) decides. `Decided` lists each decision with its reason, and [Questions the writer settles](../../templates/BRIEF.md#questions-the-writer-settles) links [Record the reasons for decisions](../../skills/change-write/SKILL.md#record-the-reasons-for-decisions).
- An area file: one living file per subsystem area at `docs/design-reads/<area>.md`, described for readers in [`docs/design-reads/README.md`](README.md). [`templates/DESIGN-READ.md`](../../templates/DESIGN-READ.md) defines its header fields, prose sections, and disagreements table.
- The design-read skill: [`skills/design-read/SKILL.md`](../../skills/design-read/SKILL.md) is the one home of the coherence trigger ([When a read is due](../../skills/design-read/SKILL.md#when-a-read-is-due)), where reads live, the area file's lifecycle ([Keep the area file current](../../skills/design-read/SKILL.md#keep-the-area-file-current)), [who runs the check](../../skills/design-read/SKILL.md#what-design-read-check-checks), and the brief review's [independent sample](../../skills/design-read/SKILL.md#sample-the-area-in-a-brief-review). It links [`templates/DESIGN-READ.md`](../../templates/DESIGN-READ.md) for the file schema.
- The check: `tools/crates/design-read`, whose [`src/main.rs`](../../tools/crates/design-read/src/main.rs) module docs are the format's reference for `design-read check`. [What `design-read check` checks](../../skills/design-read/SKILL.md#what-design-read-check-checks) states when the writer runs it.
- A review: [Read the brief, evidence, and diff](../../skills/change-review/SKILL.md#read-the-brief-evidence-and-diff) defines the brief review, and [Answer the review questions and check what tools miss](../../skills/change-review/SKILL.md#answer-the-review-questions-and-check-what-tools-miss) checks decision reasons and the area file in the change review. [`rules/review.md`](../../rules/review.md#first) `First` asks the coherence questions and links the design-read skill.
- A decision's reason: [Record the reasons for decisions](../../skills/change-write/SKILL.md#record-the-reasons-for-decisions) states which decisions and settled questions need a public reason and links [`rules/general.md`](../../rules/general.md)'s [Documentation lives beside what it documents](../../rules/general.md#writing) and [Write scoped, imperative commit messages](../../rules/general.md#version-control).
- Landing: the writer commits the read and updates the area file on the branch as [Keep the area file current](../../skills/design-read/SKILL.md#keep-the-area-file-current) and [Commit one concept at a time](../../skills/change-write/SKILL.md#commit-one-concept-at-a-time) require. [`skills/change-land/SKILL.md`](../../skills/change-land/SKILL.md) fast-forwards the writer's branch and writes nothing.

## Overlaps

- `general.md` "Keep the system coherent" states the principle and links "When a read is due"; every other site links the skill rather than restating the trigger.
- The tool's module docs list what `design-read check` does not check, and the skill's "What stays human judgement" says once that it does not read the code. The two describe the same limit from the tool's side and the reviewer's side.

## Half-done transitions

- None found in what Inspected lists.

## What later work needs

- A project adopting the model adds `docs/design-reads/` with its README and a line in its `AGENTS.md`, as `templates/AGENTS.md` shows.

## Disagreements

None found.
