# The Rust standard

This standard governs Rust in zucchero✿café projects. It applies [general.md](../../general.md) to Rust and adds Rust-specific rules. [review.md](../../review.md) turns both standards into review questions. [templates/rust/](../../../templates/rust/) contains the configuration for rules that tools can enforce.

References name paths in public repositories: [nextest](https://github.com/nextest-rs/nextest), [guppy](https://github.com/guppy-rs/guppy), [jiff](https://github.com/BurntSushi/jiff), [ripgrep](https://github.com/BurntSushi/ripgrep), and [rust-analyzer](https://github.com/rust-lang/rust-analyzer). They are products with many maintainers and outside users, so they show a design's shape, not the scale to build it at. When a rule is unclear, consult them for the shape and its reasons, and adopt it at the project's scale. Each rule ends with a Checked by entry naming the compiler, a lint, a project check, a report, or review. Where a tool checks only part of a rule, the entry names that part. Each rule's text begins with its kind, as [How strictly a rule applies](../../general.md#how-strictly-a-rule-applies) defines. Examples from [tdollhouse](https://github.com/zuccherocafe/tdollhouse) appear where they teach something the references do not.

The standard is in several files. Read this one first, then each file whose subject a change touches:

- [Design](design.md): Types, parsing, where behaviour lives, visibility, records, modules, and crates.
- [Errors](errors.md): Who fixes an error, panics by crate kind, error types, refusals, and messages.
- [Tests](tests.md): The test policy, scenarios, expected values, properties, and the mutation report.
- [Performance](performance.md): Hot paths, zero-cost checks, data layout, parallelism, and measuring.
- [Lints and documentation](documentation.md): Lint exceptions, the report scans, and documentation.
- [Async and concurrency](async.md): Tasks, cancellation, queues, and shutdown in projects that run an async runtime.

## Toolchain and lints

*Always.* Use edition 2024 and resolver 3. Each project pins a release or dated nightly in `rust-toolchain.toml`, with rustfmt and Clippy. The public API report needs rustdoc's unstable JSON output, so run it on a dated nightly pinned with a matching `cargo-public-api` release and move both together ([Toolchain](../../general.md#toolchain)). The template uses that nightly as its toolchain. Lints live in `[workspace.lints]`, and every workspace member inherits them with `[lints] workspace = true`. The canonical table is [templates/rust/Cargo.toml](../../../templates/rust/Cargo.toml). Run Clippy as `cargo clippy --all-targets -- -D warnings`, so warnings fail the check. Forbid `unsafe_code` except in a crate whose job needs it, such as an engine binding's entry point. That crate documents the exception. rustfmt governs formatting. `cargo-deny` checks advisories, licences, duplicate versions, and dependency sources. Its advisory database may use a pinned, reviewed revision for reproducible offline checks. Keep the default `maximum-db-staleness`, so a snapshot older than ninety days fails. A vulnerability check needs new advisories to remain useful. Refresh the pin rather than raise the limit.

*Default.* Declare every dependency once in `[workspace.dependencies]` as `major.minor`, such as `"0.23"` or `"2.14"`. Never specify a patch number or exact version, except for the exact-version exception below. Cargo treats a bare version as a caret requirement that accepts every compatible release. The committed `Cargo.lock` fixes the exact build version, and builds use `--locked`. A patch number in `Cargo.toml` therefore pins no build version and becomes stale. Use an exact requirement, such as `"=0.5.5"`, only when a release in the caret range breaks the project, as when an engine binding's generated API must match one engine release. Above the requirement, state in a comment what breaks. List the workspace's own crates first, by path, followed by a blank line and external crates. Members use `workspace = true` for workspace dependencies and add only features. Use the same grouping in members: workspace crates, a blank line, and external crates.

**Checked by:** rustc, rustfmt, and Clippy with `-D warnings` over the template's lint table, and `cargo-deny`, in the project's checks; `--locked` builds for the lockfile; review for how dependencies are declared.

## For a new project: not yet decided

When a project needs one of these features, it answers the open questions in its `AGENTS.md` and cites prior art. These are not rules yet. A project that does not need a feature, such as network access or async, forbids it in `AGENTS.md` rather than leaving it undecided.

- **Network and TLS.** Decide which crates may make network requests, and check that allow-list against the dependency graph.
- **Tracing and redaction.** Define structured spans per operation and log levels. Address types that contain secrets or private conversations and redact them in `Debug`. Address human diagnostics on stderr and machine output on stdout.
- **Layered configuration.** Define the order of precedence and the source tracking for files, environment variables, and flags. Address one-time parsing into typed state with `deny_unknown_fields`, secrets, and reload semantics (`nextest-runner/src/cargo_config/discovery.rs`).
- **Machine-readable command-line output.** Define semantic exit codes, broken-pipe handling, stdout and stderr separation, and a versioned machine format (`nextest-metadata`).
- **Migrations of durable data.** For data that cannot be rebuilt from sources, define schema versions, migrations, atomic and durable writes, concurrent writers, and recovery (`nextest`'s old-reader and new-data compatibility matrix in `AGENTS.md`). Derived records can instead refuse an unknown version and ask for a rebuild.
- **Protocol stability.** A protocol with outside peers needs `#[non_exhaustive]`, versions, and compatibility tests. A published crate needs a policy for SemVer, its minimum supported Rust version, and features.
- **Wall-clock time.** Distinguish timestamps and time zones, as in jiff, from durations.
