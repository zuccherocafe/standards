#!/usr/bin/env bash
set -euo pipefail
mkdir -p /app
cat > /app/findings.jsonl <<'FINDINGS'
{"file": "src/ast.rs", "lines": [44, 49], "kind": "correctness", "severity": "must-fix", "finding": "Variant coverage counts guarded arms, so a false guard falls through while the match is classed as returning; count only unguarded arms."}
{"file": "tests/match_flow.rs", "lines": [15, 16], "kind": "tests", "severity": "must-fix", "finding": "The expected value calls stmt_diverges, the predicate under test; author the expected false and the MissingReturn diagnostic."}
{"file": "tests/match_flow.rs", "lines": [19, 30], "kind": "tests", "severity": "must-fix", "finding": "The property has no fixed seed and keeps default failure persistence; set RngSeed::Fixed and failure_persistence: None."}
{"file": "src/main.rs", "lines": [28, 31], "kind": "correctness", "severity": "must-fix", "finding": "An unreadable package exits with success, so a misspelt path passes unchecked; let it fail like other errors."}
{"file": "src/infer.rs", "lines": [74, 78], "kind": "correctness", "severity": "must-fix", "finding": "Every arm shares one nullable set, so a nil check in one arm narrows its siblings and the code after the match; check each arm on a copy."}
{"file": "src/infer.rs", "lines": [48, 53], "kind": "design", "severity": "should-fix", "finding": "The wildcard in optional_binding hides future Expr variants; name Expr::Integer explicitly."}
FINDINGS
cat > /app/REVIEW.md <<'REVIEW'
# Match-flow review

I reviewed the base-to-tip change, the brief's decisions, the match-flow design read, the complete check log, and the changed AST, inference, parser consumers and tests. The log shows passing tests and Clippy. Its unguarded scenarios establish the happy paths, not guard-aware exhaustiveness. The guarded scenario has no independent oracle, so its passing result does not establish that guarded paths terminate. No backend or interpreter behaviour is claimed by these checks.

## 1. Must-fix: a false guard is mistaken for a terminating path

In `src/ast.rs`, `TypedStmtKind::diverges` counts every variant appearing in an arm without considering `MatchArm::guard`. `src/infer.rs::check_function` then suppresses `MissingReturn`, and `check_block` uses the same unsound fact to remove nullable bindings after an `IfNil` branch. For `fn choose(gate: Gate, flag: bool) -> int { match gate { Open if flag => { return 1; }, Closed => { return 2; } } }`, the ordinary call `gate = Open, flag = false` reaches the end without returning. The compiler nevertheless accepts it. The closed variant set is not a proof of runtime arm coverage when a guarded arm can be skipped.

The same input reaches nil safety: `fn size(value: Text?, gate: Gate, flag: bool) -> int { if value is nil { match gate { Open if flag => { return 0; }, Closed => { return 0; } } } return value.length; }`. With `value = nil, gate = Open, flag = false`, control reaches a nil length access, but the compiler produces no `NilDereference`. This breaks both explicit acceptance promises for real author-written programs.

Smallest fix: count a variant as covered only by an unguarded arm; keep requiring every reachable arm body to terminate. In this language slice, `guard.is_none()` in the coverage predicate suffices and costs a few lines. Retain guarded arms for body checking. Add authored expectations for these two programs: `MissingReturn` for the first and `NilDereference` for the second, alongside the existing unguarded acceptance scenarios. This repairs the shared fact rather than patching its two consumers. Rules: `/standards/rules/general.md#design`, **Correctness comes from the design first**, and `/standards/rules/languages/rust/design.md#closed-sets`.

## 2. Must-fix: the guarded test derives its expected result from the bug

In `tests/match_flow.rs::guarded_match_flow_agrees_with_the_statement_classification`, `expected = infer::stmt_diverges(&function.body[0])` calls the production predicate under test. `check_function` reaches that same predicate via `block_diverges`. Returning the wrong termination answer therefore changes both sides together. The exact guarded program above passes this test even though it can fall through. This breaks the promise that the added test detects wrong control-flow inference; it does not supply evidence for guarded narrowing either.

