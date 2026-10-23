# Check living design reads, one file per area

`tools/crates/design-read` is design-read. Today `design-read check <file>` checks one design read in a chain: each read names its previous read, and the tool checks that it carries forward every disagreement the previous one left open. Its `src/main.rs` module documentation and its tests describe that format.

A project now keeps one living design read per area instead, `docs/design-reads/<area>.md`, corrected in place as changes land, so there is no previous read to compare with. Change design-read to check this format and drop the chain entirely: the `Previous` field, the `## After the last phase` section, the carry-forward check, row IDs, statuses and the status summary go, with the tests and fixtures that exist only for them. Keep the fenced-code and table reading that still serves.

## The format

```markdown
# <Area> design read

- Area: <what the area covers, one line>
- Last full read: <the full commit id at which the area was last read in full>
- Inspected: <the contracts, folders or files read, one line>

## Models in play
## Overlaps
## Half-done transitions
## What later work needs

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| <short noun phrase, unique in the file> | <subsystem, module or maintainer> | `<file>` `<symbol>`; ... | <what goes wrong while it stands> | <the condition that ends it> |
```

Or the sentence `None found.` in the Disagreements section in place of the table. The prose sections are free text for people, and the tool does not check them. As today, headings, tables and fields inside fenced code blocks are examples and never count.

## What check refuses

- **Header**: the `- Field: value` lines before the first level-two heading. It holds exactly `Area`, `Last full read` and `Inspected`, each once and non-empty. Any other field there, such as `Previous` or `Revision`, is refused. Lines there that are not `- Field: value` are prose. `Last full read` is a full commit id, 40 lowercase hexadecimal digits.
- **Disagreements**: one `## Disagreements` section. Either it holds one table whose columns are exactly `Concept | Owner | Sites | Consequence | Done when`, followed by a delimiter row of five cells, with every row holding five non-empty cells; or it says `None found.` instead. `None found.` beside a table is refused, even beside a table left with no rows. A section with neither rows nor `None found.` is refused. No two rows may have the same Concept, compared after trimming and ignoring case.

Each refusal is one line on standard error, `<file>:<line>: <field>: <reason>`, as today. The line is where the fault is: the line of the offending header field, row or `None found.`, the header row of a second table, the second of two rows with the same Concept, the header row for wrong columns, the line after the header row in the same table for a missing or malformed delimiter row, or the header row itself when the table ends there, the section's heading for a section with no rows, and line 1 for a missing header field or a missing section. The field is the header field's name (for an unknown field, its own name), the column's name for an empty cell, `Concept` for a repeated Concept, and `Disagreements` for every other fault in that section. A file breaking one rule gets exactly one refusal.

## The command

`design-read check [<file>]...` checks any number of files, each alone and each whatever an earlier one gave, in the order named. With no file, it checks every `*.md` file directly in `docs/design-reads/` under the working directory except `README.md`, in name order.

- A file that passes prints `<file>: passes` on standard output, where `<file>` is the path as named, or `docs/design-reads/<name>.md` for the folder.
- A file that cannot be read as UTF-8 text prints `<file>: cannot be read: <reason>` on standard error.
- The exit status is 0 when every file passes, 1 when any is refused and every file could be read, and 2 for a usage fault: a malformed command line, a file that cannot be read, or, with no file named, a `docs/design-reads/` folder that cannot be listed.

The crate documentation is the format's reference: update it to describe this format and what the tool does not check.

Everything else design-read does stays as it is. Its existing tests must still pass, apart from those that exist only for the chained format, which go; update an existing test only where it asserts what this change replaces. Add fixtures and tests for the new format beside the crate's own: a valid area file with rows, one with `None found.`, one refused file per rule, and scenarios through the command for a folder run, a named-file run and a missing folder.

`AGENTS.md` at the repository root says how to build and test it.
