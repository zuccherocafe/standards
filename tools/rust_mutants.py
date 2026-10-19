"""The Rust standard's mutation report: runs cargo-mutants over the Rust a change touched, or over a whole crate, and
lists each mutant the tests missed for the writer to judge, and each that timed out for the writer to diagnose. Fails
nothing: it is a review aid (rules/languages/rust/tests.md, Mutation report).

Run from inside the repository; `--workspace` is the folder holding the workspace Cargo.toml:

  rust-mutants --workspace rust --base <revision> --out <folder outside the repository> [--shards 2] [-- <options>]
  rust-mutants --workspace rust --package <crate> --out <folder> [--shards 2] [-- <cargo-mutants options>]

With `--base`, only the Rust changed since that revision is mutated, in every workspace member. The diff is taken
relative to the workspace with plain `a/` and `b/` prefixes whatever the user's Git settings, since cargo-mutants
matches paths from the workspace: a diff relative to the repository root matches nothing and reports success. With
`--package` instead, every mutant in that crate is tested.

cargo-mutants copies only the workspace folder, so a crate that embeds a file from elsewhere in the repository does not
build in its copy. Each shard therefore gets its own copy of the whole repository's tracked and unignored files under
`--out`, and cargo-mutants mutates that copy in place, with `CARGO_TARGET_DIR` set to the copy's own `target` (which
also overrides a target folder in Cargo's configuration): shards sharing a target folder test each other's builds. The
shards run at once and their lists are merged; the copies are removed afterwards, and on any failure every started
shard is stopped first. The tool owns only `--out`'s `rust-mutants` folder, emptied at the start so what is printed is
this run's. Needs `git`, `cargo mutants` and mold's `ld.mold` on PATH, a nightly Cargo for the profile's flags and
the test harness's options, `systemd-run` for the memory cap and `unshare` for the PID namespaces.

Each shard runs in a systemd user scope holding an equal part of a 16 GiB memory cap, without swap, so a mutant that
allocates without end is killed and counted while the run goes on. Where `systemd-run --user` cannot start a scope, the
shards run without a cap and the report says so. The shards share the jobserver's tasks, one per CPU, and each test
binary stops at its first failing test, since one failure is enough to catch a mutant.

Every mutant builds with the Cargo profile `mutants`, which the tool defines, so a project need not. The profile is
the development profile with light optimisation, incremental builds, many codegen units and no debug information, and
with dependencies fully optimised, since they build once per shard. The tool passes these settings with Cargo's
`--config` on cargo-mutants' build, check and test calls, the baseline's included. A project's own `[profile.mutants]`
keeps only the keys the tool does not set.

mold links the profile, through the profile's own `rustflags`, which Cargo adds to the rustflags it selects from the
environment or its configuration. With cargo-mutants' lint cap on (`--cap-lints true`), cargo-mutants sets
`CARGO_ENCODED_RUSTFLAGS`, so Cargo drops configured target and build rustflags; the profile's own still apply.

Doctests are skipped, since each is a binary of its own to build. A `--profile` among the options after `--` turns the
tool's profile off: the tool then defines no profile and selects none, and that profile builds as the project defines
it, linked by the project's own linker.

Every test binary runs as the first process of a PID namespace of its own, through Cargo's test runner for the host
(`CARGO_TARGET_<HOST>_RUNNER`, wrapping any runner already set there), so a mutant in code that signals processes can
reach only the processes its test started, and every one of them ends with the binary. Where an unprivileged PID
namespace cannot be made, the test binaries run uncontained and the report says so.
"""

import argparse
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

LISTS = ("caught", "missed", "timeout", "unviable")
# cargo-mutants' exit codes for a finished run: all caught, some missed, some timed out.
FINISHED = (0, 2, 3)
# The one folder under `--out` this tool writes and empties.
OWNED = "rust-mutants"
# The memory every shard's scope may hold together: four times what a whole run over the standards' own tools needs.
MEMORY = 16 * 1024**3
# The test harness's arguments, through cargo test: stop each test binary at its first failure.
FAIL_FAST = ("--", "-Zunstable-options", "--fail-fast")
# Cargo's test targets without the doctests, through cargo test: the library, binaries and integration tests.
NO_DOCTESTS = "--tests"
# cargo-mutants' options whose value, given as the next argument, may start with a hyphen, even be `--`.
HYPHEN_VALUES = ("--cargo-arg", "-C", "--cargo-test-arg")
# The profile every mutant builds with unless the caller names another, and its settings as Cargo configuration.
PROFILE = "mutants"
SETTINGS = (
    'inherits="dev"',
    "incremental=true",
    "codegen-units=256",
    "opt-level=1",
    'debug="none"',
    'package."*".opt-level=3',
    'rustflags=["-Clink-arg=-fuse-ld=mold"]',
)


