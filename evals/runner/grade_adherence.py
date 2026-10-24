"""Evaluates a completed trial's standards adherence or review findings inside the task verifier.

Run by `/tests/test.sh` as `python3 -I /tests/grade_adherence.py <mode> ...`. Reads the live standards from
`/tests/live_standards/` (copied at staging time from the repository root) and writes `/logs/verifier/reward.json`,
`/logs/verifier/reward.txt`, and `/logs/verifier/adherence.json`.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import argparse
import json
import re
import subprocess
from dataclasses import dataclass
from enum import StrEnum

from adherence import (
    EvaluationResult,
    RuleCheckResult,
    StandardsInventory,
    Track,
    discover_standards,
)

_WORD_NUMBERS: dict[str, int] = {
    "one": 1,
    "two": 2,
    "three": 3,
    "four": 4,
    "five": 5,
    "six": 6,
    "seven": 7,
    "eight": 8,
    "nine": 9,
    "ten": 10,
}


@dataclass(frozen=True, slots=True)
class LiveRulesConfig:
    """Parameters parsed dynamically from the live `rules/` Markdown files."""

    has_commit_rule: bool
    commit_subject_limit: int
    forbid_type_prefixes: tuple[str, ...]
    forbid_co_authored_by: bool
    has_short_comments_rule: bool
    max_comment_lines: int
    has_line_length_rule: bool
    max_columns: int
    has_british_english_rule: bool
    american_words: tuple[str, ...]
    has_current_state_rule: bool
    history_phrases: tuple[str, ...]
    has_parse_boundary_rule: bool
    has_proptest_rule: bool
    has_scenarios_first_rule: bool


class Severity(StrEnum):
    """How strongly a finding or a planted defect blocks a change, in the change-review skill's two kinds."""

    MUST_FIX = "must-fix"
    SHOULD_FIX = "should-fix"


class FindingKind(StrEnum):
    """What sort of fault a finding or a planted defect is."""

    CORRECTNESS = "correctness"
    CONCURRENCY = "concurrency"
    SECURITY = "security"
    PERFORMANCE = "performance"
    TESTS = "tests"
    DOCUMENTATION = "documentation"
    DESIGN = "design"
    IDIOM = "idiom"


MAX_FINDING_SPAN = 30
"""A finding spanning more lines than this points at no defect, so a finding cannot claim a whole file."""

LOCATION_SLACK = 3
"""A finding is on a defect when it touches the defect's lines widened by this many lines on each side."""


@dataclass(frozen=True, slots=True)
class LineRange:
    """Inclusive one-based line numbers in a file at the reviewed revision."""

    first: int
    last: int

    def __post_init__(self) -> None:
        if self.first < 1 or self.last < self.first:
            raise ValueError(f"line range [{self.first}, {self.last}] must satisfy 1 <= first <= last")

    @property
    def span(self) -> int:
        """The number of lines the range covers."""
        return self.last - self.first + 1

    def touches(self, other: LineRange, slack: int) -> bool:
        """Whether this range shares a line with `other` widened by `slack` lines on each side."""
        return self.first <= other.last + slack and self.last >= other.first - slack


@dataclass(frozen=True, slots=True)
class Finding:
    """One finding a reviewer recorded in `findings.jsonl`."""

    file: str
    lines: LineRange
    kind: FindingKind
    severity: Severity
    text: str


@dataclass(frozen=True, slots=True)
class Site:
    """Lines in one file at the reviewed revision."""

    file: str
    lines: LineRange


@dataclass(frozen=True, slots=True)
class PlantedDefect:
    """One defect planted in a review task's change.

    `sites` and `kinds` list every place and kind a correct finding may give, since one fault can be located at its
    cause or where it does harm and described from more than one angle. `rule` names the standards rule the defect
    falls under, as `file#anchor`, exactly when the standards name it.
    """

    item_id: str
    sites: tuple[Site, ...]
    kinds: tuple[FindingKind, ...]
    severity: Severity
    rule: str | None

    def __post_init__(self) -> None:
        if not self.sites or not self.kinds:
            raise ValueError(f"defect {self.item_id!r} needs at least one site and one kind")

    @property
    def named_by_standards(self) -> bool:
        """Whether a standards rule names this defect."""
        return self.rule is not None


@dataclass(frozen=True, slots=True)
class Bait:
    """Sound code a reviewer may wrongly block on; a must-fix finding on its lines is a false positive."""

    item_id: str
    file: str
    lines: LineRange


