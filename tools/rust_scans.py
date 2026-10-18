"""The Rust standard's three report-only scans over each workspace crate's `src/`: H1 file, process, environment and
clock tokens in the crates named as cores, H2 unit-suffixed primitive fields and parameters outside kernels and tests,
H3 a code in an error's message. Prints each finding, as a path relative to the current folder, and a count per scan,
and fails nothing: a scan is a review aid until a project's maintainer approves it as a check (general.md).

Run from the repository root; a project names its cores and kernels, which only it knows:

  rust-scans --workspace rust --core <crate> --kernel <crate>=<module>[,<module>...]

A kernel is a module path under the crate's `src/` without `.rs`, covering its file and its directory: a private
numerical module whose functions may take primitives (languages/rust/design.md, Quantities). Needs `cargo` on PATH.
"""

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

SUFFIXES = ("metres", "seconds", "degrees", "radians", "hz")
PRIMITIVES = ("f32", "f64", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize")

H1_TOKEN = re.compile(r"\bstd::(?:fs|process|env)\b|\bInstant\b|\bSystemTime\b")
# A field or parameter: a name with a unit suffix, then a bare or borrowed primitive, not a `let` and not a path.
H2_TYPED = re.compile(
    rf"(?<![\w.])(?P<let>let\s+(?:mut\s+)?)?(?P<name>[a-z_][a-z0-9_]*_(?:{'|'.join(SUFFIXES)}))\s*:\s*"
    rf"(?:&\s*(?:'\w+\s+)?(?:mut\s+)?)?(?P<type>{'|'.join(PRIMITIVES)})\b(?!\s*::)"
)
H3_CODE = re.compile(r"\{code[}:]|[(,]\s*code\s*=")
CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]")
ATTRIBUTE = re.compile(r"^\s*#\[")
MOD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*(;|\{)")


def code(line: str) -> str:
    """The line without its `//` comment; a `//` inside a string literal is rare enough here to cut too."""
    return line.split("//", 1)[0]


def module_dir(source: Path) -> Path:
    """The directory a file's `mod name;` children live in."""
    return source.parent if source.name in ("lib.rs", "main.rs") else source.with_suffix("")


def closing(lines: list[str], start: int) -> int:
    """The index of the line closing the brace opened on `start`, counting braces outside comments."""
    depth = 0
    for index in range(start, len(lines)):
        depth += code(lines[index]).count("{") - code(lines[index]).count("}")
        if depth <= 0:
            return index
    return len(lines) - 1


def test_parts(source: Path, lines: list[str]) -> tuple[list[Path], list[range]]:
    """The child module files and the inline line ranges a file declares under `#[cfg(test)]`."""
    children: list[Path] = []
    spans: list[range] = []
    for index, line in enumerate(lines):
        if not CFG_TEST.match(line):
            continue
        following = index + 1
        while following < len(lines) and (ATTRIBUTE.match(lines[following]) or not lines[following].strip()):
            following += 1
        declared = MOD.match(lines[following]) if following < len(lines) else None
        if declared is None:
            continue
        if declared.group(2) == ";":
            children.append(module_dir(source) / declared.group(1))
        else:
            spans.append(range(index, closing(lines, following) + 1))
    return children, spans


def within(path: Path, prefixes: list[Path]) -> bool:
    """Whether a source is one of the modules named by `prefixes`, as its file or under its directory."""
    return any(path.with_suffix("") == prefix or prefix in path.parents for prefix in prefixes)


def members(workspace: Path) -> list[tuple[str, Path]]:
    """The workspace's member packages as Cargo lists them (a root package, exclusions and path members included), by
    name and folder, in folder order."""
    done = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=workspace,
        capture_output=True,
        text=True,
        check=False,
    )
    if done.returncode != 0:
        raise OSError(f"cargo metadata failed in {workspace}: {done.stderr.strip()}")
    metadata = json.loads(done.stdout)
    chosen = set(metadata["workspace_members"])
    found = [
        (package["name"], Path(package["manifest_path"]).parent)
        for package in metadata["packages"]
        if package["id"] in chosen
    ]
    return sorted(found, key=lambda member: member[1])


