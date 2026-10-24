---
name: concord
description: "Read an area of a project, record and maintain where its models disagree in the area's concord note, `docs/concord/<area>.md`, from `templates/CONCORD.md`, check the note with `concord check`, and sample the area in a brief review. Use when deciding whether a brief needs a concord note, or when writing, updating, checking, or reviewing one."
---

# Keep an area's models in concord

Concord is how well an area's models fit together and agree. Each project keeps one living document per subsystem area at `docs/concord/<area>.md`, recording where the area's models disagree. That document is the area's concord note. The notes in `docs/concord/` are the list of areas, and `docs/concord/README.md` describes them for readers without being parsed.

## When a concord note is due

A brief needs a new read of its area, recorded in the area's concord note, when it (a) adds a representation, calculation rule or owner for a concept the project already models elsewhere or that planned work will build on; (b) changes what another module or step must assume about an existing model's contract, invariant or owner, not a fix inside one owner that keeps its callers' assumptions; or (c) starts, resumes or works around an unfinished transition between two ways of doing the same thing. When the note's last full read already covers what the brief touches and nothing has landed in the area since, the brief cites the note instead of a new read.

## What to inspect

Read the code and contracts in the area at a single revision, following [Keep the system coherent](../../rules/general.md#design) and [Design for where the system is going](../../rules/general.md#design). Inspect the models in play, their overlaps, half-done transitions, what later work needs, and where the models disagree.

## Fill the template

[`templates/CONCORD.md`](../../templates/CONCORD.md) defines the header fields, sections, and columns of a concord note. Copy it to `docs/concord/<area>.md`, or update the existing note there, and delete the template's HTML comment. When adding a new note, add a line for it to `docs/concord/README.md`.

1. Fill the header fields as [`templates/CONCORD.md`](../../templates/CONCORD.md) defines them.
2. Write the four prose sections in the template's order as free text for people. Cite code and documentation, never private decision numbers.
3. In [`## Disagreements`](../../templates/CONCORD.md#disagreements), fill one row per open disagreement as the template defines the columns. Never name a private brief as `Owner`. Escape any literal pipe inside a cell as `\|`.
   - The table is a search result rather than a measurement, and records what was found in what `Inspected` lists, not all there is. When the search finds no open disagreements in what `Inspected` lists, replace the table with the sentence `None found.` and still list what was read in `Inspected`.
   - Briefs and the prose sections cite a disagreement by its `Concept`.

## Keep the concord note current

The new or updated note is the first commit of the change's branch, made by the writer from the note the brief links, so an abandoned branch leaves nothing on `main`. The change's landing commit deletes the rows the change settled and corrects the prose, leaving `Last full read` unchanged. Any change, whatever its size, that settles a row or makes the prose wrong updates the note in place. A disagreement owned by another area moves to that area's note, or stays in the discovering note with that area as `Owner` while the owning area has no note. Two changes in one area reconcile the note at merge.

## What `concord check` checks

The writer runs `concord check` before committing the note and before committing the landing update. [`tools/crates/concord/src/main.rs`](../../tools/crates/concord/src/main.rs) is the format's reference and defines the rules, arguments, output, and exit codes.

## What stays human judgement

`concord check` does not read the code that the note names. The author and the reviewer judge these points:

- Whether the named sites are right.
- Whether the read inspected enough of the area.
- Whether a disagreement matters.

## Sample the area in a brief review

When a [brief review](../change-review/SKILL.md#read-the-brief-evidence-and-diff) covers a concord note, take one bounded independent sample of the area. Trace one producer-to-consumer contract, or an equivalent relationship where there is none, chosen independently, and prefer a relevant site the note does not name, otherwise inspect a named one and say why.

A material omission in the note or the brief means the note or the brief is corrected before the writer starts. It does not mean every disagreement in the note must be cleared first.
