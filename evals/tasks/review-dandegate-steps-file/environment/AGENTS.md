# Working in this repository

This repository holds Rust command-line tools in one Cargo workspace under `tools/`. You work on it in place, at `/app`, inside a container from which nothing can be downloaded.

- The toolchain that `tools/rust-toolchain.toml` pins is installed, and every crate the workspace uses is already downloaded, so give Cargo `--offline`.
- Run the checks from the repository root:
  - `cargo test --offline --manifest-path tools/Cargo.toml -p dandegate`
  - `cargo fmt --manifest-path tools/Cargo.toml --check`
  - `cargo clippy --offline --manifest-path tools/Cargo.toml --all-targets -- -D warnings`
- One existing test, `scheduling::two_reports_on_one_repository_each_in_its_own_pid_namespace_keep_their_own_checkouts`, fails here whatever the code: it mounts `/proc` in a new PID namespace, which this container forbids.
- Only these tools and Git are available, and nothing can be installed. Where a guide asks for a scratch clone, a development shell, `jj`, an evidence folder, a review file elsewhere or a report to someone else, skip that step: work here and write the review where the task says.
- Your review is what `/app/findings.jsonl` holds when you stop. Nobody can answer questions, so finish the review, then stop.
