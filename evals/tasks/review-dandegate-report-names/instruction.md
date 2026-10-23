# Review the last commit

The repository at `/app` holds Rust command-line tools in one Cargo workspace under `tools/`. Its last commit, `HEAD`, is a proposed change, and its parent is the code the change starts from. `git show HEAD` gives the change's description, and `git diff HEAD~1 HEAD` the change.

Review the change as its one independent reviewer. Look for faults the change brings in, and judge it against its description and the code's own documentation. You may build, test and run the code to check what you suspect; nothing you change in the repository counts as part of the review.

Mark each finding with one severity:

- `must-fix`: an input a person can produce gets a wrong result, or a promise the change makes, in its description, its documentation or its interface, breaks.
- `should-fix`: worth changing, but no real input gives a wrong result without the change.

Give each finding one kind:

- `correctness`: a wrong result, a wrong refusal or acceptance, a crash or a hang.
- `concurrency`: a race, a deadlock, or a process, file or lock left behind.
- `security`: data or access exposed to someone who should not have it.
- `performance`: time or memory spent without need.
- `tests`: a promise without a test, or a test that cannot fail when the code is wrong.
- `documentation`: documentation, a comment or a message that is wrong or missing.
- `design`: a concept with two homes, an unfinished transition, or a simpler correct shape.
- `idiom`: code an experienced Rust reviewer would write differently, with no change in behaviour.

Write the findings to `/app/findings.jsonl`, one JSON object per line and nothing else, for example:

```json
{"file": "tools/crates/example/src/parse.rs", "lines": [40, 44], "kind": "correctness", "severity": "must-fix", "finding": "A name with a trailing space is accepted, so two entries that differ only in that space count as different steps."}
```

- `file` is the file's path from the repository root, as `HEAD` holds it.
- `lines` is the first and last line of the code the finding is about, numbered from 1 in the file as `HEAD` holds it; one line is `[40, 40]`. Point at the code at fault itself, in at most 30 lines.
- `finding` is one sentence: what goes wrong, and for a must-fix finding the input that reaches it.

When you find nothing, leave the file empty. A file with any other line cannot be read as a review.

`AGENTS.md` at the repository root says how to build and test the code.
