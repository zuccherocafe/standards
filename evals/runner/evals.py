"""Command-line entry point for running, checking, and reporting Harbor evaluations.

Run through `nix develop -c evals <command>`. Reads the live standards from the repository root, stages isolated
Harbor datasets per experimental condition under `.cache/evals/`, and summarises trial rewards, costs, and rule
adherence. Never writes outside the repository's `.cache/evals/` folder.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import argparse
import json
import os
import shutil
import subprocess
import tomllib
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Protocol

from adherence import (
    Condition,
    StandardsInventory,
    Track,
    discover_standards,
    format_condition_label,
    parse_pi_session_jsonl,
)
from grade_adherence import parse_review_spec

DEFAULT_MODEL = "openai-codex/gpt-6.1-sol"
DEFAULT_KEY_FILE = Path.home() / ".config" / "evals" / "openrouter-key"
DEFAULT_PI_AUTH_FILE = Path.home() / ".pi" / "agent" / "auth.json"
PI_VERSION = "1.4.0"
ALL_CONDITIONS = (
    Condition.NONE,
    Condition.AGENT_SKILLS,
    Condition.SUPERPOWERS,
    Condition.STANDARDS,
)


class _ExecResult(Protocol):
    stdout: str | None
    return_code: int


class _Environment(Protocol):
    async def exec(self, command: str) -> _ExecResult: ...
    async def upload_file(self, source_path: Path | str, target_path: str) -> None: ...


def normalize_model_id(model: str) -> str:
    """Route GPT models through `openai-codex/<model>` and non-GPT models through `openrouter/` when unprefixed."""
    cleaned = model.strip()
    if cleaned.startswith("openrouter/openai/gpt-"):
        return "openai-codex/" + cleaned.removeprefix("openrouter/openai/")
    if cleaned.startswith("openai/gpt-"):
        return "openai-codex/" + cleaned.removeprefix("openai/")
    if cleaned.startswith("gpt-"):
        return f"openai-codex/{cleaned}"
    if "/" not in cleaned:
        return f"openrouter/{cleaned}"
    return cleaned


def _make_preinstalled_pi_class() -> type:
    """Subclass Harbor's built-in `Pi` agent so it uses the task image's pre-installed `pi` without network."""
    from harbor.agents.installed.pi import Pi  # pyrefly: ignore[missing-import] — loaded inside Harbor's venv

    class PreinstalledPi(Pi):  # type: ignore[misc]
        async def install(self, environment: _Environment) -> None:
            install_version = getattr(self, "_version", None) or PI_VERSION
            found = await environment.exec(command='version=$(pi --version) && echo "image-pi-version=$version"')
            stdout = found.stdout or ""
            _, marker, found_version = stdout.rpartition("image-pi-version=")
            found_version = found_version.strip()
            if found.return_code != 0 or not marker or found_version != install_version:
                raise ValueError(f"Task image must provide pi {install_version} on PATH (got {found_version!r})")

            auth_file_env = os.environ.get("EVALS_PI_AUTH_FILE", "")
            auth_path = Path(auth_file_env) if auth_file_env else DEFAULT_PI_AUTH_FILE
            if auth_path.is_file():
                await environment.exec(command="mkdir -p /root/.pi/agent && chmod 700 /root/.pi/agent")
                await environment.upload_file(auth_path, "/root/.pi/agent/auth.json")
                await environment.exec(command="chmod 600 /root/.pi/agent/auth.json")

    return PreinstalledPi


def __getattr__(name: str) -> object:
    if name == "PreinstalledPi":
        return _make_preinstalled_pi_class()
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


@dataclass(frozen=True, slots=True)
class TaskSpec:
    """One Harbor evaluation task discovered under `evals/tasks/<name>/`."""

    name: str
    track: Track
    path: Path
    description: str

    def __post_init__(self) -> None:
        if not self.name:
            raise ValueError("a task specification requires a non-empty name")


@dataclass(frozen=True, slots=True)
class DefectOutcome:
    """Whether one review trial found one planted defect."""

    task: str
    defect_id: str
    named_by_standards: bool
    found: bool


@dataclass(frozen=True, slots=True)
class ReviewOutcome:
    """A review trial's planted defects, which it found, and its must-fix findings on no defect."""

    defects: tuple[DefectOutcome, ...]
    false_positives: int


@dataclass(frozen=True, slots=True)
class TrialSummary:
    """Aggregated outcome and telemetry for one completed Harbor trial."""

    task: str
    track: Track
    condition: Condition
    reward: float
    functional_score: float
    adherence_score: float
    turns: int
    cost_usd: float
    standards_files_read: tuple[str, ...]
    skills_read: tuple[str, ...]
    rule_outcomes: dict[str, bool]
    review: ReviewOutcome | None = None


def discover_tasks(
    tasks_root: Path, track_filter: Track | None = None, name_filter: str | None = None
) -> list[TaskSpec]:
    """Read every task under `tasks_root` and validate its `task.toml`, `instruction.md`, and `tests/test.sh`."""
    if not tasks_root.is_dir():
        raise ValueError(f"{tasks_root}: tasks directory does not exist")

    specs: list[TaskSpec] = []
    for entry in sorted(tasks_root.iterdir()):
        if not entry.is_dir() or entry.name.startswith("."):
            continue
        config_path = entry / "task.toml"
        instruction_path = entry / "instruction.md"
        test_path = entry / "tests" / "test.sh"
        if not config_path.is_file():
            raise ValueError(f"{config_path}: missing task.toml")
        if not instruction_path.is_file():
            raise ValueError(f"{instruction_path}: missing instruction.md")
        if not test_path.is_file():
            raise ValueError(f"{test_path}: missing tests/test.sh")

        raw_toml: object = tomllib.loads(config_path.read_text(encoding="utf-8"))
        if not isinstance(raw_toml, dict):
            raise ValueError(f"{config_path}: task.toml must be a TOML table")
        metadata = raw_toml.get("metadata")
        if not isinstance(metadata, dict):
            raise ValueError(f"{config_path}: metadata: missing [metadata] table")
        raw_track = metadata.get("track")
        if raw_track == "write":
            track = Track.WRITE
        elif raw_track == "review":
            track = Track.REVIEW
        else:
            raise ValueError(f"{config_path}: metadata.track: expected 'write' or 'review', got {raw_track!r}")

        task_table = raw_toml.get("task")
        description = ""
        if isinstance(task_table, dict):
            desc = task_table.get("description")
            if isinstance(desc, str):
                description = desc

        if track_filter is not None and track != track_filter:
            continue
        if name_filter is not None and entry.name != name_filter:
            continue
        specs.append(TaskSpec(name=entry.name, track=track, path=entry, description=description))

    return specs


