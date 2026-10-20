# The Python standard

This standard governs Python in zucchero✿café projects. It applies [general.md](../../general.md) to Python and adds Python-specific rules. [review.md](../../review.md) turns both standards into review questions. [templates/python/](../../../templates/python/) contains the linter and type checker configuration.

Python in these projects is tooling: reports, check helpers, small command-line tools, and scripts that run inside a host program such as Blender. It is never a shipped service. The rules keep that code as typed and as reviewable as the rest, without the packaging a library needs. Python does not enforce annotations at run time, so the type checker is the compiler in `general.md`'s sense: code that it rejects does not land.

References are the [typing specification](https://typing.python.org/en/latest/spec/), the [Python standard library documentation](https://docs.python.org/3/library/), [PEP 8](https://peps.python.org/pep-0008/), [Ruff's rules](https://docs.astral.sh/ruff/rules/), and [Pyrefly's documentation](https://pyrefly.org/en/docs/). Examples are short and invented.

Each rule ends with a Checked by entry naming the type checker, the linter, the formatter, or review. Where a tool checks only part of a rule, the entry names that part. Each rule's text begins with its kind, as [How strictly a rule applies](../../general.md#how-strictly-a-rule-applies) defines.

## Toolchain

### One interpreter, one checker, one formatter

*Always.* The development shell pins the Python interpreter, [Ruff](https://docs.astral.sh/ruff/), and [Pyrefly](https://pyrefly.org/) by exact version through `flake.lock` or a fixed hash ([Toolchain](../../general.md#toolchain)). The interpreter is the latest stable Python, and a project moves to each new minor release soon after it ships, since small yearly moves keep code current and cost less than skipping several. The same change moves `target-version` in `ruff.toml` and `python-version` in `pyrefly.toml`. Ruff formats and lints, and Pyrefly checks types with its `strict` preset. Both tools read the template's configuration. Lines are at most 120 columns, as in the other languages. Select Ruff rules as explicit families and single rules rather than relying on Ruff's defaults, which can change between releases. On a Ruff upgrade, review what each selected family now contains. Enable any preview rule by name with `explicit-preview-rules` so no other preview rule comes with it. The checks run `ruff format --check`, `ruff check`, and `pyrefly check` over every tracked Python file. Pass the tracked files to Pyrefly without `--config`, as in `git ls-files -z '*.py' | xargs -0 pyrefly check`, so Pyrefly checks each file under its nearest configuration rather than only the root one.

Run scripts from the shell as `python3 -I <script>` so neither the current folder nor user site packages can shadow a module. Scripts use the standard library only. A script that runs inside a host program may also import that program's modules, such as Blender's `bpy`, and extensions the project pins for that host, such as a Blender extension installed by the project's own setup. It targets the Python version the host bundles: set `per-file-target-version` in `ruff.toml` for the host's folder, and give that folder its own `pyrefly.toml` with the host's `python-version`, which Pyrefly applies when given the files as arguments.

**Checked by:** the formatter, the linter, and the type checker in the project's checks; review for imports outside the standard library and the host.

### Imports between a project's own files

*Default.* These scripts are not packages. `-I` leaves the script's own folder off the import path, so a script that imports a neighbouring module adds that folder once, from `Path(__file__).resolve().parent`, before its other imports. That line is the only statement besides definitions allowed to run at import time. The Pyrefly configuration lists the same folders in `search-path`, and any host's type stubs, so the checker resolves what the interpreter does.

**Checked by:** the type checker, which reports an unresolved import; review.

### Exceptions to a check

*Always.* A `# noqa: <rule>` or `# pyrefly: ignore[<error>]` comment covers one rule on one line and gives its reason after it. A file-wide or configuration-wide exception needs the same reason in the configuration and the maintainer's approval. An exception never covers a whole rule family.

**Checked by:** the linter's `RUF100` and the checker's `unused-ignore` for stale exceptions; review for reasons.

## Types and records

### Typed everything

*Always.* Annotate every function's parameters and return value, since an inferred return type changes silently when the body changes. Type any value from outside the program, such as parsed JSON, command-line text, or subprocess output, as `object` until a parser narrows it. `Any` appears only where a host module ships no types, and a typed wrapper around that module keeps it out of the rest of the code. Use the modern forms: `list[str]`, `X | None`, and `type` aliases.

**Checked by:** the type checker for parameter types and many implicit `Any` values; the linter's `ANN` return-type rules; review for where `Any` appears and for library results converted to `object` at a boundary.

### Records

*Default.* A value with named fields passed between functions is a dataclass, never a `dict` of mixed values or a tuple whose positions carry meaning, since only the dataclass lets the type checker see each field. Mark a value record with `@dataclass(frozen=True, slots=True)`. `frozen=True` stops a caller from mutating a value that another part of the program still holds, and `slots=True` rejects misspelt attributes. State with a real lifecycle, such as a running worker or a running total, is an ordinary class or a mutable dataclass. A record with an invariant checks it in `__post_init__`, so every construction checks it, and holds collections that cannot be changed to break it, such as a `tuple` rather than a `list`. A `dict` is a map from keys to values of one type, such as `dict[str, Path]`.

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

*Default.* Represent a closed set as an `Enum`, or as a `StrEnum` when code writes its values to a file, never as a string compared in several places. A `match` over one ends with `case _: assert_never(value)`, so adding a member fails the type check wherever a decision is missing.

**Checked by:** the type checker for `assert_never`; review for strings standing for a closed set.

## Parsing at boundaries

### Parse once

*Always.* Parse text from a file, an argument, an environment variable, or a subprocess into records where you read it, and pass those records to the rest of the program ([Parse at the boundary](../../general.md#design)). A refusal names the file and, where it has one, the line and the field. Nothing past the parser checks the shape again.

**Checked by:** review.

## Errors and the entry point

### One entry point

*Default.* A standalone script's work is functions over values. Its one entry point is `main(argv: list[str] | None = None) -> int`, which parses arguments with `argparse`, reads inputs, calls the functions, writes outputs, and returns the exit code. `if __name__ == "__main__": sys.exit(main())` is the file's last statement. Nothing runs at import time except definitions and the import-path line above, so its tests can import the script without running it.

**Checked by:** review.

### Raise errors and report them once

*Default.* A function that cannot do its job raises an exception carrying what the caller needs to report it. A standard exception, such as `ValueError` with a message naming the input, is enough when the caller handles it like any other failure. A custom exception exists only when a caller must tell this failure apart or needs structured context from it. `main` catches the exceptions it expects, prints one message naming what failed and where, and returns a non-zero code. Nothing catches `Exception` to carry on, and nothing prints an error and continues. `assert` states an invariant the code guarantees, never a check on input, since `python3 -O` removes it.

**Checked by:** the linter's `BLE001` for broad catches; review for where exceptions are reported and for `assert` on input.

### Processes and files

*Always.* Pass an argument list to a subprocess and never set `shell=True`. Either raise on failure with `check=True` or handle the return code explicitly. Represent paths as `pathlib.Path`, and open every text file with an explicit encoding because the default depends on the machine. A binary file stays binary. A tool writes only to the folder its caller names and to the project's ignored state folders ([Toolchain](../../general.md#toolchain)).

**Checked by:** the linter's `S602` for `shell=True`, `PLW1510` for an explicit `check`, `PTH` for paths, and `PLW1514` for encodings; review for return codes handled after `check=False` and for where a tool writes.

## Tests

### Tests in a tests folder

*Default.* A folder's tests live in its `tests/` folder, one `test_<tool>.py` per tool, with an empty `__init__.py`, and use `unittest`. Python's test discovery finds `test*.py` files by default, and CPython (`Lib/test/`), NumPy (`numpy/_core/tests/`), and pandas (`pandas/tests/`) also keep `test_*.py` files in separate test directories. A separate folder keeps tests from splitting the tools into groups in a sorted listing. `python3 -I -m unittest discover -s <folder>/tests -t <folder>` runs every test with no configuration and puts the tools' folder on the import path, so a test file needs no import-path line of its own. Write expected values by hand from the input ([Checks](../../general.md#checks)). A standalone tool's tests include at least one test through `main` with a made-up input, so the tests exercise the common path. A test replaces only a process, clock, or network boundary, by patching the module's own `subprocess` call, never by adding a parameter or hook to production code for the test. A test writes only inside a temporary folder it removes.

**Checked by:** review.

### Host scripts

*Default.* Code that needs a host program, such as Blender, keeps its decisions in a module that does not import the host, and its tests cover that module. The script that imports the host only reads, calls the module, and writes. The host script itself runs where the project's real caller runs it, such as the converter's own scenario, and a new check that starts the host needs the maintainer's approval like any other wider check.

**Checked by:** review.

## Idiom

*Weigh.* Write Python the way the language and its standard library intend, since that is what other Python readers recognise and what the tools understand. The sources are the [Python tutorial](https://docs.python.org/3/tutorial/) and library reference, [PEP 8](https://peps.python.org/pep-0008/), Luciano Ramalho's *Fluent Python* (2nd edition), Brett Slatkin's *Effective Python*, and Raymond Hettinger's talk *Transforming Code into Beautiful, Idiomatic Python*. Ruff's rule pages explain each idiom its selected rules enforce. Advice that disagrees with these sources needs a reason in the change.

- Loop over the items: `for item in items`, `enumerate()` for an index, and `zip(strict=True)` for parallel sequences, never `range(len(...))`.
- Build a list or dict with a comprehension when each item is a plain transformation or filter, and with a loop when there is more than one condition or a side effect.
- Hold every file, lock, and temporary folder in a `with` block so the block releases it even when an exception occurs.
- Try the operation and catch its specific exception rather than checking first. For example, open a file and catch `FileNotFoundError` rather than testing `exists()`, which another process can invalidate before the open.
- Test emptiness with truthiness, as in `if items:`, and absence with `is None`.
- Use the standard library before writing a helper: `collections`, `itertools`, `functools`, `dataclasses`, `enum`, `json`, `argparse`, and `subprocess`.
- Use a generator when you read a sequence once, so you never hold the whole sequence in memory.
- Format text with f-strings.

**Checked by:** the linter's `UP`, `SIM`, `C4`, `B`, `PERF`, `FURB`, `RET`, and `PTH` rules for the idioms they cover; review for the rest.

## Names and layout

*Default.* Modules, functions, and variables are `snake_case`, classes `CapWords`, and constants `UPPER_SNAKE`, as PEP 8 says, so names look the same as in the standard library. A name says what the value is, with its unit where it has one (`seconds`, `metres`). A module starts with a docstring saying what it does, how to run it, and what it never does. Imports are at the top of the file and sorted. A leading `_` marks a name as private only in a module that others import, where it tells an importer not to use it.

**Checked by:** the linter's `N` rules for names and `I` rules for import order, which `ruff check --select I --fix` applies; review.
