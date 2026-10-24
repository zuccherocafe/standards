# Change process concord note

- Area: the change process in zuccherocafe/standards, covering briefs, concord notes and their check, writing, reviewing and landing a change, and the coherence rules behind them
- Last full read: 3ddf4c52ef5079f3f4f977b1e3c62721dccc8b1f
- Inspected: `templates/BRIEF.md`, `templates/CONCORD.md`, `skills/concord/SKILL.md`, `skills/change-write/SKILL.md`, `skills/change-review/SKILL.md`, `skills/change-land/SKILL.md`, `tools/crates/concord/` (src, tests, fixtures, Cargo.toml), `rules/general.md` (Design, Writing, Version control), `rules/review.md` (intro and First), with `flake.nix`, `templates/nix/flake.nix` and `templates/AGENTS.md` for the places they mention the tool and the skills

## Models in play

- A brief: [`templates/BRIEF.md`](../../templates/BRIEF.md) defines `Size` in one place. A separate `Concord note` line gives a path, or `none` with a reason, as [When a concord note is due](../../skills/concord/SKILL.md#when-a-concord-note-is-due) decides. `Decided` lists each decision with its reason, and [Questions the writer settles](../../templates/BRIEF.md#questions-the-writer-settles) links [Record the reasons for decisions](../../skills/change-write/SKILL.md#record-the-reasons-for-decisions).
- A concord note: one living file per subsystem area at `docs/concord/<area>.md`, described for readers in [`docs/concord/README.md`](README.md). [`templates/CONCORD.md`](../../templates/CONCORD.md) defines its header fields, prose sections, and disagreements table.
- The concord skill: [`skills/concord/SKILL.md`](../../skills/concord/SKILL.md) is the one home of the coherence trigger ([When a concord note is due](../../skills/concord/SKILL.md#when-a-concord-note-is-due)), where notes live, the concord note's lifecycle ([Keep the concord note current](../../skills/concord/SKILL.md#keep-the-concord-note-current)), [who runs the check](../../skills/concord/SKILL.md#what-concord-check-checks), and the brief review's [independent sample](../../skills/concord/SKILL.md#sample-the-area-in-a-brief-review). It links [`templates/CONCORD.md`](../../templates/CONCORD.md) for the file schema.
- The check: `tools/crates/concord`, whose [`src/main.rs`](../../tools/crates/concord/src/main.rs) module docs are the format's reference for `concord check`. [What `concord check` checks](../../skills/concord/SKILL.md#what-concord-check-checks) states when the writer runs it.
- A review: [Read the brief, evidence, and diff](../../skills/change-review/SKILL.md#read-the-brief-evidence-and-diff) defines the brief review, and [Answer the review questions and check what tools miss](../../skills/change-review/SKILL.md#answer-the-review-questions-and-check-what-tools-miss) checks decision reasons and the concord note in the change review. [`rules/review.md`](../../rules/review.md#first) `First` asks the coherence questions and links the concord skill.
- A decision's reason: [Record the reasons for decisions](../../skills/change-write/SKILL.md#record-the-reasons-for-decisions) states which decisions and settled questions need a public reason and links [`rules/general.md`](../../rules/general.md)'s [Documentation lives beside what it documents](../../rules/general.md#writing) and [Write scoped, imperative commit messages](../../rules/general.md#version-control).
- Landing: the writer commits the note and updates it on the branch as [Keep the concord note current](../../skills/concord/SKILL.md#keep-the-concord-note-current) and [Commit one concept at a time](../../skills/change-write/SKILL.md#commit-one-concept-at-a-time) require. [`skills/change-land/SKILL.md`](../../skills/change-land/SKILL.md) fast-forwards the writer's branch and writes nothing.

## Overlaps

- `general.md` "Keep the system coherent" states the principle and links "When a concord note is due"; every other site links the skill rather than restating the trigger.
- The tool's module docs list what `concord check` does not check, and the skill's "What stays human judgement" says once that it does not read the code. The two describe the same limit from the tool's side and the reviewer's side.

## Half-done transitions

- None found in what Inspected lists.

## What later work needs

- A project adopting the model adds `docs/concord/` with its README and a line in its `AGENTS.md`, as `templates/AGENTS.md` shows.

## Disagreements

None found.
