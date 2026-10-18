"""Units of api.py's diff reading, crate selection and report, on made-up text; run by hand, as the gate runs no tool.

python3 -I -m unittest discover -s tools/tests -t tools
"""

import json
import unittest

import api
from api import Crate, Diff

DIFF = """Removed items from the public API
=================================
-pub fn toy::Rope::fray(&self) -> u32

Changed items in the public API
===============================
-pub toy::Knot::Tight(toy::error::Error)
+pub toy::Knot::Tight(toy::Error)
-pub toy::Rope::length: f64
+pub toy::Rope::length: toy::Metres

Added items to the public API
=============================
+pub mod toy::splice
+pub fn toy::splice::join(toy::Rope, toy::Rope) -> toy::Rope
"""

NOTHING = """Removed items from the public API
=================================
(none)

Changed items in the public API
===============================
(none)

Added items to the public API
=============================
(none)

"""


class Parsing(unittest.TestCase):
    def test_each_section_gives_its_items_and_a_changed_item_pairs_its_before_and_after_lines(self) -> None:
        self.assertEqual(
            api.parsed(DIFF),
            Diff(
                removed=("pub fn toy::Rope::fray(&self) -> u32",),
                changed=(
                    ("pub toy::Knot::Tight(toy::error::Error)", "pub toy::Knot::Tight(toy::Error)"),
                    ("pub toy::Rope::length: f64", "pub toy::Rope::length: toy::Metres"),
                ),
                added=("pub mod toy::splice", "pub fn toy::splice::join(toy::Rope, toy::Rope) -> toy::Rope"),
            ),
        )

    def test_an_item_printed_twice_counts_once_in_its_place(self) -> None:
        twice = DIFF.replace("+pub mod toy::splice\n", "+pub mod toy::splice\n+pub mod toy::splice\n")
        self.assertEqual(
            api.parsed(twice).added,
            ("pub mod toy::splice", "pub fn toy::splice::join(toy::Rope, toy::Rope) -> toy::Rope"),
        )
        self.assertEqual(api.distinct(["b", "a", "b", "c", "a"]), ["b", "a", "c"])

    def test_a_crate_whose_promises_did_not_move_has_an_empty_diff(self) -> None:
        self.assertTrue(api.parsed(NOTHING).empty())

    def test_text_without_the_three_sections_or_with_an_unpaired_change_is_refused(self) -> None:
        with self.assertRaises(ValueError):
            api.parsed("error: rustdoc JSON format version 60 is not supported\n")
        with self.assertRaises(ValueError):
            api.parsed(NOTHING.replace("(none)\n\nAdded", "-pub toy::Rope::length: f64\n\nAdded"))

    def test_a_crate_one_side_lacks_has_every_item_of_the_other_removed_or_added(self) -> None:
        self.assertEqual(api.one_sided(None, ["pub mod toy"]), Diff(added=("pub mod toy",)))
        self.assertEqual(api.one_sided(["pub mod toy"], None), Diff(removed=("pub mod toy",)))


class Crates(unittest.TestCase):
    def test_every_package_with_a_library_target_is_listed_and_a_binary_only_one_is_not(self) -> None:
        metadata = {
            "packages": [
                {"name": "toy-measure", "targets": [{"name": "toy-measure", "kind": ["bin"]}]},
                {
                    "name": "toy-cli",
                    "targets": [
                        {"name": "toy_cli", "kind": ["lib"]},
                        {"name": "toy", "kind": ["bin"]},
                        {"name": "help", "kind": ["test"]},
                    ],
                },
                {"name": "toy-engine", "targets": [{"name": "toy_engine", "kind": ["cdylib"]}]},
            ]
        }
        self.assertEqual(api.libraries(json.dumps(metadata)), {"toy-cli": "toy_cli", "toy-engine": "toy_engine"})


class Report(unittest.TestCase):
    def test_the_summary_counts_each_crates_items_and_changes_and_shows_a_missing_side_as_a_dash(self) -> None:
        crates = [
            Crate("toy", ["a", "b"], ["a", "c", "d"], api.parsed(DIFF)),
            Crate("toy-new", None, ["a"], Diff(added=("a",))),
            Crate("toy-broken", ["a"], None, None, ["not listed on the after side; see api.log"]),
        ]
        self.assertEqual(
            api.summary(crates).splitlines()[2:],
            ["| toy | 2 | 3 | 1 | 2 | 2 |", "| toy-new | - | 1 | 0 | 0 | 1 |", "| toy-broken | 1 | - | - | - | - |"],
        )

    def test_a_crate_with_changes_shows_them_as_a_diff_and_one_without_has_no_section(self) -> None:
        text = api.section(Crate("toy", [], [], api.parsed(DIFF)))
        self.assertIn(
            "### Changed\n\n```diff\n- pub toy::Knot::Tight(toy::error::Error)\n+ pub toy::Knot::Tight(toy::Error)\n",
            text,
        )
        self.assertNotIn("### Removed\n\n```diff\n+", text)
        self.assertEqual(api.section(Crate("toy", [], [], Diff())), "")

    def test_a_crate_that_could_not_be_listed_marks_the_report_incomplete(self) -> None:
        broken = Crate("toy", ["a"], None, None, ["not listed on the after side; see api.log"])
        self.assertIn("Incomplete", api.report("a", "b", [broken]))
        self.assertNotIn("Incomplete", api.report("a", "b", [Crate("toy", [], [], Diff())]))


if __name__ == "__main__":
    unittest.main()
