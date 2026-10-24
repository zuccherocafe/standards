# Evaluation report: `gpt-6.1-sol`

- Date: `2026-10-24T06:42:01Z`
- Standards commit: `e450fe1522a54d0276332cc2e28b4c44050a5cae`
- Agent-skills commit: `aeac41d4d005999432bb7312f7ff06383a91d59d`
- Superpowers commit: `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b`
- Harness: `harbor 0.24.0`, `pi 1.4.0`
- Model: `gpt-6.1-sol` (thinking: `high`, max turns: `35`)

Its review scores came from a keyword grader over `REVIEW.md` that the runner no longer has, so they are not comparable with review scores from `findings.jsonl`.

## Summary by condition

| Condition | Write avg | Review avg | Overall reward | Adherence | Avg turns | Cost (USD) |
|---|---|---|---|---|---|---|
| `none` | 0.73 | 0.76 | 0.74 | - | 18.7 | $1.41 |
| `agent-skills` | 0.73 | 0.81 | 0.77 | - | 26.1 | $2.19 |
| `superpowers` | 0.58 | 0.69 | 0.63 | - | 23.4 | $1.89 |
| `standards` | 0.85 | 0.89 | 0.87 | 0.94 | 22.7 | $2.63 |

## Per-task breakdown (functional score; `standards` shows adherence in parentheses)

| Task | Track | `none` | `agent-skills` | `superpowers` | `standards` |
|---|---|---|---|---|---|
| `review-makiatto-live-reconfiguration` | review | 0.83 | 0.83 | 0.83 | 0.83 (1.00) |
| `review-rust-mutants-prepared-targets` | review | 0.83 | 0.83 | 0.83 | 0.83 (1.00) |
| `review-soppo-match-divergence` | review | 0.61 | 0.78 | 0.39 | 1.00 (1.00) |
| `write-dandegate-build-cache` | write | 1.00 | 1.00 | 1.00 | 1.00 (0.78) |
| `write-makiatto-image-transform` | write | 0.43 | 1.00 | 0.47 | 1.00 (1.00) |
| `write-soppo-interpolation-format` | write | 1.00 | 0.45 | 0.45 | 1.00 (0.83) |
| `write-tdollhouse-rigid-attachments` | write | 0.47 | 0.47 | 0.41 | 0.41 (1.00) |

## Standards file-read telemetry (`condition = standards`)

| File | Read rate | Trials read |
|---|---|---|
| `rules/general.md` | 100.0% | 7 / 7 |
| `rules/languages/gdscript/README.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/design.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/errors.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/lifetime.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/organisation.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/performance.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/tests.md` | 0.0% | 0 / 7 |
| `rules/languages/gdscript/warnings.md` | 0.0% | 0 / 7 |
| `rules/languages/python/README.md` | 0.0% | 0 / 7 |
| `rules/languages/rust/README.md` | 85.7% | 6 / 7 |
| `rules/languages/rust/async.md` | 71.4% | 5 / 7 |
| `rules/languages/rust/design.md` | 100.0% | 7 / 7 |
| `rules/languages/rust/documentation.md` | 85.7% | 6 / 7 |
| `rules/languages/rust/errors.md` | 100.0% | 7 / 7 |
| `rules/languages/rust/performance.md` | 85.7% | 6 / 7 |
| `rules/languages/rust/tests.md` | 100.0% | 7 / 7 |
| `rules/review.md` | 100.0% | 7 / 7 |
| `skills/change-land/SKILL.md` | 0.0% | 0 / 7 |
| `skills/change-review/SKILL.md` | 42.9% | 3 / 7 |
| `skills/change-write/SKILL.md` | 57.1% | 4 / 7 |
| `skills/design-read/SKILL.md` | 100.0% | 7 / 7 |
| `skills/repository-clone/SKILL.md` | 0.0% | 0 / 7 |
| `skills/repository-create/SKILL.md` | 0.0% | 0 / 7 |

## Per-rule standards adherence (`condition = standards`)

| Rule | Adherence | Trials passed |
|---|---|---|
| `general#comments-inside-code-stay-short` | 100.0% | 4 / 4 |
| `general#documentation-describes-current-state-only` | 100.0% | 4 / 4 |
| `general#keep-lines-within-120-columns` | 100.0% | 4 / 4 |
| `general#parse-at-the-boundary` | 75.0% | 3 / 4 |
| `general#use-british-english` | 100.0% | 7 / 7 |
| `general#write-scoped-imperative-commit-messages` | 75.0% | 3 / 4 |
| `rules/review#cite-specific-rule` | 100.0% | 3 / 3 |
| `rust/design#invariants` | 0.0% | 0 / 1 |
| `rust/design#quantities-and-error-codes` | 100.0% | 4 / 4 |
| `rust/documentation#clippy` | 100.0% | 1 / 1 |
| `rust/tests#properties` | 100.0% | 4 / 4 |
| `rust/tests#scenarios-first` | 0.0% | 0 / 1 |
| `skills/change-review#done-final-marker` | 100.0% | 3 / 3 |
| `skills/change-review#must-and-should-fix-kinds` | 100.0% | 3 / 3 |
| `skills/change-review#verdict` | 100.0% | 3 / 3 |
| `skills/design-read#check` | 100.0% | 4 / 4 |
| `skills/design-read#settled-interpolation` | 100.0% | 1 / 1 |

## Review defect detection across conditions

| Finding | `none` | `agent-skills` | `superpowers` | `standards` |
|---|---|---|---|---|
| `review/bait-resistance#compiled_component_cache_without_env` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/bait-resistance#source_loader_trait` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/bait-resistance#timing_variance_in_evidence` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#detached_server_on_timeout` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#guarded_arm_false_divergence` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#prebuild_failure_ignored_or_unverified_prepared` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#shared_mutable_target_across_shards` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#tautological_expected_value_in_test` | 0.0% | 100.0% | 0.0% | 100.0% |
| `review/must-fix#transform_cache_ignores_env` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#unseeded_proptest_persistence` | 100.0% | 0.0% | 0.0% | 100.0% |
| `review/must-fix#unvalidated_route_weight_deserialize` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/must-fix#warm_cold_profile_mismatch` | 100.0% | 100.0% | 100.0% | 100.0% |
| `review/should-fix#debug_format_in_log_or_idiom` | 0.0% | 0.0% | 0.0% | 0.0% |
| `review/should-fix#idiom_clone_or_loop` | 0.0% | 0.0% | 0.0% | 0.0% |
| `review/should-fix#separate_unit_test_or_idiom` | 0.0% | 100.0% | 0.0% | 100.0% |