def stage_condition_dataset(
    repo_root: Path,
    tasks: list[TaskSpec],
    condition: Condition,
    stage_root: Path,
    agent_skills_dir: Path | None,
    superpowers_dir: Path | None = None,
) -> Path:
    """Stage a condition-specific Harbor dataset directory under `stage_root`."""
    dataset_dir = stage_root / format_condition_label(condition)
    if dataset_dir.exists():
        shutil.rmtree(dataset_dir)
    dataset_dir.mkdir(parents=True, exist_ok=True)

    for spec in tasks:
        dest = dataset_dir / spec.name
        shutil.copytree(spec.path, dest, symlinks=True)
        _inject_condition_overlay(repo_root, spec, dest, condition, agent_skills_dir, superpowers_dir)

    return dataset_dir


def _standards_agents_appendix(spec: TaskSpec, dest_task_dir: Path) -> str:
    """Build the `/standards` operating instructions appended to `/app/AGENTS.md` under `condition = standards`."""
    if spec.track == Track.REVIEW:
        return (
            "\n## Standards\n\n"
            "Follow `/standards/skills/change-review/SKILL.md` and `/standards/rules/review.md`. "
            "Cite the most specific applicable rule in `/standards/rules/` for each finding, including its file and "
            "heading. The container already provides the development shell's tools on `PATH`, so run any needed "
            "command directly without `nix develop -c`. Finish `/app/REVIEW.md` with the final line `DONE: final` "
            "once the review is complete.\n"
        )

    read_dir = dest_task_dir / "environment" / "repo" / "docs" / "concord"
    read_files: list[str] = (
        sorted(p.name for p in read_dir.glob("*.md") if p.name != "README.md") if read_dir.is_dir() else []
    )
    concord_ref = f"docs/concord/{read_files[0]}" if read_files else "none"
    return (
        "\n## Standards and brief\n\n"
        "- Size: normal\n"
        f"- Concord note: `{concord_ref}`\n"
        "- Decided: the product contract in the task prompt is complete; there are no open questions of intent "
        "requiring a `QUESTION:` stop.\n\n"
        "Read `/standards/rules/general.md`, `/standards/rules/languages/rust/`, and `/standards/rules/review.md`. "
        "Follow `/standards/skills/change-write/SKILL.md` and `/standards/skills/concord/SKILL.md`. `/app` is "
        "already an isolated scratch clone with the development shell's tools pre-installed on `PATH`, so run "
        "commands directly without `nix develop -c`, settle the open disagreement in the concord note, run "
        "`concord check`, and commit directly on the current branch in `/app`.\n"
    )


def _stage_container_tools(repo_root: Path, bin_dir: Path) -> None:
    """Copy container-compatible `concord` and `rust-scans` binaries into `bin_dir`."""
    bin_dir.mkdir(parents=True, exist_ok=True)
    cache_bin_dir = repo_root / ".cache" / "evals" / "container-tools"
    if (cache_bin_dir / "concord").is_file() and (cache_bin_dir / "rust-scans").is_file():
        for bin_name in ("concord", "rust-scans"):
            shutil.copy2(cache_bin_dir / bin_name, bin_dir / bin_name)


def _inject_condition_overlay(
    repo_root: Path,
    spec: TaskSpec,
    dest_task_dir: Path,
    condition: Condition,
    agent_skills_dir: Path | None,
    superpowers_dir: Path | None = None,
) -> None:
    """Populate the staged task environment for `condition`."""
    env_dir = dest_task_dir / "environment"
    overlay_dir = env_dir / "condition_overlay"
    if overlay_dir.exists():
        shutil.rmtree(overlay_dir)
    overlay_dir.mkdir(parents=True, exist_ok=True)

    (overlay_dir / "condition.txt").write_text(f"{format_condition_label(condition)}\n", encoding="utf-8")
    _stage_container_tools(repo_root, overlay_dir / "bin")

    match condition:
        case Condition.NONE:
            pass
        case Condition.AGENT_SKILLS:
            if agent_skills_dir is None or not agent_skills_dir.is_dir():
                raise ValueError("condition 'agent-skills' requires a valid EVALS_AGENT_SKILLS path")
            shutil.copytree(agent_skills_dir / "skills", overlay_dir / "skills", symlinks=True)
            if (agent_skills_dir / "references").is_dir():
                shutil.copytree(agent_skills_dir / "references", overlay_dir / "references", symlinks=True)
            if (agent_skills_dir / "docs").is_dir():
                shutil.copytree(agent_skills_dir / "docs", overlay_dir / "docs", symlinks=True)
            if (agent_skills_dir / "AGENTS.md").is_file():
                shutil.copy2(agent_skills_dir / "AGENTS.md", overlay_dir / "AGENTS.md")
        case Condition.SUPERPOWERS:
            resolved_sp = superpowers_dir or (
                Path(os.environ["EVALS_SUPERPOWERS"]) if os.environ.get("EVALS_SUPERPOWERS") else None
            )
            if resolved_sp is None or not resolved_sp.is_dir():
                raise ValueError("condition 'superpowers' requires a valid EVALS_SUPERPOWERS path")
            shutil.copytree(resolved_sp / "skills", overlay_dir / "skills", symlinks=True)
            using_skill = resolved_sp / "skills" / "using-superpowers" / "SKILL.md"
            if using_skill.is_file():
                sp_text = using_skill.read_text(encoding="utf-8")
                sp_append = (
                    "\n\n## Unattended evaluation mode\n\n"
                    "This session runs unattended with no interactive human replies. Do not stop to wait for user "
                    "approval during brainstorming or planning; proceed directly through implementation, verification, "
                    "and git commit in the same turn.\n"
                )
                (overlay_dir / "AGENTS.md").write_text(sp_text + sp_append, encoding="utf-8")
        case Condition.STANDARDS:
            standards_dest = overlay_dir / "standards"
            standards_dest.mkdir(parents=True, exist_ok=True)
            for folder in ("rules", "skills", "templates"):
                shutil.copytree(repo_root / folder, standards_dest / folder, symlinks=True)
            tools_crates = repo_root / "tools" / "crates"
            if tools_crates.is_dir():
                shutil.copytree(tools_crates, standards_dest / "tools" / "crates", symlinks=True)
            for root_doc in ("README.md", "CONTRIBUTING.md"):
                if (repo_root / root_doc).is_file():
                    shutil.copy2(repo_root / root_doc, standards_dest / root_doc)
            (overlay_dir / "standards_agents_append.md").write_text(
                _standards_agents_appendix(spec, dest_task_dir),
                encoding="utf-8",
            )

    live_rules_dest = dest_task_dir / "tests" / "live_standards"
    if live_rules_dest.exists():
        shutil.rmtree(live_rules_dest)
    live_rules_dest.mkdir(parents=True, exist_ok=True)
    shutil.copytree(repo_root / "rules", live_rules_dest / "rules", symlinks=True)
    shutil.copytree(repo_root / "skills", live_rules_dest / "skills", symlinks=True)
    shutil.copytree(repo_root / "templates", live_rules_dest / "templates", symlinks=True)
    shutil.copy2(repo_root / "evals" / "runner" / "adherence.py", dest_task_dir / "tests" / "adherence.py")
    shutil.copy2(repo_root / "evals" / "runner" / "grade_adherence.py", dest_task_dir / "tests" / "grade_adherence.py")
    (dest_task_dir / "tests" / "condition.txt").write_text(
        f"{format_condition_label(condition)}\n",
        encoding="utf-8",
    )