@dataclass(frozen=True, slots=True)
class ReviewSpec:
    """Parsed ground truth for a review task."""

    defects: tuple[PlantedDefect, ...]
    bait: tuple[Bait, ...]
    expected_verdict: str


@dataclass(frozen=True, slots=True)
class ReviewScore:
    """Which planted defects a review found, which bait it blocked on, and its false positives."""

    found: dict[str, bool]
    bait_hits: dict[str, bool]
    false_positives: int
    spec: ReviewSpec

    def recall(self, named: bool | None = None) -> float | None:
        """Defects found over defects planted, in one group or overall; `None` when the group has none."""
        group = self._group(named)
        if not group:
            return None
        return sum(1 for d in group if self.found[d.item_id]) / len(group)

    def precision(self, named: bool | None = None) -> float | None:
        """Defects found over defects planted plus must-fix findings on no defect; `None` for an empty group.

        False positives belong to no group, so a group's precision counts all of them.
        """
        group = self._group(named)
        if not group:
            return None
        return sum(1 for d in group if self.found[d.item_id]) / (len(group) + self.false_positives)

    def _group(self, named: bool | None) -> list[PlantedDefect]:
        return [d for d in self.spec.defects if named is None or d.named_by_standards == named]


def load_live_rules_config(inventory: StandardsInventory) -> LiveRulesConfig:
    """Derive active rule thresholds and forbidden terms from the live `StandardsInventory`."""
    commit_rule = inventory.find_rule("Write scoped, imperative commit messages")
    subject_limit = 72
    type_prefixes: list[str] = []
    if commit_rule is not None:
        num_match = re.search(r"at most (?:about )?(\d+) characters", commit_rule.text)
        if num_match is not None:
            subject_limit = int(num_match.group(1))
        for tok in re.findall(r"`([a-z]+):`", commit_rule.text):
            if tok not in type_prefixes:
                type_prefixes.append(tok)

    trailer_rule = inventory.find_rule("Omit author and tool attribution trailers")
    forbid_trailers = trailer_rule is not None

    comment_rule = inventory.find_rule("Comments inside code stay short")
    max_comment_lines = 6
    if comment_rule is not None:
        word_match = re.search(r"at most (\w+) lines", comment_rule.text.lower())
        if word_match is not None:
            token = word_match.group(1)
            if token.isdigit():
                max_comment_lines = int(token)
            elif token in _WORD_NUMBERS:
                max_comment_lines = _WORD_NUMBERS[token]

    col_rule = inventory.find_rule("Keep lines within")
    max_columns = 120
    if col_rule is not None:
        col_match = re.search(r"(\d+)\s+columns", f"{col_rule.title} {col_rule.text}")
        if col_match is not None:
            max_columns = int(col_match.group(1))

    brit_rule = inventory.find_rule("Use British English")
    american_words: list[str] = []
    if brit_rule is not None:
        lower_text = brit_rule.text.lower()
        uk_to_us = {
            "analyse": ("analyze", "analyzes"),
            "colour": ("color", "colors"),
            "organisation": ("organization", "organizations"),
            "centre": ("center", "centers"),
        }
        for uk_word, us_forms in uk_to_us.items():
            if uk_word in lower_text:
                american_words.extend(us_forms)

    hist_rule = inventory.find_rule("Documentation describes current state only")
    history_phrases = (
        (*(p for p in re.findall(r'"([^"]+)"', hist_rule.text.lower()) if p != "removed"), "removed in")
        if hist_rule is not None
        else ()
    )

    return LiveRulesConfig(
        has_commit_rule=commit_rule is not None,
        commit_subject_limit=subject_limit,
        forbid_type_prefixes=tuple(type_prefixes),
        forbid_co_authored_by=forbid_trailers,
        has_short_comments_rule=comment_rule is not None,
        max_comment_lines=max_comment_lines,
        has_line_length_rule=col_rule is not None,
        max_columns=max_columns,
        has_british_english_rule=brit_rule is not None,
        american_words=tuple(american_words),
        has_current_state_rule=hist_rule is not None,
        history_phrases=history_phrases,
        has_parse_boundary_rule=inventory.find_rule("Parse at the boundary") is not None,
        has_proptest_rule=inventory.find_rule("Properties") is not None,
        has_scenarios_first_rule=inventory.find_rule("Scenarios first") is not None,
    )


