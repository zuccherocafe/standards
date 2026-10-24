"""Scores made-up review findings against a made-up planted-defect spec."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from adherence import discover_standards
from grade_adherence import (
    Bait,
    Finding,
    FindingKind,
    LineRange,
    PlantedDefect,
    ReviewSpec,
    Severity,
    Site,
    grade_review_trial,
    parse_findings,
    parse_review_spec,
    score_findings,
)

SPEC = ReviewSpec(
    defects=(
        PlantedDefect(
            item_id="race",
            sites=(Site("src/a.rs", LineRange(40, 44)),),
            kinds=(FindingKind.CONCURRENCY,),
            severity=Severity.MUST_FIX,
            rule="rules/languages/rust/async.md#every-task-has-an-owner",
        ),
        PlantedDefect(
            item_id="exit",
            sites=(Site("src/b.rs", LineRange(10, 12)),),
            kinds=(FindingKind.CORRECTNESS,),
            severity=Severity.MUST_FIX,
            rule=None,
        ),
        PlantedDefect(
            item_id="clone",
            sites=(Site("src/a.rs", LineRange(90, 90)),),
            kinds=(FindingKind.IDIOM,),
            severity=Severity.SHOULD_FIX,
            rule="rules/languages/rust/performance.md#copies-and-data-layout",
        ),
    ),
    bait=(Bait(item_id="trait", file="src/c.rs", lines=LineRange(5, 8)),),
    expected_verdict="fix first",
)


def finding(
    file: str,
    first: int,
    last: int,
    kind: FindingKind = FindingKind.CORRECTNESS,
    severity: Severity = Severity.MUST_FIX,
) -> Finding:
    return Finding(file=file, lines=LineRange(first, last), kind=kind, severity=severity, text="made up")


def race(first: int, last: int, severity: Severity = Severity.MUST_FIX) -> Finding:
    return finding("src/a.rs", first, last, FindingKind.CONCURRENCY, severity)


GOOD_LINE = (
    '{"file": "src/b.rs", "lines": [10, 12], "kind": "correctness", "severity": "must-fix", "finding": "made up"}\n'
)


class LocationTests(unittest.TestCase):
    """A finding is on a defect by file, widened lines, span and kind."""

    def test_on_the_defects_lines_finds_it(self) -> None:
        self.assertTrue(score_findings((race(41, 42),), SPEC).found["race"])

    def test_off_the_defects_lines_misses_it(self) -> None:
        self.assertFalse(score_findings((race(60, 62),), SPEC).found["race"])

    def test_three_lines_of_widening_on_each_side(self) -> None:
        cases = {(37, 37): True, (36, 36): False, (47, 50): True, (48, 50): False}
        for (first, last), expected in cases.items():
            with self.subTest(lines=(first, last)):
                self.assertEqual(score_findings((race(first, last),), SPEC).found["race"], expected)

    def test_a_finding_over_thirty_lines_locates_nothing(self) -> None:
        self.assertTrue(score_findings((race(20, 49),), SPEC).found["race"])
        wide = score_findings((race(20, 50),), SPEC)
        self.assertFalse(wide.found["race"])
        self.assertEqual(wide.false_positives, 1)

    def test_wrong_kind_misses_and_is_a_false_positive(self) -> None:
        score = score_findings((finding("src/a.rs", 41, 42, FindingKind.CORRECTNESS),), SPEC)
        self.assertFalse(score.found["race"])
        self.assertEqual(score.false_positives, 1)

    def test_wrong_file_misses(self) -> None:
        score = score_findings((finding("src/z.rs", 41, 42, FindingKind.CONCURRENCY),), SPEC)
        self.assertFalse(score.found["race"])

    def test_paths_under_app_name_the_same_file(self) -> None:
        findings = parse_findings(GOOD_LINE.replace('"src/b.rs"', '"/app/src/b.rs"'))
        self.assertTrue(score_findings(findings, SPEC).found["exit"])


class AlternativeSiteAndKindTests(unittest.TestCase):
    """A defect listing several sites and kinds credits an equivalent finding at any of them equally."""

    CACHE = ReviewSpec(
        defects=(
            PlantedDefect(
                item_id="cache",
                sites=(Site("src/wasm.rs", LineRange(26, 30)), Site("src/wasm.rs", LineRange(88, 96))),
                kinds=(FindingKind.CORRECTNESS, FindingKind.PERFORMANCE),
                severity=Severity.MUST_FIX,
                rule=None,
            ),
        ),
        bait=(),
        expected_verdict="fix first",
    )

    def test_equivalent_findings_score_equally(self) -> None:
        variants = {
            "key definition": finding("src/wasm.rs", 27, 30),
            "key construction": finding("src/wasm.rs", 88, 91),
            "other accepted kind": finding("src/wasm.rs", 88, 91, FindingKind.PERFORMANCE),
        }
        for name, variant in variants.items():
            with self.subTest(variant=name):
                score = score_findings((variant,), self.CACHE)
                self.assertEqual((score.recall(), score.precision()), (1.0, 1.0))

    def test_unlisted_sites_and_kinds_still_miss(self) -> None:
        for variant in (finding("src/wasm.rs", 50, 60), finding("src/wasm.rs", 88, 91, FindingKind.DESIGN)):
            with self.subTest(lines=variant.lines, kind=variant.kind):
                score = score_findings((variant,), self.CACHE)
                self.assertEqual((score.recall(), score.precision()), (0.0, 0.0))

    def test_findings_at_both_sites_find_the_defect_once(self) -> None:
        score = score_findings((finding("src/wasm.rs", 27, 30), finding("src/wasm.rs", 88, 91)), self.CACHE)
        self.assertEqual((score.found, score.false_positives), ({"cache": True}, 0))


DEFECT: dict[str, object] = {
    "id": "exit",
    "sites": [{"file": "src/b.rs", "lines": [10, 12]}],
    "kinds": ["correctness"],
    "severity": "must-fix",
    "named_by_standards": False,
}


class SpecRefusalTests(unittest.TestCase):
    """The spec parser refuses inconsistent ground truth."""

    def parse(self, defects: list[dict[str, object]]) -> None:
        with tempfile.TemporaryDirectory() as directory:
            spec = Path(directory) / "review_spec.json"
            spec.write_text(
                json.dumps({"expected_verdict": "fix first", "defects": defects, "bait": []}), encoding="utf-8"
            )
            parse_review_spec(spec)

    def test_refusals(self) -> None:
        cases: dict[str, list[dict[str, object]]] = {
            "duplicate id": [DEFECT, DEFECT],
            "unnamed with a rule": [{**DEFECT, "rule": "rules/general.md#design"}],
            "named without a rule": [{**DEFECT, "named_by_standards": True}],
            "no sites": [{**DEFECT, "sites": []}],
            "no kinds": [{**DEFECT, "kinds": []}],
            "unknown kind": [{**DEFECT, "kinds": ["style"]}],
        }
        for name, defects in cases.items():
            with self.subTest(case=name), self.assertRaises(ValueError):
                self.parse(defects)

    def test_a_consistent_spec_parses(self) -> None:
        self.parse([DEFECT])


class SeverityTests(unittest.TestCase):
    """Must-fix defects need must-fix findings; should-fix defects take either severity."""

    def test_a_should_fix_finding_does_not_find_a_must_fix_defect(self) -> None:
        score = score_findings((race(41, 42, Severity.SHOULD_FIX),), SPEC)
        self.assertFalse(score.found["race"])
        self.assertEqual(score.false_positives, 0)

    def test_a_should_fix_defect_is_found_at_either_severity(self) -> None:
        for severity in Severity:
            with self.subTest(severity=severity):
                score = score_findings((finding("src/a.rs", 90, 90, FindingKind.IDIOM, severity),), SPEC)
                self.assertTrue(score.found["clone"])
                self.assertEqual(score.false_positives, 0)

    def test_stray_should_fix_findings_cost_nothing(self) -> None:
        score = score_findings((finding("src/z.rs", 1, 2, severity=Severity.SHOULD_FIX),), SPEC)
        self.assertEqual(score.false_positives, 0)


class BaitAndDuplicateTests(unittest.TestCase):
    """Bait is judged by location, and repeated findings on one defect count once without penalty."""

    def test_a_must_fix_finding_on_bait_is_a_false_positive(self) -> None:
        score = score_findings((finding("src/c.rs", 6, 6, FindingKind.DESIGN),), SPEC)
        self.assertTrue(score.bait_hits["trait"])
        self.assertEqual(score.false_positives, 1)

    def test_a_should_fix_finding_on_bait_is_no_hit(self) -> None:
        score = score_findings((finding("src/c.rs", 6, 6, FindingKind.DESIGN, Severity.SHOULD_FIX),), SPEC)
        self.assertFalse(score.bait_hits["trait"])

    def test_duplicates_find_a_defect_once(self) -> None:
        score = score_findings((race(40, 41), race(43, 44)), SPEC)
        self.assertTrue(score.found["race"])
        self.assertEqual(score.false_positives, 0)
        self.assertEqual(score.recall(), 1 / 3)

    def test_one_finding_earns_credit_for_one_defect(self) -> None:
        spec = ReviewSpec(
            defects=(
                PlantedDefect(
                    "first", (Site("t.rs", LineRange(15, 16)),), (FindingKind.TESTS,), Severity.MUST_FIX, None
                ),
                PlantedDefect(
                    "second", (Site("t.rs", LineRange(19, 30)),), (FindingKind.TESTS,), Severity.MUST_FIX, None
                ),
            ),
            bait=(),
            expected_verdict="fix first",
        )
        score = score_findings((finding("t.rs", 15, 16, FindingKind.TESTS),), spec)
        self.assertEqual(score.found, {"first": True, "second": False})


class ArithmeticTests(unittest.TestCase):
    """Recall and precision, overall and for the defects the standards name and those they do not."""

    def test_precision_counts_planted_defects_and_false_positives(self) -> None:
        findings = (race(41, 42), finding("src/z.rs", 1, 1), finding("src/c.rs", 5, 5))
        score = score_findings(findings, SPEC)
        self.assertEqual(score.false_positives, 2)
        self.assertEqual(score.recall(), 1 / 3)
        self.assertEqual(score.precision(), 1 / 5)
        self.assertEqual(score.recall(named=True), 1 / 2)
        self.assertEqual(score.precision(named=True), 1 / 4)
        self.assertEqual(score.recall(named=False), 0.0)
        self.assertEqual(score.precision(named=False), 0.0)

    def test_one_finding_on_each_defect_scores_one(self) -> None:
        findings = (race(40, 44), finding("src/b.rs", 10, 12), finding("src/a.rs", 90, 90, FindingKind.IDIOM))
        score = score_findings(findings, SPEC)
        self.assertEqual((score.recall(), score.precision()), (1.0, 1.0))


class FindingsFileTests(unittest.TestCase):
    """The verifier's reading of `findings.jsonl`: empty means none found, anything unreadable scores 0."""

    def grade(self, findings: str | None) -> tuple[float, bool]:
        root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(root)
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            spec = folder / "review_spec.json"
            defect = DEFECT
            spec.write_text(
                json.dumps({"expected_verdict": "fix first", "defects": [defect], "bait": []}), encoding="utf-8"
            )
            findings_path = folder / "findings.jsonl"
            if findings is not None:
                findings_path.write_text(findings, encoding="utf-8")
            result, _ = grade_review_trial(folder / "REVIEW.md", findings_path, spec, inventory)
        readable = next(r.passed for r in result.rules if r.rule_id == "review/findings#readable")
        return result.functional, readable

    def test_one_finding_on_the_defect_scores_one(self) -> None:
        self.assertEqual(self.grade(GOOD_LINE), (1.0, True))

    def test_an_empty_file_finds_nothing(self) -> None:
        self.assertEqual(self.grade(""), (0.0, True))
        self.assertEqual(self.grade("\n  \n"), (0.0, True))

    def test_a_missing_file_scores_zero(self) -> None:
        self.assertEqual(self.grade(None), (0.0, False))

    def test_an_unreadable_line_scores_zero(self) -> None:
        cases = {
            "not json": "src/b.rs:10 must-fix\n",
            "not an object": "[1, 2]\n",
            "unknown kind": GOOD_LINE.replace("correctness", "style"),
            "unknown severity": GOOD_LINE.replace('"must-fix"', '"blocker"'),
            "reversed lines": GOOD_LINE.replace("[10, 12]", "[12, 10]"),
            "missing finding": GOOD_LINE.replace(', "finding": "made up"', ""),
        }
        for name, bad in cases.items():
            with self.subTest(case=name):
                self.assertEqual(self.grade(GOOD_LINE + bad), (0.0, False))


