"""Discovers live standards files, rules, and headings from the repository root, and audits trial adherence.

Run through `evals.py` or imported by `evals/tests/test_evals.py`. Never hardcodes rule contents or skill files:
every rule file, heading anchor, numeric rule bound, and skill path is read from the live working tree at runtime.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import assert_never


class Condition(StrEnum):
    """One experimental condition in the evaluation matrix."""

    NONE = "none"
    AGENT_SKILLS = "agent-skills"
    SUPERPOWERS = "superpowers"
    STANDARDS = "standards"


class Track(StrEnum):
    """Whether a task evaluates writing a change or reviewing a change."""

    WRITE = "write"
    REVIEW = "review"


@dataclass(frozen=True, slots=True)
class RuleHeading:
    """One section or rule heading discovered from a live Markdown file under `rules/`."""

    file: str
    title: str
    anchor: str
    kind: str | None
    text: str = ""

    def __post_init__(self) -> None:
        if not self.file or not self.title or not self.anchor:
            raise ValueError("a rule heading requires a non-empty file, title, and anchor")


@dataclass(frozen=True, slots=True)
class StandardsInventory:
    """The live files, headings, and rule texts discovered from the repository root."""

    rule_files: tuple[str, ...]
    skill_files: tuple[str, ...]
    template_files: tuple[str, ...]
    headings: tuple[RuleHeading, ...]

    def find_rule(self, title_fragment: str) -> RuleHeading | None:
        """Find a live rule heading whose title matches `title_fragment` case-insensitively."""
        needle = title_fragment.lower()
        for heading in self.headings:
            if heading.title.lower() == needle or needle in heading.title.lower():
                return heading
        return None


@dataclass(frozen=True, slots=True)
class TrajectoryAudit:
    """What an agent's session JSONL shows it successfully read and ran during a trial."""

    files_read: tuple[str, ...]
    commands_run: tuple[str, ...]
    standards_files_read: tuple[str, ...]
    skills_read: tuple[str, ...]
    ran_concord_check: bool
    ran_rust_scans: bool
    ran_git_diff_or_show: bool
    turn_count: int
    input_tokens: int
    output_tokens: int
    cached_tokens: int
    cost_usd: float


@dataclass(frozen=True, slots=True)
class RuleCheckResult:
    """Whether a single applicable rule was satisfied on a trial, with its reason."""

    rule_id: str
    applicable: bool
    passed: bool
    detail: str


@dataclass(frozen=True, slots=True)
class EvaluationResult:
    """Typed grading output for a write or review trial."""

    track: Track
    reward: float
    functional: float
    adherence: float
    rules: tuple[RuleCheckResult, ...]


_HEADING_RE = re.compile(r"^(#{1,4})\s+(.+?)\s*$")
_BOLD_RULE_RE = re.compile(r"^\*\*([^*]+?)\.?\*\*\s*(?:\*(Always|Default|Weigh)\.\*)?\s*(.*)$")
_ITALIC_KIND_RE = re.compile(r"^\*(Always|Default|Weigh)\.\*\s*(.*)$")


def slugify_heading(title: str) -> str:
    """GitHub/lychee Markdown anchor slug for `title`."""
    cleaned = re.sub(r"`([^`]*)`", r"\1", title.strip().lower())
    cleaned = re.sub(r"[^\w\s-]", "", cleaned)
    return re.sub(r"\s+", "-", cleaned).strip("-")