def _run_cmd(args: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, check=False, capture_output=True, text=True)


def check_commit_hygiene(repo_dir: Path, base_rev: str, cfg: LiveRulesConfig) -> tuple[bool, str]:
    """Check scoped imperative subjects, body presence, and absence of trailers since `base_rev`."""
    proc = _run_cmd(["git", "log", "--format=%H%x1f%s%x1f%b%x1e", f"{base_rev}..HEAD"], repo_dir)
    if proc.returncode != 0:
        return False, "git log failed"
    raw_commits = [entry for entry in proc.stdout.split("\x1e") if entry.strip()]
    if not raw_commits:
        return False, "no commits created on branch since base"

    subject_re = re.compile(r"^[a-z0-9_-]+(?:: [a-z0-9_-]+)*: [a-z][^\n]+$")
    for entry in raw_commits:
        parts = entry.lstrip("\n").split("\x1f")
        if len(parts) != 3:
            return False, "malformed git log record"
        sha = parts[0].strip()[:8]
        subject = parts[1].strip()
        body = parts[2].strip()
        if cfg.forbid_co_authored_by and ("co-authored-by:" in entry.lower() or "generated by" in entry.lower()):
            return False, f"{sha}: contains attribution trailer"
        for prefix in cfg.forbid_type_prefixes:
            if subject.startswith(f"{prefix}:") or subject.startswith(f"{prefix}("):
                return False, f"{sha}: uses forbidden prefix {prefix!r} in {subject!r}"
        if len(subject) > cfg.commit_subject_limit or not subject_re.match(subject):
            return (
                False,
                f"{sha}: subject {subject!r} does not match 'scope: verb ...' <={cfg.commit_subject_limit} chars",
            )
        if not body:
            return False, f"{sha}: non-trivial commit has no commit body"
    return True, f"{len(raw_commits)} compliant commit(s)"


def check_rust_scans_clean(repo_dir: Path, manifest_rel: str, core_crates: list[str]) -> tuple[bool, str]:
    """Run `rust-scans` if installed on PATH and check that H1/H2/H3/H4 report zero findings."""
    which_proc = _run_cmd(["sh", "-c", "command -v rust-scans"], repo_dir)
    if which_proc.returncode != 0:
        return True, "rust-scans not on PATH; skipped"
    workspace_dir = (repo_dir / manifest_rel).parent
    cmd = ["rust-scans", "--workspace", str(workspace_dir)]
    for core in core_crates:
        cmd.extend(["--core", core])
    proc = _run_cmd(cmd, repo_dir)
    if proc.returncode != 0:
        return False, f"rust-scans failed: {proc.stderr.strip()}"
    for code in ("H1", "H2", "H3", "H4"):
        if f"{code}: 0" not in proc.stdout:
            return False, f"rust-scans reported findings:\n{proc.stdout.strip()}"
    return True, "rust-scans clean (H1=0, H2=0, H3=0, H4=0)"


