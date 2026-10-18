"""Units of rust_mutants.py on made-up text and a made-up Git repository in a temporary folder; cargo-mutants is
replaced at the process boundary, so they need `git` only and write nothing elsewhere.

  python3 -I -m unittest discover -s tools/tests -t tools
"""

import io
import signal
import subprocess
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from typing import override
from unittest import mock

import rust_mutants

DIFF = """diff --git a/crates/core/src/lib.rs b/crates/core/src/lib.rs
--- a/crates/core/src/lib.rs
+++ b/crates/core/src/lib.rs
@@ -1 +1 @@
-pub fn one() -> u8 { 1 }
+pub fn one() -> u8 { 2 }
diff --git a/crates/core/Cargo.toml b/crates/core/Cargo.toml
--- a/crates/core/Cargo.toml
+++ b/crates/core/Cargo.toml
@@ -1 +1 @@
-version = "0.1.0"
+version = "0.2.0"
diff --git a/crates/core/src/gone.rs b/crates/core/src/gone.rs
deleted file mode 100644
--- a/crates/core/src/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-pub fn gone() {}
"""


class ChangedRust(unittest.TestCase):
    def test_a_changed_rust_file_is_named_from_the_workspace(self) -> None:
        self.assertEqual(rust_mutants.changed_rust(DIFF), ["crates/core/src/lib.rs"])

    def test_a_diff_of_a_manifest_and_a_deleted_file_changes_no_rust(self) -> None:
        manifest_and_deleted = DIFF[DIFF.index("diff --git a/crates/core/Cargo.toml") :]
        self.assertEqual(rust_mutants.changed_rust(manifest_and_deleted), [])


