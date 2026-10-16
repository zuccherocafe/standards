"""Report what each Rust library crate's public API promises before and after a change, so a reviewer reads the
promises before the implementation. It fails nothing, and exits non-zero only when a revision cannot be archived or
listed.

Each revision is archived into scratch, its workspace's library crates are read from `cargo metadata`, and each is
documented as rustdoc JSON by plain Cargo in that revision's workspace folder, on the nightly its toolchain file pins
(rustdoc's JSON output is unstable). `cargo-public-api`, pinned below, renders each crate's items as signature text and
pairs an item whose signature changed. Listings leave out blanket and auto-trait impls (`-ss`), which follow from the
types alone; derived impls stay, since deriving or dropping one changes what the crate promises.

  python3 tools/api.py --before <revision> [--after <revision>] --out <new folder outside the repository>
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import TypeVar

sys.dont_write_bytecode = True

T = TypeVar('T')
# The repository root: this file lives in its tools/.
ROOT = Path(__file__).resolve().parents[1]

# project-specific: the workspace's folder under the repository root ('.' when Cargo.toml is at the root), and the
# cargo-public-api release that reads the pinned nightly's rustdoc JSON, which moves with the nightly; 0.52.0 reads
# the template's nightly-2026-09-24.
WORKSPACE = '.'
CARGO_PUBLIC_API = '0.52.0'


def unbuildable(workspace: Path) -> str | None:
    """Why a revision's workspace cannot be documented with the toolchain this shell runs; none when it can.
    project-specific: a project that records its nightly's `rustc --version` compares it here."""
    return None

INSTALL = f'cargo install cargo-public-api --version {CARGO_PUBLIC_API} --locked'
SIMPLIFIED = '-ss'
LIBRARY_KINDS = {'lib', 'rlib', 'dylib', 'cdylib', 'staticlib', 'proc-macro'}
REMOVED, CHANGED, ADDED = 'Removed items from the public API', 'Changed items in the public API', \
    'Added items to the public API'


class Unfinished(Exception):
    """A step without which no report can be written."""


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--before', required=True, help='revision of this repository the report starts from')
    parser.add_argument('--after', default='HEAD', help='revision of this repository the report ends at')
    parser.add_argument('--out', required=True, type=Path, help='a new folder for api.md, api.log and the listings')
    parser.add_argument('--jobs', type=int, help='Cargo jobs')
    parser.add_argument('--scratch', type=Path, help='where revisions are archived; a temporary folder if none')
    options = parser.parse_args(argv)
    if options.out.exists():
        parser.error('--out must be a new folder')
    return options


# ---------------------------------------------------------------- diffs

@dataclass(frozen=True)
class Diff:
    """One crate's items that only the before side has, that changed signature (before, after), and that only the
    after side has, each as `cargo-public-api` renders it."""
    removed: tuple[str, ...] = ()
    changed: tuple[tuple[str, str], ...] = ()
    added: tuple[str, ...] = ()

    def empty(self) -> bool:
        return not (self.removed or self.changed or self.added)


def distinct(lines: Iterable[T]) -> list[T]:
    """`lines` without repeats, in their order: `cargo-public-api` prints some items twice, such as the methods of a
    trait impl, and an item is one promise however often it is printed."""
    return list(dict.fromkeys(lines))


def parsed(text: str) -> Diff:
    """`cargo-public-api diff`'s three sections; a section is `(none)` or one item a line, `-` for the before side and
    `+` for the after, and a changed item is its `-` line followed by its `+` line."""
    sections: dict[str, list[str]] = {}
    heading = None
    for line in text.splitlines():
        if line in (REMOVED, CHANGED, ADDED):
            heading = line
            sections[heading] = []
        elif heading is not None and line[:1] in ('-', '+'):
            sections[heading].append(line)
    if set(sections) != {REMOVED, CHANGED, ADDED}:
        raise ValueError(f'expected the sections {REMOVED!r}, {CHANGED!r} and {ADDED!r}')
    changes = sections[CHANGED]
    pairs = list(zip(changes[0::2], changes[1::2]))
    if len(changes) % 2 or any(old[0] != '-' or new[0] != '+' for old, new in pairs):
        raise ValueError('a changed item is a `-` line followed by its `+` line')
    return Diff(tuple(distinct(line[1:] for line in sections[REMOVED])),
                tuple(distinct((old[1:], new[1:]) for old, new in pairs)),
                tuple(distinct(line[1:] for line in sections[ADDED])))