def changed_rust(diff: str) -> list[str]:
    """The Rust files a diff changes, as its `+++ b/` lines name them."""
    return [line[len("+++ b/") :] for line in diff.splitlines() if line.startswith("+++ b/") and line.endswith(".rs")]


def outcome_lists(shards: list[Path]) -> dict[str, list[str]]:
    """Each outcome list the shards wrote, merged in shard order, one mutant a line; a list not written is empty."""
    lists: dict[str, list[str]] = {name: [] for name in LISTS}
    for shard in shards:
        for name in LISTS:
            path = shard / "mutants.out" / f"{name}.txt"
            if path.exists():
                lists[name] += path.read_text().splitlines()
    return lists


def scope(shards: int) -> list[str]:
    """The prefix that runs a shard in a user scope holding its part of the memory cap, with no swap; a process the
    kernel kills there for memory leaves the rest of the shard running."""
    return [
        "systemd-run",
        "--user",
        "--scope",
        "--quiet",
        "--property",
        f"MemoryMax={MEMORY // shards}",
        "--property",
        "MemorySwapMax=0",
        "--property",
        "OOMPolicy=continue",
        "--",
    ]


def scopes_available() -> bool:
    """Whether `systemd-run --user` can start a scope here."""
    try:
        probe = subprocess.run([*scope(1), "true"], stdin=subprocess.DEVNULL, capture_output=True, check=False)
    except OSError:
        return False
    return probe.returncode == 0


# Runs a program as the first process of a new PID namespace, as root of a user namespace mapped to the caller, with its
# own /proc; killing `unshare` kills the program and so the whole namespace.
NAMESPACE = ("unshare", "--user", "--map-root-user", "--pid", "--fork", "--mount-proc", "--kill-child")


def namespaces_available() -> bool:
    """Whether an unprivileged PID namespace can be made here."""
    try:
        probe = subprocess.run([*NAMESPACE, "true"], stdin=subprocess.DEVNULL, capture_output=True, check=False)
    except OSError:
        return False
    return probe.returncode == 0


def host(workspace: Path) -> str | None:
    """The target triple the workspace's rustc builds for by default, or None when rustc cannot say."""
    try:
        version = subprocess.run(["rustc", "-vV"], cwd=workspace, capture_output=True, text=True, check=False)
    except OSError:
        return None
    if version.returncode != 0:
        return None
    for line in version.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    return None


def contained(environment: dict[str, str], triple: str) -> dict[str, str]:
    """`environment` with Cargo's test runner for `triple` running each test binary in its own PID namespace, around
    any runner the environment already names."""
    name = "CARGO_TARGET_" + triple.upper().replace("-", "_").replace(".", "_") + "_RUNNER"
    runner = " ".join(NAMESPACE)
    if environment.get(name):
        runner = f"{runner} {environment[name]}"
    return dict(environment, **{name: runner})


def command(
    options: argparse.Namespace, diff_file: Path | None, shard: int, output: Path, capped: bool, cpus: int
) -> list[str]:
    """The command for one shard, with the diff file in diff mode: cargo-mutants in a capped scope when `capped`, with
    its share of `cpus` jobserver tasks."""
    selected = ["--in-diff", str(diff_file), "--workspace"] if diff_file else ["--package", options.package]
    return [
        *(scope(options.shards) if capped else []),
        "cargo",
        "mutants",
        *selected,
        "--in-place",
        "--shard",
        f"{shard}/{options.shards}",
        "--output",
        str(output),
        "--jobserver-tasks",
        str(max(1, cpus // options.shards)),
        *profile(options.passed),
        f"--cargo-test-arg={NO_DOCTESTS}",
        *(f"--cargo-test-arg={argument}" for argument in FAIL_FAST),
        *options.passed,
    ]


def names_profile(passed: list[str]) -> bool:
    """Whether the options `passed` to cargo-mutants select a profile. An option that takes a hyphenated value consumes
    the argument after it, even `--`; a `--` no option consumes ends cargo-mutants' options, and what follows is cargo
    test's."""
    options = iter(passed)
    for option in options:
        if option == "--":
            return False
        if option == "--profile" or option.startswith("--profile="):
            return True
        if option in HYPHEN_VALUES:
            next(options, None)
    return False


def profile(passed: list[str]) -> list[str]:
    """cargo-mutants' options that define the tool's profile for its build, check and test calls and build with it, or
    none when the options `passed` to cargo-mutants select a profile of their own."""
    if names_profile(passed):
        return []
    return [
        "--cargo-arg=--config=unstable.profile-rustflags=true",
        *(f"--cargo-arg=--config=profile.{PROFILE}.{setting}" for setting in SETTINGS),
        "--profile",
        PROFILE,
    ]


def copy_repository(root: Path, files: list[str], destination: Path) -> None:
    """Copies the named files of the repository at `root` into `destination`, keeping their places."""
    for name in files:
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / name, target, follow_symlinks=False)


def stop(runs: list[subprocess.Popen[bytes]]) -> None:
    """Stops every shard still running, with the processes it started, and waits for each."""
    for run in runs:
        if run.poll() is None:
            os.killpg(run.pid, signal.SIGTERM)
    for run in runs:
        run.wait()


def git(arguments: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *arguments], cwd=cwd, capture_output=True, text=True, check=False)


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--workspace",
        type=Path,
        default=Path(),
        help="the folder holding the workspace Cargo.toml (default: the current folder)",
    )
    scope = parser.add_mutually_exclusive_group(required=True)
    scope.add_argument("--base", help="mutate only the Rust changed since this revision")
    scope.add_argument("--package", help="mutate every function in this crate")
    parser.add_argument("--out", type=Path, required=True, help="the report folder, outside the repository")
    parser.add_argument("--shards", type=int, default=2, help="copies tested at once (default 2)")
    parser.add_argument("passed", nargs="*", help="options for cargo-mutants, after `--`")
    options = parser.parse_args(argv)
    if options.shards < 1:
        parser.error("--shards must be at least 1")
    return options


