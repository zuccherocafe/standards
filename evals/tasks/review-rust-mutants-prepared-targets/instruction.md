# Review prepared mutation targets

Review the change between the revision recorded in `/etc/eval-base-rev` and `HEAD` in `/app`.

Write each finding to `/app/findings.jsonl`, one JSON object per line:

```json
{"file": "src/example.rs", "lines": [12, 18], "kind": "correctness", "severity": "must-fix", "finding": "What is wrong and how to fix it."}
```

- `file` is the path relative to `/app`, and `lines` gives the first and last line of the finding's site in that file at `HEAD`. Use an inclusive range of at most 30 lines, narrowly identifying one fault.
- `kind` is one of `correctness`, `concurrency`, `security`, `performance`, `tests`, `documentation`, `design` or `idiom`.
- `severity` is `must-fix` for a fault that must be fixed before the change lands, or `should-fix` for one that should be fixed but need not block it.

Leave the file empty if you find nothing. Write the review's prose to `/app/REVIEW.md`, including a line that gives your verdict.