def check_diff_rules(repo_dir: Path, base_rev: str, cfg: LiveRulesConfig) -> list[RuleCheckResult]:
    """Inspect added lines in `git diff base_rev..HEAD` against live standards rules."""
    diff_proc = _run_cmd(["git", "diff", "-U0", f"{base_rev}..HEAD"], repo_dir)
    added_rs_lines: list[str] = []
    added_doc_or_comment_lines: list[str] = []
    added_md_lines: list[str] = []
    current_file = ""
    consecutive_fn_comments = 0
    max_consecutive_fn_comments = 0

    for line in diff_proc.stdout.splitlines():
        if line.startswith("+++ b/"):
            current_file = line[6:].strip()
            consecutive_fn_comments = 0
            continue
        if not line.startswith("+") or line.startswith("+++"):
            continue
        content = line[1:]
        if current_file.endswith(".rs"):
            added_rs_lines.append(content)
            stripped = content.strip()
            if stripped.startswith("//"):
                added_doc_or_comment_lines.append(stripped)
                if not stripped.startswith("///") and not stripped.startswith("//!"):
                    consecutive_fn_comments += 1
                    max_consecutive_fn_comments = max(max_consecutive_fn_comments, consecutive_fn_comments)
                else:
                    consecutive_fn_comments = 0
            else:
                consecutive_fn_comments = 0
        elif current_file.endswith(".md"):
            added_md_lines.append(content)

    joined_rs = "\n".join(added_rs_lines)
    joined_comments = "\n".join(added_doc_or_comment_lines + added_md_lines).lower()
    results: list[RuleCheckResult] = []

    if cfg.has_parse_boundary_rule:
        has_structured_type = bool(re.search(r"\b(pub(?:\(crate\))?\s+)?(enum|struct)\s+\w+", joined_rs))
        has_string_err = "Result<(), String>" in joined_rs or "Err(format!(" in joined_rs
        results.append(
            RuleCheckResult(
                rule_id="general#parse-at-the-boundary",
                applicable=True,
                passed=has_structured_type and not has_string_err,
                detail="domain types introduced without stringly-typed Result<(), String>",
            )
        )

    if cfg.has_short_comments_rule:
        results.append(
            RuleCheckResult(
                rule_id="general#comments-inside-code-stay-short",
                applicable=True,
                passed=max_consecutive_fn_comments <= cfg.max_comment_lines,
                detail=f"max inline comment block: {max_consecutive_fn_comments} <= {cfg.max_comment_lines}",
            )
        )

    if cfg.has_current_state_rule:
        no_history_ok = not any(
            re.search(rf"\b{re.escape(phrase)}\b", joined_comments) for phrase in cfg.history_phrases
        )
        results.append(
            RuleCheckResult(
                rule_id="general#documentation-describes-current-state-only",
                applicable=True,
                passed=no_history_ok,
                detail="no historical narration in comments or docs",
            )
        )

    if cfg.has_british_english_rule:
        british_ok = not any(re.search(rf"\b{re.escape(word)}\b", joined_comments) for word in cfg.american_words)
        results.append(
            RuleCheckResult(
                rule_id="general#use-british-english",
                applicable=True,
                passed=british_ok,
                detail="British spelling in comments and documentation",
            )
        )

    if cfg.has_line_length_rule:
        line_len_ok = all(len(line.replace("\t", "    ")) <= cfg.max_columns for line in added_rs_lines)
        results.append(
            RuleCheckResult(
                rule_id="general#keep-lines-within-120-columns",
                applicable=True,
                passed=line_len_ok,
                detail=f"all added Rust lines within {cfg.max_columns} columns",
            )
        )

    if cfg.has_proptest_rule:
        proptest_ok = True
        if "proptest!" in joined_rs:
            proptest_ok = "RngSeed::Fixed" in joined_rs and "failure_persistence: None" in joined_rs
        results.append(
            RuleCheckResult(
                rule_id="rust/tests#properties",
                applicable=True,
                passed=proptest_ok,
                detail="proptest uses fixed seed and failure_persistence: None (or no proptest added)",
            )
        )

    return results


def grade_write_trial(
    repo_dir: Path,
    base_rev: str,
    manifest_rel: str,
    functional_score: float,
    core_crates: list[str],
    extra_rules_path: Path | None,
    inventory: StandardsInventory,
) -> EvaluationResult:
    """Compute the adherence breakdown and combined reward records for a writing task."""
    cfg = load_live_rules_config(inventory)
    results: list[RuleCheckResult] = []

    if cfg.has_commit_rule:
        commit_ok, commit_detail = check_commit_hygiene(repo_dir, base_rev, cfg)
        results.append(
            RuleCheckResult(
                rule_id="general#write-scoped-imperative-commit-messages",
                applicable=True,
                passed=commit_ok,
                detail=commit_detail,
            )
        )

    scans_ok, scans_detail = check_rust_scans_clean(repo_dir, manifest_rel, core_crates)
    results.append(
        RuleCheckResult(
            rule_id="rust/design#quantities-and-error-codes",
            applicable=True,
            passed=scans_ok,
            detail=scans_detail,
        )
    )

    results.extend(check_diff_rules(repo_dir, base_rev, cfg))

    if (repo_dir / "docs" / "concord").is_dir():
        dr_proc = _run_cmd(["concord", "check"], repo_dir)
        dr_ok = dr_proc.returncode == 0
        results.append(
            RuleCheckResult(
                rule_id="skills/concord#check",
                applicable=True,
                passed=dr_ok,
                detail=dr_proc.stdout.strip() or dr_proc.stderr.strip(),
            )
        )

    if extra_rules_path is not None and extra_rules_path.is_file():
        extra_obj: object = json.loads(extra_rules_path.read_text(encoding="utf-8"))
        if isinstance(extra_obj, dict):
            for k, v in extra_obj.items():
                if isinstance(v, dict):
                    results.append(
                        RuleCheckResult(
                            rule_id=str(k),
                            applicable=bool(v.get("applicable", True)),
                            passed=bool(v.get("passed", False)),
                            detail=str(v.get("detail", "")),
                        )
                    )

    applicable_results = [r.passed for r in results if r.applicable]
    adherence_score = (
        sum(1.0 for passed in applicable_results if passed) / len(applicable_results) if applicable_results else 0.0
    )
    combined_reward = round(functional_score, 4)

    return EvaluationResult(
        track=Track.WRITE,
        reward=combined_reward,
        functional=round(functional_score, 4),
        adherence=round(adherence_score, 4),
        rules=tuple(results),
    )