def load_openrouter_key(key_file: Path) -> str:
    """Read the OpenRouter API key from `OPENROUTER_API_KEY` or `key_file`."""
    from_env = os.environ.get("OPENROUTER_API_KEY", "").strip()
    if from_env:
        return from_env
    try:
        key = key_file.read_text(encoding="utf-8").strip()
    except FileNotFoundError as exc:
        raise ValueError(f"{key_file}: OpenRouter key file not found and OPENROUTER_API_KEY is unset") from exc
    if not key:
        raise ValueError(f"{key_file}: OpenRouter key file is empty")
    return key


def build_harbor_command(
    harbor_bin: str,
    dataset_dir: Path,
    jobs_dir: Path,
    job_name: str,
    model: str,
    n_concurrent: int,
    n_attempts: int,
    effort: str,
    max_turns: int,
) -> list[str]:
    """Construct the exact `harbor run` argument list for one condition."""
    resolved_model = normalize_model_id(model)
    return [
        harbor_bin,
        "run",
        "--yes",
        "--path",
        str(dataset_dir),
        "--jobs-dir",
        str(jobs_dir),
        "--job-name",
        job_name,
        "--agent",
        "evals:PreinstalledPi",
        "--model",
        resolved_model,
        "--ak",
        f"version={PI_VERSION}",
        "--ak",
        f"thinking={effort}",
        "--ak",
        f"max_turns={max_turns}",
        "--n-concurrent",
        str(n_concurrent),
        "--n-attempts",
        str(n_attempts),
    ]


def ensure_base_image(repo_root: Path) -> None:
    """Build `localhost/standards-eval-base:rust-pi-1.4.0` and container tools if not already present."""
    tag = f"localhost/standards-eval-base:rust-pi-{PI_VERSION}"
    probe = subprocess.run(["docker", "image", "inspect", tag], check=False, capture_output=True)
    if probe.returncode != 0:
        pi_dir = repo_root / "evals" / "pi"
        proc = subprocess.run(["docker", "build", "--tag", tag, str(pi_dir)], check=False)
        if proc.returncode != 0:
            raise ValueError(f"{pi_dir}: failed to build base image {tag}")

    cache_bin_dir = repo_root / ".cache" / "evals" / "container-tools"
    if not (cache_bin_dir / "concord").is_file() or not (cache_bin_dir / "rust-scans").is_file():
        cache_bin_dir.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "docker",
                "run",
                "--rm",
                "-v",
                f"{repo_root / 'tools'}:/src:ro",
                "-v",
                f"{cache_bin_dir}:/out",
                tag,
                "bash",
                "-c",
                "set -euo pipefail && cp -R /src /tmp/tools && "
                "cargo build --locked --manifest-path /tmp/tools/Cargo.toml -p concord -p rust-scans && "
                "cp /tmp/tools/target/debug/concord /tmp/tools/target/debug/rust-scans /out/",
            ],
            check=True,
            capture_output=True,
        )


