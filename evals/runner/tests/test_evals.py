"""Unit tests for the `evals` CLI and live standards discovery (`adherence.py` and `evals.py`).

Run with `python3 -I -m unittest discover -s evals/tests -t evals`. Writes only inside temporary directories that each
test removes.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from adherence import (
    Condition,
    Track,
    discover_standards,
    format_condition_label,
    parse_pi_session_jsonl,
    slugify_heading,
)

from evals import (
    TrialSummary,
    build_harbor_command,
    discover_tasks,
    main,
    render_markdown_report,
    render_report,
    resolve_run_metadata,
    stage_condition_dataset,
)


class TestEvals(unittest.TestCase):
    """Tests for standards inventory discovery, trajectory parsing, dataset staging, and reporting."""

    def test_slugify_heading_matches_github_fragments(self) -> None:
        self.assertEqual(slugify_heading("How strictly a rule applies"), "how-strictly-a-rule-applies")
        self.assertEqual(slugify_heading("What `design-read check` checks"), "what-design-read-check-checks")

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
                            "arguments": {"command": "design-read check && git diff"},
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
        self.assertTrue(audit.ran_design_read_check)
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

    def test_main_help_exits_cleanly(self) -> None:
        with self.assertRaises(SystemExit) as ctx:
            main(["--help"])
        self.assertEqual(ctx.exception.code, 0)