def discover_standards(repo_root: Path) -> StandardsInventory:
    """Scan the live `rules/`, `skills/`, and `templates/` folders under `repo_root`."""
    rules_dir = repo_root / "rules"
    skills_dir = repo_root / "skills"
    templates_dir = repo_root / "templates"
    if not rules_dir.is_dir() or not skills_dir.is_dir():
        raise ValueError(f"{repo_root}: missing rules/ or skills/ directory")

    rule_files = tuple(sorted(path.relative_to(repo_root).as_posix() for path in rules_dir.rglob("*.md")))
    skill_files = tuple(sorted(path.relative_to(repo_root).as_posix() for path in skills_dir.glob("*/SKILL.md")))
    template_files = tuple(
        sorted(path.relative_to(repo_root).as_posix() for path in templates_dir.rglob("*") if path.is_file())
        if templates_dir.is_dir()
        else ()
    )

    headings: list[RuleHeading] = []
    for rel_path in rule_files:
        full_path = repo_root / rel_path
        text = full_path.read_text(encoding="utf-8")
        headings.extend(_extract_headings(rel_path, text))

    return StandardsInventory(
        rule_files=rule_files,
        skill_files=skill_files,
        template_files=template_files,
        headings=tuple(headings),
    )


def _extract_headings(rel_path: str, text: str) -> list[RuleHeading]:
    """Extract Markdown headings and bold named rules with their Always/Default/Weigh kinds and body text."""
    results: list[RuleHeading] = []
    in_fence = False
    lines = text.splitlines()
    current_anchor = ""
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue

        heading_match = _HEADING_RE.match(line)
        if heading_match is not None:
            title = heading_match.group(2).strip()
            anchor = slugify_heading(title)
            current_anchor = anchor
            kind: str | None = None
            body_lines: list[str] = []
            for lookahead in lines[index + 1 : index + 12]:
                la_strip = lookahead.strip()
                if not la_strip:
                    continue
                if _HEADING_RE.match(lookahead) or _BOLD_RULE_RE.match(la_strip):
                    break
                kind_match = _ITALIC_KIND_RE.match(la_strip)
                if kind_match is not None and kind is None:
                    kind = kind_match.group(1)
                    body_lines.append(kind_match.group(2))
                else:
                    body_lines.append(la_strip)
            results.append(
                RuleHeading(
                    file=rel_path,
                    title=title,
                    anchor=anchor,
                    kind=kind,
                    text=" ".join(body_lines).strip(),
                )
            )
            continue

        bold_match = _BOLD_RULE_RE.match(stripped)
        if bold_match is not None and current_anchor:
            rule_title = bold_match.group(1).strip()
            rule_kind = bold_match.group(2)
            rule_body = bold_match.group(3).strip()
            results.append(
                RuleHeading(
                    file=rel_path,
                    title=rule_title,
                    anchor=current_anchor,
                    kind=rule_kind,
                    text=rule_body,
                )
            )
    return results


