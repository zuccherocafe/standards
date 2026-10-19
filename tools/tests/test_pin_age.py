"""Units of pin_age.py on made-up toolchain files and locks in a temporary folder, which they remove.

python3 -I -m unittest discover -s tools/tests -t tools
"""

import io
import json
import re
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from dataclasses import dataclass
from datetime import UTC, date, datetime, tzinfo
from pathlib import Path
from typing import Self, override
from unittest import mock

import pin_age

# 2023-11-14 22:13:20 UTC, and the first second of 2023-11-15 UTC.
LATE_ON_THE_14TH = 1700000000
START_OF_THE_15TH = 1700006400


def toolchain(channel: str) -> str:
    return f'[toolchain]\nchannel = "{channel}"\ncomponents = ["rustfmt", "clippy"]\n'


def lock(root: str, inputs: dict[str, object], nodes: dict[str, object]) -> str:
    return json.dumps({"nodes": {root: {"inputs": inputs}, **nodes}, "root": root, "version": 7})


def node(last_modified: object) -> dict[str, object]:
    return {"locked": {"lastModified": last_modified, "type": "github"}}


class Folder(unittest.TestCase):
    @override
    def setUp(self) -> None:
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name)

    def write(self, name: str, text: str) -> Path:
        path = self.folder / name
        path.write_text(text, encoding="utf-8")
        return path


class ToolchainPin(Folder):
    def test_a_dated_nightly_was_published_on_its_date(self) -> None:
        path = self.write("rust-toolchain.toml", toolchain("nightly-2026-09-24"))
        self.assertEqual(
            pin_age.toolchain_pin(path), pin_age.Pin(f"the Rust nightly-2026-09-24 in {path}", date(2026, 9, 24))
        )

    def test_a_numbered_release_is_not_aged(self) -> None:
        for channel in ("1.92.0", "1.92"):
            with self.subTest(channel=channel):
                self.assertIsNone(pin_age.toolchain_pin(self.write("rust-toolchain.toml", toolchain(channel))))

    def test_a_channel_that_moves_by_itself_is_refused(self) -> None:
        for channel in ("stable", "nightly", "beta"):
            with self.subTest(channel=channel):
                path = self.write("rust-toolchain.toml", toolchain(channel))
                with self.assertRaisesRegex(pin_age.PinError, r"\[toolchain\] channel: .*not a dated nightly"):
                    pin_age.toolchain_pin(path)

    def test_a_nightly_on_a_day_that_does_not_exist_is_refused(self) -> None:
        path = self.write("rust-toolchain.toml", toolchain("nightly-2026-02-30"))
        with self.assertRaisesRegex(pin_age.PinError, "names no real day"):
            pin_age.toolchain_pin(path)

    def test_a_file_without_a_channel_is_refused(self) -> None:
        path = self.write("rust-toolchain.toml", '[toolchain]\nprofile = "minimal"\n')
        with self.assertRaisesRegex(pin_age.PinError, r"\[toolchain\] channel: missing"):
            pin_age.toolchain_pin(path)

    def test_a_file_that_is_not_toml_is_refused(self) -> None:
        path = self.write("rust-toolchain.toml", "[toolchain\n")
        with self.assertRaisesRegex(pin_age.PinError, "is not TOML"):
            pin_age.toolchain_pin(path)

    def test_a_missing_file_is_refused(self) -> None:
        with self.assertRaisesRegex(pin_age.PinError, "cannot be read"):
            pin_age.toolchain_pin(self.folder / "rust-toolchain.toml")