def _required(item: dict[str, object], key: str, where: str) -> object:
    if key not in item:
        raise ValueError(f"{where}: missing {key!r}")
    return item[key]


def _line_range(raw: object, where: str) -> LineRange:
    if (
        not isinstance(raw, list)
        or len(raw) != 2
        or not all(isinstance(n, int) and not isinstance(n, bool) for n in raw)
    ):
        raise ValueError(f"{where}: lines: expected [first, last] integers, got {raw!r}")
    try:
        return LineRange(first=raw[0], last=raw[1])
    except ValueError as exc:
        raise ValueError(f"{where}: lines: {exc}") from exc


def _str_field(item: dict[str, object], key: str, where: str) -> str:
    value = _required(item, key, where)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{where}: {key}: expected a non-empty string, got {value!r}")
    return value


def _enum_field[E: StrEnum](item: dict[str, object], key: str, enum: type[E], where: str) -> E:
    value = _required(item, key, where)
    try:
        return enum(value)
    except ValueError as exc:
        allowed = ", ".join(member.value for member in enum)
        raise ValueError(f"{where}: {key}: expected one of {allowed}, got {value!r}") from exc


def _sites(item: dict[str, object], where: str) -> tuple[Site, ...]:
    raw = _required(item, "sites", where)
    if not isinstance(raw, list) or not raw or not all(isinstance(site, dict) for site in raw):
        raise ValueError(f"{where}: sites: expected a non-empty list of objects")
    return tuple(
        Site(
            file=_str_field(site, "file", f"{where}: sites[{index}]"),
            lines=_line_range(_required(site, "lines", f"{where}: sites[{index}]"), f"{where}: sites[{index}]"),
        )
        for index, site in enumerate(raw)
    )


def _kinds(item: dict[str, object], where: str) -> tuple[FindingKind, ...]:
    raw = _required(item, "kinds", where)
    if not isinstance(raw, list) or not raw:
        raise ValueError(f"{where}: kinds: expected a non-empty list")
    kinds: list[FindingKind] = []
    for value in raw:
        try:
            kinds.append(FindingKind(value))
        except ValueError as exc:
            allowed = ", ".join(member.value for member in FindingKind)
            raise ValueError(f"{where}: kinds: expected each of {allowed}, got {value!r}") from exc
    return tuple(kinds)


def _objects(raw_spec: dict[str, object], key: str, where: str) -> list[dict[str, object]]:
    raw = _required(raw_spec, key, where)
    if not isinstance(raw, list) or not all(isinstance(item, dict) for item in raw):
        raise ValueError(f"{where}: {key}: expected a list of objects")
    return raw


def parse_review_spec(spec_path: Path) -> ReviewSpec:
    """Parse a task's `review_spec.json`, refusing any missing or malformed field."""
    raw_spec: object = json.loads(spec_path.read_text(encoding="utf-8"))
    if not isinstance(raw_spec, dict):
        raise ValueError(f"{spec_path}: review spec must be a JSON object")

    defects: list[PlantedDefect] = []
    for index, item in enumerate(_objects(raw_spec, "defects", str(spec_path))):
        where = f"{spec_path}: defects[{index}]"
        named = _required(item, "named_by_standards", where)
        if not isinstance(named, bool):
            raise ValueError(f"{where}: named_by_standards: expected true or false, got {named!r}")
        rule = _str_field(item, "rule", where) if named else None
        if not named and "rule" in item:
            raise ValueError(f"{where}: rule: a defect the standards do not name has no rule")
        defects.append(
            PlantedDefect(
                item_id=_str_field(item, "id", where),
                sites=_sites(item, where),
                kinds=_kinds(item, where),
                severity=_enum_field(item, "severity", Severity, where),
                rule=rule,
            )
        )
    if not defects:
        raise ValueError(f"{spec_path}: defects: a review task plants at least one defect")

    bait: list[Bait] = []
    for index, item in enumerate(_objects(raw_spec, "bait", str(spec_path))):
        where = f"{spec_path}: bait[{index}]"
        bait.append(
            Bait(
                item_id=_str_field(item, "id", where),
                file=_str_field(item, "file", where),
                lines=_line_range(_required(item, "lines", where), where),
            )
        )

    ids = [d.item_id for d in defects] + [b.item_id for b in bait]
    if len(set(ids)) != len(ids):
        raise ValueError(f"{spec_path}: defect and bait ids must be unique")

    return ReviewSpec(
        defects=tuple(defects),
        bait=tuple(bait),
        expected_verdict=_str_field(raw_spec, "expected_verdict", str(spec_path)).lower(),
    )


