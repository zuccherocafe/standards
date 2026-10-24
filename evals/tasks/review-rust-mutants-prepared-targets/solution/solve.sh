#!/usr/bin/env bash
set -euo pipefail
cat > /app/REVIEW.md <<'REVIEW'
# Prepared-target review

I read the brief, complete check evidence, area read, base-to-tip diff and the command-planning-to-worker call chain. Passing tests establish cold command arguments, CLI parsing and case partitioning. They do not establish successful preparation, prepared-mode target isolation or agreement between prebuild and worker profiles.

## 1. Must-fix: the deleted Prepared command disagreement remains reachable

`docs/design-reads/rust-mutants.md`, Models in play and Disagreements, claims a successful prebuild backs `Prepared` and deletes the Prepared command row. In `tools/crates/rust-mutants/src/command.rs::Prepared::new`, construction still only stores a command. `src/shards.rs::plan` constructs every worker's value before `src/main.rs::execute` calls prebuild; the latter logs a prebuild failure and continues executing those values. A user with a valid mutation case and an unmutated subject that fails to compile therefore gets mutation classifications despite no successful preparation. This breaks the brief's explicit prepared-stage promise and makes the area read conceal the unresolved model disagreement.

The specific type rule is `/standards/rules/languages/rust/design.md#checked-against-a-source` (Checked against a source), supported by `/standards/rules/general.md#design` (Keep the system coherent). The area-file obligation is `/standards/skills/design-read/SKILL.md#keep-the-area-file-current`. Restore the row until the contract is actually settled. Make successful prebuilding the only constructor of a prepared seed tied to its subject and profile, propagate failure instead of executing prepared workers, and plan prepared execution from that result. This is a small command/CLI boundary refactor, not another check after arbitrary construction.

## 2. Must-fix: prepared workers share mutable build outputs

`tools/crates/rust-mutants/src/shards.rs::plan` assigns the same target path to every shard when `--prepared-target` is supplied. `run` starts those shards concurrently and `run_shard` removes `target/mutants` before each case, outside Cargo's build lock. Repeat `--workspace fixtures/subject --cases fixtures/cases --shards 2 --prepared-target .mutants/seed`, or supply enough cases for each worker to execute several: worker B can delete worker A's compiler outputs or test executable while A builds or launches it. Users can see build failures misclassified as caught mutations, or an aborted run. Cargo's internal lock cannot protect a directory another thread deletes before invoking Cargo. The immutable-seed and exclusive-worker-owner promises both break.

The rule is `/standards/rules/general.md#design` (One home per concept), with `/standards/rules/languages/rust/performance.md#parallelism`: concurrent items must be independent. Keep the completed seed immutable and copy its reusable outputs into each existing private per-shard target before spawning. Point every worker command and cleanup at that private target. This adds the intended initial copy per worker; serialising all workers would only work around the ownership fault and defeat the decided concurrency.

## 3. Must-fix: prebuild and mutation execution select different profiles

`tools/crates/rust-mutants/src/command.rs::CargoCommand::prebuild` constructs `cargo test --offline --no-run --target-dir ...` with the default test profile; `CargoCommand::test` selects `--profile mutants`. Both the workspace and the supplied subject define `mutants` with optimisation level one. For an ordinary prepared invocation, the seed fills `debug` while workers consume and clean `mutants`; the generated artefacts have different profile identities and the promised matching prepared build does not exist. A successful warm command is not evidence that the worker's selected build has been prepared.

This violates `/standards/rules/general.md#design` (Keep the system coherent), specifically the generated-result reuse obligation in that section (Generated output is typed and trusted). Derive both commands' profile arguments from the same profile definition, varying only whether tests run; here that means adding `--profile mutants` to prebuild through the shared command constructor. It costs a few lines and avoids keeping two disagreeing build models. This is distinct from the shared-directory fault: private copies of the wrong-profile seed remain wrong.

## 4. Should-fix: borrow the workspace instead of cloning it

`tools/crates/rust-mutants/src/shards.rs::run_shard` makes an unnecessary clone of `shard.workspace` solely to borrow it for `stage_workspace`. No ownership leaves the function and the original path stays live. Borrow `&shard.workspace` directly and delete the temporary. The current code gives the right result, so this is not blocking; the fix removes an allocation and two lines without changing the design. Rules: `/standards/rules/languages/rust/performance.md#copies-and-data-layout` and `/standards/rules/general.md#performance` (Write clear code first and optimise measured costs).

## Evidence and decisions

The reported 4% wall-time fluctuation is not a correctness finding. Timing varies with scheduling and machine load; the report gives its sample spread rather than making a timing threshold an acceptance condition. This follows `/standards/rules/general.md#checks` (A varying measurement never fails the checks). Keep reporting spread; no additional failing timing check is warranted. The brief's scoped-thread, authored-case and custom-profile decisions are sound and are not reopened here.

Verdict: **fix first** — resolve the unsupported prepared-stage claim and area-row deletion, shared mutable target ownership, and warm/worker profile mismatch before landing.

DONE: final
REVIEW
