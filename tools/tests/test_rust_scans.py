"""Units of rust_scans.py's three scans on a made-up workspace in a temporary folder; they write nothing elsewhere and
need `cargo` on PATH, as the scans do.

  python3 -I -m unittest discover -s tools/tests -t tools
"""

import tempfile
import unittest
from pathlib import Path
from typing import override

import rust_scans

CORE = """//! A core.
use std::fs;

pub struct Reach { pub margin_metres: f64 }

pub(crate) fn wait(timeout_seconds: f64) {}

#[cfg(test)]
mod tests {
    fn fixture(span_seconds: f64) { let _ = std::env::var("HOME"); }
}
"""
KERNEL = """pub(crate) fn solve(step_seconds: f64) -> f64 { let gap_metres: f64 = 1.0; gap_metres * step_seconds }
"""
SHELL = """use std::fs;

#[derive(Debug, thiserror::Error)]
#[error(
    "{code}: could not read {path}"
)]
pub struct ReadError { pub path: String, pub code: &'static str }

#[error("could not write {path}")]
pub struct WriteError { pub path: String, pub length: Length }
"""


def crate(workspace: Path, name: str, files: dict[str, str]) -> None:
    folder = workspace / "crates" / name
    (folder / "src").mkdir(parents=True)
    (folder / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nedition = "2024"\n')
    for path, text in files.items():
        (folder / "src" / path).parent.mkdir(parents=True, exist_ok=True)
        (folder / "src" / path).write_text(text)


class Scans(unittest.TestCase):
    @override
    def setUp(self) -> None:
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name).resolve()
        workspace = self.root / "rust"
        workspace.mkdir()
        (workspace / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\n')
        crate(workspace, "toy-core", {"lib.rs": CORE + "mod solve;\n", "solve.rs": KERNEL})
        crate(workspace, "toy-shell", {"lib.rs": SHELL})
        self.findings = rust_scans.scan(self.root, workspace, frozenset({"toy-core"}), {"toy-core": ("solve",)})

    def test_h1_finds_io_tokens_in_a_core_only_and_marks_those_in_test_code(self) -> None:
        self.assertEqual(
            self.findings["H1"],
            [
                "rust/crates/toy-core/src/lib.rs:2: std::fs in toy-core",
                "rust/crates/toy-core/src/lib.rs:10: std::env in toy-core (test code)",
            ],
        )

    def test_h2_finds_unit_suffixed_primitives_outside_kernels_tests_and_lets(self) -> None:
        self.assertEqual(
            self.findings["H2"],
            [
                "rust/crates/toy-core/src/lib.rs:4: margin_metres: f64",
                "rust/crates/toy-core/src/lib.rs:6: timeout_seconds: f64",
            ],
        )

    def test_h3_finds_a_code_in_an_error_message_across_lines(self) -> None:
        self.assertEqual(self.findings["H3"], ["rust/crates/toy-shell/src/lib.rs:4: a code in #[error(...)]"])

    def test_the_members_are_cargos_a_root_package_included_and_an_excluded_crate_left_out(self) -> None:
        workspace = self.root / "rooted"
        (workspace / "src").mkdir(parents=True)
        (workspace / "Cargo.toml").write_text(
            '[package]\nname = "toy-root"\nedition = "2024"\n\n'
            '[workspace]\nmembers = ["crates/*"]\nexclude = ["crates/toy-out"]\n'
        )
        (workspace / "src" / "lib.rs").write_text("pub struct Gap { pub width_metres: f64 }\n")
        crate(workspace, "toy-in", {"lib.rs": ""})
        crate(workspace, "toy-out", {"lib.rs": "pub struct Gap { pub width_metres: f64 }\n"})
        self.assertEqual([name for name, _ in rust_scans.members(workspace)], ["toy-root", "toy-in"])
        found = rust_scans.scan(self.root, workspace, frozenset(), {})["H2"]
        self.assertEqual(found, ["rooted/src/lib.rs:1: width_metres: f64"])

    def test_a_kernel_is_one_crate_module_and_a_malformed_one_is_refused(self) -> None:
        self.assertEqual(rust_scans.kernel("toy-core=qp,arms/sqp"), ("toy-core", ("qp", "arms/sqp")))
        with self.assertRaises(rust_scans.argparse.ArgumentTypeError):
            rust_scans.kernel("toy-core")


if __name__ == "__main__":
    unittest.main()
