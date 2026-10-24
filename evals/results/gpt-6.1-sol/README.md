# Evaluation report: `gpt-6.1-sol`

- Date: `2026-10-24T12:28:26Z`
- Standards commit: `85e5f99ba0c5e7eb7075327f3b9abbaeedd2c0bd`
- Agent-skills commit: `aeac41d4d005999432bb7312f7ff06383a91d59d`
- Superpowers commit: `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b`
- Harness: `harbor 0.24.0`, `pi 1.4.0`
- Model: `gpt-6.1-sol` (thinking: `high`, max turns: `35`)

## Summary by condition

| Condition | Write avg | Review avg | Overall reward | Adherence | Avg turns | Cost (USD) |
|---|---|---|---|---|---|---|
| `none` | 0.67 | 0.70 | 0.68 | - | 20.0 | $4.02 |
| `agent-skills` | 0.55 | 0.70 | 0.62 | - | 26.2 | $6.53 |
| `superpowers` | 0.63 | 0.70 | 0.66 | - | 24.9 | $6.08 |
| `standards` | 0.73 | 0.83 | 0.77 | 0.90 | 25.3 | $7.93 |

## Per-task breakdown (functional score; `standards` shows adherence in parentheses)

| Task | Track | `none` | `agent-skills` | `superpowers` | `standards` |
|---|---|---|---|---|---|
| `review-makiatto-live-reconfiguration` | review | 0.80 | 0.80 | 0.80 | 0.80 (1.00) |
| `review-rust-mutants-prepared-targets` | review | 0.80 | 0.80 | 0.80 | 0.80 (1.00) |
| `review-soppo-match-divergence` | review | 0.50 | 0.50 | 0.50 | 0.90 (1.00) |
| `write-dandegate-build-cache` | write | 0.77 | 0.77 | 1.00 | 0.53 (0.78) |
| `write-makiatto-image-transform` | write | 0.44 | 0.47 | 0.64 | 1.00 (0.90) |
| `write-soppo-interpolation-format` | write | 1.00 | 0.63 | 0.45 | 1.00 (0.78) |
| `write-tdollhouse-rigid-attachments` | write | 0.47 | 0.34 | 0.42 | 0.38 (0.85) |

## Standards file-read telemetry (`condition = standards`)

| File | Read rate | Trials read |
|---|---|---|
| `rules/general.md` | 100.0% | 21 / 21 |
| `rules/languages/gdscript/README.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/design.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/errors.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/lifetime.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/organisation.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/performance.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/tests.md` | 0.0% | 0 / 21 |
| `rules/languages/gdscript/warnings.md` | 0.0% | 0 / 21 |
| `rules/languages/python/README.md` | 0.0% | 0 / 21 |
| `rules/languages/rust/README.md` | 66.7% | 14 / 21 |
| `rules/languages/rust/async.md` | 71.4% | 15 / 21 |
| `rules/languages/rust/design.md` | 100.0% | 21 / 21 |
| `rules/languages/rust/documentation.md` | 57.1% | 12 / 21 |
| `rules/languages/rust/errors.md` | 100.0% | 21 / 21 |
| `rules/languages/rust/performance.md` | 76.2% | 16 / 21 |
| `rules/languages/rust/tests.md` | 100.0% | 21 / 21 |
| `rules/review.md` | 100.0% | 21 / 21 |
| `skills/change-land/SKILL.md` | 0.0% | 0 / 21 |
| `skills/change-review/SKILL.md` | 42.9% | 9 / 21 |
| `skills/change-write/SKILL.md` | 57.1% | 12 / 21 |
| `skills/concord/SKILL.md` | 100.0% | 21 / 21 |
| `skills/repository-clone/SKILL.md` | 0.0% | 0 / 21 |
| `skills/repository-create/SKILL.md` | 0.0% | 0 / 21 |

## Per-rule standards adherence (`condition = standards`)