def scan(
    root: Path, workspace: Path, cores: frozenset[str], kernels_by_crate: dict[str, tuple[str, ...]]
) -> dict[str, list[str]]:
    """Every finding of each scan, as `<path>:<line>: <what>` relative to `root`."""
    findings: dict[str, list[str]] = {"H1": [], "H2": [], "H3": []}
    for name, crate in members(workspace):
        src = crate / "src"
        sources = sorted(src.rglob("*.rs"))
        texts = {source: source.read_text(encoding="utf-8").splitlines() for source in sources}
        tests: list[Path] = []
        spans: dict[Path, list[range]] = {}
        for source, lines in texts.items():
            children, inline = test_parts(source, lines)
            tests.extend(children)
            spans[source] = inline
        kernels = [src / kernel for kernel in kernels_by_crate.get(name, ())]
        for source, lines in texts.items():
            relative = source.relative_to(root)
            parts = source.relative_to(src).parts
            test_file = (
                "tests" in parts[:-1]
                or source.name == "tests.rs"
                or source.name.endswith("_tests.rs")
                or within(source, tests)
            )
            kernel = within(source, kernels)
            attribute: list[str] = []
            attribute_line = 0
            for number, line in enumerate(lines, start=1):
                text = code(line)
                in_test = test_file or any(number - 1 in span for span in spans[source])
                if name in cores:
                    for token in H1_TOKEN.findall(text):
                        note = " (test code)" if in_test else ""
                        findings["H1"].append(f"{relative}:{number}: {token} in {name}{note}")
                if not (in_test or kernel):
                    for typed in H2_TYPED.finditer(text):
                        if not typed.group("let"):
                            findings["H2"].append(f"{relative}:{number}: {typed.group('name')}: {typed.group('type')}")
                if not attribute and "#[error(" in text:
                    attribute_line = number
                    text = text[text.index("#[error(") :]
                if attribute_line:
                    attribute.append(text)
                    joined = "\n".join(attribute)
                    if joined.count("(") <= joined.count(")"):
                        if H3_CODE.search(joined):
                            findings["H3"].append(f"{relative}:{attribute_line}: a code in #[error(...)]")
                        attribute = []
                        attribute_line = 0
    return findings


def kernel(text: str) -> tuple[str, tuple[str, ...]]:
    """A `--kernel` value, `<crate>=<module>[,<module>...]`."""
    crate, _, modules = text.partition("=")
    if not crate or not modules:
        raise argparse.ArgumentTypeError(f"{text!r} is not <crate>=<module>[,<module>...]")
    return crate, tuple(modules.split(","))


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--workspace",
        type=Path,
        default=Path(),
        help="the folder holding the workspace Cargo.toml (default: the current folder)",
    )
    parser.add_argument("--core", action="append", default=[], help="a crate H1 scans; repeat for each")
    parser.add_argument(
        "--kernel",
        action="append",
        type=kernel,
        default=[],
        help="<crate>=<module>[,<module>...]: modules H2 leaves out; repeat for each crate",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    options = arguments(argv)
    started = time.perf_counter()
    root = Path.cwd().resolve()
    kernels: dict[str, tuple[str, ...]] = {}
    for crate, modules in options.kernel:
        kernels[crate] = kernels.get(crate, ()) + modules
    try:
        findings = scan(root, (root / options.workspace).resolve(), frozenset(options.core), kernels)
    except (OSError, KeyError, IndexError, AttributeError, UnicodeDecodeError, ValueError) as error:
        print(f"rust_scans: cannot read the tree: {error}", file=sys.stderr)
        return 1
    for scan_name, lines in findings.items():
        for finding in lines:
            print(f"{scan_name} {finding}")
    for scan_name, lines in findings.items():
        print(f"{scan_name}: {len(lines)}")
    print(f"seconds: {time.perf_counter() - started:.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
