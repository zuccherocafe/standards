"""Checks the command boundary between adherence grading and the workspace scanner."""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

import grade_adherence


class RustScansCommandTests(unittest.TestCase):
    def test_workspace_is_passed_with_the_scanner_flag(self) -> None:
        repo = Path("/app")
        responses = [
            subprocess.CompletedProcess([], 0, "rust-scans\n", ""),
            subprocess.CompletedProcess([], 0, "H1: 0\nH2: 0\nH3: 0\nH4: 0\n", ""),
        ]
        with patch.object(grade_adherence, "_run_cmd", side_effect=responses) as run:
            passed, _ = grade_adherence.check_rust_scans_clean(repo, "Cargo.toml", [])
        self.assertTrue(passed)
        self.assertEqual(run.call_args_list[-1].args, (["rust-scans", "--workspace", "/app"], repo))

    def test_bodyless_commit_fails_commit_hygiene(self) -> None:
        root = Path(__file__).resolve().parents[3]
        inventory = grade_adherence.discover_standards(root)
        cfg = grade_adherence.load_live_rules_config(inventory)
        log_out = subprocess.CompletedProcess([], 0, "deadbeef\x1ffeat: wrong subject\x1f\x1e", "")
        with patch.object(grade_adherence, "_run_cmd", return_value=log_out):
            passed, detail = grade_adherence.check_commit_hygiene(Path("/app"), "base", cfg)
        self.assertFalse(passed)
        self.assertIn("feat", detail)