| Rule | Adherence | Trials passed |
|---|---|---|
| `general#comments-inside-code-stay-short` | 100.0% | 12 / 12 |
| `general#documentation-describes-current-state-only` | 100.0% | 12 / 12 |
| `general#keep-lines-within-120-columns` | 58.3% | 7 / 12 |
| `general#parse-at-the-boundary` | 41.7% | 5 / 12 |
| `general#use-british-english` | 100.0% | 21 / 21 |
| `general#write-scoped-imperative-commit-messages` | 75.0% | 9 / 12 |
| `rules/review#cite-specific-rule` | 100.0% | 9 / 9 |
| `rust/design#invariants` | 0.0% | 0 / 3 |
| `rust/design#quantities-and-error-codes` | 100.0% | 12 / 12 |
| `rust/documentation#clippy` | 100.0% | 3 / 3 |
| `rust/tests#properties` | 100.0% | 12 / 12 |
| `rust/tests#scenarios-first` | 0.0% | 0 / 3 |
| `skills/change-review#done-final-marker` | 100.0% | 9 / 9 |
| `skills/change-review#verdict` | 100.0% | 9 / 9 |
| `skills/concord#check` | 100.0% | 12 / 12 |
| `skills/concord#settled-interpolation` | 100.0% | 3 / 3 |

## Review recall and precision

Recall and precision are location matches over the review trials with a grading record, pooled per condition: recall is found / planted, and precision is found / (planted + must-fix findings on no defect). A finding is matched by file, lines and kind; its explanation is not judged, so these scores measure detection of the planted defects, not review quality generally. Should-fix findings on no defect cost nothing. Named defects are those a standards rule names. Ungraded counts review trials without a grading record, such as a verifier failure, which these columns leave out. Each group's precision counts every must-fix finding on no defect, since those findings belong to neither group.

| Condition | Recall | Named recall | Unnamed recall | Precision | Named precision | Unnamed precision | Ungraded |
|---|---|---|---|---|---|---|---|
| `none` | 0.69 | 0.55 | 1.00 | 0.69 | 0.55 | 1.00 | 0 |
| `agent-skills` | 0.69 | 0.55 | 1.00 | 0.69 | 0.55 | 1.00 | 0 |
| `superpowers` | 0.69 | 0.55 | 1.00 | 0.69 | 0.55 | 1.00 | 0 |
| `standards` | 0.88 | 0.82 | 1.00 | 0.84 | 0.77 | 0.88 | 0 |

## Review defects found (trials found / trials with a grading record)

| Defect | Group | `none` | `agent-skills` | `superpowers` | `standards` |
|---|---|---|---|---|---|
| `review-makiatto-live-reconfiguration/detached_server_on_timeout` | named | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-makiatto-live-reconfiguration/header_length_before_transform` | unnamed | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-makiatto-live-reconfiguration/public_input_digest` | named | 0/3 | 0/3 | 0/3 | 0/3 |
| `review-makiatto-live-reconfiguration/transform_cache_ignores_env` | named | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-makiatto-live-reconfiguration/unvalidated_route_weight_deserialize` | named | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-rust-mutants-prepared-targets/linked_workspace_overwrites_subject` | unnamed | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-rust-mutants-prepared-targets/prebuild_failure_ignored` | named | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-rust-mutants-prepared-targets/shared_mutable_target_across_shards` | named | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-rust-mutants-prepared-targets/warm_cold_profile_mismatch` | unnamed | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-rust-mutants-prepared-targets/workspace_clone` | named | 0/3 | 0/3 | 0/3 | 0/3 |
| `review-soppo-match-divergence/arms_share_nil_narrowing` | unnamed | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-soppo-match-divergence/guarded_arm_false_divergence` | unnamed | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-soppo-match-divergence/missing_source_exits_success` | named | 3/3 | 3/3 | 3/3 | 3/3 |
| `review-soppo-match-divergence/tautological_expected_value_in_test` | named | 0/3 | 0/3 | 0/3 | 3/3 |
| `review-soppo-match-divergence/unseeded_proptest_persistence` | named | 0/3 | 0/3 | 0/3 | 3/3 |
| `review-soppo-match-divergence/wildcard_hides_expression_variants` | named | 0/3 | 0/3 | 0/3 | 3/3 |
