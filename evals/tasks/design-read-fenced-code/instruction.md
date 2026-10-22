# Ignore what fenced code holds in a design read

`tools/crates/design-read` is design-read. `design-read check <file>` checks the shape of a design read, a Markdown file: it reads the header fields, the `## After the last phase` section and the `## Disagreements` section with its table, and leaves every other section as prose. Its `src/main.rs` module documentation and its tests describe the format.

A read may quote Markdown in fenced code blocks, such as an example of what another tool prints. Fenced code blocks are as CommonMark defines them. Everything inside one is content: a heading, a table or a field there belongs to the example, never to the read, and never decides whether the read passes or what is refused. Only what lies outside every fenced code block decides that. design-read does not keep this promise for every valid fenced code block today. Make it keep it.

Everything else design-read does stays as it is, and its existing tests must still pass. You need not add tests, though you may.

`AGENTS.md` at the repository root says how to build and test it.
