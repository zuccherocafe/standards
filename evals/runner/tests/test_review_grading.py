"""Authored scoring examples for missing and empty review submissions."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from adherence import discover_standards
from grade_adherence import grade_review_trial


class TestReviewGrading(unittest.TestCase):
    """A missing review and a whitespace-only review both receive no credit."""

    def test_missing_and_empty_reviews_score_zero(self) -> None:
        root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(root)
        spec = root / "evals/tasks/review-soppo-match-divergence/tests/review_spec.json"
        cache = root / ".cache"
        cache.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=cache) as directory:
            review = Path(directory) / "REVIEW.md"
            for text in (None, "", "  \n\t"):
                with self.subTest(text=text):
                    if text is not None:
                        review.write_text(text, encoding="utf-8")
                    scores = grade_review_trial(review, spec, inventory)
                    self.assertEqual(scores.reward, 0.0)
                    self.assertEqual(scores.functional, 0.0)
                    self.assertEqual(scores.adherence, 0.0)

    def test_negated_denials_do_not_receive_must_fix_credit(self) -> None:
        root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(root)
        spec = root / "evals/tasks/review-makiatto-live-reconfiguration/tests/review_spec.json"
        cache = root / ".cache"
        cache.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=cache) as directory:
            review = Path(directory) / "REVIEW.md"
            review.write_text(
                "## Notes\n"
                "In src/service.rs there is no dropped joinhandle and no detached old server task.\n"
                "In src/wasm.rs the ResultKey is valid; stale output does not occur.\n"
                "In src/config.rs there is no divide-by-zero on weight = 0.\n"
                "Verdict: land\n",
                encoding="utf-8",
            )
            scores = grade_review_trial(review, spec, inventory)
            by_id = {r.rule_id: r.passed for r in scores.rules}
            self.assertFalse(by_id["review/must-fix#detached_server_on_timeout"])
            self.assertFalse(by_id["review/must-fix#transform_cache_ignores_env"])
            self.assertFalse(by_id["review/must-fix#unvalidated_route_weight_deserialize"])
