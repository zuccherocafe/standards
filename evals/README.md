# Evaluations (`evals/`)

This folder holds the Harbor evaluation suite for `zuccherocafe/standards`. It compares coding and code-review trials across four experimental conditions on the `pi` agent harness:

- **`none`**: `pi` with only the neutral task specification and project build instructions.
- **`agent-skills`**: `pi` with [`addyosmani/agent-skills`](https://github.com/addyosmani/agent-skills) pinned in `flake.lock`.
- **`superpowers`**: `pi` with [`obra/superpowers`](https://github.com/obra/superpowers) pinned in `flake.lock`.
- **`standards`**: `pi` with this repository's live `rules/`, `skills/`, `templates/`, and tools.

## Layout

- `runner/`: `evals.py` (Harbor CLI runner and report generator), `adherence.py` (live standards discovery and session telemetry), `grade_adherence.py` (verifier grading), and `tests/` (`unittest` suite).
- `pi/`: reproducible base image recipe (`Dockerfile`, `install.sh`, `package.json`, `package-lock.json`) for `pi` and the pinned Rust nightly toolchain.
- `tasks/`: one folder per task; the folder is the list. A task folder holds `task.toml` (its Harbor configuration, with `metadata.track` set to `write` or `review`), `instruction.md` (the prompt the agent sees), `environment/` (the Dockerfile and the repository, and for a review task the `change.patch` under review), `solution/solve.sh` (the reference solution `evals check` verifies), and `tests/` (the verifier `test.sh` and what it runs, with `review_spec.json` for a review task). Writing tasks start with `write-` and review tasks with `review-`.
- `results/`: saved benchmark reports (`<name>/README.md`, `summary.json`, and per-condition trial JSONs).

## Review scoring

Each review task asks for `/app/findings.jsonl`, one finding per line with its file, first and last line, kind, severity and text, and for the prose and a verdict line in `/app/REVIEW.md`. The task's `tests/review_spec.json` records each planted defect's sites (file and line range), the kinds a correct finding may give, its severity, and whether a standards rule names it, with that rule's file and anchor. It also records bait: sound code a reviewer may wrongly block on.

- A finding is on a defect when it spans at most 30 lines, names the file of one of the defect's sites, touches that site's lines widened by 3 on each side, and gives one of the defect's kinds. A defect lists more than one site or kind only where a correct finding could reasonably locate or describe it there, such as at its cause and where it does harm. A must-fix defect needs a must-fix finding; a should-fix defect takes either severity. One finding earns credit for at most one defect, the closest it is on, and further findings on a found defect cost nothing.
- Recall is defects found over defects planted. Precision is defects found over defects planted plus must-fix findings on no defect, so it falls with every miss and every false block. A must-fix finding on a bait's lines is one of those false positives. Should-fix findings on no defect cost nothing, so a review can collect should-fix credit by spreading should-fix findings; the score is not proof against gaming. A review task's score is its precision.
- An empty findings file means no findings. A missing file, or any line that is not a finding in this format, scores 0.
- The report gives recall and precision per condition, overall and separately for the defects the standards name and those they do not, and how often each condition found each defect. False positives belong to neither group, so each group's precision counts all of them. Matching checks the file, lines and kind, never the finding's explanation, so these are location measures. A lead on unnamed defects is evidence of better detection of those planted mechanisms under this matcher, not proof of better reviewing generally. The tables cover review trials with a grading record and count the others as ungraded.
- Adherence, reported for the `standards` condition only, checks `REVIEW.md` and the findings' text against the standards' review skill: the expected verdict, the `DONE: final` line, a cited rule, and British spelling.

## Commands

Run every command inside the development shell:

```sh
# Validate task definitions, live standards discovery, and container verifiers ($0, 0 tokens)
nix develop -c evals check --quick
nix develop -c evals check

# Run the suite across all four conditions with the runner's default model, saving the report under results/<name>/
nix develop -c evals run --save <name>

# Run with an OpenRouter model (reads ~/.config/evals/openrouter-key) or filter by track, task, or condition
nix develop -c evals run --model openrouter/<provider>/<model>
nix develop -c evals run --track write
nix develop -c evals run --track review
nix develop -c evals run --task <name> --condition standards

# Inspect scores, costs, file-read telemetry, and per-rule adherence
nix develop -c evals report
nix develop -c evals report --save <name>
```

Raw Harbor job outputs and staged trial environments stay under `.cache/evals/`.