def main(argv: list[str] | None = None) -> int:
    options = arguments(argv)
    workspace = options.workspace.resolve()
    work = options.out.resolve() / OWNED
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    top = git(["rev-parse", "--show-toplevel"], workspace)
    if top.returncode != 0:
        print(f"rust_mutants: {workspace} is not in a Git repository: {top.stderr.strip()}", file=sys.stderr)
        return 1
    root = Path(top.stdout.strip())
    diff_file = None
    if options.base:
        diff = git(
            [
                "diff",
                "--relative",
                "--no-color",
                "--no-ext-diff",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                options.base,
                "--",
                ".",
            ],
            workspace,
        )
        if diff.returncode != 0:
            print(f"rust_mutants: git diff failed: {diff.stderr.strip()}", file=sys.stderr)
            return 1
        changed = changed_rust(diff.stdout)
        if not changed:
            print(f"rust_mutants: no Rust changed since {options.base}")
            return 0
        diff_file = work / "change.diff"
        diff_file.write_text(diff.stdout)
        print(f"rust_mutants: {len(changed)} Rust files changed since {options.base}", flush=True)
    listed = git(["ls-files", "-z", "--cached", "--others", "--exclude-standard"], root)
    files = [name for name in listed.stdout.split("\0") if name and (root / name).exists()]
    relative = workspace.relative_to(root)
    shards = [work / f"shard-{index}" for index in range(options.shards)]
    capped = scopes_available()
    if not capped:
        print("rust_mutants: systemd-run --user cannot start a scope here, so the shards run without a memory cap")
    triple = host(workspace)
    if triple is None or not namespaces_available():
        triple = None
        print("rust_mutants: a PID namespace cannot be made here, so the test binaries run uncontained")
    cpus = os.cpu_count() or 1
    runs: list[subprocess.Popen[bytes]] = []
    codes: list[int] = []
    try:
        for index, shard in enumerate(shards):
            copy = work / f"copy-{index}"
            copy_repository(root, files, copy)
            environment = dict(os.environ, CARGO_TARGET_DIR=str(copy / relative / "target"))
            if triple is not None:
                environment = contained(environment, triple)
            runs.append(
                subprocess.Popen(
                    command(options, diff_file, index, shard, capped, cpus),
                    cwd=copy / relative,
                    env=environment,
                    start_new_session=True,
                )
            )
        codes = [run.wait() for run in runs]
    except (OSError, KeyboardInterrupt) as error:
        stop(runs)
        print(f"rust_mutants: stopped before every shard finished: {error!r}", file=sys.stderr)
        return 1
    finally:
        for index in range(options.shards):
            shutil.rmtree(work / f"copy-{index}", ignore_errors=True)
    failed = [index for index, code in enumerate(codes) if code not in FINISHED]
    if failed:
        for index in failed:
            print(
                f"rust_mutants: shard {index} stopped with exit code {codes[index]}; its log is under "
                f"{shards[index] / 'mutants.out'}",
                file=sys.stderr,
            )
        return 1
    lists = outcome_lists(shards)
    for mutant in lists["missed"]:
        print(f"missed, to judge: {mutant}")
    for mutant in lists["timeout"]:
        print(f"timed out, to diagnose: {mutant}")
    print(" ".join(f"{name}: {len(lists[name])}" for name in LISTS))
    if diff_file and not any(lists.values()):
        print("rust_mutants: the changed Rust holds no function cargo-mutants can mutate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
