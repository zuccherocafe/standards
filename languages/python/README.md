# The Python standard

This standard governs Python in zucchero✿café projects. It applies [general.md](../../general.md) to Python and adds Python-specific rules. [review.md](../../review.md) turns both standards into review questions. [templates/python/](../../templates/python/) contains the linter and type checker configuration.

Python in these projects is tooling: reports, check helpers, small command-line tools, and scripts that run inside a host program such as Blender. It is never a shipped service. The rules keep that code as typed and as reviewable as the rest, without the packaging a library needs. Python's annotations are not enforced at run time, so the type checker is the compiler in `general.md`'s sense: code that it rejects does not land.

References are the [typing specification](https://typing.python.org/en/latest/spec/), the [Python standard library documentation](https://docs.python.org/3/library/), [PEP 8](https://peps.python.org/pep-0008/), [Ruff's rules](https://docs.astral.sh/ruff/rules/), and [Pyrefly's documentation](https://pyrefly.org/en/docs/). Examples are short and invented.

Each rule ends with a Checked by entry naming the type checker, the linter, the formatter, or review. Where a tool checks only part of a rule, the entry names that part.

## Toolchain

### One interpreter, one checker, one formatter

The development shell pins the Python interpreter, [Ruff](https://docs.astral.sh/ruff/) and [Pyrefly](https://pyrefly.org/), each by exact version, through the flake's lock or its own hash ([Toolchain](../../general.md#toolchain)). Ruff formats and lints. Pyrefly checks types with its `strict` preset. Both read the template's configuration. Ruff's rules are selected as explicit families and single rules, never by default, since a Ruff release can change its defaults, and an upgrade reviews what each selected family now contains. A rule Ruff marks as preview is enabled by name with `explicit-preview-rules`, so no other preview rule comes with it. The checks run `ruff format --check`, `ruff check`, and `pyrefly check` over every Python file the project tracks. Pyrefly is given those files as arguments, as in `git ls-files -z '*.py' | xargs -0 pyrefly check`, and no `--config`, so each file is checked under its nearest configuration; a bare `pyrefly check` uses only the root one. Lines are at most 120 columns, as in the other languages. The interpreter is the latest stable Python, and a project moves to each new minor release soon after it ships, in one change that also moves the target version in both configurations, since small yearly moves keep code current and cost less than skipping several. A script that runs inside a host targets the Python the host bundles instead: Ruff's `per-file-target-version` sets it for the host's folder, and that folder has its own `pyrefly.toml` with the host's version, which Pyrefly applies when given the files as above.

Scripts run as `python3 -I <script>` from the shell, so neither the current folder nor a user's site packages can shadow a module. Code uses the standard library only. A script that runs inside a host program may also import that program's modules, such as Blender's `bpy`, and extensions the project pins for that host, such as a Blender extension installed by the project's own setup. It targets the Python the host bundles.

**Checked by:** the formatter, the linter, and the type checker in the project's checks; review for imports outside the standard library and the host.

### Imports between a project's own files

These scripts are not packages. `-I` leaves the script's own folder off the import path, so a script that imports a neighbouring module adds that folder once, from `Path(__file__).resolve().parent`, before its other imports. That line is the only statement besides definitions allowed to run at import time. The Pyrefly configuration lists the same folders in `search-path`, and any host's type stubs, so the checker resolves what the interpreter does.

**Checked by:** the type checker, which reports an unresolved import; review.

### Exceptions to a check

A `# noqa: <rule>` or `# pyrefly: ignore[<error>]` comment covers one rule on one line and gives its reason after it. A file-wide or configuration-wide exception needs the same reason in the configuration and the maintainer's approval. An exception never covers a whole rule family.

**Checked by:** the linter's `RUF100` and the checker's `unused-ignore` for stale exceptions; review for reasons.

## Types and records

### Typed everything

Every function's parameters and return value are annotated, since an inferred return type changes silently when the body does. A value from outside the program, such as parsed JSON, command-line text, or a subprocess's output, is typed as `object` until a parser narrows it. `Any` appears only where a host module ships no types, and a typed wrapper around that module keeps it out of the rest of the code. Use the modern forms: `list[str]`, `X | None`, and `type` aliases.

**Checked by:** the type checker for parameter types and many implicit `Any` values; the linter's `ANN` return-type rules; review for where `Any` appears and for library results converted to `object` at a boundary.

### Records

A value with named fields passed between functions is a dataclass, never a `dict` of mixed values or a tuple whose positions carry meaning, since only the dataclass lets the type checker see each field. A value record is `@dataclass(frozen=True, slots=True)`: freezing stops a caller changing a value another part of the program still holds, and slots refuse a misspelt attribute. State with a real lifecycle, such as a running worker or a running total, is an ordinary class or a mutable dataclass. A record with an invariant checks it in `__post_init__`, so every construction checks it, and holds collections that cannot be changed to break it, such as a `tuple` rather than a `list`. A `dict` is a map from keys to values of one type, such as `dict[str, Path]`.

```python
@dataclass(frozen=True, slots=True)
class Timing:
    name: str
    seconds: float

    def __post_init__(self) -> None:
        if not self.seconds >= 0:
            raise ValueError(f"{self.name}: a timing of {self.seconds} s is not a duration")
```

**Checked by:** the type checker for field types; review for dictionaries carrying records and for invariants checked on every construction.

### Closed sets

A closed set is an `Enum`, or a `StrEnum` when its values are written to a file, never a string compared in several places. A `match` over one ends with `case _: assert_never(value)`, so adding a member fails the type check wherever a decision is missing.

**Checked by:** the type checker for `assert_never`; review for strings standing for a closed set.

## Parsing at boundaries

### Parse once

Text from a file, an argument, an environment variable, or a subprocess is parsed into records where it is read, and the rest of the program takes the records. A refusal names the file and, where it has one, the line and field. Nothing past the parser checks the shape again.

**Checked by:** review.

## Errors and the entry point

### One entry point

A standalone script's work is functions over values. Its one entry point is `main(argv: list[str] | None = None) -> int`, which parses arguments with `argparse`, reads inputs, calls the functions, writes outputs, and returns the exit code. `if __name__ == "__main__": sys.exit(main())` is the file's last statement. Nothing runs at import time except definitions and the import-path line above, so its tests can import the script without running it.

**Checked by:** review.

### Errors are raised, and reported once

A function that cannot do its job raises an exception carrying what the caller needs to report it. A standard exception, such as `ValueError` with a message naming the input, is enough when the caller handles it like any other failure. A custom exception exists only when a caller must tell this failure apart or needs structured context from it. `main` catches the exceptions it expects, prints one message naming what failed and where, and returns a non-zero code. Nothing catches `Exception` to carry on, and nothing prints an error and continues. `assert` states an invariant the code guarantees, never a check on input, since `python3 -O` removes it.

**Checked by:** the linter's `BLE001` for broad catches; review for where exceptions are reported and for `assert` on input.

### Processes and files

A subprocess takes an argument list and never `shell=True`, and its failure is either raised with `check=True` or handled from its return code. Paths are `pathlib.Path`. A text file is opened with an explicit encoding, since the default depends on the machine. A binary file stays binary. A tool writes only to the folder its caller names and to the project's ignored state folders ([Toolchain](../../general.md#toolchain)).

**Checked by:** the linter's `S602` for `shell=True`, `PLW1510` for an explicit `check`, `PTH` for paths, and `PLW1514` for encodings; review for return codes handled after `check=False` and for where a tool writes.

## Tests

### Tests beside the tool

A tool's tests live in `test_<tool>.py` beside it and use `unittest`, the names Python's own test discovery and other projects' readers look for. `python3 -I -m unittest discover -s <folder>` runs every test in a folder with no configuration, and adds the folder to the import path, so a test file needs no import-path line of its own. Expected values are written by hand from the input ([Checks](../../general.md#checks)). A standalone tool's tests include at least one test through `main` with a made-up input, so the common path is tested. A test replaces only a process, clock, or network boundary, by patching the module's own `subprocess` call, never by adding a parameter or hook to production code for the test. A test writes only inside a temporary folder it removes.

**Checked by:** review.

### Host scripts

Code that needs a host program, such as Blender, keeps its decisions in a module that does not import the host, and its tests cover that module. The script that imports the host only reads, calls the module, and writes. The host script itself runs where the project's real caller runs it, such as the converter's own scenario, and a new check that starts the host needs the maintainer's approval like any other wider check.

**Checked by:** review.

## Idiom

Write Python the way the language and its standard library intend, since that is what other Python readers recognise and what the tools understand. The sources are the [Python tutorial](https://docs.python.org/3/tutorial/) and library reference, [PEP 8](https://peps.python.org/pep-0008/), Luciano Ramalho's *Fluent Python* (2nd edition), Brett Slatkin's *Effective Python*, and Raymond Hettinger's talk *Transforming Code into Beautiful, Idiomatic Python*. Ruff's rule pages explain each idiom its selected rules enforce. Advice that disagrees with these sources needs a reason in the change.

- Loop over the items: `for item in items`, `enumerate()` for an index, and `zip(strict=True)` for parallel sequences, never `range(len(...))`.
- Build a list or dict with a comprehension when each item is a plain transformation or filter, and with a loop when there is more than one condition or a side effect.
- Hold every file, lock, and temporary folder in a `with` block, so it is released when an exception leaves the block.
- Try the operation and catch its specific exception rather than checking first, for example opening a file and catching `FileNotFoundError` rather than testing `exists()`, which another process can make false in between.
- Test emptiness with truthiness, as in `if items:`, and absence with `is None`.
- Use the standard library before writing a helper: `collections`, `itertools`, `functools`, `dataclasses`, `enum`, `json`, `argparse`, and `subprocess`.
- Use a generator when a sequence is read once, so it is never held in memory whole.
- Format text with f-strings.

**Checked by:** the linter's `UP`, `SIM`, `C4`, `B`, `PERF`, `FURB`, `RET`, and `PTH` rules for the idioms they cover; review for the rest.

## Names and layout

Modules, functions, and variables are `snake_case`, classes `CapWords`, and constants `UPPER_SNAKE`, as PEP 8 says, so names look the same as in the standard library. A name says what the value is, with its unit where it has one (`seconds`, `metres`). A module starts with a docstring saying what it does, how to run it, and what it never does. Imports are at the top of the file and sorted. A leading `_` marks a name as private only in a module that others import, where it tells an importer not to use it.

**Checked by:** the linter's `N` rules for names and `I` rules for import order, which `ruff check --select I --fix` applies; review.