TASKS = Path(__file__).resolve().parents[2] / "tasks"


def score_task(task: str, *findings: Finding) -> tuple[set[str], int]:
    score = score_findings(findings, parse_review_spec(TASKS / task / "tests" / "review_spec.json"))
    return {item for item, found in score.found.items() if found}, score.false_positives


class TaskSpecAlternativeTests(unittest.TestCase):
    """The task specs' alternative sites and kinds credit the defect a finding describes, and only that one."""

    def test_each_alternative_credits_its_defect(self) -> None:
        mutants = "tools/crates/rust-mutants/src/"
        should_fix = Severity.SHOULD_FIX
        cases = [
            ("review-makiatto-live-reconfiguration", finding("src/wasm.rs", 27, 30), "transform_cache_ignores_env"),
            ("review-makiatto-live-reconfiguration", finding("src/wasm.rs", 88, 91), "transform_cache_ignores_env"),
            (
                "review-rust-mutants-prepared-targets",
                finding(mutants + "shards.rs", 34, 36, FindingKind.CONCURRENCY),
                "shared_mutable_target_across_shards",
            ),
            (
                "review-rust-mutants-prepared-targets",
                finding(mutants + "shards.rs", 79, 84, FindingKind.CONCURRENCY),
                "shared_mutable_target_across_shards",
            ),
            (
                "review-rust-mutants-prepared-targets",
                finding(mutants + "command.rs", 55, 58),
                "prebuild_failure_ignored",
            ),
            ("review-rust-mutants-prepared-targets", finding(mutants + "main.rs", 70, 74), "prebuild_failure_ignored"),
            (
                "review-rust-mutants-prepared-targets",
                finding(mutants + "command.rs", 21, 22, FindingKind.PERFORMANCE),
                "warm_cold_profile_mismatch",
            ),
            (
                "review-rust-mutants-prepared-targets",
                finding(mutants + "shards.rs", 60, 63, FindingKind.CONCURRENCY),
                "linked_workspace_overwrites_subject",
            ),
            (
                "review-soppo-match-divergence",
                finding("src/infer.rs", 51, 51, FindingKind.IDIOM, should_fix),
                "wildcard_hides_expression_variants",
            ),
        ]
        for task, variant, defect in cases:
            with self.subTest(task=task, site=(variant.file, variant.lines), kind=variant.kind):
                self.assertEqual(score_task(task, variant), ({defect}, 0))

    def test_the_shared_target_line_credits_only_the_shared_target(self) -> None:
        task = "review-rust-mutants-prepared-targets"
        line = "tools/crates/rust-mutants/src/shards.rs"
        as_concurrency = finding(line, 37, 37, FindingKind.CONCURRENCY)
        as_correctness = finding(line, 37, 37, FindingKind.CORRECTNESS)
        for variant in (as_concurrency, as_correctness):
            with self.subTest(kind=variant.kind):
                self.assertEqual(score_task(task, variant), ({"shared_mutable_target_across_shards"}, 0))
        spec = parse_review_spec(TASKS / task / "tests" / "review_spec.json")
        together = score_findings((as_concurrency, as_correctness), spec)
        self.assertEqual(sum(together.found.values()), 1)
        self.assertEqual(together.false_positives, 0)
        self.assertEqual((together.recall(), together.precision()), (1 / 5, 1 / 5))