def one_sided(before: list[str] | None, after: list[str] | None) -> Diff:
    """A crate only one side has: every item it lists is removed or added."""
    return Diff(removed=tuple(before or ()), added=tuple(after or ()))


# ---------------------------------------------------------------- report

@dataclass
class Crate:
    """One library crate's listing on each side, none where that side has no such crate or could not document it,
    its diff once both sides are listed, and why a side has no listing when it should."""
    name: str
    before: list[str] | None = None
    after: list[str] | None = None
    diff: Diff | None = None
    missing: list[str] = field(default_factory=list)


def summary(crates: list[Crate]) -> str:
    rows = ['| Crate | Items before | Items after | Removed | Changed | Added |', '|---|---|---|---|---|---|']
    for crate in crates:
        counts = ['-', '-', '-'] if crate.diff is None else \
            [str(len(crate.diff.removed)), str(len(crate.diff.changed)), str(len(crate.diff.added))]
        sizes = ['-' if listing is None else str(len(listing)) for listing in (crate.before, crate.after)]
        rows.append(f'| {crate.name} | ' + ' | '.join(sizes + counts) + ' |')
    return '\n'.join(rows)


def section(crate: Crate) -> str:
    if crate.diff is None or crate.diff.empty():
        return ''
    parts = [f'## {crate.name}']
    for title, lines in (('Removed', [f'- {item}' for item in crate.diff.removed]),
                         ('Changed', [f'{sign} {item}' for pair in crate.diff.changed
                                      for sign, item in zip('-+', pair)]),
                         ('Added', [f'+ {item}' for item in crate.diff.added])):
        if lines:
            parts.append(f'### {title}\n\n```diff\n' + '\n'.join(lines) + '\n```')
    return '\n\n'.join(parts)


def report(before: str, after: str, crates: list[Crate]) -> str:
    notes = [f'- {crate.name}: {reason}' for crate in crates for reason in crate.missing]
    parts = [f'# Public API from {before} to {after}',
             'Every library crate\'s public items, blanket and auto-trait impls left out; `listings/<side>/` holds '
             'each side\'s full listing. A changed item is one whose signature changed; renamed or moved items are '
             'removed and added.']
    if notes:
        parts.append('Incomplete: a crate whose listing is missing below has no diff.\n\n' + '\n'.join(notes))
    parts.append(summary(crates))
    parts += [text for text in map(section, crates) if text]
    return '\n\n'.join(parts) + '\n'


# ---------------------------------------------------------------- revisions

@dataclass(frozen=True)
class Tree:
    """An archived revision and the library crates of its workspace, by package and target."""
    side: str
    revision: str
    root: Path
    libraries: dict[str, str]


def archived(side: str, revision: str, scratch: Path, log: Path) -> Tree:
    root = scratch/side
    root.mkdir(parents=True)
    archive = subprocess.Popen(['git', 'archive', '--format=tar', revision], cwd=ROOT, stdout=subprocess.PIPE)
    subprocess.run(['tar', '-xf', '-', '-C', str(root)], stdin=archive.stdout, check=True)
    if archive.wait() != 0:
        raise Unfinished(f'{side}: git archive {revision} failed')
    wrong = unbuildable(root/WORKSPACE)
    if wrong is not None:
        raise Unfinished(f'{side}: {revision}: {wrong}')
    metadata = run(['cargo', 'metadata', '--no-deps', '--format-version', '1', '--locked'],
                   root/WORKSPACE, log, f'{side} metadata')
    if metadata is None:
        raise Unfinished(f'{side}: cargo metadata failed in {revision}; see api.log')
    return Tree(side, revision, root, libraries(metadata))


def libraries(metadata: str) -> dict[str, str]:
    """Each workspace package with a library target, by package name, with that target's name; a binary-only package
    promises nothing to other crates."""
    found = {}
    for package in json.loads(metadata)['packages']:
        for target in package['targets']:
            if LIBRARY_KINDS & set(target['kind']):
                found[package['name']] = target['name']
    return dict(sorted(found.items()))