def _verify_task_container(repo_root: Path, spec: TaskSpec) -> tuple[bool, str]:
    """Stage `spec`, build its container image, and verify baseline red and reference green offline."""
    cache_dir = repo_root / ".cache" / "evals" / "check" / spec.name
    if cache_dir.exists():
        shutil.rmtree(cache_dir)
    dataset = stage_condition_dataset(repo_root, [spec], Condition.STANDARDS, cache_dir / "staged", None)
    staged_task = dataset / spec.name
    image_tag = f"localhost/standards-eval-check-{spec.name}:latest"
    build_proc = subprocess.run(
        ["docker", "build", "--tag", image_tag, str(staged_task / "environment")],
        check=False,
        capture_output=True,
        text=True,
    )
    if build_proc.returncode != 0:
        return False, f"docker build failed:\n{build_proc.stdout}\n{build_proc.stderr}"

    logs_dir = cache_dir / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    script = (
        "set -euo pipefail\n"
        "test -d /standards/rules && test -d /standards/skills\n"
        "VERIFIER_DIR=/logs/baseline verifier_dir=/logs/baseline bash /tests/test.sh\n"
        "bash /solution/solve.sh\n"
        "VERIFIER_DIR=/logs/reference verifier_dir=/logs/reference bash /tests/test.sh\n"
    )
    run_proc = subprocess.run(
        [
            "docker",
            "run",
            "--rm",
            "--network",
            "none",
            "--cpus",
            "4",
            "--memory",
            "4096m",
            "-v",
            f"{staged_task / 'tests'}:/tests:ro",
            "-v",
            f"{staged_task / 'solution'}:/solution:ro",
            "-v",
            f"{logs_dir}:/logs",
            image_tag,
            "bash",
            "-c",
            script,
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if run_proc.returncode != 0:
        return False, f"container check failed:\n{run_proc.stdout}\n{run_proc.stderr}"

    base_reward_path = logs_dir / "baseline" / "reward.json"
    ref_reward_path = logs_dir / "reference" / "reward.json"
    if not base_reward_path.is_file() or not ref_reward_path.is_file():
        return False, "missing baseline or reference reward.json"

    base_scores: object = json.loads(base_reward_path.read_text(encoding="utf-8"))
    ref_scores: object = json.loads(ref_reward_path.read_text(encoding="utf-8"))
    if not isinstance(base_scores, dict) or not isinstance(ref_scores, dict):
        return False, "malformed reward.json"
    base_func = base_scores.get("functional", 1.0)
    if not isinstance(base_func, (int, float)) or float(base_func) >= 1.0:
        return False, f"baseline did not fail functional check: {base_scores}"
    if ref_scores != {"reward": 1.0, "functional": 1.0, "adherence": 1.0}:
        adh_text = (logs_dir / "reference" / "adherence.json").read_text(encoding="utf-8")
        return False, f"reference did not score 1.0: {ref_scores}\n{adh_text}"

    shutil.rmtree(cache_dir, ignore_errors=True)
    return True, "verified"


def check_review_spec(spec_path: Path, inventory: StandardsInventory) -> None:
    """Refuse a review spec that does not parse or names a rule that is not a live standards heading."""
    review_spec = parse_review_spec(spec_path)
    anchors = {f"{h.file}#{h.anchor}" for h in inventory.headings}
    for defect in review_spec.defects:
        if defect.rule is not None and defect.rule not in anchors:
            raise ValueError(f"{spec_path}: {defect.item_id}: rule {defect.rule!r} is not a live standards heading")


def run_check(repo_root: Path, verify_containers: bool) -> int:
    """Validate live standards discovery, task layouts, reference solutions, and verifiers."""
    inventory = discover_standards(repo_root)
    tasks_root = repo_root / "evals" / "tasks"
    tasks = discover_tasks(tasks_root)
    if not tasks:
        raise ValueError(f"{tasks_root}: no evaluation tasks found")
    if verify_containers:
        ensure_base_image(repo_root)

    print(
        f"evals check: discovered {len(inventory.rule_files)} rule files, "
        f"{len(inventory.skill_files)} skills, {len(inventory.headings)} rule headings, "
        f"and {len(tasks)} tasks"
    )

    for spec in tasks:
        solution_path = spec.path / "solution" / "solve.sh"
        if not solution_path.is_file():
            raise ValueError(f"{solution_path}: missing reference solution/solve.sh")
        if spec.track == Track.REVIEW:
            check_review_spec(spec.path / "tests" / "review_spec.json", inventory)

        if verify_containers:
            ok, detail = _verify_task_container(repo_root, spec)
            if not ok:
                sys.stderr.write(f"evals check: {spec.name} FAILED:\n{detail}\n")
                return 1
            print(f"evals check: {spec.name} ({spec.track}) verified (baseline red, reference green)")
        else:
            print(f"evals check: {spec.name} ({spec.track}) structure OK")

    return 0


def detect_job_model(job_root: Path) -> str:
    """Detect the model name recorded in Harbor's job result under `job_root`."""
    for condition in ALL_CONDITIONS:
        cond_label = format_condition_label(condition)
        cond_dir = job_root / cond_label
        if not cond_dir.is_dir():
            continue
        for result_file in sorted(cond_dir.rglob("result.json")):
            if result_file.parent.parent != cond_dir:
                continue
            raw_job: object = json.loads(result_file.read_text(encoding="utf-8"))
            if isinstance(raw_job, dict):
                stats = raw_job.get("stats")
                if isinstance(stats, dict):
                    evals_map = stats.get("evals")
                    if isinstance(evals_map, dict):
                        for eval_key in evals_map:
                            parts = str(eval_key).split("__")
                            if len(parts) >= 2 and parts[1]:
                                return str(parts[1])
    return DEFAULT_MODEL


def collect_job_summaries(job_root: Path, inventory: StandardsInventory) -> list[TrialSummary]:
    """Read all trial outcomes and adherence records under `job_root`."""
    if not job_root.is_dir():
        raise ValueError(f"{job_root}: job directory does not exist")

    summaries: list[TrialSummary] = []
    for condition in ALL_CONDITIONS:
        cond_label = format_condition_label(condition)
        cond_dir = job_root / cond_label
        if not cond_dir.is_dir():
            continue
        for result_file in sorted(cond_dir.rglob("result.json")):
            trial_dir = result_file.parent
            if trial_dir == cond_dir or not (trial_dir / "verifier").is_dir():
                continue
            raw_result: object = json.loads(result_file.read_text(encoding="utf-8"))
            if not isinstance(raw_result, dict):
                continue
            task_name = str(raw_result.get("task_name", trial_dir.name)).removeprefix("standards/")
            verifier_res = raw_result.get("verifier_result")
            reward = 0.0
            functional_score = 0.0
            adherence_score = 0.0
            if isinstance(verifier_res, dict):
                rewards = verifier_res.get("rewards")
                if isinstance(rewards, dict):
                    r_val = rewards.get("reward", 0.0)
                    f_val = rewards.get("functional", r_val)
                    a_val = rewards.get("adherence", 0.0)
                    if isinstance(r_val, (int, float)):
                        reward = float(r_val)
                    if isinstance(f_val, (int, float)):
                        functional_score = float(f_val)
                    if isinstance(a_val, (int, float)):
                        adherence_score = float(a_val)

            session_files = sorted((trial_dir / "agent" / "pi" / "sessions").glob("*.jsonl"))
            jsonl_text = session_files[-1].read_text(encoding="utf-8") if session_files else ""
            traj_audit = parse_pi_session_jsonl(jsonl_text, inventory)

            adherence_path = trial_dir / "verifier" / "adherence.json"
            rule_outcomes: dict[str, bool] = {}
            review: ReviewOutcome | None = None
            track = Track.REVIEW if task_name.startswith("review-") else Track.WRITE
            if adherence_path.is_file():
                raw_adh: object = json.loads(adherence_path.read_text(encoding="utf-8"))
                if isinstance(raw_adh, dict):
                    raw_track = raw_adh.get("track")
                    if raw_track == "review":
                        track = Track.REVIEW
                    elif raw_track == "write":
                        track = Track.WRITE
                    rules_obj = raw_adh.get("rules")
                    if isinstance(rules_obj, dict):
                        for k, v in rules_obj.items():
                            if isinstance(v, bool):
                                rule_outcomes[str(k)] = v
                            elif (
                                isinstance(v, dict)
                                and isinstance(v.get("passed"), bool)
                                and v.get("applicable", True) is not False
                            ):
                                rule_outcomes[str(k)] = bool(v["passed"])
                    review = _parse_review_record(task_name, raw_adh.get("review"), adherence_path)

            summaries.append(
                TrialSummary(
                    task=task_name,
                    track=track,
                    condition=condition,
                    reward=reward,
                    functional_score=functional_score,
                    adherence_score=adherence_score,
                    turns=traj_audit.turn_count,
                    cost_usd=traj_audit.cost_usd,
                    standards_files_read=traj_audit.standards_files_read,
                    skills_read=traj_audit.skills_read,
                    rule_outcomes=rule_outcomes,
                    review=review,
                )
            )
    return summaries


def _parse_review_record(task: str, raw: object, where: Path) -> ReviewOutcome | None:
    """Read the `review` block a review verifier writes into `adherence.json`; `None` when it is absent."""
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise ValueError(f"{where}: review: expected an object")
    raw_defects = raw.get("defects")
    false_positives = raw.get("false_positives")
    if not isinstance(raw_defects, dict) or not isinstance(false_positives, int):
        raise ValueError(f"{where}: review: expected defects and false_positives")
    defects: list[DefectOutcome] = []
    for defect_id, record in raw_defects.items():
        if (
            not isinstance(record, dict)
            or not isinstance(record.get("found"), bool)
            or not isinstance(record.get("named_by_standards"), bool)
        ):
            raise ValueError(f"{where}: review: defects: {defect_id}: expected found and named_by_standards")
        defects.append(
            DefectOutcome(
                task=task,
                defect_id=str(defect_id),
                named_by_standards=bool(record["named_by_standards"]),
                found=bool(record["found"]),
            )
        )
    return ReviewOutcome(defects=tuple(defects), false_positives=false_positives)


@dataclass(frozen=True, slots=True)
class ReviewTotals:
    """Review outcomes pooled over a condition's trials: defects planted and found by group, and false positives."""

    planted_named: int
    found_named: int
    planted_unnamed: int
    found_unnamed: int
    false_positives: int

    def recall(self, named: bool | None = None) -> float | None:
        """Defects found over defects planted, in one group or overall; `None` when nothing was planted."""
        planted, found = self._group(named)
        return found / planted if planted else None

    def precision(self, named: bool | None = None) -> float | None:
        """Defects found over defects planted plus must-fix findings on no defect, which belong to no group."""
        planted, found = self._group(named)
        return found / (planted + self.false_positives) if planted else None

    def _group(self, named: bool | None) -> tuple[int, int]:
        if named is None:
            return self.planted_named + self.planted_unnamed, self.found_named + self.found_unnamed
        if named:
            return self.planted_named, self.found_named
        return self.planted_unnamed, self.found_unnamed


def pool_review_outcomes(summaries: list[TrialSummary]) -> ReviewTotals:
    """Pool every review outcome in `summaries` into one set of counts."""
    outcomes = [s.review for s in summaries if s.review is not None]
    defects = [d for outcome in outcomes for d in outcome.defects]
    return ReviewTotals(
        planted_named=sum(1 for d in defects if d.named_by_standards),
        found_named=sum(1 for d in defects if d.named_by_standards and d.found),
        planted_unnamed=sum(1 for d in defects if not d.named_by_standards),
        found_unnamed=sum(1 for d in defects if not d.named_by_standards and d.found),
        false_positives=sum(outcome.false_positives for outcome in outcomes),
    )


def _rate(value: float | None) -> str:
    return "-" if value is None else f"{value:.2f}"


REVIEW_SCORE_NOTE = (
    "Recall and precision are location matches over the review trials with a grading record, pooled per condition: "
    "recall is found / planted, and precision is found / (planted + must-fix findings on no defect). A finding is "
    "matched by file, lines and kind; its explanation is not judged, so these scores measure detection of the "
    "planted defects, not review quality generally. Should-fix findings on no defect cost nothing. Named defects "
    "are those a standards rule names. Ungraded counts review trials without a grading record, such as a verifier "
    "failure, which these columns leave out."
)


def review_score_rows(summaries: list[TrialSummary]) -> list[tuple[str, list[str]]]:
    """One row per condition with review trials: recall and precision overall and by group, then ungraded trials."""
    rows: list[tuple[str, list[str]]] = []
    for condition in ALL_CONDITIONS:
        review_trials = [s for s in summaries if s.condition == condition and s.track == Track.REVIEW]
        if not review_trials:
            continue
        totals = pool_review_outcomes(review_trials)
        cells = [_rate(totals.recall(named)) for named in (None, True, False)]
        cells += [_rate(totals.precision(named)) for named in (None, True, False)]
        cells.append(str(sum(1 for s in review_trials if s.review is None)))
        rows.append((format_condition_label(condition), cells))
    return rows


def defect_found_rows(summaries: list[TrialSummary]) -> list[tuple[str, str, list[str]]]:
    """One row per planted defect: its task and id, its group, and how often each condition found it."""
    keys = sorted(
        {(d.task, d.defect_id, d.named_by_standards) for s in summaries if s.review for d in s.review.defects}
    )
    rows: list[tuple[str, str, list[str]]] = []
    for task, defect_id, named in keys:
        cells: list[str] = []
        for condition in ALL_CONDITIONS:
            outcomes = [
                d.found
                for s in summaries
                if s.condition == condition and s.review is not None
                for d in s.review.defects
                if (d.task, d.defect_id) == (task, defect_id)
            ]
            cells.append(f"{sum(outcomes)}/{len(outcomes)}" if outcomes else "-")
        rows.append((f"{task}/{defect_id}", "named" if named else "unnamed", cells))
    return rows


def render_report(summaries: list[TrialSummary], inventory: StandardsInventory) -> str:
    """Format a comparison report across conditions, tracks, tasks, and individual standards rules."""
    if not summaries:
        return "No completed trials found.\n"

    lines: list[str] = []
    lines.append("Condition       Write Avg   Review Avg   Overall   Adherence   Avg Turns   Cost (USD)")
    lines.append("-------------------------------------------------------------------------------------")
    for condition in ALL_CONDITIONS:
        cond_trials = [s for s in summaries if s.condition == condition]
        if not cond_trials:
            continue
        write_trials = [s for s in cond_trials if s.track == Track.WRITE]
        review_trials = [s for s in cond_trials if s.track == Track.REVIEW]
        w_avg = sum(s.reward for s in write_trials) / len(write_trials) if write_trials else 0.0
        r_avg = sum(s.reward for s in review_trials) / len(review_trials) if review_trials else 0.0
        o_avg = sum(s.reward for s in cond_trials) / len(cond_trials)
        a_avg = sum(s.adherence_score for s in cond_trials) / len(cond_trials)
        adh_col = f"{a_avg:>9.2f}" if condition == Condition.STANDARDS else f"{'-':>9}"
        t_avg = sum(s.turns for s in cond_trials) / len(cond_trials)
        cost = sum(s.cost_usd for s in cond_trials)
        label = format_condition_label(condition)
        row = f"{label:<15} {w_avg:>9.2f}   {r_avg:>10.2f}   {o_avg:>7.2f}   {adh_col}   {t_avg:>9.1f}   ${cost:>8.2f}"
        lines.append(row)

    all_tasks = sorted({s.task for s in summaries})
    lines.append("")
    lines.append("Task Breakdown (functional score; standards shows adherence in parentheses):")
    lines.append(
        "Task                                    Track     none          agent-skills  superpowers   standards"
    )
    lines.append(
        "--------------------------------------------------------------------------------------------------------"
    )
    for task_name in all_tasks:
        task_trials = [s for s in summaries if s.task == task_name]
        track_str = str(task_trials[0].track) if task_trials else "write"
        cells: list[str] = []
        for condition in ALL_CONDITIONS:
            matched = [s for s in task_trials if s.condition == condition]
            if matched:
                m_f = sum(s.functional_score for s in matched) / len(matched)
                if condition == Condition.STANDARDS:
                    m_a = sum(s.adherence_score for s in matched) / len(matched)
                    cells.append(f"{m_f:.2f} ({m_a:.2f})")
                else:
                    cells.append(f"{m_f:.2f}")
            else:
                cells.append("   -       ")
        lines.append(f"{task_name:<39} {track_str:<9} {cells[0]:<13} {cells[1]:<13} {cells[2]:<13} {cells[3]}")

    score_rows = review_score_rows(summaries)
    if score_rows:
        lines.append("")
        lines.append("Review Recall and Precision:")
        lines.append(REVIEW_SCORE_NOTE)
        lines.append("Condition       Recall  Named  Unnamed   Precision  Named  Unnamed   Ungraded")
        lines.append("------------------------------------------------------------------------------")
        for label, c in score_rows:
            lines.append(f"{label:<15} {c[0]:>6}  {c[1]:>5}  {c[2]:>7}   {c[3]:>9}  {c[4]:>5}  {c[5]:>7}   {c[6]:>8}")
        lines.append("")
        lines.append("Review Defects Found (trials found / trials with a grading record):")
        lines.append(f"{'Defect':<61} {'Group':<9} {'none':<6} {'agent-skills':<13} {'superpowers':<12} standards")
        for name, group, c in defect_found_rows(summaries):
            lines.append(f"{name:<61} {group:<9} {c[0]:<6} {c[1]:<13} {c[2]:<12} {c[3]}")

    std_trials = [s for s in summaries if s.condition == Condition.STANDARDS]
    if std_trials:
        lines.append("")
        lines.append("Standards File Read Telemetry (condition = standards):")
        for rule_file in inventory.rule_files:
            read_count = sum(1 for s in std_trials if rule_file in s.standards_files_read)
            pct = (100.0 * read_count) / len(std_trials)
            lines.append(f"  {rule_file:<44} {pct:>5.1f}% ({read_count}/{len(std_trials)})")
        for skill_file in inventory.skill_files:
            read_count = sum(1 for s in std_trials if skill_file in s.skills_read)
            pct = (100.0 * read_count) / len(std_trials)
            lines.append(f"  {skill_file:<44} {pct:>5.1f}% ({read_count}/{len(std_trials)})")

    std_rules = sorted(
        {rule_id for s in std_trials for rule_id in s.rule_outcomes if not rule_id.startswith("review/")}
    )
    if std_rules:
        lines.append("")
        lines.append("Per-Rule Standards Adherence (condition = standards):")
        lines.append("  Rule                                          Adherence")
        lines.append("  -------------------------------------------------------")
        for rule_id in std_rules:
            relevant = [s.rule_outcomes[rule_id] for s in std_trials if rule_id in s.rule_outcomes]
            if relevant:
                pct = (100.0 * sum(1 for ok in relevant if ok)) / len(relevant)
                lines.append(f"  {rule_id:<45} {pct:>5.1f}% ({sum(1 for ok in relevant if ok)}/{len(relevant)})")

    return "\n".join(lines) + "\n"


@dataclass(frozen=True, slots=True)
class RunMetadata:
    """Provenance metadata for a benchmark report."""

    title: str
    date_utc: str
    standards_commit: str
    agent_skills_commit: str
    superpowers_commit: str
    model: str
    effort: str
    max_turns: int


def _read_flake_lock_rev(repo_root: Path, node_name: str, env_var: str) -> str:
    from_env = os.environ.get(env_var, "").strip()
    if from_env and from_env != "unknown":
        return from_env
    lock_path = repo_root / "flake.lock"
    if lock_path.is_file():
        raw_lock: object = json.loads(lock_path.read_text(encoding="utf-8"))
        if isinstance(raw_lock, dict):
            nodes = raw_lock.get("nodes")
            if isinstance(nodes, dict):
                node = nodes.get(node_name)
                if isinstance(node, dict):
                    locked = node.get("locked")
                    if isinstance(locked, dict) and isinstance(locked.get("rev"), str):
                        return str(locked["rev"])
    return "unknown"


def resolve_run_metadata(
    repo_root: Path,
    title: str,
    model: str = DEFAULT_MODEL,
    effort: str = "high",
    max_turns: int = 35,
) -> RunMetadata:
    """Read the current `standards`, `agent-skills`, and `superpowers` commits."""
    git_proc = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    standards_rev = git_proc.stdout.strip() if git_proc.returncode == 0 else "unknown"
    agent_skills_rev = _read_flake_lock_rev(repo_root, "agent-skills", "EVALS_AGENT_SKILLS_REV")
    superpowers_rev = _read_flake_lock_rev(repo_root, "superpowers", "EVALS_SUPERPOWERS_REV")

    return RunMetadata(
        title=title,
        date_utc=datetime.now(tz=UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        standards_commit=standards_rev,
        agent_skills_commit=agent_skills_rev,
        superpowers_commit=superpowers_rev,
        model=model,
        effort=effort,
        max_turns=max_turns,
    )


def render_markdown_report(
    summaries: list[TrialSummary],
    inventory: StandardsInventory,
    meta: RunMetadata,
) -> str:
    """Format a Markdown benchmark report with provenance, task scores, file-read telemetry, and rule adherence."""
    lines: list[str] = [
        f"# Evaluation report: `{meta.title}`",
        "",
        f"- Date: `{meta.date_utc}`",
        f"- Standards commit: `{meta.standards_commit}`",
        f"- Agent-skills commit: `{meta.agent_skills_commit}`",
        f"- Superpowers commit: `{meta.superpowers_commit}`",
        f"- Harness: `harbor 0.24.0`, `pi {PI_VERSION}`",
        f"- Model: `{meta.model}` (thinking: `{meta.effort}`, max turns: `{meta.max_turns}`)",
        "",
        "## Summary by condition",
        "",
        "| Condition | Write avg | Review avg | Overall reward | Adherence | Avg turns | Cost (USD) |",
        "|---|---|---|---|---|---|---|",
    ]
    for condition in ALL_CONDITIONS:
        cond_trials = [s for s in summaries if s.condition == condition]
        if not cond_trials:
            continue
        write_trials = [s for s in cond_trials if s.track == Track.WRITE]
        review_trials = [s for s in cond_trials if s.track == Track.REVIEW]
        w_avg = sum(s.reward for s in write_trials) / len(write_trials) if write_trials else 0.0
        r_avg = sum(s.reward for s in review_trials) / len(review_trials) if review_trials else 0.0
        o_avg = sum(s.reward for s in cond_trials) / len(cond_trials)
        a_avg = sum(s.adherence_score for s in cond_trials) / len(cond_trials)
        adh_cell = f"{a_avg:.2f}" if condition == Condition.STANDARDS else "-"
        t_avg = sum(s.turns for s in cond_trials) / len(cond_trials)
        cost = sum(s.cost_usd for s in cond_trials)
        label = format_condition_label(condition)
        lines.append(
            f"| `{label}` | {w_avg:.2f} | {r_avg:.2f} | {o_avg:.2f} | {adh_cell} | {t_avg:.1f} | ${cost:.2f} |"
        )

    all_tasks = sorted({s.task for s in summaries})
    lines.extend(
        [
            "",
            "## Per-task breakdown (functional score; `standards` shows adherence in parentheses)",
            "",
            "| Task | Track | `none` | `agent-skills` | `superpowers` | `standards` |",
            "|---|---|---|---|---|---|",
        ]
    )
    for task_name in all_tasks:
        task_trials = [s for s in summaries if s.task == task_name]
        track_str = str(task_trials[0].track) if task_trials else "write"
        cells: list[str] = []
        for condition in ALL_CONDITIONS:
            matched = [s for s in task_trials if s.condition == condition]
            if matched:
                m_f = sum(s.functional_score for s in matched) / len(matched)
                if condition == Condition.STANDARDS:
                    m_a = sum(s.adherence_score for s in matched) / len(matched)
                    cells.append(f"{m_f:.2f} ({m_a:.2f})")
                else:
                    cells.append(f"{m_f:.2f}")
            else:
                cells.append("-")
        lines.append(f"| `{task_name}` | {track_str} | {cells[0]} | {cells[1]} | {cells[2]} | {cells[3]} |")

    std_trials = [s for s in summaries if s.condition == Condition.STANDARDS]
    if std_trials:
        lines.extend(
            [
                "",
                "## Standards file-read telemetry (`condition = standards`)",
                "",
                "| File | Read rate | Trials read |",
                "|---|---|---|",
            ]
        )
        for rule_file in inventory.rule_files:
            read_count = sum(1 for s in std_trials if rule_file in s.standards_files_read)
            pct = (100.0 * read_count) / len(std_trials)
            lines.append(f"| `{rule_file}` | {pct:.1f}% | {read_count} / {len(std_trials)} |")
        for skill_file in inventory.skill_files:
            read_count = sum(1 for s in std_trials if skill_file in s.skills_read)
            pct = (100.0 * read_count) / len(std_trials)
            lines.append(f"| `{skill_file}` | {pct:.1f}% | {read_count} / {len(std_trials)} |")

    std_rules = sorted(
        {rule_id for s in std_trials for rule_id in s.rule_outcomes if not rule_id.startswith("review/")}
    )
    if std_rules:
        lines.extend(
            [
                "",
                "## Per-rule standards adherence (`condition = standards`)",
                "",
                "| Rule | Adherence | Trials passed |",
                "|---|---|---|",
            ]
        )
        for rule_id in std_rules:
            relevant = [s.rule_outcomes[rule_id] for s in std_trials if rule_id in s.rule_outcomes]
            if relevant:
                passed_n = sum(1 for ok in relevant if ok)
                pct = (100.0 * passed_n) / len(relevant)
                lines.append(f"| `{rule_id}` | {pct:.1f}% | {passed_n} / {len(relevant)} |")

    score_rows = review_score_rows(summaries)
    if score_rows:
        lines.extend(
            [
                "",
                "## Review recall and precision",
                "",
                REVIEW_SCORE_NOTE + " Each group's precision counts every must-fix finding on no defect, "
                "since those findings belong to neither group.",
                "",
                "| Condition | Recall | Named recall | Unnamed recall "
                "| Precision | Named precision | Unnamed precision | Ungraded |",
                "|---|---|---|---|---|---|---|---|",
            ]
        )
        lines.extend(f"| `{label}` | {' | '.join(cells)} |" for label, cells in score_rows)
        lines.extend(
            [
                "",
                "## Review defects found (trials found / trials with a grading record)",
                "",
                "| Defect | Group | `none` | `agent-skills` | `superpowers` | `standards` |",
                "|---|---|---|---|---|---|",
            ]
        )
        lines.extend(
            f"| `{name}` | {group} | {' | '.join(cells)} |" for name, group, cells in defect_found_rows(summaries)
        )

    return "\n".join(lines) + "\n"


def save_report_files(
    dest_dir: Path,
    summaries: list[TrialSummary],
    inventory: StandardsInventory,
    meta: RunMetadata,
) -> tuple[Path, Path]:
    """Write `README.md`, `summary.json`, and per-condition/per-task details into `dest_dir`."""
    dest_dir.mkdir(parents=True, exist_ok=True)
    md_path = dest_dir / "README.md"
    json_path = dest_dir / "summary.json"
    md_path.write_text(render_markdown_report(summaries, inventory, meta), encoding="utf-8")

    for summary in summaries:
        cond_label = format_condition_label(summary.condition)
        cond_dir = dest_dir / cond_label
        cond_dir.mkdir(parents=True, exist_ok=True)
        task_record = {
            "task": summary.task,
            "track": str(summary.track),
            "condition": cond_label,
            "reward": summary.reward,
            "functional": summary.functional_score,
            "adherence": summary.adherence_score,
            "turns": summary.turns,
            "cost_usd": summary.cost_usd,
            "standards_files_read": list(summary.standards_files_read),
            "skills_read": list(summary.skills_read),
            "rules": summary.rule_outcomes,
            "review": _review_json(summary.review),
        }
        (cond_dir / f"{summary.task}.json").write_text(
            json.dumps(task_record, indent=2) + "\n",
            encoding="utf-8",
        )

    payload = {
        "metadata": {
            "title": meta.title,
            "date_utc": meta.date_utc,
            "standards_commit": meta.standards_commit,
            "agent_skills_commit": meta.agent_skills_commit,
            "superpowers_commit": meta.superpowers_commit,
            "harbor_version": "0.24.0",
            "pi_version": PI_VERSION,
            "model": meta.model,
            "effort": meta.effort,
            "max_turns": meta.max_turns,
        },
        "trials": [
            {
                "task": s.task,
                "track": str(s.track),
                "condition": format_condition_label(s.condition),
                "reward": s.reward,
                "functional": s.functional_score,
                "adherence": s.adherence_score,
                "turns": s.turns,
                "cost_usd": s.cost_usd,
                "standards_files_read": list(s.standards_files_read),
                "skills_read": list(s.skills_read),
                "rules": s.rule_outcomes,
                "review": _review_json(s.review),
            }
            for s in summaries
        ],
    }
    json_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return md_path, json_path


def _review_json(review: ReviewOutcome | None) -> dict[str, object] | None:
    if review is None:
        return None
    return {
        "false_positives": review.false_positives,
        "defects": {
            d.defect_id: {"found": d.found, "named_by_standards": d.named_by_standards} for d in review.defects
        },
    }


def resolve_results_run_dir(results_root: Path, meta: RunMetadata, custom_name: str | None) -> Path:
    """Resolve `evals/results/<name>/` (or `evals/results/<date>/` when unnamed) for a saved benchmark run."""
    folder_name = custom_name.strip() if custom_name and custom_name.strip() else meta.date_utc[:10]
    return results_root / folder_name


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="evals", description="Run and inspect standards evaluations.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    check_p = subparsers.add_parser("check", help="Validate task structures, reference solutions, and verifiers.")
    check_p.add_argument(
        "--quick",
        action="store_true",
        help="Check task metadata and file layout without running container verifiers.",
    )

    run_p = subparsers.add_parser("run", help="Run Harbor evaluations across experimental conditions.")
    run_p.add_argument("--model", default=DEFAULT_MODEL, help=f"Model ID (default: {DEFAULT_MODEL}).")
    run_p.add_argument(
        "--condition",
        action="append",
        choices=[c.value for c in Condition],
        help="Condition to run (repeatable; defaults to all three: none, agent-skills, standards).",
    )
    run_p.add_argument("--track", choices=[t.value for t in Track], help="Filter tasks by track (write or review).")
    run_p.add_argument("--task", help="Run only the named task.")
    run_p.add_argument("--n-concurrent", type=int, default=2, help="Concurrent trials per job (default: 2).")
    run_p.add_argument("--n-attempts", type=int, default=1, help="Attempts per task (default: 1).")
    run_p.add_argument("--effort", default="high", help="Pi thinking effort (default: high).")
    run_p.add_argument("--max-turns", type=int, default=35, help="Maximum turns per trial (default: 35).")
    run_p.add_argument("--key-file", type=Path, default=DEFAULT_KEY_FILE, help="Path to OpenRouter API key file.")
    run_p.add_argument(
        "--dry-run", action="store_true", help="Stage datasets and print Harbor commands without running."
    )
    run_p.add_argument(
        "--save",
        help="Also save the Markdown and JSON benchmark report to evals/results/<name>.{md,json}.",
    )

    rep_p = subparsers.add_parser("report", help="Print scoreboard and standards adherence for a job run.")
    rep_p.add_argument(
        "job_dirs", nargs="*", type=Path, help="Job directory under .cache/evals/jobs/ (default: latest)."
    )
    rep_p.add_argument(
        "--save",
        help="Save the Markdown and JSON benchmark report to evals/results/<name>.{md,json}.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    """Parse arguments and run the requested `evals` subcommand."""
    parser = _build_parser()
    args = parser.parse_args(argv)
    repo_root = Path(__file__).resolve().parents[2]

    try:
        if args.command == "check":
            return run_check(repo_root, verify_containers=not bool(args.quick))

        if args.command == "run":
            track_filter = Track(args.track) if args.track else None
            conditions = [Condition(c) for c in args.condition] if args.condition else list(ALL_CONDITIONS)
            tasks = discover_tasks(repo_root / "evals" / "tasks", track_filter=track_filter, name_filter=args.task)
            if not tasks:
                raise ValueError("no tasks matched the given filters")

            agent_skills_env = os.environ.get("EVALS_AGENT_SKILLS")
            agent_skills_dir = Path(agent_skills_env) if agent_skills_env else None
            superpowers_env = os.environ.get("EVALS_SUPERPOWERS")
            superpowers_dir = Path(superpowers_env) if superpowers_env else None
            harbor_bin = os.environ.get("EVALS_HARBOR", "harbor")

            run_id = datetime.now(tz=UTC).strftime("%Y%m%d_%H%M%S")
            cache_root = repo_root / ".cache" / "evals"
            stage_root = cache_root / "staged" / run_id
            job_root = cache_root / "jobs" / run_id

            resolved_model = normalize_model_id(str(args.model))
            env = dict(os.environ)
            env["EVALS_PI_AUTH_FILE"] = str(DEFAULT_PI_AUTH_FILE)
            env["HOME"] = str(cache_root / "home")
            runner_dir = str(Path(__file__).resolve().parent)
            existing_pp = env.get("PYTHONPATH", "")
            env["PYTHONPATH"] = f"{runner_dir}:{existing_pp}" if existing_pp else runner_dir
            (cache_root / "home").mkdir(parents=True, exist_ok=True)
            if resolved_model.startswith("openrouter/") and not args.dry_run:
                env["OPENROUTER_API_KEY"] = load_openrouter_key(args.key_file)

            if not args.dry_run:
                ensure_base_image(repo_root)

            for condition in conditions:
                dataset_dir = stage_condition_dataset(
                    repo_root,
                    tasks,
                    condition,
                    stage_root,
                    agent_skills_dir,
                    superpowers_dir,
                )
                cond_label = format_condition_label(condition)
                cond_jobs_dir = job_root / cond_label
                cmd = build_harbor_command(
                    harbor_bin=harbor_bin,
                    dataset_dir=dataset_dir,
                    jobs_dir=cond_jobs_dir,
                    job_name="trial",
                    model=resolved_model,
                    n_concurrent=int(args.n_concurrent),
                    n_attempts=int(args.n_attempts),
                    effort=str(args.effort),
                    max_turns=int(args.max_turns),
                )
                print(f"evals run [{cond_label}]: {' '.join(cmd)}")
                if not args.dry_run:
                    cond_jobs_dir.mkdir(parents=True, exist_ok=True)
                    proc = subprocess.run(cmd, env=env, check=False)
                    if proc.returncode != 0:
                        sys.stderr.write(f"evals run: harbor exited with status {proc.returncode} on {cond_label}\n")
                        return proc.returncode

            if not args.dry_run:
                inventory = discover_standards(repo_root)
                summaries = collect_job_summaries(job_root, inventory)
                meta = resolve_run_metadata(
                    repo_root,
                    title=args.save or run_id,
                    model=resolved_model,
                    effort=str(args.effort),
                    max_turns=int(args.max_turns),
                )
                md_file, _ = save_report_files(job_root, summaries, inventory, meta)
                sys.stdout.write("\n" + render_report(summaries, inventory))
                print(f"\nReport written to {md_file}")
                if args.save is not None:
                    run_folder = resolve_results_run_dir(
                        repo_root / "evals" / "results",
                        meta,
                        str(args.save) or None,
                    )
                    saved_md, _ = save_report_files(run_folder, summaries, inventory, meta)
                    print(f"Saved permanent report to {saved_md}")
            return 0

        if args.command == "report":
            inventory = discover_standards(repo_root)
            jobs_base = repo_root / ".cache" / "evals" / "jobs"
            targets: list[Path] = list(args.job_dirs)
            if not targets:
                if not jobs_base.is_dir():
                    raise ValueError(f"{jobs_base}: no job runs found")
                candidates = sorted(p for p in jobs_base.iterdir() if p.is_dir())
                if not candidates:
                    raise ValueError(f"{jobs_base}: no job runs found")
                targets = [candidates[-1]]

            for target in targets:
                print(f"=== Report: {target} ===")
                summaries = collect_job_summaries(target, inventory)
                sys.stdout.write(render_report(summaries, inventory))
                meta = resolve_run_metadata(
                    repo_root,
                    title=args.save or target.name,
                    model=detect_job_model(target),
                )
                save_report_files(target, summaries, inventory, meta)
                if args.save is not None:
                    run_folder = resolve_results_run_dir(
                        repo_root / "evals" / "results",
                        meta,
                        str(args.save) or None,
                    )
                    saved_md, _ = save_report_files(run_folder, summaries, inventory, meta)
                    print(f"Saved permanent report to {saved_md}")
            return 0

    except ValueError as exc:
        sys.stderr.write(f"evals: {exc}\n")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