def _review_path(raw: str) -> str:
    """Spell a reviewer's file path as the spec does, relative to `/app`."""
    path = raw.strip()
    for prefix in ("/app/", "./"):
        path = path.removeprefix(prefix)
    return path


def parse_findings(text: str) -> tuple[Finding, ...]:
    """Parse `findings.jsonl`: one JSON object per non-blank line; raise `ValueError` on any unreadable line."""
    findings: list[Finding] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        where = f"findings.jsonl:{number}"
        try:
            item: object = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{where}: not JSON: {exc.msg}") from exc
        if not isinstance(item, dict):
            raise ValueError(f"{where}: expected a JSON object")
        findings.append(
            Finding(
                file=_review_path(_str_field(item, "file", where)),
                lines=_line_range(_required(item, "lines", where), where),
                kind=_enum_field(item, "kind", FindingKind, where),
                severity=_enum_field(item, "severity", Severity, where),
                text=_str_field(item, "finding", where),
            )
        )
    return tuple(findings)


def is_on(finding: Finding, file: str, lines: LineRange) -> bool:
    """Whether `finding` names `file`, spans at most `MAX_FINDING_SPAN` lines, and touches `lines` widened."""
    return (
        finding.file == file and finding.lines.span <= MAX_FINDING_SPAN and finding.lines.touches(lines, LOCATION_SLACK)
    )


def finds(finding: Finding, defect: PlantedDefect) -> bool:
    """Whether `finding` is on one of `defect`'s sites with one of its kinds, at its severity or above."""
    severe_enough = defect.severity == Severity.SHOULD_FIX or finding.severity == Severity.MUST_FIX
    on_a_site = any(is_on(finding, site.file, site.lines) for site in defect.sites)
    return severe_enough and finding.kind in defect.kinds and on_a_site


def _closeness(finding: LineRange, defect: LineRange) -> tuple[int, int]:
    """Sort key: the gap between the two ranges, then the negated number of lines they share."""
    gap = max(defect.first - finding.last, finding.first - defect.last, 0)
    shared = min(finding.last, defect.last) - max(finding.first, defect.first) + 1
    return gap, -max(shared, 0)


def credited_defect(finding: Finding, defects: tuple[PlantedDefect, ...]) -> PlantedDefect | None:
    """The one defect `finding` earns credit for: of those it finds, the closest; `None` when it finds none.

    One finding names one fault, so it never counts for two defects whose widened lines meet.
    """
    candidates = [d for d in defects if finds(finding, d)]
    if not candidates:
        return None
    return min(
        candidates,
        key=lambda d: min(_closeness(finding.lines, site.lines) for site in d.sites if site.file == finding.file),
    )


def score_findings(findings: tuple[Finding, ...], spec: ReviewSpec) -> ReviewScore:
    """Match findings to planted defects by location, kind and severity, and count must-fix false positives."""
    credited = [(f, credited_defect(f, spec.defects)) for f in findings]
    found_ids = {d.item_id for _, d in credited if d is not None}
    found = {d.item_id: d.item_id in found_ids for d in spec.defects}
    stray = [f for f, d in credited if d is None and f.severity == Severity.MUST_FIX]
    bait_hits = {b.item_id: any(is_on(f, b.file, b.lines) for f in stray) for b in spec.bait}
    return ReviewScore(found=found, bait_hits=bait_hits, false_positives=len(stray), spec=spec)


