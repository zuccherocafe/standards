# rust-mutants design read

- Area: the mutation report's runner, `tools/crates/rust-mutants`, covering its build settings, the repository and target copies, the processes it starts, their cleanup, and dandegate's call
- Last full read: cb77ddfae9529674f6b5514134c39a7b98f40a75
- Inspected: `tools/crates/rust-mutants/` (`src/main.rs`, `src/arguments.rs`, `src/command.rs`, `src/machine.rs`, `src/repository.rs`, `src/shards.rs`, `src/outcomes.rs`, `tests/scenarios.rs`, `tests/scenarios/support.rs`), `tools/crates/dandegate/src/rust.rs` (`mutants`, `mutants_section`, `TOOLCHAIN_COLLECTOR`), `rules/languages/rust/tests.md` (Mutation report), and cargo-mutants 27.1.0's `lab.rs`, `cargo.rs`, `build_dir.rs` and `copy_tree.rs` for what it does with `--in-place`, `--shard`, the baseline and `$CARGO`

## Models in play

- Build settings: `command.rs` defines the `mutants` profile once, as `PROFILE` and the `SETTINGS` keys that `profile` turns into `--cargo-arg=--config=...` options, with `NO_DOCTESTS` and `FAIL_FAST` for cargo test. A `--profile` among the caller's options turns the tool's profile off (`names_profile`). By default, cargo-mutants turns these options, the caller's and its own configuration into each `cargo test --no-run` and `cargo test` call (`cargo_argv` in its `cargo.rs`), and sets `CARGO_ENCODED_RUSTFLAGS` itself when its lint cap is on.
- The selection and the command's inputs: `command::Prepared` holds the `Work` folder, the workspace's place in the repository and the `Selection` (a diff file or a package). `main::run` makes it after taking the diff in diff mode, and `command::command` builds one shard's cargo-mutants call from it, the `Shard`, the caller's options and the `Machine`.
- The tool's folder: `command::Work` owns every path under `--out/rust-mutants/`: the diff, each shard's copy (`copy-<index>`) and each shard's output (`shard-<index>`). `run` empties it at the start.
- Machine facts: `machine::Machine` holds the CPUs, whether a capped scope starts and the test runner, each found once by trying it. `command::scope` gives the `systemd-run --user --scope` prefix holding a shard's part of the 16 GiB `MEMORY` cap; the probe uses the whole cap.
- Repository copies: `repository::Repository::files` lists tracked and unignored files with `git ls-files --cached --others --exclude-standard`, so a copy never holds a build folder when build outputs are untracked and ignored, and `Repository::copy` copies each through `copy_file`, which keeps permissions and modification times and copies a link as a link.
- Target folders: each shard's cargo-mutants runs `--in-place` in its copy with `CARGO_TARGET_DIR` set to the copy's own `target`, which starts empty when build outputs are untracked and ignored. By default, cargo-mutants runs the shard's baseline there over the packages of its mutants, then each mutant over its own package, all in that one folder.
- Process lifetime: `shards::Running` owns the shards' children, each in its own session; dropped early, it sends each process group `SIGTERM` and waits. `shards::Copies` removes the copies when dropped and is declared before `Running` in `run_shards`, so it drops after every shard has stopped. `run_shards` registers the interrupt flag and reads it between file copies and before each start; `Running::wait` reads it while it polls.
- Outcomes: `outcomes::Outcomes` merges the shards' lists in shard order, duplicates kept.
- dandegate's call: `dandegate::rust::mutants` runs `rust-mutants --workspace <workspace> --base <before> --out <step>/run -- <mutation settings>` in the after checkout's development shell, and puts what it printed, cargo-mutants' console output included, in the report's section. `TOOLCHAIN_COLLECTOR` includes the digests of `rust-mutants` and `cargo-mutants` in the step's identity, so a changed runner runs the step afresh.

## Overlaps

- The profile's reason in `main.rs`'s module documentation ("with dependencies fully optimised, since they build once per shard") rests on where dependency builds happen, which no type or function of the tool owns: each shard's cargo-mutants makes them in its own target, which starts empty when build outputs are untracked and ignored.
- `Prepared` is documented as "everything prepared before the shards start", but it holds only the commands' inputs. Nothing in the tool proves that a build or a copy succeeded.

## Half-done transitions

- None found in what Inspected lists.

## What later work needs

- The change report runs this step on every tdollhouse change. A faster, bounded mutation step lets a report run more steps at once. Building the dependencies once per run and copying them into each shard was built and measured in MU1 on a busy 9800X3D (8 cores, 16 threads) over tdollhouse `6fcacd75` against `ba934cb0` (367 mutants), with plain copies on ext4 and one shared 16 GiB slice because per-shard scopes could not start. Across three runs per side at the default two shards, the experiment was slower, a median 793 s (range 749 to 871) against 697 s (range 692 to 705), because preparation (repository copies, discovery, and the six builds) took 136, 128, and 136 s and the target copies took 9.9, 6.4, and 9.2 s while each shard saved about 40 s in its baseline build, and four or eight shards were no faster, since two already use every core. It gained only memory, a median 9.0 GiB peak (range 8.1 to 9.4) against 12.1 GiB (range 11.3 to 12.8). A faster step has to cut the work each mutant's build does.
- A target folder reused between copies makes `copy_file`'s kept modification times decide what Cargo rebuilds: a copied source older than a reused build output reads as fresh to Cargo.

## Disagreements

| Concept | Owner | Sites | Consequence | Done when |
|---|---|---|---|---|
| Command inputs named as a preparation | rust-mutants | `tools/crates/rust-mutants/src/command.rs` `Prepared`; `tools/crates/rust-mutants/src/main.rs` `run`, `run_shards` | A reader takes `Prepared` for proof that something was built or copied, which it does not hold | The commands' inputs have a name that says what they are, and only a value made by a successful preparation stands for a prepared target |