class OutcomeLists(unittest.TestCase):
    def test_the_shards_lists_merge_in_order_and_a_missing_list_is_empty(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            shards = [Path(folder) / "shard-0", Path(folder) / "shard-1"]
            for shard, mutant in zip(shards, ("with 0", "with 1"), strict=True):
                (shard / "mutants.out").mkdir(parents=True)
                (shard / "mutants.out" / "missed.txt").write_text(
                    f"crates/core/src/lib.rs:1:22: replace one -> u8 {mutant}\n"
                )
            (shards[0] / "mutants.out" / "caught.txt").write_text("")
            lists = rust_mutants.outcome_lists(shards)
        self.assertEqual(
            lists["missed"],
            [
                "crates/core/src/lib.rs:1:22: replace one -> u8 with 0",
                "crates/core/src/lib.rs:1:22: replace one -> u8 with 1",
            ],
        )
        self.assertEqual(lists["caught"], [])
        self.assertEqual(lists["timeout"], [])


class Command(unittest.TestCase):
    def test_diff_mode_mutates_every_member_in_place_in_its_capped_shard_with_its_share_of_the_cpus(self) -> None:
        options = rust_mutants.arguments(["--base", "main", "--out", "/report", "--", "--timeout", "60"])
        self.assertEqual(
            rust_mutants.command(options, Path("/report/change.diff"), 1, Path("/report/shard-1"), True, 16),
            [
                "systemd-run",
                "--user",
                "--scope",
                "--quiet",
                "--property",
                f"MemoryMax={8 * 1024**3}",
                "--property",
                "MemorySwapMax=0",
                "--property",
                "OOMPolicy=continue",
                "--",
                "cargo",
                "mutants",
                "--in-diff",
                "/report/change.diff",
                "--workspace",
                "--in-place",
                "--shard",
                "1/2",
                "--output",
                "/report/shard-1",
                "--jobserver-tasks",
                "8",
                "--cargo-test-arg=--",
                "--cargo-test-arg=-Zunstable-options",
                "--cargo-test-arg=--fail-fast",
                "--timeout",
                "60",
            ],
        )

    def test_package_mode_names_the_crate_and_an_uncapped_shard_runs_cargo_mutants_itself(self) -> None:
        options = rust_mutants.arguments(["--package", "core", "--out", "/report", "--shards", "3"])
        self.assertEqual(
            rust_mutants.command(options, None, 0, Path("/report/shard-0"), False, 2),
            [
                "cargo",
                "mutants",
                "--package",
                "core",
                "--in-place",
                "--shard",
                "0/3",
                "--output",
                "/report/shard-0",
                "--jobserver-tasks",
                "1",
                "--cargo-test-arg=--",
                "--cargo-test-arg=-Zunstable-options",
                "--cargo-test-arg=--fail-fast",
            ],
        )

    def test_a_scope_is_available_only_when_systemd_run_starts_one(self) -> None:
        def run(code: int | None) -> bool:
            def started(arguments: list[str], **options: object) -> subprocess.CompletedProcess[bytes]:
                if code is None:
                    raise FileNotFoundError(arguments[0])
                self.assertEqual(arguments, [*rust_mutants.scope(1), "true"])
                return subprocess.CompletedProcess(arguments, code)

            with mock.patch.object(rust_mutants.subprocess, "run", started):
                return rust_mutants.scopes_available()

        self.assertEqual([run(0), run(1), run(None)], [True, False, False])

    def test_a_namespace_is_available_only_when_unshare_makes_one(self) -> None:
        def run(code: int | None) -> bool:
            def started(arguments: list[str], **options: object) -> subprocess.CompletedProcess[bytes]:
                if code is None:
                    raise FileNotFoundError(arguments[0])
                self.assertEqual(
                    arguments,
                    ["unshare", "--user", "--map-root-user", "--pid", "--fork", "--mount-proc", "--kill-child", "true"],
                )
                return subprocess.CompletedProcess(arguments, code)

            with mock.patch.object(rust_mutants.subprocess, "run", started):
                return rust_mutants.namespaces_available()

        self.assertEqual([run(0), run(1), run(None)], [True, False, False])

    def test_the_host_is_rustcs_and_unknown_when_rustc_cannot_say(self) -> None:
        def run(code: int, printed: str) -> str | None:
            def started(arguments: list[str], **options: object) -> subprocess.CompletedProcess[str]:
                self.assertEqual(arguments, ["rustc", "-vV"])
                return subprocess.CompletedProcess(arguments, code, printed)

            with mock.patch.object(rust_mutants.subprocess, "run", started):
                return rust_mutants.host(Path("/workspace"))

        version = "rustc 1.100.0-nightly\nhost: x86_64-unknown-linux-gnu\nrelease: 1.100.0-nightly\n"
        self.assertEqual(run(0, version), "x86_64-unknown-linux-gnu")
        self.assertIsNone(run(1, version))
        self.assertIsNone(run(0, "rustc 1.100.0-nightly\n"))

    def test_the_test_runner_runs_each_binary_in_its_own_namespace_around_any_runner_already_set(self) -> None:
        namespace = "unshare --user --map-root-user --pid --fork --mount-proc --kill-child"
        name = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
        self.assertEqual(
            rust_mutants.contained({"HOME": "/home"}, "x86_64-unknown-linux-gnu"), {"HOME": "/home", name: namespace}
        )
        self.assertEqual(
            rust_mutants.contained({name: "valgrind -q"}, "x86_64-unknown-linux-gnu"),
            {name: f"{namespace} valgrind -q"},
        )
        self.assertEqual(
            rust_mutants.contained({}, "thumbv7em-none-eabihf"),
            {"CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER": namespace},
        )

    def test_a_base_and_a_package_together_are_refused(self) -> None:
        with self.assertRaises(SystemExit):
            rust_mutants.arguments(["--base", "main", "--package", "core", "--out", "/report"])


class Main(unittest.TestCase):
    """The report through its entry point, over a repository whose workspace is `rust/` below its root and whose
    crate embeds a file from outside the workspace."""

    @override
    def setUp(self) -> None:
        self.folder = tempfile.TemporaryDirectory()
        root = Path(self.folder.name)
        self.repository = root / "repo"
        self.workspace = self.repository / "rust"
        source = self.workspace / "crates" / "core" / "src"
        source.mkdir(parents=True)
        (source / "lib.rs").write_text("pub fn one() -> u8 { 1 }\n")
        (self.repository / "shared").mkdir()
        (self.repository / "shared" / "table.json").write_text("{}\n")
        self.git("init", "-q")
        self.commit("base", "--allow-empty")
        self.git("add", ".")
        self.commit("lib")
        self.git("config", "diff.noprefix", "true")
        (source / "lib.rs").write_text("pub fn one() -> u8 { 2 }\n")
        self.out = root / "report"
        self.cargo: list[tuple[list[str], Path, dict[str, str]]] = []

    @override
    def tearDown(self) -> None:
        self.folder.cleanup()

    def git(self, *arguments: str) -> None:
        subprocess.run(["git", *arguments], cwd=self.repository, check=True, capture_output=True)

    def commit(self, message: str, *extra: str) -> None:
        self.git(
            "-c",
            "user.name=units",
            "-c",
            "user.email=units@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            *extra,
            "-m",
            message,
        )

    def report(self, code: int | None, missed: list[str], capped: bool = True, namespaced: bool = True) -> str:
        """Runs the report on the change since the `lib` commit, each shard's cargo-mutants writing `missed` and
        exiting with `code`, in capped scopes when `capped` and with namespaced test binaries when `namespaced`;
        returns what it printed."""
        real = subprocess.Popen

        class Shard:
            pid = 0

            def poll(self) -> int | None:
                return code

            def wait(self) -> int | None:
                return code

        def popen(arguments: list[str], **options: object) -> object:
            if "cargo" not in arguments:
                # pyrefly: ignore[no-matching-overload]  subprocess.run's own options, passed on unchanged
                return real(arguments, **options)
            cwd, env = options["cwd"], options["env"]
            if not isinstance(cwd, Path) or not isinstance(env, dict):
                self.fail(f"cargo-mutants started without a folder and an environment: {options}")
            self.cargo.append((arguments, cwd, dict(env)))
            self.assertTrue(options["start_new_session"])
            lists = Path(arguments[arguments.index("--output") + 1]) / "mutants.out"
            lists.mkdir(parents=True)
            (lists / "missed.txt").write_text("".join(f"{mutant}\n" for mutant in missed))
            return Shard()

        printed = io.StringIO()
        with (
            mock.patch.object(rust_mutants.subprocess, "Popen", popen),
            mock.patch.object(rust_mutants, "scopes_available", lambda: capped),
            mock.patch.object(rust_mutants, "namespaces_available", lambda: namespaced),
            mock.patch.object(rust_mutants, "host", mock.Mock(return_value="x86_64-unknown-linux-gnu")),
            mock.patch.object(rust_mutants.os, "cpu_count", lambda: 4),
            redirect_stdout(printed),
        ):
            self.result = rust_mutants.main(
                ["--workspace", str(self.workspace), "--base", "HEAD", "--out", str(self.out)]
            )
        return printed.getvalue()

    def test_a_change_is_mutated_in_whole_repository_copies_with_private_targets(self) -> None:
        printed = self.report(2, ["crates/core/src/lib.rs:1:22: replace one -> u8 with 0"])
        self.assertEqual(self.result, 0)
        self.assertEqual(len(self.cargo), 2)
        work = self.out / "rust-mutants"
        for index, (arguments, cwd, environment) in enumerate(self.cargo):
            copy = work / f"copy-{index}"
            self.assertEqual(cwd, copy / "rust")
            self.assertEqual(environment["CARGO_TARGET_DIR"], str(copy / "rust" / "target"))
            self.assertEqual(
                arguments[arguments.index("cargo") + 2 : arguments.index("cargo") + 9],
                [
                    "--in-diff",
                    str(work / "change.diff"),
                    "--workspace",
                    "--in-place",
                    "--shard",
                    f"{index}/2",
                    "--output",
                ],
            )
        self.assertIn("+++ b/crates/core/src/lib.rs", (work / "change.diff").read_text())
        self.assertIn("missed, to judge: crates/core/src/lib.rs:1:22: replace one -> u8 with 0", printed)
        self.assertIn("missed: 2", printed)
        self.assertNotIn("without a memory cap", printed)
        self.assertNotIn("uncontained", printed)
        self.assertFalse((work / "copy-0").exists())

    def test_each_test_binary_runs_in_a_namespace_or_uncontained_saying_so(self) -> None:
        name = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
        self.report(0, [])
        self.assertEqual(
            [environment[name] for _, _, environment in self.cargo],
            ["unshare --user --map-root-user --pid --fork --mount-proc --kill-child"] * 2,
        )
        self.cargo.clear()
        printed = self.report(0, [], namespaced=False)
        self.assertEqual([name in environment for _, _, environment in self.cargo], [False, False])
        self.assertIn(
            "rust_mutants: a PID namespace cannot be made here, so the test binaries run uncontained\n", printed
        )

    def test_each_shard_runs_in_a_capped_scope_with_its_share_of_the_cpus_or_uncapped_saying_so(self) -> None:
        self.report(0, [])
        for arguments, _, _ in self.cargo:
            self.assertEqual(arguments[: arguments.index("cargo")], rust_mutants.scope(2))
            self.assertEqual(arguments[arguments.index("--jobserver-tasks") + 1], "2")
        self.cargo.clear()
        printed = self.report(0, [], capped=False)
        self.assertEqual([arguments[0] for arguments, _, _ in self.cargo], ["cargo", "cargo"])
        self.assertIn(
            "rust_mutants: systemd-run --user cannot start a scope here, so the shards run without a memory cap\n",
            printed,
        )

    def test_each_copy_holds_the_files_outside_the_workspace_and_the_uncommitted_change(self) -> None:
        seen = []
        real = rust_mutants.copy_repository

        def copy(root: Path, files: list[str], destination: Path) -> None:
            real(root, files, destination)
            seen.append(
                (
                    (destination / "shared" / "table.json").exists(),
                    (destination / "rust" / "crates" / "core" / "src" / "lib.rs").read_text(),
                )
            )

        with mock.patch.object(rust_mutants, "copy_repository", copy):
            self.report(0, [])
        self.assertEqual(seen, [(True, "pub fn one() -> u8 { 2 }\n")] * 2)

    def test_a_rerun_reports_only_its_own_mutants_and_leaves_other_folders_alone(self) -> None:
        old = self.out / "rust-mutants" / "shard-0" / "mutants.out"
        old.mkdir(parents=True)
        (old / "missed.txt").write_text("crates/core/src/old.rs:1:1: an earlier miss\n")
        for name in ("copy-notes", "shard-analysis"):
            (self.out / name).mkdir()
            (self.out / name / "kept.txt").write_text("mine\n")
        printed = self.report(0, [])
        self.assertNotIn("earlier miss", printed)
        self.assertIn("missed: 0", printed)
        for name in ("copy-notes", "shard-analysis"):
            self.assertEqual((self.out / name / "kept.txt").read_text(), "mine\n")

    def test_a_copy_failing_after_a_shard_started_stops_that_shard_and_removes_the_copies(self) -> None:
        real_copy = rust_mutants.copy_repository
        copies = []

        def copy(root: Path, files: list[str], destination: Path) -> None:
            copies.append(destination)
            if len(copies) == 2:
                raise OSError(28, "No space left on device")
            real_copy(root, files, destination)

        stopped: list[signal.Signals] = []

        def kill(pid: int, sent: signal.Signals) -> None:
            stopped.append(sent)

        with (
            mock.patch.object(rust_mutants, "copy_repository", copy),
            mock.patch.object(rust_mutants.os, "killpg", kill),
        ):
            self.report(None, [])
        self.assertEqual(self.result, 1)
        self.assertEqual(len(self.cargo), 1)
        self.assertEqual(stopped, [rust_mutants.signal.SIGTERM])
        self.assertFalse(copies[0].exists())

    def test_a_cargo_mutants_failure_is_reported_as_a_failure(self) -> None:
        printed = self.report(4, [])
        self.assertEqual(self.result, 1)
        self.assertNotIn("missed:", printed)


if __name__ == "__main__":
    unittest.main()
