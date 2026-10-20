---
name: design-read
description: "Read an area before a phase or a design-sized brief, record where its models disagree in `templates/DESIGN-READ.md`, check the read with `design-read check`, and sample the area in brief review. Use when writing, checking, or reviewing a design read."
---

# Read an area's design

Before a phase or a design-sized brief, read the area and record where its models disagree in one Markdown file. Every read has the same shape so `design-read check` can check it, and each read carries forward what the previous read left open so no disagreement drops out of the series unnoticed.

## When a read is due

Write a design read before a phase, and before any brief of design size as `templates/BRIEF.md` defines it, adding or changing a format, type, API or dependency.

## What to inspect

Read the code and contracts in the area at a single revision, following [Keep the system coherent](../../rules/general.md#design) and [Design for where the system is going](../../rules/general.md#design). Inspect these four things:

- The models in the area: the types, records, and owners that hold each concept, and where each lives.
- Overlaps where two models hold the same concept, or where one value has two owners.
- Half-done transitions that started and did not finish, and what still uses the old path.
- What later phases need from the models in this area.

## Fill the template

Copy [`templates/DESIGN-READ.md`](../../templates/DESIGN-READ.md) and delete its HTML comments. Keep `Models in play`, `Overlaps`, `Half-done transitions`, and `What later phases need` as free prose for people.

1. Fill the four `- Field: value` header lines before the first `## ` heading:
   - `Area`: what was read, in one line.
   - `Revision`: the 40-digit lowercase hexadecimal commit ID of the code read.
   - `Previous`: the path to the previous read relative to this file, or `none` in lowercase for a first read.
   - `Inspected`: the contracts, folders, or files read, in one line.
2. Write the `**Answer.**` paragraph stating what must happen before the phase starts, what the phase's brief settles, and what goes to the maintainer.
3. In a first read, omit `## After the last phase`. When `Previous` names an earlier read, include `## After the last phase` and state what the last phase changed in the area.
4. In `## Disagreements`, fill one row per disagreement with the columns `ID | Concept | Owner | Sites | Consequence | Done when | Status`. Escape any literal pipe inside a cell as `\|`.
   - Give each new disagreement a read-qualified ID `<read>-D<n>` in lowercase letters, digits, and hyphens followed by `-D` and a positive number, such as `mm4-D1`, and status `new`.
   - Carry forward every ID that the previous read left `new`, `open`, or `parked`, with status `open`, `resolved`, or `parked`. Use `parked` when the disagreement is outside this read's area and still unresolved, and name where it is tracked in `Done when`. An ID that the previous read marked `resolved` may be omitted.
   - When the read has no rows to record, because it finds no new disagreements and the previous read left none `new`, `open`, or `parked`, replace the table with the sentence `None found.` and still list what was read in `Inspected`. A read that resolves a carried ID keeps that row.

## What `design-read check` checks

Run `design-read check <file>` to check the header, the after-phase note, the disagreements table, and what the read carries forward from the previous one. It does not read the code the read names. See [`tools/crates/design-read/src/main.rs`](../../tools/crates/design-read/src/main.rs) for the exact rules, exit codes, and output.

## What stays human judgement

`design-read check` does not read the code that the read names. The author and the reviewer judge these points:

- Whether the named sites are right.
- Whether the read inspected enough of the area.
- Whether a disagreement matters.
- Whether a carried ID kept its meaning from read to read. `Concept` may be reworded, as long as it names the same disagreement.
- Whether a `parked` row's `Done when` cell states where the disagreement is tracked.

## Check before dispatching a writer

Before dispatching a writer for a phase or a brief of design size, the coordinator runs `design-read check <file>` on the linked read and records the result beside the brief. When the brief's scope or contracts change, the coordinator re-runs `design-read check <file>` on the updated read and records the result beside the brief before a writer continues.

## Sample the area in a brief review

In a brief review of a design-sized brief, run `design-read check <file>` and take one bounded independent sample of the area. Trace one producer-to-consumer contract, or an equivalent relationship where there is none, chosen independently, and prefer a relevant site the read does not name, otherwise inspect a named one and say why.

A material omission in the read or the brief means the read or the brief is corrected before the writer starts. It does not mean every disagreement in the read must be cleared first.

## Record the after-phase note in the next read

Do not edit an earlier read after its phase lands. Put the after-phase note at the top of the next read, in its `## After the last phase` section, with `Previous` set to the earlier read's relative path.