class LockPins(Folder):
    def test_each_direct_input_of_the_named_root_is_dated_by_its_last_modification_in_utc(self) -> None:
        path = self.write(
            "flake.lock",
            lock(
                "top",
                {"nixpkgs": "nixpkgs_2", "overlay": "overlay"},
                {"nixpkgs_2": node(LATE_ON_THE_14TH), "overlay": node(START_OF_THE_15TH)},
            ),
        )
        self.assertEqual(
            pin_age.lock_pins(path),
            [
                pin_age.Pin(f"flake input nixpkgs in {path}", date(2023, 11, 14)),
                pin_age.Pin(f"flake input overlay in {path}", date(2023, 11, 15)),
            ],
        )

    def test_an_input_that_follows_another_and_an_undated_input_are_skipped(self) -> None:
        path = self.write(
            "flake.lock",
            lock(
                "root",
                {"nixpkgs": "nixpkgs", "follower": ["nixpkgs"], "local": "local"},
                {"nixpkgs": node(LATE_ON_THE_14TH), "local": {"locked": {"path": "./local", "type": "path"}}},
            ),
        )
        self.assertEqual(pin_age.lock_pins(path), [pin_age.Pin(f"flake input nixpkgs in {path}", date(2023, 11, 14))])

    def test_a_root_without_inputs_has_no_pins(self) -> None:
        self.assertEqual(pin_age.lock_pins(self.write("flake.lock", lock("root", {}, {}))), [])

    def test_a_malformed_lock_is_refused_naming_the_field(self) -> None:
        cases = {
            "not JSON": ("{", "is not JSON"),
            "no root": (json.dumps({"nodes": {}, "version": 7}), "root: missing"),
            "a missing root node": (json.dumps({"nodes": {}, "root": "root"}), r"nodes\.root: the root node"),
            "a missing input node": (
                lock("root", {"nixpkgs": "gone"}, {}),
                r"nodes\.root\.inputs\.nixpkgs: names node 'gone'",
            ),
            "a text time": (
                lock("root", {"nixpkgs": "nixpkgs"}, {"nixpkgs": node("1700000000")}),
                r"nodes\.nixpkgs\.locked\.lastModified: '1700000000' is not an integer",
            ),
            "a null time": (
                lock("root", {"nixpkgs": "nixpkgs"}, {"nixpkgs": node(None)}),
                r"nodes\.nixpkgs\.locked\.lastModified: None is not an integer",
            ),
            "a fractional time": (
                lock("root", {"nixpkgs": "nixpkgs"}, {"nixpkgs": node(1.5)}),
                r"nodes\.nixpkgs\.locked\.lastModified: 1\.5 is not an integer",
            ),
            "a node without locked": (
                lock("root", {"nixpkgs": "nixpkgs"}, {"nixpkgs": {}}),
                r"nodes\.nixpkgs\.locked: missing",
            ),
        }
        for case, (text, message) in cases.items():
            with self.subTest(case=case):
                path = self.write("flake.lock", text)
                with self.assertRaisesRegex(pin_age.PinError, f"^{re.escape(str(path))}: {message}"):
                    pin_age.lock_pins(path)


class Due(unittest.TestCase):
    def test_a_pin_is_due_only_after_thirty_days(self) -> None:
        thirty = pin_age.Pin("thirty", date(2026, 9, 1))
        thirty_one = pin_age.Pin("thirty-one", date(2026, 8, 31))
        self.assertEqual(pin_age.due([thirty, thirty_one], date(2026, 10, 1)), [pin_age.Due(thirty_one, 31)])

    def test_the_age_counts_days_across_a_year(self) -> None:
        pin = pin_age.Pin("old", date(2025, 12, 31))
        self.assertEqual(pin_age.due([pin], date(2026, 3, 1)), [pin_age.Due(pin, 60)])


@dataclass(frozen=True, slots=True)
class Run:
    code: int
    out: str
    err: str


class NewYearsDay2024(datetime):
    """The clock at noon UTC on 2024-01-01; every other datetime method is the real one."""

    @override
    @classmethod
    def now(cls, tz: tzinfo | None = None) -> Self:
        return cls(2024, 1, 1, 12, tzinfo=UTC)


class Main(Folder):
    def run_main(self, argv: list[str]) -> Run:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err), mock.patch("pin_age.datetime", NewYearsDay2024):
            code = pin_age.main(argv)
        return Run(code, out.getvalue(), err.getvalue())

    def test_an_old_nightly_and_an_old_input_each_get_one_line_and_the_run_passes(self) -> None:
        old = self.write("rust-toolchain.toml", toolchain("nightly-2020-01-01"))
        flake = self.write("flake.lock", lock("root", {"nixpkgs": "nixpkgs"}, {"nixpkgs": node(LATE_ON_THE_14TH)}))
        self.assertEqual(
            self.run_main(["--toolchain", str(old), "--lock", str(flake)]),
            Run(
                0,
                f"pin-age: the Rust nightly-2020-01-01 in {old} is 1461 days old; "
                "move it (rules/general.md, Toolchain)\n"
                f"pin-age: flake input nixpkgs in {flake} is 48 days old; move it (rules/general.md, Toolchain)\n",
                "",
            ),
        )

    def test_a_nightly_thirty_days_old_and_a_numbered_release_print_nothing(self) -> None:
        recent = self.write("recent.toml", toolchain("nightly-2023-12-02"))
        release = self.write("release.toml", toolchain("1.92.0"))
        self.assertEqual(self.run_main(["--toolchain", str(recent), "--toolchain", str(release)]), Run(0, "", ""))

    def test_a_broken_pin_file_fails_with_one_line_naming_it(self) -> None:
        old = self.write("old.toml", toolchain("nightly-2020-01-01"))
        stable = self.write("stable.toml", toolchain("stable"))
        self.assertEqual(
            self.run_main(["--toolchain", str(old), "--toolchain", str(stable)]),
            Run(
                2,
                "",
                f"pin-age: {stable}: [toolchain] channel: 'stable' is not a dated nightly or a numbered release\n",
            ),
        )

    def test_no_file_to_read_is_a_usage_error(self) -> None:
        with self.assertRaises(SystemExit) as raised, redirect_stderr(io.StringIO()):
            pin_age.main([])
        self.assertEqual(raised.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
