"""The Rust standard's mutation report: runs cargo-mutants over the Rust a change touched, or over a whole crate, and
lists each mutant the tests missed for the writer to judge, and each that timed out for the writer to diagnose. Fails
nothing: it is a review aid (rust.md, Mutation report).

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
this run's. Needs `git` and `cargo mutants` on PATH.
"""

import argparse
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

LISTS = ('caught', 'missed', 'timeout', 'unviable')
# cargo-mutants' exit codes for a finished run: all caught, some missed, some timed out.
FINISHED = (0, 2, 3)
# The one folder under `--out` this tool writes and empties.
OWNED = 'rust-mutants'


def changed_rust(diff: str) -> list[str]:
    """The Rust files a diff changes, as its `+++ b/` lines name them."""
    return [line[len('+++ b/'):] for line in diff.splitlines()
            if line.startswith('+++ b/') and line.endswith('.rs')]


def outcome_lists(shards: list[Path]) -> dict[str, list[str]]:
    """Each outcome list the shards wrote, merged in shard order, one mutant a line; a list not written is empty."""
    lists: dict[str, list[str]] = {name: [] for name in LISTS}
    for shard in shards:
        for name in LISTS:
            path = shard / 'mutants.out' / f'{name}.txt'
            if path.exists():
                lists[name] += path.read_text().splitlines()
    return lists


def command(options: argparse.Namespace, diff_file: Path | None, shard: int, output: Path) -> list[str]:
    """The cargo-mutants command for one shard, with the diff file in diff mode."""
    scope = ['--in-diff', str(diff_file), '--workspace'] if diff_file else ['--package', options.package]
    return ['cargo', 'mutants', *scope, '--in-place', '--shard', f'{shard}/{options.shards}', '--output', str(output),
            *options.passed]


def copy_repository(root: Path, files: list[str], destination: Path) -> None:
    """Copies the named files of the repository at `root` into `destination`, keeping their places."""
    for name in files:
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / name, target, follow_symlinks=False)


def stop(runs: list[subprocess.Popen]) -> None:
    """Stops every shard still running, with the processes it started, and waits for each."""
    for run in runs:
        if run.poll() is None:
            os.killpg(run.pid, signal.SIGTERM)
    for run in runs:
        run.wait()


def git(arguments: list[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(['git', *arguments], cwd=cwd, capture_output=True, text=True, check=False)


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--workspace', type=Path, default=Path('.'),
                        help='the folder holding the workspace Cargo.toml (default: the current folder)')
    scope = parser.add_mutually_exclusive_group(required=True)
    scope.add_argument('--base', help='mutate only the Rust changed since this revision')
    scope.add_argument('--package', help='mutate every function in this crate')
    parser.add_argument('--out', type=Path, required=True, help='the report folder, outside the repository')
    parser.add_argument('--shards', type=int, default=2, help='copies tested at once (default 2)')
    parser.add_argument('passed', nargs='*', help='options for cargo-mutants, after `--`')
    options = parser.parse_args(argv)
    if options.shards < 1:
        parser.error('--shards must be at least 1')
    return options


def main(argv: list[str] | None = None) -> int:
    options = arguments(argv)
    workspace = options.workspace.resolve()
    work = options.out.resolve() / OWNED
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    top = git(['rev-parse', '--show-toplevel'], workspace)
    if top.returncode != 0:
        print(f'rust_mutants: {workspace} is not in a Git repository: {top.stderr.strip()}', file=sys.stderr)
        return 1
    root = Path(top.stdout.strip())
    diff_file = None
    if options.base:
        diff = git(['diff', '--relative', '--no-color', '--no-ext-diff', '--src-prefix=a/', '--dst-prefix=b/',
                    options.base, '--', '.'], workspace)
        if diff.returncode != 0:
            print(f'rust_mutants: git diff failed: {diff.stderr.strip()}', file=sys.stderr)
            return 1
        changed = changed_rust(diff.stdout)
        if not changed:
            print(f'rust_mutants: no Rust changed since {options.base}')
            return 0
        diff_file = work / 'change.diff'
        diff_file.write_text(diff.stdout)
        print(f'rust_mutants: {len(changed)} Rust files changed since {options.base}', flush=True)
    listed = git(['ls-files', '-z', '--cached', '--others', '--exclude-standard'], root)
    files = [name for name in listed.stdout.split('\0') if name and (root / name).exists()]
    relative = workspace.relative_to(root)
    shards = [work / f'shard-{index}' for index in range(options.shards)]
    runs: list[subprocess.Popen] = []
    codes: list[int] = []
    try:
        for index, shard in enumerate(shards):
            copy = work / f'copy-{index}'
            copy_repository(root, files, copy)
            environment = dict(os.environ, CARGO_TARGET_DIR=str(copy / relative / 'target'))
            runs.append(subprocess.Popen(command(options, diff_file, index, shard), cwd=copy / relative,
                                         env=environment, start_new_session=True))
        codes = [run.wait() for run in runs]
    except (OSError, KeyboardInterrupt) as error:
        stop(runs)
        print(f'rust_mutants: stopped before every shard finished: {error!r}', file=sys.stderr)
        return 1
    finally:
        for index in range(options.shards):
            shutil.rmtree(work / f'copy-{index}', ignore_errors=True)
    failed = [index for index, code in enumerate(codes) if code not in FINISHED]
    if failed:
        for index in failed:
            print(f'rust_mutants: shard {index} stopped with exit code {codes[index]}; its log is under '
                  f'{shards[index] / "mutants.out"}', file=sys.stderr)
        return 1
    lists = outcome_lists(shards)
    for mutant in lists['missed']:
        print(f'missed, to judge: {mutant}')
    for mutant in lists['timeout']:
        print(f'timed out, to diagnose: {mutant}')
    print(' '.join(f'{name}: {len(lists[name])}' for name in LISTS))
    if diff_file and not any(lists.values()):
        print('rust_mutants: the changed Rust holds no function cargo-mutants can mutate')
    return 0


if __name__ == '__main__':
    sys.exit(main())