def documented(tree: Tree, package: str, target: str, destination: Path, environment: dict, jobs: list[str],
               log: Path) -> Path | None:
    """The package's rustdoc JSON, copied out of the side's own target folder to `destination`; none where rustdoc
    failed. Both sides' trees name their workspace crates' outputs alike and are archived with older times than any
    output, so a shared target folder would take one side's crates for the other's."""
    command = ['nice', '-n', '19', 'cargo', 'rustdoc', '--locked', *jobs, '-p', package, '--lib',
               '-Z', 'unstable-options', '--output-format', 'json']
    if run(command, tree.root/WORKSPACE, log, f'{tree.side} rustdoc {package}', environment) is None:
        return None
    shutil.copyfile(Path(environment['CARGO_TARGET_DIR'])/'doc'/f'{target.replace("-", "_")}.json', destination)
    return destination


def run(command: list, cwd: Path, log: Path, name: str, environment: dict | None = None) -> str | None:
    """A step's standard output, its error output logged; none where it failed."""
    with log.open('a') as output:
        output.write(f'\n=== {name}: {" ".join(map(str, command))}\n')
        output.flush()
        done = subprocess.run(list(map(str, command)), cwd=cwd, env=environment, stdout=subprocess.PIPE,
                              stderr=output, text=True)
    return done.stdout if done.returncode == 0 else None


def tool() -> str:
    """The pinned `cargo-public-api`, refused by name when it is missing or another version."""
    binary = shutil.which('cargo-public-api')
    version = subprocess.run([binary, '--version'], capture_output=True, text=True).stdout.split() if binary else []
    if version[-1:] != [CARGO_PUBLIC_API]:
        found = 'is not installed' if binary is None else f'is {" ".join(version) or "unreadable"}'
        raise Unfinished(f'cargo-public-api {found}; this report needs {CARGO_PUBLIC_API}: {INSTALL}')
    return binary


def main(argv: list[str] | None = None) -> int:
    options = arguments(argv)
    started = time.perf_counter()
    try:
        binary = tool()
        options.out.mkdir(parents=True)
        log = options.out/'api.log'
        jobs = ['-j', str(options.jobs)] if options.jobs else []
        with tempfile.TemporaryDirectory(prefix='api-', dir=options.scratch) as temporary:
            scratch = Path(temporary)
            crates: dict[str, Crate] = {}
            documents: dict[str, dict[str, Path]] = {}
            for side, revision in (('before', options.before), ('after', options.after)):
                tree = archived(side, revision, scratch, log)
                environment = dict(os.environ) | {'CARGO_TARGET_DIR': str(scratch/f'{side}-target')}
                listings = options.out/'listings'/side
                listings.mkdir(parents=True)
                documents[side] = {}
                for package, target in tree.libraries.items():
                    crate = crates.setdefault(package, Crate(package))
                    document = documented(tree, package, target, scratch/f'{side}-{package}.json', environment,
                                          jobs, log)
                    listed = None if document is None else run(
                        [binary, SIMPLIFIED, '--color', 'never', '--rustdoc-json', document], scratch, log,
                        f'{side} list {package}')
                    if listed is None:
                        crate.missing.append(f'not listed on the {side} side; see api.log')
                        continue
                    items = distinct(listed.splitlines())
                    (listings/f'{package}.txt').write_text(''.join(f'{item}\n' for item in items))
                    setattr(crate, side, items)
                    documents[side][package] = document
            for crate in crates.values():
                if crate.missing:
                    continue
                pair = [documents[side].get(crate.name) for side in ('before', 'after')]
                if None in pair:
                    crate.diff = one_sided(crate.before, crate.after)
                    continue
                text = run([binary, SIMPLIFIED, '--color', 'never', 'diff', *pair], scratch, log,
                           f'diff {crate.name}')
                try:
                    crate.diff = None if text is None else parsed(text)
                except ValueError as error:
                    crate.missing.append(f'its diff could not be read: {error}')
                if text is None:
                    crate.missing.append('cargo-public-api could not diff it; see api.log')
    except Unfinished as error:
        print(f'API-REPORT-UNFINISHED: {error}', file=sys.stderr)
        return 1
    text = report(options.before, options.after, sorted(crates.values(), key=lambda crate: crate.name))
    (options.out/'api.md').write_text(text)
    print(text, end='')
    print(f'API-REPORT: {options.out/"api.md"} in {time.perf_counter() - started:.0f} s', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
