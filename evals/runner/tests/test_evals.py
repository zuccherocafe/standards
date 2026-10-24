"""Unit tests for the `evals` CLI and live standards discovery (`adherence.py` and `evals.py`).

Run with `python3 -I -m unittest discover -s evals/tests -t evals`. Writes only inside temporary directories that each
test removes.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import grade_adherence
from adherence import (
    Condition,
    Track,
    discover_standards,
    format_condition_label,
    parse_pi_session_jsonl,
    slugify_heading,
)

from evals import (
    DefectOutcome,
    ReviewOutcome,
    TrialSummary,
    build_harbor_command,
    check_review_spec,
    collect_job_summaries,
    discover_tasks,
    main,
    pool_review_outcomes,
    render_markdown_report,
    render_report,
    resolve_run_metadata,
    stage_condition_dataset,
)


class TestEvals(unittest.TestCase):
    """Tests for standards inventory discovery, trajectory parsing, dataset staging, and reporting."""

    def test_slugify_heading_matches_github_fragments(self) -> None:
        self.assertEqual(slugify_heading("How strictly a rule applies"), "how-strictly-a-rule-applies")
        self.assertEqual(slugify_heading("What `concord check` checks"), "what-concord-check-checks")

    def test_discover_live_standards_from_repository(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(repo_root)
        self.assertIn("rules/general.md", inventory.rule_files)
        self.assertIn("rules/review.md", inventory.rule_files)
        self.assertIn("rules/languages/rust/design.md", inventory.rule_files)
        self.assertIn("skills/change-write/SKILL.md", inventory.skill_files)
        self.assertIn("skills/change-review/SKILL.md", inventory.skill_files)
        titles = {h.title for h in inventory.headings}
        self.assertIn("Parse at the boundary", titles)
        self.assertIn("Keep the system coherent", titles)

    def test_parse_pi_session_jsonl_tracks_reads_and_usage(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(repo_root)
        events = [
            {"type": "session", "id": "sess-1"},
            {
                "type": "message",
                "id": "m1",
                "message": {
                    "role": "assistant",
                    "usage": {
                        "input": 1000,
                        "output": 250,
                        "cacheRead": 500,
                        "cacheWrite": 100,
                        "cost": {"total": 0.0125},
                    },
                    "content": [
                        {
                            "type": "toolCall",
                            "id": "tc1",
                            "name": "read",
                            "arguments": {"path": "/standards/rules/general.md"},
                        },
                        {
                            "type": "toolCall",
                            "id": "tc2",
                            "name": "read",
                            "arguments": {"path": "/root/.agents/skills/change-write/SKILL.md"},
                        },
                        {
                            "type": "toolCall",
                            "id": "tc3",
                            "name": "read",
                            "arguments": {"path": "/standards/rules/review.md"},
                        },
                        {
                            "type": "toolCall",
                            "id": "tc4",
                            "name": "bash",
                            "arguments": {"command": "concord check && git diff"},
                        },
                    ],
                },
            },
            {
                "type": "message",
                "id": "tr1",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "tc1",
                    "isError": False,
                    "content": "# Engineering rules",
                },
            },
            {
                "type": "message",
                "id": "tr2",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "tc2",
                    "isError": False,
                    "content": "# Write a change from its brief",
                },
            },
            {
                "type": "message",
                "id": "tr3",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "tc3",
                    "isError": True,
                    "content": "ENOENT: no such file or directory",
                },
            },
            {
                "type": "message",
                "id": "tr4",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "tc4",
                    "isError": False,
                    "content": "passes",
                },
            },
        ]
        jsonl_text = "\n".join(json.dumps(e) for e in events)
        audit = parse_pi_session_jsonl(jsonl_text, inventory)
        self.assertEqual(audit.turn_count, 1)
        self.assertEqual(audit.input_tokens, 1600)
        self.assertEqual(audit.output_tokens, 250)
        self.assertEqual(audit.cached_tokens, 500)
        self.assertAlmostEqual(audit.cost_usd, 0.0125)
        self.assertIn("rules/general.md", audit.standards_files_read)
        self.assertNotIn("rules/review.md", audit.standards_files_read)
        self.assertIn("skills/change-write/SKILL.md", audit.skills_read)
        self.assertTrue(audit.ran_concord_check)
        self.assertTrue(audit.ran_git_diff_or_show)

    def test_stage_condition_and_report(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(repo_root)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            tasks_dir = tmp_path / "tasks"
            task_a = tasks_dir / "sample-write"
            (task_a / "environment").mkdir(parents=True)
            (task_a / "tests").mkdir(parents=True)
            (task_a / "solution").mkdir(parents=True)
            (task_a / "task.toml").write_text(
                'schema_version = "1.4"\n[task]\nname = "standards/sample-write"\n'
                'description = "Sample write task"\n[metadata]\ntrack = "write"\n',
                encoding="utf-8",
            )
            (task_a / "instruction.md").write_text("Build feature X.\n", encoding="utf-8")
            (task_a / "tests" / "test.sh").write_text(
                "#!/bin/sh\necho 1.0 > /logs/verifier/reward.txt\n", encoding="utf-8"
            )
            (task_a / "solution" / "solve.sh").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")

            discovered = discover_tasks(tasks_dir)
            self.assertEqual(len(discovered), 1)
            self.assertEqual(discovered[0].track, Track.WRITE)

            stage_root = tmp_path / "staged"
            none_dir = stage_condition_dataset(repo_root, discovered, Condition.NONE, stage_root, None)
            std_dir = stage_condition_dataset(repo_root, discovered, Condition.STANDARDS, stage_root, None)

            self.assertEqual(
                (none_dir / "sample-write" / "instruction.md").read_text(encoding="utf-8"),
                "Build feature X.\n",
            )
            self.assertEqual(
                (std_dir / "sample-write" / "instruction.md").read_text(encoding="utf-8"),
                "Build feature X.\n",
            )
            self.assertIn(
                "/standards/skills/change-write/SKILL.md",
                (
                    std_dir / "sample-write" / "environment" / "condition_overlay" / "standards_agents_append.md"
                ).read_text(encoding="utf-8"),
            )
            self.assertTrue(
                (
                    std_dir
                    / "sample-write"
                    / "environment"
                    / "condition_overlay"
                    / "standards"
                    / "rules"
                    / "general.md"
                ).is_file()
            )

            cmd = build_harbor_command(
                harbor_bin="harbor",
                dataset_dir=std_dir,
                jobs_dir=tmp_path / "jobs",
                job_name="trial",
                model="gpt-6.1-sol",
                n_concurrent=1,
                n_attempts=1,
                effort="high",
                max_turns=25,
            )
            self.assertIn("openai-codex/gpt-6.1-sol", cmd)
            self.assertEqual(format_condition_label(Condition.STANDARDS), "standards")

            summaries = [
                TrialSummary(
                    task="sample-write",
                    track=Track.WRITE,
                    condition=Condition.NONE,
                    reward=0.25,
                    functional_score=0.5,
                    adherence_score=0.0,
                    turns=12,
                    cost_usd=0.05,
                    standards_files_read=(),
                    skills_read=(),
                    rule_outcomes={"general#parse-at-the-boundary": False},
                ),
                TrialSummary(
                    task="sample-write",
                    track=Track.WRITE,
                    condition=Condition.STANDARDS,
                    reward=1.0,
                    functional_score=1.0,
                    adherence_score=1.0,
                    turns=10,
                    cost_usd=0.08,
                    standards_files_read=("rules/general.md",),
                    skills_read=("skills/change-write/SKILL.md",),
                    rule_outcomes={"general#parse-at-the-boundary": True},
                ),
            ]
            report = render_report(summaries, inventory)
            self.assertIn("sample-write", report)
            self.assertIn("general#parse-at-the-boundary", report)

            meta = resolve_run_metadata(repo_root, title="test-run")
            self.assertEqual(meta.agent_skills_commit, "aeac41d4d005999432bb7312f7ff06383a91d59d")
            md_report = render_markdown_report(summaries, inventory, meta)
            self.assertIn("Agent-skills commit: `aeac41d4d005999432bb7312f7ff06383a91d59d`", md_report)
            self.assertIn("100.0%", md_report)

    def test_review_report_splits_recall_by_group(self) -> None:
        def trial(condition: Condition, found: tuple[bool, bool, bool], false_positives: int) -> TrialSummary:
            defects = tuple(
                DefectOutcome(task="review-x", defect_id=name, named_by_standards=named, found=hit)
                for (name, named), hit in zip((("a", True), ("b", True), ("c", False)), found, strict=True)
            )
            return TrialSummary(
                task="review-x",
                track=Track.REVIEW,
                condition=condition,
                reward=0.0,
                functional_score=0.0,
                adherence_score=0.0,
                turns=1,
                cost_usd=0.0,
                standards_files_read=(),
                skills_read=(),
                rule_outcomes={},
                review=ReviewOutcome(defects=defects, false_positives=false_positives),
            )

        ungraded = TrialSummary(
            task="review-x",
            track=Track.REVIEW,
            condition=Condition.NONE,
            reward=0.0,
            functional_score=0.0,
            adherence_score=0.0,
            turns=1,
            cost_usd=0.0,
            standards_files_read=(),
            skills_read=(),
            rule_outcomes={},
        )
        summaries = [
            trial(Condition.NONE, (True, False, False), 1),
            trial(Condition.NONE, (True, True, True), 0),
            ungraded,
            trial(Condition.STANDARDS, (True, True, False), 0),
        ]
        totals = pool_review_outcomes([s for s in summaries if s.condition == Condition.NONE])
        self.assertEqual(totals.recall(), 4 / 6)
        self.assertEqual(totals.recall(named=True), 3 / 4)
        self.assertEqual(totals.recall(named=False), 1 / 2)
        self.assertEqual(totals.precision(), 4 / 7)
        self.assertEqual(totals.precision(named=False), 1 / 3)

        inventory = discover_standards(Path(__file__).resolve().parents[3])
        meta = resolve_run_metadata(Path(__file__).resolve().parents[3], title="test-run")
        md_report = render_markdown_report(summaries, inventory, meta)
        self.assertIn("| `none` | 0.67 | 0.75 | 0.50 | 0.57 | 0.60 | 0.33 | 1 |", md_report)
        self.assertIn("| `standards` | 0.67 | 1.00 | 0.00 | 0.67 | 1.00 | 0.00 | 0 |", md_report)
        self.assertIn("| `review-x/c` | unnamed | 1/2 | - | - | 0/1 |", md_report)
        self.assertIn("review-x/c", render_report(summaries, inventory))

    def test_a_graded_review_reaches_the_report(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        inventory = discover_standards(repo_root)
        spec: dict[str, object] = {
            "expected_verdict": "fix first",
            "defects": [
                {
                    "id": "race",
                    "sites": [{"file": "src/a.rs", "lines": [40, 44]}],
                    "kinds": ["concurrency"],
                    "severity": "must-fix",
                    "named_by_standards": True,
                    "rule": "rules/languages/rust/async.md#every-task-has-an-owner",
                },
                {
                    "id": "exit",
                    "sites": [{"file": "src/b.rs", "lines": [10, 12]}],
                    "kinds": ["correctness"],
                    "severity": "must-fix",
                    "named_by_standards": False,
                },
            ],
            "bait": [],
        }
        findings = (
            '{"file": "src/a.rs", "lines": [41, 42], "kind": "concurrency", "severity": "must-fix", "finding": "x"}\n'
            '{"file": "src/z.rs", "lines": [1, 1], "kind": "correctness", "severity": "must-fix", "finding": "y"}\n'
        )
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            (folder / "review_spec.json").write_text(json.dumps(spec), encoding="utf-8")
            (folder / "findings.jsonl").write_text(findings, encoding="utf-8")
            job = folder / "job"
            for trial_name in ("graded", "ungraded"):
                trial_dir = job / "none" / trial_name
                (trial_dir / "verifier").mkdir(parents=True)
                (trial_dir / "result.json").write_text(
                    json.dumps({"task_name": "standards/review-x", "verifier_result": {"rewards": {"reward": 0.0}}}),
                    encoding="utf-8",
                )
            graded = job / "none" / "graded" / "verifier"
            status = grade_adherence.main(
                [
                    "review",
                    f"--review-file={folder / 'REVIEW.md'}",
                    f"--findings-file={folder / 'findings.jsonl'}",
                    f"--spec-file={folder / 'review_spec.json'}",
                    f"--standards-root={repo_root}",
                    f"--verifier-dir={graded}",
                ]
            )
            self.assertEqual(status, 0)
            summaries = collect_job_summaries(job, inventory)
            meta = resolve_run_metadata(repo_root, title="test-run")
            md_report = render_markdown_report(summaries, inventory, meta)
        self.assertIn("| `none` | 0.50 | 1.00 | 0.00 | 0.33 | 0.50 | 0.00 | 1 |", md_report)
        self.assertIn("| `review-x/exit` | unnamed | 0/1 | - | - | - |", md_report)

    def test_a_review_spec_must_cite_live_rules(self) -> None:
        inventory = discover_standards(Path(__file__).resolve().parents[3])
        defect = {
            "id": "race",
            "sites": [{"file": "src/a.rs", "lines": [40, 44]}],
            "kinds": ["concurrency"],
            "severity": "must-fix",
            "named_by_standards": True,
            "rule": "rules/languages/rust/async.md#every-task-has-an-owner",
        }
        with tempfile.TemporaryDirectory() as tmp:
            spec_path = Path(tmp) / "review_spec.json"
            for rule, refused in (
                ("rules/languages/rust/async.md#every-task-has-an-owner", False),
                ("rules/languages/rust/async.md#no-such-rule", True),
            ):
                with self.subTest(rule=rule):
                    spec: dict[str, object] = {
                        "expected_verdict": "fix first",
                        "defects": [{**defect, "rule": rule}],
                        "bait": [],
                    }
                    spec_path.write_text(json.dumps(spec), encoding="utf-8")
                    if refused:
                        with self.assertRaises(ValueError):
                            check_review_spec(spec_path, inventory)
                    else:
                        check_review_spec(spec_path, inventory)

    def test_main_help_exits_cleanly(self) -> None:
        with self.assertRaises(SystemExit) as ctx:
            main(["--help"])
        self.assertEqual(ctx.exception.code, 0)