Smallest fix: replace the computed expected value with authored `false` and assert the authored `MissingReturn` diagnostic. Keep the separate nil-safety example from finding 1 because it exercises the other consumer. The change costs only a few assertions. Rules: `/standards/rules/languages/rust/tests.md#expected-values-and-snapshots` and `/standards/rules/general.md#checks`, **A test can fail when the code is wrong** and **Expected values are authored**.

## 3. Must-fix: property checks break reproducibility and source-tree purity

In `tests/match_flow.rs`, the bare `proptest!` block uses the default configuration. It has no fixed seed or explicit case budget, and default failure persistence can write a regressions file beside the test. A maintainer rerunning a generated counterexample does not have a guaranteed identical stream; a property failure can mutate the checkout. The brief explicitly promises reproducible checks that write nothing, so these are contract breaches even though today's generated cases all pass.

Smallest fix: add `proptest_config(Config { cases: 256, rng_seed: RngSeed::Fixed(0x7d0c), failure_persistence: None, ..Config::default() })` and the corresponding imports. Keep a genuine counterexample as an ordinary authored regression test. This costs one short configuration block and changes no production design. Rule: `/standards/rules/languages/rust/tests.md#properties`.

## 4. Must-fix: a package that cannot be read passes the check

`src/main.rs` now matches `CheckError::Source(SourceError::Unavailable { .. })` and returns `ExitCode::SUCCESS` after printing "no source to check". A maintainer whose build script names a misspelt or moved package, such as `soppo src/mian.soppo`, gets exit status zero, and any automation reading the status treats a program it never read as checked. The brief's acceptance promise is that the CLI fails on diagnostics and accepts only a clean report; an unread package has no report. Before this change the same input failed.

The change moves the source read behind `PackageSourceReader` and, in moving it, stops refusing a missing file. `/standards/rules/languages/rust/errors.md#where-a-refusal-sits` and **A refusal sits where it is cheapest** in `/standards/rules/general.md#design` require a moved refusal to refuse every input it refused before. Delete the new arm so an unavailable source reaches the existing failing `Err(error)` arm. This is a four-line deletion. Add a CLI scenario with a missing path that expects a failing status.

## 5. Must-fix: one arm's nil check narrows its sibling arms

`src/infer.rs::check_block` now passes the shared `nullable` set to every arm instead of a copy. An `if value is nil { return 0; }` inside the `Open` arm removes `value` for the `Closed` arm and for code after the match. `fn size(value: Text?, gate: Gate) -> int { match gate { Open => { if value is nil { return 0; } return value.length; }, Closed => { return value.length; } } }` is accepted, although `gate = Closed, value = nil` reads `.length` on nil; the base reports `NilDereference`. A nil fact holds only on the path that established it, and arms are alternative paths.

No narrower rule names how flow facts are scoped; this breaks the brief's nil-safety promise under `/standards/rules/general.md#design`, **Correctness comes from the design first**. Restore a copy per arm, and narrow after the match only for bindings every continuing arm narrowed. Add the program above as a scenario expecting `NilDereference`.

## 6. Should-fix: a wildcard hides the next expression decision

In `src/infer.rs::optional_binding`, `_ => None` matches the crate-owned `Expr` enum. Today its only other variant is `Integer`, so there is no wrong result for a current input. A new expression variant would silently inherit the assumption that it cannot read nullable text. Replace the wildcard with `Expr::Integer(_) => None`; this is a one-line fix that restores a compile-time decision on enum growth. Rule: `/standards/rules/languages/rust/design.md#closed-sets`.

## Recorded decision retained

`PackageSourceReader` is sound dependency inversion, not an abstraction to delete because only `FilesystemReader` implements it in production. The CLI shell owns filesystem access; the core declares the boundary and computes reports over source values. This is the brief's explicit decision and the dependency-inversion exception in `/standards/rules/languages/rust/design.md#traits`, consistent with `/standards/rules/languages/rust/design.md#purity`. No additional loader implementation is needed.

## Verdict

**fix first**: repair guarded coverage, replace the tautological expectation, configure reproducible non-writing properties, keep a refused unreadable package refused, and scope nil narrowing to its arm (findings 1–5). The source-reader decision stays intact.

DONE: final
REVIEW