def _unread_score(spec: ReviewSpec) -> ReviewScore:
    return ReviewScore(
        found={d.item_id: False for d in spec.defects},
        bait_hits={b.item_id: False for b in spec.bait},
        false_positives=0,
        spec=spec,
    )


def _review_adherence(
    review_text: str, findings: tuple[Finding, ...], spec: ReviewSpec, inventory: StandardsInventory
) -> tuple[RuleCheckResult, ...]:
    """Check the review against the standards' review skill: verdict, final marker, cited rules, spelling."""
    cfg = load_live_rules_config(inventory)
    has_review = bool(review_text.strip())
    review_lower = review_text.lower()
    all_text = "\n".join([review_lower, *(f.text.lower() for f in findings)])

    verdict_ok = has_review and spec.expected_verdict in review_lower
    done_ok = has_review and review_text.strip().endswith("DONE: final")
    explicit_citations = {f"{h.file}#{h.anchor}".lower() for h in inventory.headings} | {
        f"#{h.anchor}".lower() for h in inventory.headings if len(h.anchor) > 4
    }
    multi_word_titles = {h.title.lower() for h in inventory.headings if " " in h.title and len(h.title) >= 12}
    cites_specific_rule = any(cit in all_text for cit in explicit_citations) or any(
        title in all_text for title in multi_word_titles
    )
    british_ok = has_review and not any(re.search(rf"\b{re.escape(w)}\b", all_text) for w in cfg.american_words)

    return (
        RuleCheckResult(
            rule_id="skills/change-review#verdict",
            applicable=True,
            passed=verdict_ok,
            detail=f"expected verdict {spec.expected_verdict!r} present={verdict_ok}",
        ),
        RuleCheckResult(
            rule_id="skills/change-review#done-final-marker",
            applicable=True,
            passed=done_ok,
            detail=f"ends with DONE: final = {done_ok}",
        ),
        RuleCheckResult(
            rule_id="rules/review#cite-specific-rule",
            applicable=True,
            passed=cites_specific_rule,
            detail=f"cites live standards path/anchor or multi-word rule={cites_specific_rule}",
        ),
        RuleCheckResult(
            rule_id="general#use-british-english",
            applicable=cfg.has_british_english_rule,
            passed=british_ok,
            detail=f"British spelling in review and findings={british_ok}",
        ),
    )


