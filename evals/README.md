# Evaluations (`evals/`)

This folder holds the Harbor evaluation suite for `zuccherocafe/standards`. It compares coding and code-review trials across four experimental conditions on the `pi` agent harness:

- **`none`**: `pi` with only the neutral task specification and project build instructions.
- **`agent-skills`**: `pi` with [`addyosmani/agent-skills`](https://github.com/addyosmani/agent-skills) pinned in `flake.lock`.
- **`superpowers`**: `pi` with [`obra/superpowers`](https://github.com/obra/superpowers) pinned in `flake.lock`.
- **`standards`**: `pi` with this repository's live `rules/`, `skills/`, `templates/`, and tools.

## Layout

- `runner/`: `evals.py` (Harbor CLI runner and report generator), `adherence.py` (live standards discovery and session telemetry), `grade_adherence.py` (verifier grading), and `tests/` (`unittest` suite).
- `pi/`: reproducible base image recipe (`Dockerfile`, `install.sh`, `package.json`, `package-lock.json`) for `pi 1.4.0` and the pinned Rust nightly toolchain.
- `tasks/`: four architectural writing tasks (`write-dandegate-build-cache`, `write-makiatto-image-transform`, `write-soppo-interpolation-format`, `write-tdollhouse-rigid-attachments`) and three architectural review tasks (`review-makiatto-live-reconfiguration`, `review-rust-mutants-prepared-targets`, `review-soppo-match-divergence`).
- `results/`: saved benchmark reports (`<name>/README.md`, `summary.json`, and per-condition trial JSONs).

## Commands

Run every command inside the development shell:

```sh
# Validate task definitions, live standards discovery, and container verifiers ($0, 0 tokens)
nix develop -c evals check --quick
nix develop -c evals check

# Run the suite across all four conditions (defaults to openai-codex/gpt-6.1-sol)
nix develop -c evals run --save gpt-6.1-sol

# Run with an OpenRouter model (reads ~/.config/evals/openrouter-key) or filter by track, task, or condition
nix develop -c evals run --model openrouter/deepseek/deepseek-v4.1-flash-1017
nix develop -c evals run --model openrouter/anthropic/claude-sonnet-5.5
nix develop -c evals run --track write
nix develop -c evals run --track review
nix develop -c evals run --task write-soppo-interpolation-format --condition standards

# Inspect scores, costs, file-read telemetry, and per-rule adherence
nix develop -c evals report
nix develop -c evals report --save gpt-6.1-sol
```

Raw Harbor job outputs and staged trial environments stay under `.cache/evals/`.
