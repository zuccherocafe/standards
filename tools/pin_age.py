"""Says which toolchain pins are due to move: prints one line for each pin published more than 30 days ago, and
nothing when none is due (rules/general.md, Toolchain). It advises and never fails on a pin's age, since a check that
failed on age would fail an old commit on a later date.

  pin-age [--toolchain <rust-toolchain.toml>]... [--lock <flake.lock>]

Paths are read as given. A toolchain file's dated nightly, `nightly-YYYY-MM-DD`, was published that day; a numbered
release carries no date and is not aged. A lock's pins are the direct inputs of its root node, each dated by its
`locked.lastModified` as a UTC date; an input that follows another is that other input's pin, and a node without
`lastModified`, such as a path input, has no date.

Exits 0 whatever the ages. A file that cannot be read or parsed, or a channel that is not a pin, exits 2 with one line
naming the file and field: that wiring is broken on any date.
"""

import argparse
import json
import re
import sys
import tomllib
from dataclasses import dataclass
from datetime import UTC, date, datetime
from pathlib import Path

# About a month: toolchains move monthly, so a pin older than this is due.
DUE_AFTER_DAYS = 30
NIGHTLY = re.compile(r"nightly-(\d{4}-\d{2}-\d{2})")
RELEASE = re.compile(r"\d+\.\d+(\.\d+)?")


@dataclass(frozen=True, slots=True)
class Pin:
    """One pinned toolchain or input and the day it was published."""

    what: str
    published: date


@dataclass(frozen=True, slots=True)
class Due:
    """A pin over 30 days old."""

    pin: Pin
    age_days: int


class PinError(Exception):
    """A pin file that cannot be read: names the file and the field."""


@dataclass(frozen=True, slots=True)
class Options:
    """The pin files the command line names: the toolchain files in the order given, and the lock."""

    toolchains: tuple[Path, ...]
    lock: Path | None


def read_text(path: Path) -> str:
    """The file's UTF-8 text; raises PinError when it cannot be read or is not UTF-8."""
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise PinError(f"{path}: cannot be read: {error.strerror}") from error
    except UnicodeDecodeError as error:
        raise PinError(f"{path}: is not UTF-8 text") from error


def toolchain_pin(path: Path) -> Pin | None:
    """The Rust toolchain `path` pins, or None for a numbered release, which carries no date. Raises PinError for a
    file that cannot be read or parsed, a missing channel, or a channel that is not a dated nightly or a release."""
    try:
        document: dict[str, object] = tomllib.loads(read_text(path))
    except tomllib.TOMLDecodeError as error:
        raise PinError(f"{path}: is not TOML: {error}") from error
    toolchain = document.get("toolchain")
    channel = toolchain.get("channel") if isinstance(toolchain, dict) else None
    if not isinstance(channel, str):
        raise PinError(f"{path}: [toolchain] channel: missing, or not a string")
    if RELEASE.fullmatch(channel):
        return None
    nightly = NIGHTLY.fullmatch(channel)
    if nightly is None:
        raise PinError(f"{path}: [toolchain] channel: {channel!r} is not a dated nightly or a numbered release")
    try:
        published = date.fromisoformat(nightly.group(1))
    except ValueError as error:
        raise PinError(f"{path}: [toolchain] channel: {channel!r} names no real day") from error
    return Pin(f"the Rust {channel} in {path}", published)


def lock_pins(path: Path) -> list[Pin]:
    """One pin per direct input of the lock's root node that carries a date, in the lock's order. Raises PinError for a
    file that cannot be read or parsed, or a lock whose structure or timestamps are malformed."""
    try:
        lock: object = json.loads(read_text(path))
    except json.JSONDecodeError as error:
        raise PinError(f"{path}: is not JSON: {error}") from error
    if not isinstance(lock, dict):
        raise PinError(f"{path}: is not a JSON object")
    nodes = lock.get("nodes")
    if not isinstance(nodes, dict):
        raise PinError(f"{path}: nodes: missing, or not an object")
    root_name = lock.get("root")
    if not isinstance(root_name, str):
        raise PinError(f"{path}: root: missing, or not a string")
    root = nodes.get(root_name)
    if not isinstance(root, dict):
        raise PinError(f"{path}: nodes.{root_name}: the root node is missing, or not an object")
    inputs = root.get("inputs", {})
    if not isinstance(inputs, dict):
        raise PinError(f"{path}: nodes.{root_name}.inputs: not an object")
    pins = []
    for name, reference in inputs.items():
        if isinstance(reference, list):
            continue
        if not isinstance(reference, str):
            raise PinError(f"{path}: nodes.{root_name}.inputs.{name}: neither a node name nor a follows path")
        node = nodes.get(reference)
        if not isinstance(node, dict):
            raise PinError(f"{path}: nodes.{root_name}.inputs.{name}: names node {reference!r}, which is missing")
        locked = node.get("locked")
        if not isinstance(locked, dict):
            raise PinError(f"{path}: nodes.{reference}.locked: missing, or not an object")
        if "lastModified" not in locked:
            continue
        modified = locked["lastModified"]
        field = f"nodes.{reference}.locked.lastModified"
        if not isinstance(modified, int) or isinstance(modified, bool):
            raise PinError(f"{path}: {field}: {modified!r} is not an integer")
        try:
            published = datetime.fromtimestamp(modified, UTC).date()
        except (OverflowError, OSError, ValueError) as error:
            raise PinError(f"{path}: {field}: {modified} is not a time") from error
        pins.append(Pin(f"flake input {name} in {path}", published))
    return pins


def due(pins: list[Pin], today: date) -> list[Due]:
    """The pins published more than 30 days before `today`, in the order given."""
    ages = ((pin, (today - pin.published).days) for pin in pins)
    return [Due(pin, age_days) for pin, age_days in ages if age_days > DUE_AFTER_DAYS]


def arguments(argv: list[str] | None) -> Options:
    """The parsed command line; argparse exits with status 2 on a usage error, including naming no file."""
    parser = argparse.ArgumentParser(prog="pin-age", description="Say which toolchain pins are due to move.")
    parser.add_argument(
        "--toolchain", type=Path, action="append", default=[], help="a rust-toolchain.toml; may be repeated"
    )
    parser.add_argument("--lock", type=Path, help="a flake.lock")
    namespace = parser.parse_args(argv)
    options = Options(tuple(namespace.toolchain), namespace.lock)
    if not options.toolchains and options.lock is None:
        parser.error("give at least one --toolchain or --lock")
    return options


def main(argv: list[str] | None = None) -> int:
    """Prints a line for each due pin and returns 0, or prints the first PinError and returns 2."""
    options = arguments(argv)
    try:
        toolchains = [toolchain_pin(path) for path in options.toolchains]
        pins = [pin for pin in toolchains if pin is not None]
        if options.lock is not None:
            pins += lock_pins(options.lock)
    except PinError as error:
        print(f"pin-age: {error}", file=sys.stderr)
        return 2
    for overdue in due(pins, datetime.now(UTC).date()):
        print(f"pin-age: {overdue.pin.what} is {overdue.age_days} days old; move it (rules/general.md, Toolchain)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