def parse_pi_session_jsonl(
    jsonl_text: str,
    inventory: StandardsInventory,
) -> TrajectoryAudit:
    """Extract verified file reads, bash commands, token counts, and standards coverage from a Pi session JSONL."""
    pending_reads: dict[str, str] = {}
    pending_bash: dict[str, str] = {}
    verified_reads: list[str] = []
    verified_commands: list[str] = []
    turn_count = 0
    input_tokens = 0
    output_tokens = 0
    cached_tokens = 0
    cost_usd = 0.0

    for raw_line in jsonl_text.splitlines():
        line = raw_line.strip()
        if not line.startswith("{"):
            continue
        try:
            event: object = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict):
            continue

        message = event.get("message")
        if not isinstance(message, dict):
            continue
        role = message.get("role")
        if role == "assistant":
            turn_count += 1
            usage = message.get("usage")
            if isinstance(usage, dict):
                inp = usage.get("input")
                out = usage.get("output")
                cache_r = usage.get("cacheRead")
                cache_w = usage.get("cacheWrite")
                if isinstance(inp, int):
                    input_tokens += inp
                if isinstance(out, int):
                    output_tokens += out
                if isinstance(cache_r, int):
                    cached_tokens += cache_r
                    input_tokens += cache_r
                if isinstance(cache_w, int):
                    input_tokens += cache_w
                cost = usage.get("cost")
                if isinstance(cost, dict):
                    total = cost.get("total")
                    if isinstance(total, (int, float)):
                        cost_usd += float(total)

            content = message.get("content")
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict) or block.get("type") != "toolCall":
                        continue
                    call_id = block.get("id")
                    name = block.get("name")
                    args = block.get("arguments")
                    if not isinstance(call_id, str) or not isinstance(args, dict):
                        continue
                    if name == "read":
                        path_arg = args.get("path")
                        if isinstance(path_arg, str):
                            pending_reads[call_id] = path_arg
                    elif name == "bash":
                        cmd_arg = args.get("command")
                        if isinstance(cmd_arg, str):
                            pending_bash[call_id] = cmd_arg
        elif role == "toolResult":
            call_id = message.get("toolCallId")
            is_error = bool(message.get("isError", False))
            result_text = _extract_message_text(message.get("content"))
            failed_read = is_error or result_text.startswith("ENOENT:") or result_text.startswith("Error:")
            if isinstance(call_id, str):
                if call_id in pending_reads and not failed_read:
                    read_path = pending_reads[call_id]
                    if read_path not in verified_reads:
                        verified_reads.append(read_path)
                elif call_id in pending_bash and not is_error:
                    verified_commands.append(pending_bash[call_id])

    matched_rules: list[str] = []
    matched_skills: list[str] = []
    for read_path in verified_reads:
        norm = read_path.replace("\\", "/")
        for rule_file in inventory.rule_files:
            if (norm.endswith(rule_file) or f"/{rule_file}" in norm) and rule_file not in matched_rules:
                matched_rules.append(rule_file)
        for skill_file in inventory.skill_files:
            skill_dir = str(Path(skill_file).parent)
            if (norm.endswith(skill_file) or f"/{skill_dir}/SKILL.md" in norm) and skill_file not in matched_skills:
                matched_skills.append(skill_file)

    bash_read_re = re.compile(r"\b(?:cat|head|tail|sed|awk|less|more)\b")
    for cmd in verified_commands:
        if not bash_read_re.search(cmd):
            continue
        for rule_file in inventory.rule_files:
            if (
                re.search(rf"\b(?:cat|head|tail|sed|awk|less|more)\b[^\n;&|]*{re.escape(rule_file)}", cmd)
                and rule_file not in matched_rules
            ):
                matched_rules.append(rule_file)
        for skill_file in inventory.skill_files:
            if (
                re.search(rf"\b(?:cat|head|tail|sed|awk|less|more)\b[^\n;&|]*{re.escape(skill_file)}", cmd)
                and skill_file not in matched_skills
            ):
                matched_skills.append(skill_file)

    ran_concord = any("concord check" in cmd for cmd in verified_commands)
    ran_rust_scans = any("rust-scans" in cmd for cmd in verified_commands)
    ran_diff = any("git diff" in cmd or "git show" in cmd for cmd in verified_commands)

    return TrajectoryAudit(
        files_read=tuple(verified_reads),
        commands_run=tuple(verified_commands),
        standards_files_read=tuple(sorted(matched_rules)),
        skills_read=tuple(sorted(matched_skills)),
        ran_concord_check=ran_concord,
        ran_rust_scans=ran_rust_scans,
        ran_git_diff_or_show=ran_diff,
        turn_count=turn_count,
        input_tokens=input_tokens,
        output_tokens=output_tokens,
        cached_tokens=cached_tokens,
        cost_usd=cost_usd,
    )


def _extract_message_text(content: object) -> str:
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, dict):
                text = item.get("text")
                if isinstance(text, str):
                    parts.append(text)
        return "\n".join(parts).strip()
    return ""


def format_condition_label(condition: Condition) -> str:
    """Return the display label for `condition`."""
    match condition:
        case Condition.NONE:
            return "none"
        case Condition.AGENT_SKILLS:
            return "agent-skills"
        case Condition.SUPERPOWERS:
            return "superpowers"
        case Condition.STANDARDS:
            return "standards"
        case _:
            assert_never(condition)
