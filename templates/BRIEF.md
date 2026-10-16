# <name>: <the one concept, in a few words>

<!--
- Write a brief for one change for whoever builds it, whether a person or agent.
- Keep the brief to one page, with one line per item. Link to material rather than copy it.
- Keep every section. Write "none" where a section has no content. Delete this comment.
-->

<State the date, repository, base revision, and branch name. Name the writer and the review process.>

**Concept.** <State the one thing this change makes true, in a sentence the reviewer can check.>

**Closes.** <Identify the tracked items this change finishes by identifier. Identify separately any items it advances without finishing.>

**Decided.** <List each decision the change implements and where it is recorded. The writer does not reopen those decisions.>

## Read first

- <List the governing sections of `general.md`, the language standard, and `review.md`.>
- <Link the decisions above and the tracked items' acceptance criteria.>
- <Link earlier reviews or changes whose findings this change must preserve.>

## Sites

- `<file>`: `<symbol>`, `<symbol>`: <what changes there, and which item it serves>.

<Find each site by its symbol, not its line, because other changes land while this one is written.>

## Do

<Plan one commit per concept in an order that keeps every commit's checks passing. For each commit, state what changes and what stays the same.>

1. <commit concept>
2. <commit concept>

## Type sketch

```rust
// The new or changed types and signatures, before any code: fields, constructors, what each proves, and which bare
// value each replaces. "None" when the change adds no type.
```

## Must not change

<List what the reviewer must confirm is unchanged: written bytes, public behaviour, refusal codes, and the checks' verdict and running time. Give evidence for each item.>

## Evidence

<State where the writer leaves the evidence. Include check logs per commit and at the tip, a comparison of written files, and a report of differences. List every moved, new, or removed refusal with its reason.>

## Not this change

<List nearby work excluded from this change and where each item belongs, so the writer does not take it on.>

## Questions the writer settles

<List choices left to the writer and each choice's deciding constraint. The writer records each choice and its reason. For questions of intent beyond those choices, the writer asks rather than guesses.>

## Rules

<State the branch, clone, commit-message form, resource limits, and prohibitions on running or modifying specified things. Define the report's contents: work done, evidence paths, and every departure with its reason.>

The writer follows the standards' [change-write](https://github.com/zuccherocafe/standards/blob/main/skills/change-write/SKILL.md) skill. Where the skill and this brief differ, this brief wins.
