"""Evaluates a completed trial's standards adherence or review accuracy inside the task verifier.

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

_NEGATION_WORDS = (
    "no ",
    "not ",
    "neither ",
    "without ",
    "never ",
    "does not",
    "do not",
    "cannot ",
    "is sound",
    "remains sound",
    "is valid",
    "not a bug",
    "not a finding",
    "no finding",
)

_DEFECT_NEGATION_PHRASES = (
    "no dropped",
    "no detached",
    "not detached",
    "no stale",
    "not stale",
    "stale output does not occur",
    "no divide-by-zero",
    "no division by zero",
    "is sound",
    "remains sound",
    "is valid",
    "not a bug",
    "not a finding",
    "no finding",
)


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


@dataclass(frozen=True, slots=True)
class MustFixSpec:
    """One ground-truth must-fix defect in a review task."""

    item_id: str
    file: str
    keywords: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class ShouldFixSpec:
    """One ground-truth should-fix defect in a review task."""

    item_id: str
    keywords: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class BaitSpec:
    """One false-positive trap in a review task."""

    item_id: str
    trap_keywords: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class ReviewSpec:
    """Parsed ground-truth specification for a review task."""

    must_fix: tuple[MustFixSpec, ...]
    should_fix: tuple[ShouldFixSpec, ...]
    bait: tuple[BaitSpec, ...]
    expected_verdict: str


@dataclass(frozen=True, slots=True)
class ReviewSection:
    """One Markdown section parsed from an agent's `REVIEW.md`."""

    heading: str
    body: str
    kind: str  # "must-fix", "should-fix", or "prose"


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

    if (repo_dir / "docs" / "design-reads").is_dir():
        dr_proc = _run_cmd(["design-read", "check"], repo_dir)
        dr_ok = dr_proc.returncode == 0
        results.append(
            RuleCheckResult(
                rule_id="skills/design-read#check",
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


def parse_review_spec(spec_path: Path) -> ReviewSpec:
    """Parse a JSON review specification into typed dataclass records."""
    raw_spec: object = json.loads(spec_path.read_text(encoding="utf-8"))
    if not isinstance(raw_spec, dict):
        raise ValueError(f"{spec_path}: review spec must be a JSON object")

    must_fix: list[MustFixSpec] = []
    raw_must = raw_spec.get("must_fix", [])
    if isinstance(raw_must, list):
        for item in raw_must:
            if isinstance(item, dict):
                kws = tuple(str(k).lower() for k in item.get("keywords", []) if isinstance(k, str))
                must_fix.append(
                    MustFixSpec(
                        item_id=str(item.get("id", "must-fix")),
                        file=str(item.get("file", "")).lower(),
                        keywords=kws,
                    )
                )

    should_fix: list[ShouldFixSpec] = []
    raw_should = raw_spec.get("should_fix", [])
    if isinstance(raw_should, list):
        for item in raw_should:
            if isinstance(item, dict):
                kws = tuple(str(k).lower() for k in item.get("keywords", []) if isinstance(k, str))
                should_fix.append(ShouldFixSpec(item_id=str(item.get("id", "should-fix")), keywords=kws))

    bait: list[BaitSpec] = []
    raw_bait = raw_spec.get("bait", [])
    if isinstance(raw_bait, list):
        for item in raw_bait:
            if isinstance(item, dict):
                traps = tuple(str(k).lower() for k in item.get("must_fix_trap_keywords", []) if isinstance(k, str))
                bait.append(BaitSpec(item_id=str(item.get("id", "bait")), trap_keywords=traps))

    return ReviewSpec(
        must_fix=tuple(must_fix),
        should_fix=tuple(should_fix),
        bait=tuple(bait),
        expected_verdict=str(raw_spec.get("expected_verdict", "fix first")).lower(),
    )


def _split_review_sections(review_text: str) -> list[ReviewSection]:
    """Split a Markdown review into sections and classify each as `must-fix`, `should-fix`, or `prose`."""
    sections: list[ReviewSection] = []
    current_heading = ""
    current_lines: list[str] = []
    parent_kind = "prose"

    def classify_text(text: str) -> str | None:
        low = text.lower()
        if (
            "must-fix" in low
            or "must fix" in low
            or "blocker" in low
            or "critical" in low
            or "high" in low
            or "[p0]" in low
            or "[p1]" in low
            or "[p2]" in low
            or "required findings" in low
            or re.search(r"\bm\d+\b", low)
        ):
            return "must-fix"
        if (
            "should-fix" in low
            or "should fix" in low
            or "minor" in low
            or "medium" in low
            or "low" in low
            or "[p3]" in low
            or re.search(r"\bs\d+\b", low)
        ):
            return "should-fix"
        if "verdict" in low or "what i reviewed" in low or "scope" in low or "review questions" in low:
            return "prose"
        return None

    def flush(heading: str, lines: list[str], inherited: str) -> None:
        body = "\n".join(lines).strip()
        if not heading and not body:
            return
        head_kind = classify_text(heading)
        if head_kind is not None:
            kind = head_kind
        else:
            body_kind = classify_text(body[:160])
            kind = body_kind if body_kind in ("must-fix", "should-fix") else inherited
        sections.append(ReviewSection(heading=heading, body=body, kind=kind))

    for line in review_text.splitlines():
        h_match = re.match(r"^(#{1,4})\s+", line)
        if h_match is not None or re.match(r"^\d+\.\s+\*\*", line):
            flush(current_heading, current_lines, parent_kind)
            current_heading = line.strip()
            current_lines = []
            if h_match is not None and len(h_match.group(1)) <= 2:
                top_kind = classify_text(current_heading)
                parent_kind = top_kind if top_kind is not None else "prose"
        else:
            current_lines.append(line)
    flush(current_heading, current_lines, parent_kind)
    return sections


def _keyword_asserted_positively(section_text: str, keyword: str) -> bool:
    """Check that `keyword` appears in `section_text` without an explicit defect-denial phrase in the sentence."""
    idx = section_text.find(keyword)
    if idx < 0:
        return False
    line_start = section_text.rfind("\n", 0, idx) + 1
    line_end = section_text.find("\n", idx)
    if line_end < 0:
        line_end = len(section_text)
    line = section_text[line_start:line_end]
    return not any(neg in line for neg in _DEFECT_NEGATION_PHRASES)


def grade_review_trial(
    review_path: Path,
    spec_path: Path,
    inventory: StandardsInventory,
) -> EvaluationResult:
    """Score a written `REVIEW.md` against a ground-truth fault and bait specification."""
    review_text = review_path.read_text(encoding="utf-8") if review_path.is_file() else ""
    if not review_text.strip():
        return EvaluationResult(
            track=Track.REVIEW,
            reward=0.0,
            functional=0.0,
            adherence=0.0,
            rules=(
                RuleCheckResult(
                    rule_id="skills/change-review#complete",
                    applicable=True,
                    passed=False,
                    detail=f"missing or empty review output at {review_path}",
                ),
            ),
        )

    cfg = load_live_rules_config(inventory)
    spec = parse_review_spec(spec_path)
    sections = _split_review_sections(review_text)
    must_sections = [s for s in sections if s.kind == "must-fix"]
    should_sections = [s for s in sections if s.kind in ("should-fix", "must-fix")]

    review_lower = review_text.lower()
    results: list[RuleCheckResult] = []

    must_hits = 0
    for item in spec.must_fix:
        hit = False
        for sec in must_sections:
            sec_lower = f"{sec.heading}\n{sec.body}".lower()
            file_ok = not item.file or item.file in sec_lower
            kw_ok = any(_keyword_asserted_positively(sec_lower, kw) for kw in item.keywords)
            if file_ok and kw_ok:
                hit = True
                break
        if hit:
            must_hits += 1
        results.append(
            RuleCheckResult(
                rule_id=f"review/must-fix#{item.item_id}",
                applicable=True,
                passed=hit,
                detail=f"file={item.file!r}, asserted_in_finding={hit}",
            )
        )

    should_hits = 0
    for item in spec.should_fix:
        hit = False
        for sec in should_sections:
            sec_lower = f"{sec.heading}\n{sec.body}".lower()
            if any(_keyword_asserted_positively(sec_lower, kw) for kw in item.keywords):
                hit = True
                break
        if hit:
            should_hits += 1
        results.append(
            RuleCheckResult(
                rule_id=f"review/should-fix#{item.item_id}",
                applicable=True,
                passed=hit,
                detail=f"asserted_in_finding={hit}",
            )
        )

    bait_avoided = 0
    for item in spec.bait:
        fell_for_bait = any(trap in review_lower for trap in item.trap_keywords)
        if not fell_for_bait:
            for sec in sections:
                if sec.kind == "must-fix":
                    sec_lower = f"{sec.heading}\n{sec.body}".lower()
                    if any(trap in sec_lower for trap in item.trap_keywords):
                        fell_for_bait = True
                        break
        avoided = not fell_for_bait
        if avoided:
            bait_avoided += 1
        results.append(
            RuleCheckResult(
                rule_id=f"review/bait-resistance#{item.item_id}",
                applicable=True,
                passed=avoided,
                detail=f"avoided_false_positive={avoided}",
            )
        )

    verdict_ok = spec.expected_verdict in review_lower
    done_ok = review_text.strip().endswith("DONE: final")

    explicit_citations = {f"{h.file}#{h.anchor}".lower() for h in inventory.headings} | {
        f"#{h.anchor}".lower() for h in inventory.headings if len(h.anchor) > 4
    }
    multi_word_titles = {h.title.lower() for h in inventory.headings if " " in h.title and len(h.title) >= 12}
    cites_specific_rule = any(cit in review_lower for cit in explicit_citations) or any(
        title in review_lower for title in multi_word_titles
    )
    has_must_should_labels = ("must-fix" in review_lower or "must fix" in review_lower) and (
        "should-fix" in review_lower or "should fix" in review_lower
    )
    british_ok = not any(re.search(rf"\b{re.escape(w)}\b", review_lower) for w in cfg.american_words)

    adherence_checks = (
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
            rule_id="skills/change-review#must-and-should-fix-kinds",
            applicable=True,
            passed=has_must_should_labels,
            detail=f"uses must-fix and should-fix sections={has_must_should_labels}",
        ),
        RuleCheckResult(
            rule_id="general#use-british-english",
            applicable=cfg.has_british_english_rule,
            passed=british_ok,
            detail=f"British spelling in review={british_ok}",
        ),
    )
    results.extend(adherence_checks)

    must_score = (must_hits / len(spec.must_fix)) if spec.must_fix else 1.0
    should_score = (should_hits / len(spec.should_fix)) if spec.should_fix else 1.0
    bait_score = (bait_avoided / len(spec.bait)) if spec.bait else 1.0
    functional_score = round((4.0 * must_score + 1.0 * should_score + 1.0 * bait_score) / 6.0, 4)

    active_adh = [c.passed for c in adherence_checks if c.applicable]
    adherence_score = round(sum(1.0 for ok in active_adh if ok) / len(active_adh), 4) if active_adh else 0.0
    combined_reward = round(functional_score, 4)

    return EvaluationResult(
        track=Track.REVIEW,
        reward=combined_reward,
        functional=functional_score,
        adherence=adherence_score,
        rules=tuple(results),
    )


def write_verifier_outputs(verifier_dir: Path, result: EvaluationResult) -> None:
    """Serialise `EvaluationResult` into `reward.txt`, `reward.json`, and `adherence.json`."""
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
    payload = {
        "track": str(result.track),
        "reward": result.reward,
        "functional": result.functional,
        "adherence": result.adherence,
        "rules": rules_dict,
    }
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
        result = grade_review_trial(
            review_path=args.review_file,
            spec_path=args.spec_file,
            inventory=inventory,
        )
        write_verifier_outputs(args.verifier_dir, result)
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main())