def grade_review_trial(
    review_path: Path,
    findings_path: Path,
    spec_path: Path,
    inventory: StandardsInventory,
) -> tuple[EvaluationResult, ReviewScore]:
    """Score a review's `findings.jsonl` against the planted defects, and its `REVIEW.md` against the standards.

    The functional score is the review's precision: defects found over defects planted plus must-fix false
    positives, so it falls with every miss and every false block. A missing or unreadable findings file scores 0.
    """
    spec = parse_review_spec(spec_path)
    review_text = review_path.read_text(encoding="utf-8") if review_path.is_file() else ""

    findings: tuple[Finding, ...] = ()
    readable_detail = "readable"
    try:
        findings = parse_findings(findings_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        readable_detail = f"missing {findings_path}"
    except (UnicodeDecodeError, ValueError) as exc:
        readable_detail = str(exc)
    readable = readable_detail == "readable"
    score = score_findings(findings, spec) if readable else _unread_score(spec)

    results: list[RuleCheckResult] = [
        RuleCheckResult(
            rule_id="review/findings#readable",
            applicable=True,
            passed=readable,
            detail=readable_detail,
        )
    ]
    results.extend(
        RuleCheckResult(
            rule_id=f"review/defect#{d.item_id}",
            applicable=True,
            passed=score.found[d.item_id],
            detail=f"{d.severity} {'/'.join(d.kinds)} at "
            + ", ".join(f"{site.file}:{site.lines.first}-{site.lines.last}" for site in d.sites)
            + f"; named_by_standards={d.named_by_standards}",
        )
        for d in spec.defects
    )
    results.extend(
        RuleCheckResult(
            rule_id=f"review/bait#{b.item_id}",
            applicable=True,
            passed=not score.bait_hits[b.item_id],
            detail=f"no must-fix finding at {b.file}:{b.lines.first}-{b.lines.last}",
        )
        for b in spec.bait
    )
    adherence_checks = _review_adherence(review_text, findings, spec, inventory)
    results.extend(adherence_checks)

    functional_score = round(score.precision() or 0.0, 4) if readable else 0.0
    active_adh = [c.passed for c in adherence_checks if c.applicable]
    adherence_score = round(sum(1.0 for ok in active_adh if ok) / len(active_adh), 4) if active_adh else 0.0

    result = EvaluationResult(
        track=Track.REVIEW,
        reward=functional_score,
        functional=functional_score,
        adherence=adherence_score,
        rules=tuple(results),
    )
    return result, score


def review_record(score: ReviewScore) -> dict[str, object]:
    """The `review` block of `adherence.json`, which the report reads to split recall by group."""
    return {
        "planted": len(score.spec.defects),
        "found": sum(1 for found in score.found.values() if found),
        "false_positives": score.false_positives,
        "recall": score.recall(),
        "precision": score.precision(),
        "defects": {
            d.item_id: {
                "found": score.found[d.item_id],
                "named_by_standards": d.named_by_standards,
                "severity": str(d.severity),
            }
            for d in score.spec.defects
        },
        "bait_hits": dict(score.bait_hits),
    }


def write_verifier_outputs(verifier_dir: Path, result: EvaluationResult, review: ReviewScore | None = None) -> None:
    """Serialise `EvaluationResult`, and a review's score, into `reward.txt`, `reward.json`, and `adherence.json`."""
    verifier_dir.mkdir(parents=True, exist_ok=True)
    (verifier_dir / "reward.txt").write_text(f"{result.reward:.4f}\n", encoding="utf-8")
    (verifier_dir / "reward.json").write_text(
        json.dumps(
            {
                "reward": result.reward,
                "functional": result.functional,
                "adherence": result.adherence,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    rules_dict = {
        r.rule_id: {
            "applicable": r.applicable,
            "passed": r.passed,
            "detail": r.detail,
        }
        for r in result.rules
    }
    payload: dict[str, object] = {
        "track": str(result.track),
        "reward": result.reward,
        "functional": result.functional,
        "adherence": result.adherence,
        "rules": rules_dict,
    }
    if review is not None:
        payload["review"] = review_record(review)
    (verifier_dir / "adherence.json").write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    """Entry point for `/tests/grade_adherence.py`."""
    parser = argparse.ArgumentParser(description="Grade functional and standards adherence outcomes.")
    subparsers = parser.add_subparsers(dest="mode", required=True)

    write_p = subparsers.add_parser("write")
    write_p.add_argument("--repo", type=Path, required=True)
    write_p.add_argument("--base-rev", required=True)
    write_p.add_argument("--manifest", default="Cargo.toml")
    write_p.add_argument("--functional-score", type=float, required=True)
    write_p.add_argument("--core", action="append", default=[])
    write_p.add_argument("--extra-rules", type=Path)
    write_p.add_argument(
        "--standards-root",
        type=Path,
        default=Path(__file__).resolve().parent / "live_standards",
    )
    write_p.add_argument("--verifier-dir", type=Path, default=Path("/logs/verifier"))

    rev_p = subparsers.add_parser("review")
    rev_p.add_argument("--review-file", type=Path, required=True)
    rev_p.add_argument("--findings-file", type=Path, required=True)
    rev_p.add_argument("--spec-file", type=Path, required=True)
    rev_p.add_argument(
        "--standards-root",
        type=Path,
        default=Path(__file__).resolve().parent / "live_standards",
    )
    rev_p.add_argument("--verifier-dir", type=Path, default=Path("/logs/verifier"))

    args = parser.parse_args(argv)
    standards_root: Path = args.standards_root
    if not (standards_root / "rules").is_dir():
        for parent in Path(__file__).resolve().parents:
            if (parent / "rules").is_dir() and (parent / "skills").is_dir():
                standards_root = parent
                break

    inventory = discover_standards(standards_root)
    if args.mode == "write":
        result = grade_write_trial(
            repo_dir=args.repo,
            base_rev=str(args.base_rev),
            manifest_rel=str(args.manifest),
            functional_score=float(args.functional_score),
            core_crates=[str(c) for c in args.core],
            extra_rules_path=args.extra_rules,
            inventory=inventory,
        )
        write_verifier_outputs(args.verifier_dir, result)
        return 0

    if args.mode == "review":
        result, score = grade_review_trial(
            review_path=args.review_file,
            findings_path=args.findings_file,
            spec_path=args.spec_file,
            inventory=inventory,
        )
        write_verifier_outputs(args.verifier_dir, result, score)
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main())
